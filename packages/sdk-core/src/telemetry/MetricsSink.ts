// File: packages/sdk-core/src/telemetry/MetricsSink.ts
//
// CANONICAL HOME (R-24, task #12) of the `MetricsSink` + `MetricLabels`
// contract. Per ADR-0024 §6.5 Pattern B, observability is the named convention
// author; this module is the single source of truth for the SHAPE.
//
// `packages/test-utils/src/contracts/MetricsSink.ts` keeps a PARALLEL STRUCTURAL
// COPY (Pattern A), NOT a re-export. A type-only re-export was attempted but
// REVERTED: it adds a `test-utils → @darktower/sdk-core` package edge that —
// because sdk-core already devDepends test-utils — makes the Nx build task graph
// circular (`sdk-core:build → test-utils:build → sdk-core:build`), which Nx
// rejects. The two copies are kept SHAPE-identical by the compile-time
// `src/telemetry/__tests__/contractParity.test.ts` mutual-assignability check
// (it fails `tsc` on a shape divergence). DO NOT "consolidate" the copy into a
// re-export — that reopens the cycle. See TODO.md line 62 for the full record.
//
// All four sinks conform to this shape: `OtelMetricsSink` (production),
// `ConsoleMetricsSink` (dev fallback), `NoopMetricsSink` (default), and the
// test-only `InMemoryMetricsSink` (in `@darktower/test-utils`).
//
// The `dt_client_*` naming guard does NOT live in this interface — it is a
// boundary concern of the production sinks (`nameGuard.ts`). Passive test
// recorders accept any name so they can exercise guard-wrapping behavior on
// non-compliant names.

/**
 * String-typed labels at the boundary (mirrors the Rust
 * `metrics::{counter,histogram,gauge}!` macro shape). Cardinality discipline
 * (ADR-0011: ≤10 unique values per key, ≤64 char value length, ≤1000 unique
 * combos per metric) is the **caller's** responsibility — the sink does not
 * pre-mangle/normalize labels.
 *
 * Wire reality: labels are always strings on the OTLP / Prometheus wire.
 * Production sinks accept this same string shape and pass through to the OTel
 * Meter. If a future ergonomic wrapper accepts numeric label values, that
 * wrapper MUST stringify before reaching this interface, and the stringified
 * values still count toward the ADR-0011 cardinality budget.
 */
export type MetricLabels = Readonly<Record<string, string>>;

/**
 * Metric emission contract. Three methods mirror Rust's
 * `metrics::{counter,histogram,gauge}!` macros so cross-language reasoning
 * stays consistent.
 *
 * Argument order is `(name, labels, value?)`:
 *   - `labels` is required (pass `{}` for no labels).
 *   - `value` is optional for `counter` (defaults to 1).
 *   - `value` is required for `histogram` and `gauge`.
 *
 * The R-24 `dt_client_*` naming guard does NOT live in this interface.
 * Production sinks enforce it; passive test recorders accept any name so tests
 * can verify guard-wrapping behavior on non-compliant names.
 */
export interface MetricsSink {
  /**
   * Increment a counter by `value` (default 1) for the given `(name, labels)`
   * tuple.
   */
  counter(name: string, labels: MetricLabels, value?: number): void;

  /**
   * Record an observation of `value` against a histogram identified by
   * `(name, labels)`.
   */
  histogram(name: string, labels: MetricLabels, value: number): void;

  /**
   * Set a gauge to `value` for `(name, labels)`. Most-recent-write-wins
   * semantics mirror standard gauge behavior.
   */
  gauge(name: string, labels: MetricLabels, value: number): void;
}
