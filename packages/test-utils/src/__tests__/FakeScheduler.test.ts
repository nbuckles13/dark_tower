// The hand-driven timer double every reconcile timer gate trusts
// (`sdk-core/src/media/__tests__/reconcile.test.ts`).

import { describe, expect, it } from 'vitest';

import { FakeScheduler } from '../FakeScheduler.js';

describe('FakeScheduler', () => {
  it('pending() orders by due time, ties in creation order; nothing fires on its own', () => {
    const s = new FakeScheduler();
    const fired: string[] = [];
    s.setTimeout(() => fired.push('late'), 30);
    s.setTimeout(() => fired.push('tieA'), 10);
    s.setTimeout(() => fired.push('tieB'), 10);
    expect(s.pending().map((t) => t.delayMs)).toEqual([10, 10, 30]);
    const [first, second] = s.pending();
    expect(first!.id).toBeLessThan(second!.id);
    expect(fired).toEqual([]);
    expect(s.size).toBe(3);
  });

  it('advance() fires due timers in order, including ones scheduled inside the window, and stops at the target', () => {
    const s = new FakeScheduler();
    const fired: [string, number][] = [];
    s.setTimeout(() => {
      fired.push(['a', s.clock()]);
      s.setTimeout(() => fired.push(['inside', s.clock()]), 5); // due 15: inside
      s.setTimeout(() => fired.push(['past', s.clock()]), 50); // due 60: past
    }, 10);
    s.advance(20);
    expect(fired).toEqual([
      ['a', 10],
      ['inside', 15],
    ]);
    expect(s.clock()).toBe(20);
    expect(s.pending().map((t) => t.dueMs)).toEqual([60]);
  });

  it('clearTimeout removes a timer; fire(unknown) throws', () => {
    const s = new FakeScheduler();
    let ran = false;
    const h = s.setTimeout(() => {
      ran = true;
    }, 5);
    s.clearTimeout(h);
    expect(s.size).toBe(0);
    s.advance(100);
    expect(ran).toBe(false);
    expect(() => s.fire(999)).toThrow(/no pending timer/);
  });

  it('fire() moves the clock forward to the due time; fireNext() is undefined when idle', () => {
    const s = new FakeScheduler();
    s.setTimeout(() => undefined, 10);
    const id = s.pending()[0]!.id;
    s.advance(25); // fires it at 10, clock at 25
    s.setTimeout(() => undefined, 5); // due 30
    const later = s.pending()[0]!.id;
    expect(later).not.toBe(id);
    s.fire(later);
    expect(s.clock()).toBe(30);
    s.setTimeout(() => undefined, 0); // due 30
    s.fireNext();
    expect(s.clock()).toBe(30);
    expect(s.fireNext()).toBeUndefined();
  });
});
