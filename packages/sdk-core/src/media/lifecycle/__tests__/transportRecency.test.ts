// File: packages/sdk-core/src/media/lifecycle/__tests__/transportRecency.test.ts
//
// The replay classification's membership bound (story 4 C2) FAILS CLOSED: only
// a live member, or a ref removed within the window, is "recent". Every
// negative has its positive arm on the same tracker.

import { describe, expect, it } from 'vitest';

import { TransportRecency } from '../transportRecency.js';
import { handlerRef } from '../../handlerSet.js';

const WINDOW = 60_000;
const A = handlerRef(1);
const B = handlerRef(2);
const C = handlerRef(3);

function rig() {
  let now = 0;
  const r = new TransportRecency(WINDOW, () => now);
  return { r, advance: (ms: number) => (now += ms) };
}

describe('TransportRecency — positive membership, fails closed', () => {
  it('a live member is recent; a NEVER-SEEN id is not', () => {
    const { r } = rig();
    const isRecent = r.arrival(A).isRecent;
    expect(isRecent(A)).toBe(true);
    expect(isRecent(99)).toBe(false);
  });

  it('a removed ref is recent within the window and not after', () => {
    const { r, advance } = rig();
    const isRecent = r.arrival(A).isRecent;
    r.arrival(B);
    r.forget(A);
    expect(isRecent(A)).toBe(true);
    advance(WINDOW + 1);
    expect(isRecent(A)).toBe(false);
  });

  it('PRUNING never revives a long-gone ref (forget(B) after A aged out prunes A: still not recent)', () => {
    const { r, advance } = rig();
    const isRecent = r.arrival(A).isRecent;
    r.arrival(B);
    r.forget(A);
    advance(WINDOW + 1);
    r.forget(B); // prunes A's timestamp
    expect(isRecent(A)).toBe(false);
    expect(isRecent(B)).toBe(true); // positive arm: B was just removed
  });

  it('a STRAGGLER frame after forget never re-marks the ref live — even after its timestamp is pruned', () => {
    const { r, advance } = rig();
    const isRecent = r.arrival(A).isRecent;
    r.forget(A);
    r.arrival(A); // straggler within the window
    advance(WINDOW + 1);
    r.arrival(C);
    r.forget(C); // prunes A
    r.arrival(A); // straggler long after
    expect(isRecent(A)).toBe(false);
    r.arrival(B);
    expect(isRecent(B)).toBe(true); // positive arm: a live member
  });
});
