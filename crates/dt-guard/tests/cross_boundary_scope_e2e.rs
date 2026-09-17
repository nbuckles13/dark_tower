//! Proof-of-trap fixtures for `dt-guard cross-boundary-scope` — both emittable
//! rule IDs: `scope_drift_inbound` (a diff path absent from the plan) and
//! `scope_drift_planned_untouched` (a plan path absent from the diff).
//!
//! `run()` shells `git`, so each fixture is an ISOLATED git repo in a
//! `TempDir` (never `/work`). The repo is created hermetically — explicit
//! committer identity, neutralized global/system config, signing off, pinned
//! default branch — so it inherits no ambient state and does not flake in CI
//! (@paired-test (a)). The active-edit scope is the dirty working tree: the
//! untracked `docs/devloop-outputs/x/main.md` (carrying the plan table) plus
//! the untracked drift files appear via `git ls-files --others`.

#![expect(
    clippy::panic,
    reason = "integration-test fixture helper (`git`) outside a #[test] fn; a git command that fails to spawn or exits non-zero MUST abort loudly rather than degrade to a vacuous pass (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

mod common;

use common::{assert_clean, assert_parity, run_all, Case, Expect};
use std::path::Path;
use std::process::Command;

/// Run `git` in `root` hermetically: explicit identity, no global/system
/// config, no signing, pinned default branch. Aborts loudly on failure.
fn git(root: &Path, args: &[&str]) {
    let mut full: Vec<&str> = vec![
        "-c",
        "user.name=dt-guard-e2e",
        "-c",
        "user.email=dt-guard-e2e@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "init.defaultBranch=main",
    ];
    full.extend_from_slice(args);
    let out = Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(&full)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `git init` + a committed baseline so HEAD exists (the dirty-tree branch of
/// `get_active_edit_paths` diffs against HEAD).
fn init_and_baseline(root: &Path) {
    git(root, &["init", "-q"]);
    common::write(root, "README.md", "baseline\n");
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-q", "-m", "baseline"]);
}

/// A `main.md` carrying a `## Cross-Boundary Classification` table listing
/// `plan_paths` as the single-source-of-truth scope.
fn main_md(plan_paths: &[&str]) -> String {
    let mut s = String::from(
        "# Devloop X\n\n## Cross-Boundary Classification\n\n| Path | Classification | Owner |\n|---|---|---|\n",
    );
    for p in plan_paths {
        s.push_str(&format!("| `{p}` | Mine | — |\n"));
    }
    s
}

const MAIN_MD_REL: &str = "docs/devloop-outputs/x/main.md";

fn negatives() -> Vec<Case> {
    vec![
        // scope_drift_inbound: the diff carries `crates/foo/drift.rs`, which is
        // NOT in the plan (plan lists only touched.rs). main.md + both files
        // are untracked → all three are active-edit paths; main.md is a
        // symmetric exclusion, so the effective diff is {touched.rs, drift.rs}
        // and drift.rs is inbound drift.
        Case {
            name: "scope_drift_inbound",
            build: |root| {
                init_and_baseline(root);
                common::write(root, MAIN_MD_REL, &main_md(&["crates/foo/touched.rs"]));
                common::write(root, "crates/foo/touched.rs", "// touched\n");
                common::write(root, "crates/foo/drift.rs", "// unplanned\n");
            },
            expect: Expect::Exact(&["scope_drift_inbound"]),
        },
        // scope_drift_planned_untouched: the plan lists `crates/foo/planned.rs`
        // but the diff never touches it (only touched.rs is written).
        Case {
            name: "scope_drift_planned_untouched",
            build: |root| {
                init_and_baseline(root);
                common::write(
                    root,
                    MAIN_MD_REL,
                    &main_md(&["crates/foo/touched.rs", "crates/foo/planned.rs"]),
                );
                common::write(root, "crates/foo/touched.rs", "// touched\n");
            },
            expect: Expect::Exact(&["scope_drift_planned_untouched"]),
        },
    ]
}

#[test]
fn negatives_trap_their_rule_ids() {
    run_all("cross-boundary-scope", &negatives());
}

/// Clean: the effective diff exactly equals the plan → no drift.
#[test]
fn clean_diff_equals_plan_passes() {
    let root = common::new_root();
    init_and_baseline(root.path());
    common::write(
        root.path(),
        MAIN_MD_REL,
        &main_md(&["crates/foo/touched.rs"]),
    );
    common::write(root.path(), "crates/foo/touched.rs", "// touched\n");
    assert_clean(
        root.path(),
        "cross-boundary-scope",
        "cross-boundary-scope-no-drift",
    );
}

/// Source-derived SSoT: both `*_RULE_ID` consts are fixtured; no exclusions.
#[test]
fn rule_id_inventory_is_complete() {
    assert_parity("cross_boundary_scope.rs", &negatives(), &[]);
}
