//! Egress-stream admission state owned by the session actor (story 2 R-19;
//! ADR-0036 §11 "Admission control is keyed on egress bandwidth, not
//! connection count").
//!
//! Pure data and pure functions: the actor owns one [`AdmissionWindow`] and one
//! [`ThresholdLatch`], feeds them decisions and ticks, and publishes what they
//! compute. Nothing here touches a clock, a metric or a log, so every property
//! the gauge and the log depend on is unit-testable.
//!
//! # The rejection ratio is WINDOWED, never a lifetime ratio
//!
//! A lifetime `rejected / total` is silent during total exhaustion on a
//! long-healthy pod (the history dilutes it) and can never clear on a fresh one
//! (one early rejection latches it). The window is
//! [`ADMISSION_WINDOW_BUCKETS`] × [`ADMISSION_WINDOW_BUCKET`] — five minutes —
//! and the actor rotates it on a timer, so the ratio CLEARS when decisions stop
//! rather than freezing at its last value.
//!
//! Scrape interaction, stated so it is not rediscovered: MH is scraped every
//! 15 s and a bucket is 10 s, so a sub-30 s excursion can be invisible on the
//! gauge. The ratio is a level, so undersampling loses detail but cannot
//! corrupt a value. Neither interval is tuned to the other; a test asserting
//! rejection must read `mh_media_stream_admission_total`, not a transient ratio.

use std::time::Duration;

use crate::config::EgressAdmission;
use crate::observability::metrics::StreamAdmissionOutcome;

/// Width of one window bucket, and the actor's rotation tick.
pub const ADMISSION_WINDOW_BUCKET: Duration = Duration::from_secs(10);

/// Buckets in the sliding window: 30 × 10 s = 5 minutes.
pub const ADMISSION_WINDOW_BUCKETS: usize = 30;

/// Evidence floor for the threshold WARN: fewer decisions than this in the
/// window never trip it.
///
/// A DIFFERENT rule from the 0/0 → 0.0 gauge convention. The gauge must be
/// honest about every window, including a window holding one rejected apply
/// (ratio 1.0). The WARN needs enough evidence that it is not one rejected
/// registration on an otherwise idle handler.
pub const MIN_DECISIONS_FOR_THRESHOLD_LOG: u64 = 10;

/// What the session actor enforces and reads, taken from the ONE
/// [`EgressAdmission`] computed at config load.
///
/// A handle cannot be built without one, so "a session manager with no stream
/// ceiling" is not a representable state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamAdmission {
    /// The derived egress STREAM ceiling — installed egress streams across
    /// every meeting on this handler may not exceed it.
    pub stream_ceiling: usize,
    /// `MH_EGRESS_REJECTION_RATIO_THRESHOLD`, read by the threshold log.
    pub rejection_ratio_threshold: f64,
}

impl From<&EgressAdmission> for StreamAdmission {
    fn from(admission: &EgressAdmission) -> Self {
        Self {
            // u32 -> usize: lossless on every supported (>= 32-bit) target.
            stream_ceiling: usize::try_from(admission.stream_ceiling).unwrap_or(usize::MAX),
            rejection_ratio_threshold: admission.rejection_ratio_threshold,
        }
    }
}

/// A never-binding admission for `src/` unit tests that are not ABOUT
/// admission: the ceiling is the largest the config can ever produce, so no
/// test fixture trips it by accident. Mirrors
/// `mh_test_utils::admission::never_binding` (the `tests/` twin; `src/` unit
/// tests cannot share it — through the dev-dependency they would see a
/// DIFFERENT `mh_service` crate instance and so a different type).
#[cfg(test)]
pub(crate) fn test_admission() -> StreamAdmission {
    StreamAdmission {
        stream_ceiling: crate::config::MAX_TOTAL_EGRESS_EDGES_CEILING,
        rejection_ratio_threshold: 0.05,
    }
}

/// Decision counts for one bucket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Bucket {
    admitted: u64,
    rejected: u64,
}

/// A fixed-size ring of per-bucket admission counts: the sliding window the
/// rejection ratio is computed over. Bounded memory, no allocation after
/// construction.
#[derive(Debug, Clone)]
pub struct AdmissionWindow {
    buckets: [Bucket; ADMISSION_WINDOW_BUCKETS],
    current: usize,
}

impl Default for AdmissionWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl AdmissionWindow {
    /// An empty window.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buckets: [Bucket {
                admitted: 0,
                rejected: 0,
            }; ADMISSION_WINDOW_BUCKETS],
            current: 0,
        }
    }

    /// Count one decision in the current bucket.
    ///
    /// "Fail loudly, degrade safely", split the same way as
    /// `RoutingSnapshot::projected_total_edges`: `current` is in range by
    /// construction (`new` sets 0, `rotate` wraps with `%`), so the `get_mut`
    /// `None` arm is unreachable — but if that ever stopped holding, `get_mut`
    /// alone would SILENTLY drop a decision and bias the ratio the exhaustion
    /// alert reads. The `debug_assert!` makes that break loud in every test run;
    /// `get_mut` keeps release from panicking (ADR-0002).
    pub fn record(&mut self, outcome: StreamAdmissionOutcome) {
        debug_assert!(
            self.current < ADMISSION_WINDOW_BUCKETS,
            "admission window cursor {} out of range 0..{ADMISSION_WINDOW_BUCKETS}",
            self.current
        );
        if let Some(bucket) = self.buckets.get_mut(self.current) {
            match outcome {
                StreamAdmissionOutcome::Admitted => {
                    bucket.admitted = bucket.admitted.saturating_add(1);
                }
                StreamAdmissionOutcome::RejectedStreamCeiling => {
                    bucket.rejected = bucket.rejected.saturating_add(1);
                }
            }
        }
    }

    /// Advance one bucket, discarding the oldest. Same loud-in-test,
    /// safe-in-release split as [`Self::record`]: a bucket that silently failed
    /// to clear would keep aged-out decisions in the window.
    pub fn rotate(&mut self) {
        self.current = (self.current + 1) % ADMISSION_WINDOW_BUCKETS;
        debug_assert!(
            self.current < ADMISSION_WINDOW_BUCKETS,
            "admission window cursor {} out of range 0..{ADMISSION_WINDOW_BUCKETS}",
            self.current
        );
        if let Some(bucket) = self.buckets.get_mut(self.current) {
            *bucket = Bucket::default();
        }
    }

    fn totals(&self) -> (u64, u64) {
        self.buckets.iter().fold((0, 0), |(a, r), b| {
            (a.saturating_add(b.admitted), r.saturating_add(b.rejected))
        })
    }

    /// Decisions (admitted + rejected) in the window.
    #[must_use]
    pub fn decisions(&self) -> u64 {
        let (admitted, rejected) = self.totals();
        admitted.saturating_add(rejected)
    }

    /// The windowed rejection ratio, in `0.0..=1.0`. **Never NaN.**
    ///
    /// - No decisions in the window: **0.0**, a value the exhaustion alert
    ///   never fires on (a code-computed ratio gets none of `PromQL`'s 0/0
    ///   protection, so this is decided here, not inherited).
    /// - Zero admitted with any rejected: **1.0**, never 0.0 — otherwise the
    ///   gauge is quietest exactly when exhaustion is total.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "decision counts in a five-minute window are far below 2^53"
    )]
    pub fn ratio(&self) -> f64 {
        let (admitted, rejected) = self.totals();
        let total = admitted.saturating_add(rejected);
        if total == 0 {
            return 0.0;
        }
        rejected as f64 / total as f64
    }
}

/// A threshold crossing the actor logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdCrossing {
    /// The windowed ratio rose above the threshold, with enough evidence.
    Exceeded,
    /// The ratio fell back to at most half the threshold, or the window
    /// emptied.
    Recovered,
}

/// Edge-triggered, hysteretic reader of the rejection-ratio threshold.
///
/// A debugging BREADCRUMB in MH's own log, not a pager: the Prometheus alert
/// (with its `for:` and denominator guard) is authoritative, and the two may
/// legitimately disagree at the edges.
///
/// - Trips (once) only when `ratio > threshold` AND the window holds at least
///   [`MIN_DECISIONS_FOR_THRESHOLD_LOG`] decisions.
/// - Re-arms (once) only when the ratio falls to `<= threshold / 2` or the
///   window empties — so a ratio hovering at the threshold does not produce a
///   WARN/INFO pair every tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThresholdLatch {
    tripped: bool,
}

impl ThresholdLatch {
    /// Feed the current window state; returns a crossing to log, if any.
    pub fn observe(
        &mut self,
        ratio: f64,
        decisions: u64,
        threshold: f64,
    ) -> Option<ThresholdCrossing> {
        if self.tripped {
            if decisions == 0 || ratio <= threshold / 2.0 {
                self.tripped = false;
                return Some(ThresholdCrossing::Recovered);
            }
        } else if decisions >= MIN_DECISIONS_FOR_THRESHOLD_LOG && ratio > threshold {
            self.tripped = true;
            return Some(ThresholdCrossing::Exceeded);
        }
        None
    }
}

#[cfg(test)]
// Every float compared here is an exact ratio of small integers (0/n, 1/1,
// 1/4) or an exact literal handed straight through — not an accumulated
// computation where precision loss could hide a bug — so exact equality is the
// correct check. Same reasoning as the `config.rs` test module.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn fill(window: &mut AdmissionWindow, admitted: u64, rejected: u64) {
        for _ in 0..admitted {
            window.record(StreamAdmissionOutcome::Admitted);
        }
        for _ in 0..rejected {
            window.record(StreamAdmissionOutcome::RejectedStreamCeiling);
        }
    }

    #[test]
    fn an_empty_window_publishes_zero_never_nan() {
        let window = AdmissionWindow::new();
        assert_eq!(window.ratio(), 0.0);
        assert!(!window.ratio().is_nan());
        assert_eq!(window.decisions(), 0);
    }

    #[test]
    fn all_rejected_publishes_one_never_zero() {
        let mut window = AdmissionWindow::new();
        fill(&mut window, 0, 3);
        assert_eq!(window.ratio(), 1.0);
    }

    #[test]
    fn ratio_is_rejected_over_all_decisions() {
        let mut window = AdmissionWindow::new();
        fill(&mut window, 3, 1);
        assert_eq!(window.ratio(), 0.25);
        assert_eq!(window.decisions(), 4);
    }

    #[test]
    fn a_full_rotation_clears_the_window() {
        let mut window = AdmissionWindow::new();
        fill(&mut window, 0, 5);
        for _ in 0..ADMISSION_WINDOW_BUCKETS - 1 {
            window.rotate();
            assert_eq!(window.ratio(), 1.0, "still inside the window");
        }
        window.rotate();
        assert_eq!(
            window.ratio(),
            0.0,
            "aged out: the gauge clears, never latches"
        );
        assert_eq!(window.decisions(), 0);
    }

    #[test]
    fn the_window_is_five_minutes() {
        assert_eq!(
            ADMISSION_WINDOW_BUCKET * u32::try_from(ADMISSION_WINDOW_BUCKETS).unwrap_or(0),
            Duration::from_mins(5)
        );
    }

    #[test]
    fn the_latch_needs_evidence_before_it_warns() {
        let mut latch = ThresholdLatch::default();
        // One rejection on an idle handler: ratio 1.0, but not enough evidence.
        assert_eq!(latch.observe(1.0, 1, 0.05), None);
        assert_eq!(
            latch.observe(1.0, MIN_DECISIONS_FOR_THRESHOLD_LOG, 0.05),
            Some(ThresholdCrossing::Exceeded)
        );
    }

    #[test]
    fn the_latch_is_edge_triggered_with_hysteresis() {
        let mut latch = ThresholdLatch::default();
        let n = MIN_DECISIONS_FOR_THRESHOLD_LOG;
        assert_eq!(
            latch.observe(0.2, n, 0.1),
            Some(ThresholdCrossing::Exceeded)
        );
        assert_eq!(latch.observe(0.3, n, 0.1), None, "already tripped: once");
        assert_eq!(
            latch.observe(0.09, n, 0.1),
            None,
            "below threshold but above half"
        );
        assert_eq!(
            latch.observe(0.2, n, 0.1),
            None,
            "hovering does not re-fire"
        );
        assert_eq!(
            latch.observe(0.05, n, 0.1),
            Some(ThresholdCrossing::Recovered)
        );
        assert_eq!(latch.observe(0.05, n, 0.1), None);
    }

    #[test]
    fn an_emptied_window_re_arms_the_latch() {
        let mut latch = ThresholdLatch::default();
        let n = MIN_DECISIONS_FOR_THRESHOLD_LOG;
        assert_eq!(
            latch.observe(1.0, n, 0.5),
            Some(ThresholdCrossing::Exceeded)
        );
        assert_eq!(
            latch.observe(0.0, 0, 0.5),
            Some(ThresholdCrossing::Recovered)
        );
    }
}
