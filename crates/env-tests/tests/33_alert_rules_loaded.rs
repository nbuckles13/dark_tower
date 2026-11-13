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
//! - `story_3_rule_selectors_apply_to_real_series` /
//!   `story_4_rule_selectors_apply_to_real_series` — the APPLY half (ADR-0036
//!   §11) for the story-3 and story-4 rules: every vector selector each rule names returns
//!   at least one series from a real pod, or is reported ACKNOWLEDGED-VACUOUS
//!   by name when it is a client series, whose APPLY is carried by
//!   `dt-guard client-metrics-export` (names, label keys),
//!   `packages/web-app/tests/clientMetricNames.test.ts` (names, label values)
//!   and the browser read-backs (the live pipe).
//!
//! Story 4 adds two more checks:
//!
//! - `for_literals_match_their_config_derivation` — every `for:` that stands
//!   in for a config gauge (`ANCHOR (DRY):` on the `for:` line) matches its
//!   derivation from the LIVE gauges, source consts and Prometheus's loaded
//!   timing, and the anchored set is closed.
//! - The STATIC halves (the closed anchored set, the derivation source consts,
//!   no page rule on per-pod MH liveness) are Layer-4 unit tests in
//!   `fixtures/alert_rules_loaded.rs`, so they fail without a cluster.
//!
//! **R-35 ("a single MH death never pages by itself") is a property of the
//! rule DESIGN, not something this file fires.** `MHDown` reads GC's fleet
//! gauge, not per-pod `up`; the only page a death can reach is
//! `MCMediaMeetingsAwaitingHandler`, past the replacement budget; and the
//! own-path warning needs >= 2 handlers. No env-test asserts an alert fires —
//! or stays quiet — after a crash leg: either would depend on timing against
//! `for:` windows and flake.
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
use env_tests::eventual::{poll_within, ConsistencyCategory};
use env_tests::fixtures::alert_rules_loaded::{
    apply_probes, ceil_to_minute, compare_loaded_expr, for_verdict, formatted_query,
    loaded_alert_names, loaded_alert_queries, loaded_rule_timings, on_disk_rules, range_windows,
    vector_selectors, ExprCheck, ForVerdict, OnDiskRules, ANCHORED_FOR_RULES, MH_ADMISSION_SRC,
    MH_GC_CLIENT_SRC, RULES_DIR,
};
use env_tests::fixtures::gc_fleet::{
    const_duration_secs, gc_config_gauge_agreed, gc_health_tick, live_gc_pods, GcPods,
    GC_HEALTH_TICK_SRC, LOAD_REPORT_INTERVAL_PER_REPLICA_PROMQL, STALENESS_PER_REPLICA_PROMQL,
};
use env_tests::fixtures::metric_hygiene::{
    KUBE_STATE_SERIES, STORY_3_EAGER_MC_SERIES, STORY_4_EAGER_GC_SERIES, STORY_4_EAGER_MC_SERIES,
};
use env_tests::fixtures::metrics::{
    max_service_job_scrape_interval, mc_config_gauge_agreed, mh_owner_silence_timeout,
    parse_prometheus_duration, MH_OWNER_SILENCE_TIMEOUT_GAUGE,
};
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

/// The story 4 (OBS-ALERTS) rules: every NEW or CHANGED rule — the four pages
/// (including the re-scoped `MHDown` and the kept `MCMediaGenerationDivergence`),
/// every new warning, and the reworded `MHMCNotificationFailures` /
/// `MHPodRestartingFrequently`. Positive control for the name, expr and APPLY
/// checks.
const STORY_4_ALERTS: &[&str] = &[
    // pages
    "MCMediaMeetingsAwaitingHandler",
    "MCMediaGenerationDivergence",
    "MHDown",
    "GCMhRegistrationPingPong",
    // mc-alerts.yaml warnings
    "MCMediaHandlerMigrationFailures",
    "MCMediaHandlerDrainShed",
    "MCMediaHandlerOwnPathSignature",
    "MCMediaHandlerCrashRate",
    "MCMediaHandlerClientLossDisagreement",
    "MCMediaHandlerReplacementRateLimited",
    "MCMediaHandlerConnectedSetCorrections",
    "MCMediaHandlerHeartbeatMisroute",
    "MCMediaHandlerRemovalReleaseRejected",
    "MCMediaHandlerHeartbeatAnomalies",
    "MCRegisterMeetingFailureRate",
    // mh-alerts.yaml
    "MHMCNotificationFailures",
    "MHPodRestartingFrequently",
    "MHFalseDeclarations",
    "MHOwnerSilenceReleaseWithClients",
    "MHMediaDialTokenExpired",
    "MHMediaDialNoMeetingKey",
    "MHMediaDialWrongTokenType",
    "MHMediaDialWrongHandler",
    "MHMeetingsHeldWithoutClients",
    // gc-alerts.yaml warnings
    "GCMhUnknownHandlerLoadReports",
    "GCMhAddressConflictPersisting",
    "GCMhRegistrationEndpointRejected",
    "GCMhReplacementNoCapacity",
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
/// Derived from the fixture's ONE on-disk walk ([`on_disk_rules`], over the
/// `configMapGenerator` list — the artifact that decides which files are in the
/// pod this test queries; see the module rustdoc). COULD-NOT-EVALUATE on any
/// read or parse problem, including an EMPTY mounted set (vacuous, never clean).
fn on_disk_alert_exprs() -> std::collections::BTreeMap<String, String> {
    on_disk()
        .rules
        .into_iter()
        .map(|(file, r)| {
            let expr = r.expr.unwrap_or_else(|| {
                panic!(
                    "COULD-NOT-EVALUATE: {file}: alert `{}` has no `expr`",
                    r.alert
                )
            });
            (r.alert, expr)
        })
        .collect()
}

/// [`on_disk_rules`], panicking COULD-NOT-EVALUATE with the fixture's reason.
fn on_disk() -> OnDiskRules {
    on_disk_rules().unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {e}"))
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
    for name in STORY_2_ALERTS
        .iter()
        .chain(STORY_3_ALERTS)
        .chain(STORY_4_ALERTS)
    {
        // On disk FIRST, with its own token: a typo in the const is not a
        // RULE-NOT-LOADED.
        assert!(
            expected.contains(*name),
            "COULD-NOT-EVALUATE: `{name}` is named by this test but is not on disk; fix the \
             const (or the rule was renamed)"
        );
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
    // Positive control: the comparison actually ran over these tasks' rules.
    for name in STORY_3_ALERTS.iter().chain(STORY_4_ALERTS) {
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

/// How the APPLY check treats one selector's series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeriesClass {
    /// Present on a real pod from boot (zero-initialised / boot gauge): an
    /// instant `count()` must be non-empty.
    Eager,
    /// Present after the publisher's first tick, not at boot
    /// (`gc_mh_available`): a FAILURE-ONLY poll bounded by that tick plus the
    /// scrape ceiling, so a freshly rolled replica is not a false red.
    AfterFirstTick,
    /// A client series: ACKNOWLEDGED-VACUOUS by name (see
    /// [`CLIENT_SERIES_PREFIX`]).
    Client,
}

/// The APPLY classifier: ONE home for which series a rule may select and how
/// each is checked. A selector outside every list is a GAP (COULD-NOT-EVALUATE
/// naming it), never a vacuous pass.
fn classify(name: &str) -> Option<SeriesClass> {
    if name.starts_with(CLIENT_SERIES_PREFIX) {
        return Some(SeriesClass::Client);
    }
    if name == "gc_mh_available" {
        return Some(SeriesClass::AfterFirstTick);
    }
    let eager = name.starts_with("mh_")
        || STORY_3_EAGER_MC_SERIES.contains(&name)
        || STORY_4_EAGER_MC_SERIES.contains(&name)
        || STORY_4_EAGER_GC_SERIES.contains(&name)
        || KUBE_STATE_SERIES.contains(&name);
    eager.then_some(SeriesClass::Eager)
}

/// The FAILURE-ONLY bound for a [`SeriesClass::AfterFirstTick`] series: GC's
/// health tick (read from its source) + the scrape ceiling. No new constant.
fn after_first_tick_bound() -> std::time::Duration {
    let path = repo_root().join(GC_HEALTH_TICK_SRC);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: read {}: {e}", path.display()));
    gc_health_tick(&src).unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {e}"))
        + ConsistencyCategory::MetricsScrape.timeout()
}

/// Is `count(<probe>)` non-empty right now?
async fn probe_present(prometheus: &PrometheusClient, q: &str) -> bool {
    !prometheus
        .query_promql(q)
        .await
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: `{q}` failed: {e}"))
        .data
        .result
        .is_empty()
}

/// The APPLY half (ADR-0036 §11) for one rule set: each vector selector the
/// committed expr names returns at least one series from a real pod.
///
/// A selector naming a series that does not exist yields an EMPTY result,
/// never an error, so a typo'd name or label value makes a rule that loads,
/// evaluates and can never fire. Each selector is probed WITH its positive
/// matchers, and a PURE literal alternation (`outcome=~"a|b|c"`) once PER ARM
/// ([`apply_probes`]): a `count()` over the whole regex stays green when one
/// arm is misspelled. Negated matchers are dropped (a negated set is
/// legitimately empty on a healthy cluster), never the positive ones beside
/// them.
async fn assert_rule_selectors_apply(prometheus: &PrometheusClient, story: &str, alerts: &[&str]) {
    let committed = on_disk_alert_exprs();
    // Computed once (it reads GC's source), used only by after-first-tick probes.
    let tick_bound = after_first_tick_bound();
    let mut empty = Vec::new();
    let mut gated = 0;
    for alert in alerts {
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
            let class = classify(&s.name).unwrap_or_else(|| {
                panic!(
                    "COULD-NOT-EVALUATE: `{alert}` selector `{}` is in no APPLY class (eager \
                     service series, after-first-tick, kube-state, client). Classify it in \
                     metric_hygiene.rs before trusting this test: a gap is never a vacuous pass",
                    s.text(expr)
                )
            });
            if class == SeriesClass::Client {
                eprintln!(
                    "ACKNOWLEDGED-VACUOUS: `{alert}` selector `{}` is a client series; its \
                     APPLY is carried by dt-guard client-metrics-export (names, label keys), \
                     packages/web-app/tests/clientMetricNames.test.ts (names, label values) \
                     and the browser read-backs (live pipe), not this check",
                    s.text(expr)
                );
                continue;
            }
            for probe in apply_probes(s, expr) {
                let q = format!("count({probe})");
                let present = match class {
                    SeriesClass::AfterFirstTick => poll_within(tick_bound, || {
                        let q = q.clone();
                        async move {
                            if probe_present(prometheus, &q).await {
                                Ok(())
                            } else {
                                Err(format!(
                                    "`{q}` absent after {tick_bound:?} (GC health tick + scrape ceiling)"
                                ))
                            }
                        }
                    })
                    .await
                    .map_err(|e| eprintln!("{e}"))
                    .is_ok(),
                    _ => probe_present(prometheus, &q).await,
                };
                gated += 1;
                if !present {
                    empty.push(format!("{alert}: {probe}"));
                }
            }
        }
    }
    assert!(
        empty.is_empty(),
        "SELECTOR-DOES-NOT-APPLY: these {story} rule selectors match NO series on any pod, so \
         the rule loads, evaluates and can never fire. Each is registered at boot (or after \
         the publisher's first tick, polled to that bound), so absence is a fault: a typo'd \
         metric or label value in the rule, a renamed series, or a pod that is not scraped:\n{}",
        empty.join("\n")
    );
    assert!(gated > 0, "COULD-NOT-EVALUATE: gated zero selectors");
}

/// APPLY for the story-3 rules ([`assert_rule_selectors_apply`]).
#[tokio::test]
async fn story_3_rule_selectors_apply_to_real_series() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);
    assert_rule_selectors_apply(&prometheus, "story-3", STORY_3_ALERTS).await;
}

/// APPLY for the story-4 rules ([`assert_rule_selectors_apply`]). Every story-4
/// series is classified eager (or after-first-tick for `gc_mh_available`);
/// none is client, so nothing here is acknowledged-vacuous. Covers
/// test-gates.md's "no survivors -> awaiting-handler state, counted and
/// alerting": `mc_media_meetings_without_handlers` is a boot gauge, gated here.
#[tokio::test]
async fn story_4_rule_selectors_apply_to_real_series() {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);
    assert_rule_selectors_apply(&prometheus, "story-4", STORY_4_ALERTS).await;
}

// =============================================================================
// Story 4: config-derived `for:` literals (the live half; the static half —
// the closed anchored set, the source consts, the page posture — is Layer 4 in
// `fixtures/alert_rules_loaded.rs`)
// =============================================================================

/// `const <name>: Duration = Duration::from_secs(n);` read from `src`
/// (repo-relative), SOURCE-CONST-MISSING on any other shape.
fn source_const(src: &str, name: &str) -> std::time::Duration {
    let path = repo_root().join(src);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: read {}: {e}", path.display()));
    const_duration_secs(&text, name).unwrap_or_else(|e| panic!("{e} (in {src})"))
}

/// The derivation bound `GCMhRegistrationPingPong`'s ADR-0031 block states:
/// the ping-pong rate stays at least twice the `superseded >= 20` floor only
/// while the load-report interval is at most this.
///
/// THE RULE IS THE HOME (gc-alerts.yaml, the PingPong block's "VALIDITY RANGE");
/// this is the check's copy. Changing one without the other is MEANT to go red.
const PINGPONG_MAX_LOAD_REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// The live inputs every derivation shares.
struct DerivationCtx<'a> {
    prometheus: &'a PrometheusClient,
    /// The live service-job scrape interval (`/api/v1/status/config`).
    scrape: std::time::Duration,
    /// The LOADED group's evaluation interval for the rule being derived.
    eval: std::time::Duration,
    gc: &'a GcPods,
}

/// The single range window of `expr`, COULD-NOT-EVALUATE otherwise.
fn single_window(name: &str, expr: &str) -> std::time::Duration {
    let windows = range_windows(expr)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: `{name}` expr windows: {e}"));
    let [w] = windows.as_slice() else {
        panic!(
            "COULD-NOT-EVALUATE: `{name}` must have exactly one range window, found {windows:?}"
        );
    };
    *w
}

/// The derivation terms of one anchored `for:`.
///
/// THIS LIST MIRRORS THE RULE'S ADR-0031 BLOCK one-to-one (same terms, same
/// order) — diff them. Every term comes from its source: live config gauges
/// (every pod/replica must AGREE, never a max), service source consts, the
/// expr's own window, the live scrape interval, and the LOADED group interval.
/// A rule in [`ANCHORED_FOR_RULES`] with no row here is COULD-NOT-EVALUATE.
async fn derivation_terms(
    name: &str,
    expr: &str,
    ctx: &DerivationCtx<'_>,
) -> Vec<(&'static str, std::time::Duration)> {
    match name {
        "MCMediaMeetingsAwaitingHandler" => vec![
            (
                "mc_media_handler_replacement_retry_budget_seconds",
                mc_config_gauge_agreed(
                    ctx.prometheus,
                    "mc_media_handler_replacement_retry_budget_seconds",
                )
                .await,
            ),
            (
                "bridge window (expr max_over_time range)",
                single_window(name, expr),
            ),
            ("service-job scrape_interval", ctx.scrape),
            ("group evaluation interval", ctx.eval),
        ],
        "GCMhAddressConflictPersisting" => {
            let backoff_max = source_const(MH_GC_CLIENT_SRC, "BACKOFF_MAX");
            let w = single_window(name, expr);
            // The derivation's stated precondition, with its own message.
            assert!(
                w >= backoff_max,
                "DERIVATION-OUT-OF-RANGE: `{name}`'s expr window {w:?} is shorter than \
                 BACKOFF_MAX {backoff_max:?} ({MH_GC_CLIENT_SRC}): `increase([w]) > 0` drops \
                 to false between a conflicting MH's retries and resets `for:`"
            );
            let b = gc_config_gauge_agreed(ctx.prometheus, STALENESS_PER_REPLICA_PROMQL, ctx.gc)
                .await
                .unwrap_or_else(|e| panic!("PRECONDITION: {e}"));
            vec![
                ("gc_mh_staleness_threshold_seconds", b),
                ("BACKOFF_MAX (mh-service grpc/gc_client.rs)", backoff_max),
                ("expr window", w),
                ("service-job scrape_interval", ctx.scrape),
                ("group evaluation interval", ctx.eval),
            ]
        }
        "MHMeetingsHeldWithoutClients" => vec![
            (
                MH_OWNER_SILENCE_TIMEOUT_GAUGE,
                mh_owner_silence_timeout(ctx.prometheus).await,
            ),
            (
                "ADMISSION_WINDOW_BUCKET (mh-service session/admission.rs)",
                source_const(MH_ADMISSION_SRC, "ADMISSION_WINDOW_BUCKET"),
            ),
            ("service-job scrape_interval", ctx.scrape),
            ("group evaluation interval", ctx.eval),
        ],
        other => panic!("COULD-NOT-EVALUATE: no derivation row for anchored rule `{other}`"),
    }
}

/// Every anchored `for:` matches its derivation, read from the LOADED rules.
///
/// ONE convention (story 4 OBS-ALERTS, Gate 1): `for = ceil_to_minute(sum of
/// the sourced terms)` ([`derivation_terms`]), so a correct `for:` sits in
/// `[derived, derived + 60 s)`. The anchored set is closed by a Layer-4 unit
/// test over the committed files (`committed_anchored_for_set_is_closed`).
///
/// Tokens: STALE-DEPLOY (the loaded `for:` is not the committed one: redeploy),
/// FOR-DRIFT (off its derivation, split into the UNSAFE too-short and the stale
/// too-long direction), DERIVATION-OUT-OF-RANGE (a precondition a derivation
/// states no longer holds), COULD-NOT-EVALUATE.
#[tokio::test]
async fn for_literals_match_their_config_derivation() {
    use std::time::Duration;
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);

    let rules = on_disk().rules;
    let body = prometheus
        .rules()
        .await
        .expect("COULD-NOT-EVALUATE: GET /api/v1/rules failed");
    let loaded = loaded_rule_timings(&body)
        .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: /api/v1/rules is not a rules envelope"));
    let config = prometheus
        .status_config()
        .await
        .expect("COULD-NOT-EVALUATE: GET /api/v1/status/config failed");
    let scrape = max_service_job_scrape_interval(&config)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: live service-job scrape interval: {e:?}"));
    let gc = live_gc_pods().await;

    let mut failures = Vec::new();
    for name in ANCHORED_FOR_RULES {
        // Load -> stale-deploy.
        let committed = &rules
            .iter()
            .find(|(_, r)| r.alert == *name)
            .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: `{name}` is not on disk"))
            .1;
        let expr = committed
            .expr
            .as_deref()
            .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: `{name}` has no expr"));
        let committed_for = committed
            .for_
            .as_deref()
            .and_then(parse_prometheus_duration)
            .unwrap_or_else(|| panic!("COULD-NOT-EVALUATE: `{name}` has no parseable `for:`"));
        let timing = loaded.get(*name).unwrap_or_else(|| {
            panic!(
                "COULD-NOT-EVALUATE: `{name}` is not loaded (RULE-NOT-LOADED is the name test's)"
            )
        });
        let loaded_for =
            Duration::from_secs_f64(timing.for_seconds.unwrap_or_else(|| {
                panic!("COULD-NOT-EVALUATE: loaded `{name}` has no `duration`")
            }));
        let eval = Duration::from_secs_f64(timing.group_interval_seconds.unwrap_or_else(|| {
            panic!("COULD-NOT-EVALUATE: `{name}`'s loaded group has no `interval`")
        }));
        assert_eq!(
            loaded_for, committed_for,
            "STALE-DEPLOY: `{name}` is loaded with for: {loaded_for:?} but committed with \
             {committed_for:?}; the rules ConfigMap is stale (redeploy)"
        );

        // Derive -> verdict.
        let ctx = DerivationCtx {
            prometheus: &prometheus,
            scrape,
            eval,
            gc: &gc,
        };
        let terms = derivation_terms(name, expr, &ctx).await;
        let derived: Duration = terms.iter().map(|(_, d)| *d).sum();
        let shown = terms
            .iter()
            .map(|(t, d)| format!("{t}={d:?}"))
            .collect::<Vec<_>>()
            .join(" + ");
        match for_verdict(loaded_for, derived) {
            ForVerdict::Ok => {}
            ForVerdict::TooShort => failures.push(format!(
                "FOR-DRIFT (too short, UNSAFE): `{name}` for: {loaded_for:?} < derived {derived:?} \
                 ({shown}). The alert fires before the mechanism it waits on could repair the \
                 condition; re-derive to {:?}",
                ceil_to_minute(derived)
            )),
            ForVerdict::TooLong => failures.push(format!(
                "FOR-DRIFT (stale upward): `{name}` for: {loaded_for:?} >= derived {derived:?} + \
                 60 s ({shown}); re-derive to {:?}",
                ceil_to_minute(derived)
            )),
        }
    }

    // GCMhRegistrationPingPong's derivation holds only within this interval
    // (its ADR-0031 block, "VALIDITY RANGE"): every live replica must be inside.
    let interval =
        gc_config_gauge_agreed(&prometheus, LOAD_REPORT_INTERVAL_PER_REPLICA_PROMQL, &gc)
            .await
            .unwrap_or_else(|e| panic!("PRECONDITION: {e}"));
    if interval > PINGPONG_MAX_LOAD_REPORT_INTERVAL {
        failures.push(format!(
            "DERIVATION-OUT-OF-RANGE: gc_mh_load_report_interval_seconds is {interval:?}, above \
             the {PINGPONG_MAX_LOAD_REPORT_INTERVAL:?} GCMhRegistrationPingPong's `superseded >= \
             20` floor was derived for; re-derive the page before running at this interval"
        ));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
