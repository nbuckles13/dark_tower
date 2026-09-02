//! Wrapper-invocation coverage for `mc_meeting_assignments_total`.
//!
//! # Why wrapper invocation, not the real RPC
//!
//! `McAssignmentService::new` takes an `Arc<FencedRedisClient>`, and
//! `FencedRedisClient::new` calls `get_multiplexed_async_connection().await`
//! eagerly — so it cannot be constructed without a live Redis. Driving
//! `assign_meeting_with_mh` end to end would therefore need either a
//! `ConnectionManager` trait abstraction in `redis/client.rs` or a real Redis
//! fixture in `tests/`. That is the identical situation, and the identical
//! resolution, already recorded at the top of `redis_metrics_integration.rs`
//! and tracked in `docs/TODO.md §Observability Debt`.
//!
//! # What this file does and does NOT establish
//!
//! It DOES pin the emitted metric name, the two `status` values and the whole
//! bounded `rejection_reason` vocabulary, including the `"none"` spelling on the
//! success arm — which is the part a refactor is most likely to collapse, and
//! the part that has to match GC's `gc_mc_assignments_total` for the two ends of
//! the RPC to be joinable.
//!
//! It does NOT establish that the four production call sites in
//! `grpc/mc_service.rs` pass the right arguments. That gap is real and is why
//! the label list below is derived from those call sites by inspection rather
//! than invented: asserting a combination production cannot emit would be
//! wrapper-only theater.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use mc_service::observability::metrics::record_meeting_assignment;

/// The `rejection_reason` values reachable from `assign_meeting_with_mh`'s four
/// exit points (`grpc/mc_service.rs`), via `rejection_reason_label`:
///
/// - `at_capacity` / `draining` / `unspecified` — `can_accept_meeting()`
/// - `invalid_request` / `unhealthy` — `rejection_for_store_error()`
/// - `unhealthy` — the `create_meeting` failure arm (includes meeting-KEK
///   generation failure)
const PRODUCTION_REJECTION_REASONS: &[&str] = &[
    "at_capacity",
    "draining",
    "unhealthy",
    "invalid_request",
    "unspecified",
];

#[test]
fn success_arm_uses_gc_spellings_for_status_and_reason() {
    let snap = MetricAssertion::snapshot();
    record_meeting_assignment("success", None);

    // BOTH spellings are GC's, and both are load-bearing for the cross-service
    // read. `success` not `accepted`: a responder querying `status="success"`
    // against both series must not get a silently empty one on the MC side, and
    // no guard can catch that because both spellings are canonical in
    // `label-taxonomy.md`. `none` not an absent label and not an empty string:
    // mirrors GC's `rejection_reason.unwrap_or("none")`.
    snap.counter("mc_meeting_assignments_total")
        .with_labels(&[("status", "success"), ("rejection_reason", "none")])
        .assert_delta(1);
}

#[test]
fn every_production_rejection_reason_is_emitted_with_adjacency() {
    for reason in PRODUCTION_REJECTION_REASONS {
        let snap = MetricAssertion::snapshot();
        record_meeting_assignment("rejected", Some(reason));

        snap.counter("mc_meeting_assignments_total")
            .with_labels(&[("status", "rejected"), ("rejection_reason", *reason)])
            .assert_delta(1);

        // Adjacency: recording one reason must not move a sibling, so a
        // refactor collapsing the vocabulary into a single value fails here.
        for sibling in PRODUCTION_REJECTION_REASONS {
            if sibling == reason {
                continue;
            }
            snap.counter("mc_meeting_assignments_total")
                .with_labels(&[("status", "rejected"), ("rejection_reason", *sibling)])
                .assert_delta(0);
        }

        snap.counter("mc_meeting_assignments_total")
            .with_labels(&[("status", "success"), ("rejection_reason", "none")])
            .assert_delta(0);
    }
}
