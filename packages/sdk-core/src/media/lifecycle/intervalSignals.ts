// File: packages/sdk-core/src/media/lifecycle/intervalSignals.ts
//
// The pipeline's two PER-EXPORT-INTERVAL signals (story 2 R-7, R-28), on ONE
// timer at the exporter's cadence:
//
//   * `dt_client_media_capture_source{mode}` — set at start and RE-SET every
//     tick, because the SDK exports DELTA and a last-value gauge is exported only
//     for an interval it was recorded in. Disposing stops the timer, so a
//     finished session stops asserting the series.
//   * `dt_client_media_receive_source_deficit_total` — zero-initialised at
//     start, then per tick the ACTIVE assignments that decoded nothing for a
//     whole interval (`receiveSourceDeficit.ts`, the pure decision).
//
// This timer and the OTel reader are separate clocks of the same period, so a
// single export can miss a re-set: presence is judged over the lookback window,
// which the catalog states.

import type { MediaCaptureSourceMode, MediaMetrics } from '../setup/mediaMetrics.js';
import { receiveSourceDeficit, type ActiveAssignmentsSample } from './receiveSourceDeficit.js';

type IntervalHandle = ReturnType<typeof setInterval>;

/** Start both signals. Returns the disposer, which stops the timer. */
export function startIntervalSignals(options: {
  readonly metrics: MediaMetrics;
  readonly mode: MediaCaptureSourceMode;
  readonly intervalMs: number;
  /** The current ACTIVE assignments on declared slots, with lane activity. */
  readonly sample: () => ActiveAssignmentsSample;
  readonly setInterval: (fn: () => void, ms: number) => IntervalHandle;
  readonly clearInterval: (handle: IntervalHandle) => void;
}): () => void {
  const { metrics, mode, sample } = options;
  metrics.captureSource(mode);
  metrics.receiveSourceDeficit(0);
  let previous = sample();
  let stopped = false;
  const handle = options.setInterval(() => {
    if (stopped) return;
    const current = sample();
    const deficit = receiveSourceDeficit(previous, current);
    if (deficit > 0) metrics.receiveSourceDeficit(deficit);
    metrics.captureSource(mode);
    previous = current;
  }, options.intervalMs);
  return () => {
    stopped = true;
    options.clearInterval(handle);
  };
}
