//! `todo-tracking` subcommand — port of
//! `scripts/guards/simple/validate-todo-tracking.sh`.
//!
//! Two rules:
//! 1. Only `docs/TODO.md` may exist tree-wide. Any other `TODO.md` (tracked
//!    OR untracked) is a violation.
//! 2. Each `docs/devloop-outputs/*/main.md`'s §Accepted Deferrals (or older
//!    §Tech Debt Pointers) section is pointer-only — no inlined multi-line
//!    debt bodies. A "body" line is non-blank, not a bullet, not inside a
//!    code fence, not template prose. Two consecutive body lines trigger.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::{get_tracked_files, get_untracked_files};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const STRAY_TODO_RULE_ID: &str = "stray_todo";
pub const INLINE_DEBT_BODY_RULE_ID: &str = "inline_debt_body";

#[derive(Debug)]
struct Hit {
    rule_id: &'static str,
    detail: String,
    file: PathBuf,
    line: usize,
}

fn list_tracked_todos(repo_root: &Path) -> Result<Vec<PathBuf>> {
    // Per @team-lead 2026-05-21 SoT-only-git rule: route through
    // `common::git_changes::get_tracked_files` rather than inline shell-out.
    get_tracked_files(repo_root, "*TODO.md", &[]).or(Ok(Vec::new()))
}

fn list_untracked_todos(repo_root: &Path) -> Result<Vec<PathBuf>> {
    // The existing helper takes a `&Path` search root, not a glob pathspec,
    // so we pass `.` and filter post-hoc. This matches bash semantics —
    // bash's `git ls-files --others --exclude-standard '*TODO.md'` is
    // equivalent to "list every untracked file, keep those matching the
    // glob".
    let all = get_untracked_files(repo_root, Path::new("."), &[]).unwrap_or_default();
    Ok(all
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == "TODO.md" || n.ends_with("TODO.md"))
        })
        .collect())
}

/// Inspect a single main.md and return any inline-debt-body hits.
fn check_main_md(repo_root: &Path, main_md: &Path) -> Result<Vec<Hit>> {
    let content = std::fs::read_to_string(main_md)
        .with_context(|| format!("reading {}", main_md.display()))?;
    let mut hits: Vec<Hit> = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let rel = main_md
        .strip_prefix(repo_root)
        .unwrap_or(main_md)
        .to_path_buf();

    // Find the section start.
    let mut section_start: Option<usize> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed == "## Accepted Deferrals" || trimmed == "## Tech Debt Pointers" {
            section_start = Some(idx);
            break;
        }
    }
    let Some(start) = section_start else {
        return Ok(hits);
    };

    let mut in_fence = false;
    let mut body_run = 0usize;
    for (offset, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        // Section end on next H2.
        if trimmed.starts_with("## ") {
            break;
        }
        // Fence toggle.
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            body_run = 0;
            continue;
        }
        if in_fence {
            continue;
        }
        // Blank line resets the body counter.
        if line.trim().is_empty() {
            body_run = 0;
            continue;
        }
        // Bullet → not a body line.
        if trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with('+') {
            // First char must be followed by whitespace to count as a bullet.
            let bytes = trimmed.as_bytes();
            if bytes.get(1).is_some_and(|b| b.is_ascii_whitespace()) {
                body_run = 0;
                continue;
            }
        }
        // Template-shape prose: "Example", "Examples", "or" (case-sensitive).
        let stripped = trimmed.trim_end();
        let stripped_no_colon = stripped.trim_end_matches(':').trim_end();
        if matches!(stripped_no_colon, "Example" | "Examples" | "or") {
            body_run = 0;
            continue;
        }

        body_run += 1;
        if body_run >= 2 {
            hits.push(Hit {
                rule_id: INLINE_DEBT_BODY_RULE_ID,
                detail: "tech-debt body inlined; use pointer bullets instead".to_string(),
                file: rel.clone(),
                line: offset + 1,
            });
            break;
        }
    }
    Ok(hits)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let mut all_hits: Vec<Hit> = Vec::new();

    // Rule 1 — only `docs/TODO.md` allowed.
    let canonical = PathBuf::from("docs/TODO.md");
    for path in list_tracked_todos(repo_root)?
        .into_iter()
        .chain(list_untracked_todos(repo_root)?)
    {
        if path == canonical {
            continue;
        }
        all_hits.push(Hit {
            rule_id: STRAY_TODO_RULE_ID,
            detail: format!("stray TODO.md at {}", path.display()),
            file: path,
            line: 0,
        });
    }

    // Rule 2 — main.md §Accepted Deferrals pointer-only discipline.
    let outputs_dir = repo_root.join("docs/devloop-outputs");
    if outputs_dir.is_dir() {
        for entry in std::fs::read_dir(&outputs_dir)
            .with_context(|| format!("reading {}", outputs_dir.display()))?
            .flatten()
        {
            let sub = entry.path();
            if !sub.is_dir() {
                continue;
            }
            // Skip the template directory.
            if sub.file_name().is_some_and(|n| n == "_template") {
                continue;
            }
            let main_md = sub.join("main.md");
            if !main_md.is_file() {
                continue;
            }
            all_hits.extend(check_main_md(repo_root, &main_md)?);
        }
    }

    if all_hits.is_empty() {
        emit_ok("todo-tracking-clean");
        return Ok(());
    }

    for hit in &all_hits {
        let file_disp = hit.file.display().to_string();
        if explain {
            let policy = format!("todo-tracking::{}", hit.rule_id);
            print_finding(&Finding {
                file: &file_disp,
                row: hit.line,
                col: 0,
                policy: &policy,
                matched: &hit.detail,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} [{}] {}",
                file_disp, hit.line, hit.rule_id, hit.detail
            );
        }
    }

    let stray = all_hits
        .iter()
        .filter(|h| h.rule_id == STRAY_TODO_RULE_ID)
        .count();
    if stray > 0 {
        anyhow::bail!("todo-tracking-stray-{stray}")
    } else {
        anyhow::bail!("todo-tracking-inline-debt-body-{}", all_hits.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn pointer_only_section_passes() {
        let dir = TempDir::new().unwrap();
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        write(
            &main_md,
            "# Header\n\n## Accepted Deferrals\n\n- one-line pointer A\n- one-line pointer B\n\n## Other\n",
        );
        let hits = check_main_md(dir.path(), &main_md).unwrap();
        assert!(hits.is_empty(), "expected no hits: {hits:?}");
    }

    #[test]
    fn inline_body_two_consecutive_lines_flags() {
        let dir = TempDir::new().unwrap();
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        write(
            &main_md,
            "## Accepted Deferrals\n\nThis is paragraph text.\nThat continues here.\n",
        );
        let hits = check_main_md(dir.path(), &main_md).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rule_id, INLINE_DEBT_BODY_RULE_ID);
    }

    #[test]
    fn fenced_code_block_does_not_trigger() {
        let dir = TempDir::new().unwrap();
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        write(
            &main_md,
            "## Accepted Deferrals\n\n```\nmulti-line\nfenced block content\nstill fenced\n```\n\n- pointer\n",
        );
        let hits = check_main_md(dir.path(), &main_md).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn template_prose_does_not_trigger() {
        let dir = TempDir::new().unwrap();
        let main_md = dir.path().join("docs/devloop-outputs/x/main.md");
        write(
            &main_md,
            "## Accepted Deferrals\n\nExample:\n```\nfenced\n```\nor\n```\nanother\n```\n",
        );
        let hits = check_main_md(dir.path(), &main_md).unwrap();
        assert!(hits.is_empty(), "got {hits:?}");
    }
}
