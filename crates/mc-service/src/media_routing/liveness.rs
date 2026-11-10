//! Per-(meeting, handler) heartbeat intake state (ADR-0039 §1, §2, §6).
//!
//! # What lives here, and what does not
//!
//! - [`HeartbeatTiming`]: N (`MC_MH_HEARTBEAT_TIMEOUT_MS`) and EVERYTHING
//!   derived from it — the interval pushed to MH, the intake's actor-send
//!   budget and the interval histogram's buckets. One derivation, so the two
//!   ends and MC's own telemetry cannot drift.
//! - [`HandlerLiveness`]: the record the meeting actor keeps per handler of its
//!   set, and the acceptance rule ([`HandlerLiveness::observe`]). MC-B2's
//!   `assess()` reads it; nothing here declares anything.
//! - [`retirable`]: THE remove-only retire predicate, called by BOTH stores a
//!   heartbeat reconciles (connectivity keys and the `HandedOutBindings`
//!   ledger), so the strict boundary cannot fork between them.
//! - The heartbeat metric vocabularies ([`HeartbeatOutcome`],
//!   [`HeartbeatAnomaly`]) and the domain verdict ([`HeartbeatVerdict`]) with
//!   its ONE projection to the wire.
//!
//! The verdict DECISION is not here: it is
//! `MeetingHandlers::heartbeat_verdict` (`placement.rs`), so membership has
//! one home.
//!
//! # A liveness entry never outlives its handler
//!
//! The meeting actor keeps these records in a map beside (never inside) the
//! handler set, because a liveness write must never move
//! `handler_set_version`. An entry is created only for a handler the set
//! resolves, and the actor's ONE removal helper (`remove_handler`, which is
//! the only production caller of `MeetingHandlers::remove`) drops the entry in
//! the same call. So a heartbeat can never add a handler, and a removed
//! handler's later heartbeat finds no entry to refresh.

use super::placement::RemovalTrigger;
use proto_gen::dark_tower::internal::v1::{
    ApplyRefusal, HeartbeatVerdict as WireVerdict, ReleaseCause, ServingState,
};
use std::num::NonZeroU64;
use std::time::Duration;
use tokio::time::Instant;

/// N and everything MC derives from it (ADR-0039 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatTiming {
    timeout_ms: u32,
}

impl HeartbeatTiming {
    /// Wrap N as validated by config (`(0, 30000]` ms).
    #[must_use]
    pub const fn from_timeout_ms(timeout_ms: u32) -> Self {
        Self { timeout_ms }
    }

    /// N.
    #[must_use]
    pub fn timeout(self) -> Duration {
        Duration::from_millis(u64::from(self.timeout_ms))
    }

    /// The send interval MC pushes to MH in
    /// `RegisterMeetingRequest.heartbeat_interval_ms`: `max(1, floor(N/3))` ms.
    ///
    /// Floor, so three beats fit inside N (`3 * interval <= N` for N >= 3 —
    /// the detection-safe side: a handler that missed two beats is still
    /// inside N when its third arrives). Below N = 3 the interval is 1 ms; the
    /// minimum exists because MH rejects 0 (the field's ORDERING rule).
    #[must_use]
    pub fn interval_ms(self) -> u32 {
        (self.timeout_ms / 3).max(1)
    }

    /// [`Self::interval_ms`] as a `Duration`.
    #[must_use]
    pub fn interval(self) -> Duration {
        Duration::from_millis(u64::from(self.interval_ms()))
    }

    /// How long the intake may wait to route a heartbeat INTO the meeting
    /// actor (controller lookup plus the mailbox send): half an interval,
    /// never below 1 ms. Less than one interval, so a backed-up actor costs
    /// one counted drop rather than an outstanding heartbeat overlapping the
    /// next (ADR-0039 §2). The reply wait is bounded by [`Self::interval`].
    #[must_use]
    pub fn send_budget(self) -> Duration {
        (self.interval() / 2).max(Duration::from_millis(1))
    }

    /// `mc_media_handler_heartbeat_interval_seconds` bucket bounds, derived
    /// from N: N/6, N/3, N/2, 2N/3, N, 1.5N, 2N (seconds). Around the healthy
    /// cadence (N/3), the half-way mark and the deadline.
    #[must_use]
    pub fn interval_histogram_buckets(self) -> Vec<f64> {
        let n = self.timeout().as_secs_f64();
        vec![
            n / 6.0,
            n / 3.0,
            n / 2.0,
            n * 2.0 / 3.0,
            n,
            n * 1.5,
            n * 2.0,
        ]
    }
}

/// `mc_media_handler_applied_sequence_lag` bucket bounds: integer generation
/// counts (the exporter's default buckets are seconds-shaped).
pub const APPLIED_SEQUENCE_LAG_BUCKETS: [f64; 8] = [0.0, 1.0, 2.0, 3.0, 5.0, 10.0, 25.0, 100.0];

/// A validated heartbeat, in MC's domain form. Built by the gRPC handler; the
/// wire enums arrive here already proved to be defined values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerHeartbeat {
    /// The self-asserted handler id (exact-match against the set).
    pub handler_id: String,
    /// SERVING or DRAINING (UNSPECIFIED was rejected at the boundary).
    pub serving_state: ServingState,
    /// Drain grace left; 0 unless DRAINING.
    pub drain_remaining_ms: u32,
    /// What MH has APPLIED for this pair.
    pub applied_generation: u64,
    /// Latest deterministic apply refusal (`Unspecified` = none).
    pub apply_refusal: ApplyRefusal,
    /// MH's begun-not-Disconnected connection ids (empty when
    /// `reconcile_skip` is set).
    pub connection_ids: Vec<String>,
    /// Set when the list was over MC's bound or held a malformed entry: the
    /// reconcile is SKIPPED for this heartbeat (security P-1) — skipping only
    /// keeps keys, so it fails safe — while the verdict and liveness still
    /// apply.
    pub reconcile_skip: Option<HeartbeatAnomaly>,
}

/// Whether a heartbeat counted as liveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// Accepted. `previous` is the receipt instant of the previously ACCEPTED
    /// heartbeat for this pair (`None` on the first): the reconcile's cut-off
    /// and the inter-arrival origin.
    Accepted {
        /// Previous accepted receipt, if any.
        previous: Option<Instant>,
    },
    /// `applied_generation` lower than the last accepted one: a stale or
    /// reordered heartbeat. Reads as lag, never as liveness, and changes
    /// NOTHING (no instant shift, no reconcile) — otherwise a replayed old
    /// list could retire a key bound after it was built.
    Stale,
}

/// What [`HandlerLiveness::observe`] reports to the actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observed {
    /// Liveness or not.
    pub acceptance: Acceptance,
    /// First accepted heartbeat for the pair, or the serving state or apply
    /// refusal changed: the ONLY heartbeats that are logged.
    pub transition: bool,
}

/// One handler's liveness as this meeting actor has observed it (ADR-0039 §2:
/// "last accepted entry time ... applied generation ... apply_refusal, serving
/// state"). Instants are tokio's and stamped at gRPC ARRIVAL, so mailbox delay
/// cannot age an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerLiveness {
    /// Receipt of the last accepted heartbeat.
    pub last_accepted: Instant,
    /// Receipt of the accepted heartbeat before that.
    pub previous_accepted: Option<Instant>,
    /// `applied_generation` of the last accepted heartbeat.
    pub applied_generation: u64,
    /// Latest `apply_refusal`.
    pub apply_refusal: ApplyRefusal,
    /// Latest serving state.
    pub serving_state: ServingState,
    /// Latest drain grace left (0 unless DRAINING).
    pub drain_remaining_ms: u32,
    /// The previous heartbeat's anomalies, so each WARN fires on transition
    /// only (never per heartbeat).
    last_anomalies: Vec<HeartbeatAnomaly>,
}

impl HandlerLiveness {
    /// The record a pair's first heartbeat creates (always accepted).
    #[must_use]
    pub fn first(heartbeat: &HandlerHeartbeat, receipt: Instant) -> Self {
        Self {
            last_accepted: receipt,
            previous_accepted: None,
            applied_generation: heartbeat.applied_generation,
            apply_refusal: heartbeat.apply_refusal,
            serving_state: heartbeat.serving_state,
            drain_remaining_ms: heartbeat.drain_remaining_ms,
            last_anomalies: Vec::new(),
        }
    }

    /// Record this heartbeat's anomalies (every reason, never just one).
    /// Returns those NOT present on the previous heartbeat — the only ones
    /// logged.
    pub fn note_anomalies(&mut self, anomalies: &[HeartbeatAnomaly]) -> Vec<HeartbeatAnomaly> {
        let fresh = anomalies
            .iter()
            .copied()
            .filter(|a| !self.last_anomalies.contains(a))
            .collect();
        self.last_anomalies = anomalies.to_vec();
        fresh
    }

    /// Apply a later heartbeat. Accepted iff its `applied_generation` is NOT
    /// LOWER than the last accepted one (equal is accepted: "not lower"); a
    /// stale one mutates nothing. This is NOT replay protection — an equal
    /// replay reads as live (ADR-0039 §2, S1b residual).
    pub fn observe(&mut self, heartbeat: &HandlerHeartbeat, receipt: Instant) -> Observed {
        if heartbeat.applied_generation < self.applied_generation {
            return Observed {
                acceptance: Acceptance::Stale,
                transition: false,
            };
        }
        let transition = heartbeat.serving_state != self.serving_state
            || heartbeat.apply_refusal != self.apply_refusal;
        let previous = self.last_accepted;
        self.previous_accepted = Some(previous);
        self.last_accepted = receipt;
        self.applied_generation = heartbeat.applied_generation;
        self.apply_refusal = heartbeat.apply_refusal;
        self.serving_state = heartbeat.serving_state;
        self.drain_remaining_ms = heartbeat.drain_remaining_ms;
        Observed {
            acceptance: Acceptance::Accepted {
                previous: Some(previous),
            },
            transition,
        }
    }
}

/// THE remove-only retire predicate (ADR-0039 §6), shared by the connectivity
/// keys and the `HandedOutBindings` ledger so their boundary cannot fork.
///
/// A key MC holds for a handler is retired by that handler's heartbeat iff it
/// is ABSENT from the heartbeat's `connection_ids` AND MC recorded its bind
/// STRICTLY BEFORE the previous accepted heartbeat's receipt. A bind recorded
/// at exactly that instant is KEPT.
///
/// Why "previous": per-meeting heartbeats arrive in order and MH lists a
/// connection from the moment it BEGINS `NotifyParticipantConnected`, so a
/// bind MC recorded before the previous heartbeat arrived was begun before the
/// CURRENT one was built — its absence now means MH sent (or lost) its
/// Disconnected.
///
/// Clock skew errs toward KEEPING: the bind instant is stamped in the actor
/// when MC records the answer (later than MH began the notify), and the
/// receipt at gRPC arrival (earlier than the actor sees it). Do not "fix" this
/// the other way — keeping a key costs a reconcile cycle; retiring a live one
/// silences a participant.
#[must_use]
pub fn retirable(recorded_at: Instant, before: Instant, present_in_heartbeat: bool) -> bool {
    !present_in_heartbeat && recorded_at < before
}

/// MC's domain verdict on one heartbeat. The DECISION is
/// `MeetingHandlers::heartbeat_verdict` (and, for `MeetingEnded`, the gRPC
/// router); [`Self::wire`] and [`Self::outcome`] are the ONE projection each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatVerdict {
    /// In the set (Registering or Active).
    Owned,
    /// In the meeting's removed record, with its removal trigger.
    Removed(RemovalTrigger),
    /// A live meeting whose set never held this handler: a misroute.
    Unassigned,
    /// No meeting actor (answered by the router; no tombstone).
    MeetingEnded,
}

impl HeartbeatVerdict {
    /// The wire `(verdict, cause)`. OWNED leaves `cause` unset.
    #[must_use]
    pub fn wire(self) -> (WireVerdict, ReleaseCause) {
        match self {
            Self::Owned => (WireVerdict::Owned, ReleaseCause::Unspecified),
            Self::Removed(trigger) => (WireVerdict::Released, trigger.release_cause()),
            Self::Unassigned => (WireVerdict::Released, ReleaseCause::Unspecified),
            Self::MeetingEnded => (WireVerdict::Released, ReleaseCause::MeetingEnded),
        }
    }

    /// The `mc_media_handler_heartbeats_total` outcome.
    #[must_use]
    pub fn outcome(self) -> HeartbeatOutcome {
        match self {
            Self::Owned => HeartbeatOutcome::Owned,
            Self::Removed(_) => HeartbeatOutcome::ReleasedRemoved,
            Self::Unassigned => HeartbeatOutcome::ReleasedUnassigned,
            Self::MeetingEnded => HeartbeatOutcome::ReleasedEnded,
        }
    }
}

/// `mc_media_handler_heartbeats_total{outcome}` (telemetry contract §MC). Not
/// the wire verdict vocabulary: RELEASED splits by cause, and `owner_mismatch`
/// is an error status. There is no UNKNOWN outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    /// OWNED.
    Owned,
    /// RELEASED(removal trigger): a zombie or a lost EndMeeting being fenced.
    ReleasedRemoved,
    /// RELEASED(MEETING_ENDED): the lost-EndMeeting retry rate.
    ReleasedEnded,
    /// RELEASED(UNSPECIFIED): misroute, expected zero.
    ReleasedUnassigned,
    /// `mc_id` is not this MC: FAILED_PRECONDITION, expected zero.
    OwnerMismatch,
}

impl HeartbeatOutcome {
    /// Every value, for zero-initialisation.
    pub const ALL: [Self; 5] = [
        Self::Owned,
        Self::ReleasedRemoved,
        Self::ReleasedEnded,
        Self::ReleasedUnassigned,
        Self::OwnerMismatch,
    ];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::ReleasedRemoved => "released_removed",
            Self::ReleasedEnded => "released_ended",
            Self::ReleasedUnassigned => "released_unassigned",
            Self::OwnerMismatch => "owner_mismatch",
        }
    }
}

/// `mc_media_handler_heartbeat_anomalies_total{reason}`: heartbeats MC
/// answered but could not fully use. Expected zero; non-zero is an MH
/// contract or version-skew fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatAnomaly {
    /// `connection_ids` longer than MC's bound: reconcile skipped.
    ConnectionIdsOverCap,
    /// A `connection_ids` entry empty or over-long: reconcile skipped.
    ConnectionIdMalformed,
    /// `applied_generation` above the generation MC last sent this pair (MH
    /// cannot apply what MC never sent): no lag observation.
    AppliedAheadOfSent,
}

impl HeartbeatAnomaly {
    /// Every value, for zero-initialisation.
    pub const ALL: [Self; 3] = [
        Self::ConnectionIdsOverCap,
        Self::ConnectionIdMalformed,
        Self::AppliedAheadOfSent,
    ];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ConnectionIdsOverCap => "connection_ids_over_cap",
            Self::ConnectionIdMalformed => "connection_id_malformed",
            Self::AppliedAheadOfSent => "applied_ahead_of_sent",
        }
    }
}

/// One heartbeat's applied-sequence lag reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LagReading {
    /// MC has never pushed to this pair: there is no "sent", so no reading
    /// (never a fake 0).
    Unobserved,
    /// Sent minus applied.
    Lag(u64),
    /// Applied above sent: a contract fault, counted, no reading.
    AheadOfSent,
}

/// Sent minus applied `policy_generation` for one heartbeat.
#[must_use]
pub fn applied_lag(sent: Option<NonZeroU64>, applied: u64) -> LagReading {
    match sent {
        None => LagReading::Unobserved,
        Some(sent) => sent
            .get()
            .checked_sub(applied)
            .map_or(LagReading::AheadOfSent, LagReading::Lag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn hb(applied: u64) -> HandlerHeartbeat {
        HandlerHeartbeat {
            handler_id: "mh-0".into(),
            serving_state: ServingState::Serving,
            drain_remaining_ms: 0,
            applied_generation: applied,
            apply_refusal: ApplyRefusal::Unspecified,
            connection_ids: Vec::new(),
            reconcile_skip: None,
        }
    }

    /// paired-test D: the derivation table, and the property it pins.
    #[test]
    fn interval_is_floor_third_with_a_one_ms_minimum() {
        for (n, i) in [
            (1, 1),
            (2, 1),
            (3, 1),
            (6000, 2000),
            (7000, 2333),
            (30_000, 10_000),
        ] {
            assert_eq!(
                HeartbeatTiming::from_timeout_ms(n).interval_ms(),
                i,
                "N={n}"
            );
        }
        for n in 3..=30_000u32 {
            let i = HeartbeatTiming::from_timeout_ms(n).interval_ms();
            assert!(i >= 1 && 3 * i <= n, "N={n}: 3*I <= N for N >= 3");
        }
        assert!(HeartbeatTiming::from_timeout_ms(1).interval_ms() >= 1);
    }

    #[test]
    fn send_budget_is_half_an_interval_never_zero() {
        assert_eq!(
            HeartbeatTiming::from_timeout_ms(6000).send_budget(),
            Duration::from_millis(1000)
        );
        assert_eq!(
            HeartbeatTiming::from_timeout_ms(1).send_budget(),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn interval_buckets_are_derived_from_n() {
        assert_eq!(
            HeartbeatTiming::from_timeout_ms(6000).interval_histogram_buckets(),
            vec![1.0, 2.0, 3.0, 4.0, 6.0, 9.0, 12.0]
        );
    }

    /// The ONE boundary, pinned once: strict, equal is kept.
    #[test]
    fn retirable_is_strictly_before_and_absent() {
        let t = Instant::now();
        let before = t + Duration::from_millis(10);
        assert!(retirable(t, before, false), "earlier and absent: retired");
        assert!(!retirable(before, before, false), "equal instant: kept");
        assert!(!retirable(before + Duration::from_millis(1), before, false));
        assert!(!retirable(t, before, true), "present: kept");
    }

    #[test]
    fn a_lower_generation_is_stale_and_changes_nothing() {
        let t0 = Instant::now();
        let mut l = HandlerLiveness::first(&hb(5), t0);
        let snapshot = l.clone();
        let o = l.observe(&hb(4), t0 + Duration::from_secs(1));
        assert_eq!(o.acceptance, Acceptance::Stale);
        assert_eq!(l, snapshot, "a stale heartbeat mutates nothing");
        let t2 = t0 + Duration::from_secs(2);
        let o = l.observe(&hb(5), t2);
        assert_eq!(
            o.acceptance,
            Acceptance::Accepted { previous: Some(t0) },
            "equal is not lower"
        );
        assert_eq!(l.previous_accepted, Some(t0));
        assert_eq!(l.last_accepted, t2);
        assert!(!o.transition);
        let mut draining = hb(6);
        draining.serving_state = ServingState::Draining;
        assert!(l.observe(&draining, t2 + Duration::from_secs(1)).transition);
    }

    #[test]
    fn anomalies_are_logged_on_transition_only_and_never_masked() {
        use HeartbeatAnomaly::{AppliedAheadOfSent as Ahead, ConnectionIdsOverCap as OverCap};
        let mut l = HandlerLiveness::first(&hb(1), Instant::now());
        assert_eq!(
            l.note_anomalies(&[OverCap, Ahead]),
            vec![OverCap, Ahead],
            "both fresh"
        );
        assert!(
            l.note_anomalies(&[OverCap, Ahead]).is_empty(),
            "unchanged: silent"
        );
        assert_eq!(l.note_anomalies(&[Ahead]), Vec::<HeartbeatAnomaly>::new());
        assert!(l.note_anomalies(&[]).is_empty());
        assert_eq!(
            l.note_anomalies(&[Ahead]),
            vec![Ahead],
            "returns after clearing"
        );
    }

    #[test]
    fn lag_is_unobserved_without_a_push_and_ahead_is_a_fault() {
        assert_eq!(applied_lag(None, 0), LagReading::Unobserved);
        assert_eq!(applied_lag(NonZeroU64::new(7), 4), LagReading::Lag(3));
        assert_eq!(applied_lag(NonZeroU64::new(7), 7), LagReading::Lag(0));
        assert_eq!(applied_lag(NonZeroU64::new(7), 8), LagReading::AheadOfSent);
    }

    #[test]
    fn verdict_projection_is_total_and_owned_leaves_cause_unset() {
        assert_eq!(
            HeartbeatVerdict::Owned.wire(),
            (WireVerdict::Owned, ReleaseCause::Unspecified)
        );
        assert_eq!(
            HeartbeatVerdict::Removed(RemovalTrigger::ApplyLag).wire(),
            (WireVerdict::Released, ReleaseCause::ApplyLag)
        );
        assert_eq!(
            HeartbeatVerdict::Unassigned.wire(),
            (WireVerdict::Released, ReleaseCause::Unspecified)
        );
        assert_eq!(
            HeartbeatVerdict::MeetingEnded.wire(),
            (WireVerdict::Released, ReleaseCause::MeetingEnded)
        );
    }

    #[test]
    fn vocabularies_are_complete_and_distinct() {
        let o: BTreeSet<_> = HeartbeatOutcome::ALL.iter().map(|x| x.label()).collect();
        assert_eq!(o.len(), HeartbeatOutcome::ALL.len());
        let a: BTreeSet<_> = HeartbeatAnomaly::ALL.iter().map(|x| x.label()).collect();
        assert_eq!(a.len(), HeartbeatAnomaly::ALL.len());
        // Every variant is in ALL (exhaustive match: a new variant fails here).
        for x in HeartbeatOutcome::ALL {
            match x {
                HeartbeatOutcome::Owned
                | HeartbeatOutcome::ReleasedRemoved
                | HeartbeatOutcome::ReleasedEnded
                | HeartbeatOutcome::ReleasedUnassigned
                | HeartbeatOutcome::OwnerMismatch => {}
            }
        }
    }
}
