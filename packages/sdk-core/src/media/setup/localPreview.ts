// File: packages/sdk-core/src/media/setup/localPreview.ts
//
// THE LOCAL PREVIEW CAPTURE for the R-7 self-check (story 3), beside
// `videoCaptureBrowser.ts:createCameraCapture`: one `getUserMedia` for a
// camera and/or microphone the embedder renders in a `<video>` element and
// meters, straight from the capture.
//
// IT FEEDS NOTHING. No `MediaStreamTrackProcessor`, no encoder, no packetiser,
// no SFrame, no transport: the stream is handed to the caller and goes no
// further. It is NOT the session's capture — a running session's tracks are
// read through `localCaptureTracksOf(session)` instead, so the camera-light
// rule for the publish stays the publish path's alone. Use this only where no
// session capture exists (before join).
//
// THE SAME FAILURE VOCABULARY AS EVERY OTHER CAPTURE: a rejection is
// classified by `captureFailure.ts:classifyCaptureError` into a bounded
// `CaptureFailure` token on a `MediaCaptureError`; never a platform string.
//
// Environment-independent (the acquisition is an injected seam), so it is
// unit-tested and scored rather than coverage-excluded.

import {
  CAMERA_CAPTURE_MESSAGE,
  CAPTURE_MESSAGE,
  CaptureFailure,
  MediaCaptureError,
  classifyCaptureError,
} from './captureFailure.js';

/** An open local preview. The CALLER owns it and must {@link LocalPreview.stop} it. */
export interface LocalPreview {
  /** The acquired stream: bind to a `<video>` element / an audio meter. */
  readonly stream: MediaStream;
  readonly video: MediaStreamTrack | undefined;
  readonly audio: MediaStreamTrack | undefined;
  /** Stop EVERY track (camera light and microphone indicator off). Idempotent. */
  stop(): void;
}

/** The acquisition seam (`navigator.mediaDevices.getUserMedia` by default). */
export type PreviewAcquire = (constraints: MediaStreamConstraints) => Promise<MediaStream>;

function defaultAcquire(): PreviewAcquire | undefined {
  const media = globalThis.navigator?.mediaDevices;
  return media ? (constraints) => media.getUserMedia(constraints) : undefined;
}

/**
 * Open a local preview of the camera and/or microphone. Rejects with a typed
 * {@link MediaCaptureError} (bounded reason) on any failure, having stopped
 * whatever it acquired.
 */
export async function openLocalPreview(
  kinds: { readonly video: boolean; readonly audio: boolean },
  acquire: PreviewAcquire | undefined = defaultAcquire(),
): Promise<LocalPreview> {
  const message = kinds.video ? CAMERA_CAPTURE_MESSAGE : CAPTURE_MESSAGE;
  const fail = (reason: CaptureFailure): MediaCaptureError =>
    new MediaCaptureError(reason, message[reason]);
  if (acquire === undefined) throw fail(CaptureFailure.PlatformUnsupported);
  if (!kinds.video && !kinds.audio) throw fail(CaptureFailure.NoDevice);

  let stream: MediaStream;
  try {
    stream = await acquire({ video: kinds.video, audio: kinds.audio });
  } catch (err) {
    throw fail(classifyCaptureError(err));
  }
  const [video] = stream.getVideoTracks();
  const [audio] = stream.getAudioTracks();
  const stop = (): void => {
    for (const track of stream.getTracks()) track.stop();
  };
  if (video === undefined && audio === undefined) {
    // Registered nowhere yet: stop directly rather than leak while reporting.
    stop();
    throw fail(CaptureFailure.NoDevice);
  }
  return { stream, video, audio, stop };
}
