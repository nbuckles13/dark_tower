//! The per-meeting MH -> MC heartbeat schedule (story 4 MH-2; ADR-0039 §1).
//!
//! Owned by the session actor, which is the only place a beat is BUILT: a sent
//! beat therefore proves the actor is alive, and a wedged actor sends none.
//! The actor hands each built beat to the sender task
//! (`crate::grpc::heartbeat_sender`) with a bounded `try_send` and never
//! blocks on it.
//!
//! # Schedule rules (`MeetingHeartbeatRequest` DELIVERY)
//!
//! - A meeting's schedule starts at the REGISTRATION UPSERT, not at the first
//!   successful apply, so a meeting whose first apply is refused still reports
//!   `applied_generation` 0 plus its `apply_refusal`.
//! - The first deadline is jittered uniformly in `[0, interval)` (a migration
//!   registers many meetings together). The RNG is injected: seeded from
//!   entropy in production, from an explicit seed in tests.
//! - The interval is the meeting's `RegisterMeetingRequest.heartbeat_interval_ms`;
//!   the latest registration's value applies. MH has NO cadence config.
//! - Release removes the schedule.
//!
//! All times are `tokio::time::Instant`, so paused-time tests drive the
//! cadence, and every addition is checked (no panic at `u32::MAX` ms).

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use proto_gen::dark_tower::internal::v1::{ApplyRefusal, MeetingHeartbeatRequest};
use rand::rngs::SmallRng;
use rand::RngExt;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::routing::RefusalEffect;

/// The most beats the actor builds per timer wake. Any further due meetings
/// stay due, so the arm is immediately ready again and `select!`'s fairness
/// keeps the lifecycle, config-apply and `EndMeeting` arms served even when
/// many meetings (or a 1 ms interval) are due at once.
pub const MAX_BEATS_PER_WAKE: usize = 64;

/// Capacity of the actor -> sender-task queue. A constant, not config (the
/// task adds no config key): several wakes' worth of beats, so a full queue
/// means the sender task is genuinely not keeping up, and the beat is dropped
/// and counted (`dropped_queue_full`) rather than blocking the actor.
pub const HEARTBEAT_QUEUE_CAPACITY: usize = 4 * MAX_BEATS_PER_WAKE;

/// Far enough out to read as "never" for a saturated deadline.
const FAR_FUTURE: Duration = Duration::from_hours(24 * 365);

/// The per-attempt deadline for one beat: three quarters of the meeting's
/// interval, so it is ALWAYS below the interval (ADR-0039 §1: at most one
/// beat per meeting outstanding, arriving in order). Derived, never a
/// constant: MC owns the interval. Duration arithmetic, so a 1 ms interval
/// gives 750 µs and nothing underflows; `u32::MAX` ms cannot overflow.
#[must_use]
pub fn per_attempt_deadline(interval: Duration) -> Duration {
    interval.saturating_mul(3) / 4
}

/// One built beat, on its way to the sender task.
#[derive(Debug, Clone)]
pub struct OutboundBeat {
    /// The request, built from the actor's own state.
    pub request: MeetingHeartbeatRequest,
    /// The meeting's REGISTERED MC endpoint. The only destination; nothing is
    /// ever taken from a response.
    pub endpoint: String,
    /// The registration this beat was built for. A RELEASED verdict is acted
    /// on only while the meeting's current fence still equals this.
    pub fence: u64,
    /// The meeting's interval, from which the per-attempt deadline derives.
    pub interval: Duration,
}

/// What the actor sends the sender task.
#[derive(Debug, Clone)]
pub enum SenderCommand {
    /// Send this beat (unless its meeting's previous beat is still in
    /// flight).
    Beat(OutboundBeat),
    /// No registration references this MC endpoint any more: drop its cached
    /// channel. Best-effort (a full queue drops it); the sender's cache cap is
    /// the backstop.
    Evict {
        /// The endpoint to forget.
        endpoint: String,
    },
    /// The meeting was released: forget its failure-streak state.
    /// Best-effort, with the same cap backstop.
    Forget {
        /// The released meeting.
        meeting_id: String,
    },
}

/// What the actor needs to heartbeat: who it is, where beats go, the jitter
/// RNG, and the two MC-liveness inputs story 4 MH-3 adds (the drain state the
/// beats report, and the owner-silence timeout OWNED verdicts reset).
#[derive(Debug)]
pub struct HeartbeatSetup {
    /// This handler's id, as it registers with GC.
    pub handler_id: String,
    /// The bounded queue to the sender task.
    pub commands: mpsc::Sender<SenderCommand>,
    /// Jitter source. Production: `rand::make_rng()` (entropy-seeded). Tests:
    /// `SmallRng::seed_from_u64`.
    pub rng: SmallRng,
    /// The process drain state (`crate::drain`): beats carry DRAINING and
    /// `drain_remaining_ms` while it holds a deadline.
    pub drain: crate::drain::DrainWatch,
    /// `MH_MEETING_OWNER_SILENCE_TIMEOUT_SECONDS` (`Config` validates it to
    /// [300, 3600] s; tests use a real in-range value under paused time).
    pub owner_silence_timeout: Duration,
}

/// One meeting's schedule entry.
#[derive(Debug)]
struct Entry {
    interval: Duration,
    due: (Instant, u64),
    fence: u64,
    refusal: ApplyRefusal,
}

/// The per-meeting due instants, earliest first.
///
/// A `BTreeMap` keyed by `(due, seq)` plus a per-meeting index: the earliest
/// due is O(log n) at the registered-meeting cap, where a min-scan per wake
/// would be O(n). `seq` disambiguates equal instants.
#[derive(Debug)]
pub struct HeartbeatSchedule {
    by_due: BTreeMap<(Instant, u64), String>,
    entries: HashMap<String, Entry>,
    seq: u64,
    /// Next registration fence. A plain field: the actor is the single owner.
    /// Never reused within the actor's life.
    next_fence: u64,
    rng: SmallRng,
}

impl HeartbeatSchedule {
    /// An empty schedule using `rng` for first-deadline jitter.
    #[must_use]
    pub fn new(rng: SmallRng) -> Self {
        Self {
            by_due: BTreeMap::new(),
            entries: HashMap::new(),
            seq: 0,
            next_fence: 1,
            rng,
        }
    }

    /// Whether any meeting is scheduled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The earliest due instant, if any meeting is scheduled.
    #[must_use]
    pub fn earliest(&self) -> Option<Instant> {
        self.by_due.keys().next().map(|(due, _)| *due)
    }

    /// The meeting's current registration fence.
    #[must_use]
    pub fn fence_of(&self, meeting_id: &str) -> Option<u64> {
        self.entries.get(meeting_id).map(|e| e.fence)
    }

    /// The meeting's latest deterministic apply refusal (`UNSPECIFIED` =
    /// none).
    #[must_use]
    pub fn refusal_of(&self, meeting_id: &str) -> ApplyRefusal {
        self.entries
            .get(meeting_id)
            .map_or(ApplyRefusal::Unspecified, |e| e.refusal)
    }

    /// The meeting's interval.
    #[must_use]
    pub fn interval_of(&self, meeting_id: &str) -> Option<Duration> {
        self.entries.get(meeting_id).map(|e| e.interval)
    }

    /// Upsert at the registration upsert.
    ///
    /// - New meeting: first due jittered in `[0, interval)`, a fresh fence.
    /// - Existing meeting: the latest interval applies, and the next due is
    ///   `min(existing due, now + interval)`, so a SHORTENED interval takes
    ///   effect without waiting out the old one. The fence changes only when
    ///   `identity_changed` (a different `mc_id` or endpoint), so a verdict
    ///   in flight from the OLD owner can never end the new registration.
    ///   The refusal is kept: it describes the installed snapshot, which a
    ///   re-registration does not change by itself.
    pub fn upsert(
        &mut self,
        meeting_id: &str,
        interval: Duration,
        now: Instant,
        identity_changed: bool,
    ) {
        let seq = self.next_seq();
        if let Some(entry) = self.entries.get_mut(meeting_id) {
            entry.interval = interval;
            let candidate = checked_after(now, interval);
            if candidate < entry.due.0 {
                self.by_due.remove(&entry.due);
                entry.due = (candidate, seq);
                self.by_due.insert(entry.due, meeting_id.to_owned());
            }
            if identity_changed {
                entry.fence = self.next_fence;
                self.next_fence = self.next_fence.saturating_add(1);
            }
            return;
        }
        let jitter = jitter_below(&mut self.rng, interval);
        let due = (checked_after(now, jitter), seq);
        let fence = self.next_fence;
        self.next_fence = self.next_fence.saturating_add(1);
        self.by_due.insert(due, meeting_id.to_owned());
        self.entries.insert(
            meeting_id.to_owned(),
            Entry {
                interval,
                due,
                fence,
                refusal: ApplyRefusal::Unspecified,
            },
        );
    }

    /// Make the meeting's next beat due at `now` (a meeting registered while
    /// draining: its MC learns at once, unjittered). No-op for an unknown
    /// meeting; never pushes a due instant LATER.
    pub fn due_now(&mut self, meeting_id: &str, now: Instant) {
        let seq = self.next_seq();
        if let Some(entry) = self.entries.get_mut(meeting_id) {
            if now < entry.due.0 {
                self.by_due.remove(&entry.due);
                entry.due = (now, seq);
                self.by_due.insert(entry.due, meeting_id.to_owned());
            }
        }
    }

    /// Make every meeting's next beat due at `now` (ADR-0039 §1: "DRAINING
    /// resets every meeting's deadline to now"). The cadence then resumes one
    /// interval after `now` (`take_due`).
    pub fn due_all_now(&mut self, now: Instant) {
        let meetings: Vec<String> = self.entries.keys().cloned().collect();
        for meeting_id in meetings {
            self.due_now(&meeting_id, now);
        }
    }

    /// Remove the meeting's schedule (release).
    pub fn remove(&mut self, meeting_id: &str) -> bool {
        match self.entries.remove(meeting_id) {
            Some(entry) => {
                self.by_due.remove(&entry.due);
                true
            }
            None => false,
        }
    }

    /// Apply one apply attempt's effect on the meeting's `apply_refusal`.
    pub fn apply_refusal_effect(&mut self, meeting_id: &str, effect: RefusalEffect) {
        let Some(entry) = self.entries.get_mut(meeting_id) else {
            return;
        };
        match effect {
            RefusalEffect::Set(refusal) => entry.refusal = refusal,
            RefusalEffect::Clear => entry.refusal = ApplyRefusal::Unspecified,
            RefusalEffect::Unchanged => {}
        }
    }

    /// Take up to `max` meetings due at or before `now`, rescheduling each one
    /// interval after its previous due (exact cadence), or one interval after
    /// `now` if the actor fell more than an interval behind (no burst).
    pub fn take_due(&mut self, now: Instant, max: usize) -> Vec<String> {
        let mut due_meetings = Vec::new();
        while due_meetings.len() < max {
            let Some((&key, _)) = self.by_due.first_key_value() else {
                break;
            };
            if key.0 > now {
                break;
            }
            let Some(meeting_id) = self.by_due.remove(&key) else {
                break;
            };
            let seq = self.next_seq();
            if let Some(entry) = self.entries.get_mut(&meeting_id) {
                let mut next = checked_after(key.0, entry.interval);
                if next <= now {
                    next = checked_after(now, entry.interval);
                }
                entry.due = (next, seq);
                self.by_due.insert(entry.due, meeting_id.clone());
            }
            due_meetings.push(meeting_id);
        }
        due_meetings
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        seq
    }
}

/// `now + by`, saturating to a far-future instant instead of panicking. The
/// ONE saturating deadline add of the session actor's timers (the heartbeat
/// schedule and the owner-silence timers): on overflow a deadline moves FAR
/// out (a year), so no timer fails open. Only if `now` is itself within a
/// year of the clock's maximum does the last arm fall back to `now`.
pub(super) fn checked_after(now: Instant, by: Duration) -> Instant {
    now.checked_add(by)
        .or_else(|| now.checked_add(FAR_FUTURE))
        .unwrap_or(now)
}

/// Uniform in `[0, interval)`; zero for a zero interval (unreachable: a
/// 0 interval is a record reject).
fn jitter_below(rng: &mut SmallRng, interval: Duration) -> Duration {
    let nanos = u64::try_from(interval.as_nanos()).unwrap_or(u64::MAX);
    if nanos == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos(rng.random_range(0..nanos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn schedule() -> HeartbeatSchedule {
        HeartbeatSchedule::new(SmallRng::seed_from_u64(7))
    }

    #[test]
    fn the_deadline_is_always_below_the_interval_and_never_panics() {
        for ms in [1_u64, 2, 3, 1000, 2000, 10_000, u64::from(u32::MAX)] {
            let interval = Duration::from_millis(ms);
            let deadline = per_attempt_deadline(interval);
            assert!(deadline < interval, "{ms} ms -> {deadline:?}");
            assert!(deadline > Duration::ZERO, "{ms} ms");
        }
        assert_eq!(
            per_attempt_deadline(Duration::from_millis(1)),
            Duration::from_micros(750)
        );
    }

    #[test]
    fn a_huge_interval_saturates_instead_of_panicking() {
        let mut s = schedule();
        let now = Instant::now();
        s.upsert("m", Duration::from_millis(u64::from(u32::MAX)), now, false);
        assert!(s.earliest().is_some());
        let _ = s.take_due(now, MAX_BEATS_PER_WAKE);
    }

    #[test]
    fn first_due_is_jittered_inside_one_interval() {
        let interval = Duration::from_secs(2);
        let now = Instant::now();
        let mut s = schedule();
        for i in 0..50 {
            s.upsert(&format!("m-{i}"), interval, now, false);
        }
        let mut distinct = std::collections::HashSet::new();
        for e in s.entries.values() {
            assert!(e.due.0 >= now && e.due.0 < now + interval);
            distinct.insert(e.due.0);
        }
        assert!(distinct.len() > 1, "jitter spreads first deadlines");
    }

    #[test]
    fn take_due_is_bounded_per_wake_and_reschedules_one_interval_later() {
        let interval = Duration::from_secs(1);
        let now = Instant::now();
        let mut s = schedule();
        for i in 0..(MAX_BEATS_PER_WAKE + 10) {
            s.upsert(&format!("m-{i}"), interval, now, false);
        }
        let later = now + interval;
        assert_eq!(
            s.take_due(later, MAX_BEATS_PER_WAKE).len(),
            MAX_BEATS_PER_WAKE
        );
        assert_eq!(s.take_due(later, MAX_BEATS_PER_WAKE).len(), 10);
        assert_eq!(s.take_due(later, MAX_BEATS_PER_WAKE), Vec::<String>::new());
        assert!(s.earliest().unwrap() > later);
    }

    #[test]
    fn the_fence_changes_only_on_identity_change_and_never_repeats() {
        let now = Instant::now();
        let mut s = schedule();
        s.upsert("m", Duration::from_secs(1), now, false);
        let first = s.fence_of("m").unwrap();
        s.upsert("m", Duration::from_secs(1), now, false);
        assert_eq!(s.fence_of("m"), Some(first), "same owner keeps its fence");
        s.upsert("m", Duration::from_secs(1), now, true);
        let second = s.fence_of("m").unwrap();
        assert_ne!(second, first);
        assert!(s.remove("m"));
        s.upsert("m", Duration::from_secs(1), now, false);
        let third = s.fence_of("m").unwrap();
        assert!(
            third != first && third != second,
            "a re-create gets a fresh fence"
        );
    }

    #[test]
    fn a_shortened_interval_pulls_the_next_due_in() {
        let now = Instant::now();
        let mut s = schedule();
        s.upsert("m", Duration::from_secs(10), now, false);
        // Whatever the jitter, after this beat the next due is past t+0.5 s.
        let t = now + Duration::from_secs(15);
        assert_eq!(s.take_due(t, 1).len(), 1);
        let short = Duration::from_millis(500);
        assert!(s.earliest().unwrap() > t + short);
        s.upsert("m", short, t, false);
        assert_eq!(s.earliest(), Some(t + short));
        assert_eq!(s.interval_of("m"), Some(short));
    }

    #[test]
    fn due_all_now_pulls_every_meeting_in_and_never_pushes_one_later() {
        let now = Instant::now();
        let mut s = schedule();
        for i in 0..5 {
            s.upsert(&format!("m-{i}"), Duration::from_secs(10), now, false);
        }
        let t = now + Duration::from_millis(1);
        s.due_all_now(t);
        let mut due = s.take_due(t, MAX_BEATS_PER_WAKE);
        due.sort();
        assert_eq!(due.len(), 5, "every meeting is due at once");
        // After the burst the cadence resumes one interval later.
        assert_eq!(s.earliest(), Some(t + Duration::from_secs(10)));
        // An instant LATER than a meeting's due never delays it.
        s.due_now("m-0", t + Duration::from_secs(60));
        assert_eq!(s.earliest(), Some(t + Duration::from_secs(10)));
        s.due_now("unknown", t);
    }

    #[test]
    fn refusal_effects_set_clear_and_leave_unchanged() {
        let now = Instant::now();
        let mut s = schedule();
        s.upsert("m", Duration::from_secs(1), now, false);
        s.apply_refusal_effect("m", RefusalEffect::Set(ApplyRefusal::EdgeCap));
        assert_eq!(s.refusal_of("m"), ApplyRefusal::EdgeCap);
        s.apply_refusal_effect("m", RefusalEffect::Unchanged);
        assert_eq!(s.refusal_of("m"), ApplyRefusal::EdgeCap);
        s.apply_refusal_effect("m", RefusalEffect::Clear);
        assert_eq!(s.refusal_of("m"), ApplyRefusal::Unspecified);
    }
}
