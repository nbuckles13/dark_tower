//! Path-containment gate — single SoT for "is this resolved path inside the repo?"
//!
//! Per ADR-0034 §5 / @security commitment #19: consumed by both `cite_extract`
//! (doc-cite resolution) AND `alert_rules::validate_runbook_url` (Bundle 5a).
//! Re-implementing containment in any other module is a Gate 2 reject from
//! @security.
//!
//! Uses `std::fs::canonicalize` + `Path::starts_with` (component-wise, NOT
//! string-prefix — eliminates the `root + os.sep` footgun the Python version
//! needs). Per @test R3 retraction: dangling-symlink errors are exercised via
//! `.ok()?` on real filesystem behavior, not via mocked `canonicalize`.

use std::path::{Path, PathBuf};

/// Resolve `cited_path` under `repo_root`, enforcing in-repo containment.
///
/// Returns the canonicalized absolute path on success, or `None` if the
/// path escapes `repo_root` (traversal or symlink-escape) or cannot be
/// canonicalized (dangling symlink, ENOENT, etc.).
///
/// Callers distinguish file-missing from path-escape by checking
/// `result.is_some() && Path::is_file(result)` versus a `None` return.
///
/// Per @security commitment #17 — `Path::join("/tmp/foo/repo", "/etc/passwd")`
/// returns `"/etc/passwd"` (absolute-replaces-relative). The semantic is
/// "absolute paths skip the join-relative-to-root step and then face
/// containment," not "absolute paths always rejected."
pub fn resolve_cited_path(repo_root: &Path, cited_path: &str) -> Option<PathBuf> {
    let abs_target = repo_root.join(cited_path);
    let resolved = std::fs::canonicalize(&abs_target).ok()?;
    let root_real = std::fs::canonicalize(repo_root).ok()?;
    if resolved == root_real {
        return Some(resolved);
    }
    if !resolved.starts_with(&root_real) {
        return None;
    }
    Some(resolved)
}

/// Strip `repo_root` prefix from `path`, returning a repo-relative slash-path.
///
/// Per semantic-guard watch-point #3: violation messages and `--explain`
/// output must use repo-relative paths, never raw absolute paths from
/// `canonicalize()`. Falls back to the absolute path's `display()` only if
/// the prefix doesn't match (shouldn't happen for paths from
/// `resolve_cited_path`, but is a safe fallback).
pub fn to_repo_relative(repo_root: &Path, path: &Path) -> String {
    match path.strip_prefix(repo_root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
