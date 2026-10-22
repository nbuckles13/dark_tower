// File: packages/sdk-core/src/media/setup/videoCaptureBrowser.ts
//
// The BROWSER-ONLY halves of `./videoCapture.ts` (story 3 C-A2): the camera's
// `getUserMedia({video})` acquisition and the test pattern's
// `OffscreenCanvas` + `VideoFrame` platform. Thin by construction — no decision
// with a branch beyond "the platform API is absent" lives here — and EXCLUDED
// from the node unit tier's coverage gate for the same stated reason as
// `./capture.ts` (none of these APIs exist under `environment: 'node'`). They
// are exercised for real by the browser env-tests (C-C1). If a branch is ever
// needed here, it moves to a scored module instead of the exclusion widening.

import {
  CAMERA_CAPTURE_MESSAGE,
  CaptureFailure,
  MediaCaptureError,
  cameraCaptureError,
  classifyCaptureError,
} from './captureFailure.js';
import { trackCapture, trackProcessor } from './capture.js';
import type { VideoCaptureSource, VideoCaptureSourceFactory } from './seams.js';
import { TEST_PATTERN_MARKER } from './testPattern.js';
import type { PatternCanvas, TestPatternPlatform } from './videoCapture.js';

/**
 * Production camera capture at the DIRECTED geometry.
 *
 * Thin by construction (excluded from the node coverage gate for the same
 * reason as `capture.ts`): `getUserMedia` + the shared track pump. The
 * constraints are `ideal`, never `exact`, for size and rate — a camera that
 * cannot produce exactly 640x360@30 still publishes, and the encoder scales to
 * the directed size. `deviceId` IS exact when given: a re-acquire reuses the
 * granted camera instead of re-picking one (no prompt loop, no device swap).
 */
export const createCameraCapture: VideoCaptureSourceFactory = (options) => {
  const media = globalThis.navigator?.mediaDevices;
  const Processor = trackProcessor<VideoFrame>();
  if (!media || !Processor) {
    return Promise.reject(cameraCaptureError(CaptureFailure.PlatformUnsupported));
  }
  const capture = trackCapture<VideoFrame>(Processor);
  let granted: string | undefined;

  const start = async (
    onFrame: (frame: VideoFrame) => void,
    onEnded: () => void,
  ): Promise<void> => {
    let stream: MediaStream;
    try {
      stream = await media.getUserMedia({
        video: {
          ...(options.deviceId !== undefined ? { deviceId: { exact: options.deviceId } } : {}),
          width: { ideal: options.width },
          height: { ideal: options.height },
          frameRate: { ideal: options.frameRateHz },
        },
      });
    } catch (err) {
      throw cameraCaptureError(classifyCaptureError(err));
    }
    const [first] = stream.getVideoTracks();
    if (!first) {
      for (const t of stream.getTracks()) t.stop();
      throw new MediaCaptureError(
        CaptureFailure.NoDevice,
        CAMERA_CAPTURE_MESSAGE[CaptureFailure.NoDevice],
      );
    }
    granted = first.getSettings().deviceId;
    capture.attach(first, onFrame, onEnded);
  };

  const source: VideoCaptureSource = {
    start,
    stop: capture.stop,
    get grantedDeviceId(): string | undefined {
      return granted;
    },
  };
  return Promise.resolve(source);
};

/** The browser's `OffscreenCanvas` + `VideoFrame` platform for the pattern. */
export function browserPatternPlatform(): TestPatternPlatform {
  type Ctor = new (w: number, h: number) => OffscreenCanvas;
  const Canvas = (globalThis as { OffscreenCanvas?: Ctor }).OffscreenCanvas;
  const Frame = (globalThis as { VideoFrame?: typeof VideoFrame }).VideoFrame;
  if (!Canvas || !Frame) {
    // The generator-unique marker rides this live, gated path, so the bundle
    // tests have a token only the generator carries (absent in production,
    // present in a pattern build — the positive control).
    throw new MediaCaptureError(
      CaptureFailure.PlatformUnsupported,
      `${TEST_PATTERN_MARKER}: this platform lacks OffscreenCanvas or VideoFrame`,
    );
  }
  const canvases = new WeakMap<PatternCanvas, OffscreenCanvas>();
  return {
    createCanvas: (width, height) => {
      const canvas = new Canvas(width, height);
      const context = canvas.getContext('2d');
      if (!context) throw cameraCaptureError(CaptureFailure.PlatformUnsupported);
      const handle: PatternCanvas = { context };
      canvases.set(handle, canvas);
      return handle;
    },
    createFrame: (handle, timestampUs) => {
      const canvas = canvases.get(handle);
      if (!canvas) throw cameraCaptureError(CaptureFailure.Unknown);
      return new Frame(canvas, { timestamp: timestampUs });
    },
    setInterval: (fn, ms) => setInterval(fn, ms),
    clearInterval: (handle) => clearInterval(handle),
  };
}
