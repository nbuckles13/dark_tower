//! The meeting's handler set, and which of its handlers a participant is
//! observed to be connected to (ADR-0036 §9; story 2 R-33).
//!
//! # One home for every client-facing handler url
//!
//! A meeting's handler SET is programmed once by GC (`assign_meeting`, frozen at
//! first join). Every participant is offered the WHOLE set and connects to all
//! of it it can. Every client-facing handler address — `JoinResponse.media_servers`
//! (the full set), each `StreamAssignment.media_handler_url` (the handler owning
//! that edge) and each `SendTarget.media_handler_url` (a handler owning one of
//! the sender's edges) — is read out of the same [`MeetingHandlers`] value via
//! [`MeetingHandlers::url_of`], so they are byte-identical strings by
//! construction. The client looks transports up by exact url, so a mismatch
//! among them is a silent no-send or no-receive.
//!
//! # Sorted by construction, not by a parameter name
//!
//! [`MeetingHandlers`] sorts once, at construction, and has no constructor that
//! accepts an unsorted list, so the order in which Redis enumerates
//! `MhAssignmentData.handlers` can never change anything MC computes. Nothing
//! here or downstream may depend on WHICH handler is first: the edge chooser
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
//! outside the meeting's frozen set therefore cannot become routing state: it
//! is unconstructible here, not merely checked for.
//!
//! **Residual (security S1b)**: every MH pod shares one service identity, so
//! handler identity is self-asserted among authenticated handlers. Exact-match
//! membership against the frozen set bounds the blast radius to misrouting
//! INSIDE the meeting's own registered handlers; it is never a cross-meeting
//! lever. Closing it needs per-instance service identity (`docs/TODO.md`
//! §Media Path Obligations).
//!
//! # Server-derived only
//!
//! Every url here comes from the meeting's registration (`MhAssignmentData`,
//! read from Redis, written from GC's assignment) — never from a pod ordinal
//! and never from a client. MC holds a similar-looking client-supplied url map,
//! `ParticipantActor::mh_statuses`, keyed by the truncated `mh_url` a client
//! reports in `MediaConnectionUpdate`. **That map must never be a source for
//! this one, nor for [`ConnectedHandlers`]**: a client-controlled url reaching
//! a `SendTarget` is a redirect primitive, and a client-controlled narrowing of
//! connectivity is a reachability lever over other participants.
//!
//! This is §9 edge placement, not §7 selection.

use super::assignment::HandlerId;
use std::collections::BTreeSet;

/// One assigned media handler, as the meeting's registration describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerEndpoint {
    /// The handler's identity (`MhEndpointInfo::mh_id`).
    pub id: HandlerId,
    /// The client-facing WebTransport url, exactly as the registration
    /// advertises it.
    pub webtransport_url: String,
    /// The gRPC endpoint MC programs the handler through.
    pub grpc_endpoint: String,
}

/// The registration could not be turned into a usable handler set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerSetError {
    /// No handler is assigned. There is nothing to place anyone on, and
    /// silently accepting that would make an unroutable meeting look healthy.
    Empty,
    /// Two entries name the same handler. Placement and edge ownership are
    /// keyed by id, so a duplicate has no single meaning.
    DuplicateHandlerId,
}

impl std::fmt::Display for HandlerSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("no media handler is assigned to the meeting"),
            Self::DuplicateHandlerId => {
                f.write_str("the meeting's handler assignment names one handler twice")
            }
        }
    }
}

impl std::error::Error for HandlerSetError {}

/// A meeting's assigned handler set: non-empty, duplicate-free, sorted by id.
///
/// Holding one of these is proof of all three. The meeting actor freezes it at
/// the first join, together with its urls, as ONE value, so a participant's
/// placed handler id and the url it is handed can never come from two different
/// versions of the registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingHandlers {
    sorted: Vec<HandlerEndpoint>,
}

impl MeetingHandlers {
    /// Validate and sort a registration's handler entries.
    ///
    /// # Errors
    ///
    /// [`HandlerSetError`] if the set is empty or names one handler twice.
    pub fn new(
        endpoints: impl IntoIterator<Item = HandlerEndpoint>,
    ) -> Result<Self, HandlerSetError> {
        let mut sorted: Vec<HandlerEndpoint> = endpoints.into_iter().collect();
        if sorted.is_empty() {
            return Err(HandlerSetError::Empty);
        }
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        if sorted.windows(2).any(|w| match w {
            [a, b] => a.id == b.id,
            _ => false,
        }) {
            return Err(HandlerSetError::DuplicateHandlerId);
        }
        Ok(Self { sorted })
    }

    /// Every handler, in id order.
    pub fn iter(&self) -> impl Iterator<Item = &HandlerEndpoint> {
        self.sorted.iter()
    }

    /// Every handler id, in id order.
    pub fn ids(&self) -> impl Iterator<Item = &HandlerId> {
        self.sorted.iter().map(|h| &h.id)
    }

    /// Look a handler up by id.
    #[must_use]
    pub fn get(&self, id: &HandlerId) -> Option<&HandlerEndpoint> {
        self.sorted.iter().find(|h| &h.id == id)
    }

    /// The client-facing url of a handler in this set.
    #[must_use]
    pub fn url_of(&self, id: &HandlerId) -> Option<&str> {
        self.get(id).map(|h| h.webtransport_url.as_str())
    }

    /// Resolve an MH-asserted handler id against this set by EXACT byte
    /// comparison. No trimming, no case folding, no prefix match: an id that is
    /// not byte-identical to a registered handler is not in the set.
    #[must_use]
    pub fn resolve(&self, wire_handler_id: &str) -> Option<&HandlerEndpoint> {
        self.sorted
            .iter()
            .find(|h| h.id.as_str() == wire_handler_id)
    }

    /// How many handlers the meeting is registered on.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    /// Never true: [`Self::new`] rejects an empty set. Present for the
    /// `len`/`is_empty` pairing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
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
        HandlerEndpoint {
            id: HandlerId::new(id),
            webtransport_url: format!("https://{id}.example:4434"),
            grpc_endpoint: format!("http://{id}.example:50053"),
        }
    }

    #[test]
    fn an_empty_set_is_rejected() {
        assert_eq!(
            MeetingHandlers::new(Vec::new()),
            Err(HandlerSetError::Empty)
        );
    }

    #[test]
    fn a_duplicate_handler_id_is_rejected() {
        assert_eq!(
            MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-0")]),
            Err(HandlerSetError::DuplicateHandlerId)
        );
    }

    /// The Redis enumeration order is the one input whose order genuinely
    /// varies; sorting at construction is what makes it irrelevant.
    #[test]
    fn registration_order_does_not_change_the_set() {
        let forward = MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let reversed = MeetingHandlers::new(vec![endpoint("mh-1"), endpoint("mh-0")]).unwrap();
        assert_eq!(forward, reversed);
    }

    /// S1a: resolution is exact byte equality. Near-misses an MH could assert
    /// — case, whitespace, a prefix, a suffix — resolve to nothing.
    #[test]
    fn resolve_is_exact_match_only() {
        let set = MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
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
        let set = MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
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

    #[test]
    fn url_lookup_reads_the_registration_verbatim() {
        let set = MeetingHandlers::new(vec![endpoint("mh-1")]).unwrap();
        assert_eq!(
            set.url_of(&HandlerId::new("mh-1")),
            Some("https://mh-1.example:4434")
        );
        assert_eq!(set.url_of(&HandlerId::new("mh-9")), None);
    }
}
