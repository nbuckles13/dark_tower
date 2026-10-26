//! Env-tests: MC media series carry `media_kind` and `content_kind` after a
//! real join (story 3 task 2, R-29; `content_kind` 2026-10-26; ADR-0036 §11).
//!
//! # Why this is not in `32_media_metric_hygiene.rs`
//!
//! 32 is deliberately idle-safe: it never drives a join, and MC is RECORDED
//! there, not gated. This proof needs MC to actually compose a view and a send
//! directive, so it drives a two-participant join and lives here, in the same
//! numbering band.
//!
//! # Why baseline -> rise, never presence
//!
//! MC zero-initialises every `media_kind` combination at startup (so `rate()`
//! has a series before the first event), INCLUDING `media_kind="video"`. So a
//! `{media_kind="audio"}` series exists at 0 before any join, and "present
//! after a join" would pass with no join at all. The proof is a per-instance
//! RISE over a baseline taken before the join (ADR-0032 delta semantics). The
//! same holds for any later MC video read-back (MC-A): presence is vacuous.
//!
//! # The domain check rides the proven fetch
//!
//! The "no series lacks `media_kind`" / "no value outside {audio, video}"
//! checks run over the SAME fetch in which the audio rise is re-proven, so a
//! wrong selector cannot hand them an empty set. They report under their own
//! token, distinct from the did-not-rise failure.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! One fn: 2 participants x 1 audio slot.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 2

#![cfg(all(feature = "flows", feature = "observability"))]

use std::collections::HashMap;
use std::time::Duration;

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::gc_client::GcClient;
use env_tests::fixtures::metric_hygiene::{
    CONTENT_KIND_KEY, CONTENT_KIND_MAIN, CONTENT_KIND_VALUES, MEDIA_KIND_AUDIO, MEDIA_KIND_KEY,
    MEDIA_KIND_VALUES,
};
use env_tests::fixtures::metrics::{
    format_instance_map, poll_until_any_instance_above, settled_baseline, InstanceCounters,
    PrometheusClient,
};
use env_tests::fixtures::participant::{
    self, filled_senders, meeting_of, open_sessions, targets_of, two_offered_handlers,
};
use env_tests::fixtures::AuthClient;

use env_tests::fixtures::participant::AUDIO;
/// Margin over MC's settle window (one shared home).
use env_tests::fixtures::participant::VIEW_MARGIN;
/// Failure-only ceiling for a counter rise (scrape + ingestion).
const COUNTER_BUDGET: Duration = Duration::from_secs(120);

/// The MC series this task split by kind that a join is guaranteed to move.
const SLOT_STATES: &str = "mc_media_slot_states_total";
const SEND_TARGETS: &str = "mc_media_send_targets_total";

const TRIAGE_RISE: &str = "mc-media-kind-did-not-rise";
const TRIAGE_DOMAIN: &str = "mc-media-kind-label-domain";

async fn cluster() -> ClusterConnection {
    let cluster = ClusterConnection::new()
        .await
        .expect("Failed to connect to cluster - ensure port-forwards are running");
    cluster.check_ac_health().await.expect("AC must be running");
    cluster.check_gc_health().await.expect("GC must be running");
    cluster
}

fn audio_sum(name: &str) -> String {
    format!(
        "sum by (instance) ({name}{{{MEDIA_KIND_KEY}=\"{MEDIA_KIND_AUDIO}\",{CONTENT_KIND_KEY}=\"{CONTENT_KIND_MAIN}\"}})"
    )
}

#[tokio::test]
async fn test_mc_slot_states_and_send_targets_carry_media_kind_audio_after_a_join() {
    let cluster = cluster().await;
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;

    // BASELINE before the join, per instance.
    let mut baselines: HashMap<&str, InstanceCounters> = HashMap::new();
    for name in [SLOT_STATES, SEND_TARGETS] {
        let b = settled_baseline(
            &prom,
            &audio_sum(name),
            COUNTER_BUDGET,
            "the media_kind split",
        )
        .await;
        baselines.insert(name, b);
    }

    // A two-participant join: both reach both handlers, both declare one audio
    // slot, so MC composes a view (slot states) and a directive with a target
    // (send targets) for each.
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = meeting_of(&auth, &gc, "Media Kind Labels", &["K1", "K2"]).await;
    let (x, y) = two_offered_handlers("MEDIA-KIND", &ps);
    let ids: Vec<u32> = ps.iter().map(participant::Participant::sender_id).collect();
    let _sessions = open_sessions("MEDIA-KIND", &ps, &[&x, &y]).await;
    for p in &mut ps {
        p.declare(&[1], &[]).await;
    }
    let bound = window + VIEW_MARGIN;
    for (i, p) in ps.iter_mut().enumerate() {
        let other = ids[1 - i];
        p.converge("MEDIA-KIND", bound, move |v, d| {
            v.is_some_and(|v| filled_senders(v, AUDIO) == vec![other])
                && d.is_some_and(|d| !targets_of(d).is_empty())
        })
        .await;
    }

    // RISE on the audio selector, per instance.
    for name in [SLOT_STATES, SEND_TARGETS] {
        let promql = audio_sum(name);
        let baseline = &baselines[name];
        poll_until_any_instance_above(
            &prom,
            &promql,
            baseline,
            COUNTER_BUDGET,
            Duration::from_secs(5),
            |current| {
                format!(
                    "{TRIAGE_RISE}: `{promql}` rose on no MC instance after a join that composed \
                     a view and a directive (baseline {}, now {}). EMPTY means MC no longer \
                     labels {name} with {MEDIA_KIND_KEY}=\"{MEDIA_KIND_AUDIO}\",{CONTENT_KIND_KEY}=\"{CONTENT_KIND_MAIN}\"; flat means the recorder is \
                     not reached.",
                    format_instance_map(baseline),
                    format_instance_map(current)
                )
            },
        )
        .await;
    }

    // ONE fetch of every raw series of both names: re-prove the rise in it,
    // then check the label domain over exactly that set.
    let raw = prom
        .query_promql(&format!("{{__name__=~\"{SLOT_STATES}|{SEND_TARGETS}\"}}"))
        .await
        .expect("Prometheus query should succeed")
        .data
        .result;
    for name in [SLOT_STATES, SEND_TARGETS] {
        let mut now: InstanceCounters = InstanceCounters::new();
        for s in raw.iter().filter(|s| {
            s.metric.get("__name__").map(String::as_str) == Some(name)
                && s.metric.get(MEDIA_KIND_KEY).map(String::as_str) == Some(MEDIA_KIND_AUDIO)
                && s.metric.get(CONTENT_KIND_KEY).map(String::as_str) == Some(CONTENT_KIND_MAIN)
        }) {
            let instance = s.metric.get("instance").cloned().unwrap_or_default();
            let v: f64 = s
                .value
                .as_ref()
                .and_then(|(_, v)| v.parse().ok())
                .expect("sample value is a number");
            *now.entry(instance).or_insert(0.0) += v;
        }
        assert!(
            env_tests::fixtures::metrics::any_instance_exceeds_baseline(&baselines[name], &now),
            "{TRIAGE_RISE}: the domain-check fetch does not show the audio rise just proven \
             for {name} (baseline {}, fetched {}) - the raw selector is not seeing the same \
             series, so the domain check below would be vacuous.",
            format_instance_map(&baselines[name]),
            format_instance_map(&now)
        );
    }
    let bad: Vec<_> = raw
        .iter()
        .filter(|s| {
            !s.metric
                .get(MEDIA_KIND_KEY)
                .is_some_and(|k| MEDIA_KIND_VALUES.contains(&k.as_str()))
                || !s
                    .metric
                    .get(CONTENT_KIND_KEY)
                    .is_some_and(|k| CONTENT_KIND_VALUES.contains(&k.as_str()))
        })
        .map(|s| &s.metric)
        .collect();
    assert!(
        bad.is_empty(),
        "{TRIAGE_DOMAIN}: {} series of {SLOT_STATES}/{SEND_TARGETS} lack {MEDIA_KIND_KEY} / \
         {CONTENT_KIND_KEY} or carry a value outside {MEDIA_KIND_VALUES:?} / \
         {CONTENT_KIND_VALUES:?}: {bad:?}",
        bad.len()
    );
}
