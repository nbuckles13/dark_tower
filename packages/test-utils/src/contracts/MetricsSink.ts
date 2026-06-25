// File: packages/test-utils/src/contracts/MetricsSink.ts
//
// PATTERN A parallel structural copy of the `MetricsSink` + `MetricLabels`
// contract whose CANONICAL home is
// `packages/sdk-core/src/telemetry/MetricsSink.ts` (task #12, R-24;
// observability is the named convention author per ADR-0024 §6.5 Pattern B).
//
// WHY A COPY AND NOT A RE-EXPORT: a `export type ... from '@darktower/sdk-core'`
// re-export adds a test-utils → sdk-core package edge. Because sdk-core already
// devDepends test-utils (test doubles), that edge makes the Nx build task graph
// CIRCULAR (`sdk-core:build → test-utils:build → sdk-core:build`), which Nx
// rejects outright. So the contract is duplicated here as a parallel structural
// declaration (the same Pattern A the repo uses for `IWebTransport`).
//
// ANTI-DRIFT: a compile-time conformance test in sdk-core
// (`src/telemetry/__tests__/contractParity.test.ts`) asserts the two `MetricsSink`
// interfaces (and the two `MetricLabels` types) are mutually assignable, so a
// SHAPE change to one copy without the other fails `tsc`. KEEP THE INTERFACE +
// TYPE SIGNATURES SHAPE-IDENTICAL to the canonical sdk-core declaration; the
// `contractParity.test.ts` mutual-assignability check is the enforcement. NOTE:
// it enforces SHAPE, not bytes — the doc-comments here are intentionally trimmed
// vs the canonical copy and may differ; a comment-only edit is NOT machine-caught.
//
// The `dt_client_*` naming guard does NOT live in this interface — it is a
// boundary concern of the production sinks. Passive test recorders
// (`InMemoryMetricsSink`) accept any name so they can exercise guard-wrapping
// behavior on non-compliant names.

/**
 * String-typed labels at the boundary (mirrors the Rust
 * `metrics::{counter,histogram,gauge}!` macro shape). Cardinality discipline
 * (ADR-0011: ≤10 unique values per key, ≤64 char value length, ≤1000 unique
 * combos per metric) is the **caller's** responsibility — the sink does not
 * pre-mangle/normalize labels.
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
