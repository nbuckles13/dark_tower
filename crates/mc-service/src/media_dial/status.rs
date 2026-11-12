//! Client-reported handler status, keyed by `handler_ref` (ADR-0039 §4, §5).
//!
//! The latest `MhConnectionStatus` per (participant, `handler_ref`) received
//! while that ref was in the set, plus the FAILED fast re-push limiter.
//!
//! # Why a second map, not a merge
//!
//! MC already holds one connectivity store, and this is not it:
//! `media_routing/connectivity.rs` is SERVER-observed truth (MH-reported, the
//! input to routing). This store is client-ASSERTED and DIAGNOSTIC ONLY:
//! it is NEVER read by routing, edges, slots or placement. Its consumers are
//! the client-loss evidence (story 4 MC-E) and the credential re-push below.
//!
//! # Resolution
//!
//! A report's ref is resolved by EXACT match against the meeting's CURRENT
//! Active set (the set every participant was last told). A ref outside it
//! (removed, never issued) is ignored and creates no entry. The reported
//! `handler_set_version` is stored for correlation only — never a discard
//! criterion, so a stale version on an in-set ref is still recorded. Entries
//! for refs that leave the set are pruned, so the store is bounded by the set.
//!
//! # FAILED re-push
//!
//! A FAILED report for an in-set ref asks for an immediate re-push of THAT
//! participant's set (a fresh credential; the client parks the ref after a
//! `DECLINED` close until it sees a newer token). Rate-limited per session by
//! a [`ClientWorkLimiter`] spent BEFORE any minting; an over-rate or
//! out-of-set report causes nothing.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use crate::client_work_limiter::ClientWorkLimiter;

/// FAILED re-pushes a session may trigger at once.
///
/// A real client reports FAILED once per `DECLINED` ref per set it receives,
/// so a handful covers every handler in a set failing together.
pub const FAILED_REPUSH_BURST: u32 = 4;

/// One FAILED re-push token is earned per this many milliseconds.
pub const FAILED_REPUSH_REFILL_MS: u64 = 2_000;

/// A client-reported connection state that is recorded. `UNSPECIFIED` and
/// unknown wire values are dropped at the connection and never get here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportedState {
    /// `CONNECTION_STATE_CONNECTED`.
    Connected,
    /// `CONNECTION_STATE_FAILED`.
    Failed,
    /// `CONNECTION_STATE_DISCONNECTED`.
    Disconnected,
}

/// One client report for one ref, as the connection forwards it. No
/// client-controlled strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientHandlerReport {
    /// The ref the client says it dialed (non-zero; 0 is never a member).
    pub handler_ref: NonZeroU32,
    /// The set version the client held (correlation only).
    pub handler_set_version: u64,
    /// The reported state.
    pub state: ReportedState,
}

/// The latest recorded report for one (participant, ref).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatestReport {
    /// The reported state.
    pub state: ReportedState,
    /// The set version the client held (correlation only).
    pub handler_set_version: u64,
}

/// What a batch of reports did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Intake {
    /// Reports recorded (in-set refs).
    pub recorded: usize,
    /// Reports ignored (out-of-set refs).
    pub ignored: usize,
    /// The batch asked for a FAILED re-push.
    pub repush: Repush,
}

/// The FAILED re-push decision for a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repush {
    /// No in-set FAILED in the batch.
    #[default]
    NotRequested,
    /// Granted: re-push this participant's set.
    Granted,
    /// Over the session's rate: nothing happens.
    RateLimited,
}

#[derive(Debug)]
struct ParticipantReports {
    latest: BTreeMap<NonZeroU32, LatestReport>,
    failed_repush_limiter: ClientWorkLimiter,
    rate_limit_logged: bool,
}

/// The per-meeting store (meeting-actor state).
#[derive(Debug, Default)]
pub struct ClientHandlerStatuses {
    by_participant: BTreeMap<String, ParticipantReports>,
}

impl ClientHandlerStatuses {
    /// Record a batch from `participant` (the connection's own identity).
    ///
    /// `in_set` is the exact-match membership test against the current set.
    /// `now` drives the re-push limiter.
    pub fn intake(
        &mut self,
        participant: &str,
        reports: &[ClientHandlerReport],
        in_set: impl Fn(NonZeroU32) -> bool,
        now: std::time::Instant,
    ) -> Intake {
        let entry = self
            .by_participant
            .entry(participant.to_string())
            .or_insert_with(|| ParticipantReports {
                latest: BTreeMap::new(),
                failed_repush_limiter: ClientWorkLimiter::new(
                    now,
                    FAILED_REPUSH_BURST,
                    FAILED_REPUSH_REFILL_MS,
                ),
                rate_limit_logged: false,
            });
        let mut intake = Intake::default();
        let mut failed_in_set = false;
        for report in reports {
            if !in_set(report.handler_ref) {
                intake.ignored += 1;
                continue;
            }
            entry.latest.insert(
                report.handler_ref,
                LatestReport {
                    state: report.state,
                    handler_set_version: report.handler_set_version,
                },
            );
            intake.recorded += 1;
            failed_in_set |= report.state == ReportedState::Failed;
        }
        if failed_in_set {
            intake.repush = if entry.failed_repush_limiter.try_spend(now) {
                Repush::Granted
            } else {
                Repush::RateLimited
            };
        }
        intake
    }

    /// Whether the rate-limit line for `participant` is still to be logged
    /// (once per session); marks it logged.
    pub fn first_rate_limit(&mut self, participant: &str) -> bool {
        self.by_participant
            .get_mut(participant)
            .is_some_and(|p| !std::mem::replace(&mut p.rate_limit_logged, true))
    }

    /// Prune every entry whose ref is no longer in the set.
    pub fn retain_refs(&mut self, in_set: impl Fn(NonZeroU32) -> bool) {
        for reports in self.by_participant.values_mut() {
            reports.latest.retain(|r, _| in_set(*r));
        }
    }

    /// The participant left: drop its reports and its limiter.
    pub fn forget(&mut self, participant: &str) {
        self.by_participant.remove(participant);
    }

    /// The latest report per in-set ref for `participant`.
    #[must_use]
    pub fn latest(&self, participant: &str) -> BTreeMap<NonZeroU32, LatestReport> {
        self.by_participant
            .get(participant)
            .map(|p| p.latest.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn r(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn report(n: u32, version: u64, state: ReportedState) -> ClientHandlerReport {
        ClientHandlerReport {
            handler_ref: r(n),
            handler_set_version: version,
            state,
        }
    }

    #[test]
    fn latest_per_ref_supersedes_and_stale_versions_still_record() {
        let mut s = ClientHandlerStatuses::default();
        let now = Instant::now();
        let in_set = |x: NonZeroU32| x.get() <= 2;
        s.intake(
            "p",
            &[report(1, 5, ReportedState::Disconnected)],
            in_set,
            now,
        );
        s.intake("p", &[report(1, 3, ReportedState::Connected)], in_set, now);
        let latest = s.latest("p");
        assert_eq!(latest[&r(1)].state, ReportedState::Connected);
        assert_eq!(
            latest[&r(1)].handler_set_version,
            3,
            "version never discards"
        );
    }

    #[test]
    fn out_of_set_refs_are_ignored_and_pruned_refs_disappear() {
        let mut s = ClientHandlerStatuses::default();
        let now = Instant::now();
        let intake = s.intake(
            "p",
            &[
                report(1, 1, ReportedState::Connected),
                report(9, 1, ReportedState::Failed),
            ],
            |x| x.get() == 1,
            now,
        );
        assert_eq!((intake.recorded, intake.ignored), (1, 1));
        assert_eq!(
            intake.repush,
            Repush::NotRequested,
            "an out-of-set FAILED spends nothing"
        );
        assert!(!s.latest("p").contains_key(&r(9)));

        s.retain_refs(|_| false);
        assert!(s.latest("p").is_empty());
        let after = s.intake(
            "p",
            &[report(1, 1, ReportedState::Connected)],
            |_| false,
            now,
        );
        assert_eq!(after.recorded, 0);
        assert!(s.latest("p").is_empty());
    }

    #[test]
    fn failed_repush_is_limited_per_session_and_refills() {
        let mut s = ClientHandlerStatuses::default();
        let now = Instant::now();
        let failed = [report(1, 1, ReportedState::Failed)];
        let in_set = |x: NonZeroU32| x.get() == 1;
        for _ in 0..FAILED_REPUSH_BURST {
            assert_eq!(s.intake("p1", &failed, in_set, now).repush, Repush::Granted);
        }
        assert_eq!(
            s.intake("p1", &failed, in_set, now).repush,
            Repush::RateLimited
        );
        assert!(s.first_rate_limit("p1"));
        assert!(!s.first_rate_limit("p1"), "logged once per session");
        // Isolation: another session is unaffected.
        assert_eq!(s.intake("p2", &failed, in_set, now).repush, Repush::Granted);
        // Refill.
        let later = now + Duration::from_millis(FAILED_REPUSH_REFILL_MS);
        assert_eq!(
            s.intake("p1", &failed, in_set, later).repush,
            Repush::Granted
        );

        s.forget("p1");
        assert!(s.latest("p1").is_empty());
    }
}
