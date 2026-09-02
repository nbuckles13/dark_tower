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
//! - `mh_grpc_requests_total`: `method` is **single-valued** —
//!   `register_meeting` is the only RPC on `MediaHandlerService`, because
//!   ADR-0036 §8 makes meeting registration the control plane and it gains
//!   fields rather than sibling RPCs. `status` ∈ {success, error}.
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
    record_grpc_request("success");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(1);
}

#[test]
fn record_grpc_request_emits_separate_series_per_status() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request("error");
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "error")])
        .assert_delta(1);
    // Adjacency: the success counter for the same method is NOT incremented.
    // Preserved from the pre-collapse matrix — the `status` dimension is still
    // two-valued, and a swapped-label bug there is still possible.
    snap.counter("mh_grpc_requests_total")
        .with_labels(&[("method", "register_meeting"), ("status", "success")])
        .assert_delta(0);
}

/// The `method` label VALUE is asserted explicitly, not just carried along.
///
/// Dropping the `method` parameter made a second value impossible to introduce
/// from a call site, which is the point — but it moved the one remaining
/// method-label bug into the emitter: a typo in the
/// `GRPC_METHOD_REGISTER_MEETING` const. That is invisible to a test which only
/// asserts "some method label was emitted", and it would silently blank the
/// runbook queries and dashboard panels that select
/// `method="register_meeting"` (an unmatched label yields an empty series, not
/// an error). This assertion is what catches it.
#[test]
fn record_grpc_request_emits_the_single_bounded_method_value() {
    let snap = MetricAssertion::snapshot();
    record_grpc_request("success");
    record_grpc_request("error");
    for status in ["success", "error"] {
        snap.counter("mh_grpc_requests_total")
            .with_labels(&[("method", "register_meeting"), ("status", status)])
            .assert_delta(1);
    }
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
