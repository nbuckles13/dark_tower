//! End-to-end tier for `dt-guard resolved-features`.
//!
//! The in-module unit tests exercise the pure evaluator over fixture JSON. This
//! tier pins the operator-facing surface of the real binary: a root with no
//! manifest is a PRECONDITION naming the cargo-failed token (never a pass), and
//! the real repository passes.

#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use std::path::Path;

fn run_guard(root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["resolved-features", "--root"])
        .arg(root)
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn missing_manifest_is_a_cargo_failed_precondition() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, stdout) = run_guard(dir.path());
    assert!(!ok, "a root with no Cargo.toml must not pass:\n{stdout}");
    assert!(
        stdout
            .lines()
            .any(|l| l.starts_with("ERROR: PRECONDITION [resolved-features-cargo-failed]")),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .last()
            .unwrap_or_default()
            .starts_with("STATUS=FAIL REASON=resolved-features-cargo-failed-"),
        "{stdout}"
    );
}

#[test]
fn relative_root_from_a_subdirectory_passes() {
    // `--root ..` from crates/: the manifest path must not be re-joined onto
    // the working directory cargo is spawned in.
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .current_dir(&crates)
        .args(["resolved-features", "--root", ".."])
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout
            .lines()
            .last()
            .unwrap_or_default()
            .starts_with("STATUS=OK REASON=resolved-features-clean-instances-"),
        "{stdout}"
    );
}

#[test]
fn real_repo_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (ok, stdout) = run_guard(&root);
    assert!(
        ok,
        "the real workspace must satisfy the provider policy:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .last()
            .unwrap_or_default()
            .starts_with("STATUS=OK REASON=resolved-features-clean-instances-"),
        "{stdout}"
    );
}
