// File: packages/sdk-core/src/media/setup/__tests__/videoCapture.test.ts
//
// Story 3 R-27: the test-pattern capture adapter, driven with an injected
// platform (canvas, frame wrapper, timers) — and its build gate.

import { describe, expect, it } from 'vitest';

import { TestPatternSource, type PatternPaintTarget } from '../testPattern.js';
import {
  buildTestPatternCapture,
  createTestPatternCapture,
  type TestPatternPlatform,
} from '../videoCapture.js';
import { TEST_PATTERN_REFUSAL } from '../testPattern.js';

function fakePlatform() {
  const ticks: (() => void)[] = [];
  let cleared = 0;
  const painted: boolean[][] = [];
  const platform: TestPatternPlatform = {
    createCanvas: () => {
      let row: boolean[] = [];
      const context: PatternPaintTarget = {
        fillStyle: '',
        fillRect() {
          row.push(this.fillStyle === '#ffffff');
          if (row.length === 24) {
            painted.push(row);
            row = [];
          }
        },
      };
      return { context };
    },
    createFrame: (_canvas, timestampUs) =>
      ({ timestamp: timestampUs, close: () => undefined }) as unknown as VideoFrame,
    setInterval: (fn) => {
      ticks.push(fn);
      return ticks.length as unknown as ReturnType<typeof setInterval>;
    },
    clearInterval: () => {
      cleared += 1;
    },
  };
  return {
    platform,
    painted,
    tick: () => ticks.at(-1)?.(),
    cleared: () => cleared,
  };
}

/** The README's counter bits (C3..C0 at indices 19..22) as a number. */
function counterOf(bits: boolean[]): number {
  const g = [19, 20, 21, 22].reduce((v, i) => (v << 1) | (bits[i] ? 1 : 0), 0);
  // Gray -> binary.
  let n = g;
  for (let shift = g >> 1; shift; shift >>= 1) n ^= shift;
  return n;
}

describe('test-pattern capture', () => {
  it('yields one frame per tick and advances the counter once per CAPTURED frame', async () => {
    const p = fakePlatform();
    const source = new TestPatternSource(258);
    const capture = buildTestPatternCapture(
      { width: 320, height: 180, frameRateHz: 15, source },
      p.platform,
    );
    const frames: VideoFrame[] = [];
    await capture.start(
      (f) => frames.push(f),
      () => undefined,
    );
    for (let i = 0; i < 5; i += 1) p.tick();
    expect(frames).toHaveLength(5);
    expect(p.painted.map(counterOf)).toEqual([0, 1, 2, 3, 4]);
    expect(frames.map((f) => f.timestamp)).toEqual(
      [0, 1, 2, 3, 4].map((i) => Math.round((i * 1000 * 1000) / 15)),
    );
  });

  it('stops on stop() (the synthetic camera light) and CONTINUES its counter across a restart', async () => {
    const p = fakePlatform();
    const source = new TestPatternSource(258);
    const opts = { width: 320, height: 180, frameRateHz: 15, source };
    const first = buildTestPatternCapture(opts, p.platform);
    await first.start(
      () => undefined,
      () => undefined,
    );
    p.tick();
    p.tick();
    first.stop();
    expect(p.cleared()).toBe(1);
    p.tick(); // a stale tick after stop yields nothing
    expect(p.painted).toHaveLength(2);
    const second = buildTestPatternCapture(opts, p.platform);
    await second.start(
      () => undefined,
      () => undefined,
    );
    p.tick();
    expect(p.painted.map(counterOf)).toEqual([0, 1, 2]);
  });

  it('the GATED factory refuses when __DT_TEST_PATTERN__ is false (every test runner)', async () => {
    expect(__DT_TEST_PATTERN__).toBe(false);
    const factory = createTestPatternCapture(new TestPatternSource(258));
    await expect(
      factory({ width: 320, height: 180, frameRateHz: 15, deviceId: undefined }),
    ).rejects.toThrow(TEST_PATTERN_REFUSAL);
  });
});
