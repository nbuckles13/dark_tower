// Node-tier tests of the story-3 video evidence judges (`e2e/videoEvidence.ts`)
// and the exact-key bus parsers (`e2e/videoBusShape.ts`), over FIXTURE bus
// snapshots. Each layer is shown to fail INDEPENDENTLY (wrong sender, count not
// advancing, wrong pattern id), plus state/rendered mismatches, a camera
// positive control and the patternId-vs-join-binding check. Lumas are built from
// the README pattern spec, parsed by `./_patternSpec`. Nothing is timed.

import { describe, expect, test } from 'vitest';

import {
  BusShapeError,
  parseKindedAssignments,
  parseReceiveLayers,
  parseSendTracks,
  parseVideoCaptureSource,
  parseVideoSlots,
  type VideoSlotSample,
  type VideoSlotsEvent,
} from '../e2e/videoBusShape.js';
import {
  assertVideoSlots,
  judgePatternAnnouncement,
  judgeSendTracks,
  judgeUnreachable,
  judgeVideoSlot,
  VideoEvidenceHarnessError,
  type LayerRow,
  type VideoWindow,
} from '../e2e/videoEvidence.js';
import { specLumas } from './_patternSpec.js';

/** Lumas from the README spec (parsed by `./_patternSpec`, never transcribed). */
function lumas(senderId: number, frame: number): number[] {
  return specLumas(senderId, frame, 250, 5);
}

const SLOT = 7;
const B = '42'; // the expected occupant's join-bound sender id
const C = '43';

const sample = (over: Partial<VideoSlotSample> = {}): VideoSlotsEvent => ({
  slots: [
    {
      slotId: SLOT,
      mcState: 'active',
      rendered: 'video',
      renderedSenderId: B,
      framesDecoded: 10,
      framesRendered: 10,
      ...over,
    },
  ],
});

/** A healthy ACTIVE window: B rendered, verified advancing, live pattern. */
function goodWindow(n = 4): VideoWindow {
  return {
    slots: Array.from({ length: n }, (_, i) => sample({ lumas: lumas(42, i * 3) })),
    layers: Array.from({ length: n }, (_, i): LayerRow[] => [
      { slot: SLOT, senderId: B, keyed: 10 + i * 5, verified: 10 + i * 5 },
    ]),
  };
}

const active = { state: 'active' as const, occupant: 'B' };

describe('judgeVideoSlot: the three layers', () => {
  test('a healthy window passes every layer', () => {
    expect(
      judgeVideoSlot({ slotId: SLOT, expectation: active, expectedSenderId: B }, goodWindow()),
    ).toEqual({ slotId: SLOT, failures: [] });
  });

  test('layer 1 alone: a frame keyed to the wrong sender fails, and its lumas are not pooled', () => {
    const w = goodWindow();
    // C's frame (carrying C's pattern) in one sample: layers 2 and 3 still hold for B.
    const slots = [...w.slots];
    slots[1] = sample({ renderedSenderId: C, lumas: lumas(43, 1) });
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, slots },
    );
    expect(v.failures).toHaveLength(1);
    expect(v.failures[0]).toMatch(/^layer 1: .*43.*MISROUTING/);
  });

  test('layer 2 alone: a verified count that does not advance fails', () => {
    const w = goodWindow();
    const layers = w.layers.map((): LayerRow[] => [
      { slot: SLOT, senderId: B, keyed: 99, verified: 10 },
    ]);
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, layers },
    );
    expect(v.failures).toEqual([
      `layer 2: verified-and-decrypted count for (slot ${SLOT}, sender ${B}) did not advance (10 -> 10)`,
    ]);
  });

  test('layer 2 counts only (this slot, the expected sender): another slot advancing does not satisfy it', () => {
    const w = goodWindow();
    const layers = w.layers.map((_, i): LayerRow[] => [
      { slot: SLOT, senderId: B, keyed: 1, verified: 1 },
      { slot: SLOT + 1, senderId: B, keyed: i, verified: i },
      { slot: SLOT, senderId: C, keyed: i, verified: i },
    ]);
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, layers },
    );
    expect(v.failures.map((f) => f.slice(0, 8))).toEqual(['layer 2:']);
  });

  test('layer 3 alone: the right key id but the wrong decoded pattern fails', () => {
    const w = goodWindow();
    const slots = w.slots.map((_, i) => sample({ lumas: lumas(0x99, i * 3) }));
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, slots },
    );
    expect(v.failures).toEqual([`layer 3: decoded pattern id(s) ${0x99}, expected ${B}`]);
  });

  test('layer 3: a frozen picture fails liveness', () => {
    const w = goodWindow();
    const slots = w.slots.map(() => sample({ lumas: lumas(42, 5) }));
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, slots },
    );
    expect(v.failures).toEqual(['layer 3: the slot is frozen']);
  });

  test('layer 3: leading avatar samples (avatar -> active) are outside the window, not failures', () => {
    const w = goodWindow(6);
    const slots = [...w.slots];
    slots[0] = {
      slots: [
        {
          slotId: SLOT,
          mcState: 'active',
          rendered: 'avatar',
          framesDecoded: 0,
          framesRendered: 0,
        },
      ],
    };
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, slots },
    );
    expect(v.failures).toEqual([]);
  });

  test('layer 1: falling back to an avatar AFTER video began is a failure', () => {
    const w = goodWindow(6);
    const slots = [...w.slots];
    slots[4] = {
      slots: [
        {
          slotId: SLOT,
          mcState: 'active',
          rendered: 'avatar',
          framesDecoded: 10,
          framesRendered: 10,
        },
      ],
    };
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: active, expectedSenderId: B },
      { ...w, slots },
    );
    expect(v.failures).toEqual(["layer 1: fell back to 'avatar' at sample 4 after video began"]);
  });

  test('layer 3: fewer than 2 decodable pattern samples is a HARNESS error, never a verdict', () => {
    const w = goodWindow(3);
    const slots = w.slots.map((s, i) => (i === 0 ? s : sample({ lumas: new Array(24).fill(128) })));
    expect(() =>
      judgeVideoSlot({ slotId: SLOT, expectation: active, expectedSenderId: B }, { ...w, slots }),
    ).toThrow(VideoEvidenceHarnessError);
  });

  test('an ACTIVE expectation without the join-bound sender id is a harness error', () => {
    expect(() => judgeVideoSlot({ slotId: SLOT, expectation: active }, goodWindow())).toThrow(
      VideoEvidenceHarnessError,
    );
  });

  test('a malformed window is a harness error (too short, misaligned, slot missing)', () => {
    const check = { slotId: SLOT, expectation: active, expectedSenderId: B };
    const w = goodWindow();
    expect(() =>
      judgeVideoSlot(check, { slots: w.slots.slice(0, 1), layers: w.layers.slice(0, 1) }),
    ).toThrow(VideoEvidenceHarnessError);
    expect(() => judgeVideoSlot(check, { slots: w.slots, layers: w.layers.slice(1) })).toThrow(
      VideoEvidenceHarnessError,
    );
    expect(() => judgeVideoSlot({ ...check, slotId: 99 }, w)).toThrow(/missing from sample 0/);
  });
});

describe('judgeVideoSlot: MC state and what is rendered', () => {
  const avatarWindow = (
    mcState: VideoSlotSample['mcState'],
    rendered: VideoSlotSample['rendered'],
  ): VideoWindow => {
    const s: VideoSlotsEvent = {
      slots: [{ slotId: SLOT, mcState, rendered, framesDecoded: 0, framesRendered: 0 }],
    };
    return { slots: [s, s], layers: [[], []] };
  };

  test.each([
    ['avatar_no_video', 'avatar'],
    ['avatar_client_muted', 'avatar'],
    ['avatar_server_muted', 'avatar'],
    ['fewer_sources_than_slots', 'empty'],
    ['unavailable', 'empty'],
  ] as const)('%s renders %s and passes', (state, rendered) => {
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: { state } },
      avatarWindow(state, rendered),
    );
    expect(v.failures).toEqual([]);
  });

  test('a state mismatch fails (MC says active, oracle expects server-muted)', () => {
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: { state: 'avatar_server_muted', occupant: 'B' } },
      goodWindow(),
    );
    expect(v.failures).toEqual([
      "state: MC says 'active', expected 'avatar_server_muted'",
      "rendered: 'video', expected 'avatar' for 'avatar_server_muted'",
    ]);
  });

  test('a rendered mismatch fails even when the state is right (avatar state, video painted)', () => {
    const v = judgeVideoSlot(
      { slotId: SLOT, expectation: { state: 'avatar_client_muted', occupant: 'B' } },
      avatarWindow('avatar_client_muted', 'video'),
    );
    expect(v.failures).toEqual(["rendered: 'video', expected 'avatar' for 'avatar_client_muted'"]);
  });

  test('assertVideoSlots throws naming each failing slot, and passes when clean', () => {
    expect(() => assertVideoSlots('A', [{ slotId: 1, failures: [] }])).not.toThrow();
    expect(() =>
      assertVideoSlots('A', [
        { slotId: 1, failures: [] },
        { slotId: 2, failures: ['layer 2: x'] },
      ]),
    ).toThrow(/slot 2: layer 2: x/);
  });
});

describe('send side', () => {
  const tracks = (cameraTrackLive: boolean, micTrackOpen = true) => ({
    cameraTrackLive,
    micTrackOpen,
  });
  const want = {
    directed: true,
    clientVideoMuted: false,
    cameraHealthy: true,
    publishesAudio: true,
  };

  test('live exactly when directed AND not client-muted AND healthy', () => {
    expect(judgeSendTracks(want, tracks(true))).toEqual([]);
  });
  test.each([
    ['unwatched (not directed)', { ...want, directed: false }],
    ['client-muted', { ...want, clientVideoMuted: true }],
    ['camera unhealthy', { ...want, cameraHealthy: false }],
  ])('positive control: %s must read off; a stuck-true projection FAILS', (_l, w) => {
    expect(judgeSendTracks(w, tracks(true))).toHaveLength(1);
    expect(judgeSendTracks(w, tracks(false))).toEqual([]);
  });
  test('a camera that is off while it should be live fails', () => {
    expect(judgeSendTracks(want, tracks(false))[0]).toMatch(
      /camera track live=false, expected true/,
    );
  });
  test('the mic stays open for an audio publisher; a closed mic fails', () => {
    expect(judgeSendTracks(want, tracks(true, false))).toEqual([
      'mic track closed; it stays open on client and server audio mute',
    ]);
    expect(judgeSendTracks({ ...want, publishesAudio: false }, tracks(true, false))).toEqual([]);
  });
  test('patternId must equal the join-bound sender id', () => {
    expect(judgePatternAnnouncement({ mode: 'test_pattern', patternId: B }, B)).toEqual([]);
    expect(judgePatternAnnouncement({ mode: 'test_pattern', patternId: C }, B)).toEqual([
      `announced patternId ${C} != join-bound sender ${B}`,
    ]);
    expect(judgePatternAnnouncement({ mode: 'camera' }, B)).toHaveLength(1);
  });
  test('unreachable must equal the oracle set', () => {
    expect(judgeUnreachable([B, C], [C, B])).toEqual([]);
    expect(judgeUnreachable([B], [])).toHaveLength(1);
  });
});

describe('bus shape: exact-key parsers', () => {
  const slot = {
    slotId: 1,
    mcState: 'active',
    rendered: 'video',
    framesDecoded: 1,
    framesRendered: 1,
  };

  test('accepts the agreed shapes', () => {
    expect(
      parseVideoSlots({
        type: 'videoSlots',
        slots: [{ ...slot, renderedSenderId: '9', lumas: [] }],
      }).slots,
    ).toHaveLength(1);
    expect(
      parseSendTracks({ type: 'sendTracks', cameraTrackLive: true, micTrackOpen: false }),
    ).toEqual({
      cameraTrackLive: true,
      micTrackOpen: false,
    });
    expect(
      parseVideoCaptureSource({ type: 'videoCaptureSource', mode: 'test_pattern', patternId: '9' }),
    ).toEqual({ mode: 'test_pattern', patternId: '9' });
    expect(parseKindedAssignments([{ slotId: 1, kind: 'video', slotState: 'active' }])).toEqual([
      { slotId: 1, kind: 'video', slotState: 'active' },
    ]);
  });

  test.each([
    ['a timestamp on videoSlots', { type: 'videoSlots', atMs: 1, slots: [] }],
    ['a gop counter on a slot', { type: 'videoSlots', slots: [{ ...slot, gopCounter: 3 }] }],
    ['a frame size on a slot', { type: 'videoSlots', slots: [{ ...slot, frameBytes: 900 }] }],
    ['a key id on a slot', { type: 'videoSlots', slots: [{ ...slot, keyId: 'ab' }] }],
  ])('REJECTS %s (no field slips onto the bus unnoticed)', (_l, raw) => {
    expect(() => parseVideoSlots(raw)).toThrow(/unexpected field/);
  });

  test('rejects extra fields on every event', () => {
    expect(() =>
      parseSendTracks({ type: 'sendTracks', cameraTrackLive: true, micTrackOpen: true, atMs: 1 }),
    ).toThrow(BusShapeError);
    expect(() =>
      parseVideoCaptureSource({ type: 'videoCaptureSource', mode: 'camera', userId: 'u' }),
    ).toThrow(BusShapeError);
    expect(() =>
      parseKindedAssignments([{ slotId: 1, kind: 'audio', slotState: 'active', streamNumber: 0 }]),
    ).toThrow(BusShapeError);
  });

  test('rejects malformed values', () => {
    expect(() =>
      parseVideoSlots({ type: 'videoSlots', slots: [{ ...slot, mcState: 'live' }] }),
    ).toThrow(BusShapeError);
    expect(() =>
      parseVideoSlots({ type: 'videoSlots', slots: [{ ...slot, rendered: 'x' }] }),
    ).toThrow(BusShapeError);
    expect(() =>
      parseVideoSlots({ type: 'videoSlots', slots: [{ ...slot, framesDecoded: -1 }] }),
    ).toThrow(BusShapeError);
    expect(() =>
      parseVideoSlots({ type: 'videoSlots', slots: [{ ...slot, rendered: 'avatar', lumas: [] }] }),
    ).toThrow(/not rendering video/);
    expect(() =>
      parseVideoSlots({ type: 'videoSlots', slots: [{ ...slot, renderedSenderId: 'alice@x' }] }),
    ).toThrow(BusShapeError);
    expect(() => parseVideoSlots({ type: 'videoSlots', slots: [slot, slot] })).toThrow(/duplicate/);
    expect(() =>
      parseVideoCaptureSource({ type: 'videoCaptureSource', mode: 'test_pattern' }),
    ).toThrow(/patternId/);
    expect(() =>
      parseSendTracks({ type: 'sendTracks', cameraTrackLive: 1, micTrackOpen: true }),
    ).toThrow(BusShapeError);
  });
});

describe('bus shape: receiveLayers rows (story-2 rows, exact-key since story 3)', () => {
  const row = { slot: 3, senderId: '7', keyed: 10, verified: 9 };

  test('accepts the four-key rows the app projects today, slot tokens included', () => {
    expect(
      parseReceiveLayers([row, { ...row, slot: 'undeclared' }, { ...row, slot: 'unparsed' }]),
    ).toEqual([row, { ...row, slot: 'undeclared' }, { ...row, slot: 'unparsed' }]);
    expect(parseReceiveLayers([])).toEqual([]);
  });

  test.each([['gopCounter'], ['frameBytes'], ['keyId'], ['atMs']])(
    'rejects an extra field (%s) rather than letting it reach the suite',
    (extra) => {
      expect(() => parseReceiveLayers([{ ...row, [extra]: 1 }])).toThrow(BusShapeError);
      expect(() => parseReceiveLayers([{ ...row, [extra]: 1 }])).toThrow(/unexpected field/);
    },
  );

  test('rejects a bad slot token, a non-stringified sender, a non-count and a non-array', () => {
    expect(() => parseReceiveLayers([{ ...row, slot: 'other' }])).toThrow(BusShapeError);
    expect(() => parseReceiveLayers([{ ...row, slot: -1 }])).toThrow(BusShapeError);
    expect(() => parseReceiveLayers([{ ...row, senderId: 7 }])).toThrow(BusShapeError);
    expect(() => parseReceiveLayers([{ ...row, verified: 1.5 }])).toThrow(BusShapeError);
    const { keyed: _omit, ...missing } = row;
    expect(() => parseReceiveLayers([missing])).toThrow(/missing field 'keyed'/);
    expect(() => parseReceiveLayers({ layers: [] })).toThrow(BusShapeError);
  });
});
