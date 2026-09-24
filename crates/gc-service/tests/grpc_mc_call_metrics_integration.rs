//! Integration cover for `gc_grpc_mc_calls_total` and
//! `gc_grpc_mc_call_duration_seconds` per ADR-0032 Step 5 §Cluster 5.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_grpc_mc_call` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! Production recording site: all four `record_grpc_mc_call` emission sites in
//! `McClient::assign_meeting` (`crates/gc-service/src/services/mc_client.rs`) — the
//! four `(method, status)` tuples for the
//! `assign_meeting_with_mh` RPC (success, rejected, error, plus
//! connection-failed which lands in error). Driving each branch
//! end-to-end requires an in-process tonic mock that returns each
//! `McAssignmentResult` variant. Real-recording-site coverage exists in
//! `crates/gc-service/tests/mc_assignment_rpc_tests.rs` (success path) and
//! `crates/gc-service/tests/mc_repository_tests.rs` (rejected/error paths
//! through the assignment service). The cluster file here is the per-failure-
//! class label-fidelity mirror, ensuring every (method, status) tuple stays
//! correctly named after refactors.
//!
//! The `"assign_meeting_with_mh"` literals in the emission assertions below are
//! DELIBERATE and must NOT be replaced with
//! `gc_service::observability::metrics::MC_METHOD_ASSIGN_MEETING_WITH_MH`: a boundary
//! test restates the bound as a literal while production references the constant (same
//! convention as `crates/proto-gen/tests/internal_roundtrip.rs`). Substituting the
//! constant would make every assertion `constant == constant`, green by construction.
//!
//! Those restatements are only load-bearing because of
//! `mc_method_constant_matches_emitted_literal` below — the ONE place here that reads
//! the constant, pinning it to the wire value. Without that pin this file would merely
//! prove `record_grpc_mc_call` passes its first argument through to a label, which was
//! never in doubt, and a drifted constant would still be green. The drift is not
//! hypothetical: the catalog and the in-file cluster test both documented a
//! never-emitted `assign_meeting` until 2026-09-24.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering full real-recording-site drive.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_grpc_mc_call;

/// The one place this file reads the constant: pinning it to the wire literal is what
/// makes every other literal here meaningful. Without this, the restatements below are
/// `record_grpc_mc_call(x, ..)` asserting `x` — green for any value of the constant,
/// including a drifted one.
#[test]
fn mc_method_constant_matches_emitted_literal() {
    assert_eq!(
        gc_service::observability::metrics::MC_METHOD_ASSIGN_MEETING_WITH_MH,
        "assign_meeting_with_mh",
        "MC_METHOD_ASSIGN_MEETING_WITH_MH changed; update the literals in this file, \
         the `method` label value in docs/observability/metrics/gc-service.md, and any \
         PromQL selecting {{method=\"assign_meeting_with_mh\"}} (gc-incident-response.md \
         Scenario 2 step 4b)"
    );
}

const ALL_STATUSES: &[&str] = &["success", "rejected", "error"];

#[test]
fn grpc_mc_call_success_emits_method_status_tuple() {
    let snap = MetricAssertion::snapshot();

    record_grpc_mc_call(
        "assign_meeting_with_mh",
        "success",
        Duration::from_millis(25),
    );

    // Histogram first (drain-on-read).
    snap.histogram("gc_grpc_mc_call_duration_seconds")
        .with_labels(&[("method", "assign_meeting_with_mh")])
        .assert_observation_count(1);

    snap.counter("gc_grpc_mc_calls_total")
        .with_labels(&[("method", "assign_meeting_with_mh"), ("status", "success")])
        .assert_delta(1);

    // Adjacency: other statuses silent.
    for sibling in ALL_STATUSES.iter().filter(|s| **s != "success") {
        snap.counter("gc_grpc_mc_calls_total")
            .with_labels(&[("method", "assign_meeting_with_mh"), ("status", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn grpc_mc_call_rejected_emits_status_rejected() {
    let snap = MetricAssertion::snapshot();

    record_grpc_mc_call(
        "assign_meeting_with_mh",
        "rejected",
        Duration::from_millis(10),
    );

    snap.histogram("gc_grpc_mc_call_duration_seconds")
        .with_labels(&[("method", "assign_meeting_with_mh")])
        .assert_observation_count(1);
    snap.counter("gc_grpc_mc_calls_total")
        .with_labels(&[("method", "assign_meeting_with_mh"), ("status", "rejected")])
        .assert_delta(1);
    // Label-swap catcher: success and error siblings silent.
    snap.counter("gc_grpc_mc_calls_total")
        .with_labels(&[("method", "assign_meeting_with_mh"), ("status", "success")])
        .assert_delta(0);
    snap.counter("gc_grpc_mc_calls_total")
        .with_labels(&[("method", "assign_meeting_with_mh"), ("status", "error")])
        .assert_delta(0);
}

#[test]
fn grpc_mc_call_error_emits_status_error() {
    let snap = MetricAssertion::snapshot();

    record_grpc_mc_call(
        "assign_meeting_with_mh",
        "error",
        Duration::from_millis(100),
    );

    snap.histogram("gc_grpc_mc_call_duration_seconds")
        .with_labels(&[("method", "assign_meeting_with_mh")])
        .assert_observation_count(1);
    snap.counter("gc_grpc_mc_calls_total")
        .with_labels(&[("method", "assign_meeting_with_mh"), ("status", "error")])
        .assert_delta(1);
    for sibling in ["success", "rejected"] {
        snap.counter("gc_grpc_mc_calls_total")
            .with_labels(&[("method", "assign_meeting_with_mh"), ("status", sibling)])
            .assert_delta(0);
    }
}
