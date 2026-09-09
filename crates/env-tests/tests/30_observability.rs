//! P1 Observability Tests
//!
//! Tests for Prometheus scraping and Loki log aggregation across all deployed services.
//! Loki tests auto-skip if the log aggregation stack is not available.

#![cfg(feature = "observability")]

use env_tests::cluster::ClusterConnection;
use env_tests::eventual::{assert_eventually, ConsistencyCategory};
use env_tests::fixtures::PrometheusClient;
use std::time::{SystemTime, UNIX_EPOCH};

/// Helper to create a cluster connection for tests.
async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running")
}

/// All services that must be scraped by Prometheus and have logs in Loki.
/// If a service is deployed but missing from this list, these tests won't catch it.
/// If a service is in this list but not deployed, the test will fail — which is correct.
const EXPECTED_SERVICES: &[(&str, &str)] = &[
    ("ac-service", "ac_"),
    ("gc-service", "gc_"),
    ("mc-service", "mc_"),
    ("mh-service", "mh_"),
];

#[tokio::test]
async fn test_all_services_scraped_by_prometheus() {
    let cluster = cluster().await;
    let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);

    // Verify every expected service appears in Prometheus up{} metric.
    let jobs_regex = EXPECTED_SERVICES
        .iter()
        .map(|(job, _)| *job)
        .collect::<Vec<_>>()
        .join("|");

    let up_response = prometheus_client
        .query_promql(&format!(r#"up{{job=~"{}"}}"#, jobs_regex))
        .await
        .expect("Prometheus up{} query should succeed");

    let discovered_jobs: Vec<String> = up_response
        .data
        .result
        .iter()
        .filter_map(|r| r.metric.get("job").cloned())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    eprintln!(
        "Discovered {} services in Prometheus: {:?}",
        discovered_jobs.len(),
        discovered_jobs
    );

    // Assert ALL expected services are present, not just "at least one".
    for (job, _prefix) in EXPECTED_SERVICES {
        assert!(
            discovered_jobs.iter().any(|j| j == job),
            "Expected service '{}' not found in Prometheus up{{}} metric. \
             Discovered: {:?}. Check Prometheus scrape config.",
            job,
            discovered_jobs
        );
    }

    // For each expected service, verify at least one metric with the correct prefix exists.
    for (job, prefix) in EXPECTED_SERVICES {
        let query = format!(
            r#"count({{job="{job}",__name__=~"{prefix}.*"}})"#,
            job = job,
            prefix = prefix
        );

        assert_eventually(ConsistencyCategory::MetricsScrape, || {
            let prometheus_client = PrometheusClient::new(&cluster.prometheus_base_url);
            let query = query.clone();
            async move {
                let response = match prometheus_client.query_promql(&query).await {
                    Ok(r) => r,
                    Err(_) => return false,
                };

                if response.data.result.is_empty() {
                    return false;
                }

                response.data.result[0]
                    .value
                    .as_ref()
                    .map(|(_, v)| v.parse::<f64>().unwrap_or(0.0) > 0.0)
                    .unwrap_or(false)
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Service '{}' should have at least one metric with prefix '{}' in Prometheus",
                job, prefix
            )
        });
    }
}

#[tokio::test]
async fn test_all_services_have_logs_in_loki() {
    let cluster = cluster().await;

    // Loki must be available for this test.
    assert!(
        cluster.is_loki_available().await,
        "Loki must be available for log aggregation tests - ensure observability stack is running"
    );

    let loki_url = cluster
        .loki_base_url
        .as_ref()
        .expect("Loki URL should be set");

    // Verify every expected service has logs in Loki.
    // Uses a 29-day window to catch any logs from the cluster's lifetime while
    // staying within Loki's max_query_length limit (30d1h).
    // limit=1 keeps the query cheap regardless of the wide time window.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock before UNIX epoch");
    let end_ns = now.as_nanos();
    let twenty_nine_days_ns: u128 = 29 * 24 * 3600 * 1_000_000_000;
    let start_ns = end_ns.saturating_sub(twenty_nine_days_ns);

    let expected_apps: Vec<&str> = EXPECTED_SERVICES.iter().map(|(job, _)| *job).collect();

    eprintln!(
        "Checking Loki logs for {} services: {:?}",
        expected_apps.len(),
        expected_apps
    );

    for app in &expected_apps {
        let query = format!("{{app=\"{}\"}}", app);

        assert_eventually(ConsistencyCategory::LogAggregation, || {
            let loki_url = loki_url.clone();
            let query = query.clone();
            let http_client = cluster.http_client().clone();
            async move {
                let url = format!("{}/loki/api/v1/query_range", loki_url);

                let response = match http_client
                    .get(&url)
                    .query(&[
                        ("query", query.as_str()),
                        ("start", &start_ns.to_string()),
                        ("end", &end_ns.to_string()),
                        ("limit", "1"),
                    ])
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(_) => return false,
                };

                if !response.status().is_success() {
                    return false;
                }

                let body = match response.text().await {
                    Ok(b) => b,
                    Err(_) => return false,
                };

                let json: serde_json::Value = match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(_) => return false,
                };

                let status_ok = json.get("status").and_then(|s| s.as_str()) == Some("success");
                let has_results = json
                    .get("data")
                    .and_then(|d| d.get("result"))
                    .and_then(|r| r.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                status_ok && has_results
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Loki should have logs from service '{}'. \
                 Check Promtail pipeline for this service.",
                app
            )
        });
    }
}

// test_logs_have_trace_ids removed: was aspirational (never asserted anything).
// TODO: Re-add when OpenTelemetry trace ID propagation is implemented (ADR-0011).

// ---------------------------------------------------------------------------
// ADR-0036 §11 media-path label PRESENCE, asserted against the real scrape.
// ---------------------------------------------------------------------------
//
// # What this adds, and what it deliberately does NOT re-assert
//
// `32_media_metric_hygiene.rs` already owns the ABSENCE half — no meeting
// identifier, raw or hashed, on any metric — and owns it over all four Rust
// service jobs, which is STRICTLY STRONGER than a media-only form of the same
// rule. It is not re-asserted here. A second, weaker copy of a strong control is
// worse than no copy: it doubles the maintenance surface, and when the two
// disagree the weaker one is the likelier to be "fixed".
//
// Net-new here is the two things `check_series` structurally cannot express,
// because it is a violation detector over a denylist and has no positive-presence
// predicate at all:
//
//   (a) `key_custody=operator` is PRESENT on MH's media-path series;
//   (b) the drop-by-reason counters EXIST, with the expected label shape.
//
// # Why (b) is a legitimate gate rather than a flake
//
// `resolve_media_handles()` (`crates/mh-service/src/observability/metrics.rs`)
// resolves the FULL label cross-product through the base `counter!` macro — which
// returns the handle, and therefore registers — for every `MediaDropReason` token
// plus every codec reject reason, and `main.rs` calls it unconditionally at
// startup. So on any running MH pod with no media flowing, every
// `mh_media_frames_dropped_total{reason,direction,key_custody}` series exists AT
// ZERO. Presence is a real property of a healthy pod, not a race.
//
// # MH is gated, MC is only RECORDED — do NOT "fix" this into symmetry
//
// MC's media metrics are created lazily on first emission (`init_metrics_recorder`
// sets histogram buckets only), so on an idle cluster their absence is the
// EXPECTED state. An MC-shaped presence gate would red on every idle run and be
// muted within weeks — which is how a control dies, and is the failure mode this
// whole area exists to demonstrate against. The asymmetry is deliberate and
// matches `32_media_metric_hygiene.rs`.

/// MH's eagerly-registered anchor. Its absence means MH is not up, not scraped, or
/// its metric-registration path broke — never that the labels below are fine.
const MH_MEDIA_ANCHOR: &str = "mh_media_frames_forwarded_total";

/// MH's drop counter, whose reason cross-product is registered at startup.
const MH_DROP_METRIC: &str = "mh_media_frames_dropped_total";

/// Triage strings, deliberately DISTINCT so an empty scrape can never be triaged
/// as a policy violation and "fixed" by relaxing the predicate. Same discipline as
/// `32_media_metric_hygiene.rs`, whose tokens these deliberately mirror in shape.
const TRIAGE_MEDIA_SCRAPE: &str = "Triage MH scrape/metric-registration";
const TRIAGE_MEDIA_LABELS: &str = "Triage media label-presence violation";

/// `key_custody` and its sole value, RESTATED here on purpose rather than imported
/// from `crates/common/src/observability/labels.rs`.
///
/// **This is a boundary test against the DEPLOYED ARTIFACT, and that is the whole
/// argument.** `labels.rs`'s `key_custody_literals_are_stable` only reds on a
/// rename that leaves its pin alone. Someone doing an INTENTIONAL rename updates
/// the const and its pin in the same commit — which is exactly what a normal
/// refactor looks like — and at that moment an *importing* env-test renames itself
/// too and stays green, while a wire-visible break in a label that every media
/// dashboard and the `MCMediaMissingKeyMaterial` chain select on ships with a fully
/// green suite. These restated literals are the only artifact in the tree that reds
/// there. Precedent: `crates/proto-gen/tests/internal_roundtrip.rs` names `65_535`
/// rather than referencing the constant, for the same reason.
///
/// NOT justified by "env-tests links no service crates" — that would be false as a
/// reason (`common` is a shared library, not a service crate, and `media-protocol`
/// and `proto-gen` are already dependencies on exactly that ground). Do not
/// "simplify" this into an import on the strength of refuting a limb this comment
/// does not rest on.
const KEY_CUSTODY_LABEL: &str = "key_custody";
const KEY_CUSTODY_OPERATOR: &str = "operator";

/// Bounded `direction` values. One per token by construction — a token that could
/// occur in both directions would be two conditions wearing one name.
const EXPECTED_DIRECTIONS: &[&str] = &["ingress", "egress"];

/// MH-local `MediaDropReason` tokens, NAMED rather than counted.
///
/// Restated deliberately, same argument as the `key_custody` literals above: this
/// asserts what a deployed pod exposes, and a roster imported from the emitter
/// would agree with the emitter no matter what the emitter became.
///
/// **Deliberately no integer.** The assertion is `missing.is_empty()`, never
/// `assert_eq!(found.len(), N)`: the codec reject family legitimately adds more
/// tokens to this same metric, so an equality check reds on a lawful addition while
/// telling the reader nothing about which token moved. `MediaDropReason::ALL`'s
/// written-out length is compile-checked at its home and is the guard; a count
/// restated here would be an unchecked copy — the exact defect that left
/// "11 MH-local series rather than 22" stale in `metrics.rs` through a token growth.
const MH_LOCAL_DROP_REASONS: &[&str] = &[
    "ingress_queue_overflow",
    "egress_queue_overflow",
    "transport_send_refused",
    "connection_closed",
    "oversize_datagram",
    "stream_rate_limited",
    "no_policy",
    "no_subscriber",
    "no_local_subscriber",
    "relay_rewrite_failed",
    "partial_frame_discard",
    "transport_receive_dropped",
    "no_media_session",
];

/// Media-path label hygiene, PRESENCE half.
///
/// **One fetch feeds every assertion below, and that is structural.** The anchor
/// and the for-all predicates run over the same `Vec`, so the anchor *guarantees*
/// the predicates had input — with separate queries, a wrong selector would leave
/// the anchor passing while the predicates inspected nothing and reported clean.
/// That argument is made here rather than on
/// `metric_hygiene::fetch_all_series`, which returns a `Vec` and cannot enforce it
/// for any caller.
#[tokio::test]
async fn media_path_metrics_carry_key_custody_and_every_drop_reason_is_registered() {
    let cluster = cluster().await;
    let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);

    // Scrape lag is infrastructure convergence, not test logic: polled through the
    // established category rather than a hand-rolled sleep.
    assert_eventually(ConsistencyCategory::MetricsScrape, || {
        let prometheus = PrometheusClient::new(&cluster.prometheus_base_url);
        async move {
            prometheus
                .query_promql(&format!("count({MH_MEDIA_ANCHOR})"))
                .await
                .ok()
                .filter(|r| !r.data.result.is_empty())
                .is_some()
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{TRIAGE_MEDIA_SCRAPE}: `{MH_MEDIA_ANCHOR}` is absent from Prometheus. \
             MH resolves the full media metric label cross-product unconditionally at \
             startup, so those series exist at zero on any running MH pod with no media \
             flowing. Absence means MH is not up, not scraped, or its registration path \
             is broken. DO NOT satisfy this check by deleting it: without the anchor \
             every assertion below passes vacuously."
        )
    });

    let media_jobs = EXPECTED_SERVICES
        .iter()
        .filter(|(job, _)| *job == "mc-service" || *job == "mh-service")
        .map(|(job, _)| *job)
        .collect::<Vec<_>>()
        .join("|");

    let series = env_tests::fixtures::metric_hygiene::fetch_all_series(
        &prometheus,
        &format!(r#"{{job=~"{media_jobs}",__name__=~"m[hc]_media_.*"}}"#),
        TRIAGE_MEDIA_SCRAPE,
    )
    .await;

    // Non-vacuity, over the SAME set every predicate below will see, and under a
    // triage token distinct from the policy-violation one.
    assert!(
        !series.is_empty(),
        "{TRIAGE_MEDIA_SCRAPE}: the scrape returned zero media series for jobs \
         `{media_jobs}`. Every assertion below would pass having inspected nothing."
    );
    let anchor_series: Vec<_> = series
        .iter()
        .filter(|s| s.name == MH_MEDIA_ANCHOR)
        .collect();
    assert!(
        !anchor_series.is_empty(),
        "{TRIAGE_MEDIA_SCRAPE}: `{MH_MEDIA_ANCHOR}` was not in the fetched set even \
         though the anchor query found it — the selector does not cover the MH job, so \
         this test is inspecting the wrong surface."
    );

    // (a) key_custody=operator on every media series the scrape actually returned —
    // MH AND MC. This is NOT the MH-gated/MC-recorded asymmetry: that asymmetry is
    // about PRESENCE (never gate on a lazily-created metric existing), and it is
    // honoured — this predicate ranges over whatever the scrape returned, so on an
    // idle cluster the MC set is empty and the assertion is a no-op, flake-free by
    // construction. "Do not require MC series to exist" and "do not check the ones
    // that DO exist" are different claims, and only the first follows from lazy
    // creation. It matters most on MC: MC HOLDS the KEK (ADR-0036 §4), so its media
    // series are the ones an operator is likeliest to read as a custody statement,
    // and `mc-service/src/observability/metrics.rs` already declares the label
    // mandatory on every `mc_media_*` series — this keeps that claim from silently
    // lapsing. Do NOT re-add a `mh_media_`-only filter here; the "MC is recorded, not
    // gated" note below is about presence, not this check.
    //
    // The selector already restricted the fetch to media names, so `series` IS the
    // media set. Positive control: the MH anchor itself carries this label, so a
    // fleet-wide drop puts it in the FAILING set rather than making it absent — the
    // assertion cannot pass by inspecting nothing.
    let custody_violations: Vec<String> = series
        .iter()
        .filter(|s| {
            s.labels.get(KEY_CUSTODY_LABEL).map(String::as_str) != Some(KEY_CUSTODY_OPERATOR)
        })
        .map(|s| format!("  - {} :: {:?}", s.name, s.labels))
        .collect();
    assert!(
        custody_violations.is_empty(),
        "{TRIAGE_MEDIA_LABELS}: {} media series (MH or MC) lack \
         `{KEY_CUSTODY_LABEL}={KEY_CUSTODY_OPERATOR}` (ADR-0036 §4):\n{}\n\n\
         Fix the METRIC, not the assertion. ADR-0036 §4 makes operator custody a fact \
         about this deployment, and the label is how every media dashboard and runbook \
         states it. A media series without it is either a new emission site that skipped \
         the constant, or a rename that this test exists to catch.",
        custody_violations.len(),
        custody_violations.join("\n")
    );

    // (b) Every MH-local drop reason is registered, with a bounded direction and
    // the custody label. Contains-all, never set equality: the codec reject family
    // lawfully adds tokens to this same metric.
    let drop_series: Vec<_> = series.iter().filter(|s| s.name == MH_DROP_METRIC).collect();
    assert!(
        !drop_series.is_empty(),
        "{TRIAGE_MEDIA_SCRAPE}: no `{MH_DROP_METRIC}` series at all, though the anchor \
         is present. Both are registered by the same `resolve_media_handles()` call, so \
         one without the other means registration is partial — investigate MH, not this \
         test."
    );
    let observed: std::collections::BTreeSet<&str> = drop_series
        .iter()
        .filter_map(|s| s.labels.get("reason").map(String::as_str))
        .collect();
    let missing: Vec<&str> = MH_LOCAL_DROP_REASONS
        .iter()
        .copied()
        .filter(|r| !observed.contains(r))
        .collect();
    assert!(
        missing.is_empty(),
        "{TRIAGE_MEDIA_LABELS}: `{MH_DROP_METRIC}` is missing series for these \
         MediaDropReason token(s): {missing:?}.\n\n\
         These are registered eagerly at MH startup, so absence is a REGISTRATION fault, \
         not an absence of drops — a healthy idle handler exposes all of them at zero. \
         If a token was legitimately renamed or removed, update this roster in the same \
         commit; it is a deliberate restatement, not an accidental copy."
    );
    let shape_violations: Vec<String> = drop_series
        .iter()
        .filter(|s| {
            let dir_ok = s
                .labels
                .get("direction")
                .is_some_and(|d| EXPECTED_DIRECTIONS.contains(&d.as_str()));
            let custody_ok =
                s.labels.get(KEY_CUSTODY_LABEL).map(String::as_str) == Some(KEY_CUSTODY_OPERATOR);
            !(dir_ok && custody_ok)
        })
        .map(|s| format!("  - {:?}", s.labels))
        .collect();
    assert!(
        shape_violations.is_empty(),
        "{TRIAGE_MEDIA_LABELS}: {} `{MH_DROP_METRIC}` series have an unexpected label \
         shape — every one must carry a `direction` in {EXPECTED_DIRECTIONS:?} and \
         `{KEY_CUSTODY_LABEL}={KEY_CUSTODY_OPERATOR}`:\n{}",
        shape_violations.len(),
        shape_violations.join("\n")
    );

    // MC is RECORDED, never gated. Lazily created metrics are legitimately absent
    // on an idle cluster; a gate here would red every idle run and get muted.
    let mc_media_present = series.iter().any(|s| s.name.starts_with("mc_media_"));
    eprintln!(
        "media-label-presence: inspected {} media series across jobs [{media_jobs}]; \
         MH drop reasons observed: {}; mc_media_* present: {mc_media_present}",
        series.len(),
        observed.len()
    );
}
