//! What MC tells a subscriber about its **slots** (ADR-0036 §6).
//!
//! # A join of the slot table's render and the subscriber's declaration
//!
//! MC composes a subscriber's view from two inputs:
//!
//! 1. the **forwarding assignment** rendered from the meeting's join-order slot
//!    state — the same value the MC→MH control plane is programmed from — which
//!    fixes who fills each declared audio slot and what `stream_id` MH stamps;
//!    and
//! 2. the subscriber's **capability declaration**, which fixes which slots it
//!    will accept.
//!
//! Since story 2 the declaration is the assignment's INPUT (declared audio slots
//! size the subscriber's slot vector), so every declared audio slot is either
//! filled — `ACTIVE`, or `SOURCE_MUTED` — or unfilled — `FEWER_SOURCES_THAN_SLOTS`.
//! §6's rule is that *"absence of frames is not a signal"*, so every partial
//! case is an explicit slot state.
//!
//! # What this never emits
//!
//! `SLOT_STATE_ZERO_REQUESTED` (MC never fabricates a `slot_id` to carry it) and
//! `SLOT_STATE_SOURCE_UNREACHABLE` (reserved for a pinned source with which the
//! subscriber shares no connected handler, which static fill cannot produce). A
//! participant the subscriber shares NO connected handler with consumes no slot
//! and is named in `unreachable_sender_ids` instead — one bit per roster entry,
//! relative to this subscriber, carrying no topology. A participant not yet
//! connected is in neither.
//!
//! # Each slot names ITS edge's handler
//!
//! `StreamAssignment.media_handler_url` is the handler owning that edge, so one
//! subscriber may read different slots on different handlers (ADR-0036 §9); the
//! client reads each slot on the transport its url names.
//!
//! # Mute lives here and only here
//!
//! A source's mute changes what a **subscriber is told about it**, never what a
//! **publisher is told to produce**. [`super::directive`] therefore has no
//! access to mute state at all; this module is the only one that reads it.

use super::capability::{ReceiveCapabilityDeclaration, SlotId};
use super::outcome::DirectiveOutcome;
use crate::media_admission::SenderId;
use crate::media_routing::{HandlerId, MeetingAssignment, MeetingHandlers};
use proto_gen::dark_tower::signaling::v1::{
    MediaKind, SlotState, StreamAssignment, StreamAssignments,
};
use std::collections::HashMap;

/// Which sources are currently audio-muted, for any reason.
///
/// A **read-only projection** of the meeting actor's roster, built per
/// composition and not retained. The actor builds it from
/// `audio_self_muted || audio_server_muted`: the wire's single
/// `SLOT_STATE_SOURCE_MUTED` covers both causes (who muted whom rides on
/// `ParticipantMuteUpdate`), so a subscriber's view must change when either
/// flag does. The actor is the one home of both flags; this is a view, never a
/// second copy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMuteView {
    audio_muted: HashMap<SenderId, bool>,
}

impl SourceMuteView {
    /// Build from `(sender id, audio muted for any reason)` pairs off a roster
    /// snapshot.
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (SenderId, bool)>) -> Self {
        Self {
            audio_muted: pairs.into_iter().collect(),
        }
    }

    /// Is this source audio-muted?
    ///
    /// An unknown source is reported **not muted**: the roster is the authority
    /// on mute, and a source missing from the snapshot is a staleness question,
    /// not a mute one. Answering "muted" would render a placeholder for a
    /// participant who is in fact speaking.
    #[must_use]
    pub fn is_audio_muted(&self, sender: SenderId) -> bool {
        self.audio_muted.get(&sender).copied().unwrap_or(false)
    }
}

/// The result of composing one subscriber's slot view.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotComposition {
    /// The wire message.
    pub assignments: StreamAssignments,
    /// Per-slot states, in the same order, for the bounded slot-state counter.
    ///
    /// Read off the assignments themselves rather than tracked in parallel, so
    /// the metric and the wire cannot disagree about what MC said.
    pub slot_states: Vec<SlotState>,
}

/// Bounded metric-label form for a slot state.
///
/// Exhaustive over **all eight** proto variants, not the three reachable today.
/// A wire-vocabulary mirror is supposed to cover states it does not yet emit:
/// a hand-picked subset needs editing every time §7 makes `switch_pending` live,
/// and a `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC defect that must be
/// visible rather than absent. Keeping the vocabulary identical to the wire is
/// also what makes MC's distribution comparable with the client's.
#[must_use]
pub fn slot_state_label(state: SlotState) -> &'static str {
    match state {
        SlotState::Unspecified => "unspecified",
        SlotState::Active => "active",
        SlotState::SourceMuted => "source_muted",
        SlotState::WithheldByCongestion => "withheld_by_congestion",
        SlotState::FewerSourcesThanSlots => "fewer_sources_than_slots",
        SlotState::ZeroRequested => "zero_requested",
        SlotState::SourceUnreachable => "source_unreachable",
        SlotState::SwitchPending => "switch_pending",
    }
}

/// Compose one subscriber's slot assignments — the ONE construction site of
/// `StreamAssignments`.
///
/// Total: every declared slot yields exactly one [`StreamAssignment`] carrying
/// an explicit state, and no slot is silently omitted. `unreachable` is the
/// slot table's `unreachable_for(subscriber)`, carried verbatim.
///
/// `sender_id` is `Option` throughout and absence is **never coerced to 0** — a
/// shared zero is not a recycled id but N concurrently-live colliding ones, and
/// since the key id encodes `(sender, stream, generation)`, two senders at zero
/// wrap distinct transmit keys under one KEK at one nonce.
///
/// # Errors
///
/// [`DirectiveOutcome::HandlerUrlUnresolved`] if a filled slot's handler has no
/// url in `handlers`. Unreachable by construction (one frozen handler set feeds
/// both), and it fails CLOSED: the caller emits nothing for this subscriber
/// rather than an `ACTIVE` slot with an empty url.
pub fn build_stream_assignments(
    subscriber: SenderId,
    declaration: &ReceiveCapabilityDeclaration,
    assignment: &MeetingAssignment,
    handlers: &MeetingHandlers,
    mute: &SourceMuteView,
    unreachable: &[SenderId],
) -> Result<SlotComposition, DirectiveOutcome> {
    // The subscriber's own egress plans, keyed by the slot MH will stamp.
    let mut planned: HashMap<SlotId, (&HandlerId, Option<SenderId>)> = HashMap::new();
    for (handler, handler_assignment) in &assignment.per_handler {
        for plan in &handler_assignment.egress_streams {
            if plan.subscriber != subscriber {
                continue;
            }
            // Exactly one pinned candidate under static fill.
            planned.insert(
                SlotId::from_u16(plan.slot_id),
                (handler, plan.candidate_sources.first().copied()),
            );
        }
    }

    let mut assignments = Vec::with_capacity(declaration.slots().len());
    let mut slot_states = Vec::with_capacity(declaration.slots().len());

    for slot in declaration.slots() {
        // A plan is audio in this story. Matching on kind as well as id keeps a
        // video slot from being filled with audio when video lands.
        let plan = if slot.media_kind == MediaKind::Audio {
            planned.get(&slot.slot_id)
        } else {
            None
        };

        let (sender_id, media_handler_url, slot_state) = match plan {
            Some((handler, Some(source))) => {
                let url = handlers
                    .url_of(handler)
                    .ok_or(DirectiveOutcome::HandlerUrlUnresolved)?;
                let state = if mute.is_audio_muted(*source) {
                    // §5: muting signals out of band. The receiver renders a
                    // placeholder from this state, never from a substitute
                    // frame — and MC does not touch the source's directive.
                    SlotState::SourceMuted
                } else {
                    SlotState::Active
                };
                (Some(u32::from(source.get().get())), url.to_string(), state)
            }
            // Declared, but no sender this subscriber shares a connected handler with is left to fill
            // it: the honest steady state of a meeting smaller than N, and of a
            // solo participant (R-3). The url is empty BY CONTRACT here.
            Some((_, None)) | None => (None, String::new(), SlotState::FewerSourcesThanSlots),
        };

        slot_states.push(slot_state);
        assignments.push(StreamAssignment {
            slot_id: u32::from(slot.slot_id.get()),
            sender_id,
            media_kind: slot.media_kind as i32,
            media_handler_url,
            slot_state: slot_state as i32,
            // §7 switching is not in this story. Present if and only if the
            // state is `SWITCH_PENDING`, which MC never emits here.
            switch_command_id: None,
        });
    }

    Ok(SlotComposition {
        assignments: StreamAssignments {
            assignments,
            unreachable_sender_ids: unreachable
                .iter()
                .map(|s| u32::from(s.get().get()))
                .collect(),
        },
        slot_states,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::{HandlerEndpoint, SlotTable};
    use proto_gen::dark_tower::signaling::v1::{ReceiveCapability, ReceiveSlot};
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn handler_set(handlers: &[&str]) -> MeetingHandlers {
        MeetingHandlers::new(handlers.iter().map(|h| HandlerEndpoint {
            id: HandlerId::new(*h),
            webtransport_url: format!("https://{h}.example:4434"),
            grpc_endpoint: format!("http://{h}.example:50053"),
        }))
        .unwrap()
    }

    fn declare(slots: &[(u32, MediaKind)]) -> ReceiveCapabilityDeclaration {
        let message = ReceiveCapability {
            slots: slots
                .iter()
                .map(|(id, kind)| ReceiveSlot {
                    slot_id: *id,
                    media_kind: *kind as i32,
                    pinned_sender_id: None,
                })
                .collect(),
        };
        ReceiveCapabilityDeclaration::parse(&message, 64, true)
            .expect("fixture declaration is valid")
    }

    /// A meeting on `handlers` where `others` connect only to `mh-0` after subscriber 1,
    /// which declares `slots`. Returns (assignment, handlers, table).
    fn meeting(
        handlers: &[&str],
        others: &[u16],
        slots: &[(u32, MediaKind)],
    ) -> (
        MeetingAssignment,
        MeetingHandlers,
        SlotTable,
        ReceiveCapabilityDeclaration,
    ) {
        let set = handler_set(handlers);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0"]);
        for s in others {
            table.admit_on(sender(*s), &set, &["mh-0"]);
        }
        let declaration = declare(slots);
        table.set_demand(sender(1), declaration.audio_slot_ids());
        (
            table
                .render(set.ids(), &std::collections::BTreeSet::new())
                .unwrap(),
            set,
            table,
            declaration,
        )
    }

    fn compose(
        others: &[u16],
        slots: &[(u32, MediaKind)],
        mute: &SourceMuteView,
    ) -> SlotComposition {
        let (assignment, set, table, declaration) = meeting(&["mh-0"], others, slots);
        build_stream_assignments(
            sender(1),
            &declaration,
            &assignment,
            &set,
            mute,
            &table.unreachable_for(sender(1)),
        )
        .unwrap()
    }

    #[test]
    fn a_filled_slot_names_its_source_and_is_active() {
        let composition = compose(&[2], &[(0, MediaKind::Audio)], &SourceMuteView::default());
        assert_eq!(composition.assignments.assignments.len(), 1);
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_id, 0);
        assert_eq!(
            a.sender_id,
            Some(2),
            "the peer sharing mh-0, never the subscriber itself"
        );
        assert_eq!(a.media_kind, MediaKind::Audio as i32);
        assert_eq!(a.media_handler_url, "https://mh-0.example:4434");
        assert_eq!(a.slot_state, SlotState::Active as i32);
        // Present iff SWITCH_PENDING, which MC never emits this story.
        assert!(a.switch_command_id.is_none());
    }

    /// R-3: a solo participant hears nothing, and every declared slot says so
    /// explicitly.
    #[test]
    fn a_solo_subscribers_slots_are_all_fewer_sources_than_slots() {
        let composition = compose(
            &[],
            &[(0, MediaKind::Audio), (1, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        for a in &composition.assignments.assignments {
            assert_eq!(a.slot_state, SlotState::FewerSourcesThanSlots as i32);
            assert!(a.sender_id.is_none());
            assert!(a.media_handler_url.is_empty());
        }
        assert!(composition.assignments.unreachable_sender_ids.is_empty());
    }

    #[test]
    fn a_muted_source_is_conveyed_as_source_muted_with_the_sender_still_named() {
        let mute = SourceMuteView::from_pairs(vec![(sender(2), true)]);
        let composition = compose(&[2], &[(0, MediaKind::Audio)], &mute);
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_state, SlotState::SourceMuted as i32);
        assert_eq!(a.sender_id, Some(2));
    }

    #[test]
    fn an_unfilled_slot_is_a_source_shortage_with_sender_id_absent_not_zero() {
        let composition = compose(
            &[2],
            &[(0, MediaKind::Audio), (1, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        assert_eq!(composition.assignments.assignments.len(), 2);
        let extra = &composition.assignments.assignments[1];
        assert_eq!(extra.slot_id, 1);
        assert_eq!(extra.slot_state, SlotState::FewerSourcesThanSlots as i32);
        // ABSENT, not 0.
        assert!(extra.sender_id.is_none());
        assert_ne!(extra.sender_id, Some(0));
        assert!(extra.media_handler_url.is_empty());
    }

    #[test]
    fn the_subscribers_own_slot_numbering_is_echoed_not_mcs() {
        let composition = compose(
            &[2, 3],
            &[(9, MediaKind::Audio), (7, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        let ids: Vec<u32> = composition
            .assignments
            .assignments
            .iter()
            .map(|a| a.slot_id)
            .collect();
        assert_eq!(ids, vec![9, 7]);
        let senders: Vec<Option<u32>> = composition
            .assignments
            .assignments
            .iter()
            .map(|a| a.sender_id)
            .collect();
        assert_eq!(
            senders,
            vec![Some(2), Some(3)],
            "join order into declaration order"
        );
    }

    #[test]
    fn a_video_slot_is_a_shortage_and_never_filled_with_audio() {
        let composition = compose(
            &[2],
            &[(1, MediaKind::VideoCamera)],
            &SourceMuteView::default(),
        );
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.media_kind, MediaKind::VideoCamera as i32);
        assert_eq!(a.slot_state, SlotState::FewerSourcesThanSlots as i32);
        assert!(a.sender_id.is_none());
    }

    #[test]
    fn a_zero_slot_declaration_yields_no_assignments() {
        // `slot_id` echoes a slot the subscriber CHOSE, so there is no slot to
        // carry ZERO_REQUESTED and MC never fabricates one.
        let composition = compose(&[2], &[], &SourceMuteView::default());
        assert!(composition.assignments.assignments.is_empty());
        assert!(composition.slot_states.is_empty());
    }

    /// S-A: a filled slot whose handler has no url fails CLOSED — no message,
    /// never an ACTIVE slot with an empty url.
    #[test]
    fn an_unresolvable_handler_fails_closed_rather_than_emitting_an_empty_url() {
        let (assignment, _, table, declaration) =
            meeting(&["mh-0"], &[2], &[(0, MediaKind::Audio)]);
        let outcome = build_stream_assignments(
            sender(1),
            &declaration,
            &assignment,
            &handler_set(&["mh-9"]),
            &SourceMuteView::default(),
            &table.unreachable_for(sender(1)),
        );
        assert_eq!(outcome, Err(DirectiveOutcome::HandlerUrlUnresolved));
    }

    /// No declared slot, of any shape, ever carries a state MC does not emit
    /// this story.
    #[test]
    fn zero_requested_and_source_unreachable_are_never_emitted() {
        for others in [&[][..], &[2][..], &[2, 3, 4][..]] {
            let composition = compose(
                others,
                &[
                    (0, MediaKind::Audio),
                    (1, MediaKind::Audio),
                    (2, MediaKind::VideoCamera),
                ],
                &SourceMuteView::from_pairs(vec![(sender(2), true)]),
            );
            for state in &composition.slot_states {
                assert_ne!(*state, SlotState::ZeroRequested);
                assert_ne!(*state, SlotState::SourceUnreachable);
            }
        }
    }

    /// R-33: a participant sharing no connected handler consumes no slot and is
    /// named in `unreachable_sender_ids`; a participant sharing one but left over
    /// by full slots is NOT.
    #[test]
    fn disjoint_peers_are_unreachable_and_sharing_overflow_is_not() {
        let set = handler_set(&["mh-0", "mh-1"]);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0"]);
        table.admit_on(sender(2), &set, &["mh-1"]);
        table.admit_on(sender(3), &set, &["mh-0"]);
        table.admit_on(sender(4), &set, &["mh-0"]);
        let declaration = declare(&[(0, MediaKind::Audio)]);
        table.set_demand(sender(1), declaration.audio_slot_ids());
        let composition = build_stream_assignments(
            sender(1),
            &declaration,
            &table
                .render(set.ids(), &std::collections::BTreeSet::new())
                .unwrap(),
            &set,
            &SourceMuteView::default(),
            &table.unreachable_for(sender(1)),
        )
        .unwrap();
        assert_eq!(composition.assignments.assignments[0].sender_id, Some(3));
        assert_eq!(
            composition.assignments.unreachable_sender_ids,
            vec![2],
            "exactly the cross-handler participant; 4 is 'no slot', not unreachable"
        );
    }

    #[test]
    fn slot_states_mirror_the_wire_and_cannot_disagree_with_it() {
        let composition = compose(
            &[2],
            &[(0, MediaKind::Audio), (1, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        let from_wire: Vec<i32> = composition
            .assignments
            .assignments
            .iter()
            .map(|a| a.slot_state)
            .collect();
        let from_metric: Vec<i32> = composition.slot_states.iter().map(|s| *s as i32).collect();
        assert_eq!(from_wire, from_metric);
    }

    #[test]
    fn an_unknown_source_is_reported_not_muted() {
        let view = SourceMuteView::from_pairs(vec![(sender(2), true)]);
        assert!(!view.is_audio_muted(sender(1)));
        assert!(view.is_audio_muted(sender(2)));
    }

    #[test]
    fn every_wire_slot_state_has_a_distinct_bounded_label() {
        let all = [
            SlotState::Unspecified,
            SlotState::Active,
            SlotState::SourceMuted,
            SlotState::WithheldByCongestion,
            SlotState::FewerSourcesThanSlots,
            SlotState::ZeroRequested,
            SlotState::SourceUnreachable,
            SlotState::SwitchPending,
        ];
        let labels: std::collections::HashSet<&str> =
            all.iter().map(|s| slot_state_label(*s)).collect();
        assert_eq!(labels.len(), all.len());
    }

    /// The slot's handler address IS the handler carrying this subscriber's
    /// plan, in a two-handler meeting where everyone sits on mh-0.
    ///
    /// The url is a WRITTEN LITERAL, not sourced from any helper the production
    /// path also calls. The nameable wrong answer — mh-1, present with a
    /// correct-by-design empty set — is asserted against explicitly.
    #[test]
    fn the_slot_names_the_handler_carrying_this_subscribers_plan_at_n_2() {
        let (assignment, set, table, declaration) =
            meeting(&["mh-1", "mh-0"], &[2], &[(0, MediaKind::Audio)]);
        assert!(assignment.per_handler[&HandlerId::new("mh-1")]
            .egress_streams
            .is_empty());
        let composition = build_stream_assignments(
            sender(1),
            &declaration,
            &assignment,
            &set,
            &SourceMuteView::default(),
            &table.unreachable_for(sender(1)),
        )
        .unwrap();
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_state, SlotState::Active as i32);
        assert_eq!(a.media_handler_url, "https://mh-0.example:4434");
        assert_ne!(a.media_handler_url, "https://mh-1.example:4434");
    }
}
