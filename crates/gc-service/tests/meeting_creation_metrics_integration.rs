//! Integration cover for `gc_meeting_creation_total`,
//! `gc_meeting_creation_duration_seconds`, and `gc_meeting_creation_failures_total`
//! per ADR-0032 Step 5 §Cluster 10.
//!
//! `MetricAssertion`'s per-thread recorder isolation applies. No tokio
//! runtime pinning needed — `record_meeting_creation` is synchronous.
//!
//! # WRAPPER-CAT-C framing
//!
//! Production recording sites: `crates/gc-service/src/handlers/meetings.rs`
//! lines 135, 147, 162, 169, 175, 192, 201, 209, 237, 253, 261, 269, 293
//! — covers every `error_type` value (`bad_request`, `forbidden`,
//! `unauthorized`, `internal`, `code_collision`, `db_error`) plus success.
//! Real-recording-site coverage of the `forbidden`/`bad_request`/success
//! branches exists in `crates/gc-service/tests/meeting_create_tests.rs`
//! (which drives the full `create_meeting` handler against a real DB +
//! wiremock JWKS). The cluster file here is the per-failure-class
//! label-fidelity mirror — every (status, error_type) tuple gets an
//! explicit drive and assert_delta(0) adjacency on siblings.
//!
//! See `docs/TODO.md` §Observability Debt for the orphan disposition
//! tracker covering full real-recording-site drives of the rarely-fired
//! `code_collision` / `db_error` / `internal` branches.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::record_meeting_creation;

const ALL_ERROR_TYPES: &[&str] = &[
    "bad_request",
    "forbidden",
    "unauthorized",
    "internal",
    "code_collision",
    "db_error",
];

#[test]
fn meeting_creation_success_emits_no_failure_counter() {
    let snap = MetricAssertion::snapshot();

    record_meeting_creation("success", None, Duration::from_millis(50));

    snap.histogram("gc_meeting_creation_duration_seconds")
        .with_labels(&[("status", "success")])
        .assert_observation_count(1);

    snap.counter("gc_meeting_creation_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);

    // Failures counter must be silent on success path across every bounded
    // error_type value (label-swap-bug catcher).
    for sibling in ALL_ERROR_TYPES {
        snap.counter("gc_meeting_creation_failures_total")
            .with_labels(&[("error_type", *sibling)])
            .assert_delta(0);
    }
    // assert_unobserved on the failures counter as a whole — symmetric
    // failure-path adjacency per ADR-0032 §F4.
    snap.counter("gc_meeting_creation_failures_total")
        .assert_unobserved();
}

#[test]
fn meeting_creation_failure_matrix_per_error_type() {
    // Drive each error_type value through the wrapper and assert the failures
    // counter fires with the expected label, with assert_delta(0) adjacency
    // on the other 5 error_type values (label-swap catcher).
    for category in ALL_ERROR_TYPES {
        let snap = MetricAssertion::snapshot();
        record_meeting_creation("error", Some(category), Duration::from_millis(10));

        snap.histogram("gc_meeting_creation_duration_seconds")
            .with_labels(&[("status", "error")])
            .assert_observation_count(1);
        snap.counter("gc_meeting_creation_total")
            .with_labels(&[("status", "error")])
            .assert_delta(1);
        snap.counter("gc_meeting_creation_total")
            .with_labels(&[("status", "success")])
            .assert_delta(0);
        snap.counter("gc_meeting_creation_failures_total")
            .with_labels(&[("error_type", *category)])
            .assert_delta(1);

        // Adjacency: every OTHER error_type value silent under same wrapper
        // call (label-swap-bug catcher per ADR-0032 §Pattern #3).
        for sibling in ALL_ERROR_TYPES.iter().filter(|t| **t != *category) {
            snap.counter("gc_meeting_creation_failures_total")
                .with_labels(&[("error_type", *sibling)])
                .assert_delta(0);
        }
    }
}
