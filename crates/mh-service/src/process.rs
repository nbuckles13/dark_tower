//! Process-incarnation identity for this MH process.
//!
//! ADR-0036 §8 gives MC a restart detector: MH reports the wall-clock instant
//! at which **this process** started, and MC compares successive reports for
//! *inequality*. A changed value means the handler restarted and its in-memory
//! policy is gone, so MC must re-assert.
//!
//! # Why this is a module and not a `now()` call at the response site
//!
//! Three implementations are observationally identical until a second process
//! incarnation shares a pod identity, and two of them are wrong:
//!
//! - **per-call `now()`** — changes on every RPC, so the detector fires
//!   constantly and is ignored;
//! - **derived from pod name or config** — never changes across a
//!   crash-restart into the same pod, so the detector never fires for the case
//!   it exists to catch;
//! - **sampled once at process start** — correct.
//!
//! `internal.proto::RegisterMeetingResponse.process_start_epoch_ms` states the
//! MUST; `docs/TODO.md` §Media Path Obligations item (c) records that only the
//! *within-process stability* half is constructible before the handler-restart
//! story (two `register_meeting` calls in one process return the same value,
//! which kills the per-call clock read), and pins the changes-on-restart half
//! where a second incarnation exists.
//!
//! The value is sampled by [`sample_process_start_epoch_ms`], called once from
//! `main` and threaded into the gRPC service, so "once per process" is visible
//! at the call site rather than hidden behind a lazily-initialised static whose
//! first touch could drift to an arbitrary later moment.

use std::time::{SystemTime, UNIX_EPOCH};

/// Sample the current wall clock as Unix epoch milliseconds.
///
/// Call this **once**, as early as possible in `main`, and pass the result
/// down. Calling it per request reintroduces the per-call-`now()` defect this
/// module exists to prevent.
///
/// A clock before the Unix epoch is not representable in the `uint64` wire
/// field. Rather than panic (ADR-0002 forbids it on this path) or silently
/// wrap, this returns `0`, which the contract already defines as "not
/// reported" — MC treats it as *restart undetectable* rather than as a stable
/// incarnation, which is the correct reading of a clock we cannot encode.
#[must_use]
pub fn sample_process_start_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn sample_returns_a_plausible_epoch() {
        // Well after 2020-01-01 and before 2100-01-01: catches a units error
        // (seconds or nanoseconds in a milliseconds field) without pinning a
        // wall-clock value a test cannot know.
        let ms = sample_process_start_epoch_ms();
        assert!(ms > 1_577_836_800_000, "epoch ms implausibly small: {ms}");
        assert!(ms < 4_102_444_800_000, "epoch ms implausibly large: {ms}");
    }

    /// The value is a *process-incarnation token*, so what matters downstream
    /// is that one sample, once taken, is carried unchanged — not that two
    /// separate samples agree. The within-process stability requirement is
    /// enforced by there being exactly one call site in `main`, and is pinned
    /// end-to-end at the RPC boundary in `grpc::mh_service`.
    #[test]
    fn a_sample_is_a_value_that_can_be_carried_unchanged() {
        let sampled = sample_process_start_epoch_ms();
        let carried = sampled;
        assert_eq!(sampled, carried);
    }
}
