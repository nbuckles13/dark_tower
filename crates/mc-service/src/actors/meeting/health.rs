//! The meeting actor's health driver and removal transaction (ADR-0039 §2,
//! §6; story 4 MC-B2).
//!
//! The DECISION is pure (`media_routing::handler_health::assess`); this module
//! only feeds it, acts on its transitions, and runs the one removal
//! transaction a Dead/Impaired declaration triggers.

use super::replacement::StartReason;
use super::MeetingActor;
use crate::actors::meeting_media::{Affected, HealthStep};
use crate::media_routing::{
    client_disagreement_began, ClientLoss, HandlerHealth, HandlerId, RemovalTrigger,
    SuspicionOutcome,
};
use crate::observability::metrics;
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

impl MeetingActor {
    /// The health deadline arm.
    ///
    /// **Stall guard.** A heartbeat stamped (at gRPC arrival) before the
    /// deadline may still be sitting in this actor's mailbox. So the arm FIRST
    /// applies the backlog already queued when it fired — a snapshot of the
    /// mailbox length, through the ONE dispatch path — and only then judges.
    /// Bounded by construction: the snapshot is at most the mailbox capacity,
    /// and traffic arriving after it is not waited on, so a busy meeting can
    /// never postpone the judgment (security S-9).
    ///
    /// If a drained message ends the meeting (or the channel closes) the drain
    /// stops there and NOTHING is assessed: no removal may start on a meeting
    /// being torn down. The loop's own top-of-iteration check then leaves
    /// through the single clean exit path.
    pub(super) async fn on_health_deadline(&mut self) {
        let backlog = self.receiver.len();
        for _ in 0..backlog {
            if self.end_cause.is_some() {
                return;
            }
            match self.receiver.try_recv() {
                Ok(message) => self.dispatch(message).await,
                // Empty: nothing more was queued. Closed: the main loop's
                // `recv` sees it next and exits.
                Err(_) => break,
            }
        }
        if self.end_cause.is_some() || self.cancel_token.is_cancelled() {
            return;
        }
        let now = Instant::now();
        let members: Vec<HandlerId> = self.media.handlers().ids().cloned().collect();
        for member in members {
            if let Some(step) = self.assess_member(&member, now) {
                self.act_on_health(step).await;
            }
        }
    }

    /// THE one per-member assessment (heartbeat path AND deadline arm): build
    /// a FRESH client-loss snapshot at `now` and assess. The snapshot is the
    /// one `assess` judges and, on a declaration, the one the required loss
    /// metric records — never one cached from an earlier assessment.
    fn assess_member(&mut self, member: &HandlerId, now: Instant) -> Option<HealthStep> {
        let client_loss = self.client_loss(member);
        self.media.assess_handler(member, now, client_loss)
    }

    /// The client-loss snapshot for one member (story 4 MC-E; ADR-0039 §8):
    /// over the participants MC BELIEVES connected to it (MH-reported
    /// connectivity), whose LATEST status for its ref is a loss received
    /// while the ref was in the set (MC-D's store: a later CONNECTED
    /// supersedes, a report after removal never entered it, the version is
    /// correlation-only), stamped at MC receipt. Attribution is by
    /// `handler_ref`, never by URL. A Registering member has no client ref,
    /// so nobody can have reported losing it.
    fn client_loss(&self, member: &HandlerId) -> ClientLoss {
        let handler_ref = self.media.handlers().client_handler(member);
        ClientLoss::tally(self.media.believed_connected(member).map(|participant| {
            handler_ref.and_then(|r| self.client_statuses.loss_since(participant, r))
        }))
    }

    /// Assess one member after its heartbeat was observed, act on the
    /// transition, and — while it is Lagging after an ACCEPTED heartbeat —
    /// re-publish its current snapshot (ADR-0039 §2: replaces the re-assert's
    /// self-correction on a live MH; at most once per heartbeat, bounded by L).
    pub(super) async fn assess_after_heartbeat(&mut self, member: &HandlerId, accepted: bool) {
        let Some(step) = self.assess_member(member, Instant::now()) else {
            return;
        };
        if accepted && step.next.is_lagging() {
            self.media.republish(member);
        }
        let draining = accepted && step.next.is_draining();
        self.act_on_health(step).await;
        // Story 4 MC-F: an ACCEPTED heartbeat (never a stale one) from a
        // current member (the caller passed `heartbeat_member`) that reports
        // DRAINING starts its planned transfer, or only tightens the
        // deadline of the one already open. `is_draining` is false on a
        // declaration, so a member just removed above never starts one.
        if draining {
            self.start_planned_transfer(member);
        }
    }

    /// Act on ONE assessment: log a transition once, count a suspicion
    /// outcome, and run the removal on a Dead/Impaired declaration. A
    /// declared handler leaves the health map in the same turn (removal), so
    /// a second assessment can never count it twice.
    async fn act_on_health(&mut self, step: HealthStep) {
        if !HandlerHealth::is_transition(&step.previous, &step.next) {
            return;
        }
        self.log_health_transition(&step);
        if let Some(outcome) = SuspicionOutcome::of(&step.previous, &step.next) {
            metrics::record_handler_suspicion(outcome);
        }
        // The client second opinion: counted once per episode, never acted on.
        if client_disagreement_began(&step.previous, &step.next) {
            metrics::record_client_loss_disagreement();
        }
        if let (None, Some(declaration)) = (step.previous.declaration(), step.next.declaration()) {
            let trigger = declaration.trigger();
            let now = Instant::now();
            let detection = declaration.detection(now);
            if self
                .declare_and_remove(&step.handler_id, trigger, StartReason::Declared, detection)
                .await
            {
                record_loss_observation(step.observations.client_loss, trigger, now);
            }
        }
    }

    /// THE declaration recorder and removal (both declaring paths: this
    /// module's Dead/Impaired and the planned cutover). In order:
    ///
    /// 1. claim the handler on the MC-wide census BEFORE the removal writes
    ///    the set without it — the order `HandlerCensus` dedupes on (see its
    ///    doc; reordering under-counts nothing but double-counts a handler
    ///    several meetings declare);
    /// 2. run the removal transaction;
    /// 3. only if it removed the handler, count
    ///    `mc_media_handler_declarations_total{trigger}` + detection and, for
    ///    the first claim, `mc_media_handler_first_declarations_total{trigger}`.
    ///    A refused removal (an MC invariant violation, logged at ERROR by
    ///    `remove_handler`) removed nothing an operator could act on, and
    ///    RELEASES its census claim so later declarations still count.
    pub(super) async fn declare_and_remove(
        &mut self,
        member: &HandlerId,
        trigger: RemovalTrigger,
        reason: StartReason,
        detection: Duration,
    ) -> bool {
        let first = self
            .wiring
            .as_ref()
            .is_some_and(|w| w.handler_census.first_declaration(member));
        if !self
            .remove_handler_for(member.as_str(), trigger, reason)
            .await
        {
            // Nothing is counted for a refused removal, so the claim must not
            // outlive it (the handler stays in this set, so no prune would).
            if first {
                if let Some(w) = &self.wiring {
                    w.handler_census.release_claim(member);
                }
            }
            return false;
        }
        metrics::record_handler_declaration(trigger, detection);
        if first {
            metrics::record_first_handler_declaration(trigger);
        }
        true
    }

    /// One INFO per health transition (never per heartbeat): ids in the log
    /// line only, signals as bounded scalars. No Debug of any struct, no url,
    /// no connection id, no token.
    fn log_health_transition(&self, step: &HealthStep) {
        let o = &step.observations;
        let now = Instant::now();
        let declaration = step.next.declaration();
        let trigger = declaration.map_or("", |d| d.trigger().label());
        let cause = declaration.map_or("", |d| d.cause.label());
        let detection_ms = declaration.map_or(0, |d| duration_ms(d.detection(now)));
        let refusal = match step.next {
            HandlerHealth::Live(l) => l.contract_fault.map_or("", |f| f.label()),
            HandlerHealth::Declared(_) => "",
        };
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            handler_id = %step.handler_id,
            from = step.previous.state_label(),
            to = step.next.state_label(),
            trigger,
            cause,
            detection_ms,
            serving_state = if o.draining { "DRAINING" } else { "SERVING" },
            client_disagreement = step.next.is_client_disagreement(),
            clients_connected = o.client_loss.connected(),
            clients_reporting_loss = o.client_loss.reported(),
            apply_refusal = o.refusal.label(),
            lagging = step.next.is_lagging(),
            draining = step.next.is_draining(),
            contract_fault = refusal,
            age_ms = o
                .last_accepted
                .or(o.confirmed_at)
                .map_or(0, |at| duration_ms(now.saturating_duration_since(at))),
            applied_generation = o.applied.unwrap_or(0),
            owed_generation = o.owed.unwrap_or(0),
            "Media handler health changed"
        );
    }

    /// THE removal transaction (ADR-0039 §6 "MC: forget it"). The ORDER is the
    /// security property; do not reorder:
    ///
    /// 1. drop the handler from the set and write the removed record (version
    ///    bump) — from here the verdict is RELEASED(trigger), the binding gate
    ///    refuses it and no mint can name it — together with its liveness,
    ///    health and push worker, and purge its keys from every participant's
    ///    connectivity (`MeetingMedia::remove_handler`);
    /// 2. re-render through the normal path, so edges re-place on surviving
    ///    shared handlers with no special case (nothing is published to the
    ///    removed handler: its worker is already out);
    /// 3. push the set without it to every client (typed path, tokens minted
    ///    from the post-removal snapshot), then flush the views;
    /// 4. only then bulk-release its handed-out bindings;
    /// 5. drain its worker and send ONE `EndMeeting(trigger)`, in a task the
    ///    meeting's teardown awaits (never in this actor; never retried — the
    ///    next heartbeat's RELEASED is the retry), which then evicts the
    ///    pair's generations;
    /// 6. only then feed the removal to the replacement driver
    ///    (`meeting/replacement.rs`), which may ask GC for a replacement —
    ///    removal never waits on GC (ADR-0039 §3).
    ///
    /// Returns whether it was a member.
    pub(super) async fn remove_handler_completely(
        &mut self,
        wire_handler_id: &str,
        trigger: RemovalTrigger,
    ) -> bool {
        self.remove_handler_for(wire_handler_id, trigger, StartReason::Declared)
            .await
    }

    /// [`Self::remove_handler_completely`] with the replacement's start reason
    /// (a declaration, or a first `RegisterMeeting` that never acked). Step 6
    /// of the transaction: the removal is fed to the replacement driver, which
    /// asks GC for a replacement if the set is now short — AFTER the removal
    /// completed, so removal never waits on GC (ADR-0039 §3).
    pub(super) async fn remove_handler_for(
        &mut self,
        wire_handler_id: &str,
        trigger: RemovalTrigger,
        reason: StartReason,
    ) -> bool {
        let Some(removed) = self
            .media
            .remove_handler(wire_handler_id, trigger, Instant::now())
        else {
            return false;
        };
        self.reconcile_media(Affected::All).await;
        if removed.set_change.active_changed {
            self.broadcast_handler_set().await;
        }
        self.flush_views().await;
        let released = self.mh_bindings.release_handler(wire_handler_id);
        // An open planned transfer OF this handler (removed by any other
        // path) ends `abandoned`; none keeps it as its grant. A planned
        // cutover took its own entry out first, so it is not seen here.
        self.planned_after_removal(&HandlerId::new(wire_handler_id));
        self.media
            .spawn_removal_release(removed.endpoint, trigger, removed.pusher);
        let survivors = self.media.handlers().len();
        self.publish_handler_censuses();
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            handler_id = %wire_handler_id,
            trigger = trigger.label(),
            handler_set_version = removed.set_change.version,
            survivors,
            connections_purged = removed.forgotten_keys,
            bindings_released = released.len(),
            "Media handler removed from the meeting"
        );
        self.after_handler_removed(&HandlerId::new(wire_handler_id), trigger, reason);
        true
    }
}

/// The required loss metric (story 4 MC-E; ADR-0039 §8), from the ONE
/// snapshot the declaring assessment judged. Only `heartbeat_timeout` and
/// `apply_lag` observe: a planned transfer never reaches `act_on_health` (it
/// removes through `cut_over`), and this match keeps the exclusion visible in
/// one place should that ever change.
fn record_loss_observation(loss: ClientLoss, trigger: RemovalTrigger, now: Instant) {
    match trigger {
        RemovalTrigger::HeartbeatTimeout | RemovalTrigger::ApplyLag => {
            metrics::record_handler_loss(
                loss.reported(),
                loss.not_reported(),
                loss.ratio(),
                loss.lead(now),
            );
        }
        RemovalTrigger::PlannedTransfer => {}
    }
}

/// Milliseconds of `d`, saturating (log fields; shared with the planned
/// transfer driver).
pub(super) fn duration_ms(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}
