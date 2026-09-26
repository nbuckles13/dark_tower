//! KEK lifecycle through the real meeting actor (ADR-0036 §4 Rotation; story 2
//! R-12, R-14, R-15, R-16, R-26).
//!
//! Actor-level, with every participant's outbound stream wired to an `mpsc`
//! channel, so each test reads the ACTUAL `MeetingKekUpdate` frames a client
//! would receive — not an internal flag.
//!
//! # No test waits out W
//!
//! Every meeting here is built with `MeetingSeams::kek_debounce_manual`: the
//! actor never auto-fires the leave rotation, and the test fires it with
//! `force_kek_rotation()` AFTER the departures are recorded. Coalescing then
//! holds structurally rather than by racing a small real window (ADR-0028
//! zero-retry). The only time that passes is tokio's paused virtual clock,
//! advanced past the push-outcome deadline so the detached collector has
//! recorded every outcome before counters are read.
//!
//! # `current_thread` is load-bearing
//!
//! `MetricAssertion` binds a per-thread recorder, and the rotation's metrics are
//! emitted by the meeting actor and by the detached outcome collector — both
//! spawned tasks. On `current_thread` they share the test thread; on a
//! multi-thread runtime they would land on a worker and be silently missed.
//!
//! # At most ONE histogram query per snapshot, and it goes FIRST
//!
//! Every snapshot read drains ALL histogram observations — any query, of any
//! kind, empties every histogram. So a histogram query placed after any other
//! query reads 0, and a second histogram query always does. The rotation
//! duration histogram is therefore asserted in the collector's own unit test
//! (`media_admission::rotation`), not here.
//!
//! Registration cost: none (actor-level, no AC).

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use ::common::secret::SecretBox;
use mc_service::actors::{
    ActorMetrics, ControllerMetrics, DisconnectCause, JoinResult, MeetingActor, MeetingActorHandle,
    MeetingSeams,
};
use mc_service::media_admission::rotation::KEK_PUSH_OUTCOME_TIMEOUT;
use mc_service::media_admission::KekLifecycle;
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{server_message, MeetingKekUpdate, ServerMessage};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// W as the test lifecycle carries it on the wire — the fixture's single home,
/// not a second literal.
const W_SECONDS: u32 = mc_test_utils::kek::TEST_KEK_ROTATION_WINDOW_SECONDS;

fn spawn_meeting(meeting_id: &str, seams: MeetingSeams) -> (MeetingActorHandle, Arc<KekLifecycle>) {
    let lifecycle = mc_test_utils::kek::kek_lifecycle();
    let (handle, _task) = MeetingActor::spawn_with_seams(
        meeting_id.to_string(),
        CancellationToken::new(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        SecretBox::new(Box::new(vec![0u8; 32])),
        Arc::clone(&lifecycle),
        &MeetingSeams {
            kek_debounce_manual: true,
            ..seams
        },
    )
    .expect("system CSPRNG must be available in tests");
    (handle, lifecycle)
}

/// A joined participant with its outbound stream wired, so its frames can be
/// read back.
struct Member {
    id: String,
    join: JoinResult,
    rx: mpsc::Receiver<bytes::Bytes>,
}

async fn join(handle: &MeetingActorHandle, id: &str) -> Member {
    let (tx, rx) = mpsc::channel::<bytes::Bytes>(64);
    let join = handle
        .connection_join(
            format!("conn-{id}"),
            format!("user-{id}"),
            id.to_string(),
            String::new(),
            false,
            test_common::test_identity_key(),
            Some(tx),
            test_common::standalone_join_media(),
        )
        .await
        .expect("admission must succeed");
    Member {
        id: id.to_string(),
        join,
        rx,
    }
}

async fn disconnect(handle: &MeetingActorHandle, m: &Member, cause: DisconnectCause) {
    handle
        .connection_disconnected(format!("conn-{}", m.id), m.id.clone(), cause)
        .await
        .unwrap();
    // FIFO round-trip: the disconnect has been fully processed once this returns.
    handle.get_state().await.unwrap();
}

/// Let the detached outcome collector finish. Virtual time only (paused
/// runtime): nothing here waits on the wall clock.
async fn settle_outcomes() {
    tokio::time::sleep(KEK_PUSH_OUTCOME_TIMEOUT + Duration::from_millis(100)).await;
}

/// Every frame a member has received so far, decoded, in order.
fn frames(m: &mut Member) -> Vec<server_message::Message> {
    let mut out = Vec::new();
    while let Ok(bytes) = m.rx.try_recv() {
        if let Some(msg) = ServerMessage::decode(bytes.as_ref()).unwrap().message {
            out.push(msg);
        }
    }
    out
}

fn kek_updates(frames: &[server_message::Message]) -> Vec<MeetingKekUpdate> {
    frames
        .iter()
        .filter_map(|f| match f {
            server_message::Message::MeetingKekUpdate(u) => Some(u.clone()),
            _ => None,
        })
        .collect()
}

fn sender_id(m: &Member) -> u16 {
    m.join.sender_id.get().get()
}

// ============================================================================
// S6: the rotation reaches exactly the post-removal members (R-12)
// ============================================================================

/// A leave rotates the KEK and pushes it to exactly the members present after
/// the removal. The leaver is unreachable BY CONSTRUCTION — it is off the roster
/// the push iterates — so it receives nothing.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rotation_on_leave_reaches_exactly_the_post_removal_members() {
    let snap = MetricAssertion::snapshot();
    let (handle, lifecycle) = spawn_meeting("m-s6", MeetingSeams::default());
    let mut a = join(&handle, "a").await;
    let mut b = join(&handle, "b").await;
    let mut c = join(&handle, "c").await;
    let old_kek = *a.join.meeting_kek.expose();

    disconnect(&handle, &c, DisconnectCause::ClientClosed).await;

    // The leave registered as pending in the fleet registry, anchored at the
    // removal: its age grows with the clock until the rotation clears it.
    tokio::time::advance(Duration::from_secs(5)).await;
    let (age, _) = lifecycle.sample(tokio::time::Instant::now()).await;
    assert!(
        age >= Duration::from_secs(5),
        "pending age must grow; got {age:?}"
    );

    assert!(
        handle.force_kek_rotation().await.unwrap(),
        "a leave was pending"
    );
    settle_outcomes().await;
    let (age, _) = lifecycle.sample(tokio::time::Instant::now()).await;
    assert_eq!(
        age,
        Duration::ZERO,
        "cleared at rotation, not at push success"
    );

    let a_updates = kek_updates(&frames(&mut a));
    let b_updates = kek_updates(&frames(&mut b));
    assert_eq!(
        a_updates.len(),
        1,
        "each remaining member gets exactly one update"
    );
    assert_eq!(b_updates.len(), 1);
    let update = &a_updates[0];
    assert_eq!(update.kek_generation, 1);
    assert_eq!(
        update.kek_rotation_debounce_seconds, W_SECONDS,
        "W rides every update"
    );
    assert_eq!(update.meeting_kek.len(), 32);
    assert_ne!(
        update.meeting_kek.as_slice(),
        old_kek,
        "a rotation that keeps the key is not one"
    );
    assert_eq!(
        b_updates[0].meeting_kek, update.meeting_kek,
        "one KEK per generation"
    );

    assert!(
        kek_updates(&frames(&mut c)).is_empty(),
        "the leaver must never receive the post-removal KEK"
    );

    snap.histogram("mc_meeting_kek_rotation_coalesced_leaves")
        .assert_observation_count(1);
    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "participant_left")])
        .assert_delta(1);
    snap.counter("mc_meeting_kek_pushes_total")
        .with_labels(&[("outcome", "delivered")])
        .assert_delta(2);
    for outcome in [
        "dropped_outbound",
        "participant_gone",
        "actor_unavailable",
        "timed_out",
    ] {
        snap.counter("mc_meeting_kek_pushes_total")
            .with_labels(&[("outcome", outcome)])
            .assert_delta(0);
    }
    for reason in ["rng", "generation_exhausted"] {
        snap.counter("mc_meeting_kek_rotation_failures_total")
            .with_labels(&[("reason", reason)])
            .assert_delta(0);
    }
    handle.cancel();
}

/// A rotation records how long it took to reach every recipient. Its own
/// snapshot: only one histogram query per snapshot can read correctly.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_rotation_records_its_duration() {
    let snap = MetricAssertion::snapshot();
    let (handle, _) = spawn_meeting("m-duration", MeetingSeams::default());
    let _a = join(&handle, "a").await;
    let b = join(&handle, "b").await;
    disconnect(&handle, &b, DisconnectCause::ClientClosed).await;
    assert!(handle.force_kek_rotation().await.unwrap());
    settle_outcomes().await;
    snap.histogram("mc_meeting_kek_rotation_duration_seconds")
        .assert_observation_count(1);
    handle.cancel();
}

// ============================================================================
// Gauges (R-26): what is published is what is enforced
// ============================================================================

/// W and the overdue threshold are published from the SAME value the debounce
/// and the wire field use, so the page compares two gauges with no arithmetic.
#[tokio::test(flavor = "current_thread")]
async fn the_config_echo_gauges_publish_the_enforced_window_and_threshold() {
    let snap = MetricAssertion::snapshot();
    let lifecycle = mc_test_utils::kek::kek_lifecycle();
    lifecycle.publish_config_gauges();
    let w = mc_test_utils::kek::test_kek_rotation_window().as_secs_f64();
    snap.gauge("mc_meeting_kek_rotation_window_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(w);
    snap.gauge("mc_meeting_kek_rotation_overdue_threshold_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(w * f64::from(mc_service::media_admission::KEK_ROTATION_OVERDUE_MULTIPLIER));
}

/// The fleet gauges sample what the REAL meeting actor wrote: its leave hook
/// records the pending departure, and its admission path records namespace
/// consumption. A gauge fed only by a unit-test registry would pass without
/// the actor ever writing either.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fleet_gauges_sample_what_the_meeting_actor_recorded() {
    let snap = MetricAssertion::snapshot();
    let (handle, lifecycle) = spawn_meeting("m-gauges", MeetingSeams::default());
    let _a = join(&handle, "a").await;
    let b = join(&handle, "b").await;
    disconnect(&handle, &b, DisconnectCause::ClientClosed).await;
    tokio::time::advance(Duration::from_secs(30)).await;

    lifecycle.publish_fleet_gauges().await;
    snap.gauge("mc_meeting_kek_rotation_pending_age_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(30.0);
    snap.gauge("mc_meeting_sender_ids_issued_max")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(2.0);

    // Rotation clears the pending age at once, not when pushes land.
    assert!(handle.force_kek_rotation().await.unwrap());
    lifecycle.publish_fleet_gauges().await;
    snap.gauge("mc_meeting_kek_rotation_pending_age_seconds")
        .with_labels(&[("key_custody", "operator")])
        .assert_value(0.0);
    handle.cancel();
}

// ============================================================================
// S8: departures inside one window coalesce into ONE rotation (R-12)
// ============================================================================

/// Two leaves, one rotation — asserted as ONE event.
///
/// `generated{participant_left}` moving by exactly 1 alone would only say "no
/// second rotation seen yet". The positive control is the SECOND force: it
/// returns `false`, proving nothing was left pending — both departures were
/// folded into the first rotation rather than the second still waiting. No
/// timed negative wait is needed or used.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn leaves_inside_one_window_coalesce_into_one_rotation() {
    let snap = MetricAssertion::snapshot();
    let (handle, _) = spawn_meeting("m-s8", MeetingSeams::default());
    let mut a = join(&handle, "a").await;
    let mut b = join(&handle, "b").await;
    let c = join(&handle, "c").await;
    let d = join(&handle, "d").await;

    disconnect(&handle, &c, DisconnectCause::ClientClosed).await;
    disconnect(&handle, &d, DisconnectCause::ClientClosed).await;

    assert!(
        handle.force_kek_rotation().await.unwrap(),
        "the window fired"
    );
    assert!(
        !handle.force_kek_rotation().await.unwrap(),
        "nothing may remain pending: the second departure must have folded into the first rotation"
    );
    settle_outcomes().await;

    assert_eq!(
        kek_updates(&frames(&mut a)).len(),
        1,
        "one rotation, one update"
    );
    assert_eq!(kek_updates(&frames(&mut b)).len(), 1);

    snap.histogram("mc_meeting_kek_rotation_coalesced_leaves")
        .assert_observation_count(1);
    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "participant_left")])
        .assert_delta(1);
    handle.cancel();
}

// ============================================================================
// Grace members and reconnect re-issue (R-15)
// ============================================================================

/// A member inside the reconnect grace period is counted `participant_gone`
/// (benign), and on reconnect is re-issued the CURRENT KEK, generation and W
/// WITHOUT a rotation.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_grace_member_is_gone_and_reconnect_reissues_the_current_kek_without_rotating() {
    let snap = MetricAssertion::snapshot();
    let (handle, _) = spawn_meeting("m-reconnect", MeetingSeams::default());
    let mut a = join(&handle, "a").await;
    let b = join(&handle, "b").await;
    let c = join(&handle, "c").await;

    disconnect(&handle, &b, DisconnectCause::ConnectionLost).await; // grace
    disconnect(&handle, &c, DisconnectCause::ClientClosed).await; // removed
    assert!(handle.force_kek_rotation().await.unwrap());
    settle_outcomes().await;

    let rotated = kek_updates(&frames(&mut a))
        .pop()
        .expect("A was pushed the new KEK");

    let reconnected = handle
        .connection_reconnect(
            "conn-b-2".to_string(),
            b.join.correlation_id.clone(),
            b.join.binding_token.clone(),
        )
        .await
        .expect("reconnect inside grace");
    assert_eq!(
        reconnected.kek_generation, 1,
        "the CURRENT generation, not the one B joined at"
    );
    assert_eq!(
        reconnected.meeting_kek.expose().as_slice(),
        rotated.meeting_kek.as_slice(),
        "a participant reconnecting after a rotation must not hold a stale KEK"
    );
    assert_eq!(reconnected.kek_rotation_debounce_seconds, W_SECONDS);
    assert!(
        !handle.force_kek_rotation().await.unwrap(),
        "a reconnect is not a removal and must not schedule a rotation"
    );

    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "participant_left")])
        .assert_delta(1);
    snap.counter("mc_meeting_kek_pushes_total")
        .with_labels(&[("outcome", "delivered")])
        .assert_delta(1);
    snap.counter("mc_meeting_kek_pushes_total")
        .with_labels(&[("outcome", "participant_gone")])
        .assert_delta(1);
    handle.cancel();
}

// ============================================================================
// R-16: exhaustion resets the KEK epoch; admission never fails
// ============================================================================

/// At the wall, the joiner is ADMITTED: MC rotates the KEK and reissues from a
/// fresh namespace, and incumbents get the new KEK before they hear of the
/// joiner.
///
/// Two members cross the reset and one is in GRACE (security D-4): a
/// single-joiner test passes whether or not the exclusion set covers grace
/// members, so it would be vacuous against D-1.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn sender_id_exhaustion_resets_the_kek_epoch_and_admission_never_fails() {
    let snap = MetricAssertion::snapshot();
    let (handle, _) = spawn_meeting(
        "m-exhaust",
        MeetingSeams {
            sender_id_cursor: Some(NonZeroU16::new(65534)),
            ..Default::default()
        },
    );
    let mut a = join(&handle, "a").await;
    let b = join(&handle, "b").await;
    assert_eq!((sender_id(&a), sender_id(&b)), (65534, 65535));
    disconnect(&handle, &b, DisconnectCause::ConnectionLost).await; // grace, still bound

    let c = join(&handle, "c").await;
    settle_outcomes().await;

    assert_eq!(
        c.join.kek_generation, 1,
        "the reset always bumps the generation"
    );
    assert_ne!(
        c.join.meeting_kek.expose(),
        a.join.meeting_kek.expose(),
        "and always installs a new KEK"
    );
    assert_eq!(
        sender_id(&c),
        1,
        "the fresh epoch reissues from the start of the space"
    );
    assert_ne!(sender_id(&c), sender_id(&a), "never a live member's id");
    assert_ne!(
        sender_id(&c),
        sender_id(&b),
        "never a GRACE member's id either (D-1)"
    );

    // Ordering: the incumbent gets the new KEK BEFORE it hears of the joiner.
    let a_frames = frames(&mut a);
    let kek_at = a_frames
        .iter()
        .position(|f| matches!(f, server_message::Message::MeetingKekUpdate(_)))
        .expect("A was pushed the new KEK");
    // Match C specifically: A also received ParticipantJoined(B) earlier.
    let joined_at = a_frames
        .iter()
        .position(|f| {
            matches!(f, server_message::Message::ParticipantJoined(j)
                if j.participant.as_ref().is_some_and(|p| p.participant_id == "c"))
        })
        .expect("A heard about C");
    assert!(
        kek_at < joined_at,
        "new KEK must precede ParticipantJoined(C)"
    );
    let MeetingKekUpdate {
        meeting_kek,
        kek_generation,
        ..
    } = kek_updates(&a_frames).pop().unwrap();
    assert_eq!(kek_generation, 1);
    assert_eq!(
        meeting_kek.as_slice(),
        c.join.meeting_kek.expose(),
        "one KEK for the epoch"
    );

    // The grace member keeps its id across the reset.
    handle
        .connection_reconnect(
            "conn-b-2".to_string(),
            b.join.correlation_id.clone(),
            b.join.binding_token.clone(),
        )
        .await
        .expect("reconnect inside grace");
    let state = handle.get_state().await.unwrap();
    let b_now = state
        .participants
        .iter()
        .find(|p| p.participant_id == "b")
        .expect("B is still rostered");
    assert_eq!(
        b_now.sender_id.get().get(),
        65535,
        "a reset never renumbers an incumbent"
    );

    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "sender_space_exhausted")])
        .assert_delta(1);
    snap.counter("mc_meeting_kek_pushes_total")
        .with_labels(&[("outcome", "delivered")])
        .assert_delta(1);
    snap.counter("mc_meeting_kek_pushes_total")
        .with_labels(&[("outcome", "participant_gone")])
        .assert_delta(1);
    handle.cancel();
}

/// The strengthened invariant through the real join path: **within one KEK
/// generation a `sender_id` is bound to at most one identity ever.**
///
/// A carried-over member leaves mid-epoch. A liveness-re-checked allocator
/// would hand its id to the next joiner under the SAME generation — the new
/// holder would land in the old holder's replay bucket at every receiver. The
/// exclusion snapshot is permanent, so instead the cursor runs past it, the
/// epoch resets again, and the released id is reissued only under a HIGHER
/// generation.
///
/// `epoch_reset_cursor` places the reset's exclusion set in the cursor's path;
/// without it this needs ~65,000 joins.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_id_released_mid_epoch_is_reissued_only_under_a_later_generation() {
    let snap = MetricAssertion::snapshot();
    let (handle, _) = spawn_meeting(
        "m-permanent",
        MeetingSeams {
            sender_id_cursor: Some(NonZeroU16::new(65534)),
            epoch_reset_cursor: NonZeroU16::new(65533),
            ..Default::default()
        },
    );
    let a = join(&handle, "a").await; // 65534
    let _b = join(&handle, "b").await; // 65535
    let c = join(&handle, "c").await; // reset -> generation 1, excluded {65534, 65535}
    assert_eq!((sender_id(&c), c.join.kek_generation), (65533, 1));

    // A leaves mid-epoch, releasing 65534. The generation does NOT advance —
    // its rotation is debounced (and manual here).
    disconnect(&handle, &a, DisconnectCause::ClientClosed).await;

    let d = join(&handle, "d").await;
    assert_eq!(
        d.join.kek_generation, 2,
        "65534 is still excluded in generation 1, so the cursor ran out and the epoch reset again"
    );
    assert_eq!(
        sender_id(&d),
        65534,
        "the released id is reissued — but only under a HIGHER generation"
    );
    assert!(
        !handle.force_kek_rotation().await.unwrap(),
        "the second reset rotated the KEK, so it absorbed A's pending departure"
    );

    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "sender_space_exhausted")])
        .assert_delta(2);
    snap.counter("mc_meeting_kek_generated_total")
        .with_labels(&[("trigger", "participant_left")])
        .assert_delta(0);
    handle.cancel();
}

/// Gate-3 F-1 (@paired-protocol): an id a media handler still holds for a
/// participant that has LEFT the roster is not reissued — and becomes
/// reissuable once MH reports that connection closed.
///
/// MH binds `(meeting, sender_id)` to a connection and releases it only when
/// that connection closes, which a roster removal does not cause. Reissue the
/// id while the departed holder's connection is up and MH refuses the new
/// holder: it has a KEK and can never send. A roster-only exclusion set passes
/// every other test here and fails this one.
///
/// A discriminating PAIR, in one meeting. `epoch_reset_cursor` places the
/// departed holder's id (65535) as the ONLY candidate in the fresh epoch
/// (65534 is still rostered), so:
/// - while the binding is live, the reset has nothing allocatable and fails
///   closed (D-3) rather than handing out the held id;
/// - once MH reports the close, the same reset issues exactly that id.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_id_still_bound_at_a_media_handler_is_not_reissued_until_the_handler_releases_it() {
    let (handle, _) = spawn_meeting(
        "m-mh-bound",
        MeetingSeams {
            sender_id_cursor: Some(NonZeroU16::new(65534)),
            epoch_reset_cursor: NonZeroU16::new(65535),
            ..Default::default()
        },
    );
    let a = join(&handle, "a").await;
    let b = join(&handle, "b").await;
    assert_eq!((sender_id(&a), sender_id(&b)), (65534, 65535));

    // MH binds B's connection: MC answers with B's sender id.
    let (lookup, _) = handle
        .media_connected(
            "user-b".to_string(),
            "mh-test-1".to_string(),
            "mh-conn-b".to_string(),
        )
        .await
        .unwrap();
    assert!(
        matches!(lookup, mc_service::actors::SenderLookup::Found(id) if id.get().get() == 65535),
        "PRECONDITION: MC handed MH the binding for 65535; got {lookup:?}"
    );

    // B leaves the roster — but its MH connection stays up.
    disconnect(&handle, &b, DisconnectCause::ClientClosed).await;

    // The namespace is exhausted; the reset must NOT hand out 65535.
    let (tx, _rx) = mpsc::channel::<bytes::Bytes>(64);
    let refused = handle
        .connection_join(
            "conn-c".to_string(),
            "user-c".to_string(),
            "c".to_string(),
            String::new(),
            false,
            test_common::test_identity_key(),
            Some(tx),
            test_common::standalone_join_media(),
        )
        .await;
    assert!(
        refused.is_err(),
        "65535 is still bound at MH for departed B; reissuing it would give C a KEK and no way \
         to send. Got {:?}",
        refused.map(|j| j.sender_id)
    );

    // MH reports B's connection closed. It clears its own binding BEFORE it
    // sends this, so the release is correctly ordered.
    let unapplied = handle
        .media_disconnected(
            "user-b".to_string(),
            "mh-test-1".to_string(),
            "mh-conn-b".to_string(),
        )
        .await
        .unwrap();
    assert!(
        unapplied.is_none(),
        "a departed holder's close released a binding — applied, not unknown; got {unapplied:?}"
    );

    let d = join(&handle, "d").await;
    assert_eq!(
        sender_id(&d),
        65535,
        "freed at MH, so the reset may now reissue it"
    );
    assert_eq!(
        d.join.kek_generation, 1,
        "and only under a new KEK generation"
    );
    handle.cancel();
}
