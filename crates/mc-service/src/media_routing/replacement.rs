//! Replacement after removal (ADR-0039 §3/§4; story 4 MC-C) — the PURE half.
//!
//! After a removal leaves a meeting below its hand-off size, MC asks GC for
//! ONE replacement at a time (`AcquireMediaHandler`); a grant enters the set
//! as Registering and goes through the SAME Registering → Active path as every
//! hand-off handler (push worker, `RegisterMeeting`, then the client set with
//! tokens). Removal never waits on GC. This module holds the timing (THE
//! backoff ladder and the published retry budget), the bound, the metric
//! vocabularies and the per-meeting bookkeeping; the meeting actor's driver
//! (`actors/meeting/replacement.rs`) does the I/O.
//!
//! # Counting unit (observability ruling, story 4 MC-C Gate 1)
//!
//! `mc_media_handler_migrations_total` counts ACQUISITION ATTEMPTS, not
//! migrations: an attempt is ONE `AcquireMediaHandler` call, or ONE owed
//! replacement refused at the per-meeting attempt bound. Each attempt records
//! EXACTLY ONE [`MigrationOutcome`]. A migration that retries contributes one
//! failure outcome per failed attempt and at most one `completed`. A
//! min-interval deferral is NOT an attempt and is never counted. Per-migration
//! completion is `completed / declarations_total{trigger!="planned_transfer"}`.
//!
//! # No identifiers in labels, no key material anywhere
//!
//! Ids reach logs only. Nothing here holds a token, a key or a request.

use super::{HandlerId, RemovalTrigger};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU32;
use std::time::Duration;
use tokio::time::Instant;

/// The removal trigger of a handler whose FIRST `RegisterMeeting` never acked
/// (retries exhausted or refused) — hand-off or grant alike.
///
/// ADR-0039 §3: "the unacked handler takes the normal replacement path". It is
/// `apply_lag` because `ReleaseCause::APPLY_LAG` covers "a load-dependent
/// `ApplyRefusal`": the handler never applied the meeting. This is NOT a
/// health declaration — no declaration-side series is touched on this path.
#[must_use]
pub const fn registration_failure_trigger() -> RemovalTrigger {
    RemovalTrigger::ApplyLag
}

/// Whether a meeting may still ask GC for a replacement.
///
/// The request carries `exclude` (current ∪ removed). It is allowed iff
/// `|exclude| < cap`. DELIBERATELY ONE STRICTER than GC's own wall
/// (`validate_acquire` rejects `len > cap` with INVALID_ARGUMENT): this is the
/// task's "cap − set size" bound (cap 16 with 2 initial handlers = 14
/// lifetime grants), and it guarantees MC never sends an over-cap list. Do not
/// "fix" it to `<=`. Compared in `usize` with no truncating cast.
#[must_use]
pub fn within_attempt_bound(exclude_len: usize, cap: NonZeroU32) -> bool {
    match usize::try_from(cap.get()) {
        Ok(cap) => exclude_len < cap,
        // Unreachable on any supported target; a cap that does not fit is
        // treated as AT the bound (stop), never as unbounded.
        Err(_) => false,
    }
}

/// The replacement timing (`MC_MH_REPLACEMENT_MIN_INTERVAL_MS`,
/// `MC_MH_REPLACEMENT_BACKOFF_MAX_MS`), validated at config load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacementTiming {
    min_interval: Duration,
    backoff_max: Duration,
}

impl ReplacementTiming {
    /// Build from the validated config values (`backoff_max >= min_interval`
    /// is enforced at load; it is clamped here too so the ladder is monotone).
    #[must_use]
    pub fn from_config_ms(min_interval_ms: u32, backoff_max_ms: u32) -> Self {
        let min_interval = Duration::from_millis(u64::from(min_interval_ms));
        let backoff_max = Duration::from_millis(u64::from(backoff_max_ms)).max(min_interval);
        Self {
            min_interval,
            backoff_max,
        }
    }

    /// The per-meeting minimum interval between Acquire attempts (also the
    /// ladder's first step).
    #[must_use]
    pub fn min_interval(self) -> Duration {
        self.min_interval
    }

    /// The ladder's cap.
    #[must_use]
    pub fn backoff_max(self) -> Duration {
        self.backoff_max
    }

    /// THE ladder: the un-jittered ceiling of the wait after `failures`
    /// consecutive failed attempts (`failures >= 1`): `min_interval · 2^(failures−1)`,
    /// capped at `backoff_max`. Both the scheduler ([`Self::retry_delay`]) and
    /// the published budget ([`Self::retry_budget`]) read it — no parallel
    /// arithmetic.
    #[must_use]
    pub fn ladder_step(self, failures: u32) -> Duration {
        let doublings = failures.saturating_sub(1).min(31);
        self.min_interval
            .checked_mul(1u32 << doublings)
            .map_or(self.backoff_max, |d| d.min(self.backoff_max))
    }

    /// The ladder up to AND including its first capped step.
    fn ladder_to_cap(self) -> Vec<Duration> {
        let mut steps = Vec::new();
        for failures in 1u32.. {
            let step = self.ladder_step(failures);
            steps.push(step);
            if step >= self.backoff_max || failures >= 64 {
                break;
            }
        }
        steps
    }

    /// The jittered wait after `failures` consecutive failures: earlier-only
    /// jitter into `[step/2, step]`, never below the minimum interval.
    #[must_use]
    pub fn retry_delay(self, failures: u32, rng: &mut StdRng) -> Duration {
        let step = self.ladder_step(failures);
        let low = (step / 2).max(self.min_interval).min(step);
        if low >= step {
            return step;
        }
        let span = u64::try_from((step - low).as_millis()).unwrap_or(u64::MAX);
        low + Duration::from_millis(rng.random_range(0..=span))
    }

    /// Acquisition share of the budget: the whole ladder up to its first
    /// capped step, plus one more capped step — by then a meeting awaiting a
    /// handler has made every attempt the ladder shapes.
    #[must_use]
    pub fn acquisition_budget(self) -> Duration {
        self.ladder_to_cap().iter().sum::<Duration>() + self.backoff_max
    }

    /// THE replacement retry budget (published as
    /// `mc_media_handler_replacement_retry_budget_seconds`): the acquisition
    /// share plus [`super::pusher::initial_registration_bound`] (the grant must
    /// still register before clients get it). The awaiting-handler page bound
    /// (OBS-ALERTS) and the env-test ceilings derive from this one value.
    #[must_use]
    pub fn retry_budget(self) -> Duration {
        self.acquisition_budget() + super::pusher::initial_registration_bound()
    }

    /// `mc_media_handler_migration_duration_seconds` bucket bounds, derived
    /// from config: the settle window around the reconnect phase, the
    /// ladder's cumulative waits for the replacement phase, and the
    /// registration bound and retry budget at the top — so the longest
    /// legitimate replacement lands in a real bucket, never `+Inf`.
    #[must_use]
    pub fn duration_histogram_buckets(self, connect_settle: Duration) -> Vec<f64> {
        let settle = connect_settle.as_secs_f64();
        let mut bounds = vec![settle / 2.0, settle, settle * 2.0];
        let mut cumulative = Duration::ZERO;
        for step in self.ladder_to_cap() {
            cumulative += step;
            bounds.push(cumulative.as_secs_f64());
        }
        bounds.push(super::pusher::initial_registration_bound().as_secs_f64());
        bounds.push(self.retry_budget().as_secs_f64());
        bounds.retain(|b| b.is_finite() && *b > 0.0);
        bounds.sort_by(f64::total_cmp);
        bounds.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        bounds
    }
}

/// One attempt's outcome (`mc_media_handler_migrations_total{outcome}`).
/// See the module doc for the counting unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The grant's first `RegisterMeeting` was acked: it is Active.
    Completed,
    /// GC answered `RESOURCE_EXHAUSTED`. Retried.
    NoCapacity,
    /// GC could not answer (transport, `UNAVAILABLE`, `DEADLINE_EXCEEDED`,
    /// `INTERNAL`, ...). Retried.
    GcUnavailable,
    /// The grant was removed before its first `RegisterMeeting` ack: its
    /// registration failed after retries, OR it was declared Dead/Impaired
    /// while still Registering. Acquisition continues (it is now excluded).
    AdmissionRefused,
    /// One owed replacement refused at the per-meeting attempt bound.
    RateLimited,
    /// A planned transfer's drain deadline passed (MC-F; never emitted by the
    /// crash path).
    TimedOut,
    /// A superseded grant, a refusal (`FAILED_PRECONDITION`,
    /// `INVALID_ARGUMENT`, a malformed `OK`), a grant MC cannot add, or the
    /// meeting ending with the attempt open.
    Abandoned,
}

impl MigrationOutcome {
    /// Every variant, for zero-initialisation.
    pub const ALL: [Self; 7] = [
        Self::Completed,
        Self::NoCapacity,
        Self::GcUnavailable,
        Self::AdmissionRefused,
        Self::RateLimited,
        Self::TimedOut,
        Self::Abandoned,
    ];

    /// The `outcome` label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::NoCapacity => "no_capacity",
            Self::GcUnavailable => "gc_unavailable",
            Self::AdmissionRefused => "admission_refused",
            Self::RateLimited => "rate_limited",
            Self::TimedOut => "timed_out",
            Self::Abandoned => "abandoned",
        }
    }
}

/// One phase of a migration
/// (`mc_media_handler_migration_duration_seconds{phase}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    /// The removal that owed the replacement → GC's grant.
    Replacement,
    /// The grant → its first `RegisterMeeting` ack (Active).
    Registration,
    /// Active → every participant admitted at that moment (and still present)
    /// is MH-reported connected to it.
    Reconnect,
}

impl MigrationPhase {
    /// Every variant, for zero-initialisation.
    pub const ALL: [Self; 3] = [Self::Replacement, Self::Registration, Self::Reconnect];

    /// The `phase` label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Replacement => "replacement",
            Self::Registration => "registration",
            Self::Reconnect => "reconnect",
        }
    }
}

/// One replacement the meeting owes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owed {
    /// Why the replaced handler left (the attempt's `trigger` label).
    pub trigger: RemovalTrigger,
    /// The handler being replaced (GC `replaced_handler_id`; logs).
    pub replaced: Option<HandlerId>,
    /// When the removal that owes it happened (replacement-phase origin).
    pub declared_at: Instant,
}

/// The one Acquire call in flight for a meeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlight {
    /// `handler_set_version` when the call was issued; a result for any other
    /// version is superseded.
    pub version: u64,
    /// What it is for.
    pub owed: Owed,
}

/// A granted handler that is still Registering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Granted {
    /// The replacement it fills.
    pub owed: Owed,
    /// When GC granted it.
    pub granted_at: Instant,
}

/// A grant that became Active, waiting for its participants to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reconnecting {
    trigger: RemovalTrigger,
    active_at: Instant,
    waiting: BTreeSet<String>,
}

/// What the attempt timer decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Due {
    /// Nothing to do.
    Idle,
    /// Issue one Acquire for this owed replacement (now in flight).
    Attempt(Owed),
    /// At the attempt bound: every owed replacement is refused (one
    /// `rate_limited` each) and the meeting stops acquiring for good.
    Refused(Vec<Owed>),
    /// No cap is known yet (MC not registered with GC): nothing was sent and
    /// nothing is counted; retried after a backoff step.
    NoCap,
}

/// What a removal did to the replacement bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalEffect {
    /// The removed handler was a Registering grant (its registration failed):
    /// that attempt ends `admission_refused`, and its replacement is owed again.
    pub refused: Option<Granted>,
    /// A NEW replacement became owed (a migration started).
    pub started: bool,
    /// The next attempt waits for the minimum interval (logged, never counted).
    pub deferred: bool,
}

/// One completed reconnect-phase observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectDone {
    /// The migration's trigger.
    pub trigger: RemovalTrigger,
    /// Active → last waiting participant connected.
    pub elapsed: Duration,
}

/// A meeting's replacement bookkeeping (see the module doc).
#[derive(Debug)]
pub struct ReplacementState {
    /// The hand-off size the meeting is kept at.
    target: usize,
    owed: VecDeque<Owed>,
    in_flight: Option<InFlight>,
    next_attempt_at: Option<Instant>,
    last_attempt_at: Option<Instant>,
    failures: u32,
    registering: BTreeMap<HandlerId, Granted>,
    reconnecting: BTreeMap<HandlerId, Reconnecting>,
    stopped: bool,
    rng: StdRng,
}

impl ReplacementState {
    /// A meeting kept at `target` handlers; jitter from `seed` (`Some`, tests)
    /// or an OS-seeded generator.
    #[must_use]
    pub fn new(target: usize, seed: Option<u64>) -> Self {
        let rng = match seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => rand::make_rng::<StdRng>(),
        };
        Self {
            target,
            owed: VecDeque::new(),
            in_flight: None,
            next_attempt_at: None,
            last_attempt_at: None,
            failures: 0,
            registering: BTreeMap::new(),
            reconnecting: BTreeMap::new(),
            stopped: false,
            rng,
        }
    }

    /// When the attempt timer should fire.
    #[must_use]
    pub fn next_attempt_at(&self) -> Option<Instant> {
        self.next_attempt_at
    }

    /// Whether an Acquire call is in flight.
    #[must_use]
    pub fn in_flight(&self) -> Option<&InFlight> {
        self.in_flight.as_ref()
    }

    /// How many replacements are owed and not in flight.
    #[must_use]
    pub fn owed_len(&self) -> usize {
        self.owed.len()
    }

    /// Whether the meeting stopped acquiring at its bound.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Arm the attempt timer if work is owed and nothing is armed or in
    /// flight: no earlier than one minimum interval after the last attempt.
    /// Returns whether the attempt was deferred past `now` by the interval
    /// (logged by the caller; never counted).
    fn schedule(&mut self, now: Instant, timing: ReplacementTiming) -> bool {
        if self.owed.is_empty() {
            self.next_attempt_at = None;
            return false;
        }
        if self.in_flight.is_some() || self.next_attempt_at.is_some() {
            return false;
        }
        let earliest = self
            .last_attempt_at
            .map_or(now, |last| (last + timing.min_interval()).max(now));
        self.next_attempt_at = Some(earliest);
        earliest > now
    }

    /// A handler left the meeting. `members_after` is the set size after the
    /// removal (Registering grants included).
    ///
    /// A Registering grant that left (its registration failed) re-owes ITS
    /// replacement (same trigger and origin); any other member owes a new one
    /// while the set is below target. Any pending reconnect observation is
    /// cleared: a new migration starts.
    pub fn on_removal(
        &mut self,
        id: &HandlerId,
        trigger: RemovalTrigger,
        members_after: usize,
        now: Instant,
        timing: ReplacementTiming,
    ) -> RemovalEffect {
        self.reconnecting.clear();
        let refused = self.registering.remove(id);
        let mut started = false;
        match &refused {
            Some(granted) => self.owed.push_back(granted.owed.clone()),
            None => {
                let accounted =
                    self.owed.len() + usize::from(self.in_flight.is_some()) + members_after;
                if accounted < self.target {
                    self.owed.push_back(Owed {
                        trigger,
                        replaced: Some(id.clone()),
                        declared_at: now,
                    });
                    started = true;
                }
            }
        }
        let deferred = self.schedule(now, timing);
        RemovalEffect {
            refused,
            started,
            deferred,
        }
    }

    /// The attempt timer fired. `exclude_len` is `|current ∪ removed|`.
    pub fn due(
        &mut self,
        now: Instant,
        version: u64,
        exclude_len: usize,
        cap: Option<NonZeroU32>,
        timing: ReplacementTiming,
    ) -> Due {
        if self.next_attempt_at.is_some_and(|at| at > now) {
            return Due::Idle;
        }
        self.next_attempt_at = None;
        if self.in_flight.is_some() || self.owed.is_empty() {
            return Due::Idle;
        }
        let Some(cap) = cap else {
            self.failures = self.failures.saturating_add(1);
            self.next_attempt_at = Some(now + timing.retry_delay(self.failures, &mut self.rng));
            return Due::NoCap;
        };
        if self.stopped || !within_attempt_bound(exclude_len, cap) {
            self.stopped = true;
            return Due::Refused(self.owed.drain(..).collect());
        }
        let Some(owed) = self.owed.pop_front() else {
            return Due::Idle;
        };
        self.last_attempt_at = Some(now);
        self.in_flight = Some(InFlight {
            version,
            owed: owed.clone(),
        });
        Due::Attempt(owed)
    }

    /// Take the in-flight attempt as its result arrives.
    pub fn take_in_flight(&mut self) -> Option<InFlight> {
        self.in_flight.take()
    }

    /// The attempt failed retryably (no capacity, GC unavailable, an unusable
    /// grant): owe it again and wait one more ladder step (jittered).
    pub fn retry_after_failure(&mut self, owed: Owed, now: Instant, timing: ReplacementTiming) {
        self.owed.push_front(owed);
        self.failures = self.failures.saturating_add(1);
        self.next_attempt_at = Some(now + timing.retry_delay(self.failures, &mut self.rng));
    }

    /// The attempt's grant was superseded (the set changed while GC chose):
    /// owe it again; it waits only the minimum interval, not a ladder step.
    pub fn retry_superseded(&mut self, owed: Owed, now: Instant, timing: ReplacementTiming) {
        self.owed.push_front(owed);
        self.schedule(now, timing);
    }

    /// The attempt is over without a retry (refused outright): nothing owed
    /// for it any more; later owed work is scheduled.
    pub fn give_up(&mut self, now: Instant, timing: ReplacementTiming) {
        self.schedule(now, timing);
    }

    /// GC granted `id` for `owed`; it is now Registering.
    pub fn granted(&mut self, id: HandlerId, owed: Owed, now: Instant, timing: ReplacementTiming) {
        self.failures = 0;
        self.registering.insert(
            id,
            Granted {
                owed,
                granted_at: now,
            },
        );
        self.schedule(now, timing);
    }

    /// Whether `id` is a Registering grant.
    #[must_use]
    pub fn is_registering_grant(&self, id: &HandlerId) -> bool {
        self.registering.contains_key(id)
    }

    /// `id`'s first `RegisterMeeting` was acked: it leaves Registering and its
    /// reconnect phase starts over the participants admitted now (`waiting`;
    /// an empty meeting has nothing to observe).
    pub fn on_active(
        &mut self,
        id: &HandlerId,
        now: Instant,
        waiting: BTreeSet<String>,
    ) -> Option<Granted> {
        let granted = self.registering.remove(id)?;
        if !waiting.is_empty() {
            self.reconnecting.insert(
                id.clone(),
                Reconnecting {
                    trigger: granted.owed.trigger,
                    active_at: now,
                    waiting,
                },
            );
        }
        Some(granted)
    }

    /// Whether any reconnect phase is being observed.
    #[must_use]
    pub fn has_reconnecting(&self) -> bool {
        !self.reconnecting.is_empty()
    }

    /// Re-check every open reconnect phase: `done(handler, participant)` says
    /// a waiting participant is now connected to the handler or has left
    /// (either way it stops waiting; joiners were never waited on). Returns
    /// the phases that completed.
    pub fn check_reconnects(
        &mut self,
        now: Instant,
        mut done: impl FnMut(&HandlerId, &str) -> bool,
    ) -> Vec<ReconnectDone> {
        let mut completed = Vec::new();
        self.reconnecting.retain(|id, r| {
            r.waiting.retain(|p| !done(id, p));
            if r.waiting.is_empty() {
                completed.push(ReconnectDone {
                    trigger: r.trigger,
                    elapsed: now.saturating_duration_since(r.active_at),
                });
                false
            } else {
                true
            }
        });
        completed
    }

    /// Forget owed work that can never be attempted (no routing deps were
    /// installed: nothing was ever programmed). Returns how many.
    pub fn drop_owed(&mut self) -> usize {
        self.next_attempt_at = None;
        let n = self.owed.len();
        self.owed.clear();
        n
    }

    /// The meeting is ending: every open attempt (in flight, or a grant still
    /// Registering) — each records `abandoned` at the caller. Reconnect
    /// observations are dropped.
    pub fn end(&mut self) -> Vec<Owed> {
        let mut open: Vec<Owed> = self.in_flight.take().map(|f| f.owed).into_iter().collect();
        open.extend(
            std::mem::take(&mut self.registering)
                .into_values()
                .map(|g| g.owed),
        );
        self.reconnecting.clear();
        self.owed.clear();
        self.next_attempt_at = None;
        open
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn timing() -> ReplacementTiming {
        ReplacementTiming::from_config_ms(1_000, 30_000)
    }

    fn hid(s: &str) -> HandlerId {
        HandlerId::new(s)
    }

    fn cap(n: u32) -> Option<NonZeroU32> {
        NonZeroU32::new(n)
    }

    #[test]
    fn the_ladder_doubles_from_the_minimum_interval_and_caps() {
        let t = timing();
        let steps: Vec<u64> = (1..=8).map(|f| t.ladder_step(f).as_secs()).collect();
        assert_eq!(steps, vec![1, 2, 4, 8, 16, 30, 30, 30]);
        assert_eq!(t.ladder_step(u32::MAX), t.backoff_max(), "no overflow");
    }

    #[test]
    fn jittered_delays_stay_within_half_step_and_the_minimum_interval() {
        let t = timing();
        let mut rng = StdRng::seed_from_u64(7);
        for failures in 1..=10 {
            let step = t.ladder_step(failures);
            for _ in 0..50 {
                let d = t.retry_delay(failures, &mut rng);
                assert!(d <= step, "{d:?} <= {step:?}");
                assert!(d >= (step / 2).max(t.min_interval()), "{d:?}");
                assert!(d >= t.min_interval());
            }
        }
    }

    #[test]
    fn the_budget_is_the_ladder_to_its_cap_plus_one_cap_plus_registration() {
        let t = timing();
        // 1+2+4+8+16+30, then one more 30.
        assert_eq!(t.acquisition_budget(), Duration::from_secs(91));
        let registration = super::super::pusher::initial_registration_bound();
        // 3 attempts x (10 s RPC + 5 s connect) + 1 s + 2 s backoffs.
        assert_eq!(registration, Duration::from_secs(48));
        assert_eq!(t.retry_budget(), Duration::from_secs(91 + 48));
    }

    #[test]
    fn duration_buckets_reach_the_retry_budget_and_are_sorted_unique() {
        let t = timing();
        let b = t.duration_histogram_buckets(Duration::from_millis(1500));
        assert!(b.windows(2).all(|w| w[0] < w[1]), "{b:?}");
        assert_eq!(*b.last().unwrap(), t.retry_budget().as_secs_f64());
        assert!(b.contains(&1.5), "the settle window is a bound");
    }

    #[test]
    fn the_bound_is_one_stricter_than_gc_at_exactly_cap_minus_one_and_cap() {
        let c = NonZeroU32::new(16).unwrap();
        assert!(within_attempt_bound(15, c), "cap - 1 is allowed");
        assert!(
            !within_attempt_bound(16, c),
            "cap is stopped (GC would accept it)"
        );
        assert!(!within_attempt_bound(17, c));
    }

    #[test]
    fn registration_failure_is_apply_lag() {
        assert_eq!(registration_failure_trigger(), RemovalTrigger::ApplyLag);
    }

    #[test]
    fn labels_are_the_catalogued_vocabulary() {
        let o: Vec<_> = MigrationOutcome::ALL.iter().map(|o| o.label()).collect();
        assert_eq!(
            o,
            vec![
                "completed",
                "no_capacity",
                "gc_unavailable",
                "admission_refused",
                "rate_limited",
                "timed_out",
                "abandoned"
            ]
        );
        let p: Vec<_> = MigrationPhase::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(p, vec!["replacement", "registration", "reconnect"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_removal_below_target_is_owed_and_due_now_and_one_is_in_flight_at_a_time() {
        let t = timing();
        let mut s = ReplacementState::new(2, Some(1));
        let now = Instant::now();
        let effect = s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 1, now, t);
        assert!(effect.refused.is_none() && effect.started && !effect.deferred);
        assert_eq!(s.next_attempt_at(), Some(now));
        let Due::Attempt(owed) = s.due(now, 3, 2, cap(16), t) else {
            panic!("an attempt is due");
        };
        assert_eq!(owed.replaced, Some(hid("h1")));
        assert_eq!(s.in_flight().map(|f| f.version), Some(3));
        // A second removal while one is in flight is owed, not issued.
        s.on_removal(&hid("h2"), RemovalTrigger::ApplyLag, 0, now, t);
        assert_eq!(s.owed_len(), 1);
        assert_eq!(s.due(now, 4, 3, cap(16), t), Due::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn a_removal_at_or_above_target_owes_nothing() {
        let t = timing();
        let mut s = ReplacementState::new(1, Some(1));
        let effect = s.on_removal(
            &hid("h1"),
            RemovalTrigger::HeartbeatTimeout,
            1,
            Instant::now(),
            t,
        );
        assert!(!effect.started);
        assert_eq!(s.owed_len(), 0);
        assert_eq!(s.next_attempt_at(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_next_attempt_waits_the_minimum_interval_and_the_deferral_is_reported() {
        let t = timing();
        let mut s = ReplacementState::new(3, Some(1));
        let t0 = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 2, t0, t);
        assert!(matches!(s.due(t0, 1, 3, cap(16), t), Due::Attempt(_)));
        let inflight = s.take_in_flight().unwrap();
        s.granted(hid("h9"), inflight.owed, t0, t);
        let effect = s.on_removal(&hid("h2"), RemovalTrigger::HeartbeatTimeout, 2, t0, t);
        assert!(effect.deferred, "inside the minimum interval");
        assert_eq!(s.next_attempt_at(), Some(t0 + t.min_interval()));
        assert_eq!(s.due(t0, 2, 4, cap(16), t), Due::Idle, "not yet due");
    }

    #[tokio::test(start_paused = true)]
    async fn retryable_failures_climb_the_ladder_and_a_grant_resets_it() {
        let t = timing();
        let mut s = ReplacementState::new(1, Some(3));
        let mut now = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 0, now, t);
        for failures in 1..=4u32 {
            assert!(matches!(s.due(now, 1, 1, cap(16), t), Due::Attempt(_)));
            let f = s.take_in_flight().unwrap();
            s.retry_after_failure(f.owed, now, t);
            let at = s.next_attempt_at().unwrap();
            let wait = at - now;
            assert!(wait <= t.ladder_step(failures) && wait >= t.min_interval());
            now = at;
        }
        assert!(matches!(s.due(now, 1, 1, cap(16), t), Due::Attempt(_)));
        let f = s.take_in_flight().unwrap();
        s.granted(hid("h2"), f.owed, now, t);
        assert!(s.is_registering_grant(&hid("h2")));
        assert_eq!(s.next_attempt_at(), None, "nothing more is owed");
    }

    #[tokio::test(start_paused = true)]
    async fn at_the_bound_every_owed_replacement_is_refused_and_the_meeting_stops() {
        let t = timing();
        let mut s = ReplacementState::new(3, Some(1));
        let now = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 1, now, t);
        s.on_removal(&hid("h2"), RemovalTrigger::HeartbeatTimeout, 0, now, t);
        assert_eq!(s.owed_len(), 2);
        let Due::Refused(refused) = s.due(now, 1, 16, cap(16), t) else {
            panic!("at the bound");
        };
        assert_eq!(refused.len(), 2, "one per owed replacement");
        assert!(s.is_stopped());
        // A later loss on a stopped meeting is also refused (counted again).
        s.on_removal(&hid("h3"), RemovalTrigger::ApplyLag, 0, now, t);
        let at = s.next_attempt_at().unwrap();
        let Due::Refused(again) = s.due(at, 1, 17, cap(16), t) else {
            panic!("still stopped");
        };
        assert_eq!(again.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn no_cap_sends_nothing_and_backs_off() {
        let t = timing();
        let mut s = ReplacementState::new(1, Some(1));
        let now = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 0, now, t);
        assert_eq!(s.due(now, 1, 1, None, t), Due::NoCap);
        assert!(s.in_flight().is_none());
        assert!(s.next_attempt_at().unwrap() > now);
        assert_eq!(s.owed_len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_grant_re_owes_its_own_replacement_with_its_origin() {
        let t = timing();
        let mut s = ReplacementState::new(1, Some(1));
        let t0 = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 0, t0, t);
        assert!(matches!(s.due(t0, 1, 1, cap(16), t), Due::Attempt(_)));
        let f = s.take_in_flight().unwrap();
        s.granted(hid("h2"), f.owed, t0, t);
        let later = t0 + Duration::from_secs(5);
        let effect = s.on_removal(&hid("h2"), RemovalTrigger::ApplyLag, 0, later, t);
        assert!(!effect.started, "not a new migration");
        let refused = effect.refused.expect("h2 was a Registering grant");
        assert_eq!(refused.owed.trigger, RemovalTrigger::HeartbeatTimeout);
        assert_eq!(s.owed_len(), 1);
        let Due::Attempt(again) = s.due(later, 3, 2, cap(16), t) else {
            panic!("acquisition continues");
        };
        assert_eq!(again.declared_at, t0, "the original origin");
        assert_eq!(again.replaced, Some(hid("h1")));
    }

    #[tokio::test(start_paused = true)]
    async fn the_reconnect_phase_waits_only_on_the_participants_admitted_at_active() {
        let t = timing();
        let mut s = ReplacementState::new(1, Some(1));
        let t0 = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 0, t0, t);
        let Due::Attempt(_) = s.due(t0, 1, 1, cap(16), t) else {
            panic!()
        };
        let f = s.take_in_flight().unwrap();
        s.granted(hid("h2"), f.owed, t0, t);
        let waiting: BTreeSet<String> = ["a".to_string(), "b".to_string()].into();
        assert!(s.on_active(&hid("h2"), t0, waiting).is_some());
        let later = t0 + Duration::from_secs(2);
        assert!(s.check_reconnects(later, |_, p| p == "a").is_empty());
        let done = s.check_reconnects(later, |_, p| p == "b");
        assert_eq!(
            done,
            vec![ReconnectDone {
                trigger: RemovalTrigger::HeartbeatTimeout,
                elapsed: Duration::from_secs(2)
            }]
        );
        assert!(!s.has_reconnecting());
    }

    #[tokio::test(start_paused = true)]
    async fn ending_returns_every_open_attempt() {
        let t = timing();
        let mut s = ReplacementState::new(2, Some(1));
        let now = Instant::now();
        s.on_removal(&hid("h1"), RemovalTrigger::HeartbeatTimeout, 1, now, t);
        let Due::Attempt(_) = s.due(now, 1, 2, cap(16), t) else {
            panic!()
        };
        let f = s.take_in_flight().unwrap();
        s.granted(hid("h3"), f.owed, now, t);
        s.on_removal(&hid("h2"), RemovalTrigger::HeartbeatTimeout, 1, now, t);
        let at = s.next_attempt_at().unwrap();
        let Due::Attempt(_) = s.due(at, 2, 3, cap(16), t) else {
            panic!()
        };
        assert_eq!(s.end().len(), 2, "in flight + registering");
        assert_eq!(s.next_attempt_at(), None);
    }
}
