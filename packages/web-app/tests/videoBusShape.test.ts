// Node-tier tests of the story-3 bus contract additions in `e2e/videoBusShape.ts`
// (C-C1): `sendCounts`, `receiveLayers.drops`, and the bus slot-state vocabulary.
// Exact keys, closed vocabularies, one positive and the negative cases each.

import { describe, expect, test } from 'vitest';
import { ALL_REJECT_REASONS, MEDIA_KEYFRAME_SOURCES } from '@darktower/sdk-core';
import { WIRE_TOKENS } from '../src/lib/slotState.js';
import {
  BUS_SLOT_STATES,
  BusShapeError,
  KEYFRAME_CAUSES,
  parseReceiveDrops,
  parseSendCounts,
  parseVideoSlots,
} from '../e2e/videoBusShape.js';

const SEND = {
  type: 'sendCounts',
  video: { framesSent: 12, keyframes: { directive_start: 1, request: 2, abandonment: 0 } },
};

describe('parseSendCounts', () => {
  test('accepts the agreed shape', () => {
    expect(parseSendCounts(SEND)).toEqual({ video: SEND.video });
  });
  test('the cause set is EXACTLY the SDK vocabulary (no periodic cause exists)', () => {
    expect([...KEYFRAME_CAUSES].sort()).toEqual(Object.values(MEDIA_KEYFRAME_SOURCES).sort());
    expect(KEYFRAME_CAUSES).not.toContain('interval');
  });
  test.each([
    [
      'an unknown cause',
      { ...SEND, video: { ...SEND.video, keyframes: { ...SEND.video.keyframes, interval: 1 } } },
    ],
    [
      'a missing cause',
      { ...SEND, video: { ...SEND.video, keyframes: { request: 1, abandonment: 0 } } },
    ],
    ['a stray top-level field (a timestamp)', { ...SEND, atMs: 1 }],
    ['a stray video field (a GoP value)', { ...SEND, video: { ...SEND.video, gopCounter: 3 } }],
    ['a stream number key', { type: 'sendCounts', stream1: SEND.video }],
    ['a negative count', { ...SEND, video: { ...SEND.video, framesSent: -1 } }],
  ])('rejects %s', (_label, raw) => {
    expect(() => parseSendCounts(raw)).toThrow(BusShapeError);
  });
});

describe('parseReceiveDrops', () => {
  const ROW = { slot: 3, senderId: '258', reason: ALL_REJECT_REASONS[0], count: 2 };
  test('accepts rows, including an unattributed (null sender) drop', () => {
    expect(parseReceiveDrops([ROW, { ...ROW, senderId: null, slot: 'unparsed' }])).toHaveLength(2);
  });
  test.each([
    ['a free-text reason', [{ ...ROW, reason: 'something broke' }]],
    ['a stray field (a size)', [{ ...ROW, bytes: 100 }]],
    ['a non-stringified sender', [{ ...ROW, senderId: 258 }]],
    ['a missing count', [{ slot: 3, senderId: '258', reason: ALL_REJECT_REASONS[0] }]],
  ])('rejects %s', (_label, raw) => {
    expect(() => parseReceiveDrops(raw)).toThrow(BusShapeError);
  });
});

describe('bus slot-state vocabulary', () => {
  test('is every SDK wire token (derived from the exhaustive copy table)', () => {
    expect([...BUS_SLOT_STATES].sort()).toEqual([...WIRE_TOKENS].sort());
  });
  test('a never-emitted token parses as a token (the judges report it as a mismatch)', () => {
    const parsed = parseVideoSlots({
      type: 'videoSlots',
      slots: [
        {
          slotId: 3,
          mcState: 'withheld_by_congestion',
          rendered: 'empty',
          framesDecoded: 0,
          framesRendered: 0,
        },
      ],
    });
    expect(parsed.slots[0]?.mcState).toBe('withheld_by_congestion');
  });
});
