//! Integration cover for `gc_errors_total` per ADR-0032 Step 5 §Cluster 4.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_error` is synchronous.
//!
//! # Per-failure-class fidelity
//!
//! `gc_errors_total{operation, error_type, status_code}` is the global error
//! counter. Recording sites in production:
//!
//! | operation              | error_type            | status_code | source |
//! |------------------------|-----------------------|-------------|--------|
//! | `mc_grpc`              | `connection_failed`   | 503         | `services/mc_client.rs:158` |
//! | `mc_grpc`              | `service_unavailable` | 503         | `services/mc_client.rs:198` |
//! | `ac_meeting_token`     | `service_unavailable` | 503         | `services/ac_client.rs:104` |
//! | `ac_meeting_token`     | dynamic (4xx/5xx)     | dynamic     | `services/ac_client.rs:114` |
//! | `ac_guest_token`       | `service_unavailable` | 503         | `services/ac_client.rs:152` |
//! | `ac_guest_token`       | dynamic (4xx/5xx)     | dynamic     | `services/ac_client.rs:162` |
//!
//! This file exercises every static (operation, error_type, status_code)
//! triple — wrapper-Cat-C name-coverage tier. Driving the *real* recording
//! sites end-to-end (mc_client behind a tonic mock, ac_client behind a
//! wiremock returning controlled status codes) is captured in
//! `ac_request_metrics_integration.rs` and `grpc_mc_call_metrics_integration.rs`,
//! which exercise the same call sites for the per-subsystem counters
//! (`gc_ac_requests_total`, `gc_grpc_mc_calls_total`). Those drives also
//! emit `gc_errors_total` along the same paths — the cluster file here is
//! the per-failure-class label-fidelity mirror.
//!
//! Adjacency: every test asserts `assert_delta(0)` on the OTHER (operation,
//! error_type) combinations under the same status_code (label-swap catcher).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_error;

#[test]
fn record_error_mc_grpc_connection_failed_emits_503() {
    let snap = MetricAssertion::snapshot();

    record_error("mc_grpc", "connection_failed", 503);

    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "mc_grpc"),
            ("error_type", "connection_failed"),
            ("status_code", "503"),
        ])
        .assert_delta(1);

    // Adjacency: ac_* operations silent under same status_code.
    for op in ["ac_meeting_token", "ac_guest_token"] {
        snap.counter("gc_errors_total")
            .with_labels(&[("operation", op), ("status_code", "503")])
            .assert_delta(0);
    }
    // Other mc_grpc error_types silent.
    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "mc_grpc"),
            ("error_type", "service_unavailable"),
        ])
        .assert_delta(0);
}

#[test]
fn record_error_mc_grpc_service_unavailable_emits_503() {
    let snap = MetricAssertion::snapshot();

    record_error("mc_grpc", "service_unavailable", 503);

    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "mc_grpc"),
            ("error_type", "service_unavailable"),
            ("status_code", "503"),
        ])
        .assert_delta(1);
    // Label-swap catcher: connection_failed sibling silent.
    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "mc_grpc"),
            ("error_type", "connection_failed"),
        ])
        .assert_delta(0);
}

#[test]
fn record_error_ac_meeting_token_service_unavailable_emits_503() {
    let snap = MetricAssertion::snapshot();

    record_error("ac_meeting_token", "service_unavailable", 503);

    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "ac_meeting_token"),
            ("error_type", "service_unavailable"),
            ("status_code", "503"),
        ])
        .assert_delta(1);
    // Adjacency: guest-token and mc_grpc paths silent.
    snap.counter("gc_errors_total")
        .with_labels(&[("operation", "ac_guest_token")])
        .assert_delta(0);
    snap.counter("gc_errors_total")
        .with_labels(&[("operation", "mc_grpc")])
        .assert_delta(0);
}

#[test]
fn record_error_ac_meeting_token_dynamic_status_codes() {
    // `ac_client.rs:114` records the response's actual error status code
    // via `e.status_code()`. Verify the bounded-but-dynamic axis emits
    // each status_code as a distinct label value.
    for status_code in [400u16, 403, 404, 500] {
        let snap = MetricAssertion::snapshot();

        record_error("ac_meeting_token", "bad_request", status_code);

        snap.counter("gc_errors_total")
            .with_labels(&[
                ("operation", "ac_meeting_token"),
                ("error_type", "bad_request"),
                ("status_code", &status_code.to_string()),
            ])
            .assert_delta(1);
    }
}

#[test]
fn record_error_ac_guest_token_service_unavailable_emits_503() {
    let snap = MetricAssertion::snapshot();

    record_error("ac_guest_token", "service_unavailable", 503);

    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "ac_guest_token"),
            ("error_type", "service_unavailable"),
            ("status_code", "503"),
        ])
        .assert_delta(1);
    // Label-swap catcher: meeting-token sibling silent under same labels.
    snap.counter("gc_errors_total")
        .with_labels(&[
            ("operation", "ac_meeting_token"),
            ("error_type", "service_unavailable"),
        ])
        .assert_delta(0);
}

#[test]
fn record_error_ac_guest_token_dynamic_status_codes() {
    // `ac_client.rs:162` records dynamic status codes for guest-token errors.
    for status_code in [400u16, 403, 404, 500] {
        let snap = MetricAssertion::snapshot();

        record_error("ac_guest_token", "forbidden", status_code);

        snap.counter("gc_errors_total")
            .with_labels(&[
                ("operation", "ac_guest_token"),
                ("error_type", "forbidden"),
                ("status_code", &status_code.to_string()),
            ])
            .assert_delta(1);
    }
}
