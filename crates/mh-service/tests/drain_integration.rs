//! Story 4 MH-3: the planned drain on SIGTERM (ADR-0039 §7, reversing
//! ADR-0036 §11; `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md`
//! "MH" drain bullets; R-3 MH half, R-22, R-40).
//!
//! The production `drain::run` over the production session actor and sender
//! task (`common::heartbeat_rig`, fake MC), paused time, no sleeps.
//!
//! - D1: every held meeting gets a DRAINING beat at the drain instant, and
//!   every later beat stays DRAINING with a decreasing `drain_remaining_ms`
//!   read from the one deadline.
//! - D2: no exit while holding a meeting before the deadline, whatever MC does
//!   (draining never self-fences on MC silence).
//! - D3: at the deadline the remainder is shed with the shutdown token STILL
//!   live (so sessions close `HANDLER_RELEASED`, not code-less), and the token
//!   is cancelled exactly one `CLOSE_SESSION_LINGER` later; counted ONLY on
//!   `mh_drain_meetings_shed_total`.
//! - D4/D5: early exit the instant the last meeting is released, and at once
//!   with nothing held.
//! - A meeting registered DURING the drain is accepted, beats DRAINING at once,
//!   and counts as held.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use common::heartbeat_rig::*;
use common::paused_time::{run_for, settle, step_until, STEP};
use mh_service::config::CLOSE_SESSION_LINGER;
use mh_service::drain::{self, DrainExit, DrainOutcome};
use mh_test_utils::media_policy::{TEST_HEARTBEAT_INTERVAL, TEST_MC_ID};
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerService as _;
use proto_gen::dark_tower::internal::v1::{ReleaseCause, ServingState};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// The drain window at the deployed grace (35 s): grace − margin.
const WINDOW: Duration = mh_service::config::drain_window_for(35);

async fn advance(by: Duration) {
    tokio::time::advance(by).await;
    settle().await;
}

/// Start the production drain on the rig, as `main` does.
fn start_drain(rig: &Rig) -> (JoinHandle<DrainOutcome>, CancellationToken) {
    let shutdown = CancellationToken::new();
    let sm = rig.sm.clone();
    let drain_tx = Arc::clone(&rig.drain);
    let token = shutdown.clone();
    let task = tokio::spawn(async move { drain::run(&sm, &drain_tx, WINDOW, &token).await });
    (task, shutdown)
}

/// Register `meetings` and step until each has sent its first (jittered) beat,
/// so the drain lands mid-schedule.
async fn held_mid_schedule(rig: &Rig, meetings: &[&str]) {
    for m in meetings {
        register(rig, plain(m)).await.unwrap();
    }
    step_until(
        || {
            let all_out = meetings.iter().all(|m| rig.mc.count(m) > 0);
            async move { all_out }
        },
        TEST_HEARTBEAT_INTERVAL * 2,
        "every meeting's first beat",
    )
    .await;
}

/// D1.
#[tokio::test(start_paused = true)]
async fn every_held_meeting_beats_draining_at_once_with_decreasing_remaining() {
    let rig = rig();
    held_mid_schedule(&rig, &["d1-a", "d1-b"]).await;
    for m in ["d1-a", "d1-b"] {
        let before = rig.mc.last(m);
        assert_eq!(before.serving_state, ServingState::Serving as i32);
        assert_eq!(
            before.drain_remaining_ms, 0,
            "negative arm: SERVING carries none"
        );
    }
    let counts: Vec<usize> = ["d1-a", "d1-b"].iter().map(|m| rig.mc.count(m)).collect();
    let pre_drain = counts.clone();

    let drain_at = Instant::now();
    let (_task, _shutdown) = start_drain(&rig);
    settle().await;
    for (m, before) in ["d1-a", "d1-b"].iter().zip(counts) {
        let calls = rig.mc.calls_for(m);
        assert_eq!(
            calls.len(),
            before + 1,
            "{m}: an immediate beat, no waiting"
        );
        let (at, beat) = calls.last().unwrap();
        assert_eq!(*at, drain_at, "{m}: at the drain instant");
        assert_eq!(beat.serving_state, ServingState::Draining as i32);
        assert_eq!(
            u128::from(beat.drain_remaining_ms),
            WINDOW.as_millis(),
            "{m}: remaining == deadline − elapsed, exactly"
        );
    }

    run_for(TEST_HEARTBEAT_INTERVAL * 3).await;
    for (m, before) in ["d1-a", "d1-b"].iter().zip(pre_drain) {
        // Every beat sent after the drain started (by index: the last SERVING
        // beat may share the drain instant).
        let draining: Vec<_> = rig.mc.calls_for(m).into_iter().skip(before).collect();
        assert!(draining.len() >= 3, "{m}: the cadence continues");
        for (at, beat) in &draining {
            assert_eq!(beat.serving_state, ServingState::Draining as i32, "{m}");
            let elapsed = at.saturating_duration_since(drain_at);
            assert_eq!(
                u128::from(beat.drain_remaining_ms),
                (WINDOW - elapsed).as_millis(),
                "{m}: every beat carries the time left"
            );
        }
        for pair in draining.windows(2) {
            assert!(pair[1].1.drain_remaining_ms < pair[0].1.drain_remaining_ms);
        }
    }
}

/// D2: MC hangs or errors throughout; the drain still waits for the deadline
/// and no earlier — no self-fence on MC silence.
#[tokio::test(start_paused = true)]
async fn no_exit_while_holding_a_meeting_before_the_deadline() {
    let rig = rig();
    rig.mc.reply("d2-hang", Reply::Hang);
    rig.mc
        .reply("d2-error", Reply::Status(tonic::Code::Unavailable));
    held_mid_schedule(&rig, &["d2-hang", "d2-error"]).await;
    let (task, shutdown) = start_drain(&rig);
    advance(WINDOW - STEP).await;
    assert!(!task.is_finished() && !shutdown.is_cancelled());
    assert!(rig.sm.is_meeting_registered("d2-hang").await);
    assert!(rig.sm.is_meeting_registered("d2-error").await);
}

/// D3: the deadline shed, its ORDER, and its single counting home.
#[tokio::test(start_paused = true)]
async fn the_deadline_sheds_with_the_shutdown_token_still_live_then_cancels() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    held_mid_schedule(&rig, &["d3-a", "d3-b"]).await;
    let a = connection("c-a", "p-a");
    let b = connection("c-b", "p-b");
    let (close_a, close_b) = (a.close.clone(), b.close.clone());
    admit(&rig.sm, "d3-a", a).await;
    admit(&rig.sm, "d3-b", b).await;

    let started = Instant::now();
    let (task, shutdown) = start_drain(&rig);
    settle().await;
    advance(WINDOW - STEP).await;
    assert!(!close_a.is_cancelled() && !close_b.is_cancelled());
    advance(STEP).await;
    // At the shed instant: sessions closed (HANDLER_RELEASED path), shutdown
    // NOT yet cancelled — the order that keeps the close coded.
    assert!(close_a.is_cancelled() && close_b.is_cancelled());
    assert!(!shutdown.is_cancelled(), "shed BEFORE the shutdown token");
    assert!(!rig.sm.is_meeting_registered("d3-a").await);

    advance(CLOSE_SESSION_LINGER - STEP).await;
    assert!(!shutdown.is_cancelled());
    advance(STEP).await;
    assert!(
        shutdown.is_cancelled(),
        "cancelled at exactly shed + linger"
    );
    assert_eq!(Instant::now() - started, WINDOW + CLOSE_SESSION_LINGER);

    let outcome = task.await.unwrap();
    assert_eq!(outcome.exit, DrainExit::Deadline);
    assert_eq!(outcome.shed, 2);
    assert_eq!(outcome.elapsed, WINDOW);
    // First: a snapshot read drains histogram observations.
    snap.histogram("mh_drain_duration_seconds")
        .assert_observation_count(1);
    snap.counter("mh_drain_meetings_shed_total").assert_delta(2);
    snap.gauge("mh_drain_meetings_remaining").assert_value(0.0);
    // One home per event: never a release or a teardown.
    for clients in ["present", "none"] {
        for trigger in [
            "heartbeat_timeout",
            "apply_lag",
            "planned_transfer",
            "owner_silence",
        ] {
            snap.counter(RELEASES)
                .with_labels(&[("trigger", trigger), ("clients", clients), KC])
                .assert_delta(0);
        }
    }
    snap.counter(TEARDOWNS)
        .with_labels(&[("outcome", "released"), KC])
        .assert_delta(0);
}

/// D4: released by a RELEASED verdict, then by EndMeeting: the drain exits the
/// instant the last one goes (cancel one linger later), nothing shed.
#[tokio::test(start_paused = true)]
async fn the_drain_exits_when_the_last_meeting_is_released() {
    let snap = MetricAssertion::snapshot();
    let rig = rig();
    held_mid_schedule(&rig, &["d4-verdict", "d4-end"]).await;
    let (task, shutdown) = start_drain(&rig);
    settle().await;

    rig.mc.reply(
        "d4-verdict",
        Reply::Released(ReleaseCause::PlannedTransfer as i32),
    );
    step_until(
        || async { !rig.sm.is_meeting_registered("d4-verdict").await },
        TEST_HEARTBEAT_INTERVAL * 2,
        "d4-verdict released by its RELEASED verdict",
    )
    .await;
    snap.gauge("mh_drain_meetings_remaining").assert_value(1.0);
    assert!(
        !task.is_finished() && !shutdown.is_cancelled(),
        "one still held"
    );

    advance(Duration::from_secs(3)).await;
    rig.svc
        .end_meeting(end(
            "d4-end",
            TEST_MC_ID,
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await
        .unwrap();
    let released_at = Instant::now();
    settle().await;
    assert!(!shutdown.is_cancelled());
    advance(CLOSE_SESSION_LINGER).await;
    assert!(shutdown.is_cancelled());
    let outcome = task.await.unwrap();
    assert_eq!(outcome.exit, DrainExit::Empty);
    assert_eq!(outcome.shed, 0);
    assert!(outcome.elapsed < WINDOW, "well before the deadline");
    assert_eq!(Instant::now() - released_at, CLOSE_SESSION_LINGER);
    snap.counter("mh_drain_meetings_shed_total").assert_delta(0);
}

/// D5: holding nothing, the drain exits at once (one linger, no other wait).
#[tokio::test(start_paused = true)]
async fn an_empty_handler_exits_at_once() {
    let rig = rig();
    let started = Instant::now();
    let (task, shutdown) = start_drain(&rig);
    settle().await;
    assert!(!shutdown.is_cancelled());
    advance(CLOSE_SESSION_LINGER).await;
    assert!(shutdown.is_cancelled());
    let outcome = task.await.unwrap();
    assert_eq!(outcome.exit, DrainExit::Empty);
    assert_eq!(outcome.shed, 0);
    assert_eq!(Instant::now() - started, CLOSE_SESSION_LINGER);
}

/// A RegisterMeeting during the drain is ACCEPTED (MH keeps serving), its MC
/// hears DRAINING at once (unjittered), and it counts as held: the drain does
/// not exit until it, too, is released.
#[tokio::test(start_paused = true)]
async fn a_meeting_registered_during_the_drain_is_held_and_beats_draining_at_once() {
    let rig = rig();
    held_mid_schedule(&rig, &["dr-old"]).await;
    let (task, _shutdown) = start_drain(&rig);
    advance(Duration::from_secs(1)).await;

    let registered_at = Instant::now();
    register(&rig, plain("dr-new")).await.unwrap();
    settle().await;
    let calls = rig.mc.calls_for("dr-new");
    assert_eq!(calls.len(), 1, "an immediate beat");
    assert_eq!(calls[0].0, registered_at, "unjittered");
    assert_eq!(calls[0].1.serving_state, ServingState::Draining as i32);

    rig.svc
        .end_meeting(end(
            "dr-old",
            TEST_MC_ID,
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await
        .unwrap();
    advance(Duration::from_secs(5)).await;
    assert!(!task.is_finished(), "the new meeting is held");
    rig.svc
        .end_meeting(end(
            "dr-new",
            TEST_MC_ID,
            ReleaseCause::PlannedTransfer as i32,
        ))
        .await
        .unwrap();
    advance(CLOSE_SESSION_LINGER).await;
    assert_eq!(task.await.unwrap().exit, DrainExit::Empty);
}
