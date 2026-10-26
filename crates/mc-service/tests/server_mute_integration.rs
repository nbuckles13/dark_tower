//! Server mute and client mute, end to end through MC's real WebTransport
//! accept path (story 2 task 12, R-8..R-11; story 3 MC-B, R-15..R-18;
//! ADR-0036 §5, §7).
//!
//! Every participant joins over WebTransport with a real signed meeting token —
//! the host's carrying `MeetingRole::Host`, exactly as GC stamps it for the
//! meeting's creator — and reports its MH connections through the real
//! coordination service. What MC programs into MH is read off the recording
//! `MockMhRegistrationClient`, so these tests assert the snapshot MC would have
//! sent, field for field.
//!
//! Story 3 MC-B: there is no sender-level muted set any more; a server mute is
//! the muted KIND's stream declared `DROP_AT_INGRESS` in `source_streams`
//! (edges KEPT), its send directive withdrawn (empty targets), and its holders
//! told `AVATAR_SERVER_MUTED`. A client mute is `PAUSED`, never a drop, and
//! never touches the directive.
//!
//! Reconnect-survives / rejoin-does-not is pinned at the actor level
//! (`actors::meeting` tests): the WebTransport accept path does not yet wire
//! the binding-token reconnect, so an integration test here could only
//! exercise a fresh join.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::media_kind::MediaKindLabel;
use mc_service::media_routing::SourceDispositionPlan as Disposition;
use mc_service::media_signaling::{
    MuteOutcome, MuteRejection, ServerMuteAction, ServerMuteOutcome, UnmuteRequestOutcome,
};
use mc_service::observability::metrics::JOIN_FAILURE_ERROR_TYPES;
use proto_gen::dark_tower::signaling::v1::{
    server_message, ClientMediaState, ErrorMessage, MediaKind, SendDirective, SendStream,
    ServerMessage, SlotState, StreamAssignment, StreamAssignments,
};
use test_common::media_session::{
    audio_slots, capability_frame, join_as, join_as_host, join_shaped, mute_changes_frame,
    mute_frame, mute_kinds_frame, server_mute_changes_frame, server_mute_frame,
    server_mute_kinds_frame, slot, start_stack, try_join_shaped, unmute_kinds_frame,
    unmute_request_frame, JoinShape, Session,
};
use test_common::{seed_meeting_with_mh, TestStackHandles};

const MH: &str = "http://mh-test-1:50053";

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

/// The disposition MC declared for (`sender`, `stream`) in this push.
fn disposition(
    call: &test_common::RegisterMeetingCall,
    sender: u32,
    stream: u8,
) -> Option<Disposition> {
    call.assignment
        .source_streams
        .iter()
        .find(|s| u32::from(s.sender.get().get()) == sender && s.stream_number == stream)
        .map(|s| s.disposition)
}

/// Every (sender, stream) this push declares `DROP_AT_INGRESS` — the
/// server-muted streams, the ONLY server-mute carrier to MH since MC-B.
fn dropped(call: &test_common::RegisterMeetingCall) -> Vec<(u32, u8)> {
    call.assignment
        .source_streams
        .iter()
        .filter(|s| s.disposition == Disposition::DropAtIngress)
        .map(|s| (u32::from(s.sender.get().get()), s.stream_number))
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
                .map(|c| (c.policy_generation, dropped(c)))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Host `h`, participants `a` and `b` on one handler, audio only, everyone
/// hearing everyone, settled.
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

/// Host `h`, participants `a` and `b` on one handler, EACH publishing audio
/// and video and declaring two audio slots (0, 1) and two video slots (10,
/// 11): everyone holds everyone on both kinds. Settled; returns each
/// participant's last directive and assignments alongside it.
async fn av_trio(
    rig: &test_common::accept_loop_rig::AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
) -> [(Session, SendDirective, StreamAssignments); 3] {
    seed_meeting_with_mh(stack, meeting_id).await;
    let shape = || {
        JoinShape::publishing(&[MediaKind::Audio, MediaKind::VideoCamera]).receiving(vec![
            slot(0, MediaKind::Audio),
            slot(1, MediaKind::Audio),
            slot(10, MediaKind::VideoCamera),
            slot(11, MediaKind::VideoCamera),
        ])
    };
    let mut sessions = [
        join_shaped(rig, stack, meeting_id, "host", true, shape()).await,
        join_shaped(rig, stack, meeting_id, "user-a", false, shape()).await,
        join_shaped(rig, stack, meeting_id, "user-b", false, shape()).await,
    ];
    let mut views = Vec::new();
    for s in &mut sessions {
        let (d, a) = s.settle().await;
        views.push((
            d.or_else(|| s.join_directive.clone()).expect("a directive"),
            a.or_else(|| s.join_assignments.clone())
                .expect("assignments"),
        ));
    }
    let [h, a, b] = sessions;
    let mut views = views.into_iter();
    let mut next = || views.next().unwrap();
    let (hv, av, bv) = (next(), next(), next());
    [(h, hv.0, hv.1), (a, av.0, av.1), (b, bv.0, bv.1)]
}

use mc_test_utils::media::{
    client_state_of as client_of, client_states_av as clients, server_mute_of as server_of,
};

const LIVE: ClientMediaState = ClientMediaState::Live;
const MUTED: ClientMediaState = ClientMediaState::ClientMuted;
const NOT_PUBLISHED: ClientMediaState = ClientMediaState::NotPublished;

/// The directive's stream for `kind` (audio 0, video 1).
fn stream_of(d: &SendDirective, kind: MediaKindLabel) -> &SendStream {
    let number = kind.of([0, 1]);
    d.streams
        .iter()
        .find(|s| s.stream_number == number)
        .expect("the published kind is listed")
}

/// The slot assignments of `kind` (by declared slot id: audio < 10 <= video).
fn slots_of(a: &StreamAssignments, kind: MediaKindLabel) -> Vec<StreamAssignment> {
    a.assignments
        .iter()
        .filter(|s| kind.of([s.slot_id < 10, s.slot_id >= 10]))
        .cloned()
        .collect()
}

/// A server-mute request about ONE kind (the other absent: unchanged).
fn server_mute_one(target: &str, kind: MediaKindLabel, muted: bool) -> bytes::BytesMut {
    let [audio, video] = MediaKindLabel::per_kind(|k| (k == kind).then_some(muted));
    server_mute_kinds_frame(target, audio, video)
}

/// The next `ErrorMessage`, whole (code AND text), skipping other traffic.
async fn next_error(s: &mut Session) -> ErrorMessage {
    while let Some(m) = s.next_server_message().await {
        if let Some(server_message::Message::Error(e)) = m.message {
            return e;
        }
    }
    panic!("no ErrorMessage arrived");
}

/// Every cell of both per-kind mute counters, for "nothing per-kind moved".
/// The label sets are the production vocabularies' own `ALL`, so a new kind
/// or outcome is covered here without an edit.
fn assert_no_per_kind_mute_cell_moved(snap: &MetricSnapshot) {
    for kind in MediaKindLabel::ALL.map(MediaKindLabel::label) {
        for action in ServerMuteAction::ALL.map(ServerMuteAction::label) {
            for outcome in ServerMuteOutcome::ALL.map(ServerMuteOutcome::label) {
                snap.counter("mc_media_server_mute_requests_total")
                    .with_labels(&[
                        ("action", action),
                        ("outcome", outcome),
                        ("media_kind", kind),
                    ])
                    .assert_delta(0);
            }
        }
        for outcome in MuteOutcome::ALL.map(MuteOutcome::label) {
            snap.counter("mc_media_mute_requests_total")
                .with_labels(&[("outcome", outcome), ("media_kind", kind)])
                .assert_delta(0);
        }
    }
}

// ============================================================================
// R-9 / R-16: the server-muted stream rides every snapshot; the generation
// advances on a change
// ============================================================================

#[tokio::test]
async fn a_server_mute_rides_every_snapshot_advances_the_generation_and_mutes_the_slot() {
    let (stack, rig) = start_stack("sm-snapshot").await;
    let (mut h, a, mut b) = trio(&rig, &stack, "sm-snapshot-m").await;
    let before = pushes_to(&stack, "sm-snapshot-m", MH)
        .last()
        .cloned()
        .unwrap();
    assert!(dropped(&before).is_empty(), "nobody is muted yet");

    // The host server-mutes A's audio (video absent: unchanged).
    let snap = MetricAssertion::snapshot();
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let muted = push_until(&stack, "sm-snapshot-m", MH, "the mute is programmed", |c| {
        dropped(c) == vec![(a.sender_id, 0)]
    })
    .await;
    assert_eq!(
        muted.policy_generation,
        before.policy_generation + 1,
        "a disposition change alone advances the generation by exactly one"
    );
    assert_eq!(
        muted.assignment.egress_streams, before.assignment.egress_streams,
        "the mute KEEPS every edge (R-3 stability): only the disposition moved"
    );
    // One increment per (request, NAMED kind): the request named audio only,
    // so no video cell moves.
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "applied"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    for action in ["mute", "unmute"] {
        for outcome in ["applied", "unchanged"] {
            snap.counter("mc_media_server_mute_requests_total")
                .with_labels(&[
                    ("action", action),
                    ("outcome", outcome),
                    ("media_kind", "video"),
                ])
                .assert_delta(0);
        }
    }

    // B, who holds A, is told A's slot is AVATAR_SERVER_MUTED.
    let _ = b
        .assignments_until("B sees A muted", |v| {
            v.assignments.iter().any(|s| {
                s.sender_id == Some(a.sender_id)
                    && s.slot_state == SlotState::AvatarServerMuted as i32
            })
        })
        .await;

    // ANOTHER structural change (a new joiner): the next snapshot STILL
    // carries the mute — it rides every full registration (so a re-assert and
    // an MH restart relearn it), not only the push the mute caused.
    let mut c = join_as(&rig, &stack, "sm-snapshot-m", "user-c").await;
    c.write(capability_frame(audio_slots(2))).await;
    let _ = c.settle().await;
    let later = push_until(&stack, "sm-snapshot-m", MH, "a later snapshot", |p| {
        p.policy_generation > muted.policy_generation
    })
    .await;
    assert_eq!(
        dropped(&later),
        vec![(a.sender_id, 0)],
        "the mute survives a later snapshot (and so a re-assert)"
    );

    // The host lifts it: the next snapshot forwards A and the generation
    // advances.
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
        |p| p.policy_generation > before_unmute.policy_generation && dropped(p).is_empty(),
    )
    .await;
    assert_eq!(
        unmuted.policy_generation,
        before_unmute.policy_generation + 1
    );
    assert_eq!(
        disposition(&unmuted, a.sender_id, 0),
        Some(Disposition::Forward)
    );
}

/// A participant who muted THEMSELF is PAUSED at MH, never DROP_AT_INGRESS:
/// MH would drop at ingress and the participant has no way to lift a server
/// mute (plan §1). The self mute still reaches the slot view.
///
/// Discriminating without a timed silence: after A self-mutes, the host mutes
/// B, and the push that follows must drop EXACTLY B.
#[tokio::test]
async fn a_self_muted_participant_is_paused_and_never_dropped_at_ingress() {
    let (stack, rig) = start_stack("sm-self").await;
    let (mut h, mut a, mut b) = trio(&rig, &stack, "sm-self-m").await;
    let baseline = pushes_to(&stack, "sm-self-m", MH).last().cloned().unwrap();
    assert!(dropped(&baseline).is_empty(), "nobody is muted yet");

    // A mutes its audio (video absent); B's slot view shows it.
    a.write(mute_kinds_frame(Some(true), None)).await;
    let _ = b
        .assignments_until("B sees A self-muted", |v| {
            v.assignments.iter().any(|s| {
                s.sender_id == Some(a.sender_id)
                    && s.slot_state == SlotState::AvatarClientMuted as i32
            })
        })
        .await;
    let paused = push_until(&stack, "sm-self-m", MH, "A's PAUSED disposition", |c| {
        disposition(c, a.sender_id, 0) == Some(Disposition::Paused)
    })
    .await;
    assert_eq!(
        paused.policy_generation,
        baseline.policy_generation + 1,
        "a disposition change advances the generation exactly once"
    );
    assert!(dropped(&paused).is_empty(), "PAUSED is not an ingress drop");

    // The host server-mutes B: exactly B drops, one generation on, A stays
    // PAUSED.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let only_b = push_until(&stack, "sm-self-m", MH, "the host's mute of B", |c| {
        dropped(c) == vec![(b.sender_id, 0)]
    })
    .await;
    assert_eq!(only_b.policy_generation, paused.policy_generation + 1);
    assert_eq!(
        disposition(&only_b, a.sender_id, 0),
        Some(Disposition::Paused)
    );
    assert!(
        pushes_to(&stack, "sm-self-m", MH)
            .iter()
            .all(|c| !dropped(c).contains(&(a.sender_id, 0))),
        "a self-muted participant must never be dropped at ingress"
    );

    // Positive control: the host server-muting A DOES drop A (server wins).
    h.write(server_mute_frame(&a.participant_id, true)).await;
    let mut both = vec![(a.sender_id, 0), (b.sender_id, 0)];
    both.sort_unstable();
    let with_a = push_until(&stack, "sm-self-m", MH, "the host's mute of A", |c| {
        let mut d = dropped(c);
        d.sort_unstable();
        d == both
    })
    .await;

    // Lifting A's server mute returns A to PAUSED (still self-muted): clearing
    // one mute neither clears nor promotes the other.
    h.write(server_mute_frame(&a.participant_id, false)).await;
    let lifted = push_until(&stack, "sm-self-m", MH, "the lift of A", |c| {
        c.policy_generation > with_a.policy_generation
    })
    .await;
    assert_eq!(dropped(&lifted), vec![(b.sender_id, 0)]);
    assert_eq!(
        disposition(&lifted, a.sender_id, 0),
        Some(Disposition::Paused)
    );
}

// ============================================================================
// R-16: per-kind server mute withdraws THAT kind's directive and drops it at
// ingress; the other kind is byte-identical — for EACH kind
// ============================================================================

#[tokio::test]
async fn a_server_mute_of_one_kind_withdraws_and_drops_only_that_kind_and_the_unmute_restores_it() {
    for kind in MediaKindLabel::ALL {
        let meeting = format!("sm-kind-{}", kind.label());
        let (stack, rig) = start_stack(&meeting).await;
        let [(mut h, _, _), (mut a, a_before, _), (mut b, _, b_before)] =
            av_trio(&rig, &stack, &meeting).await;
        let other = kind.other();
        let push_before = pushes_to(&stack, &meeting, MH).last().cloned().unwrap();
        assert!(
            !stream_of(&a_before, kind).targets.is_empty(),
            "{kind:?}: A is watched"
        );

        let snap = MetricAssertion::snapshot();
        h.write(server_mute_one(&a.participant_id, kind, true))
            .await;

        // The TARGET's directive: K withdrawn (listed, empty targets, encoding
        // intact); the other kind byte-identical.
        let withdrawn = a
            .directive_until("A's directive withdraws K", |d| {
                stream_of(d, kind).targets.is_empty()
            })
            .await;
        assert_eq!(
            stream_of(&withdrawn, kind).encoding,
            stream_of(&a_before, kind).encoding
        );
        assert_eq!(
            stream_of(&withdrawn, other),
            stream_of(&a_before, other),
            "{kind:?}: the other kind's directive stream is byte-identical"
        );

        // MH: K DROP_AT_INGRESS, the other kind FORWARD, every edge kept.
        let k_number = u8::try_from(stream_of(&a_before, kind).stream_number).unwrap();
        let o_number = u8::try_from(stream_of(&a_before, other).stream_number).unwrap();
        let muted = push_until(&stack, &meeting, MH, "K drops at ingress", |c| {
            disposition(c, a.sender_id, k_number) == Some(Disposition::DropAtIngress)
        })
        .await;
        assert_eq!(
            disposition(&muted, a.sender_id, o_number),
            Some(Disposition::Forward)
        );
        assert_eq!(
            muted.assignment.egress_streams, push_before.assignment.egress_streams,
            "{kind:?}: a server-muted stream KEEPS its edges"
        );
        assert_eq!(muted.policy_generation, push_before.policy_generation + 1);

        // A holder: A's K slot is AVATAR_SERVER_MUTED naming A; every slot of
        // the other kind byte-identical.
        let b_muted = b
            .assignments_until("B sees A server-muted on K", |v| {
                slots_of(v, kind).iter().any(|s| {
                    s.sender_id == Some(a.sender_id)
                        && s.slot_state == SlotState::AvatarServerMuted as i32
                })
            })
            .await;
        assert_eq!(
            slots_of(&b_muted, other),
            slots_of(&b_before, other),
            "{kind:?}: the other kind's slot states are byte-identical"
        );

        // Counted: one withdrawal of K, reason server_muted; none of the other.
        snap.counter("mc_media_send_directive_withdrawals_total")
            .with_labels(&[
                ("media_kind", kind.label()),
                ("reason", "server_muted"),
                ("key_custody", "operator"),
            ])
            .assert_delta(1);
        snap.counter("mc_media_send_directive_withdrawals_total")
            .with_labels(&[("media_kind", other.label()), ("reason", "server_muted")])
            .assert_delta(0);

        // The host lifts it: K's targets come back exactly as before, MH
        // forwards K again, the holder's view is as before.
        h.write(server_mute_one(&a.participant_id, kind, false))
            .await;
        let restored = a
            .directive_until("A's directive restores K", |d| {
                !stream_of(d, kind).targets.is_empty()
            })
            .await;
        assert_eq!(restored, a_before, "{kind:?}: the directive is restored");
        let _ = push_until(&stack, &meeting, MH, "K forwards again", |c| {
            c.policy_generation > muted.policy_generation
                && disposition(c, a.sender_id, k_number) == Some(Disposition::Forward)
        })
        .await;
        let _ = b
            .assignments_until("B's view is restored", |v| *v == b_before)
            .await;
        // An unmute is not a withdrawal.
        snap.counter("mc_media_send_directive_withdrawals_total")
            .with_labels(&[("media_kind", kind.label()), ("reason", "server_muted")])
            .assert_delta(1);
    }
}

/// A server mute of a kind NOBODY watches (its targets are already empty) is
/// no transition: neither withdrawal reason moves for that kind. The positive
/// control in the same meeting: a server mute of the WATCHED kind counts +1.
#[tokio::test]
async fn a_server_mute_of_an_unwatched_kind_is_not_a_withdrawal() {
    let (stack, rig) = start_stack("sm-unwatched").await;
    seed_meeting_with_mh(&stack, "sm-unwatched-m").await;
    // Everyone publishes audio AND video, but declares AUDIO slots only: every
    // video stream is listed with empty targets (unwatched) from the start.
    let shape = || {
        JoinShape::publishing(&[MediaKind::Audio, MediaKind::VideoCamera])
            .receiving(vec![slot(0, MediaKind::Audio), slot(1, MediaKind::Audio)])
    };
    let mut h = join_shaped(&rig, &stack, "sm-unwatched-m", "host", true, shape()).await;
    let mut a = join_shaped(&rig, &stack, "sm-unwatched-m", "user-a", false, shape()).await;
    let mut b = join_shaped(&rig, &stack, "sm-unwatched-m", "user-b", false, shape()).await;
    for s in [&mut h, &mut b] {
        let _ = s.settle().await;
    }
    let (d, _) = a.settle().await;
    let a_before = d.or_else(|| a.join_directive.clone()).unwrap();
    assert!(
        stream_of(&a_before, MediaKindLabel::Video)
            .targets
            .is_empty(),
        "precondition: A's video is unwatched"
    );
    assert!(!stream_of(&a_before, MediaKindLabel::Audio)
        .targets
        .is_empty());

    let snap = MetricAssertion::snapshot();
    h.write(server_mute_one(
        &a.participant_id,
        MediaKindLabel::Video,
        true,
    ))
    .await;
    let _ = push_until(&stack, "sm-unwatched-m", MH, "video DROP", |c| {
        disposition(c, a.sender_id, 1) == Some(Disposition::DropAtIngress)
    })
    .await;
    // Positive control, same meeting: the WATCHED kind's mute is a withdrawal.
    h.write(server_mute_one(
        &a.participant_id,
        MediaKindLabel::Audio,
        true,
    ))
    .await;
    let _ = a
        .directive_until("A's audio withdrawn", |d| {
            stream_of(d, MediaKindLabel::Audio).targets.is_empty()
        })
        .await;
    snap.counter("mc_media_send_directive_withdrawals_total")
        .with_labels(&[("media_kind", "audio"), ("reason", "server_muted")])
        .assert_delta(1);
    for reason in ["server_muted", "unwatched"] {
        snap.counter("mc_media_send_directive_withdrawals_total")
            .with_labels(&[("media_kind", "video"), ("reason", reason)])
            .assert_delta(0);
    }
}

/// Server unmute restores the directive REGARDLESS of client mute (ADR-0036
/// §5: the directive is blind to client mute; the client keeps capture off).
/// The disposition returns to PAUSED, not FORWARD.
#[tokio::test]
async fn a_server_unmute_restores_the_targets_of_a_client_muted_kind_and_leaves_it_paused() {
    let (stack, rig) = start_stack("sm-compose").await;
    let [(mut h, _, _), (mut a, a_before, _), (mut b, _, _)] =
        av_trio(&rig, &stack, "sm-compose-m").await;
    let video = MediaKindLabel::Video;

    a.write(mute_kinds_frame(None, Some(true))).await;
    let paused = push_until(&stack, "sm-compose-m", MH, "A video PAUSED", |c| {
        disposition(c, a.sender_id, 1) == Some(Disposition::Paused)
    })
    .await;
    // Client mute never touches the directive: a later structural change
    // re-sends nothing to A (fenced by the host's mute below, whose directive
    // is the FIRST A sees).
    h.write(server_mute_one(&a.participant_id, video, true))
        .await;
    let first = a.expect_directive().await;
    assert!(
        stream_of(&first, video).targets.is_empty(),
        "the first directive after the client mute is the server mute's withdrawal"
    );
    let dropped_now = push_until(&stack, "sm-compose-m", MH, "server wins", |c| {
        c.policy_generation > paused.policy_generation
    })
    .await;
    assert_eq!(
        disposition(&dropped_now, a.sender_id, 1),
        Some(Disposition::DropAtIngress)
    );
    let _ = b
        .assignments_until("B sees server-muted (server wins)", |v| {
            slots_of(v, video).iter().any(|s| {
                s.sender_id == Some(a.sender_id)
                    && s.slot_state == SlotState::AvatarServerMuted as i32
            })
        })
        .await;

    h.write(server_mute_one(&a.participant_id, video, false))
        .await;
    let restored = a
        .directive_until("targets restored while client-muted", |d| {
            !stream_of(d, video).targets.is_empty()
        })
        .await;
    assert_eq!(restored, a_before);
    let back = push_until(&stack, "sm-compose-m", MH, "back to PAUSED", |c| {
        c.policy_generation > dropped_now.policy_generation
    })
    .await;
    assert_eq!(
        disposition(&back, a.sender_id, 1),
        Some(Disposition::Paused),
        "PAUSED, never FORWARD: the client mute is still on"
    );
    let _ = b
        .assignments_until("B sees client-muted again", |v| {
            slots_of(v, video).iter().any(|s| {
                s.sender_id == Some(a.sender_id)
                    && s.slot_state == SlotState::AvatarClientMuted as i32
            })
        })
        .await;
}

// ============================================================================
// R-18: the self mute update arrives BEFORE the directive change, and before
// the holders' slot states — mute and unmute, both kinds
// ============================================================================

/// Read `s`'s raw messages until `is_change` matches; return whether a
/// `ParticipantMuteUpdate` matching `is_update` arrived STRICTLY before it.
async fn update_precedes(
    s: &mut Session,
    what: &str,
    is_update: impl Fn(&proto_gen::dark_tower::signaling::v1::ParticipantMuteUpdate) -> bool,
    is_change: impl Fn(&ServerMessage) -> bool,
) -> bool {
    let mut seen_update = false;
    while let Some(m) = s.next_server_message().await {
        if is_change(&m) {
            return seen_update;
        }
        if let Some(server_message::Message::ParticipantMuteUpdate(u)) = &m.message {
            if is_update(u) {
                seen_update = true;
            }
        }
    }
    panic!("{what}: the change never arrived");
}

#[tokio::test]
async fn the_self_mute_update_precedes_the_directive_change_and_the_holders_slot_states() {
    for kind in MediaKindLabel::ALL {
        let meeting = format!("sm-order-{}", kind.label());
        let (stack, rig) = start_stack(&meeting).await;
        let [(mut h, _, _), (mut a, _, _), (mut b, _, b_before)] =
            av_trio(&rig, &stack, &meeting).await;
        let a_id = a.participant_id.clone();
        let a_sender = a.sender_id;
        let server_muted = |u: &proto_gen::dark_tower::signaling::v1::ParticipantMuteUpdate| {
            server_of(&u.media_states, kind.to_proto()).0
        };

        for muted in [true, false] {
            h.write(server_mute_one(&a_id, kind, muted)).await;
            let is_update = |u: &proto_gen::dark_tower::signaling::v1::ParticipantMuteUpdate| {
                u.participant_id == a_id && server_muted(u) == muted
            };
            // The TARGET: the update before the directive whose K targets
            // became empty (mute) or non-empty (unmute) — the pair the client
            // keys its "a host resumed your camera/mic" notice on.
            assert!(
                update_precedes(
                    &mut a,
                    "the target's directive",
                    is_update,
                    |m| matches!(&m.message, Some(server_message::Message::SendDirective(d))
                        if stream_of(d, kind).targets.is_empty() == muted),
                )
                .await,
                "{kind:?} muted={muted}: the target's self update must precede its directive"
            );
            // A HOLDER: the update before the slot state change.
            assert!(
                update_precedes(
                    &mut b,
                    "the holder's assignments",
                    is_update,
                    |m| matches!(&m.message, Some(server_message::Message::StreamAssignments(v))
                    if if muted {
                        slots_of(v, kind).iter().any(|s| s.sender_id == Some(a_sender)
                            && s.slot_state == SlotState::AvatarServerMuted as i32)
                    } else {
                        *v == b_before
                    }),
                )
                .await,
                "{kind:?} muted={muted}: the holder's update must precede its slot states"
            );
        }
    }
}

// ============================================================================
// R-15: authority, shape and counting over the wire
// ============================================================================

/// A non-host is refused BEFORE the target is looked up: naming a participant
/// who does not exist gets the same refusal, counted `not_permitted` on the
/// named kind only, never `unknown_target`.
#[tokio::test]
async fn a_non_host_server_mute_is_refused_before_the_target_and_programs_nothing() {
    let (stack, rig) = start_stack("sm-nonhost").await;
    let (mut h, mut a, b) = trio(&rig, &stack, "sm-nonhost-m").await;
    let last_before = pushes_to(&stack, "sm-nonhost-m", MH)
        .last()
        .cloned()
        .unwrap();

    let snap = MetricAssertion::snapshot();
    a.write(server_mute_frame("ghost-participant", true)).await;
    assert_eq!(a.expect_error().await, "Server mute request refused");
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "not_permitted"),
            ("media_kind", "audio"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    for kind in ["audio", "video"] {
        snap.counter("mc_media_server_mute_requests_total")
            .with_labels(&[
                ("action", "mute"),
                ("outcome", "unknown_target"),
                ("media_kind", kind),
            ])
            .assert_delta(0);
    }
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "unmute"),
            ("outcome", "not_permitted"),
            ("media_kind", "video"),
        ])
        .assert_delta(0);

    // Positive control rather than a timed silence: the host now mutes B. That
    // push must be EXACTLY one generation past the last one before the refused
    // request — so the refusal consumed no generation and programmed nothing.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let muted = push_until(&stack, "sm-nonhost-m", MH, "the host's mute", |c| {
        dropped(c) == vec![(b.sender_id, 0)]
    })
    .await;
    assert_eq!(
        muted.policy_generation,
        last_before.policy_generation + 1,
        "the refused request was not a structural change"
    );
}

/// A server-mute request naming NO kind is refused with the ONE generic error
/// — byte-identical to a named-kind refusal, from a non-host as from a host —
/// counted only on the kind-free `mc_media_mute_requests_rejected_total`, and
/// it programs nothing.
#[tokio::test]
async fn a_server_mute_naming_no_kind_is_refused_generically_and_counted_kind_free() {
    let (stack, rig) = start_stack("sm-nokind").await;
    let (mut h, mut a, b) = trio(&rig, &stack, "sm-nokind-m").await;
    let last_before = pushes_to(&stack, "sm-nokind-m", MH)
        .last()
        .cloned()
        .unwrap();

    // The reference: a non-host's NAMED-kind refusal.
    a.write(server_mute_frame(&b.participant_id, true)).await;
    let named = next_error(&mut a).await;

    let snap = MetricAssertion::snapshot();
    // A non-host naming no kind, about a participant who does not exist.
    a.write(server_mute_kinds_frame("ghost-participant", None, None))
        .await;
    let non_host = next_error(&mut a).await;
    assert_eq!(
        non_host, named,
        "a non-host learns nothing from the shape: same code, same text"
    );
    // A host naming no kind.
    h.write(server_mute_kinds_frame(&b.participant_id, None, None))
        .await;
    let host = next_error(&mut h).await;
    assert_eq!(host, named, "one generic error for every refusal");

    snap.counter("mc_media_mute_requests_rejected_total")
        .with_labels(&[
            ("surface", "server_mute"),
            ("reason", "no_kind_named"),
            ("key_custody", "operator"),
        ])
        .assert_delta(2);
    snap.counter("mc_media_mute_requests_rejected_total")
        .with_labels(&[("surface", "client_mute"), ("reason", "no_kind_named")])
        .assert_delta(0);
    assert_no_per_kind_mute_cell_moved(&snap);

    // Positive control: the host's next real mute is exactly one generation on.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let muted = push_until(&stack, "sm-nokind-m", MH, "the host's mute", |c| {
        dropped(c) == vec![(b.sender_id, 0)]
    })
    .await;
    assert_eq!(muted.policy_generation, last_before.policy_generation + 1);
}

/// A client `MuteRequest` naming no kind is rejected with a generic error,
/// counted kind-free, spends no rate-limit token (a real toggle after a burst
/// of rejections is still applied) and reaches no per-kind cell.
#[tokio::test]
async fn a_client_mute_naming_no_kind_is_rejected_and_spends_no_token() {
    let (stack, rig) = start_stack("sm-client-nokind").await;
    let (_h, mut a, mut b) = trio(&rig, &stack, "sm-client-nokind-m").await;

    let snap = MetricAssertion::snapshot();
    a.write(mute_kinds_frame(None, None)).await;
    let error = next_error(&mut a).await;
    assert_eq!(error.message, "Mute request refused");
    snap.counter("mc_media_mute_requests_rejected_total")
        .with_labels(&[
            ("surface", "client_mute"),
            ("reason", "no_kind_named"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    assert_no_per_kind_mute_cell_moved(&snap);

    // More rejections than the whole mute burst (8): none spends a token.
    for _ in 0..9 {
        a.write(mute_kinds_frame(None, None)).await;
    }
    a.write(mute_kinds_frame(Some(true), None)).await;
    let _ = b
        .assignments_until("the real toggle is applied", |v| {
            v.assignments.iter().any(|s| {
                s.sender_id == Some(a.sender_id)
                    && s.slot_state == SlotState::AvatarClientMuted as i32
            })
        })
        .await;
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "applied"), ("media_kind", "audio")])
        .assert_delta(1);
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "rate_limited"), ("media_kind", "audio")])
        .assert_delta(0);
}

/// An identical client toggle is a no-op BEFORE the actor hop and the limiter:
/// no generation advance, no push, counted `unchanged` on the named kind.
#[tokio::test]
async fn an_identical_client_toggle_neither_pushes_nor_advances_the_generation() {
    let (stack, rig) = start_stack("sm-idem").await;
    let (mut h, mut a, b) = trio(&rig, &stack, "sm-idem-m").await;
    a.write(mute_kinds_frame(Some(true), None)).await;
    let paused = push_until(&stack, "sm-idem-m", MH, "A PAUSED", |c| {
        disposition(c, a.sender_id, 0) == Some(Disposition::Paused)
    })
    .await;
    let _ = a.settle().await;

    let snap = MetricAssertion::snapshot();
    a.write(mute_kinds_frame(Some(true), None)).await;
    // Fence: the host's mute of B is the NEXT push, exactly one generation on.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let fence = push_until(&stack, "sm-idem-m", MH, "the fence", |c| {
        dropped(c) == vec![(b.sender_id, 0)]
    })
    .await;
    assert_eq!(fence.policy_generation, paused.policy_generation + 1);
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "unchanged"), ("media_kind", "audio")])
        .assert_delta(1);
    snap.counter("mc_media_mute_requests_total")
        .with_labels(&[("outcome", "unchanged"), ("media_kind", "video")])
        .assert_delta(0);
}

// ============================================================================
// R-17: client-mute churn is bounded by the latest-wins push worker
// ============================================================================

/// The bound under test, derived from `media_routing::pusher`'s structure
/// (one `RegisterMeeting` in flight per handler, plus the latest-wins `watch`
/// slot holding at most one pending snapshot): however many mute toggles land
/// while a push is outstanding, at most ONE more push follows it.
const PUSHES_IN_FLIGHT_PLUS_PENDING: usize = 2;

/// N rapid client-mute toggles across three participants, with MH holding
/// every push: the handler sees at most one push in flight during the storm
/// and at most `PUSHES_IN_FLIGHT_PLUS_PENDING` across the whole storm, and the
/// final applied snapshot's dispositions equal the final mute state.
#[tokio::test]
async fn a_client_mute_storm_is_bounded_by_one_push_in_flight_and_one_pending() {
    /// Toggles per participant: under the mute burst (8), odd, so each
    /// participant ends client-muted.
    const TOGGLES: u32 = 7;
    let (stack, rig) = start_stack("sm-storm").await;
    let (mut h, mut a, mut b) = trio(&rig, &stack, "sm-storm-m").await;
    let start = pushes_to(&stack, "sm-storm-m", MH).len();
    let snap = MetricAssertion::snapshot();

    stack.mh_reg_client.hold_pushes();
    for i in 0..TOGGLES {
        let muted = i % 2 == 0;
        for s in [&mut h, &mut a, &mut b] {
            s.write(mute_kinds_frame(Some(muted), None)).await;
        }
    }
    // Every toggle processed (counted), not a wait for a silence.
    let total = u64::from(TOGGLES) * 3;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let applied = snap
            .counter("mc_media_mute_requests_total")
            .with_labels(&[("outcome", "applied"), ("media_kind", "audio")])
            .delta();
        if applied >= total {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "only {applied}/{total} toggles were applied"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let during = pushes_to(&stack, "sm-storm-m", MH).len() - start;
    assert_eq!(
        during, 1,
        "exactly the one held push is in flight per handler while MH holds it (the first \
         toggle is a structural change, so one push must be outstanding)"
    );

    stack.mh_reg_client.release_pushes();
    let senders = [h.sender_id, a.sender_id, b.sender_id];
    let final_push = push_until(&stack, "sm-storm-m", MH, "the final state", |c| {
        senders
            .iter()
            .all(|s| disposition(c, *s, 0) == Some(Disposition::Paused))
    })
    .await;

    // Fence (no timed silence): one more structural change, after which the
    // storm's pushes are all accounted for.
    h.write(server_mute_frame(&b.participant_id, true)).await;
    let fence = push_until(&stack, "sm-storm-m", MH, "the fence", |c| {
        c.policy_generation > final_push.policy_generation
            && disposition(c, b.sender_id, 0) == Some(Disposition::DropAtIngress)
    })
    .await;
    let storm = pushes_to(&stack, "sm-storm-m", MH)
        .iter()
        .skip(start)
        .take_while(|c| c.policy_generation < fence.policy_generation)
        .count();
    assert!(storm >= 1, "positive control: the storm did push");
    assert!(
        storm <= PUSHES_IN_FLIGHT_PLUS_PENDING,
        "{total} toggles drove {storm} pushes; the bound is {PUSHES_IN_FLIGHT_PLUS_PENDING}"
    );
}

// ============================================================================
// R-30: the per-kind muted-sources census
// ============================================================================

#[tokio::test]
async fn the_server_muted_sources_gauge_counts_each_kind_from_its_own_flag() {
    let (stack, rig) = start_stack("sm-census").await;
    let [(mut h, _, _), (a, _, _), _] = av_trio(&rig, &stack, "sm-census-m").await;
    let snap = MetricAssertion::snapshot();
    let gauge = |kind: &str, value: f64| {
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[("media_kind", kind), ("key_custody", "operator")])
            .assert_value(value);
    };
    let step = |audio: Option<bool>, video: Option<bool>| {
        server_mute_kinds_frame(&a.participant_id, audio, video)
    };
    let mut generation = pushes_to(&stack, "sm-census-m", MH)
        .last()
        .unwrap()
        .policy_generation;
    for (frame, expected) in [
        (step(None, Some(true)), [0.0, 1.0]),
        (step(Some(true), None), [1.0, 1.0]),
        (step(None, Some(false)), [1.0, 0.0]),
        (step(Some(false), None), [0.0, 0.0]),
    ] {
        h.write(frame).await;
        generation = push_until(&stack, "sm-census-m", MH, "the step", |c| {
            c.policy_generation > generation
        })
        .await
        .policy_generation;
        gauge("audio", expected[0]);
        gauge("video", expected[1]);
    }
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
    assert_eq!(
        server_of(&update.media_states, MediaKind::Audio),
        (true, h.participant_id.clone())
    );
    assert_eq!(
        server_of(&update.media_states, MediaKind::VideoCamera),
        (false, String::new()),
        "video was not named"
    );
}

/// A joiner arriving AFTER a mute learns it from its ROSTER entry (ADR-0036 §5
/// amendment 2026-10-25: one carrier, no replay). Fence for "no replay": the
/// host then mutes ANOTHER participant, and that update is the FIRST mute
/// update the joiner receives.
#[tokio::test]
async fn a_late_joiners_roster_carries_who_is_server_muted_and_no_replay_follows() {
    let (stack, rig) = start_stack("sm-late").await;
    let [(mut h, _, _), (a, _, _), (b, _, _)] = av_trio(&rig, &stack, "sm-late-m").await;
    h.write(server_mute_kinds_frame(&a.participant_id, None, Some(true)))
        .await;
    let _ = h.expect_mute_update_for(&a.participant_id.clone()).await;

    let mut late = join_as(&rig, &stack, "sm-late-m", "user-late").await;
    let entry = late
        .existing_participants
        .iter()
        .find(|p| p.participant_id == a.participant_id)
        .expect("A is on the roster");
    assert_eq!(
        server_of(&entry.media_states, MediaKind::VideoCamera),
        (true, h.participant_id.clone()),
        "the roster carries per-kind server mute and its imposer"
    );
    assert_eq!(
        server_of(&entry.media_states, MediaKind::Audio),
        (false, String::new())
    );

    h.write(server_mute_kinds_frame(&b.participant_id, Some(true), None))
        .await;
    let first = loop {
        let m = late.next_server_message().await.expect("the fence arrives");
        if let Some(server_message::Message::ParticipantMuteUpdate(u)) = m.message {
            break u;
        }
    };
    assert_eq!(
        first.participant_id, b.participant_id,
        "no replay frame about A precedes the fence"
    );
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
        dropped(c) == vec![(a.sender_id, 0)]
    })
    .await;

    // The pod-level gauge reports the one mute in force.
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("media_kind", "audio"), ("key_custody", "operator")])
        .assert_value(1.0);
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("media_kind", "video"), ("key_custody", "operator")])
        .assert_value(0.0);

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
    assert_eq!(relayed.media_kinds, vec![MediaKind::Audio as i32]);
    snap.counter("mc_media_unmute_requests_total")
        .with_labels(&[("outcome", "relayed"), ("key_custody", "operator")])
        .assert_delta(1);

    // Still muted: the gauge is unchanged and MC pushed nothing new.
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("media_kind", "audio"), ("key_custody", "operator")])
        .assert_value(1.0);
    let last = pushes_to(&stack, "sm-unmute-m", MH)
        .last()
        .cloned()
        .unwrap();
    assert_eq!(last.policy_generation, muted.policy_generation);
    assert_eq!(dropped(&last), vec![(a.sender_id, 0)]);

    // Only the host lifts it, and the gauge follows.
    h.write(server_mute_frame(&a.participant_id, false)).await;
    let _ = push_until(&stack, "sm-unmute-m", MH, "the unmute landed", |c| {
        c.policy_generation > muted.policy_generation && dropped(c).is_empty()
    })
    .await;
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("media_kind", "audio"), ("key_custody", "operator")])
        .assert_value(0.0);
    snap.gauge("mc_media_server_muted_sources")
        .with_labels(&[("media_kind", "video"), ("key_custody", "operator")])
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
    /// One more than the reply burst (`MUTE_REFUSAL_REPLY_BURST` = 4).
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
            ("media_kind", "audio"),
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
    for surface in ["client_mute", "capability"] {
        snap.counter("mc_media_refusal_replies_suppressed_total")
            .with_labels(&[("surface", surface), ("key_custody", "operator")])
            .assert_delta(0);
    }

    let mut replies = 0;
    while let Some(msg) = a.next_server_message().await {
        if matches!(msg.message, Some(server_message::Message::Error(_))) {
            replies += 1;
        }
    }
    assert_eq!(replies, SENT - 1, "exactly the burst is answered");
}

/// The both-kinds frame helper names both kinds explicitly (an explicit
/// `false` is an unmute, never "unchanged"). From an AUDIO-ONLY publisher (the
/// trio), the named video kind is not published, so the WHOLE request is
/// refused (ADR-0036 §5 amendment 2026-10-25): audio is NOT applied.
#[tokio::test]
async fn a_both_kinds_client_mute_from_an_audio_only_publisher_is_refused_whole() {
    let (stack, rig) = start_stack("sm-both").await;
    let (_h, mut a, _b) = trio(&rig, &stack, "sm-both-m").await;
    let snap = MetricAssertion::snapshot();
    a.write(mute_frame(true, false)).await;
    assert_eq!(next_error(&mut a).await.message, "Mute request refused");
    rejected(&snap, "client_mute", "not_published").assert_delta(1);
    assert_no_per_kind_mute_cell_moved(&snap);
    let _ = stack;
}

// ============================================================================
// Story 3 R-15 fix: client mute is a roster fact for every participant
// (reverses story-2 S-2; ADR-0036 §5 amendments 2026-10-24 and 2026-10-25)
// ============================================================================

/// `mc_media_mute_requests_rejected_total{surface, reason}`.
fn rejected<'a>(
    snap: &'a MetricSnapshot,
    surface: &'a str,
    reason: &'a str,
) -> common::observability::testing::CounterQuery<'a> {
    snap.counter("mc_media_mute_requests_rejected_total")
        .with_labels(&[
            ("surface", surface),
            ("reason", reason),
            ("key_custody", "operator"),
        ])
}

/// Exactly one `{surface, reason}` cell moved by one; every other reason on
/// that surface did not.
fn assert_rejected_exactly(snap: &MetricSnapshot, surface: &str, reason: &str) {
    for r in MuteRejection::ALL.map(MuteRejection::label) {
        rejected(snap, surface, r).assert_delta(u64::from(r == reason));
    }
}

/// A client `MuteRequest` reaches ANOTHER session as a `ParticipantMuteUpdate`
/// about the requester (the subject is always the connection's own
/// participant -- `MuteRequest` names no one), carrying the full per-kind state
/// with no server mute; an identical repeat emits nothing, fenced by the next
/// real transition being the NEXT update B sees about A.
#[tokio::test]
async fn a_client_mute_reaches_other_sessions_and_a_repeat_reaches_no_one() {
    let (stack, rig) = start_stack("cm-wire").await;
    let [_h, (mut a, _, _), (mut b, _, _)] = av_trio(&rig, &stack, "cm-wire-m").await;
    let a_id = a.participant_id.clone();
    let snap = MetricAssertion::snapshot();

    a.write(mute_kinds_frame(Some(true), None)).await;
    let first = b.expect_mute_update_for(&a_id).await;
    assert_eq!(clients(&first.media_states), (MUTED, LIVE));
    for kind in [
        MediaKind::Audio,
        MediaKind::VideoCamera,
        MediaKind::VideoScreen,
    ] {
        assert_eq!(server_of(&first.media_states, kind), (false, String::new()));
    }
    assert_eq!(
        client_of(&first.media_states, MediaKind::VideoScreen),
        NOT_PUBLISHED,
        "screen rides as the MC constant"
    );

    a.write(mute_kinds_frame(Some(true), None)).await; // no-op
    a.write(mute_kinds_frame(None, Some(true))).await; // the fence
    let next = b.expect_mute_update_for(&a_id).await;
    assert_eq!(
        clients(&next.media_states),
        (MUTED, MUTED),
        "the next update about A is the fence; a no-op emission would have been (MUTED, LIVE)"
    );
    // One fan-out per REAL transition (counted per subject, not per
    // recipient), none for the no-op, and the per-meeting bucket never engaged.
    snap.counter("mc_media_client_mute_fanouts_total")
        .with_labels(&[("outcome", "fanned_out")])
        .assert_delta(2);
    snap.counter("mc_media_client_mute_fanouts_total")
        .with_labels(&[("outcome", "awaiting_token")])
        .assert_delta(0);
    let _ = stack;
}

/// Test plan 6: a participant that JOINS client-muted is PAUSED at MH from its
/// first registration snapshot (no `MuteRequest` ever sent), and an unmute
/// moves it to FORWARD at the next generation -- the initial state is live
/// state, not a frozen one.
#[tokio::test]
async fn a_joined_muted_publisher_is_paused_at_once_and_an_unmute_forwards_it() {
    let (stack, rig) = start_stack("cm-joined-muted").await;
    const M: &str = "cm-joined-muted-m";
    seed_meeting_with_mh(&stack, M).await;
    let mut h = join_as_host(&rig, &stack, M, "host").await;
    h.write(capability_frame(audio_slots(2))).await;
    let _ = h.settle().await;
    let mut a = join_shaped(
        &rig,
        &stack,
        M,
        "user-a",
        false,
        JoinShape::default().client_muted(MediaKind::Audio),
    )
    .await;
    let _ = a.settle().await;
    let paused = push_until(&stack, M, MH, "a's stream is declared", |c| {
        disposition(c, a.sender_id, 0).is_some()
    })
    .await;
    assert_eq!(
        disposition(&paused, a.sender_id, 0),
        Some(Disposition::Paused),
        "joined muted: PAUSED from the first snapshot that declares it"
    );

    a.write(mute_kinds_frame(Some(false), None)).await;
    let forwarded = push_until(&stack, M, MH, "the unmute", |c| {
        disposition(c, a.sender_id, 0) == Some(Disposition::Forward)
    })
    .await;
    assert_eq!(
        forwarded.policy_generation,
        paused.policy_generation + 1,
        "FORWARD at the next generation"
    );
}

/// The join roster carries every participant's DEFINITE per-kind state (ADR-0036
/// §5 amendment 2026-10-25): a muted kind CLIENT_MUTED, a live kind LIVE, an
/// unpublished kind NOT_PUBLISHED, and a participant that joined muted is
/// CLIENT_MUTED without ever sending a `MuteRequest` -- on the late joiner's
/// `existing_participants` and on the `ParticipantJoined` the others receive.
#[tokio::test]
async fn the_join_roster_carries_definite_client_states_with_no_mute_request() {
    let (stack, rig) = start_stack("cm-roster").await;
    let [(h, _, _), (mut a, _, _), (mut b, _, _)] = av_trio(&rig, &stack, "cm-roster-m").await;
    a.write(mute_kinds_frame(None, Some(true))).await;
    let _ = b.expect_mute_update_for(&a.participant_id.clone()).await;
    let audio_only = join_as(&rig, &stack, "cm-roster-m", "user-ao").await;

    let late = join_shaped(
        &rig,
        &stack,
        "cm-roster-m",
        "user-late",
        false,
        JoinShape::publishing(&[MediaKind::Audio, MediaKind::VideoCamera])
            .client_muted(MediaKind::Audio),
    )
    .await;
    let states = |id: &str| {
        let p = late
            .existing_participants
            .iter()
            .find(|p| p.participant_id == id)
            .unwrap_or_else(|| panic!("{id} is on the roster"));
        clients(&p.media_states)
    };
    assert_eq!(states(&a.participant_id), (LIVE, MUTED));
    assert_eq!(states(&b.participant_id), (LIVE, LIVE));
    assert_eq!(states(&h.participant_id), (LIVE, LIVE));
    assert_eq!(
        states(&audio_only.participant_id),
        (LIVE, NOT_PUBLISHED),
        "an audio-only publisher's camera is NOT_PUBLISHED, never unknown"
    );

    let joined = loop {
        let msg = b.next_server_message().await.expect("b hears the join");
        if let Some(server_message::Message::ParticipantJoined(j)) = msg.message {
            let p = j.participant.expect("an entry");
            if p.participant_id == late.participant_id {
                break p;
            }
        }
    };
    assert_eq!(
        clients(&joined.media_states),
        (MUTED, LIVE),
        "the joiner's declared mute rides its own ParticipantJoined"
    );
}

/// Inverts the R-15 "ignored and counted" test (ADR-0036 §5 amendment
/// 2026-10-25): a client mute naming a kind the participant does not publish
/// is refused WHOLE -- the published audio in the same request is NOT applied
/// -- counted kind-free, answered generically, and changes nothing. Fenced: B's
/// first update about A is a LATER valid toggle, showing audio still LIVE
/// before it.
#[tokio::test]
async fn a_client_mute_naming_an_unpublished_kind_is_refused_whole() {
    let (stack, rig) = start_stack("cm-unpub").await;
    let (_h, mut a, mut b) = trio(&rig, &stack, "cm-unpub-m").await;
    let snap = MetricAssertion::snapshot();

    a.write(mute_kinds_frame(Some(true), Some(true))).await; // mixed
    assert_eq!(next_error(&mut a).await.message, "Mute request refused");
    assert_rejected_exactly(&snap, "client_mute", "not_published");
    assert_no_per_kind_mute_cell_moved(&snap);

    a.write(mute_kinds_frame(Some(true), None)).await; // the fence
    let first = b.expect_mute_update_for(&a.participant_id.clone()).await;
    assert_eq!(
        clients(&first.media_states),
        (MUTED, NOT_PUBLISHED),
        "the first update about A is the fence; the refused request applied nothing"
    );
    snap.counter("mc_media_client_mute_fanouts_total")
        .with_labels(&[("outcome", "fanned_out")])
        .assert_delta(1);
}

/// Test plan 2: every kind-list refusal, on every surface, lands on exactly
/// its own `{surface, reason}` cell, moves no per-kind cell, is answered with
/// the surface's one generic reply, and changes nothing (fence: a later valid
/// request is what B sees next). Screen is `not_published` on all three
/// surfaces.
#[tokio::test]
async fn every_kind_list_refusal_is_counted_on_its_own_cell_on_every_surface() {
    const A: i32 = MediaKind::Audio as i32;
    const V: i32 = MediaKind::VideoCamera as i32;
    const S: i32 = MediaKind::VideoScreen as i32;
    const U: i32 = MediaKind::Unspecified as i32;
    let cases: [(&str, Vec<(i32, bool)>); 7] = [
        ("no_kind_named", vec![]),
        ("media_kind_unspecified", vec![(U, true)]),
        ("media_kind_unspecified", vec![(77, true)]),
        ("duplicate_kind", vec![(A, true), (A, false)]),
        ("not_published", vec![(S, true)]),
        ("not_published", vec![(V, true)]), // the trio publishes audio only
        // Mixed: a published kind does not carry an unpublished one -- the
        // whole list is refused.
        ("not_published", vec![(A, true), (S, true)]),
    ];
    let (stack, rig) = start_stack("cm-refusals").await;
    // A fresh trio per case: each surface's refusal REPLY is rationed per
    // connection (burst `MUTE_REFUSAL_REPLY_BURST`), so one connection would
    // run out of replies before the table did.
    for (i, (reason, changes)) in cases.iter().enumerate() {
        let (mut h, mut a, mut b) = trio(&rig, &stack, &format!("cm-refusals-m-{i}")).await;
        // Client mute.
        let snap = MetricAssertion::snapshot();
        a.write(mute_changes_frame(changes)).await;
        assert_eq!(
            next_error(&mut a).await.message,
            "Mute request refused",
            "{reason}"
        );
        assert_rejected_exactly(&snap, "client_mute", reason);
        assert_no_per_kind_mute_cell_moved(&snap);

        // Server mute, from the host.
        let snap = MetricAssertion::snapshot();
        h.write(server_mute_changes_frame(&a.participant_id, changes))
            .await;
        assert_eq!(
            next_error(&mut h).await.message,
            "Server mute request refused",
            "{reason}"
        );
        assert_rejected_exactly(&snap, "server_mute", reason);
        assert_no_per_kind_mute_cell_moved(&snap);

        // Unmute request (kinds only).
        let kinds: Vec<i32> = changes.iter().map(|(k, _)| *k).collect();
        let snap = MetricAssertion::snapshot();
        a.write(unmute_kinds_frame("", &kinds)).await;
        assert_eq!(
            next_error(&mut a).await.message,
            "Unmute request refused",
            "{reason}"
        );
        assert_rejected_exactly(&snap, "unmute_request", reason);
        for outcome in UnmuteRequestOutcome::ALL.map(UnmuteRequestOutcome::label) {
            snap.counter("mc_media_unmute_requests_total")
                .with_labels(&[("outcome", outcome), ("key_custody", "operator")])
                .assert_delta(0);
        }
        // Fence: nothing above changed A; a valid toggle is B's next update
        // about A, with no server mute.
        a.write(mute_kinds_frame(Some(true), None)).await;
        let u = b.expect_mute_update_for(&a.participant_id.clone()).await;
        assert_eq!(clients(&u.media_states), (MUTED, NOT_PUBLISHED), "{reason}");
        for kind in [MediaKind::Audio, MediaKind::VideoCamera] {
            assert_eq!(server_of(&u.media_states, kind), (false, String::new()));
        }
    }
    let _ = stack;
}

/// Security Gate 1 point 1 / 6(c), must-hold 1: a host's server mute of a
/// kind the TARGET does not publish is refused with a reply BYTE-identical to
/// an unknown target's; a NON-host's reply is byte-identical to the
/// well-formed not-permitted reply WHATEVER it sends (empty list, duplicate,
/// UNSPECIFIED, screen), and each malformed list is counted kind-free on its
/// own reason cell.
#[tokio::test]
async fn a_server_mute_of_an_unpublished_kind_answers_like_an_unknown_target() {
    use prost::Message as _;
    let (stack, rig) = start_stack("sm-unpub").await;
    let (mut h, mut a, mut b) = trio(&rig, &stack, "sm-unpub-m").await;

    h.write(server_mute_kinds_frame("ghost", Some(true), None))
        .await;
    let unknown = next_error(&mut h).await;
    let snap = MetricAssertion::snapshot();
    h.write(server_mute_kinds_frame(&b.participant_id, None, Some(true)))
        .await;
    let unpublished = next_error(&mut h).await;
    assert_eq!(
        unpublished.encode_to_vec(),
        unknown.encode_to_vec(),
        "not_published is indistinguishable from unknown_target on the wire"
    );
    assert_rejected_exactly(&snap, "server_mute", "not_published");
    assert_no_per_kind_mute_cell_moved(&snap);

    // The non-host reference: a WELL-FORMED request from B (a non-host).
    let snap = MetricAssertion::snapshot();
    b.write(server_mute_kinds_frame(&a.participant_id, Some(true), None))
        .await;
    let not_permitted = next_error(&mut b).await;
    assert_eq!(not_permitted.message, "Server mute request refused");
    snap.counter("mc_media_server_mute_requests_total")
        .with_labels(&[
            ("action", "mute"),
            ("outcome", "not_permitted"),
            ("media_kind", "audio"),
        ])
        .assert_delta(1);

    // Four malformed lists from A (a non-host): within A's server-mute
    // refusal-reply burst, so every one is answered.
    const AUDIO: i32 = MediaKind::Audio as i32;
    let malformed: [(&str, Vec<(i32, bool)>); 4] = [
        ("no_kind_named", vec![]),
        ("duplicate_kind", vec![(AUDIO, true), (AUDIO, false)]),
        (
            "media_kind_unspecified",
            vec![(MediaKind::Unspecified as i32, true)],
        ),
        ("not_published", vec![(MediaKind::VideoScreen as i32, true)]),
    ];
    for (reason, changes) in &malformed {
        let snap = MetricAssertion::snapshot();
        a.write(server_mute_changes_frame(&b.participant_id, changes))
            .await;
        let reply = next_error(&mut a).await;
        assert_eq!(
            reply.encode_to_vec(),
            not_permitted.encode_to_vec(),
            "{reason}: a non-host's refusal is byte-identical whatever it sends"
        );
        assert_rejected_exactly(&snap, "server_mute", reason);
        assert_no_per_kind_mute_cell_moved(&snap);
    }
    let _ = stack;
}

/// Test plan 1: every join refusal for the published kinds leaves NO
/// trace -- the exact `error_type` cell, no roster entry, and no
/// `ParticipantJoined` at an existing peer, fenced by a later VALID join being
/// the peer's next join notice. The duplicate case has CONFLICTING values, so
/// refusal (not last-wins) is what is proven.
#[tokio::test]
async fn a_refused_join_leaves_no_trace() {
    const A: i32 = MediaKind::Audio as i32;
    let cases: [Vec<(i32, bool)>; 4] = [
        vec![(MediaKind::Unspecified as i32, false)],
        vec![(4242, false)],
        vec![(MediaKind::VideoScreen as i32, false)],
        vec![(A, true), (A, false)],
    ];
    let (stack, rig) = start_stack("join-refused").await;
    const M: &str = "join-refused-m";
    seed_meeting_with_mh(&stack, M).await;
    let mut peer = join_as(&rig, &stack, M, "user-peer").await;
    for (i, publish) in cases.into_iter().enumerate() {
        let snap = MetricAssertion::snapshot();
        let shape = JoinShape {
            publish,
            ..JoinShape::default()
        };
        let refused = try_join_shaped(&rig, &stack, M, &format!("user-bad-{i}"), shape).await;
        let err = refused.err().expect("refused");
        assert!(
            err.contains("InvalidRequest") && err.contains("Invalid join request"),
            "case {i}: the one generic refusal, got {err}"
        );
        // Exactly one join-failure cell moved: every other join-reachable
        // `error_type` (the zero-init set, itself pinned to `McError`) did not.
        assert!(
            JOIN_FAILURE_ERROR_TYPES.contains(&"publish_kind_invalid"),
            "positive control: the expected cell is in the iterated set"
        );
        for e in JOIN_FAILURE_ERROR_TYPES {
            snap.counter("mc_session_join_failures_total")
                .with_labels(&[("error_type", e)])
                .assert_delta(u64::from(*e == "publish_kind_invalid"));
        }
    }
    // Fence: the next join notice the peer sees is the VALID joiner's.
    let good = join_as(&rig, &stack, M, "user-good").await;
    let joined = loop {
        let m = peer.next_server_message().await.expect("the join notice");
        if let Some(server_message::Message::ParticipantJoined(j)) = m.message {
            break j.participant.expect("an entry");
        }
    };
    assert_eq!(
        joined.participant_id, good.participant_id,
        "no refused joiner was announced"
    );
    assert_eq!(
        good.existing_participants
            .iter()
            .map(|p| p.participant_id.clone())
            .collect::<Vec<_>>(),
        vec![peer.participant_id.clone()],
        "no refused joiner is on the roster"
    );
}

/// Test plan 3 (no-compat): an EMPTY `published_kinds` is a legal receive-only
/// join -- the only observable meaning of an absent list -- and peers see the
/// joiner with EVERY known kind NOT_PUBLISHED (all three entries).
#[tokio::test]
async fn an_empty_publish_list_joins_receive_only_with_every_kind_not_published() {
    let (stack, rig) = start_stack("join-recv-only").await;
    const M: &str = "join-recv-only-m";
    seed_meeting_with_mh(&stack, M).await;
    let mut peer = join_as(&rig, &stack, M, "user-peer").await;
    let shape = JoinShape {
        publish: Vec::new(),
        ..JoinShape::default()
    }
    .receiving(vec![slot(0, MediaKind::Audio)]);
    let ro = try_join_shaped(&rig, &stack, M, "user-ro", shape)
        .await
        .expect("receive-only is admitted");
    let joined = loop {
        let m = peer.next_server_message().await.expect("the join notice");
        if let Some(server_message::Message::ParticipantJoined(j)) = m.message {
            break j.participant.expect("an entry");
        }
    };
    assert_eq!(joined.participant_id, ro.participant_id);
    assert_eq!(joined.media_states.len(), 3, "one entry per known kind");
    for kind in [
        MediaKind::Audio,
        MediaKind::VideoCamera,
        MediaKind::VideoScreen,
    ] {
        assert_eq!(
            client_of(&joined.media_states, kind),
            NOT_PUBLISHED,
            "{kind:?}"
        );
        assert_eq!(
            server_of(&joined.media_states, kind),
            (false, String::new())
        );
    }
}
