// File: packages/sdk-core/src/media/pipeline/__tests__/hopSequenceMonitor.test.ts
//
// The downlink hop-gap detector. The first four cases are the original contract
// (moved here from `egressQueue.test.ts`, where they did not belong); the rest
// pin the story-2 resets: a slot refill and a publisher reconnect each restart
// MH's counter under an unchanged slot id, and neither may read as loss or as
// a run of reorders.

import { describe, expect, it } from 'vitest';

import {
  DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES,
  HopSequenceMonitor,
} from '../hopSequenceMonitor.js';

const A = 7;
const B = 9;
const U32_MAX = 2 ** 32 - 1;

describe('HopSequenceMonitor', () => {
  it('counts MISSING FRAMES, not gap events', () => {
    // A counter of missing numbers is comparable against
    // `frames_received_total` as a loss rate; a counter of events is not.
    const m = new HopSequenceMonitor([0]);
    expect(m.observe(0, 10, A).missing).toBe(0); // first observation is not a gap
    expect(m.observe(0, 14, A).missing).toBe(3);
    expect(m.observe(0, 15, A).missing).toBe(0);
  });

  it('counts a late arrival as a reorder, not as a further gap', () => {
    // QUIC datagrams are unordered, so a reordered frame first OPENS a gap and
    // then arrives late. Keeping the two separate is what makes
    // `gap_frames - reorder` an honest loss estimate; folded together, normal
    // reordering reads as loss in the first congestion incident.
    const m = new HopSequenceMonitor([0]);
    m.observe(0, 1, A);
    expect(m.observe(0, 3, A).missing).toBe(1);
    const late = m.observe(0, 2, A);
    expect(late.reordered).toBe(true);
    expect(late.missing).toBe(0);
  });

  it('creates NO state for an undeclared stream_id', () => {
    // The relay region is unauthenticated: a media handler writes `stream_id`
    // freely and nobody signs it. An unbounded 16-bit map key is 65 536
    // attacker-chosen state entries per connection, created at datagram rate.
    const m = new HopSequenceMonitor([0]);
    for (let i = 0; i < 1000; i += 1) {
      const observed = m.observe(0xf000 + i, i, A);
      expect(observed.undeclaredStreamId).toBe(true);
      expect(observed.missing).toBe(0);
      expect(observed.reordered).toBe(false);
    }
    // The declared slot's own state is untouched by the flood.
    expect(m.observe(0, 5, A).missing).toBe(0);
    expect(m.observe(0, 7, A).missing).toBe(1);
  });

  it('tracks declared slots independently', () => {
    const m = new HopSequenceMonitor([0, 1]);
    m.observe(0, 5, A);
    m.observe(1, 100, B);
    expect(m.observe(0, 6, A).missing).toBe(0);
    expect(m.observe(1, 103, B).missing).toBe(2);
  });

  describe('slot refill (a different sender under the same slot)', () => {
    it('a new sender whose counter starts at 0 is a new baseline, not a run of reorders', () => {
      // The defect this fixes: with a slot-only mark, every frame of the
      // refilled sender's fresh counter read as a reorder until it passed the
      // old mark.
      //
      // The old mark (21) is deliberately INSIDE the restart bound (64): the
      // backward jump alone would read as a REORDER, so only the sender-change
      // reset can explain a clean baseline here. From a mark of ~5000 the
      // restart rule would mask a missing sender reset.
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 20, A);
      m.observe(0, 21, A);
      const first = m.observe(0, 0, B);
      expect(first).toEqual({ missing: 0, reordered: false, undeclaredStreamId: false });
      // The new baseline behaves normally from here on.
      expect(m.observe(0, 1, B)).toEqual({
        missing: 0,
        reordered: false,
        undeclaredStreamId: false,
      });
      expect(m.observe(0, 3, B).missing).toBe(1);
      expect(m.observe(0, 2, B).reordered).toBe(true);
    });

    it('a new sender whose counter is AHEAD of the old mark is not counted as loss', () => {
      // MH's counter is per (publisher, egress stream): a sender that fed this
      // stream earlier resumes from where it stopped. Without the sender reset
      // that forward jump would read as thousands of missing frames.
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 100, A);
      expect(m.observe(0, 8000, B)).toEqual({
        missing: 0,
        reordered: false,
        undeclaredStreamId: false,
      });
      expect(m.observe(0, 8001, B).missing).toBe(0);
    });

    it('a frame with an unreadable key id (no sender) never triggers a reset', () => {
      // Marks stay INSIDE the restart bound so the restart rule cannot stand in
      // for the sender rule in the positive control below.
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 20, A);
      // A gap is still a gap when the frame's key id could not be read.
      expect(m.observe(0, 22, undefined).missing).toBe(1);
      // And the slot's sender was not cleared: A continuing is not a "change".
      expect(m.observe(0, 23, A).missing).toBe(0);
      // A backward step from A itself, within the bound, is a reorder...
      expect(m.observe(0, 5, A).reordered).toBe(true);
      // ...and the SAME step from B is a reset. Only the sender differs, so this
      // is a live positive control on the sender-change path.
      expect(m.observe(0, 5, B).reordered).toBe(false);
    });

    it('a slot first seen without a sender adopts the next known sender without resetting', () => {
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 10, undefined);
      expect(m.observe(0, 12, A).missing).toBe(1);
      // Now A is recorded, so B is a change.
      expect(m.observe(0, 0, B)).toEqual({
        missing: 0,
        reordered: false,
        undeclaredStreamId: false,
      });
    });
  });

  describe('publisher reconnect (same sender, counter restarted)', () => {
    it('a backward jump beyond the bound is a restart, not a reorder', () => {
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 5000, A);
      expect(m.observe(0, 0, A)).toEqual({
        missing: 0,
        reordered: false,
        undeclaredStreamId: false,
      });
      expect(m.observe(0, 1, A)).toEqual({
        missing: 0,
        reordered: false,
        undeclaredStreamId: false,
      });
    });

    it('the bound is exact: behind by the bound is a reorder, one deeper is a restart', () => {
      const bound = 64;
      const m = new HopSequenceMonitor([0], bound);
      m.observe(0, 1000, A);
      expect(m.observe(0, 1000 - bound, A).reordered).toBe(true);
      // Still at 1000 (a reorder does not move the mark).
      const restart = m.observe(0, 1000 - bound - 1, A);
      expect(restart.reordered).toBe(false);
      expect(restart.missing).toBe(0);
      // The mark moved to the restart point.
      expect(m.observe(0, 1000 - bound, A).missing).toBe(0);
    });

    it('a large FORWARD jump is still counted as real loss', () => {
      // Only backward jumps restart. A 20 s outage is loss and must say so.
      const m = new HopSequenceMonitor([0]);
      m.observe(0, 10, A);
      expect(m.observe(0, 5010, A).missing).toBe(4999);
    });
  });

  it('serial arithmetic: a wrap past u32::MAX is one step forward, not a restart', () => {
    const m = new HopSequenceMonitor([0]);
    m.observe(0, U32_MAX - 1, A);
    expect(m.observe(0, U32_MAX, A)).toEqual({
      missing: 0,
      reordered: false,
      undeclaredStreamId: false,
    });
    expect(m.observe(0, 0, A)).toEqual({ missing: 0, reordered: false, undeclaredStreamId: false });
    expect(m.observe(0, 2, A).missing).toBe(1);
    // And a late frame from just before the wrap is a reorder, not a restart.
    expect(m.observe(0, U32_MAX, A).reordered).toBe(true);
  });

  it('a repeated hop number is a reorder (at the mark), not a gap', () => {
    const m = new HopSequenceMonitor([0]);
    m.observe(0, 42, A);
    expect(m.observe(0, 42, A)).toEqual({ missing: 0, reordered: true, undeclaredStreamId: false });
  });

  it('refuses a bound outside [1, 2^31) rather than behaving strangely', () => {
    expect(() => new HopSequenceMonitor([0], 0)).toThrow(RangeError);
    expect(() => new HopSequenceMonitor([0], -1)).toThrow(RangeError);
    expect(() => new HopSequenceMonitor([0], 1.5)).toThrow(RangeError);
    expect(() => new HopSequenceMonitor([0], 2 ** 31)).toThrow(RangeError);
    expect(() => new HopSequenceMonitor([0], 2 ** 31 - 1)).not.toThrow();
    expect(
      () => new HopSequenceMonitor([0], DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES),
    ).not.toThrow();
  });

  it('clear() drops every mark', () => {
    const m = new HopSequenceMonitor([0]);
    m.observe(0, 5000, A);
    m.clear();
    // After clear, 0 is a first observation — not a reorder against 5000.
    expect(m.observe(0, 0, A)).toEqual({ missing: 0, reordered: false, undeclaredStreamId: false });
  });
});
