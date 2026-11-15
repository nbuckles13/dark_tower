//! The MC-owned, versioned, ref-addressed handler set and register-before-token
//! through the real meeting actor (ADR-0039 §3/§4; story 4 MC-A: R-12, R-17).
//!
//! Actor-level (`MeetingActor::spawn*` + handle joins), every participant's
//! outbound queue wired to an `mpsc` channel so each test reads the frames a
//! client would receive after its `JoinResponse`, and a per-test
//! `MockMhRegistrationClient` with per-handler holds and failures (the delayed
//! or absent `RegisterMeeting` ack).
//!
//! # Paused time, and proving WHICH path released a park
//!
//! `current_thread` + `start_paused`: no wall-clock sleeps. Awaiting a parked
//! join on a paused runtime auto-advances the clock to the next timer — the
//! park deadline — so a broken release-on-verdict would still be released by
//! the deadline. Every test therefore pins the release instant: a verdict
//! release happens strictly before `FIRST_JOIN_PARK_DEADLINE`, a deadline
//! release exactly at it. "Still parked" is always a POLL (`is_finished` after
//! `yield_now`), never an await that could advance time.
//!
//! # The wire-order rule these tests read
//!
//! On a participant's ordered queue every `SendTarget` / `StreamAssignment`
//! `handler_ref` must be a member of the latest set the client holds: its
//! `JoinResponse` set, updated by each `MediaHandlerSet` before it. The
//! connection writes the `JoinResponse` before forwarding the queue
//! (`webtransport/connection.rs` Step 8, then the bridge loop), so the queue
//! read here is exactly what follows it on the wire.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use proto_gen::dark_tower::signaling::v1;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use ::common::secret::SecretBox;
use mc_service::actors::join_park::FIRST_JOIN_PARK_DEADLINE;
use mc_service::actors::{
    ActorMetrics, ControllerMetrics, JoinMedia, JoinResult, MeetingActor, MeetingActorHandle,
    MeetingSeams,
};
use mc_service::errors::McError;
use mc_service::media_signaling::ReceiveCapabilityDeclaration;
use mc_service::redis::MhEndpointInfo;
use proto_gen::dark_tower::signaling::v1::{
    server_message, MediaKind, ReceiveCapability, SlotState,
};
use test_common::media_session::slot;
use test_common::run_ready;
use test_common::{
    build_test_stack, client_media_config, handler_set, join_media_on, mh_handler,
    seed_meeting_with_handlers, test_join_media, MockMhRegistrationClient,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const H_A: &str = "mh-a";
const H_B: &str = "mh-b";

fn spawn_meeting(meeting_id: &str, handlers: &[MhEndpointInfo]) -> MeetingActorHandle {
    spawn_meeting_with(meeting_id, handlers, MeetingSeams::default())
}

fn spawn_meeting_with(
    meeting_id: &str,
    handlers: &[MhEndpointInfo],
    seams: MeetingSeams,
) -> MeetingActorHandle {
    let (handle, _task) = MeetingActor::spawn_with_seams(
        meeting_id.to_string(),
        CancellationToken::new(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        SecretBox::new(Box::new(vec![0u8; 32])),
        mc_test_utils::kek::kek_lifecycle(),
        handler_set(handlers),
        &seams,
    )
    .expect("system CSPRNG must be available in tests");
    handle
}

/// A join in flight: its reply (parked or not) and its outbound queue.
struct Joining {
    reply: JoinHandle<Result<JoinResult, McError>>,
    rx: mpsc::Receiver<bytes::Bytes>,
}

/// One audio receive slot, declared at join (so the joiner gets assignments).
fn receiving_one_audio_slot(mut media: JoinMedia) -> JoinMedia {
    media.demand = ReceiveCapabilityDeclaration::parse(
        &ReceiveCapability {
            slots: vec![slot(0, MediaKind::Audio)],
        },
        true,
    )
    .expect("valid declaration")
    .truncate(client_media_config().receive_slot_caps);
    media
}

fn start_join(handle: &MeetingActorHandle, id: &str, media: JoinMedia) -> Joining {
    let (tx, rx) = mpsc::channel::<bytes::Bytes>(256);
    let handle = handle.clone();
    let id = id.to_string();
    let reply = tokio::spawn(async move {
        handle
            .connection_join(
                format!("conn-{id}"),
                format!("user-{id}"),
                id.clone(),
                String::new(),
                false,
                test_common::test_identity_key(),
                Some(tx),
                media,
            )
            .await
    });
    Joining { reply, rx }
}

/// Is this join's reply still parked? A poll, never an await.
async fn is_parked(j: &Joining) -> bool {
    run_ready().await;
    !j.reply.is_finished()
}

/// Every frame queued so far, decoded, in order.
fn frames(rx: &mut mpsc::Receiver<bytes::Bytes>) -> Vec<server_message::Message> {
    test_common::drain_frames(rx)
}

/// A set's membership — version, then (url, ref) per entry — without its
/// per-participant `media_dial` tokens (MC-D mints those per recipient).
fn membership_of(set: &v1::MediaHandlerSet) -> (u64, Vec<(String, u32)>) {
    (
        set.handler_set_version,
        set.handlers
            .iter()
            .map(|h| (h.media_handler_url.clone(), h.handler_ref))
            .collect(),
    )
}

fn refs_of(set: &v1::MediaHandlerSet) -> BTreeSet<u32> {
    set.handlers.iter().map(|h| h.handler_ref).collect()
}

/// The wire-order rule (module doc): walk the queue tracking the client's
/// current set; every ref a target or assignment names must be a member at
/// that point. The one non-fault 0 is a NO-EDGE `StreamAssignment`
/// (`signaling.proto` at `StreamAssignment.handler_ref`): ref 0 iff the url is
/// empty iff the slot state is one of the three edgeless states. A
/// `SendTarget` never carries 0. Returns how many refs were checked and the
/// final set.
fn assert_refs_are_members(
    start: BTreeSet<u32>,
    queue: &[server_message::Message],
) -> (usize, BTreeSet<u32>) {
    let mut current = start;
    let mut checked = 0;
    for frame in queue {
        let named: Vec<u32> = match frame {
            server_message::Message::MediaHandlerSet(set) => {
                current = set.handlers.iter().map(|h| h.handler_ref).collect();
                continue;
            }
            server_message::Message::SendDirective(d) => d
                .streams
                .iter()
                .flat_map(|s| s.targets.iter().map(|t| t.handler_ref))
                .collect(),
            server_message::Message::StreamAssignments(a) => a
                .assignments
                .iter()
                .filter(|a| {
                    let edgeless = [
                        SlotState::Unavailable,
                        SlotState::FewerSourcesThanSlots,
                        SlotState::AvatarNoVideo,
                    ]
                    .contains(&a.slot_state());
                    assert_eq!(a.handler_ref == 0, edgeless, "ref 0 iff edgeless: {a:?}");
                    !edgeless
                })
                .map(|a| a.handler_ref)
                .collect(),
            _ => continue,
        };
        for r in named {
            assert!(
                current.contains(&r),
                "ref {r} named before the client's set held it (set {current:?})"
            );
            checked += 1;
        }
    }
    (checked, current)
}

fn handler_sets(queue: &[server_message::Message]) -> Vec<(u64, Vec<(String, u32)>)> {
    queue
        .iter()
        .filter_map(|f| match f {
            server_message::Message::MediaHandlerSet(s) => Some((
                s.handler_set_version,
                s.handlers
                    .iter()
                    .map(|h| (h.media_handler_url.clone(), h.handler_ref))
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

// ============================================================================
// T1: the first join waits for the ack without stalling the meeting
// ============================================================================

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn first_join_parks_until_the_ack_without_stalling_the_meeting() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_A);
    let meeting = spawn_meeting("t1", &[mh_handler(H_A)]);
    let t0 = Instant::now();

    // A receives one audio slot; B publishes audio.
    let mut a = start_join(
        &meeting,
        "a",
        receiving_one_audio_slot(join_media_on(Arc::clone(&mh))),
    );
    let mut b = start_join(&meeting, "b", join_media_on(Arc::clone(&mh)));
    assert!(is_parked(&a).await, "A's reply waits for the ack");
    assert!(
        is_parked(&b).await,
        "B, arriving before the verdict, parks too"
    );

    // The mailbox is live while both replies are parked: admission completed
    // (both are on the roster) and an MH notification is processed.
    let state = meeting.get_state().await.unwrap();
    assert_eq!(state.participants.len(), 2);
    meeting
        .media_connected("user-a".into(), H_A.into(), "conn-a-mh".into())
        .await
        .expect("served while parked");
    assert!(is_parked(&a).await, "still parked: no ack yet");

    mh.release_handler(H_A);
    let a_join = (&mut a.reply).await.unwrap().expect("A joins");
    let b_join = (&mut b.reply).await.unwrap().expect("B joins");
    assert!(
        t0.elapsed() < FIRST_JOIN_PARK_DEADLINE,
        "released by the ack, not the deadline ({:?})",
        t0.elapsed()
    );

    for j in [&a_join, &b_join] {
        assert_eq!(j.handler_set.handlers.len(), 1);
        assert_eq!(
            j.handler_set.handlers[0].media_handler_url,
            "wt://mh-a:4433"
        );
        assert!(j.handler_set.handler_set_version > 0);
    }
    assert_eq!(
        a_join.handler_set.handler_set_version, b_join.handler_set.handler_set_version,
        "one snapshot at release"
    );
    assert_eq!(
        refs_of(&a_join.handler_set),
        refs_of(&b_join.handler_set),
        "one snapshot at release"
    );
    // Each joiner's set carries a token minted for IT (MC-D): never empty,
    // and never the other joiner's.
    assert!(!a_join.handler_set.handlers[0].media_dial_token.is_empty());
    // `assert!`, never `assert_ne!`: a failure must not print a token.
    assert!(
        a_join.handler_set.handlers[0].media_dial_token
            != b_join.handler_set.handlers[0].media_dial_token
    );

    // A join after the verdict replies at once, with the same membership.
    let c_started = Instant::now();
    let mut c = start_join(&meeting, "c", join_media_on(Arc::clone(&mh)));
    let c_join = (&mut c.reply).await.unwrap().expect("C joins");
    assert_eq!(
        c_started.elapsed(),
        Duration::ZERO,
        "no park after the verdict"
    );
    assert_eq!(
        membership_of(&c_join.handler_set),
        membership_of(&a_join.handler_set)
    );

    // B reaches mh-a too, and both settle: the B -> A edge now names mh-a's
    // ref on A's assignment and B's target, AFTER their JoinResponse set.
    meeting
        .media_connected("user-b".into(), H_A.into(), "conn-b-mh".into())
        .await
        .unwrap();
    tokio::time::advance(client_media_config().connect_settle_window).await;
    run_ready().await;
    meeting.get_state().await.unwrap();

    // The parked joiners got the set in their JoinResponse, not on the queue;
    // every ref on every queue is a member of the set its client holds.
    let mut checked = 0;
    for (join, queue) in [
        (&a_join, frames(&mut a.rx)),
        (&b_join, frames(&mut b.rx)),
        (&c_join, frames(&mut c.rx)),
    ] {
        assert!(handler_sets(&queue).is_empty(), "no redundant set message");
        checked += assert_refs_are_members(refs_of(&join.handler_set), &queue).0;
    }
    assert!(checked > 0, "non-vacuous: the B -> A edge named mh-a's ref");
}

// ============================================================================
// T2: no set (or target, or assignment) names a handler before its ack
// ============================================================================

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_set_names_a_handler_before_its_registermeeting_ack() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_B);
    let meeting = spawn_meeting("t2", &[mh_handler(H_A), mh_handler(H_B)]);
    // mh-a is a live (MH-2+) handler: it heartbeats once acked, so the park
    // deadline (longer than N) does not read as its death.
    mh.heartbeat_to_meeting(&meeting);
    let t0 = Instant::now();

    // A receives one audio slot; C publishes audio.
    let mut a = start_join(
        &meeting,
        "a",
        receiving_one_audio_slot(join_media_on(Arc::clone(&mh))),
    );
    let mut c = start_join(&meeting, "c", join_media_on(Arc::clone(&mh)));
    // mh-a acks at once; the park waits for EVERY initial handler.
    assert!(
        is_parked(&a).await,
        "mh-a acked, mh-b has not: still parked"
    );

    // Release by the deadline, exactly at it.
    test_common::advance_live(FIRST_JOIN_PARK_DEADLINE - Duration::from_millis(1)).await;
    assert!(is_parked(&a).await, "not one tick early");
    tokio::time::advance(Duration::from_millis(1)).await;
    let a_join = (&mut a.reply).await.unwrap().expect("A joins");
    let c_join = (&mut c.reply).await.unwrap().expect("C joins");
    assert_eq!(t0.elapsed(), FIRST_JOIN_PARK_DEADLINE);

    let urls: Vec<&str> = a_join
        .handler_set
        .handlers
        .iter()
        .map(|h| h.media_handler_url.as_str())
        .collect();
    assert_eq!(
        urls,
        vec!["wt://mh-a:4433"],
        "exactly {{mh-a}}: mh-b is unacked"
    );
    let v1 = a_join.handler_set.handler_set_version;

    // The DISTINGUISHING input: A reaches both handlers, C reaches ONLY mh-b,
    // so mh-b is the pair's only shared handler. Routing over a Registering
    // handler would put the C -> A edge on mh-b and name its ref.
    for (user, h) in [("user-a", H_A), ("user-a", H_B), ("user-c", H_B)] {
        meeting
            .media_connected(user.into(), h.into(), format!("conn-{user}-{h}"))
            .await
            .unwrap();
    }
    tokio::time::advance(client_media_config().connect_settle_window).await;
    run_ready().await;
    meeting.get_state().await.unwrap();
    let a_before = frames(&mut a.rx);
    let c_before = frames(&mut c.rx);
    assert_refs_are_members(refs_of(&a_join.handler_set), &a_before);
    assert_refs_are_members(refs_of(&c_join.handler_set), &c_before);

    // mh-b acks: exactly one full-snapshot set {mh-a, mh-b} at the next
    // version (one mark_active), ahead of anything naming mh-b's ref.
    mh.release_handler(H_B);
    run_ready().await;
    tokio::time::advance(client_media_config().connect_settle_window).await;
    run_ready().await;
    meeting.get_state().await.unwrap();
    let a_after = frames(&mut a.rx);
    let sets = handler_sets(&a_after);
    assert_eq!(sets.len(), 1, "one set message per membership change");
    let (v2, members) = &sets[0];
    assert_eq!(*v2, v1 + 1, "exactly one mark_active");
    let urls: BTreeSet<&str> = members.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(urls, BTreeSet::from(["wt://mh-a:4433", "wt://mh-b:4433"]));
    let b_ref = members
        .iter()
        .find(|(u, _)| u == "wt://mh-b:4433")
        .map(|(_, r)| *r)
        .unwrap();
    assert_ne!(b_ref, 0);

    // Negative arm (now that mh-b's ref is known): nothing before the ack
    // named it, on either queue.
    for queue in [&a_before, &c_before] {
        assert_eq!(
            first_naming(queue, b_ref),
            None,
            "mh-b named before its ack"
        );
    }
    // Positive arm: after the ack the C -> A edge sits on mh-b, so A's view
    // names its ref — strictly AFTER the set that introduced it.
    let set_at = a_after
        .iter()
        .position(|f| matches!(f, server_message::Message::MediaHandlerSet(_)))
        .unwrap();
    let named_at = first_naming(&a_after, b_ref).expect("A's view names mh-b's ref after the ack");
    assert!(
        set_at < named_at,
        "the set precedes the first frame naming its new ref"
    );
    assert_refs_are_members(refs_of(&a_join.handler_set), &a_after);
    let c_after = frames(&mut c.rx);
    assert_refs_are_members(refs_of(&c_join.handler_set), &c_after);
}

/// The index of the first frame naming `handler_ref` on a target or an
/// assignment.
fn first_naming(queue: &[server_message::Message], handler_ref: u32) -> Option<usize> {
    queue.iter().position(|f| match f {
        server_message::Message::SendDirective(d) => d
            .streams
            .iter()
            .any(|s| s.targets.iter().any(|t| t.handler_ref == handler_ref)),
        server_message::Message::StreamAssignments(a) => {
            a.assignments.iter().any(|a| a.handler_ref == handler_ref)
        }
        _ => false,
    })
}

// ============================================================================
// T3: the initial handler never acks — the join still succeeds, empty
// ============================================================================

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_initial_handler_that_never_acks_yields_a_successful_join_with_an_empty_set() {
    // Arm 1: every push FAILS — released by the pusher's exhausted retries,
    // before the deadline.
    let failing = Arc::new(MockMhRegistrationClient::new());
    failing.fail_handler(H_A);
    let meeting = spawn_meeting("t3-fail", &[mh_handler(H_A)]);
    let t0 = Instant::now();
    let mut a = start_join(&meeting, "a", join_media_on(Arc::clone(&failing)));
    let joined = (&mut a.reply)
        .await
        .unwrap()
        .expect("the join never fails on a dead MH");
    let elapsed = t0.elapsed();
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < FIRST_JOIN_PARK_DEADLINE,
        "released by the exhausted retries (1 s + 2 s backoff), not the deadline: {elapsed:?}"
    );
    assert!(joined.handler_set.handlers.is_empty());
    assert!(
        handler_sets(&frames(&mut a.rx)).is_empty(),
        "and nothing names the handler afterwards"
    );

    // Arm 2: the push HANGS — released exactly at the deadline.
    let hanging = Arc::new(MockMhRegistrationClient::new());
    hanging.hold_handler(H_A);
    let meeting = spawn_meeting("t3-hang", &[mh_handler(H_A)]);
    let t0 = Instant::now();
    let mut a = start_join(&meeting, "a", join_media_on(Arc::clone(&hanging)));
    assert!(is_parked(&a).await, "parked (the deadline is armed now)");
    test_common::advance_live(FIRST_JOIN_PARK_DEADLINE - Duration::from_millis(1)).await;
    assert!(is_parked(&a).await, "not one tick early");
    tokio::time::advance(Duration::from_millis(1)).await;
    let joined = (&mut a.reply)
        .await
        .unwrap()
        .expect("joins at the deadline");
    assert_eq!(t0.elapsed(), FIRST_JOIN_PARK_DEADLINE);
    assert!(joined.handler_set.handlers.is_empty());

    // Positive control: an acking handler IS listed (so "empty" above is not
    // vacuous).
    let acking = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("t3-ok", &[mh_handler(H_A)]);
    let mut a = start_join(&meeting, "a", join_media_on(acking));
    let joined = (&mut a.reply).await.unwrap().unwrap();
    assert_eq!(joined.handler_set.handlers.len(), 1);
}

// ============================================================================
// T4: the version advances only on membership change
// ============================================================================

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_version_advances_on_membership_change_and_on_nothing_else() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    let meeting = spawn_meeting("t4", &[mh_handler(H_A)]);
    let mut a = start_join(&meeting, "a", join_media_on(Arc::clone(&mh)));
    let a_join = (&mut a.reply).await.unwrap().unwrap();
    // Positive arm: the hand-off add is version 1; mark-active advanced it.
    assert_eq!(a_join.handler_set.handler_set_version, 2);

    // Negative arm: joins, a declaration and connectivity change nothing.
    let mut later = Vec::new();
    for id in ["b", "c", "d"] {
        let mut j = start_join(
            &meeting,
            id,
            receiving_one_audio_slot(join_media_on(Arc::clone(&mh))),
        );
        later.push((&mut j.reply).await.unwrap().unwrap());
    }
    meeting
        .media_connected("user-b".into(), H_A.into(), "conn-b-mh".into())
        .await
        .unwrap();
    let mut e = start_join(&meeting, "e", join_media_on(Arc::clone(&mh)));
    later.push((&mut e.reply).await.unwrap().unwrap());
    for j in &later {
        assert_eq!(
            membership_of(&j.handler_set),
            membership_of(&a_join.handler_set),
            "same version and refs"
        );
    }
    assert!(
        handler_sets(&frames(&mut a.rx)).is_empty(),
        "no set re-sent"
    );
}

// ============================================================================
// T5: a later hand-off / Redis change does not alter a live meeting
// ============================================================================

#[tokio::test(flavor = "current_thread")]
async fn a_later_assignment_change_does_not_alter_a_live_meetings_handlers() {
    let stack = build_test_stack("hs-t5").await;
    seed_meeting_with_handlers(&stack, "t5", vec![mh_handler(H_A)]).await;

    // Premise: GC re-assigning the live meeting (the path that rewrites its
    // Redis record) cannot reach the actor — the create is refused.
    let reassign = stack
        .controller_handle
        .create_meeting(
            "t5".to_string(),
            ::common::meeting_lifecycle::AssignmentId::new_v4(),
            handler_set(&[mh_handler("mh-evil")]),
        )
        .await;
    assert!(
        matches!(reassign, Err(McError::Conflict(_))),
        "a re-assign of a live meeting is refused: {reassign:?}"
    );

    let meeting = stack
        .controller_handle
        .get_meeting_handle("t5".to_string())
        .await
        .unwrap();
    let mut first = start_join(&meeting, "a", test_join_media(&stack));
    let first = (&mut first.reply).await.unwrap().unwrap();
    let mut later = start_join(&meeting, "b", test_join_media(&stack));
    let later = (&mut later.reply).await.unwrap().unwrap();
    assert_eq!(
        membership_of(&later.handler_set),
        membership_of(&first.handler_set)
    );
    let urls: Vec<&str> = later
        .handler_set
        .handlers
        .iter()
        .map(|h| h.media_handler_url.as_str())
        .collect();
    assert_eq!(
        urls,
        vec!["wt://mh-a:4433"],
        "never the re-assigned handler"
    );

    // Positive control: a NEW meeting assigned the other hand-off lists it.
    seed_meeting_with_handlers(&stack, "t5-new", vec![mh_handler("mh-evil")]).await;
    let fresh = stack
        .controller_handle
        .get_meeting_handle("t5-new".to_string())
        .await
        .unwrap();
    let mut c = start_join(&fresh, "c", test_join_media(&stack));
    let c = (&mut c.reply).await.unwrap().unwrap();
    assert_eq!(
        c.handler_set.handlers[0].media_handler_url,
        "wt://mh-evil:4433"
    );
}

// ============================================================================
// T7: a parked reply is answered when the meeting ends — never leaked
// ============================================================================

/// Answered `MeetingTeardownInProgress` (since the join-window devloop: an
/// ENDING meeting refuses with the code the SDK re-joins through GC on;
/// `Draining` is MC-wide shutdown only) — an `Err`, so it carries no KEK, no
/// handler set and no dial token (security R1).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_parked_join_is_answered_teardown_in_progress_when_the_meeting_ends() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_A);
    let meeting = spawn_meeting("t7", &[mh_handler(H_A)]);
    let t0 = Instant::now();
    let mut a = start_join(&meeting, "a", join_media_on(Arc::clone(&mh)));
    assert!(is_parked(&a).await);
    meeting.close_meeting("test".to_string()).await.unwrap();
    let reply = (&mut a.reply).await.unwrap();
    assert!(
        matches!(reply, Err(McError::MeetingTeardownInProgress)),
        "answered TeardownInProgress, not dropped: {reply:?}"
    );
    assert!(
        t0.elapsed() < FIRST_JOIN_PARK_DEADLINE,
        "by the ending, not the deadline"
    );
}

// ============================================================================
// T8 (security S-9): a KEK rotation during the park reaches the parked joiner
// ============================================================================

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_rotation_during_the_park_releases_the_joiner_with_the_current_kek() {
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_A);
    let meeting = spawn_meeting_with(
        "t8",
        &[mh_handler(H_A)],
        MeetingSeams {
            kek_debounce_manual: true,
            ..Default::default()
        },
    );
    let mut a = start_join(&meeting, "a", join_media_on(Arc::clone(&mh)));
    let mut b = start_join(&meeting, "b", join_media_on(Arc::clone(&mh)));
    assert!(is_parked(&a).await && is_parked(&b).await);

    // B leaves while both are parked; the leave rotation fires.
    meeting
        .participant_leave("b".to_string(), "conn-b".to_string())
        .await
        .unwrap();
    assert!(
        meeting.force_kek_rotation().await.unwrap(),
        "a leave was pending"
    );

    mh.release_handler(H_A);
    let a_join = (&mut a.reply).await.unwrap().unwrap();
    // SEC-1: the departed parked joiner is refused — it never receives the
    // generation minted to exclude it.
    let b_reply = (&mut b.reply).await.unwrap();
    assert!(
        matches!(b_reply, Err(McError::Draining)),
        "a parked joiner removed during the park is refused, never handed the KEK: {b_reply:?}"
    );
    assert!(
        frames(&mut b.rx).iter().all(|f| !matches!(
            f,
            server_message::Message::MeetingKekUpdate(u) if u.kek_generation >= 1
        )),
        "B's queue never carries the post-removal generation"
    );
    assert_eq!(
        a_join.kek_generation, 1,
        "released with the CURRENT generation, never the one the leaver holds"
    );
    let queue = frames(&mut a.rx);
    for frame in &queue {
        if let server_message::Message::MeetingKekUpdate(u) = frame {
            assert!(
                u.kek_generation >= u32::from(a_join.kek_generation),
                "anything queued behind the JoinResponse is redundant or newer"
            );
            assert_eq!(
                u.meeting_kek.as_slice(),
                a_join.meeting_kek.expose().as_slice()
            );
        }
    }
    let held = meeting
        .get_state()
        .await
        .unwrap()
        .participants
        .into_iter()
        .find(|p| p.participant_id == "a")
        .expect("A is on the roster");
    assert_eq!(
        held.sender_id, a_join.sender_id,
        "A's sender_id is the one MC holds"
    );
}
