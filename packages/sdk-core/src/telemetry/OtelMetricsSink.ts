// File: packages/sdk-core/src/telemetry/OtelMetricsSink.ts
//
// R-24: the PRODUCTION `MetricsSink`. Wraps an OTel JS `Meter` and forwards
// each emission to a cached instrument (`Counter`/`Histogram`/`Gauge`). The
// `Meter` is INJECTED via the constructor (not created here) so:
//   - `configureTelemetry` owns the single global `MeterProvider` and hands
//     this sink a meter from it (one provider per SDK, per the story);
//   - the unit tier passes a fake meter and asserts instrument creation +
//     name-guarding WITHOUT a real OTel SDK (OTel is externalized from the
//     bundle — see vite.config.ts `external`).
//
// Name guard: every name is run through `assertClientMetricName` at the
// boundary BEFORE an instrument is created/looked up, so a non-compliant name
// never reaches the meter (throws in dev/test, warns + drops in prod). This is
// the runtime half of the `dt_client_*` convention; the static half is the CI
// `dt-guard` (same regex, kept in lockstep — see nameGuard.ts).
//
// Instrument caching: OTel meters de-duplicate instruments by name internally,
// but creating an instrument object per emission is wasteful on the join hot
// path. We cache by name so repeated `counter('dt_client_join_attempts_total',
// ...)` calls reuse one `Counter`. Labels are passed per-call as attributes —
// the instrument is keyed by NAME only (labels are a recording-time concern).
//
// keepalive (R-24): NOT configured here. The OTLP-proto exporter's browser
// fetch transport sets `keepalive` adaptively per request (on by default,
// backs off above the browser's ~60KB / 9-concurrent budget). Our join-flow
// payloads are tiny, so keepalive is active in practice — metrics survive a
// page-unload/navigation mid-join, which is R-24's intent. There is no
// `keepalive` config knob on the modern browser exporter to plumb. See
// telemetryConfig.ts and docs/observability/metrics/client.md.

import type { Counter, Gauge, Histogram, Meter } from '@opentelemetry/api';

import { assertClientMetricName, type GuardMode } from './nameGuard.js';
import type { MetricLabels, MetricsSink } from './MetricsSink.js';

/** Production `MetricsSink` forwarding to an injected OTel `Meter`. */
export class OtelMetricsSink implements MetricsSink {
  readonly #meter: Meter;
  readonly #mode: GuardMode;

  readonly #counters = new Map<string, Counter>();
  readonly #histograms = new Map<string, Histogram>();
  readonly #gauges = new Map<string, Gauge>();

  /**
   * @param meter an OTel `Meter` obtained from the global `MeterProvider`
   *   (`configureTelemetry`). Injected so the unit tier can pass a fake.
   * @param mode guard mode for non-compliant names (default `'throw'`).
   */
  constructor(meter: Meter, mode: GuardMode = 'throw') {
    this.#meter = meter;
    this.#mode = mode;
  }

  counter(name: string, labels: MetricLabels, value: number = 1): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    let instrument = this.#counters.get(name);
    if (instrument === undefined) {
      // dt-metric-name-dynamic: name is runtime-validated by assertClientMetricName(name, mode) above (R-24 sink boundary); this is the canonical generic MetricsSink, the sole legitimate dynamic createX caller
      instrument = this.#meter.createCounter(name);
      this.#counters.set(name, instrument);
    }
    instrument.add(value, labels);
  }

  histogram(name: string, labels: MetricLabels, value: number): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    let instrument = this.#histograms.get(name);
    if (instrument === undefined) {
      // dt-metric-name-dynamic: name is runtime-validated by assertClientMetricName(name, mode) above (R-24 sink boundary); this is the canonical generic MetricsSink, the sole legitimate dynamic createX caller
      instrument = this.#meter.createHistogram(name);
      this.#histograms.set(name, instrument);
    }
    instrument.record(value, labels);
  }

  gauge(name: string, labels: MetricLabels, value: number): void {
    if (!assertClientMetricName(name, this.#mode)) return;
    let instrument = this.#gauges.get(name);
    if (instrument === undefined) {
      // dt-metric-name-dynamic: name is runtime-validated by assertClientMetricName(name, mode) above (R-24 sink boundary); this is the canonical generic MetricsSink, the sole legitimate dynamic createX caller
      instrument = this.#meter.createGauge(name);
      this.#gauges.set(name, instrument);
    }
    instrument.record(value, labels);
  }
}
