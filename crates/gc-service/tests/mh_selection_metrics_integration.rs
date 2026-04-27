//! Integration cover for `gc_mh_selections_total` and
//! `gc_mh_selection_duration_seconds` per ADR-0032 Step 5 §Cluster 8.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_mh_selection` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! Production recording sites: `crates/gc-service/src/services/mh_selection.rs`
//! lines 81 (error) and 143 (success). The `has_multiple` axis is `true` when
//! at least 2 healthy MHs are available for the meeting's region; `false`
//! otherwise. Driving the (success × {true, false}) and error branches
//! end-to-end requires DB seeding with a controlled mix of healthy MHs.
//! Real-recording-site coverage of the success branch exists in
//! `crates/gc-service/tests/mh_registry_tests.rs`. The error branch
//! ("no healthy MHs") is reachable but rarely driven; the (false, true)
//! `has_multiple` cells are wrapper-asserted here for label fidelity.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition tracker
//! covering full real-recording-site drive of the error branch.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_mh_selection;

#[test]
fn mh_selection_success_with_multiple_handlers_emits_has_multiple_true() {
    let snap = MetricAssertion::snapshot();

    record_mh_selection("success", true, Duration::from_millis(8));

    snap.histogram("gc_mh_selection_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_observation_count(1);
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "success"), ("has_multiple", "true")])
        .assert_delta(1);
    // Adjacency: other (status, has_multiple) tuples silent.
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "success"), ("has_multiple", "false")])
        .assert_delta(0);
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "error"), ("has_multiple", "true")])
        .assert_delta(0);
}

#[test]
fn mh_selection_success_with_single_handler_emits_has_multiple_false() {
    let snap = MetricAssertion::snapshot();

    record_mh_selection("success", false, Duration::from_millis(5));

    snap.histogram("gc_mh_selection_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_observation_count(1);
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "success"), ("has_multiple", "false")])
        .assert_delta(1);
    // Label-swap catcher: has_multiple=true sibling under same status silent.
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "success"), ("has_multiple", "true")])
        .assert_delta(0);
}

#[test]
fn mh_selection_error_emits_status_error() {
    let snap = MetricAssertion::snapshot();

    record_mh_selection("error", false, Duration::from_millis(3));

    snap.histogram("gc_mh_selection_duration_seconds")
        .with_labels(&[("status", "error")])
        .assert_observation_count(1);
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "error"), ("has_multiple", "false")])
        .assert_delta(1);
    // Label-swap catcher: status=success sibling silent under same has_multiple.
    snap.counter("gc_mh_selections_total")
        .with_labels(&[("status", "success"), ("has_multiple", "false")])
        .assert_delta(0);
    snap.histogram("gc_mh_selection_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_unobserved();
}
