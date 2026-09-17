//! Proof-of-trap fixtures for `dt-guard alert-rules-policy` — every emittable
//! rule ID: `runbook_url`, `severity`, `for_duration`, `annotation_hygiene`,
//! `lazy_ignore_reason`, `rule_file_loading`, `inventory_expr_drift`.
//!
//! # Two whole-guard interactions the fixtures respect
//!
//! * `rule_file_loading` co-fires on any root where the Prometheus configs +
//!   kustomization do not agree with the loadable rules set. So every per-alert
//!   negative ships the full VALID loading apparatus (both `prometheus.yml`
//!   configs with a `rules/*-alerts.yaml` glob, the kustomization
//!   `configMapGenerator`, a lowercase `mc-alerts.yaml`) and perturbs exactly
//!   one alert field — exact set `{target}`.
//! * The `rule_file_loading` negative is aimed at a REAL policy cause — a
//!   `.yml`-suffixed loadable-name miss — NOT "config not found".
//!
//! Fixture content uses reserved/published placeholders only (@security S5):
//! `192.0.2.1` is RFC-5737 (absent from `IPV4_ALLOWLIST`); the traversal paths
//! are strings/created-inside-the-tree, never a resolvable host or real target.

mod common;

use common::{assert_clean, assert_parity, run_all, Case, Expect};
use std::path::Path;

const ALERT_REL: &str = "infra/docker/prometheus/rules/mc-alerts.yaml";
const RUNBOOK_OK: &str = "docs/runbooks/my.md";

/// A valid Prometheus rule-loading apparatus: both configs point a
/// `rules/*-alerts.yaml` glob at the mounted dir, and the kustomization
/// `configMapGenerator` declares `mc-alerts.yaml`. Silences `rule_file_loading`.
fn write_loading_apparatus(root: &Path) {
    let prom = "global:\n  scrape_interval: 15s\nrule_files:\n  - \"rules/*-alerts.yaml\"\nscrape_configs: []\n";
    common::write(root, "infra/kubernetes/observability/prometheus.yml", prom);
    common::write(root, "infra/docker/prometheus/prometheus.yml", prom);
    common::write(
        root,
        "infra/docker/prometheus/kustomization.yaml",
        "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\n\
         configMapGenerator:\n  - name: prometheus-rules\n    files:\n      - rules/mc-alerts.yaml\n",
    );
    common::write(root, RUNBOOK_OK, "# runbook\n");
}

/// One alert file with a single `MyAlert` rule. Each argument is the field to
/// perturb; the defaults (via [`valid_alert`]) are all-valid.
fn alert_yaml(
    runbook: &str,
    severity: &str,
    for_dur: &str,
    expr: &str,
    summary: &str,
    ignore_comment: bool,
) -> String {
    let ignore = if ignore_comment {
        "        # guard:ignore(todo)\n"
    } else {
        ""
    };
    format!(
        "groups:\n  - name: g\n    rules:\n\
         {ignore}      - alert: MyAlert\n\
         \x20       expr: '{expr}'\n\
         \x20       for: {for_dur}\n\
         \x20       labels:\n          severity: {severity}\n\
         \x20       annotations:\n          runbook_url: {runbook}\n          summary: {summary}\n"
    )
}

const GOOD_EXPR: &str = "sum(rate(mc_x_total[5m])) > 0";

fn valid_alert() -> String {
    alert_yaml(
        RUNBOOK_OK,
        "warning",
        "5m",
        GOOD_EXPR,
        "all good here",
        false,
    )
}

/// Inventory doc (`alerts.md`) whose `#### MyAlert` restates `expr` and `for`.
fn inventory_md(expr: &str, for_dur: &str) -> String {
    format!("# Alerts\n\n#### MyAlert\n\n```promql\n{expr}\n```\n\n`for: {for_dur}`\n")
}

/// Lay the apparatus + a valid alert; the caller supplies the perturbed alert.
fn write_with_apparatus(root: &Path, alert: &str) {
    write_loading_apparatus(root);
    common::write(root, ALERT_REL, alert);
}

fn negatives() -> Vec<Case> {
    vec![
        // runbook_url branch (i): repo-root escape → resolve_cited_path None.
        Case {
            name: "runbook_url_repo_root_escape",
            build: |root| {
                write_with_apparatus(
                    root,
                    &alert_yaml(
                        "docs/runbooks/../../etc/passwd",
                        "warning",
                        "5m",
                        GOOD_EXPR,
                        "all good here",
                        false,
                    ),
                );
            },
            expect: Expect::ExactWithMsg {
                ids: &["runbook_url"],
                msg: &["cannot be resolved or escapes docs/runbooks/"],
            },
        },
        // runbook_url branch (ii): stays inside the repo but leaves the
        // runbooks dir → resolve succeeds, !starts_with(runbooks). The target
        // is created inside the tree so only the containment branch rejects.
        Case {
            name: "runbook_url_subdir_escape",
            build: |root| {
                common::write(root, "docs/decisions/adr.md", "# adr\n");
                write_with_apparatus(
                    root,
                    &alert_yaml(
                        "docs/runbooks/../decisions/adr.md",
                        "warning",
                        "5m",
                        GOOD_EXPR,
                        "all good here",
                        false,
                    ),
                );
            },
            expect: Expect::ExactWithMsg {
                ids: &["runbook_url"],
                msg: &["escapes docs/runbooks/ via traversal or symlink"],
            },
        },
        // severity: not in {page, warning, info}.
        Case {
            name: "severity",
            build: |root| {
                write_with_apparatus(
                    root,
                    &alert_yaml(RUNBOOK_OK, "bogus", "5m", GOOD_EXPR, "all good here", false),
                );
            },
            expect: Expect::Exact(&["severity"]),
        },
        // for_duration: for < 30s AND the expr has no qualifying window to rescue it.
        Case {
            name: "for_duration",
            build: |root| {
                write_with_apparatus(
                    root,
                    &alert_yaml(
                        RUNBOOK_OK,
                        "warning",
                        "10s",
                        "up == 0",
                        "all good here",
                        false,
                    ),
                );
            },
            expect: Expect::Exact(&["for_duration"]),
        },
        // annotation_hygiene: a non-allowlisted (RFC-5737) IPv4 in the summary.
        // Reached via the else-branch (runbook/severity/for all valid).
        Case {
            name: "annotation_hygiene",
            build: |root| {
                write_with_apparatus(
                    root,
                    &alert_yaml(
                        RUNBOOK_OK,
                        "warning",
                        "5m",
                        GOOD_EXPR,
                        "contact 192.0.2.1 now",
                        false,
                    ),
                );
            },
            expect: Expect::Exact(&["annotation_hygiene"]),
        },
        // lazy_ignore_reason: a syntactically valid guard:ignore directive with
        // a LAZY reason (the alert itself is otherwise valid).
        Case {
            name: "lazy_ignore_reason",
            build: |root| {
                write_with_apparatus(
                    root,
                    &alert_yaml(
                        RUNBOOK_OK,
                        "warning",
                        "5m",
                        GOOD_EXPR,
                        "all good here",
                        true,
                    ),
                );
            },
            expect: Expect::Exact(&["lazy_ignore_reason"]),
        },
        // rule_file_loading: a real policy cause — a `.yml`-suffixed file that
        // run() lints but loadable_rules_files rejects (only `-alerts.yaml`), so
        // the loadable set is empty (linted-but-never-loaded). The alert is
        // valid, so nothing else fires.
        Case {
            name: "rule_file_loading",
            build: |root| {
                common::write(root, RUNBOOK_OK, "# runbook\n");
                common::write(
                    root,
                    "infra/docker/prometheus/rules/mc-alerts.yml",
                    &valid_alert(),
                );
            },
            expect: Expect::Exact(&["rule_file_loading"]),
        },
        // inventory_expr_drift: the inventory's #### MyAlert restates a different
        // expr than the rule (for matches, so only expr drift fires).
        Case {
            name: "inventory_expr_drift",
            build: |root| {
                write_with_apparatus(root, &valid_alert());
                common::write(
                    root,
                    "docs/observability/alerts.md",
                    &inventory_md("sum(rate(mc_x_total[5m])) > 999", "5m"),
                );
            },
            expect: Expect::Exact(&["inventory_expr_drift"]),
        },
    ]
}

#[test]
fn negatives_trap_their_rule_ids() {
    run_all("alert-rules-policy", &negatives());
}

/// Clean: valid alert + full apparatus + a MATCHING inventory pair → STATUS=OK
/// naming ≥1 file, ≥1 loadable-covered, AND ≥1 inventory-pair (so the inventory
/// path is proven non-vacuous, not just the loading path).
#[test]
fn clean_alert_passes_non_vacuously() {
    let root = common::new_root();
    write_with_apparatus(root.path(), &valid_alert());
    common::write(
        root.path(),
        "docs/observability/alerts.md",
        &inventory_md(GOOD_EXPR, "5m"),
    );
    assert_clean(
        root.path(),
        "alert-rules-policy",
        "alert-rules-clean-1-files-1-loadable-covered-1-inventory-pairs",
    );
}

/// Source-derived SSoT: every `*_RULE_ID` const is fixtured; no exclusions.
/// (Two `runbook_url` cases both cover the same id — deduped in the parity set.)
#[test]
fn rule_id_inventory_is_complete() {
    assert_parity("alert_rules.rs", &negatives(), &[]);
}
