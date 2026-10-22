// Story 3 R-4 / wire freeze: the ONE wire -> token map. Driven over EVERY value
// of the generated enum (so a new wire value cannot escape the table) plus an
// unknown number, asserting the security property: nothing but ACTIVE is active.

import { describe, expect, it } from 'vitest';

import { MediaKind, SlotState } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { mediaKindToken, slotStateToken, wireMediaKind } from '../slotStateMap.js';

/** Every numeric member of the generated enum (TS numeric enums carry a reverse map). */
const ALL_SLOT_STATES = Object.values(SlotState).filter(
  (v): v is SlotState => typeof v === 'number',
);

const EXPECTED: Record<number, string> = {
  [SlotState.UNSPECIFIED]: 'unspecified',
  [SlotState.ACTIVE]: 'active',
  [SlotState.WITHHELD_BY_CONGESTION]: 'withheld_by_congestion',
  [SlotState.FEWER_SOURCES_THAN_SLOTS]: 'fewer_sources_than_slots',
  [SlotState.SOURCE_UNREACHABLE]: 'source_unreachable',
  [SlotState.SWITCH_PENDING]: 'switch_pending',
  [SlotState.AVATAR_NO_VIDEO]: 'avatar_no_video',
  [SlotState.UNAVAILABLE]: 'unavailable',
  [SlotState.AVATAR_CLIENT_MUTED]: 'avatar_client_muted',
  [SlotState.AVATAR_SERVER_MUTED]: 'avatar_server_muted',
};

describe('slotStateToken', () => {
  // Story 3 C-A2: the proto still carries ONE superseded member until MC-B
  // (task 15) deletes it, and no SDK code may name it. So this pins "every
  // generated member but exactly one is mapped", and that one fails closed.
  const UNMAPPED = ALL_SLOT_STATES.filter((st) => !(st in EXPECTED));

  it('the table covers every generated SlotState value but the ONE superseded member (positive control)', () => {
    expect(ALL_SLOT_STATES.length).toBeGreaterThanOrEqual(11);
    expect(UNMAPPED).toHaveLength(1);
    for (const key of Object.keys(EXPECTED).map(Number)) expect(ALL_SLOT_STATES).toContain(key);
  });

  it.each(Object.keys(EXPECTED).map(Number))(
    'maps wire value %i to its MC slot_state_label token',
    (state) => {
      expect(slotStateToken(state as SlotState)).toBe(EXPECTED[state]);
    },
  );

  it('the superseded member maps to unspecified — never active, never an avatar it cannot justify', () => {
    expect(UNMAPPED.map((st) => slotStateToken(st))).toEqual(['unspecified']);
  });

  it.each([5, 12, 99, -1])('maps the unnamed number %i to unspecified — fail closed', (n) => {
    expect(slotStateToken(n as SlotState)).toBe('unspecified');
  });

  it('NOTHING but ACTIVE maps to active', () => {
    for (const state of [...ALL_SLOT_STATES, 5, 12, 99] as SlotState[]) {
      if (state === SlotState.ACTIVE) continue;
      expect(slotStateToken(state)).not.toBe('active');
    }
  });
});

describe('slot media kind', () => {
  it.each([
    [MediaKind.AUDIO, 'audio'],
    [MediaKind.VIDEO_CAMERA, 'video'],
    [MediaKind.VIDEO_SCREEN, 'unknown'],
    [MediaKind.UNSPECIFIED, 'unknown'],
    [42 as MediaKind, 'unknown'],
  ])('maps wire kind %i to %s', (kind, expected) => {
    expect(mediaKindToken(kind)).toBe(expected);
  });

  it('the send direction uses the closed wire kinds only', () => {
    expect(wireMediaKind('audio')).toBe(MediaKind.AUDIO);
    expect(wireMediaKind('video')).toBe(MediaKind.VIDEO_CAMERA);
  });
});
