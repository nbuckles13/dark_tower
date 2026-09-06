//! Random one-in-N latency sampling.
//!
//! ADR-0036 §11: "**Sampling must be random, not deterministic per stream** —
//! the natural modulo implementation meets the CPU budget while perfectly
//! reconstructing the sequence." Timestamps are taken on every frame (a
//! monotonic clock read is a ~25 ns vDSO call and the ingress timestamp is
//! needed for frame age regardless); the histogram observation is the expensive
//! part and is the thing sampled.
//!
//! # What is forbidden here, by name
//!
//! Every one of these meets the CPU budget and every one is deterministic
//! *within* a stream, which is exactly what reconstructs the voice-activity
//! trace from an aggregate histogram:
//!
//! - modulo on `hop_sequence` or `stream_sequence`;
//! - any hash of stream, slot or sender identity;
//! - an RNG seeded from a stream identifier;
//! - a fixed period with a random phase.
//!
//! The sampler therefore holds a **per-connection** PRNG seeded from the
//! operating system and draws independently per frame. Two connections fed
//! identical arrival patterns produce different sample sets, which is asserted
//! rather than assumed (see this module's tests).

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// A per-connection one-in-N sampler.
///
/// `SmallRng` rather than `ThreadRng`: this is per-connection state on a
/// per-frame path, and `SmallRng` is a small, fast, non-cryptographic generator
/// — appropriate because nothing here is a secret. It is seeded
/// `from_entropy()`, i.e. from the OS, and **never** from anything derived from
/// stream identity.
#[derive(Debug)]
pub struct LatencySampler {
    rng: SmallRng,
    ratio: f64,
}

impl LatencySampler {
    /// A sampler drawing at `ratio` (0.0..=1.0).
    ///
    /// `ratio` comes from `Config::media_latency_sample_ratio`, the same field
    /// `mh_media_latency_sample_ratio` publishes. Config validates the range at
    /// load; the clamp here is what makes [`Self::should_sample`] total for any
    /// caller, including tests that construct one directly.
    #[must_use]
    pub fn new(ratio: f64) -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            ratio: ratio.clamp(0.0, 1.0),
        }
    }

    /// The ratio this sampler draws against.
    ///
    /// Exposed so the gauge publishes the value the sampler actually uses
    /// rather than a second reading of config.
    #[must_use]
    pub const fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Whether this frame's latency should be observed.
    pub fn should_sample(&mut self) -> bool {
        self.rng.gen_bool(self.ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_one_samples_every_frame_and_ratio_zero_samples_none() {
        // Any test that observes a latency emission forces the ratio to 1.0:
        // a random sampler must never gate an assertion.
        let mut always = LatencySampler::new(1.0);
        let mut never = LatencySampler::new(0.0);
        for _ in 0..64 {
            assert!(always.should_sample());
            assert!(!never.should_sample());
        }
    }

    #[test]
    fn out_of_range_ratios_are_clamped_rather_than_panicking() {
        assert!((LatencySampler::new(4.0).ratio() - 1.0).abs() < f64::EPSILON);
        assert!(LatencySampler::new(-1.0).ratio().abs() < f64::EPSILON);
    }

    #[test]
    fn two_samplers_fed_identical_arrival_patterns_produce_different_sample_sets() {
        // The §11 property, asserted rather than assumed. A deterministic
        // implementation — modulo on a sequence number, a hash of stream
        // identity, an identity-seeded RNG, or a fixed period — makes these two
        // sequences IDENTICAL, and an operator holding the aggregate histogram
        // can then reconstruct which frames of a single stream were observed.
        //
        // 512 draws at p=0.5: two independent sequences agreeing everywhere has
        // probability 2^-512, so this cannot flake for a correct
        // implementation. It fails deterministically for every forbidden one.
        let mut a = LatencySampler::new(0.5);
        let mut b = LatencySampler::new(0.5);
        let seq_a: Vec<bool> = (0..512).map(|_| a.should_sample()).collect();
        let seq_b: Vec<bool> = (0..512).map(|_| b.should_sample()).collect();
        assert_ne!(
            seq_a, seq_b,
            "two connections must not share a sampling schedule — a deterministic sampler \
             reconstructs the per-stream voice-activity trace (ADR-0036 §11)"
        );
    }
}
