// File: packages/test-utils/src/contracts/MetricsSink.ts
//
// MetricsSink contract — co-owned with the observability specialist per
// user-story §Design line 256 (named convention author per ADR-0024 §6.5
// Pattern B). When sdk-core (task #12, R-24) lands, it will declare its
// canonical version in `packages/sdk-core/src/telemetry/MetricsSink.ts`.
// All four sinks (`OtelMetricsSink`, `InMemoryMetricsSink`, `ConsoleMetricsSink`,
// `NoopMetricsSink`) MUST conform to the same shape. The canonical-home
// decision is tracked in docs/TODO.md as a Gate 3 follow-up.

/**
 * String-typed labels at the boundary (mirrors the Rust
 * `metrics::{counter,histogram,gauge}!` macro shape). Cardinality
 * discipline (ADR-0011: ≤10 unique values per key, ≤64 char value length,
 * ≤1000 unique combos per metric) is the **caller's** responsibility — the
 * sink does not pre-mangle/normalize labels.
 *
 * Wire reality: labels are always strings on the OTLP / Prometheus wire.
 * Production sinks (`OtelMetricsSink`, etc.) accept this same string shape
 * and pass through to the OTel Meter. If a future ergonomic wrapper
 * accepts numeric label values, that wrapper MUST stringify before
 * reaching this interface, and the stringified values still count toward
 * the ADR-0011 cardinality budget.
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
 * Production sinks enforce it; passive test recorders accept any name so
 * tests can verify guard-wrapping behavior on non-compliant names.
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
