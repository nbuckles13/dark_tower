// Node-tier SELF-TEST of the video pattern detector (`e2e/patternDetector.ts`,
// story 3 R-32 layer 3) against synthetic block lumas.
//
// The inputs are built from the README spec itself (`e2e/README.md` §"Video
// test pattern"), parsed by the test-only `./_patternSpec`. It imports no layout or
// threshold constant from the detector: a test built from the detector's own
// tables would agree with it by construction and catch nothing. Literal grids
// anchor the parse itself. The last block enforces the import rule
// (no src/, no sdk-core, no generator) with a positive control. Every input is
// deterministic; nothing is timed.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import { decodePattern, judgeSlotPattern, PatternInputError } from '../e2e/patternDetector.js';
import {
  importedSpecifiers,
  parsePatternSpec,
  PatternSpecExtractionError,
  specLumas,
} from './_patternSpec.js';

const WHITE = 255;
const BLACK = 0;

/** The 24 nominal lumas the README spec draws for (senderId, frame n). */
function spec(senderId: number, frame: number, white = WHITE, black = BLACK): number[] {
  return specLumas(senderId, frame, white, black);
}

/** As a 4x6 grid, for spatial transforms. */
const toGrid = (l: readonly number[]): number[][] =>
  [0, 1, 2, 3].map((r) => l.slice(r * 6, r * 6 + 6));
const fromGrid = (g: readonly (readonly number[])[]): number[] => g.flat();

describe('the README parse (extraction errors)', () => {
  test('a missing section or a non-4x6 table throws a distinct extraction error', () => {
    expect(() => parsePatternSpec('# nothing here')).toThrow(PatternSpecExtractionError);
    const short = '### Video test pattern\n| 0 | G0 | I15 | I14 | I13 | I12 | G1 |\n';
    expect(() => parsePatternSpec(short)).toThrow(/not 4x6/);
  });
});

describe('the parsed spec itself (literal anchors)', () => {
  test('sender 0x8001 frame 0: corners, I15 (index 1) and I0 (index 17) white, all else black', () => {
    const expected = new Array<number>(24).fill(BLACK);
    for (const i of [0, 1, 17, 23]) expected[i] = WHITE;
    expect(spec(0x8001, 0)).toEqual(expected);
  });
  test('frame 1 sets C0 (index 22); frame 2 is Gray 3 (C1 and C0, indices 21 and 22)', () => {
    const f1 = spec(0x8001, 1);
    const f2 = spec(0x8001, 2);
    expect([f1[19], f1[20], f1[21], f1[22]]).toEqual([BLACK, BLACK, BLACK, WHITE]);
    expect([f2[19], f2[20], f2[21], f2[22]]).toEqual([BLACK, BLACK, WHITE, WHITE]);
  });
});

describe('decodePattern', () => {
  test.each(Array.from({ length: 16 }, (_, b) => b))('decodes id bit %i alone', (b) => {
    const id = 1 << b;
    expect(decodePattern(spec(id, 0))).toEqual({
      decided: true,
      guardPass: true,
      id,
      counter: 0,
    });
  });

  test.each([1, 0x00ff, 0xa5a5, 0x5a5a, 0xffff, 12345])('round-trips id %i', (id) => {
    expect(decodePattern(spec(id, 7)).id).toBe(id);
  });

  test.each(Array.from({ length: 16 }, (_, n) => n))(
    'decodes counter %i from its Gray code',
    (n) => {
      expect(decodePattern(spec(0x1234, n)).counter).toBe(n);
    },
  );

  test('blank grids fail the guard (all black, all white)', () => {
    for (const v of [BLACK, WHITE]) {
      const r = decodePattern(new Array<number>(24).fill(v));
      expect(r.guardPass).toBe(false);
      expect(r.id).toBeNull();
    }
  });

  test('an inverted grid fails the guard', () => {
    const inverted = spec(0x1234, 3).map((v) => 255 - v);
    expect(decodePattern(inverted).guardPass).toBe(false);
  });

  test('mirrored and flipped grids fail the guard', () => {
    const g = toGrid(spec(0x1234, 3));
    const mirrored = fromGrid(g.map((row) => [...row].reverse()));
    const flipped = fromGrid([...g].reverse());
    expect(decodePattern(mirrored).guardPass).toBe(false);
    expect(decodePattern(flipped).guardPass).toBe(false);
  });

  test('shifted grids fail the guard where a corner lands on opposite content', () => {
    // One column right, vacated column replicated from the edge: TR reads I12.
    const right = (l: number[]) => fromGrid(toGrid(l).map((row) => [row[0]!, ...row.slice(0, 5)]));
    // I12 = 1 puts white on the black TR guard.
    expect(decodePattern(right(spec(1 << 12, 0))).guardPass).toBe(false);
    // One row down, vacated row replicated: BL reads row 2's I5.
    const down = (l: number[]) => {
      const g = toGrid(l);
      return fromGrid([g[0]!, g[0]!, g[1]!, g[2]!]);
    };
    expect(decodePattern(down(spec(1 << 5, 0))).guardPass).toBe(false);
  });

  test('a shifted LIVE window fails the guard on some sample (the counter corner moves)', () => {
    // Shift left: BR reads C0, which changes with the counter.
    const left = (l: number[]) => fromGrid(toGrid(l).map((row) => [...row.slice(1), row[5]!]));
    const window = [0, 1, 2, 3].map((n) => left(spec(0x0f0f, n)));
    expect(judgeSlotPattern(window, 0x0f0f).guardPass).toBe(false);
  });

  test('noise within the margin is accepted', () => {
    // black within 0..80, white within 175..255: the codec's observed drift and beyond.
    const noisy = spec(0xbeef, 5, 175, 80);
    expect(decodePattern(noisy)).toEqual({
      decided: true,
      guardPass: true,
      id: 0xbeef,
      counter: 5,
    });
    const mild = spec(0xbeef, 5, 236, 19);
    expect(decodePattern(mild).id).toBe(0xbeef);
  });

  test('noise beyond the margin is rejected as undecided, never coerced', () => {
    for (const [white, black] of [
      [174, 0],
      [255, 81],
      [128, 128],
    ] as const) {
      const r = decodePattern(spec(0xbeef, 5, white, black));
      expect(r).toEqual({ decided: false, guardPass: false, id: null, counter: null });
    }
    // One undecided block anywhere spoils the sample.
    const one = spec(0xbeef, 5);
    one[9] = 128;
    expect(decodePattern(one).decided).toBe(false);
  });

  test.each([
    ['23 lumas', new Array(23).fill(0)],
    ['25 lumas', new Array(25).fill(0)],
    ['NaN', [...new Array(23).fill(0), Number.NaN]],
    ['null', [...new Array(23).fill(0), null]],
    ['a non-integer', [...new Array(23).fill(0), 12.5]],
    ['-1', [...new Array(23).fill(0), -1]],
    ['256', [...new Array(23).fill(0), 256]],
    ['a string', [...new Array(23).fill(0), '0']],
    ['not an array', { length: 24 }],
    ['undefined', undefined],
  ])('THROWS a harness error on %s, never a verdict', (_label, input) => {
    expect(() => decodePattern(input)).toThrow(PatternInputError);
  });
});

describe('judgeSlotPattern', () => {
  const ID = 0x2468;

  test('live with gaps (abandonment skips frames): never requires +1', () => {
    const window = [0, 3, 4, 9, 15].map((n) => spec(ID, n));
    expect(judgeSlotPattern(window, ID)).toEqual({
      idMatch: true,
      guardPass: true,
      liveness: 'live',
      decodedIds: [ID],
      samples: 5,
      validSamples: 5,
    });
  });

  test('frozen: the same counter across the window', () => {
    const window = [6, 6, 6, 6].map((n) => spec(ID, n));
    const v = judgeSlotPattern(window, ID);
    expect(v.liveness).toBe('frozen');
    expect(v.idMatch).toBe(true);
  });

  test('a single sample is insufficient, reported apart from frozen', () => {
    expect(judgeSlotPattern([spec(ID, 1)], ID).liveness).toBe('insufficient');
    expect(judgeSlotPattern([], ID)).toMatchObject({
      liveness: 'insufficient',
      idMatch: false,
      guardPass: false,
    });
  });

  test('a guard failure mid-window fails the guard; liveness counts only valid samples', () => {
    const window = [spec(ID, 1), new Array<number>(24).fill(BLACK), spec(ID, 2)];
    const v = judgeSlotPattern(window, ID);
    expect(v.guardPass).toBe(false);
    expect(v.liveness).toBe('live');
    expect(v.validSamples).toBe(2);
  });

  test('the wrong sender fails idMatch and is named in decodedIds', () => {
    const v = judgeSlotPattern([spec(0x1111, 1), spec(0x1111, 2)], ID);
    expect(v.idMatch).toBe(false);
    expect(v.decodedIds).toEqual([0x1111]);
  });

  test('two senders in one window fail idMatch', () => {
    const v = judgeSlotPattern([spec(ID, 1), spec(0x1111, 2)], ID);
    expect(v.idMatch).toBe(false);
    expect(v.decodedIds).toEqual([0x1111, ID]);
  });

  test('malformed samples and expected ids THROW', () => {
    expect(() => judgeSlotPattern([spec(ID, 1), new Array(23).fill(0)], ID)).toThrow(
      PatternInputError,
    );
    for (const bad of [0, 65536, 1.5, Number.NaN]) {
      expect(() => judgeSlotPattern([spec(ID, 1)], bad)).toThrow(PatternInputError);
    }
  });
});

// ---------------------------------------------------------------------------
// Import rule: the detector shares no code with the generator or the app
// ---------------------------------------------------------------------------

function forbiddenImports(source: string): string[] {
  return importedSpecifiers(source).filter(
    (spec) =>
      spec === 'dynamic import' ||
      spec.includes('sdk-core') ||
      spec.includes('/src/') ||
      /pattern(?!Detector)/i.test(spec),
  );
}

describe('import rule', () => {
  test('patternDetector.ts imports nothing from src/, sdk-core or a generator', () => {
    const src = readFileSync(
      fileURLToPath(new URL('../e2e/patternDetector.ts', import.meta.url)),
      'utf8',
    );
    expect(forbiddenImports(src)).toEqual([]);
  });

  test('positive control: the check flags each coupling shape', () => {
    expect(forbiddenImports("import { x } from '@darktower/sdk-core';")).not.toEqual([]);
    expect(forbiddenImports("import type { Y } from '../src/lib/e2eBus.js';")).not.toEqual([]);
    expect(
      forbiddenImports("import { gen } from '../../sdk-core/src/media/testPattern.js';"),
    ).not.toEqual([]);
    expect(forbiddenImports("import { p } from './testPattern.js';")).not.toEqual([]);
    expect(forbiddenImports("const m = await import('./x.js');")).not.toEqual([]);
    expect(forbiddenImports("import '../src/lib/testPattern.js';")).not.toEqual([]);
    expect(forbiddenImports("const g = require('@darktower/sdk-core');")).not.toEqual([]);
    expect(forbiddenImports("import { sampleWindow } from './windowSampling.js';")).toEqual([]);
  });
});
