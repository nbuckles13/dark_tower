//! End-to-end tier for `dt-guard tool-bin-paths`: the real binary, a real `git
//! init` repo (so enumeration takes the production `git ls-files` path, untracked
//! files included), and the operator-facing output.
//!
//! Pinned here and nowhere else:
//! 1. **Positive control that MUST hit**, with the REASON token and `file:line` on
//!    the `VIOLATION:` line (Layer 3 re-emits only those lines).
//! 2. **The real repository is clean AND was actually scanned** — the PASS token
//!    carries the scanned-file count and it must clear a floor, so a wrong root
//!    or an emptied enumeration cannot read as clean.
//! 3. **Vacuity is a PRECONDITION line, printed first**, never STATUS=OK.

mod common;

use common::{git, run_guard_plain, write};
use std::path::Path;
use tempfile::TempDir;

const SUB: &str = "tool-bin-paths";
const HOME_REL: &str = "scripts/lib/tool-bins.sh";
const HOME_SRC: &str = "DT_TOOLS_PROFILE=devtools\nDT_TOOLS_CRATES=\"dt-guard dt-story\"\n";
const MANIFEST: &str =
    "[workspace]\nmembers = []\n\n[profile.devtools]\ninherits = \"release\"\nlto = false\n";

/// Floor for the real repo's scanned-file count. Well under the real count
/// (~1.5k text files outside the dated-record dirs) and far over anything a
/// mis-rooted or emptied enumeration could produce.
const REAL_REPO_SCAN_FLOOR: usize = 500;

fn fixture_repo() -> TempDir {
    let d = common::new_root();
    git(d.path(), &["init", "-q"]);
    write(d.path(), HOME_REL, HOME_SRC);
    write(d.path(), "Cargo.toml", MANIFEST);
    git(d.path(), &["add", "-A"]);
    d
}

#[test]
fn clean_fixture_passes_with_file_count() {
    let d = fixture_repo();
    let out = run_guard_plain(d.path(), SUB);
    assert!(
        out.contains("STATUS=OK REASON=tool-bin-paths-clean-files-2"),
        "{out}"
    );
}

#[test]
fn injected_literal_in_untracked_file_must_hit_with_file_line() {
    let d = fixture_repo();
    // concat! keeps this test file free of the literal it injects.
    let lit = concat!("tar", "get/release/dt-guard");
    write(
        d.path(),
        "scripts/new.sh",
        &format!("#!/bin/sh\nexec \"$R/{lit}\" kustomize\n"),
    );
    let out = run_guard_plain(d.path(), SUB);
    assert!(
        out.contains(&format!(
            "VIOLATION: [tool-bin-path-literal] scripts/new.sh:2 {lit}"
        )),
        "{out}"
    );
    assert!(
        out.contains("STATUS=FAIL REASON=tool-bin-path-literal-1"),
        "{out}"
    );
}

#[test]
fn injected_profile_build_in_runbook_must_hit() {
    let d = fixture_repo();
    let cmd = concat!("car", "go build --release -p dt-story");
    write(d.path(), "docs/runbooks/x.md", &format!("Fix: `{cmd}`.\n"));
    let out = run_guard_plain(d.path(), SUB);
    assert!(
        out.contains("VIOLATION: [tool-bin-build-outside-home] docs/runbooks/x.md:1"),
        "{out}"
    );
}

#[test]
fn not_a_work_tree_is_a_precondition_printed_first() {
    let d = common::new_root();
    write(d.path(), HOME_REL, HOME_SRC);
    write(d.path(), "Cargo.toml", MANIFEST);
    let out = run_guard_plain(d.path(), SUB);
    let first = out
        .lines()
        .find(|l| !l.starts_with("SCOPE:"))
        .unwrap_or_default();
    assert!(
        first.starts_with("ERROR: PRECONDITION [tool-bins-enumeration-failed]"),
        "{out}"
    );
    assert!(!out.contains("STATUS=OK"), "{out}");
}

#[test]
fn real_repository_is_clean_and_was_scanned() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = run_guard_plain(&repo_root, SUB);
    let token = out
        .lines()
        .find_map(|l| l.strip_prefix("STATUS=OK REASON=tool-bin-paths-clean-files-"));
    assert!(token.is_some(), "real repo not clean:\n{out}");
    let count: usize = token.unwrap_or_default().trim().parse().unwrap();
    assert!(
        count >= REAL_REPO_SCAN_FLOOR,
        "scanned {count} files, floor {REAL_REPO_SCAN_FLOOR}:\n{out}"
    );
}
