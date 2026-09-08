//! The general visibility-graph forwarding computation (ADR-0036 §9).
//!
//! Pure: no clock, no I/O, no Redis, no metrics. The caller builds the input
//! and the caller ships the output; this module only decides *what forwards to
//! whom, on which handler*.
//!
//! # There is no single-handler special case
//!
//! ADR-0036 §9 makes the one-handler loopback the **N=1 evaluation of the
//! general computation**, not a shortcut around it. There is deliberately no
//! `if handlers.len() == 1` anywhere below: the same walk that produces the
//! loopback's single self-edge produces an N-participant, M-handler assignment.
//! A special case here would be a second implementation of the routing rule
//! that only the loopback exercises, and the multi-handler path would first run
//! in production.

use crate::media_admission::SenderId;
use media_protocol::frame::KEY_ID_STREAM_BITS;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::{BTreeMap, HashMap};

/// [`MAIN_AUDIO_STREAM_NUMBER`]'s type **is** its bound, and this assertion is
/// the single-source-of-truth link that makes that true rather than merely
/// claimed.
///
/// `stream_number` rides in the signed SFrame key id's stream field. A value
/// wider than that field would truncate on encode and **alias two distinct
/// streams of one sender**. There is deliberately no local `255`, no `1 << 8`
/// and no width constant in this file, so there is nothing here that can drift
/// from the anchor — if the key-id layout ever reallots the stream field, this
/// fails the build rather than letting MC emit stream numbers that truncate.
///
/// Same discipline, and the same shape, as
/// `media_admission::sender_id`'s `KEY_ID_SENDER_ID_BITS` assertion and
/// `mh-service`'s `StreamNumber` assertion. Contrast [`egress_stream_id`]'s
/// ordinal width, which is an independent policy-plane 8 and is deliberately
/// NOT derived from this constant.
const _: () = {
    assert!(
        KEY_ID_STREAM_BITS == u8::BITS as usize,
        "stream_number is carried in the key id's stream field; MAIN_AUDIO_STREAM_NUMBER must be \
         exactly that width or MC will emit stream numbers that truncate on encode and alias two \
         distinct streams of the same sender"
    );
};

/// MC-assigned forwarding priority for the main audio egress stream
/// (ADR-0036 §7). MC owns the numbering.
///
/// # Why this is a module constant and NOT configuration
///
/// This reads like a config-over-hardcoding violation and is deliberately not
/// one. The priority group is the **bound** on how far a publisher's
/// self-declared salience signal can promote it (§7). Making it an env var or a
/// ConfigMap key hands an operator a lever on forwarding policy: a
/// misconfiguration that flattens every stream into one group removes that
/// bound fleet-wide, and §8's two-ends echo **cannot catch it** — the echo
/// covers transport mode, not priority. §5/§7 bar an operator lever over
/// forwarding policy for exactly this reason, so the value is a compile-time
/// constant that changes only by code review.
///
/// Non-zero on purpose: 0 is proto3's default for `EgressStream.priority_group`,
/// so a non-zero value distinguishes "MC assigned this group" from "the field
/// was never set" on the wire.
pub const AUDIO_PRIORITY_GROUP: u32 = 1;

/// The publisher-side stream number for a participant's main audio.
///
/// SSoT for the value MC ships: story task 14's client-facing send directive
/// consumes this same constant, so the two sides of one concept cannot drift
/// into two literals.
///
/// Its **bound** is anchored — see [`egress_stream_id`] for why the bound and
/// the id-packing ordinal width are two different 8s that must not be collapsed.
pub const MAIN_AUDIO_STREAM_NUMBER: u8 = 0;

/// The subscriber-side slot carrying main audio.
///
/// **PROVISIONAL, and this is the one site that produces it.** ADR-0036 §6 has
/// the client declare the slots it can decode; until that receive-capability
/// declaration lands (story task 14 owns it) MC composes exactly one main-audio
/// slot per subscriber and numbers it 0. When the declaration lands this stops
/// being a constant and becomes a per-subscriber input — an input change, not
/// an algorithm change.
///
/// A plain `u16`, deliberately NOT a newtype and deliberately NOT
/// [`SenderId`]: `internal.proto` bars collapsing the two by name
/// (`SubscriberSlot.slot_id` is client-chosen and renumbers; `sender_id` is
/// MC-allocated and never recycles).
pub const MAIN_AUDIO_SLOT_ID: u16 = 0;

/// A media handler's identity, as MC knows it from the Redis assignment
/// snapshot (`MhEndpointInfo::mh_id`).
///
/// Ordered, because the assignment's canonical ordering — and therefore the
/// structural equality the policy generation derives from — is defined in terms
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandlerId(String);

impl HandlerId {
    /// Wrap a handler identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The underlying identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HandlerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One participant, as the routing computation sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingParticipant {
    /// This participant's per-meeting sender handle.
    pub sender_id: SenderId,
    /// The handlers this participant is connected to.
    ///
    /// **An input, never an assumption.** Today the caller fills this with every
    /// handler assigned to the meeting — **because MC has no per-participant
    /// placement input**, not because a client chose to connect to all of them.
    /// `MhAssignmentData` is a property of the MEETING; nothing on the roster
    /// says which handler a given participant reached. When real per-participant
    /// placement lands it changes what the caller puts here — it does not change
    /// the algorithm below.
    ///
    /// # The directed-handler contract (ADR-0036 §5)
    ///
    /// Do not read the full-list fill as "the client picks a handler". It does
    /// not, and since story task 25 it cannot: MC DIRECTS where a client sends.
    /// [`compute_assignment`] places each edge on exactly one handler
    /// ([`edge_handler`]), and that placement is what the client is steered to —
    /// `SendTarget.media_handler_url` on the send directive and the active slot's
    /// `StreamAssignment.media_handler_url` are both read out of this output.
    /// `JoinResponse.media_servers` is connection bootstrap data and is
    /// explicitly not the selection mechanism.
    ///
    /// One client, one directed handler, in this story. Multi-handler send is
    /// ADR-0036 §9 and a later story; the shape here already supports it because
    /// the walk is general, but nothing today emits more than one target.
    pub handlers: Vec<HandlerId>,
}

/// The meeting state the computation runs over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingRoutingInput {
    /// Every participant currently in the meeting.
    pub participants: Vec<RoutingParticipant>,
    /// Every handler assigned to the meeting.
    ///
    /// A handler with no participants still appears here and still receives an
    /// assignment — an empty one. "Forward nothing for this meeting" is a
    /// computed output, never an operator action.
    pub handlers: Vec<HandlerId>,
}

/// One egress stream in a handler's forwarding policy.
///
/// Mirrors `internal.v1.EgressStream` field for field, but is MC's own type:
/// the computation must be testable without constructing prost messages, and
/// the wire encoding belongs to the request builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressStreamPlan {
    /// Policy-plane identity, unique within one meeting registration.
    pub egress_stream_id: u32,
    /// The subscribing participant.
    pub subscriber: SenderId,
    /// Which of the subscriber's slots this fills.
    pub slot_id: u16,
    /// Publishers eligible to fill this stream, sorted by `sender_id`.
    ///
    /// Selection among many candidates is MH's §7 job. The loopback yields
    /// exactly one, which is why this is a list at N=1 rather than a scalar
    /// that a later story would have to widen.
    pub candidate_sources: Vec<SenderId>,
    /// Which of each candidate's streams. Always [`MAIN_AUDIO_STREAM_NUMBER`]
    /// in this story.
    pub stream_number: u8,
    /// MC-assigned forwarding priority ([`AUDIO_PRIORITY_GROUP`]).
    pub priority_group: u32,
    /// Whether a superseding frame permits discarding queued frames.
    ///
    /// **False for audio.** "Every audio frame forwards" is a consequence of
    /// this flag; MH never learns the stream is audio.
    pub supersede_on_independent_frame: bool,
    /// How MH carries this stream to its subscriber.
    pub transport_mode: TransportMode,
}

/// One handler's complete forwarding policy for one meeting.
///
/// Canonically ordered (by `egress_stream_id`), which is what makes structural
/// equality well-defined — and structural equality is what the policy
/// generation derives from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HandlerAssignment {
    /// The egress streams this handler forwards, in canonical order.
    pub egress_streams: Vec<EgressStreamPlan>,
}

/// The meeting's forwarding assignment, split per handler.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MeetingAssignment {
    /// Every handler in the input, including those with an empty policy.
    pub per_handler: BTreeMap<HandlerId, HandlerAssignment>,
}

impl MeetingAssignment {
    /// This handler's policy, or `None` if the handler was not in the input.
    #[must_use]
    pub fn for_handler(&self, handler: &HandlerId) -> Option<&HandlerAssignment> {
        self.per_handler.get(handler)
    }
}

/// The assignment computation could not produce a well-formed policy.
///
/// Bounded on purpose: every variant is a structural impossibility MC must
/// refuse to encode rather than truncate into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentError {
    /// A subscriber's egress ordinal exceeded the 8 bits [`egress_stream_id`]
    /// allots it. Unreachable while there is one slot per subscriber; it
    /// becomes reachable the moment receive-capability declarations land, which
    /// is why it rejects rather than truncates.
    EgressOrdinalOverflow,
}

impl std::fmt::Display for AssignmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EgressOrdinalOverflow => {
                f.write_str("subscriber egress ordinal exceeds the 8-bit egress_stream_id field")
            }
        }
    }
}

impl std::error::Error for AssignmentError {}

impl AssignmentError {
    /// Bounded label for logs and metrics.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::EgressOrdinalOverflow => "egress_ordinal_overflow",
        }
    }
}

/// Does `subscriber` receive `publisher`'s media?
///
/// ADR-0036 §9's visibility rule: a subscriber sees a publisher iff they share
/// at least one handler.
///
/// **Reflexive in this story, and that is the whole of "hear yourself".** S
/// shares every handler with S, so S sees S and the N=1 evaluation yields one
/// self-edge. That is the general rule applied at N=1, not a loopback branch.
///
/// This is one named predicate rather than an inline condition in the graph
/// walk so the future "do not echo yourself once there is a second participant"
/// policy is a change *to a predicate* — one place, one test — instead of a
/// branch grafted into the walk.
fn subscribes_to(subscriber: &RoutingParticipant, publisher: &RoutingParticipant) -> bool {
    subscriber
        .handlers
        .iter()
        .any(|h| publisher.handlers.contains(h))
}

/// Which handler carries the `publisher -> subscriber` edge.
///
/// **Exactly one**, chosen as the first shared handler in sorted order. §9 rests
/// MH-obliviousness on this: an edge carried by two handlers would forward the
/// same media twice, and the deterministic tie-break is what makes recomputing
/// an unchanged meeting produce a byte-identical assignment — which is what the
/// policy generation's change-detection depends on.
fn edge_handler(
    subscriber: &RoutingParticipant,
    publisher: &RoutingParticipant,
) -> Option<HandlerId> {
    let mut shared: Vec<&HandlerId> = subscriber
        .handlers
        .iter()
        .filter(|h| publisher.handlers.contains(h))
        .collect();
    shared.sort();
    shared.first().map(|h| (*h).clone())
}

/// Pack a subscriber's egress ordinal into a policy-plane stream identity.
///
/// Unique **by construction**, not by luck of the loopback's shape:
/// `sender_id` is unique per participant within a meeting and is never
/// recycled (R-35), and the ordinal is unique per subscriber, so the pair is
/// unique per meeting. It is also stable across a client slot renumber, which
/// is precisely why `internal.proto` keeps `egress_stream_id` distinct from
/// `subscriber.slot_id`.
///
/// # The two adjacent 8s, named so they are not collapsed
///
/// This function's ordinal width is **8 bits, and it is an independent
/// policy-plane choice.** It merely *coincides* with
/// `media_protocol::frame::KEY_ID_STREAM_BITS`, and is deliberately NOT derived
/// from it: deriving would couple MC's policy-plane id packing to the SFrame
/// wire layout, so a future key-id reallocation would silently renumber every
/// egress stream identity.
///
/// The **other** 8 in this module — the bound on
/// [`MAIN_AUDIO_STREAM_NUMBER`] — genuinely IS
/// `KEY_ID_STREAM_BITS`: `stream_number` rides in the signed key id's stream
/// field, and a value above it would alias two of one sender's streams. That
/// one is anchored; this one is not. Same number, different reasons, opposite
/// drift obligations.
fn egress_stream_id(subscriber: SenderId, ordinal: usize) -> Result<u32, AssignmentError> {
    let ordinal = u8::try_from(ordinal).map_err(|_| AssignmentError::EgressOrdinalOverflow)?;
    Ok((u32::from(subscriber.get().get()) << 8) | u32::from(ordinal))
}

/// Run the visibility-graph computation over one meeting's state.
///
/// Steps, in order (ADR-0036 §9):
/// 1. **Visibility** — [`subscribes_to`] for every (subscriber, publisher) pair.
/// 2. **Edge placement** — [`edge_handler`] puts each edge on exactly one handler.
/// 3. **Grouping** — per (handler, subscriber), the subscriber's one main-audio
///    slot becomes one [`EgressStreamPlan`] whose candidates are the publishers
///    whose edge landed on that handler, sorted by `sender_id`.
/// 4. **Canonical ordering** — every handler in the input gets an entry (empty
///    if it carries nothing), and each handler's streams are sorted by
///    `egress_stream_id`.
///
/// # Errors
///
/// [`AssignmentError::EgressOrdinalOverflow`] if a subscriber's egress ordinal
/// does not fit the 8 bits [`egress_stream_id`] allots it.
pub fn compute_assignment(
    input: &MeetingRoutingInput,
) -> Result<MeetingAssignment, AssignmentError> {
    // (handler, subscriber) -> candidate publishers.
    //
    // A `HashMap` rather than a `BTreeMap`: [`SenderId`] is deliberately not
    // `Ord` (nothing in the admission path orders sender ids, and giving a
    // never-recycled handle an ordering invites code that infers join order
    // from it). Canonical ordering is imposed at the end by sorting each
    // handler's streams on `egress_stream_id`, which is a total order derived
    // from the same ids — so the output is deterministic without the map being.
    let mut buckets: HashMap<(HandlerId, SenderId), Vec<SenderId>> = HashMap::new();

    for subscriber in &input.participants {
        for publisher in &input.participants {
            if !subscribes_to(subscriber, publisher) {
                continue;
            }
            let Some(handler) = edge_handler(subscriber, publisher) else {
                // `subscribes_to` returned true, so a shared handler exists;
                // this arm is unreachable. Skipping rather than unwrapping
                // keeps the impossible case from becoming a panic in the join
                // path.
                continue;
            };
            buckets
                .entry((handler, subscriber.sender_id))
                .or_default()
                .push(publisher.sender_id);
        }
    }

    // Every handler in the input gets an entry, so "this handler forwards
    // nothing" is an assignment MC pushes rather than a handler MC skips.
    let mut per_handler: BTreeMap<HandlerId, HandlerAssignment> = input
        .handlers
        .iter()
        .map(|h| (h.clone(), HandlerAssignment::default()))
        .collect();

    for ((handler, subscriber), mut candidates) in buckets {
        candidates.sort_by_key(|s| s.get());
        // One main-audio slot per subscriber, so ordinal 0. `enumerate`-shaped
        // rather than a literal because the receive-capability declaration
        // (task 14) turns this into a loop over the subscriber's slots.
        let ordinal = 0usize;
        let plan = EgressStreamPlan {
            egress_stream_id: egress_stream_id(subscriber, ordinal)?,
            subscriber,
            slot_id: MAIN_AUDIO_SLOT_ID,
            candidate_sources: candidates,
            stream_number: MAIN_AUDIO_STREAM_NUMBER,
            priority_group: AUDIO_PRIORITY_GROUP,
            // False for audio: every frame forwards. MH is told the behaviour,
            // never the media type.
            supersede_on_independent_frame: false,
            // ADR-0036 §1: audio rides datagrams. The signalling enum,
            // imported — there is deliberately no parallel `internal.v1`
            // spelling and no re-mapping table, so MC and MH cannot disagree
            // about mode while §8's echo reads green.
            transport_mode: TransportMode::Datagram,
        };
        per_handler
            .entry(handler)
            .or_default()
            .egress_streams
            .push(plan);
    }

    for assignment in per_handler.values_mut() {
        assignment
            .egress_streams
            .sort_by_key(|s| s.egress_stream_id);
    }

    Ok(MeetingAssignment { per_handler })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn handler(name: &str) -> HandlerId {
        HandlerId::new(name)
    }

    fn participant(n: u16, handlers: &[&str]) -> RoutingParticipant {
        RoutingParticipant {
            sender_id: sender(n),
            handlers: handlers.iter().map(|h| handler(h)).collect(),
        }
    }

    /// The loopback: the N=1 evaluation of the general rule, not a special case.
    #[test]
    fn n1_loopback_yields_one_self_edge_with_one_audio_egress() {
        let input = MeetingRoutingInput {
            participants: vec![participant(7, &["mh-0"])],
            handlers: vec![handler("mh-0")],
        };

        let assignment = compute_assignment(&input).unwrap();
        assert_eq!(assignment.per_handler.len(), 1);

        let policy = assignment.for_handler(&handler("mh-0")).unwrap();
        assert_eq!(policy.egress_streams.len(), 1, "exactly one egress stream");

        let stream = &policy.egress_streams[0];
        assert_eq!(stream.subscriber, sender(7));
        assert_eq!(
            stream.candidate_sources,
            vec![sender(7)],
            "self->self: the publisher and the subscriber are the same participant"
        );
        assert_eq!(stream.slot_id, MAIN_AUDIO_SLOT_ID);
        assert_eq!(stream.stream_number, MAIN_AUDIO_STREAM_NUMBER);
        assert_eq!(stream.priority_group, AUDIO_PRIORITY_GROUP);
        assert!(
            !stream.supersede_on_independent_frame,
            "audio forwards every frame; this flag is what makes that true"
        );
        assert_eq!(stream.transport_mode, TransportMode::Datagram);
    }

    /// N=2 on one handler: the same walk yields the full 2x2 graph including
    /// both self-edges — evidence that N=1 took no shortcut.
    #[test]
    fn n2_on_one_handler_yields_four_edges_across_two_egress_streams() {
        let input = MeetingRoutingInput {
            participants: vec![participant(1, &["mh-0"]), participant(2, &["mh-0"])],
            handlers: vec![handler("mh-0")],
        };

        let policy = compute_assignment(&input)
            .unwrap()
            .for_handler(&handler("mh-0"))
            .cloned()
            .unwrap();

        assert_eq!(policy.egress_streams.len(), 2, "one slot per subscriber");
        let total_edges: usize = policy
            .egress_streams
            .iter()
            .map(|s| s.candidate_sources.len())
            .sum();
        assert_eq!(total_edges, 4, "2x2 including both self-edges");

        for stream in &policy.egress_streams {
            assert_eq!(
                stream.candidate_sources,
                vec![sender(1), sender(2)],
                "candidates sorted by sender_id"
            );
        }
    }

    /// Two handlers with disjoint membership: each handler receives only its own
    /// edges, and every edge lands on exactly one handler.
    #[test]
    fn disjoint_handler_membership_places_every_edge_on_exactly_one_handler() {
        let input = MeetingRoutingInput {
            participants: vec![participant(1, &["mh-0"]), participant(2, &["mh-1"])],
            handlers: vec![handler("mh-0"), handler("mh-1")],
        };

        let assignment = compute_assignment(&input).unwrap();
        let a = assignment.for_handler(&handler("mh-0")).unwrap();
        let b = assignment.for_handler(&handler("mh-1")).unwrap();

        assert_eq!(a.egress_streams.len(), 1);
        assert_eq!(a.egress_streams[0].subscriber, sender(1));
        assert_eq!(a.egress_streams[0].candidate_sources, vec![sender(1)]);

        assert_eq!(b.egress_streams.len(), 1);
        assert_eq!(b.egress_streams[0].subscriber, sender(2));
        assert_eq!(b.egress_streams[0].candidate_sources, vec![sender(2)]);

        // No participant sees the other: disjoint handler sets, no shared handler.
        let total: usize = [a, b]
            .iter()
            .flat_map(|p| p.egress_streams.iter())
            .map(|s| s.candidate_sources.len())
            .sum();
        assert_eq!(total, 2);
    }

    /// Overlapping membership: the shared handler carries the cross edges, and
    /// the deterministic tie-break puts each edge on exactly one of them.
    #[test]
    fn overlapping_handler_membership_tie_breaks_to_the_first_shared_handler() {
        let input = MeetingRoutingInput {
            participants: vec![
                participant(1, &["mh-0", "mh-1"]),
                participant(2, &["mh-0", "mh-1"]),
            ],
            handlers: vec![handler("mh-0"), handler("mh-1")],
        };

        let assignment = compute_assignment(&input).unwrap();
        let a = assignment.for_handler(&handler("mh-0")).unwrap();
        let b = assignment.for_handler(&handler("mh-1")).unwrap();

        assert_eq!(a.egress_streams.len(), 2, "sorted tie-break picks mh-0");
        assert!(
            b.egress_streams.is_empty(),
            "mh-1 carries nothing; no edge is placed twice"
        );
    }

    /// "Send nothing" is an OUTPUT of the computation, reachable only as a code
    /// path — never as an operator action or a configuration value.
    #[test]
    fn handler_with_no_participants_gets_an_empty_but_present_assignment() {
        let input = MeetingRoutingInput {
            participants: vec![participant(1, &["mh-0"])],
            handlers: vec![handler("mh-0"), handler("mh-1")],
        };

        let assignment = compute_assignment(&input).unwrap();
        let empty = assignment
            .for_handler(&handler("mh-1"))
            .expect("an idle handler still receives an assignment");
        assert!(empty.egress_streams.is_empty());
    }

    #[test]
    fn empty_meeting_yields_an_empty_assignment_per_handler() {
        let input = MeetingRoutingInput {
            participants: Vec::new(),
            handlers: vec![handler("mh-0")],
        };
        let assignment = compute_assignment(&input).unwrap();
        assert!(assignment
            .for_handler(&handler("mh-0"))
            .unwrap()
            .egress_streams
            .is_empty());
    }

    /// Both uniqueness obligations `internal.proto` places on one registration.
    #[test]
    fn egress_stream_id_and_subscriber_slot_are_unique_by_construction() {
        let input = MeetingRoutingInput {
            participants: (1..=50).map(|n| participant(n, &["mh-0"])).collect(),
            handlers: vec![handler("mh-0")],
        };

        let policy = compute_assignment(&input)
            .unwrap()
            .for_handler(&handler("mh-0"))
            .cloned()
            .unwrap();

        let ids: HashSet<u32> = policy
            .egress_streams
            .iter()
            .map(|s| s.egress_stream_id)
            .collect();
        assert_eq!(
            ids.len(),
            policy.egress_streams.len(),
            "egress_stream_id unique"
        );

        let slots: HashSet<(u16, u16)> = policy
            .egress_streams
            .iter()
            .map(|s| (s.subscriber.get().get(), s.slot_id))
            .collect();
        assert_eq!(
            slots.len(),
            policy.egress_streams.len(),
            "(sender_id, slot_id) unique"
        );
    }

    /// Canonical ordering: input order must not change the output, or the
    /// generation's structural equality would fire on a reshuffle.
    #[test]
    fn canonical_ordering_is_stable_under_input_permutation() {
        let forward = MeetingRoutingInput {
            participants: vec![
                participant(3, &["mh-0"]),
                participant(1, &["mh-0"]),
                participant(2, &["mh-0"]),
            ],
            handlers: vec![handler("mh-0")],
        };
        let reversed = MeetingRoutingInput {
            participants: vec![
                participant(2, &["mh-0"]),
                participant(1, &["mh-0"]),
                participant(3, &["mh-0"]),
            ],
            handlers: vec![handler("mh-0")],
        };

        assert_eq!(
            compute_assignment(&forward).unwrap(),
            compute_assignment(&reversed).unwrap(),
        );
    }

    #[test]
    fn egress_stream_id_packs_sender_and_ordinal_without_collision() {
        assert_eq!(egress_stream_id(sender(1), 0).unwrap(), 0x0000_0100);
        assert_eq!(egress_stream_id(sender(1), 1).unwrap(), 0x0000_0101);
        assert_eq!(egress_stream_id(sender(2), 0).unwrap(), 0x0000_0200);
        assert_eq!(
            egress_stream_id(sender(u16::MAX), 255).unwrap(),
            0x00FF_FFFF
        );
    }

    #[test]
    fn egress_ordinal_above_eight_bits_rejects_rather_than_truncating() {
        assert_eq!(
            egress_stream_id(sender(1), 256),
            Err(AssignmentError::EgressOrdinalOverflow),
        );
    }

    /// The visibility predicate is reflexive, which is what makes "hear
    /// yourself" fall out of the general rule.
    #[test]
    fn visibility_is_reflexive() {
        let p = participant(1, &["mh-0"]);
        assert!(subscribes_to(&p, &p));
    }

    #[test]
    fn visibility_is_false_across_disjoint_handler_sets() {
        let a = participant(1, &["mh-0"]);
        let b = participant(2, &["mh-1"]);
        assert!(!subscribes_to(&a, &b));
        assert!(!subscribes_to(&b, &a));
    }
}
