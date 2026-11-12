//! The MH → GC task: registration, then load reports (ADR-0010; story 4 MH-3,
//! ADR-0039 §7, §10).
//!
//! # Rules
//!
//! - **Registration is retried forever while MH is not ready**, including a
//!   rejection (GC's `FAILED_PRECONDITION` `address_conflict` against a fresh
//!   serving row: MH retries until that row goes stale).
//! - **One load report IMMEDIATELY after every successful registration** —
//!   the initial one and every re-registration after a `NOT_FOUND` — then the
//!   ticker, so a (re)started MH flips GC from pending to healthy within one
//!   RPC instead of relying on a timer's first tick. The ticker uses the
//!   interval GC pushed in `RegisterMHResponse`.
//! - **Health comes from ONE place**, [`health_of`] over the process drain
//!   state: HEALTHY, or DRAINING from signal receipt until exit. GC overwrites
//!   `health_status` per report, so draining is process state, never a
//!   one-shot: it survives a re-registration. The drain transition itself
//!   sends an immediate out-of-cycle DRAINING report.
//! - GC unreachability never delays the drain's exit: this task is cancelled
//!   by the drain coordinator's shutdown, and each RPC is bounded by the
//!   client's timeout.

use std::future::Future;
use std::time::Duration;

use proto_gen::dark_tower::internal::v1::HealthStatus;
use tokio::time::{Instant, Interval, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::drain::{is_draining, DrainWatch};
use crate::errors::MhError;
use crate::grpc::GcClient;
use crate::observability::HealthState;

/// Delay between attempts of a registration GC REJECTED (an RPC failure is
/// already retried with backoff inside `GcClient::register`).
pub const REGISTRATION_RETRY_DELAY: Duration = Duration::from_secs(5);

/// The GC registry seam: production is [`GcClient`]; Tier 1b tests inject a
/// scripted fake (paused time over real sockets would let the clock
/// auto-advance mid-read and fire deadlines spuriously — the MH-2 heartbeat
/// rig's reason, reused).
pub trait GcRegistry: Send + Sync {
    /// Register (initial). `Err` = GC rejected it (or gave up).
    fn register(&self) -> impl Future<Output = Result<(), MhError>> + Send;
    /// One re-registration attempt after a `NOT_FOUND` load report.
    fn reregister(&self) -> impl Future<Output = Result<(), MhError>> + Send;
    /// Send one load report carrying `health`. `Err(MhError::NotRegistered)`
    /// means GC has no row for this handler.
    fn send_load_report(
        &self,
        health: HealthStatus,
    ) -> impl Future<Output = Result<(), MhError>> + Send;
    /// The load-report interval GC pushed at the latest registration.
    fn load_report_interval(&self) -> Duration;
}

impl GcRegistry for GcClient {
    fn register(&self) -> impl Future<Output = Result<(), MhError>> + Send {
        GcClient::register(self)
    }

    fn reregister(&self) -> impl Future<Output = Result<(), MhError>> + Send {
        self.attempt_reregistration()
    }

    fn send_load_report(
        &self,
        health: HealthStatus,
    ) -> impl Future<Output = Result<(), MhError>> + Send {
        GcClient::send_load_report(self, health)
    }

    fn load_report_interval(&self) -> Duration {
        GcClient::load_report_interval(self)
    }
}

/// THE health every load report carries: DRAINING once the process is
/// draining, HEALTHY otherwise. No send site picks its own value.
#[must_use]
pub fn health_of(drain: &DrainWatch) -> HealthStatus {
    if is_draining(drain) {
        HealthStatus::Draining
    } else {
        HealthStatus::Healthy
    }
}

/// Run the GC task until `cancel`: register (retrying while not ready), mark
/// ready, report immediately, then report on the ticker, on every drain
/// transition, and immediately after every re-registration.
pub async fn run<G: GcRegistry>(
    gc: &G,
    health: &HealthState,
    mut drain: DrainWatch,
    cancel: CancellationToken,
) {
    info!(target: "mh.grpc.gc_task", "GC task: starting registration");
    if !register_until_accepted(gc, &cancel).await {
        info!(target: "mh.grpc.gc_task", "GC task: cancelled before registration completed");
        return;
    }
    // A no-op once draining (`HealthState::begin_drain` latches).
    health.set_ready();
    // The post-registration report carries the CURRENT drain state, so a drain
    // that began while registering is already reported: mark it seen, or the
    // loop would send a duplicate at once.
    drain.borrow_and_update();
    report_now(gc, &drain).await;
    let mut ticker = ticker_from(gc.load_report_interval());
    info!(
        target: "mh.grpc.gc_task",
        interval_ms = u64::try_from(gc.load_report_interval().as_millis()).unwrap_or(u64::MAX),
        "GC task: registered; entering the load-report loop"
    );

    let mut drain_open = true;
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            changed = drain.changed(), if drain_open => {
                if changed.is_ok() {
                    // Out of cycle: GC stops selecting this MH now, not at the
                    // next tick.
                    if report_now(gc, &drain).await {
                        ticker = ticker_from(gc.load_report_interval());
                    } else {
                        ticker.reset();
                    }
                } else {
                    drain_open = false;
                }
            }
            _ = ticker.tick() => {
                if report_now(gc, &drain).await {
                    // Re-registered: GC may have pushed a new interval.
                    ticker = ticker_from(gc.load_report_interval());
                }
            }
        }
    }
    info!(target: "mh.grpc.gc_task", "GC task: stopped");
}

/// Register until GC accepts, retrying a rejection every
/// [`REGISTRATION_RETRY_DELAY`]. `false` when cancelled first.
async fn register_until_accepted<G: GcRegistry>(gc: &G, cancel: &CancellationToken) -> bool {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return false,
            result = gc.register() => match result {
                Ok(()) => return true,
                Err(error) => warn!(
                    target: "mh.grpc.gc_task",
                    error = %error,
                    retry_delay_ms = u64::try_from(REGISTRATION_RETRY_DELAY.as_millis()).unwrap_or(u64::MAX),
                    "GC task: registration failed or was rejected; retrying while not ready"
                ),
            }
        }
        tokio::select! {
            () = cancel.cancelled() => return false,
            () = tokio::time::sleep(REGISTRATION_RETRY_DELAY) => {}
        }
    }
}

/// Send one report now. On `NOT_FOUND`, re-register once and — on success —
/// report IMMEDIATELY (the post-registration report). Returns whether a
/// re-registration succeeded (the caller then rebuilds its ticker).
async fn report_now<G: GcRegistry>(gc: &G, drain: &DrainWatch) -> bool {
    match gc.send_load_report(health_of(drain)).await {
        Ok(()) => false,
        Err(MhError::NotRegistered) => {
            warn!(
                target: "mh.grpc.gc_task",
                "Load report: GC has no row for this handler; re-registering"
            );
            match gc.reregister().await {
                Ok(()) => {
                    info!(target: "mh.grpc.gc_task", "Re-registered with GC; reporting at once");
                    if let Err(error) = gc.send_load_report(health_of(drain)).await {
                        warn!(
                            target: "mh.grpc.gc_task",
                            error = %error,
                            "Post-re-registration load report failed; the ticker retries"
                        );
                    }
                    true
                }
                Err(error) => {
                    warn!(
                        target: "mh.grpc.gc_task",
                        error = %error,
                        "Re-registration failed; retrying on the next tick"
                    );
                    false
                }
            }
        }
        Err(error) => {
            warn!(target: "mh.grpc.gc_task", error = %error, "Load report failed");
            false
        }
    }
}

/// A ticker whose first tick is ONE interval from now (the immediate report
/// has just been sent), skipping missed ticks.
fn ticker_from(period: Duration) -> Interval {
    // A zero interval would panic in `interval_at`; GC never pushes 0 (the
    // client keeps its default then), but never panic on it.
    let period = period.max(Duration::from_millis(1));
    let mut ticker = tokio::time::interval_at(Instant::now() + period, period);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker
}
