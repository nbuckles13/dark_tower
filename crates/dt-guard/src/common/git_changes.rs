//! Changed-file detection — Rust port of `scripts/guards/common.sh::get_*_files`.
//!
//! Wave-2 tightening per the existing `docs/TODO.md` §Polyglot Pipeline
//! Follow-ups entry: bash today filters via `grep "${ext}$"` where `${ext}` is
//! regex-interpreted (so `.test.ts` matches `featureXtest.ts` because `.`
//! is any-char). This module uses **literal-suffix matching** via
//! `str::ends_with`. The bash callers of `common.sh::get_*_files` are NOT
//! modified by this devloop — their false-positives remain a separate-devloop
//! concern per the TODO entry. The Rust callers (Wave-2 subcommands) get the
//! tightening for free.
//!
//! ## Diff-base resolution
//!
//! Forwarded to `scripts/lang/_get_base_ref.sh` (ADR-0033 §7 canonical
//! resolver) — same single SoT as bash today's `get_diff_base`. We invoke it
//! via `std::process::Command` rather than reimplementing the merge-base /
//! ref-validation logic in Rust.
//!
//! ## `--diff-filter=D` exclusive (no rename detection)
//!
//! Per @paired-client F2 — [`get_deleted_files`] pins `--diff-filter=D`
//! exclusively. NO `-M` / `-C` rename detection. A rename appears as
//! delete+add by design; downstream consumers (`ts_test_removal`) catch the
//! rename case via same-basename / same-package matching.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve the diff base ref. Forwards to `scripts/lang/_get_base_ref.sh`
/// per ADR-0033 §7.
///
/// Returns the SHA / ref-name string emitted by the resolver. Callers pass
/// this back to `git diff <base> --` invocations.
pub fn get_diff_base(repo_root: &Path) -> Result<String> {
    let script = repo_root.join("scripts/lang/_get_base_ref.sh");
    if !script.exists() {
        anyhow::bail!(
            "diff-base resolver missing at {} (ADR-0033 §7 forwarder gone?)",
            script.display()
        );
    }
    let output = Command::new(&script)
        .output()
        .with_context(|| format!("running {}", script.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("diff-base resolver failed: {stderr}");
    }
    let raw =
        String::from_utf8(output.stdout).context("diff-base resolver emitted non-UTF-8 output")?;
    Ok(raw.trim().to_string())
}

/// Files modified vs. the diff base, literal-suffix filtered.
///
/// Per @paired-client F2 + the bash `common.sh::get_*_files` regex-bug
/// tightening — extensions are matched literally via `ends_with`. Pass
/// `&[]` to skip suffix filtering (matches bash's empty-ext behavior).
pub fn get_modified_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let base = get_diff_base(repo_root)?;
    git_diff_name_only(
        repo_root,
        &["diff", "--name-only", &base, "--"],
        search_path,
        extensions,
    )
}

/// Files added (status `A`) vs. the diff base.
pub fn get_added_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let base = get_diff_base(repo_root)?;
    git_diff_name_only(
        repo_root,
        &["diff", "--name-only", "--diff-filter=A", &base, "--"],
        search_path,
        extensions,
    )
}

/// Files deleted (status `D`) vs. the diff base. **Pins `--diff-filter=D`
/// exclusively** — NO `-M`/`-C` rename detection. Per @paired-client F2.
pub fn get_deleted_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let base = get_diff_base(repo_root)?;
    git_diff_name_only(
        repo_root,
        &["diff", "--name-only", "--diff-filter=D", &base, "--"],
        search_path,
        extensions,
    )
}

/// Untracked files (new files not yet added to git), literal-suffix filtered.
pub fn get_untracked_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let search_arg = search_path
        .to_str()
        .context("search_path is not valid UTF-8")?;
    let output = Command::new("git")
        .current_dir(repo_root)
        .args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            search_arg,
        ])
        .output()
        .with_context(|| format!("git ls-files in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-files failed: {stderr}");
    }
    Ok(filter_lines(&output.stdout, extensions))
}

/// Union of [`get_modified_files`] + [`get_untracked_files`], deduplicated.
pub fn get_all_changed_files(
    repo_root: &Path,
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let mut modified = get_modified_files(repo_root, search_path, extensions)?;
    let untracked = get_untracked_files(repo_root, search_path, extensions)?;
    modified.extend(untracked);
    modified.sort();
    modified.dedup();
    Ok(modified)
}

/// All tracked files matching `path_glob`. Wraps `git ls-files <glob>`.
///
/// Added Wave-2 2026-05-21 per @team-lead consolidation: policy modules
/// previously inlined this call (`todo_tracking::list_tracked_todos`); the
/// SoT-only-git rule routes it through here instead.
///
/// Pass `&[]` for `extensions` to disable suffix filtering (typical for
/// callers that already constrain via the glob pathspec).
pub fn get_tracked_files(
    repo_root: &Path,
    pathspec: &str,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["ls-files", "--", pathspec])
        .output()
        .with_context(|| format!("git ls-files {pathspec:?} in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-files failed: {stderr}");
    }
    Ok(filter_lines(&output.stdout, extensions))
}

/// Resolve the "active edit" scope per bash `validate-cross-boundary-scope`
/// semantics: working-tree vs HEAD when dirty, else HEAD^..HEAD (with HEAD^2
/// merge handling). Returns the paths touched in the active scope.
///
/// **Distinct from `get_all_changed_files`**: that helper uses the
/// `_get_base_ref.sh` merge-base, which includes every commit since branch
/// divergence (the polyglot-pipeline diff base). The active-edit scope is
/// narrower — only the current commit's diff — so cross-boundary-scope
/// detects drift in THIS edit, not the cumulative branch state.
///
/// Added Wave-2 2026-05-21 per @team-lead consolidation: the orchestrator
/// in `cross_boundary_scope.rs` previously inlined this logic; routes
/// through the SoT now.
pub fn get_active_edit_paths(repo_root: &Path) -> Result<Vec<PathBuf>> {
    let porcelain = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .with_context(|| format!("git status in {}", repo_root.display()))?;
    let dirty = porcelain.status.success() && !porcelain.stdout.is_empty();

    let mut out: Vec<PathBuf> = Vec::new();
    if dirty {
        // Pending: diff vs HEAD + untracked.
        let diff = Command::new("git")
            .current_dir(repo_root)
            .args(["diff", "HEAD", "--name-only"])
            .output()
            .with_context(|| format!("git diff HEAD in {}", repo_root.display()))?;
        if diff.status.success() {
            out.extend(filter_lines(&diff.stdout, &[]));
        }
        let untracked = Command::new("git")
            .current_dir(repo_root)
            .args(["ls-files", "--others", "--exclude-standard"])
            .output()
            .with_context(|| format!("git ls-files --others in {}", repo_root.display()))?;
        if untracked.status.success() {
            out.extend(filter_lines(&untracked.stdout, &[]));
        }
    } else {
        // Clean tree — diff HEAD^..HEAD (or HEAD^2..HEAD if HEAD is a merge).
        let head2 = Command::new("git")
            .current_dir(repo_root)
            .args(["rev-parse", "--verify", "--quiet", "HEAD^2"])
            .output()
            .ok();
        let effective_head = if let Some(out) = head2
            .as_ref()
            .filter(|o| o.status.success() && !o.stdout.is_empty())
        {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            String::from_utf8_lossy(
                &Command::new("git")
                    .current_dir(repo_root)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .with_context(|| format!("git rev-parse HEAD in {}", repo_root.display()))?
                    .stdout,
            )
            .trim()
            .to_string()
        };
        // Verify parent exists.
        let parent = Command::new("git")
            .current_dir(repo_root)
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{effective_head}^"),
            ])
            .output()
            .ok();
        if parent.as_ref().is_some_and(|o| o.status.success()) {
            let diff = Command::new("git")
                .current_dir(repo_root)
                .args([
                    "diff",
                    &format!("{effective_head}^"),
                    &effective_head,
                    "--name-only",
                ])
                .output()
                .with_context(|| {
                    format!(
                        "git diff {effective_head}^..{effective_head} in {}",
                        repo_root.display()
                    )
                })?;
            if diff.status.success() {
                out.extend(filter_lines(&diff.stdout, &[]));
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// True iff `path` is currently gitignored. Wraps `git check-ignore -q`.
///
/// Added Wave-2 2026-05-21 per @team-lead consolidation: previously
/// inlined in `knowledge_index::git_check_ignore`. Returns `Ok(false)` if
/// git is unavailable or the file is not tracked-but-ignored — same
/// fallback behavior as the prior inline implementation.
pub fn is_gitignored(repo_root: &Path, path: &str) -> Result<bool> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["check-ignore", "-q", "--", path])
        .output()
        .with_context(|| format!("git check-ignore {path:?} in {}", repo_root.display()))?;
    // `git check-ignore -q` exits 0 when path IS ignored, 1 when NOT ignored,
    // ≥2 on error. We treat any non-zero non-1 as Ok(false) — same fallback
    // as the prior inline implementation.
    Ok(output.status.success())
}

// --- helpers ---

fn git_diff_name_only(
    repo_root: &Path,
    args: &[&str],
    search_path: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>> {
    let search_arg = search_path
        .to_str()
        .context("search_path is not valid UTF-8")?;
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_root);
    cmd.args(args);
    cmd.arg(search_arg);
    let output = cmd
        .output()
        .with_context(|| format!("git {args:?} in {}", repo_root.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {args:?} failed: {stderr}");
    }
    Ok(filter_lines(&output.stdout, extensions))
}

/// Split git output on `\n`, drop empties + `vendor/` paths, apply literal-suffix
/// filter. Empty `extensions` skips suffix filtering.
fn filter_lines(stdout: &[u8], extensions: &[&str]) -> Vec<PathBuf> {
    let raw = String::from_utf8_lossy(stdout);
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("vendor/"))
        .filter(|line| extensions.is_empty() || extensions.iter().any(|ext| line.ends_with(ext)))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_lines_empty_extensions_passes_through() {
        let stdout = b"a.rs\nb.ts\nc.svelte\n";
        let out = filter_lines(stdout, &[]);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn filter_lines_literal_suffix_only() {
        // Regression for the bash `.test.ts` regex bug: `.test.ts` must NOT
        // match `featureXtest.ts` (the literal `.` does not match arbitrary
        // chars in our suffix filter).
        let stdout = b"foo.test.ts\nfeatureXtest.ts\nbar.test.tsx\nbaz.ts\n";
        let out = filter_lines(stdout, &[".test.ts"]);
        assert_eq!(out.len(), 1, "got {out:?}");
        assert_eq!(out[0], PathBuf::from("foo.test.ts"));
    }

    #[test]
    fn filter_lines_multiple_extensions_or_match() {
        let stdout = b"a.ts\nb.tsx\nc.svelte\nd.rs\n";
        let out = filter_lines(stdout, &[".ts", ".tsx", ".svelte"]);
        assert_eq!(out.len(), 3);
        assert!(out.contains(&PathBuf::from("a.ts")));
        assert!(out.contains(&PathBuf::from("b.tsx")));
        assert!(out.contains(&PathBuf::from("c.svelte")));
    }

    #[test]
    fn filter_lines_drops_vendor_paths() {
        let stdout = b"vendor/foo.rs\nsrc/main.rs\n";
        let out = filter_lines(stdout, &[".rs"]);
        assert_eq!(out, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn filter_lines_drops_empties_and_blank_lines() {
        let stdout = b"\n\na.rs\n\nb.rs\n\n";
        let out = filter_lines(stdout, &[".rs"]);
        assert_eq!(out.len(), 2);
    }
}
