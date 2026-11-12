//! The meeting's handler set — MC-owned, versioned, ref-addressed — and which
//! of its handlers a participant is observed to be connected to (ADR-0036 §9;
//! ADR-0039 §3/§4, superseding §9's frozen set; story 2 R-33, story 4 R-12/R-17).
//!
//! # MC is the authority; GC's hand-off is read once
//!
//! A meeting's [`MeetingHandlers`] is built ONCE, when the meeting actor is
//! created, from GC's assignment (`AssignMeetingWithMh`, the same value MC
//! stores as Redis `MhAssignmentData`) via [`MeetingHandlers::from_handoff`].
//! Nothing re-reads it: no join, and no later change to the Redis record, can
//! alter a live meeting's set. From then on the set changes only through its
//! own mutators — [`MeetingHandlers::add`], [`MeetingHandlers::mark_active`],
//! [`MeetingHandlers::remove`] — whose triggers (detection, replacement,
//! planned transfer) are story-4 tasks MC-B2/MC-C/MC-F. The set MAY be empty
//! (a meeting awaiting a handler).
//!
//! # Per handler: a ref and a state
//!
//! - **`handler_ref`**: MC-assigned, non-zero, allocated per INCARNATION from
//!   one per-meeting counter that only moves forward and refuses (never wraps)
//!   on exhaustion. It is never derived from the handler id, the url or a hash,
//!   and never reused within the meeting — a new handler at a previously used
//!   address gets a new ref.
//! - **state**: `Registering` (in MC's set: heartbeats are OWNED and the
//!   binding gate admits it) until its push worker's first `RegisterMeeting`
//!   is acked, then `Active`. **Register before token** (L4-2): only Active
//!   handlers are ever client-visible.
//! - **removed record**: ids removed for the meeting's life, each with its
//!   [`RemovalTrigger`] (the RELEASED verdict's cause). One-way: a removed
//!   id is never re-added.
//!
//! # `handler_set_version` and when a set is emitted
//!
//! The version is the epoch of MC's INTERNAL set. It advances by exactly one on
//! every successful `add`, `mark_active` and `remove`, and on nothing else —
//! never on a token refresh, a join, a push, a slot or mute change. It starts at
//! 0, is never reset, and is `u64` (an overflow refuses, never wraps). Clients
//! see gaps; it correlates loss reports and is never an ordering key.
//!
//! **Emission rule** (for every caller, MC-B2/MC-C included): a client-facing
//! `MediaHandlerSet` is emitted iff the ACTIVE projection changed —
//! [`SetChange::active_changed`]. Removing a still-Registering handler bumps
//! the version but emits nothing, so no emitted set ever repeats the previous
//! Active list under a new version.
//!
//! # Two views, and one home for every client-facing handler fact
//!
//! - The INTERNAL view (Registering + Active): [`MeetingHandlers::resolve`],
//!   [`MeetingHandlers::iter`] — push workers, teardown, MH notifications.
//! - The CLIENT view (Active only): [`MeetingHandlers::client_handler`] and
//!   [`MeetingHandlers::client_snapshot`]. Every client-facing handler fact —
//!   the url and ref in `MediaHandlerSet`, the ref on each `SendTarget` and
//!   each `StreamAssignment` — comes from that one view, so every ref is a
//!   member of the set the client holds, and nothing names a handler before
//!   its registration was acked.
//!
//! Edges never land on a non-Active handler: observed connectivity is routable
//! only over Active members (`connectivity.rs` `routable`).
//!
//! # Sorted by construction, not by a parameter name
//!
//! Members are kept sorted by id, so the order in which GC/Redis enumerates
//! the assignment can never change anything MC computes. Nothing here or
//! downstream may depend on WHICH handler is first: the edge chooser
//! (`edges.rs`) is correct for any choice.
//!
//! # Connectivity is observed, and structurally cannot carry a wire string
//!
//! Which handlers a participant is connected to is what the HANDLERS told MC
//! (`NotifyParticipantConnected` / `NotifyParticipantDisconnected`), never
//! what a client reported. [`ConnectedHandlers`] is the only representation of
//! that set, and its only insert takes a `&HandlerEndpoint` — which can only be
//! obtained from [`MeetingHandlers`] (via [`MeetingHandlers::resolve`], an
//! exact byte comparison with no normalisation). An MH-asserted `handler_id`
//! outside the meeting's set therefore cannot become routing state: it is
//! unconstructible here, not merely checked for.
//!
//! **Residual (security S1b)**: every MH pod shares one service identity, so
//! handler identity is self-asserted among authenticated handlers. Exact-match
//! membership bounds the blast radius to misrouting INSIDE the meeting's own
//! handlers; it is never a cross-meeting lever. Closing it needs per-instance
//! service identity (`docs/TODO.md` §Media Path Obligations).
//!
//! # Server-derived only
//!
//! Every url here comes from GC's assignment — never from a pod ordinal and
//! never from a client. A client's `MediaConnectionUpdate` reports (by
//! `handler_ref`) **must never be a source for this set, nor for
//! [`ConnectedHandlers`]**: a client-chosen handler reaching a `SendTarget` is
//! a redirect primitive, and a client-controlled narrowing of connectivity is
//! a reachability lever over other participants.
//!
//! This is §9 edge placement, not §7 selection.

use super::assignment::HandlerId;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

/// One assigned media handler, as GC's assignment describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerEndpoint {
    /// The handler's identity (`MhEndpointInfo::mh_id`).
    pub id: HandlerId,
    /// The client-facing WebTransport url, exactly as the assignment
    /// advertises it.
    pub webtransport_url: String,
    /// The gRPC endpoint MC programs the handler through.
    pub grpc_endpoint: String,
}

/// Why one GC-supplied `MhAssignment` is not a usable handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentRejected {
    /// `mh_id` is empty.
    EmptyHandlerId,
    /// `webtransport_endpoint` (the url clients receive) is empty.
    EmptyWebtransportEndpoint,
    /// `grpc_endpoint` (where MC sends its bearer in `RegisterMeeting`) is
    /// empty.
    EmptyGrpcEndpoint,
}

impl AssignmentRejected {
    /// Bounded log value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EmptyHandlerId => "empty_mh_id",
            Self::EmptyWebtransportEndpoint => "empty_webtransport_endpoint",
            Self::EmptyGrpcEndpoint => "empty_grpc_endpoint",
        }
    }
}

/// THE one intake predicate for a GC-supplied handler (security S2, story 4
/// MC-C): BOTH sources — the meeting's hand-off (`AssignMeetingWithMh`, in
/// `grpc/mc_service.rs`) and a replacement grant (`AcquireMediaHandler`, in
/// `grpc/gc_client.rs`) — go through it, so neither can be stricter or looser
/// than the other. The endpoint receives MC's bearer (`RegisterMeeting`) and
/// its url goes to clients. GC mirrors it producer-side
/// (`selection_is_malformed`, gc-service `services/mc_assignment.rs`; ANCHOR).
impl TryFrom<&proto_gen::dark_tower::internal::v1::MhAssignment> for HandlerEndpoint {
    type Error = AssignmentRejected;

    /// The first empty field, in id -> url -> grpc order.
    fn try_from(
        assignment: &proto_gen::dark_tower::internal::v1::MhAssignment,
    ) -> Result<Self, Self::Error> {
        if assignment.mh_id.is_empty() {
            return Err(AssignmentRejected::EmptyHandlerId);
        }
        if assignment.webtransport_endpoint.is_empty() {
            return Err(AssignmentRejected::EmptyWebtransportEndpoint);
        }
        if assignment.grpc_endpoint.is_empty() {
            return Err(AssignmentRejected::EmptyGrpcEndpoint);
        }
        Ok(Self {
            id: HandlerId::new(assignment.mh_id.clone()),
            webtransport_url: assignment.webtransport_endpoint.clone(),
            grpc_endpoint: assignment.grpc_endpoint.clone(),
        })
    }
}

/// Why MC removed a handler from a meeting (ADR-0039 §2/§6). MC's domain
/// enum, which is also the fleet-wide `trigger` label vocabulary: its single
/// home is `common::observability::labels::RemovalTrigger`, re-exported here so
/// MC call sites read `media_routing::RemovalTrigger`. The wire `ReleaseCause`
/// is reached ONLY through [`ReleaseCauseExt::release_cause`], and
/// `MEETING_ENDED`/`UNSPECIFIED` are not removal triggers, so they are
/// unrepresentable here.
pub use common::observability::labels::RemovalTrigger;

/// MC's projection of a [`RemovalTrigger`] onto the wire. An extension trait
/// because the enum lives in `crates/common`, which has no proto-gen
/// dependency.
pub trait ReleaseCauseExt {
    /// The ONE projection onto the wire `ReleaseCause` (RELEASED verdicts,
    /// `EndMeetingRequest.cause`, `AcquireMediaHandlerRequest.trigger`).
    fn release_cause(self) -> proto_gen::dark_tower::internal::v1::ReleaseCause;
}

impl ReleaseCauseExt for RemovalTrigger {
    fn release_cause(self) -> proto_gen::dark_tower::internal::v1::ReleaseCause {
        use proto_gen::dark_tower::internal::v1::ReleaseCause;
        match self {
            Self::HeartbeatTimeout => ReleaseCause::HeartbeatTimeout,
            Self::ApplyLag => ReleaseCause::ApplyLag,
            Self::PlannedTransfer => ReleaseCause::PlannedTransfer,
        }
    }
}

/// A handler-set mutation was refused. Every refusal leaves the set (and its
/// version) unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerSetError {
    /// GC's hand-off names no handler. Only [`MeetingHandlers::from_handoff`]
    /// returns this: an assignment with nothing to place anyone on stays a
    /// loud contract violation. A LIVE set may be empty.
    Empty,
    /// The id is already a member. Placement and edge ownership are keyed by
    /// id, so a duplicate has no single meaning.
    DuplicateHandlerId,
    /// Another member already advertises this client url. The client looks
    /// transports up by url, so two members sharing one is ambiguous.
    DuplicateUrl,
    /// The id was removed from this meeting; removal is one-way.
    PreviouslyRemoved,
    /// The meeting's `handler_ref` space is exhausted. Refused, never wrapped:
    /// a reused ref would let a client confuse two incarnations.
    RefSpaceExhausted,
    /// `handler_set_version` would overflow. Refused, never wrapped.
    VersionExhausted,
}

impl std::fmt::Display for HandlerSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "no media handler is assigned to the meeting",
            Self::DuplicateHandlerId => "the handler is already in the meeting's set",
            Self::DuplicateUrl => "another handler in the meeting's set has the same url",
            Self::PreviouslyRemoved => "the handler was removed from this meeting",
            Self::RefSpaceExhausted => "the meeting's handler_ref space is exhausted",
            Self::VersionExhausted => "the meeting's handler_set_version is exhausted",
        })
    }
}

impl std::error::Error for HandlerSetError {}

/// A handler's registration state within the meeting (ADR-0039 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandlerState {
    /// In MC's set; its `RegisterMeeting` is not yet acked. Never client-visible.
    Registering,
    /// Its push worker's first `RegisterMeeting` was acked.
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Member {
    endpoint: HandlerEndpoint,
    handler_ref: NonZeroU32,
    state: HandlerState,
    /// When the first `RegisterMeeting` was confirmed (Registering → Active):
    /// the origin of the health function's silence clock (ADR-0039 §2). The
    /// ONE home of that instant.
    confirmed_at: Option<tokio::time::Instant>,
}

impl Member {
    fn is_active(&self) -> bool {
        self.state == HandlerState::Active
    }
}

/// The outcome of a successful mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetChange {
    /// The version after the change.
    pub version: u64,
    /// Did the Active (client-visible) projection change? The emission rule:
    /// a `MediaHandlerSet` goes out iff this is true.
    pub active_changed: bool,
}

/// One entry of a client-facing snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientHandlerEntry {
    /// The handler's identity: the `aud` of the `media_dial` token minted for
    /// this entry (ADR-0039 §5). MC-internal — NEVER projected onto the wire
    /// (the client sees only the url and the ref, S-4).
    pub handler_id: HandlerId,
    /// The client-facing url.
    pub media_handler_url: String,
    /// The handler's per-incarnation ref.
    pub handler_ref: NonZeroU32,
}

/// The client-facing snapshot: Active handlers only, in id order, and the
/// version it was taken at. The ONE source of `MediaHandlerSet`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientHandlerSet {
    /// `handler_set_version` at the time of the snapshot.
    pub version: u64,
    /// The Active handlers.
    pub handlers: Vec<ClientHandlerEntry>,
}

/// A meeting's MC-owned handler set (see the module doc).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingHandlers {
    /// Sorted by id; ids and client urls unique.
    members: Vec<Member>,
    /// Ids removed for the meeting's life (one-way), each with WHY — the
    /// RELEASED verdict's cause (ADR-0039 §6).
    removed: BTreeMap<HandlerId, RemovalTrigger>,
    version: u64,
    /// The next `handler_ref` to allocate; `None` once exhausted.
    next_ref: Option<NonZeroU32>,
}

impl Default for MeetingHandlers {
    fn default() -> Self {
        Self::empty()
    }
}

impl MeetingHandlers {
    /// An empty set at version 0.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            members: Vec::new(),
            removed: BTreeMap::new(),
            version: 0,
            next_ref: Some(NonZeroU32::MIN),
        }
    }

    /// Build a meeting's initial set from GC's hand-off: every handler enters
    /// as Registering, in id order (so the enumeration order of the hand-off
    /// never matters).
    ///
    /// # Errors
    ///
    /// [`HandlerSetError::Empty`] if the hand-off names no handler, or the
    /// first refusal [`Self::add`] returns.
    pub fn from_handoff(
        endpoints: impl IntoIterator<Item = HandlerEndpoint>,
    ) -> Result<Self, HandlerSetError> {
        let mut sorted: Vec<HandlerEndpoint> = endpoints.into_iter().collect();
        if sorted.is_empty() {
            return Err(HandlerSetError::Empty);
        }
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        let mut set = Self::empty();
        for endpoint in sorted {
            set.add(endpoint)?;
        }
        Ok(set)
    }

    /// Test fixture: a set whose every member is already Active.
    ///
    /// # Errors
    ///
    /// As [`Self::from_handoff`].
    #[cfg(test)]
    pub(crate) fn active_for_test(
        endpoints: impl IntoIterator<Item = HandlerEndpoint>,
    ) -> Result<Self, HandlerSetError> {
        let mut set = Self::from_handoff(endpoints)?;
        let refs: Vec<(HandlerId, NonZeroU32)> = set
            .members
            .iter()
            .map(|m| (m.endpoint.id.clone(), m.handler_ref))
            .collect();
        let now = tokio::time::Instant::now();
        for (id, handler_ref) in refs {
            set.mark_active(&id, handler_ref, now)?;
        }
        Ok(set)
    }

    /// The ONE place the version advances. Refuses rather than wraps.
    fn next_version(&self) -> Result<u64, HandlerSetError> {
        self.version
            .checked_add(1)
            .ok_or(HandlerSetError::VersionExhausted)
    }

    /// Add a handler as Registering, allocating its `handler_ref`.
    ///
    /// # Errors
    ///
    /// A duplicate id, a previously removed id, a duplicate client url, an
    /// exhausted ref space or version. Nothing changes on error.
    pub fn add(
        &mut self,
        endpoint: HandlerEndpoint,
    ) -> Result<(NonZeroU32, SetChange), HandlerSetError> {
        if self.removed.contains_key(&endpoint.id) {
            return Err(HandlerSetError::PreviouslyRemoved);
        }
        if self.members.iter().any(|m| m.endpoint.id == endpoint.id) {
            return Err(HandlerSetError::DuplicateHandlerId);
        }
        if self
            .members
            .iter()
            .any(|m| m.endpoint.webtransport_url == endpoint.webtransport_url)
        {
            return Err(HandlerSetError::DuplicateUrl);
        }
        let handler_ref = self.next_ref.ok_or(HandlerSetError::RefSpaceExhausted)?;
        let version = self.next_version()?;
        // Commit only after every fallible step.
        self.next_ref = handler_ref.checked_add(1);
        self.version = version;
        let at = self
            .members
            .partition_point(|m| m.endpoint.id < endpoint.id);
        self.members.insert(
            at,
            Member {
                endpoint,
                handler_ref,
                state: HandlerState::Registering,
                confirmed_at: None,
            },
        );
        Ok((
            handler_ref,
            SetChange {
                version,
                active_changed: false,
            },
        ))
    }

    /// Mark a Registering handler Active (its first `RegisterMeeting` was
    /// acked at `confirmed_at`). Keyed by id AND ref, so a late ack for a
    /// removed or replaced incarnation is a no-op. `Ok(None)` (no change, no
    /// bump) for an unknown id, a stale ref, or an already-Active handler.
    ///
    /// # Errors
    ///
    /// [`HandlerSetError::VersionExhausted`]; nothing changes.
    pub fn mark_active(
        &mut self,
        id: &HandlerId,
        handler_ref: NonZeroU32,
        confirmed_at: tokio::time::Instant,
    ) -> Result<Option<SetChange>, HandlerSetError> {
        let Some(at) = self
            .members
            .iter()
            .position(|m| &m.endpoint.id == id && m.handler_ref == handler_ref && !m.is_active())
        else {
            return Ok(None);
        };
        // Commit only after every fallible step: a no-op never reports
        // exhaustion.
        let next = self.next_version()?;
        if let Some(member) = self.members.get_mut(at) {
            member.state = HandlerState::Active;
            member.confirmed_at = Some(confirmed_at);
        }
        self.version = next;
        Ok(Some(SetChange {
            version: next,
            active_changed: true,
        }))
    }

    /// Remove a handler for the rest of the meeting (one-way), recording why.
    /// `Ok(None)` (no bump) if it is not a member. `active_changed` is whether
    /// it was Active.
    ///
    /// Production callers go through the meeting actor's ONE removal helper
    /// (`remove_handler`), which also drops the handler's liveness record —
    /// never call this directly from actor code.
    ///
    /// # Errors
    ///
    /// [`HandlerSetError::VersionExhausted`]; nothing changes.
    pub fn remove(
        &mut self,
        id: &HandlerId,
        trigger: RemovalTrigger,
    ) -> Result<Option<SetChange>, HandlerSetError> {
        let Some(at) = self.members.iter().position(|m| &m.endpoint.id == id) else {
            return Ok(None);
        };
        let next = self.next_version()?;
        let member = self.members.remove(at);
        let was_active = member.is_active();
        self.removed.insert(member.endpoint.id, trigger);
        self.version = next;
        Ok(Some(SetChange {
            version: next,
            active_changed: was_active,
        }))
    }

    /// The verdict a heartbeat from `wire_handler_id` gets (ADR-0039 §6), by
    /// exact match: a member (Registering included — the race gate) is OWNED;
    /// a removed id is RELEASED with its trigger; anything else on this live
    /// meeting is a misroute. Never mutates; never adds.
    #[must_use]
    pub fn heartbeat_verdict(&self, wire_handler_id: &str) -> super::HeartbeatVerdict {
        self.heartbeat_member(wire_handler_id)
            .map_or_else(|released| released, |_| super::HeartbeatVerdict::Owned)
    }

    /// [`Self::heartbeat_verdict`] carrying the resolved member on OWNED, so
    /// the caller's liveness write is keyed by a handler the set already
    /// proved a member (no second, fallible lookup). `Err` is the RELEASED
    /// verdict.
    ///
    /// # Errors
    ///
    /// The non-OWNED verdict: [`super::HeartbeatVerdict::Removed`] or
    /// [`super::HeartbeatVerdict::Unassigned`].
    pub fn heartbeat_member(
        &self,
        wire_handler_id: &str,
    ) -> Result<HandlerId, super::HeartbeatVerdict> {
        if let Some(endpoint) = self.resolve(wire_handler_id) {
            return Ok(endpoint.id.clone());
        }
        Err(match self.removed.get(&HandlerId::new(wire_handler_id)) {
            Some(trigger) => super::HeartbeatVerdict::Removed(*trigger),
            None => super::HeartbeatVerdict::Unassigned,
        })
    }

    /// Every id a replacement request must exclude (ADR-0039 §3): current
    /// members ∪ removed. A grant refused at admission is removed, so it is
    /// here too. A set: deduplicated by construction.
    #[must_use]
    pub fn excluded_ids(&self) -> BTreeSet<HandlerId> {
        self.members
            .iter()
            .map(|m| m.endpoint.id.clone())
            .chain(self.removed.keys().cloned())
            .collect()
    }

    /// Whether `id` is a member at incarnation `handler_ref` that is still
    /// Registering (its first `RegisterMeeting` not yet acked).
    #[must_use]
    pub fn is_registering(&self, id: &HandlerId, handler_ref: NonZeroU32) -> bool {
        self.member(id)
            .is_some_and(|m| !m.is_active() && m.handler_ref == handler_ref)
    }

    /// The current `handler_set_version`.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Every member (Registering and Active), in id order.
    pub fn iter(&self) -> impl Iterator<Item = &HandlerEndpoint> {
        self.members.iter().map(|m| &m.endpoint)
    }

    /// Every member (any state) with its ref, in id order — for callers that
    /// need both, so no second lookup can miss.
    pub fn iter_with_ref(&self) -> impl Iterator<Item = (&HandlerEndpoint, NonZeroU32)> {
        self.members.iter().map(|m| (&m.endpoint, m.handler_ref))
    }

    /// Every member id, in id order.
    pub fn ids(&self) -> impl Iterator<Item = &HandlerId> {
        self.members.iter().map(|m| &m.endpoint.id)
    }

    /// Every member still Registering, with its ref, in id order.
    pub fn registering(&self) -> impl Iterator<Item = (&HandlerId, NonZeroU32)> {
        self.members
            .iter()
            .filter(|m| !m.is_active())
            .map(|m| (&m.endpoint.id, m.handler_ref))
    }

    /// The one member lookup every accessor below is built on.
    fn member(&self, id: &HandlerId) -> Option<&Member> {
        self.members.iter().find(|m| &m.endpoint.id == id)
    }

    /// Look a member up by id (any state).
    #[must_use]
    pub fn get(&self, id: &HandlerId) -> Option<&HandlerEndpoint> {
        self.member(id).map(|m| &m.endpoint)
    }

    /// When a member's first `RegisterMeeting` was confirmed; `None` while it
    /// is Registering (or not a member).
    #[must_use]
    pub fn confirmed_at(&self, id: &HandlerId) -> Option<tokio::time::Instant> {
        self.member(id).and_then(|m| m.confirmed_at)
    }

    /// The `handler_ref` of a member (any state).
    #[must_use]
    pub fn handler_ref_of(&self, id: &HandlerId) -> Option<NonZeroU32> {
        self.member(id).map(|m| m.handler_ref)
    }

    /// Look an ACTIVE member up by id — the routable subset.
    #[must_use]
    pub fn active_endpoint(&self, id: &HandlerId) -> Option<&HandlerEndpoint> {
        self.member(id)
            .filter(|m| m.is_active())
            .map(|m| &m.endpoint)
    }

    /// The client-visible ref of an ACTIVE member — the ONE lookup every
    /// `SendTarget` and `StreamAssignment` uses, so every ref they carry is a
    /// member of the client's set. `None` for a Registering, removed or
    /// unknown handler.
    #[must_use]
    pub fn client_handler(&self, id: &HandlerId) -> Option<NonZeroU32> {
        self.member(id)
            .filter(|m| m.is_active())
            .map(|m| m.handler_ref)
    }

    /// Whether `handler_ref` is an ACTIVE member — i.e. in the client-facing
    /// set every participant was last told (ADR-0039 §4). EXACT comparison
    /// against the current incarnations; a removed or never-issued ref is not.
    /// The resolution a client-asserted `MhConnectionStatus.handler_ref` is
    /// checked with (diagnostic only, never routing).
    #[must_use]
    pub fn is_active_ref(&self, handler_ref: NonZeroU32) -> bool {
        self.members
            .iter()
            .any(|m| m.is_active() && m.handler_ref == handler_ref)
    }

    /// The client-facing snapshot: Active members only, with the version.
    #[must_use]
    pub fn client_snapshot(&self) -> ClientHandlerSet {
        ClientHandlerSet {
            version: self.version,
            handlers: self
                .members
                .iter()
                .filter(|m| m.is_active())
                .map(|m| ClientHandlerEntry {
                    handler_id: m.endpoint.id.clone(),
                    media_handler_url: m.endpoint.webtransport_url.clone(),
                    handler_ref: m.handler_ref,
                })
                .collect(),
        }
    }

    /// Resolve an MH-asserted handler id against this set (any state) by
    /// EXACT byte comparison. No trimming, no case folding, no prefix match.
    #[must_use]
    pub fn resolve(&self, wire_handler_id: &str) -> Option<&HandlerEndpoint> {
        self.members
            .iter()
            .find(|m| m.endpoint.id.as_str() == wire_handler_id)
            .map(|m| &m.endpoint)
    }

    /// How many members (any state).
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// How many Active members.
    #[must_use]
    pub fn active_len(&self) -> usize {
        self.members.iter().filter(|m| m.is_active()).count()
    }

    /// Does the set have no member? True for a meeting awaiting a handler.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Test fixture: an endpoint named `id`, the suite's one url convention
    /// (`https://{id}.example:4434`, gRPC `http://{id}.example:50053`).
    #[cfg(test)]
    pub(crate) fn test_endpoint(id: &str) -> HandlerEndpoint {
        HandlerEndpoint {
            id: HandlerId::new(id),
            webtransport_url: format!("https://{id}.example:4434"),
            grpc_endpoint: format!("http://{id}.example:50053"),
        }
    }

    /// Test fixture: a set of `ids` (via [`Self::test_endpoint`]), every
    /// member already Active.
    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    pub(crate) fn active_test_ids(ids: &[&str]) -> Self {
        Self::active_for_test(ids.iter().map(|id| Self::test_endpoint(id))).unwrap()
    }

    /// Test fixture: `id`'s ref in the client view (the value a send target or
    /// slot naming it carries). Panics if `id` is not an Active member.
    #[cfg(test)]
    pub(crate) fn client_ref_for_test(&self, id: &str) -> u32 {
        self.client_handler(&HandlerId::new(id))
            .expect("an Active member")
            .get()
    }

    /// Test fixture: start the ref counter at `next` (exhaustion tests).
    #[cfg(test)]
    fn with_next_ref(mut self, next: Option<NonZeroU32>) -> Self {
        self.next_ref = next;
        self
    }

    /// Test fixture: start the version at `version` (exhaustion tests).
    #[cfg(test)]
    fn with_version(mut self, version: u64) -> Self {
        self.version = version;
        self
    }
}

/// The handlers one participant is observed to be connected to.
///
/// The only insert takes a `&HandlerEndpoint`, which only [`MeetingHandlers`]
/// hands out, so a string copied off the wire can never become a member (see
/// the module doc). Removal takes a `&HandlerId`: removing can only narrow, so
/// it cannot inject anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ConnectedHandlers(BTreeSet<HandlerId>);

impl ConnectedHandlers {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a handler of the meeting's set. Returns `true` if it was new.
    pub fn insert(&mut self, endpoint: &HandlerEndpoint) -> bool {
        self.0.insert(endpoint.id.clone())
    }

    /// Remove a handler. Returns `true` if it was present.
    pub fn remove(&mut self, id: &HandlerId) -> bool {
        self.0.remove(id)
    }

    /// Is this handler in the set?
    #[must_use]
    pub fn contains(&self, id: &HandlerId) -> bool {
        self.0.contains(id)
    }

    /// Is the set empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many handlers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Every handler, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &HandlerId> {
        self.0.iter()
    }

    /// The handlers both sets contain, in id order.
    #[must_use]
    pub fn shared_with(&self, other: &Self) -> Vec<HandlerId> {
        self.0.intersection(&other.0).cloned().collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn endpoint(id: &str) -> HandlerEndpoint {
        MeetingHandlers::test_endpoint(id)
    }

    fn endpoint_at(id: &str, url: &str) -> HandlerEndpoint {
        HandlerEndpoint {
            id: HandlerId::new(id),
            webtransport_url: url.to_string(),
            grpc_endpoint: format!("http://{id}.example:50053"),
        }
    }

    fn hid(id: &str) -> HandlerId {
        HandlerId::new(id)
    }

    #[test]
    fn the_first_confirmation_is_recorded_once_on_the_member() {
        let mut set =
            MeetingHandlers::from_handoff([MeetingHandlers::test_endpoint("mh-0")]).unwrap();
        let r = set.handler_ref_of(&hid("mh-0")).unwrap();
        assert_eq!(
            set.confirmed_at(&hid("mh-0")),
            None,
            "Registering: unconfirmed"
        );
        let t = tokio::time::Instant::now();
        set.mark_active(&hid("mh-0"), r, t).unwrap();
        assert_eq!(set.confirmed_at(&hid("mh-0")), Some(t));
        let later = t + std::time::Duration::from_secs(5);
        assert_eq!(set.mark_active(&hid("mh-0"), r, later).unwrap(), None);
        assert_eq!(set.confirmed_at(&hid("mh-0")), Some(t), "never moved");
        assert_eq!(set.confirmed_at(&hid("mh-9")), None);
    }

    #[test]
    fn an_empty_handoff_is_rejected_but_an_empty_live_set_is_legal() {
        assert_eq!(
            MeetingHandlers::from_handoff(Vec::new()),
            Err(HandlerSetError::Empty)
        );
        let empty = MeetingHandlers::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.version(), 0);
        assert!(empty.client_snapshot().handlers.is_empty());
    }

    #[test]
    fn a_duplicate_handler_id_or_url_is_rejected_without_a_bump() {
        assert_eq!(
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-0")]),
            Err(HandlerSetError::DuplicateHandlerId)
        );
        let mut set = MeetingHandlers::from_handoff(vec![endpoint("mh-0")]).unwrap();
        let before = set.clone();
        assert_eq!(
            set.add(endpoint("mh-0")),
            Err(HandlerSetError::DuplicateHandlerId)
        );
        assert_eq!(
            set.add(endpoint_at("mh-1", "https://mh-0.example:4434")),
            Err(HandlerSetError::DuplicateUrl)
        );
        assert_eq!(set, before, "a refusal changes nothing, version included");
        // Positive arm: a distinct id and url is accepted and bumps by one.
        let (_, change) = set.add(endpoint("mh-1")).unwrap();
        assert_eq!(change.version, before.version() + 1);
        assert!(
            !change.active_changed,
            "a Registering add is not client-visible"
        );
    }

    /// The hand-off enumeration order is the one input whose order genuinely
    /// varies; sorting at construction is what makes it irrelevant.
    #[test]
    fn handoff_order_does_not_change_the_set() {
        let forward =
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let reversed =
            MeetingHandlers::from_handoff(vec![endpoint("mh-1"), endpoint("mh-0")]).unwrap();
        assert_eq!(forward, reversed);
    }

    #[test]
    fn refs_are_non_zero_distinct_and_never_reused_even_at_a_reused_address() {
        let mut set =
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let r0 = set.handler_ref_of(&hid("mh-0")).unwrap();
        let r1 = set.handler_ref_of(&hid("mh-1")).unwrap();
        assert_ne!(r0, r1);
        // Remove mh-0, then a NEW incarnation at the SAME address.
        set.remove(&hid("mh-0"), RemovalTrigger::HeartbeatTimeout)
            .unwrap()
            .unwrap();
        let (r2, _) = set
            .add(endpoint_at("mh-0b", "https://mh-0.example:4434"))
            .unwrap();
        assert!(
            r2 > r0 && r2 > r1,
            "a new incarnation gets a new, larger ref"
        );
    }

    /// ADR-0039 §6 verdict arms from the set's own state; each removed id
    /// keeps ITS OWN trigger (two removals, two triggers, so a hard-coded
    /// trigger cannot pass).
    #[test]
    fn heartbeat_verdict_reads_membership_and_the_removed_record() {
        use crate::media_routing::HeartbeatVerdict;
        let mut set = MeetingHandlers::from_handoff(vec![
            endpoint("mh-0"),
            endpoint("mh-1"),
            endpoint("mh-2"),
        ])
        .unwrap();
        let r = set.handler_ref_of(&hid("mh-2")).unwrap();
        set.mark_active(&hid("mh-2"), r, tokio::time::Instant::now())
            .unwrap();
        set.remove(&hid("mh-0"), RemovalTrigger::HeartbeatTimeout)
            .unwrap()
            .unwrap();
        set.remove(&hid("mh-1"), RemovalTrigger::PlannedTransfer)
            .unwrap()
            .unwrap();
        let before = set.clone();
        assert_eq!(
            set.heartbeat_verdict("mh-0"),
            HeartbeatVerdict::Removed(RemovalTrigger::HeartbeatTimeout)
        );
        assert_eq!(
            set.heartbeat_verdict("mh-1"),
            HeartbeatVerdict::Removed(RemovalTrigger::PlannedTransfer)
        );
        assert_eq!(set.heartbeat_verdict("mh-2"), HeartbeatVerdict::Owned);
        assert_eq!(set.heartbeat_verdict("mh-9"), HeartbeatVerdict::Unassigned);
        assert_eq!(set, before, "verdicts never mutate the set");
        let (_, _) = set.add(endpoint("mh-3")).unwrap();
        assert_eq!(
            set.heartbeat_verdict("mh-3"),
            HeartbeatVerdict::Owned,
            "Registering is OWNED (the race gate)"
        );
    }

    #[test]
    fn removal_is_one_way_and_a_removed_id_is_never_re_added() {
        let mut set = MeetingHandlers::from_handoff(vec![endpoint("mh-0")]).unwrap();
        let removed = set
            .remove(&hid("mh-0"), RemovalTrigger::HeartbeatTimeout)
            .unwrap()
            .unwrap();
        let before = set.clone();
        assert_eq!(
            set.add(endpoint("mh-0")),
            Err(HandlerSetError::PreviouslyRemoved)
        );
        assert_eq!(set, before, "refused re-add does not bump");
        assert_eq!(set.version(), removed.version);
        // Removing an unknown (or already removed) id is a no-op.
        assert_eq!(
            set.remove(&hid("mh-0"), RemovalTrigger::ApplyLag).unwrap(),
            None
        );
        assert_eq!(
            set.remove(&hid("mh-9"), RemovalTrigger::ApplyLag).unwrap(),
            None
        );
        assert_eq!(set, before);
    }

    #[test]
    fn mark_active_is_keyed_by_id_and_ref_and_bumps_once() {
        let mut set =
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        assert_eq!(set.version(), 2, "two adds");
        let r0 = set.handler_ref_of(&hid("mh-0")).unwrap();
        let r1 = set.handler_ref_of(&hid("mh-1")).unwrap();
        // Stale ref (another incarnation's) and unknown id: no-op, no bump.
        assert_eq!(
            set.mark_active(&hid("mh-0"), r1, tokio::time::Instant::now())
                .unwrap(),
            None
        );
        assert_eq!(
            set.mark_active(&hid("mh-9"), r0, tokio::time::Instant::now())
                .unwrap(),
            None
        );
        assert_eq!(set.version(), 2);
        // Positive arm.
        let change = set
            .mark_active(&hid("mh-0"), r0, tokio::time::Instant::now())
            .unwrap()
            .unwrap();
        assert_eq!(change.version, 3);
        assert!(change.active_changed);
        // Already Active: no-op, no bump.
        assert_eq!(
            set.mark_active(&hid("mh-0"), r0, tokio::time::Instant::now())
                .unwrap(),
            None
        );
        assert_eq!(set.version(), 3);
        // A late ack for a removed incarnation is a no-op.
        set.remove(&hid("mh-1"), RemovalTrigger::PlannedTransfer)
            .unwrap()
            .unwrap();
        let v = set.version();
        assert_eq!(
            set.mark_active(&hid("mh-1"), r1, tokio::time::Instant::now())
                .unwrap(),
            None
        );
        assert_eq!(set.version(), v);
    }

    #[test]
    fn is_registering_is_keyed_by_id_and_ref_and_false_once_active() {
        let mut set = MeetingHandlers::from_handoff([endpoint("h1")]).unwrap();
        let r = set.handler_ref_of(&hid("h1")).unwrap();
        assert!(set.is_registering(&hid("h1"), r));
        assert!(
            !set.is_registering(&hid("h1"), r.checked_add(1).unwrap()),
            "stale ref"
        );
        assert!(!set.is_registering(&hid("nope"), r));
        set.mark_active(&hid("h1"), r, tokio::time::Instant::now())
            .unwrap();
        assert!(!set.is_registering(&hid("h1"), r), "Active");
    }

    #[test]
    fn an_assignment_is_a_handler_only_with_id_url_and_grpc() {
        use proto_gen::dark_tower::internal::v1::MhAssignment;
        let ok = MhAssignment {
            mh_id: "h".into(),
            webtransport_endpoint: "https://h:4434".into(),
            grpc_endpoint: "http://h:50053".into(),
        };
        let e = HandlerEndpoint::try_from(&ok).unwrap();
        assert_eq!(
            (e.id.as_str(), e.webtransport_url.as_str()),
            ("h", "https://h:4434")
        );
        for (broken, want) in [
            (
                MhAssignment {
                    mh_id: String::new(),
                    ..ok.clone()
                },
                AssignmentRejected::EmptyHandlerId,
            ),
            (
                MhAssignment {
                    webtransport_endpoint: String::new(),
                    ..ok.clone()
                },
                AssignmentRejected::EmptyWebtransportEndpoint,
            ),
            (
                MhAssignment {
                    grpc_endpoint: String::new(),
                    ..ok.clone()
                },
                AssignmentRejected::EmptyGrpcEndpoint,
            ),
        ] {
            assert_eq!(HandlerEndpoint::try_from(&broken), Err(want));
        }
    }

    #[test]
    fn removing_a_registering_handler_bumps_but_is_not_client_visible() {
        let mut set =
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let r0 = set.handler_ref_of(&hid("mh-0")).unwrap();
        set.mark_active(&hid("mh-0"), r0, tokio::time::Instant::now())
            .unwrap();
        let registering = set
            .remove(&hid("mh-1"), RemovalTrigger::PlannedTransfer)
            .unwrap()
            .unwrap();
        assert!(!registering.active_changed);
        let active = set
            .remove(&hid("mh-0"), RemovalTrigger::HeartbeatTimeout)
            .unwrap()
            .unwrap();
        assert!(active.active_changed);
        assert_eq!(active.version, registering.version + 1);
    }

    #[test]
    fn the_client_view_lists_active_members_only() {
        let mut set =
            MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        assert!(set.client_snapshot().handlers.is_empty());
        assert!(set.client_handler(&hid("mh-0")).is_none());
        assert!(set.active_endpoint(&hid("mh-0")).is_none());
        // Internal view still resolves a Registering member.
        assert!(set.resolve("mh-0").is_some());
        let r1 = set.handler_ref_of(&hid("mh-1")).unwrap();
        set.mark_active(&hid("mh-1"), r1, tokio::time::Instant::now())
            .unwrap();
        let snap = set.client_snapshot();
        assert_eq!(snap.version, set.version());
        assert_eq!(
            snap.handlers,
            vec![ClientHandlerEntry {
                handler_id: hid("mh-1"),
                media_handler_url: "https://mh-1.example:4434".to_string(),
                handler_ref: r1,
            }]
        );
        assert_eq!(set.client_handler(&hid("mh-1")), Some(r1));
        assert!(set.client_handler(&hid("mh-0")).is_none());
        assert!(set.client_handler(&hid("mh-9")).is_none());
    }

    #[test]
    fn ref_and_version_exhaustion_refuse_and_never_wrap() {
        let mut set = MeetingHandlers::empty().with_next_ref(NonZeroU32::new(u32::MAX));
        let (last, _) = set.add(endpoint("mh-0")).unwrap();
        assert_eq!(last.get(), u32::MAX);
        let before = set.clone();
        assert_eq!(
            set.add(endpoint("mh-1")),
            Err(HandlerSetError::RefSpaceExhausted)
        );
        assert_eq!(set, before);

        let mut set = MeetingHandlers::from_handoff(vec![endpoint("mh-0")])
            .unwrap()
            .with_version(u64::MAX);
        let before = set.clone();
        let r0 = set.handler_ref_of(&hid("mh-0")).unwrap();
        assert_eq!(
            set.add(endpoint("mh-1")),
            Err(HandlerSetError::VersionExhausted)
        );
        assert_eq!(
            set.mark_active(&hid("mh-0"), r0, tokio::time::Instant::now()),
            Err(HandlerSetError::VersionExhausted)
        );
        assert_eq!(
            set.remove(&hid("mh-0"), RemovalTrigger::PlannedTransfer),
            Err(HandlerSetError::VersionExhausted)
        );
        assert_eq!(set, before);
        // A NO-OP at exhaustion is still the documented Ok(None).
        assert_eq!(
            set.remove(&hid("mh-9"), RemovalTrigger::PlannedTransfer),
            Ok(None)
        );
        assert_eq!(
            set.mark_active(&hid("mh-9"), r0, tokio::time::Instant::now()),
            Ok(None)
        );
    }

    /// S1a: resolution is exact byte equality. Near-misses an MH could assert
    /// — case, whitespace, a prefix, a suffix — resolve to nothing.
    #[test]
    fn resolve_is_exact_match_only() {
        let set = MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        assert_eq!(set.resolve("mh-1").map(|h| h.id.as_str()), Some("mh-1"));
        for near_miss in ["MH-1", " mh-1", "mh-1 ", "mh-", "mh-10", "", "mh-1\0"] {
            assert!(
                set.resolve(near_miss).is_none(),
                "{near_miss:?} must not resolve"
            );
        }
    }

    #[test]
    fn connected_handlers_only_admit_resolved_endpoints_and_intersect() {
        let set = MeetingHandlers::from_handoff(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let mut a = ConnectedHandlers::new();
        let mut b = ConnectedHandlers::new();
        assert!(a.insert(set.resolve("mh-0").unwrap()));
        assert!(a.insert(set.resolve("mh-1").unwrap()));
        assert!(!a.insert(set.resolve("mh-1").unwrap()), "idempotent");
        assert!(b.insert(set.resolve("mh-1").unwrap()));
        assert_eq!(a.shared_with(&b), vec![HandlerId::new("mh-1")]);
        assert!(b.remove(&HandlerId::new("mh-1")));
        assert!(a.shared_with(&b).is_empty());
    }
}
