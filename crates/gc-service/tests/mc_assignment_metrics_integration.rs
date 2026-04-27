//! Integration cover for `gc_mc_assignments_total` and
//! `gc_mc_assignment_duration_seconds` per ADR-0032 Step 5 §Cluster 2.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_mc_assignment` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! Production recording sites: `crates/gc-service/src/services/mc_assignment.rs`
//! lines 153, 251, 294 — covers (status × rejection_reason) for success,
//! all rejection reasons (at_capacity, draining, unhealthy), and error.
//! Real-recording-site coverage of the success path exists in
//! `crates/gc-service/tests/mc_assignment_rpc_tests.rs` and
//! `crates/gc-service/tests/meeting_assignment_tests.rs`. The rejection
//! branches require the `MockMcClient::rejecting()` fixture — also covered
//! in those files.
//!
//! Cluster file here is the per-failure-class label-fidelity mirror,
//! ensuring every (status, rejection_reason) tuple stays correctly named.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering coordinated drive across both files for label fidelity.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_mc_assignment;

const ALL_REJECTION_REASONS: &[&str] = &["at_capacity", "draining", "unhealthy"];

#[test]
fn mc_assignment_success_emits_status_success_with_rejection_reason_none() {
    let snap = MetricAssertion::snapshot();

    record_mc_assignment("success", None, Duration::from_millis(15));

    snap.histogram("gc_mc_assignment_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_observation_count(1);

    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "success"), ("rejection_reason", "none")])
        .assert_delta(1);

    // Adjacency: rejection reasons silent on success path.
    for reason in ALL_REJECTION_REASONS {
        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "rejected"), ("rejection_reason", *reason)])
            .assert_delta(0);
    }
}

#[test]
fn mc_assignment_rejected_at_capacity_emits_rejection_reason_at_capacity() {
    let snap = MetricAssertion::snapshot();

    record_mc_assignment("rejected", Some("at_capacity"), Duration::from_millis(10));

    snap.histogram("gc_mc_assignment_duration_seconds")
        .with_labels(&[("status", "rejected")])
        .assert_observation_count(1);
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "rejected"), ("rejection_reason", "at_capacity")])
        .assert_delta(1);
    // Label-swap catcher: other rejection_reason values silent.
    for sibling in ALL_REJECTION_REASONS
        .iter()
        .filter(|r| **r != "at_capacity")
    {
        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "rejected"), ("rejection_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn mc_assignment_rejected_draining_emits_rejection_reason_draining() {
    let snap = MetricAssertion::snapshot();

    record_mc_assignment("rejected", Some("draining"), Duration::from_millis(8));

    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "rejected"), ("rejection_reason", "draining")])
        .assert_delta(1);
    for sibling in ALL_REJECTION_REASONS.iter().filter(|r| **r != "draining") {
        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "rejected"), ("rejection_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn mc_assignment_rejected_unhealthy_emits_rejection_reason_unhealthy() {
    let snap = MetricAssertion::snapshot();

    record_mc_assignment("rejected", Some("unhealthy"), Duration::from_millis(5));

    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "rejected"), ("rejection_reason", "unhealthy")])
        .assert_delta(1);
    for sibling in ALL_REJECTION_REASONS.iter().filter(|r| **r != "unhealthy") {
        snap.counter("gc_mc_assignments_total")
            .with_labels(&[("status", "rejected"), ("rejection_reason", *sibling)])
            .assert_delta(0);
    }
}

#[test]
fn mc_assignment_error_emits_status_error_with_rejection_reason_value() {
    // The error path passes a rejection_reason describing the rpc-level
    // failure (e.g. "rpc_failed"). Verify the wrapper accepts dynamic
    // bounded strings on this axis without label-explosion.
    let snap = MetricAssertion::snapshot();

    record_mc_assignment("error", Some("rpc_failed"), Duration::from_millis(100));

    snap.histogram("gc_mc_assignment_duration_seconds")
        .with_labels(&[("status", "error")])
        .assert_observation_count(1);
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "error"), ("rejection_reason", "rpc_failed")])
        .assert_delta(1);
    // Adjacency: success path silent.
    snap.counter("gc_mc_assignments_total")
        .with_labels(&[("status", "success")])
        .assert_delta(0);
}
