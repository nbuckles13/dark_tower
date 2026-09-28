//! Session-actor fixtures for `mh-service`'s `tests/` binaries.
//!
//! # Why tests go through [`apply_registered`] rather than `apply_policy` alone
//!
//! MH's session actor refuses to install a policy for a meeting that is not
//! registered (the released-meeting guard, story 2 R-20: an apply queued before
//! an `EndMeeting` and run after it must install nothing). Production can never
//! reach `apply_policy` without a registration, because the `RegisterMeeting`
//! handler upserts the registration BEFORE it queues the apply. A test that
//! calls `SessionManagerHandle::apply_policy` on its own skips that upsert, so
//! it would be exercising a path production does not have, and would now be
//! refused as `RejectedStale`.
//!
//! [`apply_registered`] restores the production order in one place: upsert,
//! then apply. The registration carries the same `mc_id` and endpoint the
//! [`crate::media_policy`] builders stamp on every fixture request, so a test
//! that later drives `RegisterMeeting` or `EndMeeting` for the same meeting
//! agrees with it on ownership.

use std::time::{Duration, Instant};

use mh_service::routing::MeetingPolicy;
use mh_service::session::{ApplyOutcome, MeetingRegistration, SessionManagerHandle};

use crate::admission::fixture_policy_limits;
use crate::media_policy::{TEST_MC_ENDPOINT, TEST_MC_ID};

/// Register `policy`'s meeting on `sm` as the fixture MC, then apply `policy`
/// with the fixture limits, in the order the `RegisterMeeting` handler uses.
///
/// Registration is an idempotent upsert, so this is safe to call for every
/// apply in a sequence (re-asserts, new generations) on the same meeting.
///
/// # Panics
///
/// If the registration is refused (the meeting cap). A fixture that hits the
/// cap is misconfigured, and a silent refusal would surface later as a
/// confusing `RejectedStale` rather than at the cause.
pub async fn apply_registered(sm: &SessionManagerHandle, policy: MeetingPolicy) -> ApplyOutcome {
    let limits = fixture_policy_limits();
    sm.register_meeting(
        policy.meeting.as_str().to_string(),
        MeetingRegistration {
            mc_id: TEST_MC_ID.to_string(),
            mc_grpc_endpoint: TEST_MC_ENDPOINT.to_string(),
            registered_at: Instant::now(),
        },
        limits.max_registered_meetings,
    )
    .await
    .unwrap_or_else(|refusal| panic!("fixture registration refused before apply: {refusal:?}"));
    sm.apply_policy(
        policy,
        limits.max_total_egress_edges,
        Duration::from_millis(limits.policy_apply_timeout_ms),
    )
    .await
}
