//! The ONE weighted-random load-balancing kernel (MC assignment and MH
//! selection both pick through it), plus its random-draw seam.
//!
//! Weight is inversely proportional to load ratio (`1 − min(load, 0.99)`):
//! an empty instance weighs 1.0, a 90%-loaded one 0.1. Randomness spreads a
//! burst of concurrent picks across instances instead of herding them onto the
//! single least-loaded one (ADR-0039 §3: the spread is what absorbs a burst of
//! replacements from many MCs).
//!
//! # Security
//!
//! Production draws come from a CSPRNG (`ring::rand::SystemRandom`) through
//! [`SystemSelectionRng`]. The [`SelectionRng`] trait exists because ring's
//! `SecureRandom` is sealed: tests need a deterministic source to prove that
//! selection depends only on its inputs (ADR-0039 §9, seeded RNG).

use ring::rand::{SecureRandom, SystemRandom};

/// A source of uniform 64-bit draws for [`weighted_random_select`].
///
/// `pub` (not `pub(crate)`) only because integration tests inject a seeded
/// implementation through `McService::with_selection_rng`. Production uses
/// [`SystemSelectionRng`] everywhere.
pub trait SelectionRng: Send + Sync {
    /// One uniform draw, or `None` if the source failed (the kernel then
    /// falls back to the first, i.e. least-loaded, candidate).
    fn next_u64(&self) -> Option<u64>;
}

/// The production draw source: the system CSPRNG.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSelectionRng;

impl SelectionRng for SystemSelectionRng {
    fn next_u64(&self) -> Option<u64> {
        let mut bytes = [0u8; 8];
        SystemRandom::new().fill(&mut bytes).ok()?;
        Some(u64::from_le_bytes(bytes))
    }
}

/// Pick one candidate, weighted by `1 − min(load_ratio, 0.99)`.
///
/// `None` only for an empty slice. A single candidate is returned without a
/// draw. A failed draw falls back to the first candidate (callers pass
/// candidates least-loaded first) and logs a warning.
pub(crate) fn weighted_random_select<'a, T>(
    candidates: &'a [T],
    load_ratio: impl Fn(&T) -> f64,
    rng: &dyn SelectionRng,
) -> Option<&'a T> {
    if candidates.len() <= 1 {
        return candidates.first();
    }

    let weights: Vec<f64> = candidates
        .iter()
        .map(|c| 1.0 - load_ratio(c).min(0.99))
        .collect();

    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return candidates.first();
    }

    let Some(random_u64) = rng.next_u64() else {
        tracing::warn!(
            target: "gc.service.weighted_pick",
            "CSPRNG failed, falling back to first candidate"
        );
        return candidates.first();
    };

    // Scale to [0, 1], then walk the cumulative weights.
    let random_f64 = (random_u64 as f64) / (u64::MAX as f64);
    let mut choice = random_f64 * total;
    for (i, weight) in weights.iter().enumerate() {
        choice -= weight;
        if choice <= 0.0 {
            return candidates.get(i);
        }
    }

    // Floating-point edge case.
    candidates.last()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// (id, load) pairs; the kernel is generic, so tests need no domain type.
    type C = (&'static str, f64);

    fn load(c: &C) -> f64 {
        c.1
    }

    /// Replays a fixed sequence of draws.
    struct Scripted(Mutex<Vec<Option<u64>>>);

    impl SelectionRng for Scripted {
        fn next_u64(&self) -> Option<u64> {
            self.0.lock().unwrap().pop().flatten()
        }
    }

    fn scripted(draw: Option<u64>) -> Scripted {
        Scripted(Mutex::new(vec![draw]))
    }

    #[test]
    fn empty_returns_none() {
        let candidates: Vec<C> = vec![];
        assert!(weighted_random_select(&candidates, load, &SystemSelectionRng).is_none());
    }

    #[test]
    fn single_returns_it_without_a_draw() {
        let candidates: Vec<C> = vec![("a", 0.5)];
        // A failing source would fall back too, so prove no draw happens by
        // giving a source that panics if consulted.
        struct Never;
        impl SelectionRng for Never {
            fn next_u64(&self) -> Option<u64> {
                panic!("single candidate must not draw");
            }
        }
        assert_eq!(
            weighted_random_select(&candidates, load, &Never).unwrap().0,
            "a"
        );
    }

    #[test]
    fn draw_maps_onto_cumulative_weights() {
        // Weights 1.0 and 0.5 (total 1.5): a draw near 0 lands on the first,
        // a draw at the top lands on the second.
        let candidates: Vec<C> = vec![("a", 0.0), ("b", 0.5)];
        let low = weighted_random_select(&candidates, load, &scripted(Some(0))).unwrap();
        let high = weighted_random_select(&candidates, load, &scripted(Some(u64::MAX))).unwrap();
        assert_eq!(low.0, "a");
        assert_eq!(high.0, "b");
    }

    #[test]
    fn failed_draw_falls_back_to_first() {
        let candidates: Vec<C> = vec![("a", 0.9), ("b", 0.0)];
        let picked = weighted_random_select(&candidates, load, &scripted(None)).unwrap();
        assert_eq!(picked.0, "a");
    }

    #[test]
    fn same_draw_same_pick() {
        let candidates: Vec<C> = vec![("a", 0.1), ("b", 0.4), ("c", 0.7)];
        for draw in [1u64, u64::MAX / 3, u64::MAX / 2, u64::MAX - 7] {
            let first = weighted_random_select(&candidates, load, &scripted(Some(draw))).unwrap();
            let second = weighted_random_select(&candidates, load, &scripted(Some(draw))).unwrap();
            assert_eq!(first.0, second.0, "draw {draw}");
        }
    }

    #[test]
    fn system_rng_always_returns_a_member() {
        let candidates: Vec<C> = vec![("a", 0.1), ("b", 0.9)];
        for _ in 0..100 {
            let id = weighted_random_select(&candidates, load, &SystemSelectionRng)
                .unwrap()
                .0;
            assert!(id == "a" || id == "b");
        }
    }

    #[test]
    fn system_rng_prefers_lower_load() {
        let candidates: Vec<C> = vec![("light", 0.0), ("heavy", 0.99)];
        let mut light = 0;
        let mut heavy = 0;
        for _ in 0..1000 {
            match weighted_random_select(&candidates, load, &SystemSelectionRng)
                .unwrap()
                .0
            {
                "light" => light += 1,
                "heavy" => heavy += 1,
                other => panic!("unexpected {other}"),
            }
        }
        assert!(
            light > heavy * 10,
            "expected light ({light}) to be picked much more than heavy ({heavy})"
        );
    }

    #[test]
    fn load_above_cap_keeps_minimum_weight() {
        // Loads >= 0.99 still weigh 0.01, so an all-full pool is still picked
        // from rather than always the first.
        let candidates: Vec<C> = vec![("a", 1.5), ("b", 1.5)];
        let picked = weighted_random_select(&candidates, load, &scripted(Some(u64::MAX))).unwrap();
        assert_eq!(picked.0, "b");
    }
}
