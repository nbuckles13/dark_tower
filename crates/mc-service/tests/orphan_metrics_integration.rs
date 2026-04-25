//! Wrapper-only tests for the **orphan metrics**: `mc_message_latency_seconds`,
//! `mc_recovery_duration_seconds`, `mc_errors_total`.
//!
//! # Status: orphans — no production caller at HEAD
//!
//! These three wrappers exist in `src/observability/metrics.rs` but have
//! ZERO call sites in `crates/mc-service/src/`:
//!   - `record_message_latency` at `metrics.rs:154` — 0 callers
//!   - `record_recovery_duration` at `metrics.rs:182` — 0 callers
//!   - `record_error` at `metrics.rs:455` — 0 callers (the `mc_errors_total`
//!     `counter!` macro at `metrics.rs:456` is invoked ONLY from inside the
//!     wrapper itself)
//!
//! Tests in this file prove only that the wrapper's `metrics::histogram!` /
//! `metrics::counter!` macro emission lands in the recorder. They do NOT
//! prove any production path reaches them — because none does at HEAD.
//!
//! # Why this file exists at all
//!
//! ADR-0032 §Implementation Notes phasing step 3 requires draining the
//! `validate-metric-coverage.sh` guard for mc-service to 0. The guard is
//! presence-based (fixed-string scan over `tests/**/*.rs`); orphans need a
//! reference somewhere under `tests/` to satisfy it. @observability sign-off
//! routes this to a tech-debt entry "MC observability orphans — wire
//! production callers OR remove" in `docs/TODO.md §Observability Debt`.
//! Either disposition closes the debt; the entry deliberately does NOT bias
//! toward "wire it up" so a maintainer who decides the metric is unnecessary
//! can simply delete it.
//!
//! When a real production caller lands, replace the test in this file with a
//! real-recording-site drive (see e.g. `webtransport_accept_loop_integration.rs`
//! for the rig pattern).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use mc_service::observability::metrics::{
    record_error, record_message_latency, record_recovery_duration,
};

#[test]
fn orphan_record_message_latency_emits_per_message_type() {
    // `message_type` cardinality is bounded by signaling protobuf message
    // types (see `metrics.rs:151`). When the production caller lands it
    // will likely emit one or more of: `join_request`, `leave_request`,
    // `layout_update`, `media_control`. Test all four to lock the bounded
    // set in place.
    //
    // NOTE on drain-on-read: `Snapshotter::snapshot` drains ALL histogram
    // observations every time `take_entries` is called. Asserting on
    // multiple `(name, labels)` tuples in one snapshot would see the first
    // assertion drain everything. Fresh snapshot per message_type sidesteps
    // this. See `crates/common/src/observability/testing.rs` §Delta semantics.
    for mt in &[
        "join_request",
        "leave_request",
        "layout_update",
        "media_control",
    ] {
        let snap = MetricAssertion::snapshot();
        record_message_latency(mt, Duration::from_millis(10));
        snap.histogram("mc_message_latency_seconds")
            .with_labels(&[("message_type", *mt)])
            .assert_observation_count_at_least(1);
    }
}

#[test]
fn orphan_record_recovery_duration_emits_observation() {
    let snap = MetricAssertion::snapshot();
    record_recovery_duration(Duration::from_millis(100));

    snap.histogram("mc_recovery_duration_seconds")
        .assert_observation_count_at_least(1);
}

#[test]
fn orphan_record_error_emits_per_operation_per_error_type() {
    // `operation` and `error_type` are unbounded by the wrapper signature
    // but ADR-0011 caps cardinality. Representative combinations match the
    // existing `metrics.rs::tests::test_record_error` set.
    let snap = MetricAssertion::snapshot();
    record_error("token_refresh", "http", 6);
    record_error("gc_heartbeat", "grpc", 6);
    record_error("redis_session", "redis", 6);
    record_error("meeting_join", "capacity_exceeded", 7);
    record_error("session_binding", "session_binding", 2);

    snap.counter("mc_errors_total")
        .with_labels(&[
            ("operation", "token_refresh"),
            ("error_type", "http"),
            ("status_code", "6"),
        ])
        .assert_delta(1);
    snap.counter("mc_errors_total")
        .with_labels(&[
            ("operation", "meeting_join"),
            ("error_type", "capacity_exceeded"),
            ("status_code", "7"),
        ])
        .assert_delta(1);
    snap.counter("mc_errors_total")
        .with_labels(&[
            ("operation", "session_binding"),
            ("error_type", "session_binding"),
            ("status_code", "2"),
        ])
        .assert_delta(1);
}
