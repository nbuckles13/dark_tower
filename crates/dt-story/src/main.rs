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

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dt_story::engine::{self, NextOutcome};
use dt_story::manifest::{Manifest, Slug, Status};
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
    /// Mark a pending task completed, optionally recording its commit/slug.
    Complete {
        /// Path to the user story markdown file.
        story: PathBuf,
        /// Task id to complete.
        id: u32,
        /// Commit sha to record on the task.
        #[arg(long)]
        commit: Option<String>,
        /// Devloop-output slug of the attempt that completed this task.
        /// Last-writer-wins: supplying it on an already-completed task
        /// REPAIRS a missing or stale slug. Validated at the clap boundary
        /// as well as in the deserializer, because this flag is a second
        /// producer (a model on a command line) less constrained than the
        /// runner's own derived value.
        #[arg(long)]
        slug: Option<Slug>,
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
        /// WEAK idempotency — the no-op does NOT apply the other flags.
        #[arg(long)]
        tag: String,
        /// Mark the task as requiring env-tests (default false).
        #[arg(long)]
        env_tests: bool,
        /// Comma-separated dependency task ids, e.g. `--deps 1,2`.
        /// Omit the flag for no deps; `--deps ''` is an error, because a
        /// caller with an unset variable emits exactly that and a silently
        /// dep-free task is a masked failure.
        /// `Deps` newtype, not `Vec<u32>`: clap's derive reads a bare `Vec<T>`
        /// as a MULTI-VALUE argument and tries to parse each occurrence as
        /// `T`, which conflicts with our single-token comma-separated parser.
        #[arg(long, value_parser = parse_deps)]
        deps: Option<Deps>,
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
/// 1. **Every field has a NAMED consumer, and the consumer is named here;
///    add a field only together with its reader.** `id` and `status` drive
///    `run-story.sh`'s `--stop-after` validation. `deps` drives its
///    deps-aware refusal ("task 3 exists but is unreachable: dep 2 is
///    pending"), which needs each *dep's* status — that is why this is the
///    whole array and not a per-id lookup. Do not "optimise" it into a
///    single-task query. `slug` is read by `/close-story` Phase 1 (mapping
///    completed tasks to their devloop outputs) and Phase 4 (PR synthesis).
///    This rule previously said "a named consumer **in
///    `scripts/workflow/run-story.sh`**"; that stopped being true when
///    `/close-story` became a consumer, and it is widened here rather than
///    silently reread — the rule's force is "no field without a reader", not
///    "the runner is the only reader".
/// 2. **Never project an uncontrolled value.** `prompt` is arbitrary text;
///    keeping it off consumer command lines is the point of the runner's
///    out-of-band prompt handling. `slug` satisfies this BY CONSTRUCTION
///    rather than by a producer-side filter: [`Slug`] is a newtype whose
///    only constructor enforces `SLUG_PATTERN` in the deserializer, so an
///    unvalidated value cannot exist in a parsed `Task`. That type is a
///    PRECONDITION for projecting `slug` at all — a devloop-output directory
///    name is model-produced, exactly the trust level this rule exists for.
/// 3. **Never project a reference whose referent can be invalidated without
///    this artifact being rewritten** — unless it is admitted on the
///    revalidate-at-use branch, and says so. `escalation` holds a `$RUN_DIR`
///    path that vanishes with the container; `commit` holds a sha that the
///    runner's post-completion `git commit --amend` invalidates by
///    construction (`run-story.sh` says so where it declines to set it).
///    Different mechanisms, same hazard — and neither argument finds the
///    other's instance, so test the property, not the shape: both are
///    `Option<String>`, so references are invisible to the type system.
///
///    `slug` IS such a reference and is admitted anyway, on the explicit
///    condition that consumers revalidate. It is not durable-by-nature: it
///    travels with its referent (the runner writes it in the same commit
///    that contains `docs/devloop-outputs/<slug>/`, which is git-tracked),
///    but a later commit deleting or renaming that directory invalidates the
///    slug while the story file stays byte-identical — the literal test the
///    `docs/TODO.md` referent-durability entry proposes. So "travels with",
///    NOT "cannot break". The remedy is the one that entry documents as the
///    codebase's own positive instance: revalidate at use. `/close-story`
///    re-checks that `main.md` exists before constructing paths from a slug,
///    mirroring how `run-story.sh` re-checks `$slug_file` rather than
///    trusting it on read.
/// 4. **`deps` is not `skip_serializing_if`.** Consumers get a total shape,
///    so `jq` never needs `// []`. Storage-side optionality is a YAML
///    round-tripping concern and does not belong on the wire. `slug` is
///    likewise projected as an always-present key (`null` when unset), so a
///    consumer never has to distinguish absent from empty.
#[derive(Debug, serde::Serialize)]
struct TaskSummary<'a> {
    id: u32,
    status: Status,
    deps: &'a [u32],
    slug: Option<&'a str>,
}

/// Parse `--deps 1,2,3` into ids. Our own parser rather than clap's
/// `value_delimiter`: a single quotable token is auditable in an emitted
/// command line, where repeated flags force bash array-building and a
/// variable-length command line — first-class concerns after R-2 defect 5 —
/// and it mirrors the manifest's own YAML sequence.
///
/// Every rejection is explicit and none silently normalises: an empty value,
/// an empty element, a non-numeric element, and DUPLICATES (a repeated dep
/// is a caller bug; deduping would hide it).
#[derive(Debug, Clone)]
struct Deps(Vec<u32>);

fn parse_deps(raw: &str) -> std::result::Result<Deps, String> {
    if raw.trim().is_empty() {
        return Err(
            "--deps was given an empty value. Omit the flag entirely for a task with no \
             dependencies; an empty value usually means an unset variable in the caller."
                .to_string(),
        );
    }
    let mut ids = Vec::new();
    for element in raw.split(',') {
        let element = element.trim();
        if element.is_empty() {
            return Err(format!("--deps '{raw}' has an empty element"));
        }
        let id: u32 = element
            .parse()
            .map_err(|_| format!("--deps element '{element}' is not a task id"))?;
        if ids.contains(&id) {
            return Err(format!("--deps '{raw}' repeats dep {id}"));
        }
        ids.push(id);
    }
    Ok(Deps(ids))
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

/// Write the mutated manifest back, then PROVE the file still round-trips.
///
/// This is the single chokepoint for every mutating verb (`next`'s reopen,
/// `complete`, `escalate`, `add-task`), so the post-write check makes ANY
/// corruption mechanism loud at write time — including ones nobody has
/// thought of — rather than at `next`-returns-AllDone time, where a
/// truncated manifest is indistinguishable from a finished story. That
/// signature (green over work that never ran) is the one ADR-0035 §3 exists
/// to prevent, which is why this is worth a re-read of the file.
///
/// Scope, stated so it is not over-read: it detects corruption INTRODUCED by
/// this write. It cannot detect corruption INHERITED — a file that was
/// already truncated when we loaded it round-trips its own truncation
/// perfectly. `markdown::find_manifest_block`'s orphan scan covers that.
fn save(path: &Path, doc: &Doc) -> Result<()> {
    let new_text = engine::rewrite_story(&doc.text, &doc.span, &doc.manifest)?;
    engine::write_atomic(path, &new_text)?;

    // Compare the RE-SERIALIZED BODY, not the task-id vector. Ids are the
    // wrong comparand for the loss class this crate defends against: a
    // truncation inside the LAST task's prompt leaves every `- id: N` line
    // intact, so an id comparison passes over a mangled file. Body equality
    // catches that, plus prompt/slug/status changes and mechanisms nobody has
    // enumerated — which is the only job this check is here to do.
    // (`Task` derives no `PartialEq`, hence comparing serialized form rather
    // than the structs.)
    let expected = doc.manifest.to_block_body()?;
    let reloaded = load(path).with_context(|| {
        format!(
            "WROTE A MANIFEST THAT NO LONGER PARSES ({}). The write has landed on disk; inspect \
             the file before rerunning.",
            path.display()
        )
    })?;
    if reloaded.manifest.to_block_body()? != expected {
        bail!(
            "manifest did not round-trip: reading {} back yields a different manifest than was \
             written. The file on disk is corrupt — most likely a prompt containing a markdown \
             fence truncated the block.",
            path.display()
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Next { story } => cmd_next(&story),
        Command::Complete {
            story,
            id,
            commit,
            slug,
        } => exit_on_error(cmd_complete(&story, id, commit, slug)),
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
            deps,
        } => cmd_add_task(
            &story,
            &specialist,
            &prompt_file,
            &tag,
            env_tests,
            deps.map(|d| d.0).unwrap_or_default(),
        ),
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

fn cmd_complete(story: &Path, id: u32, commit: Option<String>, slug: Option<Slug>) -> Result<()> {
    let mut doc = load(story)?;
    // Read prior values before the mutation. `commit` conflicts warn but do
    // not overwrite; `slug` conflicts warn AND overwrite (last-writer-wins).
    // Both are loud — same visibility, opposite action, which is what makes
    // the pair legible rather than looking like an inconsistency.
    let prior = doc.manifest.tasks.iter().find(|t| t.id == id);
    let prior_commit = prior.and_then(|t| t.commit.clone());
    let prior_slug = prior.and_then(|t| t.slug.clone());
    match engine::complete(&mut doc.manifest, id, commit.clone(), slug.clone())? {
        engine::CompleteOutcome::Completed => save(story, &doc),
        engine::CompleteOutcome::AlreadyComplete { slug_updated } => {
            if let (Some(new), Some(old)) = (&commit, &prior_commit) {
                if new != old {
                    eprintln!(
                        "dt-story: task {id} already completed at {old}; not overwriting with {new}"
                    );
                }
            }
            if slug_updated {
                match &prior_slug {
                    Some(old) => eprintln!(
                        "dt-story: task {id} already completed; replacing slug {old} with {} \
                         (last-writer-wins)",
                        slug.as_ref().map_or("<none>", Slug::as_str)
                    ),
                    None => eprintln!(
                        "dt-story: task {id} already completed; recording missing slug {}",
                        slug.as_ref().map_or("<none>", Slug::as_str)
                    ),
                }
                save(story, &doc)
            } else {
                eprintln!("dt-story: task {id} already completed — no change");
                Ok(())
            }
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
    deps: Vec<u32>,
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
            engine::NewTask {
                specialist: specialist.to_string(),
                prompt,
                env_tests,
                tag: tag.to_string(),
                deps,
            },
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
            // Name what was NOT done. The tag check returns before any field
            // is written, so every supplied flag was accepted and discarded;
            // silently reporting "already exists" reads as convergence and
            // is not. A caller re-running with a CORRECTED dependency graph
            // gets no error and no edit.
            eprintln!(
                "dt-story: task with tag '{tag}' already exists (id {id}) — NOT UPDATED. The \
                 supplied --specialist/--prompt-file/--env-tests/--deps were NOT applied; this \
                 verb appends, it does not update. To change an existing task, edit the manifest \
                 block directly, or reset it to a skeleton and re-emit the whole plan."
            );
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
            slug: task.slug.as_ref().map(dt_story::manifest::Slug::as_str),
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
