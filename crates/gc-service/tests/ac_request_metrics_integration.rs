//! Integration cover for `gc_ac_requests_total` and
//! `gc_ac_request_duration_seconds` per ADR-0032 Step 5 §Cluster 9.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_ac_request` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! Production recording sites: `crates/gc-service/src/services/ac_client.rs`
//! lines 103, 111, 113, 151, 159, 161 — every (operation × status) cell for
//! `meeting_token` and `guest_token`. Driving each branch end-to-end
//! through `AcClient` requires a `wiremock::MockServer` returning each
//! response shape (success JSON, 503, 4xx). Real-recording-site coverage
//! through the existing GC test harness is non-trivial because `AcClient`
//! isn't a public type with a constructor that bypasses the
//! `TokenReceiver` requirement.
//!
//! Per ADR-0032 Step 5 plan-stage scope: full per-failure-class label
//! fidelity asserted at the wrapper here; deeper end-to-end coverage is
//! captured in the cross-cluster `errors_metric_integration.rs` (which
//! exercises the `record_error` calls that fire from the same source lines).
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition
//! tracker covering full real-recording-site drive via wiremock fixtures.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_ac_request;

const OPERATIONS: &[&str] = &["meeting_token", "guest_token"];

#[test]
fn ac_request_meeting_token_success_emits_operation_status_tuple() {
    let snap = MetricAssertion::snapshot();

    record_ac_request("meeting_token", "success", Duration::from_millis(100));

    snap.histogram("gc_ac_request_duration_seconds")
        .with_labels(&[("operation", "meeting_token")])
        .assert_observation_count(1);

    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "meeting_token"), ("status", "success")])
        .assert_delta(1);

    // Adjacency: meeting_token error and guest_token paths silent.
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "meeting_token"), ("status", "error")])
        .assert_delta(0);
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "guest_token")])
        .assert_delta(0);
}

#[test]
fn ac_request_meeting_token_error_emits_status_error() {
    let snap = MetricAssertion::snapshot();

    record_ac_request("meeting_token", "error", Duration::from_millis(200));

    snap.histogram("gc_ac_request_duration_seconds")
        .with_labels(&[("operation", "meeting_token")])
        .assert_observation_count(1);
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "meeting_token"), ("status", "error")])
        .assert_delta(1);
    // Label-swap catcher: success sibling silent under same operation.
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "meeting_token"), ("status", "success")])
        .assert_delta(0);
}

#[test]
fn ac_request_guest_token_success_emits_operation_status_tuple() {
    let snap = MetricAssertion::snapshot();

    record_ac_request("guest_token", "success", Duration::from_millis(80));

    snap.histogram("gc_ac_request_duration_seconds")
        .with_labels(&[("operation", "guest_token")])
        .assert_observation_count(1);
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "guest_token"), ("status", "success")])
        .assert_delta(1);
    // Adjacency: meeting_token paths silent.
    for op in OPERATIONS.iter().filter(|o| **o != "guest_token") {
        snap.counter("gc_ac_requests_total")
            .with_labels(&[("operation", *op)])
            .assert_delta(0);
    }
}

#[test]
fn ac_request_guest_token_error_emits_status_error() {
    let snap = MetricAssertion::snapshot();

    record_ac_request("guest_token", "error", Duration::from_millis(150));

    snap.histogram("gc_ac_request_duration_seconds")
        .with_labels(&[("operation", "guest_token")])
        .assert_observation_count(1);
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "guest_token"), ("status", "error")])
        .assert_delta(1);
    // Label-swap catcher: meeting_token "error" silent under same status.
    snap.counter("gc_ac_requests_total")
        .with_labels(&[("operation", "meeting_token"), ("status", "error")])
        .assert_delta(0);
}
