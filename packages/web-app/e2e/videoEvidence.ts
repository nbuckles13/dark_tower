// File: packages/web-app/e2e/videoEvidence.ts
//
// Story 3 R-32: PER-RECEIVER, PER-VIDEO-SLOT evidence, as PURE judges over
// parsed bus samples (`./videoBusShape`). The node tier
// (`tests/videoEvidence.test.ts`) drives them with fixture snapshots. The
// Playwright side (bus reads, sampling on the window stop rule) lives beside
// the story-2 helpers in `receiveEvidence.ts`, which calls these judges. That
// file imports `./fixtures`, which needs a live E2E environment, so the judges
// stay here, importable without one.
//
// For each ACTIVE video slot, against the oracle's expected occupant
// (`./slotOracle`) and that occupant's sender_id FROM THE HARNESS'S OWN JOIN
// BINDING (never a value the occupant announced about itself):
//   * layer 1: the key-id sender of the rendered frames (`renderedSenderId`)
//     IS the expected sender. A rendered frame from anyone else is a layer-1
//     failure, and that sample's lumas are NEVER pooled into layer 3;
//   * layer 2: the verified-and-decrypted count for (slot, expected sender) on
//     `receiveLayers` ADVANCES across the window;
//   * layer 3: the pattern detector (`./patternDetector`) reads the expected id,
//     the guard passes and the slot is live. Only samples with rendered ===
//     'video', lumas present and the expected sender count. Leading avatar
//     samples (avatar -> active keeps the avatar until the first decode, R-5)
//     are outside the window, not failures. An `insufficient` window is a
//     HARNESS error and throws.
// Separately for every slot: the MC state token (wire names) and what is
// actually rendered (video / avatar / empty), from the window's LAST sample.
// Send side: camera-track-live === (directed AND NOT client-muted AND camera
// healthy); the mic stays open on client and server audio mute; the announced
// patternId equals the member's join-bound sender_id.
//
// Failure text names the slot, the wire states and booleans. A sender id
// appears only in thrown assertion text: the meeting-scoped sender_id in a
// local report (story-2 precedent). Nothing here logs or attaches anything.
// No wall clock: windows are counted by sample order.

import { judgeSlotPattern } from './patternDetector.js';
import type { ExpectedSlotState, SlotExpectation } from './slotOracle.js';
import type {
  RenderedKind,
  SendTracksEvent,
  VideoCaptureSourceEvent,
  VideoSlotSample,
  VideoSlotsEvent,
} from './videoBusShape.js';

/**
 * One (observed slot, sender) row of layers 1 and 2, as `receiveLayers` projects
 * it, for audio and video slots alike. THE one home of this shape:
 * `receiveEvidence.ts` re-exports it (it lives here because this module must
 * stay importable without the E2E environment).
 */
export interface LayerRow {
  readonly slot: number | 'undeclared' | 'unparsed';
  readonly senderId: string;
  readonly keyed: number;
  readonly verified: number;
}

/** A harness fault (too few samples, no slot in the window): never a media verdict. */
export class VideoEvidenceHarnessError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'VideoEvidenceHarnessError';
  }
}

/** What a slot in each MC state must render. */
export function renderedFor(state: ExpectedSlotState): RenderedKind {
  if (state === 'active') return 'video';
  if (state === 'fewer_sources' || state === 'unavailable') return 'empty';
  return 'avatar';
}

/** The expectation for one video slot of one receiver. */
export interface VideoSlotCheck {
  readonly slotId: number;
  /** From `SlotOracle.expected(receiver).video[i]`. */
  readonly expectation: SlotExpectation;
  /**
   * The expected occupant's sender_id from the harness's join binding
   * (stringified). Required when the expectation is ACTIVE.
   */
  readonly expectedSenderId?: string;
}

/** One slot's verdict. `failures` is empty on a pass. */
export interface VideoSlotVerdict {
  readonly slotId: number;
  readonly failures: readonly string[];
}

/** The window: `videoSlots` samples and `receiveLayers` rows, index-aligned, in bus order. */
export interface VideoWindow {
  readonly slots: readonly VideoSlotsEvent[];
  readonly layers: readonly (readonly LayerRow[])[];
}

function slotIn(event: VideoSlotsEvent, slotId: number): VideoSlotSample | undefined {
  return event.slots.find((s) => s.slotId === slotId);
}

function verifiedAt(rows: readonly LayerRow[], slotId: number, senderId: string): number {
  return rows
    .filter((r) => r.slot === slotId && r.senderId === senderId)
    .reduce((sum, r) => sum + r.verified, 0);
}

/**
 * Judge one video slot over a window. Throws {@link VideoEvidenceHarnessError}
 * on a malformed window, a slot missing from a sample, or an `insufficient`
 * layer-3 window. Returns every failure (all layers are reported, so one
 * layer's failure never hides another's).
 */
export function judgeVideoSlot(check: VideoSlotCheck, window: VideoWindow): VideoSlotVerdict {
  const { slotId, expectation } = check;
  if (window.slots.length < 2 || window.slots.length !== window.layers.length) {
    throw new VideoEvidenceHarnessError(
      `slot ${slotId}: a window needs >= 2 index-aligned samples ` +
        `(${window.slots.length} videoSlots, ${window.layers.length} receiveLayers)`,
    );
  }
  const samples = window.slots.map((e, i) => {
    const s = slotIn(e, slotId);
    if (s === undefined) {
      throw new VideoEvidenceHarnessError(`slot ${slotId} is missing from sample ${i}`);
    }
    return s;
  });
  const failures: string[] = [];
  const last = samples[samples.length - 1]!;
  if (last.mcState !== expectation.state) {
    failures.push(`state: MC says '${last.mcState}', expected '${expectation.state}'`);
  }
  const wantRendered = renderedFor(expectation.state);
  if (last.rendered !== wantRendered) {
    failures.push(
      `rendered: '${last.rendered}', expected '${wantRendered}' for '${expectation.state}'`,
    );
  }
  if (expectation.state !== 'active') {
    return { slotId, failures };
  }

  const expected = check.expectedSenderId;
  if (expected === undefined) {
    throw new VideoEvidenceHarnessError(
      `slot ${slotId}: an ACTIVE expectation needs the occupant's join-bound sender id`,
    );
  }
  const mine = expectedSenderSamples(samples, expected);
  return {
    slotId,
    failures: [
      ...failures,
      ...judgeLayer1(samples, expected),
      ...judgeLayer2(window.layers, slotId, expected),
      ...judgeLayer3(mine, slotId, expected),
    ],
  };
}

/** The samples that rendered a frame keyed to `expected` — layer 3's only evidence. */
function expectedSenderSamples(
  samples: readonly VideoSlotSample[],
  expected: string,
): VideoSlotSample[] {
  return samples.filter((s) => s.rendered === 'video' && s.renderedSenderId === expected);
}

/**
 * Layer 1: the key-id sender of every RENDERED frame is the expected occupant,
 * at least one such frame was rendered, and once the expected sender's video
 * has begun the slot never falls back to an avatar or empty (leading avatar
 * samples before the first decode are outside the window, R-5).
 */
function judgeLayer1(samples: readonly VideoSlotSample[], expected: string): string[] {
  const failures: string[] = [];
  const shown = samples.filter((s) => s.rendered === 'video' && s.renderedSenderId !== undefined);
  const wrong = [
    ...new Set(shown.filter((s) => s.renderedSenderId !== expected).map((s) => s.renderedSenderId)),
  ];
  if (wrong.length > 0) {
    failures.push(
      `layer 1: rendered frames keyed to sender(s) ${wrong.join(', ')}, expected ${expected} (MISROUTING)`,
    );
  }
  const began = samples.findIndex((s) => s.rendered === 'video' && s.renderedSenderId === expected);
  if (began === -1) {
    failures.push(
      `layer 1: no frame from the expected sender ${expected} was rendered in the window`,
    );
    return failures;
  }
  const fellBack = samples.findIndex((s, i) => i > began && s.rendered !== 'video');
  if (fellBack !== -1) {
    failures.push(
      `layer 1: fell back to '${samples[fellBack]!.rendered}' at sample ${fellBack} after video began`,
    );
  }
  return failures;
}

/** Layer 2: the (slot, sender) verified-and-decrypted count advances over the window. */
function judgeLayer2(
  layers: readonly (readonly LayerRow[])[],
  slotId: number,
  expected: string,
): string[] {
  const first = verifiedAt(layers[0]!, slotId, expected);
  const end = verifiedAt(layers[layers.length - 1]!, slotId, expected);
  return end > first
    ? []
    : [
        `layer 2: verified-and-decrypted count for (slot ${slotId}, sender ${expected}) did not ` +
          `advance (${first} -> ${end})`,
      ];
}

/**
 * Layer 3: over the expected sender's rendered samples with lumas only, the
 * decoded pattern id is the expected sender, the guard passes, and the slot is
 * live. Fewer than 2 decodable samples is a HARNESS error, never a verdict.
 */
function judgeLayer3(mine: readonly VideoSlotSample[], slotId: number, expected: string): string[] {
  if (mine.length === 0) return []; // layer 1 already reports no rendered frame
  const lumaSamples = mine.filter((s) => s.lumas !== undefined).map((s) => s.lumas!);
  const verdict = judgeSlotPattern(lumaSamples, Number(expected));
  if (verdict.liveness === 'insufficient') {
    throw new VideoEvidenceHarnessError(
      `slot ${slotId}: layer 3 had ${verdict.validSamples} decodable pattern sample(s) of ` +
        `${verdict.samples}; liveness over fewer than 2 is vacuous. Widen the window ` +
        `(a harness/load failure, not a media verdict)`,
    );
  }
  const failures: string[] = [];
  if (!verdict.idMatch) {
    failures.push(
      `layer 3: decoded pattern id(s) ${verdict.decodedIds.join(', ') || '(none)'}, expected ${expected}`,
    );
  }
  if (!verdict.guardPass) failures.push('layer 3: the pattern guard failed on some sample');
  if (verdict.liveness !== 'live') failures.push(`layer 3: the slot is ${verdict.liveness}`);
  return failures;
}

/** Throw one error naming every failing slot, or return on a full pass. */
export function assertVideoSlots(who: string, verdicts: readonly VideoSlotVerdict[]): void {
  const bad = verdicts.filter((v) => v.failures.length > 0);
  if (bad.length === 0) return;
  throw new Error(
    `${who}: video slot evidence failed —\n` +
      bad.map((v) => `  slot ${v.slotId}: ${v.failures.join('; ')}`).join('\n'),
  );
}

/** The per-subscriber unreachable set equals the oracle's (as sender ids). */
export function judgeUnreachable(
  expectedSenderIds: readonly string[],
  busUnreachable: readonly string[],
): string[] {
  const want = [...new Set(expectedSenderIds)].sort();
  const got = [...new Set(busUnreachable)].sort();
  return want.join(',') === got.join(',')
    ? []
    : [`unreachable: bus [${got.join(', ')}], expected [${want.join(', ')}]`];
}

/** The send-side inputs the scenario knows. */
export interface SendSideExpectation {
  /** MC directed this member's video (it is watched and not server-muted). */
  readonly directed: boolean;
  readonly clientVideoMuted: boolean;
  readonly cameraHealthy: boolean;
  /** The member publishes audio: its mic stays open through client and server audio mute. */
  readonly publishesAudio: boolean;
}

/**
 * camera-track-live is EXACTLY (directed AND NOT client-muted AND camera
 * healthy). A stuck-true projection fails on the false cases.
 *
 * Server mute withdraws the directive, so here it is the same row as an
 * unwatched publisher (`directed: false`). "Ended" (server-muted) and "off"
 * (unwatched) are INDISTINGUISHABLE on `cameraTrackLive` in the agreed bus
 * shape — both read false; C-C1 asserts "ended" in its own specs.
 */
export function judgeSendTracks(want: SendSideExpectation, got: SendTracksEvent): string[] {
  const failures: string[] = [];
  const live = want.directed && !want.clientVideoMuted && want.cameraHealthy;
  if (got.cameraTrackLive !== live) {
    failures.push(
      `camera track live=${got.cameraTrackLive}, expected ${live} ` +
        `(directed=${want.directed}, clientMuted=${want.clientVideoMuted}, healthy=${want.cameraHealthy})`,
    );
  }
  if (want.publishesAudio && !got.micTrackOpen) {
    failures.push('mic track closed; it stays open on client and server audio mute');
  }
  return failures;
}

/**
 * The member's announced pattern source names its OWN join-bound sender_id. A
 * generator drifting from sender_id fails here, loudly, because layer 3 checks
 * against the join binding and never against this announcement.
 */
export function judgePatternAnnouncement(
  source: VideoCaptureSourceEvent,
  joinBoundSenderId: string,
): string[] {
  if (source.mode !== 'test_pattern') {
    return [`capture mode '${source.mode}', expected 'test_pattern' in the test build`];
  }
  return source.patternId === joinBoundSenderId
    ? []
    : [
        `announced patternId ${source.patternId ?? '(none)'} != join-bound sender ${joinBoundSenderId}`,
      ];
}
