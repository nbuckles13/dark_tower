// Node-tier tests of the test's own R-3 slot oracle (`e2e/slotOracle.ts`,
// story 3 R-32 layer 1).
//
// Every expected occupant below is WRITTEN BY HAND from R-3's text (and the
// 2026-10-21 KIND ORDER amendment), never computed by a second oracle call: an
// expectation derived from the oracle would agree with it by construction. The
// last block enforces the import rule (nothing from crates/mc-service).

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import { KIND_FILL_ORDER, SlotOracle, type OracleJoin } from '../e2e/slotOracle.js';
import { importedSpecifiers } from './_patternSpec.js';

const H = ['mh-0'];
const CAPS = { audio: 8, video: 4 };

const both = (audio: number, video: number, handlers = H): OracleJoin => ({
  publish: 'both',
  slots: { audio, video },
  handlers,
});
const audioOnly = (audio: number, video: number, handlers = H): OracleJoin => ({
  publish: 'audio',
  slots: { audio, video },
  handlers,
});
const receiveOnly = (audio: number, video: number, handlers = H): OracleJoin => ({
  publish: 'none',
  slots: { audio, video },
  handlers,
});

describe('R-3 fill', () => {
  test('the kind order is pinned: audio, then video', () => {
    expect(KIND_FILL_ORDER).toEqual(['audio', 'video']);
  });

  test('pair completion: a both-kinds publisher takes the audio AND the video slot', () => {
    // A receive-only joins first, B publishes both, then S declares N=M=1.
    // Audio first: audio -> B (A publishes no audio); video rule (1): B holds
    // audio but not video -> B, although A joined earlier.
    const o = new SlotOracle(CAPS);
    o.join('A', receiveOnly(1, 1));
    o.join('B', both(1, 1));
    o.join('S', both(1, 1));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'active', occupant: 'B' }],
      video: [{ state: 'active', occupant: 'B' }],
      unreachable: [],
    });
  });

  // Story MC-A slot table, NO-VIDEO PRECEDENCE amendment 2026-10-21: a video slot
  // whose occupant publishes no video is avatar_no_video whatever its video mutes;
  // a video publisher under the same mutes is client/server-muted, server winning.
  test.each([
    ['client video mute', { client: true }],
    ['server video mute', { server: true }],
    ['both video mutes', { client: true, server: true }],
  ] as const)('no-video precedence holds under a %s', (_label, mute) => {
    for (const [who, join] of [
      ['audio-only', audioOnly(1, 1)],
      ['receive-only', receiveOnly(1, 1)],
    ] as const) {
      const o = new SlotOracle(CAPS);
      o.join('A', join);
      o.join('S', both(1, 1));
      o.setMute('A', 'video', mute);
      expect(o.expected('S').video, who).toEqual([{ state: 'avatar_no_video', occupant: 'A' }]);
    }
    const p = new SlotOracle(CAPS);
    p.join('V', both(1, 1));
    p.join('S', both(1, 1));
    p.setMute('V', 'video', mute);
    const want = 'server' in mute ? 'avatar_server_muted' : 'avatar_client_muted';
    expect(p.expected('S').video).toEqual([{ state: want, occupant: 'V' }]);
  });

  test('earliest eligible joiner fills, never the subscriber itself', () => {
    const o = new SlotOracle(CAPS);
    o.join('A', both(2, 2));
    o.join('B', both(2, 2));
    o.join('C', both(2, 2));
    expect(o.expected('A').audio).toEqual([
      { state: 'active', occupant: 'B' },
      { state: 'active', occupant: 'C' },
    ]);
    expect(o.expected('C').video).toEqual([
      { state: 'active', occupant: 'A' },
      { state: 'active', occupant: 'B' },
    ]);
  });

  test('receive-only in a video slot shows as avatar_no_video, never in an audio slot', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('R', receiveOnly(1, 1));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'fewer_sources_than_slots' }],
      video: [{ state: 'avatar_no_video', occupant: 'R' }],
      unreachable: [],
    });
  });

  test('an audio-only participant holds both kinds, the video one as avatar_no_video', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('A', audioOnly(1, 0));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'active', occupant: 'A' }],
      video: [{ state: 'avatar_no_video', occupant: 'A' }],
      unreachable: [],
    });
  });

  test('accepted consequence: a no-camera participant keeps the slot, a later camera goes unseen', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('R', receiveOnly(0, 1));
    o.join('V', both(1, 1));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'active', occupant: 'V' }],
      video: [{ state: 'avatar_no_video', occupant: 'R' }],
      unreachable: [],
    });
  });

  test('stability across a leave: only the leaver slot is refilled, earliest eligible first', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(2, 2));
    o.join('A', both(1, 1));
    o.join('B', both(1, 1));
    o.join('C', both(1, 1));
    expect(o.expected('S').video).toEqual([
      { state: 'active', occupant: 'A' },
      { state: 'active', occupant: 'B' },
    ]);
    o.leave('A');
    // B stays in slot 1. Audio slot 0: no rule-(1) candidate, so rule (2)
    // gives the earliest free joiner, C. Video slot 0: rule (1) now pairs C.
    expect(o.expected('S')).toEqual({
      audio: [
        { state: 'active', occupant: 'C' },
        { state: 'active', occupant: 'B' },
      ],
      video: [
        { state: 'active', occupant: 'C' },
        { state: 'active', occupant: 'B' },
      ],
      unreachable: [],
    });
  });

  test('a join never moves an occupied slot', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('B', both(1, 1));
    o.join('A', both(1, 1)); // later joiner, no free slot
    expect(o.expected('S').audio).toEqual([{ state: 'active', occupant: 'B' }]);
    expect(o.expected('S').video).toEqual([{ state: 'active', occupant: 'B' }]);
  });

  test('mutes change state, never the occupant; server wins over client', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('B', both(1, 1));
    o.setMute('B', 'video', { client: true });
    expect(o.expected('S').video).toEqual([{ state: 'avatar_client_muted', occupant: 'B' }]);
    expect(o.expected('S').audio).toEqual([{ state: 'active', occupant: 'B' }]);
    o.setMute('B', 'video', { server: true });
    expect(o.expected('S').video).toEqual([{ state: 'avatar_server_muted', occupant: 'B' }]);
    o.setMute('B', 'audio', { server: true });
    expect(o.expected('S').audio).toEqual([{ state: 'avatar_server_muted', occupant: 'B' }]);
    o.setMute('B', 'video', { server: false });
    expect(o.expected('S').video).toEqual([{ state: 'avatar_client_muted', occupant: 'B' }]);
  });

  test('truncation: slots over the per-kind cap are unavailable, after the filled ones', () => {
    const o = new SlotOracle({ audio: 8, video: 1 });
    o.join('S', both(1, 3));
    o.join('B', both(1, 1));
    o.join('C', both(1, 1));
    expect(o.expected('S').video).toEqual([
      { state: 'active', occupant: 'B' },
      { state: 'unavailable' },
      { state: 'unavailable' },
    ]);
  });

  test('a video cap of 0 makes every video slot unavailable and leaves audio untouched', () => {
    const o = new SlotOracle({ audio: 8, video: 0 });
    o.join('S', both(1, 2));
    o.join('B', both(1, 1));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'active', occupant: 'B' }],
      video: [{ state: 'unavailable' }, { state: 'unavailable' }],
      unreachable: [],
    });
  });

  test('redeclaration keeps occupants by ordinal; growth fills; the other kind is untouched', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    o.join('B', both(1, 1));
    o.join('C', both(1, 1));
    o.redeclare('S', { video: 2 });
    expect(o.expected('S').video).toEqual([
      { state: 'active', occupant: 'B' },
      { state: 'active', occupant: 'C' },
    ]);
    expect(o.expected('S').audio).toEqual([{ state: 'active', occupant: 'B' }]);
    o.redeclare('S', { video: 1 });
    expect(o.expected('S').video).toEqual([{ state: 'active', occupant: 'B' }]);
  });

  test('two-handler visibility: a pair sharing no handler is unreachable and takes no slot', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(2, 2, ['mh-0']));
    o.join('X', both(1, 1, ['mh-1']));
    o.join('Y', both(1, 1, ['mh-0', 'mh-1']));
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'active', occupant: 'Y' }, { state: 'fewer_sources_than_slots' }],
      video: [{ state: 'active', occupant: 'Y' }, { state: 'fewer_sources_than_slots' }],
      unreachable: ['X'],
    });
    // X connects to mh-0 too: now reachable, fills the free slots; Y never moves.
    o.setHandlers('X', ['mh-0', 'mh-1']);
    expect(o.expected('S')).toEqual({
      audio: [
        { state: 'active', occupant: 'Y' },
        { state: 'active', occupant: 'X' },
      ],
      video: [
        { state: 'active', occupant: 'Y' },
        { state: 'active', occupant: 'X' },
      ],
      unreachable: [],
    });
    // Y drops mh-0: S and Y become unreachable; only Y's slots are freed.
    o.setHandlers('Y', ['mh-1']);
    expect(o.expected('S')).toEqual({
      audio: [{ state: 'fewer_sources_than_slots' }, { state: 'active', occupant: 'X' }],
      video: [{ state: 'fewer_sources_than_slots' }, { state: 'active', occupant: 'X' }],
      unreachable: ['Y'],
    });
  });

  test('bad input throws', () => {
    const o = new SlotOracle(CAPS);
    o.join('S', both(1, 1));
    expect(() => o.join('S', both(1, 1))).toThrow(/already joined/);
    expect(() => o.expected('nobody')).toThrow(/no participant/);
    expect(() => o.redeclare('S', { audio: -1 })).toThrow(RangeError);
    expect(() => new SlotOracle({ audio: 1.5, video: 1 })).toThrow(RangeError);
  });
});

// ---------------------------------------------------------------------------
// Import rule: the oracle shares no code with MC's fill
// ---------------------------------------------------------------------------

function mcCoupling(source: string): string[] {
  return importedSpecifiers(source).filter(
    (spec) =>
      spec === 'dynamic import' || /mc-service|crates\//.test(spec) || spec.includes('sdk-core'),
  );
}

describe('import rule', () => {
  test('slotOracle.ts imports nothing from crates/ (MC) or sdk-core', () => {
    const src = readFileSync(
      fileURLToPath(new URL('../e2e/slotOracle.ts', import.meta.url)),
      'utf8',
    );
    expect(mcCoupling(src)).toEqual([]);
  });
  test('positive control', () => {
    expect(mcCoupling("import { x } from '../../../crates/mc-service/slots.js';")).not.toEqual([]);
    expect(mcCoupling("import type { Y } from '@darktower/sdk-core';")).not.toEqual([]);
    expect(mcCoupling("import '../../../crates/mc-service/x.js';")).not.toEqual([]);
    expect(mcCoupling("const s = require('../../../crates/mc-service/slots');")).not.toEqual([]);
    expect(mcCoupling("import type { PublishSet } from './cohort.js';")).toEqual([]);
  });
});
