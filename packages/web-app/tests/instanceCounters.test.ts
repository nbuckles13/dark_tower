// Mirror-parity unit test for the per-instance counter-delta decision — the TS
// twin of the Rust `crates/env-tests/src/fixtures/metrics.rs` `#[cfg(test)]`
// cases. Runs in the node tier (`pnpm test:unit` -> `vitest.node.config.ts`,
// `include: tests/**/*.test.ts`) because `./instanceCounters` is pure and has NO
// `./env` import, so no E2E environment is needed.
//
// The two language families MUST keep identical semantics (single source of
// truth — the whole point of the counter-delta hardening task); these cases pin
// that the TS decision behaves like the Rust one:
//   (a) an old instance present at baseline and GONE at poll, with a NEW
//       instance above zero -> MUST pass;
//   (b) all instances present, none increased -> MUST NOT pass;
//   (c) a non-numeric sample (the analogue of the Rust query error / anomaly)
//       -> MUST throw loudly, not read as zero;
//   plus: an empty result -> empty map (legitimately zero, distinct from error).

import { expect, test } from 'vitest';

import {
  anyInstanceExceedsBaseline,
  resultsToInstanceMap,
  type PromResultRow,
} from '../e2e/instanceCounters.js';

const TEST_PROMQL = 'sum by (instance) (mc_participant_mh_status_total{state="connected"})';

/** Build `sum by (instance)(...)`-shaped result rows from (instance, value) pairs. */
function rows(pairs: ReadonlyArray<readonly [string, string]>): PromResultRow[] {
  return pairs.map(([instance, value]) => ({ metric: { instance }, value: [0, value] }));
}

test('(a) passes when the old instance is gone and a fresh instance is above zero', () => {
  // Baseline captured before a rollover: only the old pod, at 5.
  const baseline = resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.1:8081', '5']]));
  // After the rollover the old series expired and a FRESH pod (new IP, absent
  // from baseline) is at 1. Cluster-wide sum would have blocked this; the
  // per-instance decision must PASS.
  const current = resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.2:8081', '1']]));
  expect(anyInstanceExceedsBaseline(baseline, current)).toBe(true);
});

test('(b) does not pass when all instances are present and none increased', () => {
  const baseline = resultsToInstanceMap(
    TEST_PROMQL,
    rows([
      ['10.0.0.1:8081', '5'],
      ['10.0.0.2:8081', '3'],
    ]),
  );
  const current = resultsToInstanceMap(
    TEST_PROMQL,
    rows([
      ['10.0.0.1:8081', '5'],
      ['10.0.0.2:8081', '3'],
    ]),
  );
  expect(anyInstanceExceedsBaseline(baseline, current)).toBe(false);
});

test('(c) a non-numeric sample throws loudly, not read as zero', () => {
  expect(() => resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.1:8081', 'not-a-number']]))).toThrow(
    /non-finite\/non-numeric/,
  );
});

// Fail-loud parse parity with the Rust twin (@code-reviewer + @test-reviewer,
// Gate 3): pinned on exactly the inputs where the two runtimes' DEFAULT coercions
// diverge, so a future refactor that reintroduces `parseFloat` / a bare `Number`
// is caught. "NaN"/"+Inf" are the sharp ones — the old `parseFloat` path let
// "+Inf" through as a silent FALSE PASS; "" is the one the `Number(...)` fix
// itself would reintroduce (`Number("")===0`) if the empty-guard were dropped.
test('a "NaN" sample throws loudly (not masked as no-increment)', () => {
  expect(() => resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.1:8081', 'NaN']]))).toThrow(
    /non-finite\/non-numeric/,
  );
});

test('a "+Inf" sample throws loudly (not a silent false-pass)', () => {
  expect(() => resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.1:8081', '+Inf']]))).toThrow(
    /non-finite\/non-numeric/,
  );
});

test('an empty-string sample throws loudly (not coerced to zero)', () => {
  expect(() => resultsToInstanceMap(TEST_PROMQL, rows([['10.0.0.1:8081', '']]))).toThrow(
    /empty sample/,
  );
});

test('a row with no instance label throws loudly, not bucketed under the empty key', () => {
  // @security Gate 3: silently defaulting a label-less row to "" would re-open
  // cluster-wide summation if `by (instance)` were ever dropped from the PromQL.
  const labelless: PromResultRow[] = [{ metric: {}, value: [0, '7'] }];
  expect(() => resultsToInstanceMap(TEST_PROMQL, labelless)).toThrow(/no "instance" label/);
});

test('an empty result is an empty map (legitimately zero, distinct from an error)', () => {
  const map = resultsToInstanceMap(TEST_PROMQL, []);
  expect(map.size).toBe(0);
  // Empty baseline vs empty current does not pass.
  expect(anyInstanceExceedsBaseline(map, map)).toBe(false);
});
