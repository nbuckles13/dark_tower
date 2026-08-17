//! State transitions (`next`, `complete`, `escalate`, `validate`) and the
//! atomic write used by mutating subcommands.

use crate::manifest::{Manifest, Slug, Status, Task};
use crate::markdown;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::Write;
use std::path::Path;

/// JSON payload printed by `dt-story next` for a runnable task.
#[derive(Debug, Serialize)]
pub struct RunnableTask {
    pub id: u32,
    pub specialist: String,
    pub env_tests: bool,
    pub prompt: String,
}

/// Outcome of `next` resolution (exit codes 0 / 3 / 4 respectively).
/// `reopened` is true when the selected task was escalated and has been
/// flipped back to pending for retry — the caller must persist that
/// mutation to the story file before acting on the payload.
#[derive(Debug)]
pub enum NextOutcome {
    Runnable { task: RunnableTask, reopened: bool },
    AllDone,
    Blocked(String),
}

/// Resolve the next runnable task: the lowest-id pending OR escalated task
/// whose deps are ALL completed. Serial execution — no critical-path
/// prioritization. Escalated tasks are retryable: when one is selected it
/// is mutated back to pending (escalation cleared) and reported with
/// `reopened: true` so the caller can persist the transition.
pub fn next(manifest: &mut Manifest) -> Result<NextOutcome> {
    let completed: HashSet<u32> = manifest
        .tasks
        .iter()
        .filter(|t| t.status == Status::Completed)
        .map(|t| t.id)
        .collect();

    let mut candidates: Vec<&Task> = manifest
        .tasks
        .iter()
        .filter(|t| matches!(t.status, Status::Pending | Status::Escalated))
        .collect();
    if candidates.is_empty() {
        return Ok(NextOutcome::AllDone);
    }
    candidates.sort_by_key(|t| t.id);

    let selected = candidates
        .iter()
        .find(|t| t.deps.iter().all(|d| completed.contains(d)))
        .map(|t| t.id);

    let Some(id) = selected else {
        let first = candidates
            .first()
            .ok_or_else(|| anyhow!("candidate set vanished (internal error)"))?;
        let waits: Vec<String> = first
            .deps
            .iter()
            .filter(|d| !completed.contains(d))
            .map(|d| match manifest.task(*d) {
                Some(dep) if dep.status == Status::Escalated => format!(
                    "dep {d} is escalated and blocks dependents until it is selected and retried"
                ),
                Some(dep) => format!("dep {d} is {}", dep.status),
                None => format!("dep {d} is missing"),
            })
            .collect();
        return Ok(NextOutcome::Blocked(format!(
            "no runnable task: task {} ({}) is blocked ({})",
            first.id,
            first.status,
            waits.join(", ")
        )));
    };

    let task = manifest
        .task_mut(id)
        .ok_or_else(|| anyhow!("selected task {id} vanished (internal error)"))?;
    let reopened = task.status == Status::Escalated;
    if reopened {
        task.status = Status::Pending;
        task.escalation = None;
    }
    let payload = runnable_payload(task)?;
    Ok(NextOutcome::Runnable {
        task: payload,
        reopened,
    })
}

/// Enforce pending-task field requirements on `next`'s output path.
fn runnable_payload(task: &Task) -> Result<RunnableTask> {
    let specialist = task
        .specialist
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("task {} is pending but has no specialist", task.id))?;
    let env_tests = task
        .env_tests
        .ok_or_else(|| anyhow!("task {} is pending but has no env_tests value", task.id))?;
    let prompt = task
        .prompt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("task {} is pending but has no prompt", task.id))?;
    Ok(RunnableTask {
        id: task.id,
        specialist: specialist.to_string(),
        env_tests,
        prompt: prompt.to_string(),
    })
}

/// Mark a pending task completed, recording the commit if given.
/// Outcome of `complete`, so callers can distinguish a real transition from an
/// idempotent no-op (e.g. the runner marking a task the devloop already marked).
#[derive(Debug, PartialEq, Eq)]
pub enum CompleteOutcome {
    /// Task was pending and is now completed.
    Completed,
    /// Task was already completed. `slug_updated` is true when a
    /// newly-supplied slug replaced a different (or absent) one, which is the
    /// ONLY case in which this arm mutates the manifest — the caller must
    /// persist when it is set. See [`complete`] for why `slug` overwrites
    /// here and `commit` deliberately does not.
    AlreadyComplete { slug_updated: bool },
}

/// Mark a task completed, recording `commit` and/or `slug` if supplied.
///
/// The `AlreadyComplete` arm is reached when a headless devloop marked its
/// own task before the runner's `dt-story complete` ran (double-writer, not
/// an error — task #60 collision, 2026-08-06).
///
/// On that arm `slug` is LAST-WRITER-WINS and IS applied, while `commit` is
/// first-writer-wins and is NOT. The asymmetry is deliberate; see the field
/// docs on [`crate::manifest::Task::slug`] for the provenance argument. The
/// practical consequence is that `complete --slug` REPAIRS a completed task
/// that is missing its slug, which is what lets the runner's `NO-SLUG`
/// diagnostic name a recovery gesture that actually works.
pub fn complete(
    manifest: &mut Manifest,
    id: u32,
    commit: Option<String>,
    slug: Option<Slug>,
) -> Result<CompleteOutcome> {
    let task = manifest
        .task_mut(id)
        .ok_or_else(|| anyhow!("task {id} not found in manifest"))?;
    match task.status {
        Status::Pending => {
            task.status = Status::Completed;
            if commit.is_some() {
                task.commit = commit;
            }
            if slug.is_some() {
                task.slug = slug;
            }
            Ok(CompleteOutcome::Completed)
        }
        Status::Completed => {
            let slug_updated = match slug {
                Some(new) if task.slug.as_ref() != Some(&new) => {
                    task.slug = Some(new);
                    true
                }
                _ => false,
            };
            Ok(CompleteOutcome::AlreadyComplete { slug_updated })
        }
        Status::Escalated => {
            bail!("task {id} is escalated, not pending; cannot complete")
        }
    }
}

/// Outcome of `add_task`: either a new task was appended (with its id) or a
/// task with the same tag already existed (idempotent no-op, its id returned).
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added(u32),
    Exists(u32),
}

/// Fields for [`add_task`], as a struct rather than positional parameters.
///
/// NOT for arity: `specialist`, `prompt` and `tag` are three ADJACENT
/// same-typed (`String`) parameters, so transposing any two compiles cleanly
/// and produces a task carrying the specialist in its tag field — silent
/// corruption of the struct this crate makes authoritative. Named fields
/// make the transposition impossible to write.
pub struct NewTask {
    pub specialist: String,
    pub prompt: String,
    pub env_tests: bool,
    pub tag: String,
    pub deps: Vec<u32>,
}

/// Append a pending task, idempotent on `tag`. If a task with the same tag
/// already exists, return `Exists(id)` without mutating; otherwise append a
/// pending task with a fresh id (max existing id + 1) and return `Added(id)`.
///
/// IDEMPOTENT IN THE WEAK SENSE: "will not duplicate", NOT "converges to the
/// supplied values". The tag check returns BEFORE any field is written, so a
/// re-run supplying corrected `specialist` / `prompt` / `env_tests` / `deps`
/// silently discards them. Callers must surface that (`main.rs` does, on the
/// rc-4 path); the gesture for a changed plan is to reset the manifest to a
/// skeleton and re-emit, not to re-run `add-task` over a populated one.
///
/// CALLERS MUST NOT PREDICT THE ASSIGNED ID. It is `max + 1` over existing
/// tasks, so it depends on what is already in the file — and the `Exists`
/// short-circuit means a re-run does not advance the sequence. A caller that
/// computes dep numbers from its own ordering will, on any re-run or partial
/// run, produce deps that point at the WRONG TASKS AND STILL VALIDATE. Read
/// the id back from the returned outcome (both arms carry it) and map your
/// own numbering onto it.
///
/// Dep validation is DELTA-form: an append is refused only if it INTRODUCES
/// new violations. An absolute check would turn `add-task` into a surprise
/// gate on unrelated pre-existing state — fatal on the audit-remediation
/// path, which appends to a live mid-flight story — and would need a special
/// case for the legitimately-empty skeleton, i.e. a hole drilled in the very
/// gate that guards it.
pub fn add_task(manifest: &mut Manifest, new: NewTask) -> Result<AddOutcome> {
    if let Some(existing) = manifest
        .tasks
        .iter()
        .find(|t| t.tag.as_deref() == Some(new.tag.as_str()))
    {
        return Ok(AddOutcome::Exists(existing.id));
    }
    let before = validate_manifest(manifest);
    let new_id = manifest.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    manifest.tasks.push(Task {
        id: new_id,
        status: Status::Pending,
        specialist: Some(new.specialist),
        env_tests: Some(new.env_tests),
        deps: new.deps,
        prompt: Some(new.prompt),
        commit: None,
        slug: None,
        escalation: None,
        tag: Some(new.tag),
    });
    let after = validate_manifest(manifest);
    let introduced: Vec<String> = after
        .iter()
        .filter(|v| !before.contains(v))
        .cloned()
        .collect();
    if !introduced.is_empty() {
        manifest.tasks.pop();
        bail!(
            "refusing to append task {new_id}: it would introduce manifest violations that the \
             runner's own gate would then reject — {}",
            introduced.join("; ")
        );
    }
    Ok(AddOutcome::Added(new_id))
}

/// Mark a pending task escalated, recording the escalation log path.
pub fn escalate(manifest: &mut Manifest, id: u32, log: &str) -> Result<()> {
    let task = manifest
        .task_mut(id)
        .ok_or_else(|| anyhow!("task {id} not found in manifest"))?;
    if task.status != Status::Pending {
        bail!("task {id} is {}, not pending; cannot escalate", task.status);
    }
    task.status = Status::Escalated;
    task.escalation = Some(log.to_string());
    Ok(())
}

/// Validate a full story file. Returns one message per violation; empty
/// means valid. Block-extraction and YAML-parse failures short-circuit
/// (later checks would be meaningless without a parsed manifest).
pub fn validate_story(text: &str) -> Vec<String> {
    let span = match markdown::find_manifest_block(text) {
        Ok(span) => span,
        Err(e) => return vec![format!("{e:#}")],
    };
    let yaml = match markdown::manifest_yaml(text, &span) {
        Ok(yaml) => yaml,
        Err(e) => return vec![format!("{e:#}")],
    };
    let manifest = match Manifest::from_yaml(yaml) {
        Ok(m) => m,
        Err(e) => return vec![format!("{e:#}")],
    };
    validate_manifest(&manifest)
}

/// Semantic checks on a parsed manifest: unique ids, no dangling deps, no
/// dependency cycles, per-status field requirements for pending tasks.
/// (Status legality and unknown fields are enforced by deserialization.)
pub fn validate_manifest(manifest: &Manifest) -> Vec<String> {
    let mut violations = Vec::new();

    // MANIFEST-LEVEL, not status-aware — the only such check here, and the
    // reason this function's description in `manifest.rs` had to change.
    //
    // A zero-task manifest is a valid document that ASSERTS THE STORY IS
    // COMPLETE: `next` finds no pending candidates and exits 3 (AllDone),
    // which `run-story.sh` reads as "all tracked tasks done" and follows
    // straight to the story-close gate. So an interrupted `/user-story` —
    // which writes this skeleton before appending tasks — leaves behind an
    // artifact that does not look broken, it looks FINISHED.
    //
    // There is no valid state this excludes: at planning time the manifest
    // holds the planned set, and completed tasks remain in the list marked
    // `completed` rather than being removed.
    //
    // It composes with the emission route because `add-task` goes through
    // `load()` and never calls this function, so the skeleton stays fully
    // usable by the one tool whose job is to populate it while never passing
    // the gate that declares a story runnable. Transient by construction,
    // not by convention. `preflight-story.sh` and the Layer-3 guard both run
    // `validate`, so a skeleton left on disk stops the runner and reds CI.
    if manifest.tasks.is_empty() {
        violations.push(
            "manifest has no tasks — an empty task list asserts the story is COMPLETE (`next` \
             exits 3 AllDone). If this is a planning skeleton, finish emitting tasks into it."
                .to_string(),
        );
    }

    let mut ids: HashSet<u32> = HashSet::new();
    let mut duplicates = false;
    for task in &manifest.tasks {
        if !ids.insert(task.id) {
            violations.push(format!("duplicate task id {}", task.id));
            duplicates = true;
        }
    }

    for task in &manifest.tasks {
        for dep in &task.deps {
            if !ids.contains(dep) {
                violations.push(format!(
                    "task {} dep {dep} references no existing task",
                    task.id
                ));
            }
        }
    }

    // Cycle detection is only meaningful once ids are unique.
    if !duplicates {
        let members = cycle_members(&manifest.tasks, &ids);
        if !members.is_empty() {
            let list: Vec<String> = members.iter().map(u32::to_string).collect();
            violations.push(format!(
                "dependency cycle involving tasks [{}]",
                list.join(", ")
            ));
        }
    }

    for task in &manifest.tasks {
        if task.status != Status::Pending {
            continue;
        }
        if task
            .specialist
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
        {
            violations.push(format!("pending task {} has no specialist", task.id));
        }
        if task.env_tests.is_none() {
            violations.push(format!("pending task {} has no env_tests value", task.id));
        }
        if task.prompt.as_deref().is_none_or(|s| s.trim().is_empty()) {
            violations.push(format!("pending task {} has no prompt", task.id));
        }
    }

    violations
}

/// Kahn's algorithm over dep edges (dangling deps ignored — reported
/// separately). Returns the ids left unprocessed, i.e. cycle members.
fn cycle_members(tasks: &[Task], ids: &HashSet<u32>) -> Vec<u32> {
    let mut indegree: BTreeMap<u32, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut dependents: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for task in tasks {
        for dep in &task.deps {
            if ids.contains(dep) {
                if let Some(count) = indegree.get_mut(&task.id) {
                    *count += 1;
                }
                dependents.entry(*dep).or_default().push(task.id);
            }
        }
    }

    let mut queue: VecDeque<u32> = indegree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    while let Some(id) = queue.pop_front() {
        indegree.remove(&id);
        for dependent in dependents.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(count) = indegree.get_mut(dependent) {
                *count -= 1;
                if *count == 0 {
                    queue.push_back(*dependent);
                }
            }
        }
    }

    indegree.into_keys().collect()
}

/// Atomically replace `path`: write a temp file in the same directory,
/// then rename over the original.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("failed to create temp file in {}", dir.display()))?;
    tmp.write_all(contents.as_bytes())
        .context("failed to write temp file")?;
    tmp.flush().context("failed to flush temp file")?;
    tmp.persist(path)
        .with_context(|| format!("failed to rename temp file over {}", path.display()))?;
    Ok(())
}

/// Rewrite ONLY the fenced manifest block of `text` from `manifest`,
/// preserving every other byte, and return the new file contents. The
/// re-serialized block keeps the `# task-metadata (dt-story manifest v1)`
/// comment as its first line.
pub fn rewrite_story(
    text: &str,
    span: &markdown::BlockSpan,
    manifest: &Manifest,
) -> Result<String> {
    let body = manifest.to_block_body()?;
    markdown::replace_manifest_block(text, span, &body)
}
