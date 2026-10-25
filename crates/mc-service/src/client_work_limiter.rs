//! The token bucket every repeatable client-driven MC path is bounded by
//! (moved out of `webtransport::connection` when the meeting actor's
//! client-mute fan-out became a second user, story 3 R-15 fix).

use std::time::{Duration, Instant};

/// Token bucket bounding a repeatable client-driven action.
///
/// # Why a rate limit and NOT a budget
///
/// The receive-capability path takes a cumulative budget for ACCEPTED
/// declarations because re-declaring is rare and a client that stops
/// re-declaring loses nothing. Every action bounded here is the opposite: a
/// repeatable steady-state path that must keep working for the life of the
/// session. A cumulative budget would spend out and then permanently deny — for
/// instance freezing a client-mute fact on the roster so every other
/// participant renders a live speaker as muted, or silencing the error a client
/// needs in order to correct its declaration.
///
/// A rate limit bounds work per unit time and never permanently denies. See
/// `media_signaling`'s module doc for the criterion in general form.
///
/// # One instance per bounded path, deliberately never a shared bucket
///
/// The connection's `MediaSignalingContext` holds a separate bucket for every bounded path —
/// client mute, client-mute rejection replies, capability rejection replies,
/// server mute, server-mute refusal replies, unmute requests — so that
/// flooding ANY one of them cannot suppress any other on the same connection.
/// A shared bucket would make each path a denial vector against the rest,
/// which is the amplification problem one level down.
///
/// One further path is PER MEETING rather than per connection: the meeting
/// actor's client-mute roster fan-out
/// (`actors::client_mute_fanout::ClientMuteFanout`), whose cost is O(roster)
/// per spend, so a per-connection bound alone would leave the meeting's
/// aggregate O(N²).
#[derive(Debug)]
pub(crate) struct ClientWorkLimiter {
    tokens: u32,
    burst: u32,
    refill_interval_ms: u64,
    refilled_at: Instant,
}

impl ClientWorkLimiter {
    pub(crate) fn new(now: Instant, burst: u32, refill_interval_ms: u64) -> Self {
        Self {
            tokens: burst,
            burst,
            refill_interval_ms,
            refilled_at: now,
        }
    }

    /// Refill, then spend one token. `false` means the caller must do no work.
    ///
    /// Tokens are spent ONLY on work actually about to be done — the caller
    /// short-circuits identical repeats before reaching here. Draining the
    /// bucket on no-op messages would let a client spamming a steady state
    /// suppress its own next genuine toggle, and would report `rate_limited`
    /// while the expensive path was never approached.
    pub(crate) fn try_spend(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }

    /// When the next [`Self::try_spend`] can succeed: `now` if a token is
    /// available, else the instant the next token is earned. For a caller that
    /// defers work rather than dropping it and must wake to resume.
    pub(crate) fn next_token_at(&mut self, now: Instant) -> Instant {
        self.refill(now);
        if self.tokens > 0 {
            now
        } else {
            self.refilled_at + Duration::from_millis(self.refill_interval_ms)
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed_ms = now.saturating_duration_since(self.refilled_at).as_millis();
        let earned = elapsed_ms / u128::from(self.refill_interval_ms);
        if earned == 0 {
            return;
        }
        if earned >= u128::from(self.burst) {
            // Long enough idle that the bucket is full however the arithmetic
            // is rounded; reset the clock rather than advance it, which also
            // keeps `Duration` multiplication away from overflow.
            self.tokens = self.burst;
            self.refilled_at = now;
            return;
        }
        // `earned < self.burst` here, so the narrowing cannot lose a token.
        let earned = u32::try_from(earned).unwrap_or(self.burst);
        self.tokens = self.tokens.saturating_add(earned).min(self.burst);
        // Advance by exactly what was granted, so a sub-interval remainder is
        // carried rather than discarded — otherwise a client polling just under
        // the interval would earn nothing forever.
        self.refilled_at += Duration::from_millis(u64::from(earned) * self.refill_interval_ms);
    }
}
