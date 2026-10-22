// File: packages/sdk-core/src/media/setup/__tests__/testPattern.test.ts
//
// Story 3 R-27: the test-pattern generator against the test's OWN
// transcription of `packages/web-app/e2e/README.md` §"Video test pattern (THE
// spec)" — never against the generator's constants (an expectation taken from
// the subject is vacuous).

import { describe, expect, it } from 'vitest';

import {
  TestPatternSource,
  paintTestPattern,
  testPatternBits,
  testPatternBlocks,
  testPatternRects,
  type PatternPaintTarget,
} from '../testPattern.js';

// --- The test's own transcription of the README table ------------------------
//
//   row | c0  | c1  | c2  | c3  | c4  | c5
//    0  | G0  | I15 | I14 | I13 | I12 | G1
//    1  | I11 | I10 | I9  | I8  | I7  | I6
//    2  | I5  | I4  | I3  | I2  | I1  | I0
//    3  | G2  | C3  | C2  | C1  | C0  | G3
// G0 white, G1 black, G2 black, G3 white. White = 1.
type Cell = { g: 0 | 1 } | { i: number } | { c: number };
const TABLE: Cell[] = [
  { g: 1 }, { i: 15 }, { i: 14 }, { i: 13 }, { i: 12 }, { g: 0 },
  { i: 11 }, { i: 10 }, { i: 9 }, { i: 8 }, { i: 7 }, { i: 6 },
  { i: 5 }, { i: 4 }, { i: 3 }, { i: 2 }, { i: 1 }, { i: 0 },
  { g: 0 }, { c: 3 }, { c: 2 }, { c: 1 }, { c: 0 }, { g: 1 },
]; // prettier-ignore

function expectedBits(senderId: number, frameIndex: number): boolean[] {
  const n = frameIndex % 16;
  const gray = n ^ (n >> 1);
  return TABLE.map((cell) => {
    if ('g' in cell) return cell.g === 1;
    if ('i' in cell) return ((senderId >> cell.i) & 1) === 1;
    return ((gray >> cell.c) & 1) === 1;
  });
}

/** The README's floor() block edges, transcribed. */
function expectedRect(c: number, r: number, w: number, h: number) {
  const x0 = Math.floor((c * w) / 6);
  const x1 = Math.floor(((c + 1) * w) / 6);
  const y0 = Math.floor((r * h) / 4);
  const y1 = Math.floor(((r + 1) * h) / 4);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** A recording 2D-context fake: what fillRect was asked to paint, in what colour. */
class RecordingContext implements PatternPaintTarget {
  fillStyle: string | CanvasGradient | CanvasPattern = '';
  readonly calls: { x: number; y: number; w: number; h: number; colour: string }[] = [];
  fillRect(x: number, y: number, w: number, h: number): void {
    this.calls.push({ x, y, w, h, colour: String(this.fillStyle) });
  }
}

describe('test pattern — bits (README table)', () => {
  it.each([1, 0x8001, 0xa5a5, 65535])('paints sender id %i per the README table', (id) => {
    for (let frame = 0; frame < 3; frame += 1) {
      expect(testPatternBits(id, frame)).toEqual(expectedBits(id, frame));
    }
  });

  it('the guards are the fixed checkerboard G0 W, G1 B, G2 B, G3 W', () => {
    const bits = testPatternBits(0xa5a5, 0);
    expect([bits[0], bits[5], bits[18], bits[23]]).toEqual([true, false, false, true]);
  });

  it('the counter is Gray-coded mod 16, MSB first, and adjacent CAPTURED frames differ by exactly one bit', () => {
    const counterOf = (frame: number): number[] =>
      [19, 20, 21, 22].map((i) => (testPatternBits(1, frame)[i] ? 1 : 0));
    const seen: string[] = [];
    for (let f = 0; f < 34; f += 1) {
      const now = counterOf(f);
      const next = counterOf(f + 1);
      const diff = now.reduce((d, b, i) => d + (b === next[i] ? 0 : 1), 0);
      expect(diff, `frames ${f} -> ${f + 1}`).toBe(1);
      seen.push(now.join(''));
    }
    // It wraps: 16 distinct values, then repeats.
    expect(new Set(seen.slice(0, 16)).size).toBe(16);
    expect(seen[16]).toBe(seen[0]);
    expect(seen.slice(0, 4)).toEqual(['0000', '0001', '0011', '0010']);
  });

  it.each([0, 65536, -1, Number.NaN, 1.5])(
    'THROWS for sender id %s — never a default watermark',
    (id) => {
      expect(() => testPatternBits(id, 0)).toThrow(RangeError);
      expect(() => new TestPatternSource(id)).toThrow(RangeError);
    },
  );
});

describe('test pattern — geometry (floor() edges)', () => {
  it.each([
    [320, 180],
    [640, 360],
  ])('block rects at %ix%i follow the README floor() edges and tile the picture', (w, h) => {
    const rects = testPatternRects(w, h);
    expect(rects).toHaveLength(24);
    for (let r = 0; r < 4; r += 1) {
      for (let c = 0; c < 6; c += 1) {
        expect(rects[r * 6 + c]).toEqual(expectedRect(c, r, w, h));
      }
    }
    expect(rects.reduce((area, b) => area + b.width * b.height, 0)).toBe(w * h);
  });

  it('pins pixel-level edges at a NON-divisible width (320 / 6 = 53.33)', () => {
    const rects = testPatternRects(320, 180);
    expect(rects.slice(0, 6).map((b) => b.x)).toEqual([0, 53, 106, 160, 213, 266]);
    expect(rects.slice(0, 6).map((b) => b.width)).toEqual([53, 53, 54, 53, 53, 54]);
    expect([0, 6, 12, 18].map((i) => rects[i]!.y)).toEqual([0, 45, 90, 135]);
  });
});

describe('test pattern — the painter (not only the pure bits)', () => {
  it('fills exactly the 24 rects with pure black/white, flat, per the table', () => {
    const ctx = new RecordingContext();
    paintTestPattern(ctx, 0xa5a5, 5, 320, 180);
    const bits = expectedBits(0xa5a5, 5);
    expect(ctx.calls).toHaveLength(24);
    ctx.calls.forEach((call, i) => {
      const rect = expectedRect(i % 6, Math.floor(i / 6), 320, 180);
      expect({ x: call.x, y: call.y, width: call.w, height: call.h }).toEqual(rect);
      expect(call.colour).toBe(bits[i] ? '#ffffff' : '#000000');
    });
  });

  it('the blocks list zips rects and bits', () => {
    const blocks = testPatternBlocks(7, 0, 640, 360);
    expect(blocks.map((b) => b.white)).toEqual(expectedBits(7, 0));
  });

  it('TestPatternSource advances its counter once per CAPTURED (painted) frame', () => {
    const source = new TestPatternSource(0x8001);
    expect(source.patternId).toBe(0x8001);
    for (let f = 0; f < 18; f += 1) {
      const ctx = new RecordingContext();
      source.paintNext(ctx, 320, 180);
      const bits = expectedBits(0x8001, f);
      expect(ctx.calls.map((c) => c.colour === '#ffffff')).toEqual(bits);
    }
  });
});
