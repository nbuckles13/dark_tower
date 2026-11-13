//! The meeting actor's planned-transfer driver (ADR-0039 §4/§7; story 4 MC-F).
//!
//! The DECISIONS live in `media_routing::planned_transfer` (pure). This module
//! only performs them, reusing MC-C's machinery unchanged:
//! - the acquisition is MC-C's ONE path (`ReplacementState::on_draining` owes
//!   it; `on_replacement_due` / `on_acquire_result` issue and classify it);
//! - the removal is THE removal transaction (`remove_handler_for`, trigger
//!   `planned_transfer`): set without the old handler to clients FIRST, then
//!   the binding release, then — in a spawned task, never on this actor — the
//!   old handler's worker drain and ONE `EndMeeting(PLANNED_TRANSFER)`;
//! - the cutover deadline is a `select!` arm built per iteration; the
//!   coverage check is ONE post-turn hook ([`MeetingActor::check_planned_transfers`]).
//!
//! Make-before-break: the draining handler stays in the set, keeps its push
//! worker and keeps routing until it is removed; edges move at that removal
//! (ADR-0036 §9 edge stability), so the blip is one push, directive change and
//! keyframe.

use super::health::duration_ms as ms;
use super::replacement::StartReason;
use super::MeetingActor;
use crate::actors::meeting_media::Coverage;
use crate::media_routing::planned_transfer::{cutover_at, Started, Transfer};
use crate::media_routing::{HandlerId, MigrationOutcome, RemovalTrigger};
use crate::observability::metrics;
use tokio::time::Instant;
use tracing::{error, info};

/// Which target a planned transfer's removal landed on (log field only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    /// The planned grant (participants were on it).
    Grant,
    /// Surviving non-draining handlers (no grant, or the deadline rule).
    Survivor,
    /// No eligible target at the deadline: the meeting's sessions are shed.
    None,
}

impl TargetKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Survivor => "survivor",
            Self::None => "none",
        }
    }
}

/// How a transfer ended (log detail for [`MeetingActor::end_transfer`]).
#[derive(Debug, Clone, Copy)]
enum TransferEnd {
    /// The transfer removed its handler (cutover).
    Removed {
        target: TargetKind,
        /// The deadline fired (`cutover="deadline"`), or the target covered
        /// everyone first (`cutover="covered"`).
        at_deadline: bool,
        coverage: Coverage,
    },
    /// It ended without removing it; the bounded reason.
    Abandoned(&'static str),
}

impl MeetingActor {
    /// An ACCEPTED heartbeat from current member `id` reported DRAINING (the
    /// caller's gate: `heartbeat_member` — owner check + current member — then
    /// `Acceptance::Accepted`, then `assess` saying draining; a stale or
    /// non-member heartbeat never reaches here). The first one starts the
    /// transfer and owes its acquisition; every later one only tightens the
    /// deadline. A SERVING flip-back is ignored (`draining_since` is sticky):
    /// a started transfer runs to its end.
    pub(super) fn start_planned_transfer(&mut self, id: &HandlerId) {
        let Some((planned, replacement)) = self
            .media
            .deps()
            .map(|d| (d.planned_transfer, d.replacement))
        else {
            return;
        };
        // Only a beat that ITSELF reports DRAINING carries a deadline. The
        // health state is sticky, so after a (contract-violating) SERVING
        // flip-back `assess` still says draining — but that beat's
        // `drain_remaining_ms` is 0 by contract and must never tighten the
        // deadline to "now".
        let Some((receipt, remaining_ms)) = self
            .media
            .liveness(id.as_str())
            .filter(|l| crate::media_routing::handler_health::is_draining(l.serving_state))
            .map(|l| (l.last_accepted, l.drain_remaining_ms))
        else {
            return;
        };
        let cutover = cutover_at(
            receipt,
            remaining_ms,
            planned,
            replacement.max_planned_hold(),
        );
        if let Started::New(at) = self.planned.start(id, receipt, cutover) {
            let now = Instant::now();
            let deferred = self.replacement.on_draining(id, receipt, now, replacement);
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %id,
                trigger = RemovalTrigger::PlannedTransfer.label(),
                drain_remaining_ms = remaining_ms,
                margin_ms = ms(planned.margin()),
                cutover_in_ms = ms(at.saturating_duration_since(now)),
                acquire_deferred = deferred,
                "Planned handler transfer started"
            );
        }
    }

    /// THE post-turn hook (called once per loop iteration, before the wakes
    /// are computed): cut over every transfer whose deadline passed, then
    /// every transfer whose eligible target now covers every routable
    /// participant. Each cutover consumes its entry, so nothing re-fires.
    /// Which loop fires never decides the outcome (see [`Self::cut_over`]).
    pub(super) async fn check_planned_transfers(&mut self) {
        if self.planned.is_empty() || self.end_cause.is_some() {
            return;
        }
        for id in self.planned.due(Instant::now()) {
            self.cut_over(&id, true).await;
        }
        let open: Vec<HandlerId> = self.planned.ids().cloned().collect();
        for id in open {
            let candidates = self.media.non_draining_active();
            let eligible = self.planned.eligible(&id, &candidates, false);
            if !eligible.is_empty() && self.media.coverage(&eligible).is_complete() {
                self.cut_over(&id, false).await;
            }
        }
    }

    /// Remove draining `id` through THE removal transaction and record its
    /// transfer's ONE terminal outcome, decided by the OBSERVED coverage at
    /// removal, never by which loop fired (observability ruling, Gate 3):
    /// - `completed`: the eligible target is non-empty and every routable
    ///   participant is MH-reported Connected to it — also when that happens
    ///   to coincide with the deadline (e.g. GC never answered but everyone
    ///   was already on a survivor);
    /// - `timed_out`: cut over at the deadline with stragglers, or shed (no
    ///   eligible target at all: neither a survivor nor a grant) — the
    ///   drain-shed signal.
    ///
    /// `at_deadline` selects only the eligibility rule (the deadline adds the
    /// survivors) and the log's `cutover` field.
    async fn cut_over(&mut self, id: &HandlerId, at_deadline: bool) {
        let candidates = self.media.non_draining_active();
        let eligible = self.planned.eligible(id, &candidates, at_deadline);
        let coverage = self.media.coverage(&eligible);
        // Taken out BEFORE the removal: the transaction's abandon hook then
        // finds nothing, so exactly one terminal is recorded — here.
        let Some(transfer) = self.planned.finish(id) else {
            return;
        };
        let target = match &transfer.grant {
            Some(grant)
                if eligible.contains(grant)
                    && self
                        .media
                        .coverage(std::slice::from_ref(grant))
                        .is_complete() =>
            {
                TargetKind::Grant
            }
            _ if eligible.is_empty() => TargetKind::None,
            _ => TargetKind::Survivor,
        };
        // Detection for a planned transfer: transfer start -> the cutover
        // decision (measured before the removal transaction runs).
        let detection = Instant::now().saturating_duration_since(transfer.started_at);
        let removed = self
            .declare_and_remove(
                id,
                RemovalTrigger::PlannedTransfer,
                StartReason::PlannedTransfer,
                detection,
            )
            .await;
        if !removed {
            error!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %id,
                "A planned transfer's handler was no longer a member at cutover; MC-internal \
                 invariant violation: capture and escalate"
            );
            self.end_transfer(
                id,
                &transfer,
                MigrationOutcome::Abandoned,
                TransferEnd::Abandoned("not_a_member"),
            );
            return;
        }
        let outcome = if !eligible.is_empty() && coverage.is_complete() {
            MigrationOutcome::Completed
        } else {
            MigrationOutcome::TimedOut
        };
        self.end_transfer(
            id,
            &transfer,
            outcome,
            TransferEnd::Removed {
                target,
                at_deadline,
                coverage,
            },
        );
    }

    /// A member left through ANY removal (the transaction calls this): an open
    /// transfer OF it ends `abandoned` (it was declared Dead/Impaired, or its
    /// first registration failed, before its cutover — the crash path owns the
    /// rest), and no transfer keeps it as its grant.
    pub(super) fn planned_after_removal(&mut self, removed: &HandlerId) {
        self.planned.grant_removed(removed);
        if let Some(transfer) = self.planned.finish(removed) {
            self.end_transfer(
                removed,
                &transfer,
                MigrationOutcome::Abandoned,
                TransferEnd::Abandoned("handler_removed"),
            );
        }
    }

    /// The meeting is ending: every open transfer ends `abandoned`.
    pub(super) fn end_planned_transfers(&mut self) {
        for (id, transfer) in self.planned.end() {
            self.end_transfer(
                &id,
                &transfer,
                MigrationOutcome::Abandoned,
                TransferEnd::Abandoned("meeting_ended"),
            );
        }
    }

    /// THE recorder of a planned transfer's TERMINAL (`completed`, `timed_out`
    /// or a transfer-level `abandoned`). Every caller first took the entry out
    /// with `PlannedTransfers::finish` (by value, once), so each transfer
    /// records exactly one terminal. NOT the only writer of
    /// `migrations_total{trigger="planned_transfer"}`: `end_attempt` records
    /// the planned owe's attempt outcomes, and `abandoned` is SHARED with its
    /// attempt-level discards (superseded, refused, unusable grant) — hence the
    /// invariant `completed + timed_out == declarations{planned_transfer}`.
    ///
    /// Ids in the log only. `new_handler_id` only when the target was the
    /// grant; `target` (`grant|survivor|none`) and `cutover`
    /// (`covered|deadline`) only on a removal, so `target="none"` means a shed
    /// and nothing else. The outcome token is the metric label's.
    fn end_transfer(
        &self,
        id: &HandlerId,
        transfer: &Transfer,
        outcome: MigrationOutcome,
        how: TransferEnd,
    ) {
        metrics::record_migration(RemovalTrigger::PlannedTransfer, outcome);
        let elapsed_ms = ms(Instant::now().saturating_duration_since(transfer.started_at));
        let trigger = RemovalTrigger::PlannedTransfer.label();
        match how {
            TransferEnd::Abandoned(reason) => info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                old_handler_id = %id,
                trigger,
                outcome = outcome.label(),
                reason,
                elapsed_ms,
                "Planned handler transfer ended"
            ),
            TransferEnd::Removed {
                target,
                at_deadline,
                coverage,
            } => {
                let cutover = if at_deadline { "deadline" } else { "covered" };
                match (target, &transfer.grant) {
                    (TargetKind::Grant, Some(grant)) => info!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        old_handler_id = %id,
                        new_handler_id = %grant,
                        trigger,
                        outcome = outcome.label(),
                        target = target.label(),
                        cutover,
                        routable = coverage.routable,
                        stragglers = coverage.stragglers,
                        elapsed_ms,
                        "Planned handler transfer ended"
                    ),
                    _ => info!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        old_handler_id = %id,
                        trigger,
                        outcome = outcome.label(),
                        target = target.label(),
                        cutover,
                        routable = coverage.routable,
                        stragglers = coverage.stragglers,
                        elapsed_ms,
                        "Planned handler transfer ended"
                    ),
                }
            }
        }
    }
}
