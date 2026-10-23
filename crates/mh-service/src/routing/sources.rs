//! Declared source streams (story 3 R-16, R-17, R-20; `internal.proto`
//! `SourceStream`).
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
//! There is ONE reading: what `source_streams` declares. An empty list
//! declares nothing, so every egress candidate is undeclared and the
//! registration rejects `rejected_transport_inconsistent` (MC-B deleted the
//! story-2 transitional reading with the superseded proto fields).
//!
//! # The label is publisher-set and unauthenticated
//!
//! Nothing here, or in the lookup that reads it, identifies a frame's TRUE
//! stream. The uplink relay id is chosen by the publisher, sits outside the
//! signed span, and MH never reads the payload or the `SFrame` key id. Why
//! per-kind server mute nevertheless holds, in slot terms, is stated once at
//! `internal.proto` `SourceStream` and not restated here.

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

/// Every source stream one registration declares (`source_streams`), in
/// request order — the ONE owner. The lookup index and the all-muted set are
/// derived from this at install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declarations {
    streams: Vec<SourceDecl>,
}

impl Declarations {
    /// The ONE lookup, at apply time and per frame: `index` is the
    /// `(sender, stream) -> position in streams` map built from THIS value
    /// ([`Self::build_index`]).
    pub(super) fn resolve(
        &self,
        index: &HashMap<(SenderId, StreamNumber), usize>,
        sender: SenderId,
        stream: StreamNumber,
    ) -> Option<SourceDecl> {
        index
            .get(&(sender, stream))
            .and_then(|&i| self.streams.get(i))
            .copied()
    }

    /// The per-frame index for [`Self::resolve`], derived from the one owner.
    /// Bounded by `MH_MAX_SOURCE_STREAMS_PER_MEETING` (the count check runs
    /// before parse) and freed with the meeting's routes.
    pub(super) fn build_index(&self) -> HashMap<(SenderId, StreamNumber), usize> {
        self.streams
            .iter()
            .enumerate()
            .map(|(i, d)| ((d.sender, d.stream), i))
            .collect()
    }

    /// Senders every frame of which drops `server_muted` — the sender-level
    /// shortcut, checked FIRST.
    ///
    /// **Non-vacuous.** A sender with ZERO declared streams is NOT in this set
    /// (a naive "all of its streams are `DROP_AT_INGRESS`" over an empty set is
    /// true, and would turn an undeclared sender's `unmatched_stream` into
    /// `server_muted`). Membership: at least one declared stream and every one
    /// `DROP_AT_INGRESS`.
    pub(super) fn all_muted(&self) -> HashSet<SenderId> {
        let mut not_muted: HashSet<SenderId> = HashSet::new();
        let mut declared: HashSet<SenderId> = HashSet::new();
        for decl in &self.streams {
            declared.insert(decl.sender);
            if decl.disposition != Disposition::DropAtIngress {
                not_muted.insert(decl.sender);
            }
        }
        declared.difference(&not_muted).copied().collect()
    }

    /// The per-stream echo (`RegisterMeetingResponse.applied_source_streams`).
    /// Called ONLY on the installed copy, so it is the applied set, never the
    /// received one. Not filtered by disposition: a PAUSED or `DROP_AT_INGRESS`
    /// stream is still declared, and echoed.
    #[must_use]
    pub fn echo(&self) -> Vec<SourceStreamEcho> {
        self.streams
            .iter()
            .map(|d| SourceStreamEcho {
                sender_id: u32::from(d.sender.get()),
                stream_number: u32::from(d.stream.get()),
                transport_mode: d.transport as i32,
            })
            .collect()
    }

    /// Set equality with `other`, for the equal-generation divergence
    /// detector (order-insensitive; counts are what that detector logs).
    pub(super) fn same_as(&self, other: &Self) -> bool {
        let a: HashSet<&SourceDecl> = self.streams.iter().collect();
        let b: HashSet<&SourceDecl> = other.streams.iter().collect();
        a == b
    }

    /// How many declared streams use `transport`. Counts only.
    #[must_use]
    pub fn count_transport(&self, transport: TransportMode) -> usize {
        self.streams
            .iter()
            .filter(|d| d.transport == transport)
            .count()
    }

    /// How many streams are declared. Counts only — for the divergence WARN.
    #[must_use]
    pub fn declared_count(&self) -> usize {
        self.streams.len()
    }
}

/// Parse `source_streams`. The COUNT is already bounded by the caller, before
/// any per-element work; this does the per-element work. An EMPTY list parses
/// to zero declarations — there is no fallback reading — so the caller's
/// every-candidate-declared walk rejects any egress candidate as
/// [`PolicyRejection::UndeclaredCandidate`].
///
/// # Errors
///
/// `rejected_invalid`-mapped rejections only: malformed ids, an UNSPECIFIED or
/// unknown transport or disposition, a duplicate `(sender, stream)`.
pub(super) fn parse_declared(
    request: &RegisterMeetingRequest,
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
    Ok(Declarations { streams })
}
