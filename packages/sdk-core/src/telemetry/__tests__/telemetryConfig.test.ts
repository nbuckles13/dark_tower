// File: packages/sdk-core/src/telemetry/__tests__/telemetryConfig.test.ts
//
// R-19/R-24: the single global providers + name-guard mode resolution. The
// OTLP-proto exporter is MOCKED (OTel is externalized; the unit tier must not
// open a real network exporter) so the provider-wiring lines execute and clear
// the ≥90% branch gate (per @test's Gate-2 note that v8 branch coverage dips on
// provider construction).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Mock the OTLP exporter so no real exporter / network is constructed. Capture
// the constructor options to assert the `/v1/metrics` endpoint suffix.
const exporterCtor = vi.fn();
vi.mock('@opentelemetry/exporter-metrics-otlp-proto', () => ({
  OTLPMetricExporter: class {
    constructor(opts: unknown) {
      exporterCtor(opts);
    }
  },
}));

import { metrics, propagation, trace } from '@opentelemetry/api';

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

  afterEach(() => {
    resetTelemetryForTest();
    metrics.disable();
    trace.disable();
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
