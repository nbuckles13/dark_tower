// File: packages/sdk-core/src/media/pipeline/__tests__/egressQueue.test.ts

import { describe, expect, it } from 'vitest';

import { BoundedDropOldestQueue } from '../egressQueue.js';
import { HopSequenceMonitor } from '../hopSequenceMonitor.js';

describe('BoundedDropOldestQueue', () => {
  it('returns the evicted item so the CALLER counts it', () => {
    // The queue holds no metric handle and no sink reference, mirroring
    // `crates/mh-service/src/media/queue.rs::BoundedDropOldest`. Coupling the
    // ring to telemetry is what makes a data structure untestable and a counter
    // unmovable.
    const q = new BoundedDropOldestQueue<number>(2);
    expect(q.push(1)).toBeUndefined();
    expect(q.push(2)).toBeUndefined();
    expect(q.push(3)).toBe(1);
    expect(q.depth).toBe(2);
  });

  it('drops OLDEST, never newest', () => {
    // A realtime path keeps the freshest audio: the oldest frame in a stalled
    // queue is already past usefulness by the time the stall clears. Note the
    // deliberate non-collapse with MC's participant mailbox, which drops NEWEST
    // for the opposite reason — a control plane wants the earliest instruction.
    const q = new BoundedDropOldestQueue<string>(2);
    q.push('a');
    q.push('b');
    q.push('c');
    expect(q.shift()).toBe('b');
    expect(q.shift()).toBe('c');
    expect(q.shift()).toBeUndefined();
  });

  it('refuses a non-positive bound rather than behaving as unbounded', () => {
    expect(() => new BoundedDropOldestQueue<number>(0)).toThrow(RangeError);
  });

  it('drains everything at teardown', () => {
    const q = new BoundedDropOldestQueue<number>(4);
    q.push(1);
    q.push(2);
    expect(q.drain()).toEqual([1, 2]);
    expect(q.depth).toBe(0);
  });
});

describe('HopSequenceMonitor', () => {
  it('counts MISSING FRAMES, not gap events', () => {
    // A counter of missing numbers is comparable against
    // `frames_received_total` as a loss rate; a counter of events is not.
    const m = new HopSequenceMonitor([0]);
    expect(m.observe(0, 10).missing).toBe(0); // first observation is not a gap
    expect(m.observe(0, 14).missing).toBe(3);
    expect(m.observe(0, 15).missing).toBe(0);
  });

  it('counts a late arrival as a reorder, not as a further gap', () => {
    // QUIC datagrams are unordered, so a reordered frame first OPENS a gap and
    // then arrives late. Keeping the two separate is what makes
    // `gap_frames - reorder` an honest loss estimate; folded together, normal
    // reordering reads as loss in the first congestion incident.
    const m = new HopSequenceMonitor([0]);
    m.observe(0, 1);
    expect(m.observe(0, 3).missing).toBe(1);
    const late = m.observe(0, 2);
    expect(late.reordered).toBe(true);
    expect(late.missing).toBe(0);
  });

  it('creates NO state for an undeclared stream_id', () => {
    // The relay region is unauthenticated: a media handler writes `stream_id`
    // freely and nobody signs it. An unbounded 16-bit map key is 65 536
    // attacker-chosen state entries per connection, created at datagram rate.
    const m = new HopSequenceMonitor([0]);
    for (let i = 0; i < 1000; i += 1) {
      const observed = m.observe(0xf000 + i, i);
      expect(observed.undeclaredStreamId).toBe(true);
      expect(observed.missing).toBe(0);
      expect(observed.reordered).toBe(false);
    }
    // The declared slot's own state is untouched by the flood.
    expect(m.observe(0, 5).missing).toBe(0);
    expect(m.observe(0, 7).missing).toBe(1);
  });

  it('tracks declared slots independently', () => {
    const m = new HopSequenceMonitor([0, 1]);
    m.observe(0, 5);
    m.observe(1, 100);
    expect(m.observe(0, 6).missing).toBe(0);
    expect(m.observe(1, 103).missing).toBe(2);
  });
});
