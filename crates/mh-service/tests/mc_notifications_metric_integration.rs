//! Integration coverage for `mh_mc_notifications_total{event_type, status}`
//! (R-16/R-17) via [`record_mc_notification`].
//!
//! Production call sites at `crates/mh-service/src/grpc/mc_client.rs:184,197,210`
//! drive `event_type` ∈ {`connected`, `disconnected`} and `status` ∈ {`success`,
//! `error`} (cardinality 4). The Wave-2 dt-guard `validate-metric-coverage`
//! port surfaced this counter as previously-uncovered — the bash predecessor's
//! single-line regex missed the multi-line `counter!(...)` emission shape.
//!
//! Direct-wrapper-driven (matches the existing `record_*` test pattern in
//! `token_refresh_integration.rs`). Going through the real gRPC retry seam
//! adds zero coverage value here — the wrapper has no policy between call
//! and emission.

use common::observability::testing::MetricAssertion;
use mh_service::observability::metrics::record_mc_notification;

#[test]
fn record_mc_notification_connected_success_emits_counter() {
    let snap = MetricAssertion::snapshot();
    record_mc_notification("connected", "success");
    snap.counter("mh_mc_notifications_total")
        .with_labels(&[("event_type", "connected"), ("status", "success")])
        .assert_delta(1);
}

#[test]
fn record_mc_notification_disconnected_error_emits_counter() {
    let snap = MetricAssertion::snapshot();
    record_mc_notification("disconnected", "error");
    snap.counter("mh_mc_notifications_total")
        .with_labels(&[("event_type", "disconnected"), ("status", "error")])
        .assert_delta(1);
    // Adjacency: success counter for the same event_type NOT incremented
    // (label-swap-bug catcher).
    snap.counter("mh_mc_notifications_total")
        .with_labels(&[("event_type", "disconnected"), ("status", "success")])
        .assert_delta(0);
}

/// Matrix over the 4 bounded (event_type, status) combinations documented at
/// `metrics.rs:212`. Verifies each combination yields a distinct series.
#[test]
fn record_mc_notification_distinguishes_all_four_combinations() {
    let snap = MetricAssertion::snapshot();
    for event_type in ["connected", "disconnected"] {
        for status in ["success", "error"] {
            record_mc_notification(event_type, status);
        }
    }
    for event_type in ["connected", "disconnected"] {
        for status in ["success", "error"] {
            snap.counter("mh_mc_notifications_total")
                .with_labels(&[("event_type", event_type), ("status", status)])
                .assert_delta(1);
        }
    }
}
