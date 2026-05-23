//! `ts-no-test-removal` subcommand — port of
//! `scripts/guards/simple/ts/no-test-removal-ts.sh`.
//!
//! v1 scope (per task #37 paired-test ruling + this devloop's plan): file-
//! deletion-only check. The net `(it|test)(` block-count heuristic stays
//! deferred (see §Accepted Deferrals in this devloop's main.md).
//!
//! Per @paired-client F2: the underlying git-diff call pins
//! `--diff-filter=D` exclusively (no `-M`/`-C` rename detection). A rename
//! appears as delete+add by design, and the same-basename/same-package match
//! policy catches it naturally.
//!
//! Match policy (matches bash today):
//! 1. For each DELETED test file, accept it if the ADDED set contains a
//!    file with the same basename, OR a file under the same `packages/<pkg>`
//!    prefix.
//! 2. Otherwise emit a VIOLATION.

use crate::common::explain::{print_finding, Finding};
use crate::common::git_changes::{get_added_files, get_deleted_files, get_modified_files};
use crate::common::status::emit_ok;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const TEST_REMOVAL_UNMATCHED_RULE_ID: &str = "test_removal_unmatched";

const TEST_SUFFIXES: &[&str] = &[
    ".test.ts",
    ".spec.ts",
    ".test.tsx",
    ".spec.tsx",
    ".test.svelte",
];

const TS_EXTS_FOR_TESTS_DIR: &[&str] = &[".ts", ".tsx"];

const EXCLUDED_PATH_PATTERNS: &[&str] = &[
    "/node_modules/",
    "/dist/",
    "/build/",
    "/.svelte-kit/",
    "/coverage/",
    "/.nx/cache/",
];

/// Predicate: is `path` a test file we care about?
fn is_test_file(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return false;
    };
    if TEST_SUFFIXES.iter().any(|suf| s.ends_with(suf)) {
        return true;
    }
    // Files under any `/__tests__/` subdirectory with `.ts` or `.tsx`.
    if s.contains("/__tests__/") && TS_EXTS_FOR_TESTS_DIR.iter().any(|ext| s.ends_with(ext)) {
        return true;
    }
    false
}

fn is_excluded_path(path: &Path) -> bool {
    let Some(s) = path.to_str() else {
        return true;
    };
    EXCLUDED_PATH_PATTERNS.iter().any(|pat| s.contains(pat))
}

/// Extract the `packages/<name>` prefix from a path, if present.
fn package_prefix(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    let stripped = s.strip_prefix("packages/")?;
    let pkg = stripped.split('/').next()?;
    Some(format!("packages/{pkg}"))
}

fn basename(path: &Path) -> Option<String> {
    path.file_name()?.to_str().map(String::from)
}

/// Entry point for the `ts-no-test-removal` subcommand.
pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let added = collect_test_files(repo_root, get_added_files)?;
    let deleted = collect_test_files(repo_root, get_deleted_files)?;
    let modified = collect_test_files(repo_root, get_modified_files)?;

    if deleted.is_empty() && added.is_empty() && modified.is_empty() {
        emit_ok("ts-no-test-removal-no-changes");
        return Ok(());
    }

    if deleted.is_empty() {
        emit_ok("ts-no-test-removal-no-deletions");
        return Ok(());
    }

    let added_basenames: Vec<String> = added.iter().filter_map(|p| basename(p)).collect();

    let mut unmatched: Vec<PathBuf> = Vec::new();
    for d in &deleted {
        let local_base = match basename(d) {
            Some(b) => b,
            None => continue,
        };
        // Same-basename match in added set.
        if added_basenames.iter().any(|b| b == &local_base) {
            continue;
        }
        // Same-package match: any added file under the deleted file's
        // `packages/<pkg>` prefix.
        if let Some(pkg_prefix) = package_prefix(d) {
            let needle = format!("{pkg_prefix}/");
            if added
                .iter()
                .any(|p| p.to_str().is_some_and(|s| s.starts_with(&needle)))
            {
                continue;
            }
        }
        unmatched.push(d.clone());
    }

    if unmatched.is_empty() {
        emit_ok(format!(
            "ts-no-test-removal-clean-{}-deletions",
            deleted.len()
        ));
        return Ok(());
    }

    for path in &unmatched {
        let file_disp = path.display().to_string();
        if explain {
            let msg = "test file deleted without matching addition".to_string();
            print_finding(&Finding {
                file: &file_disp,
                row: 0,
                col: 0,
                policy: "ts-no-test-removal::test_removal_unmatched",
                matched: &msg,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {} — test file deleted without matching addition (same basename or same package)",
                file_disp
            );
        }
    }

    anyhow::bail!("ts-test-removal-unmatched-{}", unmatched.len())
}

/// Collect test files via a git-changes getter, filter to test-file shapes,
/// drop excluded paths.
fn collect_test_files<F>(repo_root: &Path, getter: F) -> Result<Vec<PathBuf>>
where
    F: FnOnce(&Path, &Path, &[&str]) -> Result<Vec<PathBuf>>,
{
    // Pass empty `extensions` — we filter by `is_test_file` (which inspects
    // both suffix shapes and `/__tests__/` substring) rather than a single
    // suffix list.
    let all = getter(repo_root, Path::new("."), &[])
        .context("collecting changed TS files for test-removal check")?;
    let filtered: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| is_test_file(p))
        .filter(|p| !is_excluded_path(p))
        .collect();
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_test_file_matches_canonical_suffixes() {
        assert!(is_test_file(Path::new("packages/x/src/foo.test.ts")));
        assert!(is_test_file(Path::new("packages/x/src/foo.spec.tsx")));
        assert!(is_test_file(Path::new("packages/x/tests/foo.test.svelte")));
        assert!(is_test_file(Path::new("packages/x/src/__tests__/foo.ts")));
        assert!(is_test_file(Path::new(
            "packages/x/src/__tests__/nested/bar.tsx"
        )));
    }

    #[test]
    fn is_test_file_does_not_match_feature_x_test_regression() {
        // Regression for the bash `.test.ts` regex bug: `featureXtest.ts`
        // must NOT match because the dot in `.test` is literal here.
        assert!(!is_test_file(Path::new("packages/x/src/featureXtest.ts")));
    }

    #[test]
    fn is_test_file_rejects_non_test_typescript() {
        assert!(!is_test_file(Path::new("packages/x/src/index.ts")));
        assert!(!is_test_file(Path::new("packages/x/src/utils.ts")));
    }

    #[test]
    fn package_prefix_extracts_first_segment() {
        assert_eq!(
            package_prefix(Path::new("packages/sdk-core/src/foo.ts")),
            Some("packages/sdk-core".to_string())
        );
        assert_eq!(
            package_prefix(Path::new("packages/web-app/tests/x.test.ts")),
            Some("packages/web-app".to_string())
        );
        assert_eq!(package_prefix(Path::new("crates/foo/bar.rs")), None);
    }

    #[test]
    fn basename_extracts_file_name() {
        assert_eq!(
            basename(Path::new("packages/x/src/foo.test.ts")),
            Some("foo.test.ts".to_string())
        );
    }

    #[test]
    fn excluded_path_drops_build_dirs() {
        assert!(is_excluded_path(Path::new(
            "packages/x/node_modules/foo.test.ts"
        )));
        assert!(is_excluded_path(Path::new("packages/x/dist/foo.test.ts")));
        assert!(!is_excluded_path(Path::new("packages/x/src/foo.test.ts")));
    }
}
