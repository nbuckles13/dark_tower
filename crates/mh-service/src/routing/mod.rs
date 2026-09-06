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
//! `sender_id` is the load-bearing case. It is a **16-bit per-meeting ordinal**
//! (the `SFrame` key id is `sender_id`(16) | `stream`(8) | `generation`(40)), so
//! `sender_id` 5 exists *concurrently in every meeting on this handler*. A flat
//! `HashMap<SenderId, _>` is therefore not merely imprecise — it is a
//! **cross-meeting media-crossing primitive**, and cross-tenant leakage is the
//! one threat class a keyless relay does not otherwise expose.
//!
//! So the snapshot is a nest of maps, every field is private, and
//! [`RoutingSnapshot::for_each_source`] is the only lookup. It takes a
//! [`MeetingKey`] and a [`SenderId`] together; there is no function that
//! accepts a `SenderId` alone. **The wrong index is a missing function, not a
//! remembered rule.**
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
use proto_gen::dark_tower::internal::v1::RegisterMeetingRequest;
use proto_gen::dark_tower::signaling::v1::TransportMode;

use crate::config::PolicyLimits;

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
/// This is NOT [`SlotId`], which is 16-bit and lives in the relay region rather
/// than in the signed key id.
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
    pub priority_group: u32,
    /// Whether a superseding frame permits discarding queued frames for this
    /// egress. A **behaviour**, not a media type: MH is told what a stream
    /// does, never what it is (ADR-0036 §7 type-blindness).
    pub supersede_on_independent_frame: bool,
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
    /// The single transport mode all egress streams share. `None` exactly when
    /// there are no egress streams — an empty policy names no mode, and
    /// fabricating one would put a mode on the §8 echo that MH never applied.
    pub transport_mode: Option<TransportMode>,
    /// The complete egress set. Empty is legal and meaningful: "this handler
    /// forwards nothing for this meeting".
    pub edges: Vec<EgressEdge>,
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
    /// An egress stream named no transport mode. `TRANSPORT_MODE_UNSPECIFIED`
    /// is a rejection, never a datagram default.
    UnspecifiedTransportMode,
    /// Egress streams disagreed on transport mode. The §8 echo is one field
    /// for a per-egress property, so it is unambiguous only while a meeting's
    /// streams are homogeneous. Echoing one of N would make the two-ends-agree
    /// check silently meaningless, so MH rejects instead. **This scope limit
    /// expires when video lands** (§1: audio on datagrams, video on
    /// stream-per-group), at which point the field moves per-egress — which
    /// this reject forces to happen loudly.
    HeterogeneousTransportModes,
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
    pub const ALL: [Self; 11] = [
        Self::TooManyEgressStreams,
        Self::TooManyCandidateSources,
        Self::MissingSubscriber,
        Self::Id(IdError::SenderIdZero),
        Self::Id(IdError::SenderIdOutOfRange),
        Self::Id(IdError::SlotIdOutOfRange),
        Self::Id(IdError::StreamNumberOutOfRange),
        Self::DuplicateEgressStreamId,
        Self::DuplicateSubscriberSlot,
        Self::UnspecifiedTransportMode,
        Self::HeterogeneousTransportModes,
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
            Self::MissingSubscriber => "egress stream is missing its subscriber",
            Self::Id(IdError::SenderIdZero) => "sender_id 0 is reserved and never valid",
            Self::Id(IdError::SenderIdOutOfRange) => "sender_id is out of range",
            Self::Id(IdError::SlotIdOutOfRange) => "slot_id is out of range",
            Self::Id(IdError::StreamNumberOutOfRange) => "stream_number is out of range",
            Self::DuplicateEgressStreamId => "duplicate egress_stream_id",
            Self::DuplicateSubscriberSlot => "duplicate subscriber slot",
            Self::UnspecifiedTransportMode => {
                "transport_mode is required and must not be \
                 TRANSPORT_MODE_UNSPECIFIED"
            }
            Self::HeterogeneousTransportModes => "all egress streams must share one transport_mode",
        }
    }
}

impl MeetingPolicy {
    /// Parse and validate a registration into an installable policy.
    ///
    /// # Check order is load-bearing
    ///
    /// The two **count bounds run first**, before anything that iterates or
    /// allocates per element. `internal.proto` requires them "before building
    /// any routing table"; that is the floor rather than the ceiling, because
    /// duplicate detection builds `HashSet`s sized by the attacker-controlled
    /// repeated field — the same allocation the bound exists to prevent, merely
    /// moved earlier than the routing table.
    ///
    /// Order: counts -> shape -> identifier widths -> duplicates -> transport
    /// mode.
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

        let mut edges = Vec::with_capacity(request.egress_streams.len());
        let mut seen_ids = HashSet::with_capacity(request.egress_streams.len());
        let mut seen_slots = HashSet::with_capacity(request.egress_streams.len());
        let mut transport_mode: Option<TransportMode> = None;

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

            // ---- transport mode -----------------------------------------
            let mode = TransportMode::try_from(stream.transport_mode)
                .unwrap_or(TransportMode::Unspecified);
            if mode == TransportMode::Unspecified {
                return Err(PolicyRejection::UnspecifiedTransportMode);
            }
            match transport_mode {
                None => transport_mode = Some(mode),
                Some(existing) if existing == mode => {}
                Some(_) => return Err(PolicyRejection::HeterogeneousTransportModes),
            }

            edges.push(EgressEdge {
                egress_stream_id: stream.egress_stream_id,
                subscriber,
                candidates,
                priority_group: stream.priority_group,
                supersede_on_independent_frame: stream.supersede_on_independent_frame,
            });
        }

        Ok(Self {
            meeting: MeetingKey::new(&request.meeting_id),
            generation: request.policy_generation,
            transport_mode,
            edges,
        })
    }
}

// ---------------------------------------------------------------------------
// The published snapshot
// ---------------------------------------------------------------------------

/// The installed forwarding policy for **one** meeting.
///
/// All fields are private. A `pub by_sender` would degrade this module's
/// structural guarantee back into a convention: the point is that no caller
/// can reach a sender-keyed map except through
/// [`RoutingSnapshot::for_each_source`], which requires a [`MeetingKey`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingRoutes {
    generation: u64,
    transport_mode: Option<TransportMode>,
    edges: Vec<EgressEdge>,
    /// Ingress-side lookup: publishing sender -> indices into `edges`.
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
    by_sender: HashMap<SenderId, Vec<usize>>,
}

impl MeetingRoutes {
    fn build(policy: &MeetingPolicy) -> Self {
        let mut by_sender: HashMap<SenderId, Vec<usize>> = HashMap::new();
        for (index, edge) in policy.edges.iter().enumerate() {
            for candidate in &edge.candidates {
                by_sender.entry(candidate.sender).or_default().push(index);
            }
        }
        Self {
            generation: policy.generation,
            transport_mode: policy.transport_mode,
            edges: policy.edges.clone(),
            by_sender,
        }
    }

    /// The generation this meeting's live forward path reflects.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The transport mode applied to every egress stream of [`Self::generation`].
    #[must_use]
    pub const fn transport_mode(&self) -> Option<TransportMode> {
        self.transport_mode
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
    /// Visit every egress edge a sender's frames feed, **within one meeting**.
    ///
    /// This is the only sender lookup in the crate, and it is impossible to
    /// call without naming a meeting. A `sender_id` valid in another meeting
    /// does not resolve here — not because a check rejects it, but because the
    /// lookup never leaves this meeting's routes. If you only ever look inside
    /// one meeting, you cannot cross meetings.
    ///
    /// **Borrowing and non-allocating, because the forward path calls it on
    /// every frame** (ADR-0036 §11: zero allocation per frame). Returning
    /// `Vec<&EgressEdge>` would heap-allocate per frame; handing out
    /// `by_sender` would make the maps public and turn this module's structural
    /// guarantee back into a convention. The visitor is the shape that is both.
    ///
    /// The forward path **re-reads the table per frame and caches nothing**.
    /// That is not a performance oversight: ADR-0036 §7 enforces server mute
    /// and participant removal through this lookup, so a cached edge list that
    /// outlives a policy change is a mute bypass with a staleness window.
    ///
    /// Returns the number of edges visited, so a caller can distinguish "no
    /// edges for this sender" from "edges visited" without counting in the
    /// closure — the two are different drop reasons on the forward path.
    pub fn for_each_source<'a>(
        &'a self,
        meeting: &MeetingKey,
        sender: SenderId,
        mut visit: impl FnMut(&'a EgressEdge),
    ) -> usize {
        let Some(routes) = self.meetings.get(meeting) else {
            return 0;
        };
        let Some(indices) = routes.by_sender.get(&sender) else {
            return 0;
        };
        // Resolve through the MEETING-LOCAL slice. `indexing_slicing = "deny"`
        // forces `.get`, which is also what keeps a stale index from reading a
        // neighbouring meeting's edge if the invariant above is ever broken.
        let mut visited = 0;
        for &i in indices {
            if let Some(edge) = routes.edges.get(i) {
                visit(edge);
                visited += 1;
            }
        }
        visited
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

    /// The transport mode a meeting's live forward path applies.
    #[must_use]
    pub fn transport_mode_for(&self, meeting: &MeetingKey) -> Option<TransportMode> {
        self.meetings.get(meeting).and_then(|r| r.transport_mode)
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
    fn with_policy(&self, policy: &MeetingPolicy) -> Self {
        // Deliberately the SAME function the aggregate bound is tested
        // against, not a second copy of the same subtract-then-add. Two copies
        // would let the accounting and the check drift, and the direction that
        // drift breaks in is silent: the bound would be enforced against a
        // total the snapshot does not actually carry.
        let total_edges = self.projected_total_edges(policy);
        let mut meetings = self.meetings.clone();
        meetings.insert(
            policy.meeting.clone(),
            Arc::new(MeetingRoutes::build(policy)),
        );
        Self {
            meetings,
            total_edges,
        }
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
        let displaced = self
            .meetings
            .get(&policy.meeting)
            .map_or(0, |r| r.edges.len());
        // The two halves of "fail loudly, degrade safely", deliberately split.
        //
        // `saturating_sub` rather than `-` in PRODUCTION: `displaced <=
        // total_edges` holds because `with_policy` is the only writer of
        // either, but a plain subtraction turns any future break of that
        // invariant into a release-mode wrap to `usize::MAX` — which reads as
        // "the handler is over its aggregate bound" and refuses EVERY apply on
        // the pod, permanently and silently.
        //
        // The `debug_assert` is there because saturating alone would mask the
        // break in the other direction too: an under-reported projection means
        // the bound is enforced against a total lower than reality and the pod
        // quietly runs over it. Every test exercising this path runs in debug,
        // so a broken invariant is loud where it can be fixed and safe where
        // it cannot.
        debug_assert!(
            displaced <= self.total_edges,
            "edge accounting broke: one meeting holds {displaced} edges of a {} handler total",
            self.total_edges
        );
        self.total_edges.saturating_sub(displaced) + policy.edges.len()
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
}

impl Default for RoutingTable {
    fn default() -> Self {
        Self::new()
    }
}

impl RoutingTable {
    /// A table with no policy installed for any meeting.
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(RoutingSnapshot::default()),
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
        let next = self.snapshot.load().with_policy(policy);
        self.snapshot.store(Arc::new(next));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    // One fixture home for all three test sites that build these messages —
    // see `mh_test_utils::media_policy` for why.
    use mh_test_utils::media_policy::{egress, register_request as request};
    use proto_gen::dark_tower::internal::v1::{CandidateSource, EgressStream};

    fn sender(v: u32) -> SenderId {
        SenderId::from_wire(v).unwrap()
    }

    /// Collect [`RoutingSnapshot::for_each_source`] into a `Vec` **at the test
    /// site**.
    ///
    /// Deliberately here and not on the type: an allocating traversal on the
    /// snapshot is a footgun the per-frame forward path would have to remember
    /// not to call, and this module's whole posture is that the wrong call is a
    /// missing function. A test may allocate; the hot path may not.
    fn sources_for<'a>(
        snapshot: &'a RoutingSnapshot,
        meeting: &MeetingKey,
        sender: SenderId,
    ) -> Vec<&'a EgressEdge> {
        let mut out = Vec::new();
        snapshot.for_each_source(meeting, sender, |edge| out.push(edge));
        out
    }

    fn policy(meeting: &str, generation: u64, streams: Vec<EgressStream>) -> MeetingPolicy {
        MeetingPolicy::from_request(
            &request(meeting, generation, streams),
            &PolicyLimits::default(),
        )
        .unwrap()
    }

    // -- identifier widths ------------------------------------------------

    #[test]
    fn sender_id_rejects_zero_and_over_range_never_truncates() {
        assert_eq!(SenderId::from_wire(0), Err(IdError::SenderIdZero));
        assert_eq!(
            SenderId::from_wire(65_536),
            Err(IdError::SenderIdOutOfRange)
        );
        // 65536 must NOT wrap to 0, and 65537 must NOT wrap to 1.
        assert_eq!(
            SenderId::from_wire(65_537),
            Err(IdError::SenderIdOutOfRange)
        );
        assert_eq!(SenderId::from_wire(65_535).unwrap().get(), 65_535);
        assert_eq!(SenderId::from_wire(1).unwrap().get(), 1);
    }

    #[test]
    fn slot_id_allows_zero_and_rejects_over_range() {
        // Zero IS valid for a slot — two subscribers both choosing slot 0 is
        // normal, which is why this differs from SenderId.
        assert_eq!(SlotId::from_wire(0).unwrap().get(), 0);
        assert_eq!(SlotId::from_wire(65_535).unwrap().get(), 65_535);
        assert_eq!(SlotId::from_wire(65_536), Err(IdError::SlotIdOutOfRange));
    }

    #[test]
    fn stream_number_is_eight_bit_and_rejects_never_truncates() {
        assert_eq!(StreamNumber::from_wire(255).unwrap().get(), 255);
        // 256 must not alias stream 0 of the same sender.
        assert_eq!(
            StreamNumber::from_wire(256),
            Err(IdError::StreamNumberOutOfRange)
        );
    }

    // -- the two-meeting acceptance pin -----------------------------------

    /// **The acceptance pin for `docs/TODO.md` §Media Path Obligations (a).**
    ///
    /// Two meetings are registered into ONE routing table and ONE published
    /// snapshot — two separately-constructed tables would pass vacuously and
    /// pin nothing.
    ///
    /// Both arms are asserted deliberately. Without the positive arm an
    /// implementation that resolves nothing anywhere also passes, so the
    /// negative arm alone proves nothing.
    ///
    /// The corpse this test is looking for: a flat
    /// `HashMap<SenderId, Vec<EgressEdge>>` at snapshot level instead of the
    /// per-meeting nesting. Under that implementation meeting B's lookup for
    /// sender 5 returns meeting A's edge, and one meeting's media reaches
    /// another meeting's subscriber. **No other validation layer can catch
    /// this**: loopback is one meeting with one participant, so the correct and
    /// the leaking implementations are observationally identical everywhere
    /// except a deliberately multi-meeting fixture.
    #[test]
    fn sender_id_valid_in_one_meeting_does_not_resolve_in_another() {
        let table = RoutingTable::new();
        table.install(&policy("meeting-a", 1, vec![egress(1, 5, 0, 5)]));
        table.install(&policy("meeting-b", 1, vec![egress(1, 7, 0, 7)]));

        let snapshot = table.load();
        let meeting_a = MeetingKey::new("meeting-a");
        let meeting_b = MeetingKey::new("meeting-b");

        // Positive arm: sender 5 DOES resolve inside meeting A.
        let in_a = sources_for(&snapshot, &meeting_a, sender(5));
        assert_eq!(
            in_a.len(),
            1,
            "sender 5 must resolve within its own meeting"
        );
        assert_eq!(in_a[0].subscriber.sender, sender(5));

        // Negative arm: the same sender id resolves to NOTHING in meeting B,
        // even though meeting B has a live policy of its own.
        let in_b = sources_for(&snapshot, &meeting_b, sender(5));
        assert!(
            in_b.is_empty(),
            "sender 5 is meeting A's ordinal; resolving it in meeting B is cross-tenant leakage"
        );

        // And meeting B's own sender still resolves, so the negative arm above
        // is not just "meeting B is empty".
        assert_eq!(sources_for(&snapshot, &meeting_b, sender(7)).len(), 1);
        assert!(sources_for(&snapshot, &meeting_a, sender(7)).is_empty());
    }

    // -- structural rejects -----------------------------------------------

    #[test]
    fn duplicate_egress_stream_id_rejects_whole_registration() {
        let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(1, 6, 1, 6)]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::DuplicateEgressStreamId)
        );
    }

    #[test]
    fn duplicate_subscriber_slot_rejects_whole_registration() {
        // Same (sender, slot) twice means "two sources into one slot".
        let req = request("m", 1, vec![egress(1, 5, 3, 5), egress(2, 5, 3, 6)]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::DuplicateSubscriberSlot)
        );
    }

    #[test]
    fn same_slot_id_for_different_subscribers_is_legal() {
        // The reciprocal of the previous test: slot ids are subscriber-scoped,
        // so two subscribers both choosing slot 0 must NOT collide.
        let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)]);
        assert!(MeetingPolicy::from_request(&req, &PolicyLimits::default()).is_ok());
    }

    #[test]
    fn missing_subscriber_rejects() {
        let mut stream = egress(1, 5, 0, 5);
        stream.subscriber = None;
        let req = request("m", 1, vec![stream]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::MissingSubscriber)
        );
    }

    #[test]
    fn malformed_sender_id_rejects_on_subscriber_and_on_candidate() {
        let req = request("m", 1, vec![egress(1, 0, 0, 5)]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::Id(IdError::SenderIdZero))
        );
        // The candidate side is the ingress-side lookup, where a global index
        // is most tempting — it carries the identical rule.
        let req = request("m", 1, vec![egress(1, 5, 0, 0)]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::Id(IdError::SenderIdZero))
        );
    }

    #[test]
    fn over_wide_stream_number_rejects() {
        let mut stream = egress(1, 5, 0, 5);
        stream.candidate_sources[0].stream_number = 256;
        let req = request("m", 1, vec![stream]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::Id(IdError::StreamNumberOutOfRange))
        );
    }

    #[test]
    fn unspecified_transport_mode_rejects_and_does_not_default_to_datagram() {
        let mut stream = egress(1, 5, 0, 5);
        stream.transport_mode = TransportMode::Unspecified as i32;
        let req = request("m", 1, vec![stream]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::UnspecifiedTransportMode)
        );
    }

    #[test]
    fn unknown_transport_mode_value_rejects_rather_than_resolving() {
        let mut stream = egress(1, 5, 0, 5);
        stream.transport_mode = 9_999;
        let req = request("m", 1, vec![stream]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::UnspecifiedTransportMode)
        );
    }

    #[test]
    fn heterogeneous_transport_modes_reject() {
        let mut second = egress(2, 6, 0, 6);
        second.transport_mode = TransportMode::StreamPerGroup as i32;
        let req = request("m", 1, vec![egress(1, 5, 0, 5), second]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &PolicyLimits::default()),
            Err(PolicyRejection::HeterogeneousTransportModes)
        );
    }

    #[test]
    fn count_bounds_reject_before_any_per_element_work() {
        let limits = PolicyLimits {
            max_egress_streams_per_meeting: 1,
            ..PolicyLimits::default()
        };
        // Two streams that ALSO duplicate their egress_stream_id: the count
        // bound must be what fires, proving it ran before duplicate detection
        // allocated a HashSet sized by the repeated field.
        let req = request("m", 1, vec![egress(1, 5, 0, 5), egress(1, 6, 1, 6)]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &limits),
            Err(PolicyRejection::TooManyEgressStreams)
        );

        let limits = PolicyLimits {
            max_candidate_sources_per_egress: 1,
            ..PolicyLimits::default()
        };
        let mut stream = egress(1, 5, 0, 5);
        stream.candidate_sources.push(CandidateSource {
            sender_id: 0, // malformed, but the COUNT must fire first
            stream_number: 0,
        });
        let req = request("m", 1, vec![stream]);
        assert_eq!(
            MeetingPolicy::from_request(&req, &limits),
            Err(PolicyRejection::TooManyCandidateSources)
        );
    }

    #[test]
    fn empty_egress_set_is_legal_and_names_no_transport_mode() {
        let parsed =
            MeetingPolicy::from_request(&request("m", 1, vec![]), &PolicyLimits::default())
                .unwrap();
        assert!(parsed.edges.is_empty());
        assert_eq!(
            parsed.transport_mode, None,
            "an empty policy applies no mode; fabricating one would put a mode on the §8 echo \
             that MH never applied"
        );
    }

    #[test]
    fn reject_reasons_are_static_and_carry_no_request_data() {
        // A bounded reason string is what keeps identities out of outbound
        // status messages. `reason()` returning `&'static str` makes formatting
        // request data into it impossible by construction; this walks every
        // variant, sourced from `ALL` so the list cannot go short.
        for rejection in PolicyRejection::ALL {
            let reason = rejection.reason();
            assert!(
                !reason.is_empty(),
                "every rejection needs an operator reason"
            );
            // Multi-line literals in this table have been wrong before: an
            // embedded newline makes the outbound gRPC status multi-line and
            // breaks line-oriented log parsing of the `reason` field.
            assert!(
                !reason.contains('\n'),
                "reject reason must be a single line: {reason:?}"
            );
        }

        // The reasons must also be DISTINCT, or an operator reading one cannot
        // tell which check refused the registration.
        let reasons: HashSet<&str> = PolicyRejection::ALL.iter().map(|r| r.reason()).collect();
        assert_eq!(
            reasons.len(),
            PolicyRejection::ALL.len(),
            "two rejections share a reason string"
        );

        // And every `IdError` reaches a distinct one through the `From` impl,
        // which is the arm that was silently uncovered before.
        for id_error in IdError::ALL {
            assert!(PolicyRejection::ALL.contains(&PolicyRejection::from(id_error)));
        }
    }

    // -- snapshot behaviour -----------------------------------------------

    #[test]
    fn install_publishes_generation_and_transport_mode() {
        let table = RoutingTable::new();
        let meeting = MeetingKey::new("m");
        assert_eq!(table.load().generation_for(&meeting), 0);
        assert_eq!(table.load().transport_mode_for(&meeting), None);

        table.install(&policy("m", 7, vec![egress(1, 5, 0, 5)]));
        assert_eq!(table.load().generation_for(&meeting), 7);
        assert_eq!(
            table.load().transport_mode_for(&meeting),
            Some(TransportMode::Datagram)
        );
    }

    #[test]
    fn installing_one_meeting_leaves_another_meetings_routes_untouched() {
        let table = RoutingTable::new();
        table.install(&policy("a", 1, vec![egress(1, 5, 0, 5)]));
        let a_before = Arc::clone(table.load().routes_for(&MeetingKey::new("a")).unwrap());

        table.install(&policy("b", 1, vec![egress(1, 7, 0, 7)]));
        let a_after = Arc::clone(table.load().routes_for(&MeetingKey::new("a")).unwrap());

        assert!(
            Arc::ptr_eq(&a_before, &a_after),
            "installing meeting B must carry meeting A's routes over by pointer, not rebuild them"
        );
    }

    #[test]
    fn projected_total_subtracts_the_meetings_own_current_edges() {
        let table = RoutingTable::new();
        let three = vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6), egress(3, 7, 0, 7)];
        table.install(&policy("m", 1, three.clone()));
        assert_eq!(table.load().total_edges(), 3);

        // An IDENTICAL re-assert must project to 3, not 6. Without the
        // subtraction, ADR-0036 §8's <=10s re-assert cadence would trip the
        // aggregate bound for every meeting in turn once the handler is half
        // full — and that defect passes every single-meeting test that only
        // ever registers once.
        let reassert = policy("m", 2, three);
        assert_eq!(table.load().projected_total_edges(&reassert), 3);

        // A different meeting genuinely adds.
        let other = policy("other", 1, vec![egress(1, 9, 0, 9)]);
        assert_eq!(table.load().projected_total_edges(&other), 4);
    }

    #[test]
    fn total_edges_tracks_replacement_not_accumulation() {
        let table = RoutingTable::new();
        table.install(&policy(
            "m",
            1,
            vec![egress(1, 5, 0, 5), egress(2, 6, 0, 6)],
        ));
        assert_eq!(table.load().total_edges(), 2);
        table.install(&policy("m", 2, vec![egress(1, 5, 0, 5)]));
        assert_eq!(table.load().total_edges(), 1);
        table.install(&policy("m", 3, vec![]));
        assert_eq!(table.load().total_edges(), 0);
        assert_eq!(table.load().meeting_count(), 1);
    }

    #[test]
    fn multiple_candidates_all_index_the_same_edge() {
        let mut stream = egress(1, 5, 0, 5);
        stream.candidate_sources.push(CandidateSource {
            sender_id: 6,
            stream_number: 1,
        });
        let table = RoutingTable::new();
        table.install(&policy("m", 1, vec![stream]));
        let snapshot = table.load();
        let meeting = MeetingKey::new("m");
        assert_eq!(sources_for(&snapshot, &meeting, sender(5)).len(), 1);
        assert_eq!(sources_for(&snapshot, &meeting, sender(6)).len(), 1);
        assert!(sources_for(&snapshot, &meeting, sender(8)).is_empty());
    }
}
