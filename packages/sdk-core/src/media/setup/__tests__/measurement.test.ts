// File: packages/sdk-core/src/media/setup/__tests__/measurement.test.ts
//
// ADR-0036 §10: latency is OBSERVED, NEVER GATED. Note what is deliberately
// absent from this file — any assertion that the observed value is below a
// threshold. A wall-clock gate on a shared machine is a permanent flake, and
// ADR-0028 forbids quarantining quality gates, so the test would end up deleted
// and the headline objective would have zero coverage.

import { describe, expect, it } from 'vitest';
import { InMemoryMetricsSink } from '@darktower/test-utils';

import { MediaMetrics } from '../mediaMetrics.js';
import { FirstMediaObserver } from '../measurement.js';

function rig(): {
  sink: InMemoryMetricsSink;
  observer: FirstMediaObserver;
  observed: number[];
  tick(): void;
} {
  const sink = new InMemoryMetricsSink();
  const metrics = new MediaMetrics({ clientVersion: 'v', orgId: 'o' }, sink);
  let now = 0;
  const observed: number[] = [];
  const observer = new FirstMediaObserver(
    metrics,
    () => now,
    (ms) => observed.push(ms),
  );
  return {
    sink,
    observer,
    observed,
    tick(): void {
      now += 1;
    },
  };
}

function histograms(sink: InMemoryMetricsSink): readonly number[] {
  return sink
    .getRecordedMetrics()
    .filter((m) => m.name === 'dt_client_time_to_first_media_frame_ms')
    .map((m) => m.value);
}

describe('FirstMediaObserver', () => {
  it('observes once, on the first frame, measured from MEDIA start', () => {
    const r = rig();
    r.observer.start();
    r.tick();
    r.tick();
    r.observer.onFrameReceived();
    r.observer.onFrameReceived();
    r.observer.onFrameReceived();

    expect(histograms(r.sink)).toEqual([2]);
    expect(r.observed).toEqual([2]);
    expect(r.observer.hasObserved).toBe(true);
  });

  it('reports the SAME number to the metric and to the event', () => {
    // So an embedder and the histogram can never disagree about the value.
    const r = rig();
    r.observer.start();
    r.tick();
    r.observer.onFrameReceived();
    expect(r.observed).toEqual(histograms(r.sink));
  });

  it('reports nothing when no epoch was set, rather than inventing one', () => {
    // Silently skipping is correct HERE and only here: the alternative is
    // fabricating an epoch, which produces a plausible wrong number instead of
    // none.
    const r = rig();
    r.observer.onFrameReceived();
    expect(histograms(r.sink)).toEqual([]);
    expect(r.observer.hasObserved).toBe(false);
  });

  it('re-arms on a fresh start', () => {
    const r = rig();
    r.observer.start();
    r.observer.onFrameReceived();
    r.observer.start();
    r.tick();
    r.observer.onFrameReceived();
    expect(histograms(r.sink)).toEqual([0, 1]);
  });

  it('clears', () => {
    const r = rig();
    r.observer.start();
    r.observer.onFrameReceived();
    r.observer.clear();
    expect(r.observer.hasObserved).toBe(false);
    r.observer.onFrameReceived();
    expect(histograms(r.sink)).toEqual([0]);
  });
});
