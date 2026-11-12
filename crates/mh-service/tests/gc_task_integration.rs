//! Story 4 MH-3: the MH → GC task's load-report hygiene and drain reporting
//! (ADR-0039 §7, §10; `docs/debates/2026-10-28-mh-replacement-and-migration/test-gates.md`
//! "MH": "Structural load report immediately after every successful register
//! (t=0, paused time); DRAINING report on SIGTERM").
//!
//! The production `grpc::gc_task::run` over a scripted fake `GcRegistry`
//! (paused time over a real tonic socket auto-advances the clock mid-read).
//! The wire shape (`health = 4` on the RPC) is pinned unpaused in
//! `gc_integration.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mh_service::drain::{self, DrainDeadline};
use mh_service::errors::MhError;
use mh_service::grpc::gc_task::{self, GcRegistry, REGISTRATION_RETRY_DELAY};
use mh_service::observability::HealthState;
use proto_gen::dark_tower::internal::v1::HealthStatus;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use common::paused_time::{run_for, settle, STEP};

/// The interval the fake GC pushes: NOT the client's 10 s default, so a test
/// that passes proves the pushed value is used.
const PUSHED: Duration = Duration::from_secs(7);
/// The interval GC pushes on a RE-registration (L2): different again, so the
/// ticker provably re-reads it.
const REPUSHED: Duration = Duration::from_secs(4);

/// One scripted answer.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Ok,
    /// GC rejected the registration (e.g. `address_conflict`).
    Reject,
    /// `NOT_FOUND` on a load report.
    NotFound,
    /// Any other failure.
    Fail,
}

fn result(answer: Answer) -> Result<(), MhError> {
    match answer {
        Answer::Ok => Ok(()),
        Answer::Reject => Err(MhError::Grpc(
            "GC rejected registration: address_conflict".into(),
        )),
        Answer::NotFound => Err(MhError::NotRegistered),
        Answer::Fail => Err(MhError::Grpc("unavailable".into())),
    }
}

#[derive(Default)]
struct FakeGc {
    register_script: Mutex<VecDeque<Answer>>,
    reregister_script: Mutex<VecDeque<Answer>>,
    report_script: Mutex<VecDeque<Answer>>,
    registers: Mutex<Vec<Instant>>,
    reregisters: Mutex<Vec<Instant>>,
    reports: Mutex<Vec<(Instant, HealthStatus)>>,
    /// The interval GC last pushed (`None` = [`PUSHED`]); a successful
    /// re-registration pushes [`REPUSHED`].
    interval: Mutex<Option<Duration>>,
}

impl FakeGc {
    fn next(script: &Mutex<VecDeque<Answer>>) -> Answer {
        script.lock().unwrap().pop_front().unwrap_or(Answer::Ok)
    }

    fn reports(&self) -> Vec<(Instant, HealthStatus)> {
        self.reports.lock().unwrap().clone()
    }
}

impl GcRegistry for FakeGc {
    async fn register(&self) -> Result<(), MhError> {
        self.registers.lock().unwrap().push(Instant::now());
        result(Self::next(&self.register_script))
    }

    async fn reregister(&self) -> Result<(), MhError> {
        self.reregisters.lock().unwrap().push(Instant::now());
        let outcome = result(Self::next(&self.reregister_script));
        if outcome.is_ok() {
            *self.interval.lock().unwrap() = Some(REPUSHED);
        }
        outcome
    }

    async fn send_load_report(&self, health: HealthStatus) -> Result<(), MhError> {
        self.reports.lock().unwrap().push((Instant::now(), health));
        result(Self::next(&self.report_script))
    }

    fn load_report_interval(&self) -> Duration {
        self.interval.lock().unwrap().unwrap_or(PUSHED)
    }
}

struct Task {
    gc: Arc<FakeGc>,
    health: Arc<HealthState>,
    drain: watch::Sender<Option<DrainDeadline>>,
    cancel: CancellationToken,
    t0: Instant,
}

impl Drop for Task {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn start(gc: FakeGc) -> Task {
    let gc = Arc::new(gc);
    let health = Arc::new(HealthState::new());
    let (drain_tx, drain_rx) = drain::channel();
    let cancel = CancellationToken::new();
    let (g, h, c) = (Arc::clone(&gc), Arc::clone(&health), cancel.clone());
    tokio::spawn(async move { gc_task::run(g.as_ref(), &h, drain_rx, c).await });
    Task {
        gc,
        health,
        drain: drain_tx,
        cancel,
        t0: Instant::now(),
    }
}

/// Start the drain as `main` does: latch readiness, publish a deadline.
fn begin_drain(task: &Task) {
    task.health.begin_drain();
    task.drain.send_replace(Some(DrainDeadline::new(
        Instant::now() + Duration::from_secs(30),
    )));
}

/// L1 + L3: one report at the registration instant (no time advanced), then
/// exactly one per PUSHED interval — never earlier.
#[tokio::test(start_paused = true)]
async fn a_report_goes_out_at_once_after_registering_then_on_the_pushed_interval() {
    let task = start(FakeGc::default());
    settle().await;
    let reports = task.gc.reports();
    assert_eq!(reports, vec![(task.t0, HealthStatus::Healthy)], "t = 0");
    assert!(task.health.is_ready());

    run_for(PUSHED - STEP).await;
    assert_eq!(
        task.gc.reports().len(),
        1,
        "negative arm: nothing before +interval"
    );
    run_for(STEP).await;
    let reports = task.gc.reports();
    assert_eq!(reports.len(), 2);
    assert_eq!(
        reports[1].0 - task.t0,
        PUSHED,
        "exactly the pushed interval"
    );
}

/// L2: NOT_FOUND → re-register → one report at the re-registration instant,
/// not at the next tick. A FAILED re-registration sends no report.
#[tokio::test(start_paused = true)]
async fn a_report_goes_out_at_once_after_every_reregistration() {
    let gc = FakeGc::default();
    // t0 report OK; tick 1 NOT_FOUND (re-register OK); tick 2 NOT_FOUND
    // (re-register FAILS).
    gc.report_script.lock().unwrap().extend([
        Answer::Ok,
        Answer::NotFound,
        Answer::Ok,
        Answer::NotFound,
    ]);
    gc.reregister_script
        .lock()
        .unwrap()
        .extend([Answer::Ok, Answer::Fail]);
    let task = start(gc);
    settle().await;

    run_for(PUSHED).await;
    let tick1 = task.t0 + PUSHED;
    let reports = task.gc.reports();
    assert_eq!(reports.len(), 3, "t0, the NOT_FOUND, the immediate one");
    assert_eq!(reports[2].0, tick1, "at the re-registration instant");
    assert_eq!(*task.gc.reregisters.lock().unwrap(), vec![tick1]);

    // The ticker restarts from the re-registration, on the interval GC pushed
    // THERE (4 s), not the original 7 s.
    run_for(REPUSHED - STEP).await;
    assert_eq!(
        task.gc.reports().len(),
        3,
        "nothing before the re-pushed interval"
    );
    run_for(STEP).await;
    let reports = task.gc.reports();
    assert_eq!(reports.len(), 4, "a failed re-registration sends no report");
    assert_eq!(
        reports[3].0,
        tick1 + REPUSHED,
        "the re-pushed interval applies"
    );
    assert_eq!(task.gc.reregisters.lock().unwrap().len(), 2);
}

/// L4: a rejected registration (`address_conflict`) is retried on the retry
/// cadence while MH stays NOT ready; ready and the first report follow only
/// acceptance.
#[tokio::test(start_paused = true)]
async fn a_rejected_registration_is_retried_while_not_ready() {
    let gc = FakeGc::default();
    gc.register_script
        .lock()
        .unwrap()
        .extend([Answer::Reject, Answer::Reject, Answer::Reject]);
    let task = start(gc);
    settle().await;
    run_for(REGISTRATION_RETRY_DELAY * 3 - STEP).await;
    assert!(!task.health.is_ready(), "not ready while rejected");
    assert!(task.gc.reports().is_empty(), "no report before acceptance");
    assert_eq!(task.gc.registers.lock().unwrap().len(), 3);

    run_for(STEP).await;
    let accepted_at = task.t0 + REGISTRATION_RETRY_DELAY * 3;
    let registers = task.gc.registers.lock().unwrap().clone();
    assert_eq!(
        registers,
        (0..4)
            .map(|k| task.t0 + REGISTRATION_RETRY_DELAY * k)
            .collect::<Vec<_>>(),
        "retried on the retry cadence"
    );
    assert!(task.health.is_ready(), "positive arm: ready on acceptance");
    assert_eq!(
        task.gc.reports(),
        vec![(accepted_at, HealthStatus::Healthy)]
    );
}

/// L5: the drain sends DRAINING at once with no tick, every later report stays
/// DRAINING, and a re-registration DURING the drain still reports DRAINING.
/// Negative arm: every report before the drain is HEALTHY.
#[tokio::test(start_paused = true)]
async fn draining_is_reported_at_once_and_survives_reregistration() {
    let gc = FakeGc::default();
    gc.report_script.lock().unwrap().extend([
        Answer::Ok,       // t0
        Answer::Ok,       // tick 1 (serving)
        Answer::Ok,       // the drain's out-of-cycle report
        Answer::NotFound, // tick after the drain -> re-register
    ]);
    let task = start(gc);
    run_for(PUSHED + Duration::from_secs(1)).await;
    assert!(task
        .gc
        .reports()
        .iter()
        .all(|(_, h)| *h == HealthStatus::Healthy));

    let drain_at = Instant::now();
    begin_drain(&task);
    settle().await;
    let reports = task.gc.reports();
    assert_eq!(reports.len(), 3);
    assert_eq!(
        reports[2],
        (drain_at, HealthStatus::Draining),
        "no tick needed"
    );
    assert!(!task.health.is_ready());

    run_for(PUSHED * 2).await;
    let after: Vec<_> = task.gc.reports().into_iter().skip(2).collect();
    assert!(
        after.len() >= 4,
        "drain, NOT_FOUND, post-re-register, a tick"
    );
    assert!(
        after.iter().all(|(_, h)| *h == HealthStatus::Draining),
        "draining is process state: {after:?}"
    );
    assert_eq!(task.gc.reregisters.lock().unwrap().len(), 1);
    assert!(
        !task.health.is_ready(),
        "a re-registration never restores readiness"
    );
}

/// A registration that completes only AFTER the drain began: never ready, and
/// its first report is DRAINING.
#[tokio::test(start_paused = true)]
async fn a_registration_completing_mid_drain_never_makes_mh_ready() {
    let gc = FakeGc::default();
    gc.register_script.lock().unwrap().push_back(Answer::Reject);
    let task = start(gc);
    settle().await;
    begin_drain(&task);
    run_for(REGISTRATION_RETRY_DELAY).await;
    assert_eq!(task.gc.registers.lock().unwrap().len(), 2, "accepted now");
    assert!(!task.health.is_ready());
    assert_eq!(
        task.gc.reports(),
        vec![(task.t0 + REGISTRATION_RETRY_DELAY, HealthStatus::Draining)]
    );
}
