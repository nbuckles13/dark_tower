// File: packages/sdk-core/src/telemetry/__tests__/telemetryConfig.test.ts
//
// R-19/R-24: the single global providers + name-guard mode resolution. The
// OTLP-proto exporter is MOCKED tier-wide (`otlpExporterSetup.ts`; the unit tier
// must not open a real network exporter) so the provider-wiring lines execute and
// clear the ≥90% branch gate (per @test's Gate-2 note that v8 branch coverage dips
// on provider construction). The mock's `exporterCtor` captures constructor options.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AggregationTemporality } from '@opentelemetry/sdk-metrics';
import { diag, DiagLogLevel, propagation } from '@opentelemetry/api';

import { exporterCtor, OTLPMetricExporter } from './otlpExporterMock.js';

import {
  configureTelemetry,
  getMeter,
  getMetricsSink,
  getTracer,
  resetTelemetryForTest,
} from '../telemetryConfig.js';

describe('configureTelemetry', () => {
  beforeEach(() => {
    exporterCtor.mockClear();
  });

  afterEach(async () => {
    await resetTelemetryForTest();
    vi.restoreAllMocks();
  });

  it('returns undefined accessors before configuration', () => {
    expect(getMeter()).toBeUndefined();
    expect(getTracer()).toBeUndefined();
    expect(getMetricsSink()).toBeUndefined();
  });

  it('constructs the OTLP exporter against the /v1/metrics endpoint', () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    expect(exporterCtor).toHaveBeenCalledTimes(1);
    expect(exporterCtor).toHaveBeenCalledWith({
      url: 'https://gc.example/api/v1/telemetry/v1/metrics',
      // Delta temporality: N same-identity browsers must SUM at the collector
      // rather than overwrite each other (ADR-0036 §11 bars a per-browser label).
      //
      // THIS ASSERTION IS THE WEAK HALF AND IS NOT THE REAL COVERAGE. The
      // exporter is mocked here, so this only proves what we PASSED. The
      // exporter's own selector compares against a DIFFERENT enum
      // (`AggregationTemporalityPreference`) that agrees with this one only
      // because both use 0 — a renumbering would pass right through this mock
      // and silently fall back to cumulative. `telemetryConfig.delta.test.ts`
      // builds the REAL exporter and asserts what it SELECTS; that is the test
      // that would catch it.
      temporalityPreference: AggregationTemporality.DELTA,
      // A per-export header factory carrying the user bearer (GC's telemetry
      // proxy is behind `require_user_auth`). Asserted only as "a function is
      // passed" — WHAT it yields, and that the exporter re-asks on every export,
      // are asserted against the REAL exporter in `telemetryConfig.delta.test.ts`.
      // A mock cannot show either, and the re-ask is the whole credential-lifetime
      // property.
      headers: expect.any(Function),
    });
  });

  it('installs global meter + tracer providers and a metrics sink', () => {
    const sink = configureTelemetry({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'development',
    });
    expect(sink).toBeDefined();
    expect(getMeter()).toBeDefined();
    expect(getTracer()).toBeDefined();
    expect(getMetricsSink()).toBe(sink);
  });

  it('registers a global propagator (R-19) so injection has a propagator', () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    // A real (non-noop) propagator advertises the W3C fields.
    expect(propagation.fields()).toContain('traceparent');
  });

  it("resolves guard mode 'warn' for production (sink does not throw on a bad name)", () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const sink = configureTelemetry({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'production',
    });
    expect(() => sink.counter('not_dt_client', {})).not.toThrow();
    expect(warn).toHaveBeenCalled();
  });

  it("resolves guard mode 'throw' for dev/test (sink throws on a bad name)", () => {
    const sink = configureTelemetry({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'development',
    });
    expect(() => sink.counter('not_dt_client', {})).toThrow();
  });

  it('a compliant emission through the configured sink does not throw', () => {
    const sink = configureTelemetry({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'test',
    });
    expect(() =>
      sink.counter('dt_client_join_attempts_total', { status: 'success' }),
    ).not.toThrow();
  });
});

// The 16 duplicate-registration lines this guards against (pre-existing until
// 2026-10-21) are printed by vitest's `default` reporter only; the agent reporter
// hides a passing test's console. So the guard is this assertion, not a log grep.
describe('resetTelemetryForTest', () => {
  afterEach(async () => {
    await resetTelemetryForTest();
  });

  it('lets configureTelemetry run again with no duplicate global registration', async () => {
    const error = vi.fn();
    const logger = { error, warn: vi.fn(), info: vi.fn(), debug: vi.fn(), verbose: vi.fn() };
    const endpoint = 'https://gc.example/api/v1/telemetry';
    // `production` keeps configureTelemetry from replacing this logger with a console one.
    diag.setLogger(logger, DiagLogLevel.ERROR);
    configureTelemetry({ telemetryEndpoint: endpoint, env: 'production' });
    await resetTelemetryForTest();
    diag.setLogger(logger, DiagLogLevel.ERROR);
    configureTelemetry({ telemetryEndpoint: endpoint, env: 'production' });

    const messages = error.mock.calls.map((call) => String(call[0]));
    expect(messages.filter((m) => m.includes('duplicate registration'))).toEqual([]);
    expect(getTracer()).toBeDefined();
  });

  // Shutdown runs BEFORE any global is disabled, `diag` last, so a teardown failure
  // stays visible. The two failure kinds surface differently (sdk-metrics 2.12): a
  // rejecting exporter shutdown rejects the reset; a failed final export is reported
  // through the global error handler, i.e. `diag`, which must still be live.
  function errorLogger() {
    const error = vi.fn();
    diag.setLogger(
      { error, warn: vi.fn(), info: vi.fn(), debug: vi.fn(), verbose: vi.fn() },
      DiagLogLevel.ERROR,
    );
    return error;
  }

  it('a rejecting exporter shutdown rejects the reset AND still clears every global', async () => {
    const endpoint = 'https://gc.example/api/v1/telemetry';
    configureTelemetry({ telemetryEndpoint: endpoint, env: 'production' });
    vi.spyOn(OTLPMetricExporter.prototype, 'shutdown').mockRejectedValueOnce(
      new Error('shutdown boom'),
    );
    await expect(resetTelemetryForTest()).rejects.toThrow('shutdown boom');

    // A failed teardown must not leave the globals registered: the next configure
    // would otherwise reproduce the duplicate-registration cascade in every later test.
    const error = errorLogger();
    configureTelemetry({ telemetryEndpoint: endpoint, env: 'production' });
    const messages = error.mock.calls.map((call) => String(call[0]));
    expect(messages.filter((m) => m.includes('duplicate registration'))).toEqual([]);
  });

  // Non-production configure installs a DiagConsoleLogger; a reset that left it
  // installed would print every later test's OTel diagnostics. DiagConsoleLogger
  // binds the console at module load (a console spy cannot see it), so observe the
  // registry instead: setting a logger over a still-registered one warns to the new
  // logger. The control below proves that signal exists, so (a) cannot go vacuous.
  function warnSpyLogger() {
    const warn = vi.fn();
    diag.setLogger(
      { error: vi.fn(), warn, info: vi.fn(), debug: vi.fn(), verbose: vi.fn() },
      DiagLogLevel.WARN,
    );
    return warn;
  }

  it("removes configureTelemetry's console diag logger", async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    await resetTelemetryForTest();
    expect(warnSpyLogger()).not.toHaveBeenCalled();
  });

  it('control: without a reset, a new logger IS warned about the registered one', () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    expect(warnSpyLogger()).toHaveBeenCalled();
  });

  it('a failed final export during the reset reaches diag', async () => {
    configureTelemetry({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'production',
    });
    const error = errorLogger();
    getMeter()!.createCounter('reset_probe').add(1);
    vi.spyOn(OTLPMetricExporter.prototype, 'export').mockImplementationOnce((_metrics, done) =>
      done({ code: 1, error: new Error('export boom') } as never),
    );
    await resetTelemetryForTest();
    expect(error.mock.calls.map((call) => String(call[0])).join('\n')).toContain('export boom');
  });
});
