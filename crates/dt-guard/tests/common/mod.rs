//! Shared harness for the dt-guard proof-of-trap e2e suites.
//!
//! ADR-0002 proof-of-trap: for every rule ID a guard can emit there is a
//! negative fixture that MUST produce that rule's finding, plus a clean
//! fixture that MUST pass. These suites drive the real `dt-guard` binary
//! against synthetic `TempDir` roots (the `release_build_profile_e2e.rs`
//! shape) so `run()` — the coverage gap — is exercised and visible to
//! `cargo llvm-cov`.
//!
//! # Why a subprocess, not in-process `run()`
//!
//! `Finding::print` / `common::explain::print_finding` write with `println!`,
//! which libtest captures out of the test body's reach, and an in-process
//! `run()` returns only a violation *count* in its `Err` — not the per-rule
//! `policy=` tokens the exact-set assertion depends on. Driving the binary as
//! a child recovers the per-finding rule IDs from its stdout, and the child
//! inherits `LLVM_PROFILE_FILE` (`%p`) so coverage is still collected.
//! (@observability, @dry-reviewer, @team-lead #4.)
//!
//! # Binary resolution
//!
//! `CARGO_BIN_EXE_dt-guard` (pinned by cargo at test-compile), NOT
//! `assert_cmd::cargo_bin` — the latter re-derives `target/debug` and
//! mis-resolves under `cargo llvm-cov`'s separate target dir. `DT_GUARD`
//! stays the documented override. Mirrors `dt_guard_bin()` in
//! `tests/credential_leak_key_custody_fixtures.rs` (added in `c5ca799`).

// Shared across five test binaries; each uses a subset of these helpers, so
// `dead_code` is expected per-binary and `allow` (not `expect`) is correct —
// the binary that happens to use everything would make `expect(dead_code)`
// unfulfilled.
#![allow(dead_code)]
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration-test fixture helpers outside #[test] fns; a broken fixture MUST abort loudly rather than degrade to a vacuous pass — a silently-skipped fixture is indistinguishable from a passing one, which is the exact failure these suites exist to detect (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Locate the guard binary. `DT_GUARD` override else the compile-time-pinned
/// `CARGO_BIN_EXE_dt-guard`, so it resolves under any target dir (plain
/// `cargo test`, the Layer-4 release build, `cargo llvm-cov`'s target dir).
pub fn dt_guard_bin() -> PathBuf {
    std::env::var_os("DT_GUARD").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_dt-guard")),
        PathBuf::from,
    )
}

/// A throwaway synthetic repo root. Built under the session scratchpad when
/// one is exported (`TMPDIR`/`CLAUDE_SCRATCHPAD`), else the system temp dir —
/// so a failed fixture leaves a triageable tree rather than an already-deleted
/// one. The root path is asserted to contain NO `/tests/` or `/fixtures/`
/// segment: `metric_labels`' `is_scan_exempt` matches those segments against
/// the repo-relative path, and a root under such a segment would silently
/// green the whole metric_labels suite. (@observability, @team-lead #5.)
pub fn new_root() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().to_string_lossy().to_string();
    assert!(
        !p.contains("/tests/") && !p.contains("/fixtures/"),
        "TempDir root {p:?} contains a `/tests/` or `/fixtures/` segment — \
         metric_labels' is_scan_exempt would silently green the suite; \
         set TMPDIR to a path without those segments"
    );
    tmp
}

/// Write `content` to `<root>/<rel>`, creating parent dirs. Aborts loudly on
/// any IO error — a broken fixture must not degrade to a vacuous pass.
pub fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
}

/// The outcome of one guard invocation.
pub struct GuardRun {
    /// True iff the child exited 0 (STATUS=OK).
    pub ok: bool,
    /// stdout + stderr concatenated (STATUS/SCOPE/EXPLAIN on stdout, the
    /// `{e:#}` error chain and bail message on stderr).
    pub combined: String,
    /// The set of rule IDs parsed from `policy=<guard>::<rule_id>` EXPLAIN
    /// tokens. Empty for a clean run or a `bail!`-only failure.
    pub rule_ids: BTreeSet<String>,
}

/// Spawn `dt-guard <leading...> --root <root>`, panic on spawn failure, and
/// return `(success, combined stdout+stderr)`. Shared spawn/capture boilerplate
/// for `run_guard` (with `--explain`) and `run_guard_plain` (without).
fn spawn(root: &Path, leading: &[&str]) -> (bool, String) {
    let out = Command::new(dt_guard_bin())
        .args(leading)
        .arg("--root")
        .arg(root)
        .output()
        .unwrap_or_else(|e| panic!("spawning dt-guard {leading:?}: {e}"));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

/// Run `dt-guard <subcommand> --root <root> --explain` and capture the result.
/// `--explain` is used uniformly so per-finding rule IDs ride the machine-
/// stable `policy=` token rather than the prose `VIOLATION:` line.
pub fn run_guard(root: &Path, subcommand: &str) -> GuardRun {
    let (ok, combined) = spawn(root, &[subcommand, "--explain"]);
    let rule_ids = parse_policy_rule_ids(&combined);
    GuardRun {
        ok,
        combined,
        rule_ids,
    }
}

/// Run the guard WITHOUT `--explain` and return combined stdout+stderr. The
/// prose `VIOLATION:` lines carry the FULL (untruncated) finding message, so
/// message-specific assertions read them rather than the truncated `matched=`.
pub fn run_guard_plain(root: &Path, subcommand: &str) -> String {
    spawn(root, &[subcommand]).1
}

/// Extract rule IDs from every `policy=<guard>::<rule_id>` token. The rule ID
/// is the segment after the final `::` (guard names themselves carry no `::`).
fn parse_policy_rule_ids(output: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for tok in output.split_whitespace() {
        if let Some(policy) = tok.strip_prefix("policy=") {
            if let Some((_, rule_id)) = policy.rsplit_once("::") {
                out.insert(rule_id.to_string());
            }
        }
    }
    out
}

/// Assert the guard FAILS and the EXACT set of emitted rule IDs equals
/// `expected` — every expected ID present AND no spurious one. Whole-set
/// compare (not "contains"), the `fixtures_match_their_invariant_blocks`
/// discipline: a fixture aimed at one rule that also trips another is a
/// mis-aimed trap and reds here.
pub fn assert_traps_exact(root: &Path, subcommand: &str, expected: &[&str]) -> GuardRun {
    let run = run_guard(root, subcommand);
    let want: BTreeSet<String> = expected.iter().map(|s| (*s).to_string()).collect();
    assert!(
        !run.ok,
        "{subcommand}: expected FAIL for rules {expected:?}, got STATUS=OK:\n{}",
        run.combined
    );
    assert_eq!(
        run.rule_ids, want,
        "{subcommand}: rule-id set mismatch.\n  expected: {want:?}\n  got:      {:?}\n{}",
        run.rule_ids, run.combined
    );
    run
}

/// Assert the guard FAILS and its emitted rule-id set CONTAINS every id in
/// `must_fire` and NONE in `must_not_fire`. For fixtures with a documented
/// structural co-fire (e.g. `catalog_but_no_entrypoint` + `missing_zero_init`)
/// where exact-set equality would over-specify, but the must-not-fire guard
/// still pins the isolation invariant (@paired-test (c)).
pub fn assert_traps_including(
    root: &Path,
    subcommand: &str,
    must_fire: &[&str],
    must_not_fire: &[&str],
) -> GuardRun {
    let run = run_guard(root, subcommand);
    assert!(
        !run.ok,
        "{subcommand}: expected FAIL, got STATUS=OK:\n{}",
        run.combined
    );
    for id in must_fire {
        assert!(
            run.rule_ids.contains(*id),
            "{subcommand}: expected rule {id:?} to fire; got {:?}\n{}",
            run.rule_ids,
            run.combined
        );
    }
    for id in must_not_fire {
        assert!(
            !run.rule_ids.contains(*id),
            "{subcommand}: rule {id:?} must NOT fire but did; got {:?}\n{}",
            run.rule_ids,
            run.combined
        );
    }
    run
}

/// Assert a `bail!`-path failure (no `policy=` findings): the run failed and
/// `combined` contains `token` (the raw rule-id string, printed verbatim to
/// stderr via `{e:#}`). Used for `empty_scan_no_counters` / `scan_root_absent`
/// which surface as an anyhow error, not a Finding (@paired-test #3).
pub fn assert_bail_token(root: &Path, subcommand: &str, token: &str) -> GuardRun {
    let run = run_guard(root, subcommand);
    assert!(
        !run.ok,
        "{subcommand}: expected FAIL (bail {token:?}), got STATUS=OK:\n{}",
        run.combined
    );
    assert!(
        run.combined.contains(token),
        "{subcommand}: expected bail token {token:?} in output:\n{}",
        run.combined
    );
    run
}

/// Assert the guard PASSES and the STATUS=OK REASON token contains
/// `ok_reason_substr` — a non-vacuous positive control, so a `no-dir` /
/// `no-files` empty scan is distinguishable from a targeted pass.
pub fn assert_clean(root: &Path, subcommand: &str, ok_reason_substr: &str) -> GuardRun {
    let run = run_guard(root, subcommand);
    assert!(
        run.ok,
        "{subcommand}: expected STATUS=OK, got FAIL:\n{}",
        run.combined
    );
    assert!(
        run.combined.contains("STATUS=OK"),
        "{subcommand}: no STATUS=OK line:\n{}",
        run.combined
    );
    assert!(
        run.combined.contains(ok_reason_substr),
        "{subcommand}: OK reason must contain {ok_reason_substr:?} (non-vacuous positive control):\n{}",
        run.combined
    );
    run
}

/// What a negative fixture is expected to produce.
pub enum Expect {
    /// The EXACT set of emitted rule IDs equals this (single-fire isolation;
    /// implies every other rule is asserted-absent).
    Exact(&'static [&'static str]),
    /// `fire` all present and `absent` all absent, but the set may carry other
    /// (documented, structural) co-fires. Used sparingly.
    Including {
        fire: &'static [&'static str],
        absent: &'static [&'static str],
    },
    /// The EXACT set equals `ids` AND every substring in `msg` appears in the
    /// output. For rules that share a rule_id across distinct causes (e.g.
    /// `runbook_url`'s two containment branches), where the token alone cannot
    /// distinguish which branch rejected. (@security F3.)
    ExactWithMsg {
        ids: &'static [&'static str],
        msg: &'static [&'static str],
    },
    /// A `bail!`-path token in the combined output (not a `policy=` finding).
    Bail(&'static str),
}

/// One negative fixture: a name, a builder that lays its synthetic tree into a
/// root, and its expectation. The catalog of these IS the fixtured-rule-id
/// inventory the parity test derives from — no parallel hand-list.
pub struct Case {
    pub name: &'static str,
    pub build: fn(&Path),
    pub expect: Expect,
}

impl Case {
    /// The rule IDs this case is the trap FOR (drives parity coverage).
    fn covered(&self) -> Vec<&'static str> {
        match &self.expect {
            Expect::Exact(ids) => ids.to_vec(),
            Expect::Including { fire, .. } => fire.to_vec(),
            Expect::ExactWithMsg { ids, .. } => ids.to_vec(),
            Expect::Bail(tok) => vec![*tok],
        }
    }
}

/// Build the case's root and assert its expectation. Panics with the fixture
/// name on failure so the aborting-loud helper still names which trap broke.
pub fn run_case(subcommand: &str, case: &Case) {
    let root = new_root();
    (case.build)(root.path());
    match &case.expect {
        Expect::Exact(ids) => {
            assert_traps_exact(root.path(), subcommand, ids);
        }
        Expect::Including { fire, absent } => {
            assert_traps_including(root.path(), subcommand, fire, absent);
        }
        Expect::ExactWithMsg { ids, msg } => {
            assert_traps_exact(root.path(), subcommand, ids);
            // The `--explain` `matched=` text is truncated; the non-explain
            // `VIOLATION:` line carries the full message, so branch-specific
            // substrings are asserted against that.
            let plain = run_guard_plain(root.path(), subcommand);
            for needle in *msg {
                assert!(
                    plain.contains(*needle),
                    "{subcommand} ({}): expected message substring {needle:?}:\n{plain}",
                    case.name,
                );
            }
        }
        Expect::Bail(tok) => {
            assert_bail_token(root.path(), subcommand, tok);
        }
    }
}

/// Run every case in the catalog.
pub fn run_all(subcommand: &str, cases: &[Case]) {
    for case in cases {
        run_case(subcommand, case);
    }
}

/// The union of rule IDs the catalog covers, as owned strings.
pub fn covered_ids(cases: &[Case]) -> BTreeSet<String> {
    cases
        .iter()
        .flat_map(|c| c.covered())
        .map(str::to_string)
        .collect()
}

/// Source-derived rule-ID inventory: every `pub const <NAME>_RULE_ID: &str =
/// "<literal>";` in the guard's own source, read as DATA at test time. This is
/// the SSoT the per-guard completeness test asserts the fixtured set against —
/// no hand-copied list, reddening in BOTH directions (a new const with no
/// fixture; a fixtured id whose const was removed/renamed). (@paired-test
/// binding, @code-reviewer #1, @team-lead #6.)
///
/// `guard_src_basename` is e.g. `"dashboard_panels.rs"`, resolved under
/// `CARGO_MANIFEST_DIR/src/`.
pub fn source_rule_ids(guard_src_basename: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(guard_src_basename);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading guard source {}: {e}", path.display()));
    let mut out = BTreeSet::new();
    for line in src.lines() {
        // Match `pub const <IDENT>_RULE_ID: &str = "<value>";`. Line-based,
        // no regex dependency; the const shape is uniform across all guards
        // (verified with @paired-test).
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("pub const ") else {
            continue;
        };
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let name = rest[..colon].trim();
        if !name.ends_with("_RULE_ID") {
            continue;
        }
        // Value is the first double-quoted literal on the line.
        let Some(open) = line.find('"') else { continue };
        let Some(close_rel) = line[open + 1..].find('"') else {
            continue;
        };
        out.insert(line[open + 1..open + 1 + close_rel].to_string());
    }
    assert!(
        !out.is_empty(),
        "no `pub const *_RULE_ID` literals found in {} — the source-derived SSoT \
         is vacuous; did the const shape change?",
        path.display()
    );
    out
}

/// Assert the fixtured rule-id set exactly equals the source-derived inventory
/// (both directions). `excluded` is a machine-checked set of source IDs
/// deliberately NOT cargo-fixtured here (e.g. counter_zero_init IDs covered by
/// the bash suite); it must be a subset of the source inventory, so a renamed
/// exclusion also reds.
pub fn assert_rule_id_parity(guard_src_basename: &str, fixtured: &[&str], excluded: &[&str]) {
    let fixtured_set: BTreeSet<String> = fixtured.iter().map(|s| (*s).to_string()).collect();
    assert_rule_id_parity_set(guard_src_basename, &fixtured_set, excluded);
}

/// Parity against the catalog-derived covered set (the strong form: the
/// fixtured inventory is DERIVED from the cases actually run, not a parallel
/// hand-list). `excluded` names source IDs deliberately not cargo-fixtured in
/// this suite (e.g. bash-covered counter_zero_init IDs), each with a reason at
/// the call site.
pub fn assert_parity(guard_src_basename: &str, cases: &[Case], excluded: &[&str]) {
    assert_rule_id_parity_set(guard_src_basename, &covered_ids(cases), excluded);
}

fn assert_rule_id_parity_set(
    guard_src_basename: &str,
    fixtured_set: &BTreeSet<String>,
    excluded: &[&str],
) {
    let source = source_rule_ids(guard_src_basename);
    let excluded_set: BTreeSet<String> = excluded.iter().map(|s| (*s).to_string()).collect();

    // Excluded IDs must actually exist in source (a renamed/removed exclusion
    // is itself drift).
    let stale_exclusions: BTreeSet<_> = excluded_set.difference(&source).cloned().collect();
    assert!(
        stale_exclusions.is_empty(),
        "{guard_src_basename}: EXCLUDED ids not present in source (renamed/removed?): {stale_exclusions:?}"
    );

    // Fixtured and excluded must be disjoint.
    let overlap: BTreeSet<_> = fixtured_set.intersection(&excluded_set).cloned().collect();
    assert!(
        overlap.is_empty(),
        "{guard_src_basename}: ids both fixtured AND excluded: {overlap:?}"
    );

    let covered: BTreeSet<String> = fixtured_set.union(&excluded_set).cloned().collect();
    assert_eq!(
        covered, source,
        "{guard_src_basename}: rule-id SSoT drift.\n  source consts:   {source:?}\n  fixtured+excluded: {covered:?}\n  \
         A new *_RULE_ID const needs a negative fixture (or an explicit EXCLUDED entry with reason); \
         a removed const needs its fixture deleted."
    );
}
