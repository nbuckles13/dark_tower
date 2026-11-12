//! The meeting actor's health driver and removal transaction (ADR-0039 §2,
//! §6; story 4 MC-B2).
//!
//! The DECISION is pure (`media_routing::handler_health::assess`); this module
//! only feeds it, acts on its transitions, and runs the one removal
//! transaction a Dead/Impaired declaration triggers.

use super::replacement::StartReason;
use super::MeetingActor;
use crate::actors::meeting_media::{Affected, HealthStep};
use crate::media_routing::{HandlerHealth, HandlerId, RemovalTrigger, SuspicionOutcome};
use crate::observability::metrics;
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
        for step in self.media.assess_all(Instant::now()) {
            self.act_on_health(step).await;
        }
    }

    /// Assess one member after its heartbeat was observed, act on the
    /// transition, and — while it is Lagging after an ACCEPTED heartbeat —
    /// re-publish its current snapshot (ADR-0039 §2: replaces the re-assert's
    /// self-correction on a live MH; at most once per heartbeat, bounded by L).
    pub(super) async fn assess_after_heartbeat(&mut self, member: &HandlerId, accepted: bool) {
        let Some(step) = self.media.assess_handler(member, Instant::now()) else {
            return;
        };
        if accepted && step.next.is_lagging() {
            self.media.republish(member);
        }
        self.act_on_health(step).await;
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
        if let (None, Some(declaration)) = (step.previous.declaration(), step.next.declaration()) {
            let trigger = declaration.trigger();
            let detection = declaration.detection(Instant::now());
            // Counted only when the removal happened: a refused removal (an MC
            // invariant violation, logged at ERROR by `remove_handler`)
            // removed nothing an operator could act on.
            if self
                .remove_handler_completely(step.handler_id.as_str(), trigger)
                .await
            {
                metrics::record_handler_declaration(trigger, detection);
            }
        }
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
        self.media
            .spawn_removal_release(removed.endpoint, trigger, removed.pusher);
        let survivors = self.media.handlers().len();
        self.publish_handlerless();
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

fn duration_ms(d: std::time::Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}
