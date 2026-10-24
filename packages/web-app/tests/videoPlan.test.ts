// Node tier for `e2e/videoPlan.ts` (story 3 C-C1).

import { describe, expect, test } from 'vitest';
import { videoSlotIds, videoSlotPlans } from '../e2e/videoPlan.js';

describe('videoSlotIds / videoSlotPlans', () => {
  test('video ids follow the declared audio ids (N..N+M-1)', () => {
    expect(videoSlotIds({ audioReceiveSlots: 3, videoReceiveSlots: 2 })).toEqual([3, 4]);
    expect(videoSlotIds({ audioReceiveSlots: 0, videoReceiveSlots: 2 })).toEqual([0, 1]);
    expect(videoSlotIds({ audioReceiveSlots: 3, videoReceiveSlots: 0 })).toEqual([]);
  });
  test('zips the oracle expectations in declaration order, truncated tail included', () => {
    expect(
      videoSlotPlans({ audioReceiveSlots: 1, videoReceiveSlots: 2 }, [
        { state: 'active', occupant: 'A' },
        { state: 'unavailable' },
      ]),
    ).toEqual([
      { slotId: 1, expectation: { state: 'active', occupant: 'A' } },
      { slotId: 2, expectation: { state: 'unavailable' } },
    ]);
  });
  test('a length mismatch is a HARNESS error, never padded', () => {
    expect(() =>
      videoSlotPlans({ audioReceiveSlots: 1, videoReceiveSlots: 2 }, [{ state: 'active' }]),
    ).toThrow(/HARNESS/);
  });
});
