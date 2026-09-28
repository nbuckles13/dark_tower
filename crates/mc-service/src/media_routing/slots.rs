//! Per-meeting join-order slot state and shared-handler edge assignment
//! (ADR-0036 §6, §9; story 2 R-2, R-3, R-4, R-33).
//!
//! # One general rule, no special cases
//!
//! Every participant is offered every handler of the meeting and connects to
//! all it can. A subscriber hears a sender **if and only if the two share at
//! least one connected handler** (§9 "share a media handler"). For every pair
//! the subscriber's slot table admits, the edge is assigned to exactly ONE
//! handler in `connected(sender) ∩ connected(subscriber)`; if that set is empty
//! the sender is unreachable for that subscriber (named in its
//! `unreachable_sender_ids`, consuming no slot). A sender sends to exactly the
//! handlers owning at least one of its out-edges. There is no single-handler
//! branch: one handler is simply a connected set of size one, and "everyone
//! connected everywhere ⇒ everyone hears everyone on one handler" is what the
//! rule plus the co-locating chooser produce, not a separate path.
//!
//! WHICH shared handler carries an edge is policy (`edges.rs`) and nothing may
//! depend on it: the model check below runs under the production chooser AND
//! an adversarial one.
//!
//! # Connectivity is observed, and "not yet connected" is not "unreachable"
//!
//! A member's `connected` is `None` until the meeting actor has settled what
//! the HANDLERS reported (`NotifyParticipantConnected`); see
//! `actors/meeting_media.rs` for the settle rule. A member that is not routable
//! has no edges in either direction and is named unreachable to NO ONE — it is
//! still establishing its connections (or has lost all of them and is
//! re-establishing). Only two routable members with disjoint connected sets
//! are unreachable to each other.
//!
//! # Why state, and not a pure recompute
//!
//! R-4 slot stability and edge stability are history-dependent: a from-scratch
//! "earliest joiners fill the slots, co-locate the edges" recompute would shift
//! slots when a middle sender leaves and reshuffle edges whenever a handler
//! joins someone's connected set. So the meeting actor holds a [`SlotTable`]
//! and mutates it with four rules:
//!
//! - **admit** — a joiner gets a rank. It is not yet routable, so NOTHING
//!   moves. (There is no placement input: a participant is not bound to a
//!   handler, it is connected to a set of them.)
//! - **set_demand** — a subscriber's declared audio slots size its slot vector.
//!   Ordinal *i* is the *i*-th AUDIO slot in declaration order. Same count: the
//!   occupants keep their ordinals and only the slot ids change. Growing adds
//!   empty slots, then refills. Shrinking truncates the tail.
//! - **remove** — for each subscriber, only the leaver's slot is freed and
//!   refilled with the earliest-joined eligible sender it does not hold.
//! - **set_connectivity** — the ONLY way connectivity enters. For every edge
//!   touching the changed member whose handler has left the pair's shared set:
//!   if the shared set is still non-empty the edge moves to a handler in it
//!   (the slot keeps its sender and its `egress_stream_id`); otherwise the slot
//!   is freed (the pair is now unreachable) and refilled earliest-first. Then
//!   every subscriber's empty slots are refilled. **An edge whose handler is
//!   still shared by both parties is never touched**, and no edge that does not
//!   involve the changed member is touched at all.
//!
//! # The invariants every mutation preserves
//!
//! 1. No subscriber holds itself (R-3: loopback is removed).
//! 2. No subscriber holds one sender twice.
//! 3. Every edge's handler is in `connected(subscriber) ∩ connected(sender)`.
//! 4. No subscriber has an empty slot while an eligible sender (routable,
//!    sharing a handler) it does not hold exists.
//! 5. A subscriber's slot vector is exactly as long as its declared audio
//!    slots.
//! 6. `unreachable_for(s)` is exactly the routable others whose connected set
//!    is disjoint from `s`'s, and is empty while `s` is not routable.
//! 7. A sender's targets are exactly the handlers owning its out-edges.
//!
//! plus two transition rules: an edge's handler changes only if that handler
//! left one party's connected set; a filled slot's sender changes only on that
//! sender's leave, the subscriber's redeclaration, or the pair becoming
//! unreachable (R-4). All are asserted after every step of the exhaustive
//! small-scope model check in this module's tests.
//!
//! # Rank is not `sender_id`, and not a handler key
//!
//! [`JoinRank`] is a per-meeting monotonic counter and the ONLY ordering key
//! for R-2 join-order fill. It selects nothing about handlers. [`SenderId`] is
//! deliberately not `Ord`. A rank is internal: it never reaches the wire, a
//! metric label, a log field or a span attribute.
//!
//! # No key material
//!
//! Sender ids, slot ids and handler ids only.

use super::assignment::{
    egress_stream_id, AssignmentError, EgressStreamPlan, HandlerAssignment, HandlerId,
    MeetingAssignment, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
};
use super::edges::{colocate, meeting_seed, EdgeChooser, EdgeContext};
use super::placement::ConnectedHandlers;
use crate::media_admission::SenderId;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A participant's per-meeting join-order position.
///
/// Monotonic and never reused within a meeting. Internal only: never on the
/// wire, never a metric label, never a log field or span attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JoinRank(u64);

/// A slot-table mutation could not be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotTableError {
    /// The sender is already a member. A second admission would give one
    /// sender two ranks.
    AlreadyAdmitted,
    /// The per-meeting rank counter cannot advance. Unreachable in practice
    /// (2^64 admissions); a refusal rather than a wrap, so a rank is never
    /// reused.
    RankSpaceExhausted,
}

impl std::fmt::Display for SlotTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAdmitted => f.write_str("participant is already in the slot table"),
            Self::RankSpaceExhausted => f.write_str("join-order rank space exhausted"),
        }
    }
}

impl std::error::Error for SlotTableError {}

/// One publisher→subscriber edge: who fills a slot, and which handler carries
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    /// The sender this slot receives.
    pub source: SenderId,
    /// The one handler carrying this edge, a member of both parties' connected
    /// sets.
    pub handler: HandlerId,
}

/// One roster participant, as slot state sees them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Member {
    sender: SenderId,
    /// `None` = not routable (not yet connected, or connectivity not yet
    /// settled). `Some` is never empty: an emptied set is reported as `None`.
    connected: Option<ConnectedHandlers>,
    /// Declared audio slot ids, in declaration order. `slots[i]` fills
    /// `slot_ids[i]`; the two always have equal length.
    slot_ids: Vec<u16>,
    /// Who fills each declared slot, and on which handler, if anyone.
    slots: Vec<Option<Edge>>,
}

impl Member {
    fn holds(&self, sender: SenderId) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|edge| edge.source == sender)
    }
}

/// The handlers `subscriber` and `publisher` both connect to, in id order.
///
/// ADR-0036 §9's visibility rule, **non-reflexive** (story 2 R-3). Empty if
/// either party is not routable. This is the ONE eligibility rule: every fill,
/// every edge move and the unreachable set derive from it.
fn shared(subscriber: &Member, publisher: &Member) -> Vec<HandlerId> {
    if subscriber.sender == publisher.sender {
        return Vec::new();
    }
    match (&subscriber.connected, &publisher.connected) {
        (Some(a), Some(b)) => a.shared_with(b),
        _ => Vec::new(),
    }
}

/// Current edge counts, kept in step with each mutation so a choice never
/// rescans the table.
#[derive(Default)]
struct Loads {
    per_sender: HashMap<SenderId, BTreeMap<HandlerId, usize>>,
    meeting: BTreeMap<HandlerId, usize>,
}

impl Loads {
    fn of(members: &BTreeMap<JoinRank, Member>) -> Self {
        let mut loads = Self::default();
        for member in members.values() {
            for edge in member.slots.iter().flatten() {
                loads.add(edge);
            }
        }
        loads
    }

    fn add(&mut self, edge: &Edge) {
        *self
            .per_sender
            .entry(edge.source)
            .or_default()
            .entry(edge.handler.clone())
            .or_default() += 1;
        *self.meeting.entry(edge.handler.clone()).or_default() += 1;
    }

    fn sub(&mut self, edge: &Edge) {
        if let Some(per) = self.per_sender.get_mut(&edge.source) {
            if let Some(n) = per.get_mut(&edge.handler) {
                *n = n.saturating_sub(1);
            }
        }
        if let Some(n) = self.meeting.get_mut(&edge.handler) {
            *n = n.saturating_sub(1);
        }
    }

    fn choose(
        &self,
        chooser: EdgeChooser,
        seed: u64,
        source: SenderId,
        candidates: &[HandlerId],
    ) -> Option<HandlerId> {
        let empty = BTreeMap::new();
        let ctx = EdgeContext {
            sender_out: self.per_sender.get(&source).unwrap_or(&empty),
            meeting: &self.meeting,
            meeting_seed: seed,
        };
        chooser(candidates, &ctx).cloned()
    }
}

/// The meeting's join-order slot and edge state.
#[derive(Clone)]
pub struct SlotTable {
    /// Every roster participant, in join order.
    members: BTreeMap<JoinRank, Member>,
    /// Reverse index. `SenderId` is not `Ord`, so this is a hash map; ordering
    /// always comes from `members`.
    rank_of: HashMap<SenderId, JoinRank>,
    /// The next rank to issue.
    next_rank: u64,
    /// Which shared handler a new or moved edge goes to. Production is always
    /// [`colocate`]; tests substitute adversarial choosers.
    chooser: EdgeChooser,
    /// This meeting's tiebreak rotation (`edges::meeting_seed`). Policy, like
    /// the chooser: it can only decide genuine ties, and no correctness
    /// property may depend on it.
    seed: u64,
}

impl std::fmt::Debug for SlotTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotTable")
            .field("members", &self.members)
            .field("next_rank", &self.next_rank)
            .finish_non_exhaustive()
    }
}

/// State equality; the chooser and the seed are policy, not state.
impl PartialEq for SlotTable {
    fn eq(&self, other: &Self) -> bool {
        self.members == other.members
            && self.rank_of == other.rank_of
            && self.next_rank == other.next_rank
    }
}

impl Eq for SlotTable {}

/// Test-only, for the same reason [`SlotTable::new`] is: a defaulted table
/// carries the unrotated tiebreak.
#[cfg(test)]
impl Default for SlotTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotTable {
    /// An empty table for one meeting, with the production edge policy. The
    /// meeting id is used for NOTHING but the tiebreak rotation — it is never
    /// stored, logged or compared.
    #[must_use]
    pub fn for_meeting(meeting_id: &str) -> Self {
        Self {
            seed: meeting_seed(meeting_id),
            ..Self::with_chooser(colocate)
        }
    }

    /// An empty table with the production edge policy and the UNROTATED
    /// tiebreak (seed 0).
    ///
    /// **`#[cfg(test)]` on purpose** (Gate 3, operations): a live meeting must
    /// go through [`Self::for_meeting`], or every meeting in the fleet
    /// tie-breaks to the same handler and half the egress capacity idles
    /// (`edges.rs`). Gating it makes that unreachable from production rather
    /// than merely documented — a future caller here is a compile error, not a
    /// silent capacity regression.
    #[cfg(test)]
    #[must_use]
    pub fn new() -> Self {
        Self::with_chooser(colocate)
    }

    /// An empty table with a given edge policy and the unrotated tiebreak.
    #[must_use]
    fn with_chooser(chooser: EdgeChooser) -> Self {
        Self {
            members: BTreeMap::new(),
            rank_of: HashMap::new(),
            next_rank: 0,
            chooser,
            seed: 0,
        }
    }

    /// Admit a participant with a fresh rank. It is not routable yet, so no
    /// slot anywhere changes.
    ///
    /// # Errors
    ///
    /// [`SlotTableError`] if the sender is already a member or the rank space
    /// is exhausted. Nothing is mutated on error.
    pub fn admit(&mut self, sender: SenderId) -> Result<JoinRank, SlotTableError> {
        if self.rank_of.contains_key(&sender) {
            return Err(SlotTableError::AlreadyAdmitted);
        }
        let rank = JoinRank(self.next_rank);
        self.next_rank = self
            .next_rank
            .checked_add(1)
            .ok_or(SlotTableError::RankSpaceExhausted)?;
        self.members.insert(
            rank,
            Member {
                sender,
                connected: None,
                slot_ids: Vec::new(),
                slots: Vec::new(),
            },
        );
        self.rank_of.insert(sender, rank);
        Ok(rank)
    }

    /// Remove a participant. For every other subscriber only the leaver's slot
    /// is freed, then refilled earliest-first. Returns `false` if the sender was
    /// not a member.
    pub fn remove(&mut self, sender: SenderId) -> bool {
        let Some(rank) = self.rank_of.remove(&sender) else {
            return false;
        };
        self.members.remove(&rank);
        for member in self.members.values_mut() {
            for slot in &mut member.slots {
                if slot.as_ref().is_some_and(|edge| edge.source == sender) {
                    *slot = None;
                }
            }
        }
        self.refill_all();
        true
    }

    /// Set a subscriber's declared audio slots (already validated and capped
    /// upstream — never sized from raw client input). Returns `false` if the
    /// sender is not a member.
    pub fn set_demand(&mut self, sender: SenderId, slot_ids: Vec<u16>) -> bool {
        let Some(&rank) = self.rank_of.get(&sender) else {
            return false;
        };
        let Some(member) = self.members.get_mut(&rank) else {
            return false;
        };
        member.slots.resize(slot_ids.len(), None);
        member.slot_ids = slot_ids;
        let mut loads = Loads::of(&self.members);
        self.refill(rank, &mut loads);
        true
    }

    /// Set a member's settled connectivity. `None`, or an empty set, makes it
    /// not routable. Returns `false` if the sender is not a member.
    ///
    /// Only edges involving `sender` can change, and of those only the ones
    /// whose handler is no longer shared by both parties (see the module doc).
    pub fn set_connectivity(
        &mut self,
        sender: SenderId,
        connected: Option<ConnectedHandlers>,
    ) -> bool {
        let Some(&rank) = self.rank_of.get(&sender) else {
            return false;
        };
        let connected = connected.filter(|set| !set.is_empty());
        let Some(member) = self.members.get_mut(&rank) else {
            return false;
        };
        member.connected = connected;

        // Pass 1: repair every edge touching `sender` that its handler no
        // longer serves. Computed against the table as it now stands.
        let mut loads = Loads::of(&self.members);
        let ranks: Vec<JoinRank> = self.members.keys().copied().collect();
        for subscriber_rank in &ranks {
            let Some(subscriber) = self.members.get(subscriber_rank) else {
                continue;
            };
            let mut repairs: Vec<(usize, Option<Edge>)> = Vec::new();
            for (ordinal, slot) in subscriber.slots.iter().enumerate() {
                let Some(edge) = slot else {
                    continue;
                };
                if subscriber.sender != sender && edge.source != sender {
                    continue;
                }
                let Some(publisher) = self.member(edge.source) else {
                    continue;
                };
                let candidates = shared(subscriber, publisher);
                if candidates.contains(&edge.handler) {
                    continue; // still valid: never touched
                }
                loads.sub(edge);
                let replacement = loads
                    .choose(self.chooser, self.seed, edge.source, &candidates)
                    .map(|handler| Edge {
                        source: edge.source,
                        handler,
                    });
                if let Some(new_edge) = &replacement {
                    loads.add(new_edge);
                }
                repairs.push((ordinal, replacement));
            }
            if let Some(subscriber) = self.members.get_mut(subscriber_rank) {
                for (ordinal, replacement) in repairs {
                    if let Some(slot) = subscriber.slots.get_mut(ordinal) {
                        *slot = replacement;
                    }
                }
            }
        }

        // Pass 2: fill every empty slot the change made fillable.
        for subscriber_rank in ranks {
            self.refill(subscriber_rank, &mut loads);
        }
        true
    }

    /// Test fixture: admit `sender` and connect it to `on` through the
    /// production connectivity path (endpoints resolved from `handlers`). A
    /// fixture that bypassed `ConnectedHandlers` would test a table production
    /// can never build.
    #[cfg(test)]
    pub(crate) fn admit_on(
        &mut self,
        sender: SenderId,
        handlers: &super::placement::MeetingHandlers,
        on: &[&str],
    ) {
        let _ = self.admit(sender);
        let mut connected = ConnectedHandlers::new();
        for id in on {
            if let Some(endpoint) = handlers.resolve(id) {
                connected.insert(endpoint);
            }
        }
        self.set_connectivity(sender, Some(connected));
    }

    fn refill_all(&mut self) {
        let mut loads = Loads::of(&self.members);
        let ranks: Vec<JoinRank> = self.members.keys().copied().collect();
        for rank in ranks {
            self.refill(rank, &mut loads);
        }
    }

    /// Fill a subscriber's empty slots, first empty first, with the
    /// earliest-joined eligible senders it does not already hold.
    fn refill(&mut self, subscriber: JoinRank, loads: &mut Loads) {
        loop {
            let Some(member) = self.members.get(&subscriber) else {
                return;
            };
            let Some(empty) = member.slots.iter().position(Option::is_none) else {
                return;
            };
            let candidate = self.members.values().find_map(|publisher| {
                if member.holds(publisher.sender) {
                    return None;
                }
                let handlers = shared(member, publisher);
                if handlers.is_empty() {
                    None
                } else {
                    Some((publisher.sender, handlers))
                }
            });
            let Some((source, handlers)) = candidate else {
                return;
            };
            let Some(handler) = loads.choose(self.chooser, self.seed, source, &handlers) else {
                return;
            };
            let edge = Edge { source, handler };
            loads.add(&edge);
            let Some(slot) = self
                .members
                .get_mut(&subscriber)
                .and_then(|m| m.slots.get_mut(empty))
            else {
                return;
            };
            *slot = Some(edge);
        }
    }

    fn member(&self, sender: SenderId) -> Option<&Member> {
        self.rank_of.get(&sender).and_then(|r| self.members.get(r))
    }

    /// Is this sender a member?
    #[must_use]
    pub fn contains(&self, sender: SenderId) -> bool {
        self.rank_of.contains_key(&sender)
    }

    /// The sender's join-order rank.
    #[must_use]
    pub fn rank_of(&self, sender: SenderId) -> Option<JoinRank> {
        self.rank_of.get(&sender).copied()
    }

    /// The sender's settled connected set, or `None` if it is not routable.
    #[must_use]
    pub fn connected_of(&self, sender: SenderId) -> Option<&ConnectedHandlers> {
        self.member(sender).and_then(|m| m.connected.as_ref())
    }

    /// Who fills each of the sender's declared slots, and on which handler, in
    /// ordinal order.
    #[must_use]
    pub fn slots_of(&self, sender: SenderId) -> Option<&[Option<Edge>]> {
        self.member(sender).map(|m| m.slots.as_slice())
    }

    /// The subscribers currently holding `source` in some slot, in join order.
    #[must_use]
    pub fn holders_of(&self, source: SenderId) -> Vec<SenderId> {
        self.members
            .values()
            .filter(|m| m.holds(source))
            .map(|m| m.sender)
            .collect()
    }

    /// The handlers `sender` must send to: exactly those owning at least one of
    /// its out-edges.
    #[must_use]
    pub fn targets_of(&self, sender: SenderId) -> BTreeSet<HandlerId> {
        self.members
            .values()
            .flat_map(|m| m.slots.iter().flatten())
            .filter(|edge| edge.source == sender)
            .map(|edge| edge.handler.clone())
            .collect()
    }

    /// How many of `subscriber`'s in-edges each handler carries — the
    /// identity-free summary the connectivity INFO line logs.
    #[must_use]
    pub fn in_edge_handlers(&self, subscriber: SenderId) -> BTreeMap<HandlerId, usize> {
        let mut out = BTreeMap::new();
        if let Some(member) = self.member(subscriber) {
            for edge in member.slots.iter().flatten() {
                *out.entry(edge.handler.clone()).or_default() += 1;
            }
        }
        out
    }

    /// Do `a` and `b` currently both connect to `handler`?
    #[must_use]
    pub fn both_connected_to(&self, a: SenderId, b: SenderId, handler: &HandlerId) -> bool {
        let on = |s: SenderId| {
            self.connected_of(s)
                .is_some_and(|set| set.contains(handler))
        };
        on(a) && on(b)
    }

    /// How many members are not routable (not yet connected, or settling).
    #[must_use]
    pub fn not_routable_count(&self) -> usize {
        self.members
            .values()
            .filter(|m| m.connected.is_none())
            .count()
    }

    /// The roster participants `subscriber` cannot reach: routable members whose
    /// connected set is disjoint from the subscriber's (§9; story 2 R-33),
    /// sorted numerically. Empty while the subscriber itself is not routable.
    ///
    /// Membership follows `signaling.proto`'s rule exactly. It EXCLUDES the
    /// subscriber itself, every sender it shares a handler with (including one
    /// left unassigned only because the slots are full — that is "no slot", not
    /// "unreachable"), and every member not yet connected ("not yet connected"
    /// is not "unreachable").
    ///
    /// "Only participants that HAVE a `sender_id`" is structural: a participant
    /// enters this table only through [`Self::admit`], which takes a
    /// `SenderId`. This reads `members` alone — the actor's own meeting — and
    /// carries no handler identity.
    #[must_use]
    pub fn unreachable_for(&self, subscriber: SenderId) -> Vec<SenderId> {
        let Some(me) = self.member(subscriber) else {
            return Vec::new();
        };
        if me.connected.is_none() {
            return Vec::new();
        }
        let mut unreachable: Vec<SenderId> = self
            .members
            .values()
            .filter(|other| {
                other.sender != me.sender
                    && other.connected.is_some()
                    && shared(me, other).is_empty()
            })
            .map(|other| other.sender)
            .collect();
        unreachable.sort_by_key(|s| s.get());
        unreachable
    }

    /// Render the slot state into the MC→MH forwarding assignment.
    ///
    /// One [`EgressStreamPlan`] per FILLED slot, with exactly one pinned
    /// candidate, on the handler that owns that edge. An unfilled slot emits NO
    /// egress stream. `egress_stream_id` packs the slot's ordinal in the
    /// subscriber's slot vector, so a slot keeps its id across a middle leave, a
    /// refill, and an edge moving between handlers.
    ///
    /// Every handler in `handlers` gets an entry, possibly empty: MH only
    /// reports a participant's connection once the meeting is REGISTERED on
    /// that handler, so a handler owning no edges must still be registered or
    /// its connectivity could never be observed.
    ///
    /// # The server-muted set, per handler
    ///
    /// `server_muted` is the meeting's SERVER-muted senders (never self-muted:
    /// see [`HandlerAssignment::server_muted_sources`]). Each handler's snapshot
    /// carries the members that are the SOURCE of at least one edge this
    /// render places on it — edge ownership, never connectivity. A sender
    /// connected to H need not send to H, and MH enforces mute at ingress with
    /// no cross-handler forwarding, so a sender whose edges span two handlers
    /// is in BOTH sets and a muted sender with no edge anywhere is in none.
    ///
    /// A required parameter rather than table state, deliberately: the meeting
    /// actor's roster is the one home for mute, and making every render supply
    /// the set means a caller cannot render without it. A render that silently
    /// defaulted to an empty set would tell MH "nobody is muted" — an unmute
    /// nobody asked for.
    ///
    /// # Errors
    ///
    /// [`AssignmentError::EgressOrdinalOverflow`] if an ordinal does not fit
    /// the 8 bits `egress_stream_id` allots it.
    pub fn render<'a>(
        &self,
        handlers: impl IntoIterator<Item = &'a HandlerId>,
        server_muted: &BTreeSet<SenderId>,
    ) -> Result<MeetingAssignment, AssignmentError> {
        let mut per_handler: BTreeMap<HandlerId, HandlerAssignment> = handlers
            .into_iter()
            .map(|h| (h.clone(), HandlerAssignment::default()))
            .collect();

        for member in self.members.values() {
            for (ordinal, (slot, slot_id)) in member.slots.iter().zip(&member.slot_ids).enumerate()
            {
                let Some(edge) = slot else {
                    continue;
                };
                let plan = EgressStreamPlan {
                    egress_stream_id: egress_stream_id(member.sender, ordinal)?,
                    subscriber: member.sender,
                    slot_id: *slot_id,
                    // Exactly one candidate: the muted-set bound on
                    // `HandlerAssignment::server_muted_sources` rests on it.
                    // Adding a candidate here voids that bound; re-derive it.
                    candidate_sources: vec![edge.source],
                    stream_number: MAIN_AUDIO_STREAM_NUMBER,
                    priority_group: AUDIO_PRIORITY_GROUP,
                    // False for audio: every frame forwards. MH is told the
                    // behaviour, never the media type.
                    supersede_on_independent_frame: false,
                    // ADR-0036 §1: audio rides datagrams.
                    transport_mode: TransportMode::Datagram,
                };
                per_handler
                    .entry(edge.handler.clone())
                    .or_default()
                    .egress_streams
                    .push(plan);
            }
        }

        for assignment in per_handler.values_mut() {
            assignment
                .egress_streams
                .sort_by_key(|s| s.egress_stream_id);
            // Edge ownership: a muted sender is on THIS handler's set iff it
            // sources one of THIS handler's egress streams. Built as a set, so
            // a sender sourcing several of this handler's streams appears once.
            assignment.server_muted_sources = assignment
                .egress_streams
                .iter()
                .flat_map(|plan| plan.candidate_sources.iter().copied())
                .filter(|source| server_muted.contains(source))
                .collect();
        }
        Ok(MeetingAssignment { per_handler })
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "model-check transition assertions")]
mod tests {
    use super::*;
    use crate::media_routing::placement::{HandlerEndpoint, MeetingHandlers};
    use std::collections::{HashSet, VecDeque};
    use std::num::NonZeroU16;

    const H: [&str; 2] = ["mh-0", "mh-1"];

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn h(name: &str) -> HandlerId {
        HandlerId::new(name)
    }

    fn handler_set() -> MeetingHandlers {
        MeetingHandlers::new(H.iter().map(|id| HandlerEndpoint {
            id: h(id),
            webtransport_url: format!("https://{id}.example:4434"),
            grpc_endpoint: format!("http://{id}.example:50053"),
        }))
        .unwrap()
    }

    /// A connected set built the only way production can build one: from
    /// endpoints the meeting's handler set resolved.
    fn on(ids: &[&str]) -> Option<ConnectedHandlers> {
        let set = handler_set();
        let mut connected = ConnectedHandlers::new();
        for id in ids {
            connected.insert(set.resolve(id).unwrap());
        }
        Some(connected)
    }

    fn all() -> Option<ConnectedHandlers> {
        on(&H)
    }

    /// Adversarial chooser: always the LAST candidate. Correctness must not
    /// depend on which shared handler is picked.
    fn last<'a>(candidates: &'a [HandlerId], _: &EdgeContext<'_>) -> Option<&'a HandlerId> {
        candidates.last()
    }

    /// Adversarial chooser that ignores co-location: the least-loaded
    /// candidate, which spreads a sender across handlers whenever it can.
    fn spread<'a>(candidates: &'a [HandlerId], ctx: &EdgeContext<'_>) -> Option<&'a HandlerId> {
        candidates
            .iter()
            .min_by_key(|h| ctx.sender_out.get(*h).copied().unwrap_or(0))
    }

    fn sources(table: &SlotTable, s: u16) -> Vec<Option<u16>> {
        table
            .slots_of(sender(s))
            .unwrap()
            .iter()
            .map(|slot| slot.as_ref().map(|e| e.source.get().get()))
            .collect()
    }

    fn unreachable(table: &SlotTable, s: u16) -> Vec<u16> {
        table
            .unreachable_for(sender(s))
            .iter()
            .map(|x| x.get().get())
            .collect()
    }

    fn connect_all(t: &mut SlotTable, ids: &[u16]) {
        for s in ids {
            t.set_connectivity(sender(*s), all());
        }
    }

    fn admit(t: &mut SlotTable, s: u16) {
        t.admit(sender(s)).unwrap();
    }

    /// The production wiring of the per-meeting tiebreak: `for_meeting` really
    /// threads the seed into the chooser, so distinct meetings with IDENTICAL
    /// inputs do not all pile their edges onto one handler. A regression that
    /// dropped the seed (or left production on `SlotTable::new`) shows up here
    /// as one handler taking all 64 meetings.
    #[test]
    fn distinct_meetings_do_not_all_place_their_edges_on_one_handler() {
        let mut used: BTreeMap<HandlerId, usize> = BTreeMap::new();
        for n in 0..64 {
            let mut t = SlotTable::for_meeting(&format!("meeting-{n}"));
            admit(&mut t, 1);
            admit(&mut t, 2);
            t.set_demand(sender(1), vec![0]);
            t.set_demand(sender(2), vec![0]);
            connect_all(&mut t, &[1, 2]);
            let handler = t
                .slots_of(sender(1))
                .unwrap()
                .iter()
                .flatten()
                .map(|e| e.handler.clone())
                .next()
                .expect("an all-connected pair has an edge");
            *used.entry(handler).or_default() += 1;
        }
        assert_eq!(
            used.len(),
            2,
            "every meeting landed on one handler: {used:?}"
        );
    }

    /// One meeting's placement is reproducible: the same id and the same input
    /// build the same table, on any pod and after any restart.
    #[test]
    fn one_meeting_id_places_reproducibly() {
        let build = || {
            let mut t = SlotTable::for_meeting("meeting-stable");
            admit(&mut t, 1);
            admit(&mut t, 2);
            t.set_demand(sender(1), vec![0]);
            t.set_demand(sender(2), vec![0]);
            connect_all(&mut t, &[1, 2]);
            t.slots_of(sender(1))
                .unwrap()
                .iter()
                .flatten()
                .map(|e| e.handler.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }

    /// Every property the task names, checked for any table: exactly one edge
    /// per filled slot on a handler both parties are connected to, unreachable
    /// if and only if disjoint, targets equal edge owners.
    fn assert_properties(t: &SlotTable) {
        for m in t.members.values() {
            for edge in m.slots.iter().flatten() {
                assert!(
                    t.both_connected_to(m.sender, edge.source, &edge.handler),
                    "edge on a handler both parties are connected to"
                );
            }
            let expected: HashSet<u16> = if m.connected.is_none() {
                HashSet::new()
            } else {
                t.members
                    .values()
                    .filter(|o| o.sender != m.sender && o.connected.is_some())
                    .filter(|o| shared(m, o).is_empty())
                    .map(|o| o.sender.get().get())
                    .collect()
            };
            let got: HashSet<u16> = t
                .unreachable_for(m.sender)
                .iter()
                .map(|s| s.get().get())
                .collect();
            assert_eq!(got, expected, "unreachable iff connected sets are disjoint");
            let owners: BTreeSet<HandlerId> = t
                .members
                .values()
                .flat_map(|o| o.slots.iter().flatten())
                .filter(|e| e.source == m.sender)
                .map(|e| e.handler.clone())
                .collect();
            assert_eq!(
                t.targets_of(m.sender),
                owners,
                "targets = owners of out-edges"
            );
        }
    }

    // ------------------------------------------------ the canonical case

    /// A on both handlers, B only on one, C only on the other. A hears B and
    /// C; B and C each hear only A and are unreachable to each other; A sends
    /// to both handlers. Asserted as properties — never which handler is which
    /// beyond "the one the pair shares", which here is forced.
    #[test]
    fn canonical_three_participants_with_partial_connectivity() {
        for chooser in [colocate as EdgeChooser, last, spread] {
            let mut t = SlotTable::with_chooser(chooser);
            for s in 1..=3 {
                admit(&mut t, s);
                t.set_demand(sender(s), vec![10, 20]);
            }
            t.set_connectivity(sender(1), all());
            t.set_connectivity(sender(2), on(&["mh-0"]));
            t.set_connectivity(sender(3), on(&["mh-1"]));

            let mut a = sources(&t, 1);
            a.sort();
            assert_eq!(a, vec![Some(2), Some(3)], "A hears B and C");
            assert_eq!(sources(&t, 2), vec![Some(1), None], "B hears only A");
            assert_eq!(sources(&t, 3), vec![Some(1), None], "C hears only A");
            assert_eq!(unreachable(&t, 1), Vec::<u16>::new());
            assert_eq!(unreachable(&t, 2), vec![3]);
            assert_eq!(unreachable(&t, 3), vec![2]);
            assert_eq!(t.targets_of(sender(1)).len(), 2, "A sends to both handlers");
            assert_eq!(t.targets_of(sender(2)).len(), 1);
            assert_eq!(t.targets_of(sender(3)).len(), 1);
            assert_properties(&t);
        }
    }

    /// Everyone on every handler: everyone hears everyone, and with the
    /// production chooser each sender has exactly ONE target — the rule plus
    /// co-location, not a branch.
    #[test]
    fn all_connected_everyone_hears_everyone_on_one_handler_per_sender() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0, 1, 2]);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        for s in 1..=4u16 {
            let heard: HashSet<u16> = sources(&t, s).into_iter().flatten().collect();
            let others: HashSet<u16> = (1..=4).filter(|x| *x != s).collect();
            assert_eq!(heard, others);
            assert!(unreachable(&t, s).is_empty());
            assert_eq!(t.targets_of(sender(s)).len(), 1, "one target per sender");
        }
        assert_properties(&t);
    }

    /// A handler leaving ONE participant's set moves only that participant's
    /// edges; every other edge is byte-identical.
    #[test]
    fn a_disconnect_moves_only_the_disconnected_participants_edges() {
        let mut t = SlotTable::with_chooser(last);
        for s in 1..=4 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0, 1, 2]);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        let before = t.clone();
        // Participant 4 loses whichever handler carries its edges.
        let lost = t.targets_of(sender(4)).into_iter().next().unwrap();
        let kept: Vec<&str> = H.iter().copied().filter(|x| *x != lost.as_str()).collect();
        t.set_connectivity(sender(4), on(&kept));

        for m in before.members.values() {
            let now = t.member(m.sender).unwrap();
            for (old, new) in m.slots.iter().zip(&now.slots) {
                let touches_4 =
                    m.sender == sender(4) || old.as_ref().is_some_and(|e| e.source == sender(4));
                if !touches_4 {
                    assert_eq!(old, new, "an edge not involving 4 never moves");
                } else if let (Some(o), Some(n)) = (old, new) {
                    assert_eq!(o.source, n.source, "the slot keeps its sender (R-4)");
                }
            }
        }
        assert_properties(&t);
    }

    /// Edge stability: a recompute driven by an unrelated change (a fifth
    /// participant joining and connecting) leaves every existing edge exactly
    /// where it was.
    #[test]
    fn existing_edges_survive_an_unrelated_recompute() {
        let mut t = SlotTable::with_chooser(spread);
        for s in 1..=3 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0, 1, 2, 3]);
        }
        connect_all(&mut t, &[1, 2, 3]);
        let before: Vec<Vec<Option<Edge>>> = (1..=3)
            .map(|s| t.slots_of(sender(s)).unwrap().to_vec())
            .collect();
        admit(&mut t, 4);
        t.set_connectivity(sender(4), on(&["mh-1"]));
        for (i, s) in (1..=3u16).enumerate() {
            let after = t.slots_of(sender(s)).unwrap();
            for (old, new) in before[i].iter().zip(after) {
                if old.is_some() {
                    assert_eq!(
                        old.as_ref(),
                        new.as_ref(),
                        "a valid edge is never reshuffled"
                    );
                }
            }
        }
        assert_properties(&t);
    }

    /// R-4 across a connectivity change: senders that stay reachable keep their
    /// slots; only the slot of the newly unreachable sender is freed and
    /// refilled earliest-first.
    #[test]
    fn slots_are_stable_across_a_connectivity_change() {
        let mut t = SlotTable::new();
        for s in 1..=5 {
            admit(&mut t, s);
        }
        t.set_demand(sender(1), vec![0, 1, 2]);
        connect_all(&mut t, &[1, 2, 3, 4, 5]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);

        // 3 drops to a handler 1 is not on: move 1 to mh-0 only, 3 to mh-1.
        t.set_connectivity(sender(1), on(&["mh-0"]));
        t.set_connectivity(sender(3), on(&["mh-1"]));
        assert_eq!(
            sources(&t, 1),
            vec![Some(2), Some(5), Some(4)],
            "only 3's slot changed; refilled with the earliest eligible"
        );
        assert_eq!(unreachable(&t, 1), vec![3]);
        assert_properties(&t);
    }

    /// Not yet connected is not unreachable, in both directions, and holds no
    /// slot.
    #[test]
    fn a_participant_not_yet_connected_has_no_edges_and_is_not_unreachable() {
        let mut t = SlotTable::new();
        for s in 1..=2 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0]);
        }
        t.set_connectivity(sender(1), all());
        assert_eq!(sources(&t, 1), vec![None]);
        assert!(
            unreachable(&t, 1).is_empty(),
            "2 is establishing, not unreachable"
        );
        assert!(
            unreachable(&t, 2).is_empty(),
            "2 claims nobody unreachable yet"
        );
        assert_eq!(t.not_routable_count(), 1);
    }

    /// A participant whose set empties goes back to not-connected: no edges,
    /// named unreachable to nobody.
    #[test]
    fn losing_every_handler_returns_to_not_connected() {
        let mut t = SlotTable::new();
        for s in 1..=3 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0, 1]);
        }
        connect_all(&mut t, &[1, 2, 3]);
        t.set_connectivity(sender(3), Some(ConnectedHandlers::new()));
        assert!(
            t.connected_of(sender(3)).is_none(),
            "empty is normalised to None"
        );
        assert_eq!(sources(&t, 3), vec![None, None]);
        for s in [1u16, 2] {
            assert!(!sources(&t, s).contains(&Some(3)));
            assert!(unreachable(&t, s).is_empty());
        }
        assert_properties(&t);
    }

    // --------------------------------------------- task-6 rules, still true

    #[test]
    fn a_joiner_fills_existing_subscribers_first_empty_slot_in_join_order() {
        let mut t = SlotTable::new();
        admit(&mut t, 1);
        t.set_demand(sender(1), vec![0, 1]);
        connect_all(&mut t, &[1]);
        assert_eq!(sources(&t, 1), vec![None, None], "solo: nobody to hear");

        admit(&mut t, 2);
        assert_eq!(
            sources(&t, 1),
            vec![None, None],
            "admission alone moves nothing"
        );
        connect_all(&mut t, &[2]);
        assert_eq!(sources(&t, 1), vec![Some(2), None]);
        admit(&mut t, 3);
        connect_all(&mut t, &[3]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3)]);

        admit(&mut t, 4);
        connect_all(&mut t, &[4]);
        t.set_demand(sender(4), vec![0, 1]);
        assert_eq!(
            sources(&t, 4),
            vec![Some(1), Some(2)],
            "earliest-joined first"
        );
    }

    /// The render's per-handler server-muted set, by EDGE OWNERSHIP.
    ///
    /// A is on both handlers, B only on mh-0, C only on mh-1. B and C each hear
    /// A; A hears B and C. So A sources an edge on EACH handler, B only on mh-0
    /// (its only shared handler with A) and C only on mh-1.
    fn split_meeting() -> (SlotTable, MeetingHandlers) {
        let set = handler_set();
        let mut t = SlotTable::new();
        t.admit_on(sender(1), &set, &["mh-0", "mh-1"]);
        t.admit_on(sender(2), &set, &["mh-0"]);
        t.admit_on(sender(3), &set, &["mh-1"]);
        t.set_demand(sender(1), vec![0, 1]);
        t.set_demand(sender(2), vec![0]);
        t.set_demand(sender(3), vec![0]);
        (t, set)
    }

    fn muted_on(render: &MeetingAssignment, handler: &str) -> Vec<u16> {
        render
            .for_handler(&h(handler))
            .unwrap()
            .server_muted_sources
            .iter()
            .map(|s| s.get().get())
            .collect()
    }

    #[test]
    fn a_muted_sender_whose_edges_span_both_handlers_is_in_both_sets() {
        let (t, set) = split_meeting();
        let render = t.render(set.ids(), &BTreeSet::from([sender(1)])).unwrap();
        assert_eq!(muted_on(&render, "mh-0"), vec![1]);
        assert_eq!(muted_on(&render, "mh-1"), vec![1]);
    }

    #[test]
    fn a_muted_sender_is_only_on_the_handlers_carrying_its_edges() {
        let (t, set) = split_meeting();
        let render = t.render(set.ids(), &BTreeSet::from([sender(2)])).unwrap();
        assert_eq!(
            muted_on(&render, "mh-0"),
            vec![2],
            "B's one edge is on mh-0"
        );
        assert!(
            muted_on(&render, "mh-1").is_empty(),
            "B sources nothing on mh-1, and connectivity alone never puts it there"
        );
    }

    #[test]
    fn a_muted_sender_with_no_edge_is_in_no_set_and_nobody_muted_is_empty() {
        let (mut t, set) = split_meeting();
        t.admit_on(sender(4), &set, &[]); // admitted, connected to nothing
        let render = t.render(set.ids(), &BTreeSet::from([sender(4)])).unwrap();
        assert!(muted_on(&render, "mh-0").is_empty());
        assert!(muted_on(&render, "mh-1").is_empty());

        let unmuted = t.render(set.ids(), &BTreeSet::new()).unwrap();
        assert!(muted_on(&unmuted, "mh-0").is_empty());
        assert!(muted_on(&unmuted, "mh-1").is_empty());
    }

    /// The bound MH relies on: each handler's set is no larger than its egress
    /// streams, even with EVERY participant muted.
    #[test]
    fn a_handlers_muted_set_never_exceeds_its_egress_streams() {
        let (t, set) = split_meeting();
        let everyone = BTreeSet::from([sender(1), sender(2), sender(3)]);
        let render = t.render(set.ids(), &everyone).unwrap();
        for handler in ["mh-0", "mh-1"] {
            let policy = render.for_handler(&h(handler)).unwrap();
            assert!(
                policy.server_muted_sources.len() <= policy.egress_streams.len(),
                "{handler}: |muted| <= |egress_streams|"
            );
        }
    }

    #[test]
    fn a_middle_leave_changes_only_the_leavers_slot() {
        let mut t = SlotTable::new();
        for s in 1..=5 {
            admit(&mut t, s);
        }
        connect_all(&mut t, &[1, 2, 3, 4, 5]);
        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        t.remove(sender(3));
        assert_eq!(sources(&t, 1), vec![Some(2), Some(5), Some(4)]);
    }

    #[test]
    fn the_n_plus_second_sharer_is_unassigned_and_not_unreachable() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        t.set_demand(sender(1), vec![0, 1]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3)]);
        assert!(unreachable(&t, 1).is_empty(), "no slot is not unreachable");
    }

    #[test]
    fn redeclare_shrink_grow_and_renumber() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        t.set_demand(sender(1), vec![0, 1]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3)]);
        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        t.set_demand(sender(1), vec![9, 8, 7]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        let render = t
            .render(handler_set().ids(), &std::collections::BTreeSet::new())
            .unwrap();
        let ids: Vec<u16> = render
            .per_handler
            .values()
            .flat_map(|p| &p.egress_streams)
            .filter(|p| p.subscriber == sender(1))
            .map(|p| p.slot_id)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![7, 8, 9], "the declared ids ride the wire");
    }

    #[test]
    fn a_fresh_rejoin_gets_a_new_rank_and_loses_its_place() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        t.set_demand(sender(4), vec![0]);
        assert_eq!(sources(&t, 4), vec![Some(1)]);
        t.remove(sender(1));
        admit(&mut t, 5);
        connect_all(&mut t, &[5]);
        assert_eq!(t.rank_of(sender(5)), Some(JoinRank(4)));
        assert_eq!(sources(&t, 4), vec![Some(2)]);
    }

    #[test]
    fn a_solo_participant_hears_nothing_and_mh_carries_no_stream() {
        let mut t = SlotTable::new();
        admit(&mut t, 7);
        t.set_demand(sender(7), vec![0, 1]);
        connect_all(&mut t, &[7]);
        assert_eq!(sources(&t, 7), vec![None, None]);
        let render = t
            .render(handler_set().ids(), &std::collections::BTreeSet::new())
            .unwrap();
        assert!(render
            .per_handler
            .values()
            .all(|p| p.egress_streams.is_empty()));
        assert_eq!(
            render.per_handler.len(),
            2,
            "every handler registered, even empty"
        );
    }

    #[test]
    fn egress_stream_id_is_stable_when_an_edge_moves_handler() {
        let mut t = SlotTable::new();
        for s in 1..=2 {
            admit(&mut t, s);
            t.set_demand(sender(s), vec![0]);
        }
        connect_all(&mut t, &[1, 2]);
        let find = |t: &SlotTable| {
            t.render(handler_set().ids(), &std::collections::BTreeSet::new())
                .unwrap()
                .per_handler
                .iter()
                .find_map(|(handler, p)| {
                    p.egress_streams
                        .iter()
                        .find(|e| e.subscriber == sender(1))
                        .map(|e| (handler.clone(), e.egress_stream_id))
                })
                .unwrap()
        };
        let (before_handler, before_id) = find(&t);
        let other = H
            .iter()
            .copied()
            .find(|x| *x != before_handler.as_str())
            .unwrap();
        t.set_connectivity(sender(2), on(&[other]));
        let (after_handler, after_id) = find(&t);
        assert_ne!(after_handler, before_handler, "the edge had to move");
        assert_eq!(after_id, before_id, "and kept its egress stream id");
    }

    #[test]
    fn admission_errors_leave_the_table_untouched() {
        let mut t = SlotTable::new();
        admit(&mut t, 1);
        let before = t.clone();
        assert_eq!(t.admit(sender(1)), Err(SlotTableError::AlreadyAdmitted));
        assert_eq!(t, before);
    }

    // ------------------------------------------- property-style generator

    /// Fixed-seed generated connectivity patterns (no clock, reproducible):
    /// random roster size, random connected subsets, random slot counts, in
    /// random join/connect order, under every chooser. The properties hold for
    /// all of them, and the production chooser yields one target per sender
    /// whenever everyone is connected to everything.
    #[test]
    fn properties_hold_over_generated_connectivity() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed_0020);
        let subsets: [&[&str]; 4] = [&[], &["mh-0"], &["mh-1"], &["mh-0", "mh-1"]];
        for case in 0..400 {
            for (chooser, is_production) in [
                (colocate as EdgeChooser, true),
                (last as EdgeChooser, false),
                (spread as EdgeChooser, false),
            ] {
                let mut t = SlotTable::with_chooser(chooser);
                let n: u16 = rng.gen_range(1..=7);
                let mut all_connected = true;
                for s in 1..=n {
                    admit(&mut t, s);
                    t.set_demand(sender(s), (0..rng.gen_range(0..=4u16)).collect());
                }
                for s in 1..=n {
                    let pick = subsets[rng.gen_range(0..subsets.len())];
                    all_connected &= pick.len() == H.len();
                    t.set_connectivity(sender(s), if pick.is_empty() { None } else { on(pick) });
                }
                assert_properties(&t);
                // Determinism: same inputs, same state.
                let mut replay = SlotTable::with_chooser(chooser);
                for m in t.members.values() {
                    replay.admit(m.sender).unwrap();
                    replay.set_demand(m.sender, m.slot_ids.clone());
                }
                for m in t.members.values() {
                    replay.set_connectivity(m.sender, m.connected.clone());
                }
                assert_eq!(replay, t, "case {case}: deterministic");
                if all_connected && is_production {
                    for s in 1..=n {
                        assert!(t.targets_of(sender(s)).len() <= 1, "case {case}");
                    }
                }
            }
        }
    }

    // ------------------------------------------------- exhaustive model check

    /// Canonical state: slots as positions in the join-ordered member list plus
    /// the edge's handler index, and each member's connectivity. Two tables that
    /// differ only in id/rank VALUES are the same state for every invariant.
    fn canonical(t: &SlotTable, people: &HashMap<SenderId, u8>) -> Vec<u8> {
        let order: Vec<SenderId> = t.members.values().map(|m| m.sender).collect();
        let pos = |s: SenderId| {
            order
                .iter()
                .position(|x| *x == s)
                .map_or(u8::MAX, |p| u8::try_from(p).unwrap())
        };
        let hidx = |id: &HandlerId| u8::from(id.as_str() == "mh-1");
        let mut key = Vec::new();
        for m in t.members.values() {
            key.push(people[&m.sender]);
            key.push(match &m.connected {
                None => 0,
                Some(c) => {
                    u8::from(c.contains(&h("mh-0"))) + 2 * u8::from(c.contains(&h("mh-1"))) + 4
                }
            });
            key.push(u8::try_from(m.slots.len()).unwrap());
            for slot in &m.slots {
                match slot {
                    None => key.push(u8::MAX - 1),
                    Some(e) => {
                        key.push(pos(e.source));
                        key.push(hidx(&e.handler));
                    }
                }
            }
            key.push(u8::MAX - 2);
        }
        key
    }

    fn check_state(t: &SlotTable) {
        for m in t.members.values() {
            assert_eq!(m.slots.len(), m.slot_ids.len(), "invariant 5");
            let mut seen = HashSet::new();
            for edge in m.slots.iter().flatten() {
                assert_ne!(edge.source, m.sender, "invariant 1: no self");
                assert!(seen.insert(edge.source.get()), "invariant 2: no duplicate");
                let publisher = t.member(edge.source).expect("filled with a member");
                assert!(
                    shared(m, publisher).contains(&edge.handler),
                    "invariant 3: edge on a shared handler"
                );
            }
            if m.slots.iter().any(Option::is_none) {
                for other in t.members.values() {
                    if !shared(m, other).is_empty() {
                        assert!(
                            m.holds(other.sender),
                            "invariant 4: empty slot beside an unheld eligible sharer"
                        );
                    }
                }
            }
        }
        assert_properties(t); // invariants 6 and 7

        let render = t
            .render(handler_set().ids(), &std::collections::BTreeSet::new())
            .unwrap();
        assert_eq!(render.per_handler.len(), 2, "one entry per handler, always");
        let mut ids = HashSet::new();
        let mut streams = 0usize;
        for (handler, policy) in &render.per_handler {
            assert!(
                policy
                    .egress_streams
                    .windows(2)
                    .all(|w| w[0].egress_stream_id < w[1].egress_stream_id),
                "canonical order"
            );
            for plan in &policy.egress_streams {
                streams += 1;
                assert!(ids.insert(plan.egress_stream_id), "unique egress_stream_id");
                assert_eq!(
                    plan.candidate_sources.len(),
                    1,
                    "one candidate per stream is the premise of the muted-set bound \
                     (`HandlerAssignment::server_muted_sources`: |set| <= |egress_streams|)"
                );
                let sub = t.member(plan.subscriber).unwrap();
                let ordinal =
                    crate::media_routing::assignment::egress_ordinal(plan.egress_stream_id);
                let edge = sub.slots.get(ordinal).unwrap().as_ref().unwrap();
                assert_eq!(&edge.handler, handler, "rendered on the edge's handler");
                assert_eq!(edge.source, plan.candidate_sources[0]);
                assert_eq!(sub.slot_ids.get(ordinal), Some(&plan.slot_id));
            }
        }
        let filled: usize = t
            .members
            .values()
            .map(|m| m.slots.iter().flatten().count())
            .sum();
        assert_eq!(streams, filled, "one stream per FILLED slot");
    }

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Join(u8),
        Leave(u8),
        Declare(u8, usize),
        Connect(u8, usize),
    }

    fn check_transition(before: &SlotTable, after: &SlotTable, op: Op, actor: Option<SenderId>) {
        let left = matches!(op, Op::Leave(_)).then_some(actor).flatten();
        for m in before.members.values() {
            if Some(m.sender) == actor && !matches!(op, Op::Connect(..)) {
                continue; // the redeclaring / leaving participant's own vector
            }
            let Some(now) = after.member(m.sender) else {
                continue;
            };
            for (i, old) in m.slots.iter().enumerate() {
                let new = now.slots.get(i).cloned().flatten();
                if *old == new {
                    continue;
                }
                match (op, old) {
                    (Op::Join(_), _) => panic!("admission moved slot {i}"),
                    (Op::Declare(..), _) => {
                        panic!("another participant's demand moved slot {i}")
                    }
                    (Op::Leave(_), Some(o)) => {
                        assert_eq!(
                            Some(o.source),
                            left,
                            "on leave only the leaver's slot changes"
                        );
                    }
                    (Op::Leave(_), None) => {} // refill of an empty slot
                    (Op::Connect(..), Some(o)) => {
                        let actor = actor.unwrap();
                        assert!(
                            m.sender == actor || o.source == actor,
                            "a connectivity change touched an edge not involving the changed member"
                        );
                        let still_shared = after.both_connected_to(m.sender, o.source, &o.handler);
                        assert!(
                            !still_shared,
                            "edge stability: a still-valid edge moved or was dropped"
                        );
                        if let Some(n) = &new {
                            let pair_reachable = after
                                .member(m.sender)
                                .zip(after.member(o.source))
                                .is_some_and(|(a, b)| !shared(a, b).is_empty());
                            if pair_reachable {
                                assert_eq!(
                                    n.source, o.source,
                                    "R-4: reachable sender keeps its slot"
                                );
                            }
                        }
                    }
                    (Op::Connect(..), None) => {} // refill of an empty slot
                }
            }
        }
    }

    /// The test that proves R-4 and edge stability. Breadth-first over
    /// canonical states: 4 people x 2 handlers x connectivity {none, h0, h1,
    /// both} x slot counts {1,2}, join / leave / fresh rejoin / redeclare /
    /// connectivity change, to depth 6, under the production chooser AND an
    /// adversarial one, asserting every state invariant, every render invariant
    /// and every transition rule.
    #[test]
    fn exhaustive_small_scope_model_check() {
        const PEOPLE: u8 = 4;
        const DEPTH: usize = 6;
        let connectivity: [&[&str]; 4] = [&[], &["mh-0"], &["mh-1"], &["mh-0", "mh-1"]];

        #[derive(Clone)]
        struct World {
            table: SlotTable,
            present: HashMap<u8, SenderId>,
            next_sender: u16,
        }

        for chooser in [colocate as EdgeChooser, last] {
            let started = std::time::Instant::now();
            let start = World {
                table: SlotTable::with_chooser(chooser),
                present: HashMap::new(),
                next_sender: 1,
            };
            let mut queue = VecDeque::from([(start, 0usize)]);
            let mut visited: HashSet<Vec<u8>> = HashSet::new();
            let mut transitions = 0usize;

            while let Some((world, depth)) = queue.pop_front() {
                let people: HashMap<SenderId, u8> =
                    world.present.iter().map(|(p, s)| (*s, *p)).collect();
                if !visited.insert(canonical(&world.table, &people)) {
                    continue;
                }
                check_state(&world.table);
                if depth == DEPTH {
                    continue;
                }
                let mut ops = Vec::new();
                for p in 0..PEOPLE {
                    if world.present.contains_key(&p) {
                        ops.push(Op::Leave(p));
                        for n in 1..=2 {
                            ops.push(Op::Declare(p, n));
                        }
                        for c in 0..connectivity.len() {
                            ops.push(Op::Connect(p, c));
                        }
                    } else {
                        ops.push(Op::Join(p));
                    }
                }
                for op in ops {
                    let mut next = world.clone();
                    let actor = match op {
                        Op::Join(p) => {
                            let s = sender(next.next_sender);
                            next.next_sender += 1; // never recycled
                            next.table.admit(s).unwrap();
                            next.present.insert(p, s);
                            Some(s)
                        }
                        Op::Leave(p) => {
                            let s = next.present.remove(&p).unwrap();
                            assert!(next.table.remove(s));
                            Some(s)
                        }
                        Op::Declare(p, n) => {
                            let s = next.present[&p];
                            let n = u16::try_from(n).unwrap();
                            let ids: Vec<u16> = (0..n).map(|i| i * 300 + u16::from(p)).collect();
                            assert!(next.table.set_demand(s, ids));
                            Some(s)
                        }
                        Op::Connect(p, c) => {
                            let s = next.present[&p];
                            let pick = connectivity[c];
                            let set = if pick.is_empty() { None } else { on(pick) };
                            assert!(next.table.set_connectivity(s, set));
                            Some(s)
                        }
                    };
                    transitions += 1;
                    check_transition(&world.table, &next.table, op, actor);
                    queue.push_back((next, depth + 1));
                }
            }

            let states = visited.len();
            println!(
                "slot/edge model check: {states} canonical states, {transitions} transitions, {:?}",
                started.elapsed()
            );
            // Positive control: a generator bug that explores nothing cannot pass.
            assert!(states > 10_000, "explored only {states} states");
        }
    }
}
