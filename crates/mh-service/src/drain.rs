//! Planned drain on SIGTERM (story 4 MH-3; ADR-0039 §7, which reverses
//! ADR-0036 §11 "MH sheds media sessions on restart").
//!
//! One mechanism for `mh.sh drain`, upgrade, scale-down and node maintenance:
//! MH stops being selectable, tells every MC holding a meeting on it that it is
//! draining and how long it has left, KEEPS FORWARDING while MC moves each
//! meeting away, and exits as soon as it holds nothing or at the deadline.
//!
//! # One state, one deadline
//!
//! Draining is PROCESS state ([`DrainWatch`]): a `watch` channel holding the
//! one [`DrainDeadline`], set once at signal receipt and never cleared. Every
//! outward "am I serving" signal reads it: GC's load-report health
//! (`grpc::gc_task`), the heartbeat's `ServingState` and `drain_remaining_ms`
//! (the session actor), and readiness (`HealthState::begin_drain`, latched by
//! `main`). Nothing recomputes the deadline; `drain_remaining_ms` is derived
//! from it at beat time, so MH's grace stays the only copy.
//!
//! # Sequence ([`run`])
//!
//! 1. Publish the deadline (`signal + Config::drain_window`, i.e.
//!    `grace − SHUTDOWN_MARGIN_SECONDS`). GC gets an immediate DRAINING
//!    report; every held meeting gets an immediate DRAINING beat.
//! 2. Wait — on the actor's held-meeting count, never by polling — until it is
//!    zero OR the deadline passes. Neither GC nor MC reachability is an input:
//!    draining never self-fences on MC silence.
//! 3. At the deadline, shed what is left in ONE actor turn: each meeting is
//!    released through the one release path with `ReleaseKind::DrainShed`, so
//!    its sessions close `HANDLER_RELEASED` (MH-CLOSE's mapping).
//! 4. Wait [`CLOSE_SESSION_LINGER`] so those coded closes (or the last
//!    release's, on the early exit) leave the process, THEN cancel the
//!    shutdown token. The order is load-bearing: a session whose shutdown
//!    token is cancelled first ends code-less (`ConnectionEnd::Shutdown`).
//!
//! Exit-time metrics are best-effort (they may never be scraped); the final
//! INFO line carries the outcome, the duration and the shed count, and MC's
//! `mc_media_handler_migrations_total{trigger="planned_transfer"}` is the
//! authoritative outcome.

use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::config::{CLOSE_SESSION_LINGER, SHUTDOWN_MARGIN_SECONDS};

/// The most the deadline shed may take (one actor turn behind the lifecycle
/// mailbox). Bounded so a full mailbox or a slow actor can never push the
/// drain past `grace − margin`: the linger, the shutdown and — above all —
/// the final "Drain finished" log line must still land before SIGKILL.
pub const SHED_BUDGET: Duration = Duration::from_secs(2);

// Shed budget + linger must leave room inside the margin for the TokenManager
// abort and the OTel flush.
const _: () =
    assert!(SHED_BUDGET.as_secs() + CLOSE_SESSION_LINGER.as_secs() < SHUTDOWN_MARGIN_SECONDS);
use crate::observability::metrics;
use crate::session::SessionManagerHandle;

/// The one drain deadline, a monotonic instant (paused-time testable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainDeadline {
    deadline: Instant,
}

impl DrainDeadline {
    /// A deadline at `deadline`. Production builds exactly one, in [`run`];
    /// public so the GC-task and actor rigs can publish a drain state.
    #[must_use]
    pub const fn new(deadline: Instant) -> Self {
        Self { deadline }
    }

    /// The deadline instant.
    #[must_use]
    pub const fn at(self) -> Instant {
        self.deadline
    }

    /// `drain_remaining_ms` for a heartbeat built at `now`: RELATIVE (no clock
    /// skew between MH and MC), saturating at 0 past the deadline and clamped
    /// into the proto's `uint32`, never cast.
    #[must_use]
    pub fn remaining_ms(self, now: Instant) -> u32 {
        let remaining = self.deadline.saturating_duration_since(now);
        u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX)
    }
}

/// The process-wide draining state: `None` while serving, `Some(deadline)`
/// from signal receipt until exit. Set once, never cleared.
pub type DrainWatch = watch::Receiver<Option<DrainDeadline>>;

/// A fresh, not-draining drain state.
#[must_use]
pub fn channel() -> (watch::Sender<Option<DrainDeadline>>, DrainWatch) {
    watch::channel(None)
}

/// Whether the process is draining.
#[must_use]
pub fn is_draining(drain: &DrainWatch) -> bool {
    drain.borrow().is_some()
}

/// Why the drain ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainExit {
    /// MH held no meetings (MC moved them all away, or there were none).
    Empty,
    /// The deadline passed with meetings still held; they were shed.
    Deadline,
}

impl DrainExit {
    /// Bounded value for the final log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Deadline => "deadline",
        }
    }
}

/// What a drain did. The shed count in the final log line IS this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainOutcome {
    /// Why it ended.
    pub exit: DrainExit,
    /// Meetings still held at the deadline and shed (0 on an empty exit, and
    /// 0 when the shed did not complete — see [`Self::shed_completed`]).
    pub shed: usize,
    /// `false` only when the deadline shed could not complete (the actor was
    /// gone, or did not answer within [`SHED_BUDGET`]): its remaining sessions
    /// then end code-less at shutdown, and `shed` is unknown (reported 0).
    pub shed_completed: bool,
    /// Signal receipt to the exit decision.
    pub elapsed: Duration,
}

/// Run the planned drain: publish the deadline, wait for "holds nothing" or the
/// deadline, shed the remainder, linger for the coded closes, then cancel
/// `shutdown`. See the module docs for the sequence and why its order matters.
///
/// `window` is [`crate::config::Config::drain_window`]. The caller has already
/// marked the pod not-ready.
pub async fn run(
    session: &SessionManagerHandle,
    drain: &watch::Sender<Option<DrainDeadline>>,
    window: Duration,
    shutdown: &CancellationToken,
) -> DrainOutcome {
    let started = Instant::now();
    // Checked, never `+`: the grace is startup-bounded, but the no-panic
    // property must not rest on validation alone. Overflow means "now".
    let deadline = DrainDeadline::new(started.checked_add(window).unwrap_or_else(|| {
        warn!(
            target: "mh.drain",
            drain_window_seconds = window.as_secs(),
            "Drain window overflows the clock; draining for zero time"
        );
        started
    }));
    let mut held = session.held_meetings();
    let held_at_start = *held.borrow_and_update();
    drain.send_replace(Some(deadline));
    metrics::set_drain_meetings_remaining(held_at_start);
    info!(
        target: "mh.drain",
        meetings_held = held_at_start,
        drain_deadline_seconds = window.as_secs(),
        "Drain started: reporting DRAINING to GC and every meeting's MC; forwarding \
         continues until no meeting is held or the deadline"
    );

    let exit = wait_for_empty_or_deadline(&mut held, deadline.at()).await;
    let (shed, shed_completed) = match exit {
        DrainExit::Empty => (0, true),
        DrainExit::Deadline => shed_remaining(session).await,
    };
    let elapsed = Instant::now().saturating_duration_since(started);
    metrics::record_drain_meetings_shed(shed);
    metrics::record_drain_duration(elapsed);
    metrics::set_drain_meetings_remaining(*held.borrow());

    // The runbook's evidence, written BEFORE the linger so nothing after the
    // exit decision can lose it: exit-time counters may never be scraped.
    info!(
        target: "mh.drain",
        exit_reason = exit.as_str(),
        meetings_shed = shed,
        shed_completed,
        drain_duration_seconds = elapsed.as_secs_f64(),
        "Drain finished"
    );

    // The coded closes just issued (shed, or the last release) must leave the
    // process before the shutdown token ends every remaining session
    // code-less. Bounded, and < SHUTDOWN_MARGIN_SECONDS (const-asserted).
    tokio::time::sleep(CLOSE_SESSION_LINGER).await;
    shutdown.cancel();
    DrainOutcome {
        exit,
        shed,
        shed_completed,
        elapsed,
    }
}

/// Wait until the actor holds no meeting, or the deadline. "Holds nothing"
/// wins a tie. An actor that has gone away holds nothing.
async fn wait_for_empty_or_deadline(
    held: &mut watch::Receiver<usize>,
    deadline: Instant,
) -> DrainExit {
    loop {
        let now_held = *held.borrow_and_update();
        metrics::set_drain_meetings_remaining(now_held);
        if now_held == 0 {
            return DrainExit::Empty;
        }
        tokio::select! {
            biased;
            changed = held.changed() => {
                if changed.is_err() {
                    warn!(
                        target: "mh.drain",
                        "Session actor gone during the drain; treating it as holding nothing"
                    );
                    return DrainExit::Empty;
                }
            }
            () = tokio::time::sleep_until(deadline) => {
                return if *held.borrow() == 0 {
                    DrainExit::Empty
                } else {
                    DrainExit::Deadline
                };
            }
        }
    }
}

/// Shed every meeting still held, in one actor turn. An actor that cannot be
/// reached, or does not answer within [`SHED_BUDGET`], sheds nothing it can
/// count (and is said so, loudly); `.1` is whether the shed completed.
async fn shed_remaining(session: &SessionManagerHandle) -> (usize, bool) {
    match tokio::time::timeout(SHED_BUDGET, session.shed_all()).await {
        Ok(Some(shed)) => (shed, true),
        Ok(None) => {
            warn!(
                target: "mh.drain",
                "Session actor unavailable at the drain deadline; nothing could be shed with a \
                 close code (remaining sessions end code-less at shutdown)"
            );
            (0, false)
        }
        Err(_elapsed) => {
            warn!(
                target: "mh.drain",
                shed_budget_ms = u64::try_from(SHED_BUDGET.as_millis()).unwrap_or(u64::MAX),
                "Drain-deadline shed did not complete within its budget; remaining sessions end \
                 code-less at shutdown and the shed count is unknown (reported 0)"
            );
            (0, false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_is_relative_saturating_and_clamped() {
        let now = Instant::now();
        let d = DrainDeadline {
            deadline: now + Duration::from_millis(1_500),
        };
        assert_eq!(d.remaining_ms(now), 1_500);
        assert_eq!(d.remaining_ms(now + Duration::from_millis(500)), 1_000);
        assert_eq!(
            d.remaining_ms(now + Duration::from_secs(5)),
            0,
            "never negative"
        );
        let far = DrainDeadline {
            deadline: now + Duration::from_secs(u64::from(u32::MAX)),
        };
        assert_eq!(far.remaining_ms(now), u32::MAX, "clamped, never wrapped");
    }

    #[test]
    fn exit_reasons_are_a_bounded_vocabulary() {
        assert_eq!(DrainExit::Empty.as_str(), "empty");
        assert_eq!(DrainExit::Deadline.as_str(), "deadline");
    }
}
