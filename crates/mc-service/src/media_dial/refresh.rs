//! Per-participant `media_dial` refresh schedule and coalesced push reasons
//! (ADR-0039 §5 "Delivery").
//!
//! After EVERY push of a participant's set (join, change, failed report,
//! refresh) its next refresh is due at `now + interval - jitter`, with
//! `interval` = TTL/3 and `jitter` uniform in `[0, interval/10]`
//! ([`MediaDialTiming`]). The jitter is only ever subtracted, so the longest
//! gap between two pushes to one participant is TTL/3 < TTL: a held token is
//! expired only after about three consecutive dropped pushes. It spreads one
//! meeting's refresh work so a cohort of joiners does not re-sign together.
//!
//! Due refreshes and FAILED re-pushes are queued here as PENDING, each with
//! ONE coalesced [`HandlerSetPushReason`], and the meeting actor flushes them
//! under the same per-turn bound as its slot views
//! (`actors::meeting_media::SLOT_VIEW_FLUSH_BATCH`). A change or join push is
//! sent immediately by the actor (ordering against directives naming a new
//! ref) and then calls [`DialRefresh::pushed`], which clears any pending entry
//! for that participant.
//!
//! The jitter RNG is a seedable, NON-cryptographic generator (`StdRng`),
//! entirely separate from the system CSPRNG that draws the signing key: no
//! seam reaches key generation.

use std::collections::{BTreeMap, BTreeSet};

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use tokio::time::Instant;

use super::{HandlerSetPushReason, MediaDialTiming};

/// The per-meeting refresh schedule (meeting-actor state).
#[derive(Debug)]
pub struct DialRefresh {
    /// Participant -> when its next refresh is due.
    next_due: BTreeMap<String, Instant>,
    /// The same entries, ordered by due time (the actor's wake).
    by_due: BTreeSet<(Instant, String)>,
    /// Participants with a push queued for the bounded flush, and why.
    pending: BTreeMap<String, HandlerSetPushReason>,
    rng: StdRng,
}

impl DialRefresh {
    /// A schedule with jitter drawn from `seed` (`Some`, tests) or from a
    /// fresh OS-seeded generator (`None`, production).
    #[must_use]
    pub fn new(seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => rand::make_rng::<StdRng>(),
        };
        Self {
            next_due: BTreeMap::new(),
            by_due: BTreeSet::new(),
            pending: BTreeMap::new(),
            rng,
        }
    }

    /// One participant's gap until its next refresh:
    /// `interval - U[0, max_jitter]`.
    fn gap(&mut self, timing: &MediaDialTiming) -> std::time::Duration {
        let max = u64::try_from(timing.max_jitter().as_millis()).unwrap_or(u64::MAX);
        let jitter = std::time::Duration::from_millis(self.rng.random_range(0..=max));
        timing.refresh_interval().saturating_sub(jitter)
    }

    /// A set was just pushed to `participant` at `now`: clear its pending
    /// entry and schedule its next refresh.
    pub fn pushed(&mut self, participant: &str, now: Instant, timing: &MediaDialTiming) {
        self.pending.remove(participant);
        self.unschedule(participant);
        let due = now + self.gap(timing);
        self.next_due.insert(participant.to_string(), due);
        self.by_due.insert((due, participant.to_string()));
    }

    /// The participant left or disconnected: no further refresh, no pending
    /// push.
    pub fn forget(&mut self, participant: &str) {
        self.pending.remove(participant);
        self.unschedule(participant);
    }

    fn unschedule(&mut self, participant: &str) {
        if let Some(due) = self.next_due.remove(participant) {
            self.by_due.remove(&(due, participant.to_string()));
        }
    }

    /// When the earliest scheduled refresh is due.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        self.by_due.first().map(|(due, _)| *due)
    }

    /// Move every refresh due at or before `now` into the pending queue.
    pub fn collect_due(&mut self, now: Instant) {
        while let Some((due, _)) = self.by_due.first() {
            if *due > now {
                break;
            }
            let Some((_, participant)) = self.by_due.pop_first() else {
                break;
            };
            self.next_due.remove(&participant);
            self.mark(&participant, HandlerSetPushReason::TokenRefresh);
        }
    }

    /// Queue a push for `participant`, coalescing with any queued reason.
    pub fn mark(&mut self, participant: &str, reason: HandlerSetPushReason) {
        self.pending
            .entry(participant.to_string())
            .and_modify(|r| *r = r.coalesce(reason))
            .or_insert(reason);
    }

    /// Whether any push is queued.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take up to `limit` queued pushes (the rest wait for a later turn).
    pub fn take_batch(&mut self, limit: usize) -> Vec<(String, HandlerSetPushReason)> {
        let mut batch = Vec::with_capacity(limit.min(self.pending.len()));
        while batch.len() < limit {
            let Some(entry) = self.pending.pop_first() else {
                break;
            };
            batch.push(entry);
        }
        batch
    }

    /// When `participant`'s next refresh is due. **Test builds only.**
    #[cfg(any(test, feature = "test-seams"))]
    #[must_use]
    pub fn due_of(&self, participant: &str) -> Option<Instant> {
        self.next_due.get(participant).copied()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn timing() -> MediaDialTiming {
        MediaDialTiming::from_ttl_seconds(120)
    }

    fn offsets(seed: u64, n: usize) -> Vec<Duration> {
        let now = Instant::now();
        let mut r = DialRefresh::new(Some(seed));
        (0..n)
            .map(|i| {
                let p = format!("p{i}");
                r.pushed(&p, now, &timing());
                r.due_of(&p).unwrap() - now
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn offsets_are_bounded_jittered_and_reproducible() {
        let t = timing();
        let a = offsets(42, 8);
        for gap in &a {
            assert!(*gap <= t.refresh_interval());
            assert!(*gap >= t.refresh_interval() - t.max_jitter());
            assert!(*gap < t.ttl());
        }
        let mut distinct = a.clone();
        distinct.sort();
        distinct.dedup();
        assert!(distinct.len() > 1, "the jitter is real");
        assert_eq!(a, offsets(42, 8), "same seed, same schedule");
    }

    #[tokio::test(start_paused = true)]
    async fn due_refreshes_queue_with_coalesced_reasons_and_bounded_batches() {
        let t = timing();
        let start = Instant::now();
        let mut r = DialRefresh::new(Some(1));
        for p in ["a", "b", "c"] {
            r.pushed(p, start, &t);
        }
        assert!(!r.has_pending());
        r.collect_due(start + t.refresh_interval() - t.max_jitter() - Duration::from_millis(1));
        assert!(!r.has_pending(), "nothing is due before the earliest bound");

        // A FAILED report lands on b, then everyone falls due: b keeps the
        // higher reason, counted once.
        r.mark("b", HandlerSetPushReason::FailedReport);
        r.collect_due(start + t.refresh_interval());
        assert!(r.next_wake().is_none());
        let first = r.take_batch(2);
        assert_eq!(first.len(), 2);
        let rest = r.take_batch(2);
        assert_eq!(rest.len(), 1);
        let all: BTreeMap<_, _> = first.into_iter().chain(rest).collect();
        assert_eq!(all["a"], HandlerSetPushReason::TokenRefresh);
        assert_eq!(all["b"], HandlerSetPushReason::FailedReport);
        assert_eq!(all["c"], HandlerSetPushReason::TokenRefresh);
    }

    #[tokio::test(start_paused = true)]
    async fn a_push_clears_pending_and_reschedules_and_forget_drops_everything() {
        let t = timing();
        let start = Instant::now();
        let mut r = DialRefresh::new(Some(3));
        r.pushed("a", start, &t);
        r.mark("a", HandlerSetPushReason::FailedReport);
        let later = start + Duration::from_secs(5);
        r.pushed("a", later, &t);
        assert!(!r.has_pending());
        assert!(r.due_of("a").unwrap() >= later + t.refresh_interval() - t.max_jitter());

        r.mark("a", HandlerSetPushReason::TokenRefresh);
        r.forget("a");
        assert!(!r.has_pending());
        assert!(r.next_wake().is_none());
    }
}
