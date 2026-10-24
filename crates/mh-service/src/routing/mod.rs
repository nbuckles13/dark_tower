//! Forwarding-policy domain types and the lock-free routing snapshot.
//!
//! This module owns the ADR-0036 §8 control plane's *data model*: the validated
//! shape MC's `RegisterMeetingRequest` parses into, and the snapshot the data
//! plane reads on every frame.
//!
//! # The one discipline this module exists to enforce
//!
//! **Every identifier on the MC→MH contract is an ordinal scoped to an
//! enclosing entity, and MH must be structurally incapable of resolving one
//! outside its scope.**
//!
//! | Identifier | Enclosing scope | Normative source |
//! |---|---|---|
//! | `sender_id` | the meeting | `internal.proto::SubscriberSlot.sender_id` |
//! | `slot_id` | one subscriber's connection | `signaling.proto::ReceiveSlot.slot_id` |
//! | `egress_stream_id` | one registration | `internal.proto::EgressStream.egress_stream_id` |
//! | `stream_number` | one sender | `signaling.proto::SendStream.stream_number` |
//!
//! The frame's relay-region `stream_id` is **hop-local** (ADR-0036 §2,
//! amendment 2026-10-13) and carries two of these identifiers on different
//! hops: on the UPLINK it is the publisher's `stream_number` (MC's send id,
//! publisher-set, unauthenticated, never cross-checked against the key id), on
//! the DOWNLINK MH writes the subscriber's `slot_id`. It is not merely
//! subscriber-scoped. Attribution by (connection-bound sender, uplink id) is
//! stated at `internal.proto::SourceStream`.
//!
//! `sender_id` is the load-bearing case. It is a **16-bit per-meeting ordinal**
//! (the `SFrame` key id is `sender_id`(16) | `stream`(8) | `generation`(40)), so
//! `sender_id` 5 exists *concurrently in every meeting on this handler*. A flat
//! `HashMap<SenderId, _>` is therefore not merely imprecise — it is a
//! **cross-meeting media-crossing primitive**, and cross-tenant leakage is the
//! one threat class a keyless relay does not otherwise expose.
//!
//! So the snapshot is a nest of maps, every field is private, and
//! [`RoutingSnapshot::attribute`] is the only source lookup (with
//! [`RoutingSnapshot::is_server_muted`], its first step, exposed so the
//! datagram path can run it before decode). Both take a [`MeetingKey`] and a
//! [`SenderId`] together; there is no function that accepts a `SenderId`
//! alone. **The wrong index is a missing function, not a remembered rule.**
//! The edge index is keyed by the declared SOURCE STREAM (sender, uplink id),
//! never by the sender alone, so a frame reaches only its own stream's edges.
//!
//! The same discipline removed the traversal's earlier `Vec`-returning form at
//! story task 16: an allocating traversal sitting beside a per-frame path that
//! must not allocate is a footgun the hot path has to *remember* not to call,
//! which is the shape this module exists to reject. A test wanting a `Vec`
//! collects at the test site.
//!
//! No gate in this story can catch a global index by observation: loopback is
//! one meeting with one participant, so a correctly meeting-scoped index and a
//! global one are observationally identical across every validation layer. The
//! acceptance pin is therefore an executable **two-meeting** test asserting
//! both arms (see this module's tests), and the structure above is what makes
//! the wrong implementation fail to compile rather than fail to be noticed.
//!
//! # Widths are derived, never restated
//!
//! Every width here comes from `media-protocol`'s exported key-id layout
//! constants through a `const` assertion, so no production path in this crate
//! narrows a wire value against a hand-written bound: if the layout moves,
//! this module stops compiling instead of silently truncating into the wrong
//! field. The `const _` asserts below are the enforcement — the claim is
//! "derived, not restated", and those asserts are what makes it checkable.
//!
//! Width literals do appear twice, deliberately and only where a derived
//! constant would defeat the purpose: in the boundary tests, which must name
//! `65_535`, `65_536`, `255` and `256` outright (a test written against the
//! same constant as the code passes no matter what the constant becomes), and
//! in the prose of the newtype docstrings, which quote today's range to a
//! reader. Neither participates in narrowing. A grep for width literals in
//! this crate therefore has hits, and those are the only two kinds.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU16;
use std::sync::Arc;

use arc_swap::ArcSwap;
use media_protocol::frame::{KEY_ID_SENDER_ID_BITS, KEY_ID_STREAM_BITS, STREAM_ID_FIELD_BYTES};
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, SourceStreamEcho};
use proto_gen::dark_tower::signaling::v1::TransportMode;

use crate::config::PolicyLimits;
use crate::media::edge_runtime::{self, EdgeContext, EdgeRuntime, SourceStatus};
use crate::media::keyframe::PublisherKeyframesRegistry;
use crate::observability::metrics::PolicyApplyOutcome;
use crate::session::LocalSubscribers;

mod sources;
pub use sources::{Declarations, Disposition, SourceDecl};

// ---------------------------------------------------------------------------
// Width derivation
// ---------------------------------------------------------------------------

const _: () = assert!(
    KEY_ID_SENDER_ID_BITS == 16,
    "SenderId wraps NonZeroU16 because the key id allots 16 bits to sender_id; \
     if that width moved, this newtype silently truncates every id above the new bound"
);

const _: () = assert!(
    KEY_ID_STREAM_BITS == 8,
    "StreamNumber wraps u8 because the key id allots 8 bits to the stream; \
     a wider value would alias two distinct streams of one sender in the signed key id"
);

const _: () = assert!(
    STREAM_ID_FIELD_BYTES == 2,
    "SlotId wraps u16 because the frame's relay-region stream id is 2 bytes; \
     a wider value cannot be written into the relay region without truncation"
);

// ---------------------------------------------------------------------------
// Scoped identifiers
// ---------------------------------------------------------------------------

/// A publishing participant's compact **per-meeting** sender handle.
///
/// Valid range `1..=65535`. **Zero is reserved-invalid**, which is why this
/// wraps [`NonZeroU16`] rather than `u16`: a bare `u16::try_from` would accept
/// `0`, the one value the contract reserves. Two senders sharing `0` is not a
/// recycled id but N concurrently-live colliding ones, and because the wrap
/// nonce derives from the key id, two senders at `0` with the same stream and
/// generation wrap distinct transmit keys under one KEK at one nonce — AES-GCM
/// authentication-key recovery (ADR-0036 §4), not merely a confidentiality
/// loss.
///
/// # This is deliberately a fresh type
///
/// - **Not `common::types::UserId`** (`u64`, random, globally stable). That
///   type carries exactly the global-identifier semantics ADR-0036 deletes from
///   the media path; reusing it would re-import cross-meeting linkability
///   through the back door.
/// - **Not `common::types::StreamId`** (`u32`, subscriber-chosen). Wrong width
///   and pre-ADR-0036 semantics.
/// - **Not shared with `mc_service::media_admission::sender_id::SenderId`.**
///   Same wire width, different invariant: MC's is an *allocation* invariant
///   (monotonic, never recycled, allocator is the sole production constructor)
///   and MH's is a *validation* invariant (parse-and-reject untrusted inbound).
///   The width — the only thing that can actually drift — has one home, in
///   `media-protocol`, and both derive from it. Collapsing the types would
///   force a shared crate to carry a construction policy it has no business
///   holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SenderId(NonZeroU16);

impl SenderId {
    /// Parse a wire `uint32` carrying 16-bit sender semantics.
    ///
    /// Rejects `0` and anything above the 16-bit range. **Rejects, never
    /// truncates**: silently narrowing would map two distinct wire values onto
    /// one sender.
    ///
    /// # Errors
    ///
    /// [`IdError::SenderIdZero`] for the reserved `0`, or
    /// [`IdError::SenderIdOutOfRange`] above the 16-bit key-id field.
    pub fn from_wire(value: u32) -> Result<Self, IdError> {
        let narrowed = u16::try_from(value).map_err(|_| IdError::SenderIdOutOfRange)?;
        NonZeroU16::new(narrowed)
            .map(Self)
            .ok_or(IdError::SenderIdZero)
    }

    /// The numeric value, always in `1..=65535`.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

/// One subscriber's receive-slot identifier.
///
/// **Scoped to one subscriber's connection, not globally unique.** Two
/// subscribers both choosing slot `0` is normal and legal, which is why the
/// routing table keys on `(subscriber, slot)` and never on the slot alone, and
/// why — unlike [`SenderId`] — **zero is valid here**.
///
/// That difference is the reason there is no shared `MAX_16BIT_ID` constant or
/// shared `validate_16bit_id()` helper: `sender_id` carries a non-recycling
/// *allocation* bound and `slot_id` a per-subscriber *validation* bound, and
/// collapsing them silently loses the former.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotId(u16);

impl SlotId {
    /// Parse a wire `uint32` carrying 16-bit slot semantics.
    ///
    /// Rejects out-of-range values; **never truncates** into the u16 relay
    /// field.
    ///
    /// # Errors
    ///
    /// [`IdError::SlotIdOutOfRange`] above the 16-bit relay-region field. Zero
    /// is valid and is not an error here.
    pub fn from_wire(value: u32) -> Result<Self, IdError> {
        u16::try_from(value)
            .map(Self)
            .map_err(|_| IdError::SlotIdOutOfRange)
    }

    /// The numeric value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Which of a publisher's streams a candidate source names.
///
/// A wire `uint32` carrying **8-bit** semantics: the 64-bit `SFrame` key id
/// allots 8 bits to the stream. A stored value above the 8-bit range would
/// truncate into the key id's stream field and **alias two distinct streams of
/// the same sender** — the same bug class as an over-wide `sender_id`, one
/// scope down.
///
/// This is NOT [`SlotId`]. Both values travel in the 16-bit relay-region
/// `stream_id`, on different hops: this one on the uplink (the publisher's
/// label), the slot id on the downlink. Only this one also lives in the signed
/// key id, and the relay copy is never cross-checked against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamNumber(u8);

impl StreamNumber {
    /// Parse a wire `uint32` carrying 8-bit stream semantics. Rejects, never
    /// truncates.
    ///
    /// # Errors
    ///
    /// [`IdError::StreamNumberOutOfRange`] above the 8-bit key-id field, which
    /// would otherwise alias two distinct streams of one sender.
    pub fn from_wire(value: u32) -> Result<Self, IdError> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| IdError::StreamNumberOutOfRange)
    }

    /// The numeric value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Narrow a 16-bit uplink relay `stream_id` to the 8-bit send id it carries.
///
/// **Rejects, never truncates.** The relay field is 16 bits and send ids are
/// 8, so a label above 255 can never name a declared stream; an `as u8` here
/// would alias `256` onto stream 0 and silently re-attribute the frame.
impl TryFrom<u16> for StreamNumber {
    type Error = IdError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| IdError::StreamNumberOutOfRange)
    }
}

/// The meeting a policy, a route or a lookup belongs to.
///
/// `Arc<str>` rather than `String` because the key is cloned into the snapshot
/// on every apply and read on every lookup, and rather than
/// `common::types::MeetingId(Uuid)` because MH's meeting ids arrive as opaque
/// strings on the wire and in `SessionState` — parsing them into a UUID would
/// invent a validation MH has no reason to perform and no contract requiring.
///
/// Its role is structural: it is the token that makes a scoped lookup
/// *constructible*. [`RoutingSnapshot`] exposes no function that resolves a
/// [`SenderId`] without one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeetingKey(Arc<str>);

impl MeetingKey {
    /// Build a meeting key from the registration's `meeting_id`.
    #[must_use]
    pub fn new(meeting_id: &str) -> Self {
        Self(Arc::from(meeting_id))
    }

    /// The underlying meeting id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a scoped identifier could not be parsed from its wire value.
///
/// Deliberately carries **no value**: these become an outward-facing gRPC
/// status message, and echoing attacker-supplied input into one is how a
/// bounded reject message becomes a reflection surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdError {
    /// `sender_id` was `0`, which the contract reserves as never-valid.
    SenderIdZero,
    /// `sender_id` exceeded the 16-bit key-id field.
    SenderIdOutOfRange,
    /// `slot_id` exceeded the 16-bit relay-region field.
    SlotIdOutOfRange,
    /// `stream_number` exceeded the 8-bit key-id field.
    StreamNumberOutOfRange,
}

impl IdError {
    /// Every variant. See [`PolicyRejection::ALL`] for why this exists.
    pub const ALL: [Self; 4] = [
        Self::SenderIdZero,
        Self::SenderIdOutOfRange,
        Self::SlotIdOutOfRange,
        Self::StreamNumberOutOfRange,
    ];
}

// ---------------------------------------------------------------------------
// Policy shape
// ---------------------------------------------------------------------------

/// Who receives an egress stream, and in which of their slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberRef {
    /// The subscribing participant, meeting-scoped.
    pub sender: SenderId,
    /// Which of that subscriber's declared slots this stream lands in.
    pub slot: SlotId,
}

/// One source an egress stream may forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CandidateRef {
    /// The publishing participant, meeting-scoped.
    pub sender: SenderId,
    /// Which of that publisher's streams.
    pub stream: StreamNumber,
}

/// One egress stream: a complete, self-contained forwarding instruction.
///
/// Self-contained by construction (ADR-0036 §6): behaviours live on the edge
/// rather than in a parallel list keyed by id, so "a behaviour whose id matches
/// no edge" and "an edge with no behaviour" are both unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressEdge {
    /// MC-allocated policy-plane identity, stable across slot renumbering.
    pub egress_stream_id: u32,
    /// Data-plane address: MH writes `subscriber.slot` into the frame's
    /// relay-region stream id for this egress.
    pub subscriber: SubscriberRef,
    /// Sources eligible to fill this egress stream. Exactly one this story;
    /// repeated because audio selection among many speakers must be additive.
    pub candidates: Vec<CandidateRef>,
    /// MC-assigned forwarding priority. **Assigned by MC, never by a client**,
    /// and that is a security property: priority groups bound how far a
    /// publisher's self-declared salience signal can promote it, so a client
    /// able to name its own group could promote itself past that bound.
    ///
    /// **A lower number is a higher priority** (`internal.proto`
    /// `EgressStream.priority_group`): MC assigns audio edges 1 and video edges
    /// 2, which keeps MH type-blind. Recorded here; the stream path (MH-A2)
    /// maps it onto QUIC stream priority.
    pub priority_group: u32,
    /// Whether a superseding frame permits discarding queued frames for this
    /// egress. A **behaviour**, not a media type: MH is told what a stream
    /// does, never what it is (ADR-0036 §7 type-blindness).
    pub supersede_on_independent_frame: bool,
    /// How this edge reaches its subscriber, DERIVED from its candidates'
    /// declared `SourceStream` transport (`EgressStream` carries no transport
    /// of its own; field 6 is reserved). `None` exactly when the
    /// edge has no candidate — a legal edge that forwards nothing, which names
    /// no transport rather than fabricating one.
    pub transport: Option<TransportMode>,
}

/// A validated, ready-to-install policy for one meeting.
///
/// Reaching this type means every structural check passed. Parsing produces it
/// or fails; there is no partially-valid policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingPolicy {
    /// Which meeting this policy governs.
    pub meeting: MeetingKey,
    /// The generation MC asserts. `0` means "MC named no generation".
    pub generation: u64,
    /// The complete egress set. Empty is legal and meaningful: "this handler
    /// forwards nothing for this meeting".
    pub edges: Vec<EgressEdge>,
    /// Every source stream MH may receive for this meeting on this handler,
    /// with what to do with it (story 3 R-16, R-17, R-20).
    pub declarations: Declarations,
}

/// Why a registration was refused outright.
///
/// Every variant is evaluable **from the request alone**, needs no external
/// state, and rejects the *whole* registration — never last-write-wins, never
/// partial application. Variants carry no attacker-supplied values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyRejection {
    /// `egress_streams` exceeded the configured per-meeting bound.
    TooManyEgressStreams,
    /// An egress stream's `candidate_sources` exceeded the configured bound.
    TooManyCandidateSources,
    /// An egress stream omitted its `subscriber` submessage.
    MissingSubscriber,
    /// A scoped identifier was malformed.
    Id(IdError),
    /// Two egress streams shared an `egress_stream_id`.
    DuplicateEgressStreamId,
    /// Two egress streams targeted one `(sender, slot)` — "two sources into
    /// one slot".
    DuplicateSubscriberSlot,
    /// `source_streams` exceeded `MH_MAX_SOURCE_STREAMS_PER_MEETING`.
    TooManySourceStreams,
    /// A `(sender_id, stream_number)` appeared twice in `source_streams`.
    DuplicateSourceStream,
    /// A `SourceStream` named no (or an unknown) transport mode.
    UnspecifiedSourceTransport,
    /// A `SourceStream` named no (or an unknown) disposition.
    UnspecifiedDisposition,
    /// An egress candidate names a stream absent from the declarations —
    /// `rejected_transport_inconsistent`. Includes EVERY candidate of a
    /// registration whose `source_streams` is absent or empty: there is no
    /// fallback reading (MC-B deleted the story-2 one).
    UndeclaredCandidate,
    /// One egress stream's candidates declare different transports —
    /// `rejected_transport_inconsistent`. (Different EGRESSES may differ: one
    /// meeting legally mixes DATAGRAM audio and `STREAM_PER_GROUP` video.)
    InconsistentCandidateTransports,
}

impl From<IdError> for PolicyRejection {
    fn from(e: IdError) -> Self {
        Self::Id(e)
    }
}

impl PolicyRejection {
    /// Every rejection reachable from [`MeetingPolicy::from_request`],
    /// including all four [`IdError`] arms.
    ///
    /// Exists because the property the reason strings carry —
    /// `&'static str`, no attacker-supplied value reaching an outbound gRPC
    /// status — is only as good as the test that walks the variants, and a
    /// hand-written list of variants in that test goes short silently. The
    /// `Id(_)` nesting made the first version short *on arrival*: it listed
    /// one of the four `IdError` arms while claiming to cover them all, so
    /// three quarters of the identifier rejections — the ones carrying
    /// attacker-chosen `sender_id`, `slot_id` and `stream_number` values —
    /// were unpinned in a test asserting they were pinned. Building the list
    /// from `IdError::ALL` makes that class of gap unrepresentable.
    pub const ALL: [Self; 15] = [
        Self::TooManyEgressStreams,
        Self::TooManyCandidateSources,
        Self::TooManySourceStreams,
        Self::MissingSubscriber,
        Self::Id(IdError::SenderIdZero),
        Self::Id(IdError::SenderIdOutOfRange),
        Self::Id(IdError::SlotIdOutOfRange),
        Self::Id(IdError::StreamNumberOutOfRange),
        Self::DuplicateEgressStreamId,
        Self::DuplicateSubscriberSlot,
        Self::DuplicateSourceStream,
        Self::UnspecifiedSourceTransport,
        Self::UnspecifiedDisposition,
        Self::UndeclaredCandidate,
        Self::InconsistentCandidateTransports,
    ];

    /// A bounded, generic operator-facing reason.
    ///
    /// `&'static str` by construction, so no caller can format request data
    /// into an outbound status message or a log line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::TooManyEgressStreams => "egress_streams exceeds the configured per-meeting bound",
            Self::TooManyCandidateSources => {
                "candidate_sources exceeds the configured per-egress bound"
            }
            Self::TooManySourceStreams => "source_streams exceeds the configured per-meeting bound",
            Self::MissingSubscriber => "egress stream is missing its subscriber",
            Self::Id(IdError::SenderIdZero) => "sender_id 0 is reserved and never valid",
            Self::Id(IdError::SenderIdOutOfRange) => "sender_id is out of range",
            Self::Id(IdError::SlotIdOutOfRange) => "slot_id is out of range",
            Self::Id(IdError::StreamNumberOutOfRange) => "stream_number is out of range",
            Self::DuplicateEgressStreamId => "duplicate egress_stream_id",
            Self::DuplicateSubscriberSlot => "duplicate subscriber slot",
            Self::DuplicateSourceStream => "duplicate source_streams (sender_id, stream_number)",
            Self::UnspecifiedSourceTransport => {
                "source_streams transport_mode is required and must be a known mode"
            }
            Self::UnspecifiedDisposition => {
                "source_streams disposition is required and must be a known value"
            }
            Self::UndeclaredCandidate => "an egress candidate is not declared in source_streams",
            Self::InconsistentCandidateTransports => {
                "an egress stream's candidates declare different transport modes"
            }
        }
    }

    /// The `mh_media_policy_applies_total` outcome this rejection records.
    ///
    /// Exhaustive with no wildcard arm, so a new rejection must choose. The
    /// two transport-consistency rejections have their own value because their
    /// remedy differs (MC's declaration vs its egress set disagree; alert A8);
    /// everything else is a malformed registration.
    #[must_use]
    pub const fn outcome(self) -> PolicyApplyOutcome {
        match self {
            Self::UndeclaredCandidate | Self::InconsistentCandidateTransports => {
                PolicyApplyOutcome::RejectedTransportInconsistent
            }
            Self::TooManyEgressStreams
            | Self::TooManyCandidateSources
            | Self::MissingSubscriber
            | Self::Id(_)
            | Self::DuplicateEgressStreamId
            | Self::DuplicateSubscriberSlot
            | Self::TooManySourceStreams
            | Self::DuplicateSourceStream
            | Self::UnspecifiedSourceTransport
            | Self::UnspecifiedDisposition => PolicyApplyOutcome::RejectedInvalid,
        }
    }
}

impl MeetingPolicy {
    /// Parse and validate a registration into an installable policy.
    ///
    /// # Check order is load-bearing
    ///
    /// The three **count bounds run first**, before anything that iterates or
    /// allocates per element. `internal.proto` requires them "before building
    /// any routing table"; that is the floor rather than the ceiling, because
    /// duplicate detection builds `HashSet`s sized by the attacker-controlled
    /// repeated field — the same allocation the bound exists to prevent, merely
    /// moved earlier than the routing table.
    ///
    /// Order: counts -> egress shape / identifier widths / duplicates -> the
    /// declarations (`source_streams`; empty declares nothing) -> every
    /// candidate against the declarations.
    /// Width checks (a candidate `stream_number` above 255 is
    /// `rejected_invalid`) therefore precede the undeclared-candidate check.
    ///
    /// # Errors
    ///
    /// Returns the first [`PolicyRejection`]; the whole registration is
    /// refused and nothing is applied.
    pub fn from_request(
        request: &RegisterMeetingRequest,
        limits: &PolicyLimits,
    ) -> Result<Self, PolicyRejection> {
        // ---- counts, before any per-element work -------------------------
        if request.egress_streams.len() > limits.max_egress_streams_per_meeting {
            return Err(PolicyRejection::TooManyEgressStreams);
        }
        for stream in &request.egress_streams {
            if stream.candidate_sources.len() > limits.max_candidate_sources_per_egress {
                return Err(PolicyRejection::TooManyCandidateSources);
            }
        }
        if request.source_streams.len() > limits.max_source_streams_per_meeting {
            return Err(PolicyRejection::TooManySourceStreams);
        }

        let mut edges = Vec::with_capacity(request.egress_streams.len());
        let mut seen_ids = HashSet::with_capacity(request.egress_streams.len());
        let mut seen_slots = HashSet::with_capacity(request.egress_streams.len());

        for stream in &request.egress_streams {
            // ---- shape ---------------------------------------------------
            let subscriber_msg = stream
                .subscriber
                .as_ref()
                .ok_or(PolicyRejection::MissingSubscriber)?;

            // ---- identifier widths --------------------------------------
            let subscriber = SubscriberRef {
                sender: SenderId::from_wire(subscriber_msg.sender_id)?,
                slot: SlotId::from_wire(subscriber_msg.slot_id)?,
            };

            let mut candidates = Vec::with_capacity(stream.candidate_sources.len());
            for candidate in &stream.candidate_sources {
                candidates.push(CandidateRef {
                    sender: SenderId::from_wire(candidate.sender_id)?,
                    stream: StreamNumber::from_wire(candidate.stream_number)?,
                });
            }

            // ---- duplicates ---------------------------------------------
            if !seen_ids.insert(stream.egress_stream_id) {
                return Err(PolicyRejection::DuplicateEgressStreamId);
            }
            if !seen_slots.insert((subscriber.sender, subscriber.slot)) {
                return Err(PolicyRejection::DuplicateSubscriberSlot);
            }

            edges.push(EgressEdge {
                egress_stream_id: stream.egress_stream_id,
                subscriber,
                candidates,
                priority_group: stream.priority_group,
                supersede_on_independent_frame: stream.supersede_on_independent_frame,
                transport: None,
            });
        }

        // ---- declarations ------------------------------------------------
        // One reading. An absent or empty `source_streams` declares nothing,
        // so the walk below rejects every candidate-bearing egress as
        // `UndeclaredCandidate` (`rejected_transport_inconsistent`) — never a
        // fallback (MC-B deleted the story-2 transitional reading).
        let declarations = sources::parse_declared(request)?;

        // ---- every candidate against the declarations --------------------
        // ONE index, built once (O(source_streams)), so this walk is
        // O(egress x candidates) — never x source_streams: a per-candidate
        // linear scan would be attacker-sized work before the policy check.
        let declared_index = declarations.build_index();
        for edge in &mut edges {
            let mut transport: Option<TransportMode> = None;
            for candidate in &edge.candidates {
                let declared = declarations
                    .resolve(&declared_index, candidate.sender, candidate.stream)
                    .ok_or(PolicyRejection::UndeclaredCandidate)?;
                match transport {
                    None => transport = Some(declared.transport),
                    Some(existing) if existing == declared.transport => {}
                    Some(_) => return Err(PolicyRejection::InconsistentCandidateTransports),
                }
            }
            edge.transport = transport;
        }

        Ok(Self {
            meeting: MeetingKey::new(&request.meeting_id),
            generation: request.policy_generation,
            edges,
            declarations,
        })
    }
}

// ---------------------------------------------------------------------------
// The published snapshot
// ---------------------------------------------------------------------------

/// The installed forwarding policy for **one** meeting.
///
/// All fields are private. A `pub by_source` would degrade this module's
/// structural guarantee back into a convention: the point is that no caller
/// can reach a sender-keyed map except through [`RoutingSnapshot::attribute`]
/// or [`RoutingSnapshot::is_server_muted`], which require a [`MeetingKey`].
///
/// **Every per-source-stream fact lives here** — the declarations, their
/// index, the all-muted set, and (MH-B) any per-stream state hung off them —
/// so it is bounded by `MH_MAX_SOURCE_STREAMS_PER_MEETING`, replaced whole on
/// re-push, and freed when the meeting's routes are removed. Nothing keyed by
/// sender or stream exists outside a `MeetingKey` scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingRoutes {
    generation: u64,
    edges: Vec<EgressEdge>,
    /// Ingress-side lookup: declared source `(sender, stream)` -> indices into
    /// `edges`.
    ///
    /// Keyed by the SOURCE STREAM, not the sender: a frame attributed to one
    /// of a sender's streams reaches only that stream's edges, so audio never
    /// reaches a video slot and vice versa (R-20), and a relabelled frame
    /// reaches only the slots of the stream it was relabelled onto (R-16).
    ///
    /// **INVARIANT: these indices index into THIS `MeetingRoutes.edges` and
    /// nothing else.** A shared or flattened cross-meeting edge arena is
    /// forbidden. A `usize` carries no scope in its type — unlike [`SenderId`],
    /// where the wrong lookup is a missing function — so this index is
    /// meeting-local only because `edges` is *contained* here. The plausible
    /// way that breaks is the per-frame cache-locality optimisation that
    /// flattens every meeting's edges into one arena and has this map index it;
    /// at that moment every `usize` becomes a global handle into a
    /// cross-meeting array, rebuilding the exact primitive this module exists
    /// to prevent — reached through a *performance* change nobody would think
    /// to route past a correctness constraint.
    by_source: HashMap<(SenderId, StreamNumber), Vec<usize>>,
    /// The declarations this generation installed — the one owner.
    declarations: Declarations,
    /// `(sender, stream)` -> position in the declarations, derived from them.
    declared_index: HashMap<(SenderId, StreamNumber), usize>,
    /// One runtime cell per edge, aligned with `edges` — the state that must
    /// SURVIVE a re-push (hop sequence; a stream edge's switch machine,
    /// downlinks and backlog budget). Held here by `Arc` and carried into the
    /// successor routes by `egress_stream_id` (`crate::media::edge_runtime`),
    /// so the snapshot stays immutable while the cells it points at are not.
    cells: EdgeCells,
    /// `(subscriber, slot)` -> index into `edges`: a receiver's
    /// `RecoveryRequest` resolves its `slot_id` ONLY through its own
    /// connection's bound sender here (story 3 R-13), meeting-local like
    /// `by_source`.
    by_slot: HashMap<(SenderId, SlotId), usize>,
    /// Senders every frame of which drops `server_muted` (the shortcut),
    /// derived from the declarations; non-vacuous (see
    /// `Declarations::all_muted`). Keyed by the scoped [`SenderId`] inside this
    /// meeting's routes because `sender_id` 5 exists in every meeting on this
    /// handler: a handler-global set would silence an innocent sender in
    /// another meeting.
    all_muted: HashSet<SenderId>,
}

impl MeetingRoutes {
    /// Build the routes for `policy`, reconciling the runtime cells of the
    /// installed `previous` routes (if any). Reconcile's side effects —
    /// resetting removed edges' downlinks, assigning switch targets — happen
    /// HERE, before the successor snapshot is published.
    fn build(policy: &MeetingPolicy, previous: Option<&Self>, context: &EdgeContext) -> Self {
        let declared_index = policy.declarations.build_index();
        let all_muted = policy.declarations.all_muted();
        // The same kernel, in the same order, as `attribute()`: a wholly
        // muted sender's source reads Muted even if this stream is undeclared.
        let status = |candidate: CandidateRef| -> SourceStatus {
            match classify(
                &policy.declarations,
                &declared_index,
                &all_muted,
                candidate.sender,
                candidate.stream,
            ) {
                Classified::SenderMuted => SourceStatus::Muted,
                Classified::Undeclared => SourceStatus::Undeclared,
                Classified::Declared(decl) => match decl.disposition {
                    Disposition::Forward => SourceStatus::Forward,
                    Disposition::Paused => SourceStatus::Paused,
                    Disposition::DropAtIngress => SourceStatus::Muted,
                },
            }
        };
        let reconciled = edge_runtime::reconcile(
            &policy.meeting,
            previous.map(|routes| (routes.edges.as_slice(), routes.cells.0.as_slice())),
            &policy.edges,
            status,
            context,
        );

        // Per-source keyframe state of every publisher bound in this meeting
        // (story 3 MH-B): records of streams MC no longer declares are FREED
        // (unassign), and a queued request for a source no stream edge assigns
        // with a FORWARD declaration any more (edge removed, PAUSED,
        // DROP_AT_INGRESS) is discarded.
        for (sender, keyframes) in context.keyframes.in_meeting(&policy.meeting) {
            keyframes.reconcile(
                |stream| {
                    policy
                        .declarations
                        .resolve(&declared_index, sender, stream)
                        .is_some()
                },
                |stream| {
                    reconciled
                        .wanted_sources
                        .contains(&CandidateRef { sender, stream })
                },
            );
        }

        // (subscriber, slot) is unique: validation rejects a repeat
        // (`DuplicateSubscriberSlot`).
        let by_slot: HashMap<(SenderId, SlotId), usize> = policy
            .edges
            .iter()
            .enumerate()
            .map(|(index, edge)| ((edge.subscriber.sender, edge.subscriber.slot), index))
            .collect();
        debug_assert_eq!(
            by_slot.len(),
            policy.edges.len(),
            "a validated policy holds one edge per (subscriber, slot)"
        );

        let mut by_source: HashMap<(SenderId, StreamNumber), Vec<usize>> = HashMap::new();
        for (index, edge) in policy.edges.iter().enumerate() {
            for candidate in &edge.candidates {
                by_source
                    .entry((candidate.sender, candidate.stream))
                    .or_default()
                    .push(index);
            }
        }
        // A stream edge mid-switch still receives the source it forwards
        // until the target's group start (ADR-0036 §7); its edge machine
        // skips that source's frames once the switch lands.
        for (source, index) in reconciled.still_forwarding {
            let indices = by_source.entry((source.sender, source.stream)).or_default();
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        Self {
            generation: policy.generation,
            edges: policy.edges.clone(),
            by_source,
            by_slot,
            declared_index,
            all_muted,
            declarations: policy.declarations.clone(),
            cells: EdgeCells(reconciled.cells),
        }
    }

    /// Every runtime cell of this meeting, aligned with [`Self::edges`].
    #[must_use]
    pub fn cells(&self) -> &[Arc<EdgeRuntime>] {
        &self.cells.0
    }

    /// `STREAM_PER_GROUP` source streams this policy declares. Counts only.
    #[must_use]
    pub fn stream_sources_declared(&self) -> usize {
        self.declarations
            .count_transport(TransportMode::StreamPerGroup)
    }

    /// The generation this meeting's live forward path reflects.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// How many egress edges this meeting contributes to the handler total.
    #[must_use]
    pub const fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Every egress edge, in policy order.
    #[must_use]
    pub fn edges(&self) -> &[EgressEdge] {
        &self.edges
    }

    /// Whether this installed policy's declarations equal `other` as SETS
    /// (order-insensitive). Used by the equal-generation divergence detector,
    /// which logs counts only — never which sender or stream.
    #[must_use]
    pub fn declarations_match(&self, other: &Declarations) -> bool {
        self.declarations.same_as(other)
    }

    /// How many source streams are declared. Counts only.
    #[must_use]
    pub fn declared_count(&self) -> usize {
        self.declarations.declared_count()
    }

    /// How many publishers drop `server_muted` on every frame. Counts only.
    #[must_use]
    pub fn server_muted_count(&self) -> usize {
        self.all_muted.len()
    }

    /// The per-stream echo, from THIS installed generation.
    #[must_use]
    pub fn applied_source_streams(&self) -> Vec<SourceStreamEcho> {
        self.declarations.echo()
    }
}

/// The runtime cells of one meeting's edges. Compared by identity of the
/// edges they serve (`egress_stream_id`), never by their mutable contents, so
/// `MeetingRoutes` equality still means "the same installed policy".
#[derive(Debug, Clone)]
struct EdgeCells(Vec<Arc<EdgeRuntime>>);

impl PartialEq for EdgeCells {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|(a, b)| a.egress_stream_id() == b.egress_stream_id())
    }
}

impl Eq for EdgeCells {}

/// Which declared source stream an ingress frame belongs to, and what to do
/// with it — the result of [`RoutingSnapshot::attribute`].
///
/// **Only [`Attribution::Route`] carries edges.** Every drop variant carries
/// nothing to forward to, so a caller (the datagram path today, the stream
/// path in MH-A2) cannot route a dropped frame by forgetting a check.
#[must_use]
#[derive(Debug)]
pub enum Attribution<'a> {
    /// No policy is installed for this meeting.
    NoPolicy,
    /// Drop `server_muted`: the sender-level shortcut, or the attributed
    /// stream is `DROP_AT_INGRESS`.
    ServerMuted,
    /// Drop `unmatched_stream`: no declared stream of this sender carries this
    /// uplink id.
    Unmatched,
    /// Drop `transport_mismatch`: declared, but on another transport.
    TransportMismatch,
    /// Forward along this source stream's edges (possibly none:
    /// `no_subscriber`).
    Route {
        /// The declared stream the label resolved to — narrowed from the
        /// 16-bit label ONCE, here, so no caller re-narrows it.
        stream: StreamNumber,
        /// The stream is PAUSED (client mute): forwarded normally, counted on
        /// the paused-arrivals overlay.
        paused: bool,
        /// This source stream's edges, and no other stream's.
        edges: SourceEdges<'a>,
    },
}

/// How one `(sender, stream)` reads against a meeting's declarations — the
/// ONE kernel behind both [`RoutingSnapshot::attribute`] (per frame) and the
/// edge-runtime reconcile's view of a candidate (per apply), so the two can
/// never order the checks differently.
///
/// **Order, stated once:** the sender-level server-mute shortcut FIRST (a
/// wholly muted sender is muted whatever stream it names, declared or not),
/// then the declaration lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Classified {
    /// Every declared stream of the sender is `DROP_AT_INGRESS` (at least one
    /// declared).
    SenderMuted,
    /// No declared stream of this sender carries this number.
    Undeclared,
    /// Declared.
    Declared(SourceDecl),
}

fn classify(
    declarations: &Declarations,
    declared_index: &HashMap<(SenderId, StreamNumber), usize>,
    all_muted: &HashSet<SenderId>,
    sender: SenderId,
    stream: StreamNumber,
) -> Classified {
    if all_muted.contains(&sender) {
        return Classified::SenderMuted;
    }
    declarations
        .resolve(declared_index, sender, stream)
        .map_or(Classified::Undeclared, Classified::Declared)
}

/// The egress edges one source stream feeds, within one meeting. Borrowing and
/// non-allocating — the per-frame path walks it on every frame.
#[derive(Debug, Clone)]
pub struct SourceEdges<'a> {
    edges: &'a [EgressEdge],
    cells: &'a [Arc<EdgeRuntime>],
    indices: std::slice::Iter<'a, usize>,
}

/// One edge a source stream feeds, with its runtime cell.
#[derive(Debug, Clone, Copy)]
pub struct EdgeRef<'a> {
    /// The installed instruction.
    pub edge: &'a EgressEdge,
    /// Its runtime cell (hop, and a stream edge's switch state).
    pub runtime: &'a Arc<EdgeRuntime>,
}

impl SourceEdges<'_> {
    const EMPTY: &'static [usize] = &[];

    /// Whether this source stream feeds no edge at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.len() == 0
    }
}

impl<'a> Iterator for SourceEdges<'a> {
    type Item = EdgeRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // Resolve through the MEETING-LOCAL slices. `indexing_slicing = "deny"`
        // forces `.get`, which is also what keeps a stale index from reading a
        // neighbouring meeting's edge if the invariant on `by_source` is ever
        // broken.
        for &i in self.indices.by_ref() {
            if let (Some(edge), Some(runtime)) = (self.edges.get(i), self.cells.get(i)) {
                return Some(EdgeRef { edge, runtime });
            }
        }
        None
    }
}

/// The complete forwarding policy for every meeting on this handler.
///
/// Read by the data plane on every frame via [`RoutingTable::load`]; replaced
/// wholesale on apply. Per-meeting routes sit behind `Arc` so installing one
/// meeting's policy clones pointers rather than every other meeting's edges.
#[derive(Debug, Default)]
pub struct RoutingSnapshot {
    meetings: HashMap<MeetingKey, Arc<MeetingRoutes>>,
    total_edges: usize,
}

impl RoutingSnapshot {
    /// Attribute one ingress frame to a declared source stream — **the single
    /// attribution lookup**, for the datagram path and the stream path alike
    /// (story 3 R-20; `internal.proto` `SourceStream`).
    ///
    /// The key is (`sender`, `uplink_id`): the publisher bound to the
    /// connection at attach (JWT-gated, from MC), and the uplink relay-region
    /// `stream_id` the publisher stamped. That label is PUBLISHER-SET AND
    /// UNAUTHENTICATED: MH does not identify a frame's true stream, and never
    /// reads the payload or the `SFrame` key id. What the lookup guarantees is
    /// structural — a frame reaches only the edges of the declared stream its
    /// label and transport match — and why that suffices for per-kind server
    /// mute is stated once at `internal.proto` `SourceStream`.
    ///
    /// Order, each step load-bearing:
    /// 1. no policy for the meeting -> [`Attribution::NoPolicy`];
    /// 2. the sender-level shortcut -> [`Attribution::ServerMuted`], FIRST, so
    ///    a fully muted sender's frames count `server_muted` whatever their
    ///    label or transport. The datagram path also runs it before decode
    ///    ([`Self::is_server_muted`]); it is repeated here because the stream
    ///    path (MH-A2) has no pre-decode check;
    /// 3. the 16-bit label narrowed to the 8-bit send id WITHOUT truncation —
    ///    above 255 is [`Attribution::Unmatched`], never aliased (`256` must
    ///    not become stream 0);
    /// 4. undeclared -> [`Attribution::Unmatched`];
    /// 5. arrived on another transport -> [`Attribution::TransportMismatch`]
    ///    (before the disposition, so a muted stream's wrong-transport frame
    ///    is a mismatch — the remedy differs);
    /// 6. `DROP_AT_INGRESS` -> [`Attribution::ServerMuted`];
    /// 7. otherwise [`Attribution::Route`] over that stream's edges only.
    ///
    /// Borrowing and non-allocating; re-read per frame and never cached —
    /// server mute and participant removal are enforced through this lookup,
    /// so a cached answer that outlives a policy change is a mute bypass.
    pub fn attribute(
        &self,
        meeting: &MeetingKey,
        sender: SenderId,
        uplink_id: u16,
        arrival: TransportMode,
    ) -> Attribution<'_> {
        let Some(routes) = self.meetings.get(meeting) else {
            return Attribution::NoPolicy;
        };
        if routes.all_muted.contains(&sender) {
            return Attribution::ServerMuted;
        }
        let Ok(stream) = StreamNumber::try_from(uplink_id) else {
            return Attribution::Unmatched;
        };
        let declared = match classify(
            &routes.declarations,
            &routes.declared_index,
            &routes.all_muted,
            sender,
            stream,
        ) {
            Classified::SenderMuted => return Attribution::ServerMuted,
            Classified::Undeclared => return Attribution::Unmatched,
            Classified::Declared(declared) => declared,
        };
        if declared.transport != arrival {
            return Attribution::TransportMismatch;
        }
        let paused = match declared.disposition {
            Disposition::DropAtIngress => return Attribution::ServerMuted,
            Disposition::Paused => true,
            Disposition::Forward => false,
        };
        let indices = routes
            .by_source
            .get(&(sender, stream))
            .map_or(SourceEdges::EMPTY, Vec::as_slice);
        Attribution::Route {
            stream,
            paused,
            edges: SourceEdges {
                edges: &routes.edges,
                cells: &routes.cells.0,
                indices: indices.iter(),
            },
        }
    }

    /// The declared stream an uplink label resolves to for `sender` on
    /// `arrival`, whatever its disposition — the per-source keyframe record's
    /// key for a frame [`Self::attribute`] dropped `server_muted` (story 3
    /// MH-B: the counter is taken from muted frames too). The SAME
    /// declaration lookup as `attribute`, so a label `attribute` would call
    /// unmatched or mismatched never yields a key here: records exist only
    /// for declared streams.
    #[must_use]
    pub fn declared_stream(
        &self,
        meeting: &MeetingKey,
        sender: SenderId,
        uplink_id: u16,
        arrival: TransportMode,
    ) -> Option<StreamNumber> {
        let routes = self.meetings.get(meeting)?;
        let stream = StreamNumber::try_from(uplink_id).ok()?;
        routes
            .declarations
            .resolve(&routes.declared_index, sender, stream)
            .filter(|declared| declared.transport == arrival)
            .map(|declared| declared.stream)
    }

    /// The source a receiver's `RecoveryRequest` names by `slot_id` (story 3
    /// R-13): resolved ONLY among `own`'s edges in `meeting` (the requesting
    /// connection's bound sender), and only to a stream-per-group edge whose
    /// MC-ASSIGNED source (B during a pending A→B switch) is declared
    /// `FORWARD`. `None` is `unmapped_slot`.
    #[must_use]
    pub fn slot_source(
        &self,
        meeting: &MeetingKey,
        own: SenderId,
        slot_id: u32,
    ) -> Option<CandidateRef> {
        let routes = self.meetings.get(meeting)?;
        let slot = SlotId::from_wire(slot_id).ok()?;
        let edge = routes.edges.get(*routes.by_slot.get(&(own, slot))?)?;
        if edge.transport != Some(TransportMode::StreamPerGroup) {
            return None;
        }
        let source = *edge.candidates.first()?;
        match classify(
            &routes.declarations,
            &routes.declared_index,
            &routes.all_muted,
            source.sender,
            source.stream,
        ) {
            Classified::Declared(declared)
                if declared.disposition == Disposition::Forward
                    && declared.transport == TransportMode::StreamPerGroup =>
            {
                Some(source)
            }
            _ => None,
        }
    }

    /// The sender-level server-mute shortcut **within `meeting`** — true when
    /// every declared stream of `sender` is `DROP_AT_INGRESS` (at least one
    /// declared).
    ///
    /// Meeting-scoped for the same reason as [`Self::attribute`]: the same
    /// `sender_id` is a different participant in every other meeting on this
    /// handler, so there is no function taking a `SenderId` alone.
    ///
    /// # `false` has TWO meanings — the absent-meeting arm is FAIL-OPEN
    ///
    /// `false` means "not muted" **or** "no policy is installed for this
    /// meeting". That is safe at the one current caller, `forward_one`, only
    /// because it then calls [`Self::attribute`], which fails closed on an
    /// absent policy (`no_policy`). Any future caller that ACTS on `false`
    /// without also failing closed on an absent policy turns an unprogrammed
    /// handler into a mute bypass.
    #[must_use]
    pub fn is_server_muted(&self, meeting: &MeetingKey, sender: SenderId) -> bool {
        self.meetings
            .get(meeting)
            .is_some_and(|routes| routes.all_muted.contains(&sender))
    }

    /// The installed routes for one meeting, if any policy is installed.
    #[must_use]
    pub fn routes_for(&self, meeting: &MeetingKey) -> Option<&Arc<MeetingRoutes>> {
        self.meetings.get(meeting)
    }

    /// The generation a meeting's live forward path reflects.
    ///
    /// `0` means "nothing applied" — a legal, meaningful value, and the one a
    /// handler that has never successfully applied a policy for this meeting
    /// reports.
    #[must_use]
    pub fn generation_for(&self, meeting: &MeetingKey) -> u64 {
        self.meetings.get(meeting).map_or(0, |r| r.generation)
    }

    /// The per-stream echo for a meeting's APPLIED policy; empty when nothing
    /// is installed.
    #[must_use]
    pub fn applied_source_streams_for(&self, meeting: &MeetingKey) -> Vec<SourceStreamEcho> {
        self.meetings
            .get(meeting)
            .map(|r| r.applied_source_streams())
            .unwrap_or_default()
    }

    /// Egress edges across **all** meetings on this handler.
    #[must_use]
    pub const fn total_edges(&self) -> usize {
        self.total_edges
    }

    /// Number of meetings with an installed policy.
    #[must_use]
    pub fn meeting_count(&self) -> usize {
        self.meetings.len()
    }

    /// Build the successor snapshot that installs `policy`.
    ///
    /// Every other meeting's routes carry over by `Arc` clone. The result is
    /// complete before it is published: apply is all-edges-or-none, so a
    /// partial install is not a representable state.
    fn with_policy(&self, policy: &MeetingPolicy, context: &EdgeContext) -> Self {
        // Deliberately the SAME accounting the aggregate bound is tested
        // against, not a second copy of the same subtract-then-add. Two copies
        // would let the accounting and the check drift, and the direction that
        // drift breaks in is silent: the bound would be enforced against a
        // total the snapshot does not actually carry. `projected_total_edges`,
        // this function and `without_meeting` all go through ONE kernel,
        // [`Self::edges_after_displacing`].
        let total_edges = self.projected_total_edges(policy);
        let mut meetings = self.meetings.clone();
        let previous = self.meetings.get(&policy.meeting).map(Arc::as_ref);
        let routes = MeetingRoutes::build(policy, previous, context);
        meetings.insert(policy.meeting.clone(), Arc::new(routes));
        Self {
            meetings,
            total_edges,
        }
    }

    /// Build the successor snapshot with `meeting`'s routes REMOVED (story 2
    /// R-20 `EndMeeting` release).
    ///
    /// Removing the entry is also what FORGETS the meeting's applied
    /// generation: [`Self::generation_for`] reads 0 for an absent meeting, so a
    /// same-id meeting re-created later installs fresh from generation 1
    /// rather than being refused as stale. `None` when nothing is installed
    /// for `meeting`, so the caller can skip the swap.
    fn without_meeting(&self, meeting: &MeetingKey) -> Option<Self> {
        if !self.meetings.contains_key(meeting) {
            return None;
        }
        let total_edges = self.edges_after_displacing(meeting);
        let mut meetings = self.meetings.clone();
        meetings.remove(meeting);
        Some(Self {
            meetings,
            total_edges,
        })
    }

    /// Egress-edge total this handler would carry if `policy` were installed.
    ///
    /// **Subtracts the meeting's own currently-installed edges before adding
    /// its new ones.** Without that subtraction an idempotent re-assert of
    /// unchanged policy double-counts itself, and under ADR-0036 §8's <=10s
    /// re-assert cadence every meeting would trip the aggregate bound in turn
    /// once the handler is even half full — turning the cadence itself into a
    /// rotating apply failure. That defect passes every single-meeting test.
    #[must_use]
    pub fn projected_total_edges(&self, policy: &MeetingPolicy) -> usize {
        self.edges_after_displacing(&policy.meeting) + policy.edges.len()
    }

    /// The handler's egress-edge total with `meeting`'s installed edges taken
    /// out — the ONE accounting kernel behind install, projection and release.
    ///
    /// The two halves of "fail loudly, degrade safely", deliberately split.
    ///
    /// `saturating_sub` rather than `-` in PRODUCTION: `displaced <=
    /// total_edges` holds because this snapshot's successors are only ever
    /// built through this kernel, but a plain subtraction turns any future
    /// break of that invariant into a release-mode wrap to `usize::MAX` —
    /// which reads as "the handler is over its aggregate bound" and refuses
    /// EVERY apply on the pod, permanently and silently.
    ///
    /// The `debug_assert` is there because saturating alone would mask the
    /// break in the other direction too: an under-reported total means the
    /// bound is enforced against a total lower than reality and the pod quietly
    /// runs over it. Every test exercising this path runs in debug, so a broken
    /// invariant is loud where it can be fixed and safe where it cannot.
    fn edges_after_displacing(&self, meeting: &MeetingKey) -> usize {
        let displaced = self.meetings.get(meeting).map_or(0, |r| r.edges.len());
        debug_assert!(
            displaced <= self.total_edges,
            "edge accounting broke: one meeting holds {displaced} edges of a {} handler total",
            self.total_edges
        );
        self.total_edges.saturating_sub(displaced)
    }
}

/// Lock-free publication of [`RoutingSnapshot`] to the data plane.
///
/// `ArcSwap` rather than `tokio::sync::watch`: the forward path wants "hand me
/// the current table, never block, never await, never track seen-ness" on every
/// frame. `watch::Receiver::borrow()` returns a guard holding an internal
/// `RwLock` read — held across a forwarding decision it blocks the publisher —
/// and needs a per-reader `Receiver` carrying `changed()` state nobody reads.
/// [`ArcSwap::load_full`] is a plain atomic returning an `Arc` with no guard
/// lifetime.
///
/// It also makes "did a swap happen?" directly observable: `Arc::ptr_eq` across
/// a re-assert is how re-assert idempotency is asserted as a **true data-plane
/// no-op** rather than as an equal-valued rebuild.
#[derive(Debug)]
pub struct RoutingTable {
    snapshot: ArcSwap<RoutingSnapshot>,
    /// What edge-runtime reconcile needs beyond the policy.
    edge_context: EdgeContext,
}

impl RoutingTable {
    /// FOR TESTS ONLY: a table whose stream edges get a never-binding backlog
    /// cap and freshly resolved handles — for tests that are not about the
    /// stream path. The name says so, so no production call site can reach for
    /// it by accident; production uses [`Self::with_edge_context`].
    #[must_use]
    pub fn unbounded_for_tests() -> Self {
        Self::with_edge_context(EdgeContext::new(
            crate::config::MAX_EGRESS_STREAM_BACKLOG_BYTES,
            Arc::new(crate::observability::metrics::resolve_media_handles()),
        ))
    }

    /// A table whose stream edges use `edge_context`.
    #[must_use]
    pub fn with_edge_context(edge_context: EdgeContext) -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(RoutingSnapshot::default()),
            edge_context,
        }
    }

    /// The current snapshot. Lock-free; safe to call per frame.
    #[must_use]
    pub fn load(&self) -> Arc<RoutingSnapshot> {
        self.snapshot.load_full()
    }

    /// Install `policy`, replacing this meeting's routes wholesale.
    ///
    /// One swap of a fully-built successor: readers observe either the prior
    /// generation or the new one, never a partial install.
    pub fn install(&self, policy: &MeetingPolicy) {
        let next = self.snapshot.load().with_policy(policy, &self.edge_context);
        self.snapshot.store(Arc::new(next));
    }

    /// Remove `meeting`'s routes, mirroring [`Self::install`]: one fully-built
    /// successor, one swap, so the data plane sees the meeting either wholly
    /// present or wholly gone. Returns whether an entry was removed.
    ///
    /// Same single-writer contract as `install` (the session actor), which is
    /// why a plain load-modify-store is safe here.
    pub fn remove(&self, meeting: &MeetingKey) -> bool {
        let current = self.snapshot.load_full();
        match current.without_meeting(meeting) {
            Some(next) => {
                self.snapshot.store(Arc::new(next));
                // After the swap: the released meeting's stream groups end
                // (`edge_removed`). A frame already holding the old snapshot
                // finds its downlinks gone.
                if let Some(routes) = current.meetings.get(meeting) {
                    for cell in routes.cells() {
                        cell.release();
                    }
                }
                for (_, keyframes) in self.edge_context.keyframes.in_meeting(meeting) {
                    keyframes.clear();
                }
                true
            }
            None => false,
        }
    }

    /// The publisher `sender` of `meeting` disconnected: every edge it fed or
    /// was switching to learns its sources ended (`source_ended`; a pending
    /// switch to it is abandoned and the current source kept). Called by the
    /// connection teardown, a sibling of the hot path.
    pub fn sender_ended(&self, meeting: &MeetingKey, sender: SenderId) {
        if let Some(routes) = self.snapshot.load().meetings.get(meeting) {
            for cell in routes.cells() {
                cell.on_sender_ended(sender);
            }
        }
    }

    /// The subscriber registry stream edges judge presence against (one per
    /// table; the session handle and the media tasks share it).
    #[must_use]
    pub fn subscribers(&self) -> &Arc<LocalSubscribers> {
        &self.edge_context.subscribers
    }

    /// The publisher keyframe registry (one per table).
    #[must_use]
    pub fn keyframes(&self) -> &Arc<PublisherKeyframesRegistry> {
        &self.edge_context.keyframes
    }

    /// MH refused a new group of `sender`'s `stream` at the stream-open limit,
    /// after recording its counter: every edge of that source repairs by
    /// request (`EdgeRuntime::on_source_group_refused`).
    pub fn source_group_refused(
        &self,
        meeting: &MeetingKey,
        sender: SenderId,
        stream: StreamNumber,
    ) {
        let snapshot = self.snapshot.load();
        let Some(routes) = snapshot.meetings.get(meeting) else {
            return;
        };
        let source = CandidateRef { sender, stream };
        for &index in routes
            .by_source
            .get(&(sender, stream))
            .into_iter()
            .flatten()
        {
            if let Some(cell) = routes.cells.0.get(index) {
                cell.on_source_group_refused(source);
            }
        }
    }

    /// `sender`'s subscriber connection registered (`true`) or was removed
    /// (`false`) on this handler: its stream edges in `meeting` enter or
    /// leave pending (and request) accordingly. Called by the connection
    /// lifecycle AFTER the registry write.
    pub fn subscriber_presence(&self, meeting: &MeetingKey, sender: SenderId, present: bool) {
        let snapshot = self.snapshot.load();
        let Some(routes) = snapshot.meetings.get(meeting) else {
            return;
        };
        for (edge, cell) in routes.edges.iter().zip(&routes.cells.0) {
            if edge.subscriber.sender == sender {
                cell.set_subscriber_present(present);
            }
        }
    }

    /// A publisher binding registered in `meeting`: edges waiting for one of
    /// its sources become pending (and request once its source is seen).
    pub fn publisher_bound(&self, meeting: &MeetingKey) {
        if let Some(routes) = self.snapshot.load().meetings.get(meeting) {
            for cell in routes.cells() {
                cell.resettle();
            }
        }
    }

    /// Sample every installed edge's pending stamp and publish the pod-wide
    /// `mh_media_edges_pending` (count) and `mh_media_edge_pending_oldest_seconds`
    /// (max age; 0 when nothing is pending), and sum every edge's destination
    /// backlog into `mh_media_egress_stream_backlog_bytes`. Called from the
    /// session actor's tick — the single writer — never per frame or per chunk.
    /// Blind spot of the backlog sum: a writer still draining after its edge
    /// left the snapshot is not counted (a short tail).
    #[expect(
        clippy::cast_precision_loss,
        reason = "an edge count and a backlog byte sum (bounded by cap x stream ceiling) far below 2^52"
    )]
    pub fn publish_pending(&self, now: tokio::time::Instant) {
        let snapshot = self.snapshot.load();
        let mut count = 0usize;
        let mut oldest = std::time::Duration::ZERO;
        let mut backlog = 0usize;
        for routes in snapshot.meetings.values() {
            for cell in routes.cells() {
                if let Some(since) = cell.pending_since() {
                    count += 1;
                    oldest = oldest.max(now.saturating_duration_since(since));
                }
                backlog = backlog.saturating_add(cell.backlog_used());
            }
        }
        let handles = &self.edge_context.handles;
        handles.edges_pending().set(count as f64);
        handles.edge_pending_oldest().set(oldest.as_secs_f64());
        // Story 3 R-21: the stream backlog in use, summed on this same tick
        // from every destination budget (off the hot path; never per chunk).
        handles.egress_stream_backlog_bytes().set(backlog as f64);
    }

    /// `STREAM_PER_GROUP` source streams declared across every installed
    /// policy — `mh_media_stream_sources_declared`.
    #[must_use]
    pub fn stream_sources_declared(&self) -> usize {
        self.snapshot
            .load()
            .meetings
            .values()
            .map(|routes| routes.stream_sources_declared())
            .sum()
    }
}

#[cfg(test)]
mod tests;
