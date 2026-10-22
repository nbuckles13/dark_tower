// File: packages/sdk-core/src/signaling/pairing.ts
//
// The participant PAIRING VIEW (story 3 R-3): which audio slot and which video
// slot show each participant, keyed by SENDER id from MC's assignment data.
// Pairing is by publisher, never by slot index.
//
// FROM ASSIGNMENT DATA ONLY. An assignment names its sender for audio `active`
// and for every avatar state; a LIVE VIDEO slot names none (MC never names the
// live video source — the verified key-id sender does, story 3 R-5), so a live
// video slot does not appear here. Story 3 C-B (task 14) adds live-video
// pairing from verified frames. Presentation state, never attribution.

import type { StreamAssignmentEvent } from './events.js';

/** The slots showing one participant. Either may be absent. */
export interface SlotPair {
  readonly audioSlotId?: number;
  readonly videoSlotId?: number;
}

/** Sender id -> the slots naming that sender, from one assignments snapshot. */
export function pairSlotsBySender(
  assignments: readonly StreamAssignmentEvent[],
): ReadonlyMap<number, SlotPair> {
  const pairs = new Map<number, SlotPair>();
  for (const a of assignments) {
    if (a.senderId === undefined) continue;
    if (a.mediaKind !== 'audio' && a.mediaKind !== 'video') continue;
    const prev = pairs.get(a.senderId) ?? {};
    pairs.set(
      a.senderId,
      a.mediaKind === 'audio'
        ? { ...prev, audioSlotId: a.slotId }
        : { ...prev, videoSlotId: a.slotId },
    );
  }
  return pairs;
}
