//! Declared source streams (story 3 R-16, R-17, R-20; `internal.proto`
//! `SourceStream`) and the one transitional reading of a registration that
//! declares none.
//!
//! # What lives here, and why it is a separate file
//!
//! The PARSED declaration of every stream MH may receive for one meeting: who
//! sends it (the meeting-scoped [`SenderId`]), which of their streams it is
//! (the publisher's uplink relay id, [`StreamNumber`]), the transport it
//! arrives on, and what MH does with it ([`Disposition`]). The lookup that
//! USES these — [`super::RoutingSnapshot::attribute`] — stays in `mod.rs`,
//! beside every other `MeetingKey`-scoped lookup, so the scoping discipline
//! keeps one home.
//!
//! It is its own file so the MC-B deletion of the transitional reading is one
//! function, one type and the enum arms the compiler then points at, in a file
//! small enough to read whole.
//!
//! # The label is publisher-set and unauthenticated
//!
//! Nothing here, or in the lookup that reads it, identifies a frame's TRUE
//! stream. The uplink relay id is chosen by the publisher, sits outside the
//! signed span, and MH never reads the payload or the `SFrame` key id. Why
//! per-kind server mute nevertheless holds, in slot terms, is stated once at
//! `internal.proto` `MutedSource` and not restated here.

use std::collections::{HashMap, HashSet};

use proto_gen::dark_tower::internal::v1::{
    RegisterMeetingRequest, SourceDisposition, SourceStreamEcho,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;

use super::{PolicyRejection, SenderId, StreamNumber};

/// What MH does with one declared source stream (`internal.proto`
/// `SourceDisposition`). Behaviour names: MH learns neither the stream's media
/// kind nor the moderation fact behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// Forward along this stream's edges.
    Forward,
    /// Client mute (R-17). NEVER dropped at ingress: a resume frame that beats
    /// MC's update must still reach subscribers. Arrivals are counted on
    /// `mh_media_paused_source_frames_total` and forwarded normally. Task 9
    /// adds the edge semantics (a switch to a paused source completes as
    /// "forward nothing").
    Paused,
    /// Server mute (R-16). Frames attributed to this stream drop after
    /// attribution and before any routing work, counted `server_muted`.
    DropAtIngress,
}

impl Disposition {
    /// Parse the wire enum. UNSPECIFIED **and any value this build does not
    /// know** reject: proto3 enums are open, and `unwrap_or_default` would turn
    /// an unknown value into a forwarding decision nobody made.
    fn from_wire(value: i32) -> Result<Self, PolicyRejection> {
        match SourceDisposition::try_from(value) {
            Ok(SourceDisposition::Forward) => Ok(Self::Forward),
            Ok(SourceDisposition::Paused) => Ok(Self::Paused),
            Ok(SourceDisposition::DropAtIngress) => Ok(Self::DropAtIngress),
            Ok(SourceDisposition::Unspecified) | Err(_) => {
                Err(PolicyRejection::UnspecifiedDisposition)
            }
        }
    }
}

/// One declared source stream, validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceDecl {
    /// The publisher, meeting-scoped.
    pub sender: SenderId,
    /// The publisher's stream — MC's send id, which is also the uplink relay
    /// id the publisher stamps.
    pub stream: StreamNumber,
    /// The transport this stream arrives on and its edges leave on. Never
    /// `Unspecified` (rejected at parse).
    pub transport: TransportMode,
    /// What MH does with it.
    pub disposition: Disposition,
}

/// Every source stream one registration declares, in one of two readings.
///
/// The `Transitional` arm is the story-2 reading of a registration carrying no
/// `source_streams` at all; it is built ONLY by [`transitional_story2_reading`]
/// and dies with it (MC-B). Every other use of this type goes through methods
/// whose `Transitional` arms delegate to [`TransitionalStory2`], so deleting
/// that type makes the compiler name every site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declarations {
    /// MC declared every stream (`source_streams` non-empty).
    Declared {
        /// The declarations, in request order — the ONE owner. The lookup
        /// index and the all-muted set are derived from this at install.
        streams: Vec<SourceDecl>,
        /// Senders in the superseded `server_muted_sources`, in request order.
        /// UNION reading (`internal.proto` field 7, NON-EMPTY bullet): every
        /// stream of such a sender is `DROP_AT_INGRESS` whatever `streams` says.
        /// Transitional input, deleted by MC-B with the field.
        union_muted: Vec<SenderId>,
    },
    /// The story-2 reading of an empty `source_streams`.
    Transitional(TransitionalStory2),
}

impl Declarations {
    /// The ONE lookup, at apply time and per frame: `index` is the
    /// `(sender, stream) -> position in streams` map built from THIS value
    /// ([`Self::build_index`]); the transitional reading needs none.
    pub(super) fn resolve(
        &self,
        index: &HashMap<(SenderId, StreamNumber), usize>,
        sender: SenderId,
        stream: StreamNumber,
    ) -> Option<SourceDecl> {
        match self {
            Self::Declared { streams, .. } => index
                .get(&(sender, stream))
                .and_then(|&i| streams.get(i))
                .copied(),
            Self::Transitional(_) => TransitionalStory2::resolve(sender, stream),
        }
    }

    /// The per-frame index for [`Self::resolve`], derived from the one owner.
    /// Bounded by `MH_MAX_SOURCE_STREAMS_PER_MEETING` (the count check runs
    /// before parse) and freed with the meeting's routes.
    pub(super) fn build_index(&self) -> HashMap<(SenderId, StreamNumber), usize> {
        match self {
            Self::Declared { streams, .. } => streams
                .iter()
                .enumerate()
                .map(|(i, d)| ((d.sender, d.stream), i))
                .collect(),
            Self::Transitional(_) => HashMap::new(),
        }
    }

    /// Senders every frame of which drops `server_muted` — the sender-level
    /// shortcut, checked FIRST.
    ///
    /// **Non-vacuous.** A sender with ZERO declared streams is NOT in this set
    /// (a naive "all of its streams are `DROP_AT_INGRESS`" over an empty set is
    /// true, and would turn an undeclared sender's `unmatched_stream` into
    /// `server_muted`). Membership: at least one declared stream and every one
    /// `DROP_AT_INGRESS`, OR union-muted.
    pub(super) fn all_muted(&self) -> HashSet<SenderId> {
        match self {
            Self::Declared {
                streams,
                union_muted,
            } => {
                let mut not_muted: HashSet<SenderId> = HashSet::new();
                let mut declared: HashSet<SenderId> = HashSet::new();
                for decl in streams {
                    declared.insert(decl.sender);
                    if decl.disposition != Disposition::DropAtIngress {
                        not_muted.insert(decl.sender);
                    }
                }
                let mut muted: HashSet<SenderId> =
                    declared.difference(&not_muted).copied().collect();
                muted.extend(union_muted.iter().copied());
                muted
            }
            Self::Transitional(reading) => reading.muted.iter().copied().collect(),
        }
    }

    /// The per-stream echo (`RegisterMeetingResponse.applied_source_streams`).
    /// Called ONLY on the installed copy, so it is the applied set, never the
    /// received one. Not filtered by disposition: a PAUSED or `DROP_AT_INGRESS`
    /// stream is still declared, and echoed.
    #[must_use]
    pub fn echo(&self) -> Vec<SourceStreamEcho> {
        match self {
            Self::Declared { streams, .. } => streams
                .iter()
                .map(|d| SourceStreamEcho {
                    sender_id: u32::from(d.sender.get()),
                    stream_number: u32::from(d.stream.get()),
                    transport_mode: d.transport as i32,
                })
                .collect(),
            Self::Transitional(reading) => reading.echo(),
        }
    }

    /// The superseded single-mode echo (`RegisterMeetingResponse.transport_mode`,
    /// field 5). The declared path ALWAYS echoes `Unspecified`: MC-A compares
    /// the per-stream echo, and an MC that forgot to stop comparing field 5
    /// then reads a mismatch (loud) rather than a derived mode that reads green
    /// for an audio-only meeting (silent).
    #[must_use]
    pub fn legacy_transport_echo(&self, edge_count: usize) -> TransportMode {
        match self {
            Self::Declared { .. } => TransportMode::Unspecified,
            Self::Transitional(_) => TransitionalStory2::transport_echo(edge_count),
        }
    }

    /// Set equality with `other`, for the equal-generation divergence
    /// detector (order-insensitive; counts are what that detector logs).
    pub(super) fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Declared {
                    streams: a,
                    union_muted: am,
                },
                Self::Declared {
                    streams: b,
                    union_muted: bm,
                },
            ) => {
                let sa: HashSet<&SourceDecl> = a.iter().collect();
                let sb: HashSet<&SourceDecl> = b.iter().collect();
                let ma: HashSet<&SenderId> = am.iter().collect();
                let mb: HashSet<&SenderId> = bm.iter().collect();
                sa == sb && ma == mb
            }
            (Self::Transitional(a), Self::Transitional(b)) => a.same_as(b),
            (Self::Declared { .. }, Self::Transitional(_))
            | (Self::Transitional(_), Self::Declared { .. }) => false,
        }
    }

    /// How many streams are declared (the transitional reading declares one
    /// per NAMED sender). Counts only — for the divergence WARN.
    #[must_use]
    pub fn declared_count(&self) -> usize {
        match self {
            Self::Declared { streams, .. } => streams.len(),
            Self::Transitional(reading) => reading.named.len(),
        }
    }
}

/// Parse a non-empty `source_streams`. The COUNT is already bounded by the
/// caller, before any per-element work; this does the per-element work.
///
/// `union_muted` is the already-parsed `server_muted_sources` (UNION reading).
///
/// # Errors
///
/// `rejected_invalid`-mapped rejections only: malformed ids, an UNSPECIFIED or
/// unknown transport or disposition, a duplicate `(sender, stream)`.
pub(super) fn parse_declared(
    request: &RegisterMeetingRequest,
    union_muted: Vec<SenderId>,
) -> Result<Declarations, PolicyRejection> {
    let mut streams = Vec::with_capacity(request.source_streams.len());
    let mut seen = HashSet::with_capacity(request.source_streams.len());
    for source in &request.source_streams {
        let sender = SenderId::from_wire(source.sender_id)?;
        let stream = StreamNumber::from_wire(source.stream_number)?;
        let transport = match TransportMode::try_from(source.transport_mode) {
            Ok(mode @ (TransportMode::Datagram | TransportMode::StreamPerGroup)) => mode,
            Ok(TransportMode::Unspecified) | Err(_) => {
                return Err(PolicyRejection::UnspecifiedSourceTransport)
            }
        };
        let disposition = Disposition::from_wire(source.disposition)?;
        if !seen.insert((sender, stream)) {
            return Err(PolicyRejection::DuplicateSourceStream);
        }
        streams.push(SourceDecl {
            sender,
            stream,
            transport,
            disposition,
        });
    }
    Ok(Declarations::Declared {
        streams,
        union_muted,
    })
}

// ===========================================================================
// THE TRANSITIONAL READING — transitional: deleted by MC-B with the superseded proto fields
// ===========================================================================

/// The story-2 reading of a registration whose `source_streams` is EMPTY (an
/// MC before MC-A). Stated normatively once, at `internal.proto`
/// `RegisterMeetingRequest.server_muted_sources` (TRANSITIONAL READING).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionalStory2 {
    /// `server_muted_sources`, in request order: `DROP_AT_INGRESS` on every
    /// stream (the sender-level shortcut covers them).
    muted: Vec<SenderId>,
    /// Every distinct sender the registration NAMES (candidates, then muted
    /// senders), first-appearance order — what the per-stream echo lists.
    named: Vec<SenderId>,
}

/// The stream the transitional reading declares for every sender.
///
/// `MAIN_AUDIO_STREAM_NUMBER` in MC; MH knows it ONLY here, and only for the
/// transitional reading.
const TRANSITIONAL_STREAM: u8 = 0;

/// **transitional: deleted by MC-B with the superseded proto fields**
///
/// Build the transitional reading of a registration carrying NO
/// `source_streams`: stream 0 on DATAGRAM is declared for EVERY
/// connection-bound sender, whether or not this registration names it (so an
/// unwatched sender's frames drop `no_subscriber`, never `unmatched_stream`),
/// and every sender in `server_muted_sources` is `DROP_AT_INGRESS`. That
/// reproduces story-2 behaviour, with one strictness: story-2 MH ignored the
/// uplink label, and this reading accepts ONLY label 0. Every current producer
/// stamps 0 — the SDK stamps its first declared slot id, which is 0 by
/// construction (`packages/sdk-core/src/media/lifecycle/AudioPipeline.ts`),
/// and the env-test fixture stamps `AUDIO_SEND_ID` — so MC-B and C-A1 inherit
/// that dependency.
///
/// The rule is applied at LOOKUP time ([`TransitionalStory2::resolve`]):
/// bound senders change without a re-registration, so nothing is materialised
/// per sender and there is nothing to bound.
///
/// `EgressStream.transport_mode` (field 6) is read ONLY here: UNSPECIFIED or an
/// unknown value rejects as before (`rejected_invalid`), and a value other
/// than DATAGRAM contradicts the synthesized declaration
/// (`rejected_transport_inconsistent`).
///
/// DEPLOY ORDER (story 3 R-25): this MH must be fully rolled out before MC-A
/// ships `source_streams`. Rolling MH back below this reading's introduction
/// after MC-A ships is unsafe: a pre-story MH rejects MC-A's mixed-transport
/// registrations. Runbook wording: the MH/MC deployment runbooks' cutover
/// order.
///
/// MC-B deletes this function, [`TransitionalStory2`], the
/// `Declarations::Transitional` arm and the superseded proto fields together.
/// After that, an empty `source_streams` with egress streams rejects as
/// `rejected_transport_inconsistent` through the generic undeclared-candidate
/// rule, with no new token. No other code may construct this reading or rely
/// on stream 0.
///
/// # Errors
///
/// The field-6 rejections above.
pub(super) fn transitional_story2_reading(
    request: &RegisterMeetingRequest,
    muted: Vec<SenderId>,
) -> Result<Declarations, PolicyRejection> {
    for stream in &request.egress_streams {
        match TransportMode::try_from(stream.transport_mode) {
            Ok(TransportMode::Datagram) => {}
            Ok(TransportMode::StreamPerGroup) => {
                return Err(PolicyRejection::TransitionalNonDatagramEgress)
            }
            Ok(TransportMode::Unspecified) | Err(_) => {
                return Err(PolicyRejection::UnspecifiedTransportMode)
            }
        }
    }

    // Widths were already validated by the caller's egress walk, so the
    // `from_wire` here cannot fail; it is not re-trusted, it is re-parsed.
    let mut named: Vec<SenderId> = Vec::new();
    let mut seen: HashSet<SenderId> = HashSet::new();
    let candidates = request
        .egress_streams
        .iter()
        .flat_map(|s| s.candidate_sources.iter().map(|c| c.sender_id));
    for wire in candidates {
        let sender = SenderId::from_wire(wire)?;
        if seen.insert(sender) {
            named.push(sender);
        }
    }
    for sender in &muted {
        if seen.insert(*sender) {
            named.push(*sender);
        }
    }

    Ok(Declarations::Transitional(TransitionalStory2 {
        muted,
        named,
    }))
}

impl TransitionalStory2 {
    /// The lookup-time rule: label 0 on DATAGRAM, FORWARD, for ANY sender.
    /// Any other label resolves to nothing (`unmatched_stream`). The muted
    /// senders never reach this: the sender-level shortcut drops them first.
    fn resolve(sender: SenderId, stream: StreamNumber) -> Option<SourceDecl> {
        (stream.get() == TRANSITIONAL_STREAM).then_some(SourceDecl {
            sender,
            stream,
            transport: TransportMode::Datagram,
            disposition: Disposition::Forward,
        })
    }

    /// One `{sender, 0, DATAGRAM}` per named sender.
    fn echo(&self) -> Vec<SourceStreamEcho> {
        self.named
            .iter()
            .map(|s| SourceStreamEcho {
                sender_id: u32::from(s.get()),
                stream_number: u32::from(TRANSITIONAL_STREAM),
                transport_mode: TransportMode::Datagram as i32,
            })
            .collect()
    }

    /// Story 2's field-5 echo: DATAGRAM when any egress was applied, else
    /// UNSPECIFIED. The story-2 MC compares it unconditionally.
    const fn transport_echo(edge_count: usize) -> TransportMode {
        if edge_count == 0 {
            TransportMode::Unspecified
        } else {
            TransportMode::Datagram
        }
    }

    fn same_as(&self, other: &Self) -> bool {
        let a: HashSet<&SenderId> = self.muted.iter().collect();
        let b: HashSet<&SenderId> = other.muted.iter().collect();
        a == b
    }
}
