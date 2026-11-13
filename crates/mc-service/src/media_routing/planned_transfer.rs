//! Planned transfer off a DRAINING handler (ADR-0039 §4/§7; story 4 MC-F) —
//! the PURE half.
//!
//! When a member's heartbeat first reports DRAINING, the meeting moves off it
//! make-before-break: the acquisition goes through MC-C's ONE path
//! (`ReplacementState::on_draining`), the draining handler stays in the set
//! and keeps serving, and it is removed through the ONE removal transaction
//! only once every routable participant is MH-reported Connected to an
//! eligible target, or at the cutover deadline.
//!
//! This module holds the decisions with no I/O and no timers inside:
//! - THE cutover instant ([`cutover_at`]): receipt of a draining heartbeat +
//!   `drain_remaining_ms` (clamped to a config-derived ceiling) − margin,
//!   saturating; later heartbeats only ever TIGHTEN it;
//! - the per-handler bookkeeping ([`PlannedTransfers`]): which handlers are
//!   being transferred, their deadlines, their grant, and whether survivors
//!   are eligible yet;
//! - THE eligibility rule ([`PlannedTransfers::eligible`]).
//!
//! # Target rule (Gate 1, paired-test Issue 1; lead ruling 2)
//!
//! Before the deadline, the target is the planned GRANT only (once Active),
//! until the acquisition answers "no grant" (no capacity, GC unavailable or
//! not registered, refused, or the attempt bound); from then on the
//! non-draining Active survivors are eligible too (and stay so). At the
//! deadline, the target is every non-draining Active member other than the
//! draining one, whatever the acquisition state; the meeting is shed only if
//! that is empty.
//!
//! # Counting (observability ruling, lead ruling 1)
//!
//! Each transfer records EXACTLY ONE terminal outcome — `completed`,
//! `timed_out` or `abandoned` — when its entry leaves the map
//! ([`PlannedTransfers::finish`] hands the entry out by value, once). Failed
//! acquisition attempts count per attempt as in MC-C; a planned grant reaching
//! Active records nothing ([`super::replacement::Granted::records_on_active`]).
//!
//! # No identifiers in labels, no key material anywhere
//!
//! Ids reach logs only. Nothing here holds a token, a key or a URL.

use super::HandlerId;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::time::Instant;

/// The planned-transfer cutover margin (`MC_PLANNED_TRANSFER_MARGIN_MS`),
/// validated at config load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedTransferTiming {
    margin: Duration,
}

impl PlannedTransferTiming {
    /// Build from the validated config value.
    #[must_use]
    pub fn from_margin_ms(margin_ms: u32) -> Self {
        Self {
            margin: Duration::from_millis(u64::from(margin_ms)),
        }
    }

    /// The margin MC cuts over ahead of MH's drain deadline.
    #[must_use]
    pub fn margin(self) -> Duration {
        self.margin
    }
}

/// THE cutover instant for one draining heartbeat (security S-1, operations
/// O1/O2, media-handler 1):
///
/// `receipt + min(drain_remaining, max_hold) − margin`, saturating at
/// `receipt`. `receipt` is the heartbeat's gRPC-arrival stamp (so mailbox
/// delay never moves the deadline later); `drain_remaining_ms` is relative to
/// MH building the beat and is untrusted input — clamped to `max_hold`
/// ([`super::ReplacementTiming::max_planned_hold`], config-derived) so no value
/// can hold a meeting on a draining handler indefinitely. A remaining at or
/// below the margin (late registration, delayed beat, 0) means an immediate
/// cutover — never an underflow or panic.
#[must_use]
pub fn cutover_at(
    receipt: Instant,
    drain_remaining_ms: u32,
    timing: PlannedTransferTiming,
    max_hold: Duration,
) -> Instant {
    let remaining = Duration::from_millis(u64::from(drain_remaining_ms)).min(max_hold);
    receipt + remaining.saturating_sub(timing.margin)
}

/// One open planned transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    /// Receipt of the heartbeat that first reported DRAINING.
    pub started_at: Instant,
    /// The cutover deadline (only ever tightened).
    pub cutover_at: Instant,
    /// The planned grant for this transfer, once GC granted one.
    pub grant: Option<HandlerId>,
    /// The acquisition answered "no grant": survivors are eligible (sticky).
    pub survivors_eligible: bool,
}

/// What [`PlannedTransfers::start`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// A new transfer, with its cutover deadline.
    New(Instant),
    /// Already transferring: the deadline was tightened (or kept).
    Existing,
}

/// Every open planned transfer of a meeting, keyed by the draining handler.
#[derive(Debug, Default)]
pub struct PlannedTransfers {
    open: BTreeMap<HandlerId, Transfer>,
}

impl PlannedTransfers {
    /// Whether no transfer is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Whether `id` is being transferred.
    #[must_use]
    pub fn contains(&self, id: &HandlerId) -> bool {
        self.open.contains_key(id)
    }

    /// The open transfers' handlers, in id order.
    pub fn ids(&self) -> impl Iterator<Item = &HandlerId> {
        self.open.keys()
    }

    /// One open transfer.
    #[must_use]
    pub fn get(&self, id: &HandlerId) -> Option<&Transfer> {
        self.open.get(id)
    }

    /// A DRAINING heartbeat from `id` was accepted with `cutover`
    /// ([`cutover_at`] of it). The first starts the transfer; every later
    /// one only TIGHTENS the deadline (`min`), never extends it (security
    /// S-1b), and never starts a second transfer (operations O5).
    pub fn start(&mut self, id: &HandlerId, receipt: Instant, cutover: Instant) -> Started {
        if let Some(open) = self.open.get_mut(id) {
            open.cutover_at = open.cutover_at.min(cutover);
            return Started::Existing;
        }
        self.open.insert(
            id.clone(),
            Transfer {
                started_at: receipt,
                cutover_at: cutover,
                grant: None,
                survivors_eligible: false,
            },
        );
        Started::New(cutover)
    }

    /// GC granted `grant` for `replaced`'s transfer (ignored if that transfer
    /// is no longer open: the grant is then only a size restore).
    pub fn granted(&mut self, replaced: &HandlerId, grant: &HandlerId) {
        if let Some(open) = self.open.get_mut(replaced) {
            open.grant = Some(grant.clone());
        }
    }

    /// The acquisition answered "no grant". GC capacity is not per transfer,
    /// so every open transfer's survivors become eligible (sticky).
    pub fn no_grant(&mut self) {
        for open in self.open.values_mut() {
            open.survivors_eligible = true;
        }
    }

    /// `removed` left the meeting: no transfer keeps it as its grant (a later
    /// grant may take its place).
    pub fn grant_removed(&mut self, removed: &HandlerId) {
        for open in self.open.values_mut() {
            if open.grant.as_ref() == Some(removed) {
                open.grant = None;
            }
        }
    }

    /// THE eligibility rule for `id`'s transfer (see the module doc).
    /// `candidates` are the meeting's non-draining Active members (one
    /// filter, `MeetingMedia::non_draining_active`); `at_deadline` selects the
    /// deadline rule. Returns the eligible targets (never `id` itself).
    #[must_use]
    pub fn eligible(
        &self,
        id: &HandlerId,
        candidates: &[HandlerId],
        at_deadline: bool,
    ) -> Vec<HandlerId> {
        let Some(open) = self.open.get(id) else {
            return Vec::new();
        };
        candidates
            .iter()
            .filter(|c| *c != id)
            .filter(|c| at_deadline || open.survivors_eligible || open.grant.as_ref() == Some(*c))
            .cloned()
            .collect()
    }

    /// The earliest open deadline (the actor's cutover wake).
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        self.open.values().map(|t| t.cutover_at).min()
    }

    /// The handlers whose deadline is at or before `now` (still open; the
    /// caller [`Self::finish`]es each, so an elapsed deadline never re-fires).
    #[must_use]
    pub fn due(&self, now: Instant) -> Vec<HandlerId> {
        self.open
            .iter()
            .filter(|(_, t)| t.cutover_at <= now)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Take `id`'s transfer out — THE single exit, so a terminal outcome can
    /// be recorded at most once per transfer (by whoever got the entry).
    pub fn finish(&mut self, id: &HandlerId) -> Option<Transfer> {
        self.open.remove(id)
    }

    /// The meeting is ending: every open transfer (each ends `abandoned`).
    pub fn end(&mut self) -> Vec<(HandlerId, Transfer)> {
        std::mem::take(&mut self.open).into_iter().collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn hid(s: &str) -> HandlerId {
        HandlerId::new(s)
    }

    fn margin_5s() -> PlannedTransferTiming {
        PlannedTransferTiming::from_margin_ms(5_000)
    }

    const HOLD: Duration = Duration::from_secs(139);

    #[test]
    fn cutover_is_receipt_plus_remaining_minus_margin() {
        let t0 = Instant::now();
        assert_eq!(
            cutover_at(t0, 30_000, margin_5s(), HOLD),
            t0 + Duration::from_secs(25)
        );
    }

    /// S-1a / O2 / MH-1: remaining at or below the margin (incl. 0) is an
    /// immediate cutover at receipt — no underflow, no panic.
    #[test]
    fn remaining_at_or_below_the_margin_cuts_over_at_receipt() {
        let t0 = Instant::now();
        for remaining in [0, 1, 4_999, 5_000] {
            assert_eq!(
                cutover_at(t0, remaining, margin_5s(), HOLD),
                t0,
                "remaining={remaining}"
            );
        }
        assert_eq!(
            cutover_at(t0, 5_001, margin_5s(), HOLD),
            t0 + Duration::from_millis(1),
            "positive arm: one past the margin waits"
        );
    }

    /// S-1c: the largest value MH can send still yields a bounded hold.
    #[test]
    fn remaining_is_clamped_to_the_max_hold() {
        let t0 = Instant::now();
        assert_eq!(
            cutover_at(t0, u32::MAX, margin_5s(), HOLD),
            t0 + HOLD - Duration::from_secs(5)
        );
    }

    /// S-1b / O1 / O5: a later beat tightens, never extends, and never starts
    /// a second transfer.
    #[test]
    fn later_beats_only_tighten_and_never_restart() {
        let t0 = Instant::now();
        let mut t = PlannedTransfers::default();
        let first = t0 + Duration::from_secs(25);
        assert_eq!(t.start(&hid("x"), t0, first), Started::New(first));
        assert_eq!(
            t.start(&hid("x"), t0, first + Duration::from_secs(60)),
            Started::Existing
        );
        assert_eq!(t.next_wake(), Some(first), "never extended");
        let earlier = first - Duration::from_secs(3);
        assert_eq!(t.start(&hid("x"), t0, earlier), Started::Existing);
        assert_eq!(t.next_wake(), Some(earlier), "tightened");
        assert_eq!(t.get(&hid("x")).unwrap().started_at, t0, "start kept");
    }

    /// The grant is the only target until "no grant"; survivors then join and
    /// stay; at the deadline every candidate is eligible. Never the drained
    /// handler itself.
    #[test]
    fn eligibility_follows_the_grant_then_survivors_then_deadline_rule() {
        let t0 = Instant::now();
        let mut t = PlannedTransfers::default();
        t.start(&hid("x"), t0, t0);
        let candidates = [hid("s"), hid("x"), hid("y")];
        assert!(
            t.eligible(&hid("x"), &candidates, false).is_empty(),
            "no grant yet"
        );
        assert_eq!(
            t.eligible(&hid("x"), &candidates, true),
            vec![hid("s"), hid("y")],
            "deadline: every candidate but x"
        );
        t.granted(&hid("x"), &hid("y"));
        assert_eq!(t.eligible(&hid("x"), &candidates, false), vec![hid("y")]);
        t.grant_removed(&hid("y"));
        assert!(t.eligible(&hid("x"), &candidates, false).is_empty());
        t.no_grant();
        assert_eq!(
            t.eligible(&hid("x"), &candidates, false),
            vec![hid("s"), hid("y")],
            "survivors eligible after no grant"
        );
        t.granted(&hid("x"), &hid("y"));
        assert_eq!(
            t.eligible(&hid("x"), &candidates, false),
            vec![hid("s"), hid("y")],
            "sticky: a later grant only adds"
        );
        assert!(t.eligible(&hid("other"), &candidates, true).is_empty());
    }

    #[test]
    fn a_grant_for_a_closed_transfer_is_ignored() {
        let mut t = PlannedTransfers::default();
        t.granted(&hid("x"), &hid("y"));
        assert!(t.is_empty());
    }

    /// Code-review 6: finishing a due transfer consumes it, so an elapsed
    /// deadline never re-fires; finish hands the entry out once.
    #[test]
    fn finish_consumes_once_and_due_lists_only_open_elapsed() {
        let t0 = Instant::now();
        let mut t = PlannedTransfers::default();
        t.start(&hid("a"), t0, t0 + Duration::from_secs(1));
        t.start(&hid("b"), t0, t0 + Duration::from_secs(9));
        let now = t0 + Duration::from_secs(1);
        assert_eq!(t.due(now), vec![hid("a")]);
        assert!(t.finish(&hid("a")).is_some());
        assert!(t.finish(&hid("a")).is_none(), "at most once");
        assert!(t.due(now).is_empty());
        assert_eq!(t.next_wake(), Some(t0 + Duration::from_secs(9)));
        assert_eq!(t.end().len(), 1);
        assert!(t.is_empty() && t.next_wake().is_none());
    }
}
