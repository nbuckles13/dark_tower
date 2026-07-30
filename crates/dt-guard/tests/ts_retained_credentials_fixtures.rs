// Crate-level test override, per the note in `clippy.toml`: `allow-expect-in-tests` /
// `allow-unwrap-in-tests` only apply inside `#[cfg(test)]` modules and `#[test]` fns,
// so an integration test's non-`#[test]` HELPER functions (`mechanical_set`, `find`, …)
// still trip `expect_used` / `panic`. Failing loudly on a missing or unreadable fixture
// is the correct behaviour here — a fixture harness that degrades to "no fixtures" would
// pass while proving nothing, which is the exact failure class this file exists to lock.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration-test helpers outside #[test] fns; a missing fixture MUST abort loudly rather than yield an empty set (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

//! Fixture-driven tests for the `ts-no-retained-credentials` policy (task #58).
//!
//! # Design — drive the two-phase KERNEL as a SET, not `run()`, and not file-by-file
//!
//! Two independent traps make the obvious harness vacuous, and both were caught at
//! Gate 1 rather than by a failing test:
//!
//! 1. **Double exemption.** `common::test_code_filter::is_scan_exempt` excludes both
//!    `/fixtures/` and `crates/dt-guard/**`, so these files are exempt *twice over*.
//!    Anything routed through `run()` skips them before a rule evaluates, and the
//!    test passes having demonstrated nothing. So we call the kernel directly
//!    (`cite_extract_e2e.rs` precedent: direct library invocation, no `assert_cmd`).
//!
//! 2. **Phase shape.** Rule 2's pass 1 is **repo-wide** — `AuthResult` is declared in
//!    `types.ts` and retained in `App.svelte`. A per-file kernel hands it an EMPTY
//!    declaration index, so every retention assertion returns silent, and silent is
//!    exactly what a negative fixture asserts: the whole matrix would go green with
//!    the rule structurally disabled. So [`collect_credential_types`] runs over the
//!    fixture set as a SET, and [`scan_retention`] evaluates against that index.
//!
//! What the kernel path cannot cover — collection, filtering, exit code, and whether
//! the wrapper is registered and executable — is covered by a live `run-guards.sh`
//! smoke plus the pre/post-B2 runs recorded in the devloop output. A fixture suite
//! that passes while the wrapper is unregistered is the same class of lie.
//!
//! # Layer is in the PATH, not in a harness exemption list
//!
//! `fixtures/ts_retained_credentials/lens/**` are fixtures for the *semantic* lens,
//! where the mechanical guard is CORRECTLY silent (transmission has no mechanical
//! counterpart). They carry no assertion here — their executor is the semantic-guard
//! agent's Gate-3 report. Encoding that in the path rather than in an exemption list
//! is deliberate: an exemption list is the thing this guard refuses to ship for
//! itself, and it would leave an invitation to "fix" the guard until `pos_*` fires,
//! which would mean retention-free transmission detection in a syntax matcher.

use dt_guard::ts_retained_credentials::{
    collect_credential_types, scan_retention, AUTH_STATE_RULE_ID, RETAINED_CRED_RULE_ID,
};
use std::path::{Path, PathBuf};

const FIXTURE_DIR: &str = "tests/fixtures/ts_retained_credentials";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_DIR)
}

/// Load the MECHANICAL fixture set (top level only — `lens/` is excluded by design).
fn mechanical_set() -> Vec<(PathBuf, String)> {
    let dir = fixture_root();
    let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(&dir)
        .expect("fixture dir readable")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| {
            let content = std::fs::read_to_string(e.path()).expect("fixture readable");
            (e.path(), content)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!out.is_empty(), "mechanical fixture set must not be empty");
    out
}

fn lens_set() -> Vec<(PathBuf, String)> {
    let dir = fixture_root().join("lens");
    let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(&dir)
        .expect("lens fixture dir readable")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .map(|e| {
            let content = std::fs::read_to_string(e.path()).expect("fixture readable");
            (e.path(), content)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn find<'a>(set: &'a [(PathBuf, String)], name: &str) -> &'a (PathBuf, String) {
    set.iter()
        .find(|(p, _)| p.file_name().is_some_and(|f| f == name))
        .unwrap_or_else(|| panic!("fixture {name} missing"))
}

/// Every fixture, scanned as a set, with its full expected finding list.
///
/// The matrix is asserted EXHAUSTIVELY (exact rule IDs, exact counts) rather than as
/// a lower bound: an over-broad exclusion shows up as zero hits, which reads
/// identically to "clean" under an at-least-one assertion.
#[test]
fn mechanical_fixture_matrix() {
    let set = mechanical_set();
    let index = collect_credential_types(&set);

    // (fixture, expected (rule_id, line) pairs in line order)
    //
    // The LINE is asserted, not just its presence: a scanner firing on the right file
    // for the wrong reason — a stray match several lines off — would otherwise pass
    // (@test T2). Comment and code must claim the same thing; the first version of
    // this asserted `line > 0` while claiming to pin the line.
    let expected: &[(&str, &[(&str, usize)])] = &[
        // --- positives -------------------------------------------------------
        // Cross-file: the retention half fires, and ONLY as a set.
        ("pos_xfile_retention.svelte", &[(AUTH_STATE_RULE_ID, 10)]),
        // Union alias: fires only because the closure resolves through the alias.
        // The REFINED rule, correctly: `JoinCredentials` unions `LoginCredentials`
        // (password) with `TokenCredentials` (userToken), so a retained value of that
        // type genuinely holds a credential alongside a token — which is the sharper
        // finding. Worth noting the guard was right here and this expectation was
        // wrong on first write.
        (
            "pos_retained_union_alias.svelte",
            &[(AUTH_STATE_RULE_ID, 7)],
        ),
        // Nested object literal attributes to the enclosing declaration; the class
        // field is the retention site.
        ("pos_nested_credential.tsx", &[(AUTH_STATE_RULE_ID, 10)]),
        // --- positives' declaration halves: SILENT ON THEIR OWN ---------------
        // A declaration without a retention site is not a finding. This is a
        // precision assertion — it is what makes the cross-file pair meaningful.
        ("pos_xfile_decl.ts", &[]),
        ("pos_union_decl.ts", &[]),
        // --- negatives -------------------------------------------------------
        // Same as the positive pair with ONLY the credential field removed, retention
        // site intact: the controlled experiment behind verification item (a).
        ("neg_xfile_decl.ts", &[]),
        ("neg_xfile_retention.svelte", &[]),
        // The three exclusions, on their real shapes.
        ("neg_exclusions_real_shapes.ts", &[]),
        ("neg_props_type_literal.svelte", &[]),
        // Credential type used only as a call-scoped parameter (the lens-4 case).
        ("neg_transient_login_params.ts", &[]),
        // Documented blind spot, pinned.
        ("neg_bare_string_form_state.svelte", &[]),
        // Cycle: must terminate AND stay silent.
        ("neg_recursive_types.ts", &[]),
    ];

    for (name, want) in expected {
        let (path, content) = find(&set, name);
        let hits = scan_retention(path, content, &index);
        let got: Vec<(&str, usize)> = hits.iter().map(|h| (h.rule_id, h.line)).collect();
        assert_eq!(
            &got.as_slice(),
            want,
            "fixture {name}: expected (rule_id, line) {want:?}, got {got:?}"
        );
        for hit in &hits {
            assert!(
                !hit.type_name.is_empty(),
                "fixture {name}: hit names no type"
            );
        }
    }
}

/// Verification item (a) as a CONTROLLED EXPERIMENT: the positive fires, and the same
/// shape with only the `password` field removed does not.
///
/// The negative keeps its retention site. If it dropped both, the pair would pass for
/// the wrong reason — a tautological fixture one level subtler than the double
/// exemption.
#[test]
fn item_a_positive_fires_and_negative_is_clean() {
    let set = mechanical_set();
    let index = collect_credential_types(&set);

    let (pos_path, pos_content) = find(&set, "pos_xfile_retention.svelte");
    let pos_hits = scan_retention(pos_path, pos_content, &index);
    assert_eq!(pos_hits.len(), 1, "positive must fire exactly once");
    assert_eq!(pos_hits[0].rule_id, AUTH_STATE_RULE_ID);
    assert_eq!(pos_hits[0].type_name, "AuthResult");

    let (neg_path, neg_content) = find(&set, "neg_xfile_retention.svelte");
    assert!(
        scan_retention(neg_path, neg_content, &index).is_empty(),
        "negative (same shape, credential field removed) must be clean"
    );
}

/// The F11 lock. Scanned ALONE, the retention file must be silent — its declaration
/// lives in another file. If this ever passes with a per-file kernel, every rule-2
/// assertion in the matrix above is vacuous.
#[test]
fn rule_two_is_cross_file_and_a_lone_file_proves_nothing() {
    let set = mechanical_set();
    let retention = find(&set, "pos_xfile_retention.svelte");

    let lonely_index = collect_credential_types(std::slice::from_ref(retention));
    assert!(
        scan_retention(&retention.0, &retention.1, &lonely_index).is_empty(),
        "retention file scanned alone must be silent — the declaration is elsewhere. \
         If this fires, the fixture is self-contained and does not test cross-file \
         resolution; if the SET case is silent instead, pass 1 has gone per-file and \
         rule 2 is structurally disabled."
    );

    let full_index = collect_credential_types(&set);
    assert_eq!(
        scan_retention(&retention.0, &retention.1, &full_index).len(),
        1,
        "the SAME file must fire when the declaration is in the index"
    );
}

/// The subset is STRUCTURAL: `auth_state_password_with_token` is a refinement of
/// `retained_credential_binding`, never independent. "Rule 1 fires, Rule 2 silent" is
/// impossible by construction (both are emitted from inside the retention branch), so
/// assert it as impossible — a missing case documents nothing.
#[test]
fn auth_state_findings_are_a_strict_subset_of_retention_findings() {
    let set = mechanical_set();
    let index = collect_credential_types(&set);
    let mut saw_refined = false;

    for (path, content) in &set {
        for hit in scan_retention(path, content, &index) {
            assert!(
                hit.rule_id == AUTH_STATE_RULE_ID || hit.rule_id == RETAINED_CRED_RULE_ID,
                "unexpected rule id {}",
                hit.rule_id
            );
            if hit.rule_id == AUTH_STATE_RULE_ID {
                saw_refined = true;
                assert!(
                    index.is_credential_bearing(&hit.type_name),
                    "an auth-state finding for a type that is not credential-bearing \
                     would mean the refinement escaped the retention gate"
                );
                assert!(
                    index.is_token_bearing(&hit.type_name),
                    "the refined rule must only fire when a session token is present"
                );
            }
        }
    }
    assert!(
        saw_refined,
        "matrix must exercise the refined rule at least once"
    );
}

/// Lens fixtures: the mechanical guard is CORRECTLY silent. Asserted so that silence
/// is pinned as the expected result rather than left ambiguous — and so that a later
/// change making the guard fire here (retention-free transmission detection in a
/// syntax matcher) fails loudly instead of looking like new coverage.
#[test]
fn lens_fixtures_are_silent_for_the_mechanical_guard() {
    let lens = lens_set();
    assert_eq!(
        lens.len(),
        3,
        "lens fixture set changed — update this assertion"
    );

    // Index built over BOTH sets, so silence cannot be an artefact of a missing
    // declaration: even with every declaration in scope, transmission does not fire.
    let mut all = mechanical_set();
    all.extend(lens.iter().cloned());
    let index = collect_credential_types(&all);

    for (path, content) in &lens {
        let hits = scan_retention(path, content, &index);
        assert!(
            hits.is_empty(),
            "lens fixture {} must be silent mechanically (transmission has no \
             mechanical counterpart); got {hits:?}",
            path.display()
        );
    }
}

/// Termination guard. Mutually recursive types are legal TS; without a visited set the
/// closure does not terminate, and a hang emits no VIOLATION line — just a 30s guard
/// timeout on every devloop, presenting as a performance problem.
#[test]
fn recursive_types_terminate() {
    let set = mechanical_set();
    let index = collect_credential_types(&set); // would hang here without the visited set
    assert!(!index.is_credential_bearing("TreeNode"));
    assert!(!index.is_credential_bearing("TreeLeaf"));
}
