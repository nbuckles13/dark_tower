// File: packages/sdk-core/src/media/setup/__tests__/localPreview.test.ts
//
// The R-7 self-check's own capture (pre-join): one acquisition, the SAME
// bounded failure vocabulary as every capture (`classifyCaptureError`), and
// every acquired track stopped on `stop()` or on a no-track result.

import { describe, expect, it } from 'vitest';

import { CaptureFailure, MediaCaptureError } from '../captureFailure.js';
import { openLocalPreview, type PreviewAcquire } from '../localPreview.js';

class FakeTrack {
  stopped = 0;
  constructor(readonly kind: 'audio' | 'video') {}
  stop(): void {
    this.stopped += 1;
  }
}

function fakeStream(tracks: FakeTrack[]): MediaStream {
  return {
    getTracks: () => tracks,
    getVideoTracks: () => tracks.filter((t) => t.kind === 'video'),
    getAudioTracks: () => tracks.filter((t) => t.kind === 'audio'),
  } as unknown as MediaStream;
}

describe('openLocalPreview', () => {
  it('acquires exactly the requested kinds and stop() stops every track', async () => {
    const tracks = [new FakeTrack('video'), new FakeTrack('audio')];
    const asked: MediaStreamConstraints[] = [];
    const acquire: PreviewAcquire = async (c) => {
      asked.push(c);
      return fakeStream(tracks);
    };
    const preview = await openLocalPreview({ video: true, audio: true }, acquire);
    expect(asked).toEqual([{ video: true, audio: true }]);
    expect(preview.video).toBe(tracks[0]);
    expect(preview.audio).toBe(tracks[1]);
    preview.stop();
    expect(tracks.map((t) => t.stopped)).toEqual([1, 1]);
  });

  it.each([
    ['NotAllowedError', CaptureFailure.PermissionDenied],
    ['SecurityError', CaptureFailure.PermissionDenied],
    ['NotFoundError', CaptureFailure.NoDevice],
    ['NotReadableError', CaptureFailure.DeviceUnavailable],
    ['AbortError', CaptureFailure.DeviceUnavailable],
    ['OverconstrainedError', CaptureFailure.Unsupported],
    ['WeirdError', CaptureFailure.Unknown],
  ])('a %s rejection is the bounded %s token, never the platform message', async (name, reason) => {
    const acquire: PreviewAcquire = async () => {
      throw Object.assign(new Error('secret platform detail /dev/video0'), { name });
    };
    const err = await openLocalPreview({ video: true, audio: false }, acquire).catch((e) => e);
    expect(err).toBeInstanceOf(MediaCaptureError);
    expect((err as MediaCaptureError).reason).toBe(reason);
    expect((err as Error).message).not.toContain('secret');
  });

  it('a stream with no usable track is NoDevice, and anything acquired is stopped', async () => {
    const other = new FakeTrack('audio');
    const stream = {
      getTracks: () => [other],
      getVideoTracks: () => [],
      getAudioTracks: () => [],
    } as unknown as MediaStream;
    const err = await openLocalPreview({ video: true, audio: false }, async () => stream).catch(
      (e) => e,
    );
    expect((err as MediaCaptureError).reason).toBe(CaptureFailure.NoDevice);
    expect(other.stopped).toBe(1);
  });

  it('no platform acquisition is PlatformUnsupported; asking for nothing is refused', async () => {
    const none = await openLocalPreview({ video: true, audio: true }, undefined).catch((e) => e);
    expect((none as MediaCaptureError).reason).toBe(CaptureFailure.PlatformUnsupported);
    let called = 0;
    const empty = await openLocalPreview({ video: false, audio: false }, async () => {
      called += 1;
      return fakeStream([]);
    }).catch((e) => e);
    expect((empty as MediaCaptureError).reason).toBe(CaptureFailure.NoDevice);
    expect(called).toBe(0);
  });
});
