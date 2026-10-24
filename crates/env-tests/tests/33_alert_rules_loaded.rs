//! Every alert rule on disk is actually LOADED by the running Prometheus.
//!
//! # WHAT A GREEN HERE PROVES — AND WHAT IT DOES NOT
//!
//! **The name check proves that the rules are loaded and evaluating.** It
//! proves **nothing** about whether any rule's expression can ever match.
//!
//! That distinction is not pedantry. A rule can load, evaluate, and be
//! incapable of matching (a selector naming a series that does not exist yields
//! an empty result, not an error). Two further tests below narrow the gap
//! without closing it:
//!
//! - `every_loaded_rule_expression_is_the_committed_one` — every loaded rule's
//!   `query` equals the committed `expr`, both normalised through Prometheus's
//!   own `/api/v1/format_query` (Prometheus re-stringifies what it loads, so a
//!   literal comparison would always fail).
//! - `story_3_rule_selectors_apply_to_real_series` — the APPLY half (ADR-0036
//!   §11) for the story-3 rules: every vector selector each rule names returns
//!   at least one series from a real pod, or is reported ACKNOWLEDGED-VACUOUS
//!   by name when it is a client series, whose APPLY is carried by
//!   `dt-guard client-metrics-export` (names, label keys),
//!   `packages/web-app/tests/clientMetricNames.test.ts` (names, label values)
//!   and the browser read-backs (the live pipe).
//!
//! Neither proves a rule FIRES. Client series DO reach Prometheus (the
//! collector's `prometheus` exporter and the `otel-collector` scrape job), and
//! `MCMediaMissingKeyMaterial` has been shown to match live data; the FIRE
//! halves of the story-3 client rules are in `31_gc_telemetry.rs`, of
//! `MHVideoEdgeStuckPending` in `39_mh_video_keyframes.rs`, and of
//! `MHTransportInconsistent` in `37_mh_stream_attribution.rs`. *"The alerts
//! loaded" must not be read as "the alerts fire"* (`docs/TODO.md`'s recorded
//! defect against the alerting chain): a test named `alert_rules_loaded` is
//! one careless reading away from being cited as evidence that alerting works.
//!
//! # Why this test exists at all
//!
//! Until 2026-09-09 nothing in this tree evaluated an alert rule: `rule_files:`
//! was commented out in the compose config and absent from the cluster config,
//! while `dt-guard alert-rules-policy` reported clean over rule *syntax*. Static
//! validation cannot see whether a process reads the files it validates. Two
//! failures in particular are invisible to config inspection and visible here:
//! a **tolerated parse failure** (Prometheus logs and continues), and a **mount
//! misconfiguration** (the ConfigMap is generated but not mounted, or mounted at
//! the wrong path).
//!
//! # Where the expected set comes from
//!
//! From the kustomize `configMapGenerator` list — the artifact that decides
//! which rule files are in the pod being queried — not from a predicate written
//! in this crate. `dt-guard alert-rules-policy` asserts set equality between
//! that list, the on-disk loadable-rules predicate, and both `rule_files` globs,
//! failing in either direction, so a file added to a glob but not the ConfigMap
//! fails Layer 3 before this test could silently skip it.
//!
//! # No skip-on-absent
//!
//! There is deliberately no `is_available` early return. "The rules are not
//! loaded" is precisely the state this test exists to go red on, so a self-skip
//! would be a check that ran where it could observe nothing and reported clean.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::alert_rules_loaded::{
    alert_expr_in_yaml, alert_names_in_yaml, compare_loaded_expr, formatted_query,
    loaded_alert_names, loaded_alert_queries, mounted_rules_files, vector_selectors, ExprCheck,
    RULES_DIR, RULES_KUSTOMIZATION,
};
use env_tests::fixtures::metric_hygiene::STORY_3_EAGER_MC_SERIES;
use env_tests::fixtures::PrometheusClient;
use env_tests::repo_root;
use std::collections::BTreeSet;

/// The story 2 rules this test was written for (named individually, and
/// separately from the "non-empty" control, because a non-empty on-disk set
/// proves only that the extractor ran: the pre-existing alerts would carry the
/// pass while a NEW rule group silently failed to parse and Prometheus
/// tolerated it). Story 3's rules are [`STORY_3_ALERTS`]; the two sets overlap
/// on purpose where a story-3 task re-derived a story-2 rule.
const STORY_2_ALERTS: &[&str] = &[
    "MCMediaGenerationDivergence",
    "MCMediaMissingKeyMaterial",
    "MHMediaEgressQueueOverflowRate",
];

/// The story 3 (OBS-B) rules: A1-A9, the send-directive pair, and the refreshed
/// `MCMediaGenerationDivergence`. Positive control for the name check, and the
/// set the APPLY test inspects.
const STORY_3_ALERTS: &[&str] = &[
    "MHVideoKeyframeStorm",
    "MHVideoSubscriberBacklogResets",
    "MHVideoEdgeStuckPending",
    "MHPausedSourceIngress",
    "MHTransportInconsistent",
    "ClientVideoServerMutedSenderFrames",
    "ClientVideoDecodeErrorRatio",
    "ClientVideoSlotUnfulfilled",
    "MCMediaMissingKeyMaterial",
    "MCMediaSendDirectiveDefect",
    "MCMediaSendDirectiveFailures",
    "MCMediaGenerationDivergence",
];

/// Client series: present only while a browser (or a test post) has emitted
/// within the collector's `metric_expiration`, so their absence on an idle
/// cluster is expected. Their APPLY is carried elsewhere, by three controls
/// that are not this test: `dt-guard client-metrics-export` (every selected
/// name is exported by the collector and every matched label key survives its
/// `keep_keys`), `packages/web-app/tests/clientMetricNames.test.ts` (every
/// selected name is SDK-emitted and every matched `reason` / `media_kind` VALUE
/// is in the SDK's vocabulary), and the R-27/R-29 browser read-backs (a real
/// browser's series stored through the live pipe). `31_gc_telemetry.rs` is the
/// FIRE of the pipeline, not their APPLY. Here they are REPORTED by name.
const CLIENT_SERIES_PREFIX: &str = "dt_client_";

/// Every `alert:` name across the loadable rules files on disk.
///
/// Panics with a COULD-NOT-EVALUATE reason on any read or parse problem. That
/// token is deliberately distinct from the content-mismatch token below: a
/// broken extractor and a genuinely-unloaded rule are different faults, and
/// collapsing them is how a broken extractor gets "fixed" by relaxing it.
fn on_disk_alert_names() -> BTreeSet<String> {
    on_disk_alert_exprs().into_keys().collect()
}

/// Every `alert:` name across the mounted rules files, with its committed `expr`.
///
/// Same COULD-NOT-EVALUATE discipline as the name extraction.
fn on_disk_alert_exprs() -> std::collections::BTreeMap<String, String> {
    let root = repo_root();
    // DERIVED FROM THE MOUNT DECISION, NOT FROM A PREDICATE WRITTEN HERE.
    // The `configMapGenerator` list is what puts files in the pod this test
    // queries, so it is the artifact under test. An earlier version hand-wrote
    // a mirror of the `rule_files` glob and its header claimed dt-guard kept
    // the two honest; it did not, and they already disagreed. See the module
    // rustdoc.
    let kust_path = root.join(RULES_KUSTOMIZATION);
    let kust = std::fs::read_to_string(&kust_path).unwrap_or_else(|e| {
        panic!(
            "COULD-NOT-EVALUATE: cannot read {}: {e}",
            kust_path.display()
        )
    });
    let mounted = mounted_rules_files(&kust).unwrap_or_else(|| {
        panic!(
            "COULD-NOT-EVALUATE: no `configMapGenerator` rule-file entries found in \
             {RULES_KUSTOMIZATION}. An empty mounted set makes every assertion below pass \
             vacuously, so this is a broken-extraction failure, NOT a clean result."
        )
    });

    let dir = root.join(RULES_DIR);
    let mut out = std::collections::BTreeMap::new();
    for file_name in &mounted {
        let path = dir.join(file_name);
        let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "COULD-NOT-EVALUATE: {RULES_KUSTOMIZATION} mounts {file_name}, but it cannot \
                 be read at {}: {e}",
                path.display()
            )
        });
        let parsed = alert_names_in_yaml(&content).unwrap_or_else(|| {
            panic!("COULD-NOT-EVALUATE: {file_name} did not parse as alert-rules YAML")
        });
        for alert in parsed {
            let expr = alert_expr_in_yaml(&content, &alert).unwrap_or_else(|| {
                panic!("COULD-NOT-EVALUATE: {file_name}: alert `{alert}` has no `expr`")
            });
            out.insert(alert, expr);
        }
    }
    out
}

#[tokio::test]
async fn every_on_disk_alert_rule_is_loaded_by_prometheus() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);

    let expected = on_disk_alert_names();
    // POSITIVE CONTROL #1 — the extraction produced something.
    assert!(
        !expected.is_empty(),
        "COULD-NOT-EVALUATE: extracted zero alert names from {RULES_DIR}"
    );

    let body = prometheus
        .rules()
        .await
        .expect("COULD-NOT-EVALUATE: GET /api/v1/rules failed");
    let loaded = loaded_alert_names(&body).unwrap_or_else(|| {
        panic!(
            "COULD-NOT-EVALUATE: /api/v1/rules did not return a successful, well-formed \
             rules envelope. Body starts: {}",
            body.chars().take(200).collect::<String>()
        )
    });

    // POSITIVE CONTROL #2 — this task's OWN rules loaded, not merely somebody's.
    // Checked before the subset assertion so a new group that failed to parse is
    // reported as itself rather than as one name among many.
    for name in STORY_2_ALERTS.iter().chain(STORY_3_ALERTS) {
        assert!(
            loaded.contains(*name),
            "RULE-NOT-LOADED: {name} is on disk but absent from Prometheus's loaded rule \
             groups. Prometheus TOLERATES a rule-file parse error by logging and continuing, \
             so a syntax error in a new group looks exactly like this. Check the Prometheus \
             pod logs, then that the rules ConfigMap is mounted at /etc/prometheus/rules and \
             matched by the `rule_files` glob."
        );
    }

    // Direction: on-disk SUBSET-OF loaded. Never the reverse — "every loaded
    // rule is on disk" is trivially true when nothing loaded at all.
    let missing: Vec<&String> = expected.difference(&loaded).collect();
    assert!(
        missing.is_empty(),
        "RULE-NOT-LOADED: {} of {} on-disk alert rules are absent from Prometheus's loaded \
         rule groups: {missing:?}. Causes this catches that config inspection cannot: a \
         tolerated rule-file parse failure, and a ConfigMap that is generated but not mounted \
         (or mounted at a path the `rule_files` glob does not cover).",
        missing.len(),
        expected.len()
    );
}

async fn format_via_prometheus(
    prometheus: &PrometheusClient,
    what: &str,
    alert: &str,
    expr: &str,
) -> Option<String> {
    let body = prometheus.format_query(expr).await.unwrap_or_else(|e| {
        panic!("COULD-NOT-EVALUATE: format_query for {what} of `{alert}`: {e}")
    });
    formatted_query(&body)
}

/// Every loaded rule's expression is the COMMITTED one — not merely a rule of
/// the same name. Both sides go through `/api/v1/format_query`, because
/// Prometheus serves the re-stringified parse in `query`, never the YAML text.
#[tokio::test]
async fn every_loaded_rule_expression_is_the_committed_one() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);

    let committed = on_disk_alert_exprs();
    assert!(
        !committed.is_empty(),
        "COULD-NOT-EVALUATE: extracted zero alert expressions from {RULES_DIR}"
    );
    let body = prometheus
        .rules()
        .await
        .expect("COULD-NOT-EVALUATE: GET /api/v1/rules failed");
    let loaded = loaded_alert_queries(&body).unwrap_or_else(|| {
        panic!("COULD-NOT-EVALUATE: /api/v1/rules did not return a well-formed rules envelope")
    });

    let mut drift = Vec::new();
    let mut compared = 0;
    for (alert, expr) in &committed {
        let Some(loaded_query) = loaded.get(alert) else {
            // Absence is the name test's finding (RULE-NOT-LOADED), not this one's.
            continue;
        };
        let c = format_via_prometheus(&prometheus, "the committed expr", alert, expr).await;
        let l = match loaded_query {
            Some(q) => format_via_prometheus(&prometheus, "the loaded query", alert, q).await,
            None => None,
        };
        match compare_loaded_expr(c.as_deref(), l.as_deref()) {
            ExprCheck::Match => compared += 1,
            ExprCheck::Drift { committed, loaded } => {
                drift.push(format!(
                    "{alert}:\n  committed: {committed}\n  loaded:    {loaded}"
                ));
            }
            ExprCheck::CouldNotEvaluate(why) => {
                panic!("COULD-NOT-EVALUATE: `{alert}`: {why}")
            }
        }
    }
    assert!(
        drift.is_empty(),
        "EXPR-DRIFT: {} loaded rule(s) evaluate an expression other than the committed one. \
         The deployed rules ConfigMap is stale (redeploy: content addressing rolls Prometheus \
         on a rules change) or a second copy of a rule name exists:\n{}",
        drift.len(),
        drift.join("\n")
    );
    // Positive control: the comparison actually ran over this task's rules.
    for name in STORY_3_ALERTS {
        assert!(
            committed.contains_key(*name) && loaded.contains_key(*name),
            "COULD-NOT-EVALUATE: `{name}` was not compared (on disk: {}, loaded: {})",
            committed.contains_key(*name),
            loaded.contains_key(*name)
        );
    }
    assert!(
        compared > 0,
        "COULD-NOT-EVALUATE: compared zero expressions"
    );
}

/// The APPLY half (ADR-0036 §11) for every story-3 rule: each vector selector
/// the committed expr names returns at least one series from a real pod.
///
/// A selector naming a series that does not exist yields an EMPTY result,
/// never an error, so a typo'd name or label value makes a rule that loads,
/// evaluates and can never fire. Each selector is checked WITH its matchers
/// (as `count(<selector>)`), so a label value such as
/// `reason="subscriber_backlog"` is checked as its own series, not just the
/// name.
///
/// - `mh_*` series are GATED: `resolve_media_handles` pre-registers every one at
///   MH startup.
/// - The MC series in [`STORY_3_EAGER_MC_SERIES`] are GATED (boot gauge / boot
///   zero-init). A selector with a NEGATED matcher (the send-directive warning
///   rule's `outcome!~...`) is checked as its bare NAME: on a healthy cluster
///   the negated match is legitimately empty, and that empty set is the
///   healthy state, not a typo.
/// - `dt_client_*` selectors are ACKNOWLEDGED-VACUOUS here, each by name, never
///   silently passed: client series exist only while something emitted within
///   `metric_expiration`. Their APPLY is carried by `client-metrics-export`
///   (names, label keys), `clientMetricNames.test.ts` (names, label values)
///   and the browser read-backs (live pipe); see [`CLIENT_SERIES_PREFIX`].
#[tokio::test]
async fn story_3_rule_selectors_apply_to_real_series() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);
    let committed = on_disk_alert_exprs();

    let mut empty = Vec::new();
    let mut gated = 0;
    for alert in STORY_3_ALERTS {
        let expr = committed
            .get(*alert)
            .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: `{alert}` is not on disk"));
        let selectors = vector_selectors(expr)
            .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: `{alert}`'s expr did not scan: {e}"));
        assert!(
            !selectors.is_empty(),
            "COULD-NOT-EVALUATE: `{alert}`'s expr yielded no selector"
        );
        for s in &selectors {
            if s.name.starts_with(CLIENT_SERIES_PREFIX) {
                eprintln!(
                    "ACKNOWLEDGED-VACUOUS: `{alert}` selector `{}` is a client series; its \
                     APPLY is carried by dt-guard client-metrics-export (names, label keys), \
                     packages/web-app/tests/clientMetricNames.test.ts (names, label values) \
                     and the browser read-backs (live pipe), not this check",
                    s.text(expr)
                );
                continue;
            }
            let eager =
                s.name.starts_with("mh_") || STORY_3_EAGER_MC_SERIES.contains(&s.name.as_str());
            assert!(
                eager,
                "COULD-NOT-EVALUATE: `{alert}` selector `{}` is neither an eager MH/MC series \
                 nor a client series; classify it here before trusting this test",
                s.text(expr)
            );
            let probe = if s.matchers.iter().any(|m| m.op.is_negated()) {
                s.name.clone()
            } else {
                s.text(expr).to_string()
            };
            let q = format!("count({probe})");
            let n = prometheus
                .query_promql(&q)
                .await
                .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: `{q}` failed: {e}"))
                .data
                .result
                .len();
            gated += 1;
            if n == 0 {
                empty.push(format!("{alert}: {probe}"));
            }
        }
    }
    assert!(
        empty.is_empty(),
        "SELECTOR-DOES-NOT-APPLY: these story-3 rule selectors match NO series on any pod, so \
         the rule loads, evaluates and can never fire. Each is eagerly registered at boot, so \
         absence is a fault: a typo'd metric or label value in the rule, a renamed series, or \
         a pod that is not scraped:\n{}",
        empty.join("\n")
    );
    assert!(gated > 0, "COULD-NOT-EVALUATE: gated zero selectors");
}
