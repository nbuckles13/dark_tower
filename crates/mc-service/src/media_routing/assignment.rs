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
use crate::media_kind::MediaKindLabel;
use media_protocol::frame::KEY_ID_STREAM_BITS;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::{BTreeMap, BTreeSet};

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

/// The publisher-side stream number for a participant's main (camera) video.
///
/// A CONTRACT constant, not config, exactly as [`MAIN_AUDIO_STREAM_NUMBER`]:
/// MC's send id IS the uplink relay `stream_id` the publisher stamps, and MH
/// attributes every ingress frame by (connection-bound sender, that id)
/// against `source_streams` (story 3 R-20). The pre-story SDK stamps 0 in
/// practice, which is why audio must stay 0 and video takes 1. Pinned in the
/// directive AND `source_streams` tests.
pub const MAIN_VIDEO_STREAM_NUMBER: u8 = 1;

/// MC-assigned forwarding priority for the main video egress stream
/// (ADR-0036 §7; story 3 R-20). A LOWER number is a HIGHER priority, so video
/// ranks below audio ([`AUDIO_PRIORITY_GROUP`]) and MH stays type-blind: it
/// learns the order, never the kind. A compile-time constant for the same
/// reason as [`AUDIO_PRIORITY_GROUP`].
pub const VIDEO_PRIORITY_GROUP: u32 = 2;

// Video ranks BELOW audio: a lower number is a higher priority. A build
// failure, not a test, if the two are ever reordered.
const _: () = assert!(AUDIO_PRIORITY_GROUP < VIDEO_PRIORITY_GROUP);

/// The stream number MC directs and declares for a kind. The ONE kind ->
/// stream mapping; an edge's stream derives from its SLOT's kind through here,
/// never from a candidate's publish set.
#[must_use]
pub const fn stream_number_of(kind: MediaKindLabel) -> u8 {
    match kind {
        MediaKindLabel::Audio => MAIN_AUDIO_STREAM_NUMBER,
        MediaKindLabel::Video => MAIN_VIDEO_STREAM_NUMBER,
    }
}

/// The transport a kind's stream travels on (ADR-0036 §1): audio on
/// datagrams, video stream-per-group. The ONE kind -> transport mapping, read
/// by both the `source_streams` declaration and the send directive's targets.
#[must_use]
pub const fn transport_of(kind: MediaKindLabel) -> TransportMode {
    match kind {
        MediaKindLabel::Audio => TransportMode::Datagram,
        MediaKindLabel::Video => TransportMode::StreamPerGroup,
    }
}

/// The forwarding priority group for a kind's stream.
#[must_use]
pub const fn priority_group_of(kind: MediaKindLabel) -> u32 {
    match kind {
        MediaKindLabel::Audio => AUDIO_PRIORITY_GROUP,
        MediaKindLabel::Video => VIDEO_PRIORITY_GROUP,
    }
}

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
    /// Which of each candidate's streams: [`stream_number_of`] the SLOT's
    /// kind.
    pub stream_number: u8,
    /// MC-assigned forwarding priority ([`priority_group_of`] the slot's kind).
    pub priority_group: u32,
    /// Whether a superseding frame permits discarding queued frames.
    ///
    /// **False for audio** ("every audio frame forwards"), **true for video**
    /// (a new group supersedes a stale one). MH never learns which is which.
    pub supersede_on_independent_frame: bool,
}

/// One stream a publisher may send to a handler, as MC declares it
/// (`internal.v1.SourceStream`; story 3 R-16, R-17, R-20).
///
/// Ordered by (sender, stream) so a handler's set is canonical and structural
/// equality — what the policy generation derives from — is well-defined. A
/// set, so a duplicate (sender, stream), which MH hard-rejects, is
/// unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceStreamPlan {
    /// The publisher.
    pub sender: SenderId,
    /// Its stream: MC's send id and the uplink relay id.
    pub stream_number: u8,
    /// The transport the stream arrives on and its edges leave on
    /// ([`transport_of`] its kind).
    pub transport_mode: TransportModeOrd,
    /// What MH does with it.
    pub disposition: SourceDispositionPlan,
}

/// [`TransportMode`] with an ordering, so [`SourceStreamPlan`] can live in a
/// `BTreeSet`. The proto enum derives no `Ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransportModeOrd(i32);

impl TransportModeOrd {
    /// Wrap a proto transport mode.
    #[must_use]
    pub fn new(mode: TransportMode) -> Self {
        Self(mode as i32)
    }

    /// The proto value (an unknown i32 reads as `Unspecified`).
    #[must_use]
    pub fn mode(self) -> TransportMode {
        TransportMode::try_from(self.0).unwrap_or(TransportMode::Unspecified)
    }
}

/// What MH does with one declared source stream (`internal.v1.SourceDisposition`).
///
/// Behaviour names: MH learns neither the kind nor the moderation fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDispositionPlan {
    /// Forward along the stream's edges.
    Forward,
    /// Client-muted: forward nothing new, never drop at ingress (R-17).
    Paused,
    /// Server-muted: drop at ingress (R-16). Wins over client mute.
    DropAtIngress,
}

impl SourceDispositionPlan {
    /// The disposition for a stream whose kind is server- and/or client-muted.
    /// Server wins: a server-muted stream is dropped whatever the client says.
    #[must_use]
    pub fn from_mute(server_muted: bool, client_muted: bool) -> Self {
        if server_muted {
            Self::DropAtIngress
        } else if client_muted {
            Self::Paused
        } else {
            Self::Forward
        }
    }
}

/// One handler's complete forwarding policy for one meeting.
///
/// Canonically ordered (by `egress_stream_id`, and the muted set by sender id),
/// which is what makes structural equality well-defined — and structural
/// equality is what the policy generation derives from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HandlerAssignment {
    /// The egress streams this handler forwards, in canonical order.
    pub egress_streams: Vec<EgressStreamPlan>,
    /// The server-muted senders THIS handler must drop at ingress
    /// (`RegisterMeetingRequest.server_muted_sources`, ADR-0036 §7; story 2 R-9).
    ///
    /// # A field on the snapshot, not a sibling channel
    ///
    /// Living here is what gives a mute every property the edges already have,
    /// with no second mechanism: it rides EVERY full registration (so it
    /// survives a re-assert and an MH restart), it is swapped atomically with
    /// the edges, and a change to it is a change to this struct — so
    /// `PolicyGenerations::next_generation` advances the generation on exactly
    /// the handlers whose set changed, with no second "did the mute change"
    /// decider anywhere. An unchanged set carries an unchanged generation.
    ///
    /// # Membership: filtered by EDGE OWNERSHIP, never by connectivity
    ///
    /// A sender is in H's set iff it is server-muted AND is the source of at
    /// least one edge this render places on H — see
    /// [`super::slots::SlotTable::render`], the one place the set is built. MH
    /// enforces at ingress and never forwards across handlers, so a muted
    /// sender with no edge on H forwards nothing there anyway, and gains its
    /// mute on H in the very snapshot that gives it an edge there.
    ///
    /// That rule also bounds the set: `|set| <= |egress_streams|`, because
    /// each member is the source of at least one of this handler's egress
    /// streams AND each egress stream carries exactly ONE candidate source
    /// (the `SlotTable::render` construction), so each stream contributes at
    /// most one member. That second premise is load-bearing: MH accepts up to
    /// `MAX_CANDIDATE_SOURCES_PER_EGRESS_CEILING` candidates per stream, and a
    /// multi-candidate render (story 3 selection) voids this bound, which must
    /// then be re-derived. `egress_streams` is bounded by
    /// `MH_MAX_EGRESS_STREAMS_PER_MEETING`, which MH's boot refuses to exceed
    /// `MH_MAX_SOURCE_STREAMS_PER_MEETING` — the bound MH applies to this set
    /// (`crates/mh-service/src/config.rs`).
    /// So a host muting every participant cannot make MC emit a registration
    /// MH rejects WHOLE — which would freeze the meeting's forwarding at its
    /// previous policy.
    ///
    /// A set, never a `Vec`: MH rejects the whole registration on a duplicate
    /// sender id (a duplicate means MC's set construction is broken), so a
    /// duplicate must be unrepresentable rather than merely unlikely.
    ///
    /// # Server mute ONLY — never self-mute
    ///
    /// Deliberately NEVER built from client-mute flags. A self-muted
    /// participant in MH's ingress-drop set would be muted by the server in a
    /// way they could never lift by unmuting — and their capture is already
    /// stopped on the client anyway (ADR-0036 §5). Client mute reaches MH only
    /// as a PAUSED disposition in [`Self::source_streams`]. SUPERSEDED by
    /// `source_streams`; MC-B deletes this field.
    pub server_muted_sources: BTreeSet<SenderId>,
    /// Every (sender, stream) in EVERY member's publish set, with its
    /// transport and disposition (`RegisterMeetingRequest.source_streams`;
    /// story 3). A superset of every directed stream, so nothing MC directs is
    /// ever unmatched at MH; and a participant that did not declare video has
    /// NO stream-1 entry, so its stream-1 frames are `unmatched_stream` at MH —
    /// what makes the publish set "fixed for the membership" a real control.
    /// See [`super::slots::SlotTable::render`], the one place it is built.
    pub source_streams: BTreeSet<SourceStreamPlan>,
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
    /// allots it. Unreachable from real input while each per-kind cap
    /// (`MC_MAX_AUDIO_RECEIVE_SLOTS`, `MC_MAX_VIDEO_RECEIVE_SLOTS`) is bounded at
    /// 64 (config load rejects more) and truncation keeps at most the cap, but
    /// it rejects rather than truncates.
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

/// Bit position of the slot KIND in [`egress_stream_id`] (story 3).
const EGRESS_KIND_SHIFT: u32 = 24;
/// Bit position of the subscriber's sender id in [`egress_stream_id`].
const EGRESS_SENDER_SHIFT: u32 = 8;

/// Pack a subscriber's (kind, egress ordinal) into a policy-plane stream
/// identity.
///
/// # Layout
///
/// | bits | field |
/// |---|---|
/// | 0-7 | the slot's ordinal in the subscriber's slot vector OF THAT KIND |
/// | 8-23 | the subscriber's `sender_id` (16 bits) |
/// | 24 | the slot kind: 0 audio, 1 video |
///
/// Audio ids are byte-identical to story 2's (kind bit 0), so a meeting with
/// no video renders exactly as before. Unique **by construction** — injective
/// over (subscriber, kind, ordinal): `sender_id` is unique per participant and
/// never recycled (R-35), the ordinal is the slot's index in that subscriber's
/// per-kind vector (never an enumeration over FILLED slots), and the kind bit
/// keeps one subscriber's audio and video edges apart. Stable across a middle
/// leave, a refill, an edge moving between handlers, and a client slot
/// renumber — which is why `internal.proto` keeps `egress_stream_id` distinct
/// from `subscriber.slot_id`.
///
/// # The two adjacent 8s, named so they are not collapsed
///
/// The ordinal width is **8 bits, an independent policy-plane choice.** It
/// merely *coincides* with `media_protocol::frame::KEY_ID_STREAM_BITS` and is
/// deliberately NOT derived from it. The **other** 8 in this module — the bound
/// on [`MAIN_AUDIO_STREAM_NUMBER`] — genuinely IS `KEY_ID_STREAM_BITS`. Same
/// number, different reasons, opposite drift obligations.
pub(crate) fn egress_stream_id(
    subscriber: SenderId,
    kind: MediaKindLabel,
    ordinal: usize,
) -> Result<u32, AssignmentError> {
    let ordinal = u8::try_from(ordinal).map_err(|_| AssignmentError::EgressOrdinalOverflow)?;
    let kind_bit = u32::from(kind == MediaKindLabel::Video);
    Ok((kind_bit << EGRESS_KIND_SHIFT)
        | (u32::from(subscriber.get().get()) << EGRESS_SENDER_SHIFT)
        | u32::from(ordinal))
}

/// The inverse of [`egress_stream_id`]'s (kind, ordinal) half. Kept beside the
/// packer so pack and unpack are read and changed together.
#[cfg(test)]
pub(crate) fn egress_kind_and_ordinal(egress_stream_id: u32) -> (MediaKindLabel, usize) {
    let kind = if (egress_stream_id >> EGRESS_KIND_SHIFT) & 1 == 1 {
        MediaKindLabel::Video
    } else {
        MediaKindLabel::Audio
    };
    (kind, usize::from(egress_stream_id.to_le_bytes()[0]))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    const A: MediaKindLabel = MediaKindLabel::Audio;
    const V: MediaKindLabel = MediaKindLabel::Video;

    #[test]
    fn egress_stream_id_packs_sender_kind_and_ordinal_without_collision() {
        // Audio is byte-identical to story 2's layout.
        assert_eq!(egress_stream_id(sender(1), A, 0).unwrap(), 0x0000_0100);
        assert_eq!(egress_stream_id(sender(1), A, 1).unwrap(), 0x0000_0101);
        assert_eq!(egress_stream_id(sender(2), A, 0).unwrap(), 0x0000_0200);
        assert_eq!(egress_stream_id(sender(1), V, 0).unwrap(), 0x0100_0100);
        assert_eq!(
            egress_stream_id(sender(u16::MAX), V, 255).unwrap(),
            0x01FF_FFFF
        );
        for kind in MediaKindLabel::ALL {
            for ordinal in [0usize, 1, 255] {
                let id = egress_stream_id(sender(7), kind, ordinal).unwrap();
                assert_eq!(egress_kind_and_ordinal(id), (kind, ordinal));
            }
        }
    }

    /// Injective over (subscriber, kind, ordinal): one subscriber's audio and
    /// video edges never collide, nor two subscribers'.
    #[test]
    fn egress_stream_id_is_free_of_cross_kind_and_cross_subscriber_collisions() {
        let mut seen = std::collections::HashSet::new();
        for s in [1u16, 2, 255, 256, 257, u16::MAX] {
            for kind in MediaKindLabel::ALL {
                for ordinal in 0..=64usize {
                    assert!(
                        seen.insert(egress_stream_id(sender(s), kind, ordinal).unwrap()),
                        "collision at ({s}, {kind:?}, {ordinal})"
                    );
                }
            }
        }
    }

    #[test]
    fn stream_numbers_are_pinned_audio_zero_video_one() {
        assert_eq!(MAIN_AUDIO_STREAM_NUMBER, 0);
        assert_eq!(MAIN_VIDEO_STREAM_NUMBER, 1);
        assert_eq!(stream_number_of(A), 0);
        assert_eq!(stream_number_of(V), 1);
        assert_eq!(transport_of(A), TransportMode::Datagram);
        assert_eq!(transport_of(V), TransportMode::StreamPerGroup);
    }

    #[test]
    fn server_mute_wins_over_client_mute_in_the_disposition() {
        use SourceDispositionPlan::*;
        assert_eq!(SourceDispositionPlan::from_mute(false, false), Forward);
        assert_eq!(SourceDispositionPlan::from_mute(false, true), Paused);
        assert_eq!(SourceDispositionPlan::from_mute(true, false), DropAtIngress);
        assert_eq!(SourceDispositionPlan::from_mute(true, true), DropAtIngress);
    }

    /// The 256-ordinal boundary. Unreachable from a real declaration (the
    /// configured slot cap is bounded at 64 at config load), so it is exercised
    /// here, on the one function every producer packs through.
    #[test]
    fn egress_ordinal_above_eight_bits_rejects_rather_than_truncating() {
        assert_eq!(
            egress_stream_id(sender(1), A, 256),
            Err(AssignmentError::EgressOrdinalOverflow),
        );
    }
}
