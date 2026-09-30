// File: packages/sdk-core/src/media/lifecycle/__tests__/receiveSourceDeficit.test.ts
//
// The per-interval deficit decision (story 2 R-28), without timers.

import { describe, expect, it } from 'vitest';

import { receiveSourceDeficit, type ActiveAssignmentSample } from '../receiveSourceDeficit.js';

const at = (
  slotId: number,
  senderId: number,
  decoded: number | undefined,
  epoch = 1,
): ActiveAssignmentSample => ({
  slotId,
  senderId,
  activity: decoded === undefined ? undefined : { epoch, decoded },
});

describe('receiveSourceDeficit', () => {
  it('counts an assignment active for a full interval that decoded nothing', () => {
    expect(receiveSourceDeficit([at(0, 7, 10)], [at(0, 7, 10)])).toBe(1);
  });

  it('does not count one that decoded', () => {
    expect(receiveSourceDeficit([at(0, 7, 10)], [at(0, 7, 11)])).toBe(0);
  });

  it('GRACE: an assignment that became active mid-interval does not count', () => {
    expect(receiveSourceDeficit([], [at(0, 7, 0)])).toBe(0);
    // A re-map to a new slot is a new assignment too.
    expect(receiveSourceDeficit([at(0, 7, 5)], [at(1, 7, 5)])).toBe(0);
  });

  it('a lane rebuilt within the interval (new epoch) is not a full interval', () => {
    expect(receiveSourceDeficit([at(0, 7, 5, 1)], [at(0, 7, 5, 2)])).toBe(0);
  });

  it('an active assignment with no lane at all counts once it has had a full interval', () => {
    expect(receiveSourceDeficit([at(0, 7, undefined)], [at(0, 7, undefined)])).toBe(1);
  });

  it('adds the COUNT of silent assignments, not 1', () => {
    const previous = [at(0, 7, 3), at(1, 8, 3), at(2, 9, 3)];
    const current = [at(0, 7, 3), at(1, 8, 3), at(2, 9, 4)];
    expect(receiveSourceDeficit(previous, current)).toBe(2);
  });

  it('an assignment that left is not counted', () => {
    expect(receiveSourceDeficit([at(0, 7, 3)], [])).toBe(0);
  });
});
