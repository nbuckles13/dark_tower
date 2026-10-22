// File: packages/sdk-core/src/media/setup/videoCapture.ts
//
// Camera capture and the build-gated synthetic test pattern (story 3 R-17,
// R-19, R-27), behind ONE video capture seam (`./seams.ts:VideoCaptureSource`).
//
// Both sources ride `./capture.ts`'s conventions: every failure is a typed
// `MediaCaptureError` with a bounded reason (`./captureFailure.ts`, the same
// vocabulary as the microphone); the track is registered for stop the moment it
// exists, so the camera light goes out from any partially-constructed state.
//
// WHEN the camera runs is not decided here. `lifecycle/cameraGate.ts` decides it
// (directed with targets AND not client-muted AND healthy) and the publisher
// starts and stops these sources on exactly those edges. The test pattern rides
// the SAME seam, so it starts and stops on exactly the camera's edges — otherwise
// every camera-light assertion against it would be vacuous.

import type { VideoCaptureSource, VideoCaptureSourceFactory } from './seams.js';
import { TEST_PATTERN_REFUSAL, type TestPatternSource } from './testPattern.js';
import type { PatternPaintTarget } from './testPattern.js';
import { browserPatternPlatform } from './videoCaptureBrowser.js';

// The browser-only halves (camera `getUserMedia`, the OffscreenCanvas platform)
// live in `./videoCaptureBrowser.ts`, excluded from the node coverage gate like
// `./capture.ts`; every decision with a branch is here and scored.
export { createCameraCapture } from './videoCaptureBrowser.js';

/** A canvas the pattern paints into: a 2D context plus whatever a frame is built from. */
export interface PatternCanvas {
  readonly context: PatternPaintTarget;
}

/** The platform pieces the test-pattern capture uses, injectable for the node tier. */
export interface TestPatternPlatform {
  readonly createCanvas: (width: number, height: number) => PatternCanvas;
  /** Wrap the painted canvas as a captured frame, at a timestamp in microseconds. */
  readonly createFrame: (canvas: PatternCanvas, timestampUs: number) => VideoFrame;
  readonly setInterval: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval: (handle: ReturnType<typeof setInterval>) => void;
}

/** Options for the test-pattern capture. */
export interface TestPatternCaptureOptions {
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  /** The participant's ONE pattern source (sender id + captured-frame counter). */
  readonly source: TestPatternSource;
}

/**
 * The test-pattern capture, UNGATED (the build gate is
 * {@link createTestPatternCapture}). One captured frame per tick at the
 * directed rate: paint (advancing the captured-frame counter), wrap, hand off.
 * `stop()` clears the timer — the synthetic "camera light" — and a stopped
 * source never ticks again.
 */
export function buildTestPatternCapture(
  options: TestPatternCaptureOptions,
  platform: TestPatternPlatform,
): VideoCaptureSource {
  let timer: ReturnType<typeof setInterval> | undefined;
  let stopped = false;
  let tick = 0;
  const periodMs = 1000 / options.frameRateHz;

  const stop = (): void => {
    stopped = true;
    if (timer !== undefined) platform.clearInterval(timer);
    timer = undefined;
  };

  const start = (onFrame: (frame: VideoFrame) => void): Promise<void> => {
    if (stopped) return Promise.resolve();
    const canvas = platform.createCanvas(options.width, options.height);
    timer = platform.setInterval(() => {
      if (stopped) return;
      options.source.paintNext(canvas.context, options.width, options.height);
      const frame = platform.createFrame(canvas, Math.round(tick * periodMs * 1000));
      tick += 1;
      onFrame(frame);
    }, periodMs);
    return Promise.resolve();
  };

  return { start, stop, grantedDeviceId: undefined };
}

/**
 * The GATED test-pattern capture factory.
 *
 * REACHABLE ONLY FROM INSIDE `if (__DT_TEST_PATTERN__)` in
 * `session/mediaSelection.ts:selectVideoCaptureSource`, and it THROWS
 * {@link TEST_PATTERN_REFUSAL} if reached in a build without the define: a
 * production (or SDK library) bundle that somehow called it fails loudly
 * rather than publishing a synthetic camera.
 */
export function createTestPatternCapture(source: TestPatternSource): VideoCaptureSourceFactory {
  return (options) => {
    if (!__DT_TEST_PATTERN__) return Promise.reject(new Error(TEST_PATTERN_REFUSAL));
    return Promise.resolve(
      buildTestPatternCapture(
        {
          width: options.width,
          height: options.height,
          frameRateHz: options.frameRateHz,
          source,
        },
        browserPatternPlatform(),
      ),
    );
  };
}
