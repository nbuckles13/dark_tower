// File: packages/sdk-core/src/media/setup/capture.ts
//
// Microphone capture behind a device-selection seam.
//
// ---------------------------------------------------------------------------
// EVERY FAILURE HERE IS LOUD AND TYPED
// ---------------------------------------------------------------------------
//
// Permission denied, no device, a device unplugged mid-call: each becomes a
// typed `MediaCaptureError` with a bounded reason, or the `onEnded` callback.
// None is a silent catch-and-continue. A capture path that fails quietly is the
// worst version of this story's headline failure mode — every signal green and
// no audio — and it is the one the loopback exists to localise.
//
// ---------------------------------------------------------------------------
// THE TRACK IS REGISTERED FOR TEARDOWN THE MOMENT IT EXISTS
// ---------------------------------------------------------------------------
//
// A leaked `MediaStreamTrack` keeps the microphone hot and the browser's
// recording indicator lit after the meeting ends. That is a user-visible privacy
// failure, not a memory nit, so `stop()` must reach a partially-constructed
// capture: the track is stored on the instance before any further await.

import {
  CAPTURE_MESSAGE,
  CaptureFailure,
  MediaCaptureError,
  captureError,
  classifyCaptureError,
} from './captureFailure.js';

// The failure VOCABULARY and its classifier live in `./captureFailure.ts`, not
// here — this file is excluded from the coverage gate because it cannot execute
// under `environment: 'node'`, and a six-way branch hidden behind that exclusion
// would have been untestable and unscored while the exclusion's own
// justification claimed otherwise. Re-exported so callers keep one import.
export { CAPTURE_MESSAGE, CaptureFailure, MediaCaptureError, classifyCaptureError };

/**
 * Chrome's `MediaStreamTrackProcessor`, which the DOM lib does not declare.
 *
 * Typed narrowly and looked up off `globalThis` so its absence is a loud, typed
 * `platform_unsupported` rather than a `ReferenceError` from somewhere deeper.
 */
interface TrackProcessorLike {
  readonly readable: ReadableStream<AudioData>;
}
type TrackProcessorCtor = new (init: { track: MediaStreamTrack }) => TrackProcessorLike;

/**
 * Enumerate available microphones.
 *
 * Labels are empty until permission has been granted — that is the platform's
 * behaviour, not a bug to work around, and a device picker must render the
 * empty case rather than assume labels exist.
 */
export async function listMicrophones(): Promise<MediaDeviceInfo[]> {
  const media = globalThis.navigator?.mediaDevices;
  if (!media) {
    throw captureError(CaptureFailure.PlatformUnsupported);
  }
  const devices = await media.enumerateDevices();
  return devices.filter((d) => d.kind === 'audioinput');
}

/**
 * Production capture source.
 *
 * Thin by construction: acquisition and seam delegation only, with every branch
 * that has a decision in it living in the classification helpers above. That is
 * what earns it the narrow coverage exclusion — if a branch appears here, it
 * moves to a tested module instead.
 */
export function createMicrophoneCapture(options: {
  readonly sampleRateHz: number;
  readonly channels: number;
  readonly deviceId: string | undefined;
}): Promise<{
  start(onFrame: (data: AudioData) => void, onEnded: () => void): Promise<void>;
  stop(): void;
}> {
  const media = globalThis.navigator?.mediaDevices;
  const Processor = (globalThis as { MediaStreamTrackProcessor?: TrackProcessorCtor })
    .MediaStreamTrackProcessor;
  if (!media || !Processor) {
    return Promise.reject(captureError(CaptureFailure.PlatformUnsupported));
  }

  let track: MediaStreamTrack | undefined;
  let reader: ReadableStreamDefaultReader<AudioData> | undefined;
  let stopped = false;

  const stop = (): void => {
    stopped = true;
    // Order matters: cancel the reader first so the pump's `read()` settles,
    // then stop the track. Stopping first would leave the pump awaiting a stream
    // that never produces and never ends.
    try {
      void reader?.cancel();
    } catch {
      // Already cancelled or errored. One bad cancel must not prevent the track
      // from stopping — the microphone indicator is the thing that matters.
    }
    reader = undefined;
    track?.stop();
    track = undefined;
  };

  const start = async (onFrame: (data: AudioData) => void, onEnded: () => void): Promise<void> => {
    let stream: MediaStream;
    try {
      stream = await media.getUserMedia({
        audio: {
          ...(options.deviceId !== undefined ? { deviceId: { exact: options.deviceId } } : {}),
          channelCount: options.channels,
          sampleRate: options.sampleRateHz,
        },
      });
    } catch (err) {
      throw captureError(classifyCaptureError(err));
    }

    const [first] = stream.getAudioTracks();
    if (!first) {
      // Registered nowhere yet, so stop the stream directly rather than leaking
      // it while reporting the failure.
      for (const t of stream.getTracks()) t.stop();
      throw new MediaCaptureError(
        CaptureFailure.NoDevice,
        CAPTURE_MESSAGE[CaptureFailure.NoDevice],
      );
    }
    // REGISTERED BEFORE THE NEXT AWAIT. If anything below throws, `stop()` still
    // reaches the track and the microphone indicator goes out.
    track = first;
    if (stopped) {
      stop();
      return;
    }
    // A device unplugged, revoked, or taken by the OS ends the track. Surfaced,
    // never absorbed: absence of frames is not a signal.
    track.addEventListener('ended', onEnded, { once: true });

    const processor = new Processor({ track });
    reader = processor.readable.getReader();
    void pump(reader, onFrame);
  };

  return Promise.resolve({ start, stop });
}

/**
 * Drain captured frames.
 *
 * Deliberately NOT awaited by `start`: capture runs for the life of the session,
 * so awaiting it would never return. The loop exits when the reader is cancelled
 * by `stop()`.
 */
async function pump(
  reader: ReadableStreamDefaultReader<AudioData>,
  onFrame: (data: AudioData) => void,
): Promise<void> {
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) return;
      if (value !== undefined) onFrame(value);
    }
  } catch {
    // The reader was cancelled by `stop()`, or the track errored. Either way the
    // capture is over; the `ended` listener above is what reports the second
    // case. Swallowing HERE is correct and is the only place it is: rethrowing
    // from a detached pump would surface as an unhandled rejection with no
    // caller able to act on it.
  }
}
