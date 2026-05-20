//! Binary-surface STATUS-line regression test (TD-SG-1 fold-in 2026-05-19).
//!
//! Pins ADR-0033 watch-point #2 by binary surface: when `dt-guard` exits
//! non-zero, stdout MUST start with `STATUS=FAIL REASON=` and the REASON
//! token MUST be kebab-case + space-free. The implementation lives at
//! `src/main.rs:117` (`emit_fail(reason_token(&e))`); a future edit that
//! short-circuits or removes that call would compile clean and pass every
//! library-level test while breaking the wrapper contract. This harness
//! drives the binary via `assert_cmd` so the contract is enforced at the
//! observable surface, not just the function call site.
//!
//! Two driving fixtures (one per error kind):
//! 1. Missing `--root` directory → file-system error in the guard kernel
//!    → STATUS=FAIL REASON=<slugified-chain>.
//! 2. Unknown subcommand → clap parse-time exit. Clap writes its own
//!    error to stderr and exits 2; we do NOT assert STATUS= on this
//!    surface (clap exit isn't routed through `main.rs:117`'s mapping
//!    by design). This branch is here as a documentary smoke check that
//!    clap exits non-zero with diagnostic on stderr.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

/// Watch-point #2: every non-clap dt-guard failure must emit a
/// `STATUS=FAIL REASON=<slug>` line as the FIRST stdout line, with REASON
/// in `[a-z0-9-]+` (kebab-case, no spaces, no uppercase).
#[test]
fn nonzero_exit_emits_kebab_case_status_fail_reason() {
    // Build a tempdir that mimics the repo layout enough to make the
    // alert-rules subcommand discover an alerts file and trip a parse
    // failure inside the kernel — that's the failure path that flows
    // through `main.rs:117` `emit_fail(reason_token(&e))`.
    let root = tempdir().expect("tempdir");
    let alerts_dir = root.path().join("infra/docker/prometheus/rules");
    fs::create_dir_all(&alerts_dir).expect("mkdir alerts");
    fs::write(
        alerts_dir.join("bad.yaml"),
        // Unbalanced flow mapping — serde_norway::from_str returns Err.
        b"groups: [ { name: g, rules: [ { alert: A, expr: ' ",
    )
    .expect("write bad yaml");

    let cmd = Command::cargo_bin("dt-guard")
        .expect("dt-guard binary")
        .args(["alert-rules-policy", "--root"])
        .arg(root.path())
        .assert()
        .failure();

    let stdout = String::from_utf8(cmd.get_output().stdout.clone()).expect("utf-8 stdout");
    let first_line = stdout.lines().next().expect("at least one stdout line");

    assert!(
        first_line.starts_with("STATUS=FAIL REASON="),
        "first stdout line must start with `STATUS=FAIL REASON=`; got: {first_line:?}",
    );

    let reason = first_line
        .strip_prefix("STATUS=FAIL REASON=")
        .expect("STATUS=FAIL REASON= prefix");

    assert!(!reason.is_empty(), "REASON token must not be empty");
    assert!(
        !reason.contains(' '),
        "REASON token must not contain spaces; got: {reason:?}",
    );
    assert!(
        reason
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "REASON token must be kebab-case (`[a-z0-9-]+`); got: {reason:?}",
    );
    assert!(
        reason.len() <= 60,
        "REASON token must be ≤60 chars per status.rs cap; got {} chars: {reason:?}",
        reason.len(),
    );
}

/// Clap-error branch — exits non-zero with clap's own diagnostic on
/// stderr. Documentary smoke check; we do NOT assert STATUS= here because
/// clap's exit is not routed through `main.rs:117` by design.
#[test]
fn unknown_subcommand_exits_nonzero_with_clap_diagnostic() {
    let output = Command::cargo_bin("dt-guard")
        .expect("dt-guard binary")
        .arg("not-a-real-subcommand")
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        !stderr.is_empty(),
        "clap should emit a diagnostic on stderr for unknown subcommands",
    );
}
