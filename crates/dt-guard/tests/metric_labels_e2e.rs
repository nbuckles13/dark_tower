//! Proof-of-trap fixtures for `dt-guard metric-labels` — every emittable rule
//! ID: `label_secret`, `label_pii`, `label_naming`, `literal_value_length`,
//! `unbounded_value`, `lazy_pii_safe_reason`, `metric_name_length`,
//! `metric_name_naming`, `parse_error`.
//!
//! Each fixture is a single `crates/gc-service/src/observability/metrics.rs`
//! (the PRIMARY scan path, which bypasses `is_scan_exempt` and is always
//! scanned). The `TempDir` root carries no `/tests/` or `/fixtures/` segment
//! (asserted in `common::new_root`), so the repo-relative path the scanner
//! computes is clean — the highest-risk vacuity path @observability flagged.
//! Each negative perturbs exactly ONE thing so the exact rule-id set is
//! `{target}`; every other label/metric-name is valid.
//!
//! No policy content is touched — no vocabulary, allowlist, or threshold edit.
//! Fixtures exercise the existing rules only (ADR-0002).

mod common;

use common::{assert_clean, assert_parity, run_all, Case, Expect};
use std::path::Path;

const METRICS_REL: &str = "crates/gc-service/src/observability/metrics.rs";

fn write_metrics(root: &Path, body: &str) {
    common::write(root, METRICS_REL, body);
}

fn negatives() -> Vec<Case> {
    vec![
        // label_secret: Cat-A denylist token `token` (non-bypassable). Value is
        // a bare ident (not a string literal, not an unbounded pattern), metric
        // name valid → single-fire.
        Case {
            name: "label_secret",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", \"token\" => v).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["label_secret"]),
        },
        // label_pii: Cat-B token `email`, NO `# pii-safe` marker present, so the
        // Cat-B-only suppression path is not a factor.
        Case {
            name: "label_pii",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", \"email\" => v).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["label_pii"]),
        },
        // label_naming: an uppercase + dash key. Not a PII token; value bare.
        Case {
            name: "label_naming",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", \"Bad-Key\" => v).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["label_naming"]),
        },
        // literal_value_length: a value string literal > 64 chars. Key valid,
        // metric valid, value is a plain literal (no unbounded pattern).
        Case {
            name: "literal_value_length",
            build: |root| {
                let val = "b".repeat(65);
                write_metrics(
                    root,
                    &format!(
                        "pub fn f() {{ counter!(\"gc_x_total\", \"region\" => \"{val}\").increment(1); }}\n"
                    ),
                );
            },
            expect: Expect::Exact(&["literal_value_length"]),
        },
        // unbounded_value: value expr contains `request_path` (an unbounded
        // heuristic). Key valid, value not a string literal (no length hit).
        Case {
            name: "unbounded_value",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", \"region\" => request_path).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["unbounded_value"]),
        },
        // lazy_pii_safe_reason: a `// pii-safe: <lazy>` marker (reason in the
        // lazy vocabulary). Scanned in its own pass; the counter is clean.
        Case {
            name: "lazy_pii_safe_reason",
            build: |root| {
                write_metrics(
                    root,
                    "// pii-safe: todo\n\
                     pub fn f() { counter!(\"gc_x_total\", \"region\" => region).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["lazy_pii_safe_reason"]),
        },
        // metric_name_length: metric name literal > 64 chars, still snake_case
        // (so `metric_name_naming` stays silent). Label valid.
        Case {
            name: "metric_name_length",
            build: |root| {
                let name = "a".repeat(65);
                write_metrics(
                    root,
                    &format!(
                        "pub fn f() {{ counter!(\"{name}\", \"region\" => region).increment(1); }}\n"
                    ),
                );
            },
            expect: Expect::Exact(&["metric_name_length"]),
        },
        // metric_name_naming: metric name not snake_case (uppercase), short (so
        // `metric_name_length` stays silent). Label valid.
        Case {
            name: "metric_name_naming",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"BadName\", \"region\" => region).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["metric_name_naming"]),
        },
        // parse_error: a second arg with no `=>` that is not a string literal
        // (and `counter!` is not a describe macro). Metric name valid.
        Case {
            name: "parse_error",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", bad_arg_no_arrow).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["parse_error"]),
        },
    ]
}

#[test]
fn negatives_trap_their_rule_ids() {
    run_all("metric-labels", &negatives());
}

/// Clean: a valid, bounded, non-PII, snake_case label → STATUS=OK naming ≥1
/// file scanned (the negatives above are the proof the scanner reaches labels).
#[test]
fn clean_metrics_file_passes_non_vacuously() {
    let root = common::new_root();
    write_metrics(
        root.path(),
        "pub fn f() { counter!(\"gc_x_total\", \"region\" => region).increment(1); }\n",
    );
    assert_clean(root.path(), "metric-labels", "metric-labels-clean-1-files");
}

/// Source-derived SSoT: every `*_RULE_ID` const is fixtured; no exclusions.
#[test]
fn rule_id_inventory_is_complete() {
    assert_parity("metric_labels.rs", &negatives(), &[]);
}
