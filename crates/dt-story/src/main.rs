//! dt-story binary entry point: pure clap dispatch + exit-code mapping.
//!
//! Exit-code contract (consumed by the story runner's bash wrapper):
//! * `next`     — 0 runnable (JSON on stdout), 3 no pending/escalated
//!   tasks left, 4 blocked (diagnosis on stderr), 2 malformed. Escalated
//!   tasks are retryable: selecting one reopens it (status back to
//!   pending, escalation cleared, file rewritten) with a stderr notice.
//! * `complete` / `escalate` — 0 on success, 2 on any error.
//! * `add-task` — 0 appended (new id on stdout), 4 a task with the same
//!   tag already exists (existing id on stdout, notice on stderr), 2 on
//!   file-read / parse / write error.
//! * `validate` — 0 valid, 1 with one violation per stderr line.
//! * `list-tasks <story>` — 0 with the manifest's tasks as a JSON array on
//!   stdout (`[]` when the manifest has no tasks — never empty output);
//!   2 on any read/parse/serialize error, with **empty stdout**. **No other
//!   exit code from the program's own logic.** Two things that claim does
//!   NOT cover, both established by measurement rather than assumed:
//!   (a) `--help` / `--version` / a usage error are clap's, not ours — they
//!   exit 0 (help, with non-JSON stdout) or 2, which is why the staleness
//!   probe in `preflight-story.sh` can use `list-tasks --help` at all;
//!   the JSON-array clause is scoped to invocations carrying a story path.
//!   (b) `println!` panics with rc **101** and *partial* stdout if stdout
//!   closes early (EPIPE) — a property of any stdout writer, shared with
//!   `cmd_next`, and unreachable from every in-tree consumer because they
//!   drain (command substitution, or a pipe into `jq`). Serializing before
//!   printing closes the *serialization*-failure hole below; it does not
//!   close the *write*-failure one, so do not read it as doing so.
//!   Unlike `next` (3, 4) and `add-task` (4), a
//!   read-only projection has no third state to report, so a consumer may
//!   read any non-zero as "this binary cannot answer" — which is what lets
//!   `preflight-story.sh` treat rc-0-with-empty-stdout as a contract
//!   violation rather than an expected shape. **Do not weaken "with a JSON
//!   array on stdout" to a bare "0 on success": that clause is asserted on
//!   by a `[ -n "$out" ]` check in `preflight-story.sh`, and nothing fails
//!   if it is softened here.** The guarantee is per-verb, not crate-wide —
//!   `validate` and `complete` both legitimately exit 0 with empty stdout,
//!   so it cannot be inferred from sibling verbs. Note rc 4 already means
//!   BLOCKED from `next` and TAG_EXISTS from `add-task`; this verb
//!   deliberately does not widen that ambiguity further.
//!   **Read-only**: it never writes the story file. `next` is not — it
//!   rewrites the file when it reopens an escalated task — which is why
//!   validating a flag against the manifest needs its own verb rather than
//!   a `next` call.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dt_story::engine::{self, NextOutcome};
use dt_story::manifest::{Manifest, Status};
use dt_story::markdown::{self, BlockSpan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const EXIT_MALFORMED: u8 = 2;
const EXIT_ALL_DONE: u8 = 3;
const EXIT_BLOCKED: u8 = 4;
/// `add-task`: a task with the requested tag already exists (idempotent).
const EXIT_TAG_EXISTS: u8 = 4;

#[derive(Parser, Debug)]
#[command(
    name = "dt-story",
    about = "State engine for the story runner: owns the embedded task manifest.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Resolve the next runnable task and print it as JSON on stdout.
    Next {
        /// Path to the user story markdown file.
        story: PathBuf,
    },
    /// Mark a pending task completed, optionally recording its commit.
    Complete {
        /// Path to the user story markdown file.
        story: PathBuf,
        /// Task id to complete.
        id: u32,
        /// Commit sha to record on the task.
        #[arg(long)]
        commit: Option<String>,
    },
    /// Mark a pending task escalated, recording the escalation log path.
    Escalate {
        /// Path to the user story markdown file.
        story: PathBuf,
        /// Task id to escalate.
        id: u32,
        /// Escalation reason token (recorded in the --out JSON).
        #[arg(long)]
        reason: String,
        /// Path to the escalation log (recorded in the manifest).
        #[arg(long)]
        log: String,
        /// Optional path to write an escalation JSON summary.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Append a pending task, idempotent on --tag. Prints the task id.
    AddTask {
        /// Path to the user story markdown file.
        story: PathBuf,
        /// Implementing specialist for the new task.
        #[arg(long)]
        specialist: String,
        /// File whose contents become the task's devloop prompt.
        #[arg(long)]
        prompt_file: PathBuf,
        /// Idempotency key: a second add-task with this tag is a no-op.
        #[arg(long)]
        tag: String,
        /// Mark the task as requiring env-tests (default false).
        #[arg(long)]
        env_tests: bool,
    },
    /// Check the story's manifest; print violations to stderr.
    Validate {
        /// Path to the user story markdown file.
        story: PathBuf,
    },
    /// Print the manifest's tasks as a JSON array. Read-only.
    ListTasks {
        /// Path to the user story markdown file.
        story: PathBuf,
    },
}

/// JSON payload printed by `dt-story list-tasks`: a projection of the
/// manifest onto the fields the story runner reads — deliberately **not** a
/// serialization of [`dt_story::manifest::Task`], which is the *storage*
/// type. Serializing the storage type would couple this output to manifest
/// schema evolution and would emit fields no consumer reads.
///
/// Four rules govern changes to this struct. They are held apart rather than
/// merged into "don't emit extra fields" because they fail for unrelated
/// reasons and would need unrelated fixes:
///
/// 1. **Every field has a named consumer in `scripts/workflow/run-story.sh`;
///    add one only together with its reader.** `id` and `status` drive
///    `--stop-after` validation. `deps` drives the deps-aware refusal
///    ("task 3 exists but is unreachable: dep 2 is pending"), which needs
///    each *dep's* status — that is why this is the whole array and not a
///    per-id lookup. Do not "optimise" it into a single-task query.
/// 2. **Never project an uncontrolled value.** `prompt` is arbitrary text;
///    keeping it off consumer command lines is the point of the runner's
///    out-of-band prompt handling.
/// 3. **Never project a reference whose referent can be invalidated without
///    this artifact being rewritten.** `escalation` holds a `$RUN_DIR` path
///    that vanishes with the container; `commit` holds a sha that the
///    runner's post-completion `git commit --amend` invalidates by
///    construction (`run-story.sh` says so where it declines to set it).
///    Different mechanisms, same hazard — and neither argument finds the
///    other's instance, so test the property, not the shape: both are
///    `Option<String>`, so references are invisible to the type system.
/// 4. **`deps` is not `skip_serializing_if`.** Consumers get a total shape,
///    so `jq` never needs `// []`. Storage-side optionality is a YAML
///    round-tripping concern and does not belong on the wire.
#[derive(Debug, serde::Serialize)]
struct TaskSummary<'a> {
    id: u32,
    status: Status,
    deps: &'a [u32],
}

/// A loaded story file: full text, manifest block span, parsed manifest.
struct Doc {
    text: String,
    span: BlockSpan,
    manifest: Manifest,
}

fn load(path: &Path) -> Result<Doc> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let span = markdown::find_manifest_block(&text)?;
    let yaml = markdown::manifest_yaml(&text, &span)?;
    let manifest = Manifest::from_yaml(yaml)?;
    Ok(Doc {
        text,
        span,
        manifest,
    })
}

fn save(path: &Path, doc: &Doc) -> Result<()> {
    let new_text = engine::rewrite_story(&doc.text, &doc.span, &doc.manifest)?;
    engine::write_atomic(path, &new_text)
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Next { story } => cmd_next(&story),
        Command::Complete { story, id, commit } => exit_on_error(cmd_complete(&story, id, commit)),
        Command::Escalate {
            story,
            id,
            reason,
            log,
            out,
        } => exit_on_error(cmd_escalate(&story, id, &reason, &log, out.as_deref())),
        Command::AddTask {
            story,
            specialist,
            prompt_file,
            tag,
            env_tests,
        } => cmd_add_task(&story, &specialist, &prompt_file, &tag, env_tests),
        Command::Validate { story } => cmd_validate(&story),
        Command::ListTasks { story } => cmd_list_tasks(&story),
    }
}

/// Map mutation results onto the exit-code contract (0 ok, 2 error).
fn exit_on_error(result: Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dt-story: {e:#}");
            ExitCode::from(EXIT_MALFORMED)
        }
    }
}

fn cmd_next(story: &Path) -> ExitCode {
    let mut doc = match load(story) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("dt-story: {e:#}");
            return ExitCode::from(EXIT_MALFORMED);
        }
    };
    match engine::next(&mut doc.manifest) {
        Ok(NextOutcome::Runnable { task, reopened }) => match serde_json::to_string(&task) {
            Ok(json) => {
                // Persist the escalated→pending reopen before emitting the
                // payload; only reopens rewrite the file — a plain `next`
                // stays read-only.
                if reopened {
                    if let Err(e) = save(story, &doc) {
                        eprintln!("dt-story: {e:#}");
                        return ExitCode::from(EXIT_MALFORMED);
                    }
                    eprintln!("dt-story: reopening escalated task {} for retry", task.id);
                }
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("dt-story: failed to serialize task JSON: {e}");
                ExitCode::from(EXIT_MALFORMED)
            }
        },
        Ok(NextOutcome::AllDone) => ExitCode::from(EXIT_ALL_DONE),
        Ok(NextOutcome::Blocked(diagnosis)) => {
            eprintln!("dt-story: {diagnosis}");
            ExitCode::from(EXIT_BLOCKED)
        }
        Err(e) => {
            eprintln!("dt-story: {e:#}");
            ExitCode::from(EXIT_MALFORMED)
        }
    }
}

fn cmd_complete(story: &Path, id: u32, commit: Option<String>) -> Result<()> {
    let mut doc = load(story)?;
    // Warn (don't fail) if the task is already completed against a different
    // recorded commit — a genuine conflict worth surfacing, but not one that
    // should red the runner. Read the existing commit before the mutation.
    let prior_commit = doc
        .manifest
        .tasks
        .iter()
        .find(|t| t.id == id)
        .and_then(|t| t.commit.clone());
    match engine::complete(&mut doc.manifest, id, commit.clone())? {
        engine::CompleteOutcome::Completed => save(story, &doc),
        engine::CompleteOutcome::AlreadyComplete => {
            if let (Some(new), Some(old)) = (&commit, &prior_commit) {
                if new != old {
                    eprintln!(
                        "dt-story: task {id} already completed at {old}; not overwriting with {new}"
                    );
                }
            }
            eprintln!("dt-story: task {id} already completed — no change");
            Ok(())
        }
    }
}

fn cmd_escalate(story: &Path, id: u32, reason: &str, log: &str, out: Option<&Path>) -> Result<()> {
    let mut doc = load(story)?;
    engine::escalate(&mut doc.manifest, id, log)?;
    save(story, &doc)?;
    if let Some(out_path) = out {
        /// Escalation summary written to `--out` (stable key order).
        #[derive(serde::Serialize)]
        struct EscalationOut<'a> {
            story: &'a str,
            task_id: u32,
            reason: &'a str,
            log: &'a str,
        }
        let payload = EscalationOut {
            story: &doc.manifest.story,
            task_id: id,
            reason,
            log,
        };
        let mut body =
            serde_json::to_string(&payload).context("failed to serialize escalation JSON")?;
        body.push('\n');
        fs::write(out_path, body)
            .with_context(|| format!("failed to write {}", out_path.display()))?;
    }
    Ok(())
}

fn cmd_add_task(
    story: &Path,
    specialist: &str,
    prompt_file: &Path,
    tag: &str,
    env_tests: bool,
) -> ExitCode {
    // Read the prompt first: a bad --prompt-file is a caller error (exit 2),
    // distinct from a manifest/write error, but both map to EXIT_MALFORMED.
    let result = (|| -> Result<engine::AddOutcome> {
        let prompt = fs::read_to_string(prompt_file)
            .with_context(|| format!("failed to read prompt file {}", prompt_file.display()))?;
        // Trim only trailing whitespace; internal newlines are load-bearing
        // (multi-line devloop prompt round-tripping as a YAML block scalar).
        let prompt = prompt.trim_end().to_string();
        let mut doc = load(story)?;
        let outcome = engine::add_task(
            &mut doc.manifest,
            specialist.to_string(),
            prompt,
            env_tests,
            tag.to_string(),
        )?;
        if let engine::AddOutcome::Added(_) = outcome {
            save(story, &doc)?;
        }
        Ok(outcome)
    })();

    match result {
        Ok(engine::AddOutcome::Added(id)) => {
            println!("{id}");
            ExitCode::SUCCESS
        }
        Ok(engine::AddOutcome::Exists(id)) => {
            println!("{id}");
            eprintln!("dt-story: task with tag '{tag}' already exists (id {id})");
            ExitCode::from(EXIT_TAG_EXISTS)
        }
        Err(e) => {
            eprintln!("dt-story: {e:#}");
            ExitCode::from(EXIT_MALFORMED)
        }
    }
}

/// Read-only projection of the manifest's tasks onto stdout as JSON, in
/// manifest order. Order is the file's own: re-sorting here would be
/// `dt-story` forming a second opinion about a document whose task order
/// [`Manifest::to_block_body`] otherwise preserves exactly. Consumers that
/// want ids sorted can sort in `jq`, where it is a presentation choice.
fn cmd_list_tasks(story: &Path) -> ExitCode {
    let doc = match load(story) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("dt-story: {e:#}");
            return ExitCode::from(EXIT_MALFORMED);
        }
    };
    let summaries: Vec<TaskSummary<'_>> = doc
        .manifest
        .tasks
        .iter()
        .map(|task| TaskSummary {
            id: task.id,
            status: task.status,
            deps: &task.deps,
        })
        .collect();
    // Serialize fully BEFORE printing anything, so a SERIALIZATION failure
    // cannot leave partial bytes on stdout alongside a non-zero exit —
    // that would break the "exit 2 => empty stdout" guarantee preflight's
    // non-empty check relies on to tell a contract violation from an
    // expected shape. `cmd_next` does the same.
    //
    // Scope, deliberately stated: this closes the serialization-failure
    // hole, NOT the write-failure one. A payload larger than the pipe
    // buffer spans several write syscalls, so a reader that closes early
    // still yields partial stdout with rc 101 (measured). Unreachable from
    // every in-tree consumer — all of them drain — but do not read the
    // ordering below as a guarantee against it.
    match serde_json::to_string(&summaries) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dt-story: failed to serialize task list JSON: {e}");
            ExitCode::from(EXIT_MALFORMED)
        }
    }
}

fn cmd_validate(story: &Path) -> ExitCode {
    let text = match fs::read_to_string(story) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("failed to read {}: {e}", story.display());
            return ExitCode::FAILURE;
        }
    };
    let violations = engine::validate_story(&text);
    if violations.is_empty() {
        return ExitCode::SUCCESS;
    }
    for violation in &violations {
        eprintln!("{violation}");
    }
    ExitCode::FAILURE
}
