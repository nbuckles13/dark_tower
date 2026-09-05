//! What MC tells a subscriber about its **slots** (ADR-0036 §6).
//!
//! # This is a join, and every partial case is explicit
//!
//! MC composes a subscriber's view from two inputs that arrive at different
//! times and are numbered in different namespaces:
//!
//! 1. the **forwarding policy already pushed to MH**, which fixes what MH stamps
//!    into each frame's relay-region `stream_id`; and
//! 2. the subscriber's **capability declaration**, which fixes what that
//!    subscriber will accept.
//!
//! The assignment is the join of the two, and §6's rule is that *"absence of
//! frames is not a signal"* — so every partial case is conveyed as an explicit
//! slot state rather than left to be inferred from silence. A declared slot with
//! no plan is a source shortage, and says so; it is not confused with a source
//! that exists but is muted, or one that is unreachable.
//!
//! The one case that is **not** conveyed here is a declaration asking for audio
//! in a slot MC has no plan for. That is rejected upstream in
//! [`super::capability`], because reporting it as a source shortage would tell
//! the client "there is nobody to show you" while MH actively forwards a source
//! it will drop.
//!
//! # Mute lives here and only here
//!
//! A source's client mute changes what a **subscriber is told about it**, never
//! what a **publisher is told to produce**. [`super::directive`] therefore has
//! no access to mute state at all; this module is the only one that reads it.

use super::capability::{ReceiveCapabilityDeclaration, SlotId};
use super::directive::HandlerUrls;
use crate::media_admission::SenderId;
use crate::media_routing::{HandlerId, MeetingAssignment};
use proto_gen::dark_tower::signaling::v1::{
    MediaKind, SlotState, StreamAssignment, StreamAssignments,
};
use std::collections::HashMap;

/// Which sources are currently reporting themselves audio-muted.
///
/// A **read-only projection** of the meeting actor's roster, built per
/// composition and not retained. MC keeps exactly one home for mute state —
/// `ParticipantInfo::audio_self_muted`, updated through
/// `MeetingActorHandle::update_self_mute` — and this type is a view of it, never
/// a second copy that could disagree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMuteView {
    audio_muted: HashMap<SenderId, bool>,
}

impl SourceMuteView {
    /// Build from `(sender id, audio self-muted)` pairs off a roster snapshot.
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (SenderId, bool)>) -> Self {
        Self {
            audio_muted: pairs.into_iter().collect(),
        }
    }

    /// Is this source reporting itself audio-muted?
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
    /// Planned egress slots this subscriber never declared.
    ///
    /// Reachable in this story only through the zero-audio exemption: a
    /// subscriber declaring `{}` or video-only slots is accepted, and MC's
    /// planned audio slot then matches nothing. MH forwards egress that
    /// subscriber will never accept — wasted uplink and handler work — which is
    /// the operational question the counter answers.
    pub unmatched_plan_slots: usize,
}

/// Bounded metric-label form for a slot state.
///
/// Exhaustive over **all eight** proto variants, not the four reachable today.
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

/// Compose one subscriber's slot assignments.
///
/// Total: every declared slot yields exactly one [`StreamAssignment`] carrying
/// an explicit state, and no slot is silently omitted.
///
/// `sender_id` is `Option` throughout and absence is **never coerced to 0** — a
/// shared zero is not a recycled id but N concurrently-live colliding ones, and
/// since the key id encodes `(sender, stream, generation)`, two senders at zero
/// wrap distinct transmit keys under one KEK at one nonce.
#[must_use]
pub fn build_stream_assignments(
    subscriber: SenderId,
    declaration: &ReceiveCapabilityDeclaration,
    assignment: &MeetingAssignment,
    handler_urls: &HandlerUrls,
    mute: &SourceMuteView,
) -> SlotComposition {
    // The subscriber's own egress plans, keyed by the slot MH will stamp.
    // Built from the same `compute_assignment` output the MC->MH control plane
    // is programmed from, so the two sides cannot disagree about who fills what.
    let mut planned: HashMap<SlotId, (&HandlerId, Option<SenderId>)> = HashMap::new();
    for (handler, handler_assignment) in &assignment.per_handler {
        for plan in &handler_assignment.egress_streams {
            if plan.subscriber != subscriber {
                continue;
            }
            // Candidates are sorted by the assignment, so `first` is
            // deterministic rather than incidental. Selection among many
            // candidates is MH's §7 job; MC names the pool's head as the source
            // it expects to see.
            planned.insert(
                SlotId::from_u16(plan.slot_id),
                (handler, plan.candidate_sources.first().copied()),
            );
        }
    }

    let mut matched_plan_slots = 0usize;
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
                matched_plan_slots += 1;
                match handler_urls.get(handler) {
                    Some(url) => {
                        let state = if mute.is_audio_muted(*source) {
                            // §5: muting signals out of band. The receiver
                            // renders a placeholder from this state, never from
                            // a substitute frame — and MC does not touch the
                            // source's send directive.
                            SlotState::SourceMuted
                        } else {
                            SlotState::Active
                        };
                        (Some(u32::from(source.get().get())), url.to_string(), state)
                    }
                    // A source MC cannot route to is structurally persistent
                    // rather than transient (§9), and carries no sender id: the
                    // subscriber is not being told a source is arriving.
                    None => (None, String::new(), SlotState::SourceUnreachable),
                }
            }
            // A plan with no candidate is a plan with no source; treat it as
            // the shortage it is rather than asserting a source.
            Some((_, None)) => {
                matched_plan_slots += 1;
                (None, String::new(), SlotState::FewerSourcesThanSlots)
            }
            // Declared, but MC has nothing to put in it. The honest steady
            // state, and NOT the same thing as the rejected case where a
            // subscriber asks for audio in a slot MC cannot address at all.
            None => (None, String::new(), SlotState::FewerSourcesThanSlots),
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

    SlotComposition {
        assignments: StreamAssignments { assignments },
        slot_states,
        unmatched_plan_slots: planned.len().saturating_sub(matched_plan_slots),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::{compute_assignment, MeetingRoutingInput, RoutingParticipant};
    use crate::media_signaling::capability::PlannedAudioSlot;
    use proto_gen::dark_tower::signaling::v1::{ReceiveCapability, ReceiveSlot};
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn loopback() -> (MeetingAssignment, HandlerUrls) {
        let ids = vec![HandlerId::new("mh-0")];
        let input = MeetingRoutingInput {
            participants: vec![RoutingParticipant {
                sender_id: sender(1),
                handlers: ids.clone(),
            }],
            handlers: ids.clone(),
        };
        let urls = HandlerUrls::from_pairs(vec![(
            ids[0].clone(),
            "https://mh-0.example:4434".to_string(),
        )]);
        (compute_assignment(&input).unwrap(), urls)
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
        ReceiveCapabilityDeclaration::parse(
            &message,
            64,
            true,
            PlannedAudioSlot::new(SlotId::from_u16(0)),
        )
        .expect("fixture declaration is valid")
    }

    fn compose(slots: &[(u32, MediaKind)], mute: &SourceMuteView) -> SlotComposition {
        let (assignment, urls) = loopback();
        build_stream_assignments(sender(1), &declare(slots), &assignment, &urls, mute)
    }

    #[test]
    fn own_audio_fills_own_slot_and_is_active() {
        let composition = compose(&[(0, MediaKind::Audio)], &SourceMuteView::default());
        assert_eq!(composition.assignments.assignments.len(), 1);
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_id, 0);
        assert_eq!(a.sender_id, Some(1));
        assert_eq!(a.media_kind, MediaKind::Audio as i32);
        assert_eq!(a.media_handler_url, "https://mh-0.example:4434");
        assert_eq!(a.slot_state, SlotState::Active as i32);
        // Present iff SWITCH_PENDING, which MC never emits this story.
        assert!(a.switch_command_id.is_none());
        assert_eq!(composition.unmatched_plan_slots, 0);
    }

    #[test]
    fn a_muted_source_is_conveyed_as_source_muted_with_the_sender_still_named() {
        // The source is PRESENT and has muted itself; the receiver renders a
        // placeholder from the state, never from a substitute frame. The sender
        // id stays, because the subscriber still knows who occupies the slot.
        let mute = SourceMuteView::from_pairs(vec![(sender(1), true)]);
        let composition = compose(&[(0, MediaKind::Audio)], &mute);
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_state, SlotState::SourceMuted as i32);
        assert_eq!(a.sender_id, Some(1));
    }

    #[test]
    fn an_extra_slot_is_a_source_shortage_with_sender_id_absent_not_zero() {
        let composition = compose(
            &[(0, MediaKind::Audio), (1, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        assert_eq!(composition.assignments.assignments.len(), 2);
        let extra = &composition.assignments.assignments[1];
        assert_eq!(extra.slot_id, 1);
        assert_eq!(extra.slot_state, SlotState::FewerSourcesThanSlots as i32);
        // ABSENT, not 0. A shared zero is not a recycled id but N
        // concurrently-live colliding ones, and the key id encodes the sender.
        assert!(extra.sender_id.is_none());
        assert_ne!(extra.sender_id, Some(0));
        assert!(extra.media_handler_url.is_empty());
    }

    #[test]
    fn the_subscribers_own_slot_numbering_is_echoed_not_mcs() {
        // `{0, 7}`: slot 0 is served, slot 7 must come back carrying 7. A
        // hardcoded-0 implementation cannot produce this.
        let composition = compose(
            &[(0, MediaKind::Audio), (7, MediaKind::Audio)],
            &SourceMuteView::default(),
        );
        let ids: Vec<u32> = composition
            .assignments
            .assignments
            .iter()
            .map(|a| a.slot_id)
            .collect();
        assert_eq!(ids, vec![0, 7]);
        assert_eq!(
            composition.assignments.assignments[1].slot_state,
            SlotState::FewerSourcesThanSlots as i32
        );
    }

    #[test]
    fn a_video_slot_is_a_shortage_and_never_filled_with_audio() {
        let composition = compose(&[(1, MediaKind::VideoCamera)], &SourceMuteView::default());
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.media_kind, MediaKind::VideoCamera as i32);
        assert_eq!(a.slot_state, SlotState::FewerSourcesThanSlots as i32);
        assert!(a.sender_id.is_none());
        // The audio plan matched nothing: MH forwards egress this subscriber
        // will not accept.
        assert_eq!(composition.unmatched_plan_slots, 1);
    }

    #[test]
    fn a_zero_slot_declaration_yields_no_assignments_and_one_unmatched_plan() {
        // The reachable path for the unmatched-plan counter in this story. It is
        // legitimate client behaviour ("send but do not receive"), not an
        // injected fault, and there is no slot to carry ZERO_REQUESTED because
        // the subscriber declared none — `slot_id` echoes a slot the subscriber
        // CHOSE, so fabricating id 0 here would break the echo property.
        let composition = compose(&[], &SourceMuteView::default());
        assert!(composition.assignments.assignments.is_empty());
        assert!(composition.slot_states.is_empty());
        assert_eq!(composition.unmatched_plan_slots, 1);
    }

    #[test]
    fn an_unresolvable_handler_makes_the_source_unreachable_with_no_sender_id() {
        let (assignment, _) = loopback();
        let composition = build_stream_assignments(
            sender(1),
            &declare(&[(0, MediaKind::Audio)]),
            &assignment,
            &HandlerUrls::default(),
            &SourceMuteView::default(),
        );
        let a = &composition.assignments.assignments[0];
        assert_eq!(a.slot_state, SlotState::SourceUnreachable as i32);
        assert!(a.sender_id.is_none());
        assert!(a.media_handler_url.is_empty());
    }

    #[test]
    fn slot_states_mirror_the_wire_and_cannot_disagree_with_it() {
        let composition = compose(
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
        // The roster is the authority. Answering "muted" for a source missing
        // from a stale snapshot would render a placeholder for someone speaking.
        let view = SourceMuteView::from_pairs(vec![(sender(2), true)]);
        assert!(!view.is_audio_muted(sender(1)));
        assert!(view.is_audio_muted(sender(2)));
    }

    #[test]
    fn every_wire_slot_state_has_a_distinct_bounded_label() {
        // Exhaustive over all eight, not the four reachable today: a hand-picked
        // subset needs editing the moment §7 makes switch_pending live, and an
        // UNSPECIFIED on the wire is an MC defect that must be visible.
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
}
