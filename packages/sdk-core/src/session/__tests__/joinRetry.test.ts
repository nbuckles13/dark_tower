// File: packages/sdk-core/src/session/__tests__/joinRetry.test.ts
//
// The pure join-retry rule (`session/joinRetry.ts`): the equal-jitter delay, its
// guaranteed minimum span, and the retryable-code ALLOW-LIST. The session-level
// behaviour (N attempts = N GC joins, release, telemetry) is in
// `meeting-session.test.ts` ("join retry").

import { describe, expect, it } from 'vitest';

import { JOIN_RETRY, MC_END_MEETING_BACKOFF_MS } from '../../config/clientConfig.js';
import { SignalingError, SignalingErrorCode } from '../../errors/SignalingError.js';
import {
  isRetryableJoinRejection,
  JOIN_RETRY_MIN_SPAN_MS,
  joinRetryDelayMs,
  joinRetryMinSpanMs,
} from '../joinRetry.js';

describe('joinRetryDelayMs — equal jitter', () => {
  it('is the window floor at random = 0, and the cap applies', () => {
    expect(joinRetryDelayMs(0, () => 0)).toBe(250);
    expect(joinRetryDelayMs(1, () => 0)).toBe(500);
    expect(joinRetryDelayMs(2, () => 0)).toBe(1_000);
    // Capped: 500 * 2^3 = 4000 > cap 2000.
    expect(joinRetryDelayMs(3, () => 0)).toBe(1_000);
  });

  it('stays strictly below the window as random -> 1', () => {
    const almostOne = (): number => 1 - Number.EPSILON;
    for (let n = 0; n < 6; n += 1) {
      const window = Math.min(JOIN_RETRY.capMs, JOIN_RETRY.baseMs * 2 ** n);
      const d = joinRetryDelayMs(n, almostOne);
      expect(d).toBeLessThan(window);
      expect(d).toBeGreaterThanOrEqual(Math.floor(window / 2));
    }
  });
});

describe('JOIN_RETRY_MIN_SPAN_MS', () => {
  it('is derived from JOIN_RETRY (the sum of the window floors)', () => {
    expect(JOIN_RETRY_MIN_SPAN_MS).toBe(joinRetryMinSpanMs());
    expect(JOIN_RETRY_MIN_SPAN_MS).toBe(250 + 500 + 1_000);
  });

  // THE floor check, with the REAL formula against the MC mirror (itself pinned
  // EQUAL to MC's END_MEETING_BACKOFF by the Rust test
  // `the_sdk_end_meeting_backoff_mirror_equals_mcs`).
  it("exceeds MC's one release retry (END_MEETING_BACKOFF)", () => {
    expect(JOIN_RETRY_MIN_SPAN_MS).toBeGreaterThan(MC_END_MEETING_BACKOFF_MS);
  });

  it('equals the cumulative delay of an all-zero random draw', () => {
    let total = 0;
    for (let n = 0; n < JOIN_RETRY.maxAttempts - 1; n += 1) total += joinRetryDelayMs(n, () => 0);
    expect(total).toBe(JOIN_RETRY_MIN_SPAN_MS);
  });
});

describe('isRetryableJoinRejection — an allow-list of exactly NotFound and Conflict', () => {
  // Every code, so a new one forces a decision here.
  const RETRYABLE = new Set<string>([SignalingErrorCode.NotFound, SignalingErrorCode.Conflict]);
  for (const code of Object.values(SignalingErrorCode)) {
    it(`${code} -> ${RETRYABLE.has(code) ? 'retried' : 'not retried'}`, () => {
      expect(isRetryableJoinRejection(new SignalingError(code, 'x'))).toBe(RETRYABLE.has(code));
    });
  }

  it('is false for anything that is not a SignalingError', () => {
    expect(isRetryableJoinRejection(new Error('NOT_FOUND'))).toBe(false);
    expect(isRetryableJoinRejection(undefined)).toBe(false);
  });
});
