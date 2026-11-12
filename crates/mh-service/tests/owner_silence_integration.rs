//! Story 4 MH-3: the owner-silence release (ADR-0039 §6, user ruling
//! 2026-10-31; R-11; `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md`
//! "MH").
//!
//! MH releases a meeting whose OWNING MC has not communicated for
//! `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`. Only a record-valid
//! `RegisterMeeting` and an OWNED verdict for the current registration reset
//! the timer; an owner `EndMeeting` / RELEASED ends the meeting outright.
//! Every other input — transport errors, timeouts, `FAILED_PRECONDITION`,
//! `UNSPECIFIED` and unknown verdicts, a record-invalid registration, a
//! non-owner `EndMeeting`, a verdict for a superseded registration — must
//! NOT reset it.
//!
//! The rig is the production actor + sender task over the fake MC
//! (`common::heartbeat_rig`), with the REAL config floor (300 s) under paused
//! time. Every meeting below is registered at the same instant `t0`; the fake
//! MC answers with an error by default, so beats themselves never reset.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use common::heartbeat_rig::*;
use common::paused_time::{settle, step_until};
use mh_test_utils::media_policy::{egress, register_request, TEST_HEARTBEAT_INTERVAL, TEST_MC_ID};
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService as _;
use proto_gen::dark_tower::internal::v1::ReleaseCause;
use tokio::sync::Notify;
use tokio::time::Instant;

/// One millisecond: "just before" an exact deadline.
const EPS: Duration = Duration::from_millis(1);
const TRANSPORT_ERROR: Reply = Reply::Status(tonic::Code::Unavailable);

/// Advance paused time by `by` in one jump, then let everything run. Owner
/// silence fires on its own timer, so a single jump lands exactly on it.
async fn advance(by: Duration) {
    tokio::time::advance(by).await;
    settle().await;
}

/// Advance to `target` (relative to `t0`).
async fn advance_to(t0: Instant, target: Duration) {
    let now = Instant::now() - t0;
    assert!(target >= now, "time only moves forward");
    advance(target - now).await;
}

async fn held(rig: &Rig, meeting: &str) -> bool {
    rig.sm.is_meeting_registered(meeting).await
}

/// O1: the release fires EXACTLY at the timeout, through the release path:
/// the meeting's connection is closed (its close token, the
/// `HANDLER_RELEASED` path, which sends no Disconnected), and it is counted
/// `trigger="owner_silence"` with `clients` from the one predicate. Nothing
/// is counted on the teardown series.
#[tokio::test(start_paused = true)]
async fn it_fires_exactly_at_the_timeout_and_releases_like_released() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let t0 = Instant::now();
    for m in ["os-present", "os-none"] {
        rig.mc.reply(m, TRANSPORT_ERROR);
        register(&rig, plain(m)).await.unwrap();
    }
    // One BOUND session (the common live case) and one still awaiting its
    // bind: owner silence closes both.
    let conn = connection("c-1", "p-1");
    let close = conn.close.clone();
    admit(&rig.sm, "os-present", conn).await;
    let bound = connection("c-bound", "p-bound");
    let bound_close = bound.close.clone();
    admit(&rig.sm, "os-present", bound).await;
    assert_eq!(
        rig.sm
            .bind_connection(
                "os-present",
                "c-bound",
                mh_service::routing::SenderId::from_wire(7).unwrap()
            )
            .await,
        mh_service::session::BindOutcome::Bound
    );

    advance_to(t0, OWNER_SILENCE - EPS).await;
    assert!(held(&rig, "os-present").await && held(&rig, "os-none").await);
    assert!(!close.is_cancelled(), "nothing closed before the timeout");
    assert!(
        !bound_close.is_cancelled(),
        "the bound session is open before T"
    );
    for clients in ["present", "none"] {
        snap.counter(RELEASES)
            .with_labels(&[("trigger", "owner_silence"), ("clients", clients), KC])
            .assert_delta(0);
    }

    advance(EPS).await;
    assert!(!held(&rig, "os-present").await, "released at exactly T");
    assert!(!held(&rig, "os-none").await);
    assert!(close.is_cancelled(), "its awaiting session closes");
    // The BOUND session's close ends as `ConnectionEnd::MeetingReleased`:
    // HANDLER_RELEASED and no Disconnected (wire evidence: D6).
    assert!(bound_close.is_cancelled(), "its BOUND session closes");
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "owner_silence"), ("clients", "present"), KC])
        .assert_delta(1);
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "owner_silence"), ("clients", "none"), KC])
        .assert_delta(1);
    snap.counter(TEARDOWNS)
        .with_labels(&[("outcome", "released"), KC])
        .assert_delta(0);
}

/// O2: each owner message type resets the timer — a record-valid
/// RegisterMeeting (also one whose POLICY is refused), an OWNED verdict, and a
/// takeover registration from another `mc_id` (which then owns the timer).
/// Each meeting is held past `T` and released exactly `T` after its reset.
/// Positive control: an un-reset meeting fires at `T`.
#[tokio::test(start_paused = true)]
async fn each_owner_message_resets_the_timer() {
    let rig = rig();
    let t0 = Instant::now();
    let meetings = [
        "reset-register",
        "reset-policy-refused",
        "reset-owned",
        "reset-takeover",
        "control",
    ];
    for m in meetings {
        rig.mc.reply(m, TRANSPORT_ERROR);
        register(&rig, plain(m)).await.unwrap();
    }

    let reset_at = Duration::from_secs(200);
    advance_to(t0, reset_at).await;
    // Owner re-registration.
    register(&rig, register_request("reset-register", 2, Vec::new()))
        .await
        .unwrap();
    // Record-valid, policy-refused (a duplicate egress_stream_id): the record
    // is upserted, so it is the owner communicating.
    let refused = register(
        &rig,
        register_request(
            "reset-policy-refused",
            2,
            vec![egress(1, 2, 0, 3), egress(1, 2, 1, 3)],
        ),
    )
    .await;
    assert!(refused.is_err(), "the policy is refused");
    // Takeover from another MC.
    let mut takeover = plain("reset-takeover");
    takeover.mc_id = "mc-b".to_string();
    register(&rig, takeover).await.unwrap();
    // OWNED: answer exactly one beat with OWNED, then errors again.
    rig.mc.reply("reset-owned", Reply::Owned);
    let before = rig.mc.count("reset-owned");
    step_until(
        || {
            let beat = rig.mc.count("reset-owned") > before;
            async move { beat }
        },
        TEST_HEARTBEAT_INTERVAL * 2,
        "reset-owned's OWNED beat",
    )
    .await;
    rig.mc.reply("reset-owned", TRANSPORT_ERROR);
    let owned_at = rig.mc.calls_for("reset-owned").last().unwrap().0 - t0;

    advance_to(t0, OWNER_SILENCE).await;
    assert!(!held(&rig, "control").await, "positive control fires at T");
    for m in &meetings[..4] {
        assert!(held(&rig, m).await, "{m}: reset, so still held at T");
    }

    advance_to(t0, reset_at + OWNER_SILENCE - EPS).await;
    for m in ["reset-register", "reset-policy-refused", "reset-takeover"] {
        assert!(held(&rig, m).await, "{m}: held until reset + T");
    }
    advance(EPS).await;
    for m in ["reset-register", "reset-policy-refused", "reset-takeover"] {
        assert!(!held(&rig, m).await, "{m}: released at exactly reset + T");
    }

    advance_to(t0, owned_at + OWNER_SILENCE - EPS).await;
    assert!(held(&rig, "reset-owned").await);
    advance(EPS).await;
    assert!(
        !held(&rig, "reset-owned").await,
        "released exactly T after the OWNED beat"
    );
}

/// O3: nothing else resets the timer. Every meeting here gets its non-reset
/// input mid-window and is still released at exactly `T`.
#[tokio::test(start_paused = true)]
async fn no_other_input_resets_the_timer() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let t0 = Instant::now();
    let replies = [
        ("nr-transport-error", TRANSPORT_ERROR),
        ("nr-timeout", Reply::Hang),
        (
            "nr-failed-precondition",
            Reply::Status(tonic::Code::FailedPrecondition),
        ),
        ("nr-unspecified", Reply::Verdict(0)),
        ("nr-unknown-verdict", Reply::Verdict(99)),
        ("nr-invalid-register", TRANSPORT_ERROR),
        ("nr-foreign-end", TRANSPORT_ERROR),
    ];
    for (m, reply) in &replies {
        rig.mc.reply(m, reply.clone());
        register(&rig, plain(m)).await.unwrap();
    }

    advance_to(t0, Duration::from_secs(200)).await;
    // A record-INVALID registration from the owner (interval 0) is refused
    // before the actor.
    let mut invalid = plain("nr-invalid-register");
    invalid.heartbeat_interval_ms = 0;
    assert!(register(&rig, invalid).await.is_err());
    // EndMeeting from an MC that does not own the meeting: refused, nothing
    // released, and no reset.
    let foreign = rig
        .svc
        .end_meeting(end(
            "nr-foreign-end",
            "mc-not-owner",
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await;
    assert_eq!(foreign.unwrap_err().code(), tonic::Code::FailedPrecondition);
    assert!(held(&rig, "nr-foreign-end").await);

    advance_to(t0, OWNER_SILENCE - EPS).await;
    for (m, _) in &replies {
        assert!(held(&rig, m).await, "{m}: held before T");
    }
    advance(EPS).await;
    for (m, _) in &replies {
        assert!(
            !held(&rig, m).await,
            "{m}: NOT reset, released at exactly T"
        );
    }
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "owner_silence"), ("clients", "none"), KC])
        .assert_delta(u64::try_from(replies.len()).unwrap());
}

/// Security (ii): a verdict whose beat was built for a SUPERSEDED registration
/// never refreshes the new owner's timer. A beat is held in flight across a
/// takeover (new `mc_id` and endpoint, hence a new fence) and answered
/// afterwards: the takeover reset the timer, the stale verdict does not, so
/// the release lands exactly `T` after the takeover. Run for a stale OWNED and
/// a stale RELEASED (which also must not end the new registration).
#[tokio::test(start_paused = true)]
async fn a_verdict_for_a_superseded_registration_never_resets_the_new_owner() {
    stale_verdict_case(Reply::Owned, "owned").await;
    stale_verdict_case(
        Reply::Released(ReleaseCause::HeartbeatTimeout as i32),
        "released_stale",
    )
    .await;
}

async fn stale_verdict_case(inner: Reply, counted_as: &str) {
    const M: &str = "stale";
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let t0 = Instant::now();
    let hold = Arc::new(Notify::new());
    rig.mc
        .reply(M, Reply::HoldThen(Arc::clone(&hold), Box::new(inner)));
    register(&rig, plain(M)).await.unwrap();
    // Step until the first (jittered) beat is out and held.
    step_until(
        || {
            let out = rig.mc.count(M) > 0;
            async move { out }
        },
        TEST_HEARTBEAT_INTERVAL * 2,
        "the stale case's first beat",
    )
    .await;
    let takeover_at = Instant::now() - t0;
    let mut takeover = plain(M);
    takeover.mc_id = "mc-b".to_string();
    takeover.mc_grpc_endpoint = "http://mc-b:50052".to_string();
    register(&rig, takeover).await.unwrap();
    rig.mc.reply(M, TRANSPORT_ERROR);
    // Answer the in-flight beat 1 s AFTER the takeover (inside its 1.5 s
    // per-attempt deadline), so a wrongful reset would move the release.
    advance(Duration::from_secs(1)).await;
    hold.notify_waiters();
    settle().await;
    assert!(
        held(&rig, M).await,
        "{counted_as}: never ends the new registration"
    );
    snap.counter(VERDICTS)
        .with_labels(&[("outcome", counted_as)])
        .assert_delta(1);

    advance_to(t0, takeover_at + OWNER_SILENCE - EPS).await;
    assert!(held(&rig, M).await, "{counted_as}: held until takeover + T");
    advance(EPS).await;
    assert!(
        !held(&rig, M).await,
        "{counted_as}: released exactly T after the takeover (the stale verdict did not reset)"
    );
}

/// An owner `EndMeeting` ends the meeting by the message, never by silence
/// later (owner-silence unobserved for it). Positive control: a sibling
/// meeting fires at `T`.
#[tokio::test(start_paused = true)]
async fn an_owner_end_meeting_releases_by_the_message_not_by_silence() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    let t0 = Instant::now();
    for m in ["ended", "sibling"] {
        rig.mc.reply(m, TRANSPORT_ERROR);
        register(&rig, plain(m)).await.unwrap();
    }
    advance_to(t0, Duration::from_secs(100)).await;
    rig.svc
        .end_meeting(end(
            "ended",
            TEST_MC_ID,
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await
        .unwrap();
    assert!(!held(&rig, "ended").await);
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "planned_transfer"), ("clients", "none"), KC])
        .assert_delta(1);

    advance_to(t0, OWNER_SILENCE).await;
    assert!(!held(&rig, "sibling").await, "positive control");
    snap.counter(RELEASES)
        .with_labels(&[("trigger", "owner_silence"), ("clients", "none"), KC])
        .assert_delta(1);
}

/// The two story-4 MH-3 config gauges (ADR-0032): each publishes its configured
/// value in seconds (the idle timeout converted from ms once, at publish), with
/// `key_custody`. Non-deployed values, so a constant cannot pass.
#[test]
fn the_liveness_config_gauges_publish_the_configured_values() {
    let snapshot = MetricAssertion::snapshot();
    mh_service::observability::metrics::publish_liveness_config(Duration::from_secs(1_234), 4_321);
    snapshot
        .gauge("mh_media_meeting_owner_silence_timeout_seconds")
        .with_labels(&[KC])
        .assert_value(1_234.0);
    snapshot
        .gauge("mh_media_transport_idle_timeout_seconds")
        .with_labels(&[KC])
        .assert_value(4.321);
}
