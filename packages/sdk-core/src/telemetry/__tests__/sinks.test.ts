// File: packages/sdk-core/src/telemetry/__tests__/sinks.test.ts
//
// R-24: the three production-side sinks. `OtelMetricsSink` is tested with a
// FAKE injected `Meter` (OTel is externalized — see vite.config.ts — so the
// unit tier never runs the real SDK). Covers instrument caching, label
// pass-through, and name-guard wiring (throw vs warn) for each sink.

import { describe, expect, it, vi } from 'vitest';
import type { Counter, Gauge, Histogram, Meter } from '@opentelemetry/api';

import { ConsoleMetricsSink } from '../ConsoleMetricsSink.js';
import type { MetricsSink } from '../MetricsSink.js';
import { NoopMetricsSink } from '../NoopMetricsSink.js';
import { OtelMetricsSink } from '../OtelMetricsSink.js';

// --- Fake OTel Meter that records instrument creation + add/record calls ---

interface FakeInstrument {
  readonly name: string;
  readonly calls: Array<{ value: number; attributes: unknown }>;
}

function makeFakeMeter() {
  const counters = new Map<string, FakeInstrument>();
  const histograms = new Map<string, FakeInstrument>();
  const gauges = new Map<string, FakeInstrument>();

  function make(store: Map<string, FakeInstrument>, name: string) {
    const inst: FakeInstrument = { name, calls: [] };
    store.set(name, inst);
    const record = (value: number, attributes?: unknown) => {
      inst.calls.push({ value, attributes });
    };
    return { inst, add: record, record };
  }

  const createCounter = vi.fn((name: string) => {
    const { add } = make(counters, name);
    return { add } as unknown as Counter;
  });
  const createHistogram = vi.fn((name: string) => {
    const { record } = make(histograms, name);
    return { record } as unknown as Histogram;
  });
  const createGauge = vi.fn((name: string) => {
    const { record } = make(gauges, name);
    return { record } as unknown as Gauge;
  });

  const meter = { createCounter, createHistogram, createGauge } as unknown as Meter;
  return { meter, counters, histograms, gauges, createCounter, createHistogram, createGauge };
}

describe('NoopMetricsSink', () => {
  it('discards every emission without throwing', () => {
    // Typed as the `MetricsSink` interface — how callers consume it. The
    // concrete Noop methods take no params (structural assignability), but
    // through the interface the wide `(name, labels, value?)` signature applies.
    const sink: MetricsSink = new NoopMetricsSink();
    expect(() => {
      sink.counter('anything-goes', { a: '1' });
      sink.histogram('anything', {}, 5);
      sink.gauge('anything', {}, 3);
    }).not.toThrow();
  });
});

describe('ConsoleMetricsSink', () => {
  it('logs a compliant counter/histogram/gauge', () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => {});
    const sink = new ConsoleMetricsSink('throw');
    sink.counter('dt_client_join_attempts_total', { status: 'success' });
    sink.histogram('dt_client_time_to_signaling_ready_ms', {}, 12);
    sink.gauge('dt_client_mh_connection_total', {}, 1);
    expect(debug).toHaveBeenCalledTimes(3);
    debug.mockRestore();
  });

  it("throws on a non-compliant name in 'throw' mode", () => {
    const sink = new ConsoleMetricsSink('throw');
    expect(() => sink.counter('bad_name', {})).toThrow(/dt_client_/);
  });

  it("warns and drops a non-compliant counter/histogram/gauge in 'warn' mode", () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => {});
    const sink = new ConsoleMetricsSink('warn');
    sink.counter('bad_name', {});
    sink.histogram('bad_name', {}, 1);
    sink.gauge('bad_name', {}, 1);
    expect(warn).toHaveBeenCalledTimes(3);
    // All three dropped — never reach the debug log.
    expect(debug).not.toHaveBeenCalled();
    warn.mockRestore();
    debug.mockRestore();
  });

  it("throws on a non-compliant histogram/gauge name in 'throw' mode", () => {
    const sink = new ConsoleMetricsSink('throw');
    expect(() => sink.histogram('bad_name', {}, 1)).toThrow();
    expect(() => sink.gauge('bad_name', {}, 1)).toThrow();
  });

  it("defaults to 'throw' mode when no mode is passed", () => {
    const sink = new ConsoleMetricsSink();
    expect(() => sink.counter('bad_name', {})).toThrow();
  });
});

describe('OtelMetricsSink', () => {
  it('creates and caches a counter, forwarding value + labels', () => {
    const { meter, counters, createCounter } = makeFakeMeter();
    const sink = new OtelMetricsSink(meter, 'throw');

    sink.counter('dt_client_join_attempts_total', { status: 'success' });
    sink.counter('dt_client_join_attempts_total', { status: 'failure' }, 2);

    // Instrument created once (cached on the second call).
    expect(createCounter).toHaveBeenCalledTimes(1);
    const inst = counters.get('dt_client_join_attempts_total');
    expect(inst?.calls).toEqual([
      { value: 1, attributes: { status: 'success' } },
      { value: 2, attributes: { status: 'failure' } },
    ]);
  });

  it('records histograms and gauges, caching each instrument by name', () => {
    const { meter, histograms, gauges, createHistogram, createGauge } = makeFakeMeter();
    const sink = new OtelMetricsSink(meter, 'throw');

    sink.histogram('dt_client_time_to_signaling_ready_ms', {}, 10);
    sink.histogram('dt_client_time_to_signaling_ready_ms', {}, 20);
    sink.gauge('dt_client_mh_connection_total', { mh_index_bucket: '0' }, 1);

    expect(createHistogram).toHaveBeenCalledTimes(1);
    expect(createGauge).toHaveBeenCalledTimes(1);
    expect(
      histograms.get('dt_client_time_to_signaling_ready_ms')?.calls.map((c) => c.value),
    ).toEqual([10, 20]);
    expect(gauges.get('dt_client_mh_connection_total')?.calls[0]?.attributes).toEqual({
      mh_index_bucket: '0',
    });
  });

  it("throws on a non-compliant name in 'throw' mode and never touches the meter", () => {
    const { meter, createCounter } = makeFakeMeter();
    const sink = new OtelMetricsSink(meter, 'throw');
    expect(() => sink.counter('mc_join_total', {})).toThrow();
    expect(createCounter).not.toHaveBeenCalled();
  });

  it("warns and drops a non-compliant name in 'warn' mode (no instrument created)", () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { meter, createHistogram, createGauge } = makeFakeMeter();
    const sink = new OtelMetricsSink(meter, 'warn');
    sink.histogram('bad', {}, 1);
    sink.gauge('also_bad', {}, 1);
    expect(warn).toHaveBeenCalledTimes(2);
    expect(createHistogram).not.toHaveBeenCalled();
    expect(createGauge).not.toHaveBeenCalled();
    warn.mockRestore();
  });

  it('defaults counter value to 1', () => {
    const { meter, counters } = makeFakeMeter();
    const sink = new OtelMetricsSink(meter, 'throw');
    sink.counter('dt_client_signaling_connection_total', { status: 'success' });
    expect(counters.get('dt_client_signaling_connection_total')?.calls[0]?.value).toBe(1);
  });
});
