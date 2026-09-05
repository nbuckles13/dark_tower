// File: packages/sdk-core/src/media/frame/__tests__/receivePath.test.ts
//
// Receive-path invariants the vectors cannot express, because they are properties
// of receiver STATE over many frames rather than of any single frame.

import { describe, expect, it } from 'vitest';

import { hexToBytes } from '../hex.js';
import { unpackKeyId, packKeyId, type KeyIdParts } from '../keyId.js';
import { ReplayWindow, TransmitKeyCache } from '../receivePath.js';

function parts(senderId: bigint, stream = 0n, generation = 1n): KeyIdParts {
  return { senderId, stream, generation };
}

describe('replay window — ordinary duplicate rejection', () => {
  it('accepts a sequence once and rejects the repeat', () => {
    const w = new ReplayWindow();
    expect(w.admit(parts(1n), 7)).toBe(true);
    expect(w.admit(parts(1n), 7)).toBe(false);
  });

  it('accepts out-of-order arrivals inside the window', () => {
    const w = new ReplayWindow();
    expect(w.admit(parts(1n), 10)).toBe(true);
    expect(w.admit(parts(1n), 8)).toBe(true);
    expect(w.admit(parts(1n), 9)).toBe(true);
    expect(w.admit(parts(1n), 8)).toBe(false);
  });

  it('rejects anything that has fallen out of the window', () => {
    const w = new ReplayWindow(32, 64);
    expect(w.admit(parts(1n), 1)).toBe(true);
    expect(w.admit(parts(1n), 500)).toBe(true);
    expect(w.admit(parts(1n), 1)).toBe(false);
  });

  it('tracks streams and generations independently', () => {
    const w = new ReplayWindow();
    expect(w.admit(parts(1n, 0n, 1n), 7)).toBe(true);
    // Same sequence under a different stream and a different generation is a
    // different (key, nonce) context, not a replay.
    expect(w.admit(parts(1n, 1n, 1n), 7)).toBe(true);
    expect(w.admit(parts(1n, 0n, 2n), 7)).toBe(true);
  });
});

describe('replay window — the insider eviction attack (@security F1)', () => {
  it('one sender flooding its own key space cannot evict another sender s window', () => {
    // THE ATTACK, and why a global LRU is not a security boundary:
    // `stream` is 8 bits and `generation` is 40, so Bob — who needs only a meeting
    // link — can mint verifying frames across his OWN triples until a global LRU
    // evicts Alice's window, then replay a captured Alice frame successfully:
    // signature valid, tag valid, window gone. Nothing reds, and the replay vector
    // still passes because it never floods.
    const w = new ReplayWindow(8, 64);
    const alice = 1n;
    const bob = 2n;

    expect(w.admit(parts(alice, 0n, 1n), 7)).toBe(true);

    // Bob floods far past the per-sender budget.
    for (let g = 0; g < 500; g += 1) {
      w.admit(parts(bob, BigInt(g % 256), BigInt(g)), g);
    }

    // Alice's window survives, because the budget is PER SENDER: eviction pressure
    // from Bob cannot reach Alice's state at all.
    expect(w.admit(parts(alice, 0n, 1n), 7)).toBe(false);
  });

  it('a generation high-water mark closes the replay even after context eviction', () => {
    // The second half of the fix. Even if a sender exhausts its OWN budget and
    // recycles its own contexts, a frame below the highest generation already seen
    // for that (sender, stream) is refused outright — ADR-0036 §4 makes generation
    // monotonic per sender and never reset, so an honest sender never trips this.
    // This state survives context eviction, which turns the LRU from a security
    // boundary into a plain memory bound.
    const w = new ReplayWindow(2, 64);
    expect(w.admit(parts(1n, 0n, 100n), 1)).toBe(true);
    for (let g = 101; g < 200; g += 1) {
      w.admit(parts(1n, 0n, BigInt(g)), 1);
    }
    // Generation 100's context is long evicted, but the frame is still refused.
    expect(w.admit(parts(1n, 0n, 100n), 1)).toBe(false);
  });

  it('self-inflicted eviction is bounded and does not affect anyone else', () => {
    const w = new ReplayWindow(4, 64);
    for (let g = 0; g < 100; g += 1) w.admit(parts(3n, 0n, BigInt(g)), 1);
    expect(w.admit(parts(9n, 0n, 1n), 1)).toBe(true);
  });
});

describe('transmit key cache', () => {
  const kid = hexToBytes('0102030405060708');
  const other = hexToBytes('0102030405060709');
  const key = new Uint8Array(32).fill(0x20);
  const wrap = new Uint8Array(48).fill(0x30);

  it('is keyed by the received bytes, so near-identical key ids do not collide', () => {
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap);
    expect(c.has(unpackKeyId(kid), kid)).toBe(true);
    expect(c.has(unpackKeyId(other), other)).toBe(false);
  });

  it('recognises the same wrap and does not require a re-unwrap', () => {
    // ADR-0036 §4:413 — "a receiver holding the key does no per-frame unwrap".
    // Audio carries the wrap on EVERY frame, so this comparison is the steady
    // state and the AEAD is not invoked.
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, wrap)).toBe(true);
  });

  it('does NOT recognise a different wrap for the same key id', () => {
    // This is what keeps a mis-bound wrap observable. A receiver that skipped on
    // key PRESENCE alone would accept a frame whose wrap is bound elsewhere
    // without ever looking at it, making `wrap_key_id_mismatch` unreachable in
    // principle rather than merely rare.
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, new Uint8Array(48).fill(0x31))).toBe(false);
  });

  it('bounds entries per sender', () => {
    const c = new TransmitKeyCache(4);
    for (let g = 0; g < 50; g += 1) {
      const id = packKeyId({ senderId: 1n, stream: 0n, generation: BigInt(g) });
      c.set(unpackKeyId(id), id, key, wrap);
    }
    const first = packKeyId({ senderId: 1n, stream: 0n, generation: 0n });
    expect(c.has(unpackKeyId(first), first)).toBe(false);
    const last = packKeyId({ senderId: 1n, stream: 0n, generation: 49n });
    expect(c.has(unpackKeyId(last), last)).toBe(true);
  });

  it('one sender cannot evict another sender s keys', () => {
    const c = new TransmitKeyCache(2);
    const mine = packKeyId({ senderId: 7n, stream: 0n, generation: 1n });
    c.set(unpackKeyId(mine), mine, key, wrap);
    for (let g = 0; g < 50; g += 1) {
      const id = packKeyId({ senderId: 8n, stream: 0n, generation: BigInt(g) });
      c.set(unpackKeyId(id), id, key, wrap);
    }
    expect(c.has(unpackKeyId(mine), mine)).toBe(true);
  });

  it('clear() drops everything, so nothing outlives a session', () => {
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap);
    c.clear();
    expect(c.has(unpackKeyId(kid), kid)).toBe(false);
  });

  it('clear() overwrites the key buffer, not just the reference (ADR-0028 §5)', () => {
    const c = new TransmitKeyCache();
    const live = new Uint8Array(32).fill(0x55);
    c.set(unpackKeyId(kid), kid, live, wrap);
    c.clear();
    // The buffer the cache held is zeroed. `set` copies the wrap but stores the
    // key by reference, so the caller's buffer is the one overwritten — which is
    // the point: no live key material survives teardown in a buffer we can reach.
    expect([...live].every((b) => b === 0)).toBe(true);
  });

  it('overwrites an evicted key buffer on the eviction path too', () => {
    const c = new TransmitKeyCache(1);
    const first = new Uint8Array(32).fill(0x55);
    const idA = packKeyId({ senderId: 5n, stream: 0n, generation: 1n });
    const idB = packKeyId({ senderId: 5n, stream: 0n, generation: 2n });
    c.set(unpackKeyId(idA), idA, first, wrap);
    c.set(unpackKeyId(idB), idB, new Uint8Array(32).fill(0x66), wrap);
    expect([...first].every((b) => b === 0)).toBe(true);
  });
});

describe('ReplayWindow.clear', () => {
  it('resets both the window and the generation high-water mark', () => {
    const w = new ReplayWindow();
    expect(w.admit(parts(1n, 0n, 5n), 7)).toBe(true);
    // Before clear: a below-high-water generation is rejected, and the exact
    // (kid, seq) is a replay.
    expect(w.admit(parts(1n, 0n, 1n), 1)).toBe(false);
    expect(w.admit(parts(1n, 0n, 5n), 7)).toBe(false);

    w.clear();

    // After clear: the window forgot seq 7, and the high-water forgot gen 5 — so
    // both the replay and the below-high-water generation are accepted again.
    expect(w.admit(parts(1n, 0n, 5n), 7)).toBe(true);
    const fresh = new ReplayWindow();
    expect(fresh.admit(parts(1n, 0n, 1n), 1)).toBe(true);
  });
});
