//! What MC tells a subscriber about its **slots** (ADR-0036 §6; story 3 R-1,
//! R-3, R-4, R-5).
//!
//! # Composed from the slot table and the truncated declaration
//!
//! MC composes a subscriber's view from three inputs:
//!
//! 1. the meeting's **slot table** — who occupies each kept slot of each kind,
//!    and which handler carries the pair (the same state the MC→MH snapshot is
//!    rendered from);
//! 2. the subscriber's **truncated declaration** ([`SlotDemand`]), which fixes
//!    the declared slots in declaration order and which of them truncation
//!    made `UNAVAILABLE`; and
//! 3. a **per-kind mute projection** ([`SourceMuteView`]) of the roster.
//!
//! Every declared slot yields exactly one assignment carrying an explicit
//! state (§6: *"absence of frames is not a signal"*).
//!
//! # The flat slot-state vocabulary (story 3 wire freeze)
//!
//! | Slot | State | `sender_id` | Url |
//! |---|---|---|---|
//! | video, occupant publishes video, not muted | `ACTIVE` | **absent** — MC never names a live video source (R-5) | the edge's handler |
//! | video, occupant publishes NO video (any mute) | `AVATAR_NO_VIDEO` | named | empty (no edge) |
//! | client-muted occupant (audio, or video publisher) | `AVATAR_CLIENT_MUTED` | named | the edge's handler |
//! | server-muted occupant (wins over client-muted) | `AVATAR_SERVER_MUTED` | named | the edge's handler |
//! | audio, occupant not muted | `ACTIVE` | named | the edge's handler |
//! | kept, unfilled | `FEWER_SOURCES_THAN_SLOTS` | absent | empty |
//! | truncated by the per-kind cap | `UNAVAILABLE` | absent | empty |
//!
//! NO-VIDEO PRECEDENCE (lead ruling 2026-10-21): a video slot whose occupant
//! publishes no video is `AVATAR_NO_VIDEO` whatever its mutes. A mute avatar
//! KEEPS its edge's url (Lead ruling 3, MC-A Gate 1): the edge still exists, so
//! the client keeps that handler's receive transport and a resume needs no
//! reconnect (R-5's decode-before-live). Audio never uses `AVATAR_NO_VIDEO`
//! (an audio slot holds only audio publishers). **Nothing emits
//! `SOURCE_MUTED`** any more (MC-B deletes the value).
//!
//! # What this never emits
//!
//! `SLOT_STATE_ZERO_REQUESTED` (reserved) and `SLOT_STATE_SOURCE_UNREACHABLE`
//! (a participant the subscriber shares NO connected handler with consumes no
//! slot and is named in `unreachable_sender_ids` instead; one not yet connected
//! is in neither).
//!
//! # Mute lives here and only here
//!
//! A source's mute changes what a **subscriber is told about it**, never what a
//! **publisher is told to produce**. [`super::directive`] has no access to mute
//! state at all.

use super::capability::SlotDemand;
use super::outcome::{DirectiveOutcome, MediaKindLabel};
use crate::media_admission::SenderId;
pub use crate::media_routing::SourceMuteView;
use crate::media_routing::{MeetingHandlers, SlotTable};
use proto_gen::dark_tower::signaling::v1::{SlotState, StreamAssignment, StreamAssignments};

/// The result of composing one subscriber's slot view.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotComposition {
    /// The wire message.
    pub assignments: StreamAssignments,
    /// Per-slot `(kind, state)`, in the same order, for the bounded slot-state
    /// counter — read off the assignments themselves, so the metric and the
    /// wire cannot disagree.
    pub slot_states: Vec<(MediaKindLabel, SlotState)>,
}

/// Bounded metric-label form for a slot state.
///
/// Exhaustive over **every** proto variant, including the superseded
/// `source_muted` (MC-B deletes it with the wire value) and states MC does not
/// emit: a `SLOT_STATE_UNSPECIFIED` reaching the wire is an MC defect that must
/// be visible rather than absent, and identical vocabularies keep MC's
/// distribution comparable with the client's. Labels follow the wire names.
#[must_use]
pub fn slot_state_label(state: SlotState) -> &'static str {
    match state {
        SlotState::Unspecified => "unspecified",
        SlotState::Active => "active",
        SlotState::SourceMuted => "source_muted",
        SlotState::WithheldByCongestion => "withheld_by_congestion",
        SlotState::FewerSourcesThanSlots => "fewer_sources_than_slots",
        SlotState::SourceUnreachable => "source_unreachable",
        SlotState::SwitchPending => "switch_pending",
        SlotState::AvatarNoVideo => "avatar_no_video",
        SlotState::Unavailable => "unavailable",
        SlotState::AvatarClientMuted => "avatar_client_muted",
        SlotState::AvatarServerMuted => "avatar_server_muted",
    }
}

/// Compose one subscriber's slot assignments — the ONE construction site of
/// `StreamAssignments`.
///
/// Total: every declared slot yields exactly one [`StreamAssignment`].
/// `sender_id` is `Option` throughout and absence is **never coerced to 0**.
///
/// # Errors
///
/// [`DirectiveOutcome::HandlerUrlUnresolved`] if an edge's handler has no url
/// in `handlers`. Unreachable by construction (one frozen handler set feeds
/// both), and it fails CLOSED: the caller emits nothing for this subscriber.
pub fn build_stream_assignments(
    subscriber: SenderId,
    demand: &SlotDemand,
    table: &SlotTable,
    handlers: &MeetingHandlers,
    mute: &SourceMuteView,
    unreachable: &[SenderId],
) -> Result<SlotComposition, DirectiveOutcome> {
    let entries = demand.entries();
    let mut assignments = Vec::with_capacity(entries.len());
    let mut slot_states = Vec::with_capacity(entries.len());

    for (slot, ordinal) in entries {
        let kind = slot.kind_label;
        let edge = ordinal.and_then(|i| {
            table
                .slots_of(subscriber, kind)
                .and_then(|slots| slots.get(i))
                .cloned()
                .flatten()
        });
        let (sender_id, media_handler_url, slot_state) = match (ordinal, edge) {
            // Truncated by the per-kind cap: permanent for this declaration.
            (None, _) => (None, String::new(), SlotState::Unavailable),
            // Kept, but nobody eligible fills it.
            (Some(_), None) => (None, String::new(), SlotState::FewerSourcesThanSlots),
            (Some(_), Some(edge)) => {
                let named = Some(u32::from(edge.source.get().get()));
                let publishes_video = table
                    .publishes_of(edge.source)
                    .is_some_and(|p| p.publishes(MediaKindLabel::Video));
                if kind == MediaKindLabel::Video && !publishes_video {
                    // No-video precedence: no edge, so no url.
                    (named, String::new(), SlotState::AvatarNoVideo)
                } else {
                    let url = handlers
                        .url_of(&edge.handler)
                        .ok_or(DirectiveOutcome::HandlerUrlUnresolved)?
                        .to_string();
                    let flags = mute.flags(edge.source);
                    let (server, client) =
                        (*kind.of_ref(&flags.server), *kind.of_ref(&flags.client));
                    if server {
                        (named, url, SlotState::AvatarServerMuted)
                    } else if client {
                        (named, url, SlotState::AvatarClientMuted)
                    } else if kind == MediaKindLabel::Video {
                        // R-5: MC never names a live video source; the
                        // verified key-id sender does.
                        (None, url, SlotState::Active)
                    } else {
                        (named, url, SlotState::Active)
                    }
                }
            }
        };

        slot_states.push((kind, slot_state));
        assignments.push(StreamAssignment {
            slot_id: u32::from(slot.slot_id.get()),
            sender_id,
            media_kind: slot.media_kind as i32,
            media_handler_url,
            slot_state: slot_state as i32,
            // §7 switching is not in this story.
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
    use crate::media_kind::PublishSet;
    use crate::media_routing::SourceMute;
    use crate::media_routing::{HandlerEndpoint, HandlerId};
    use crate::media_signaling::ReceiveCapabilityDeclaration;
    use proto_gen::dark_tower::signaling::v1::{MediaKind, ReceiveCapability, ReceiveSlot};
    use std::collections::HashMap;
    use std::num::NonZeroU16;

    const AU: MediaKindLabel = MediaKindLabel::Audio;
    const VI: MediaKindLabel = MediaKindLabel::Video;

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

    fn demand(slots: &[(u32, MediaKind)], caps: [u8; 2]) -> SlotDemand {
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
        ReceiveCapabilityDeclaration::parse(&message, true)
            .expect("fixture declaration is valid")
            .truncate(caps)
    }

    fn publishing(s: &str) -> PublishSet {
        let mut k = Vec::new();
        if s.contains('A') {
            k.push(AU);
        }
        if s.contains('V') {
            k.push(VI);
        }
        PublishSet::of(&k)
    }

    /// Subscriber 1 (AV) then `others` (each with its publish set), all on
    /// `mh-0`; subscriber 1 declares `slots` truncated to `caps`.
    fn compose_with(
        others: &[(u16, &str)],
        slots: &[(u32, MediaKind)],
        caps: [u8; 2],
        overrides: &[(u16, SourceMute)],
    ) -> SlotComposition {
        let set = handler_set(&["mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on_publishing(sender(1), publishing("AV"), &set, &["mh-0"]);
        for (s, p) in others {
            table.admit_on_publishing(sender(*s), publishing(p), &set, &["mh-0"]);
        }
        let d = demand(slots, caps);
        table.set_demand(sender(1), [d.kept_slot_ids(AU), d.kept_slot_ids(VI)]);
        // Everyone present and unmuted, then the overrides.
        let mut map: HashMap<SenderId, SourceMute> = [1u16]
            .iter()
            .chain(others.iter().map(|(s, _)| s))
            .map(|s| (sender(*s), SourceMute::default()))
            .collect();
        for (s, m) in overrides {
            map.insert(sender(*s), *m);
        }
        build_stream_assignments(
            sender(1),
            &d,
            &table,
            &set,
            &SourceMuteView::from_map(map),
            &table.unreachable_for(sender(1)),
        )
        .unwrap()
    }

    fn compose(
        others: &[u16],
        slots: &[(u32, MediaKind)],
        overrides: &[(u16, SourceMute)],
    ) -> SlotComposition {
        let others: Vec<(u16, &str)> = others.iter().map(|s| (*s, "A")).collect();
        compose_with(&others, slots, [8, 4], overrides)
    }

    fn muted(s: u16, server: [bool; 2], client: [bool; 2]) -> Vec<(u16, SourceMute)> {
        vec![(s, SourceMute { server, client })]
    }

    fn state(a: &StreamAssignment) -> SlotState {
        SlotState::try_from(a.slot_state).unwrap()
    }

    #[test]
    fn a_filled_audio_slot_names_its_source_and_is_active() {
        let c = compose(&[2], &[(0, MediaKind::Audio)], &[]);
        let a = &c.assignments.assignments[0];
        assert_eq!(a.slot_id, 0);
        assert_eq!(a.sender_id, Some(2));
        assert_eq!(a.media_kind, MediaKind::Audio as i32);
        assert_eq!(a.media_handler_url, "https://mh-0.example:4434");
        assert_eq!(state(a), SlotState::Active);
        assert!(a.switch_command_id.is_none());
    }

    /// S8: a live VIDEO slot names no sender — and carries nothing derived
    /// from it — while live AUDIO keeps the sender.
    #[test]
    fn live_video_names_no_sender_and_live_audio_does() {
        let c = compose_with(
            &[(2, "AV")],
            &[(0, MediaKind::Audio), (1, MediaKind::VideoCamera)],
            [8, 4],
            &[],
        );
        let [audio, video] = [&c.assignments.assignments[0], &c.assignments.assignments[1]];
        assert_eq!(
            (state(audio), audio.sender_id),
            (SlotState::Active, Some(2))
        );
        assert_eq!(state(video), SlotState::Active);
        assert_eq!(video.sender_id, None);
        assert_eq!(video.slot_id, 1, "only the subscriber's own id is echoed");
        assert!(video.switch_command_id.is_none());
        assert_eq!(video.media_handler_url, "https://mh-0.example:4434");
        assert_eq!(video.media_kind, MediaKind::VideoCamera as i32);
    }

    /// The table, row by row, for both kinds — including server-wins and
    /// no-video precedence, and which states carry a url.
    #[test]
    fn slot_state_mapping_for_both_kinds() {
        let slots = [(0, MediaKind::Audio), (1, MediaKind::VideoCamera)];
        let case = |publish: &str, server: [bool; 2], client: [bool; 2]| {
            let c = compose_with(&[(2, publish)], &slots, [8, 4], &muted(2, server, client));
            c.assignments
                .assignments
                .iter()
                .map(|a| (state(a), a.sender_id, !a.media_handler_url.is_empty()))
                .collect::<Vec<_>>()
        };
        use SlotState::*;
        // Client mute of both kinds: named avatars, url kept.
        assert_eq!(
            case("AV", [false; 2], [true; 2]),
            vec![
                (AvatarClientMuted, Some(2), true),
                (AvatarClientMuted, Some(2), true)
            ]
        );
        // Server wins over client.
        assert_eq!(
            case("AV", [true; 2], [true; 2]),
            vec![
                (AvatarServerMuted, Some(2), true),
                (AvatarServerMuted, Some(2), true)
            ]
        );
        // Kinds are independent: video-only server mute leaves audio active.
        assert_eq!(
            case("AV", [false, true], [false; 2]),
            vec![(Active, Some(2), true), (AvatarServerMuted, Some(2), true)]
        );
        // No-video precedence: whatever the mutes, a non-video occupant of a
        // video slot is AVATAR_NO_VIDEO, named, with no url.
        for (server, client) in [([false; 2], [false; 2]), ([true; 2], [true; 2])] {
            assert_eq!(
                case("A", server, client)[1],
                (AvatarNoVideo, Some(2), false)
            );
        }
        // A receive-only participant: no audio representation, a no-video
        // avatar in the video slot.
        assert_eq!(
            case("", [false; 2], [false; 2]),
            vec![
                (FewerSourcesThanSlots, None, false),
                (AvatarNoVideo, Some(2), false)
            ]
        );
    }

    /// R-1: an over-cap slot is UNAVAILABLE — declared id echoed, no sender,
    /// empty url — and the kept slots fill normally.
    #[test]
    fn truncated_slots_are_unavailable_with_their_ids_echoed() {
        let c = compose_with(
            &[(2, "AV"), (3, "AV")],
            &[
                (5, MediaKind::VideoCamera),
                (6, MediaKind::VideoCamera),
                (7, MediaKind::Audio),
            ],
            [8, 1],
            &[],
        );
        let got: Vec<(u32, SlotState, Option<u32>, bool)> = c
            .assignments
            .assignments
            .iter()
            .map(|a| {
                (
                    a.slot_id,
                    state(a),
                    a.sender_id,
                    a.media_handler_url.is_empty(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                (5, SlotState::Active, None, false),
                (6, SlotState::Unavailable, None, true),
                (7, SlotState::Active, Some(2), false),
            ]
        );
    }

    #[test]
    fn a_solo_subscribers_slots_are_all_fewer_sources_than_slots() {
        let c = compose(
            &[],
            &[(0, MediaKind::Audio), (1, MediaKind::VideoCamera)],
            &[],
        );
        for a in &c.assignments.assignments {
            assert_eq!(state(a), SlotState::FewerSourcesThanSlots);
            assert!(a.sender_id.is_none());
            assert!(a.media_handler_url.is_empty());
        }
    }

    #[test]
    fn nothing_emits_source_muted() {
        for (server, client) in [([true; 2], [false; 2]), ([false; 2], [true; 2])] {
            let c = compose_with(
                &[(2, "AV"), (3, "A")],
                &[
                    (0, MediaKind::Audio),
                    (1, MediaKind::VideoCamera),
                    (2, MediaKind::VideoCamera),
                ],
                [8, 4],
                &muted(2, server, client),
            );
            assert!(c
                .slot_states
                .iter()
                .all(|(_, s)| *s != SlotState::SourceMuted));
        }
    }

    #[test]
    fn the_subscribers_own_slot_numbering_is_echoed_not_mcs() {
        let c = compose(
            &[2, 3],
            &[(9, MediaKind::Audio), (7, MediaKind::Audio)],
            &[],
        );
        let ids: Vec<u32> = c
            .assignments
            .assignments
            .iter()
            .map(|a| a.slot_id)
            .collect();
        assert_eq!(ids, vec![9, 7]);
        let senders: Vec<Option<u32>> = c
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
    fn a_zero_slot_declaration_yields_no_assignments() {
        let c = compose(&[2], &[], &[]);
        assert!(c.assignments.assignments.is_empty());
        assert!(c.slot_states.is_empty());
    }

    /// A filled slot whose handler has no url fails CLOSED.
    #[test]
    fn an_unresolvable_handler_fails_closed_rather_than_emitting_an_empty_url() {
        let set = handler_set(&["mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0"]);
        table.admit_on(sender(2), &set, &["mh-0"]);
        let d = demand(&[(0, MediaKind::Audio)], [8, 4]);
        table.set_demand(sender(1), [d.kept_slot_ids(AU), d.kept_slot_ids(VI)]);
        let outcome = build_stream_assignments(
            sender(1),
            &d,
            &table,
            &handler_set(&["mh-9"]),
            &table.quiet_mute(),
            &[],
        );
        assert_eq!(outcome, Err(DirectiveOutcome::HandlerUrlUnresolved));
    }

    /// R-33: a participant sharing no connected handler consumes no slot and is
    /// named in `unreachable_sender_ids`; one left over by full slots is NOT.
    #[test]
    fn disjoint_peers_are_unreachable_and_sharing_overflow_is_not() {
        let set = handler_set(&["mh-0", "mh-1"]);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0"]);
        table.admit_on(sender(2), &set, &["mh-1"]);
        table.admit_on(sender(3), &set, &["mh-0"]);
        table.admit_on(sender(4), &set, &["mh-0"]);
        let d = demand(&[(0, MediaKind::Audio)], [8, 4]);
        table.set_demand(sender(1), [d.kept_slot_ids(AU), d.kept_slot_ids(VI)]);
        let c = build_stream_assignments(
            sender(1),
            &d,
            &table,
            &set,
            &table.quiet_mute(),
            &table.unreachable_for(sender(1)),
        )
        .unwrap();
        assert_eq!(c.assignments.assignments[0].sender_id, Some(3));
        assert_eq!(c.assignments.unreachable_sender_ids, vec![2]);
    }

    #[test]
    fn slot_states_mirror_the_wire_and_cannot_disagree_with_it() {
        let c = compose_with(
            &[(2, "AV")],
            &[
                (0, MediaKind::Audio),
                (1, MediaKind::VideoCamera),
                (2, MediaKind::Audio),
            ],
            [8, 4],
            &[],
        );
        let from_wire: Vec<(Option<MediaKindLabel>, i32)> = c
            .assignments
            .assignments
            .iter()
            .map(|a| {
                (
                    MediaKind::try_from(a.media_kind)
                        .ok()
                        .and_then(MediaKindLabel::from_proto),
                    a.slot_state,
                )
            })
            .collect();
        let from_metric: Vec<(Option<MediaKindLabel>, i32)> = c
            .slot_states
            .iter()
            .map(|(k, s)| (Some(*k), *s as i32))
            .collect();
        assert_eq!(from_wire, from_metric);
    }

    /// One accessor, one default: a source absent from the projection fails
    /// CLOSED (server-muted), the same answer the render gives MH.
    #[test]
    fn an_unknown_source_fails_closed_as_server_muted() {
        let view = SourceMuteView::from_map(HashMap::from([(
            sender(2),
            SourceMute {
                server: [false; 2],
                client: [true, false],
            },
        )]));
        assert_eq!(
            view.flags(sender(1)),
            SourceMute {
                server: [true; 2],
                client: [false; 2]
            }
        );
        assert_eq!(view.flags(sender(2)).client, [true, false]);
    }

    #[test]
    fn every_wire_slot_state_has_a_distinct_bounded_label() {
        let all = [
            SlotState::Unspecified,
            SlotState::Active,
            SlotState::SourceMuted,
            SlotState::WithheldByCongestion,
            SlotState::FewerSourcesThanSlots,
            SlotState::SourceUnreachable,
            SlotState::SwitchPending,
            SlotState::AvatarNoVideo,
            SlotState::Unavailable,
            SlotState::AvatarClientMuted,
            SlotState::AvatarServerMuted,
        ];
        let labels: std::collections::HashSet<&str> =
            all.iter().map(|s| slot_state_label(*s)).collect();
        assert_eq!(labels.len(), all.len());
        for state in all {
            let wire = state
                .as_str_name()
                .strip_prefix("SLOT_STATE_")
                .unwrap()
                .to_ascii_lowercase();
            assert_eq!(slot_state_label(state), wire, "{state:?}");
        }
    }

    /// The slot's handler address IS the handler carrying this pair, in a
    /// two-handler meeting where everyone sits on mh-0 (written literals).
    #[test]
    fn the_slot_names_the_handler_carrying_this_subscribers_pair_at_n_2() {
        let set = handler_set(&["mh-1", "mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0"]);
        table.admit_on(sender(2), &set, &["mh-0"]);
        let d = demand(&[(0, MediaKind::Audio)], [8, 4]);
        table.set_demand(sender(1), [d.kept_slot_ids(AU), d.kept_slot_ids(VI)]);
        let c = build_stream_assignments(sender(1), &d, &table, &set, &table.quiet_mute(), &[])
            .unwrap();
        let a = &c.assignments.assignments[0];
        assert_eq!(state(a), SlotState::Active);
        assert_eq!(a.media_handler_url, "https://mh-0.example:4434");
        assert_ne!(a.media_handler_url, "https://mh-1.example:4434");
    }
}
