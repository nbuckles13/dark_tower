//! End-to-end tier for `dt-guard workspace-deps`.
//!
//! The in-module unit tests exercise `evaluate()`. This tier pins the
//! operator-facing surface produced by `run()`: the REASON token inside each
//! `VIOLATION:` / `ERROR: PRECONDITION` line (the only channel that survives
//! `run-guards.sh`'s non-verbose capture), precondition lines printed first,
//! the final `STATUS=` line, and the anti-vacuity member count on the PASS
//! token. It also runs the guard against the real repository and asserts the
//! member count equals the root `[workspace] members` length, so a guard that
//! silently dropped members cannot read as clean.

#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use std::path::Path;
use tempfile::TempDir;

const ROOT: &str = "[workspace]\nmembers = [\"a\", \"b\"]\nexclude = [\"fuzz\"]\n\
                    [workspace.dependencies]\nserde = \"1.0\"\n\
                    base64 = { version = \"0.23\", default-features = false }\n\
                    tokio = \"1\"\n";

/// Every SINGLE_VERSION_CRATES entry exactly once (R4's anchor).
const LOCK_OK: &str = "version = 4\n\
                       [[package]]\nname = \"metrics\"\nversion = \"0.24.6\"\n\
                       [[package]]\nname = \"tracing-core\"\nversion = \"0.1.34\"\n\
                       [[package]]\nname = \"opentelemetry\"\nversion = \"0.24.0\"\n";

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn pkg(name: &str, deps: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{deps}")
}

/// A clean fixture; callers override single files to trip one rule.
fn fixture(a_deps: &str, b_deps: &str, fuzz_deps: &str, lock: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", ROOT);
    write(dir.path(), "a/Cargo.toml", &pkg("a", a_deps));
    write(dir.path(), "b/Cargo.toml", &pkg("b", b_deps));
    write(
        dir.path(),
        "fuzz/Cargo.toml",
        &format!("[package]\nname = \"f\"\nversion = \"0.0.0\"\n[workspace]\n{fuzz_deps}"),
    );
    write(dir.path(), "Cargo.lock", lock);
    // R6: the fixtures' only fuzz-only crate is base64 (declared by the fuzz crate, no member uses it).
    write(
        dir.path(),
        ".github/dependabot.yml",
        &dependabot(&["base64"]),
    );
    dir
}

/// A root cargo block plus a fuzz block over `/fuzz` allowing `allow`.
fn dependabot(allow: &[&str]) -> String {
    let mut y = String::from(
        "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n  \
         - package-ecosystem: \"cargo\"\n    directories: [\"/fuzz\"]\n    allow:\n",
    );
    for a in allow {
        y.push_str(&format!("      - dependency-name: \"{a}\"\n"));
    }
    y
}

fn run_guard(root: &Path) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["workspace-deps", "--root"])
        .arg(root)
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

fn assert_fails_with(root: &Path, token: &str) -> String {
    let (ok, stdout) = run_guard(root);
    assert!(!ok, "expected failure, got:\n{stdout}");
    let status = stdout.lines().last().unwrap_or_default();
    assert!(
        status.starts_with(&format!("STATUS=FAIL REASON={token}-")),
        "STATUS line must name `{token}`; got:\n{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| (l.starts_with("VIOLATION:") || l.starts_with("ERROR:"))
                && l.contains(&format!("[{token}]"))),
        "the token must also ride in a VIOLATION/ERROR line; got:\n{stdout}"
    );
    stdout
}

const A_CLEAN: &str = "[dependencies]\nserde = { workspace = true, features = [\"derive\"] }\n";
const FUZZ_CLEAN: &str =
    "[dependencies]\nbase64 = { version = \"0.23\", default-features = false }\n";

#[test]
fn clean_fixture_passes_with_member_count() {
    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, LOCK_OK);
    let (ok, stdout) = run_guard(dir.path());
    assert!(ok, "{stdout}");
    assert_eq!(
        stdout.lines().last().unwrap(),
        "STATUS=OK REASON=workspace-deps-clean-members-2-excluded-1"
    );
}

#[test]
fn r1_positive_control() {
    let dir = fixture("[dependencies]\nserde = \"1.0\"\n", "", FUZZ_CLEAN, LOCK_OK);
    assert_fails_with(dir.path(), "workspace-dep-redeclared");
}

#[test]
fn r2_positive_control() {
    let dir = fixture(
        "[dependencies]\nregex = \"1\"\n",
        "[dev-dependencies]\nregex = \"1\"\n",
        FUZZ_CLEAN,
        LOCK_OK,
    );
    assert_fails_with(dir.path(), "dep-pinned-in-multiple-members");
}

#[test]
fn r3_positive_control_default_features() {
    let dir = fixture(A_CLEAN, "", "[dependencies]\nbase64 = \"0.23\"\n", LOCK_OK);
    assert_fails_with(dir.path(), "excluded-workspace-version-drift");
}

#[test]
fn r4_positive_control() {
    let lock = format!("{LOCK_OK}[[package]]\nname = \"tracing-core\"\nversion = \"0.2.0\"\n");
    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, &lock);
    assert_fails_with(dir.path(), "facade-crate-duplicated");
}

#[test]
fn r4_facade_absent_from_lock_is_a_precondition() {
    let lock = "version = 4\n[[package]]\nname = \"metrics\"\nversion = \"0.24.6\"\n";
    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, lock);
    assert_fails_with(dir.path(), "workspace-deps-facade-absent-from-lock");
}

#[test]
fn r6_positive_control_allow_missing() {
    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, LOCK_OK);
    write(dir.path(), ".github/dependabot.yml", &dependabot(&[]));
    assert_fails_with(dir.path(), "dependabot-fuzz-allow-missing");
}

#[test]
fn r6_positive_control_block_missing() {
    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, LOCK_OK);
    write(
        dir.path(),
        ".github/dependabot.yml",
        "version: 2\nupdates:\n  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n",
    );
    assert_fails_with(dir.path(), "dependabot-fuzz-block-missing");
}

#[test]
fn r5_positive_control() {
    let dir = fixture(
        "[dependencies]\ntokio = { workspace = true, default-features = false }\n",
        "",
        FUZZ_CLEAN,
        LOCK_OK,
    );
    assert_fails_with(dir.path(), "member-default-features-ignored");
}

#[test]
fn preconditions_fail_and_print_first() {
    // Missing lock: precondition, and it outranks the R1 violation also present.
    let dir = fixture("[dependencies]\nserde = \"1.0\"\n", "", FUZZ_CLEAN, LOCK_OK);
    std::fs::remove_file(dir.path().join("Cargo.lock")).unwrap();
    let stdout = assert_fails_with(dir.path(), "workspace-deps-cargo-lock-unreadable");
    let first_finding = stdout
        .lines()
        .find(|l| l.starts_with("VIOLATION:") || l.starts_with("ERROR:"))
        .unwrap();
    assert!(first_finding.starts_with("ERROR: PRECONDITION"), "{stdout}");

    let empty = tempfile::tempdir().unwrap();
    assert_fails_with(empty.path(), "workspace-deps-workspace-manifest-unreadable");

    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    assert_fails_with(dir.path(), "workspace-deps-workspace-members-glob");

    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", "[workspace]\nmembers = []\n");
    assert_fails_with(dir.path(), "workspace-deps-workspace-members-empty");

    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, LOCK_OK);
    std::fs::remove_file(dir.path().join("b/Cargo.toml")).unwrap();
    assert_fails_with(dir.path(), "workspace-deps-member-manifest-unreadable");

    let dir = fixture(A_CLEAN, "", FUZZ_CLEAN, LOCK_OK);
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"a\", \"b\"]\n",
    );
    assert_fails_with(dir.path(), "workspace-deps-workspace-dependencies-empty");
}

/// The real repository: clean, and the guard reached a verdict over EVERY
/// member. The expected count is read from the real root manifest here, never
/// hardcoded; the root is resolved from this crate's manifest dir, never cwd.
#[test]
fn real_repo_is_clean_over_every_member() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
    let members = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .unwrap()
        .len();
    assert!(members > 0);
    // The RAW `exclude` length, deliberately unfiltered: filtering on "has a
    // Cargo.toml" here would apply the same lossy step a guard could, so a moved fuzz
    // crate would shrink both counts and still agree. Unfiltered, it goes red.
    let excluded = manifest
        .get("workspace")
        .and_then(|w| w.get("exclude"))
        .and_then(toml::Value::as_array)
        .unwrap()
        .len();
    assert!(excluded > 0);

    let (ok, stdout) = run_guard(&root);
    assert!(ok, "real repo must be clean:\n{stdout}");
    let status = stdout.lines().last().unwrap();
    assert_eq!(
        status,
        format!("STATUS=OK REASON=workspace-deps-clean-members-{members}-excluded-{excluded}"),
        "guard must report all {members} root members and all {excluded} excluded crates"
    );
}
