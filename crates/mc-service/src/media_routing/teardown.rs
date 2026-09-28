//! Releasing an ended meeting on its media handlers (story 2 R-20; ADR-0036 §7).
//!
//! # Two different "end meeting"s, named apart
//!
//! - **`EndMeeting` (this module)** is the MC→MH RPC: "release this meeting's
//!   routing state, edge budget and connections on THIS handler". It changes no
//!   forwarding policy; it frees a handler's resources.
//! - **Closing the MC-side meeting** is `MeetingMessage::CloseMeeting` /
//!   `MeetingActorHandle::close_meeting` — the meeting actor's own lifecycle.
//!   It was called `EndMeeting` before this task and was renamed so the two
//!   concepts stop sharing a name one call apart.
//! - Neither is `NotifyMeetingEnded`, which is MC→GC fleet bookkeeping.
//!
//! # The ordering MC owes, and how the types carry it
//!
//! `EndMeeting` is unconditional by meeting id and carries no generation, so
//! ordering is MC's obligation (`EndMeetingRequest` in `internal.proto` is the
//! normative statement): before the call, MC QUIESCES every `RegisterMeeting`
//! push for the meeting — no new attempt starts, and every attempt already
//! issued has RETURNED rather than been dropped.
//!
//! [`quiesce`] is the only thing that can produce a [`Quiesced`], and
//! [`end_meeting_plan`] will not build a plan without one. So "after the last
//! push returned" is a type obligation, not a comment a later edit can skip.
//!
//! # Story-2 assumption, and the story-4 fence
//!
//! STORY-2 ASSUMPTION: no periodic §8 re-assert exists yet, so the only
//! `RegisterMeeting` pushes are the structural ones this meeting's own push
//! workers make, and draining those workers is sufficient ordering.
//!
//! STORY-4 NOTE: once the periodic re-assert lands, a re-assert racing this
//! release RESURRECTS the ended meeting's routes and edge budget on a timer.
//! Draining the workers does not cover a timer-driven push. That needs a real
//! fence — `docs/TODO.md`, "`EndMeeting` has no fence against a late
//! `RegisterMeeting`". MH's apply-time registration check (story 2 task 12) is
//! ORTHOGONAL to that fence: it refuses APPLIES for an UNREGISTERED meeting,
//! and a re-assert re-registers the meeting, so it passes by design.
//!
//! # What an unclean end does NOT do
//!
//! A meeting actor that panics, or an MC process killed outright, never reaches
//! this module: nothing is released, and MH keeps the meeting's registration
//! until its own restart. That residual is tracked in `docs/TODO.md`, "A
//! meeting whose MC never sends `EndMeeting` is never reclaimed".
//!
//! # No key material
//!
//! A release carries a meeting id and an MC id. Nothing else.

use super::generation::PolicyGenerations;
use super::placement::MeetingHandlers;
use crate::grpc::mh_client::{MH_CONNECT_TIMEOUT, MH_RPC_TIMEOUT};
use crate::grpc::MhRegistrationClient;
use crate::observability::metrics;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Headroom for a drained push worker's own bookkeeping between its last RPC
/// returning and its task ending (the confirm/record and the loop's exit).
///
/// Named so the derivation below reads as a derivation: a healthy drain never
/// comes near the bound, and this exists only so it cannot be tripped by
/// scheduling jitter at the edge.
pub const QUIESCE_SLACK: Duration = Duration::from_secs(1);

/// How long a teardown waits for its push workers to drain, AND — on the
/// timeout path — how long it then waits again before releasing.
///
/// # Derived, not chosen
///
/// A worker honours cancel at attempt start, in backoff and while idle, but
/// never mid-RPC, so after cancel at most ONE attempt can still be in flight.
/// One attempt is bounded by the channel's connect timeout plus its RPC
/// timeout. A bound shorter than that would give up on a legitimate in-flight
/// attempt, which is exactly what `EndMeetingRequest`'s quiesce MUST forbids.
///
/// # Why the SAME constant is also the ordered wait (Race B)
///
/// If the drain does time out, an attempt may still be in flight whose
/// `RegisterMeeting` has not yet reached MH. Releasing now would let that
/// registration's upsert land AFTER the release and re-register the meeting —
/// and MH cannot tell a re-registered meeting from a live one, so its
/// apply-time check correctly lets it install. Waiting one further full attempt
/// window guarantees that attempt's client deadline has expired, so its request
/// has been dropped and MH's server handler cancelled, before the release goes
/// out.
///
/// That guarantee rests on the RPC layer cancelling a dropped request's server
/// handler — a property of the transport library, not of anything in this tree.
/// It FAILS OPEN: if it ever changed, nothing here would turn red. So this wait
/// ORDERS Race B; it does not close it. Emitted on MC's startup line as
/// `push_quiesce_bound_seconds`.
pub const PUSH_QUIESCE_BOUND: Duration = MH_CONNECT_TIMEOUT
    .saturating_add(MH_RPC_TIMEOUT)
    .saturating_add(QUIESCE_SLACK);

/// Attempts per handler for one release.
///
/// `EndMeeting` IS safe to retry: a release is idempotent (an unknown or
/// already-released meeting acknowledges), so a retry cannot release anything
/// the first attempt did not. Contrast `NotifyMeetingEnded`, which is NOT
/// retried on an ambiguous failure — see `grpc::gc_client`.
pub const MAX_END_MEETING_ATTEMPTS: u32 = 2;

/// Backoff between release attempts. The pusher's first backoff step, reused
/// rather than restated.
pub const END_MEETING_BACKOFF: Duration = super::pusher::REGISTER_BACKOFF_DELAYS[0];

/// The worst case one teardown can take, end to end: a drain that times out,
/// the ordered wait after it, then every release attempt at its full deadline
/// with the backoff between them.
///
/// This is how long a rejoin of the same meeting id can be held behind the
/// controller's teardown fence. Emitted on MC's startup line as
/// `teardown_fence_hold_max_seconds`, so the runbook names a field rather than
/// restating a number.
pub const TEARDOWN_MAX: Duration = {
    let release_attempt = MH_CONNECT_TIMEOUT.saturating_add(MH_RPC_TIMEOUT);
    let mut releases = Duration::ZERO;
    let mut i = 0;
    while i < MAX_END_MEETING_ATTEMPTS {
        releases = releases.saturating_add(release_attempt);
        if i > 0 {
            releases = releases.saturating_add(END_MEETING_BACKOFF);
        }
        i += 1;
    }
    PUSH_QUIESCE_BOUND
        .saturating_add(PUSH_QUIESCE_BOUND)
        .saturating_add(releases)
};

/// `terminationGracePeriodSeconds` of the MC pods (`infra/services/mc-service/mc-{0,1}-deployment.yaml`).
///
/// A second encoding of a manifest value, so it is GUARDED: the unit test
/// `shutdown_budget_matches_the_deployed_pod_grace` reads both manifests and
/// fails on drift. Change the manifests and this together.
pub const SHUTDOWN_TERMINATION_GRACE_SECONDS: u64 = 35;

/// `main`'s pause between cancelling the auxiliary tasks (GC task, servers)
/// and draining the actor system. The meeting actors are still running during
/// it, so it buys the teardowns nothing and is SUBTRACTED from their window.
pub const SHUTDOWN_PRE_DRAIN_SECONDS: u64 = 2;

/// Seconds kept between the end of the release window and `SIGKILL`, for what
/// runs after it: the `TokenManager` abort and the OpenTelemetry guard's flush
/// at the end of `main` (the same sizing as mh-service's
/// `SHUTDOWN_MARGIN_SECONDS`).
pub const SHUTDOWN_MARGIN_SECONDS: u64 = 5;

/// How long a graceful shutdown lets its meetings' teardowns run: the actor
/// drain AND the release settle share this ONE deadline.
///
/// Derived from the pod grace rather than chosen:
/// `SHUTDOWN_TERMINATION_GRACE_SECONDS − SHUTDOWN_PRE_DRAIN_SECONDS −
/// SHUTDOWN_MARGIN_SECONDS − SHUTDOWN_NOTIFY_FLUSH_BUDGET` (35 − 2 − 5 − 3 =
/// 25 s today). A healthy teardown (nothing in flight, `EndMeeting` answered in
/// milliseconds) fits with room to spare, and so does one with a push still in
/// flight at the full [`PUSH_QUIESCE_BOUND`]. What does NOT fit is
/// [`TEARDOWN_MAX`]'s degraded path (an unreachable handler): those teardowns
/// are cut off at the deadline, logged with their count, and collapse into the
/// crash residual. Raising the pod grace is the knob; this follows it.
pub const SHUTDOWN_RELEASE_BUDGET: Duration = Duration::from_secs(
    SHUTDOWN_TERMINATION_GRACE_SECONDS
        - SHUTDOWN_PRE_DRAIN_SECONDS
        - SHUTDOWN_MARGIN_SECONDS
        - SHUTDOWN_NOTIFY_FLUSH_BUDGET_SECONDS,
);

/// Seconds RESERVED, out of the same pod grace, for flushing GC meeting-ended
/// notifications AFTER the releases that produce them.
///
/// # Why this is carved out rather than added on
///
/// A meeting whose teardown completes during the release window calls
/// `meeting_ended` at that moment, so its notification is produced LAST — after
/// the release, by construction. If the notify drain ended when the shutdown
/// token fired, every such notification would be dropped (counted, but lost):
/// the stale GC assignment row that the revive fix exists to prevent, arriving
/// through the release window itself. So the drain outlives the releases and
/// gets this slice. It is SUBTRACTED from [`SHUTDOWN_RELEASE_BUDGET`] rather
/// than appended to it, so the grace arithmetic still closes:
/// pre-drain + releases + flush + margin == the pod grace, exactly.
pub const SHUTDOWN_NOTIFY_FLUSH_BUDGET_SECONDS: u64 = 3;

/// How long the GC meeting-ended drain flushes what is already queued once
/// shutdown begins. Bounded on BOTH sides: this deadline, and the drain
/// awaiting it in `main`.
pub const SHUTDOWN_NOTIFY_FLUSH_BUDGET: Duration =
    Duration::from_secs(SHUTDOWN_NOTIFY_FLUSH_BUDGET_SECONDS);

const _: () = assert!(
    SHUTDOWN_TERMINATION_GRACE_SECONDS
        > SHUTDOWN_PRE_DRAIN_SECONDS
            + SHUTDOWN_MARGIN_SECONDS
            + SHUTDOWN_NOTIFY_FLUSH_BUDGET_SECONDS,
    "the pod grace leaves no window for shutdown teardowns"
);

/// Counts teardowns that have been handed off and not yet finished, so a
/// graceful shutdown can wait for them instead of dropping them with the
/// runtime.
///
/// One per MC process, shared by every meeting actor through its wiring. A
/// teardown is counted from the moment its meeting actor hands it off (taken
/// synchronously, before the task is spawned, so there is no window in which
/// a handed-off teardown is uncounted) until its task ends on any path.
#[derive(Debug, Default)]
pub struct TeardownTracker {
    in_flight: std::sync::atomic::AtomicUsize,
    idle: tokio::sync::Notify,
}

/// One counted teardown; uncounts itself on drop (every task exit, panic
/// included).
#[derive(Debug)]
pub struct TeardownInFlight {
    tracker: Arc<TeardownTracker>,
}

impl TeardownTracker {
    /// Count one teardown until the returned guard drops.
    #[must_use]
    pub fn begin(self: &Arc<Self>) -> TeardownInFlight {
        self.in_flight
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        TeardownInFlight {
            tracker: Arc::clone(self),
        }
    }

    /// Teardowns handed off and not yet finished.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Wait until no teardown is in flight or `deadline` passes; returns how
    /// many were still in flight (cut off) at the end.
    pub async fn settle_until(&self, deadline: tokio::time::Instant) -> usize {
        loop {
            // Create the `Notified` future BEFORE reading the count: it
            // snapshots the notify generation at creation, so a drop-to-zero
            // that fires `notify_waiters` between the read and the await is not
            // lost — the next poll returns `Ready` rather than blocking.
            let notified = self.idle.notified();
            let remaining = self.in_flight();
            if remaining == 0 {
                return 0;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.in_flight();
            }
        }
    }
}

impl Drop for TeardownInFlight {
    fn drop(&mut self) {
        let previous = self
            .tracker
            .in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        if previous == 1 {
            self.tracker.idle.notify_waiters();
        }
    }
}

/// Witness that a meeting's push workers were drained.
///
/// Only [`quiesce`] constructs one. [`end_meeting_plan`] requires it, so a
/// release cannot be planned for a meeting whose pushes might still be issuing.
#[derive(Debug)]
pub struct Quiesced {
    /// Whether the drain hit [`PUSH_QUIESCE_BOUND`] (and the ordered wait was
    /// then performed). The release still goes out: skipping it is a guaranteed
    /// leak of the meeting's MH state, whereas proceeding risks only the
    /// residual the ordered wait exists to reduce.
    pub timed_out: bool,
}

/// How a teardown's drain ended. One increment per MEETING teardown — never
/// per handler (contrast [`EndMeetingOutcome`], which is per (meeting,
/// handler)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuiesceOutcome {
    /// Every worker's task ended within the bound.
    Quiesced,
    /// The bound expired; the ordered wait followed and the release was still
    /// sent. The leading indicator of a late apply racing a release.
    TimedOut,
}

impl QuiesceOutcome {
    /// Every variant, for exhaustive metric-vocabulary tests and zero-init.
    pub const ALL: [Self; 2] = [Self::Quiesced, Self::TimedOut];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Quiesced => "quiesced",
            Self::TimedOut => "timed_out",
        }
    }
}

/// Drain a meeting's push workers: cancel their token, then AWAIT each task.
///
/// Never aborts. If the bound expires, the remaining tasks are detached (a
/// dropped `JoinHandle` detaches; it does not abort), the ordered wait follows,
/// and the result says so.
pub async fn quiesce(cancel: CancellationToken, tasks: Vec<JoinHandle<()>>) -> Quiesced {
    cancel.cancel();
    let drained = tokio::time::timeout(PUSH_QUIESCE_BOUND, async move {
        for task in tasks {
            // A worker that panicked has still stopped issuing; that is all the
            // quiesce needs. The panic is the worker's to report.
            let _ = task.await;
        }
    })
    .await;
    if drained.is_ok() {
        metrics::record_push_quiesce(QuiesceOutcome::Quiesced);
        return Quiesced { timed_out: false };
    }
    metrics::record_push_quiesce(QuiesceOutcome::TimedOut);
    error!(
        target: "mc.teardown",
        bound_ms = PUSH_QUIESCE_BOUND.as_millis() as u64,
        "Push workers did not drain within the quiesce bound; waiting one further attempt window \
         before releasing so an in-flight registration's deadline has expired first. The release \
         is still sent."
    );
    tokio::time::sleep(PUSH_QUIESCE_BOUND).await;
    Quiesced { timed_out: true }
}

/// One handler to release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndMeetingCall {
    /// The handler's id, for the log line (never a metric label).
    pub handler_id: String,
    /// Its gRPC endpoint.
    pub mh_grpc_endpoint: String,
    /// The meeting to release.
    pub meeting_id: String,
    /// The `mc_id` this MC registered the meeting with.
    pub mc_id: String,
}

/// The pure teardown decision: which calls, to whom, carrying what.
///
/// - EVERY handler in the meeting's frozen set, not only those this MC pushed
///   to — a handler that never received a registration acknowledges the
///   unknown meeting as a no-op, and skipping it would rest on MC's bookkeeping
///   of which pushes landed, which is exactly the state a teardown cannot trust.
/// - `mc_id` is the caller's single source — `MediaRoutingDeps.mc_id`, the same
///   field every `PushTarget.mc_id` reads — never re-derived here. A mismatch is
///   rejected by MH by design.
/// - Requires a [`Quiesced`], so no plan exists while pushes might be issuing.
#[must_use]
pub fn end_meeting_plan(
    meeting_id: &str,
    mc_id: &str,
    handlers: &MeetingHandlers,
    _quiesced: &Quiesced,
) -> Vec<EndMeetingCall> {
    handlers
        .iter()
        .map(|h| EndMeetingCall {
            handler_id: h.id.to_string(),
            mh_grpc_endpoint: h.grpc_endpoint.clone(),
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
        })
        .collect()
}

/// Why one `EndMeeting` attempt did not succeed, as classified from MH's reply.
///
/// Carries the status detail (code and message) so the caller logs the cause
/// rather than discarding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndMeetingFailure {
    /// The class, which decides retry and outcome.
    pub kind: EndMeetingFailureKind,
    /// Status code and message, for the log line only.
    pub detail: String,
}

/// The class of a failed `EndMeeting` attempt (`EndMeetingRequest`'s contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMeetingFailureKind {
    /// `FAILED_PRECONDITION`: another MC's `mc_id` holds the meeting. Terminal.
    RejectedOwnership,
    /// `UNIMPLEMENTED`: the handler predates the RPC. Expected during a rollout
    /// in the wrong order; non-fatal, never retried.
    Unimplemented,
    /// `INVALID_ARGUMENT`: an empty or over-long id. Unreachable from MC's own
    /// values; terminal.
    InvalidArgument,
    /// Transport failure, `UNAVAILABLE` or `DEADLINE_EXCEEDED`: nothing was
    /// released on this attempt, and the release is idempotent, so it retries.
    Retryable,
    /// Anything else, including an `Ok` that did not acknowledge (a failure
    /// must never read as an empty reply). Terminal.
    Other,
}

/// The outcome of releasing ONE handler — one increment per (meeting, handler).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMeetingOutcome {
    /// Acknowledged: released, OR the meeting was unknown there. The two are one
    /// outcome by design — MC's correct action is identical.
    Released,
    /// A different `mc_id` holds the meeting, on a teardown for a meeting that
    /// ENDED (emptied or closed). An MC defect: MH rejects a mismatched id by
    /// design, this MC only ever sends its own, and GC cannot re-assign an
    /// ended meeting until MC's post-teardown notify lands.
    ///
    /// The shutdown population is split out as [`Self::SupersededBySuccessor`]
    /// on purpose: a departing MC's release being refused because a successor
    /// already took the meeting over is ROUTINE on every rolling deploy, and
    /// folded in here it would make this value's alert fire on deploys, get
    /// silenced, and hide the defect. Do not merge them back.
    RejectedOwnership,
    /// A different `mc_id` holds the meeting, on a GRACEFUL-SHUTDOWN teardown.
    /// The expected cause is a successor MC that already re-registered the
    /// meeting — MH's ownership check refusing a release that would have cut a
    /// live meeting. CONSISTENT with supersession, not proof of it: a wrong
    /// `mc_id` sent during shutdown lands here too and is indistinguishable at
    /// this recording site. Decided from MC's own teardown reason, never
    /// inferred from MH's reply. Shares no word with `rejected_ownership` so a
    /// grep for either names one thing.
    SupersededBySuccessor,
    /// The handler predates the RPC; see [`EndMeetingFailureKind::Unimplemented`].
    Unimplemented,
    /// Retries exhausted on transport or availability failures.
    UnavailableExhausted,
    /// Rejected as malformed.
    InvalidArgument,
    /// Any other failure.
    Error,
}

impl EndMeetingOutcome {
    /// Every variant, for exhaustive metric-vocabulary tests and zero-init.
    ///
    /// **ADDING A VARIANT IS ALSO AN ALERT EDIT, and nothing in Rust will tell
    /// you.** `MCEndMeetingFailureRate`
    /// (`infra/docker/prometheus/rules/mc-alerts.yaml`) enumerates its failure
    /// values POSITIVELY, so a new variant defaults to NOT being a failure and
    /// is invisible to that rule until someone classifies it. That default is
    /// deliberate — the alternative silently pages on arrival, which is what a
    /// negated predicate would have done to `SupersededBySuccessor` on every
    /// rolling deploy — but it means the obligation lands here, at the one place
    /// a variant is added: decide whether the new value is a failure, a benign
    /// outcome, or neither-and-out-of-the-denominator, and edit both selectors
    /// in that rule plus the row in
    /// `docs/observability/metrics/mc-service.md`.
    pub const ALL: [Self; 7] = [
        Self::Released,
        Self::RejectedOwnership,
        Self::SupersededBySuccessor,
        Self::Unimplemented,
        Self::UnavailableExhausted,
        Self::InvalidArgument,
        Self::Error,
    ];

    /// Bounded metric-label form.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::RejectedOwnership => "rejected_ownership",
            Self::SupersededBySuccessor => "superseded_by_successor",
            Self::Unimplemented => "unimplemented",
            Self::UnavailableExhausted => "unavailable_exhausted",
            Self::InvalidArgument => "invalid_argument",
            Self::Error => "error",
        }
    }

    /// The terminal outcome for a non-retryable failure, given why MC is
    /// tearing the meeting down.
    fn terminal(kind: EndMeetingFailureKind, reason: TeardownReason) -> Self {
        match kind {
            EndMeetingFailureKind::RejectedOwnership => match reason {
                TeardownReason::MeetingEnded => Self::RejectedOwnership,
                TeardownReason::Shutdown => Self::SupersededBySuccessor,
            },
            EndMeetingFailureKind::Unimplemented => Self::Unimplemented,
            EndMeetingFailureKind::InvalidArgument => Self::InvalidArgument,
            EndMeetingFailureKind::Retryable => Self::UnavailableExhausted,
            EndMeetingFailureKind::Other => Self::Error,
        }
    }
}

/// Why MC is tearing a meeting down. Decides only how an ownership refusal is
/// classified; every other step of the teardown is identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeardownReason {
    /// The meeting ended (emptied or closed).
    MeetingEnded,
    /// MC is shutting down gracefully; a successor MC may already hold the
    /// meeting.
    Shutdown,
}

/// Release one handler, retrying only what the contract says may be retried.
async fn release_one(
    mh_client: &dyn MhRegistrationClient,
    call: &EndMeetingCall,
    reason: TeardownReason,
) -> EndMeetingOutcome {
    let mut attempt = 1;
    loop {
        match mh_client.end_meeting(call).await {
            Ok(()) => return EndMeetingOutcome::Released,
            Err(failure) => {
                let retry = failure.kind == EndMeetingFailureKind::Retryable
                    && attempt < MAX_END_MEETING_ATTEMPTS;
                if retry {
                    warn!(
                        target: "mc.teardown",
                        meeting_id = %call.meeting_id,
                        mh_id = %call.handler_id,
                        attempt,
                        max_attempts = MAX_END_MEETING_ATTEMPTS,
                        detail = %failure.detail,
                        "EndMeeting attempt failed; retrying (a release is idempotent)"
                    );
                    tokio::time::sleep(END_MEETING_BACKOFF).await;
                    attempt += 1;
                    continue;
                }
                let outcome = EndMeetingOutcome::terminal(failure.kind, reason);
                log_release_failure(call, outcome, attempt, &failure.detail);
                return outcome;
            }
        }
    }
}

fn log_release_failure(
    call: &EndMeetingCall,
    outcome: EndMeetingOutcome,
    attempts: u32,
    detail: &str,
) {
    match outcome {
        // Expected during a wrong-order rollout, and non-fatal: MH keeps the
        // meeting until its own restart. WARN, not ERROR.
        EndMeetingOutcome::Unimplemented => warn!(
            target: "mc.teardown",
            meeting_id = %call.meeting_id,
            mh_id = %call.handler_id,
            outcome = outcome.label(),
            detail = %detail,
            "Handler does not implement EndMeeting (it predates the RPC); it keeps this meeting's \
             state until it restarts. Check the rollout order: MH rolls forward first."
        ),
        // Expected on a rolling deploy: the successor already holds the meeting.
        EndMeetingOutcome::SupersededBySuccessor => info!(
            target: "mc.teardown",
            meeting_id = %call.meeting_id,
            mh_id = %call.handler_id,
            outcome = outcome.label(),
            detail = %detail,
            "Shutdown release refused: another MC holds this meeting (expected when a successor \
             took it over during a rollout)"
        ),
        _ => error!(
            target: "mc.teardown",
            meeting_id = %call.meeting_id,
            mh_id = %call.handler_id,
            outcome = outcome.label(),
            attempts,
            detail = %detail,
            "EndMeeting failed; this handler keeps the meeting's routing state and edge budget"
        ),
    }
}

/// Everything a meeting's MH-side teardown needs, taken from the meeting actor
/// as it exits.
pub struct MediaTeardown {
    /// The meeting being released.
    pub meeting_id: String,
    /// This MC's registering `mc_id`.
    pub mc_id: String,
    /// The meeting's frozen handler set.
    pub handlers: MeetingHandlers,
    /// The push workers' token (moved out of the actor, never cancelled by it).
    pub pushers_cancel: CancellationToken,
    /// The push workers' tasks, taken so nothing can abort them.
    pub pusher_tasks: Vec<JoinHandle<()>>,
    /// The registration client.
    pub mh_client: Arc<dyn MhRegistrationClient>,
    /// The generation registry, evicted AFTER the drain.
    pub policy_generations: Arc<PolicyGenerations>,
    /// Why the meeting is being torn down.
    pub reason: TeardownReason,
}

impl std::fmt::Debug for MediaTeardown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaTeardown")
            .field("meeting_id", &self.meeting_id)
            .field("handlers", &self.handlers.len())
            .field("pusher_tasks", &self.pusher_tasks.len())
            .finish_non_exhaustive()
    }
}

/// Run one meeting's MH-side teardown: drain, plan, release every handler
/// concurrently, then evict the meeting's policy generations.
///
/// Eviction comes LAST, after the drain: a still-running worker could otherwise
/// re-create the registry entry (floor adoption writes it), and a meeting later
/// re-created under this id must push from generation 1 into a handler that
/// forgot the old one.
pub async fn run(teardown: MediaTeardown) {
    let MediaTeardown {
        meeting_id,
        mc_id,
        handlers,
        pushers_cancel,
        pusher_tasks,
        mh_client,
        policy_generations,
        reason,
    } = teardown;

    let quiesced = quiesce(pushers_cancel, pusher_tasks).await;
    let calls = end_meeting_plan(&meeting_id, &mc_id, &handlers, &quiesced);

    let mut releases = tokio::task::JoinSet::new();
    for call in calls {
        let client = Arc::clone(&mh_client);
        releases.spawn(async move {
            let outcome = release_one(client.as_ref(), &call, reason).await;
            metrics::record_end_meeting(outcome);
            outcome
        });
    }
    let mut released = 0usize;
    let mut failed = 0usize;
    while let Some(result) = releases.join_next().await {
        match result {
            Ok(EndMeetingOutcome::Released) => released += 1,
            Ok(_) => failed += 1,
            Err(join_error) => {
                // A release task that panicked recorded nothing; count it as the
                // failure it is, loudly.
                metrics::record_end_meeting(EndMeetingOutcome::Error);
                failed += 1;
                error!(
                    target: "mc.teardown",
                    meeting_id = %meeting_id,
                    error = %join_error,
                    "An EndMeeting task did not complete"
                );
            }
        }
    }

    policy_generations.remove_meeting(&meeting_id).await;

    info!(
        target: "mc.teardown",
        meeting_id = %meeting_id,
        handlers = handlers.len(),
        released,
        failed,
        quiesce_timed_out = quiesced.timed_out,
        "Meeting released on its media handlers"
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::placement::HandlerEndpoint;
    use crate::media_routing::HandlerId;

    fn handlers(ids: &[&str]) -> MeetingHandlers {
        MeetingHandlers::new(ids.iter().map(|id| HandlerEndpoint {
            id: HandlerId::new(*id),
            webtransport_url: format!("https://{id}.example:4434"),
            grpc_endpoint: format!("http://{id}.example:50053"),
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn the_plan_names_every_frozen_handler_with_this_mcs_id_and_the_right_meeting() {
        let quiesced = quiesce(CancellationToken::new(), Vec::new()).await;
        let plan = end_meeting_plan(
            "meeting-7",
            "mc-self",
            &handlers(&["mh-0", "mh-1"]),
            &quiesced,
        );
        assert_eq!(
            plan,
            vec![
                EndMeetingCall {
                    handler_id: "mh-0".to_string(),
                    mh_grpc_endpoint: "http://mh-0.example:50053".to_string(),
                    meeting_id: "meeting-7".to_string(),
                    mc_id: "mc-self".to_string(),
                },
                EndMeetingCall {
                    handler_id: "mh-1".to_string(),
                    mh_grpc_endpoint: "http://mh-1.example:50053".to_string(),
                    meeting_id: "meeting-7".to_string(),
                    mc_id: "mc-self".to_string(),
                },
            ],
            "EVERY handler in the frozen set, none extra, none missing, each carrying this \
             MC's own id and the meeting being released"
        );
    }

    #[tokio::test]
    async fn a_single_handler_meeting_plans_exactly_one_call() {
        let quiesced = quiesce(CancellationToken::new(), Vec::new()).await;
        assert_eq!(
            end_meeting_plan("m", "mc", &handlers(&["only"]), &quiesced).len(),
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn quiesce_awaits_an_in_flight_worker_rather_than_aborting_it() {
        let cancel = CancellationToken::new();
        let returned = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&returned);
        let worker_cancel = cancel.clone();
        // A worker "in flight": it ignores cancel until its RPC returns, as the
        // real worker does mid-RPC.
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = worker_cancel;
        });
        let quiesced = quiesce(cancel.clone(), vec![task]).await;
        assert!(cancel.is_cancelled(), "the workers' token is cancelled");
        assert!(
            !quiesced.timed_out,
            "a 3 s in-flight RPC is inside the bound"
        );
        assert!(
            returned.load(std::sync::atomic::Ordering::SeqCst),
            "the in-flight attempt RETURNED before quiesce did — it was awaited, not aborted"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_drain_past_the_bound_reports_timed_out_after_the_ordered_wait() {
        let start = tokio::time::Instant::now();
        let stuck = tokio::spawn(std::future::pending::<()>());
        let quiesced = quiesce(CancellationToken::new(), vec![stuck]).await;
        assert!(quiesced.timed_out);
        assert!(
            start.elapsed() >= PUSH_QUIESCE_BOUND * 2,
            "the timeout path waits one FURTHER attempt window before releasing (Race B)"
        );
    }

    #[test]
    fn the_quiesce_bound_covers_one_full_attempt() {
        assert!(PUSH_QUIESCE_BOUND > MH_CONNECT_TIMEOUT + MH_RPC_TIMEOUT);
    }

    #[test]
    fn the_teardown_maximum_covers_drain_wait_and_every_release_attempt() {
        let attempt = MH_CONNECT_TIMEOUT + MH_RPC_TIMEOUT;
        assert_eq!(
            TEARDOWN_MAX,
            PUSH_QUIESCE_BOUND * 2
                + attempt * MAX_END_MEETING_ATTEMPTS
                + END_MEETING_BACKOFF * (MAX_END_MEETING_ATTEMPTS - 1)
        );
    }

    #[test]
    fn only_availability_failures_are_retryable_and_each_class_has_its_own_outcome() {
        use EndMeetingFailureKind as K;
        use TeardownReason::{MeetingEnded as E, Shutdown as S};
        assert_eq!(
            EndMeetingOutcome::terminal(K::RejectedOwnership, E),
            EndMeetingOutcome::RejectedOwnership
        );
        // The shutdown population is its own value, decided from MC's reason.
        assert_eq!(
            EndMeetingOutcome::terminal(K::RejectedOwnership, S),
            EndMeetingOutcome::SupersededBySuccessor
        );
        assert_eq!(
            EndMeetingOutcome::terminal(K::Unimplemented, E),
            EndMeetingOutcome::Unimplemented
        );
        assert_eq!(
            EndMeetingOutcome::terminal(K::InvalidArgument, E),
            EndMeetingOutcome::InvalidArgument
        );
        assert_eq!(
            EndMeetingOutcome::terminal(K::Retryable, E),
            EndMeetingOutcome::UnavailableExhausted
        );
        assert_eq!(
            EndMeetingOutcome::terminal(K::Other, E),
            EndMeetingOutcome::Error
        );
        // Only an ownership refusal depends on the reason.
        for kind in [K::Unimplemented, K::InvalidArgument, K::Retryable, K::Other] {
            assert_eq!(
                EndMeetingOutcome::terminal(kind, E),
                EndMeetingOutcome::terminal(kind, S)
            );
        }
    }

    #[test]
    fn outcome_labels_are_distinct() {
        let labels: std::collections::HashSet<_> =
            EndMeetingOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(labels.len(), EndMeetingOutcome::ALL.len());
        let q: std::collections::HashSet<_> =
            QuiesceOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(q.len(), QuiesceOutcome::ALL.len());
    }

    // ------------------------------------------------------------------
    // Shutdown settle (OPS-18)
    // ------------------------------------------------------------------

    #[tokio::test(start_paused = true)]
    async fn settle_returns_at_once_when_nothing_is_in_flight() {
        let tracker = Arc::new(TeardownTracker::default());
        let start = tokio::time::Instant::now();
        let cut_off = tracker.settle_until(start + Duration::from_secs(10)).await;
        assert_eq!(cut_off, 0);
        assert_eq!(start.elapsed(), Duration::ZERO, "no waiting when idle");
    }

    #[tokio::test(start_paused = true)]
    async fn settle_waits_for_a_teardown_that_finishes_inside_the_deadline() {
        let tracker = Arc::new(TeardownTracker::default());
        let guard = tracker.begin();
        assert_eq!(tracker.in_flight(), 1);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            drop(guard);
        });
        let start = tokio::time::Instant::now();
        let cut_off = tracker.settle_until(start + Duration::from_secs(10)).await;
        assert_eq!(cut_off, 0);
        assert_eq!(
            start.elapsed(),
            Duration::from_secs(3),
            "returns when the teardown ends, not at the deadline"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn settle_reports_teardowns_cut_off_at_the_deadline() {
        let tracker = Arc::new(TeardownTracker::default());
        let _stuck = tracker.begin();
        let done = tracker.begin();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            drop(done);
        });
        let start = tokio::time::Instant::now();
        let cut_off = tracker.settle_until(start + Duration::from_secs(10)).await;
        assert_eq!(cut_off, 1, "the one still running is reported cut off");
        assert_eq!(start.elapsed(), Duration::from_secs(10));
    }

    /// `SHUTDOWN_TERMINATION_GRACE_SECONDS` restates the pods'
    /// `terminationGracePeriodSeconds`; this is the guard on that second copy.
    /// Reading the manifests (not a fixture) is the point.
    #[test]
    fn shutdown_budget_matches_the_deployed_pod_grace() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../infra/services/mc-service");
        let mut checked = 0;
        for manifest in ["mc-0-deployment.yaml", "mc-1-deployment.yaml"] {
            let text = std::fs::read_to_string(dir.join(manifest))
                .expect("the MC deployment manifest is readable from the crate");
            let graces: Vec<u64> = text
                .lines()
                .filter_map(|l| l.trim().strip_prefix("terminationGracePeriodSeconds:"))
                .map(|v| v.trim().parse().expect("an integer grace"))
                .collect();
            assert_eq!(
                graces,
                vec![SHUTDOWN_TERMINATION_GRACE_SECONDS],
                "{manifest}: terminationGracePeriodSeconds must equal \
                 SHUTDOWN_TERMINATION_GRACE_SECONDS, which sizes SHUTDOWN_RELEASE_BUDGET"
            );
            checked += 1;
        }
        assert_eq!(checked, 2);
        assert!(
            SHUTDOWN_RELEASE_BUDGET >= PUSH_QUIESCE_BOUND,
            "the shutdown window must at least cover draining one in-flight push"
        );
    }
}
