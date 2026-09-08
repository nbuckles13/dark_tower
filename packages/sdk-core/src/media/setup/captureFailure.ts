// File: packages/sdk-core/src/media/setup/captureFailure.ts
//
// The capture failure vocabulary and its classifier.
//
// ---------------------------------------------------------------------------
// WHY THIS IS A SEPARATE MODULE FROM `capture.ts`
// ---------------------------------------------------------------------------
//
// `capture.ts` is excluded from the coverage gate because it cannot execute
// under the unit tier's `environment: 'node'` — it is `getUserMedia`,
// `MediaStreamTrackProcessor` and track wiring, with no branch of its own.
//
// This classifier is the opposite: a pure, environment-independent six-way
// branch over `err.name`, with zero browser dependency. Left inside the excluded
// file it would have been untestable AND unscored — and the exclusion's own
// justification ("every decision with a branch in it lives in a tested module")
// would have been a claim the tree did not support. That is the shape this
// devloop kept finding: a control that reads as coverage without providing it.
//
// The consequence is not abstract. This classifier decides which bounded reason
// an operator sees for the silent-no-audio case the loopback story exists to
// localise. A wrong or opaque arm here sends someone to the wrong subsystem.

/** Bounded capture failure reasons. Never a raw platform message. */
export const CaptureFailure = {
  /** The user or policy denied microphone access (`NotAllowedError`). */
  PermissionDenied: 'permission_denied',
  /** No microphone is available (`NotFoundError`). */
  NoDevice: 'no_device',
  /** The device exists but is in use or unreadable (`NotReadableError`). */
  DeviceUnavailable: 'device_unavailable',
  /** The requested constraints cannot be satisfied (`OverconstrainedError`). */
  Unsupported: 'unsupported',
  /** The platform lacks the capture APIs entirely. */
  PlatformUnsupported: 'platform_unsupported',
  /** Anything else. Deliberately last and deliberately opaque. */
  Unknown: 'unknown',
} as const;

/** One of {@link CaptureFailure}. */
export type CaptureFailure = (typeof CaptureFailure)[keyof typeof CaptureFailure];

/** A typed capture failure. Carries a bounded reason, never a platform string. */
export class MediaCaptureError extends Error {
  readonly reason: CaptureFailure;

  constructor(reason: CaptureFailure, message: string) {
    super(message);
    this.name = 'MediaCaptureError';
    this.reason = reason;
  }
}

/** Human-readable, bounded text per reason. Static strings only. */
export const CAPTURE_MESSAGE: Record<CaptureFailure, string> = {
  [CaptureFailure.PermissionDenied]:
    'microphone access was denied; the participant must grant it before media can be sent',
  [CaptureFailure.NoDevice]: 'no microphone is available on this device',
  [CaptureFailure.DeviceUnavailable]: 'the microphone is in use or could not be read',
  [CaptureFailure.Unsupported]: 'the requested capture constraints cannot be satisfied',
  [CaptureFailure.PlatformUnsupported]:
    'this platform does not expose microphone capture; the media pipeline requires a secure ' +
    'context with getUserMedia and MediaStreamTrackProcessor',
  [CaptureFailure.Unknown]: 'microphone capture failed',
};

/**
 * Map a `getUserMedia` rejection onto the bounded vocabulary.
 *
 * The platform's `name` is a small, stable, non-secret enum and is the ONLY
 * thing read. The platform's `message` is carried nowhere, because `sdk-core` is
 * a published SDK whose `Error.message` reaches an embedder's error-reporting
 * hook.
 *
 * `SecurityError` joins `NotAllowedError` because both mean the page may not
 * have the device; `AbortError` joins `NotReadableError` because both mean the
 * device exists and could not be opened. Neither pairing is arbitrary and both
 * are pinned by tests, so a future split is a deliberate act rather than a
 * silent one.
 */
export function classifyCaptureError(err: unknown): CaptureFailure {
  const name =
    typeof err === 'object' && err !== null ? (err as { name?: unknown }).name : undefined;
  switch (name) {
    case 'NotAllowedError':
    case 'SecurityError':
      return CaptureFailure.PermissionDenied;
    case 'NotFoundError':
      return CaptureFailure.NoDevice;
    case 'NotReadableError':
    case 'AbortError':
      return CaptureFailure.DeviceUnavailable;
    case 'OverconstrainedError':
      return CaptureFailure.Unsupported;
    default:
      return CaptureFailure.Unknown;
  }
}

/** Build the typed error for a classified failure. */
export function captureError(reason: CaptureFailure): MediaCaptureError {
  return new MediaCaptureError(reason, CAPTURE_MESSAGE[reason]);
}
