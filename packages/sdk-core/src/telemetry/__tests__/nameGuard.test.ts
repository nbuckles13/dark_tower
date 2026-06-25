// File: packages/sdk-core/src/telemetry/__tests__/nameGuard.test.ts
//
// R-24: the `dt_client_*` name guard. Covers BOTH the throw (dev/test) and warn
// (prod) branches in one suite — the runtime `mode` seam makes that possible
// without a build-time PROD constant-fold (a single Vitest config exercises
// both, so the ≥90% branch gate is satisfiable). Also locks the guard's regex
// in lockstep with the CI `dt-guard` rule.

import { afterEach, describe, expect, it, vi } from 'vitest';

import { assertClientMetricName, DT_CLIENT_NAME_PATTERN } from '../nameGuard.js';

describe('assertClientMetricName', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("mode 'throw' (dev/test)", () => {
    it('returns true for a compliant name and does not throw', () => {
      expect(assertClientMetricName('dt_client_join_attempts_total', 'throw')).toBe(true);
    });

    it('throws for a non-compliant name', () => {
      expect(() => assertClientMetricName('mc_join_total', 'throw')).toThrow(/dt_client_/);
    });

    it('throws for an uppercase name (regex non-match)', () => {
      expect(() => assertClientMetricName('dt_client_FOO_total', 'throw')).toThrow();
    });
  });

  describe("mode 'warn' (prod)", () => {
    it('returns true for a compliant name and does not warn', () => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      expect(assertClientMetricName('dt_client_signaling_connection_total', 'warn')).toBe(true);
      expect(warn).not.toHaveBeenCalled();
    });

    it('warns and returns false for a non-compliant name (never throws in prod)', () => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
      let result: boolean | undefined;
      expect(() => {
        result = assertClientMetricName('mc_join_total', 'warn');
      }).not.toThrow();
      expect(result).toBe(false);
      expect(warn).toHaveBeenCalledTimes(1);
      expect(warn).toHaveBeenCalledWith(expect.stringContaining('metric dropped'));
    });
  });

  describe('lockstep with CI dt-guard regex', () => {
    // The SDK guard MUST use the COMPILED pattern from
    // crates/dt-guard/src/ts_metric_naming.rs (R26_NAME_RE), NOT the `{0,53}`
    // doc form. They diverge on a trailing underscore: the compiled rule
    // REJECTS it. The Rust side reciprocally pins THIS literal in its
    // `r26_pattern_is_locked_to_the_sdk_runtime_guard` test, so a Rust-only edit
    // also trips a test (the two literals are mutually asserted).
    it('uses the compiled pattern (rejects a trailing underscore)', () => {
      expect(DT_CLIENT_NAME_PATTERN.source).toBe('^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$');
      // `dt_client_foo_` passes the `{0,53}` doc form but FAILS the compiled
      // rule — this assertion is the drift tripwire.
      expect(() => assertClientMetricName('dt_client_foo_', 'throw')).toThrow();
      expect(assertClientMetricName('dt_client_foo', 'throw')).toBe(true);
    });

    it('accepts a single-char body and rejects an empty body', () => {
      expect(assertClientMetricName('dt_client_a', 'throw')).toBe(true);
      expect(() => assertClientMetricName('dt_client_', 'throw')).toThrow();
    });

    it('accepts all five R-25 join-flow metric names', () => {
      for (const name of [
        'dt_client_join_attempts_total',
        'dt_client_time_to_signaling_ready_ms',
        'dt_client_time_to_first_mh_connected_ms',
        'dt_client_signaling_connection_total',
        'dt_client_mh_connection_total',
      ]) {
        expect(assertClientMetricName(name, 'throw')).toBe(true);
      }
    });
  });
});
