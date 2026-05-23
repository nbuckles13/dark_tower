//! `ts-no-dev-trust-path-in-prod-bundle` subcommand — port of
//! `scripts/guards/simple/ts/no-dev-trust-path-in-prod-bundle.sh`.
//!
//! R-14 transition guard: prod bundle MUST NOT contain the literal
//! `serverCertificateHashes`. The dev-only WebTransport fingerprint trust
//! path is gated behind `__DEV_TRUST_FINGERPRINT__` build-time literal,
//! false in prod, tree-shaken out.
//!
//! Four-state machine (evaluated in order):
//! 1. `packages/sdk-core/` does not exist → STATUS=OK (rule scaffolded; no
//!    consumer yet).
//! 2. `packages/sdk-core/` exists BUT
//!    `packages/sdk-core/tests/bundle-content.test.ts` does not exist →
//!    STATUS=FAIL (forcing function).
//! 3. Both present → STATUS=OK (Vitest contract test in Layer 4 carries the
//!    real check), plus state-4 belt-and-suspenders if dist/ exists.
//! 4. Belt-and-suspenders: if `packages/sdk-core/dist/` exists, grep for
//!    `serverCertificateHashes` — fail on hit.
//!
//! Per @paired-client F1: paths are JOINED under the repo-root explicitly —
//! NO bare-relative paths. (Bash regressed to bare relative in task #37
//! Gate-2; this port pins the join.)

use crate::common::explain::{print_finding, Finding};
use crate::common::status::emit_ok;
use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const R14_CANONICAL_TEST_MISSING_RULE_ID: &str = "r14_canonical_test_missing";
pub const R14_STALE_DIST_LEAK_RULE_ID: &str = "r14_stale_dist_leak";

const SDK_CORE_DIR: &str = "packages/sdk-core";
const CANONICAL_TEST: &str = "packages/sdk-core/tests/bundle-content.test.ts";
const SDK_CORE_DIST: &str = "packages/sdk-core/dist";
const FORBIDDEN_LITERAL: &str = "serverCertificateHashes";

pub fn run(repo_root: &Path, explain: bool) -> Result<()> {
    let sdk_core = repo_root.join(SDK_CORE_DIR);
    let canonical_test = repo_root.join(CANONICAL_TEST);
    let dist_dir = repo_root.join(SDK_CORE_DIST);

    // State 1 — sdk-core not yet present.
    if !sdk_core.is_dir() {
        emit_ok("ts-no-dev-trust-path-sdk-core-absent");
        return Ok(());
    }

    // State 2 — sdk-core exists BUT canonical test missing (FORCING).
    if !canonical_test.is_file() {
        if explain {
            let canonical_disp = CANONICAL_TEST.to_string();
            print_finding(&Finding {
                file: &canonical_disp,
                row: 0,
                col: 0,
                policy: "ts-no-dev-trust-path-in-prod-bundle::r14_canonical_test_missing",
                matched: "canonical contract test file does not exist",
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {CANONICAL_TEST} missing — R-14 enforcement gap. \
                 The canonical contract test MUST grep the prod-mode \
                 `vite build` output for the literal `serverCertificateHashes` \
                 and fail on any hit. See \"R-14 Transition\" in docs/TODO.md."
            );
        }
        anyhow::bail!("ts-r14-canonical-test-missing");
    }

    // State 3 — both present.
    //
    // State 4 — belt-and-suspenders dist scan (only if dist/ exists).
    let mut dist_hits: Vec<(PathBuf, usize)> = Vec::new();
    if dist_dir.is_dir() {
        for entry in WalkDir::new(&dist_dir).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path().to_path_buf();
            let content = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue, // binary or unreadable — skip
            };
            for (idx, line) in content.lines().enumerate() {
                if line.contains(FORBIDDEN_LITERAL) {
                    dist_hits.push((path.clone(), idx + 1));
                }
            }
        }
    }

    if dist_hits.is_empty() {
        emit_ok("ts-no-dev-trust-path-clean");
        return Ok(());
    }

    for (path, line_no) in &dist_hits {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path.as_path())
            .display()
            .to_string();
        if explain {
            print_finding(&Finding {
                file: &rel,
                row: *line_no,
                col: 0,
                policy: "ts-no-dev-trust-path-in-prod-bundle::r14_stale_dist_leak",
                matched: FORBIDDEN_LITERAL,
                extras: &[],
                src_file: file!(),
                src_line: line!(),
            });
        } else {
            println!(
                "VIOLATION: {}:{} contains `{}` in prod bundle artifact (stale build?)",
                rel, line_no, FORBIDDEN_LITERAL
            );
        }
    }

    anyhow::bail!("ts-r14-stale-dist-leak-{}", dist_hits.len())
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
    fn state1_sdk_core_absent_passes() {
        let dir = TempDir::new().unwrap();
        let result = run(dir.path(), false);
        assert!(result.is_ok(), "got {result:?}");
    }

    #[test]
    fn state2_sdk_core_present_test_missing_fails() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(SDK_CORE_DIR)).unwrap();
        let result = run(dir.path(), false);
        assert!(result.is_err(), "expected R-14 forcing function to fire");
    }

    #[test]
    fn state3_both_present_passes() {
        let dir = TempDir::new().unwrap();
        write(
            &dir.path().join(CANONICAL_TEST),
            "// vitest contract test stub",
        );
        let result = run(dir.path(), false);
        assert!(result.is_ok(), "got {result:?}");
    }

    #[test]
    fn state4_dist_scan_fires_on_literal() {
        let dir = TempDir::new().unwrap();
        write(
            &dir.path().join(CANONICAL_TEST),
            "// vitest contract test stub",
        );
        // Plant a leaked literal in the dist artifact.
        let dist_artifact = dir.path().join(SDK_CORE_DIST).join("bundle.js");
        write(&dist_artifact, "const x = { serverCertificateHashes: [] };");
        let result = run(dir.path(), false);
        assert!(result.is_err(), "expected stale-dist leak finding");
    }

    #[test]
    fn state4_clean_dist_passes() {
        let dir = TempDir::new().unwrap();
        write(
            &dir.path().join(CANONICAL_TEST),
            "// vitest contract test stub",
        );
        let dist_artifact = dir.path().join(SDK_CORE_DIST).join("bundle.js");
        write(&dist_artifact, "const x = { other: 'value' };");
        let result = run(dir.path(), false);
        assert!(result.is_ok(), "got {result:?}");
    }

    #[test]
    fn paths_are_joined_under_repo_root_not_bare_relative() {
        // Per @paired-client F1: paths MUST be joined under the repo-root.
        // Running with a tempdir that has NO packages/sdk-core/ should
        // resolve state 1 (absent), NOT misfire by checking bare-relative
        // paths against the test runner's cwd (which is `/work` and HAS
        // these dirs).
        let dir = TempDir::new().unwrap();
        let result = run(dir.path(), false);
        assert!(
            result.is_ok(),
            "tempdir without sdk-core/ should resolve state 1; got {result:?}"
        );
    }
}
