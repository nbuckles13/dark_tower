//! Component tests for the actor-system metrics: `mc_meetings_active`,
//! `mc_connections_active`, `mc_actor_mailbox_depth`,
//! `mc_actor_panics_total`, `mc_messages_dropped_total`.
//!
//! All emissions happen inside `ActorMetrics` / `MailboxMonitor` methods at
//! `crates/mc-service/src/actors/metrics.rs:139,173,183,352,365,375,388,398`.
//! These methods are synchronous; tests bind a `MetricAssertion` snapshot,
//! invoke the methods, and assert per-`actor_type` deltas with `(0)` adjacency
//! across all three sibling actor types (label-swap-bug catcher per ADR-0032
//! §Pattern #3).
//!
//! # Carve-out: wrapper-only coverage for these metrics
//!
//! Per ADR-0032 Step 3 §Test review F4 — these tests call `ActorMetrics` /
//! `MailboxMonitor` directly rather than driving them through the full actor
//! supervision tree (which would require a panicking actor harness, mailbox
//! overflow simulation, and timed back-pressure). The fidelity gap is
//! acceptable here because:
//!   1. The metric methods are trivial pass-throughs to `metrics::counter!` /
//!      `metrics::gauge!` — the only thing that can drift is the label tuple,
//!      and the per-`actor_type` adjacency below catches that.
//!   2. The production call sites are themselves covered by actor-loop unit
//!      tests in `crates/mc-service/src/actors/*.rs::tests`, which exercise
//!      the `meeting_created`/`record_panic`/`record_drop` calls under real
//!      panic-and-recover paths.
//!
//! # Why a fresh `MailboxMonitor` per actor_type
//!
//! `MailboxMonitor::new(actor_type, ...)` is the unit of label scoping —
//! gauges/counters emitted from a monitor carry that monitor's `actor_type`.
//! Sharing one monitor across labels would label-collapse the gauge to the
//! last constructor arg. ADR-0032 §F3 codified this discipline.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use mc_service::actors::metrics::ActorType;
use mc_service::actors::{ActorMetrics, MailboxMonitor};

const ALL_ACTOR_TYPES: &[(&str, ActorType)] = &[
    ("controller", ActorType::Controller),
    ("meeting", ActorType::Meeting),
    ("participant", ActorType::Participant),
];

// ---------------------------------------------------------------------------
// `mc_meetings_active` and `mc_connections_active` (gauges)
// ---------------------------------------------------------------------------

// `assert_value(0.0)` on an un-observed gauge panics with "not observed", so a
// pure absence-of-emission check is not directly expressible against this API.
// Where adjacency matters most for gauges (a refactor swapping the two metric
// names below), the per-`actor_type` adjacency on `mc_actor_panics_total` /
// `mc_messages_dropped_total` already catches the analogous label-swap class
// for the per-actor-type metrics. Tracked in ADR-0032 §F4 as a deferred
// fidelity gap (note: not a blocker — the prod call sites are also covered by
// `crates/mc-service/src/actors/*.rs::tests`).

#[test]
fn actor_metrics_meeting_created_increments_meetings_active_gauge() {
    let metrics = ActorMetrics::new();
    let snap = MetricAssertion::snapshot();

    metrics.meeting_created();
    metrics.meeting_created();
    metrics.meeting_created();

    snap.gauge("mc_meetings_active").assert_value(3.0);

    metrics.meeting_removed();
    snap.gauge("mc_meetings_active").assert_value(2.0);
}

#[test]
fn actor_metrics_connection_created_increments_connections_active_gauge() {
    let metrics = ActorMetrics::new();
    let snap = MetricAssertion::snapshot();

    metrics.connection_created();
    metrics.connection_created();
    metrics.connection_created();
    metrics.connection_created();
    metrics.connection_created();

    snap.gauge("mc_connections_active").assert_value(5.0);

    metrics.connection_closed();
    snap.gauge("mc_connections_active").assert_value(4.0);
}

// ---------------------------------------------------------------------------
// `mc_actor_mailbox_depth` (gauge per actor_type)
// ---------------------------------------------------------------------------

#[test]
fn mailbox_monitor_record_enqueue_sets_mailbox_depth_per_actor_type() {
    // Fresh `MailboxMonitor` per actor_type — instance-mutation pattern would
    // label-collapse the gauge to the last constructor arg.
    let snap = MetricAssertion::snapshot();

    for (label, actor_type) in ALL_ACTOR_TYPES {
        let monitor = MailboxMonitor::new(*actor_type, format!("{label}-id"));
        for _ in 0..5 {
            monitor.record_enqueue();
        }
        snap.gauge("mc_actor_mailbox_depth")
            .with_labels(&[("actor_type", *label)])
            .assert_value(5.0);
    }
}

// ---------------------------------------------------------------------------
// `mc_actor_panics_total` (counter per actor_type)
// ---------------------------------------------------------------------------

#[test]
fn actor_metrics_record_panic_increments_per_actor_type_with_adjacency() {
    let metrics = ActorMetrics::new();

    for (named_label, named_type) in ALL_ACTOR_TYPES {
        let snap = MetricAssertion::snapshot();
        metrics.record_panic(*named_type);

        snap.counter("mc_actor_panics_total")
            .with_labels(&[("actor_type", *named_label)])
            .assert_delta(1);
        // Adjacency on the other two actor_types — catches a label-swap bug.
        for (sibling_label, _) in ALL_ACTOR_TYPES {
            if sibling_label == named_label {
                continue;
            }
            snap.counter("mc_actor_panics_total")
                .with_labels(&[("actor_type", *sibling_label)])
                .assert_delta(0);
        }
    }
}

// ---------------------------------------------------------------------------
// `mc_messages_dropped_total` (counter per actor_type, via MailboxMonitor::drop)
// ---------------------------------------------------------------------------

#[test]
fn mailbox_monitor_record_drop_increments_messages_dropped_per_actor_type() {
    for (named_label, named_type) in ALL_ACTOR_TYPES {
        let snap = MetricAssertion::snapshot();
        let monitor = MailboxMonitor::new(*named_type, format!("{named_label}-drop-id"));
        monitor.record_drop();

        snap.counter("mc_messages_dropped_total")
            .with_labels(&[("actor_type", *named_label)])
            .assert_delta(1);
        for (sibling_label, _) in ALL_ACTOR_TYPES {
            if sibling_label == named_label {
                continue;
            }
            snap.counter("mc_messages_dropped_total")
                .with_labels(&[("actor_type", *sibling_label)])
                .assert_delta(0);
        }
    }
}
