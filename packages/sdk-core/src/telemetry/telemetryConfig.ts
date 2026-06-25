// File: packages/sdk-core/src/telemetry/telemetryConfig.ts
//
// SINGLE global telemetry providers (R-19, R-24). One `MeterProvider` + one
// `WebTracerProvider` for the whole SDK, configured ONCE via
// `configureTelemetry({ telemetryEndpoint, env })`. Registers the
// `W3CTraceContextPropagator` globally so `tracePropagation.injectIntoClientMessage`
// can inject the active span's context into outbound envelopes.
//
// SITING DECISION (Gate-1, approved): the story names `MeetingSession.configure`
// as the entry point, but `MeetingSession` (R-22) does not exist yet. Rather
// than introduce a throwaway shell that R-22 must later absorb, the config
// entry point lives HERE as `configureTelemetry`. When R-22 lands, its
// `MeetingSession.configure` will DELEGATE to this function (one line), so the
// named-in-spec API is preserved and traceable. Documented in client.md too.
//
// keepalive (R-24): the OTLP-proto browser exporter's fetch transport sets
// `keepalive` adaptively per request (on by default; backs off only above the
// browser's ~60KB / 9-concurrent budget). It is NOT a constructor option on the
// modern browser exporter — there is no flag to plumb. Our join-flow payloads
// are tiny, so keepalive is active in practice (metrics survive page-unload
// mid-join), which is R-24's intent. See OtelMetricsSink.ts + client.md.
//
// VERIFIABLE TRANSPORT PATH (traced @ 0.219.0, so reviewers can confirm intent-
// satisfied without re-deriving): we resolve the BROWSER package entry of
// `@opentelemetry/exporter-metrics-otlp-proto` (via its package.json `browser`
// field → `platform/browser/OTLPMetricExporter`), NOT the node entry. That
// browser exporter calls `createLegacyOtlpBrowserExportDelegate` →
// `createOtlpFetchExportDelegate` → `createFetchTransport`. In
// `otlp-exporter-base/.../transport/fetch-transport.js`, `FetchTransport.send()`
// sets `keepalive: useKeepalive` on the `fetch()` options PER REQUEST, where
// `useKeepalive = !wouldExceedSize && !wouldExceedCount` against the cumulative
// browser budget (MAX_KEEPALIVE_BODY_SIZE = 60KB, MAX_KEEPALIVE_REQUESTS = 9).
// This is the W3C-fetch `keepalive` flag — distinct from the node exporter's
// `keepAlive` HTTP-agent connection-pooling option, which we deliberately do
// NOT use (wrong concept for a browser SDK).
//
// Resource attrs (per the story): service.name=darktower-sdk-core,
// service.version=__SDK_VERSION__ (build-time define), service.namespace=
// darktower, deployment.environment=<env>.

import { metrics, propagation, type Meter, type Tracer } from '@opentelemetry/api';
import { W3CTraceContextPropagator } from '@opentelemetry/core';
import { OTLPMetricExporter } from '@opentelemetry/exporter-metrics-otlp-proto';
import { resourceFromAttributes } from '@opentelemetry/resources';
import { MeterProvider, PeriodicExportingMetricReader } from '@opentelemetry/sdk-metrics';
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web';
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_NAMESPACE,
  ATTR_SERVICE_VERSION,
} from '@opentelemetry/semantic-conventions';
import { ATTR_DEPLOYMENT_ENVIRONMENT } from '@opentelemetry/semantic-conventions/incubating';

import { OtelMetricsSink } from './OtelMetricsSink.js';
import type { GuardMode } from './nameGuard.js';
import type { MetricsSink } from './MetricsSink.js';

const SERVICE_NAME = 'darktower-sdk-core';
const SERVICE_NAMESPACE = 'darktower';
const METER_NAME = 'darktower-sdk-core';

/** Deployment environment. `prod` is the only mode that downgrades the name guard to warn. */
export type TelemetryEnv = 'production' | 'development' | 'test';

/** Options for {@link configureTelemetry} (the R-22 `MeetingSession.configure` payload). */
export interface TelemetryConfig {
  /**
   * Base URL of the GC telemetry proxy, e.g. `https://gc.example/api/v1/telemetry`.
   * The OTLP-proto metric exporter appends `/v1/metrics` (per task #12).
   */
  readonly telemetryEndpoint: string;
  /** Deployment environment; drives the name-guard mode + resource attribute. */
  readonly env: TelemetryEnv;
}

interface ConfiguredProviders {
  readonly meterProvider: MeterProvider;
  readonly tracerProvider: WebTracerProvider;
  readonly metricsSink: MetricsSink;
}

let configured: ConfiguredProviders | undefined;

/** Only prod warns-and-drops on a bad metric name; dev/test throw (fail loud). */
function guardModeFor(env: TelemetryEnv): GuardMode {
  return env === 'production' ? 'warn' : 'throw';
}

/**
 * Configure the SDK's single global telemetry providers. Idempotent-ish: a
 * second call replaces the previous configuration (last-config-wins). Safe to
 * call before any join; until called, the SDK uses `NoopMetricsSink` (telemetry
 * is opt-in).
 *
 * R-22's `MeetingSession.configure` will delegate here.
 */
export function configureTelemetry(config: TelemetryConfig): MetricsSink {
  const resource = resourceFromAttributes({
    [ATTR_SERVICE_NAME]: SERVICE_NAME,
    // `__SDK_VERSION__` is a build-time Vite define (see vite.config.ts).
    [ATTR_SERVICE_VERSION]: __SDK_VERSION__,
    [ATTR_SERVICE_NAMESPACE]: SERVICE_NAMESPACE,
    [ATTR_DEPLOYMENT_ENVIRONMENT]: config.env,
  });

  // --- Metrics: single MeterProvider exporting OTLP-proto to the GC proxy ---
  const exporter = new OTLPMetricExporter({
    // The browser exporter appends `v1/metrics`; pass the proxy base URL.
    url: `${config.telemetryEndpoint}/v1/metrics`,
  });
  const reader = new PeriodicExportingMetricReader({ exporter });
  const meterProvider = new MeterProvider({ resource, readers: [reader] });
  metrics.setGlobalMeterProvider(meterProvider);

  // --- Tracing: single WebTracerProvider + GLOBAL W3C propagator (R-19) ---
  const tracerProvider = new WebTracerProvider({ resource });
  tracerProvider.register({ propagator: new W3CTraceContextPropagator() });

  const meter: Meter = meterProvider.getMeter(METER_NAME, __SDK_VERSION__);
  const metricsSink = new OtelMetricsSink(meter, guardModeFor(config.env));

  configured = { meterProvider, tracerProvider, metricsSink };
  return metricsSink;
}

/** The configured global `Meter`, or `undefined` if `configureTelemetry` has not run. */
export function getMeter(): Meter | undefined {
  return configured?.meterProvider.getMeter(METER_NAME, __SDK_VERSION__);
}

/** The configured global `Tracer`, or `undefined` if `configureTelemetry` has not run. */
export function getTracer(): Tracer | undefined {
  return configured?.tracerProvider.getTracer(METER_NAME, __SDK_VERSION__);
}

/** The configured production `MetricsSink`, or `undefined` if unconfigured. */
export function getMetricsSink(): MetricsSink | undefined {
  return configured?.metricsSink;
}

/**
 * Reset global telemetry state. TEST-ONLY seam — lets a unit test re-run
 * `configureTelemetry` cleanly. Not part of the production join flow.
 */
export function resetTelemetryForTest(): void {
  configured = undefined;
  propagation.disable();
}
