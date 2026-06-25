// File: packages/sdk-core/src/telemetry/ConsoleMetricsSink.ts
//
// R-24: the DEV-fallback `MetricsSink`. Logs each emission to the console
// instead of shipping to a collector — useful for local development without a
// running GC telemetry proxy. Name-guarded at the boundary (same as
// `OtelMetricsSink`): a non-compliant name throws in `'throw'` mode or warns +
// drops in `'warn'` mode, so a dev sees naming violations immediately.
//
// PII: this sink only ever logs the `(name, labels, value)` tuple the caller
// passed. Label PII-safety is the CALLER's responsibility (the R-25 catalog
// forbids user_id/email/IP/raw meeting id labels); the sink does not inspect or
// redact label VALUES — it trusts the catalog contract, identical to the OTel
// sink's pass-through.

import { assertClientMetricName, type GuardMode } from './nameGuard.js';
import type { MetricLabels, MetricsSink } from './MetricsSink.js';

/** A `MetricsSink` that logs emissions to `console.debug` (dev fallback). */
export class ConsoleMetricsSink implements MetricsSink {
  readonly #mode: GuardMode;

  /**
   * @param mode guard mode for non-compliant names (default `'throw'` — the
   *   dev-safe default; `configureTelemetry` passes `'warn'` only for prod).
   */
  constructor(mode: GuardMode = 'throw') {
    this.#mode = mode;
  }

  // PII-SAFETY of the `name` arg to `console.debug` below: the `ts-no-pii-in-logs`
  // guard flags the identifier `name` (it is in the person-name PII vocabulary),
  // but here `name` is a bounded `dt_client_*` METRIC name (regex-enforced by the
  // guard above), not user PII; `labels` are PII-free per the R-25 cardinality
  // contract; and this is a dev-only console sink. The `// pii-safe:` trailing
  // marker on each emit line is the guard's same-line allow annotation.

  counter(name: string, labels: MetricLabels, value: number = 1): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    console.debug('[dt-metric] counter', name, value, labels); // pii-safe: bounded dt_client_* metric name + PII-free labels (R-25), dev-only sink
  }

  histogram(name: string, labels: MetricLabels, value: number): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    console.debug('[dt-metric] histogram', name, value, labels); // pii-safe: bounded dt_client_* metric name + PII-free labels (R-25), dev-only sink
  }

  gauge(name: string, labels: MetricLabels, value: number): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    console.debug('[dt-metric] gauge', name, value, labels); // pii-safe: bounded dt_client_* metric name + PII-free labels (R-25), dev-only sink
  }
}
