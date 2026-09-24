//! Per-meeting static join-order slot state (ADR-0036 §6, §9; story 2 R-2, R-3,
//! R-4, R-33).
//!
//! # Why state, and not a pure recompute
//!
//! R-4 slot stability is history-dependent: when a middle sender leaves, a
//! from-scratch "earliest joiners fill the slots" recompute would shift every
//! later sender down one slot, and every subscriber would see unrelated slots
//! change. So the meeting actor holds a [`SlotTable`] — one per meeting, owned
//! by the actor, dropped with it — and mutates it with three rules:
//!
//! - **admit** — the joiner takes the FIRST empty slot of each existing
//!   co-handler subscriber; nothing else moves. The joiner starts with zero
//!   slots: slot demand is client-declared, and it has not declared yet.
//! - **set_demand** — a subscriber's declared audio slots size its slot vector.
//!   Ordinal *i* is the *i*-th AUDIO slot in declaration order. Same count: the
//!   occupants keep their ordinals and only the slot ids change. Growing adds
//!   empty slots, then refills. Shrinking truncates the tail; senders in the
//!   truncated slots become unassigned for that subscriber and are NOT placed
//!   elsewhere, except that any empty slot that remains is refilled
//!   earliest-first like every other refill.
//! - **remove** — for each subscriber, only the leaver's slot is freed, and it
//!   is refilled with the earliest-joined currently-unassigned eligible sender,
//!   if any. Every other slot is untouched.
//!
//! A reconnect changes nothing here: the participant never left the roster, so
//! it keeps its rank and its slots. A fresh rejoin is a new participant with a
//! new `sender_id` and a new rank.
//!
//! # The invariants every mutation preserves
//!
//! 1. No subscriber holds itself (R-3: loopback is removed).
//! 2. No subscriber holds one sender twice.
//! 3. A subscriber holds only co-handler senders (§9 visibility).
//! 4. No subscriber has an empty slot while an eligible sender it does not hold
//!    exists.
//! 5. A subscriber's slot vector is exactly as long as its declared audio slots.
//!
//! They are asserted after every step of an exhaustive small-scope model check
//! in this module's tests, which is the test that actually proves R-4.
//!
//! # Rank is not `sender_id`
//!
//! [`JoinRank`] is a per-meeting monotonic counter and the ONLY ordering key
//! here. [`SenderId`] is deliberately not `Ord` (nothing may infer join order
//! from an allocator handle), and it still comes only from
//! `SenderIdAllocator::allocate()`. A rank is internal: it never reaches the
//! wire, a metric label, a log field or a span attribute.
//!
//! # No key material
//!
//! Sender ids, slot ids and handler ids only. Nothing here reads, holds or
//! derives a KEK, transmit key or identity key.

use super::assignment::{
    egress_stream_id, AssignmentError, EgressStreamPlan, HandlerAssignment, HandlerId,
    MeetingAssignment, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
};
use crate::media_admission::SenderId;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::{BTreeMap, HashMap};

/// A participant's per-meeting join-order position.
///
/// Monotonic and never reused within a meeting. Internal only: never on the
/// wire, never a metric label, never a log field or span attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JoinRank(u64);

impl JoinRank {
    /// The numeric value, for placement arithmetic.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Wrap a raw rank. Test fixtures only; production ranks come from
    /// [`SlotTable::admit`].
    #[cfg(test)]
    #[must_use]
    pub(crate) fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

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
    /// Placement named no handler for the joiner.
    NoPlacement,
}

impl std::fmt::Display for SlotTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAdmitted => f.write_str("participant is already in the slot table"),
            Self::RankSpaceExhausted => f.write_str("join-order rank space exhausted"),
            Self::NoPlacement => f.write_str("no media handler could be placed for the joiner"),
        }
    }
}

impl std::error::Error for SlotTableError {}

/// One roster participant, as slot state sees them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Member {
    sender: SenderId,
    /// The one handler this participant is placed on.
    handler: HandlerId,
    /// Declared audio slot ids, in declaration order. `slots[i]` fills
    /// `slot_ids[i]`; the two always have equal length.
    slot_ids: Vec<u16>,
    /// Who fills each declared slot, if anyone.
    slots: Vec<Option<SenderId>>,
}

impl Member {
    fn holds(&self, sender: SenderId) -> bool {
        self.slots.contains(&Some(sender))
    }
}

/// Does `subscriber` receive `publisher`'s media?
///
/// ADR-0036 §9's visibility rule — they share a handler — made **non-reflexive**
/// (story 2 R-3: loopback is removed; a participant never subscribes to itself,
/// including when alone). This is the ONE eligibility rule: admit, set_demand,
/// remove and the unreachable set all call it, so the invariant cannot drift
/// between sites.
fn subscribes_to(subscriber: &Member, publisher: &Member) -> bool {
    subscriber.sender != publisher.sender && subscriber.handler == publisher.handler
}

/// The meeting's join-order slot state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotTable {
    /// Every roster participant, in join order.
    members: BTreeMap<JoinRank, Member>,
    /// Reverse index. `SenderId` is not `Ord`, so this is a hash map; ordering
    /// always comes from `members`.
    rank_of: HashMap<SenderId, JoinRank>,
    /// The next rank to issue.
    next_rank: u64,
}

impl SlotTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit a participant, placing it with `place`, and give it the first empty
    /// slot of every existing co-handler subscriber. Nothing else moves.
    ///
    /// Returns the joiner's rank and the handler it was placed on.
    ///
    /// # Errors
    ///
    /// [`SlotTableError`] if the sender is already a member, the rank space is
    /// exhausted, or `place` names no handler. Nothing is mutated on error.
    pub fn admit<F>(
        &mut self,
        sender: SenderId,
        place: F,
    ) -> Result<(JoinRank, HandlerId), SlotTableError>
    where
        F: FnOnce(JoinRank) -> Option<HandlerId>,
    {
        if self.rank_of.contains_key(&sender) {
            return Err(SlotTableError::AlreadyAdmitted);
        }
        let rank = JoinRank(self.next_rank);
        let next = self
            .next_rank
            .checked_add(1)
            .ok_or(SlotTableError::RankSpaceExhausted)?;
        let handler = place(rank).ok_or(SlotTableError::NoPlacement)?;

        self.next_rank = next;
        self.members.insert(
            rank,
            Member {
                sender,
                handler: handler.clone(),
                slot_ids: Vec::new(),
                slots: Vec::new(),
            },
        );
        self.rank_of.insert(sender, rank);

        // Existing subscribers only; the joiner has no slots yet.
        let others: Vec<JoinRank> = self
            .members
            .keys()
            .copied()
            .filter(|r| *r != rank)
            .collect();
        for subscriber in others {
            self.refill(subscriber);
        }
        Ok((rank, handler))
    }

    /// Remove a participant. For every other subscriber only the leaver's slot
    /// is freed, then refilled earliest-first. Returns `false` if the sender was
    /// not a member.
    pub fn remove(&mut self, sender: SenderId) -> bool {
        let Some(rank) = self.rank_of.remove(&sender) else {
            return false;
        };
        self.members.remove(&rank);

        let ranks: Vec<JoinRank> = self.members.keys().copied().collect();
        for subscriber in ranks {
            if let Some(member) = self.members.get_mut(&subscriber) {
                for slot in &mut member.slots {
                    if *slot == Some(sender) {
                        *slot = None;
                    }
                }
            }
            self.refill(subscriber);
        }
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
        self.refill(rank);
        true
    }

    /// Fill a subscriber's empty slots, first empty first, with the
    /// earliest-joined eligible senders it does not already hold.
    fn refill(&mut self, subscriber: JoinRank) {
        loop {
            let Some(member) = self.members.get(&subscriber) else {
                return;
            };
            let Some(empty) = member.slots.iter().position(Option::is_none) else {
                return;
            };
            let candidate = self
                .members
                .values()
                .find(|publisher| {
                    subscribes_to(member, publisher) && !member.holds(publisher.sender)
                })
                .map(|publisher| publisher.sender);
            let Some(candidate) = candidate else {
                return;
            };
            let Some(member) = self.members.get_mut(&subscriber) else {
                return;
            };
            let Some(slot) = member.slots.get_mut(empty) else {
                return;
            };
            *slot = Some(candidate);
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

    /// The handler the sender is placed on.
    #[must_use]
    pub fn handler_of(&self, sender: SenderId) -> Option<&HandlerId> {
        self.member(sender).map(|m| &m.handler)
    }

    /// Who fills each of the sender's declared slots, in ordinal order.
    #[must_use]
    pub fn slots_of(&self, sender: SenderId) -> Option<&[Option<SenderId>]> {
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

    /// The roster participants `subscriber` cannot reach: those placed on a
    /// different handler (§9; story 2 R-33), sorted numerically.
    ///
    /// Membership follows `signaling.proto`'s rule exactly. It EXCLUDES the
    /// subscriber itself, and it excludes EVERY co-handler sender, including
    /// one left unassigned only because the subscriber's slots are full — that
    /// is "no slot", not "unreachable".
    ///
    /// "Only participants that HAVE a `sender_id`" is structural, not a filter:
    /// a participant enters this table only through [`Self::admit`], which takes
    /// a `SenderId`, so a roster entry without one cannot be named here. This
    /// reads `members` alone — the actor's own meeting — and has no path to any
    /// cross-meeting or global registry. It carries no handler identity.
    #[must_use]
    pub fn unreachable_for(&self, subscriber: SenderId) -> Vec<SenderId> {
        let Some(me) = self.member(subscriber) else {
            return Vec::new();
        };
        let mut unreachable: Vec<SenderId> = self
            .members
            .values()
            .filter(|other| other.sender != me.sender && !subscribes_to(me, other))
            .map(|other| other.sender)
            .collect();
        unreachable.sort_by_key(|s| s.get());
        unreachable
    }

    /// Render the slot state into the MC→MH forwarding assignment.
    ///
    /// One [`EgressStreamPlan`] per FILLED slot, with exactly one pinned
    /// candidate, placed on the subscriber's handler (which the pinned source
    /// shares, by invariant 3). An unfilled slot emits NO egress stream: MH
    /// would otherwise install a sourceless edge that still consumes its
    /// per-meeting and aggregate edge bounds. "Fewer sources than slots" is a
    /// client-wire state only.
    ///
    /// `egress_stream_id` packs the slot's ordinal in the subscriber's slot
    /// vector — never an enumeration over filled slots — so an unchanged slot
    /// keeps its id across a middle leave and a refill keeps the slot's id.
    ///
    /// Every handler in `handlers` gets an entry, possibly empty, so the first
    /// push registers every assigned handler with MH.
    ///
    /// # Errors
    ///
    /// [`AssignmentError::EgressOrdinalOverflow`] if an ordinal does not fit
    /// the 8 bits `egress_stream_id` allots it.
    pub fn render<'a>(
        &self,
        handlers: impl IntoIterator<Item = &'a HandlerId>,
    ) -> Result<MeetingAssignment, AssignmentError> {
        let mut per_handler: BTreeMap<HandlerId, HandlerAssignment> = handlers
            .into_iter()
            .map(|h| (h.clone(), HandlerAssignment::default()))
            .collect();

        for member in self.members.values() {
            for (ordinal, (slot, slot_id)) in member.slots.iter().zip(&member.slot_ids).enumerate()
            {
                let Some(source) = slot else {
                    continue;
                };
                let plan = EgressStreamPlan {
                    egress_stream_id: egress_stream_id(member.sender, ordinal)?,
                    subscriber: member.sender,
                    slot_id: *slot_id,
                    candidate_sources: vec![*source],
                    stream_number: MAIN_AUDIO_STREAM_NUMBER,
                    priority_group: AUDIO_PRIORITY_GROUP,
                    // False for audio: every frame forwards. MH is told the
                    // behaviour, never the media type.
                    supersede_on_independent_frame: false,
                    // ADR-0036 §1: audio rides datagrams. The signalling enum,
                    // imported — there is deliberately no parallel `internal.v1`
                    // spelling, so MC and MH cannot disagree about mode while
                    // §8's echo reads green.
                    transport_mode: TransportMode::Datagram,
                };
                per_handler
                    .entry(member.handler.clone())
                    .or_default()
                    .egress_streams
                    .push(plan);
            }
        }

        for assignment in per_handler.values_mut() {
            assignment
                .egress_streams
                .sort_by_key(|s| s.egress_stream_id);
        }
        Ok(MeetingAssignment { per_handler })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn h(name: &str) -> HandlerId {
        HandlerId::new(name)
    }

    /// Round-robin over `["mh-0", "mh-1"][..n]`, the production rule's shape.
    fn round_robin(n: u64) -> impl Fn(JoinRank) -> Option<HandlerId> {
        move |rank| Some(h(if rank.get() % n == 0 { "mh-0" } else { "mh-1" }))
    }

    fn one_handler(_: JoinRank) -> Option<HandlerId> {
        Some(h("mh-0"))
    }

    fn filled(table: &SlotTable, s: u16) -> Vec<Option<u16>> {
        table
            .slots_of(sender(s))
            .unwrap()
            .iter()
            .map(|slot| slot.map(|x| x.get().get()))
            .collect()
    }

    fn admit(table: &mut SlotTable, s: u16, place: impl FnOnce(JoinRank) -> Option<HandlerId>) {
        table.admit(sender(s), place).unwrap();
    }

    // ---------------------------------------------------------------- named

    #[test]
    fn a_joiner_fills_existing_subscribers_first_empty_slot_and_its_own_slots_in_join_order() {
        let mut t = SlotTable::new();
        admit(&mut t, 1, one_handler);
        t.set_demand(sender(1), vec![0, 1]);
        assert_eq!(filled(&t, 1), vec![None, None], "solo: nobody to hear");

        admit(&mut t, 2, one_handler);
        assert_eq!(filled(&t, 1), vec![Some(2), None]);
        admit(&mut t, 3, one_handler);
        assert_eq!(filled(&t, 1), vec![Some(2), Some(3)]);

        // The joiner's own vector fills from the earliest-joined others.
        admit(&mut t, 4, one_handler);
        t.set_demand(sender(4), vec![0, 1]);
        assert_eq!(filled(&t, 4), vec![Some(1), Some(2)]);
    }

    /// R-4: A,B,C,D; B leaves. Only B's slot changes, refilled by the earliest
    /// unassigned sender.
    #[test]
    fn a_middle_leave_changes_only_the_leavers_slot() {
        let mut t = SlotTable::new();
        for s in 1..=5 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(filled(&t, 1), vec![Some(2), Some(3), Some(4)]);

        t.remove(sender(3));
        assert_eq!(
            filled(&t, 1),
            vec![Some(2), Some(5), Some(4)],
            "slot 0 and slot 2 untouched; the freed slot refilled with the earliest unassigned"
        );
    }

    #[test]
    fn the_n_plus_second_sender_is_unassigned_and_not_unreachable() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(1), vec![0, 1]);
        assert_eq!(filled(&t, 1), vec![Some(2), Some(3)]);
        assert!(!t.slots_of(sender(1)).unwrap().contains(&Some(sender(4))));
        assert!(
            t.unreachable_for(sender(1)).is_empty(),
            "a co-handler sender with no slot is 'no slot', not 'unreachable'"
        );
    }

    #[test]
    fn slot_counts_are_per_subscriber() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(1), vec![0]);
        t.set_demand(sender(2), vec![5, 6, 7]);
        assert_eq!(filled(&t, 1), vec![Some(2)]);
        assert_eq!(filled(&t, 2), vec![Some(1), Some(3), Some(4)]);
    }

    /// The pinned rule: shrink truncates the tail; the truncated sender becomes
    /// unassigned for this subscriber. Grow refills earliest-first. Same count
    /// keeps occupants and changes only ids.
    #[test]
    fn redeclare_shrink_grow_and_renumber() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(filled(&t, 1), vec![Some(2), Some(3), Some(4)]);

        t.set_demand(sender(1), vec![0, 1]);
        assert_eq!(
            filled(&t, 1),
            vec![Some(2), Some(3)],
            "sender 4 truncated, not re-placed"
        );

        t.set_demand(sender(1), vec![0, 1, 2]);
        assert_eq!(
            filled(&t, 1),
            vec![Some(2), Some(3), Some(4)],
            "grow refills earliest-first"
        );

        t.set_demand(sender(1), vec![9, 8, 7]);
        assert_eq!(
            filled(&t, 1),
            vec![Some(2), Some(3), Some(4)],
            "renumber keeps occupants"
        );
        let render = t.render([&h("mh-0")]).unwrap();
        let ids: Vec<u16> = render.per_handler[&h("mh-0")]
            .egress_streams
            .iter()
            .filter(|p| p.subscriber == sender(1))
            .map(|p| p.slot_id)
            .collect();
        assert_eq!(
            ids,
            vec![9, 8, 7],
            "the declared ids ride the wire, in ordinal order"
        );
    }

    /// A fresh rejoin is a new sender with a new, later rank, and loses its
    /// place. (A RECONNECT never reaches this table at all — the participant
    /// never left the roster — so there is nothing to test here; the reconnect
    /// proof is actor-level: `slot_placement_integration.rs::
    /// a_reconnect_in_grace_keeps_slots_and_re_sends_only_the_reconnectors_view`.)
    #[test]
    fn a_fresh_rejoin_gets_a_new_rank_and_loses_its_place() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(4), vec![0]);
        assert_eq!(filled(&t, 4), vec![Some(1)]);
        assert_eq!(t.rank_of(sender(1)), Some(JoinRank(0)));

        // Fresh rejoin of participant 1: a NEW sender id (never recycled), a
        // new, later rank, and it loses its place.
        let mut rejoined = t.clone();
        rejoined.remove(sender(1));
        rejoined.admit(sender(5), one_handler).unwrap();
        assert_eq!(rejoined.rank_of(sender(5)), Some(JoinRank(4)));
        assert_eq!(
            filled(&rejoined, 4),
            vec![Some(2)],
            "4's slot refilled by the earliest remaining"
        );
        assert_ne!(filled(&t, 4), filled(&rejoined, 4));
    }

    /// R-3: a solo participant hears nothing. All its slots are unfilled (the
    /// client wire says FEWER_SOURCES_THAN_SLOTS) and MH carries NO egress
    /// stream for it — not merely "no self candidate".
    #[test]
    fn a_solo_participant_hears_nothing() {
        let mut t = SlotTable::new();
        admit(&mut t, 7, one_handler);
        t.set_demand(sender(7), vec![0, 1]);
        assert_eq!(filled(&t, 7), vec![None, None]);
        let render = t.render([&h("mh-0")]).unwrap();
        assert!(render.per_handler[&h("mh-0")].egress_streams.is_empty());
    }

    #[test]
    fn visibility_is_non_reflexive_and_per_handler() {
        let a = Member {
            sender: sender(1),
            handler: h("mh-0"),
            slot_ids: vec![],
            slots: vec![],
        };
        let b = Member {
            sender: sender(2),
            handler: h("mh-0"),
            slot_ids: vec![],
            slots: vec![],
        };
        let c = Member {
            sender: sender(3),
            handler: h("mh-1"),
            slot_ids: vec![],
            slots: vec![],
        };
        assert!(!subscribes_to(&a, &a), "never yourself, even alone");
        assert!(subscribes_to(&a, &b) && subscribes_to(&b, &a));
        assert!(!subscribes_to(&a, &c) && !subscribes_to(&c, &a));
    }

    #[test]
    fn a_split_meeting_fills_per_handler_and_reports_the_rest_unreachable() {
        let mut t = SlotTable::new();
        for s in 1..=5 {
            admit(&mut t, s, round_robin(2));
        }
        // Ranks 0,2,4 -> mh-0 (1,3,5); ranks 1,3 -> mh-1 (2,4).
        for s in 1..=5 {
            t.set_demand(sender(s), vec![0, 1, 2]);
        }
        assert_eq!(filled(&t, 1), vec![Some(3), Some(5), None]);
        assert_eq!(filled(&t, 2), vec![Some(4), None, None]);
        let as_u16 = |v: Vec<SenderId>| v.into_iter().map(|s| s.get().get()).collect::<Vec<_>>();
        assert_eq!(as_u16(t.unreachable_for(sender(1))), vec![2, 4]);
        assert_eq!(as_u16(t.unreachable_for(sender(2))), vec![1, 3, 5]);

        // Each handler's snapshot carries only its own participants' edges.
        let render = t.render([&h("mh-0"), &h("mh-1")]).unwrap();
        for (handler, policy) in &render.per_handler {
            for plan in &policy.egress_streams {
                assert_eq!(t.handler_of(plan.subscriber), Some(handler));
                assert_eq!(t.handler_of(plan.candidate_sources[0]), Some(handler));
            }
        }
    }

    #[test]
    fn egress_stream_id_is_stable_across_a_middle_leave() {
        let mut t = SlotTable::new();
        for s in 1..=4 {
            admit(&mut t, s, one_handler);
        }
        t.set_demand(sender(1), vec![0, 1, 2]);
        let id_of = |t: &SlotTable, src: u16| {
            t.render([&h("mh-0")]).unwrap().per_handler[&h("mh-0")]
                .egress_streams
                .iter()
                .find(|p| p.subscriber == sender(1) && p.candidate_sources == vec![sender(src)])
                .map(|p| p.egress_stream_id)
        };
        let before = id_of(&t, 4);
        t.remove(sender(3));
        assert_eq!(
            id_of(&t, 4),
            before,
            "sender 4's stream keeps its id when 3 leaves"
        );
    }

    #[test]
    fn admission_errors_leave_the_table_untouched() {
        let mut t = SlotTable::new();
        admit(&mut t, 1, one_handler);
        let before = t.clone();
        assert_eq!(
            t.admit(sender(1), one_handler),
            Err(SlotTableError::AlreadyAdmitted)
        );
        assert_eq!(
            t.admit(sender(2), |_| None),
            Err(SlotTableError::NoPlacement)
        );
        assert_eq!(t, before);
    }

    // ------------------------------------------------- exhaustive model check

    /// Canonical state: slots expressed as positions in the join-ordered member
    /// list rather than as sender ids or ranks, plus the parity that decides the
    /// next placement. Two tables that differ only in id/rank VALUES are the
    /// same state for every invariant, which is what keeps the search small.
    fn canonical(t: &SlotTable, people: &HashMap<SenderId, u8>) -> Vec<u8> {
        let order: Vec<SenderId> = t.members.values().map(|m| m.sender).collect();
        let pos = |s: SenderId| {
            order
                .iter()
                .position(|x| *x == s)
                .map_or(u8::MAX, |p| p as u8)
        };
        let mut key = vec![(t.next_rank % 2) as u8];
        for m in t.members.values() {
            key.push(people[&m.sender]);
            key.push(u8::from(m.handler == h("mh-1")));
            key.push(m.slots.len() as u8);
            for slot in &m.slots {
                key.push(slot.map_or(u8::MAX - 1, pos));
            }
            key.push(u8::MAX - 2);
        }
        key
    }

    fn check_state(t: &SlotTable) {
        let members: Vec<&Member> = t.members.values().collect();
        for m in &members {
            assert_eq!(m.slots.len(), m.slot_ids.len(), "invariant 5");
            let mut seen = HashSet::new();
            for s in m.slots.iter().flatten() {
                assert_ne!(*s, m.sender, "invariant 1: no self");
                assert!(seen.insert(s.get()), "invariant 2: no duplicate");
                let publisher = t.member(*s).expect("filled with a member");
                assert!(subscribes_to(m, publisher), "invariant 3: co-handler only");
            }
            if m.slots.contains(&None) {
                for other in &members {
                    if subscribes_to(m, other) {
                        assert!(
                            m.holds(other.sender),
                            "invariant 4: empty slot beside an unassigned eligible sender"
                        );
                    }
                }
            }
            let unreachable: HashSet<u16> = t
                .unreachable_for(m.sender)
                .iter()
                .map(|s| s.get().get())
                .collect();
            let expected: HashSet<u16> = members
                .iter()
                .filter(|o| o.handler != m.handler)
                .map(|o| o.sender.get().get())
                .collect();
            assert_eq!(
                unreachable, expected,
                "unreachable = exactly the cross-handler roster"
            );
        }

        // Render invariants.
        let render = t.render([&h("mh-0"), &h("mh-1")]).unwrap();
        // Every handler has an entry, even one nobody is placed on and even in
        // an empty meeting (it is registered empty so MH can promote later).
        assert_eq!(
            render.per_handler.keys().cloned().collect::<Vec<_>>(),
            vec![h("mh-0"), h("mh-1")],
            "one entry per handler, always"
        );
        let mut ids = HashSet::new();
        let mut streams = 0usize;
        for (handler, policy) in &render.per_handler {
            // Canonical order: a structurally unchanged table renders
            // byte-identically, so its generation does not move.
            assert!(
                policy
                    .egress_streams
                    .windows(2)
                    .all(|w| w[0].egress_stream_id < w[1].egress_stream_id),
                "egress streams strictly ascending by egress_stream_id"
            );
            for plan in &policy.egress_streams {
                streams += 1;
                assert!(ids.insert(plan.egress_stream_id), "unique egress_stream_id");
                assert_eq!(
                    plan.candidate_sources.len(),
                    1,
                    "exactly one pinned candidate"
                );
                let sub = t.member(plan.subscriber).unwrap();
                assert_eq!(
                    &sub.handler, handler,
                    "edge on the subscriber's own handler"
                );
                let ordinal =
                    crate::media_routing::assignment::egress_ordinal(plan.egress_stream_id);
                assert_eq!(
                    sub.slots.get(ordinal),
                    Some(&Some(plan.candidate_sources[0]))
                );
                assert_eq!(sub.slot_ids.get(ordinal), Some(&plan.slot_id));
            }
        }
        let filled: usize = members
            .iter()
            .map(|m| m.slots.iter().flatten().count())
            .sum();
        assert_eq!(
            streams, filled,
            "one stream per FILLED slot, none for an unfilled one"
        );
    }

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Join(u8),
        Leave(u8),
        Declare(u8, usize),
    }

    fn check_transition(
        before: &SlotTable,
        after: &SlotTable,
        op: Op,
        people: &HashMap<SenderId, u8>,
        by_person: &HashMap<u8, SenderId>,
    ) {
        let actor = match op {
            Op::Join(p) | Op::Leave(p) | Op::Declare(p, _) => p,
        };
        let joiner = after
            .members
            .values()
            .find(|m| before.member(m.sender).is_none())
            .map(|m| m.sender);
        let left = if let Op::Leave(p) = op {
            by_person.get(&p).copied()
        } else {
            None
        };
        for m in before.members.values() {
            if people.get(&m.sender) == Some(&actor) {
                continue; // the redeclaring / leaving participant's own vector
            }
            let Some(now) = after.member(m.sender) else {
                continue;
            };
            let mut changed = 0;
            for (i, slot) in m.slots.iter().enumerate() {
                let new = now.slots.get(i).copied().flatten();
                if *slot == new {
                    continue;
                }
                changed += 1;
                match op {
                    Op::Leave(_) => {
                        assert_eq!(*slot, left, "on leave only the leaver's slot changes");
                        // Refill: the EARLIEST-rank eligible sender this
                        // subscriber did not already hold, or nobody.
                        let earliest = after
                            .members
                            .values()
                            .filter(|o| o.sender != now.sender)
                            .filter(|o| subscribes_to(now, o) && !m.holds(o.sender))
                            .min_by_key(|o| after.rank_of(o.sender))
                            .map(|o| o.sender);
                        assert_eq!(new, earliest, "leave refills with the earliest unassigned");
                    }
                    Op::Join(_) => {
                        assert!(slot.is_none(), "on join only an empty slot changes");
                        assert_eq!(
                            m.slots.iter().position(Option::is_none),
                            Some(i),
                            "on join the FIRST empty slot fills"
                        );
                        assert_eq!(new, joiner, "and it fills with the joiner");
                    }
                    _ => panic!(
                        "another participant's demand moved slot {i} of an unrelated subscriber"
                    ),
                }
            }
            if let Op::Join(_) = op {
                assert!(changed <= 1, "on join at most the first empty slot changes");
            }
        }
    }

    /// The test that actually proves R-4. Breadth-first over canonical states,
    /// 4 people x 2 handlers x slot counts {1,2,3}, join / leave / fresh rejoin
    /// (leave then join) / redeclare, to depth 6, asserting every
    /// state invariant, every render invariant and every transition rule.
    #[test]
    fn exhaustive_small_scope_model_check() {
        const PEOPLE: u8 = 4;
        const DEPTH: usize = 6;
        let started = std::time::Instant::now();

        #[derive(Clone)]
        struct World {
            table: SlotTable,
            present: HashMap<u8, SenderId>,
            next_sender: u16,
        }

        let start = World {
            table: SlotTable::new(),
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
                    for n in 1..=3 {
                        ops.push(Op::Declare(p, n));
                    }
                } else {
                    ops.push(Op::Join(p));
                }
            }
            for op in ops {
                let mut next = world.clone();
                match op {
                    Op::Join(p) => {
                        let s = sender(next.next_sender);
                        next.next_sender += 1; // never recycled
                        next.table.admit(s, round_robin(2)).unwrap();
                        next.present.insert(p, s);
                    }
                    Op::Leave(p) => {
                        let s = next.present.remove(&p).unwrap();
                        assert!(next.table.remove(s));
                    }
                    Op::Declare(p, n) => {
                        let s = next.present[&p];
                        let ids: Vec<u16> = (0..n as u16).map(|i| i * 3 + u16::from(p)).collect();
                        assert!(next.table.set_demand(s, ids));
                    }
                }
                transitions += 1;
                check_transition(&world.table, &next.table, op, &people, &world.present);
                queue.push_back((next, depth + 1));
            }
        }

        let states = visited.len();
        println!(
            "slot model check: {states} canonical states, {transitions} transitions, {:?}",
            started.elapsed()
        );
        // Positive control: a generator bug that explores nothing cannot pass.
        assert!(states > 1_000, "explored only {states} states");
    }
}
