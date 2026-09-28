//! Server mute, end to end through MC's real WebTransport accept path (story 2
//! task 12; R-8..R-11; ADR-0036 §5, §7).
//!
//! Every participant joins over WebTransport with a real signed meeting token —
//! the host's carrying `MeetingRole::Host`, exactly as GC stamps it for the
//! meeting's creator — and reports its MH connections through the real
//! coordination service. What MC programs into MH is read off the recording
//! `MockMhRegistrationClient`, so these tests assert the snapshot MC would have
//! sent, field for field.
//!
//! Reconnect-survives / rejoin-does-not is pinned at the actor level
//! (`actors::meeting` tests): the WebTransport accept path does not yet wire
//! the binding-token reconnect, so an integration test here could only
//! exercise a fresh join.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use common::observability::testing::MetricAssertion;
use proto_gen::dark_tower::signaling::v1::{server_message, SlotState};
use test_common::media_session::{
    audio_slots, capability_frame, join_as, join_as_host, join_as_on, mute_frame,
    server_mute_frame, start_stack, unmute_request_frame, Session,
};
use test_common::{mh_handler, seed_meeting_with_handlers, seed_meeting_with_mh, TestStackHandles};

/// Every push MC has made for `meeting_id` to `endpoint`, in order.
fn pushes_to(
    stack: &TestStackHandles,
    meeting_id: &str,
    endpoint: &str,
) -> Vec<test_common::RegisterMeetingCall> {
    stack
        .mh_reg_client
        .calls()
        .into_iter()
        .filter(|c| c.meeting_id == meeting_id && c.mh_grpc_endpoint == endpoint)
        .collect()
}

fn muted_ids(call: &test_common::RegisterMeetingCall) -> Vec<u16> {
    call.assignment
        .server_muted_sources
        .iter()
        .map(|s| s.get().get())
        .collect()
}

/// Wait until a push to `endpoint` satisfies `predicate`; return it.
async fn push_until(
    stack: &TestStackHandles,
    meeting_id: &str,
    endpoint: &str,
    what: &str,
    predicate: impl Fn(&test_common::RegisterMeetingCall) -> bool,
) -> test_common::RegisterMeetingCall {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(call) = pushes_to(stack, meeting_id, endpoint)
            .into_iter()
            .rev()
            .find(|c| predicate(c))
        {
            return call;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what}: no push to {endpoint} matched; pushes: {:?}",
            pushes_to(stack, meeting_id, endpoint)
                .iter()
                .map(|c| (c.policy_generation, muted_ids(c)))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Host `h`, participants `a` and `b` on one handler, everyone hearing
/// everyone, settled.
async fn trio(
    rig: &test_common::accept_loop_rig::AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
) -> (Session, Session, Session) {
    seed_meeting_with_mh(stack, meeting_id).await;
    let mut h = join_as_host(rig, stack, meeting_id, "host").await;
    let mut a = join_as(rig, stack, meeting_id, "user-a").await;
    let mut b = join_as(rig, stack, meeting_id, "user-b").await;
    for s in [&mut h, &mut a, &mut b] {
        s.write(capability_frame(audio_slots(2))).await;
    }
    for s in [&mut h, &mut a, &mut b] {
        let _ = s.settle().await;
    }
    (h, a, b)
}

const MH: &str = "http://mh-test-1:50053";

// ============================================================================
// R-9: the muted set rides every snapshot, and a change advances the generation
// ============================================================================

#[tokio::test]
async fn a_server_mute_rides_every_snapshot_advances_the_generation_and_mutes_the_slot() {
    let (stack, rig) = start_stack("sm-snapshot").await;
    let (mut h, a, mut b) = trio(&rig, &stack, "sm-snapshot-m").await;
    let before = pushes_to(&stack, "sm-snapshot-m", MH)
        .last()
        .cloned()
        .unwrap();
    assert!(muted_ids(&before).is_empty(), "nobody is muted yet");

    // The host server-mutes A.
    let snap = MetricAssertion::snapshot();
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let muted = push_until(&stack, "sm-snapshot-m", MH, "the mute is programmed", |c| {
        muted_ids(c) == vec![a.sender_id as u16]
    })
    .await;
    assert_eq!(
        muted.policy_generation,
        before.policy_generation + 1,
        "a change to the muted set alone advances the generation by exactly one"
    );
    assert_eq!(
        muted.assignment.egress_streams, before.assignment.egress_streams,
        "the mute changed nothing but the muted set"
    );
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "applied"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);

    // B, who holds A, is told A's slot is SOURCE_MUTED (the R-4 third driver).
    let view = b
        .assignments_until("B sees A muted", |v| {
            v.assignments.iter().any(|s| {
                s.sender_id == Some(a.sender_id) && s.slot_state == SlotState::SourceMuted as i32
            })
        })
        .await;
    assert!(view
        .assignments
        .iter()
        .any(|s| s.sender_id == Some(a.sender_id)));

    // ANOTHER structural change (a new joiner): the next snapshot STILL
    // carries the mute — it rides every full registration, not only the push
    // the mute caused.
    let mut c = join_as(&rig, &stack, "sm-snapshot-m", "user-c").await;
    c.write(capability_frame(audio_slots(2))).await;
    let _ = c.settle().await;
    let later = push_until(&stack, "sm-snapshot-m", MH, "a later snapshot", |p| {
        p.policy_generation > muted.policy_generation
    })
    .await;
    assert_eq!(
        muted_ids(&later),
        vec![a.sender_id as u16],
        "the mute survives a later snapshot (and so a re-assert)"
    );

    // The host lifts it: the next snapshot drops A and the generation advances.
    let before_unmute = pushes_to(&stack, "sm-snapshot-m", MH)
        .last()
        .cloned()
        .unwrap();
    h.write(server_mute_frame(&a.participant_id, false)).await;
    let unmuted = push_until(
        &stack,
        "sm-snapshot-m",
        MH,
        "the unmute is programmed",
        |p| p.policy_generation > before_unmute.policy_generation && muted_ids(p).is_empty(),
    )
    .await;
    assert_eq!(
        unmuted.policy_generation,
        before_unmute.policy_generation + 1
    );
}

/// MH's ingress-drop set is SERVER mute only: a participant who muted THEMSELF
/// is never in it, because MH drops at ingress and the participant has no way
/// to lift a server mute (plan §1). The self mute still reaches the slot view.
///
/// Discriminating without a timed silence: after A self-mutes, the host mutes B,
/// and the push that follows must carry EXACTLY `[B]` at one generation past the
/// baseline. If self mute leaked into the set, every later push would carry A
/// too, so `[B]` alone would never appear.
#[tokio::test]
async fn a_self_muted_participant_is_never_in_the_mh_muted_set() {
    let (stack, rig) = start_stack("sm-self").await;
    let (mut h, mut a, mut b) = trio(&rig, &stack, "sm-self-m").await;
    let baseline = pushes_to(&stack, "sm-self-m", MH).last().cloned().unwrap();
    assert!(muted_ids(&baseline).is_empty(), "nobody is muted yet");

    // A mutes itself; B's slot view shows it (so the self mute was processed).
    a.write(mute_frame(true, false)).await;
    let _ = b
        .assignments_until("B sees A self-muted", |v| {
            v.assignments.iter().any(|s| {
                s.sender_id == Some(a.sender_id) && s.slot_state == SlotState::SourceMuted as i32
            })
        })
        .await;

    // The host server-mutes B: the set is exactly B, one generation on.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let only_b = push_until(&stack, "sm-self-m", MH, "the host's mute of B", |c| {
        muted_ids(c) == vec![b.sender_id as u16]
    })
    .await;
    assert_eq!(
        only_b.policy_generation,
        baseline.policy_generation + 1,
        "A's self mute consumed no generation: MH's snapshot did not change"
    );
    assert!(
        pushes_to(&stack, "sm-self-m", MH)
            .iter()
            .all(|c| !muted_ids(c).contains(&(a.sender_id as u16))),
        "a self-muted participant must never be programmed into MH's muted set"
    );

    // Positive control: the host server-muting A DOES put A in the set.
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let mut both = vec![a.sender_id as u16, b.sender_id as u16];
    both.sort_unstable();
    let with_a = push_until(&stack, "sm-self-m", MH, "the host's mute of A", |c| {
        muted_ids(c) == both
    })
    .await;

    // Lifting A's server mute takes A out of MH's set even though A is still
    // self-muted: clearing one mute neither clears nor promotes the other.
    h.write(server_mute_frame(&a.participant_id, false)).await;
    let lifted = push_until(&stack, "sm-self-m", MH, "the lift of A", |c| {
        c.policy_generation > with_a.policy_generation
    })
    .await;
    assert_eq!(muted_ids(&lifted), vec![b.sender_id as u16]);
}

// ============================================================================
// R-9 per-handler filter over a split meeting
// ============================================================================

/// Handler `mh-0` carries only the edges whose parties share it, and its
/// snapshot carries only the muted senders sourcing one of THOSE edges.
#[tokio::test]
async fn each_handler_is_programmed_with_only_the_muted_senders_sourcing_its_edges() {
    let (stack, rig) = start_stack("sm-split").await;
    seed_meeting_with_handlers(
        &stack,
        "sm-split-m",
        vec![mh_handler("mh-0"), mh_handler("mh-1")],
    )
    .await;
    // The host is on both handlers; A only on mh-0; B only on mh-1. So A's
    // edge to the host is on mh-0 and B's on mh-1.
    let mut h = join_as_host(&rig, &stack, "sm-split-m", "host").await;
    let mut a = join_as_on(&rig, &stack, "sm-split-m", "user-a", &["mh-0"]).await;
    let mut b = join_as_on(&rig, &stack, "sm-split-m", "user-b", &["mh-1"]).await;
    for s in [&mut h, &mut a, &mut b] {
        s.write(capability_frame(audio_slots(2))).await;
    }
    for s in [&mut h, &mut a, &mut b] {
        let _ = s.settle().await;
    }

    h.write(server_mute_frame(&a.participant_id, true)).await;
    let on_mh0 = push_until(
        &stack,
        "sm-split-m",
        "http://mh-0:50053",
        "mh-0 mutes A",
        |c| muted_ids(c) == vec![a.sender_id as u16],
    )
    .await;
    assert!(!on_mh0.assignment.server_muted_sources.is_empty());

    // Positive control on mh-1, instead of waiting out a silence: mute B, who
    // DOES source an edge there. mh-1's next snapshot must carry B — and only
    // B. A, muted all along, is absent from it: A sources no edge on mh-1, and
    // connectivity alone never puts a sender in a handler's set.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let on_mh1 = push_until(
        &stack,
        "sm-split-m",
        "http://mh-1:50053",
        "mh-1 mutes B",
        |c| !muted_ids(c).is_empty(),
    )
    .await;
    assert_eq!(muted_ids(&on_mh1), vec![b.sender_id as u16]);
    assert!(
        pushes_to(&stack, "sm-split-m", "http://mh-1:50053")
            .iter()
            .all(|c| !muted_ids(c).contains(&(a.sender_id as u16))),
        "no snapshot to mh-1 ever named A"
    );
}

// ============================================================================
// R-8: authority over the wire
// ============================================================================

#[tokio::test]
async fn a_non_host_server_mute_is_refused_and_programs_nothing() {
    let (stack, rig) = start_stack("sm-nonhost").await;
    let (mut h, mut a, b) = trio(&rig, &stack, "sm-nonhost-m").await;
    let last_before = pushes_to(&stack, "sm-nonhost-m", MH)
        .last()
        .cloned()
        .unwrap();

    let snap = MetricAssertion::snapshot();
    a.write(server_mute_frame(&b.participant_id, true)).await;
    assert_eq!(a.expect_error().await, "Server mute request refused");

    // Positive control rather than a timed silence: the host now mutes B. That
    // push must be EXACTLY one generation past the last one before the refused
    // request — so the refusal consumed no generation and programmed nothing.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let muted = push_until(&stack, "sm-nonhost-m", MH, "the host's mute", |c| {
        muted_ids(c) == vec![b.sender_id as u16]
    })
    .await;
    assert_eq!(
        muted.policy_generation,
        last_before.policy_generation + 1,
        "the refused request was not a structural change"
    );
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "not_permitted"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// ============================================================================
// R-11: who-muted-whom on the wire — the target, and a late joiner
// ============================================================================

/// The muted participant itself learns it is server-muted, and by whom — so
/// its client can show that it cannot clear the mute itself.
#[tokio::test]
async fn the_muted_participant_is_told_it_is_server_muted_and_by_whom() {
    let (stack, rig) = start_stack("sm-target").await;
    let (mut h, mut a, _b) = trio(&rig, &stack, "sm-target-m").await;
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let update = a.expect_mute_update_for(&a.participant_id.clone()).await;
    assert!(update.audio_server_muted);
    assert_eq!(update.server_muted_by, h.participant_id);
}

/// A joiner arriving AFTER a mute is told about it immediately — the first
/// message after its `JoinResponse` — not at the next mute change.
#[tokio::test]
async fn a_late_joiner_is_told_who_is_server_muted_right_after_its_join_response() {
    let (stack, rig) = start_stack("sm-late").await;
    let (mut h, a, _b) = trio(&rig, &stack, "sm-late-m").await;
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let _ = push_until(&stack, "sm-late-m", MH, "the mute landed", |c| {
        muted_ids(c) == vec![a.sender_id as u16]
    })
    .await;

    let mut late = join_as(&rig, &stack, "sm-late-m", "user-late").await;
    let first = late
        .next_server_message()
        .await
        .expect("a message follows the JoinResponse");
    let Some(server_message::Message::ParticipantMuteUpdate(update)) = first.message else {
        panic!("the replay is the FIRST thing after the JoinResponse, got {first:?}");
    };
    assert_eq!(update.participant_id, a.participant_id);
    assert!(update.audio_server_muted);
    assert_eq!(update.server_muted_by, h.participant_id);
}

// ============================================================================
// R-10: an unmute request notifies the host and never clears the mute
// ============================================================================

/// A muted participant asks to be unmuted. The host receives the request
/// carrying the REQUESTER's authenticated id (the id the client wrote — here
/// a forgery naming B — is overwritten), the request is counted `relayed`,
/// and the mute stays in force: the pod gauge still reports it and no snapshot
/// drops it.
#[tokio::test]
async fn an_unmute_request_reaches_the_host_as_the_requester_and_leaves_the_mute_in_force() {
    let (stack, rig) = start_stack("sm-unmute").await;
    let (mut h, mut a, b) = trio(&rig, &stack, "sm-unmute-m").await;
    // Opened before the mute: the gauge is a level SET on each change, so the
    // window must contain the set.
    let snap = MetricAssertion::snapshot();
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let muted = push_until(&stack, "sm-unmute-m", MH, "the mute landed", |c| {
        muted_ids(c) == vec![a.sender_id as u16]
    })
    .await;

    // The pod-level gauge reports the one mute in force.
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(1.0);

    a.write(unmute_request_frame(&b.participant_id)).await;
    let relayed = loop {
        let msg = h
            .next_server_message()
            .await
            .expect("the host is sent the unmute request");
        if let Some(server_message::Message::UnmuteRequest(r)) = msg.message {
            break r;
        }
    };
    assert_eq!(
        relayed.participant_id, a.participant_id,
        "the relay names the authenticated requester, never the client's claim"
    );
    assert!(relayed.request_audio);
    snap.counter("mc_media_unmute_requests_total")
        .with_labels(&[("outcome", "relayed"), ("key_custody", "operator")])
        .assert_delta(1);

    // Still muted: the gauge is unchanged and MC pushed nothing new.
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(1.0);
    let last = pushes_to(&stack, "sm-unmute-m", MH)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(last.policy_generation, muted.policy_generation);
    assert_eq!(muted_ids(&last), vec![a.sender_id as u16]);

    // Only the host lifts it, and the gauge follows.
    h.write(server_mute_frame(&a.participant_id, false)).await;
    let _ = push_until(&stack, "sm-unmute-m", MH, "the unmute landed", |c| {
        c.policy_generation > muted.policy_generation && muted_ids(c).is_empty()
    })
    .await;
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(0.0);
}

// ============================================================================
// Refusal replies are bounded; every refusal is still counted
// ============================================================================

/// A non-host sending server-mute requests past the refusal-reply burst gets
/// no further replies — but every request is still counted `not_permitted`,
/// and each withheld reply is counted on the suppression series.
#[tokio::test]
async fn refusal_replies_past_the_burst_are_withheld_and_counted() {
    /// One more than the reply burst (`SERVER_MUTE_REFUSAL_REPLY_BURST` = 4).
    const SENT: u64 = 5;
    let (stack, rig) = start_stack("sm-suppress").await;
    let (_h, mut a, b) = trio(&rig, &stack, "sm-suppress-m").await;

    let snap = MetricAssertion::snapshot();
    for _ in 0..SENT {
        a.write(server_mute_frame(&b.participant_id, true)).await;
    }
    // Wait until every request is processed (counted), not for a silence.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while snap
        .counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "not_permitted"),
            ("key_custody", "operator"),
        ])
        .delta()
        < SENT
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "not every refused request was counted"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    snap.counter("mc_media_refusal_replies_suppressed_total")
        .with_labels(&[("surface", "server_mute"), ("key_custody", "operator")])
        .assert_delta(1);
    snap.counter("mc_media_refusal_replies_suppressed_total")
        .with_labels(&[("surface", "capability"), ("key_custody", "operator")])
        .assert_delta(0);

    let mut replies = 0;
    while let Some(msg) = a.next_server_message().await {
        if matches!(msg.message, Some(server_message::Message::Error(_))) {
            replies += 1;
        }
    }
    assert_eq!(replies, SENT - 1, "exactly the burst is answered");
}
