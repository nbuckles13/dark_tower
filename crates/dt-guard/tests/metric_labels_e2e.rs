//! Proof-of-trap fixtures for `dt-guard metric-labels` — every emittable rule
//! ID: `label_secret`, `label_pii`, `label_naming`, `literal_value_length`,
//! `unbounded_value`, `lazy_pii_safe_reason`, `metric_name_length`,
//! `metric_name_naming`, `parse_error`, and the kind-label rules
//! `kind_label_literal`, `kind_label_on_mh`, `kind_label_vacuous`.
//!
//! Every fixture tree carries the kind-label SCAFFOLD (the key constants in
//! `labels.rs`, the Rust projection home, and one projection-valued kind site),
//! because the kind rules refuse to report clean without them; the
//! `kind_label_vacuous` case is the tree WITHOUT it.
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

const LABELS_REL: &str = "crates/common/src/observability/labels.rs";
const HOME_REL: &str = "crates/mc-service/src/media_kind.rs";
const MH_METRICS_REL: &str = "crates/mh-service/src/observability/metrics.rs";
/// One kind-labelled site whose values come from the projection (green).
const KIND_SITE: &str = "pub fn k(l: L) { counter!(\"gc_k_total\", MEDIA_KIND_LABEL => l.media_kind.label(), CONTENT_KIND_LABEL => l.content_kind.label()).increment(1); }\n";

/// A projection home the guard accepts: `KindLabels`, `from_proto`, and one
/// two-valued `label()` per kind key.
const HOME_BODY: &str = "pub struct KindLabels;\n\
impl KindLabels {\n    pub const fn from_proto(k: K) -> Option<Self> { None }\n}\n\
impl A {\n    pub const fn label(self) -> &'static str {\n        match self {\n            Self::A => \"audio\",\n            Self::V => \"video\",\n        }\n    }\n}\n\
impl C {\n    pub const fn label(self) -> &'static str {\n        match self {\n            Self::M => \"main\",\n            Self::S => \"screen\",\n        }\n    }\n}\n";

/// The kind-label inputs: key constants and a projection home holding values.
fn write_kind_scaffold(root: &Path) {
    common::write(
        root,
        LABELS_REL,
        "pub const MEDIA_KIND_LABEL: &str = \"media_kind\";\npub const CONTENT_KIND_LABEL: &str = \"content_kind\";\n",
    );
    common::write(root, HOME_REL, HOME_BODY);
}

fn write_metrics(root: &Path, body: &str) {
    write_kind_scaffold(root);
    common::write(root, METRICS_REL, &format!("{body}{KIND_SITE}"));
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
        // kind_label_literal: a kind key (constant spelling) whose value is a
        // literal, outside the projection home.
        Case {
            name: "kind_label_literal",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", MEDIA_KIND_LABEL => \"audio\").increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["kind_label_literal"]),
        },
        // kind_label_on_mh: a projection-valued kind label, but on an MH metric.
        Case {
            name: "kind_label_on_mh",
            build: |root| {
                write_metrics(
                    root,
                    "pub fn f() { counter!(\"gc_x_total\", \"region\" => region).increment(1); }\n",
                );
                common::write(root, MH_METRICS_REL, KIND_SITE);
            },
            expect: Expect::Exact(&["kind_label_on_mh"]),
        },
        // kind_label_vacuous: the tree WITHOUT the scaffold — no key constants
        // and no projection home is never "nothing to check".
        Case {
            name: "kind_label_vacuous",
            build: |root| {
                common::write(
                    root,
                    METRICS_REL,
                    "pub fn f() { counter!(\"gc_x_total\", \"region\" => region).increment(1); }\n",
                );
            },
            expect: Expect::Exact(&["kind_label_vacuous"]),
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
