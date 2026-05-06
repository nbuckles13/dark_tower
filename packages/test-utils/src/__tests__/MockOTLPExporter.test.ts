import { describe, expect, it } from 'vitest';
import { MockOTLPExporter } from '../MockOTLPExporter.js';

describe('MockOTLPExporter', () => {
  it('captures per-call payloads, filters by kind, and drives failure injection (single-shot + sticky)', async () => {
    const exp = new MockOTLPExporter();

    // Default response is success.
    const okResult = await exp.export({ kind: 'metrics', capturedAt: 1, body: { fixture: 'metrics' } });
    expect(okResult).toEqual({ code: 'success' });
    expect(exp.getMetricPayloads()).toHaveLength(1);
    expect(exp.getTracePayloads()).toHaveLength(0);

    // Single-shot 429 with retryAfterMs.
    exp.simulateNextResponse({ status: 429, retryAfterMs: 200 });
    const rateLimited = await exp.export({ kind: 'traces', capturedAt: 2, body: { fixture: 'traces' } });
    expect(rateLimited.code).toBe('failure');
    if (rateLimited.code === 'failure') {
      expect(rateLimited.error.message).toMatch(/429/);
      expect(rateLimited.error.message).toMatch(/retryAfterMs=200/);
    }
    expect(exp.getTracePayloads()).toHaveLength(1);

    // After single-shot consumed, next call falls back to success default.
    const okAgain = await exp.export({ kind: 'metrics', capturedAt: 3, body: {} });
    expect(okAgain).toEqual({ code: 'success' });

    // Sticky 502 — applies until cleared.
    exp.simulateAlwaysRespond({ status: 502 });
    const r1 = await exp.export({ kind: 'metrics', capturedAt: 4, body: {} });
    const r2 = await exp.export({ kind: 'metrics', capturedAt: 5, body: {} });
    expect(r1.code).toBe('failure');
    expect(r2.code).toBe('failure');
    if (r1.code === 'failure') expect(r1.error.message).toMatch(/502/);

    // Lifecycle methods captured.
    await exp.shutdown();
    await exp.forceFlush();
    expect(exp.callCount('shutdown')).toBe(1);
    expect(exp.callCount('forceFlush')).toBe(1);
    expect(exp.callCount('export')).toBe(5);

    // Network-error spec.
    exp.simulateNextResponse({ status: 'network-error' });
    const ne = await exp.export({ kind: 'metrics', capturedAt: 6, body: {} });
    expect(ne.code).toBe('failure');
    if (ne.code === 'failure') expect(ne.error.message).toMatch(/network-error/);

    // clear() resets capture and unsticks the always-respond.
    exp.clear();
    expect(exp.getExportCount()).toBe(0);
    expect(exp.callCount()).toBe(0);
    const afterClear = await exp.export({ kind: 'metrics', capturedAt: 7, body: {} });
    expect(afterClear).toEqual({ code: 'success' });
  });
});
