//! Owner-silence release (story 4 MH-3; ADR-0039 §6, user ruling 2026-10-31;
//! R-11).
//!
//! MH releases a meeting when its OWNING MC — the `mc_id` the meeting's record
//! holds — has not communicated for `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS`
//! (default 900 s, startup-validated to [300, 3600]). The release is exactly a
//! RELEASED verdict's (`HANDLER_RELEASED`, no Disconnected), counted under the
//! MH-local trigger `owner_silence`.
//!
//! # What this is, and what it is NOT
//!
//! **Capacity reclamation, not authorization.** `mc_id` is self-asserted (any
//! holder of MH's service scope can claim any id), so nothing here decides who
//! may act on a meeting; the takeover rule (a record-valid `RegisterMeeting`
//! from another `mc_id` makes it the owner) is unchanged and is the only way
//! ownership moves. **Not an MC-failover mechanism**: it exists because MC ids
//! are per process, so a RESTARTED MC answers its predecessor's meetings'
//! heartbeats with owner-mismatch forever; those meetings are already lost (MC
//! failure is out of scope), and this only gives their capacity back.
//!
//! # Why it is safe
//!
//! MC's N is at most 30 s, so its heartbeat interval is at most 10 s and the
//! 300 s floor spans at least 30 intervals (const-asserted in `crate::config`).
//! An MC that cannot reach MH for that long has declared it dead and moved
//! off; an MH that stops heartbeating is declared dead by a live MC. Draining
//! never self-fences on MC silence: this is the ONLY silence rule MH has.
//!
//! # The reset allow-list
//!
//! [`OwnerSilence::note_owner_contact`] is the ONE reset point. The session
//! actor calls it from exactly two sites, each AFTER validation passed:
//!
//! 1. a `RegisterMeeting` whose record upsert succeeded (including a takeover
//!    and a record-valid registration whose POLICY was refused — the record
//!    validation passed and it is the owner communicating);
//! 2. an OWNED verdict whose beat was built for the meeting's CURRENT
//!    registration (fence match), i.e. answered by the registered MC.
//!
//! An owner `EndMeeting` and a fence-matching RELEASED verdict end the meeting,
//! and [`OwnerSilence::forget`] removes its entry with it. Transport errors,
//! timeouts, `FAILED_PRECONDITION`, `UNSPECIFIED`/unknown verdicts, stale
//! verdicts, a non-owner `EndMeeting` and record-invalid registrations never
//! reach either site.
//!
//! # Clock
//!
//! `tokio::time::Instant` only (monotonic; paused-time tests drive it). Never
//! `SystemTime`.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use tokio::time::Instant;

/// The most releases the actor performs per timer wake, for the same fairness
/// reason as `heartbeat::MAX_BEATS_PER_WAKE`: further expired meetings stay
/// expired and the arm is ready again at once.
pub const MAX_RELEASES_PER_WAKE: usize = 64;

/// One expired meeting: who owned it and how long it was silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expired {
    /// The released meeting.
    pub meeting_id: String,
    /// Time since the owner was last heard (monotonic).
    pub silence: Duration,
}

/// One meeting's last owner contact and its queue key.
#[derive(Debug)]
struct Heard {
    at: Instant,
    key: (Instant, u64),
}

/// Per-meeting owner-silence timers with LAZY deadlines: a reset only
/// overwrites the meeting's `at` (O(1)); the queue entry is re-armed when it
/// comes due early. Each held meeting has exactly one queue entry, removed
/// with the meeting ([`Self::forget`]), so the queue never outgrows the held
/// set.
#[derive(Debug)]
pub struct OwnerSilence {
    timeout: Duration,
    heard: HashMap<String, Heard>,
    queue: BTreeMap<(Instant, u64), String>,
    seq: u64,
}

impl OwnerSilence {
    /// Timers that expire after `timeout` of owner silence.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            heard: HashMap::new(),
            queue: BTreeMap::new(),
            seq: 0,
        }
    }

    /// THE one reset point: the meeting's owning MC communicated at `now`.
    /// Starts the timer for a meeting not yet tracked.
    pub fn note_owner_contact(&mut self, meeting_id: &str, now: Instant) {
        if let Some(heard) = self.heard.get_mut(meeting_id) {
            heard.at = now;
            return;
        }
        let key = (self.timeout_after(now), self.next_seq());
        self.queue.insert(key, meeting_id.to_owned());
        self.heard
            .insert(meeting_id.to_owned(), Heard { at: now, key });
    }

    /// The meeting is no longer held: drop its timer.
    pub fn forget(&mut self, meeting_id: &str) {
        if let Some(heard) = self.heard.remove(meeting_id) {
            self.queue.remove(&heard.key);
        }
    }

    /// The earliest instant the actor must wake at, if any meeting is
    /// tracked. May be earlier than a true expiry (a reset since); the wake
    /// then re-arms that entry and expires nothing.
    #[must_use]
    pub fn earliest(&self) -> Option<Instant> {
        self.queue.keys().next().map(|(at, _)| *at)
    }

    /// Take up to `max` meetings whose owner has been silent for the whole
    /// timeout as of `now`, removing their timers. Entries that came due but
    /// were reset since are re-armed at `last contact + timeout`.
    pub fn take_expired(&mut self, now: Instant, max: usize) -> Vec<Expired> {
        let mut expired = Vec::new();
        while expired.len() < max {
            let Some((&key, _)) = self.queue.first_key_value() else {
                break;
            };
            if key.0 > now {
                break;
            }
            let Some(meeting_id) = self.queue.remove(&key) else {
                break;
            };
            let Some(at) = self.heard.get(&meeting_id).map(|heard| heard.at) else {
                continue;
            };
            let due = self.timeout_after(at);
            if due <= now {
                self.heard.remove(&meeting_id);
                expired.push(Expired {
                    meeting_id,
                    silence: now.saturating_duration_since(at),
                });
            } else {
                // Reset since this entry was queued: re-arm one timeout after
                // the last contact.
                let rearmed = (due, self.next_seq());
                if let Some(heard) = self.heard.get_mut(&meeting_id) {
                    heard.key = rearmed;
                }
                self.queue.insert(rearmed, meeting_id);
            }
        }
        expired
    }

    /// How many meetings are tracked (tests).
    #[must_use]
    pub fn len(&self) -> usize {
        self.heard.len()
    }

    /// Whether no meeting is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heard.is_empty()
    }

    fn timeout_after(&self, at: Instant) -> Instant {
        // The actor's one saturating add: on overflow the deadline moves far
        // out (fails CLOSED: the meeting is kept), never to `at`.
        super::heartbeat::checked_after(at, self.timeout)
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_secs(300);

    #[test]
    fn a_tracked_meeting_expires_exactly_at_the_timeout() {
        let t0 = Instant::now();
        let mut s = OwnerSilence::new(T);
        s.note_owner_contact("m", t0);
        assert_eq!(s.earliest(), Some(t0 + T));
        let just_before = t0 + T - Duration::from_millis(1);
        assert_eq!(
            s.take_expired(just_before, MAX_RELEASES_PER_WAKE),
            Vec::new()
        );
        assert_eq!(
            s.take_expired(t0 + T, MAX_RELEASES_PER_WAKE),
            vec![Expired {
                meeting_id: "m".into(),
                silence: T
            }]
        );
        assert!(s.is_empty() && s.earliest().is_none());
    }

    #[test]
    fn a_reset_rearms_lazily_and_expires_one_timeout_after_the_last_contact() {
        let t0 = Instant::now();
        let mut s = OwnerSilence::new(T);
        s.note_owner_contact("m", t0);
        let reset = t0 + Duration::from_secs(200);
        s.note_owner_contact("m", reset);
        // The queue head still says t0 + T (lazy) ...
        assert_eq!(s.earliest(), Some(t0 + T));
        // ... and that wake expires nothing, re-arming at reset + T.
        assert_eq!(s.take_expired(t0 + T, MAX_RELEASES_PER_WAKE), Vec::new());
        assert_eq!(s.earliest(), Some(reset + T));
        assert_eq!(s.len(), 1);
        let fired = s.take_expired(reset + T, MAX_RELEASES_PER_WAKE);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].silence, T);
    }

    #[test]
    fn forget_removes_the_timer_and_a_recreated_meeting_starts_fresh() {
        let t0 = Instant::now();
        let mut s = OwnerSilence::new(T);
        s.note_owner_contact("m", t0);
        s.forget("m");
        assert!(s.is_empty() && s.earliest().is_none());
        // Re-created later: one entry, timed from the new contact only.
        let later = t0 + Duration::from_secs(100);
        s.note_owner_contact("m", later);
        assert_eq!(s.take_expired(t0 + T, MAX_RELEASES_PER_WAKE), Vec::new());
        assert_eq!(s.earliest(), Some(later + T));
        assert_eq!(s.take_expired(later + T, MAX_RELEASES_PER_WAKE).len(), 1);
        // Forgetting an unknown meeting is a no-op.
        s.forget("never");
    }

    #[test]
    fn expiries_are_bounded_per_wake() {
        let t0 = Instant::now();
        let mut s = OwnerSilence::new(T);
        for i in 0..(MAX_RELEASES_PER_WAKE + 3) {
            s.note_owner_contact(&format!("m-{i}"), t0);
        }
        assert_eq!(
            s.take_expired(t0 + T, MAX_RELEASES_PER_WAKE).len(),
            MAX_RELEASES_PER_WAKE
        );
        assert_eq!(s.take_expired(t0 + T, MAX_RELEASES_PER_WAKE).len(), 3);
        assert!(s.is_empty());
    }
}
