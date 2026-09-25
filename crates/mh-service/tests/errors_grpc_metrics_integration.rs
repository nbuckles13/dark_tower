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
//! Bounded label values per `docs/observability/metrics/mh-service.md`:
//!
//! - `mh_grpc_requests_total`: `method` is one value per RPC on
//!   `MediaHandlerService` — `register_meeting` and `end_meeting`, the
//!   `observability::metrics::GrpcMethod` vocabulary, whose source of truth is
//!   `internal.proto`'s service block. Both are emitted, so this file asserts
//!   both. `status` ∈ {success, error}.
//! - `mh_errors_total`: `operation` is a stable identifier from the call site
//!   (e.g. `register_meeting`, `mc_notify`), `error_type` is from
//!   `MhError`-variant naming, `status_code` is the HTTP/gRPC status int.

use common::observability::testing::MetricAssertion;
use mh_service::observability::metrics::{record_error, record_grpc_request, GrpcMethod};

// ---------------------------------------------------------------------------
// mh_grpc_requests_total
// ---------------------------------------------------------------------------

#[test]
fn record_grpc_request_emits_grpc_requests_counter() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request(GrpcMethod::RegisterMeeting, "success");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(1);
}

#[test]
fn record_grpc_request_emits_separate_series_per_status() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request(GrpcMethod::RegisterMeeting, "error");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "error")])
        .assert_delta(1);
    // Adjacency: the success counter for the same method is NOT incremented —
    // a swapped-label bug on the two-valued `status` dimension is still
    // possible.
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(0);
}

/// The two `method` label VALUES are asserted explicitly, each recorded under
/// its own label and neither under the other's.
///
/// The cross-check is the load-bearing half: a recorder that counted a
/// teardown as `register_meeting` would forge the R-26 `RegisterMeeting`
/// receipt signal, and an unmatched label value silently blanks the runbook
/// queries and panels selecting it (an empty series, not an error).
#[test]
fn record_grpc_request_emits_both_bounded_method_values_and_never_crosses_them() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request(GrpcMethod::EndMeeting, "success");
    record_grpc_request(GrpcMethod::EndMeeting, "error");
    for status in ["success", "error"] {
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "end_meeting"), ("status", status)])
            .assert_delta(1);
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", status)])
            .assert_delta(0);
    }

    let snap = MetricAssertion::snapshot();
    record_grpc_request(GrpcMethod::RegisterMeeting, "success");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(1);
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "end_meeting"), ("status", "success")])
        .assert_delta(0);

    // The three values retired by the 2026-09-01 `internal.proto` reshape can
    // never be emitted again — the proto's tombstone block forbids resurrecting
    // the RPC names, so these series must stay dead.
    for retired in ["register", "route_media", "stream_telemetry"] {
        for status in ["success", "error"] {
            snap.counter("mh_grpc_requests_total")
                .with_labels(&[("method", retired), ("status", status)])
                .assert_delta(0);
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
