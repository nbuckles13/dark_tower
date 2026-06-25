// File: packages/sdk-core/src/telemetry/__tests__/closeReason.test.ts
//
// R-25 cardinality contract: `dt_client_signaling_connection_total{close_reason}`
// is bounded. `normalizeCloseReason` maps numeric close codes onto the small
// fixed enum; unknown codes collapse to `'unknown'` (never the raw string).

import { describe, expect, it } from 'vitest';

import { CloseReason, normalizeCloseReason } from '../closeReason.js';

describe('normalizeCloseReason (R-25 bounded close_reason)', () => {
  it('maps known close codes to the bounded enum', () => {
    expect(normalizeCloseReason(0)).toBe(CloseReason.Normal);
    expect(normalizeCloseReason(1001)).toBe(CloseReason.GoingAway);
    expect(normalizeCloseReason(3401)).toBe(CloseReason.AuthFailed);
    expect(normalizeCloseReason(3408)).toBe(CloseReason.Timeout);
    expect(normalizeCloseReason(1011)).toBe(CloseReason.ServerError);
  });

  it('collapses unknown / undefined codes to unknown (never a raw string)', () => {
    expect(normalizeCloseReason(4999)).toBe(CloseReason.Unknown);
    expect(normalizeCloseReason(undefined)).toBe(CloseReason.Unknown);
  });

  it('only ever returns one of the bounded enum values', () => {
    const allowed = new Set(Object.values(CloseReason));
    for (const code of [0, 1001, 1011, 3401, 3408, 42, -1, undefined]) {
      expect(allowed.has(normalizeCloseReason(code))).toBe(true);
    }
  });
});
