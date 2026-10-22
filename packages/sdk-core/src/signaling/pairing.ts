// File: packages/sdk-core/src/signaling/pairing.ts
//
// The participant PAIRING VIEW (story 3 R-3): which audio slot and which video
// slot show each participant, keyed by SENDER id from MC's assignment data.
// Pairing is by publisher, never by slot index.
//
// An assignment names its sender for audio `active` and for every avatar state.
// A LIVE VIDEO slot names none (MC never names the live video source, story 3
// R-5): it pairs by the VERIFIED key-id sender of its last RENDERED frame,
// passed in as `liveVideo` (story 3 C-B; `VideoReceiver.liveSenders`). Never by
// slot index, and never from an unverified header. Presentation state.

import type { StreamAssignmentEvent } from './events.js';

/** The slots showing one participant. Either may be absent. */
export interface SlotPair {
  readonly audioSlotId?: number;
  readonly videoSlotId?: number;
}

/**
 * Sender id -> the slots showing that sender: from one assignments snapshot,
 * plus `liveVideo` (video slot id -> the verified sender its live slot last
 * rendered) for the live video slots the assignments cannot name.
 */
export function pairSlotsBySender(
  assignments: readonly StreamAssignmentEvent[],
  liveVideo: ReadonlyMap<number, number> = new Map(),
): ReadonlyMap<number, SlotPair> {
  const pairs = new Map<number, SlotPair>();
  for (const [slotId, senderId] of liveVideo) {
    pairs.set(senderId, { ...pairs.get(senderId), videoSlotId: slotId });
  }
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
