// File: packages/web-app/e2e/videoBusShape.ts
//
// Story 3: the AGREED test-bus shape for video (paired-client, T-A planning).
// The client implements the projection in its own tasks (C-B, C-C1), under
// `__E2E_HOOKS__`. This module is the contract the suite's helpers consume,
// with STRICT parsers. Pure (no Playwright), so the node tier self-tests it.
//
// EXACT KEYS (@observability): every parser below rejects an unknown field.
// So a GoP value, a frame size, a key id or a timestamp cannot slip onto the bus
// unnoticed. Adding a field is a deliberate edit here plus a review, never a
// silent widening.
//
// IDENTITY (R-30 vs R-32, recorded in T-A main.md): `renderedSenderId`,
// `receiveLayers.senderId` and `patternId` carry the MEETING-SCOPED sender_id,
// stringified, on the TEST-BUILD bus only. That is a deliberate R-32 carve-out
// following the story-2 `receiveLayers` precedent. There is never a stream
// number, relay id, GoP, size, key id, timing or durable identity.
//
// Not imported from `src/`: the new events do not exist in the app yet, so
// the contract lives here and the app's projection is typed against the README.

import { PENDING_SLOT_STATES, SHARED_SLOT_STATES, type ExpectedSlotState } from './slotOracle.js';
import type { LayerRow } from './videoEvidence.js';

/**
 * The slot-state wire tokens the bus carries. ONE home: derived from the
 * oracle's vocabulary (`slotOracle.ts`, where the two tokens the SDK already
 * carries are checked against its union and the five pending ones wait for
 * C-A1), so the oracle's expectation and the bus value are the same type.
 */
export const BUS_SLOT_STATES = [
  ...SHARED_SLOT_STATES,
  ...PENDING_SLOT_STATES,
] as const satisfies readonly ExpectedSlotState[];
export type BusSlotState = ExpectedSlotState;

/** What a video slot is actually showing. */
export type RenderedKind = 'video' | 'avatar' | 'empty';

/** One video slot, as the sampled `videoSlots` event projects it. */
export interface VideoSlotSample {
  readonly slotId: number;
  readonly mcState: BusSlotState;
  readonly rendered: RenderedKind;
  /** The verified key-id sender of the last RENDERED frame; absent until one renders. */
  readonly renderedSenderId?: string;
  readonly framesDecoded: number;
  readonly framesRendered: number;
  /** 24 block-mean lumas at the render surface; present only while rendering video. */
  readonly lumas?: readonly number[];
}

/** `{ type: 'videoSlots', slots }`, with no timestamp (order is the bus replay order). */
export interface VideoSlotsEvent {
  readonly slots: readonly VideoSlotSample[];
}

/** `{ type: 'sendTracks', cameraTrackLive, micTrackOpen }`. */
export interface SendTracksEvent {
  readonly cameraTrackLive: boolean;
  readonly micTrackOpen: boolean;
}

/** `{ type: 'videoCaptureSource', mode, patternId? }`, emitted once at first capture. */
export interface VideoCaptureSourceEvent {
  readonly mode: 'camera' | 'test_pattern';
  readonly patternId?: string;
}

/** One `slotAssignments` entry with the story-3 `kind`. */
export interface KindedSlotAssignment {
  readonly slotId: number;
  readonly kind: 'audio' | 'video';
  readonly slotState: string;
  readonly senderId?: string;
}

/** A bus shape violation: a HARNESS/contract error, never a media verdict. */
export class BusShapeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'BusShapeError';
  }
}

type Rec = Readonly<Record<string, unknown>>;

function record(v: unknown, where: string): Rec {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) {
    throw new BusShapeError(`${where}: expected an object`);
  }
  return v as Rec;
}

/** Throw on any key outside `required` + `optional`, or a missing required key. */
function exactKeys(
  r: Rec,
  where: string,
  required: readonly string[],
  optional: readonly string[] = [],
): void {
  for (const k of Object.keys(r)) {
    if (!required.includes(k) && !optional.includes(k)) {
      throw new BusShapeError(`${where}: unexpected field '${k}' (the bus contract is exact-key)`);
    }
  }
  for (const k of required) {
    if (!(k in r)) throw new BusShapeError(`${where}: missing field '${k}'`);
  }
}

const isCount = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0;
const isSenderId = (v: unknown): v is string => typeof v === 'string' && /^[1-9][0-9]*$/.test(v);

function count(r: Rec, k: string, where: string): number {
  const v = r[k];
  if (!isCount(v)) throw new BusShapeError(`${where}: '${k}' must be a non-negative integer`);
  return v;
}

function optionalSenderId(r: Rec, k: string, where: string): string | undefined {
  if (!(k in r)) return undefined;
  const v = r[k];
  if (!isSenderId(v)) throw new BusShapeError(`${where}: '${k}' must be a stringified sender id`);
  return v;
}

function bool(r: Rec, k: string, where: string): boolean {
  const v = r[k];
  if (typeof v !== 'boolean') throw new BusShapeError(`${where}: '${k}' must be a boolean`);
  return v;
}

function parseSlot(raw: unknown, where: string): VideoSlotSample {
  const r = record(raw, where);
  exactKeys(
    r,
    where,
    ['slotId', 'mcState', 'rendered', 'framesDecoded', 'framesRendered'],
    ['renderedSenderId', 'lumas'],
  );
  const mcState = r['mcState'];
  if (typeof mcState !== 'string' || !(BUS_SLOT_STATES as readonly string[]).includes(mcState)) {
    throw new BusShapeError(`${where}: 'mcState' is not a known wire token`);
  }
  const rendered = r['rendered'];
  if (rendered !== 'video' && rendered !== 'avatar' && rendered !== 'empty') {
    throw new BusShapeError(`${where}: 'rendered' must be video | avatar | empty`);
  }
  const lumas = r['lumas'];
  if (lumas !== undefined) {
    // Shape only; the detector validates the values (PatternInputError).
    if (!Array.isArray(lumas)) throw new BusShapeError(`${where}: 'lumas' must be an array`);
    if (rendered !== 'video') {
      throw new BusShapeError(`${where}: 'lumas' present on a slot not rendering video`);
    }
  }
  const renderedSenderId = optionalSenderId(r, 'renderedSenderId', where);
  return {
    slotId: count(r, 'slotId', where),
    mcState: mcState as BusSlotState,
    rendered,
    ...(renderedSenderId !== undefined ? { renderedSenderId } : {}),
    framesDecoded: count(r, 'framesDecoded', where),
    framesRendered: count(r, 'framesRendered', where),
    ...(lumas !== undefined ? { lumas: lumas as number[] } : {}),
  };
}

/** Parse one `videoSlots` bus event (with its `type`). */
export function parseVideoSlots(raw: unknown): VideoSlotsEvent {
  const r = record(raw, 'videoSlots');
  exactKeys(r, 'videoSlots', ['type', 'slots']);
  if (r['type'] !== 'videoSlots') throw new BusShapeError(`videoSlots: wrong type`);
  const slots = r['slots'];
  if (!Array.isArray(slots)) throw new BusShapeError(`videoSlots: 'slots' must be an array`);
  const parsed = slots.map((s, i) => parseSlot(s, `videoSlots.slots[${i}]`));
  const ids = new Set(parsed.map((s) => s.slotId));
  if (ids.size !== parsed.length) throw new BusShapeError('videoSlots: duplicate slotId');
  return { slots: parsed };
}

/** Parse one `sendTracks` bus event. */
export function parseSendTracks(raw: unknown): SendTracksEvent {
  const r = record(raw, 'sendTracks');
  exactKeys(r, 'sendTracks', ['type', 'cameraTrackLive', 'micTrackOpen']);
  if (r['type'] !== 'sendTracks') throw new BusShapeError(`sendTracks: wrong type`);
  return {
    cameraTrackLive: bool(r, 'cameraTrackLive', 'sendTracks'),
    micTrackOpen: bool(r, 'micTrackOpen', 'sendTracks'),
  };
}

/** Parse one `videoCaptureSource` bus event. */
export function parseVideoCaptureSource(raw: unknown): VideoCaptureSourceEvent {
  const r = record(raw, 'videoCaptureSource');
  exactKeys(r, 'videoCaptureSource', ['type', 'mode'], ['patternId']);
  if (r['type'] !== 'videoCaptureSource') {
    throw new BusShapeError(`videoCaptureSource: wrong type`);
  }
  const mode = r['mode'];
  if (mode !== 'camera' && mode !== 'test_pattern') {
    throw new BusShapeError(`videoCaptureSource: 'mode' must be camera | test_pattern`);
  }
  const patternId = optionalSenderId(r, 'patternId', 'videoCaptureSource');
  if (mode === 'test_pattern' && patternId === undefined) {
    throw new BusShapeError(`videoCaptureSource: test_pattern mode must announce its patternId`);
  }
  if (mode === 'camera' && patternId !== undefined) {
    throw new BusShapeError(`videoCaptureSource: camera mode carries no patternId`);
  }
  return { mode, ...(patternId !== undefined ? { patternId } : {}) };
}

/**
 * Parse the `assignments` of a story-3 `slotAssignments` event (each entry
 * gains `kind`). The other fields are as story 2 emits them.
 */
export function parseKindedAssignments(raw: unknown): KindedSlotAssignment[] {
  if (!Array.isArray(raw)) throw new BusShapeError(`slotAssignments.assignments must be an array`);
  return raw.map((a, i) => {
    const where = `slotAssignments.assignments[${i}]`;
    const r = record(a, where);
    exactKeys(r, where, ['slotId', 'kind', 'slotState'], ['senderId']);
    const kind = r['kind'];
    if (kind !== 'audio' && kind !== 'video') {
      throw new BusShapeError(`${where}: 'kind' must be audio | video`);
    }
    const slotState = r['slotState'];
    if (typeof slotState !== 'string') throw new BusShapeError(`${where}: 'slotState' missing`);
    const senderId = optionalSenderId(r, 'senderId', where);
    return {
      slotId: count(r, 'slotId', where),
      kind,
      slotState,
      ...(senderId !== undefined ? { senderId } : {}),
    };
  });
}

/**
 * `receiveLayers.layers` — the story-2 layer-1/2 rows story 3's layer 2 now
 * consumes, held to the same exact-key rule as the new events: exactly
 * `{ slot, senderId, keyed, verified }` (what `src/lib/e2eBus.ts` projects
 * today), so a GoP, size, key-id or timing field added in C-B/C-C1 fails here
 * instead of reaching the suite unnoticed. `slot` is a slot id or one of the
 * two non-slot tokens the SDK recorder uses.
 */
export function parseReceiveLayers(raw: unknown): LayerRow[] {
  if (!Array.isArray(raw)) throw new BusShapeError('receiveLayers.layers: expected an array');
  return raw.map((entry: unknown, i) => {
    const where = `receiveLayers.layers[${i}]`;
    const r = record(entry, where);
    exactKeys(r, where, ['slot', 'senderId', 'keyed', 'verified']);
    const slot = r['slot'];
    if (!isCount(slot) && slot !== 'undeclared' && slot !== 'unparsed') {
      throw new BusShapeError(`${where}: 'slot' must be a slot id or 'undeclared' | 'unparsed'`);
    }
    const senderId = optionalSenderId(r, 'senderId', where);
    if (senderId === undefined) throw new BusShapeError(`${where}: missing field 'senderId'`);
    return {
      slot,
      senderId,
      keyed: count(r, 'keyed', where),
      verified: count(r, 'verified', where),
    };
  });
}
