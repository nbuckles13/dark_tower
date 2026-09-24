// File: packages/sdk-core/src/media/pipeline/__tests__/egressQueue.test.ts

import { describe, expect, it } from 'vitest';

import { BoundedDropOldestQueue } from '../egressQueue.js';

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
