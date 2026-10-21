// File: packages/web-app/e2e/patternDetector.ts
//
// Story 3 R-32 layer 3: the RECEIVE-SIDE VIDEO PATTERN DETECTOR. Pure (no
// `./env`, no Playwright, no clock), so the node unit tier
// (`tests/patternDetector.test.ts`) self-tests it against synthetic block lumas
// built from the README spec.
//
// ---------------------------------------------------------------------------
// THE SPEC LIVES IN PROSE, ONCE
// ---------------------------------------------------------------------------
//
// The test-pattern SPEC is written once, in `e2e/README.md` §"Video test
// pattern". The client's generator restates that prose in its own file
// header. This module is written from the README only:
//
//   * it imports NOTHING from `src/`, `@darktower/sdk-core` or any generator
//     module, so a shared bug cannot cancel out (the testTone.ts/toneDetector.ts
//     split). `tests/patternDetector.test.ts` enforces that rule, with a
//     positive control;
//   * its self-test does not import the layout or threshold constants below. It
//     transcribes the grid from the README independently.
//
// ---------------------------------------------------------------------------
// WHAT IT RUNS OVER
// ---------------------------------------------------------------------------
//
// One sample is the 24 block-mean lumas the test-build bus projects for one
// video slot (`videoSlots[].lumas`). They are sampled at the slot's RENDER
// surface, so a frame drawn into the wrong slot reads as the wrong id. The array
// is in row-major block order, integers 0..255. Lumas are pixel statistics, not
// identity. The decoded id is computed HERE, test-side, and is never written
// back to the bus.
//
// ---------------------------------------------------------------------------
// THE RULES
// ---------------------------------------------------------------------------
//
// - A block is BLACK at <= BLACK_MAX, WHITE at >= WHITE_MIN, and UNDECIDED in
//   between. An undecided block is never coerced to a bit. A sample with any
//   undecided block is "not a pattern frame" (`decided: false`): no guard pass,
//   no id, no counter.
// - The guard passes when the four corner blocks read the fixed checkerboard
//   (TL white, TR black, BL black, BR white).
// - The id is read only from a decided, guard-passing sample: 16 bits, MSB
//   first, white = 1.
// - The counter is the Gray-decoded 4-bit value of the counter blocks.
// - Over a window: the guard passes only if EVERY sample passed. Liveness counts
//   only the decided, guard-passing samples. It is `live` when there are at
//   least two of them and the counter differs across any pair. It never requires
//   "+1", because abandonment (R-12) legitimately skips frames. One or zero valid
//   samples is `insufficient`, which is reported apart from `frozen`.
// - Malformed input THROWS `PatternInputError`. A harness fault is never
//   reported as a verdict (e.g. "guard failed").

/** Grid geometry. */
export const GRID_COLUMNS = 6;
export const GRID_ROWS = 4;
export const BLOCK_COUNT = GRID_COLUMNS * GRID_ROWS;

/**
 * DESIGN EXPECTATION, not a measured guarantee. The generator paints pure black
 * (0) and pure white (255) in large flat blocks, which VP8 at the Kind test
 * profile is not expected to smear, and the projection averages each block's
 * central half. Each threshold leaves a margin of 80 on its own side of the
 * generator's value; the 94-wide band between them is UNDECIDED, which catches
 * mid-grey garbage, half-blended blocks and wrong geometry instead of rounding
 * them to a bit.
 *
 * Supporting evidence (paired-client, T-A review): 40 frames of this pattern at
 * 320x180 / 15 fps / 250 kbps, VP8-encoded, decoded and drawn to a canvas, gave
 * central-half block means of black max 1 and white min 255. C-C1's first live
 * run confirms it at the real render surface.
 */
export const BLACK_MAX = 80;
/** See {@link BLACK_MAX}. */
export const WHITE_MIN = 175;

/** Row-major index of the block in column `c`, row `r`. */
const at = (c: number, r: number): number => r * GRID_COLUMNS + c;

/** The four corner guards and their fixed checkerboard value (1 = white). */
const GUARDS: ReadonlyArray<readonly [index: number, bit: 0 | 1]> = [
  [at(0, 0), 1],
  [at(5, 0), 0],
  [at(0, 3), 0],
  [at(5, 3), 1],
];

/**
 * The 16 id blocks, MSB first: row 0 columns 1..4, then rows 1 and 2 in full.
 */
const ID_BLOCKS: readonly number[] = [
  at(1, 0),
  at(2, 0),
  at(3, 0),
  at(4, 0),
  ...[0, 1, 2, 3, 4, 5].map((c) => at(c, 1)),
  ...[0, 1, 2, 3, 4, 5].map((c) => at(c, 2)),
];

/** The four Gray-coded counter blocks, MSB first: row 3 columns 1..4. */
const COUNTER_BLOCKS: readonly number[] = [at(1, 3), at(2, 3), at(3, 3), at(4, 3)];

/** Malformed detector input: a HARNESS error, never a verdict. */
export class PatternInputError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PatternInputError';
  }
}

/** One sample's reading. */
export interface PatternReading {
  /** Every block decided (black or white, none in the undecided band). */
  readonly decided: boolean;
  /** Decided AND the corner checkerboard matched. */
  readonly guardPass: boolean;
  /** The 16-bit id, only when `guardPass`. */
  readonly id: number | null;
  /** The Gray-decoded counter 0..15, only when `guardPass`. */
  readonly counter: number | null;
}

/** Validate one luma array, or throw {@link PatternInputError}. */
function validate(lumas: unknown, where: string): readonly number[] {
  if (!Array.isArray(lumas)) {
    throw new PatternInputError(`${where}: lumas must be an array of ${BLOCK_COUNT} integers`);
  }
  if (lumas.length !== BLOCK_COUNT) {
    throw new PatternInputError(
      `${where}: expected ${BLOCK_COUNT} block lumas, got ${lumas.length}`,
    );
  }
  lumas.forEach((v: unknown, i) => {
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 0 || v > 255) {
      throw new PatternInputError(
        `${where}: block ${i} is not an integer luma in 0..255 (${String(v)})`,
      );
    }
  });
  return lumas as number[];
}

/** 0, 1, or null for an undecided block. */
function bitOf(luma: number): 0 | 1 | null {
  if (luma <= BLACK_MAX) return 0;
  if (luma >= WHITE_MIN) return 1;
  return null;
}

/** Gray code to binary (4 bits). */
function grayToBinary(gray: number): number {
  let n = gray;
  for (let shift = 1; shift < 4; shift <<= 1) n ^= n >> shift;
  return n;
}

/**
 * Decode one sample. Throws {@link PatternInputError} on malformed input.
 */
export function decodePattern(lumas: unknown): PatternReading {
  const values = validate(lumas, 'decodePattern');
  const bits = values.map(bitOf);
  if (bits.some((b) => b === null)) {
    return { decided: false, guardPass: false, id: null, counter: null };
  }
  const bit = (i: number): number => bits[i] as number;
  const guardPass = GUARDS.every(([i, expected]) => bit(i) === expected);
  if (!guardPass) return { decided: true, guardPass: false, id: null, counter: null };
  const id = ID_BLOCKS.reduce((acc, i) => (acc << 1) | bit(i), 0);
  const gray = COUNTER_BLOCKS.reduce((acc, i) => (acc << 1) | bit(i), 0);
  return { decided: true, guardPass: true, id, counter: grayToBinary(gray) };
}

/** Liveness over a window. */
export type PatternLiveness = 'live' | 'frozen' | 'insufficient';

/** The window verdict for one slot against the id the test expects there. */
export interface SlotPatternVerdict {
  /** Every valid sample decoded the expected id, and there was at least one. */
  readonly idMatch: boolean;
  /** Every sample was decided and passed the guard. */
  readonly guardPass: boolean;
  readonly liveness: PatternLiveness;
  /** The distinct ids decoded over the window, sorted (test-side only). */
  readonly decodedIds: readonly number[];
  /** Samples in the window, and how many were decided and guard-passing. */
  readonly samples: number;
  readonly validSamples: number;
}

/**
 * Judge one slot's window of samples (in bus order) against `expectedId`. The
 * caller passes only samples taken while the slot was rendering video from the
 * expected sender (see `videoEvidence.ts`). Throws {@link PatternInputError}
 * on any malformed sample or an invalid `expectedId`.
 */
export function judgeSlotPattern(
  samples: readonly unknown[],
  expectedId: number,
): SlotPatternVerdict {
  if (!Number.isInteger(expectedId) || expectedId < 1 || expectedId > 0xffff) {
    throw new PatternInputError(`expected id must be an integer in 1..65535 (${expectedId})`);
  }
  const readings = samples.map((s, i) => {
    validate(s, `judgeSlotPattern sample ${i}`);
    return decodePattern(s);
  });
  const valid = readings.filter((r) => r.guardPass);
  const counters = valid.map((r) => r.counter as number);
  const ids = [...new Set(valid.map((r) => r.id as number))].sort((a, b) => a - b);
  let liveness: PatternLiveness;
  if (valid.length < 2) liveness = 'insufficient';
  else liveness = counters.some((c) => c !== counters[0]) ? 'live' : 'frozen';
  return {
    idMatch: ids.length === 1 && ids[0] === expectedId,
    guardPass: readings.length > 0 && readings.every((r) => r.guardPass),
    liveness,
    decodedIds: ids,
    samples: readings.length,
    validSamples: valid.length,
  };
}
