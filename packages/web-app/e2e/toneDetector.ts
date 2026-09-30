// File: packages/web-app/e2e/toneDetector.ts
//
// Story 2 R-30 layer 3: the RECEIVE-SIDE TONE DETECTOR and the two-sided
// per-receiver-per-sender verdict. Pure — no `./env`, no Playwright — so the
// node unit tier (`tests/toneDetector.test.ts`) self-tests it against spectra
// computed from synthetic PCM.
//
// ---------------------------------------------------------------------------
// WHAT IT RUNS OVER
// ---------------------------------------------------------------------------
//
// The bus's `receiveAnalysis` lane record (`src/lib/e2eAnalysis.ts`): a
// band-clipped dB MAGNITUDE spectrum from a native AnalyserNode (Blackman
// window, smoothing 0) over the lane's decoded, pre-mix audio. There is no PCM
// on the bus by design (@security), so the FFT has already run in the browser,
// over the signal; this module reads the spectrum it produced. Every frequency
// mapping reads `bandStartHz` / `binHz` FROM THE RECORD — never an assumed
// sample rate, FFT size or band constant.
//
// Lanes are keyed by the VERIFIED sender, not by slot: "B's slot" is resolved
// through the latest `slotAssignments` (`slotId -> senderId`) and then B's lane
// is read. The per-slot misroute check (a count under the wrong (slot, sender)
// key) is layers 1/2 on `receiveLayers`, not this module.
//
// ---------------------------------------------------------------------------
// SHARES NO CODE WITH THE SEND SIDE
// ---------------------------------------------------------------------------
//
// Nothing here imports `@darktower/sdk-core` (whose `media/setup/testTone.ts`
// and `capture.ts` derive and synthesise the tones) or restates its tone
// formula: a shared bug would cancel out. The EXPECTED tone of each participant
// is what that participant's own client ANNOUNCED on the bus
// (`captureSource.toneHz`). `tests/toneDetector.test.ts` enforces the import
// rule with a positive control.
//
// ---------------------------------------------------------------------------
// THE RULES
// ---------------------------------------------------------------------------
//
// - PRESENT: the expected tone's level (max over ±{@link TONE_TOLERANCE_BINS} of
//   its nearest bin) is at least {@link PRESENCE_ABOVE_FLOOR_DB} above the band
//   median (the noise floor — a tone occupies a handful of the ~110 bins)
//   AND within {@link FOREIGN_BELOW_EXPECTED_DB} of the band's strongest bin,
//   so an adjacent tone's Blackman leakage never reads as the expected tone.
// - DOMINANT: the band's global peak lies within that tolerance too (+ half a
//   bin of nearest-bin rounding) — checked after FOREIGN, so a comparable
//   cohort intruder is diagnosed as such rather than by who won the peak.
// - FOREIGN: any OTHER cohort tone — the receiver's own included — whose level
//   is within {@link FOREIGN_BELOW_EXPECTED_DB} of the expected tone's. Relative
//   to the CARRIER, not the floor: codec frame-rate sidebands sit far below the
//   carrier yet far above a clean floor, while the bugs this catches (wrong
//   lane fed into this one, own audio looped back, two senders mixed) put a
//   foreign tone at a comparable level.
// - SEPARABLE COHORT (a precondition, not a verdict): tones must be at least
//   {@link BLACKMAN_MAIN_LOBE_HALF_WIDTH_BINS} + {@link TONE_TOLERANCE_BINS}
//   bins apart, so no tone's tolerance window reaches into another's Blackman
//   main lobe. Two participants sharing a tone is a cohort collision, reported
//   as such — never as misrouting.

import type { LaneAnalysis } from '../src/lib/e2eAnalysis.js';

/** Half-width of the Blackman window's main lobe, in bins (first null at ±3). */
export const BLACKMAN_MAIN_LOBE_HALF_WIDTH_BINS = 3;
/** How far from a tone's nearest bin its peak may land (bin-centre scalloping). */
export const TONE_TOLERANCE_BINS = 1;
/** Expected tone above the band median for PRESENT. */
export const PRESENCE_ABOVE_FLOOR_DB = 20;
/** A foreign tone within this many dB of the expected tone's level is PRESENT. */
export const FOREIGN_BELOW_EXPECTED_DB = 20;
/**
 * At or below this RMS level (dBFS) the lane is SILENT — "nothing decoded into
 * it" — reported apart from "the wrong tone arrived".
 */
export const SILENCE_LEVEL_DBFS = -70;

/**
 * One lane record AS IT ARRIVES OFF THE BUS: the producer's shape with the
 * sender id stringified (the bus convention), and dB values that may be
 * `-Infinity` — or `null`, if the record ever went through `JSON.stringify`.
 */
export type BusLaneAnalysis = Omit<LaneAnalysis, 'senderId' | 'levelDbfs' | 'spectrumDb'> & {
  readonly senderId: string;
  readonly levelDbfs: number | null;
  readonly spectrumDb: readonly (number | null)[];
};

/** One entry of the bus's latest `slotAssignments` view. */
export interface BusSlotAssignment {
  readonly slotId: number;
  readonly senderId?: string;
  readonly slotState: string;
}

/** A cohort participant as the harness knows it: its own announced identity + tone. */
export interface CohortTone {
  /** Human label for diagnostics ("A", "B", ...). */
  readonly label: string;
  /** Stringified sender id, from the participant's own `joined` event. */
  readonly senderId: string;
  /** The participant's own `captureSource.toneHz`. */
  readonly toneHz: number;
}

/** The spectral part of a lane record the detector reads. */
export type Spectrum = Pick<BusLaneAnalysis, 'binHz' | 'bandStartHz' | 'bandEndHz' | 'spectrumDb'>;

const db = (v: number | null | undefined): number =>
  v === null || v === undefined || Number.isNaN(v) ? -Infinity : v;

/**
 * A harness defect — the spectrum cannot answer the question (tone outside the
 * published band, malformed record). Thrown, never folded into a verdict: an
 * unanswerable question must not read as "tone absent".
 */
export class ToneHarnessError extends Error {
  override readonly name = 'ToneHarnessError';
}

/**
 * Cohort-precondition failure: two participants' tones cannot be told apart.
 * Distinct from every receive verdict so a collision never reads as misrouting.
 */
export class ToneCollisionError extends Error {
  override readonly name = 'ToneCollisionError';
}

function assertSpectrumShape(spectrum: Spectrum): void {
  if (!(spectrum.binHz > 0) || !Number.isFinite(spectrum.bandStartHz)) {
    throw new ToneHarnessError(
      `malformed spectrum record (binHz=${spectrum.binHz}, bandStartHz=${spectrum.bandStartHz})`,
    );
  }
  if (spectrum.spectrumDb.length === 0) {
    throw new ToneHarnessError('spectrum record carries no bins');
  }
}

/**
 * Level of `hz` in `spectrum`: the max over ±{@link TONE_TOLERANCE_BINS} of its
 * nearest bin. THROWS {@link ToneHarnessError} if that window does not lie
 * wholly inside the published spectrum.
 */
export function toneLevelDb(spectrum: Spectrum, hz: number): number {
  assertSpectrumShape(spectrum);
  const centre = Math.round((hz - spectrum.bandStartHz) / spectrum.binHz);
  const lo = centre - TONE_TOLERANCE_BINS;
  const hi = centre + TONE_TOLERANCE_BINS;
  if (lo < 0 || hi > spectrum.spectrumDb.length - 1) {
    throw new ToneHarnessError(
      `tone ${hz} Hz (±${TONE_TOLERANCE_BINS} bin) lies outside the published spectrum ` +
        `${spectrum.bandStartHz}-${spectrum.bandEndHz} Hz — a harness/band defect, not an absent tone`,
    );
  }
  let level = -Infinity;
  for (let i = lo; i <= hi; i += 1) level = Math.max(level, db(spectrum.spectrumDb[i]));
  return level;
}

/** Band median (dB): the noise floor a tone is measured against. */
export function noiseFloorDb(spectrum: Spectrum): number {
  assertSpectrumShape(spectrum);
  const sorted = spectrum.spectrumDb.map(db).sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? -Infinity;
}

/**
 * The band's highest bin — its frequency (the DOMINANT frequency) and level.
 * The one peak scan; ties keep the lowest bin, `null`/`-Infinity` read as
 * -Infinity.
 */
export function peakBin(spectrum: Spectrum): { readonly hz: number; readonly db: number } {
  assertSpectrumShape(spectrum);
  let best = 0;
  for (let i = 1; i < spectrum.spectrumDb.length; i += 1) {
    if (db(spectrum.spectrumDb[i]) > db(spectrum.spectrumDb[best])) best = i;
  }
  return { hz: spectrum.bandStartHz + best * spectrum.binHz, db: db(spectrum.spectrumDb[best]) };
}

/**
 * Cohort precondition: every pair of tones is at least main-lobe + tolerance
 * bins apart at `binHz`, so each is resolvable. Throws {@link ToneCollisionError}.
 */
export function assertCohortTonesSeparable(cohort: readonly CohortTone[], binHz: number): void {
  const minHz = (BLACKMAN_MAIN_LOBE_HALF_WIDTH_BINS + TONE_TOLERANCE_BINS) * binHz;
  for (let i = 0; i < cohort.length; i += 1) {
    for (let j = i + 1; j < cohort.length; j += 1) {
      const a = cohort[i];
      const b = cohort[j];
      if (a === undefined || b === undefined) continue;
      if (Math.abs(a.toneHz - b.toneHz) < minHz) {
        throw new ToneCollisionError(
          `cohort tone collision: ${a.label} (${a.toneHz} Hz) and ${b.label} (${b.toneHz} Hz) are ` +
            `closer than ${minHz.toFixed(1)} Hz at ${binHz.toFixed(2)} Hz/bin — a PRECONDITION ` +
            `failure (the participants cannot be told apart by tone), not misrouting. MC allocates ` +
            `sender ids sequentially per meeting, so a fresh small cohort cannot collide by chance: ` +
            `suspect sender-id churn in this meeting (reconnects/rejoins pushing ids a full tone ` +
            `cycle apart) or a defect in tone derivation/announcement. Investigate; do not re-run.`,
        );
      }
    }
  }
}

/**
 * THE tone-presence rule for one lane — the single clause both directions share:
 * the lane has analysed a full FFT window, is above silence, and carries `hz`
 * at least {@link PRESENCE_ABOVE_FLOOR_DB} above its noise floor.
 *
 * `evaluateSenderLane` composes it with its comparability and dominance clauses
 * (the strict "hears the sender" verdict); an ABSENCE assertion uses it alone,
 * which is the fail-closed direction for "must be absent" — any real component
 * of the tone counts as present. A lane that is not yet ready has decoded no
 * full window, so it carries no tone.
 */
export function laneCarriesTone(
  lane: Spectrum & Pick<BusLaneAnalysis, 'ready' | 'levelDbfs'>,
  hz: number,
): boolean {
  if (!lane.ready || db(lane.levelDbfs) <= SILENCE_LEVEL_DBFS) return false;
  return toneLevelDb(lane, hz) - noiseFloorDb(lane) >= PRESENCE_ABOVE_FLOOR_DB;
}

/** Why a receiver does or does not hear a sender at layer 3. */
export type ToneVerdictKind =
  /** Expected tone dominant, no other cohort tone present. */
  | 'ok'
  /** The latest `slotAssignments` gives the sender no ACTIVE slot at this receiver. */
  | 'not_assigned'
  /** The latest `slotAssignments` places the sender in MORE THAN ONE slot — an assignment bug. */
  | 'duplicate_assignment'
  /** Assigned, but no analysis lane exists for the sender (nothing decoded yet). */
  | 'lane_missing'
  /** The lane has not yet analysed a full FFT window of decoded audio. */
  | 'not_ready'
  /** The lane's level is at or below {@link SILENCE_LEVEL_DBFS}. */
  | 'silent'
  /** Audio is there, but the sender's tone is not present-and-dominant. */
  | 'expected_absent'
  /** The sender's tone is there, but another cohort tone is too. */
  | 'foreign_present';

export interface ToneVerdict {
  readonly kind: ToneVerdictKind;
  /** One-line human explanation, including the measured levels. */
  readonly detail: string;
}

/**
 * The two-sided layer-3 verdict for `receiver` hearing `sender`:
 * `sender` holds an ACTIVE slot in the receiver's latest assignment, the
 * sender's lane carries f(sender) as its dominant tone, AND no other cohort
 * member's tone (the receiver's own included) is present in that lane.
 *
 * Pure and non-throwing for every receive outcome; throws
 * {@link ToneCollisionError} for an inseparable cohort and
 * {@link ToneHarnessError} for a tone outside the published band.
 */
export function evaluateSenderLane(args: {
  readonly receiver: CohortTone;
  readonly sender: CohortTone;
  readonly cohort: readonly CohortTone[];
  readonly assignments: readonly BusSlotAssignment[];
  readonly lanes: readonly BusLaneAnalysis[];
}): ToneVerdict {
  const { receiver, sender, cohort, assignments, lanes } = args;
  if (receiver.senderId === sender.senderId) {
    throw new ToneHarnessError(
      `${receiver.label} cannot be asked to hear itself (loopback removed)`,
    );
  }
  for (const member of [receiver, sender]) {
    if (!cohort.some((c) => c.senderId === member.senderId)) {
      throw new ToneHarnessError(
        `${member.label} (sender ${member.senderId}) is not in the cohort`,
      );
    }
  }
  const who = `${receiver.label} hearing ${sender.label} (sender ${sender.senderId}, ${sender.toneHz} Hz)`;

  const slots = assignments.filter((a) => a.senderId === sender.senderId);
  if (slots.length > 1) {
    // Lanes are per sender, so layer 3 would read ok; the duplicate placement
    // is itself the defect.
    return {
      kind: 'duplicate_assignment',
      detail:
        `${who}: the latest slotAssignments places the sender in ${slots.length} slots ` +
        `(${slots.map((a) => `${a.slotId}:${a.slotState}`).join(', ')})`,
    };
  }
  const slot = slots[0];
  if (slot === undefined || slot.slotState !== 'active') {
    return {
      kind: 'not_assigned',
      detail:
        `${who}: the latest slotAssignments ` +
        (slot === undefined
          ? 'gives the sender no slot'
          : `puts the sender on slot ${slot.slotId} in state '${slot.slotState}', not 'active'`),
    };
  }

  const lane = lanes.find((l) => l.senderId === sender.senderId);
  if (lane === undefined) {
    return {
      kind: 'lane_missing',
      detail: `${who}: slot ${slot.slotId} is active but no analysis lane exists for the sender`,
    };
  }
  if (!lane.ready) {
    return { kind: 'not_ready', detail: `${who}: lane not yet ready (no full FFT window decoded)` };
  }
  const level = db(lane.levelDbfs);
  if (level <= SILENCE_LEVEL_DBFS) {
    return { kind: 'silent', detail: `${who}: lane is silent (level ${level} dBFS)` };
  }

  assertCohortTonesSeparable(cohort, lane.binHz);
  const floor = noiseFloorDb(lane);
  const expected = toneLevelDb(lane, sender.toneHz);
  // Every OTHER cohort tone, the receiver's own included, measured before any
  // verdict: an out-of-band foreign tone is a harness error too.
  const foreign = cohort
    .filter((c) => c.senderId !== sender.senderId)
    .map((c) => ({ member: c, level: toneLevelDb(lane, c.toneHz) }));
  const peak = peakBin(lane);
  const peakHz = peak.hz;
  const levels =
    `expected ${expected.toFixed(1)} dB, floor ${floor.toFixed(1)} dB, dominant ${peakHz.toFixed(1)} Hz; ` +
    foreign
      .map((f) => `${f.member.label}@${f.member.toneHz}Hz ${f.level.toFixed(1)} dB`)
      .join(', ');

  // PRESENT means a real component, not another tone's window leakage: above
  // the floor AND comparable to the band's strongest bin.
  if (!laneCarriesTone(lane, sender.toneHz) || !(expected >= peak.db - FOREIGN_BELOW_EXPECTED_DB)) {
    return { kind: 'expected_absent', detail: `${who}: expected tone not present (${levels})` };
  }
  // Foreign BEFORE dominance: when a mis-fed cohort tone sits at a comparable
  // level, which of the two wins the peak is scalloping luck — the diagnosis is
  // the intruder, not "absent".
  const intruders = foreign.filter((f) => f.level > expected - FOREIGN_BELOW_EXPECTED_DB);
  if (intruders.length > 0) {
    return {
      kind: 'foreign_present',
      detail:
        `${who}: foreign tone(s) ${intruders.map((f) => f.member.label).join(', ')} within ` +
        `${FOREIGN_BELOW_EXPECTED_DB} dB of the expected tone (${levels})`,
    };
  }
  // Present, no cohort intruder — and it must also be the DOMINANT frequency:
  // a louder non-cohort signal in the lane is not "hearing the sender".
  if (Math.abs(peakHz - sender.toneHz) > (TONE_TOLERANCE_BINS + 0.5) * lane.binHz) {
    return {
      kind: 'expected_absent',
      detail: `${who}: expected tone present but not dominant (${levels})`,
    };
  }
  return { kind: 'ok', detail: `${who}: ok (${levels})` };
}
