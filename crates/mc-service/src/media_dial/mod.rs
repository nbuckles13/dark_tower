//! MC-minted `media_dial` dial credential (ADR-0039 §5).
//!
//! The join-time meeting JWT (900 s, not retained by the SDK) cannot dial a
//! replacement handler after minute 15, so MC signs a short-lived per-handler
//! dial credential ITSELF, with no AC call, and keeps every participant
//! supplied with a valid one for every handler in its set.
//!
//! - [`key`]: the per-meeting Ed25519 signing key (held only by the meeting
//!   actor, beside and separate from the KEK) and the token mint.
//! - [`refresh`]: the per-participant TTL/3 re-push schedule (jittered,
//!   seedable) and the coalesced push reasons.
//! - [`status`]: the latest client-reported status per (participant,
//!   `handler_ref`), and the FAILED fast re-push limiter.
//!
//! The wire rules for the token are stated once, at `signaling.proto`
//! `MediaServerInfo.media_dial_token`; the claims shape is
//! `common::jwt::MediaDialClaims`.

pub mod key;
pub mod refresh;
pub mod status;

use std::time::Duration;

/// The `media_dial` credential timing (`MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`).
///
/// Built ONCE from the validated config (`Config::media_dial`). The minted
/// `exp`, the refresh schedule and both published config gauges read it from
/// here, so the TTL and its TTL/3 derivation cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaDialTiming {
    ttl_seconds: u32,
}

impl MediaDialTiming {
    /// Timing for a TTL, in seconds, already validated at config load.
    #[must_use]
    pub fn from_ttl_seconds(ttl_seconds: u32) -> Self {
        Self { ttl_seconds }
    }

    /// The token lifetime: `exp = iat + ttl`.
    #[must_use]
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(u64::from(self.ttl_seconds))
    }

    /// The TTL in whole seconds, for the `exp` claim.
    #[must_use]
    pub fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }

    /// THE refresh-interval derivation: TTL/3 (ADR-0039 §5).
    ///
    /// The nominal cadence. Each participant's actual gap is this minus a
    /// jitter in `[0, max_jitter()]` (see [`refresh`]), never more, so the
    /// longest gap between two pushes is TTL/3 < TTL.
    #[must_use]
    pub fn refresh_interval(&self) -> Duration {
        self.ttl() / 3
    }

    /// The largest per-participant jitter: a tenth of the refresh interval,
    /// subtracted only (a refresh is only ever EARLIER than nominal).
    #[must_use]
    pub fn max_jitter(&self) -> Duration {
        self.refresh_interval() / 10
    }

    /// Publish the two config gauges from this value (ADR-0039 §8).
    pub fn publish_config_gauges(&self) {
        crate::observability::metrics::set_media_dial_token_timing(self);
    }
}

/// Why a participant's `MediaHandlerSet` was pushed — the
/// `mc_media_handler_set_pushes_total{reason}` vocabulary (ADR-0039 §8).
///
/// Exactly one reason per message. When several land on one participant in
/// one turn, the push carries the highest by [`Self::precedence`]:
/// `join > change > failed_report > token_refresh`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerSetPushReason {
    /// The (re)join's own set (`JoinResponse`, or a reconnect).
    Join,
    /// A handler-membership change (the set's version advanced).
    Change,
    /// The client's FAILED report for an in-set ref: the credential re-push.
    FailedReport,
    /// The TTL/3 token refresh (version and refs unchanged).
    TokenRefresh,
}

impl HandlerSetPushReason {
    /// Every reason, for zero-initialisation.
    pub const ALL: [Self; 4] = [
        Self::Join,
        Self::Change,
        Self::FailedReport,
        Self::TokenRefresh,
    ];

    /// The metric label value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Change => "change",
            Self::FailedReport => "failed_report",
            Self::TokenRefresh => "token_refresh",
        }
    }

    /// Coalescing rank: higher wins.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Join => 3,
            Self::Change => 2,
            Self::FailedReport => 1,
            Self::TokenRefresh => 0,
        }
    }

    /// The reason a coalesced push carries.
    #[must_use]
    pub fn coalesce(self, other: Self) -> Self {
        if other.precedence() > self.precedence() {
            other
        } else {
            self
        }
    }
}

/// Where a minted token's `iat` comes from.
///
/// Production is the wall clock. The anchored variant exists only in
/// `test-seams` builds (it cannot be constructed in a release build): it
/// advances with tokio's clock, so a paused-time test moves `iat` and the
/// refresh arm together; a wall-clock `iat` under paused time would never
/// expire and an "never holds only an expired token" test would be vacuous.
#[derive(Debug, Clone, Copy, Default)]
pub enum DialClock {
    /// `SystemTime` (UNIX seconds).
    #[default]
    Wall,
    /// `base_unix` + the tokio time elapsed since `base`. **Test builds only.**
    #[cfg(feature = "test-seams")]
    Anchored {
        /// UNIX seconds at `base`.
        base_unix: i64,
        /// The tokio instant `base_unix` corresponds to.
        base: tokio::time::Instant,
    },
}

impl DialClock {
    /// An anchored clock reading `base_unix` now. **Test builds only.**
    #[cfg(feature = "test-seams")]
    #[must_use]
    pub fn anchored(base_unix: i64) -> Self {
        Self::Anchored {
            base_unix,
            base: tokio::time::Instant::now(),
        }
    }

    /// Now, in UNIX seconds.
    #[must_use]
    pub fn now_unix(&self) -> i64 {
        match self {
            Self::Wall => chrono::Utc::now().timestamp(),
            #[cfg(feature = "test-seams")]
            Self::Anchored { base_unix, base } => {
                let elapsed = tokio::time::Instant::now().saturating_duration_since(*base);
                base_unix.saturating_add(i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_interval_is_a_third_and_jitter_keeps_it_under_ttl() {
        for ttl in [60, 120, 300, 900] {
            let t = MediaDialTiming::from_ttl_seconds(ttl);
            assert_eq!(t.refresh_interval() * 3, t.ttl());
            assert!(t.max_jitter() < t.refresh_interval());
            assert!(t.refresh_interval() < t.ttl());
        }
    }

    #[test]
    fn push_reasons_coalesce_by_precedence() {
        use HandlerSetPushReason::*;
        let ordered = [Join, Change, FailedReport, TokenRefresh];
        for (i, a) in ordered.iter().enumerate() {
            for b in &ordered[i..] {
                assert_eq!(a.coalesce(*b), *a, "{a:?} vs {b:?}");
                assert_eq!(b.coalesce(*a), *a, "{b:?} vs {a:?}");
            }
        }
        let labels: Vec<_> = HandlerSetPushReason::ALL
            .iter()
            .map(|r| r.label())
            .collect();
        assert_eq!(labels, ["join", "change", "failed_report", "token_refresh"]);
    }
}
