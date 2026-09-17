//! Proof-of-trap fixtures for `dt-guard dashboard-panels` — every emittable
//! rule ID (13). Each fixture is a synthetic root: one metrics.rs (declaring
//! the referenced metric types), one catalog, and one dashboard JSON with a
//! single perturbed panel.
//!
//! # Two whole-catalog interactions the fixtures must respect
//!
//! * `expected_empty_set_empty` fires whenever the catalogs yield ZERO
//!   `Expected-empty: yes` annotations. So its OWN fixture uses the annotation-
//!   free catalog, and EVERY other fixture (and the clean one) seeds one via
//!   `gc_seed_total` — otherwise it would contaminate their exact-set
//!   assertions.
//! * The metric-type / catalog rules need a real metrics.rs + catalog, or
//!   `metric_not_in_code` fires first and `continue`s past the misuse block.
//!
//! Isolation (each exact-set is `{target}`, which asserts every sibling absent):
//! `rate_window`/`metric_not_in_catalog` on a NON-governed `table` panel so
//! `counter_window` does not own the verdict; `gauge_misuse` with
//! `[$__rate_interval]` so `rate_window` stays silent; `histogram_misuse` on a
//! `_bucket` ref whose stripped base is the declared histogram.
//!
//! No policy content is touched (no threshold / panel-type-list edit).

mod common;

use common::{assert_clean, assert_parity, run_all, Case, Expect};
use std::path::Path;

/// Declares every metric the fixtures reference, with the type each rule needs.
const METRICS_BASE: &str = "pub fn m() {\n\
     \x20   counter!(\"gc_x_total\").increment(1);\n\
     \x20   counter!(\"gc_seed_total\").increment(1);\n\
     \x20   counter!(\"gc_uncatalogued_total\").increment(1);\n\
     \x20   counter!(\"mc_dropped_total\").increment(1);\n\
     \x20   counter!(\"gc_req_total\").increment(1);\n\
     \x20   gauge!(\"mc_active\").set(1.0);\n\
     \x20   histogram!(\"gc_dur_seconds\").record(1.0);\n\
     }\n";

/// Catalog WITH the seed `Expected-empty` annotation (silences
/// `expected_empty_set_empty`). Omits `gc_uncatalogued_total` on purpose (the
/// `metric_not_in_catalog` trap). `mc_dropped_total` is expected-empty +
/// non-exempt; `gc_req_total` is exempt — the pair the mixed-class panel needs.
const CATALOG_SEEDED: &str = "# GC Metrics\n\n\
     ### `gc_seed_total`\n- **Type**: Counter\n- **Expected-empty**: yes — seed so the vacuity control stays silent\n\n\
     ### `gc_x_total`\n- **Type**: Counter\n\n\
     ### `mc_active`\n- **Type**: Gauge\n\n\
     ### `gc_dur_seconds`\n- **Type**: Histogram\n\n\
     ### `mc_dropped_total`\n- **Type**: Counter\n- **Expected-empty**: yes — reads zero when healthy\n\n\
     ### `gc_req_total`\n- **Type**: Counter\n- **Zero-init**: exempt — free-form label, an unbounded domain\n";

/// Catalog with NO `Expected-empty` annotation — only for the
/// `expected_empty_set_empty` trap.
const CATALOG_NO_EE: &str = "# GC Metrics\n\n### `gc_x_total`\n- **Type**: Counter\n";

/// JSON-encode a string that carries no `"` or `\` (fixtures keep exprs and
/// descriptions quote-free).
fn j(s: &str) -> String {
    assert!(
        !s.contains('"') && !s.contains('\\'),
        "fixture string must be quote/backslash-free: {s:?}"
    );
    format!("\"{s}\"")
}

/// Build one panel. `with_unit`/`ds_uid` let the unit and datasource rules be
/// perturbed; every other fixture passes `with_unit=true, ds_uid="$datasource"`.
fn panel(ptype: &str, expr: &str, description: &str, with_unit: bool, ds_uid: &str) -> String {
    let unit = if with_unit {
        "\"fieldConfig\":{\"defaults\":{\"unit\":\"short\"}},"
    } else {
        ""
    };
    format!(
        "{{\"type\":\"{ptype}\",\"id\":1,\"title\":\"T\",\"description\":{desc},{unit}\
         \"datasource\":{{\"type\":\"prometheus\",\"uid\":\"{ds_uid}\"}},\
         \"targets\":[{{\"refId\":\"A\",\"expr\":{expr},\
         \"datasource\":{{\"type\":\"prometheus\",\"uid\":\"$datasource\"}}}}]}}",
        desc = j(description),
        expr = j(expr),
    )
}

fn dashboard(panel_json: &str) -> String {
    format!("{{\"panels\":[{panel_json}]}}")
}

/// Lay metrics.rs + catalog + one dashboard into the root.
fn write_tree(root: &Path, catalog: &str, panel_json: &str) {
    common::write(
        root,
        "crates/gc-service/src/observability/metrics.rs",
        METRICS_BASE,
    );
    common::write(root, "docs/observability/metrics/gc-service.md", catalog);
    common::write(
        root,
        "infra/grafana/dashboards/gc.json",
        &dashboard(panel_json),
    );
}

/// A clean, valid timeseries panel over a counter (correct window). Used as the
/// good baseline the perturbation fixtures start from.
fn clean_panel() -> String {
    panel(
        "timeseries",
        "sum by (x) (increase(gc_x_total[$__rate_interval]))",
        "healthy",
        true,
        "$datasource",
    )
}

fn negatives() -> Vec<Case> {
    vec![
        Case {
            name: "panel_unit",
            build: |root| {
                let p = panel(
                    "timeseries",
                    "sum by (x) (increase(gc_x_total[$__rate_interval]))",
                    "healthy",
                    false, // unit removed
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["panel_unit"]),
        },
        Case {
            name: "hardcoded_datasource",
            build: |root| {
                let p = panel(
                    "timeseries",
                    "sum by (x) (increase(gc_x_total[$__rate_interval]))",
                    "healthy",
                    true,
                    "prometheus", // hard-coded panel datasource
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["hardcoded_datasource"]),
        },
        Case {
            name: "rate_window",
            build: |root| {
                // Non-governed `table` panel: the counter-window rule does not
                // own the verdict, so a hard-coded [5m] trips generic rate_window.
                let p = panel(
                    "table",
                    "sum(rate(gc_x_total[5m]))",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["rate_window"]),
        },
        Case {
            name: "counter_window",
            build: |root| {
                // Stat over a *_total counter using $__rate_interval — a stat
                // wants $__range, so counter_window fires (rate_window defers).
                let p = panel(
                    "stat",
                    "sum(increase(gc_x_total[$__rate_interval]))",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["counter_window"]),
        },
        Case {
            name: "counter_misuse",
            build: |root| {
                // Bare counter, not inside rate()/increase().
                let p = panel("stat", "gc_x_total", "healthy", true, "$datasource");
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["counter_misuse"]),
        },
        Case {
            name: "gauge_misuse",
            build: |root| {
                // Gauge wrapped in rate(); $__rate_interval keeps rate_window silent.
                let p = panel(
                    "stat",
                    "rate(mc_active[$__rate_interval])",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["gauge_misuse"]),
        },
        Case {
            name: "histogram_misuse",
            build: |root| {
                // _bucket ref not inside rate(); stripped base is the declared histogram.
                let p = panel(
                    "stat",
                    "gc_dur_seconds_bucket",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["histogram_misuse"]),
        },
        Case {
            name: "metric_not_in_code",
            build: |root| {
                // gc_unknown_total is in neither metrics.rs nor catalog.
                let p = panel(
                    "stat",
                    "sum(increase(gc_unknown_total[$__range]))",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["metric_not_in_code"]),
        },
        Case {
            name: "metric_not_in_catalog",
            build: |root| {
                // gc_uncatalogued_total IS in metrics.rs but omitted from the
                // catalog; on a table panel so counter_window doesn't own it.
                let p = panel(
                    "table",
                    "sum(increase(gc_uncatalogued_total[$__range]))",
                    "healthy",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["metric_not_in_catalog"]),
        },
        Case {
            name: "lazy_ignore_reason",
            build: |root| {
                // Syntactically valid ignore directive with a LAZY reason.
                let p = panel(
                    "timeseries",
                    "sum by (x) (increase(gc_x_total[$__rate_interval]))",
                    "# guard:ignore(todo)",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["lazy_ignore_reason"]),
        },
        Case {
            name: "expected_empty_description",
            build: |root| {
                // Governed panel over an expected-empty (non-exempt) counter,
                // description lacking the "zero is healthy" marker.
                let p = panel(
                    "stat",
                    "sum(increase(gc_seed_total[$__range]))",
                    "total failures in range",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["expected_empty_description"]),
        },
        Case {
            name: "mixed_panel_attribution",
            build: |root| {
                // Panel over a zero-init'd expected-empty ref (mc_dropped_total)
                // AND an exempt ref (gc_req_total). Both markers present + the
                // exempt ref named, but NO count() discriminator → attribution.
                let p = panel(
                    "stat",
                    "sum(increase(mc_dropped_total[$__range])) + sum(increase(gc_req_total[$__range]))",
                    "zero is healthy for mc_dropped_total. empty is healthy for gc_req_total.",
                    true,
                    "$datasource",
                );
                write_tree(root, CATALOG_SEEDED, &p);
            },
            expect: Expect::Exact(&["mixed_panel_attribution"]),
        },
        Case {
            name: "expected_empty_set_empty",
            build: |root| {
                // Dashboards exist but the catalog yields ZERO Expected-empty
                // annotations. Panel is otherwise clean.
                write_tree(root, CATALOG_NO_EE, &clean_panel());
            },
            expect: Expect::Exact(&["expected_empty_set_empty"]),
        },
    ]
}

#[test]
fn negatives_trap_their_rule_ids() {
    run_all("dashboard-panels", &negatives());
}

/// Clean: a valid counter panel + seeded catalog → STATUS=OK naming ≥1 file.
#[test]
fn clean_dashboard_passes_non_vacuously() {
    let root = common::new_root();
    write_tree(root.path(), CATALOG_SEEDED, &clean_panel());
    assert_clean(
        root.path(),
        "dashboard-panels",
        "dashboard-panels-clean-1-files",
    );
}

/// Source-derived SSoT: every `*_RULE_ID` const is fixtured; no exclusions.
#[test]
fn rule_id_inventory_is_complete() {
    assert_parity("dashboard_panels.rs", &negatives(), &[]);
}
