//! Integration coverage for the global error counter and gRPC request counter
//! in `mh_service::observability::metrics`:
//!
//! - `mh_errors_total{operation, error_type, status_code}` via [`record_error`]
//! - `mh_grpc_requests_total{method, status}` via [`record_grpc_request`]
//!
//! Both wrappers are direct, label-stable functions (no surrounding policy or
//! retry logic) so the component tests drive them directly — mirrors the
//! `token_refresh_integration.rs` direct-wrapper pattern. The Wave-2 dt-guard
//! `validate-metric-coverage` port surfaced these as previously-uncovered (the
//! bash predecessor's single-line regex missed the multi-line `counter!(...)`
//! emission shape; the Rust port matches across lines).
//!
//! Bounded label values per `observability/metrics.rs` docstrings:
//!
//! - `mh_grpc_requests_total`: `method` ∈ {register, register_meeting,
//!   route_media, stream_telemetry}, `status` ∈ {success, error}.
//! - `mh_errors_total`: `operation` is a stable identifier from the call site
//!   (e.g. `register_meeting`, `mc_notify`), `error_type` is from
//!   `MhError`-variant naming, `status_code` is the HTTP/gRPC status int.

use common::observability::testing::MetricAssertion;
use mh_service::observability::metrics::{record_error, record_grpc_request};

// ---------------------------------------------------------------------------
// mh_grpc_requests_total
// ---------------------------------------------------------------------------

#[test]
fn record_grpc_request_emits_grpc_requests_counter() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request("register_meeting", "success");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(1);
}

#[test]
fn record_grpc_request_emits_separate_series_per_status() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request("route_media", "error");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "route_media"), ("status", "error")])
        .assert_delta(1);
    // Adjacency: success counter for the same method NOT incremented.
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "route_media"), ("status", "success")])
        .assert_delta(0);
}

/// Matrix over the 4 bounded methods × 2 bounded statuses documented at
/// `metrics.rs:134`. Verifies the per-method × per-status series are
/// independent (label-swap-bug catcher). Mirrors
/// `mc_notifications_metric_integration.rs::record_mc_notification_distinguishes_all_four_combinations`.
#[test]
fn record_grpc_request_distinguishes_all_combinations() {
    let snap = MetricAssertion::snapshot();
    for method in [
        "register",
        "register_meeting",
        "route_media",
        "stream_telemetry",
    ] {
        for status in ["success", "error"] {
            record_grpc_request(method, status);
        }
    }
    for method in [
        "register",
        "register_meeting",
        "route_media",
        "stream_telemetry",
    ] {
        for status in ["success", "error"] {
            snap.counter("mh_grpc_requests_total")
                .with_labels(&[("method", method), ("status", status)])
                .assert_delta(1);
        }
    }
}

// ---------------------------------------------------------------------------
// mh_errors_total
// ---------------------------------------------------------------------------

#[test]
fn record_error_emits_errors_counter() {
    let snap = MetricAssertion::snapshot();
    record_error("register_meeting", "Timeout", 504);
    snap.counter("mh_errors_total")
        .with_labels(&[
            ("operation", "register_meeting"),
            ("error_type", "Timeout"),
            ("status_code", "504"),
        ])
        .assert_delta(1);
}

#[test]
fn record_error_emits_distinct_series_per_status_code() {
    let snap = MetricAssertion::snapshot();
    record_error("mc_notify", "Internal", 500);
    record_error("mc_notify", "BadRequest", 400);
    snap.counter("mh_errors_total")
        .with_labels(&[
            ("operation", "mc_notify"),
            ("error_type", "Internal"),
            ("status_code", "500"),
        ])
        .assert_delta(1);
    snap.counter("mh_errors_total")
        .with_labels(&[
            ("operation", "mc_notify"),
            ("error_type", "BadRequest"),
            ("status_code", "400"),
        ])
        .assert_delta(1);
}
