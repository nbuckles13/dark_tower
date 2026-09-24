//! Which media handler each participant is placed on (ADR-0036 §9; story 2
//! R-33).
//!
//! # One home for placement
//!
//! A meeting's handler SET is programmed once by GC (`assign_meeting`, frozen at
//! first join). Placement picks ONE handler of that set per participant, and
//! this module is the only place that decides it. Every client-facing handler
//! address — `JoinResponse.media_servers`, `StreamAssignment.media_handler_url`
//! and `SendTarget.media_handler_url` — is read out of the same
//! [`MeetingHandlers`] value and the participant's placed handler, so the three
//! are byte-identical strings by construction. The client looks transports up
//! by exact url, so a mismatch among them is a silent no-send or no-receive.
//!
//! # Sorted by construction, not by a parameter name
//!
//! [`MeetingHandlers`] sorts once, at construction, and has no constructor that
//! accepts an unsorted list. Round-robin placement indexes into that order, so
//! the order in which Redis enumerates `MhAssignmentData.handlers` can never
//! change which handler a participant lands on. That used to be a prose control
//! beside an unsorted `media_servers` list; it is now a property of the type.
//!
//! # Server-derived only
//!
//! Every url here comes from the meeting's registration (`MhAssignmentData`,
//! read from Redis, written from GC's assignment) — never from a pod ordinal
//! and never from a client. MC holds a similar-looking client-supplied url map,
//! `ParticipantActor::mh_statuses`, keyed by the truncated `mh_url` a client
//! reports in `MediaConnectionUpdate`. **That map must never be a source for
//! this one**: a client-controlled url reaching a `SendTarget` is a redirect
//! primitive.
//!
//! This is §9 *placement*, not §7 *selection*.

use super::assignment::HandlerId;
use super::slots::JoinRank;

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

    /// Round-robin placement keyed by join-order rank over the sorted set.
    ///
    /// A single-handler meeting places everyone on that handler; a two-handler
    /// meeting alternates. Co-location-preferred placement (fill one handler to
    /// its egress ceiling, then spill) is deferred: it needs the per-handler
    /// stream ceiling exposed to placement (`docs/TODO.md` §Media Path
    /// Obligations).
    ///
    /// `None` only if the set were empty, which [`Self::new`] makes impossible;
    /// the `Option` is here so the arithmetic has no panicking path.
    #[must_use]
    pub fn place(&self, rank: JoinRank) -> Option<&HandlerEndpoint> {
        let len = u64::try_from(self.sorted.len()).ok()?;
        let index = rank.get().checked_rem(len)?;
        self.sorted.get(usize::try_from(index).ok()?)
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

    fn rank(n: u64) -> JoinRank {
        JoinRank::from_raw(n)
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

    #[test]
    fn a_single_handler_meeting_places_everyone_on_it() {
        let set = MeetingHandlers::new(vec![endpoint("mh-0")]).unwrap();
        for n in 0..5 {
            assert_eq!(set.place(rank(n)).unwrap().id, HandlerId::new("mh-0"));
        }
    }

    #[test]
    fn a_two_handler_meeting_alternates_by_rank() {
        let set = MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let placed: Vec<String> = (0..5)
            .map(|n| set.place(rank(n)).unwrap().id.to_string())
            .collect();
        assert_eq!(placed, vec!["mh-0", "mh-1", "mh-0", "mh-1", "mh-0"]);
    }

    /// The Redis enumeration order is the one input whose order genuinely
    /// varies; sorting at construction is what makes it irrelevant.
    #[test]
    fn registration_order_cannot_change_placement() {
        let forward = MeetingHandlers::new(vec![endpoint("mh-0"), endpoint("mh-1")]).unwrap();
        let reversed = MeetingHandlers::new(vec![endpoint("mh-1"), endpoint("mh-0")]).unwrap();
        for n in 0..4 {
            assert_eq!(forward.place(rank(n)), reversed.place(rank(n)));
        }
        assert_eq!(forward, reversed);
        // Rank 0 lands on the sorted-first handler, not on the first entry
        // Redis happened to return.
        assert_eq!(reversed.place(rank(0)).unwrap().id, HandlerId::new("mh-0"));
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
