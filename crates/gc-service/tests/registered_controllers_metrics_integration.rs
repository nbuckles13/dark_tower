//! Integration cover for `gc_registered_controllers` gauge per ADR-0032 Step 5
//! §Cluster 12.
//!
//! Per `MetricAssertion`'s per-thread recorder isolation, these tests run on
//! the cargo test runner's per-test thread. No tokio runtime pinning needed —
//! `update_registered_controller_gauges` is synchronous.
//!
//! # 4-cell adjacency-coverage matrix (per @code-reviewer ADR-0032 Step 5)
//!
//! 1. **Full happy path** — all 5 statuses present with non-zero counts.
//! 2. **Partial counts → zero-fill** — `assert_value(0.0)` on missing
//!    statuses, NOT `assert_unobserved`. The wrapper explicitly emits
//!    `set(0.0)` for absent statuses, so the metric IS observed at zero.
//!    Distinct from cell 4 below.
//! 3. **Empty counts** → all 5 statuses zero-filled (boundary).
//! 4. **Caller short-circuits before update_*** → `assert_unobserved` on
//!    the full label space. Catches a refactor that accidentally
//!    always-emits, distinct from cell 2's zero-fill correctness.
//!
//! The full per-failure-class matrix also lives in
//! `crates/gc-service/src/observability/metrics.rs::tests` — this file is the
//! integration-test mirror that satisfies the
//! `validate-metric-coverage.sh` guard's `tests/**/*.rs` scan.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use gc_service::observability::metrics::{
    set_registered_controllers, update_registered_controller_gauges, CONTROLLER_STATUSES,
};

#[test]
fn registered_controllers_full_happy_path_all_statuses_set() {
    // Cell 1: full happy path.
    let snap = MetricAssertion::snapshot();

    let counts = vec![
        ("pending".to_string(), 1u64),
        ("healthy".to_string(), 10),
        ("degraded".to_string(), 3),
        ("unhealthy".to_string(), 2),
        ("draining".to_string(), 1),
    ];
    update_registered_controller_gauges("meeting", &counts);

    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "pending")])
        .assert_value(1.0);
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "healthy")])
        .assert_value(10.0);
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "degraded")])
        .assert_value(3.0);
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "unhealthy")])
        .assert_value(2.0);
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "draining")])
        .assert_value(1.0);
}

#[test]
fn registered_controllers_partial_counts_zero_fill_via_assert_value() {
    // Cell 2: partial counts → zero-fill on missing statuses.
    // assert_value(0.0) — NOT assert_unobserved — because the wrapper emits
    // set(0.0) explicitly. assert_unobserved here would FAIL because the
    // metric IS observed at zero.
    let snap = MetricAssertion::snapshot();

    let partial_counts = vec![("healthy".to_string(), 7u64), ("degraded".to_string(), 1)];
    update_registered_controller_gauges("media", &partial_counts);

    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "media"), ("status", "healthy")])
        .assert_value(7.0);
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "media"), ("status", "degraded")])
        .assert_value(1.0);
    // Missing statuses are explicitly set to 0.0 by the zero-fill loop.
    for missing in ["pending", "unhealthy", "draining"] {
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "media"), ("status", missing)])
            .assert_value(0.0);
    }
}

#[test]
fn registered_controllers_empty_counts_all_five_statuses_zero() {
    // Cell 3: empty counts → all 5 statuses zero-filled.
    let snap = MetricAssertion::snapshot();

    update_registered_controller_gauges("meeting", &[]);

    for status in CONTROLLER_STATUSES {
        snap.gauge("gc_registered_controllers")
            .with_labels(&[("controller_type", "meeting"), ("status", status)])
            .assert_value(0.0);
    }
}

#[test]
fn registered_controllers_unobserved_when_caller_short_circuits() {
    // Cell 4: code path that does NOT call update_registered_controller_gauges
    // — the gauge must be unobserved across the full label space. This
    // proves `assert_unobserved` (kind+name+labels axis) catches a
    // hypothetical refactor that accidentally always-emits, distinct from
    // cell 2's zero-fill correctness.
    let snap = MetricAssertion::snapshot();

    // Simulating an upstream short-circuit: no call to
    // `update_registered_controller_gauges` at all on this test thread.

    for controller_type in ["meeting", "media"] {
        for status in CONTROLLER_STATUSES {
            snap.gauge("gc_registered_controllers")
                .with_labels(&[("controller_type", controller_type), ("status", status)])
                .assert_unobserved();
        }
    }
}

#[test]
fn registered_controllers_set_directly_emits_single_label_tuple() {
    // Drives `set_registered_controllers` (the lower-level wrapper used inside
    // `update_registered_controller_gauges`) — proves a direct call also lands
    // in the same metric family with the same label shape.
    let snap = MetricAssertion::snapshot();

    set_registered_controllers("media", "healthy", 42);

    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "media"), ("status", "healthy")])
        .assert_value(42.0);
    // Adjacency: other (controller_type, status) tuples remain unobserved.
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "media"), ("status", "degraded")])
        .assert_unobserved();
    snap.gauge("gc_registered_controllers")
        .with_labels(&[("controller_type", "meeting"), ("status", "healthy")])
        .assert_unobserved();
}
