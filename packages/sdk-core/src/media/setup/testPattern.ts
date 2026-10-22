// File: packages/sdk-core/src/media/setup/testPattern.ts
//
// The synthetic test-pattern GENERATOR (story 3 R-27). Pure: no canvas, no
// timer, no VideoFrame here — the capture adapter that paints these blocks into
// an `OffscreenCanvas` is `createTestPatternCapture` in `./videoCapture.ts`,
// behind the same capture seam a camera uses.
//
// ---------------------------------------------------------------------------
// THE SPEC — restated from its SINGLE HOME
// ---------------------------------------------------------------------------
//
// `packages/web-app/e2e/README.md` §"Video test pattern (THE spec)" is the one
// normative text. This header restates it; on any disagreement the README wins
// and this file is wrong. The e2e detector (`e2e/patternDetector.ts`) is written
// from that text and shares NO code with this file: this file never imports
// from `e2e/`, and the detector never imports this file.
//
//   * INPUT: only the meeting-scoped `sender_id`, an integer in 1..65535
//     (`./meetingSenderId.ts`, the same check the test tone uses). Never a
//     durable identity — no user id, email, display name or token. Throws on 0,
//     out of range or a non-integer.
//   * GEOMETRY: the whole encoded picture (directed width W, height H) is a
//     6-column x 4-row grid of 24 blocks. Block (c, r) spans
//     x in [floor(c*W/6), floor((c+1)*W/6)) and y in [floor(r*H/4), floor((r+1)*H/4)).
//     Its row-major index is i = r*6 + c.
//   * COLOURS: every block is one flat colour, pure black rgb(0,0,0) or pure
//     white rgb(255,255,255). No gradients, text or borders. White = 1.
//   * ASSIGNMENT (row-major):
//
//         row | c0  | c1  | c2  | c3  | c4  | c5
//         ----+-----+-----+-----+-----+-----+-----
//          0  | G0  | I15 | I14 | I13 | I12 | G1
//          1  | I11 | I10 | I9  | I8  | I7  | I6
//          2  | I5  | I4  | I3  | I2  | I1  | I0
//          3  | G2  | C3  | C2  | C1  | C0  | G3
//
//     - I15..I0: the 16 bits of `sender_id`, MSB first (index 1 = bit 15 ...
//       index 17 = bit 0).
//     - GUARDS: a fixed corner checkerboard, G0 (top-left) = white,
//       G1 (top-right) = black, G2 (bottom-left) = black, G3 (bottom-right) =
//       white.
//     - C3..C0: the Gray code g = n ^ (n >> 1) of n = capturedFrameIndex mod 16,
//       MSB first. It advances on every CAPTURED frame (never per encoded frame),
//       so frames the encoder drops or a hop abandons simply skip values.
//   * GATING: reachable only behind the opt-in `__DT_TEST_PATTERN__` build define
//     (`packages/web-app/vite/testDefines.ts:resolveOptInTestDefine`); false in
//     every production build and in the SDK library build. The capture adapter
//     THROWS if reached without it, and the bundle-content tests prove the
//     marker strings below are absent from production output.

import { assertMeetingSenderId } from './meetingSenderId.js';

/** Grid columns (the spec's 6). */
export const TEST_PATTERN_COLUMNS = 6;
/** Grid rows (the spec's 4). */
export const TEST_PATTERN_ROWS = 4;
/** Captured frames before the counter wraps (the spec's mod 16). */
const COUNTER_MODULUS = 16;

/**
 * Generator-unique marker literal. It lives in the error message of the
 * pattern platform's missing-API path (`./videoCaptureBrowser.ts:
 * browserPatternPlatform`), which is LIVE code in a pattern build — unlike
 * {@link TEST_PATTERN_REFUSAL}, whose `!__DT_TEST_PATTERN__` branch folds away
 * there — so it is the token that proves both directions: present in the gated
 * positive-control bundle, absent from production (`tests/bundle-content.test.ts`).
 */
export const TEST_PATTERN_MARKER = 'dt-test-pattern';

/** The refusal the capture adapter throws when reached in a non-pattern build. */
export const TEST_PATTERN_REFUSAL =
  'dt-test-pattern: the synthetic test pattern was reached in a build without __DT_TEST_PATTERN__';

/** One block's pixel rectangle. */
export interface PatternRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** One painted block: its rectangle and whether it is white. */
export interface PatternBlock extends PatternRect {
  readonly white: boolean;
}

/**
 * The 24 block rectangles for a W x H picture, row-major, with the spec's
 * floor() edges. Pure; the painter consumes exactly this list.
 *
 * @throws {RangeError} for a non-positive or non-integer dimension.
 */
export function testPatternRects(width: number, height: number): readonly PatternRect[] {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new RangeError('a test pattern needs positive integer picture dimensions');
  }
  const rects: PatternRect[] = [];
  for (let r = 0; r < TEST_PATTERN_ROWS; r++) {
    const y0 = Math.floor((r * height) / TEST_PATTERN_ROWS);
    const y1 = Math.floor(((r + 1) * height) / TEST_PATTERN_ROWS);
    for (let c = 0; c < TEST_PATTERN_COLUMNS; c++) {
      const x0 = Math.floor((c * width) / TEST_PATTERN_COLUMNS);
      const x1 = Math.floor(((c + 1) * width) / TEST_PATTERN_COLUMNS);
      rects.push({ x: x0, y: y0, width: x1 - x0, height: y1 - y0 });
    }
  }
  return rects;
}

/**
 * The 24 block bits (white = 1), row-major, for one captured frame.
 *
 * @param capturedFrameIndex the index of this CAPTURED frame (0-based, any
 *   non-negative integer; reduced mod 16 here).
 * @throws {RangeError} for an invalid sender id or frame index.
 */
export function testPatternBits(senderId: number, capturedFrameIndex: number): readonly boolean[] {
  assertMeetingSenderId(senderId, 'test pattern');
  if (!Number.isInteger(capturedFrameIndex) || capturedFrameIndex < 0) {
    throw new RangeError('a test pattern frame index must be a non-negative integer');
  }
  const n = capturedFrameIndex % COUNTER_MODULUS;
  const gray = n ^ (n >> 1);
  const idBit = (bit: number): boolean => ((senderId >> bit) & 1) === 1;
  const counterBit = (bit: number): boolean => ((gray >> bit) & 1) === 1;
  return [
    // row 0: G0 I15 I14 I13 I12 G1
    true,
    idBit(15),
    idBit(14),
    idBit(13),
    idBit(12),
    false,
    // row 1: I11..I6
    idBit(11),
    idBit(10),
    idBit(9),
    idBit(8),
    idBit(7),
    idBit(6),
    // row 2: I5..I0
    idBit(5),
    idBit(4),
    idBit(3),
    idBit(2),
    idBit(1),
    idBit(0),
    // row 3: G2 C3 C2 C1 C0 G3
    false,
    counterBit(3),
    counterBit(2),
    counterBit(1),
    counterBit(0),
    true,
  ];
}

/** The painted blocks for one frame: {@link testPatternRects} zipped with {@link testPatternBits}. */
export function testPatternBlocks(
  senderId: number,
  capturedFrameIndex: number,
  width: number,
  height: number,
): readonly PatternBlock[] {
  const bits = testPatternBits(senderId, capturedFrameIndex);
  return testPatternRects(width, height).map((rect, i) => ({ ...rect, white: bits[i] === true }));
}

/** The 2D-context surface the painter uses. A recording fake satisfies it in tests. */
export interface PatternPaintTarget {
  fillStyle: string | CanvasGradient | CanvasPattern;
  fillRect(x: number, y: number, width: number, height: number): void;
}

/** Pure black and pure white, the only two colours the spec allows. */
const WHITE = '#ffffff';
const BLACK = '#000000';

/**
 * Paint one frame's blocks into `target`, flat fills only, exactly the
 * {@link testPatternBlocks} list (the painter computes no geometry of its own).
 */
export function paintTestPattern(
  target: PatternPaintTarget,
  senderId: number,
  capturedFrameIndex: number,
  width: number,
  height: number,
): void {
  for (const block of testPatternBlocks(senderId, capturedFrameIndex, width, height)) {
    target.fillStyle = block.white ? WHITE : BLACK;
    target.fillRect(block.x, block.y, block.width, block.height);
  }
}

/**
 * One participant's pattern source: the sender id plus the CAPTURED-frame
 * counter. The counter CONTINUES across a capture stop/restart (one instance per
 * session, owned by the publisher's capture selection), so a resume does not
 * replay old counter values. Never exposes the counter (R-30: no frame index on
 * any bus or log).
 */
export class TestPatternSource {
  readonly #senderId: number;
  #frameIndex = 0;

  constructor(senderId: number) {
    assertMeetingSenderId(senderId, 'test pattern');
    this.#senderId = senderId;
  }

  /** The pattern id (the sender id) — for the test bus's `videoCaptureSource.patternId`. */
  get patternId(): number {
    return this.#senderId;
  }

  /** Paint the next CAPTURED frame and advance the counter. */
  paintNext(target: PatternPaintTarget, width: number, height: number): void {
    paintTestPattern(target, this.#senderId, this.#frameIndex, width, height);
    this.#frameIndex += 1;
  }
}
