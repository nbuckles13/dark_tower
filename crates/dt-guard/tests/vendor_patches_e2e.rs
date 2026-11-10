//! End-to-end tier for `dt-guard vendor-patches` — the guard's self-test.
//!
//! Every negative case drives the real binary against a synthetic tree built
//! from ONE clean fixture with a single mutation, so a failure names one cause;
//! the clean fixture is the positive control (a guard that failed
//! unconditionally would satisfy every negative). The last test runs the guard
//! against the real repository and pins the vendored count (1) and a non-empty
//! diff, so a wholesale-empty run cannot read as clean.

#![expect(
    clippy::unwrap_used,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use assert_cmd::Command;
use dt_guard::vendor_patches::{read_tree, tree_digest};
use std::path::Path;
use tempfile::TempDir;

const PRISTINE: &str = "pub fn a() {}\npub fn b() {}\npub fn c() {}\n";
const VENDORED: &str = "pub fn a() {}\npub fn b() {}\n// PATCH\npub fn added() {}\npub fn c() {}\n";
const DIFF: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,5 @@\n pub fn a() {}\n pub fn b() {}\n+// PATCH\n+pub fn added() {}\n pub fn c() {}\n";
const UP_MANIFEST: &str = "[package]\nname = \"up\"\nversion = \"1.2.3\"\n";

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// The digest of the pristine crate, computed by the SAME function the guard
/// uses, from a pristine tree written to disk (never a literal).
fn pristine_digest() -> String {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", UP_MANIFEST);
    write(dir.path(), "src/lib.rs", PRISTINE);
    tree_digest(&read_tree(dir.path()).unwrap())
}

fn patch_md(digest: &str) -> String {
    format!(
        "# up\n<!-- vendor-patch:begin -->\n```\nupstream-crate: up\nupstream-version: 1.2.3\n\
         upstream-crate-sha256: 00\npristine-tree-digest: {digest}\n```\n<!-- vendor-patch:end -->\n"
    )
}

const ROOT: &str = "[workspace]\nmembers = [\"m\"]\nexclude = [\"vendor/up\"]\n\
                    [patch.crates-io]\nup = { path = \"vendor/up\" }\n";
const LOCK: &str = "version = 4\n[[package]]\nname = \"up\"\nversion = \"1.2.3\"\n";

fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", ROOT);
    write(dir.path(), "Cargo.lock", LOCK);
    write(dir.path(), "vendor/up/Cargo.toml", UP_MANIFEST);
    write(dir.path(), "vendor/up/src/lib.rs", VENDORED);
    write(
        dir.path(),
        "vendor/patches/up/PATCH.md",
        &patch_md(&pristine_digest()),
    );
    write(dir.path(), "vendor/patches/up/PATCH.diff", DIFF);
    dir
}

fn run(root: &Path, extra: &[&str]) -> (bool, String) {
    let out = Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["vendor-patches", "--root"])
        .arg(root)
        .args(extra)
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

fn assert_fails_with(root: &Path, token: &str) {
    let (ok, stdout) = run(root, &[]);
    assert!(!ok, "expected `{token}`, got a pass:\n{stdout}");
    let status = stdout.lines().last().unwrap_or_default();
    assert!(
        status.starts_with(&format!("STATUS=FAIL REASON={token}-")),
        "STATUS must name `{token}`; got:\n{stdout}"
    );
    assert!(
        stdout.lines().any(|l| l.contains(&format!("[{token}]"))),
        "the token must ride in a VIOLATION/ERROR line; got:\n{stdout}"
    );
}

#[test]
fn clean_fixture_passes_with_counts() {
    let dir = fixture();
    let (ok, stdout) = run(dir.path(), &[]);
    assert!(ok, "{stdout}");
    assert_eq!(
        stdout.lines().last().unwrap(),
        "STATUS=OK REASON=vendor-patches-clean-vendored-1-added-2"
    );
}

#[test]
fn a_one_byte_drift_outside_the_patch_fails() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/up/src/lib.rs",
        &VENDORED.replace("pub fn c() {}", "pub fn c() { }"),
    );
    // The context line `pub fn c() {}` no longer matches: the diff cannot
    // apply. A drift in a line the diff does not touch is the next case.
    assert_fails_with(dir.path(), "vendor-patch-diff-invalid");
}

#[test]
fn a_drift_in_an_untouched_file_fails_the_digest() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/up/Cargo.toml",
        &format!("{UP_MANIFEST}# edited\n"),
    );
    assert_fails_with(dir.path(), "vendor-tree-drift");
}

#[test]
fn an_extra_file_fails_the_digest() {
    let dir = fixture();
    write(dir.path(), "vendor/up/src/extra.rs", "pub fn sneaky() {}\n");
    assert_fails_with(dir.path(), "vendor-tree-drift");
}

#[test]
fn a_removed_line_is_not_additive() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/patches/up/PATCH.diff",
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n pub fn a() {}\n-pub fn b() {}\n+pub fn b2() {}\n",
    );
    assert_fails_with(dir.path(), "vendor-patch-not-additive");
}

#[test]
fn hunk_counts_that_disagree_with_the_body_are_invalid() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/patches/up/PATCH.diff",
        &DIFF.replace("@@ -1,3 +1,5 @@", "@@ -1,3 +1,6 @@"),
    );
    assert_fails_with(dir.path(), "vendor-patch-diff-invalid");
}

#[test]
fn a_hunk_for_an_absent_file_is_invalid() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/patches/up/PATCH.diff",
        &DIFF.replace("src/lib.rs", "src/gone.rs"),
    );
    assert_fails_with(dir.path(), "vendor-patch-diff-invalid");
}

#[test]
fn an_unparseable_diff_is_invalid() {
    let dir = fixture();
    write(
        dir.path(),
        "vendor/patches/up/PATCH.diff",
        "this is not a diff\n",
    );
    assert_fails_with(dir.path(), "vendor-patch-diff-invalid");
}

#[test]
fn an_empty_patch_is_refused() {
    let dir = fixture();
    write(dir.path(), "vendor/patches/up/PATCH.diff", "");
    assert_fails_with(dir.path(), "vendor-patch-diff-empty");
}

#[test]
fn an_unpatched_lock_fails() {
    let dir = fixture();
    // A requirement bumped past the vendored version: cargo relinks upstream
    // and only warns "patch was not used".
    write(
        dir.path(),
        "Cargo.lock",
        "version = 4\n[[package]]\nname = \"up\"\nversion = \"1.2.4\"\n\
         source = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
    );
    assert_fails_with(dir.path(), "vendor-patch-not-in-lock");
}

#[test]
fn a_missing_tree_fails() {
    let dir = fixture();
    std::fs::remove_dir_all(dir.path().join("vendor/up")).unwrap();
    assert_fails_with(dir.path(), "vendor-crate-missing");
}

#[test]
fn missing_metadata_fails() {
    let dir = fixture();
    std::fs::remove_file(dir.path().join("vendor/patches/up/PATCH.md")).unwrap();
    assert_fails_with(dir.path(), "vendor-patch-metadata-missing");
}

#[test]
fn a_crate_not_excluded_from_the_workspace_fails() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        &ROOT.replace("exclude = [\"vendor/up\"]", "exclude = []"),
    );
    assert_fails_with(dir.path(), "vendor-not-excluded");
}

#[test]
fn an_unwired_vendored_dir_fails() {
    let dir = fixture();
    write(dir.path(), "vendor/stray/Cargo.toml", UP_MANIFEST);
    assert_fails_with(dir.path(), "vendor-dir-not-patch-target");
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_crate_fails() {
    let dir = fixture();
    std::os::unix::fs::symlink("/etc/hostname", dir.path().join("vendor/up/src/link")).unwrap();
    assert_fails_with(dir.path(), "vendor-tree-symlink");
}

#[test]
fn an_unreadable_root_manifest_is_a_precondition() {
    let dir = fixture();
    write(dir.path(), "Cargo.toml", "[workspace\n");
    let (ok, stdout) = run(dir.path(), &[]);
    assert!(!ok);
    assert!(
        stdout.contains("ERROR: PRECONDITION [vendor-patches-manifest-unreadable]"),
        "{stdout}"
    );
}

#[test]
fn the_pristine_digest_mode_anchors_and_detects_a_mismatch() {
    let dir = fixture();
    let up = tempfile::tempdir().unwrap();
    write(up.path(), "Cargo.toml", UP_MANIFEST);
    write(up.path(), "src/lib.rs", PRISTINE);
    let (ok, stdout) = run(
        dir.path(),
        &[
            "--pristine-digest",
            up.path().to_str().unwrap(),
            "--vendored",
            "vendor/up",
        ],
    );
    assert!(ok, "{stdout}");
    write(up.path(), "src/lib.rs", "tampered\n");
    let (ok, stdout) = run(
        dir.path(),
        &[
            "--pristine-digest",
            up.path().to_str().unwrap(),
            "--vendored",
            "vendor/up",
        ],
    );
    assert!(!ok, "{stdout}");
    assert!(
        stdout.contains("vendor-pristine-digest-mismatch"),
        "{stdout}"
    );
}

#[test]
fn the_attributed_lock_restores_the_registry_source() {
    let dir = fixture();
    let out = dir.path().join("attributed.lock");
    let (ok, stdout) = run(dir.path(), &["--attributed-lock", out.to_str().unwrap()]);
    assert!(ok, "{stdout}");
    let lock: toml::Table = toml::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let pkg = lock["package"].as_array().unwrap()[0].as_table().unwrap();
    assert_eq!(
        pkg["source"].as_str(),
        Some("registry+https://github.com/rust-lang/crates.io-index")
    );
    assert_eq!(pkg["version"].as_str(), Some("1.2.3"));
}

#[test]
fn list_prints_the_derived_paths() {
    let dir = fixture();
    let (ok, stdout) = run(dir.path(), &["--list"]);
    assert!(ok);
    assert_eq!(stdout, "vendor/up\n");
}

/// The real repository: exactly one vendored crate, a non-empty patch, clean.
#[test]
fn real_repo_vendors_exactly_wtransport_and_is_clean() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (ok, stdout) = run(&root, &["--list"]);
    assert!(ok, "{stdout}");
    assert_eq!(stdout, "vendor/wtransport\n", "the vendored set changed");
    let (ok, stdout) = run(&root, &[]);
    assert!(ok, "real repo must be clean:\n{stdout}");
    let status = stdout.lines().last().unwrap();
    assert!(
        status.starts_with("STATUS=OK REASON=vendor-patches-clean-vendored-1-added-"),
        "exactly one vendored crate expected; got: {status}"
    );
    let added: usize = status
        .strip_prefix("STATUS=OK REASON=vendor-patches-clean-vendored-1-added-")
        .unwrap()
        .parse()
        .unwrap();
    assert!(added > 0, "the recorded wtransport patch must add lines");
}

#[test]
fn a_patch_target_outside_vendor_fails() {
    for target in ["vendor/nested/up", "crates/up-patched"] {
        let dir = fixture();
        // The crate really sits at the target; the shape rule must still refuse
        // it (nested: collides with vendor/patches/<last component>; outside:
        // escapes the content scans' vendor/ prefix).
        std::fs::create_dir_all(dir.path().join(target).parent().unwrap()).unwrap();
        std::fs::rename(dir.path().join("vendor/up"), dir.path().join(target)).unwrap();
        write(dir.path(), "Cargo.toml", &ROOT.replace("vendor/up", target));
        let (ok, stdout) = run(dir.path(), &[]);
        assert!(!ok, "{target}: expected a failure, got:\n{stdout}");
        assert!(
            stdout
                .lines()
                .any(|l| l.contains("[vendor-patch-target-outside-vendor]") && l.contains(target)),
            "{target}: the shape violation must be named; got:\n{stdout}"
        );
    }
}

fn exit_code(root: &Path, extra: &[&str]) -> i32 {
    Command::cargo_bin("dt-guard")
        .unwrap()
        .args(["vendor-patches", "--root"])
        .arg(root)
        .args(extra)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

#[test]
fn modes_are_mutually_exclusive_and_vendored_needs_its_mode() {
    let dir = fixture();
    let lock = dir.path().join("x.lock");
    // Two modes at once: a usage error (2), never a silent pick.
    assert_eq!(
        exit_code(
            dir.path(),
            &["--list", "--attributed-lock", lock.to_str().unwrap()]
        ),
        2
    );
    assert!(!lock.exists(), "a rejected invocation must write nothing");
    // `--vendored` without a mode that takes it: usage error, not the full guard.
    assert_eq!(exit_code(dir.path(), &["--vendored", "vendor/up"]), 2);
    // Positive controls: each mode alone succeeds.
    assert_eq!(exit_code(dir.path(), &["--list"]), 0);
    assert_eq!(
        exit_code(dir.path(), &["--upstream-meta", "--vendored", "vendor/up"]),
        0
    );
}

#[test]
fn packages_prints_name_and_version_from_the_vendored_manifest() {
    let dir = fixture();
    let (ok, stdout) = run(dir.path(), &["--packages"]);
    assert!(ok);
    assert_eq!(stdout, "up 1.2.3\n");
}
