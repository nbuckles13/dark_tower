// File: packages/sdk-core/src/media/setup/__tests__/captureFailure.test.ts
//
// The classifier decides which bounded reason an operator sees for the
// silent-no-audio case this story exists to localise, so a wrong or opaque arm
// sends someone to the wrong subsystem. Every arm is pinned, including the two
// deliberate pairings, so a future split is a deliberate act rather than a
// silent one.

import { describe, expect, it } from 'vitest';

import {
  CAPTURE_MESSAGE,
  CaptureFailure,
  MediaCaptureError,
  captureError,
  classifyCaptureError,
} from '../captureFailure.js';

function named(name: string): unknown {
  const err = new Error('platform text that must never be carried');
  err.name = name;
  return err;
}

describe('classifyCaptureError', () => {
  it.each([
    ['NotAllowedError', CaptureFailure.PermissionDenied],
    ['SecurityError', CaptureFailure.PermissionDenied],
    ['NotFoundError', CaptureFailure.NoDevice],
    ['NotReadableError', CaptureFailure.DeviceUnavailable],
    ['AbortError', CaptureFailure.DeviceUnavailable],
    ['OverconstrainedError', CaptureFailure.Unsupported],
  ])('maps %s to %s', (name, expected) => {
    expect(classifyCaptureError(named(name))).toBe(expected);
  });

  it('pins the two deliberate pairings so a future split is deliberate', () => {
    // `SecurityError` joins `NotAllowedError` because both mean the page may not
    // have the device; `AbortError` joins `NotReadableError` because both mean
    // the device exists and could not be opened. Neither is arbitrary.
    expect(classifyCaptureError(named('SecurityError'))).toBe(
      classifyCaptureError(named('NotAllowedError')),
    );
    expect(classifyCaptureError(named('AbortError'))).toBe(
      classifyCaptureError(named('NotReadableError')),
    );
  });

  it('falls back to unknown rather than guessing', () => {
    expect(classifyCaptureError(named('SomeFutureError'))).toBe(CaptureFailure.Unknown);
    expect(classifyCaptureError(new Error('no name set'))).toBe(CaptureFailure.Unknown);
  });

  it('survives a non-Error rejection without throwing', () => {
    // `getUserMedia` is a platform call; a caller cannot assume what it rejects
    // with, and a classifier that threw would replace a typed capture failure
    // with an untyped one at exactly the wrong moment.
    for (const value of [undefined, null, 'a string', 42, {}, []]) {
      expect(classifyCaptureError(value)).toBe(CaptureFailure.Unknown);
    }
  });
});

describe('captureError', () => {
  it('carries the bounded reason and NEVER the platform message', () => {
    const err = captureError(classifyCaptureError(named('NotAllowedError')));
    expect(err).toBeInstanceOf(MediaCaptureError);
    expect(err.reason).toBe(CaptureFailure.PermissionDenied);
    expect(err.message).toBe(CAPTURE_MESSAGE[CaptureFailure.PermissionDenied]);
    // `sdk-core` is a published SDK: `Error.message` reaches an embedder's
    // error-reporting hook, so the platform's own text is carried nowhere.
    expect(err.message).not.toContain('platform text that must never be carried');
  });

  it('has a static message for every reason — no arm can produce an empty one', () => {
    for (const reason of Object.values(CaptureFailure)) {
      const err = captureError(reason);
      expect(err.reason).toBe(reason);
      expect(err.message.length).toBeGreaterThan(0);
    }
  });
});
