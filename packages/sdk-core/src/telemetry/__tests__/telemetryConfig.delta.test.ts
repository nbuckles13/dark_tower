// File: packages/sdk-core/src/telemetry/__tests__/telemetryConfig.delta.test.ts
//
// The SDK MUST export DELTA temporality. This is not a style preference: every
// browser in a meeting emits one shared series identity (ADR-0036 §11 bars a
// participant dimension), so CUMULATIVE means N browsers overwrite each other
// at the collector and the stored series is meaningless. DELTA is what lets
// `delta_to_cumulative` sum the fleet into one correct series.
//
// WHY THIS ASSERTS THE EXPORTER'S BEHAVIOUR RATHER THAN ITS CONSTRUCTOR ARG:
// the exporter's selector compares against `AggregationTemporalityPreference`,
// a different enum from the `AggregationTemporality` we pass; they coincide only
// because both use 0 for DELTA. A mock capturing the constructor argument would
// pass through exactly the renumbering regression that matters, so we build the
// real exporter and ask it what it selects.

import { describe, expect, it } from 'vitest';
import { AggregationTemporality, InstrumentType } from '@opentelemetry/sdk-metrics';

import { createMetricExporter } from '../telemetryConfig.js';

describe('createMetricExporter temporality', () => {
  const exporter = createMetricExporter('https://gc.example/api/v1/telemetry');

  it.each([
    ['counter', InstrumentType.COUNTER],
    ['histogram', InstrumentType.HISTOGRAM],
  ])('selects DELTA for %s', (_label, instrumentType) => {
    expect(exporter.selectAggregationTemporality(instrumentType)).toBe(
      AggregationTemporality.DELTA,
    );
  });

  it("leaves UpDownCounter CUMULATIVE, which is upstream's mapping and not a gap", () => {
    // Pinned as a CHARACTERIZATION, not an aspiration. The DELTA preference
    // deliberately maps UpDownCounter to cumulative upstream: a delta on a
    // value that can decrease is not summable across writers the way a
    // monotonic counter is. The SDK creates no UpDownCounter today — the only
    // non-counter instrument the media path uses is a Gauge
    // (`OtelMetricsSink.gauge` -> `createGauge`), whose N-browser
    // last-writer-wins behaviour is documented in
    // `docs/observability/metrics/client.md` rather than fixed here, because
    // fixing it would need the identity dimension §11 bars.
    //
    // This assertion exists so that if someone later reaches for an
    // UpDownCounter on the media path, the temporality surprise is already
    // written down instead of being discovered in a dashboard.
    expect(exporter.selectAggregationTemporality(InstrumentType.UP_DOWN_COUNTER)).toBe(
      AggregationTemporality.CUMULATIVE,
    );
  });
});

describe('createMetricExporter auth headers', () => {
  /**
   * Reach the configured headers factory the way the transport does. The
   * exporter keeps its config privately, so this reads the same field the
   * fetch transport awaits on each export.
   */
  function headersFactory(exporter: unknown): () => Promise<Record<string, string>> {
    const found = findHeadersFactory(exporter, new Set(), 0);
    if (!found) throw new Error('no headers factory found on the exporter');
    return found;
  }

  function findHeadersFactory(
    node: unknown,
    seen: Set<object>,
    depth: number,
  ): (() => Promise<Record<string, string>>) | undefined {
    if (depth > 6 || node === null || typeof node !== 'object' || seen.has(node)) return undefined;
    seen.add(node);
    for (const value of Object.values(node as Record<string, unknown>)) {
      if (typeof value === 'function' && value.length === 0) {
        try {
          const out = (value as () => unknown)();
          if (out instanceof Promise) {
            // Only a headers factory returns a promise of a plain record here.
            return value as () => Promise<Record<string, string>>;
          }
        } catch {
          // Not the factory; keep looking.
        }
      }
      const nested = findHeadersFactory(value, seen, depth + 1);
      if (nested) return nested;
    }
    return undefined;
  }

  it('sends the bearer token when one is available', async () => {
    const exporter = createMetricExporter('https://gc.example/api/v1/telemetry', () => 'tok-abc');
    await expect(headersFactory(exporter)()).resolves.toMatchObject({
      Authorization: 'Bearer tok-abc',
    });
  });

  it('omits the header entirely when there is no token, never sending `Bearer undefined`', async () => {
    const exporter = createMetricExporter('https://gc.example/api/v1/telemetry', () => undefined);
    const headers = await headersFactory(exporter)();
    expect(headers.Authorization).toBeUndefined();
    expect(JSON.stringify(headers)).not.toContain('undefined');
  });

  it('omits the header when no provider is configured at all', async () => {
    const exporter = createMetricExporter('https://gc.example/api/v1/telemetry');
    const headers = await headersFactory(exporter)();
    expect(headers.Authorization).toBeUndefined();
  });

  it('RE-EVALUATES the provider on every export, which is the lifetime fix itself', async () => {
    // This is the assertion that distinguishes a working token lifetime from mere
    // plumbing. Telemetry is configured once at mount, before sign-in; the token
    // then appears, changes on re-auth, and is cleared on sign-out. A factory
    // captured once would freeze the FIRST of those — for the app's real ordering,
    // that is "no token", forever, and every export would 401 invisibly.
    let token: string | undefined;
    const exporter = createMetricExporter('https://gc.example/api/v1/telemetry', () => token);
    const factory = headersFactory(exporter);

    expect((await factory()).Authorization).toBeUndefined(); // pre-sign-in
    token = 'first-token';
    expect((await factory()).Authorization).toBe('Bearer first-token'); // after sign-in
    token = 'rotated-token';
    expect((await factory()).Authorization).toBe('Bearer rotated-token'); // after re-auth
    token = undefined;
    expect((await factory()).Authorization).toBeUndefined(); // after sign-out
  });
});
