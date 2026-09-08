//! Fixture catalog + in-tree premise pin for `dt-guard media-telemetry-deny`.
//!
//! ADR-0036 §11's acceptance frame is that **a control's coverage must be
//! demonstrated, not asserted**, and that a demonstration has two halves:
//! does the control *fire*, and does it *apply*. Both halves live here.
//!
//! * **Fires** — [`catalog`] drives every `pos_*` fixture through
//!   `check_file` and asserts the per-rule hit counts stated in each
//!   fixture's `// Invariant:` block, and every `neg_*` fixture through the
//!   same function asserting silence.
//! * **Applies** — [`configured_scope_is_a_real_non_empty_rust_tree`] asserts
//!   against the **real repository** that every directory in the shipped
//!   manifest resolves, is a directory, and holds at least one `.rs` file.
//!   That is "confirm the premise against the real artifact", machine-checked
//!   on every `cargo test` rather than narrated in a comment.
//!
//! # The *applies* half is not paranoia — the in-tree precedent
//!
//! A guard silently disarmed by a directory rename reports clean forever, and
//! reads as coverage while covering nothing. That is not hypothetical in this
//! repository: `crates/dt-guard/src/env_config.rs:23-27` records the
//! env-config guard **warn-skipping `mc-service` and `mh-service` while
//! emitting `STATUS=OK REASON=env-config-clean-4-services`** — a clean verdict
//! carrying a confident count that included the two services it had not
//! checked. The WARNING went to stderr; the STATUS line said clean; nobody
//! noticed.
//!
//! Anyone weighing whether the scope-missing and scope-empty tests below are
//! over-engineering should read that entry first. They are the difference
//! between a control and a control-shaped object, and the failure they prevent
//! has already happened here once.
//!
//! # The expectation is the fixture owner's, not the walker's
//!
//! `catalog()`'s rows are transcribed from the `// Invariant:` paragraphs the
//! fixtures carry. If this harness disagrees with a fixture, **the fixture
//! wins and the disagreement is a question for `observability`** (CLAUDE.md
//! §Specialists). Editing a row to match the implementation would be
//! precisely the "a guard reporting clean while the offending line ships
//! reads as coverage" failure, relocated into the test for it.
//!
//! # Design — direct library invocation, one test walking the catalog
//!
//! `check_file` is a pure function of `(rel_path, content)`, so the fixtures
//! are read as data and no synthetic repo root is needed here. The
//! wire-format layer (manifest parsing, scope liveness, STATUS emission,
//! exit codes) is exercised end-to-end by
//! `scripts/guards/media-telemetry-deny.test.sh` against throwaway roots.
//!
//! One test walks the whole table rather than one test per fixture — the
//! deliberate counter-pattern `cite_extract_e2e.rs` documents. A per-fixture
//! test multiplies boilerplate without adding a distinguishable failure: the
//! assertion message already names the file.

#![expect(
    clippy::panic,
    reason = "integration-test fixture helpers outside #[test] fns; a fixture that cannot be read MUST abort loudly rather than degrade to a vacuous pass — a silently-skipped `pos_` fixture is indistinguishable from a passing one, which is the exact failure this suite exists to detect (ADR-0002 test carve-out, clippy.toml §Integration tests)"
)]

use dt_guard::media_telemetry_deny::{check_file, Rule};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Expected per-rule hit counts for one fixture, transcribed from its
/// `// Invariant:` block.
#[derive(Debug)]
struct Expectation {
    /// File name under `tests/fixtures/media_telemetry_deny/`.
    file: &'static str,
    /// `(rule suffix, count)` pairs. A rule absent from this list must
    /// produce zero hits — the assertion compares the whole map, so an
    /// unexpected rule firing is a failure even if the expected ones match.
    per_rule: &'static [(&'static str, usize)],
}

fn fixtures_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR`, never `current_dir()`: the latter depends on how
    // the test binary was invoked.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media_telemetry_deny")
}

fn catalog() -> Vec<Expectation> {
    vec![
        // ---- Fires: metrics family (derived from `MacroKind::ALL`) ----
        Expectation {
            file: "pos_metrics_label_macros.rs",
            per_rule: &[("macro-in-media-path", 4)],
        },
        Expectation {
            file: "pos_metrics_describe_macros.rs",
            per_rule: &[("macro-in-media-path", 4)],
        },
        // ---- Fires: tracing / log / event ----
        Expectation {
            file: "pos_tracing_levels.rs",
            per_rule: &[("macro-in-media-path", 7)],
        },
        Expectation {
            file: "pos_tracing_event.rs",
            per_rule: &[("macro-in-media-path", 2), ("telemetry-crate-import", 1)],
        },
        Expectation {
            file: "pos_log_crate_qualified.rs",
            per_rule: &[("macro-in-media-path", 3), ("telemetry-crate-import", 1)],
        },
        // ---- Fires: span family ----
        Expectation {
            file: "pos_span_bare.rs",
            per_rule: &[("macro-in-media-path", 2), ("telemetry-crate-import", 1)],
        },
        Expectation {
            file: "pos_span_suffixed.rs",
            per_rule: &[("macro-in-media-path", 5)],
        },
        Expectation {
            file: "pos_span_open_shape.rs",
            per_rule: &[("macro-in-media-path", 2)],
        },
        // ---- Fires: print family ----
        Expectation {
            file: "pos_print_family.rs",
            per_rule: &[("macro-in-media-path", 5)],
        },
        // ---- Fires: whitespace-before-`!` and non-paren delimiters ----
        // Added 2026-09-08. The whitespace class was a live evasion; the
        // delimiter class was already denied in code but pinned only by unit
        // tests, and the catalog is the artifact a later auditor reads as the
        // coverage inventory.
        Expectation {
            file: "pos_macro_space_before_bang.rs",
            per_rule: &[("macro-in-media-path", 6)],
        },
        // ---- Fires: the allow is BY CONSTRUCTION, demonstrated ----
        // One physical line, a handle call and a real `counter!`. Exactly one
        // hit: not zero (a line filter would mask it), not two (flagging
        // `.increment` would ban the pattern §11 exists to enforce).
        Expectation {
            file: "pos_handle_call_colocated_with_macro.rs",
            per_rule: &[("macro-in-media-path", 1)],
        },
        // ---- Fires: `#[instrument]` attribute ----
        Expectation {
            file: "pos_instrument_bare.rs",
            per_rule: &[("macro-in-media-path", 2)],
        },
        Expectation {
            file: "pos_instrument_qualified.rs",
            per_rule: &[("macro-in-media-path", 2)],
        },
        // ---- Fires: telemetry-crate import deny ----
        Expectation {
            file: "pos_import_renamed_tracing.rs",
            per_rule: &[("macro-in-media-path", 1), ("telemetry-crate-import", 3)],
        },
        Expectation {
            file: "pos_import_leading_colons_tracing.rs",
            per_rule: &[("telemetry-crate-import", 4)],
        },
        Expectation {
            file: "pos_import_brace_group.rs",
            per_rule: &[("telemetry-crate-import", 3)],
        },
        Expectation {
            file: "pos_unparseable_use.rs",
            per_rule: &[("telemetry-crate-import", 1), ("unparseable-use", 2)],
        },
        // ---- Fires: no suppression path, no test-block exemption ----
        Expectation {
            file: "pos_ignore_annotation_not_honored.rs",
            per_rule: &[("macro-in-media-path", 2)],
        },
        Expectation {
            file: "pos_macro_in_test_module.rs",
            per_rule: &[("macro-in-media-path", 1)],
        },
        // ---- Fires: lifetime ticks must not blank the rest of the line ----
        // Regression pin for the live bypass @security reproduced 2026-09-07.
        Expectation {
            file: "pos_lifetime_tick_does_not_blank.rs",
            per_rule: &[("macro-in-media-path", 8), ("telemetry-crate-import", 1)],
        },
        // ---- Fires: the redaction sentinel carrier ----
        Expectation {
            file: "pos_argument_text_not_echoed.rs",
            per_rule: &[("macro-in-media-path", 3)],
        },
        // ---- Applies: the negatives that must stay green ----
        Expectation {
            file: "neg_cached_handles.rs",
            per_rule: &[],
        },
        Expectation {
            file: "neg_bare_identifiers.rs",
            per_rule: &[],
        },
        Expectation {
            file: "neg_comments_only.rs",
            per_rule: &[],
        },
        Expectation {
            file: "neg_import_crate_local_paths.rs",
            per_rule: &[],
        },
        Expectation {
            file: "neg_non_telemetry_macros.rs",
            per_rule: &[],
        },
    ]
}

/// Every fixture on disk must have a catalog row, and vice versa.
///
/// Without this, adding a fixture and forgetting its row means the fixture is
/// never run — and a `pos_` fixture that is never run is indistinguishable
/// from a passing one, which is the same empty-result-reads-as-success shape
/// the guard itself exists to prevent.
#[test]
fn catalog_covers_every_fixture_on_disk() {
    let dir = fixtures_dir();
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".rs"))
        .collect();
    on_disk.sort();

    let mut in_catalog: Vec<String> = catalog().iter().map(|e| e.file.to_string()).collect();
    in_catalog.sort();

    assert_eq!(
        on_disk, in_catalog,
        "fixture directory and catalog disagree — an uncatalogued fixture never runs"
    );
}

/// Drive every fixture through the policy function and compare the full
/// per-rule hit map.
///
/// Comparing whole maps rather than totals is deliberate: a fixture expecting
/// 3 `macro-in-media-path` would otherwise pass while producing 2 of those
/// and 1 spurious `telemetry-crate-import`.
#[test]
fn fixtures_match_their_invariant_blocks() {
    let dir = fixtures_dir();
    let mut failures: Vec<String> = Vec::new();

    for exp in catalog() {
        let path = dir.join(exp.file);
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

        let findings = check_file(exp.file, &content);

        let mut actual: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &findings {
            *actual.entry(f.rule.suffix()).or_insert(0) += 1;
        }
        let expected: BTreeMap<&str, usize> = exp.per_rule.iter().copied().collect();

        if actual != expected {
            failures.push(format!(
                "{}: expected {expected:?}, got {actual:?} — the fixture's `// Invariant:` block is the specification; if the walker is right, that is a question for @observability, NOT an edit to this row",
                exp.file
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "fixture expectation mismatches:\n{}",
        failures.join("\n")
    );
}

/// No finding may carry text read out of the scanned file.
///
/// The identifiers ADR-0036 §11 exists to keep out of logs live inside the
/// denied macro's argument list. A guard that echoed the matched span would
/// move that identifier from a process-local log into CI output and artifact
/// retention — the control reproducing the leak it exists to prevent, on a
/// worse surface, in the moment it reports success.
///
/// The structural half is that `common::explain::SecretFinding` has no
/// `matched` field. This is the behavioural half.
#[test]
fn findings_never_echo_scanned_file_content() {
    const SENTINEL: &str = "SENTINEL_MUST_NOT_APPEAR_IN_GUARD_OUTPUT";
    let path = fixtures_dir().join("pos_argument_text_not_echoed.rs");
    let content = std::fs::read_to_string(&path).expect("fixture readable");
    assert!(
        content.contains(SENTINEL),
        "fixture must actually carry the sentinel, or this test is vacuous"
    );

    let findings = check_file("pos_argument_text_not_echoed.rs", &content);
    assert!(
        !findings.is_empty(),
        "fixture must fire, or absence proves nothing"
    );

    for f in &findings {
        assert!(
            !f.spelling.contains(SENTINEL),
            "finding for {}:{} carried argument text: {}",
            f.file,
            f.line,
            f.spelling
        );
        // Spellings are macro forms, not source excerpts.
        assert!(
            f.spelling.ends_with('!')
                || f.spelling == "#[instrument]"
                || f.spelling.starts_with("use "),
            "unexpected spelling shape (is this a source excerpt?): {}",
            f.spelling
        );
    }
}

/// **The APPLIES half.** Assert against the real repository that the shipped
/// manifest's scope is a real, non-empty Rust tree.
///
/// Without this, every fixture test above could pass while the manifest
/// pointed at a directory that no longer exists — the control alive and out
/// of scope, which ADR-0036 §11 names as reading like coverage while covering
/// nothing.
///
/// Deliberately asserts "≥ 1 `.rs` file", never a hardcoded count: adding a
/// media file must not red this, and removing the last one must.
#[test]
fn configured_scope_is_a_real_non_empty_rust_tree() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves");
    let manifest_path = repo_root.join("scripts/guards/simple/media-telemetry-deny.yaml");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", manifest_path.display()));

    // Minimal read of the one key, so this pin does not depend on the guard's
    // own parsing being correct — if the guard's deserialization broke, this
    // test should still be able to state what the manifest says.
    let dirs: Vec<String> = raw
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("denied_directories:"))
        .skip(1)
        .take_while(|l| l.trim_start().starts_with("- "))
        .map(|l| l.trim().trim_start_matches("- ").trim().to_string())
        .collect();

    assert!(
        !dirs.is_empty(),
        "the shipped manifest configures no directories — the guard would check nothing"
    );

    for rel in &dirs {
        let abs = repo_root.join(rel.trim_end_matches('/'));
        assert!(
            abs.is_dir(),
            "configured scope `{rel}` is not a directory in-tree — the guard is alive but applied to nothing (ADR-0036 §11)"
        );
        let rs_count = walkdir_count_rs(&abs);
        assert!(
            rs_count >= 1,
            "configured scope `{rel}` holds zero `.rs` files — the guard would walk nothing and report clean"
        );
    }
}

fn walkdir_count_rs(dir: &std::path::Path) -> usize {
    let mut n = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                n += 1;
            }
        }
    }
    n
}

/// The real media tree must be CLEAN under this guard.
///
/// Complements the scope pin above: that one proves the scope is real, this
/// proves the guard actually passes on it. A guard that is in scope and
/// permanently red would be reverted rather than obeyed, so its green on the
/// real artifact is part of the demonstration.
#[test]
fn real_media_tree_is_clean() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves");
    let media = repo_root.join("crates/mh-service/src/media");
    if !media.is_dir() {
        // The scope pin above is the test that fails loudly for this; do not
        // duplicate its failure here.
        return;
    }

    let mut offenders: Vec<String> = Vec::new();
    let mut stack = vec![media.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&p) else {
                continue;
            };
            let rel = p
                .strip_prefix(&repo_root)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string();
            for f in check_file(&rel, &content) {
                offenders.push(format!(
                    "{}:{} [{}] {}",
                    f.file,
                    f.line,
                    f.rule.suffix(),
                    f.spelling
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "ADR-0036 §11 violation(s) in the real media path — report to @observability and @media-handler; do NOT weaken the guard:\n{}",
        offenders.join("\n")
    );
    let _ = Rule::ORDER;
}
