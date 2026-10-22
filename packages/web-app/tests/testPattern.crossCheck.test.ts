// File: packages/web-app/tests/testPattern.crossCheck.test.ts
//
// Story 3 R-27 / R-32: the SPEC-DRIFT cross-check between the SDK's test-pattern
// GENERATOR and the e2e DETECTOR. Neither module imports the other (the README
// rule); this TEST imports both, renders a generator frame through the SDK's own
// painter into an RGBA buffer, projects it with the README's render-surface luma
// rule (mean of Y = 0.299R + 0.587G + 0.114B over each block's CENTRAL HALF), and
// decodes it with `e2e/patternDetector.ts`. A disagreement between the two
// transcriptions of `e2e/README.md` §"Video test pattern (THE spec)" fails here.

import { describe, expect, it } from 'vitest';

import {
  paintTestPattern,
  type PatternPaintTarget,
} from '../../sdk-core/src/media/setup/testPattern.js';
import { decodePattern } from '../e2e/patternDetector.js';

/** A minimal 2D context over an RGBA buffer: flat `#rrggbb` fills only. */
class RgbaCanvas implements PatternPaintTarget {
  fillStyle: string | CanvasGradient | CanvasPattern = '#000000';
  readonly data: Uint8ClampedArray;
  constructor(
    readonly width: number,
    readonly height: number,
  ) {
    this.data = new Uint8ClampedArray(width * height * 4);
  }
  fillRect(x: number, y: number, w: number, h: number): void {
    const hex = String(this.fillStyle);
    const rgb = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
    for (let yy = y; yy < y + h; yy += 1) {
      for (let xx = x; xx < x + w; xx += 1) {
        const o = (yy * this.width + xx) * 4;
        this.data[o] = rgb[0]!;
        this.data[o + 1] = rgb[1]!;
        this.data[o + 2] = rgb[2]!;
        this.data[o + 3] = 255;
      }
    }
  }
}

/** The README's luma projection, transcribed independently of both modules. */
function projectLumas(canvas: RgbaCanvas): number[] {
  const out: number[] = [];
  for (let r = 0; r < 4; r += 1) {
    for (let c = 0; c < 6; c += 1) {
      const bx = Math.floor((c * canvas.width) / 6);
      const bw = Math.floor(((c + 1) * canvas.width) / 6) - bx;
      const by = Math.floor((r * canvas.height) / 4);
      const bh = Math.floor(((r + 1) * canvas.height) / 4) - by;
      let sum = 0;
      let n = 0;
      for (let y = Math.floor(by + bh / 4); y < by + (3 * bh) / 4; y += 1) {
        for (let x = Math.floor(bx + bw / 4); x < bx + (3 * bw) / 4; x += 1) {
          const o = (y * canvas.width + x) * 4;
          sum +=
            0.299 * canvas.data[o]! + 0.587 * canvas.data[o + 1]! + 0.114 * canvas.data[o + 2]!;
          n += 1;
        }
      }
      out.push(Math.round(sum / n));
    }
  }
  return out;
}

describe('test pattern generator <-> e2e detector (spec drift)', () => {
  it.each([
    [320, 180],
    [640, 360],
  ])('at %ix%i the detector reads back the painted id and counter for every frame', (w, h) => {
    for (const id of [1, 0x8001, 0xa5a5, 65535]) {
      for (let frame = 0; frame < 18; frame += 1) {
        const canvas = new RgbaCanvas(w, h);
        paintTestPattern(canvas, id, frame, w, h);
        const reading = decodePattern(projectLumas(canvas));
        expect(reading, `id ${id} frame ${frame}`).toEqual({
          decided: true,
          guardPass: true,
          id,
          counter: frame % 16,
        });
      }
    }
  });
});
