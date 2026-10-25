//! The per-meeting bound on the client-mute roster fan-out (story 3 R-15 fix;
//! ADR-0036 §5 amendment 2026-10-24).
//!
//! Every real client-mute transition is told to every other participant, and
//! one fan-out costs O(roster) sends on the shared meeting actor. The
//! per-connection mute bucket bounds each SENDER, which leaves the meeting's
//! aggregate O(N²) with N uncapped per meeting -- security S-2's objection
//! (2026-09-03). This bounds the AGGREGATE: one per-meeting token per fan-out,
//! and a subject that finds no token waits here, latest-state-wins, until one
//! is earned. The bound delays facts under churn and never drops one: the meeting actor
//! composes each fan-out from the roster AT FLUSH TIME, so a subject toggled
//! many times while pending costs one fan-out carrying its current state.
//!
//! FIFO, never id order: a subject already pending keeps its place, so every
//! pending subject is served within `pending` pops and a few churning
//! participants cannot starve everyone else's facts (security, Gate 1).
//!
//! The queue/membership invariant lives here and nowhere else; the actor only
//! calls [`ClientMuteFanout::enqueue`], [`ClientMuteFanout::remove`],
//! [`ClientMuteFanout::pop_ready`] and [`ClientMuteFanout::next_wake`].

use std::collections::{HashSet, VecDeque};

use tokio::time::Instant;

use crate::client_work_limiter::ClientWorkLimiter;
use crate::webtransport::connection::{MUTE_WORK_BURST, MUTE_WORK_REFILL_INTERVAL_MS};

/// Client-mute fan-outs a meeting may send immediately: DERIVED as two
/// connections' worth of the per-connection `MUTE_WORK_BURST`
/// (`webtransport::connection`), so retuning that bucket moves this one.
///
/// Sized so a realistic room never waits (at today's value, 16 participants
/// muting at the same moment -- the start of a talk -- all fan out at once).
/// A Rust constant rather than a config key, matching `MUTE_WORK_*`: a
/// client-input security cap with no operator-tuning story.
pub const CLIENT_MUTE_FANOUT_BURST: u32 = 2 * MUTE_WORK_BURST;

/// Milliseconds per refilled per-meeting fan-out token: DERIVED as half the
/// per-connection `MUTE_WORK_REFILL_INTERVAL_MS`, i.e. the sustained rate of
/// two connections' mute buckets for the whole meeting -- far above a
/// meeting's human mute rate.
pub const CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS: u64 = MUTE_WORK_REFILL_INTERVAL_MS / 2;

// The per-meeting bucket is never tighter than ONE connection's: a single
// participant's legitimate toggles must never queue behind the meeting bound
// on their own.
const _: () = {
    assert!(CLIENT_MUTE_FANOUT_BURST >= MUTE_WORK_BURST);
    assert!(CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS <= MUTE_WORK_REFILL_INTERVAL_MS);
};

/// The pending subjects and the per-meeting bucket they wait on.
#[derive(Debug)]
pub(crate) struct ClientMuteFanout {
    limiter: ClientWorkLimiter,
    /// Pending subjects in first-pending order.
    queue: VecDeque<String>,
    /// Exactly the members of `queue` (dedupe in O(1)).
    pending: HashSet<String>,
}

impl ClientMuteFanout {
    pub(crate) fn new(now: Instant) -> Self {
        Self::with_bucket(
            now,
            CLIENT_MUTE_FANOUT_BURST,
            CLIENT_MUTE_FANOUT_REFILL_INTERVAL_MS,
        )
    }

    fn with_bucket(now: Instant, burst: u32, refill_interval_ms: u64) -> Self {
        Self {
            limiter: ClientWorkLimiter::new(now.into_std(), burst, refill_interval_ms),
            queue: VecDeque::new(),
            pending: HashSet::new(),
        }
    }

    /// Mark `subject` as owing a fan-out. A subject already pending keeps its
    /// place (latest-state-wins: its eventual fan-out carries current state).
    pub(crate) fn enqueue(&mut self, subject: &str) {
        if self.pending.insert(subject.to_string()) {
            self.queue.push_back(subject.to_string());
        }
    }

    /// Forget `subject` (it left, or a fan-out carrying its current state was
    /// just sent by another path). Spends nothing.
    pub(crate) fn remove(&mut self, subject: &str) {
        if self.pending.remove(subject) {
            self.queue.retain(|s| s != subject);
        }
    }

    /// The next subject to fan out, if one is pending AND a token is available
    /// (the token is spent).
    pub(crate) fn pop_ready(&mut self, now: Instant) -> Option<String> {
        if self.queue.is_empty() || !self.limiter.try_spend(now.into_std()) {
            return None;
        }
        let subject = self.queue.pop_front()?;
        self.pending.remove(&subject);
        Some(subject)
    }

    /// Subjects still waiting.
    pub(crate) fn pending_len(&self) -> usize {
        self.queue.len()
    }

    /// When the actor must wake to resume, or `None` when nothing is pending.
    pub(crate) fn next_wake(&mut self, now: Instant) -> Option<Instant> {
        if self.queue.is_empty() {
            return None;
        }
        Some(Instant::from_std(
            self.limiter.next_token_at(now.into_std()),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn drain(f: &mut ClientMuteFanout, now: Instant) -> Vec<String> {
        std::iter::from_fn(|| f.pop_ready(now)).collect()
    }

    #[test]
    fn subjects_are_served_first_pending_first_and_a_re_toggle_keeps_its_place() {
        let now = Instant::now();
        let mut f = ClientMuteFanout::new(now);
        for s in ["c", "a", "b", "c", "a"] {
            f.enqueue(s);
        }
        assert_eq!(f.pending_len(), 3, "a pending subject is never duplicated");
        assert_eq!(drain(&mut f, now), ["c", "a", "b"], "FIFO, not id order");
        assert_eq!(
            f.next_wake(now),
            None,
            "nothing pending, nothing to wake for"
        );
    }

    #[test]
    fn remove_clears_both_structures_and_spends_nothing() {
        let now = Instant::now();
        let mut f = ClientMuteFanout::with_bucket(now, 1, 1_000);
        f.enqueue("a");
        f.enqueue("b");
        f.remove("a");
        f.remove("never-pending");
        assert_eq!(f.pending_len(), 1);
        assert_eq!(drain(&mut f, now), ["b"], "the one token went to b");
        // Re-enqueue after removal is a fresh entry (membership was cleared too).
        f.enqueue("a");
        assert_eq!(f.pending_len(), 1);
    }

    #[test]
    fn pop_ready_respects_the_bucket_and_next_wake_is_the_next_token() {
        let t0 = Instant::now();
        let mut f = ClientMuteFanout::with_bucket(t0, 2, 100);
        for s in ["a", "b", "c", "d"] {
            f.enqueue(s);
        }
        assert_eq!(drain(&mut f, t0), ["a", "b"], "burst of 2");
        assert_eq!(f.pending_len(), 2);
        let wake = f.next_wake(t0).unwrap();
        assert_eq!(wake, t0 + Duration::from_millis(100));
        assert_eq!(f.pop_ready(wake - Duration::from_millis(1)), None);
        assert_eq!(f.pop_ready(wake).as_deref(), Some("c"));
        assert_eq!(f.next_wake(wake), Some(wake + Duration::from_millis(100)));
    }

    /// The FIFO guarantee under churn: two subjects re-enqueued after every
    /// pop cannot keep a third waiting longer than the queue ahead of it.
    #[test]
    fn churning_subjects_cannot_starve_a_single_toggle() {
        let t0 = Instant::now();
        let mut f = ClientMuteFanout::with_bucket(t0, 1, 10);
        f.enqueue("a");
        f.enqueue("b");
        f.enqueue("c");
        let mut now = t0;
        let mut served = Vec::new();
        for _ in 0..3 {
            let s = f.pop_ready(now).unwrap();
            served.push(s.clone());
            if s != "c" {
                f.enqueue(&s); // the churner toggles again at once
            }
            now = f.next_wake(now).unwrap();
        }
        assert!(
            served.contains(&"c".to_string()),
            "c is served within |pending| = 3 pops, got {served:?}"
        );
    }
}
