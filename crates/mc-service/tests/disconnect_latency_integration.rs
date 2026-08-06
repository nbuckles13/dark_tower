// Every `#[tokio::test]` here is pinned to `flavor = "current_thread"` and that
// pinning is LOAD-BEARING — do not "simplify" it away. `MetricAssertion` binds a
// per-thread recorder; the `mc_participant_leaves_total` /
// `mc_participant_disconnects_total` emissions happen inside the spawned
// `MeetingActor` task. On `current_thread` that task shares the test thread, so
// the recorder sees the emissions (identical model to the accept-loop rigs). On a
// multi-thread runtime they would land on a worker and be silently missed.
//
// SYNCHRONIZATION: the counters are asserted only AFTER a `get_state().await`
// round-trip that follows the `connection_disconnected(...)` send. The meeting
// mailbox is FIFO, so once `get_state` returns, the earlier disconnect message
// has been fully processed and its metric emitted — no sleep-racing on the
// assertion.
//
// The FIFO round-trip covers only *message*-driven work. The abrupt-loss grace
// EXPIRY is driven instead by the actor's `grace_check` interval-tick `select!`
// branch (meeting.rs) — a non-mailbox event the FIFO ordering does not sequence
// against `get_state`. There, after advancing past the grace period, we yield
// once with a virtual-time `tokio::time::sleep` (auto-advanced under
// `start_paused`, so still no wall-clock wait) to let the actor drain the pending
// grace tick BEFORE the `get_state` round-trip, mirroring the lib test
// `test_disconnect_grace_period_expires`. This keeps the grace-expiry assertion
// deterministic rather than racing the pseudo-random `select!`.
//
//! Production-path component tests for the roster-leave-latency work (task #64):
//! drive a real `MeetingActor` through `connection_disconnected(..., cause)` and
//! assert BOTH the new departure counters AND the end-to-end roster behavior —
//! that a clean close removes the participant IMMEDIATELY and broadcasts an actual
//! `ParticipantLeft{Voluntary}` WIRE frame, while an abrupt loss keeps the grace
//! period and only broadcasts `ParticipantLeft{Timeout}` after grace expires.
//!
//! Behavioral crux (why the wire-frame assertion matters alongside the counter):
//! a `Disconnected` update is NOT serialized to the wire, only `Left` is. The
//! clean-close path must therefore produce a real `ParticipantLeft`, not merely an
//! internal-map removal — otherwise the peer's roster never updates. This is the
//! exact bug the task targets.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use ::common::secret::SecretBox;
use mc_service::actors::{
    ActorMetrics, ControllerMetrics, DisconnectCause, MeetingActor, MeetingActorHandle,
    ParticipantStatus,
};
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{self, server_message, ServerMessage};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn spawn_meeting(meeting_id: &str) -> (MeetingActorHandle, tokio::task::JoinHandle<()>) {
    MeetingActor::spawn(
        meeting_id.to_string(),
        CancellationToken::new(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        SecretBox::new(Box::new(vec![0u8; 32])),
    )
}

/// Drain a wired participant's outbound stream and return the FIRST decoded
/// `ParticipantLeft`, if any.
fn drain_for_left(rx: &mut mpsc::Receiver<bytes::Bytes>) -> Option<v1::ParticipantLeft> {
    while let Ok(bytes) = rx.try_recv() {
        let msg = ServerMessage::decode(bytes.as_ref()).expect("decodable ServerMessage");
        if let Some(server_message::Message::ParticipantLeft(left)) = msg.message {
            return Some(left);
        }
    }
    None
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn clean_close_removes_immediately_emits_voluntary_and_broadcasts_left_frame() {
    let snap = MetricAssertion::snapshot();
    let (handle, _task) = spawn_meeting("m-clean-close");

    // Participant B has a wired outbound stream so we can observe the wire frame.
    let (b_tx, mut b_rx) = mpsc::channel::<bytes::Bytes>(64);
    handle
        .connection_join(
            "conn-b".to_string(),
            "user-b".to_string(),
            "part-b".to_string(),
            String::new(),
            false,
            Some(b_tx),
        )
        .await
        .unwrap();
    // Participant A (the one that will close its tab).
    handle
        .connection_join(
            "conn-a".to_string(),
            "user-a".to_string(),
            "part-a".to_string(),
            String::new(),
            false,
            None,
        )
        .await
        .unwrap();
    // Drop the ParticipantJoined(A) frame B received on join.
    let _ = drain_for_left(&mut b_rx);

    // A cleanly closes its WebTransport session (tab close).
    handle
        .connection_disconnected(
            "conn-a".to_string(),
            "part-a".to_string(),
            DisconnectCause::ClientClosed,
        )
        .await
        .unwrap();

    // FIFO round-trip: once this returns, the disconnect above has been processed
    // and its metrics emitted. A is gone immediately (grace skipped) — only B left.
    let state = handle.get_state().await.unwrap();
    assert_eq!(state.participants.len(), 1);
    assert_eq!(state.participants[0].participant_id, "part-b");

    // Production-path counters: disconnect cause=client_closed, leave reason=voluntary.
    snap.counter("mc_participant_disconnects_total")
        .with_labels(&[("cause", "client_closed")])
        .assert_delta(1);
    snap.counter("mc_participant_disconnects_total")
        .with_labels(&[("cause", "connection_lost")])
        .assert_delta(0);
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "voluntary")])
        .assert_delta(1);
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "timeout")])
        .assert_delta(0);

    // WIRE frame: B actually received ParticipantLeft{Voluntary} for A.
    let left = drain_for_left(&mut b_rx).expect("B must receive a ParticipantLeft frame");
    assert_eq!(left.participant_id, "part-a");
    assert_eq!(left.reason, v1::LeaveReason::Voluntary as i32);

    handle.cancel();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn abrupt_loss_keeps_grace_emits_connection_lost_then_timeout_left_after_grace() {
    let snap = MetricAssertion::snapshot();
    let (handle, _task) = spawn_meeting("m-abrupt-loss");

    let (b_tx, mut b_rx) = mpsc::channel::<bytes::Bytes>(64);
    handle
        .connection_join(
            "conn-b".to_string(),
            "user-b".to_string(),
            "part-b".to_string(),
            String::new(),
            false,
            Some(b_tx),
        )
        .await
        .unwrap();
    handle
        .connection_join(
            "conn-a".to_string(),
            "user-a".to_string(),
            "part-a".to_string(),
            String::new(),
            false,
            None,
        )
        .await
        .unwrap();
    let _ = drain_for_left(&mut b_rx);

    // Abrupt loss (idle-timeout / network drop) — grace preserved.
    handle
        .connection_disconnected(
            "conn-a".to_string(),
            "part-a".to_string(),
            DisconnectCause::ConnectionLost,
        )
        .await
        .unwrap();

    // FIFO round-trip: disconnect processed. A still present, Disconnected (grace
    // running) — roster NOT yet updated, no Left frame yet.
    let state = handle.get_state().await.unwrap();
    assert_eq!(state.participants.len(), 2);
    let a = state
        .participants
        .iter()
        .find(|p| p.participant_id == "part-a")
        .unwrap();
    assert_eq!(a.status, ParticipantStatus::Disconnected);
    assert!(drain_for_left(&mut b_rx).is_none());

    // Disconnect counter fired with cause=connection_lost; no leave YET.
    snap.counter("mc_participant_disconnects_total")
        .with_labels(&[("cause", "connection_lost")])
        .assert_delta(1);
    snap.counter("mc_participant_disconnects_total")
        .with_labels(&[("cause", "client_closed")])
        .assert_delta(0);
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "timeout")])
        .assert_delta(0);

    // Advance past the 30s grace + a grace-check tick. Grace expiry is driven by
    // the actor's interval-tick `select!` branch, NOT a mailbox message, so the
    // FIFO round-trip does NOT order it before `get_state` — after `advance`, the
    // ready tick and a subsequently-sent `get_state` would race in the actor's
    // pseudo-random `select!`. Yield first (virtual-time sleep under `start_paused`,
    // no wall-clock wait) so the actor drains the pending grace tick before any
    // `get_state` message competes with it; THEN round-trip to observe A removed.
    tokio::time::advance(Duration::from_secs(36)).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let state = handle.get_state().await.unwrap();
    assert_eq!(state.participants.len(), 1);

    // Now the terminal leave fires with reason=timeout (siblings flat).
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "timeout")])
        .assert_delta(1);
    snap.counter("mc_participant_leaves_total")
        .with_labels(&[("reason", "voluntary")])
        .assert_delta(0);

    // B received a ParticipantLeft{Timeout}.
    let left = drain_for_left(&mut b_rx).expect("B must receive a ParticipantLeft frame");
    assert_eq!(left.participant_id, "part-a");
    assert_eq!(left.reason, v1::LeaveReason::Timeout as i32);

    handle.cancel();
}
