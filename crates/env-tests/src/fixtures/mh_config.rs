//! How env-test 01(c) compares a MH config GAUGE with the deployed ConfigMap
//! value (`tests/01_mh_deployment_config.rs`). PURE, and in the library — not
//! behind the env-test feature gates — so its branches are unit-tested at
//! Layer 4 rather than only against a live cluster at Layer 7, where a logic
//! bug would be triaged as an environment failure.

/// How MH turns a deployed ConfigMap value into the value it ENFORCES — and
/// publishes. Each arm is the SAME expression MH's config load uses, so the
/// comparison against the published gauge is exact, with no tolerance.
/// `convert` is TOTAL: a bit value that is not a multiple of 8 is valid config
/// MH floors or ceils, never an error here.
#[derive(Clone, Copy, Debug)]
pub enum Conversion {
    /// Counts and bounds, published verbatim.
    Identity,
    /// `bits / 8`, floored — `crates/mh-service/src/config.rs`
    /// `EgressAdmission::derive` (`budget_bps / 8`).
    BitsToBytesFloor,
    /// `bits.div_ceil(8)` — `EgressAdmission::derive` (the stream costs).
    BitsToBytesCeil,
    /// `ms as f64 / 1000.0` — `publish_egress_admission`
    /// (`mh_media_policy_apply_timeout_seconds`). Compared as the integer
    /// round trip `round(gauge * 1000) == ms`, never as a float equality: the
    /// value crosses Prometheus text exposition and the query API's formatting.
    MsToSeconds,
}

/// The largest integer an `f64` gauge carries exactly.
pub const F64_EXACT_INT_MAX: f64 = 9_007_199_254_740_992.0; // 2^53

/// Outcome of comparing one published gauge with its deployed value.
#[derive(Debug, PartialEq, Eq)]
pub enum GaugeVerdict {
    Match,
    /// The gauge's value is not a shape its Conversion can produce (a
    /// non-integral, negative, non-finite or > 2^53 value for an integer row;
    /// negative or non-finite for the timeout): the publisher changed shape.
    Shape,
    /// Well-formed, but not the deployed value. Carries the expected value.
    Mismatch(String),
}

/// Compare published `gauge` with `deployed` under `conversion`. Exhaustive:
/// each integer row names its expected value and shares [`integral`]'s shape
/// check; the timeout row is the integer round trip `round(gauge * 1000) == ms`.
#[must_use]
pub fn gauge_verdict(conversion: Conversion, deployed: u64, gauge: f64) -> GaugeVerdict {
    if !gauge.is_finite() || gauge < 0.0 {
        return GaugeVerdict::Shape;
    }
    match conversion {
        Conversion::Identity => integral(gauge, deployed),
        Conversion::BitsToBytesFloor => integral(gauge, deployed / 8),
        Conversion::BitsToBytesCeil => integral(gauge, deployed.div_ceil(8)),
        Conversion::MsToSeconds => {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "gauge is finite and non-negative (checked above); a timeout in ms fits u64"
            )]
            let ms = (gauge * 1000.0).round() as u64;
            if ms == deployed {
                GaugeVerdict::Match
            } else {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "diagnostic text only; a configured timeout is far below 2^53 ms"
                )]
                let seconds = deployed as f64 / 1000.0;
                GaugeVerdict::Mismatch(format!("{seconds} s"))
            }
        }
    }
}

/// An integer row: the gauge must be an exact non-negative integer (the caller
/// checked finite/non-negative) no larger than 2^53, equal to `expected`.
fn integral(gauge: f64, expected: u64) -> GaugeVerdict {
    if gauge.fract() != 0.0 || gauge > F64_EXACT_INT_MAX {
        return GaugeVerdict::Shape;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "gauge is an exact non-negative integer <= 2^53 (checked above)"
    )]
    let published = gauge as u64;
    if published == expected {
        GaugeVerdict::Match
    } else {
        GaugeVerdict::Mismatch(expected.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauge_verdict_branches() {
        use Conversion::*;
        assert_eq!(gauge_verdict(Identity, 512, 512.0), GaugeVerdict::Match);
        assert_eq!(
            gauge_verdict(Identity, 512, 511.0),
            GaugeVerdict::Mismatch("512".into())
        );
        assert_eq!(gauge_verdict(Identity, 512, 512.5), GaugeVerdict::Shape);
        assert_eq!(gauge_verdict(Identity, 1, -1.0), GaugeVerdict::Shape);
        assert_eq!(gauge_verdict(Identity, 1, f64::NAN), GaugeVerdict::Shape);
        assert_eq!(gauge_verdict(Identity, 1, 1e300), GaugeVerdict::Shape);
        // Budget floors, costs ceil — a non-multiple of 8 is valid config.
        assert_eq!(
            gauge_verdict(BitsToBytesFloor, 100_000_007, 12_500_000.0),
            GaugeVerdict::Match
        );
        assert_eq!(
            gauge_verdict(BitsToBytesCeil, 90_001, 11_251.0),
            GaugeVerdict::Match
        );
        assert_eq!(
            gauge_verdict(BitsToBytesCeil, 90_001, 11_250.0),
            GaugeVerdict::Mismatch("11251".into())
        );
        // The timeout is legitimately fractional; the integer round trip decides.
        assert_eq!(gauge_verdict(MsToSeconds, 2_500, 2.5), GaugeVerdict::Match);
        assert_eq!(gauge_verdict(MsToSeconds, 1_000, 1.0), GaugeVerdict::Match);
        assert_eq!(
            gauge_verdict(MsToSeconds, 1_000, 1.5),
            GaugeVerdict::Mismatch("1 s".into())
        );
        assert_eq!(gauge_verdict(MsToSeconds, 1_000, -1.0), GaugeVerdict::Shape);
    }
}
