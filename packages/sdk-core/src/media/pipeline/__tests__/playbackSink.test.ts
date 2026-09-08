// File: packages/sdk-core/src/media/pipeline/__tests__/playbackSink.test.ts
//
// The scheduling logic, tested against an injected context so no `AudioContext`
// is required. `setup/audioPlayback.ts` — which builds the real context and
// refuses a suspended one — is the thin construction half and is excluded from
// the coverage gate with that reason stated in `vitest.config.ts`.

import { describe, expect, it } from 'vitest';
import { makeAudioData } from '@darktower/test-utils';

import { createScheduledPlaybackSink } from '../playbackSink.js';

interface Started {
  readonly at: number;
  readonly frames: number;
}

/** A minimal `BaseAudioContext` stand-in that records scheduling decisions. */
function fakeContext(): {
  context: Parameters<typeof createScheduledPlaybackSink>[0];
  started: Started[];
  advance(seconds: number): void;
} {
  let now = 0;
  const started: Started[] = [];
  const destination = {} as AudioNode;
  const context = {
    get currentTime(): number {
      return now;
    },
    destination,
    createBuffer(channels: number, frames: number, sampleRate: number) {
      const data = new Float32Array(frames);
      return {
        length: frames,
        sampleRate,
        numberOfChannels: channels,
        getChannelData: () => data,
      } as unknown as AudioBuffer;
    },
    createBufferSource() {
      let buffer: AudioBuffer | null = null;
      return {
        set buffer(value: AudioBuffer | null) {
          buffer = value;
        },
        get buffer(): AudioBuffer | null {
          return buffer;
        },
        connect(): void {},
        start(at: number): void {
          started.push({ at, frames: buffer?.length ?? 0 });
        },
      } as unknown as AudioBufferSourceNode;
    },
  } as unknown as Parameters<typeof createScheduledPlaybackSink>[0];
  return {
    context,
    started,
    advance(seconds: number): void {
      now += seconds;
    },
  };
}

const SAMPLE_RATE = 48_000;
const FRAME_SAMPLES = 960; // 20 ms

function frame(onClose?: () => void) {
  return makeAudioData({
    samples: new Float32Array(FRAME_SAMPLES).fill(1),
    sampleRate: SAMPLE_RATE,
    ...(onClose ? { onClose } : {}),
  });
}

describe('scheduled playback sink', () => {
  it('schedules frames back to back rather than all at once', () => {
    // Without an advancing playhead every frame would start at `currentTime` and
    // the platform would play them simultaneously — audible as a burst, not as
    // speech.
    const { context, started } = fakeContext();
    const sink = createScheduledPlaybackSink(context, 1);
    sink.enqueue(frame() as never);
    sink.enqueue(frame() as never);
    sink.enqueue(frame() as never);

    expect(started).toHaveLength(3);
    const frameSeconds = FRAME_SAMPLES / SAMPLE_RATE;
    expect(started[1]!.at - started[0]!.at).toBeCloseTo(frameSeconds, 6);
    expect(started[2]!.at - started[1]!.at).toBeCloseTo(frameSeconds, 6);
  });

  it('RE-SEATS the playhead after a gap instead of scheduling in the past', () => {
    // A gap where nothing arrived leaves the playhead behind `currentTime`.
    // Scheduling relative to it would queue audio in the past, which the
    // platform plays immediately and all at once. Re-seating turns a gap into
    // silence followed by resumed audio, which is what a listener expects.
    const { context, started, advance } = fakeContext();
    const sink = createScheduledPlaybackSink(context, 1);
    sink.enqueue(frame() as never);
    advance(5);
    sink.enqueue(frame() as never);
    expect(started[1]!.at).toBeGreaterThanOrEqual(5);
  });

  it('RELEASES every frame it is handed, including after close', () => {
    // `AudioData` holds a platform-side buffer that is not garbage collected on
    // our schedule. At 50 frames a second a leak is a real resource fault.
    const { context } = fakeContext();
    const sink = createScheduledPlaybackSink(context, 1);
    let closed = 0;
    sink.enqueue(frame(() => (closed += 1)) as never);
    sink.close();
    sink.enqueue(frame(() => (closed += 1)) as never);
    expect(closed).toBe(2);
  });

  it('releases the frame even when the copy throws', () => {
    const { context } = fakeContext();
    const sink = createScheduledPlaybackSink(context, 1);
    let closed = 0;
    const hostile = {
      ...frame(() => (closed += 1)),
      copyTo(): void {
        throw new Error('copy failed');
      },
    };
    expect(() => sink.enqueue(hostile as never)).toThrow('copy failed');
    expect(closed).toBe(1);
  });

  it('stops scheduling after close', () => {
    const { context, started } = fakeContext();
    const sink = createScheduledPlaybackSink(context, 1);
    sink.close();
    sink.enqueue(frame() as never);
    expect(started).toEqual([]);
  });
});
