// File: packages/sdk-core/src/session/joinRetry.ts
//
// THE join-retry rule of `MeetingSession.join()` (join-window devloop,
// 2026-11-12). Pure: the random source is injected, so tests pin it.
//
//   delay(n) = floor(w(n) / 2) + floor(random() * (w(n) - floor(w(n) / 2)))
//   w(n)     = min(capMs, baseMs * 2^n)
//
// EQUAL jitter, deliberately not `redialDelayMs`' full jitter: full jitter's
// guaranteed minimum is ~0, so every retry could land inside the window this
// retry exists to outlast. The floor (half of each window) makes the minimum
// cumulative backoff a guarantee, `JOIN_RETRY_MIN_SPAN_MS`, which the test keeps
// above `MC_END_MEETING_BACKOFF_MS` (MC's one release retry; that mirror is
// pinned EQUAL to MC's constant by a Rust test). The formula lives ONLY here.

import { JOIN_RETRY } from '../config/clientConfig.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import { backoffWindowMs } from '../media/redialBackoff.js';

/** The inputs of {@link joinRetryDelayMs}. */
export interface JoinRetryBackoff {
  readonly baseMs: number;
  readonly capMs: number;
}

/**
 * The wait before retry `retryIndex + 1` (0-based: index 0 is the wait after the
 * FIRST rejection). `random` is uniform in [0, 1). Never below half the window,
 * never at or above the window.
 */
export function joinRetryDelayMs(
  retryIndex: number,
  random: () => number,
  backoff: JoinRetryBackoff = JOIN_RETRY,
): number {
  const w = backoffWindowMs(retryIndex, backoff);
  const floor = Math.floor(w / 2);
  return floor + Math.floor(random() * (w - floor));
}

/** The guaranteed minimum cumulative backoff across all of a join's retries. */
export function joinRetryMinSpanMs(
  maxAttempts: number = JOIN_RETRY.maxAttempts,
  backoff: JoinRetryBackoff = JOIN_RETRY,
): number {
  let total = 0;
  for (let n = 0; n < maxAttempts - 1; n += 1) {
    total += Math.floor(backoffWindowMs(n, backoff) / 2);
  }
  return total;
}

/** Derived from `JOIN_RETRY`, never restated (1750 ms with today's values). */
export const JOIN_RETRY_MIN_SPAN_MS = joinRetryMinSpanMs();

/**
 * Whether a join rejection is "this meeting ended or is ending — re-join through
 * GC". An ALLOW-LIST of exactly two server codes: `NotFound` (MC
 * `meeting_not_found`) and `Conflict` (MC `teardown_in_progress`; the only other
 * join-reachable `Conflict`, "participant already in meeting", cannot recur on a
 * fresh attempt, which presents a fresh participant). Every other code — auth
 * class, invalid request, internal, capacity, contract fault and the local
 * connect-stage codes — is NOT retried.
 */
export function isRetryableJoinRejection(err: unknown): err is SignalingError {
  return (
    err instanceof SignalingError &&
    (err.signalingCode === SignalingErrorCode.NotFound ||
      err.signalingCode === SignalingErrorCode.Conflict)
  );
}
