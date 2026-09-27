//! Sizing for scenario S9 (MH egress-budget exhaustion,
//! `tests/28_mh_egress_admission.rs`) — ONE home for both the test that runs
//! it and the deployment-config test that bounds the Kind budget against it
//! (`tests/01_mh_deployment_config.rs`). The two are separate test binaries,
//! so without this module the upper bound in 01 would rest on a COPY of
//! [`MAX_S9_PARTICIPANTS`].
//!
//! Two different kinds of bound live here and are kept textually apart:
//!
//! - [`MAX_S9_PARTICIPANTS`] is an ECONOMIC feasibility limit — how many real
//!   AC registrations and GC/MC/MH joins one test may spend.
//! - [`size_s9_meeting`]'s `p * s > 2 * ceiling` is the CORRECTNESS bound —
//!   the pigeonhole condition that makes exhaustion certain for ANY edge
//!   chooser MC uses across two handlers.

/// The largest participant count S9 will spend before calling the deployment
/// unsuitable (a ceiling S9 cannot exceed economically). An economic limit,
/// NOT a correctness bound: raising it buys a larger feasible Kind ceiling at
/// the cost of that many more registrations and joins in one test.
pub const MAX_S9_PARTICIPANTS: u64 = 40;

/// Smallest `p` (with its per-participant slot count `s = min(p - 1,
/// slot_cap)`) satisfying the pigeonhole condition `p * s > 2 * ceiling`
/// within [`MAX_S9_PARTICIPANTS`]; `None` when the deployed ceiling is too
/// large for S9 to exceed at any affordable size.
#[must_use]
pub fn size_s9_meeting(ceiling: u64, slot_cap: u64) -> Option<(u64, u64)> {
    (2..=MAX_S9_PARTICIPANTS)
        .map(|p| (p, (p - 1).min(slot_cap)))
        .find(|(p, s)| p * s > 2 * ceiling)
}

/// The largest stream ceiling S9 can still exceed, for `slot_cap`: the
/// deployment-config test's UPPER bound on the Kind budget.
#[must_use]
pub fn max_s9_feasible_ceiling(slot_cap: u64) -> u64 {
    let most_streams = MAX_S9_PARTICIPANTS * (MAX_S9_PARTICIPANTS - 1).min(slot_cap);
    // p*s > 2*ceiling  <=>  ceiling < p*s / 2  <=>  ceiling <= (p*s - 1) / 2.
    most_streams.saturating_sub(1) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_sizing_matches_the_recorded_figures() {
        // Kind: ceiling 100 (250M / 2.5M), MC_MAX_RECEIVE_SLOTS 8.
        assert_eq!(size_s9_meeting(100, 8), Some((26, 8)));
        // The pre-story-2-task-10 budget, for the record.
        assert_eq!(size_s9_meeting(40, 8), Some((11, 8)));
    }

    #[test]
    fn the_feasible_ceiling_is_exactly_the_last_one_size_can_satisfy() {
        let edge = max_s9_feasible_ceiling(8);
        assert_eq!(edge, 159);
        assert!(size_s9_meeting(edge, 8).is_some());
        assert!(size_s9_meeting(edge + 1, 8).is_none());
    }
}
