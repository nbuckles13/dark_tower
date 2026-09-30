// Node-tier SELF-TEST of the layer-3 tone detector (`e2e/toneDetector.ts`,
// story 2 R-30) against SYNTHETIC PCM of known frequencies.
//
// The detector reads a lane record's dB magnitude spectrum. To test it from PCM
// this file carries its own REFERENCE ANALYSER: the WebAudio AnalyserNode's
// definition (Blackman window with alpha 0.16, X[k] = (1/N) sum x[n] w[n]
// e^(-2 pi i k n / N), 20 log10 |X[k]|) evaluated per bin with Goertzel, clipped
// to a band exactly as `src/lib/e2eAnalysis.ts` clips it, into the SAME record
// shape the bus carries. The reference is anchored by an absolute-level check,
// so a wrong reference cannot silently agree with a wrong detector.
//
// Nothing here — PCM, reference, or detector — shares code with the SDK's tone
// derivation (`media/setup/testTone.ts`) or synthesis (`capture.ts`); the last
// block enforces that with a positive control. Every input is deterministic
// (seeded noise); nothing is timed.

import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import {
  evaluateSenderLane,
  laneCarriesTone,
  ToneCollisionError,
  ToneHarnessError,
  type BusLaneAnalysis,
  type BusSlotAssignment,
  type CohortTone,
} from '../e2e/toneDetector.js';

// ---------------------------------------------------------------------------
// Synthetic PCM + reference analyser (test-only)
// ---------------------------------------------------------------------------

const FFT_SIZE = 8192;
/** This test's own analysis band — deliberately not imported from src/lib. */
const BAND_START_HZ = 500;
const BAND_END_HZ = 1300;

/** Deterministic noise source (mulberry32). */
function seededRandom(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface SineComponent {
  readonly hz: number;
  readonly amplitude: number;
}

/** N samples of a sum of sines plus uniform noise of peak `noise` (seeded). */
function synthesise(
  sampleRate: number,
  partials: readonly SineComponent[],
  noise = 0,
  seed = 1,
): Float64Array {
  const rand = seededRandom(seed);
  const out = new Float64Array(FFT_SIZE);
  for (let n = 0; n < FFT_SIZE; n += 1) {
    let v = 0;
    for (const p of partials)
      v += p.amplitude * Math.sin((2 * Math.PI * p.hz * n) / sampleRate + 0.3);
    out[n] = v + (noise > 0 ? (rand() * 2 - 1) * noise : 0);
  }
  return out;
}

/** Blackman-windowed |X[k]| / N via Goertzel, in dB — the AnalyserNode definition. */
function referenceBinDb(windowed: Float64Array, k: number): number {
  const w = (2 * Math.PI * k) / FFT_SIZE;
  const c = 2 * Math.cos(w);
  let s1 = 0;
  let s2 = 0;
  for (let n = 0; n < FFT_SIZE; n += 1) {
    const s = (windowed[n] ?? 0) + c * s1 - s2;
    s2 = s1;
    s1 = s;
  }
  const power = s1 * s1 + s2 * s2 - c * s1 * s2;
  const magnitude = Math.sqrt(Math.max(power, 0)) / FFT_SIZE;
  return magnitude > 0 ? 20 * Math.log10(magnitude) : -Infinity;
}

function blackman(pcm: Float64Array): Float64Array {
  const alpha = 0.16;
  const a0 = (1 - alpha) / 2;
  const a1 = 0.5;
  const a2 = alpha / 2;
  return pcm.map(
    (x, n) =>
      x *
      (a0 -
        a1 * Math.cos((2 * Math.PI * n) / FFT_SIZE) +
        a2 * Math.cos((4 * Math.PI * n) / FFT_SIZE)),
  );
}

/** A lane record in the bus's shape, analysed from `pcm`. */
function laneFrom(
  senderId: string,
  sampleRate: number,
  pcm: Float64Array,
  overrides: Partial<BusLaneAnalysis> = {},
): BusLaneAnalysis {
  const binHz = sampleRate / FFT_SIZE;
  const first = Math.floor(BAND_START_HZ / binHz);
  const last = Math.ceil(BAND_END_HZ / binHz);
  const windowed = blackman(pcm);
  const spectrumDb: number[] = [];
  for (let k = first; k <= last; k += 1) {
    spectrumDb.push(Math.round(referenceBinDb(windowed, k) * 10) / 10);
  }
  let sumSquares = 0;
  for (const v of pcm) sumSquares += v * v;
  const rms = Math.sqrt(sumSquares / pcm.length);
  return {
    senderId,
    levelDbfs: rms > 0 ? 20 * Math.log10(rms) : -Infinity,
    ready: true,
    sampleRateHz: sampleRate,
    fftSize: FFT_SIZE,
    smoothingTimeConstant: 0,
    binHz,
    bandStartHz: first * binHz,
    bandEndHz: last * binHz,
    spectrumDb,
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// A synthetic four-member cohort (this test's own frequencies; A-B-C are the
// tightest spacing the detector must resolve, 25 Hz)
// ---------------------------------------------------------------------------

const A: CohortTone = { label: 'A', senderId: '11', toneHz: 700 };
const B: CohortTone = { label: 'B', senderId: '12', toneHz: 725 };
const C: CohortTone = { label: 'C', senderId: '13', toneHz: 750 };
const D: CohortTone = { label: 'D', senderId: '14', toneHz: 1012.5 };
const COHORT = [A, B, C, D] as const;

const assignedTo = (...members: CohortTone[]): BusSlotAssignment[] =>
  members.map((m, i) => ({ slotId: i + 1, senderId: m.senderId, slotState: 'active' }));

const TONE_AMPLITUDE = 0.5;
/** Codec-noise stand-in, about 60 dB under the tone. */
const NOISE = 0.0005;

function verdictFor(
  receiver: CohortTone,
  sender: CohortTone,
  lane: BusLaneAnalysis | undefined,
  assignments = assignedTo(...COHORT.filter((m) => m !== receiver)),
) {
  return evaluateSenderLane({
    receiver,
    sender,
    cohort: COHORT,
    assignments,
    lanes: lane === undefined ? [] : [lane],
  });
}

// ---------------------------------------------------------------------------

describe('reference analyser anchor', () => {
  test('a unit sine at a bin centre reads 20log10(0.42/2) dB there and nothing far away', () => {
    const sampleRate = 48_000;
    const binHz = sampleRate / FFT_SIZE;
    const k = 128;
    const windowed = blackman(synthesise(sampleRate, [{ hz: k * binHz, amplitude: 1 }]));
    expect(referenceBinDb(windowed, k)).toBeCloseTo(20 * Math.log10(0.21), 1);
    expect(referenceBinDb(windowed, k + 10)).toBeLessThan(-100);
  });
});

describe.each([44_100, 48_000])('detector at %i Hz', (sampleRate) => {
  test.each(COHORT.map((s) => [s.label, s] as const))(
    'every receiver hears %s two-sidedly (tone dominant, no other cohort tone)',
    (_label, sender) => {
      const lane = laneFrom(
        sender.senderId,
        sampleRate,
        synthesise(sampleRate, [{ hz: sender.toneHz, amplitude: TONE_AMPLITUDE }], NOISE, 7),
      );
      for (const receiver of COHORT.filter((m) => m !== sender)) {
        const v = verdictFor(receiver, sender, lane);
        expect(v.kind, v.detail).toBe('ok');
      }
    },
  );

  test('a tone half a bin off-centre is still detected (scalloping)', () => {
    const binHz = sampleRate / FFT_SIZE;
    const hz = (Math.round(D.toneHz / binHz) + 0.5) * binHz;
    const offCentre: CohortTone = { ...D, toneHz: hz };
    const lane = laneFrom(
      D.senderId,
      sampleRate,
      synthesise(sampleRate, [{ hz, amplitude: TONE_AMPLITUDE }], NOISE),
    );
    const v = evaluateSenderLane({
      receiver: A,
      sender: offCentre,
      cohort: [A, B, C, offCentre],
      assignments: assignedTo(B, C, offCentre),
      lanes: [lane],
    });
    expect(v.kind, v.detail).toBe('ok');
  });

  test("the adjacent tone (25 Hz away) is NOT B's: B's lane carrying A's tone is expected_absent", () => {
    const lane = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(sampleRate, [{ hz: A.toneHz, amplitude: TONE_AMPLITUDE }], NOISE),
    );
    const v = verdictFor(C, B, lane);
    expect(v.kind).toBe('expected_absent');
  });

  test("the receiver's OWN tone mixed into the sender's lane at equal level is foreign_present", () => {
    const lane = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(
        sampleRate,
        [
          { hz: B.toneHz, amplitude: TONE_AMPLITUDE },
          { hz: A.toneHz, amplitude: TONE_AMPLITUDE },
        ],
        NOISE,
      ),
    );
    const v = verdictFor(A, B, lane);
    expect(v.kind).toBe('foreign_present');
    expect(v.detail).toMatch(/foreign tone\(s\) A /);
  });

  test("a third participant's tone mixed in 10 dB down is foreign_present", () => {
    const lane = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(
        sampleRate,
        [
          { hz: B.toneHz, amplitude: TONE_AMPLITUDE },
          { hz: D.toneHz, amplitude: TONE_AMPLITUDE * 10 ** (-10 / 20) },
        ],
        NOISE,
      ),
    );
    expect(verdictFor(A, B, lane).kind).toBe('foreign_present');
  });

  test('a leak 30 dB down (far below any mis-feed) does not fail the sender', () => {
    const lane = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(
        sampleRate,
        [
          { hz: B.toneHz, amplitude: TONE_AMPLITUDE },
          { hz: C.toneHz, amplitude: TONE_AMPLITUDE * 10 ** (-30 / 20) },
        ],
        NOISE,
      ),
    );
    const v = verdictFor(A, B, lane);
    expect(v.kind, v.detail).toBe('ok');
  });

  test('a louder NON-cohort tone makes the expected tone present but not dominant', () => {
    const lane = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(
        sampleRate,
        [
          { hz: B.toneHz, amplitude: TONE_AMPLITUDE / 2 },
          { hz: 900, amplitude: TONE_AMPLITUDE },
        ],
        NOISE,
      ),
    );
    const v = verdictFor(A, B, lane);
    expect(v.kind).toBe('expected_absent');
    expect(v.detail).toMatch(/not dominant/);
  });

  test('noise without a tone is expected_absent, not ok', () => {
    const lane = laneFrom(B.senderId, sampleRate, synthesise(sampleRate, [], 0.05, 3));
    expect(verdictFor(A, B, lane).kind).toBe('expected_absent');
  });

  test('digital silence is silent, reported apart from a wrong tone', () => {
    const lane = laneFrom(B.senderId, sampleRate, synthesise(sampleRate, []));
    expect(verdictFor(A, B, lane).kind).toBe('silent');
  });
});

describe('lane and assignment states', () => {
  const sampleRate = 48_000;
  const bLane = laneFrom(
    B.senderId,
    sampleRate,
    synthesise(sampleRate, [{ hz: B.toneHz, amplitude: TONE_AMPLITUDE }], NOISE),
  );

  test('a lane that has not analysed a full window is not_ready, even with the tone in it', () => {
    expect(verdictFor(A, B, { ...bLane, ready: false }).kind).toBe('not_ready');
  });

  test('an assigned sender with no lane is lane_missing', () => {
    expect(verdictFor(A, B, undefined).kind).toBe('lane_missing');
  });

  test('a sender with no slot, or a non-active slot, is not_assigned even if a stale lane is ok', () => {
    expect(verdictFor(A, B, bLane, assignedTo(C, D)).kind).toBe('not_assigned');
    const muted: BusSlotAssignment[] = [
      { slotId: 1, senderId: B.senderId, slotState: 'source_muted' },
    ];
    const v = verdictFor(A, B, bLane, muted);
    expect(v.kind).toBe('not_assigned');
    expect(v.detail).toContain("'source_muted'");
  });

  test('a sender placed in two slots is duplicate_assignment, even with a clean lane', () => {
    const dup: BusSlotAssignment[] = [
      { slotId: 1, senderId: B.senderId, slotState: 'active' },
      { slotId: 2, senderId: B.senderId, slotState: 'active' },
    ];
    const v = verdictFor(A, B, bLane, dup);
    expect(v.kind).toBe('duplicate_assignment');
    expect(v.detail).toContain('1:active, 2:active');
  });

  test('a record that went through JSON (-Infinity -> null) is still read', () => {
    const quiet = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(sampleRate, [{ hz: B.toneHz, amplitude: TONE_AMPLITUDE }]),
    );
    const withGaps: BusLaneAnalysis = {
      ...quiet,
      spectrumDb: quiet.spectrumDb.map((v, i) => (i < 5 ? -Infinity : v)),
    };
    const roundTripped = JSON.parse(JSON.stringify(withGaps)) as BusLaneAnalysis;
    expect(roundTripped.spectrumDb[0]).toBeNull();
    expect(verdictFor(A, B, roundTripped).kind).toBe('ok');
  });
});

describe('laneCarriesTone — the one presence rule (absence assertions use it alone)', () => {
  const sampleRate = 48_000;
  const bLane = laneFrom(
    B.senderId,
    sampleRate,
    synthesise(sampleRate, [{ hz: B.toneHz, amplitude: TONE_AMPLITUDE }], NOISE),
  );

  test('a lane with the tone carries it; the same lane does not carry another cohort tone', () => {
    expect(laneCarriesTone(bLane, B.toneHz)).toBe(true);
    expect(laneCarriesTone(bLane, D.toneHz)).toBe(false);
  });

  test('silence, noise alone and a not-ready lane carry no tone', () => {
    const silent = laneFrom(B.senderId, sampleRate, new Float64Array(FFT_SIZE));
    expect(laneCarriesTone(silent, B.toneHz)).toBe(false);
    const noise = laneFrom(B.senderId, sampleRate, synthesise(sampleRate, [], 0.05, 7));
    expect(laneCarriesTone(noise, B.toneHz)).toBe(false);
    expect(laneCarriesTone({ ...bLane, ready: false }, B.toneHz)).toBe(false);
  });

  test('it is LOOSER than the hearing verdict: a tone under a louder foreign one is still present', () => {
    const mixed = laneFrom(
      B.senderId,
      sampleRate,
      synthesise(
        sampleRate,
        [
          { hz: B.toneHz, amplitude: TONE_AMPLITUDE * 0.1 },
          { hz: D.toneHz, amplitude: TONE_AMPLITUDE },
        ],
        NOISE,
      ),
    );
    expect(laneCarriesTone(mixed, B.toneHz)).toBe(true);
    expect(verdictFor(A, B, mixed).kind).not.toBe('ok');
  });
});

describe('harness errors and cohort preconditions (thrown, never a verdict)', () => {
  const sampleRate = 48_000;
  const bLane = laneFrom(
    B.senderId,
    sampleRate,
    synthesise(sampleRate, [{ hz: B.toneHz, amplitude: TONE_AMPLITUDE }], NOISE),
  );

  test('a cohort tone outside the published band is a harness error, not an absent tone', () => {
    const E: CohortTone = { label: 'E', senderId: '15', toneHz: 1500 };
    expect(() =>
      evaluateSenderLane({
        receiver: A,
        sender: B,
        cohort: [A, B, E],
        assignments: assignedTo(B, E),
        lanes: [bLane],
      }),
    ).toThrow(ToneHarnessError);
  });

  test('two members closer than main lobe + tolerance is a collision precondition', () => {
    const tooClose: CohortTone = { label: 'E', senderId: '15', toneHz: B.toneHz + 10 };
    expect(() =>
      evaluateSenderLane({
        receiver: A,
        sender: B,
        cohort: [A, B, tooClose],
        assignments: assignedTo(B, tooClose),
        lanes: [bLane],
      }),
    ).toThrow(ToneCollisionError);
  });

  test('asking a receiver to hear itself, or an outsider, is a harness error', () => {
    expect(() => verdictFor(A, A, bLane)).toThrow(/cannot be asked to hear itself/);
    const outsider: CohortTone = { label: 'Z', senderId: '99', toneHz: 900 };
    expect(() => verdictFor(A, outsider, bLane)).toThrow(/not in the cohort/);
  });
});

// ---------------------------------------------------------------------------
// No shared code with the send side
// ---------------------------------------------------------------------------

/** Import specifiers in `source` that would couple it to the SDK's tone send side. */
function sendSideCoupling(source: string): string[] {
  const findings: string[] = [];
  const importRe = /^\s*import\s+(type\s+)?[^;]*?from\s+['"]([^'"]+)['"]/gm;
  for (const m of source.matchAll(importRe)) {
    const typeOnly = m[1] !== undefined;
    const spec = m[2] ?? '';
    if (/@darktower\/sdk-core|sdk-core\/|testTone|\/capture(\.js|\.ts)?$/.test(spec)) {
      findings.push(`imports ${spec}`);
    } else if (spec.startsWith('../') && !typeOnly) {
      findings.push(`runtime import from outside e2e/: ${spec}`);
    }
  }
  if (/TEST_TONE_|testToneFrequencyHz/.test(source)) findings.push('names the SDK tone constants');
  return findings;
}

/** Tone NAMES and PATHS of the SDK send side — forbidden anywhere in e2e/. */
function namesSendSide(source: string): string[] {
  const findings: string[] = [];
  const importRe = /^\s*import\s+[^;]*?from\s+['"]([^'"]+)['"]/gm;
  for (const m of source.matchAll(importRe)) {
    if (/testTone|\/capture(\.js|\.ts)?$/.test(m[1] ?? '')) findings.push(`imports ${m[1]}`);
  }
  if (/TEST_TONE_|testToneFrequencyHz|createTestToneCapture/.test(source)) {
    findings.push('names an SDK tone export');
  }
  return findings;
}

describe('the receive-side harness shares no code with the SDK tone derivation/synthesis', () => {
  const e2eDir = fileURLToPath(new URL('../e2e/', import.meta.url));
  const read = (rel: string) => readFileSync(fileURLToPath(new URL(rel, import.meta.url)), 'utf8');

  test('strict rule for the detector: no sdk-core import, no runtime import outside e2e/', () => {
    expect(sendSideCoupling(read('../e2e/toneDetector.ts'))).toEqual([]);
  });

  test('name/path rule over EVERY e2e/*.ts (the scan is not vacuous)', () => {
    const files = readdirSync(e2eDir).filter((f) => f.endsWith('.ts'));
    expect(files).toContain('toneDetector.ts');
    expect(files).toContain('fixtures.ts');
    for (const f of files) {
      expect(namesSendSide(readFileSync(`${e2eDir}${f}`, 'utf8')), f).toEqual([]);
    }
  });

  test('positive control: the checks flag each coupling shape', () => {
    expect(
      sendSideCoupling("import { testToneFrequencyHz } from '@darktower/sdk-core';"),
    ).not.toEqual([]);
    expect(
      sendSideCoupling("import { x } from '../../sdk-core/src/media/setup/testTone.js';"),
    ).not.toEqual([]);
    expect(sendSideCoupling("import { y } from '../src/lib/e2eAnalysis.js';")).not.toEqual([]);
    expect(sendSideCoupling('const f = TEST_TONE_STEP_HZ * 2;')).not.toEqual([]);
    // A type-only import of the bus record shape is allowed by the strict rule.
    expect(
      sendSideCoupling("import type { LaneAnalysis } from '../src/lib/e2eAnalysis.js';"),
    ).toEqual([]);
    // A tone export smuggled into a legitimate multi-name sdk-core import.
    expect(
      namesSendSide(
        "import {\n  MeetingApiClient,\n  testToneFrequencyHz,\n} from '@darktower/sdk-core';",
      ),
    ).not.toEqual([]);
    expect(
      namesSendSide("import { c } from '../../sdk-core/src/media/setup/capture.js';"),
    ).not.toEqual([]);
    expect(namesSendSide("import { MeetingApiClient } from '@darktower/sdk-core';")).toEqual([]);
  });
});
