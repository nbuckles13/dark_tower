// Story 3 R-3: the pairing view — by publisher (sender id), never by slot index.

import { describe, expect, it } from 'vitest';

import { pairSlotsBySender } from '../pairing.js';
import type { StreamAssignmentEvent } from '../events.js';

function slot(
  slotId: number,
  mediaKind: StreamAssignmentEvent['mediaKind'],
  senderId: number | undefined,
  slotState: StreamAssignmentEvent['slotState'],
): StreamAssignmentEvent {
  return { slotId, mediaKind, senderId, mediaHandlerUrl: '', slotState };
}

describe('pairSlotsBySender', () => {
  it("pairs one participant's audio and video slots by sender id, whatever the slot indices", () => {
    const pairs = pairSlotsBySender([
      slot(0, 'audio', 7, 'active'),
      slot(1, 'audio', 8, 'active'),
      slot(2, 'video', 8, 'avatar_no_video'),
      slot(3, 'video', 7, 'avatar_client_muted'),
    ]);
    expect(pairs.get(7)).toEqual({ audioSlotId: 0, videoSlotId: 3 });
    expect(pairs.get(8)).toEqual({ audioSlotId: 1, videoSlotId: 2 });
  });

  it('a live video slot names no sender, so it does not appear (C-B pairs it from frames)', () => {
    const pairs = pairSlotsBySender([slot(0, 'video', undefined, 'active')]);
    expect(pairs.size).toBe(0);
  });

  it('an unknown-kind slot never pairs', () => {
    expect(pairSlotsBySender([slot(0, 'unknown', 7, 'active')]).size).toBe(0);
  });

  it('a participant with only one kind has only that half', () => {
    expect(pairSlotsBySender([slot(4, 'video', 9, 'avatar_no_video')]).get(9)).toEqual({
      videoSlotId: 4,
    });
  });
});
