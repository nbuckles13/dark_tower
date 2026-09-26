//! KEK rotation lifecycle (ADR-0036 §4 Rotation; story 2 R-12, R-15, R-26).
//!
//! Everything here is either pure or sits beside the meeting actor rather than
//! inside it:
//!
//! - [`RotationTrigger`] / [`KekPushOutcome`] — the two bounded vocabularies.
//! - [`KekRotationDebounce`] — the leave debounce, a pure state machine driven
//!   by `now`, so paused-time and unit tests need no seam.
//! - [`KekLifecycle`] — W, the overdue threshold, and the per-meeting state the
//!   fleet gauges sample. Shared as an `Arc`, like `PolicyGenerations`.
//! - [`collect_push_outcomes`] — the per-recipient outcome collector and the
//!   rotation's INFO lifecycle log.
//!
//! # Key custody
//!
//! Nothing in this module holds, formats or logs key bytes. The KEK travels to
//! participants as an `Arc<MeetingKek>` inside `ParticipantMessage::KekUpdate`
//! and leaves the `Arc` only at the participant actor's `MeetingKekUpdate`
//! encode. Telemetry carries `key_custody=operator`, never an end-to-end,
//! zero-trust or forward-secrecy claim: MC holds every KEK it issues.

use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock};
use tokio::time::Instant;
use tracing::info;

/// Overdue threshold as a multiple of W.
///
/// A named constant, not configuration (story 2 task 9, @operations OPS-E): the
/// operator-facing artifact is the published threshold GAUGE, which
/// `MCKekRotationOverdue` compares against pending age with no arithmetic, and
/// nothing an operator would tune at 3am lives here.
///
/// Must be at least 2. Healthy pending age peaks at W plus rotation latency
/// just before each rotation, so at 1 the page would be satisfiable by a
/// healthy steady state; at 2 it never is.
pub const KEK_ROTATION_OVERDUE_MULTIPLIER: u32 = 2;

const _: () = assert!(
    KEK_ROTATION_OVERDUE_MULTIPLIER >= 2,
    "at 1, healthy pending age reaches the threshold just before every rotation and \
     MCKekRotationOverdue (a page) would fire on a healthy fleet"
);

/// How long the outcome collector waits for participant actors to answer.
///
/// Comfortably above p99 fan-out — each recipient is one non-blocking
/// `try_send` plus one oneshot round-trip — and far below the W floor of 30 s,
/// so a stalled collector can never overlap the next rotation. The duration
/// histogram's top buckets sit at or above this, so a fully-timed-out rotation
/// lands in a real bucket rather than saturating `+Inf`.
pub const KEK_PUSH_OUTCOME_TIMEOUT: Duration = Duration::from_secs(2);

/// How often the fleet gauges are recomputed from actor-written state.
///
/// Pending age grows with wall-clock, so an event-driven gauge would read stale
/// and never cross the threshold. Sampling `Instant`s the actors wrote — rather
/// than asking the actors — means a WEDGED actor still shows a growing age,
/// which is the case the page exists for. Well inside the 15 s scrape interval
/// and the alert's `for: 2m`.
pub const KEK_GAUGE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

// ============================================================================
// Vocabularies
// ============================================================================

/// What caused a KEK to be generated. `trigger` on
/// `mc_meeting_kek_generated_total`.
///
/// # Alert partition — every value MUST be classified into exactly one
///
/// - `participant_left` → `MCKekRotationStorm` (debounced; its `2/W` threshold
///   is derived from the debounce).
/// - `sender_space_exhausted` → `MCKekEpochResetOnSenderIdExhaustion` (info).
///   Exempt from the debounce, so it can never approach `2/W`; a per-trigger
///   storm series would read as coverage while being unable to fire.
/// - `meeting_created` → neither (it is meeting creation).
///
/// The storm rule uses a POSITIVE selector, so **a new value added here is
/// silently unalerted until it is classified above** — the one place in the
/// KEK telemetry that trades fail-closed for semantic honesty.
///
/// Label key is `trigger`, deliberately not `reason` (reserved for the
/// per-frame drop families) nor `event_type` (bound to connect/disconnect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationTrigger {
    /// The meeting actor was created.
    MeetingCreated,
    /// Leave-debounced rotation after one or more roster removals.
    ParticipantLeft,
    /// Immediate KEK-epoch reset on `sender_id` exhaustion.
    SenderSpaceExhausted,
}

impl RotationTrigger {
    /// Every variant; the exhaustive `label()` match is the witness.
    pub const ALL: [Self; 3] = [
        Self::MeetingCreated,
        Self::ParticipantLeft,
        Self::SenderSpaceExhausted,
    ];

    /// Bounded `trigger` label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MeetingCreated => "meeting_created",
            Self::ParticipantLeft => "participant_left",
            Self::SenderSpaceExhausted => "sender_space_exhausted",
        }
    }
}

/// One recipient's outcome for one `MeetingKekUpdate`. `outcome` on
/// `mc_meeting_kek_pushes_total`, recorded exactly ONCE per rostered recipient.
///
/// **Not `PolicyPushOutcome`.** Same `label()` + `ALL` idiom, disjoint value
/// sets: that one partitions MH's echo to a control-plane push; this one
/// partitions per-recipient client delivery. Do not hoist them together.
///
/// Five disjoint values, each naming a distinct remedy. Only
/// `participant_gone` is benign, which is why `MCKekPushFailureRate` selects
/// `outcome!~"delivered|participant_gone"` — negated, so a value added later
/// fails CLOSED into the alert.
///
/// # Why an undelivered push matters (security S-3a)
///
/// A member whose update is not delivered never rotates its own transmit keys,
/// so a departed participant keeps opening THAT member's media past W. This
/// counter and `MCKekPushFailureRate` are the only control for that residual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KekPushOutcome {
    /// Handed to the connection's outbound stream. Not a client ack.
    Delivered,
    /// The outbound stream channel was full or closed; the client did not get
    /// it. Also counted on `mc_participant_outbound_messages_dropped_total`.
    DroppedOutbound,
    /// Rostered, but no live connection (inside the reconnect grace period).
    /// BENIGN: a returning participant receives the current KEK — on reconnect
    /// re-issue, or on a fresh join.
    ParticipantGone,
    /// The participant actor's mailbox was closed, or it exited before
    /// answering. Points at `MCActorPanic` territory. Explicitly NOT the
    /// alive-but-silent case, which is [`Self::TimedOut`].
    ActorUnavailable,
    /// Handed to the mailbox, and [`KEK_PUSH_OUTCOME_TIMEOUT`] elapsed with no
    /// reply.
    ///
    /// Names the OBSERVABLE, not an inferred cause (the `wrap_key_id_mismatch`
    /// lesson in `label-taxonomy.md`). MC does not observe that the actor is
    /// wedged: a lost oneshot, scheduler starvation or a merely slow turn all
    /// produce this same one bit, so `wedged`/`unresponsive` would assert more
    /// than MC can see. Correlate with `mc_actor_mailbox_depth` — this points at
    /// `MCHighMailboxDepthCritical` territory, a different fault from
    /// `actor_unavailable`, which is why the two are separate values.
    ///
    /// Emitted rather than simply no longer waited on: an unrecorded
    /// non-responder would leave both the numerator and the denominator of the
    /// failure ratio, so the alert would get QUIETER as more actors wedged.
    TimedOut,
}

impl KekPushOutcome {
    /// Every variant; the exhaustive `label()` match is the witness.
    pub const ALL: [Self; 5] = [
        Self::Delivered,
        Self::DroppedOutbound,
        Self::ParticipantGone,
        Self::ActorUnavailable,
        Self::TimedOut,
    ];

    /// Bounded `outcome` label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::DroppedOutbound => "dropped_outbound",
            Self::ParticipantGone => "participant_gone",
            Self::ActorUnavailable => "actor_unavailable",
            Self::TimedOut => "timed_out",
        }
    }
}

// ============================================================================
// Debounce
// ============================================================================

/// Departures folded into one rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coalesced {
    /// Roster removals covered by this rotation.
    pub leaves: u32,
    /// When the OLDEST of them happened.
    pub oldest: Instant,
}

/// The leave debounce: at most one leave-triggered rotation per W, measured
/// from the OLDEST un-rotated removal and **never reset by a later one**.
///
/// # Not `media_routing/connectivity.rs`'s window — do not unify them
///
/// That module also holds a per-participant window with a deadline, and it is
/// the structurally nearest neighbour. Its semantics are the opposite in
/// exactly the way that matters: its floor anchors on the LAST settle and "only
/// ever delays itself". This anchors on the OLDEST un-rotated removal. Lifting
/// that shape here would restart the timer on every departure, so a steady
/// trickle of leaves — one every W/2, say — would never rotate at all, and W
/// would silently stop being a bound on a departed participant's access.
///
/// Also not `media_routing/pusher.rs`'s coalescing, which is a latest-wins
/// watch. This one COUNTS departures into a histogram.
#[derive(Debug)]
pub struct KekRotationDebounce {
    window: Duration,
    pending: Option<Coalesced>,
    /// Set only after a failed rotation, so a persistent failure retries once
    /// per W instead of spinning, while `pending.oldest` — and so the published
    /// pending age — keeps growing and the overdue page fires.
    retry_at: Option<Instant>,
}

impl KekRotationDebounce {
    /// A debounce with nothing pending.
    #[must_use]
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            pending: None,
            retry_at: None,
        }
    }

    /// Record a roster removal. Starts the window if none is pending; otherwise
    /// only counts it — the window's anchor is NEVER moved.
    pub fn record_removal(&mut self, now: Instant) {
        match &mut self.pending {
            Some(p) => p.leaves = p.leaves.saturating_add(1),
            None => {
                self.pending = Some(Coalesced {
                    leaves: 1,
                    oldest: now,
                });
            }
        }
    }

    /// When the actor should next wake to rotate, if anything is pending.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        let due = self.pending?.oldest + self.window;
        Some(match self.retry_at {
            Some(retry) if retry > due => retry,
            _ => due,
        })
    }

    /// If the rotation is due at `now`, take what it covers.
    ///
    /// The caller MUST either complete the rotation or call
    /// [`Self::defer_after_failure`] with the returned value.
    pub fn take_due(&mut self, now: Instant) -> Option<Coalesced> {
        if self.next_wake()? > now {
            return None;
        }
        self.retry_at = None;
        self.pending.take()
    }

    /// A rotation failed: put the departures back, oldest anchor intact, and
    /// retry after W.
    pub fn defer_after_failure(&mut self, taken: Coalesced, now: Instant) {
        self.pending = Some(taken);
        self.retry_at = Some(now + self.window);
    }

    /// An epoch reset rotated the KEK for its own reason; it covers any pending
    /// departures too. Returns how many it absorbed.
    pub fn absorb(&mut self) -> Option<Coalesced> {
        self.retry_at = None;
        self.pending.take()
    }

    /// The oldest un-rotated removal, if any. What pending age is measured from.
    #[must_use]
    pub fn pending_since(&self) -> Option<Instant> {
        self.pending.map(|p| p.oldest)
    }
}

// ============================================================================
// Fleet state for the gauges
// ============================================================================

#[derive(Debug, Default, Clone, Copy)]
struct MeetingKekStatus {
    pending_since: Option<Instant>,
    sender_ids_issued: u32,
}

/// W, the overdue threshold, and each live meeting's rotation state, for the
/// pod-level gauges.
///
/// Both per-meeting gauges are published as a **MAXIMUM across live meetings**.
/// ADR-0036 §11 bars a meeting label, so every actor would write one series;
/// last-writer-wins would let a healthy meeting's 0 erase an overdue meeting's
/// age, and a sum would hide one meeting near the cliff behind fleet noise.
///
/// `tokio::sync::RwLock`, matching `PolicyGenerations` and `redis/client.rs` —
/// the in-tree idiom for a shared map on an async path. A `std::sync::Mutex`
/// would need poison handling the workspace lints forbid. The access pattern is
/// read-mostly in RwLock's shape: N actors write on change, one sampler reads.
///
/// Evicted on BOTH teardown paths — the actor's own exit and the controller's
/// `remove_meeting` — so a panicked actor does not pin a stale age forever.
#[derive(Debug)]
pub struct KekLifecycle {
    /// W in whole seconds — held in the WIRE's own type and width.
    ///
    /// Deliberately `u32` seconds rather than a `Duration`
    /// (story 2 task 9, @paired-protocol F-2): the wire field
    /// `kek_rotation_debounce_seconds` is a `uint32`, and every other form of W
    /// (the debounce `Duration`, the window gauge, the threshold) is DERIVED
    /// from this one number by widening. With a `Duration` here, putting W on
    /// the wire needed a narrowing, and a narrowing that saturated would have
    /// published a W LARGER than the one the timer enforces — reversing
    /// "publish the value enforcement reads". No narrowing exists now.
    window_seconds: u32,
    meetings: RwLock<HashMap<String, MeetingKekStatus>>,
}

impl KekLifecycle {
    /// W as MC enforces it, in seconds. The ONE source for the debounce, the
    /// wire field and the window gauge, so the three cannot drift.
    #[must_use]
    pub fn new(window_seconds: u32) -> Self {
        Self {
            window_seconds,
            meetings: RwLock::new(HashMap::new()),
        }
    }

    /// W, widened from the stored seconds.
    #[must_use]
    pub fn window(&self) -> Duration {
        Duration::from_secs(u64::from(self.window_seconds))
    }

    /// W in whole seconds, for `kek_rotation_debounce_seconds` on the wire —
    /// the stored value itself, not a conversion.
    #[must_use]
    pub fn window_seconds(&self) -> u32 {
        self.window_seconds
    }

    /// W × [`KEK_ROTATION_OVERDUE_MULTIPLIER`].
    #[must_use]
    pub fn overdue_threshold(&self) -> Duration {
        self.window() * KEK_ROTATION_OVERDUE_MULTIPLIER
    }

    /// Record a meeting's oldest un-rotated removal (`None` once rotated).
    pub async fn set_pending(&self, meeting_id: &str, pending_since: Option<Instant>) {
        let mut meetings = self.meetings.write().await;
        meetings
            .entry(meeting_id.to_string())
            .or_default()
            .pending_since = pending_since;
    }

    /// Record a meeting's namespace consumption in its current epoch.
    pub async fn set_sender_ids_issued(&self, meeting_id: &str, issued: u32) {
        let mut meetings = self.meetings.write().await;
        meetings
            .entry(meeting_id.to_string())
            .or_default()
            .sender_ids_issued = issued;
    }

    /// Forget a meeting. Idempotent; called on both teardown paths.
    ///
    /// Named `remove_meeting` to match `PolicyGenerations::remove_meeting`, the
    /// sibling registry this one is modelled on — same operation, same call
    /// sites, one name.
    pub async fn remove_meeting(&self, meeting_id: &str) {
        self.meetings.write().await.remove(meeting_id);
    }

    /// `(max pending age, max sender ids issued)` across live meetings.
    pub async fn sample(&self, now: Instant) -> (Duration, u32) {
        let meetings = self.meetings.read().await;
        let age = meetings
            .values()
            .filter_map(|m| m.pending_since)
            .map(|since| now.saturating_duration_since(since))
            .max()
            .unwrap_or_default();
        let issued = meetings
            .values()
            .map(|m| m.sender_ids_issued)
            .max()
            .unwrap_or(0);
        (age, issued)
    }

    /// Publish the two config-echo gauges, once, from the values enforced here.
    pub fn publish_config_gauges(&self) {
        crate::observability::metrics::set_kek_rotation_window(self.window());
        crate::observability::metrics::set_kek_rotation_overdue_threshold(self.overdue_threshold());
    }

    /// Recompute and publish the two fleet gauges.
    pub async fn publish_fleet_gauges(&self) {
        let (age, issued) = self.sample(Instant::now()).await;
        crate::observability::metrics::set_kek_rotation_pending_age(age);
        crate::observability::metrics::set_sender_ids_issued_max(issued);
    }
}

// ============================================================================
// Outcome collection
// ============================================================================

/// One recipient's pending outcome.
#[derive(Debug)]
pub enum PendingOutcome {
    /// Decided at send time (no connection, or the mailbox was closed).
    Decided(KekPushOutcome),
    /// Handed to the participant actor, which answers on this channel.
    Awaiting(oneshot::Receiver<KekPushOutcome>),
}

/// What a rotation reports once its outcomes are in. No key material.
#[derive(Debug, Clone, Copy)]
pub struct RotationReport {
    /// What caused it.
    pub trigger: RotationTrigger,
    /// Departures folded in. For `sender_space_exhausted`, the pending
    /// departures the reset absorbed (often 0).
    pub coalesced_leaves: u32,
    /// The new generation. Log-only: never a metric label (ADR-0036 §11 — a
    /// per-meeting generation series is a membership-change trace).
    pub generation: u16,
    /// When the rotation began, for the duration histogram.
    pub started: Instant,
}

/// Await every recipient's outcome (bounded by [`KEK_PUSH_OUTCOME_TIMEOUT`]),
/// then record them all, the duration, and the INFO lifecycle log.
///
/// Runs detached from the meeting actor, holding only oneshot receivers, so the
/// actor never awaits a participant actor (which could deadlock against a
/// participant blocked on the meeting mailbox).
///
/// Every recipient yields exactly one outcome, and the histogram and log are
/// emitted UNCONDITIONALLY — a wedged recipient becomes `timed_out`, never an
/// absence. Otherwise one silent actor would erase the telemetry for a rotation
/// that mostly succeeded, and absence would be indistinguishable from "no
/// rotation happened".
pub async fn collect_push_outcomes(pending: Vec<PendingOutcome>, report: RotationReport) {
    let deadline = Instant::now() + KEK_PUSH_OUTCOME_TIMEOUT;
    let members = pending.len();
    let mut tally = OutcomeTally::default();
    // Sequential awaits against ONE shared deadline: the total wait is bounded
    // by the deadline however many recipients there are, exactly as a join
    // would be, without another dependency.
    for p in pending {
        let outcome = match p {
            PendingOutcome::Decided(o) => o,
            PendingOutcome::Awaiting(rx) => match tokio::time::timeout_at(deadline, rx).await {
                Ok(Ok(o)) => o,
                Ok(Err(_)) => KekPushOutcome::ActorUnavailable,
                Err(_) => KekPushOutcome::TimedOut,
            },
        };
        crate::observability::metrics::record_kek_push(outcome);
        tally.add(outcome);
    }
    let duration = report.started.elapsed();
    crate::observability::metrics::record_kek_rotation_duration(duration);

    // The lifecycle record. Carries NO key bytes and NO participant identifier
    // (those stay on the span); meeting_id rides the actor's span. Generation
    // is safe here — inside a log line, never a label.
    info!(
        target: "mc.kek.lifecycle",
        trigger = report.trigger.label(),
        coalesced_leaves = report.coalesced_leaves,
        members,
        generation = report.generation,
        duration_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        delivered = tally.delivered,
        dropped_outbound = tally.dropped_outbound,
        participant_gone = tally.participant_gone,
        actor_unavailable = tally.actor_unavailable,
        timed_out = tally.timed_out,
        key_custody = crate::observability::metrics::KEY_CUSTODY_OPERATOR,
        "Meeting KEK rotated"
    );
}

/// Per-rotation outcome counts for the lifecycle log.
#[derive(Debug, Default)]
struct OutcomeTally {
    delivered: usize,
    dropped_outbound: usize,
    participant_gone: usize,
    actor_unavailable: usize,
    timed_out: usize,
}

impl OutcomeTally {
    /// Exhaustive, so a new outcome is a compile error here rather than a
    /// silently uncounted log field.
    fn add(&mut self, outcome: KekPushOutcome) {
        let slot = match outcome {
            KekPushOutcome::Delivered => &mut self.delivered,
            KekPushOutcome::DroppedOutbound => &mut self.dropped_outbound,
            KekPushOutcome::ParticipantGone => &mut self.participant_gone,
            KekPushOutcome::ActorUnavailable => &mut self.actor_unavailable,
            KekPushOutcome::TimedOut => &mut self.timed_out,
        };
        *slot += 1;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const W_SECS: u32 = 60;
    const W: Duration = Duration::from_secs(60);

    #[test]
    fn nothing_pending_means_no_wake() {
        assert_eq!(KekRotationDebounce::new(W).next_wake(), None);
    }

    /// THE property: measured from the oldest removal, never reset by later
    /// ones. A trickle of removals every W/2 still rotates at oldest + W.
    #[test]
    fn debounce_is_measured_from_the_oldest_removal_and_never_reset() {
        let t0 = Instant::now();
        let mut d = KekRotationDebounce::new(W);
        d.record_removal(t0);
        d.record_removal(t0 + W / 2);
        d.record_removal(t0 + W - Duration::from_secs(1));
        assert_eq!(
            d.next_wake(),
            Some(t0 + W),
            "a later removal moved the deadline; under a steady trickle W would stop being a bound"
        );
        assert!(d.take_due(t0 + W - Duration::from_millis(1)).is_none());
        let c = d.take_due(t0 + W).unwrap();
        assert_eq!(c.oldest, t0);
    }

    #[test]
    fn departures_in_one_window_coalesce_into_one_rotation() {
        let t0 = Instant::now();
        let mut d = KekRotationDebounce::new(W);
        for i in 0..5 {
            d.record_removal(t0 + Duration::from_secs(i));
        }
        assert_eq!(d.take_due(t0 + W).unwrap().leaves, 5);
        assert!(d.take_due(t0 + W * 3).is_none(), "one rotation, not five");
    }

    #[test]
    fn a_removal_after_rotation_starts_a_new_window() {
        let t0 = Instant::now();
        let mut d = KekRotationDebounce::new(W);
        d.record_removal(t0);
        d.take_due(t0 + W).unwrap();
        d.record_removal(t0 + W * 2);
        assert_eq!(d.next_wake(), Some(t0 + W * 3));
    }

    /// A failed rotation keeps the ORIGINAL anchor — so pending age keeps
    /// growing and the overdue page fires — and retries once per W.
    #[test]
    fn failure_keeps_the_oldest_anchor_and_retries_after_w() {
        let t0 = Instant::now();
        let mut d = KekRotationDebounce::new(W);
        d.record_removal(t0);
        let due = t0 + W;
        let taken = d.take_due(due).unwrap();
        d.defer_after_failure(taken, due);
        assert_eq!(d.pending_since(), Some(t0), "anchor preserved");
        assert_eq!(d.next_wake(), Some(due + W), "retried after W, not spun");
        assert!(d.take_due(due + W / 2).is_none());
        assert!(d.take_due(due + W).is_some());
    }

    #[test]
    fn an_epoch_reset_absorbs_pending_departures() {
        let t0 = Instant::now();
        let mut d = KekRotationDebounce::new(W);
        d.record_removal(t0);
        d.record_removal(t0);
        assert_eq!(d.absorb().unwrap().leaves, 2);
        assert_eq!(d.next_wake(), None);
    }

    /// The DISCRIMINATING fleet-max fixture (@test 2): two live meetings, the
    /// YOUNGER written LAST. Last-writer-wins publishes the younger age and
    /// passes any single-meeting test; only this ordering tells them apart.
    #[tokio::test]
    async fn fleet_pending_age_is_the_maximum_not_the_last_write() {
        let lifecycle = KekLifecycle::new(W_SECS);
        let now = Instant::now();
        lifecycle
            .set_pending("older", Some(now - Duration::from_secs(90)))
            .await;
        lifecycle
            .set_pending("younger", Some(now - Duration::from_secs(10)))
            .await;
        let (age, _) = lifecycle.sample(now).await;
        assert_eq!(
            age,
            Duration::from_secs(90),
            "a healthy meeting's recent write must not erase an overdue meeting's age"
        );
    }

    #[tokio::test]
    async fn fleet_issued_is_the_maximum_and_a_rotated_meeting_reads_zero_age() {
        let lifecycle = KekLifecycle::new(W_SECS);
        let now = Instant::now();
        lifecycle.set_sender_ids_issued("a", 50_000).await;
        lifecycle.set_sender_ids_issued("b", 3).await;
        lifecycle.set_pending("a", None).await;
        let (age, issued) = lifecycle.sample(now).await;
        assert_eq!(issued, 50_000);
        assert_eq!(age, Duration::ZERO);
    }

    #[tokio::test]
    async fn removing_a_meeting_drops_it_from_both_gauges() {
        let lifecycle = KekLifecycle::new(W_SECS);
        let now = Instant::now();
        lifecycle
            .set_pending("gone", Some(now - Duration::from_secs(500)))
            .await;
        lifecycle.set_sender_ids_issued("gone", 60_000).await;
        lifecycle.remove_meeting("gone").await;
        assert_eq!(lifecycle.sample(now).await, (Duration::ZERO, 0));
    }

    fn report() -> RotationReport {
        RotationReport {
            trigger: RotationTrigger::ParticipantLeft,
            coalesced_leaves: 1,
            generation: 1,
            started: Instant::now(),
        }
    }

    /// THE discriminating collector case (@test 5; @observability P1;
    /// @operations OPS-M). A recipient that is ALIVE with an open channel but
    /// never answers. The happy-path integration test passes identically with
    /// and without the bounded wait; only this case tells them apart.
    ///
    /// Without the bound, this collector would never return: no duration, no
    /// log, and — the part that matters — not even the `delivered` outcome
    /// already in hand. `timed_out` is EMITTED, so the failure ratio's
    /// denominator stays complete and the alert cannot get quieter as actors
    /// wedge.
    #[tokio::test(start_paused = true)]
    async fn a_silent_recipient_times_out_and_everything_is_still_recorded() {
        use common::observability::testing::MetricAssertion;
        let snap = MetricAssertion::snapshot();
        // Kept alive for the whole test, so this is a timeout, not a dropped
        // sender (which would be actor_unavailable).
        let (_silent_tx, silent_rx) = oneshot::channel::<KekPushOutcome>();
        collect_push_outcomes(
            vec![
                PendingOutcome::Decided(KekPushOutcome::Delivered),
                PendingOutcome::Awaiting(silent_rx),
            ],
            report(),
        )
        .await;
        // Histogram FIRST: every snapshot read drains histogram observations
        // (common::observability::testing, "drain-on-read"), so a counter
        // query before this one would consume the observation and read 0.
        snap.histogram("mc_meeting_kek_rotation_duration_seconds")
            .assert_observation_count(1);
        snap.counter("mc_meeting_kek_pushes_total")
            .with_labels(&[("outcome", "delivered")])
            .assert_delta(1);
        snap.counter("mc_meeting_kek_pushes_total")
            .with_labels(&[("outcome", "timed_out")])
            .assert_delta(1);
    }

    /// A dropped sender is a closed actor, not a silent one — the two point at
    /// different alerts (`MCActorPanic` vs `MCHighMailboxDepthCritical`).
    #[tokio::test]
    async fn a_dropped_sender_is_actor_unavailable_not_timed_out() {
        use common::observability::testing::MetricAssertion;
        let snap = MetricAssertion::snapshot();
        let (tx, rx) = oneshot::channel::<KekPushOutcome>();
        drop(tx);
        collect_push_outcomes(vec![PendingOutcome::Awaiting(rx)], report()).await;
        snap.counter("mc_meeting_kek_pushes_total")
            .with_labels(&[("outcome", "actor_unavailable")])
            .assert_delta(1);
        snap.counter("mc_meeting_kek_pushes_total")
            .with_labels(&[("outcome", "timed_out")])
            .assert_delta(0);
    }

    /// The wire value is the stored value — no conversion, so no way for the
    /// published W to differ from the enforced one (F-2).
    #[test]
    fn the_wire_w_and_the_timer_w_are_one_number() {
        let lifecycle = KekLifecycle::new(W_SECS);
        assert_eq!(lifecycle.window_seconds(), W_SECS);
        assert_eq!(lifecycle.window(), W);
        // Even at the top of the u32 range the two agree exactly.
        let max = KekLifecycle::new(u32::MAX);
        assert_eq!(max.window().as_secs(), u64::from(max.window_seconds()));
    }

    #[test]
    fn threshold_is_w_times_the_multiplier() {
        assert_eq!(KekLifecycle::new(W_SECS).overdue_threshold(), W * 2);
    }

    #[test]
    fn vocabularies_have_the_documented_values() {
        let t: Vec<_> = RotationTrigger::ALL.iter().map(|x| x.label()).collect();
        assert_eq!(
            t,
            [
                "meeting_created",
                "participant_left",
                "sender_space_exhausted"
            ]
        );
        let o: Vec<_> = KekPushOutcome::ALL.iter().map(|x| x.label()).collect();
        assert_eq!(
            o,
            [
                "delivered",
                "dropped_outbound",
                "participant_gone",
                "actor_unavailable",
                "timed_out"
            ]
        );
    }
}
