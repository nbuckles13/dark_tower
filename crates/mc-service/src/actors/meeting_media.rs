//! The meeting actor's media state: join-order slots, handler placement, the
//! per-handler push workers, and every declared participant's last-emitted view
//! (ADR-0036 §5, §6, §8, §9; story 2 R-1..R-4, R-33).
//!
//! # The actor is the single writer AND the single composer
//!
//! Membership, join order, slot demand and mute state all live on the meeting
//! actor, so the actor is the only place that can know when a participant's
//! view changed — including when the cause is SOMEONE ELSE's join, leave,
//! declaration or mute, for which that participant's own connection never runs
//! any code. After every structural change the actor:
//!
//! 1. renders the slot table into a per-handler forwarding snapshot;
//! 2. takes each handler's `policy_generation` (advances only when that
//!    handler's snapshot changed) and publishes it to that handler's push
//!    worker, which pushes and confirms MH's applied-generation echo; and
//! 3. marks the affected declared participants dirty and flushes their views.
//!
//! # The flush bound: per turn, deferring, never shedding
//!
//! A roster change in an N-party meeting can change N views, so filling a
//! meeting is O(N²) emissions and none of it is client-rate-limited. The actor
//! therefore takes at most [`SLOT_VIEW_FLUSH_BATCH`] dirty participants per
//! actor turn and DEFERS the rest to later turns, interleaved with mailbox
//! traffic. Nothing is dropped: a stale view means hearing the wrong person or
//! nobody, and the client has no message it could send to fix it. The dirty
//! structure is a SET keyed by participant with idempotent marking, so the
//! carried backlog is bounded by declared participants, never by event count,
//! and a burst of changes coalesces into one emission per participant carrying
//! the latest state. The retained view state is O(participants × declared
//! slots), bounded by the roster. See `media_signaling`'s module doc for when
//! this kind of bound is the right one.
//!
//! One accepted DISTINCT re-declaration costs a render, one `next_generation`
//! per handler, one publish per handler (coalesced by the worker's latest-wins
//! channel, so a re-declaration loop cannot drive a per-declaration MH RPC
//! rate) and a dirty-flush over declared participants — bounded upstream by the
//! per-connection declaration budget.
//!
//! # Only what changed, and only to declared participants
//!
//! Each declared participant's last emitted `SendDirective` and
//! `StreamAssignments` are retained; a flush emits a message only if it differs
//! (directive first). A participant that has not declared is sent nothing: the
//! declaration is the client's precondition for publishing, and an empty list
//! would read as "declared zero slots".
//!
//! # The frozen handler set is the meeting's authority
//!
//! The handler set — ids AND urls, as ONE [`MeetingHandlers`] value — is
//! frozen at the first join. No later join, and no changed or tampered Redis
//! entry, can add a handler to a live meeting or move an already-placed
//! participant: a join carrying a different set is logged at ERROR, counted on
//! `mc_media_handler_set_divergence_total`, and otherwise ignored. That is what
//! stops a Redis-state change from redirecting existing participants' media to
//! a handler MC never vetted.
//!
//! # No key material
//!
//! Sender ids, slot ids, handler ids and urls only. Nothing here reads the
//! meeting KEK; it stays in `MeetingKeyState`.

use super::messages::SignalingPayload;
use super::participant::ParticipantActorHandle;
use crate::errors::McError;
use crate::grpc::MhRegistrationClient;
use crate::media_admission::SenderId;
use crate::media_routing::{
    HandlerEndpoint, HandlerId, HandlerPusher, MeetingAssignment, MeetingHandlers,
    PolicyGenerations, PushJob, PushTarget, SlotTable,
};
use crate::media_signaling::{
    build_send_directive, build_stream_assignments, DirectiveOutcome, MediaStreamPolicy,
    ReceiveCapabilityDeclaration, SlotViewEmission, SourceMuteView,
};
use crate::observability::metrics;
use proto_gen::dark_tower::signaling::v1::{
    server_message, SendDirective, ServerMessage, StreamAssignments,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Dirty participants the actor flushes per actor turn.
///
/// A server work bound, not an operator lever: it moves latency only, never
/// correctness (deferred views are flushed on later turns, coalesced), so it is
/// a Rust constant in the register of `webtransport::connection`'s
/// `MUTE_WORK_BURST` rather than a ConfigMap key. Sized so a meeting at the
/// organisation participant cap flushes in two turns. Why this bound defers
/// where the other two deny or shed: `media_signaling` §"Bounding work on the
/// shared meeting actor".
pub const SLOT_VIEW_FLUSH_BATCH: usize = 64;

/// The server-level dependencies media routing needs, built once by the
/// WebTransport server and shared by every meeting.
pub struct MediaRoutingDeps {
    /// MC→MH registration client.
    pub mh_client: Arc<dyn MhRegistrationClient>,
    /// Process-wide `policy_generation` registry (evicted on meeting teardown).
    pub policy_generations: Arc<PolicyGenerations>,
    /// This MC's id.
    pub mc_id: String,
    /// This MC's advertised gRPC endpoint, for MH→MC callbacks.
    pub mc_grpc_endpoint: String,
    /// The stream-number to media-kind/encoding table MC directs from.
    pub stream_policy: MediaStreamPolicy,
}

impl std::fmt::Debug for MediaRoutingDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaRoutingDeps")
            .field("mc_id", &self.mc_id)
            .field("mc_grpc_endpoint", &self.mc_grpc_endpoint)
            .finish_non_exhaustive()
    }
}

/// What a join carries for media routing: the server-level dependencies and
/// the meeting's handler set as the registration (Redis) describes it at this
/// join. The actor freezes the set at the first join.
#[derive(Debug, Clone)]
pub struct JoinMedia {
    /// Server-level dependencies.
    pub deps: Arc<MediaRoutingDeps>,
    /// The meeting's handler set, as read for this join.
    pub handlers: MeetingHandlers,
}

/// Test-only construction overrides for a meeting actor. **Test builds only.**
///
/// Compiled only under the non-default `test-seams` feature, which fails to
/// compile without `debug_assertions` (see `lib.rs`). The TYPE is gated, not
/// its fields, so a default build has nothing here to extend.
#[cfg(feature = "test-seams")]
#[derive(Debug, Clone, Default)]
pub struct MeetingSeams {
    /// Pre-seed the `sender_id` allocator cursor (`Some`) — the exhaustion
    /// guard's bypass. `None` means a fresh allocator.
    pub sender_id_cursor: Option<Option<std::num::NonZeroU16>>,
    /// Pin participants, by token `sub`, to a named handler of the meeting's
    /// own set. A pin naming a handler outside the set FAILS the join; there is
    /// no fallback to round-robin. Takes an id, never a url, host or port.
    pub placement_pins: BTreeMap<String, HandlerId>,
    /// Override [`SLOT_VIEW_FLUSH_BATCH`] (e.g. 1, to observe deferral).
    pub slot_view_flush_batch: Option<usize>,
}

/// Which declared participants a structural change may have affected.
pub(super) enum Affected {
    /// Every declared participant (join, leave, declaration, reconnect).
    All,
    /// The subscribers holding this source (its mute changed).
    HoldersOf(SenderId),
}

/// One roster participant, as a flush needs it.
pub(super) struct RosterEntry<'a> {
    /// The participant's sender id.
    pub sender: SenderId,
    /// Its live connection, if connected.
    pub connection: Option<&'a ParticipantActorHandle>,
    /// Audio-muted for ANY reason (self or server): one wire state covers both.
    pub audio_muted: bool,
}

/// One participant's declaration and last-emitted view.
struct View {
    sender: SenderId,
    declaration: Option<ReceiveCapabilityDeclaration>,
    last_directive: Option<SendDirective>,
    last_assignments: Option<StreamAssignments>,
}

/// The meeting actor's media state.
pub(super) struct MeetingMedia {
    meeting_id: String,
    /// The meeting's cancellation token: push workers live as long as the
    /// MEETING, never a connection.
    cancel: CancellationToken,
    deps: Option<Arc<MediaRoutingDeps>>,
    /// Frozen at the first join.
    handlers: Option<MeetingHandlers>,
    slots: SlotTable,
    pushers: BTreeMap<HandlerId, HandlerPusher>,
    views: HashMap<String, View>,
    dirty: BTreeSet<String>,
    flush_batch: usize,
    /// The latest render, shared by the push and the flush of one change.
    assignment: Option<MeetingAssignment>,
    #[cfg(feature = "test-seams")]
    placement_pins: BTreeMap<String, HandlerId>,
}

impl MeetingMedia {
    pub(super) fn new(meeting_id: String, cancel: CancellationToken) -> Self {
        Self {
            meeting_id,
            cancel,
            deps: None,
            handlers: None,
            slots: SlotTable::new(),
            pushers: BTreeMap::new(),
            views: HashMap::new(),
            dirty: BTreeSet::new(),
            flush_batch: SLOT_VIEW_FLUSH_BATCH,
            assignment: None,
            #[cfg(feature = "test-seams")]
            placement_pins: BTreeMap::new(),
        }
    }

    #[cfg(feature = "test-seams")]
    pub(super) fn apply_seams(&mut self, seams: &MeetingSeams) {
        self.placement_pins = seams.placement_pins.clone();
        if let Some(batch) = seams.slot_view_flush_batch {
            self.flush_batch = batch.max(1);
        }
    }

    /// Install the join's routing inputs. The first join freezes the handler
    /// set; a later join carrying a different one is loud and ignored.
    pub(super) fn install(&mut self, media: JoinMedia) {
        if self.deps.is_none() {
            self.deps = Some(media.deps);
        }
        match &self.handlers {
            None => self.handlers = Some(media.handlers),
            Some(frozen) if *frozen == media.handlers => {}
            Some(_) => {
                metrics::record_handler_set_divergence();
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    "A join carried a handler assignment that differs from this meeting's frozen \
                     handler set; keeping the frozen set. No join can widen a live meeting's set \
                     or move a placed participant. This is an MC-internal invariant violation: \
                     capture and escalate."
                );
            }
        }
    }

    /// Place and admit a participant. Returns the handler it was placed on.
    ///
    /// # Errors
    ///
    /// [`McError::MhAssignmentMissing`] if no handler set is installed;
    /// [`McError::Internal`] if placement fails (including a test-seam pin
    /// outside the meeting's set) or the slot table refuses the admission.
    pub(super) fn admit(
        &mut self,
        participant_id: &str,
        user_id: &str,
        sender: SenderId,
    ) -> Result<HandlerEndpoint, McError> {
        let Some(handlers) = &self.handlers else {
            return Err(McError::MhAssignmentMissing(
                "no handler set installed for this meeting".to_string(),
            ));
        };

        #[cfg(feature = "test-seams")]
        let pinned: Option<HandlerId> = match self.placement_pins.get(user_id) {
            Some(pin) if handlers.get(pin).is_some() => Some(pin.clone()),
            Some(_) => {
                return Err(McError::Internal(
                    "placement pin names a handler outside the meeting's set".to_string(),
                ))
            }
            None => None,
        };
        #[cfg(not(feature = "test-seams"))]
        let pinned: Option<HandlerId> = {
            let _ = user_id;
            None
        };

        let (_, handler) = self
            .slots
            .admit(sender, |rank| {
                pinned.or_else(|| handlers.place(rank).map(|h| h.id.clone()))
            })
            .map_err(|e| McError::Internal(format!("slot admission failed: {e}")))?;
        let endpoint = handlers
            .get(&handler)
            .cloned()
            .ok_or_else(|| McError::Internal("placed handler missing from the set".to_string()))?;

        self.views.insert(
            participant_id.to_string(),
            View {
                sender,
                declaration: None,
                last_directive: None,
                last_assignments: None,
            },
        );

        // "Which handler did this participant land on" — the operator's first
        // question when someone hears only part of the roster. Participant id
        // and handler id only: no rank, no sender id.
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            mh_id = %endpoint.id,
            "Participant placed on media handler"
        );
        Ok(endpoint)
    }

    /// Remove a participant from slot state and view tracking.
    pub(super) fn remove(&mut self, participant_id: &str) {
        if let Some(view) = self.views.remove(participant_id) {
            self.slots.remove(view.sender);
        }
        self.dirty.remove(participant_id);
    }

    /// Register a validated receive-capability declaration.
    ///
    /// # Errors
    ///
    /// [`McError::ParticipantNotFound`] if the participant is not tracked.
    pub(super) fn declare(
        &mut self,
        participant_id: &str,
        declaration: ReceiveCapabilityDeclaration,
    ) -> Result<(), McError> {
        let Some(view) = self.views.get_mut(participant_id) else {
            return Err(McError::ParticipantNotFound(
                "Participant not found".to_string(),
            ));
        };
        self.slots
            .set_demand(view.sender, declaration.audio_slot_ids());
        view.declaration = Some(declaration);
        Ok(())
    }

    /// A reconnect: the new connection has received nothing, so forget what
    /// was emitted and re-dirty. Slots and rank are untouched.
    pub(super) fn reset_view(&mut self, participant_id: &str) {
        if let Some(view) = self.views.get_mut(participant_id) {
            view.last_directive = None;
            view.last_assignments = None;
            if view.declaration.is_some() {
                self.dirty.insert(participant_id.to_string());
            }
        }
    }

    /// Is any participant waiting to be flushed?
    pub(super) fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Re-render, re-publish to every handler, and mark `affected` dirty.
    pub(super) async fn reconcile(&mut self, affected: Affected) {
        self.render_and_publish().await;
        match affected {
            Affected::All => {
                for (participant_id, view) in &self.views {
                    if view.declaration.is_some() {
                        self.dirty.insert(participant_id.clone());
                    }
                }
            }
            Affected::HoldersOf(source) => {
                let holders = self.slots.holders_of(source);
                for (participant_id, view) in &self.views {
                    if view.declaration.is_some() && holders.contains(&view.sender) {
                        self.dirty.insert(participant_id.clone());
                    }
                }
            }
        }
    }

    async fn render_and_publish(&mut self) {
        let (Some(deps), Some(handlers)) = (&self.deps, &self.handlers) else {
            return;
        };
        let assignment = match self.slots.render(handlers.ids()) {
            Ok(assignment) => assignment,
            Err(e) => {
                // Fail loud: no push is better than pushing a policy MH will
                // reject whole. Every flush of this state then counts a
                // composition failure.
                error!(
                    target: "mc.register_meeting.trigger",
                    meeting_id = %self.meeting_id,
                    reason = e.label(),
                    error = %e,
                    "Forwarding assignment could not be rendered; MH not programmed"
                );
                self.assignment = None;
                return;
            }
        };

        for endpoint in handlers.iter() {
            let Some(policy) = assignment.for_handler(&endpoint.id) else {
                continue;
            };
            let generation = match deps
                .policy_generations
                .next_generation(&self.meeting_id, &endpoint.id, policy)
                .await
            {
                Ok(generation) => generation,
                Err(e) => {
                    error!(
                        target: "mc.register_meeting.trigger",
                        meeting_id = %self.meeting_id,
                        mh_grpc_endpoint = %endpoint.grpc_endpoint,
                        error = %e,
                        "Could not derive a policy generation; MH not programmed"
                    );
                    continue;
                }
            };
            let pusher = self.pushers.entry(endpoint.id.clone()).or_insert_with(|| {
                HandlerPusher::spawn(
                    PushTarget {
                        mh_client: Arc::clone(&deps.mh_client),
                        policy_generations: Arc::clone(&deps.policy_generations),
                        meeting_id: self.meeting_id.clone(),
                        mc_id: deps.mc_id.clone(),
                        mc_grpc_endpoint: deps.mc_grpc_endpoint.clone(),
                        handler: endpoint.clone(),
                    },
                    self.cancel.child_token(),
                )
            });
            // `send_replace` on EVERY reconcile, never behind an equality
            // guard: that is what re-sends a push that failed and is therefore
            // unconfirmed, even when this handler's snapshot did not change.
            pusher.publish(PushJob {
                generation,
                assignment: policy.clone(),
            });
        }
        self.assignment = Some(assignment);
    }

    /// Flush up to the per-turn bound of dirty participants; defer the rest.
    pub(super) async fn flush(&mut self, roster: &HashMap<String, RosterEntry<'_>>) {
        let mute = SourceMuteView::from_pairs(roster.values().map(|e| (e.sender, e.audio_muted)));
        let mut taken = 0usize;
        while taken < self.flush_batch {
            let Some(participant_id) = self.dirty.pop_first() else {
                break;
            };
            taken += 1;
            self.flush_one(&participant_id, roster, &mute).await;
        }
        for _ in 0..self.dirty.len() {
            metrics::record_slot_view_emission(SlotViewEmission::Deferred);
        }
    }

    /// A dirty participant that cannot be flushed because MC's own state
    /// sets disagree. Logged, not counted (the invariant-violation register is
    /// `mc_media_handler_set_divergence_total`'s; three more series would be
    /// noise).
    fn warn_unflushable(&self, participant_id: &str, reason: &'static str) {
        warn!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            reason = reason,
            "Dirty participant has no state to flush (MC-internal invariant violation); \
             its media view was not sent"
        );
    }

    async fn flush_one(
        &mut self,
        participant_id: &str,
        roster: &HashMap<String, RosterEntry<'_>>,
        mute: &SourceMuteView,
    ) {
        // Three of the five early returns below are INVARIANT VIOLATIONS (a
        // dirty participant with no roster entry, no routing deps, or no view):
        // if those sets ever drift, the participant silently stops receiving
        // view updates for the rest of its session, so each is logged loudly.
        // The other two are routine and produce no disposition.
        let Some(entry) = roster.get(participant_id) else {
            self.warn_unflushable(participant_id, "not_in_roster");
            return;
        };
        // Routine: disconnected (in grace), nothing to send to. A reconnect
        // re-dirties.
        let Some(connection) = entry.connection else {
            return;
        };
        let (Some(deps), Some(handlers)) = (&self.deps, &self.handlers) else {
            self.warn_unflushable(participant_id, "no_routing_deps");
            return;
        };
        let Some(view) = self.views.get(participant_id) else {
            self.warn_unflushable(participant_id, "no_view");
            return;
        };
        // Routine: not yet declared a receive capability; its declaration
        // dirties it.
        let Some(declaration) = &view.declaration else {
            return;
        };

        let composed = match &self.assignment {
            None => Err(DirectiveOutcome::AssignmentFailed),
            Some(assignment) => {
                build_send_directive(view.sender, assignment, handlers, &deps.stream_policy)
                    .and_then(|(directive, outcome)| {
                        build_stream_assignments(
                            view.sender,
                            declaration,
                            assignment,
                            handlers,
                            mute,
                            &self.slots.unreachable_for(view.sender),
                        )
                        .map(|composition| (directive, outcome, composition))
                    })
            }
        };

        let (directive, directive_outcome, composition) = match composed {
            Ok(composed) => composed,
            Err(outcome) => {
                // Fails CLOSED: nothing is emitted for this participant — never
                // an ACTIVE slot or a send target with an empty url.
                metrics::record_send_directive(outcome);
                metrics::record_slot_view_emission(SlotViewEmission::CompositionFailed);
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    outcome = outcome.label(),
                    "Media signalling could not be composed; this participant's view was not sent"
                );
                return;
            }
        };

        let directive_changed = view.last_directive.as_ref() != Some(&directive);
        let assignments_changed = view.last_assignments.as_ref() != Some(&composition.assignments);
        if !directive_changed && !assignments_changed {
            return;
        }

        let mut delivered = true;
        if directive_changed {
            metrics::record_send_directive(directive_outcome);
            delivered &= send(
                connection,
                server_message::Message::SendDirective(directive.clone()),
            )
            .await;
        }
        if assignments_changed && delivered {
            for state in &composition.slot_states {
                metrics::record_slot_state(*state);
            }
            delivered &= send(
                connection,
                server_message::Message::StreamAssignments(composition.assignments.clone()),
            )
            .await;
        }

        if !delivered {
            metrics::record_slot_view_emission(SlotViewEmission::DeliveryFailed);
            warn!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Failed to deliver media signalling to the participant actor (mailbox closed); \
                 the client was NOT told what mc_media_send_directives_total says was composed"
            );
            return;
        }

        metrics::record_slot_view_emission(SlotViewEmission::Sent);
        // Counted only on a DELIVERED `StreamAssignments`, matching the
        // `outcome="sent"` denominator of the catalog's ratio.
        if assignments_changed {
            metrics::record_unreachable_senders(
                composition.assignments.unreachable_sender_ids.len(),
            );
        }
        if let Some(view) = self.views.get_mut(participant_id) {
            view.last_directive = Some(directive);
            view.last_assignments = Some(composition.assignments);
        }
    }
}

/// Hand one `ServerMessage` to the participant actor. `false` means the
/// participant actor is gone (mailbox closed).
async fn send(connection: &ParticipantActorHandle, message: server_message::Message) -> bool {
    // R-57: carry the current server-side trace context (bounded W3C ids only).
    let (trace_parent, trace_state) = crate::webtransport::trace::inject_current_context();
    let message = ServerMessage {
        message: Some(message),
        trace_parent,
        trace_state,
    };
    connection
        .send(SignalingPayload::server_message(&message))
        .await
        .is_ok()
}
