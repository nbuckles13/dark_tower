// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`; the local
// Prometheus recorder below is thread-bound, so that pinning is LOAD-BEARING.
//
//! Component test (ADR-0032) for the EXPOSED LABEL SHAPE of
//! `gc_mh_replacement_requests_total` and `gc_mh_replacement_duration_seconds`,
//! through the real `McService::acquire_media_handler` recording site and a
//! real Prometheus exporter render (`MetricAssertion`'s label filter is a
//! subset match, so it cannot prove a label is ABSENT):
//!
//! - the counter's label keys are exactly {trigger, outcome, gc_view}; the
//!   histogram's exactly {outcome} (+ `le` on buckets);
//! - no handler, meeting or controller id appears anywhere in the render,
//!   while the same ids DO appear in the `gc.mh.replacement_report` event
//!   (positive control: the ids were really in play).
//!
//! Per-outcome cell deltas are in `mh_replacement_tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;

use std::collections::BTreeSet;

use gc_service::grpc::McService;
use metrics_exporter_prometheus::PrometheusBuilder;
use proto_gen::dark_tower::internal::v1::global_controller_service_server::GlobalControllerService;
use proto_gen::dark_tower::internal::v1::{AcquireMediaHandlerRequest, ReleaseCause};
use sqlx::PgPool;
use test_common::event_capture::EventCapture;
use test_common::mc_grpc::{mc_app_state, seed_assignment, seed_mc};
use test_common::mh_registry::seed;
use tonic::Request;

const IDS: [&str; 4] = ["mh-repl-a", "mh-repl-b", "mc-repl-1", "meeting-repl-1"];

fn req(meeting: &str, exclude: &[&str], replaced: &str) -> Request<AcquireMediaHandlerRequest> {
    Request::new(AcquireMediaHandlerRequest {
        meeting_id: meeting.to_string(),
        controller_id: "mc-repl-1".to_string(),
        exclude_handler_ids: exclude.iter().map(|s| (*s).to_string()).collect(),
        trigger: ReleaseCause::HeartbeatTimeout as i32,
        replaced_handler_id: Some(replaced.to_string()),
    })
}

/// The label keys of one rendered series line (`name{k="v",...} value`).
fn label_keys(line: &str) -> BTreeSet<String> {
    let Some(open) = line.find('{') else {
        return BTreeSet::new();
    };
    let close = line.rfind('}').expect("closing brace");
    line[open + 1..close]
        .split("\",")
        .filter_map(|kv| kv.split_once('=').map(|(k, _)| k.trim().to_string()))
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn replacement_series_label_shape(pool: PgPool) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);
    let capture = EventCapture::install("gc.mh.replacement_report");

    seed_mc(&pool, "mc-repl-1", "us-east-1").await;
    seed_assignment(&pool, "meeting-repl-1", "mc-repl-1", "us-east-1", false).await;
    seed(&pool, "mh-repl-a", "us-east-1", "healthy", 0).await;
    seed(&pool, "mh-repl-b", "us-east-1", "healthy", 0).await;
    let svc = McService::new(mc_app_state(pool.clone(), None));

    // granted, no_capacity, not_owner: three outcomes, ids in every field.
    svc.acquire_media_handler(req("meeting-repl-1", &["mh-repl-a"], "mh-repl-a"))
        .await
        .unwrap();
    svc.acquire_media_handler(req(
        "meeting-repl-1",
        &["mh-repl-a", "mh-repl-b"],
        "mh-repl-b",
    ))
    .await
    .unwrap_err();
    svc.acquire_media_handler(req("meeting-other", &[], "mh-repl-a"))
        .await
        .unwrap_err();

    let rendered = handle.render();
    let counter: Vec<&str> = rendered
        .lines()
        .filter(|l| l.starts_with("gc_mh_replacement_requests_total{"))
        .collect();
    assert_eq!(counter.len(), 3, "positive presence: {counter:?}");
    for line in &counter {
        assert_eq!(
            label_keys(line),
            BTreeSet::from(["trigger".into(), "outcome".into(), "gc_view".into()]),
            "{line}"
        );
    }
    let hist: Vec<&str> = rendered
        .lines()
        .filter(|l| l.starts_with("gc_mh_replacement_duration_seconds"))
        .collect();
    assert!(!hist.is_empty(), "positive presence of the histogram");
    for line in &hist {
        let keys = label_keys(line);
        // This bare builder renders a summary (`quantile`); production
        // buckets render `le`. Either is the exporter's own key, not ours.
        let mut expected: BTreeSet<String> = BTreeSet::from(["outcome".into()]);
        for exporter_key in ["le", "quantile"] {
            if keys.contains(exporter_key) {
                expected.insert(exporter_key.into());
            }
        }
        assert_eq!(keys, expected, "{line}");
    }
    for id in IDS {
        assert!(
            !rendered.contains(id),
            "id {id} must never be a label value"
        );
    }

    // Positive control: the ids WERE in play, in the event.
    let events = capture.take();
    assert_eq!(events.len(), 3);
    let logged: BTreeSet<&str> = events
        .iter()
        .flat_map(|e| [e.field("handler_id"), e.field("controller_id")])
        .flatten()
        .collect();
    assert!(logged.contains("mh-repl-a") && logged.contains("mc-repl-1"));
}
