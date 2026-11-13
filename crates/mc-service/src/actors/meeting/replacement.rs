//! The meeting actor's replacement driver (ADR-0039 §3/§4; story 4 MC-C).
//!
//! The DECISIONS live in `media_routing::replacement` (pure). This module only
//! performs them: it issues ONE `AcquireMediaHandler` at a time in a spawned
//! task whose result comes back on a oneshot the actor's `select!` drains
//! (the actor never awaits GC, never awaits a `JoinHandle`), adds a grant as
//! Registering so the ordinary push path registers it, and counts every
//! attempt's outcome exactly once.
//!
//! Crash = break-before-make: the removal already sent clients the set without
//! the dead handler; the replacement reaches them only once its
//! `RegisterMeeting` is acked (register before token). Recovery is the join
//! path: participants connect, MH reports them, the normal render places
//! edges on the new handler — no special case.

use super::MeetingActor;
use crate::grpc::gc_client::{AcquireCall, AcquireResult};
use crate::media_routing::pusher::{HandlerRegistration, RegistrationOutcome};
use crate::media_routing::replacement::{
    registration_failure_trigger, Due, Granted, Owed, RemovalEffect, ReplacementState,
};
use crate::media_routing::{HandlerId, MigrationOutcome, MigrationPhase, RemovalTrigger};
use crate::observability::metrics;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tracing::{error, info, warn};

/// The attempt-end reason of a superseded grant (GC had capacity; re-asked
/// at once). ONE constant, because `end_attempt` DECIDES on it (a superseded
/// attempt is not a "no grant" answer), not just logs it.
const SUPERSEDED: &str = "superseded";

/// Why a migration started (the start log's `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartReason {
    /// A Dead/Impaired declaration (or a planned removal) took the handler out.
    Declared,
    /// The handler's FIRST `RegisterMeeting` never acked (ADR-0039 §3).
    InitialRegistrationFailed,
    /// A planned transfer cut a DRAINING handler over (story 4 MC-F).
    PlannedTransfer,
}

impl StartReason {
    const fn label(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::InitialRegistrationFailed => "initial_registration_failed",
            Self::PlannedTransfer => "planned_transfer",
        }
    }
}

/// The ONE Acquire call in flight. The result channel is a oneshot held only
/// while the call is open, so at most one report can exist by construction.
struct PendingAcquire {
    rx: oneshot::Receiver<AcquireResult>,
    task: tokio::task::AbortHandle,
}

/// A meeting's replacement state plus the in-flight call.
pub(super) struct Replacement {
    state: ReplacementState,
    pending: Option<PendingAcquire>,
}

impl Replacement {
    /// A meeting kept at `target` handlers (its hand-off size).
    pub(super) fn new(target: usize, seed: Option<u64>) -> Self {
        Self {
            state: ReplacementState::new(target, seed),
            pending: None,
        }
    }

    /// Owe `id`'s replacement while it is still a member (planned transfer;
    /// MC-C's one acquisition path). Returns whether it was interval-deferred.
    pub(super) fn on_draining(
        &mut self,
        id: &HandlerId,
        declared_at: Instant,
        now: Instant,
        timing: crate::media_routing::ReplacementTiming,
    ) -> bool {
        self.state.on_draining(id, declared_at, now, timing)
    }

    /// The attempt timer's wake.
    pub(super) fn next_wake(&self) -> Option<Instant> {
        self.state.next_attempt_at()
    }

    /// The in-flight result, as a future for the actor's `select!` (pending
    /// forever when nothing is in flight).
    pub(super) async fn result(&mut self) -> Result<AcquireResult, oneshot::error::RecvError> {
        match self.pending.as_mut() {
            Some(pending) => (&mut pending.rx).await,
            None => std::future::pending().await,
        }
    }
}

impl Drop for Replacement {
    fn drop(&mut self) {
        // GC persists nothing per meeting, so an abandoned call leaks nothing;
        // aborting just stops a task whose answer nobody will read.
        if let Some(pending) = self.pending.take() {
            pending.task.abort();
        }
    }
}

impl MeetingActor {
    /// Feed one completed removal into the replacement bookkeeping (called by
    /// the ONE removal transaction, after the set without the handler went
    /// out).
    pub(super) fn after_handler_removed(
        &mut self,
        removed: &HandlerId,
        trigger: RemovalTrigger,
        reason: StartReason,
    ) {
        let Some(timing) = self.media.deps().map(|d| d.replacement) else {
            return;
        };
        let now = Instant::now();
        // Draining members do not count toward the target (story 4 MC-F):
        // the ONE count every removal path passes.
        let members_after = self.media.non_draining_member_count();
        let RemovalEffect {
            refused,
            started,
            deferred,
        } = self
            .replacement
            .state
            .on_removal(removed, trigger, members_after, now, timing);
        if let Some(Granted { owed, .. }) = refused {
            // The grant left before its first ack: its registration failed,
            // OR it was declared Dead/Impaired while still Registering. The
            // removal's own reason and trigger say which.
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
                new_handler_id = %removed,
                removal_reason = reason.label(),
                removal_trigger = trigger.label(),
                "Replacement grant removed before its first RegisterMeeting ack"
            );
            self.end_attempt(
                &owed,
                Some(removed),
                MigrationOutcome::AdmissionRefused,
                reason.label(),
            );
        }
        if started {
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %removed,
                trigger = trigger.label(),
                reason = reason.label(),
                survivors = members_after,
                "Media handler migration started"
            );
        }
        if deferred {
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %removed,
                trigger = trigger.label(),
                reason = "min_interval",
                "Media handler replacement deferred by the per-meeting minimum interval"
            );
        }
    }

    /// The attempt timer fired: issue one Acquire, or refuse at the bound.
    pub(super) fn on_replacement_due(&mut self) {
        let Some(deps) = self.media.deps().cloned() else {
            let dropped = self.replacement.state.drop_owed();
            warn!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                dropped,
                "Media handler replacement owed before any join installed the routing deps; \
                 nothing was ever programmed, so nothing is acquired"
            );
            return;
        };
        let now = Instant::now();
        let handlers = self.media.handlers();
        let version = handlers.version();
        let exclude = handlers.excluded_ids();
        let cap = deps.acquirer.max_exclude_handler_ids();
        match self
            .replacement
            .state
            .due(now, version, exclude.len(), cap, deps.replacement)
        {
            Due::Idle => {}
            Due::NoCap => {
                warn!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    "Media handler replacement postponed: no exclude-list cap from GC yet (MC \
                     not registered with GC); nothing sent"
                );
                // No grant can come now: a planned transfer's survivors become
                // eligible (story 4 MC-F).
                self.planned.no_grant();
            }
            Due::Refused(refused) => {
                for owed in refused {
                    self.end_attempt(&owed, None, MigrationOutcome::RateLimited, "attempt_bound");
                }
            }
            Due::Attempt(owed) => {
                let call = AcquireCall {
                    meeting_id: self.meeting_id.clone(),
                    exclude,
                    trigger: owed.trigger,
                    replaced: owed.replaced,
                };
                let acquirer = Arc::clone(&deps.acquirer);
                let (tx, rx) = oneshot::channel();
                // Captures ONLY the call (ids + trigger) and the acquirer.
                let task = tokio::spawn(async move {
                    let result = acquirer.acquire(&call).await;
                    let _ = tx.send(result);
                });
                self.replacement.pending = Some(PendingAcquire {
                    rx,
                    task: task.abort_handle(),
                });
            }
        }
    }

    /// The in-flight Acquire answered. Exactly one outcome per attempt
    /// ([`Self::end_attempt`] is the only recorder).
    pub(super) async fn on_acquire_result(
        &mut self,
        result: Result<AcquireResult, oneshot::error::RecvError>,
    ) {
        // Cleared FIRST: a completed oneshot is never polled again.
        self.replacement.pending = None;
        let Some(in_flight) = self.replacement.state.take_in_flight() else {
            error!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                "An Acquire result arrived with no attempt in flight; MC-internal invariant \
                 violation: capture and escalate"
            );
            return;
        };
        let owed = in_flight.owed;
        let Some(timing) = self.media.deps().map(|d| d.replacement) else {
            // Unreachable: an attempt is issued only with deps installed.
            error!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                "An Acquire result arrived with no routing deps; MC-internal invariant \
                 violation: capture and escalate"
            );
            self.end_attempt(&owed, None, MigrationOutcome::Abandoned, "invariant");
            return;
        };
        let result = result.unwrap_or_else(|_| AcquireResult::Unavailable {
            code: tonic::Code::Unknown,
            message: "the acquire task ended without an answer".to_string(),
        });
        let now = Instant::now();

        if in_flight.version != self.media.handlers().version() {
            // Superseded: the set changed while GC was choosing. Discarded,
            // never registered (no worker, no token, nothing at MH).
            let granted = match &result {
                AcquireResult::Granted(endpoint) => Some(endpoint.id.clone()),
                _ => None,
            };
            self.end_attempt(
                &owed,
                granted.as_ref(),
                MigrationOutcome::Abandoned,
                SUPERSEDED,
            );
            self.replacement.state.retry_superseded(owed, now, timing);
            return;
        }

        match result {
            AcquireResult::Granted(endpoint) => {
                let id = endpoint.id.clone();
                if self.media.handlers().excluded_ids().contains(&id) {
                    error!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        new_handler_id = %id,
                        "GC granted a handler this meeting excluded (GC contract violation); \
                         discarded, never added"
                    );
                    self.end_attempt(
                        &owed,
                        Some(&id),
                        MigrationOutcome::Abandoned,
                        "excluded_grant",
                    );
                    self.replacement
                        .state
                        .retry_after_failure(owed, now, timing);
                    return;
                }
                match self.media.add_granted(endpoint) {
                    Ok(change) => {
                        metrics::record_migration_phase(
                            owed.trigger,
                            MigrationPhase::Replacement,
                            now.saturating_duration_since(owed.declared_at),
                        );
                        info!(
                            target: "mc.actor.meeting",
                            meeting_id = %self.meeting_id,
                            old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
                            new_handler_id = %id,
                            trigger = owed.trigger.label(),
                            handler_set_version = change.version,
                            "Media handler replacement granted; registering it"
                        );
                        if owed.trigger == RemovalTrigger::PlannedTransfer {
                            if let Some(replaced) = &owed.replaced {
                                self.planned.granted(replaced, &id);
                            }
                        }
                        self.replacement.state.granted(id, owed, now, timing);
                        self.publish_handler_censuses();
                        // Spawns its push worker: RegisterMeeting (with the
                        // heartbeat interval and the public dial key) goes out
                        // before any client set can name it.
                        self.reconcile_media(crate::actors::meeting_media::Affected::All)
                            .await;
                    }
                    Err(e) => {
                        error!(
                            target: "mc.actor.meeting",
                            meeting_id = %self.meeting_id,
                            new_handler_id = %id,
                            error = %e,
                            "GC's grant cannot join the meeting's handler set; discarded"
                        );
                        self.end_attempt(
                            &owed,
                            Some(&id),
                            MigrationOutcome::Abandoned,
                            "add_refused",
                        );
                        self.replacement
                            .state
                            .retry_after_failure(owed, now, timing);
                    }
                }
            }
            AcquireResult::NoCapacity { message } => {
                warn!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
                    gc_message = %message,
                    survivors = self.media.handlers().len(),
                    "GC has no replacement capacity; staying on survivors and retrying"
                );
                self.end_attempt(&owed, None, MigrationOutcome::NoCapacity, "no_capacity");
                self.replacement
                    .state
                    .retry_after_failure(owed, now, timing);
            }
            AcquireResult::Unavailable { code, message } => {
                // Every attempt is WARNed with its code: a lasting credential
                // fault (UNAUTHENTICATED / PERMISSION_DENIED) is never silent.
                warn!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
                    code = ?code,
                    gc_message = %message,
                    survivors = self.media.handlers().len(),
                    "GC did not answer the replacement request; staying on survivors and retrying"
                );
                self.end_attempt(
                    &owed,
                    None,
                    MigrationOutcome::GcUnavailable,
                    "gc_unavailable",
                );
                self.replacement
                    .state
                    .retry_after_failure(owed, now, timing);
            }
            AcquireResult::Rejected {
                reason,
                code,
                message,
            } => {
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
                    reason = reason.label(),
                    code = ?code,
                    gc_message = %message,
                    "GC refused the replacement request; not retried"
                );
                self.end_attempt(&owed, None, MigrationOutcome::Abandoned, reason.label());
                self.replacement.state.give_up(now, timing);
            }
        }
    }

    /// The registration side of a migration, after the actor applied the
    /// report (`handle_registration`):
    /// - a grant's first ack completes its attempt (`completed`) and starts
    ///   its reconnect phase;
    /// - a handler — grant OR hand-off — whose FIRST `RegisterMeeting` never
    ///   acked is removed through the ONE removal transaction and replaced
    ///   (ADR-0039 §3), trigger [`registration_failure_trigger`].
    pub(super) async fn after_registration(
        &mut self,
        report: &HandlerRegistration,
        active_changed: bool,
    ) {
        let now = Instant::now();
        match report.outcome {
            RegistrationOutcome::Registered => {
                if !active_changed {
                    return;
                }
                let waiting = self.media.admitted();
                if let Some(granted) =
                    self.replacement
                        .state
                        .on_active(&report.handler_id, now, waiting)
                {
                    metrics::record_migration_phase(
                        granted.owed.trigger,
                        MigrationPhase::Registration,
                        now.saturating_duration_since(granted.granted_at),
                    );
                    // A planned grant's Active records nothing: its
                    // transfer's terminal is recorded at the old handler's
                    // removal (story 4 MC-F, lead ruling 1).
                    if granted.records_on_active() {
                        self.end_attempt(
                            &granted.owed,
                            Some(&report.handler_id),
                            MigrationOutcome::Completed,
                            "registered",
                        );
                    }
                    self.check_reconnects();
                }
            }
            RegistrationOutcome::InitialRegistrationFailed => {
                if self
                    .media
                    .handlers()
                    .is_registering(&report.handler_id, report.handler_ref)
                {
                    self.remove_handler_for(
                        report.handler_id.as_str(),
                        registration_failure_trigger(),
                        StartReason::InitialRegistrationFailed,
                    )
                    .await;
                }
            }
        }
    }

    /// Close every reconnect phase whose waiting participants are all
    /// connected to the new handler (or gone).
    pub(super) fn check_reconnects(&mut self) {
        if !self.replacement.state.has_reconnecting() {
            return;
        }
        let media = &self.media;
        for done in self
            .replacement
            .state
            .check_reconnects(Instant::now(), |h, p| media.reconnected_or_gone(h, p))
        {
            metrics::record_migration_phase(done.trigger, MigrationPhase::Reconnect, done.elapsed);
        }
    }

    /// The meeting is ending: every open attempt is `abandoned`, and so is
    /// every open planned transfer.
    pub(super) fn end_replacement(&mut self) {
        self.end_planned_transfers();
        if let Some(pending) = self.replacement.pending.take() {
            pending.task.abort();
        }
        for owed in self.replacement.state.end() {
            self.end_attempt(&owed, None, MigrationOutcome::Abandoned, "meeting_ended");
        }
    }

    /// THE membership-change hook: re-set BOTH handler censuses for this
    /// meeting from its set — the awaiting-handler census and the MC-wide
    /// `HandlerCensus` (every member, Registering and Active). Called at actor
    /// start, on a grant joining the set and after every removal (the planned
    /// cutover included); freshness only, the controller's walk is the
    /// load-bearing publish.
    pub(super) fn publish_handler_censuses(&self) {
        if let Some(wiring) = &self.wiring {
            let handlers = self.media.handlers();
            wiring
                .handlerless_census
                .set(&self.meeting_id, handlers.is_empty());
            wiring.handlerless_census.publish();
            wiring
                .handler_census
                .set(&self.meeting_id, handlers.ids().cloned().collect());
        }
    }

    /// THE end of one acquisition attempt: counts its ONE outcome and emits
    /// its terminal log line (ids in the log only; bounded fields otherwise).
    /// The only recorder of ATTEMPT outcomes (a planned transfer's terminal is
    /// `end_transfer`'s; the two share the `abandoned` token), so "exactly one
    /// outcome per attempt" holds by construction.
    ///
    /// Also THE one place "the acquisition answered no grant" is derived for
    /// planned transfers (code-review F3): every attempt that ends without a
    /// usable grant — any outcome but `completed`, except a `superseded` grant
    /// (re-asked at once, GC did have capacity) — makes the open transfers'
    /// survivors eligible. `Due::NoCap` (no attempt was made) is the only
    /// other site.
    fn end_attempt(
        &mut self,
        owed: &Owed,
        new_handler: Option<&HandlerId>,
        outcome: MigrationOutcome,
        reason: &'static str,
    ) {
        metrics::record_migration(owed.trigger, outcome);
        if outcome != MigrationOutcome::Completed && reason != SUPERSEDED {
            self.planned.no_grant();
        }
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            old_handler_id = %owed.replaced.as_ref().map_or("", HandlerId::as_str),
            new_handler_id = %new_handler.map_or("", HandlerId::as_str),
            trigger = owed.trigger.label(),
            outcome = outcome.label(),
            reason,
            "Media handler replacement attempt ended"
        );
    }
}
