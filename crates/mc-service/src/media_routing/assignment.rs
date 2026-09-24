//! The forwarding assignment's OUTPUT types and wire constants (ADR-0036 §7, §9).
//!
//! Pure data: no clock, no I/O, no Redis, no metrics. The computation that
//! produces a [`MeetingAssignment`] lives in [`super::slots`] — per-meeting
//! join-order slot state held by the meeting actor (story 2, R-2/R-4) — and the
//! caller ships the output. This module owns only *what an assignment is* and
//! the two id-packing rules every producer must share.
//!
//! # There is no single-handler special case
//!
//! ADR-0036 §9: one handler is simply N=1 of the general computation. There is
//! deliberately no `if handlers.len() == 1` in [`super::slots`]; a single-handler
//! meeting and a split meeting run the same fill and the same render.

use crate::media_admission::SenderId;
use media_protocol::frame::KEY_ID_STREAM_BITS;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::BTreeMap;

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
    /// Which of the subscriber's slots this fills: the slot id the subscriber
    /// DECLARED at this stream's ordinal (never fabricated by MC).
    ///
    /// A plain `u16`, deliberately NOT a newtype and deliberately NOT
    /// [`SenderId`]: `internal.proto` bars collapsing the two by name
    /// (`SubscriberSlot.slot_id` is client-chosen and renumbers; `sender_id` is
    /// MC-allocated and never recycles).
    pub slot_id: u16,
    /// The publisher pinned into this slot.
    ///
    /// **Exactly one** under story 2's static join-order fill: MC pins the
    /// source and MH never reselects. A `Vec` because the wire field is
    /// repeated and story 5's MH-side §7 selection returns multi-candidate
    /// streams; a scalar here would have to be widened back then.
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
    /// allots it. Unreachable from real input while `MC_MAX_RECEIVE_SLOTS` is
    /// bounded at 64 (config load rejects more), but the ordinal is
    /// client-sized, so it rejects rather than truncates.
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

/// Pack a subscriber's egress ordinal into a policy-plane stream identity.
///
/// Unique **by construction**:
/// `sender_id` is unique per participant within a meeting and is never
/// recycled (R-35), and the ordinal is the slot's index in that subscriber's
/// slot vector (never an enumeration over FILLED slots), so the pair is unique
/// per meeting and stable across a middle leave or a refill. It is also stable across a client slot renumber, which
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
pub(crate) fn egress_stream_id(
    subscriber: SenderId,
    ordinal: usize,
) -> Result<u32, AssignmentError> {
    let ordinal = u8::try_from(ordinal).map_err(|_| AssignmentError::EgressOrdinalOverflow)?;
    Ok((u32::from(subscriber.get().get()) << 8) | u32::from(ordinal))
}

/// The inverse of [`egress_stream_id`]'s ordinal half. Kept beside the packer
/// so pack and unpack are read and changed together; only the slot-table
/// model check unpacks today.
#[cfg(test)]
pub(crate) fn egress_ordinal(egress_stream_id: u32) -> usize {
    usize::from(egress_stream_id.to_le_bytes()[0])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
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
        for ordinal in [0usize, 1, 255] {
            let id = egress_stream_id(sender(7), ordinal).unwrap();
            assert_eq!(egress_ordinal(id), ordinal);
        }
    }

    /// The 256-ordinal boundary. Unreachable from a real declaration (the
    /// configured slot cap is bounded at 64 at config load), so it is exercised
    /// here, on the one function every producer packs through.
    #[test]
    fn egress_ordinal_above_eight_bits_rejects_rather_than_truncating() {
        assert_eq!(
            egress_stream_id(sender(1), 256),
            Err(AssignmentError::EgressOrdinalOverflow),
        );
    }
}
