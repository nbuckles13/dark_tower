//! dt-story binary entry point: pure clap dispatch + exit-code mapping.
//!
//! Exit-code contract (consumed by the story runner's bash wrapper):
//! * `next`     — 0 runnable (JSON on stdout), 3 no pending/escalated
//!   tasks left, 4 blocked (diagnosis on stderr), 2 malformed. Escalated
//!   tasks are retryable: selecting one reopens it (status back to
//!   pending, escalation cleared, file rewritten) with a stderr notice.
//! * `complete` / `escalate` — 0 on success, 2 on any error.
//! * `validate` — 0 valid, 1 with one violation per stderr line.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dt_story::engine::{self, NextOutcome};
use dt_story::manifest::Manifest;
use dt_story::markdown::{self, BlockSpan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const EXIT_MALFORMED: u8 = 2;
const EXIT_ALL_DONE: u8 = 3;
const EXIT_BLOCKED: u8 = 4;

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
    /// Check the story's manifest; print violations to stderr.
    Validate {
        /// Path to the user story markdown file.
        story: PathBuf,
    },
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
        Command::Validate { story } => cmd_validate(&story),
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
