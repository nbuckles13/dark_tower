// File: packages/test-utils/src/InMemoryMetricsSink.ts
//
// IMPORTANT: This is a passive recorder. The R-24 `dt_client_*` naming guard
// is a PRODUCTION sink concern (`OtelMetricsSink` in sdk-core task #12) and
// MUST NOT be enforced here — `InMemoryMetricsSink` is also used to test the
// guard's wrapping behavior, which requires recording non-compliant names.
// Self-tests in this package use only `dt_client_test_*` names to model good
// citizenship for downstream consumers who copy the test scaffolding.

import type { MetricLabels, MetricsSink } from './contracts/MetricsSink.js';

/** Discriminator for entries in the recorded metric history. */
export type RecordedKind = 'counter' | 'histogram' | 'gauge';

/**
 * One recorded metric emission. Captured in insertion order; tests asserting
 * on emit ordering iterate `getRecordedMetrics()`.
 */
export interface RecordedMetric {
  readonly kind: RecordedKind;
  readonly name: string;
  readonly labels: MetricLabels;
  readonly value: number;
  /** `Date.now()` at the time of recording, for ordering / debugging. */
  readonly recordedAt: number;
}

/**
 * In-memory `MetricsSink` implementation for tests. Records every emission
 * and exposes inspection + assertion helpers mirroring the Rust ADR-0032
 * `MetricAssertion::with_labels(...)` vocabulary.
 *
 * **SUBSET-FILTER label semantics for both read AND assert APIs** (per
 * @observability Gate 1 final lock — Path A). The `labels` argument is
 * treated as a subset filter throughout: a recorded entry matches if all
 * `labels` keys are present and equal on the recorded entry; extra
 * labels on the recorded entry are ignored. Empty `labels: {}` matches
 * ALL recorded entries for that metric name. Single coherent semantic
 * for both paths — mirrors Rust `MetricAssertion::with_labels(...)`.
 *
 * - **Read APIs aggregate across matches**: `getCounter` SUMs values;
 *   `getHistogramObservations` returns the concatenated ordered
 *   observations across all matches; `getGauge` returns the
 *   LAST-RECORDED matching value (`recordedAt`-ordered) — tests with
 *   multiple gauge series should pass exact labels to disambiguate.
 * - **Assertion APIs sum/count across matches** then compare to the
 *   expected value/range/count.
 *
 * **Read APIs NEVER throw** — they return 0 / empty / undefined for
 * no-match.
 *
 * **Assertion APIs THROW on mismatch** with descriptive errors.
 *
 * @example
 * const sink = new InMemoryMetricsSink();
 * sink.counter('dt_client_test_join_total', { outcome: 'success', mh: '0' });
 * sink.counter('dt_client_test_join_total', { outcome: 'success', mh: '1' });
 * // Subset filter: ignores `mh`, sums both.
 * sink.assertCounter('dt_client_test_join_total', { outcome: 'success' }, 2);
 * expect(sink.getCounter('dt_client_test_join_total', { outcome: 'success' })).toBe(2);
 */
export class InMemoryMetricsSink implements MetricsSink {
  readonly #recorded: RecordedMetric[] = [];

  // ---------------- Production interface ----------------

  /** Increment a counter by `value` (default 1). */
  counter(name: string, labels: MetricLabels, value: number = 1): void {
    this.#recorded.push({
      kind: 'counter',
      name,
      labels: freezeLabels(labels),
      value,
      recordedAt: Date.now(),
    });
  }

  /** Record a histogram observation of `value`. */
  histogram(name: string, labels: MetricLabels, value: number): void {
    this.#recorded.push({
      kind: 'histogram',
      name,
      labels: freezeLabels(labels),
      value,
      recordedAt: Date.now(),
    });
  }

  /** Set a gauge to `value` (last-write-wins on read). */
  gauge(name: string, labels: MetricLabels, value: number): void {
    this.#recorded.push({
      kind: 'gauge',
      name,
      labels: freezeLabels(labels),
      value,
      recordedAt: Date.now(),
    });
  }

  // ---------------- Inspection (never throw, SUBSET-FILTER labels) ----------------

  /**
   * Sum of recorded counter values for `(name, labels-as-subset)`.
   * Subset filter; recorded entry matches if all assertion labels are
   * present and equal; extra labels are ignored. Empty `labels: {}` sums
   * across all recorded counters for `name`. Returns 0 if no match.
   */
  getCounter(name: string, labels: MetricLabels): number {
    return this.#recorded
      .filter((r) => r.kind === 'counter' && r.name === name && labelsMatchSubset(labels, r.labels))
      .reduce((acc, r) => acc + r.value, 0);
  }

  /**
   * Concatenated ordered observations across all histogram entries
   * matching `(name, labels-as-subset)`. Subset filter; recorded entry
   * matches if all assertion labels are present and equal; extra labels
   * are ignored. Empty `labels: {}` matches all histogram entries for
   * `name`. Returns empty array if no match.
   */
  getHistogramObservations(name: string, labels: MetricLabels): readonly number[] {
    return this.#recorded
      .filter((r) => r.kind === 'histogram' && r.name === name && labelsMatchSubset(labels, r.labels))
      .map((r) => r.value);
  }

  /**
   * Returns the LAST-RECORDED matching gauge value for
   * `(name, labels-as-subset)` (recordedAt-ordered). Subset filter;
   * recorded entry matches if all assertion labels are present and
   * equal; extra labels are ignored. Tests with multiple gauge series
   * should pass exact labels to disambiguate. Returns `undefined` if no
   * match.
   */
  getGauge(name: string, labels: MetricLabels): number | undefined {
    let result: number | undefined = undefined;
    for (const r of this.#recorded) {
      if (r.kind === 'gauge' && r.name === name && labelsMatchSubset(labels, r.labels)) {
        result = r.value;
      }
    }
    return result;
  }

  /** Full ordered history of every emission. Useful for debugging or order assertions. */
  getRecordedMetrics(): readonly RecordedMetric[] {
    return this.#recorded;
  }

  /** Reset between tests. */
  clear(): void {
    this.#recorded.length = 0;
  }

  // ---------------- Assertion helpers (throw on mismatch) ----------------

  /**
   * Assert the summed counter value for `(name, labels-as-subset)` equals
   * `expected`. Throws if no matching entries exist or the sum differs.
   * Subset semantics: extra labels on recorded entries are ignored.
   */
  assertCounter(name: string, labels: MetricLabels, expected: number): void {
    const matches = this.#recorded.filter(
      (r) => r.kind === 'counter' && r.name === name && labelsMatchSubset(labels, r.labels),
    );
    if (matches.length === 0) {
      throw new Error(
        `assertCounter: no counter entries found for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}`,
      );
    }
    const sum = matches.reduce((acc, r) => acc + r.value, 0);
    if (sum !== expected) {
      throw new Error(
        `assertCounter: expected sum=${expected} for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}, got ${sum}`,
      );
    }
  }

  /**
   * Assert the summed counter value for `(name, labels-as-subset)` is at
   * least `min`. For unpredictable counts (retries, race-y emissions).
   * Subset filter; see `assertCounter`.
   */
  assertCounterAtLeast(name: string, labels: MetricLabels, min: number): void {
    const sum = this.getCounter(name, labels);
    if (sum < min) {
      throw new Error(
        `assertCounterAtLeast: expected sum>=${min} for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}, got ${sum}`,
      );
    }
  }

  /**
   * Assert at least `minCount` (default 1) histogram observations were
   * recorded for `(name, labels-as-subset)`. For "the metric was emitted,
   * exact value not predictable" patterns. Subset filter — extras on
   * recorded entries ignored; empty `labels: {}` matches all entries
   * for `name`.
   */
  assertHistogramObserved(name: string, labels: MetricLabels, minCount: number = 1): void {
    const count = this.getHistogramObservations(name, labels).length;
    if (count < minCount) {
      throw new Error(
        `assertHistogramObserved: expected count>=${minCount} for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}, got ${count}`,
      );
    }
  }

  /**
   * Assert that there exists at least one gauge value for
   * `(name, labels-as-subset)` and that the most-recently-recorded matching
   * value is within the inclusive `[min, max]` range. For unpredictable
   * values (timestamps, durations, queue depths). Subset filter; throws
   * if no matching gauge exists.
   */
  assertGaugeInRange(
    name: string,
    labels: MetricLabels,
    range: { min: number; max: number },
  ): void {
    const value = this.getGauge(name, labels);
    if (value === undefined) {
      throw new Error(
        `assertGaugeInRange: no gauge entries found for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}`,
      );
    }
    if (value < range.min || value > range.max) {
      throw new Error(
        `assertGaugeInRange: expected ${range.min}<=value<=${range.max} for name=${JSON.stringify(name)} labels=${JSON.stringify(labels)}, got ${value}`,
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function freezeLabels(labels: MetricLabels): MetricLabels {
  return Object.freeze({ ...labels });
}

function labelsMatchSubset(filter: MetricLabels, recorded: MetricLabels): boolean {
  for (const key of Object.keys(filter)) {
    if (recorded[key] !== filter[key]) return false;
  }
  return true;
}
