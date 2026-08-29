// File: packages/web-app/e2e/instanceCounters.ts
//
// Pure per-instance counter-delta logic for the browser E2E metrics helpers —
// the TS mirror of the Rust `crates/env-tests/src/fixtures/metrics.rs` decision
// functions. Kept in its OWN module with NO `./env` import so the vitest node
// unit test (`packages/web-app/tests/instanceCounters.test.ts`) can exercise it
// hermetically, without resolving the E2E environment.
//
// Single source of truth WITHIN this language: `mcMetrics.ts` owns the PromQL
// literals + the fetch / fail-loud transport layer and delegates the parsing +
// the "does any instance beat its baseline?" decision here. ACROSS languages,
// this is the deliberate mirror of the Rust `metrics.rs` functions — parity is
// held by convention + the mirrored unit tests (`tests/instanceCounters.test.ts`
// and the Rust `#[cfg(test)]` module), NOT structurally. Nothing enforces that
// the two stay identical; the parity tests (including the NaN/+Inf/empty-sample
// cases, where the two runtimes' default coercions actually diverge) are what
// keep them from drifting. Add a case to BOTH suites when you change either.

/** A per-instance snapshot of a counter: `instance` label (pod IP:port) → value. */
export type InstanceCounters = ReadonlyMap<string, number>;

/** One row of a Prometheus instant-query `data.result` array. */
export interface PromResultRow {
  readonly metric?: Readonly<Record<string, string>>;
  readonly value?: readonly [number, string];
}

/**
 * Build an {@link InstanceCounters} map from a `sum by (instance)(...)` instant
 * query's `data.result` rows (`promql` names the query in diagnostics). Pure
 * (no I/O): the caller has already performed the fetch and thrown on any
 * transport/status error, so an EMPTY `rows` here is the legitimately-zero,
 * series-not-yet-observed case → an empty map.
 *
 * Two anomalies THROW rather than being silently absorbed (mirrors the Rust
 * `results_to_instance_map`):
 * - a non-numeric sample value;
 * - a row with NO `instance` label. Defaulting such a row to the empty key
 *   would silently bucket every unlabeled series together — and if a future
 *   edit ever dropped `by (instance)` from the PromQL, every pod would collapse
 *   into that one bucket holding the cluster-wide sum, reverting this fix to
 *   `sum(...) > baseline` with nothing failing.
 */
export function resultsToInstanceMap(
  promql: string,
  rows: readonly PromResultRow[],
): InstanceCounters {
  const map = new Map<string, number>();
  for (const row of rows) {
    // Rows without an instant `value` carry nothing to compare; skip them
    // rather than inventing a zero.
    if (row.value === undefined) {
      continue;
    }
    const raw = row.value[1];
    // Reject empty/whitespace BEFORE numeric coercion: `Number("")` and
    // `Number(" ")` are 0 (would mask as a legitimate zero), whereas the Rust
    // twin's `"".parse::<f64>()` errors. Guarding here keeps the two aligned.
    if (raw.trim() === '') {
      throw new Error(`Prometheus returned an empty sample for query ${promql}`);
    }
    // `Number(raw)` (strict) rejects trailing garbage ("5abc"→NaN, matching
    // Rust's `parse`), and the `isFinite` guard rejects "NaN"/"+Inf"/"-Inf"
    // (Number("+Inf")→NaN; Rust rejects them via `is_finite()`). Using
    // `Number.parseFloat` instead would silently accept "5abc"→5 and diverge.
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
      throw new Error(
        `Prometheus returned a non-finite/non-numeric sample "${raw}" for query ${promql}`,
      );
    }
    const instance = row.metric?.instance;
    if (instance === undefined) {
      throw new Error(
        `Prometheus result row for query ${promql} has no "instance" label — the per-instance ` +
          `grouping key cannot be formed. A "sum by (instance)(...)" query must attach "instance"; ` +
          `a row without it means the grouping was lost (e.g. "by (instance)" was dropped from the ` +
          `PromQL), which would silently revert the per-instance comparison to a cluster-wide sum.`,
      );
    }
    map.set(instance, parsed);
  }
  return map;
}

/**
 * Pure per-instance monotonic-increase decision: does ANY currently-present
 * instance's value exceed its OWN baseline?
 *
 * An instance absent from `baseline` (a fresh pod that appeared after a
 * rollover) is treated as baseline 0, so it passes as soon as its value exceeds
 * zero. A stale instance that expired between baseline and now is simply absent
 * from `current` and cannot inflate anything — because no other pod shares its
 * `instance` value, per-instance comparison never sums across pods. That is the
 * whole point of the fix: the old cluster-wide `sum(...)` baseline was inflated
 * by soon-to-expire old-pod series, so fresh pods could never beat it.
 *
 * RESIDUAL FALSE-PASS WINDOW (@security, Gate 3 — this is NOT safe "by
 * construction"): treating an absent-from-baseline instance as baseline 0 is
 * sound for the intended case (a fresh post-rollover pod whose counter really
 * starts at 0), but NOT for a *pre-existing* pod that was merely MISSING from
 * the baseline read (its target unscraped beyond `query.lookback-delta`, or a
 * transient empty result) and then reappears carrying its full prior value —
 * that value exceeds 0 without the test having caused any increment, a narrow
 * false pass the old cluster-wide form could not produce (its churn failure was
 * one-directional, false-negative only). Rollover-robustness was traded for it;
 * there is no cheap way to tell a new pod from a returning one without pod start
 * time, so it is documented, not designed out.
 */
export function anyInstanceExceedsBaseline(
  baseline: InstanceCounters,
  current: InstanceCounters,
): boolean {
  for (const [instance, value] of current) {
    if (value > (baseline.get(instance) ?? 0)) {
      return true;
    }
  }
  return false;
}

/** Render a per-instance map for a diagnostic message: `{a=1, b=2}` (or `{}`). */
export function formatInstanceMap(map: InstanceCounters): string {
  if (map.size === 0) {
    // Self-describing rather than a bare `{}` (which reads as a broken format
    // placeholder): an empty snapshot means Prometheus answered but has NO
    // series for this selector — a different investigation from "instances
    // present, none incremented". Mirrors the Rust `format_instance_map`.
    return '{} (no instances — Prometheus returned an empty vector for this selector; check the MC/MH pods are up and scraped)';
  }
  // Sorted by instance so the diagnostic is deterministic across runs and
  // byte-comparable across Layer-7 retry attempts (the Rust twin sorts too).
  const entries = [...map.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([instance, value]) => `${instance}=${value}`);
  return `{${entries.join(', ')}}`;
}
