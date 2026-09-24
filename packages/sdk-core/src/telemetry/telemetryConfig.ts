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

import {
  diag,
  DiagConsoleLogger,
  DiagLogLevel,
  metrics,
  propagation,
  type Meter,
  type Tracer,
} from '@opentelemetry/api';
import { W3CTraceContextPropagator } from '@opentelemetry/core';
import { OTLPMetricExporter } from '@opentelemetry/exporter-metrics-otlp-proto';
import { resourceFromAttributes } from '@opentelemetry/resources';
import {
  AggregationTemporality,
  MeterProvider,
  PeriodicExportingMetricReader,
} from '@opentelemetry/sdk-metrics';
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web';
import {
  ATTR_SERVICE_NAME,
  ATTR_SERVICE_NAMESPACE,
  ATTR_SERVICE_VERSION,
} from '@opentelemetry/semantic-conventions';
import { ATTR_DEPLOYMENT_ENVIRONMENT } from '@opentelemetry/semantic-conventions/incubating';

import { DEFAULT_METRIC_EXPORT_INTERVAL_MS } from '../config/clientConfig.js';
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
   *
   * **THIS URL RECEIVES THE USER'S BEARER TOKEN.** Every export carries
   * `Authorization: Bearer <userToken>` when {@link authTokenProvider} yields one
   * (GC's telemetry route is behind `require_user_auth` and rejects otherwise).
   * So this is not merely "where metrics go" — pointing it at a third-party
   * collector hands that party a live user credential. It is deliberately
   * config-supplied rather than user-supplied, and the app points it same-origin.
   */
  readonly telemetryEndpoint: string;
  /**
   * Returns the CURRENT user bearer token, or `undefined` when unauthenticated.
   *
   * A GETTER, NOT A TOKEN, and that is the whole point. GC's telemetry proxy is
   * behind `require_user_auth`, so exports need a credential — but telemetry is
   * configured once while the token arrives later, changes on re-auth, and is
   * cleared on sign-out. Passing a VALUE here would capture whichever of those
   * moments happened to be first.
   *
   * It must READ the application's single session home rather than hold a copy.
   * A second holder of a bearer credential needs its own clearing on sign-out and
   * on the 401 path, and drifts from the first the moment one is cleared and the
   * other is not — which is the defect class `lib/types.ts` documents at length
   * for `AuthSession.userToken`.
   *
   * The SDK never stores the returned value: it is read per export and used to
   * build one header.
   */
  readonly authTokenProvider?: () => string | undefined;
  /** Deployment environment; drives the name-guard mode + resource attribute. */
  readonly env: TelemetryEnv;
  /**
   * OTel metric export interval, in milliseconds. Defaults to
   * {@link DEFAULT_METRIC_EXPORT_INTERVAL_MS} (the client cadence observability
   * documents).
   *
   * THIS IS GLOBAL, NOT MEDIA-ONLY. The SDK has ONE `MeterProvider` (R-19/R-24),
   * so this cadence applies to EVERY `dt_client_*` metric — including the
   * ADR-0028 join-flow metrics grandfathered by ADR-0036 §11. Moving from the
   * OTel JS default of 60 s to 10 s is a ~6x export-volume change with a blast
   * radius wider than the media path; it was checked against GC's telemetry
   * proxy limits (`TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE = 60`, per-`sub`,
   * per-pod) before landing, taking a client from ~1 to ~6 exports/min.
   *
   * Do NOT "scope" the cadence by standing up a second `MeterProvider` — that
   * breaks R-19's single-provider requirement. If a media-only cadence is ever
   * genuinely needed, it is a design change, not a constructor argument.
   */
  readonly metricExportIntervalMs?: number;
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
 * Build the OTLP metric exporter. Extracted so the temporality choice below is
 * testable against the REAL exporter rather than against the argument passed to
 * it — see `__tests__/telemetryConfig.test.ts`.
 *
 * ---------------------------------------------------------------------------
 * DELTA IS DELIBERATE: AGGREGATION HAPPENS IN THE COLLECTOR, NOT PER BROWSER
 * ---------------------------------------------------------------------------
 *
 * Every browser in a meeting emits the SAME series identity
 * (`{client_version, org_id, key_custody}` — ADR-0036 §11 permits no participant
 * dimension, and a per-session `service.instance.id` is the same thing under
 * another name). With CUMULATIVE temporality, N browsers each export their own
 * lifetime total under one identity and the collector's exporter keeps whichever
 * arrived last: the stored series oscillates between unrelated totals and
 * `rate()` sees a reset on nearly every scrape.
 *
 * With DELTA, each export carries only that interval's increment, so the
 * collector's `delta_to_cumulative` processor SUMS them into one correct
 * cumulative series with no per-browser label
 * (`infra/services/otel-collector/configmap.yaml`). That is the entire reason
 * the fleet can be counted without identifying anyone.
 *
 * ---------------------------------------------------------------------------
 * WHY THE TEST ASSERTS BEHAVIOUR AND NOT THIS ARGUMENT
 * ---------------------------------------------------------------------------
 *
 * The exporter's internal selector compares against
 * `AggregationTemporalityPreference.DELTA`, a DIFFERENT enum from the
 * `AggregationTemporality.DELTA` passed here. They agree today only because both
 * happen to use the value `0`; `AggregationTemporalityPreference` is not
 * re-exported by the proto package, so using it would mean a new dependency for
 * a numerically identical value. If either enum is ever renumbered, this silently
 * falls through to CUMULATIVE and the whole client pipeline goes quietly wrong —
 * which is why the test builds the real exporter and asserts what it SELECTS.
 */
export function createMetricExporter(
  telemetryEndpoint: string,
  authTokenProvider?: () => string | undefined,
): OTLPMetricExporter {
  return new OTLPMetricExporter({
    // The browser exporter appends `v1/metrics`; pass the proxy base URL.
    url: `${telemetryEndpoint}/v1/metrics`,
    temporalityPreference: AggregationTemporality.DELTA,
    // A FACTORY, NOT A FIXED RECORD, AND IT MUST STAY ONE. The exporter awaits
    // this on EVERY export (`otlp-exporter-base`'s `HeadersFactory`).
    //
    // TWO reasons, and the second is the one that gets optimised away. (1)
    // ROTATION: telemetry is configured once at mount, the token appears at
    // sign-in and changes on re-auth, so a header record captured at construction
    // would freeze whichever of those was true first — for the app's real
    // ordering, "no token", forever. (2) RETENTION: caching the resolved token
    // here would give a bearer credential a SECOND home inside the SDK, one that
    // no sign-out path clears — so a token the user revoked would keep being sent.
    // THERE IS NO MECHANICAL BACKSTOP FOR THIS, which is why it is spelled out
    // rather than left to review. `crates/dt-guard/src/ts_retained_credentials.rs`
    // does scan this package, but it indexes DECLARATION MEMBERS — interface and
    // class fields — so a `let cached` inside this closure matches nothing it
    // inspects. The per-export unit test in
    // `__tests__/telemetryConfig.delta.test.ts` is the only executable defence,
    // and it asserts through the real exporter's factory precisely so it proves
    // the EXPORTER re-asks rather than merely that the provider works.
    //
    // So: do not memoise the token "to save an await". The SDK holds the function
    // REFERENCE and never a resolved value.
    //
    // Without this, every export 401s at GC's `require_user_auth`. That failure
    // is INVISIBLE on both sides: the middleware is a route layer, so it rejects
    // before the handler and `gc_telemetry_ingest_total` never increments even as
    // a rejection; and OTel reports export failure through `diag`. The result
    // would be empty panels, a silent console and a zero counter — exactly the
    // shape of "no browser connected".
    //
    // The header is OMITTED ENTIRELY when there is no token, never sent as
    // `Bearer undefined`. In practice the unauthenticated case does not arise:
    // `PeriodicExportingMetricReader` skips the export when nothing was recorded,
    // and every `dt_client_*` metric fires after sign-in. So an export with no
    // token means something real — post-sign-out residue, or a misconfigured
    // provider — and is worth letting fail rather than suppressing.
    headers: () => {
      const token = authTokenProvider?.();
      return Promise.resolve(token === undefined ? {} : { Authorization: `Bearer ${token}` });
    },
  });
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
  // OUTSIDE PRODUCTION, MAKE EXPORT FAILURE AUDIBLE. OTel reports export errors
  // — a 401 from the telemetry proxy, a CORS rejection, an unreachable endpoint —
  // through `diag`, and nothing registers a `diag` logger by default, so they go
  // nowhere. Combined with GC's route-layer auth (which rejects before the handler
  // and so never increments the ingest counter), a broken token produces empty
  // panels, a silent console and a zero counter: indistinguishable from nobody
  // being connected. One console logger in dev/test removes that blind spot at
  // the only moment anyone is watching.
  //
  // NOT in production, following `guardModeFor`'s existing precedent for the same
  // trade-off: a per-export console error on a flaky network is noise a user
  // cannot act on.
  //
  // VERSION-CHECKED, NOT ASSUMED: this logger could itself become the leak if the
  // exporter put the outgoing request in its error. At the pinned
  // `otlp-exporter-base` 0.221.0 it does not — `fetch-transport.js` builds the
  // non-retryable error as `Fetch request failed with non-retryable status
  // ${response.status}` (the status number only), network errors pass the fetch
  // rejection as `cause`, and both `diag` calls interpolate `${error}`, never the
  // request. So no `Authorization` header can reach the console.
  //
  // RE-CHECK THIS ON ANY EXPORTER BUMP. `ts_pii.rs` scans `console.*`/`logger.*`
  // call sites, so it would give NO signal if a future version started attaching
  // request context to the error — this is review-enforced, not guarded.
  if (config.env !== 'production') {
    diag.setLogger(new DiagConsoleLogger(), DiagLogLevel.ERROR);
  }

  const resource = resourceFromAttributes({
    [ATTR_SERVICE_NAME]: SERVICE_NAME,
    // `__SDK_VERSION__` is a build-time Vite define (see vite.config.ts).
    [ATTR_SERVICE_VERSION]: __SDK_VERSION__,
    [ATTR_SERVICE_NAMESPACE]: SERVICE_NAMESPACE,
    [ATTR_DEPLOYMENT_ENVIRONMENT]: config.env,
  });

  // --- Metrics: single MeterProvider exporting OTLP-proto to the GC proxy ---
  const exporter = createMetricExporter(config.telemetryEndpoint, config.authTokenProvider);
  // Read from ONE named configuration point, never a literal here. See
  // `TelemetryConfig.metricExportIntervalMs` for the global blast radius.
  const reader = new PeriodicExportingMetricReader({
    exporter,
    exportIntervalMillis: config.metricExportIntervalMs ?? DEFAULT_METRIC_EXPORT_INTERVAL_MS,
  });
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
 * Flush pending metric deltas immediately.
 *
 * Called on media/session teardown. At a 10 s export cadence, up to 10 s of
 * counters otherwise die with the tab — and the failures where that bites are
 * exactly the ones at end of session, where the lost window is the one containing
 * the incident. The OTLP fetch transport's `keepalive` flag rescues requests
 * already in flight; it does nothing for deltas not yet exported, which is what
 * this covers.
 *
 * Resolves (never rejects) when telemetry is unconfigured, so teardown paths can
 * call it unconditionally.
 */
export async function flushMetrics(): Promise<void> {
  await configured?.meterProvider.forceFlush();
}

/**
 * Reset global telemetry state. TEST-ONLY seam — lets a unit test re-run
 * `configureTelemetry` cleanly. Not part of the production join flow.
 */
export function resetTelemetryForTest(): void {
  // `configureTelemetry` registers a GLOBAL console `diag` logger outside
  // production; without this, one test that configured telemetry leaves it
  // registered for every later test in the worker, breaking this seam's
  // "re-run cleanly" contract.
  diag.disable();
  configured = undefined;
  propagation.disable();
}
