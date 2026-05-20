// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// — the `notify_participant_*` handlers `record_mh_notification` synchronously
// on the caller's task. On `current_thread` that's the test thread and
// `MetricAssertion` captures the emission. See
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `McMediaCoordinationService` driving real
//! `mc_mh_notifications_received_total` emissions per ADR-0032 Step 3
//! §Cluster D.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ::common::observability::testing::MetricAssertion;
use mc_service::grpc::McMediaCoordinationService;
use mc_service::mh_connection_registry::MhConnectionRegistry;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantDisconnectedRequest,
};
use tonic::Request;

fn make_service() -> McMediaCoordinationService {
    McMediaCoordinationService::new(Arc::new(MhConnectionRegistry::new()))
}

// ---------------------------------------------------------------------------
// `mc_mh_notifications_received_total` — direct-call coverage
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn notify_participant_connected_records_event_connected() {
    let svc = make_service();
    let req = Request::new(NotifyParticipantConnectedRequest {
        meeting_id: "meeting-1".to_string(),
        participant_id: "part-1".to_string(),
        handler_id: "mh-1".to_string(),
    });

    let snap = MetricAssertion::snapshot();
    svc.notify_participant_connected(req).await.unwrap();

    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "connected")])
        .assert_delta(1);
    // Adjacency catches a future event-label swap.
    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "disconnected")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn notify_participant_disconnected_records_event_disconnected() {
    let svc = make_service();
    let req = Request::new(NotifyParticipantDisconnectedRequest {
        meeting_id: "meeting-1".to_string(),
        participant_id: "part-1".to_string(),
        handler_id: "mh-1".to_string(),
        reason: 0,
    });

    let snap = MetricAssertion::snapshot();
    svc.notify_participant_disconnected(req).await.unwrap();

    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "disconnected")])
        .assert_delta(1);
    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "connected")])
        .assert_delta(0);
}
