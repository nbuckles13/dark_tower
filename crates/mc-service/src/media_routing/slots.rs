//! Per-meeting join-order slot state and shared-handler edge assignment, for
//! BOTH media kinds (ADR-0036 §6, §7, §9; story 2 R-2..R-4, R-33; story 3
//! R-1, R-3, R-20, R-26).
//!
//! # One general rule, no special cases
//!
//! Every participant is offered every handler of the meeting and connects to
//! all it can. A subscriber can be assigned a publisher **if and only if the
//! two share at least one connected handler** (§9 "share a media handler").
//! For every pair the subscriber's slot table admits, the edge is assigned to
//! exactly ONE handler in `connected(sender) ∩ connected(subscriber)`; if that
//! set is empty the sender is unreachable for that subscriber (named in its
//! `unreachable_sender_ids`, consuming no slot of either kind). There is no
//! single-handler branch and no per-kind topology: a kind changes only WHO is
//! eligible for a slot and what the edge carries.
//!
//! WHICH shared handler carries an edge is policy (`edges.rs`) and nothing may
//! depend on it: the model check below runs under the production chooser AND
//! an adversarial one.
//!
//! # Two independent per-kind slot vectors per subscriber
//!
//! Each subscriber S holds an AUDIO vector and a VIDEO vector, each sized by
//! S's TRUNCATED declaration of that kind (`capability::SlotDemand`): the cap
//! is applied BEFORE any per-slot work here, so a vector is never longer than
//! its cap and an over-cap slot has no table position at all (it is
//! `SLOT_STATE_UNAVAILABLE` on the wire). Caps are read ONCE at MC startup:
//! live vectors are never re-truncated in place, so an operator's cap change
//! (including the R-26 video-off lever, a video cap of 0) applies to the
//! declarations made after the MC restart that picks it up — and that restart
//! reconnects everyone.
//!
//! Eligibility for an empty slot of kind K in S's table:
//! - never S itself (explicit for BOTH kinds — video eligibility is "any
//!   participant", so self-exclusion is not inherited from a publish filter);
//! - routable, sharing a connected handler with S;
//! - for AUDIO: the candidate publishes audio. For VIDEO: ANY participant, so
//!   a receive-only or audio-only participant occupies a video slot and is
//!   shown `AVATAR_NO_VIDEO` (it has no video edge to render);
//! - not already holding a slot of kind K with S.
//!
//! Fill order for one empty slot of kind K: (1) the earliest joiner (lowest
//! [`JoinRank`]) holding a slot of the OTHER kind with S but none of K — which
//! completes a pair; else (2) the earliest eligible joiner. When S has empty
//! slots of BOTH kinds at one event, they fill [`KIND_FILL_ORDER`] (audio
//! first), ONE slot per kind per pass, repeating to a fixed point (lead ruling
//! 2026-10-21; the order changes the outcome, and audio-first is what lets
//! rule (1) complete pairs).
//!
//! # Stability beats pairing
//!
//! No join, leave, mute, other-kind redeclaration or unrelated connectivity
//! change ever changes an occupied slot. A leave frees only the leaver's
//! slots, each refilled by the fill rule; a redeclaration that keeps a kind's
//! count keeps its occupants by ordinal (growing appends empty slots, then
//! refills; shrinking truncates the tail). Mute is not table state at all.
//!
//! # Connectivity is observed, and "not yet connected" is not "unreachable"
//!
//! A member's `connected` is `None` until the meeting actor has settled what
//! the HANDLERS reported; see `actors/meeting_media.rs`. A member that is not
//! routable has no edges in either direction and is named unreachable to NO
//! ONE. Only two routable members with disjoint connected sets are unreachable
//! to each other.
//!
//! # Why state, and not a pure recompute
//!
//! Slot stability and edge stability are history-dependent, so the meeting
//! actor holds a [`SlotTable`] and mutates it: **admit** (a rank; nothing
//! moves), **set_demand** (both kinds' kept slot ids; then refill),
//! **remove** (free only the leaver's slots; refill) and **set_connectivity**
//! (for every edge touching the changed member whose handler left the pair's
//! shared set: move it to a still-shared handler, slot and `egress_stream_id`
//! kept, or free it if the pair is now unreachable; then refill). **An edge
//! whose handler is still shared by both parties is never touched.**
//!
//! # The invariants every mutation preserves
//!
//! 1. No subscriber holds itself, in either kind.
//! 2. No subscriber holds one sender twice within a kind.
//! 3. Every edge's handler is in `connected(subscriber) ∩ connected(sender)`.
//! 4. No subscriber has an empty slot of kind K while an eligible sender for K
//!    it does not hold in K exists.
//! 5. Each per-kind vector is exactly as long as the kept declaration of that
//!    kind; audio slots hold audio publishers only.
//! 6. `unreachable_for(s)` is exactly the routable others whose connected set
//!    is disjoint from `s`'s, and is empty while `s` is not routable.
//! 7. A rendered edge's stream is its SLOT's kind's stream; a video slot whose
//!    occupant publishes no video renders no edge.
//!
//! All are asserted after every step of the exhaustive small-scope model
//! check in this module's tests, against an INDEPENDENT expected-occupant
//! oracle.
//!
//! # Rank is not `sender_id`, and not a handler key
//!
//! [`JoinRank`] is a per-meeting monotonic counter and the ONLY ordering key
//! for join-order fill. A rank is internal: it never reaches the wire, a
//! metric label, a log field or a span attribute.
//!
//! # No key material
//!
//! Sender ids, slot ids, handler ids and booleans only.

use super::assignment::{
    egress_stream_id, priority_group_of, stream_number_of, transport_of, AssignmentError,
    EgressStreamPlan, HandlerAssignment, HandlerId, MeetingAssignment, SourceDispositionPlan,
    SourceStreamPlan, TransportModeOrd,
};
use super::edges::{colocate, meeting_seed, EdgeChooser, EdgeContext};
use super::placement::ConnectedHandlers;
use crate::media_admission::SenderId;
use crate::media_kind::{MediaKindLabel, PublishSet};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The kind order when a subscriber has empty slots of BOTH kinds at one
/// event: audio first (lead ruling 2026-10-21). The independent test oracles
/// restate it on purpose and import nothing from here
/// (`packages/web-app/e2e/slotOracle.ts:KIND_FILL_ORDER`, and this module's
/// own test oracle).
pub const KIND_FILL_ORDER: [MediaKindLabel; 2] = [MediaKindLabel::Audio, MediaKindLabel::Video];

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

/// One slot's occupant, and which handler carries the pair.
///
/// For a video slot whose occupant publishes no video this is a placeholder
/// assignment (`AVATAR_NO_VIDEO`): the handler is still one both parties
/// share, but no egress stream is rendered for it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    /// The participant filling this slot.
    pub source: SenderId,
    /// The one handler carrying this pair, a member of both parties' connected
    /// sets.
    pub handler: HandlerId,
}

/// One roster participant, as slot state sees them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Member {
    sender: SenderId,
    /// What this participant publishes, fixed for its membership.
    publishes: PublishSet,
    /// `None` = not routable. `Some` is never empty.
    connected: Option<ConnectedHandlers>,
    /// Kept slot ids per kind, in declaration order (laid out by
    /// [`MediaKindLabel::index`]). `slots[k][i]` fills `slot_ids[k][i]`.
    slot_ids: [Vec<u16>; 2],
    /// Who fills each kept slot, per kind.
    slots: [Vec<Option<Edge>>; 2],
}

impl Member {
    fn holds(&self, kind: MediaKindLabel, sender: SenderId) -> bool {
        kind.of_ref(&self.slots)
            .iter()
            .flatten()
            .any(|edge| edge.source == sender)
    }

    fn all_edges(&self) -> impl Iterator<Item = &Edge> {
        self.slots.iter().flat_map(|v| v.iter().flatten())
    }
}

/// The handlers `subscriber` and `publisher` both connect to, in id order.
///
/// ADR-0036 §9's visibility rule, **non-reflexive**. Empty if either party is
/// not routable. This is the ONE topology rule: every fill, every edge move and
/// the unreachable set derive from it.
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
            for edge in member.all_edges() {
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

/// One source's mute state, per kind, as the render needs it.
///
/// A READ-ONLY projection of the meeting actor's roster flags, built per
/// render and never retained (the roster is the one home of mute). Explicit
/// fields only — never a roster struct, which also holds identity material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceMute {
    /// Server-muted, per kind (laid out by [`MediaKindLabel::index`]).
    pub server: [bool; 2],
    /// Client-muted, per kind.
    pub client: [bool; 2],
}

/// The roster's per-kind mute flags, as every reader of mute sees them: the
/// render (`source_streams` dispositions and the legacy server-muted set) and
/// the slot-state composition (`media_signaling::assignments`).
///
/// A **read-only projection** of the meeting actor's roster, built per
/// reconcile and never retained (the roster is the one home of mute). ONE
/// accessor, [`Self::flags`], with ONE documented default, so a slot state and
/// an MH disposition cannot disagree about one source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMuteView {
    by_sender: HashMap<SenderId, SourceMute>,
}

impl SourceMuteView {
    /// Build from a roster projection.
    #[must_use]
    pub fn from_map(by_sender: HashMap<SenderId, SourceMute>) -> Self {
        Self { by_sender }
    }

    /// This source's flags.
    ///
    /// **A source absent from the projection fails CLOSED: server-muted on
    /// both kinds.** Unreachable by construction (the actor projects its whole
    /// roster, and every slot-table member is on it), so the default exists
    /// only to decide which way an MC defect fails: MH drops the stream at
    /// ingress and subscribers see `AVATAR_SERVER_MUTED`, never an unmute
    /// nobody asked for.
    #[must_use]
    pub fn flags(&self, sender: SenderId) -> SourceMute {
        self.by_sender.get(&sender).copied().unwrap_or(SourceMute {
            server: [true; 2],
            client: [false; 2],
        })
    }
}

/// The meeting's join-order slot and edge state.
#[derive(Clone)]
pub struct SlotTable {
    /// Every roster participant, in join order.
    members: BTreeMap<JoinRank, Member>,
    /// Reverse index; ordering always comes from `members`.
    rank_of: HashMap<SenderId, JoinRank>,
    /// The next rank to issue.
    next_rank: u64,
    /// Which shared handler a new or moved edge goes to. Production is always
    /// [`colocate`]; tests substitute adversarial choosers.
    chooser: EdgeChooser,
    /// This meeting's tiebreak rotation (`edges::meeting_seed`). Policy only.
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

/// Test-only, for the same reason [`SlotTable::new`] is.
#[cfg(test)]
impl Default for SlotTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotTable {
    /// An empty table for one meeting, with the production edge policy. The
    /// meeting id is used for NOTHING but the tiebreak rotation.
    #[must_use]
    pub fn for_meeting(meeting_id: &str) -> Self {
        Self {
            seed: meeting_seed(meeting_id),
            ..Self::with_chooser(colocate)
        }
    }

    /// An empty table with the production edge policy and the UNROTATED
    /// tiebreak (seed 0). **`#[cfg(test)]` on purpose**: a live meeting must go
    /// through [`Self::for_meeting`].
    #[cfg(test)]
    #[must_use]
    pub fn new() -> Self {
        Self::with_chooser(colocate)
    }

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

    /// Admit a participant with a fresh rank and its fixed publish set. It is
    /// not routable yet, so no slot anywhere changes.
    ///
    /// # Errors
    ///
    /// [`SlotTableError`] if the sender is already a member or the rank space
    /// is exhausted. Nothing is mutated on error.
    pub fn admit(
        &mut self,
        sender: SenderId,
        publishes: PublishSet,
    ) -> Result<JoinRank, SlotTableError> {
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
                publishes,
                connected: None,
                slot_ids: [Vec::new(), Vec::new()],
                slots: [Vec::new(), Vec::new()],
            },
        );
        self.rank_of.insert(sender, rank);
        Ok(rank)
    }

    /// Test fixture: admit an AUDIO-only publisher (story 2's only shape).
    #[cfg(test)]
    pub(crate) fn admit_audio(&mut self, sender: SenderId) -> Result<JoinRank, SlotTableError> {
        self.admit(sender, PublishSet::of(&[MediaKindLabel::Audio]))
    }

    /// Remove a participant. For every other subscriber only the leaver's
    /// slots are freed, then refilled. Returns `false` if not a member.
    pub fn remove(&mut self, sender: SenderId) -> bool {
        let Some(rank) = self.rank_of.remove(&sender) else {
            return false;
        };
        self.members.remove(&rank);
        for member in self.members.values_mut() {
            for slot in member.slots.iter_mut().flatten() {
                if slot.as_ref().is_some_and(|edge| edge.source == sender) {
                    *slot = None;
                }
            }
        }
        self.refill_all();
        true
    }

    /// Set a subscriber's KEPT slot ids of both kinds (already validated and
    /// TRUNCATED upstream — never sized from raw client input), laid out by
    /// [`MediaKindLabel::index`]. Both kinds land before ONE refill, so a
    /// declaration of both kinds fills in [`KIND_FILL_ORDER`]. A kind whose
    /// count is unchanged keeps its occupants by ordinal. Returns `false` if
    /// the sender is not a member.
    pub fn set_demand(&mut self, sender: SenderId, slot_ids: [Vec<u16>; 2]) -> bool {
        let Some(&rank) = self.rank_of.get(&sender) else {
            return false;
        };
        let Some(member) = self.members.get_mut(&rank) else {
            return false;
        };
        let [audio, video] = slot_ids;
        for (kind, ids) in [
            (MediaKindLabel::Audio, audio),
            (MediaKindLabel::Video, video),
        ] {
            kind.of_mut(&mut member.slots).resize(ids.len(), None);
            *kind.of_mut(&mut member.slot_ids) = ids;
        }
        let mut loads = Loads::of(&self.members);
        self.refill(rank, &mut loads);
        true
    }

    /// Test fixture: every member present and unmuted.
    #[cfg(test)]
    pub(crate) fn quiet_mute(&self) -> SourceMuteView {
        SourceMuteView::from_map(
            self.members
                .values()
                .map(|m| (m.sender, SourceMute::default()))
                .collect(),
        )
    }

    /// Test fixture: set AUDIO demand only (story 2's only shape), leaving
    /// video empty.
    #[cfg(test)]
    pub(crate) fn set_audio_demand(&mut self, sender: SenderId, slot_ids: Vec<u16>) -> bool {
        self.set_demand(sender, [slot_ids, Vec::new()])
    }

    /// Set a member's settled connectivity. `None`, or an empty set, makes it
    /// not routable. Returns `false` if the sender is not a member.
    ///
    /// Only edges involving `sender` can change, and of those only the ones
    /// whose handler is no longer shared by both parties.
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
        // longer serves, in both kinds.
        let mut loads = Loads::of(&self.members);
        let ranks: Vec<JoinRank> = self.members.keys().copied().collect();
        for subscriber_rank in &ranks {
            let Some(subscriber) = self.members.get(subscriber_rank) else {
                continue;
            };
            let mut repairs: Vec<(MediaKindLabel, usize, Option<Edge>)> = Vec::new();
            for kind in MediaKindLabel::ALL {
                for (ordinal, slot) in kind.of_ref(&subscriber.slots).iter().enumerate() {
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
                    repairs.push((kind, ordinal, replacement));
                }
            }
            if let Some(subscriber) = self.members.get_mut(subscriber_rank) {
                for (kind, ordinal, replacement) in repairs {
                    if let Some(slot) = kind.of_mut(&mut subscriber.slots).get_mut(ordinal) {
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

    /// Test fixture: admit `sender` (audio-only) and connect it to `on`
    /// through the production connectivity path.
    #[cfg(test)]
    pub(crate) fn admit_on(
        &mut self,
        sender: SenderId,
        handlers: &super::placement::MeetingHandlers,
        on: &[&str],
    ) {
        self.admit_on_publishing(
            sender,
            PublishSet::of(&[MediaKindLabel::Audio]),
            handlers,
            on,
        );
    }

    /// Test fixture: as [`Self::admit_on`], with a chosen publish set.
    #[cfg(test)]
    pub(crate) fn admit_on_publishing(
        &mut self,
        sender: SenderId,
        publishes: PublishSet,
        handlers: &super::placement::MeetingHandlers,
        on: &[&str],
    ) {
        let _ = self.admit(sender, publishes);
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

    /// The fill rule's pick for one empty slot of `kind` in `member`'s table:
    /// rule (1) — the earliest eligible joiner holding a slot of the OTHER kind
    /// with this subscriber — else rule (2), the earliest eligible joiner.
    fn candidate(
        &self,
        member: &Member,
        kind: MediaKindLabel,
    ) -> Option<(SenderId, Vec<HandlerId>)> {
        let mut earliest: Option<(SenderId, Vec<HandlerId>)> = None;
        for publisher in self.members.values() {
            if member.holds(kind, publisher.sender) {
                continue;
            }
            if kind == MediaKindLabel::Audio
                && !publisher.publishes.publishes(MediaKindLabel::Audio)
            {
                continue;
            }
            let handlers = shared(member, publisher);
            if handlers.is_empty() {
                continue; // includes self (non-reflexive) and the unroutable
            }
            if member.holds(kind.other(), publisher.sender) {
                return Some((publisher.sender, handlers)); // rule (1)
            }
            if earliest.is_none() {
                earliest = Some((publisher.sender, handlers));
            }
        }
        earliest // rule (2)
    }

    /// Fill a subscriber's empty slots to a fixed point, one slot per kind per
    /// pass in [`KIND_FILL_ORDER`], first empty slot of the kind first.
    fn refill(&mut self, subscriber: JoinRank, loads: &mut Loads) {
        loop {
            let mut progressed = false;
            for kind in KIND_FILL_ORDER {
                let Some(member) = self.members.get(&subscriber) else {
                    return;
                };
                let Some(empty) = kind.of_ref(&member.slots).iter().position(Option::is_none)
                else {
                    continue;
                };
                let Some((source, handlers)) = self.candidate(member, kind) else {
                    continue;
                };
                let Some(handler) = loads.choose(self.chooser, self.seed, source, &handlers) else {
                    continue;
                };
                let edge = Edge { source, handler };
                loads.add(&edge);
                if let Some(slot) = self
                    .members
                    .get_mut(&subscriber)
                    .and_then(|m| kind.of_mut(&mut m.slots).get_mut(empty))
                {
                    *slot = Some(edge);
                    progressed = true;
                }
            }
            if !progressed {
                return;
            }
        }
    }

    fn member(&self, sender: SenderId) -> Option<&Member> {
        self.rank_of.get(&sender).and_then(|r| self.members.get(r))
    }

    /// Does this occupant of a `kind` slot render an egress stream? Audio
    /// always (audio slots hold audio publishers only); video only if the
    /// occupant publishes video — otherwise the slot is `AVATAR_NO_VIDEO`.
    fn renders(&self, kind: MediaKindLabel, source: SenderId) -> bool {
        kind == MediaKindLabel::Audio
            || self
                .member(source)
                .is_some_and(|p| p.publishes.publishes(MediaKindLabel::Video))
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

    /// The sender's fixed publish set.
    #[must_use]
    pub fn publishes_of(&self, sender: SenderId) -> Option<PublishSet> {
        self.member(sender).map(|m| m.publishes)
    }

    /// The sender's settled connected set, or `None` if it is not routable.
    #[must_use]
    pub fn connected_of(&self, sender: SenderId) -> Option<&ConnectedHandlers> {
        self.member(sender).and_then(|m| m.connected.as_ref())
    }

    /// Who fills each of the sender's kept slots of `kind`, in ordinal order.
    #[must_use]
    pub fn slots_of(&self, sender: SenderId, kind: MediaKindLabel) -> Option<&[Option<Edge>]> {
        self.member(sender)
            .map(|m| kind.of_ref(&m.slots).as_slice())
    }

    /// The subscribers currently holding `source` in some slot of either kind,
    /// in join order.
    #[must_use]
    pub fn holders_of(&self, source: SenderId) -> Vec<SenderId> {
        self.members
            .values()
            .filter(|m| MediaKindLabel::ALL.into_iter().any(|k| m.holds(k, source)))
            .map(|m| m.sender)
            .collect()
    }

    /// The handlers `sender` must send `kind` to: exactly those owning at
    /// least one of its RENDERED out-edges of that kind.
    #[must_use]
    pub fn targets_of(&self, sender: SenderId, kind: MediaKindLabel) -> BTreeSet<HandlerId> {
        if !self.renders(kind, sender) {
            return BTreeSet::new();
        }
        self.members
            .values()
            .flat_map(|m| kind.of_ref(&m.slots).iter().flatten())
            .filter(|edge| edge.source == sender)
            .map(|edge| edge.handler.clone())
            .collect()
    }

    /// How many of `subscriber`'s RENDERED in-edges each handler carries — the
    /// identity-free summary the connectivity INFO line logs.
    #[must_use]
    pub fn in_edge_handlers(&self, subscriber: SenderId) -> BTreeMap<HandlerId, usize> {
        let mut out = BTreeMap::new();
        if let Some(member) = self.member(subscriber) {
            for kind in MediaKindLabel::ALL {
                for edge in kind.of_ref(&member.slots).iter().flatten() {
                    if self.renders(kind, edge.source) {
                        *out.entry(edge.handler.clone()).or_default() += 1;
                    }
                }
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

    /// The roster participants `subscriber` cannot reach: routable members
    /// whose connected set is disjoint from the subscriber's (§9; story 2
    /// R-33), sorted numerically. Empty while the subscriber itself is not
    /// routable. Kind-free: unreachability is a property of the pair.
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
    /// # Egress streams
    ///
    /// One [`EgressStreamPlan`] per RENDERED slot — every filled audio slot,
    /// and every filled video slot whose occupant publishes video — with
    /// exactly one pinned candidate, on the handler that owns that edge. No
    /// egress stream exists for an `AVATAR_NO_VIDEO`, `UNAVAILABLE`,
    /// `FEWER_SOURCES_THAN_SLOTS` or unreachable slot. The plan's stream is
    /// [`stream_number_of`] the SLOT's kind — never derived from the
    /// candidate's publish set — so an edge never crosses kinds. Video edges
    /// supersede on an independent frame and rank below audio
    /// ([`priority_group_of`]). `EgressStream.transport_mode` is no longer
    /// written: an edge's transport derives from its candidate's
    /// `SourceStream`.
    ///
    /// Every handler in `handlers` gets an entry, possibly empty: MH only
    /// reports a participant's connection once the meeting is REGISTERED on
    /// that handler.
    ///
    /// # `source_streams`: every published stream, on every handler
    ///
    /// Each handler's snapshot declares one [`SourceStreamPlan`] per (member,
    /// stream) in EVERY member's publish set — a superset of every directed
    /// stream, so nothing MC directs is ever unmatched at MH. Transport is
    /// [`transport_of`] the kind; the disposition is DROP_AT_INGRESS if that
    /// kind is server-muted, else PAUSED if client-muted, else FORWARD. Size:
    /// at most 2 x roster, inside MH's `MH_MAX_SOURCE_STREAMS_PER_MEETING`
    /// (1024 in `infra/services/mh-service/config.env`) for any meeting of up
    /// to 512 participants (the organisation per-meeting default is 100,
    /// `migrations/20250118000001_initial_schema.sql`). MH's three
    /// apply-time rejects hold by construction and are tested: one entry per
    /// (sender, stream) (a set), every candidate listed (a candidate renders
    /// only for a published stream of a member), one transport per (sender,
    /// stream) (a function of the kind).
    ///
    /// `mute` is REQUIRED, never defaulted: the roster is the one home for
    /// mute, and a render that defaulted to "nobody muted" would be an unmute
    /// nobody asked for. A member absent from `mute` fails CLOSED
    /// ([`SourceMuteView::flags`]).
    ///
    /// # The legacy server-muted set (MC-B deletes it)
    ///
    /// `server_muted_sources` is written exactly as story 2 computed it: the
    /// AUDIO-server-muted senders that source at least one AUDIO egress stream
    /// on this handler (edge ownership, never connectivity), bounded by
    /// `|set| <= |audio egress_streams|` because each audio stream carries one
    /// candidate.
    ///
    /// # Errors
    ///
    /// [`AssignmentError::EgressOrdinalOverflow`] if an ordinal does not fit
    /// the 8 bits `egress_stream_id` allots it.
    pub fn render<'a>(
        &self,
        handlers: impl IntoIterator<Item = &'a HandlerId>,
        mute: &SourceMuteView,
    ) -> Result<MeetingAssignment, AssignmentError> {
        let mut per_handler: BTreeMap<HandlerId, HandlerAssignment> = handlers
            .into_iter()
            .map(|h| (h.clone(), HandlerAssignment::default()))
            .collect();

        let mut source_streams: BTreeSet<SourceStreamPlan> = BTreeSet::new();
        for member in self.members.values() {
            let flags = mute.flags(member.sender);
            for kind in member.publishes.kinds() {
                source_streams.insert(SourceStreamPlan {
                    sender: member.sender,
                    stream_number: stream_number_of(kind),
                    transport_mode: TransportModeOrd::new(transport_of(kind)),
                    disposition: SourceDispositionPlan::from_mute(
                        *kind.of_ref(&flags.server),
                        *kind.of_ref(&flags.client),
                    ),
                });
            }

            for kind in MediaKindLabel::ALL {
                let slots = kind.of_ref(&member.slots);
                let ids = kind.of_ref(&member.slot_ids);
                for (ordinal, (slot, slot_id)) in slots.iter().zip(ids).enumerate() {
                    let Some(edge) = slot else {
                        continue;
                    };
                    if !self.renders(kind, edge.source) {
                        continue; // AVATAR_NO_VIDEO: no stream to forward
                    }
                    let plan = EgressStreamPlan {
                        egress_stream_id: egress_stream_id(member.sender, kind, ordinal)?,
                        subscriber: member.sender,
                        slot_id: *slot_id,
                        // Exactly one candidate: the muted-set bound rests on it.
                        candidate_sources: vec![edge.source],
                        stream_number: stream_number_of(kind),
                        priority_group: priority_group_of(kind),
                        // MH is told the behaviour, never the media type.
                        supersede_on_independent_frame: kind == MediaKindLabel::Video,
                    };
                    per_handler
                        .entry(edge.handler.clone())
                        .or_default()
                        .egress_streams
                        .push(plan);
                }
            }
        }

        let audio_stream = stream_number_of(MediaKindLabel::Audio);
        for assignment in per_handler.values_mut() {
            assignment
                .egress_streams
                .sort_by_key(|s| s.egress_stream_id);
            assignment.server_muted_sources = assignment
                .egress_streams
                .iter()
                .filter(|plan| plan.stream_number == audio_stream)
                .flat_map(|plan| plan.candidate_sources.iter().copied())
                .filter(|source| *MediaKindLabel::Audio.of_ref(&mute.flags(*source).server))
                .collect();
            assignment.source_streams = source_streams.clone();
        }
        Ok(MeetingAssignment { per_handler })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_routing::placement::{HandlerEndpoint, MeetingHandlers};
    use proto_gen::dark_tower::signaling::v1::SlotState;
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
            .slots_of(sender(s), MediaKindLabel::Audio)
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

    /// Every listed sender AUDIO-server-muted; everyone else absent (the
    /// legacy muted set reads presence; these tests do not read dispositions).
    fn audio_server_muted(t: &SlotTable, ids: &[u16]) -> SourceMuteView {
        let mut map: HashMap<SenderId, SourceMute> = t
            .members
            .values()
            .map(|m| (m.sender, SourceMute::default()))
            .collect();
        for s in ids {
            map.insert(
                sender(*s),
                SourceMute {
                    server: [true, false],
                    client: [false; 2],
                },
            );
        }
        SourceMuteView::from_map(map)
    }

    fn admit(t: &mut SlotTable, s: u16) {
        t.admit_audio(sender(s)).unwrap();
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
            t.set_audio_demand(sender(1), vec![0]);
            t.set_audio_demand(sender(2), vec![0]);
            connect_all(&mut t, &[1, 2]);
            let handler = t
                .slots_of(sender(1), MediaKindLabel::Audio)
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
            t.set_audio_demand(sender(1), vec![0]);
            t.set_audio_demand(sender(2), vec![0]);
            connect_all(&mut t, &[1, 2]);
            t.slots_of(sender(1), MediaKindLabel::Audio)
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
            for edge in m.all_edges() {
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
            for kind in MediaKindLabel::ALL {
                let owners: BTreeSet<HandlerId> = if t.renders(kind, m.sender) {
                    t.members
                        .values()
                        .flat_map(|o| kind.of_ref(&o.slots).iter().flatten())
                        .filter(|e| e.source == m.sender)
                        .map(|e| e.handler.clone())
                        .collect()
                } else {
                    BTreeSet::new()
                };
                assert_eq!(
                    t.targets_of(m.sender, kind),
                    owners,
                    "targets = owners of rendered out-edges"
                );
            }
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
                t.set_audio_demand(sender(s), vec![10, 20]);
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
            assert_eq!(
                t.targets_of(sender(1), MediaKindLabel::Audio).len(),
                2,
                "A sends to both handlers"
            );
            assert_eq!(t.targets_of(sender(2), MediaKindLabel::Audio).len(), 1);
            assert_eq!(t.targets_of(sender(3), MediaKindLabel::Audio).len(), 1);
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
            t.set_audio_demand(sender(s), vec![0, 1, 2]);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        for s in 1..=4u16 {
            let heard: HashSet<u16> = sources(&t, s).into_iter().flatten().collect();
            let others: HashSet<u16> = (1..=4).filter(|x| *x != s).collect();
            assert_eq!(heard, others);
            assert!(unreachable(&t, s).is_empty());
            assert_eq!(
                t.targets_of(sender(s), MediaKindLabel::Audio).len(),
                1,
                "one target per sender"
            );
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
            t.set_audio_demand(sender(s), vec![0, 1, 2]);
        }
        connect_all(&mut t, &[1, 2, 3, 4]);
        let before = t.clone();
        // Participant 4 loses whichever handler carries its edges.
        let lost = t
            .targets_of(sender(4), MediaKindLabel::Audio)
            .into_iter()
            .next()
            .unwrap();
        let kept: Vec<&str> = H.iter().copied().filter(|x| *x != lost.as_str()).collect();
        t.set_connectivity(sender(4), on(&kept));

        for m in before.members.values() {
            let now = t.member(m.sender).unwrap();
            let audio = MediaKindLabel::Audio;
            for (old, new) in audio.of_ref(&m.slots).iter().zip(audio.of_ref(&now.slots)) {
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
            t.set_audio_demand(sender(s), vec![0, 1, 2, 3]);
        }
        connect_all(&mut t, &[1, 2, 3]);
        let before: Vec<Vec<Option<Edge>>> = (1..=3)
            .map(|s| {
                t.slots_of(sender(s), MediaKindLabel::Audio)
                    .unwrap()
                    .to_vec()
            })
            .collect();
        admit(&mut t, 4);
        t.set_connectivity(sender(4), on(&["mh-1"]));
        for (i, s) in (1..=3u16).enumerate() {
            let after = t.slots_of(sender(s), MediaKindLabel::Audio).unwrap();
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
        t.set_audio_demand(sender(1), vec![0, 1, 2]);
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
            t.set_audio_demand(sender(s), vec![0]);
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
            t.set_audio_demand(sender(s), vec![0, 1]);
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
        t.set_audio_demand(sender(1), vec![0, 1]);
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
        t.set_audio_demand(sender(4), vec![0, 1]);
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
        t.set_audio_demand(sender(1), vec![0, 1]);
        t.set_audio_demand(sender(2), vec![0]);
        t.set_audio_demand(sender(3), vec![0]);
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
        let render = t.render(set.ids(), &audio_server_muted(&t, &[1])).unwrap();
        assert_eq!(muted_on(&render, "mh-0"), vec![1]);
        assert_eq!(muted_on(&render, "mh-1"), vec![1]);
    }

    #[test]
    fn a_muted_sender_is_only_on_the_handlers_carrying_its_edges() {
        let (t, set) = split_meeting();
        let render = t.render(set.ids(), &audio_server_muted(&t, &[2])).unwrap();
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
        let render = t.render(set.ids(), &audio_server_muted(&t, &[4])).unwrap();
        assert!(muted_on(&render, "mh-0").is_empty());
        assert!(muted_on(&render, "mh-1").is_empty());

        let unmuted = t.render(set.ids(), &t.quiet_mute()).unwrap();
        assert!(muted_on(&unmuted, "mh-0").is_empty());
        assert!(muted_on(&unmuted, "mh-1").is_empty());
    }

    /// The bound MH relies on: each handler's set is no larger than its egress
    /// streams, even with EVERY participant muted.
    #[test]
    fn a_handlers_muted_set_never_exceeds_its_egress_streams() {
        let (t, set) = split_meeting();
        let render = t
            .render(set.ids(), &audio_server_muted(&t, &[1, 2, 3]))
            .unwrap();
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
        t.set_audio_demand(sender(1), vec![0, 1, 2]);
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
        t.set_audio_demand(sender(1), vec![0, 1]);
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
        t.set_audio_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        t.set_audio_demand(sender(1), vec![0, 1]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3)]);
        t.set_audio_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        t.set_audio_demand(sender(1), vec![9, 8, 7]);
        assert_eq!(sources(&t, 1), vec![Some(2), Some(3), Some(4)]);
        let render = t.render(handler_set().ids(), &t.quiet_mute()).unwrap();
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
        t.set_audio_demand(sender(4), vec![0]);
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
        t.set_audio_demand(sender(7), vec![0, 1]);
        connect_all(&mut t, &[7]);
        assert_eq!(sources(&t, 7), vec![None, None]);
        let render = t.render(handler_set().ids(), &t.quiet_mute()).unwrap();
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
            t.set_audio_demand(sender(s), vec![0]);
        }
        connect_all(&mut t, &[1, 2]);
        let find = |t: &SlotTable| {
            t.render(handler_set().ids(), &t.quiet_mute())
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
        assert_eq!(
            t.admit_audio(sender(1)),
            Err(SlotTableError::AlreadyAdmitted)
        );
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
        use rand::{RngExt, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed_0020);
        let subsets: [&[&str]; 4] = [&[], &["mh-0"], &["mh-1"], &["mh-0", "mh-1"]];
        let mut production_full_mesh_cases = 0u32;
        for case in 0..400 {
            for (chooser, is_production) in [
                (colocate as EdgeChooser, true),
                (last as EdgeChooser, false),
                (spread as EdgeChooser, false),
            ] {
                let mut t = SlotTable::with_chooser(chooser);
                let n: u16 = rng.random_range(1..=7);
                let mut all_connected = true;
                for s in 1..=n {
                    admit(&mut t, s);
                    t.set_audio_demand(sender(s), (0..rng.random_range(0..=4u16)).collect());
                }
                for s in 1..=n {
                    let pick = subsets[rng.random_range(0..subsets.len())];
                    all_connected &= pick.len() == H.len();
                    t.set_connectivity(sender(s), if pick.is_empty() { None } else { on(pick) });
                }
                assert_properties(&t);
                // Determinism: same inputs, same state.
                let mut replay = SlotTable::with_chooser(chooser);
                for m in t.members.values() {
                    replay.admit(m.sender, m.publishes).unwrap();
                    replay.set_demand(m.sender, m.slot_ids.clone());
                }
                for m in t.members.values() {
                    replay.set_connectivity(m.sender, m.connected.clone());
                }
                assert_eq!(replay, t, "case {case}: deterministic");
                if all_connected && is_production {
                    production_full_mesh_cases += 1;
                    for s in 1..=n {
                        assert!(
                            t.targets_of(sender(s), MediaKindLabel::Audio).len() <= 1,
                            "case {case}"
                        );
                    }
                }
            }
        }
        assert!(
            production_full_mesh_cases > 0,
            "generator never produced an all-connected roster for the production chooser"
        );
    }

    // ------------------------------------------- two-kind behaviour (story 3)

    const AU: MediaKindLabel = MediaKindLabel::Audio;
    const VI: MediaKindLabel = MediaKindLabel::Video;

    fn kinds_of(s: &str) -> PublishSet {
        let mut k = Vec::new();
        if s.contains('A') {
            k.push(AU);
        }
        if s.contains('V') {
            k.push(VI);
        }
        PublishSet::of(&k)
    }

    fn occupants(t: &SlotTable, s: u16, kind: MediaKindLabel) -> Vec<Option<u16>> {
        t.slots_of(sender(s), kind)
            .unwrap()
            .iter()
            .map(|slot| slot.as_ref().map(|e| e.source.get().get()))
            .collect()
    }

    /// The lead's kind-order example (2026-10-21), LITERAL: receive-only A
    /// earliest, B publishes both, N=M=1 for S. Audio-first gives audio=B,
    /// video=B (rule (1) completes the pair); video-first would give video=A.
    #[test]
    fn kind_order_example_fills_audio_first_and_completes_the_pair() {
        let mut t = SlotTable::new();
        t.admit(sender(1), kinds_of("")).unwrap(); // A, receive-only
        t.admit(sender(2), kinds_of("AV")).unwrap(); // B
        t.admit(sender(3), kinds_of("AV")).unwrap(); // S
        connect_all(&mut t, &[1, 2, 3]);
        t.set_demand(sender(3), [vec![0], vec![1]]);
        assert_eq!(occupants(&t, 3, AU), vec![Some(2)]);
        assert_eq!(occupants(&t, 3, VI), vec![Some(2)]);
    }

    /// Stability beats pairing: R (receive-only) holds S's video slot before P
    /// (AV) joins; P takes S's audio, and the video slot STILL holds R. When R
    /// leaves, video refills to P by rule (1).
    #[test]
    fn stability_beats_pairing_then_a_leave_refills_by_rule_one() {
        let mut t = SlotTable::new();
        t.admit(sender(1), kinds_of("AV")).unwrap(); // S
        t.set_demand(sender(1), [vec![0], vec![1]]);
        t.admit(sender(2), kinds_of("")).unwrap(); // R
        connect_all(&mut t, &[1, 2]);
        assert_eq!(occupants(&t, 1, AU), vec![None], "R publishes no audio");
        assert_eq!(
            occupants(&t, 1, VI),
            vec![Some(2)],
            "any participant fills video"
        );
        t.admit(sender(3), kinds_of("AV")).unwrap(); // P
        connect_all(&mut t, &[3]);
        assert_eq!(occupants(&t, 1, AU), vec![Some(3)]);
        assert_eq!(
            occupants(&t, 1, VI),
            vec![Some(2)],
            "an occupied slot never moves"
        );
        t.remove(sender(2));
        assert_eq!(occupants(&t, 1, VI), vec![Some(3)]);
    }

    /// No self in a video slot, even though video eligibility is "any
    /// participant": self-exclusion is explicit.
    #[test]
    fn a_solo_participant_holds_itself_in_no_slot_of_either_kind() {
        let mut t = SlotTable::new();
        t.admit(sender(1), kinds_of("AV")).unwrap();
        t.set_demand(sender(1), [vec![0], vec![1]]);
        connect_all(&mut t, &[1]);
        assert_eq!(occupants(&t, 1, AU), vec![None]);
        assert_eq!(occupants(&t, 1, VI), vec![None]);
    }

    /// A video slot held by a non-video publisher renders NO egress stream;
    /// one held by a video publisher renders stream 1, stream-per-group,
    /// superseding, priority 2 — and audio is unchanged.
    #[test]
    fn video_edges_render_only_for_video_publishers_with_the_video_behaviours() {
        let set = handler_set();
        let mut t = SlotTable::new();
        t.admit_on_publishing(sender(1), kinds_of("AV"), &set, &["mh-0"]);
        t.admit_on_publishing(sender(2), kinds_of("A"), &set, &["mh-0"]);
        t.admit_on_publishing(sender(3), kinds_of("AV"), &set, &["mh-0"]);
        t.set_demand(sender(1), [vec![0, 1], vec![10, 11]]);
        let render = t.render(set.ids(), &t.quiet_mute()).unwrap();
        let plans: Vec<&EgressStreamPlan> = render
            .per_handler
            .values()
            .flat_map(|p| &p.egress_streams)
            .collect();
        let video: Vec<&&EgressStreamPlan> =
            plans.iter().filter(|p| p.stream_number == 1).collect();
        assert_eq!(
            video.len(),
            1,
            "2 is in a video slot as AVATAR_NO_VIDEO: no stream"
        );
        assert_eq!(video[0].candidate_sources, vec![sender(3)]);
        assert!(video[0].supersede_on_independent_frame);
        assert_eq!(video[0].priority_group, 2);
        let audio: Vec<&&EgressStreamPlan> =
            plans.iter().filter(|p| p.stream_number == 0).collect();
        assert_eq!(audio.len(), 2);
        assert!(audio
            .iter()
            .all(|p| !p.supersede_on_independent_frame && p.priority_group == 1));
        assert_eq!(occupants(&t, 1, VI), vec![Some(2), Some(3)]);
    }

    /// `source_streams`: one entry per (sender, stream) in every publish set,
    /// audio 0 DATAGRAM / video 1 STREAM_PER_GROUP, dispositions server >
    /// client > forward, identical on every handler; a non-video publisher has
    /// no stream-1 entry.
    #[test]
    fn source_streams_declare_every_published_stream_with_its_disposition() {
        use proto_gen::dark_tower::signaling::v1::TransportMode;
        let set = handler_set();
        let mut t = SlotTable::new();
        t.admit_on_publishing(sender(1), kinds_of("AV"), &set, &["mh-0"]);
        t.admit_on_publishing(sender(2), kinds_of("A"), &set, &["mh-1"]);
        t.admit_on_publishing(sender(3), kinds_of(""), &set, &["mh-0"]);
        let mute = HashMap::from([
            (
                sender(1),
                SourceMute {
                    server: [false, true],
                    client: [true, true],
                },
            ),
            (sender(2), SourceMute::default()),
            (sender(3), SourceMute::default()),
        ]);
        let render = t
            .render(set.ids(), &SourceMuteView::from_map(mute))
            .unwrap();
        let expected: BTreeSet<(u16, u8, TransportMode, SourceDispositionPlan)> = [
            (1, 0, TransportMode::Datagram, SourceDispositionPlan::Paused),
            (
                1,
                1,
                TransportMode::StreamPerGroup,
                SourceDispositionPlan::DropAtIngress,
            ),
            (
                2,
                0,
                TransportMode::Datagram,
                SourceDispositionPlan::Forward,
            ),
        ]
        .into_iter()
        .collect();
        for policy in render.per_handler.values() {
            let got: BTreeSet<(u16, u8, TransportMode, SourceDispositionPlan)> = policy
                .source_streams
                .iter()
                .map(|s| {
                    (
                        s.sender.get().get(),
                        s.stream_number,
                        s.transport_mode.mode(),
                        s.disposition,
                    )
                })
                .collect();
            assert_eq!(got, expected);
        }
    }

    /// A disposition change changes the rendered output (so the generation
    /// advances); an identical re-render is equal (so it does not).
    #[test]
    fn a_disposition_change_changes_the_render_and_a_rerender_does_not() {
        let set = handler_set();
        let mut t = SlotTable::new();
        t.admit_on_publishing(sender(1), kinds_of("AV"), &set, &["mh-0"]);
        t.admit_on_publishing(sender(2), kinds_of("AV"), &set, &["mh-0"]);
        t.set_demand(sender(2), [vec![0], vec![1]]);
        let quiet = HashMap::from([
            (sender(1), SourceMute::default()),
            (sender(2), SourceMute::default()),
        ]);
        let quiet = SourceMuteView::from_map(quiet);
        let a = t.render(set.ids(), &quiet).unwrap();
        assert_eq!(a, t.render(set.ids(), &quiet).unwrap());
        let muted = HashMap::from([
            (
                sender(1),
                SourceMute {
                    server: [false; 2],
                    client: [false, true],
                },
            ),
            (sender(2), SourceMute::default()),
        ]);
        assert_ne!(
            a,
            t.render(set.ids(), &SourceMuteView::from_map(muted))
                .unwrap()
        );
    }

    // ------------------------------------------------- exhaustive model check

    /// Per-kind caps the model check truncates to: 1 slot of each kind, so a
    /// declared count of 2 is over cap. Small on purpose (state-space cost).
    const CAPS: [u8; 2] = [1, 1];

    /// One participant in the model-check world (person index, not sender id).
    #[derive(Clone, Debug)]
    struct Person {
        sender: SenderId,
        publish: [bool; 2],
        /// 0 = not routable; bit 0 = mh-0, bit 1 = mh-1.
        conn: u8,
        declared: [u8; 2],
        client: [bool; 2],
        server: [bool; 2],
    }

    // ---- the INDEPENDENT oracle ------------------------------------------
    //
    // Written from the rule text (story 3 MC-A "SLOT TABLE" bullets) and
    // sharing NO code with the fill above: no `shared`, no `holds`, no
    // `Loads`, no `KIND_FILL_ORDER`, no `candidate`. It tracks occupants by
    // PERSON index, per subscriber, per kind. Deliberately a second
    // implementation — collapsing it into the fill's helpers would let one
    // bug pass both (DRY Gate 1, item G). Also independent of
    // `packages/web-app/e2e/slotOracle.ts`; neither is derived from the other.

    /// The oracle's own kind order: audio, then video, ONE slot at a time, to
    /// a fixed point (lead ruling 2026-10-21). Restated here on purpose; it
    /// mirrors the rule text and `slotOracle.ts:KIND_FILL_ORDER`, and is NOT
    /// imported from the fill.
    const ORACLE_KIND_ORDER: [usize; 2] = [0, 1]; // 0 = audio, 1 = video

    #[derive(Clone, Debug, Default)]
    struct OMember {
        person: u8,
        publish: [bool; 2],
        conn: u8,
        /// Occupants by person index, per kind (0 audio, 1 video); length is
        /// the KEPT (truncated) count.
        slots: [Vec<Option<u8>>; 2],
    }

    #[derive(Clone, Debug, Default)]
    struct Oracle {
        /// In join order.
        members: Vec<OMember>,
        caps: [usize; 2],
    }

    #[derive(Default, Debug)]
    struct Controls {
        rule_one_differs: usize,
        order_mattered: usize,
        unavailable_seen: usize,
        non_video_in_video_slot: usize,
        mute_flip_changed_a_view: usize,
        per_op: [usize; 6],
    }

    impl Oracle {
        fn new(caps: [u8; 2]) -> Self {
            Self {
                members: Vec::new(),
                caps: [usize::from(caps[0]), usize::from(caps[1])],
            }
        }

        fn idx(&self, person: u8) -> usize {
            self.members
                .iter()
                .position(|m| m.person == person)
                .unwrap()
        }

        /// Two members can be paired iff they are distinct, both routable, and
        /// share a handler.
        fn reach(a: &OMember, b: &OMember) -> bool {
            a.person != b.person && a.conn != 0 && b.conn != 0 && (a.conn & b.conn) != 0
        }

        /// The rule-text pick for an empty slot of `kind` at subscriber `i`.
        fn pick(&self, i: usize, kind: usize, controls: &mut Controls) -> Option<u8> {
            let s = &self.members[i];
            let eligible: Vec<&OMember> = self
                .members
                .iter()
                .filter(|c| Self::reach(s, c))
                .filter(|c| kind == 1 || c.publish[0])
                .filter(|c| !s.slots[kind].contains(&Some(c.person)))
                .collect();
            let earliest = eligible.first().map(|c| c.person);
            let pair = eligible
                .iter()
                .find(|c| s.slots[1 - kind].contains(&Some(c.person)))
                .map(|c| c.person);
            if pair.is_some() && pair != earliest {
                controls.rule_one_differs += 1;
            }
            pair.or(earliest)
        }

        fn fill(&mut self, i: usize, order: [usize; 2], controls: &mut Controls) {
            loop {
                let mut changed = false;
                for kind in order {
                    let Some(empty) = self.members[i].slots[kind].iter().position(Option::is_none)
                    else {
                        continue;
                    };
                    if let Some(c) = self.pick(i, kind, controls) {
                        self.members[i].slots[kind][empty] = Some(c);
                        changed = true;
                    }
                }
                if !changed {
                    return;
                }
            }
        }

        fn refill_all(&mut self, controls: &mut Controls) {
            for i in 0..self.members.len() {
                let both_empty = (0..2).all(|k| self.members[i].slots[k].contains(&None));
                if both_empty {
                    let mut reversed = self.clone();
                    reversed.fill(i, [1, 0], &mut Controls::default());
                    self.fill(i, ORACLE_KIND_ORDER, controls);
                    if reversed.members[i].slots != self.members[i].slots {
                        controls.order_mattered += 1;
                    }
                } else {
                    self.fill(i, ORACLE_KIND_ORDER, controls);
                }
            }
        }

        fn join(&mut self, person: u8, publish: [bool; 2]) {
            self.members.push(OMember {
                person,
                publish,
                ..OMember::default()
            });
        }

        fn leave(&mut self, person: u8, controls: &mut Controls) {
            self.members.retain(|m| m.person != person);
            for m in &mut self.members {
                for kind in 0..2 {
                    for slot in &mut m.slots[kind] {
                        if *slot == Some(person) {
                            *slot = None;
                        }
                    }
                }
            }
            self.refill_all(controls);
        }

        fn declare(&mut self, person: u8, kind: usize, n: usize, controls: &mut Controls) {
            let i = self.idx(person);
            let kept = n.min(self.caps[kind]);
            self.members[i].slots[kind].resize(kept, None);
            self.refill_all(controls);
        }

        fn connect(&mut self, person: u8, conn: u8, controls: &mut Controls) {
            let i = self.idx(person);
            self.members[i].conn = conn;
            let snapshot = self.members.clone();
            for m in &mut self.members {
                for kind in 0..2 {
                    for slot in &mut m.slots[kind] {
                        if let Some(o) = *slot {
                            let occupant = snapshot.iter().find(|x| x.person == o).unwrap();
                            let me = snapshot.iter().find(|x| x.person == m.person).unwrap();
                            if !Self::reach(me, occupant) {
                                *slot = None;
                            }
                        }
                    }
                }
            }
            self.refill_all(controls);
        }

        /// The expected wire view of `person`'s declared slots, kind by kind in
        /// declaration order (audio then video): (state, named occupant).
        fn expected_view(
            &self,
            person: u8,
            people: &BTreeMap<u8, Person>,
        ) -> Vec<(SlotState, Option<u16>)> {
            let me = &self.members[self.idx(person)];
            let mut out = Vec::new();
            for kind in 0..2 {
                for i in 0..usize::from(people[&person].declared[kind]) {
                    if i >= self.caps[kind] {
                        out.push((SlotState::Unavailable, None));
                        continue;
                    }
                    let Some(o) = me.slots[kind][i] else {
                        out.push((SlotState::FewerSourcesThanSlots, None));
                        continue;
                    };
                    let p = &people[&o];
                    let named = Some(p.sender.get().get());
                    // NO-VIDEO PRECEDENCE: a video occupant that publishes no
                    // video is AVATAR_NO_VIDEO whatever its mutes.
                    out.push(if kind == 1 && !p.publish[1] {
                        (SlotState::AvatarNoVideo, named)
                    } else if p.server[kind] {
                        (SlotState::AvatarServerMuted, named)
                    } else if p.client[kind] {
                        (SlotState::AvatarClientMuted, named)
                    } else if kind == 1 {
                        (SlotState::Active, None) // live video names no sender
                    } else {
                        (SlotState::Active, named)
                    });
                }
            }
            out
        }
    }

    /// The oracle anchored to the rule text with LITERAL expectations (never
    /// read off production).
    #[test]
    fn oracle_self_tests_against_the_rule_text() {
        let mut c = Controls::default();
        // Kind-order example: A receive-only earliest, B AV, S AV; N=M=1. S
        // declares BOTH kinds while unconnected, so its connect is the ONE
        // event at which both kinds are empty.
        let mut o = Oracle::new([1, 1]);
        o.join(0, [false, false]);
        o.join(1, [true, true]);
        o.join(2, [true, true]);
        o.connect(0, 1, &mut c);
        o.connect(1, 1, &mut c);
        o.declare(2, 0, 1, &mut c);
        o.declare(2, 1, 1, &mut c);
        let mut video_first = o.clone();
        o.connect(2, 1, &mut c);
        // Audio first: audio=B (A has no audio), then video=B by rule (1).
        assert_eq!(o.members[2].slots, [vec![Some(1)], vec![Some(1)]]);
        // The case DISCRIMINATES: video first would give video=A.
        video_first.members[2].conn = 1;
        video_first.fill(2, [1, 0], &mut Controls::default());
        assert_eq!(video_first.members[2].slots, [vec![Some(1)], vec![Some(0)]]);

        // Stability beats pairing: R occupies video before P joins.
        let mut o = Oracle::new([1, 1]);
        o.join(0, [true, true]); // S
        o.declare(0, 0, 1, &mut c);
        o.declare(0, 1, 1, &mut c);
        o.join(1, [false, false]); // R
        o.connect(0, 1, &mut c);
        o.connect(1, 1, &mut c);
        assert_eq!(o.members[0].slots, [vec![None], vec![Some(1)]]);
        o.join(2, [true, true]); // P
        o.connect(2, 1, &mut c);
        assert_eq!(o.members[0].slots, [vec![Some(2)], vec![Some(1)]]);

        // NO-VIDEO PRECEDENCE under both mutes.
        let mut people = BTreeMap::new();
        for (p, publish) in [(0u8, [true, true]), (1, [true, false])] {
            people.insert(
                p,
                Person {
                    sender: sender(u16::from(p) + 1),
                    publish,
                    conn: 1,
                    declared: [1, 1],
                    client: [true, true],
                    server: [true, true],
                },
            );
        }
        let mut o = Oracle::new([1, 1]);
        o.join(0, [true, true]);
        o.join(1, [true, false]);
        o.connect(0, 1, &mut c);
        o.connect(1, 1, &mut c);
        o.declare(0, 0, 1, &mut c);
        o.declare(0, 1, 1, &mut c);
        assert_eq!(
            o.expected_view(0, &people),
            vec![
                (SlotState::AvatarServerMuted, Some(2)),
                (SlotState::AvatarNoVideo, Some(2)),
            ]
        );
    }

    #[derive(Clone, Copy, Debug)]
    enum Op {
        /// Join with a publish set, declaring one slot of each kind (as the
        /// join carries its declaration), then connecting to `mask`.
        Join(u8, usize, u8),
        Leave(u8),
        Declare(u8, usize, u8),
        ClientMute(u8, usize),
        ServerMute(u8, usize),
        Connect(u8, u8),
    }

    impl Op {
        fn index(self) -> usize {
            match self {
                Self::Join(..) => 0,
                Self::Leave(_) => 1,
                Self::Declare(..) => 2,
                Self::ClientMute(..) => 3,
                Self::ServerMute(..) => 4,
                Self::Connect(..) => 5,
            }
        }
    }

    const PUBLISH_SETS: [[bool; 2]; 4] =
        [[true, false], [false, true], [true, true], [false, false]];

    #[derive(Clone)]
    struct World {
        table: SlotTable,
        oracle: Oracle,
        people: BTreeMap<u8, Person>,
        next_sender: u16,
    }

    fn label_of(kind: usize) -> MediaKindLabel {
        if kind == 0 {
            AU
        } else {
            VI
        }
    }

    fn conn_set(mask: u8) -> Option<ConnectedHandlers> {
        match mask {
            0 => None,
            1 => on(&["mh-0"]),
            2 => on(&["mh-1"]),
            _ => all(),
        }
    }

    fn declaration_of(person: u8, declared: [u8; 2]) -> crate::media_signaling::SlotDemand {
        use proto_gen::dark_tower::signaling::v1::{MediaKind, ReceiveCapability, ReceiveSlot};
        let mut slots = Vec::new();
        for (kind, wire) in [(0usize, MediaKind::Audio), (1, MediaKind::VideoCamera)] {
            for i in 0..u32::from(declared[kind]) {
                slots.push(ReceiveSlot {
                    slot_id: u32::from(person) * 20 + u32::try_from(kind).unwrap() * 10 + i,
                    media_kind: wire as i32,
                    pinned_sender_id: None,
                });
            }
        }
        crate::media_signaling::ReceiveCapabilityDeclaration::parse(
            &ReceiveCapability { slots },
            true,
        )
        .unwrap()
        .truncate(CAPS)
    }

    fn mute_map(people: &BTreeMap<u8, Person>) -> HashMap<SenderId, SourceMute> {
        people
            .values()
            .map(|p| {
                (
                    p.sender,
                    SourceMute {
                        server: p.server,
                        client: p.client,
                    },
                )
            })
            .collect()
    }

    /// Every composed view, by person: (state, sender) per declared slot.
    fn views(w: &World) -> BTreeMap<u8, Vec<(SlotState, Option<u16>)>> {
        let set = handler_set();
        let mute = SourceMuteView::from_map(mute_map(&w.people));
        w.people
            .iter()
            .map(|(p, person)| {
                let composition = crate::media_signaling::build_stream_assignments(
                    person.sender,
                    &declaration_of(*p, person.declared),
                    &w.table,
                    &set,
                    &mute,
                    &w.table.unreachable_for(person.sender),
                )
                .unwrap();
                for a in &composition.assignments.assignments {
                    assert_ne!(
                        a.slot_state,
                        SlotState::SourceMuted as i32,
                        "never SOURCE_MUTED"
                    );
                    let state = SlotState::try_from(a.slot_state).unwrap();
                    match state {
                        SlotState::AvatarNoVideo
                        | SlotState::Unavailable
                        | SlotState::FewerSourcesThanSlots => {
                            assert!(a.media_handler_url.is_empty(), "{state:?} carries no url");
                        }
                        _ => assert!(!a.media_handler_url.is_empty(), "{state:?} names its edge"),
                    }
                    if state == SlotState::Active
                        && a.media_kind
                            == proto_gen::dark_tower::signaling::v1::MediaKind::VideoCamera as i32
                    {
                        assert!(a.sender_id.is_none(), "live video names no sender");
                    }
                }
                (
                    *p,
                    composition
                        .assignments
                        .assignments
                        .iter()
                        .map(|a| {
                            (
                                SlotState::try_from(a.slot_state).unwrap(),
                                a.sender_id.map(|s| u16::try_from(s).unwrap()),
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    fn person_of(w: &World, s: SenderId) -> u8 {
        *w.people.iter().find(|(_, p)| p.sender == s).unwrap().0
    }

    /// Canonical key: everything the invariants and the oracle can observe,
    /// by person and join position — publish sets, connectivity, declared
    /// counts, per-kind mutes and both slot vectors (occupant position +
    /// handler). Two worlds that differ only in id VALUES are one state.
    fn canonical(w: &World) -> Vec<u8> {
        let order: Vec<SenderId> = w.table.members.values().map(|m| m.sender).collect();
        let pos = |s: SenderId| u8::try_from(order.iter().position(|x| *x == s).unwrap()).unwrap();
        let mut key = Vec::new();
        for m in w.table.members.values() {
            let p = &w.people[&person_of(w, m.sender)];
            key.push(person_of(w, m.sender));
            key.push(u8::from(p.publish[0]) | u8::from(p.publish[1]) << 1);
            key.push(p.conn);
            key.extend(p.declared);
            key.push(
                u8::from(p.client[0])
                    | u8::from(p.client[1]) << 1
                    | u8::from(p.server[0]) << 2
                    | u8::from(p.server[1]) << 3,
            );
            for kind in MediaKindLabel::ALL {
                key.push(u8::MAX - 3);
                for slot in kind.of_ref(&m.slots) {
                    match slot {
                        None => key.push(u8::MAX - 1),
                        Some(e) => {
                            key.push(pos(e.source));
                            key.push(u8::from(e.handler.as_str() == "mh-1"));
                        }
                    }
                }
            }
            key.push(u8::MAX - 2);
        }
        key
    }

    /// State invariants, render invariants, and EXACT equality with the
    /// oracle (occupants and wire states).
    fn check_world(w: &World, controls: &mut Controls) {
        let t = &w.table;
        for m in t.members.values() {
            for kind in MediaKindLabel::ALL {
                let slots = kind.of_ref(&m.slots);
                assert_eq!(slots.len(), kind.of_ref(&m.slot_ids).len());
                let mut seen = HashSet::new();
                for edge in slots.iter().flatten() {
                    assert_ne!(edge.source, m.sender, "no self in any slot");
                    assert!(
                        seen.insert(edge.source.get()),
                        "no sender twice within a kind"
                    );
                    let publisher = t.member(edge.source).unwrap();
                    if kind == AU {
                        assert!(
                            publisher.publishes.publishes(AU),
                            "audio slots hold audio publishers"
                        );
                    } else if !publisher.publishes.publishes(VI) {
                        controls.non_video_in_video_slot += 1;
                    }
                    // From the WORLD's connectivity bits (mh-0 = bit 0, mh-1 =
                    // bit 1), never the production `shared`.
                    let bit = if edge.handler.as_str() == "mh-1" {
                        2
                    } else {
                        1
                    };
                    let sub_conn = w.people[&person_of(w, m.sender)].conn;
                    let pub_conn = w.people[&person_of(w, edge.source)].conn;
                    assert!(
                        sub_conn & bit != 0 && pub_conn & bit != 0,
                        "every edge handler in both connected sets"
                    );
                }
            }
            // Exact oracle equality, per slot.
            let person = person_of(w, m.sender);
            let om = &w.oracle.members[w.oracle.idx(person)];
            for kind in MediaKindLabel::ALL {
                let got: Vec<Option<u8>> = kind
                    .of_ref(&m.slots)
                    .iter()
                    .map(|s| s.as_ref().map(|e| person_of(w, e.source)))
                    .collect();
                assert_eq!(
                    got,
                    om.slots[kind.index()],
                    "oracle occupants, {kind:?} of {person}"
                );
            }
        }
        assert_properties(t);

        // Render invariants.
        let render = t
            .render(
                handler_set().ids(),
                &SourceMuteView::from_map(mute_map(&w.people)),
            )
            .unwrap();
        let mut ids = HashSet::new();
        let mut rendered = 0usize;
        let mut streams_any: Option<&BTreeSet<SourceStreamPlan>> = None;
        for (handler, policy) in &render.per_handler {
            if let Some(prev) = streams_any {
                assert_eq!(
                    prev, &policy.source_streams,
                    "same source_streams on every handler"
                );
            }
            streams_any = Some(&policy.source_streams);
            let mut keys = HashSet::new();
            for s in &policy.source_streams {
                assert!(
                    keys.insert((s.sender, s.stream_number)),
                    "one entry per (sender, stream)"
                );
                let kind = if s.stream_number == 0 { AU } else { VI };
                assert_eq!(
                    s.transport_mode.mode(),
                    transport_of(kind),
                    "one transport per stream"
                );
                let p = &w.people[&person_of(w, s.sender)];
                assert!(
                    p.publish[kind.index()],
                    "only published streams are declared"
                );
                assert_eq!(
                    s.disposition,
                    SourceDispositionPlan::from_mute(
                        p.server[kind.index()],
                        p.client[kind.index()]
                    )
                );
            }
            let declared_count: usize = w
                .people
                .values()
                .map(|p| p.publish.iter().filter(|b| **b).count())
                .sum();
            assert_eq!(policy.source_streams.len(), declared_count);
            for plan in &policy.egress_streams {
                rendered += 1;
                assert!(ids.insert(plan.egress_stream_id), "unique egress_stream_id");
                assert_eq!(plan.candidate_sources.len(), 1);
                let (kind, ordinal) = crate::media_routing::assignment::egress_kind_and_ordinal(
                    plan.egress_stream_id,
                );
                assert_eq!(
                    plan.stream_number,
                    stream_number_of(kind),
                    "edge kind == slot kind"
                );
                assert!(
                    policy
                        .source_streams
                        .iter()
                        .any(|s| s.sender == plan.candidate_sources[0]
                            && s.stream_number == plan.stream_number),
                    "every edge candidate listed in source_streams"
                );
                let sub = t.member(plan.subscriber).unwrap();
                let edge = kind.of_ref(&sub.slots)[ordinal].as_ref().unwrap();
                assert_eq!(&edge.handler, handler);
                assert_eq!(edge.source, plan.candidate_sources[0]);
                assert_eq!(kind.of_ref(&sub.slot_ids).get(ordinal), Some(&plan.slot_id));
            }
        }
        // From the WORLD's publish flags, never the production `renders`:
        // audio slots always render, video only for a video publisher.
        let expected_rendered: usize = t
            .members
            .values()
            .map(|m| {
                MediaKindLabel::ALL
                    .into_iter()
                    .map(|k| {
                        k.of_ref(&m.slots)
                            .iter()
                            .flatten()
                            .filter(|e| k == AU || w.people[&person_of(w, e.source)].publish[1])
                            .count()
                    })
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(
            rendered, expected_rendered,
            "no edge for NO_VIDEO/FEWER/UNAVAILABLE"
        );

        // Wire states equal the oracle's, slot by slot.
        for (person, view) in views(w) {
            let expected = w.oracle.expected_view(person, &w.people);
            controls.unavailable_seen += view
                .iter()
                .filter(|(s, _)| *s == SlotState::Unavailable)
                .count();
            assert_eq!(view, expected, "oracle states for {person}");
        }
    }

    /// An occupied slot changes only through its occupant's leave, the pair
    /// becoming unreachable, or that subscriber's redeclaration of that kind.
    fn check_transition(before: &World, after: &World, op: Op) {
        for (person, p) in &before.people {
            let Some(now) = after.people.get(person) else {
                continue;
            };
            for kind in MediaKindLabel::ALL {
                let old = before.table.slots_of(p.sender, kind).unwrap();
                let new = after.table.slots_of(now.sender, kind).unwrap();
                for (i, slot) in old.iter().enumerate() {
                    let Some(o) = slot else { continue };
                    let after_slot = new.get(i).cloned().flatten();
                    if after_slot.as_ref().map(|e| e.source) == Some(o.source) {
                        continue;
                    }
                    let occupant = person_of(before, o.source);
                    let allowed = match op {
                        Op::Leave(x) => x == occupant,
                        Op::Declare(x, k, _) => x == *person && k == kind.index(),
                        Op::Connect(x, _) => {
                            (x == *person || x == occupant)
                                && after
                                    .table
                                    .member(p.sender)
                                    .zip(after.table.member(o.source))
                                    .is_none_or(|(a, b)| shared(a, b).is_empty())
                        }
                        _ => false,
                    };
                    assert!(allowed, "{op:?} moved {person}'s {kind:?} slot {i}");
                }
            }
        }
    }

    fn apply(w: &mut World, op: Op, controls: &mut Controls) {
        match op {
            Op::Join(p, set, mask) => {
                let s = sender(w.next_sender);
                w.next_sender += 1; // never recycled
                let publish = PUBLISH_SETS[set];
                let kinds: Vec<MediaKindLabel> = publish
                    .iter()
                    .enumerate()
                    .filter(|(_, published)| **published)
                    .map(|(k, _)| label_of(k))
                    .collect();
                w.table.admit(s, PublishSet::of(&kinds)).unwrap();
                w.people.insert(
                    p,
                    Person {
                        sender: s,
                        publish,
                        conn: mask,
                        declared: [1, 1],
                        client: [false; 2],
                        server: [false; 2],
                    },
                );
                let demand = declaration_of(p, [1, 1]);
                assert!(w
                    .table
                    .set_demand(s, [demand.kept_slot_ids(AU), demand.kept_slot_ids(VI)]));
                assert!(w.table.set_connectivity(s, conn_set(mask)));
                w.oracle.join(p, publish);
                w.oracle.declare(p, 0, 1, controls);
                w.oracle.declare(p, 1, 1, controls);
                w.oracle.connect(p, mask, controls);
            }
            Op::Leave(p) => {
                let s = w.people.remove(&p).unwrap().sender;
                assert!(w.table.remove(s));
                w.oracle.leave(p, controls);
            }
            Op::Declare(p, kind, n) => {
                let person = w.people.get_mut(&p).unwrap();
                person.declared[kind] = n;
                let demand = declaration_of(p, person.declared);
                assert!(w.table.set_demand(
                    person.sender,
                    [demand.kept_slot_ids(AU), demand.kept_slot_ids(VI)]
                ));
                w.oracle.declare(p, kind, usize::from(n), controls);
            }
            Op::ClientMute(p, kind) => {
                let person = w.people.get_mut(&p).unwrap();
                person.client[kind] = !person.client[kind];
            }
            Op::ServerMute(p, kind) => {
                let person = w.people.get_mut(&p).unwrap();
                person.server[kind] = !person.server[kind];
            }
            Op::Connect(p, mask) => {
                let person = w.people.get_mut(&p).unwrap();
                person.conn = mask;
                assert!(w.table.set_connectivity(person.sender, conn_set(mask)));
                w.oracle.connect(p, mask, controls);
            }
        }
    }

    fn run_model_check(
        chooser: EdgeChooser,
        people: u8,
        depth: usize,
        controls: &mut Controls,
    ) -> usize {
        let start = World {
            table: SlotTable::with_chooser(chooser),
            oracle: Oracle::new(CAPS),
            people: BTreeMap::new(),
            next_sender: 1,
        };
        let mut queue = VecDeque::from([(start, 0usize)]);
        let mut visited: HashSet<Vec<u8>> = HashSet::new();
        while let Some((world, d)) = queue.pop_front() {
            if !visited.insert(canonical(&world)) {
                continue;
            }
            check_world(&world, controls);
            let world_views = views(&world);
            if d == depth {
                continue;
            }
            let mut ops = Vec::new();
            for p in 0..people {
                if world.people.contains_key(&p) {
                    ops.push(Op::Leave(p));
                    for kind in 0..2 {
                        for n in 0..=2 {
                            ops.push(Op::Declare(p, kind, n));
                        }
                        ops.push(Op::ClientMute(p, kind));
                        ops.push(Op::ServerMute(p, kind));
                    }
                    for mask in 0..4 {
                        ops.push(Op::Connect(p, mask));
                    }
                } else {
                    for set in 0..PUBLISH_SETS.len() {
                        for mask in [1, 3] {
                            ops.push(Op::Join(p, set, mask));
                        }
                    }
                }
            }
            for op in ops {
                let mut next = world.clone();
                apply(&mut next, op, controls);
                controls.per_op[op.index()] += 1;
                check_transition(&world, &next, op);
                if matches!(op, Op::ClientMute(..) | Op::ServerMute(..))
                    && world_views != views(&next)
                {
                    controls.mute_flip_changed_a_view += 1;
                }
                queue.push_back((next, d + 1));
            }
        }
        visited.len()
    }

    /// The test that proves R-3/R-4 for both kinds: breadth-first over
    /// canonical states — up to 3 people x 2 handlers x connectivity {none,
    /// h0, h1, both} x publish sets {A, V, AV, none} x per-kind declared counts
    /// {0, 1, 2 (over cap)} x per-kind client and server mute flips — to depth
    /// 4 (~150k canonical states per chooser), under the production chooser
    /// AND an adversarial one. Depth 5 is ~10x slower in debug and was cut
    /// (never sampled instead); 4 people run to depth 3. A join carries a one-slot-per-kind declaration and its
    /// connectivity (as a real join carries its declaration), so the
    /// interesting fills are reachable within the depth; the cap keeps the
    /// enumeration fast in debug. Deeper cases are the named tests above. After EVERY step: the state and render invariants, the
    /// transition rule, and exact occupant + wire-state equality with the
    /// independent oracle. Never sampled.
    #[test]
    fn exhaustive_small_scope_model_check() {
        for (people, depth) in [(3u8, 4usize), (4, 3)] {
            for chooser in [colocate as EdgeChooser, last] {
                let started = std::time::Instant::now();
                let mut controls = Controls::default();
                let states = run_model_check(chooser, people, depth, &mut controls);
                println!(
                    "two-kind slot model check ({people} people, depth {depth}): {states} states, \
                     {controls:?}, {:?}",
                    started.elapsed()
                );
                // Positive controls: a generator that explores nothing, or an
                // op set that never exercises a rule, cannot pass.
                assert!(states > 10_000, "explored only {states} states");
                assert!(
                    controls.rule_one_differs > 0,
                    "rule (1) never differed from rule (2)"
                );
                assert!(controls.order_mattered > 0, "kind order never mattered");
                assert!(controls.unavailable_seen > 0, "no over-cap slot was seen");
                assert!(
                    controls.non_video_in_video_slot > 0,
                    "no AVATAR_NO_VIDEO occupant"
                );
                assert!(
                    controls.mute_flip_changed_a_view > 0,
                    "mute flips were vacuous"
                );
                assert!(
                    controls.per_op.iter().all(|n| *n > 0),
                    "an op never ran: {:?}",
                    controls.per_op
                );
            }
        }
    }

    /// R-26, named: with a video cap of 0 every video slot is UNAVAILABLE, no
    /// video edge exists, and audio is identical to a run with no video
    /// declared.
    #[test]
    fn a_video_cap_of_zero_leaves_audio_identical_and_renders_no_video() {
        use proto_gen::dark_tower::signaling::v1::{MediaKind, ReceiveCapability, ReceiveSlot};
        let set = handler_set();
        let build = |video_slots: u32| {
            let mut t = SlotTable::new();
            for s in 1..=3 {
                t.admit_on_publishing(sender(s), kinds_of("AV"), &set, &["mh-0"]);
            }
            let mut slots = vec![ReceiveSlot {
                slot_id: 0,
                media_kind: MediaKind::Audio as i32,
                pinned_sender_id: None,
            }];
            for i in 0..video_slots {
                slots.push(ReceiveSlot {
                    slot_id: 10 + i,
                    media_kind: MediaKind::VideoCamera as i32,
                    pinned_sender_id: None,
                });
            }
            let demand = crate::media_signaling::ReceiveCapabilityDeclaration::parse(
                &ReceiveCapability { slots },
                true,
            )
            .unwrap()
            .truncate([8, 0]);
            t.set_demand(
                sender(1),
                [demand.kept_slot_ids(AU), demand.kept_slot_ids(VI)],
            );
            (t, demand)
        };
        let (with_video, demand) = build(2);
        let (without, _) = build(0);
        assert!(with_video.slots_of(sender(1), VI).unwrap().is_empty());
        let r1 = with_video
            .render(set.ids(), &with_video.quiet_mute())
            .unwrap();
        let r0 = without.render(set.ids(), &without.quiet_mute()).unwrap();
        assert_eq!(r1, r0, "audio (and everything else) identical");
        assert!(r1
            .per_handler
            .values()
            .flat_map(|p| &p.egress_streams)
            .all(|p| p.stream_number == 0));
        let composition = crate::media_signaling::build_stream_assignments(
            sender(1),
            &demand,
            &with_video,
            &set,
            &with_video.quiet_mute(),
            &[],
        )
        .unwrap();
        let states: Vec<i32> = composition
            .assignments
            .assignments
            .iter()
            .map(|a| a.slot_state)
            .collect();
        assert_eq!(
            states,
            vec![
                SlotState::Active as i32,
                SlotState::Unavailable as i32,
                SlotState::Unavailable as i32
            ]
        );
    }
}
