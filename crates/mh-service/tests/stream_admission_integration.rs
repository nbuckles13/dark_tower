//! Component coverage for the egress-budget admission chain (story 2 R-19,
//! R-23; ADR-0036 §11; ADR-0032).
//!
//! Drives the REAL session actor over the config-apply mailbox — the same path
//! `RegisterMeeting` takes — and asserts, with `MetricAssertion`:
//!
//! - `mh_media_stream_admission_total{outcome}` counts one decision per
//!   generation-advancing apply, by outcome, with a positive control (the
//!   admitted count rises too, so a flat rejected count cannot be vacuous);
//! - stale and equal-generation applies are NOT admission decisions, and an
//!   equal-generation re-assert on a full handler is not refused;
//! - `mh_media_stream_admission_rejection_ratio` publishes the windowed ratio,
//!   0.0 from construction (never absent, never NaN);
//! - `mh_media_egress_edges` publishes installed egress streams, 0 from
//!   construction;
//! - every static gauge `publish_egress_admission` sets publishes EXACTLY the
//!   `EgressAdmission` / `PolicyLimits` field its consumer reads (gauge ==
//!   field — the anti-drift control and the ADR-0032 coverage in one
//!   assertion).
//!
//! The snapshot is taken BEFORE the session manager is built: the actor
//! resolves its handles at construction, and `MetricAssertion` binds a
//! thread-local recorder (the `#[tokio::test]` runtime is current-thread, so
//! the actor runs on this thread).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use common::observability::testing::MetricAssertion;
use mh_service::config::{EgressAdmission, EGRESS_STREAM_CEILING_RECOMMENDED_MIN};
use mh_service::observability::metrics::publish_egress_admission;
use mh_service::routing::MeetingPolicy;
use mh_service::session::{ApplyFailure, ApplyOutcome, SessionManagerHandle};
use mh_test_utils::admission::{fixture_policy_limits, with_ceiling};
use mh_test_utils::media_policy::{egress, register_request};
use mh_test_utils::session::apply_registered;

const KEY_CUSTODY: (&str, &str) = ("key_custody", "operator");

fn policy(meeting: &str, generation: u64, streams: u32) -> MeetingPolicy {
    let edges = (1..=streams)
        .map(|i| egress(i, i, 0, i + 1_000))
        .collect::<Vec<_>>();
    MeetingPolicy::from_request(
        &register_request(meeting, generation, edges),
        &fixture_policy_limits(),
    )
    .unwrap()
}

/// Register, then apply: the production order (see `mh_test_utils::session`).
async fn apply(sm: &SessionManagerHandle, policy: MeetingPolicy) -> ApplyOutcome {
    apply_registered(sm, policy).await
}

#[tokio::test]
async fn gauges_exist_at_zero_from_construction() {
    let snap = MetricAssertion::snapshot();
    let _sm = SessionManagerHandle::new(with_ceiling(4));
    snap.gauge("mh_media_stream_admission_rejection_ratio")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
    snap.gauge("mh_media_egress_edges")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
}

#[tokio::test]
async fn admission_is_counted_by_outcome_and_the_ratio_follows() {
    let snap = MetricAssertion::snapshot();
    let sm = SessionManagerHandle::new(with_ceiling(4));

    // Positive control: two admissions (3 streams, then +1 = 4, AT the ceiling).
    assert_eq!(apply(&sm, policy("m-a", 1, 3)).await, ApplyOutcome::Applied);
    assert_eq!(apply(&sm, policy("m-b", 1, 1)).await, ApplyOutcome::Applied);
    snap.gauge("mh_media_egress_edges")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(4.0);

    // Over the ceiling: refused whole, prior state stays live.
    let before = sm.routing_snapshot();
    assert_eq!(
        apply(&sm, policy("m-c", 1, 1)).await,
        ApplyOutcome::Failed(ApplyFailure::StreamCeilingExceeded)
    );
    assert!(std::sync::Arc::ptr_eq(&before, &sm.routing_snapshot()));

    snap.counter("mh_media_stream_admission_total")
        .with_labels(&[("outcome", "admitted"), KEY_CUSTODY])
        .assert_delta(2);
    snap.counter("mh_media_stream_admission_total")
        .with_labels(&[("outcome", "rejected_stream_ceiling"), KEY_CUSTODY])
        .assert_delta(1);
    // 1 rejected of 3 decisions in the window.
    snap.gauge("mh_media_stream_admission_rejection_ratio")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(1.0 / 3.0);
    snap.gauge("mh_media_egress_edges")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(4.0);
}

#[tokio::test]
async fn stale_and_equal_generation_applies_are_not_admission_decisions() {
    let snap = MetricAssertion::snapshot();
    let sm = SessionManagerHandle::new(with_ceiling(2));
    assert_eq!(apply(&sm, policy("m-a", 5, 2)).await, ApplyOutcome::Applied);

    // Full handler. An equal-generation re-assert is a no-op and can never
    // trip a capacity refusal; a stale one is rejected as stale, not capacity.
    assert_eq!(apply(&sm, policy("m-a", 5, 2)).await, ApplyOutcome::Applied);
    assert_eq!(
        apply(&sm, policy("m-a", 4, 2)).await,
        ApplyOutcome::RejectedStale
    );

    snap.counter("mh_media_stream_admission_total")
        .with_labels(&[("outcome", "admitted"), KEY_CUSTODY])
        .assert_delta(1);
    snap.counter("mh_media_stream_admission_total")
        .with_labels(&[("outcome", "rejected_stream_ceiling"), KEY_CUSTODY])
        .assert_delta(0);
}

#[tokio::test]
async fn a_shrinking_policy_releases_streams_and_the_gauge_falls() {
    let snap = MetricAssertion::snapshot();
    let sm = SessionManagerHandle::new(with_ceiling(3));
    assert_eq!(apply(&sm, policy("m-a", 1, 3)).await, ApplyOutcome::Applied);
    // Ordinary departure: MC re-pushes a smaller policy at a higher generation.
    assert_eq!(apply(&sm, policy("m-a", 2, 0)).await, ApplyOutcome::Applied);
    snap.gauge("mh_media_egress_edges")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
    // The released streams are admissible again.
    assert_eq!(apply(&sm, policy("m-b", 1, 3)).await, ApplyOutcome::Applied);
}

/// Every static gauge publishes the field its consumer reads — never a
/// re-conversion or a parallel constant. That includes every `PolicyLimits`
/// `_limit` gauge (story 2 R-21; ADR-0038 step 3), the policy-apply timeout
/// and both per-stream costs, read from the fields `MhMediaService` hands the
/// actor and admission enforces.
#[test]
fn static_admission_gauges_publish_the_fields_enforcement_reads() {
    let admission = EgressAdmission::derive(100_000_000, 90_001, 2_500_001, 0.05, 65_536).unwrap();
    // Deliberately distinct from the fixture's values, so a gauge that read
    // some OTHER source (a constant, the fixture) could not pass by accident.
    // EVERY field is set here, each to a value no other field (and no fixture)
    // carries, so a gauge wired to the wrong field cannot pass.
    let limits = mh_service::config::PolicyLimits {
        max_egress_streams_per_meeting: 777,
        max_candidate_sources_per_egress: 13,
        max_total_egress_edges: 4_321,
        policy_apply_timeout_ms: 2_500,
        max_registered_meetings: 1_234,
        max_muted_sources_per_meeting: 909,
    };
    let snap = MetricAssertion::snapshot();
    publish_egress_admission(&admission, &limits);

    // BYTES: the key is bits, converted once at load; the gauge reads the
    // converted field, so nothing downstream of load sees bits.
    snap.gauge("mh_media_egress_budget_bytes_per_second")
        .with_labels(&[("basis", "unmeasured"), KEY_CUSTODY])
        .assert_value(12_500_000.0);
    snap.gauge("mh_media_egress_budget_bytes_per_second")
        .with_labels(&[("basis", "unmeasured"), KEY_CUSTODY])
        .assert_value(admission.budget_bytes_per_second as f64);
    snap.gauge("mh_media_egress_stream_ceiling")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(f64::from(admission.stream_ceiling));
    snap.gauge("mh_media_egress_stream_ceiling_recommended_min")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(EGRESS_STREAM_CEILING_RECOMMENDED_MIN as f64);
    snap.gauge("mh_media_stream_admission_rejection_ratio_threshold")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(admission.rejection_ratio_threshold);
    snap.gauge("mh_media_egress_edges_limit")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(limits.max_total_egress_edges as f64);
    snap.gauge("mh_media_registered_meetings_limit")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(limits.max_registered_meetings as f64);
    snap.gauge("mh_media_egress_streams_per_meeting_limit")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(limits.max_egress_streams_per_meeting as f64);
    snap.gauge("mh_media_candidate_sources_per_egress_limit")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(limits.max_candidate_sources_per_egress as f64);
    snap.gauge("mh_media_muted_sources_per_meeting_limit")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(limits.max_muted_sources_per_meeting as f64);
    // SECONDS: the field is ms; 2_500 ms publishes 2.5.
    snap.gauge("mh_media_policy_apply_timeout_seconds")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(2.5);
    // BYTES, the ENFORCED costs (`div_ceil(8)` at load): 90_001 bits -> 11_251
    // and 2_500_001 bits -> 312_501 (a floor would give 11_250 / 312_500, so both
    // literals discriminate ceil from floor). The literals pin the conversion; the
    // field reads pin the wiring.
    snap.gauge("mh_media_stream_cost_audio_bytes_per_second")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(11_251.0);
    snap.gauge("mh_media_stream_cost_audio_bytes_per_second")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(admission.stream_cost_audio_bytes_per_second as f64);
    snap.gauge("mh_media_stream_cost_video_bytes_per_second")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(312_501.0);
    snap.gauge("mh_media_stream_cost_video_bytes_per_second")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(admission.stream_cost_video_bytes_per_second as f64);
}
