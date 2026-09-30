// The test-tone capture's graph (story 2 R-7), against platform stubs.
//
// Regression pinned here: a `MediaStreamAudioDestinationNode` defaults to STEREO,
// and Chromium's `AudioEncoder` rejects a 2-channel frame against the mono
// config — a FATAL encoder fault on the first frame, so a tone build sent
// nothing. Found only by the Layer-7 multi-party env-test (story 2 task 15); the
// pipeline tiers use seam doubles and never build this graph.

import { afterEach, describe, expect, it } from 'vitest';
import { createTestToneCapture } from '../capture.js';

interface Recorded {
  destination?: { channelCount: number; channelCountMode: string };
  frequency?: number;
}

function stubPlatform(recorded: Recorded): () => void {
  const g = globalThis as Record<string, unknown>;
  const saved = { AudioContext: g['AudioContext'], Processor: g['MediaStreamTrackProcessor'] };
  const track = { addEventListener: () => {}, stop: () => {} };
  g['AudioContext'] = class {
    createOscillator() {
      const osc = {
        type: '',
        frequency: { value: 0 },
        connect: () => {},
        disconnect: () => {},
        start: () => {
          recorded.frequency = osc.frequency.value;
        },
      };
      return osc;
    }
    createMediaStreamDestination() {
      const dest = {
        channelCount: 2,
        channelCountMode: 'max',
        stream: { getAudioTracks: () => [track] },
      };
      recorded.destination = dest;
      return dest;
    }
    async resume() {}
    async close() {}
  };
  g['MediaStreamTrackProcessor'] = class {
    readable = { getReader: () => ({ read: () => new Promise(() => {}), cancel: async () => {} }) };
  };
  return () => {
    g['AudioContext'] = saved.AudioContext;
    g['MediaStreamTrackProcessor'] = saved.Processor;
  };
}

let restore: (() => void) | undefined;
afterEach(() => {
  restore?.();
  restore = undefined;
});

describe('createTestToneCapture', () => {
  it.each([1, 2])(
    'pins the tone track to the encoder channel count (%i), explicitly',
    async (channels) => {
      const recorded: Recorded = {};
      restore = stubPlatform(recorded);
      const capture = await createTestToneCapture({
        sampleRateHz: 48_000,
        channels,
        frequencyHz: 625,
      });
      await capture.start(
        () => {},
        () => {},
      );
      expect(recorded.destination).toMatchObject({
        channelCount: channels,
        channelCountMode: 'explicit',
      });
      expect(recorded.frequency).toBe(625);
      capture.stop();
    },
  );
});
