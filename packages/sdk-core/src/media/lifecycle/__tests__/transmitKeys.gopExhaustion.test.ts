// File: packages/sdk-core/src/media/lifecycle/__tests__/transmitKeys.gopExhaustion.test.ts
//
// THE u32::MAX STOP (ADR-0036 §2): "It never wraps: at u32::MAX the sender stops
// that stream, counts it, and resumes only under a fresh stream number or sender
// id."
//
// 2^32 calls are not runnable, so this file narrows the FIELD WIDTH the owner
// derives its ceiling from (`wire_constants.gop_counter_field_bytes`) to one
// byte. That exercises the real boundary arithmetic in `transmitKeys.ts` — the
// ceiling is `2^(8 * width) - 1`, never a literal — with no test-only seam on
// the production class (a seek/seed parameter would itself be a reset path).
// Only `transmitKeys.ts`'s view of the constant changes; nothing here builds a
// frame, so the codec's own layout check never sees the narrowed width.
//
// What `null` turns into on the wire path (nothing sent, counted per active
// lane) is `pipeline/__tests__/egress.multiTarget.test.ts`.

import { describe, expect, it, vi } from 'vitest';

import type * as WireConstantsModule from '../../frame/wireConstants.js';

vi.mock('../../frame/wireConstants.js', async (importOriginal) => {
  const real = await importOriginal<typeof WireConstantsModule>();
  return { ...real, WIRE_CONSTANTS: { ...real.WIRE_CONSTANTS, gop_counter_field_bytes: 1 } };
});

const { TransmitKeyManager } = await import('../transmitKeys.js');

/** The narrowed field's maximum, i.e. the analogue of u32::MAX. */
const MAX = 0xff;

describe('gop_counter exhaustion: the last legal value is issued, then the stream stops', () => {
  it('issues 0..=MAX exactly once each, then null — it never wraps to 0', () => {
    const mgr = new TransmitKeyManager(258);
    const issued: number[] = [];
    for (let i = 0; i <= MAX; i += 1) issued.push(mgr.nextGopCounter(0)!);
    expect(issued).toEqual(Array.from({ length: MAX + 1 }, (_, i) => i));
    expect(issued.at(-1)).toBe(MAX);
    expect(mgr.nextGopCounter(0)).toBeNull();
  });

  it('stays stopped: rotation and teardown do not revive it', () => {
    const mgr = new TransmitKeyManager(258);
    for (let i = 0; i <= MAX; i += 1) mgr.nextGopCounter(0);
    for (let i = 0; i < 3; i += 1) expect(mgr.nextGopCounter(0)).toBeNull();
    mgr.rotate();
    expect(mgr.nextGopCounter(0)).toBeNull();
    mgr.clear();
    expect(mgr.nextGopCounter(0)).toBeNull();
  });

  it('stops only that stream; a fresh stream number resumes at 0', () => {
    const mgr = new TransmitKeyManager(258);
    for (let i = 0; i <= MAX; i += 1) mgr.nextGopCounter(0);
    expect(mgr.nextGopCounter(0)).toBeNull();
    expect(mgr.nextGopCounter(1)).toBe(0);
  });

  it('a fresh sender id (a new manager, i.e. a new join) resumes at 0', () => {
    const old = new TransmitKeyManager(258);
    for (let i = 0; i <= MAX; i += 1) old.nextGopCounter(0);
    expect(old.nextGopCounter(0)).toBeNull();
    expect(new TransmitKeyManager(259).nextGopCounter(0)).toBe(0);
  });
});
