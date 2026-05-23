//! `test-coverage` subcommand — port of
//! `scripts/guards/simple/test-coverage.sh` (quick-mode only).
//!
//! Per the Wave-2 plan §Accepted Deferrals: `--full` mode (`cargo llvm-cov`
//! threshold enforcement) is deferred to Wave 3. Wave-2 ports the quick-mode
//! heuristic only: for each NEW (added or untracked) production `.rs` file,
//! check that it has a corresponding test (matching `_test.rs` sibling,
//! `tests/` sibling, OR inline `#[cfg(test)]` block). Always exits 0 — quick
//! mode is warning-only per bash.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::{get_added_files, get_untracked_files};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const MISSING_TEST_RULE_ID: &str = "missing_test";

fn is_production_rs(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    if !s.ends_with(".rs") {
        return false;
    }
    if s.ends_with("_test.rs") {
        return false;
    }
    if s.contains("/tests/") || s.contains("/fuzz/") {
        return false;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("test_") {
            return false;
        }
    }
    true
}

fn is_test_addition(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    s.ends_with("_test.rs") || s.contains("/tests/") || s.contains("/fuzz/") || {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.starts_with("test_"))
    }
}

fn has_corresponding_test(
    repo_root: &Path,
    prod_path: &Path,
    new_test_files: &[PathBuf],
) -> Result<bool> {
    let abs = repo_root.join(prod_path);
    let Some(stem) = prod_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(String::from)
    else {
        return Ok(false);
    };
    let dir = prod_path.parent().unwrap_or_else(|| Path::new(""));

    // Sibling `_test.rs`.
    let sibling_test_name = format!("{stem}_test.rs");
    if new_test_files
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(&sibling_test_name))
    {
        return Ok(true);
    }
    // `tests/` sibling directory added under the production module's dir.
    let tests_dir_str = format!("{}/tests/", dir.display());
    if new_test_files
        .iter()
        .any(|p| p.to_str().is_some_and(|s| s.starts_with(&tests_dir_str)))
    {
        return Ok(true);
    }
    // Inline `#[cfg(test)]` in the production file itself.
    if abs.is_file() {
        let content =
            std::fs::read_to_string(&abs).with_context(|| format!("reading {}", abs.display()))?;
        if content.contains("#[cfg(test)]") {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let added = get_added_files(repo_root, Path::new("."), &[".rs"])
        .context("collecting added Rust files")?;
    let untracked = get_untracked_files(repo_root, Path::new("."), &[".rs"])
        .context("collecting untracked Rust files")?;
    let mut all_new: Vec<PathBuf> = added.into_iter().chain(untracked).collect();
    all_new.sort();
    all_new.dedup();

    let new_prod: Vec<PathBuf> = all_new
        .iter()
        .filter(|p| is_production_rs(p))
        .cloned()
        .collect();
    let new_test: Vec<PathBuf> = all_new
        .iter()
        .filter(|p| is_test_addition(p))
        .cloned()
        .collect();

    if new_prod.is_empty() {
        emit_ok("test-coverage-no-new-prod-files");
        return Ok(());
    }

    let mut warnings: Vec<PathBuf> = Vec::new();
    for prod in &new_prod {
        if !has_corresponding_test(repo_root, prod, &new_test)? {
            warnings.push(prod.clone());
        }
    }

    if warnings.is_empty() {
        emit_ok(format!(
            "test-coverage-all-{}-files-covered",
            new_prod.len()
        ));
        return Ok(());
    }

    // Quick mode: warning-only. Emit WARNING lines + STATUS=OK. Per bash today.
    for path in &warnings {
        let file_disp = path.display().to_string();
        if explain {
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: "test-coverage::missing_test",
                matched: "no corresponding test file or #[cfg(test)] module found",
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "WARNING: {} — no corresponding test file or #[cfg(test)] module found",
                file_disp
            );
        }
    }
    emit_ok(format!("test-coverage-{}-warnings", warnings.len()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_production_rs_classifies_correctly() {
        assert!(is_production_rs(Path::new("crates/foo/src/lib.rs")));
        assert!(is_production_rs(Path::new("crates/foo/src/sub/mod.rs")));
        assert!(!is_production_rs(Path::new("crates/foo/src/lib_test.rs")));
        assert!(!is_production_rs(Path::new("crates/foo/tests/it.rs")));
        assert!(!is_production_rs(Path::new(
            "crates/foo/src/test_helpers.rs"
        )));
        assert!(!is_production_rs(Path::new("crates/foo/fuzz/main.rs")));
        assert!(!is_production_rs(Path::new("crates/foo/src/lib.toml")));
    }

    #[test]
    fn is_test_addition_recognizes_test_paths() {
        assert!(is_test_addition(Path::new("crates/foo/src/lib_test.rs")));
        assert!(is_test_addition(Path::new("crates/foo/tests/it.rs")));
        assert!(is_test_addition(Path::new(
            "crates/foo/src/test_helpers.rs"
        )));
        assert!(!is_test_addition(Path::new("crates/foo/src/lib.rs")));
    }

    #[test]
    fn has_corresponding_test_detects_sibling_test_file() {
        let dir = tempfile::tempdir().unwrap();
        // Place a fake production file (content doesn't matter for the
        // sibling-match path).
        std::fs::create_dir_all(dir.path().join("crates/foo/src")).unwrap();
        std::fs::write(dir.path().join("crates/foo/src/lib.rs"), "fn x() {}").unwrap();

        let prod = PathBuf::from("crates/foo/src/lib.rs");
        let new_tests = vec![PathBuf::from("crates/foo/src/lib_test.rs")];
        assert!(has_corresponding_test(dir.path(), &prod, &new_tests).unwrap());
    }

    #[test]
    fn has_corresponding_test_detects_inline_cfg_test() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/foo/src")).unwrap();
        std::fs::write(
            dir.path().join("crates/foo/src/lib.rs"),
            "fn x() {}\n\n#[cfg(test)]\nmod tests { #[test] fn it() {} }\n",
        )
        .unwrap();

        let prod = PathBuf::from("crates/foo/src/lib.rs");
        let new_tests: Vec<PathBuf> = vec![];
        assert!(has_corresponding_test(dir.path(), &prod, &new_tests).unwrap());
    }

    #[test]
    fn has_corresponding_test_returns_false_when_nothing_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/foo/src")).unwrap();
        std::fs::write(dir.path().join("crates/foo/src/lib.rs"), "fn x() {}\n").unwrap();

        let prod = PathBuf::from("crates/foo/src/lib.rs");
        let new_tests: Vec<PathBuf> = vec![];
        assert!(!has_corresponding_test(dir.path(), &prod, &new_tests).unwrap());
    }
}
