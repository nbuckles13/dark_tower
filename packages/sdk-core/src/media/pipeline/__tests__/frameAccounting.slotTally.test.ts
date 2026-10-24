// File: packages/sdk-core/src/media/pipeline/__tests__/frameAccounting.slotTally.test.ts
//
// The per-slot LOCAL tallies (story 3 R-35): pre-allocated for the declared
// slot ids at construction, bumped by integer `++` only, and silent (no throw,
// nothing added) for any slot outside that set. Never exported.

import { describe, expect, it } from 'vitest';

import { FrameAccounting } from '../frameAccounting.js';
import { videoMetrics } from './videoFixtures.js';

describe('FrameAccounting per-slot tallies (R-35)', () => {
  it('counts packets, bytes and drops by reason for a declared slot, exactly', () => {
    const { metrics } = videoMetrics();
    const acc = new FrameAccounting(metrics, undefined, [3, 4]);
    acc.arrived(3, 100);
    acc.arrived(3, 250);
    acc.dropped('replay_detected', 3, 7);
    acc.dropped('signature_invalid', 3, undefined);
    acc.dropped('signature_invalid', 3, undefined);
    const three = acc.slotTally(3)!;
    expect(three.packets).toBe(2);
    expect(three.bytes).toBe(350);
    expect(three.drops.replay_detected).toBe(1);
    expect(three.drops.signature_invalid).toBe(2);
    expect(acc.slotTally(4)).toEqual({ packets: 0, bytes: 0, drops: acc.slotTally(4)!.drops });
    expect(Object.values(acc.slotTally(4)!.drops).every((n) => n === 0)).toBe(true);
    // The kind totals are unchanged in meaning.
    expect(acc.framesDropped).toBe(3);
  });

  it('a slot outside the declared set is ignored: no throw, no tally created', () => {
    const { metrics } = videoMetrics();
    const acc = new FrameAccounting(metrics, undefined, [3]);
    expect(() => {
      acc.arrived(99, 10);
      acc.arrived('undeclared', 10);
      acc.arrived('unparsed', 10);
      acc.dropped('truncated', 99, undefined);
      acc.dropped('truncated', 'undeclared', undefined);
    }).not.toThrow();
    expect(acc.slotTally(99)).toBeUndefined();
    expect(acc.slotTally(3)!.packets).toBe(0);
    expect(acc.slotTally(3)!.drops.truncated).toBe(0);
    expect(acc.framesDropped).toBe(2);
  });

  it('with no declared slots (the default) nothing is tallied', () => {
    const { metrics } = videoMetrics();
    const acc = new FrameAccounting(metrics);
    acc.arrived(1, 10);
    expect(acc.slotTally(1)).toBeUndefined();
  });
});
