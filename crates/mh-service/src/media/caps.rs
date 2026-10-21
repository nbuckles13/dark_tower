//! Ingress denial-of-service caps, enforced **before** any per-frame work.
//!
//! ADR-0036 §11 names three: "maximum frame payload before any allocation;
//! per-connection concurrent-stream and creation-rate limits enforced before
//! per-frame work". Where each lives:
//!
//! | Cap | Where |
//! |---|---|
//! | Maximum frame payload before any allocation | Datagrams: **here**, [`check_datagram_size`], wired in `crate::media::ingress::run_ingress`. Streams: `media_protocol::reader::HeaderReader` rejects a declared `payload_length` over `MAX_PAYLOAD_BYTES` from the 14-byte prefix, before anything is sized from it (`crate::media::stream_ingress`) |
//! | Per-connection **concurrent** stream limit | A declared QUIC transport parameter, `MH_MAX_CONCURRENT_UNI_STREAMS` (`crate::config`) — enforced by quinn. MH has no event to count; it publishes the declared limit and the live groups as gauges |
//! | Per-connection stream **creation-rate** limit | **Here**, [`TokenBucket`], admitted at accept in `crate::media::stream_ingress::run_stream_ingress`. A refused stream has ONLY its fixed header prefix read, through the same `HeaderReader` (its GoP counter recorded so MH can repair it by request, story 3 amendment 4), then is `STOP_SENDING`'d and counted `stream_rate_limited`; a malformed header is stopped with no counter and no repair |
//!
//! > **Corrected (story 3 MH-A2).** This module used to say the creation-rate
//! > cap was "not enforced, and not enforceable today" because MH had no
//! > unidirectional-stream accept loop. MH-A2 added that loop, so the cap now
//! > has its enforcement point and its counter a firing path.
//!
//! Nothing here emits and nothing here allocates; each check returns a value
//! the caller counts through a pre-resolved handle.

use crate::observability::metrics::MediaDropReason;
use media_protocol::frame::MAX_FRAME_BYTES;
use tokio::time::{Duration, Instant};

/// A token bucket: `rate` tokens per second, holding at most `burst`.
///
/// Generic on purpose — one bucket type, not one per limit. The stream-open
/// cap is its only user: story 3 amendment 4 removed the per-connection
/// recovery-request rate limit (recovery requests are bounded by QUIC flow
/// control, a fixed-size parse and one lookup, and deduped by counter).
///
/// # Why not `governor`
///
/// `governor` (a workspace dependency, used by gc-service) needs a clock whose
/// instant implements its `Reference` trait. MH's deadlines run on tokio time
/// so tests drive them with `start_paused` + `advance` (ADR-0036 §10: "all
/// deadline logic takes an injected clock"), and tokio's `Instant` is foreign
/// to both crates: using governor would mean a newtype adapter whose
/// nanosecond arithmetic needs its own tests — more code than this bucket.
///
/// Owned state, `now` passed in, no interior mutability: the caller (one
/// accept loop per connection) owns it.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Whole tokens available.
    tokens: u32,
    burst: u32,
    /// Time to accrue one token.
    period: Duration,
    last_refill: Instant,
}

impl TokenBucket {
    /// A full bucket of `burst` tokens refilling at `rate` per second.
    /// `rate` and `burst` are clamped to at least 1 (config forbids 0).
    #[must_use]
    pub fn new(rate: u32, burst: u32, now: Instant) -> Self {
        let rate = rate.max(1);
        Self {
            tokens: burst.max(1),
            burst: burst.max(1),
            period: Duration::from_secs(1) / rate,
            last_refill: now,
        }
    }

    /// Take one token if available.
    pub fn admit(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }

    fn refill(&mut self, now: Instant) {
        if self.tokens >= self.burst {
            self.last_refill = now;
            return;
        }
        let elapsed = now.saturating_duration_since(self.last_refill);
        let earned = elapsed.as_nanos() / self.period.as_nanos().max(1);
        if earned == 0 {
            return;
        }
        let earned = u32::try_from(earned).unwrap_or(u32::MAX);
        self.tokens = self.tokens.saturating_add(earned).min(self.burst);
        // Advance by whole periods only, keeping the remainder.
        let advanced = self.period.saturating_mul(earned);
        self.last_refill += advanced;
        if self.tokens >= self.burst {
            self.last_refill = now;
        }
    }
}

/// Reject a datagram whose **byte length** exceeds the wire-format frame
/// maximum, before any parse and before anything is allocated on our behalf.
///
/// The bound is `media_protocol::frame::MAX_FRAME_BYTES` — derived in the codec
/// from the header, payload and signature sizes, never recomputed here. A whole
/// datagram carries one complete frame (ADR-0036 §1, one frame per datagram),
/// so the whole-frame maximum is the right bound; `MAX_PAYLOAD_BYTES` would
/// reject legal frames by the size of their own header.
///
/// This is deliberately **not** the codec's `payload_length_exceeds_max`. That
/// token means "the declared `payload_length` *field* exceeded the max during
/// header validation" and is identical to the client's condition, so MH must
/// use the shared spelling there. This one is a pre-parse whole-datagram cap
/// and gets MH's own token. Same constant, two checks, two tokens.
///
/// # Errors
///
/// [`MediaDropReason::OversizeDatagram`] when `len` exceeds the bound.
pub const fn check_datagram_size(len: usize) -> Result<(), MediaDropReason> {
    if len > MAX_FRAME_BYTES {
        return Err(MediaDropReason::OversizeDatagram);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datagram_at_the_bound_is_admitted_and_one_byte_over_is_not() {
        // Written against the derived constant, never a literal: the bound
        // moves with the header, and a test carrying its own number would keep
        // passing against the old wire format.
        assert!(check_datagram_size(MAX_FRAME_BYTES).is_ok());
        assert_eq!(
            check_datagram_size(MAX_FRAME_BYTES + 1),
            Err(MediaDropReason::OversizeDatagram)
        );
        assert!(check_datagram_size(0).is_ok());
    }

    #[test]
    fn the_bucket_admits_its_burst_then_refills_at_its_rate() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(10, 10, start);
        for _ in 0..10 {
            assert!(bucket.admit(start));
        }
        assert!(
            !bucket.admit(start),
            "the eleventh in the same instant is refused"
        );
        assert!(
            !bucket.admit(start + Duration::from_millis(99)),
            "one token takes 100 ms at 10/s"
        );
        assert!(bucket.admit(start + Duration::from_millis(100)));
        assert!(!bucket.admit(start + Duration::from_millis(100)));
        let later = start + Duration::from_secs(5);
        for _ in 0..10 {
            assert!(
                bucket.admit(later),
                "never more than the burst after a long idle"
            );
        }
        assert!(!bucket.admit(later));
    }
}
