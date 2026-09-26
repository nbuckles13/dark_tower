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

/**
 * The KEK generation — the replay/cache SCOPE — for cases about behaviour
 * WITHIN one scope. Cases about scoping itself name their generations.
 */
const G = 1;

describe('replay window — ordinary duplicate rejection', () => {
  it('accepts a sequence once and rejects the repeat', () => {
    const w = new ReplayWindow();
    expect(w.admit(G, parts(1n), 7)).toBe(true);
    expect(w.admit(G, parts(1n), 7)).toBe(false);
  });

  it('accepts out-of-order arrivals inside the window', () => {
    const w = new ReplayWindow();
    expect(w.admit(G, parts(1n), 10)).toBe(true);
    expect(w.admit(G, parts(1n), 8)).toBe(true);
    expect(w.admit(G, parts(1n), 9)).toBe(true);
    expect(w.admit(G, parts(1n), 8)).toBe(false);
  });

  it('rejects anything that has fallen out of the window', () => {
    const w = new ReplayWindow(32, 64);
    expect(w.admit(G, parts(1n), 1)).toBe(true);
    expect(w.admit(G, parts(1n), 500)).toBe(true);
    expect(w.admit(G, parts(1n), 1)).toBe(false);
  });

  it('tracks streams and generations independently', () => {
    const w = new ReplayWindow();
    expect(w.admit(G, parts(1n, 0n, 1n), 7)).toBe(true);
    // Same sequence under a different stream and a different generation is a
    // different (key, nonce) context, not a replay.
    expect(w.admit(G, parts(1n, 1n, 1n), 7)).toBe(true);
    expect(w.admit(G, parts(1n, 0n, 2n), 7)).toBe(true);
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

    expect(w.admit(G, parts(alice, 0n, 1n), 7)).toBe(true);

    // Bob floods far past the per-sender budget.
    for (let g = 0; g < 500; g += 1) {
      w.admit(G, parts(bob, BigInt(g % 256), BigInt(g)), g);
    }

    // Alice's window survives, because the budget is PER SENDER: eviction pressure
    // from Bob cannot reach Alice's state at all.
    expect(w.admit(G, parts(alice, 0n, 1n), 7)).toBe(false);
  });

  it('a generation high-water mark closes the replay even after context eviction', () => {
    // The second half of the fix. Even if a sender exhausts its OWN budget and
    // recycles its own contexts, a frame below the highest generation already seen
    // for that (scope, sender, stream) is refused outright. Transmit-key generation
    // is monotonic per (KEK generation, sender_id) — NOT per sender across the
    // meeting: a reissued id's new holder restarts at 0 under a NEW KEK generation
    // (story 2 R-16), which is a different scope (A5, D1). Within one scope an
    // honest sender never trips this.
    // This state survives context eviction, which turns the LRU from a security
    // boundary into a plain memory bound.
    const w = new ReplayWindow(2, 64);
    expect(w.admit(G, parts(1n, 0n, 100n), 1)).toBe(true);
    for (let g = 101; g < 200; g += 1) {
      w.admit(G, parts(1n, 0n, BigInt(g)), 1);
    }
    // Generation 100's context is long evicted, but the frame is still refused.
    expect(w.admit(G, parts(1n, 0n, 100n), 1)).toBe(false);
  });

  it('self-inflicted eviction is bounded and does not affect anyone else', () => {
    const w = new ReplayWindow(4, 64);
    for (let g = 0; g < 100; g += 1) w.admit(G, parts(3n, 0n, BigInt(g)), 1);
    expect(w.admit(G, parts(9n, 0n, 1n), 1)).toBe(true);
  });
});

describe('transmit key cache', () => {
  const kid = hexToBytes('0102030405060708');
  const other = hexToBytes('0102030405060709');
  const key = new Uint8Array(32).fill(0x20);
  const wrap = new Uint8Array(48).fill(0x30);

  it('is keyed by the received bytes, so near-identical key ids do not collide', () => {
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap, G);
    expect(c.has(unpackKeyId(kid), kid)).toBe(true);
    expect(c.has(unpackKeyId(other), other)).toBe(false);
  });

  it('recognises the same wrap and does not require a re-unwrap', () => {
    // ADR-0036 §4:413 — "a receiver holding the key does no per-frame unwrap".
    // Audio carries the wrap on EVERY frame, so this comparison is the steady
    // state and the AEAD is not invoked.
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap, G);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, wrap)).toBe(true);
  });

  it('does NOT recognise a different wrap for the same key id', () => {
    // This is what keeps a mis-bound wrap observable. A receiver that skipped on
    // key PRESENCE alone would accept a frame whose wrap is bound elsewhere
    // without ever looking at it, making `wrap_key_id_mismatch` unreachable in
    // principle rather than merely rare.
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap, G);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, new Uint8Array(48).fill(0x31))).toBe(false);
  });

  it('bounds entries per sender', () => {
    const c = new TransmitKeyCache(4);
    for (let g = 0; g < 50; g += 1) {
      const id = packKeyId({ senderId: 1n, stream: 0n, generation: BigInt(g) });
      c.set(unpackKeyId(id), id, key, wrap, G);
    }
    const first = packKeyId({ senderId: 1n, stream: 0n, generation: 0n });
    expect(c.has(unpackKeyId(first), first)).toBe(false);
    const last = packKeyId({ senderId: 1n, stream: 0n, generation: 49n });
    expect(c.has(unpackKeyId(last), last)).toBe(true);
  });

  it('one sender cannot evict another sender s keys', () => {
    const c = new TransmitKeyCache(2);
    const mine = packKeyId({ senderId: 7n, stream: 0n, generation: 1n });
    c.set(unpackKeyId(mine), mine, key, wrap, G);
    for (let g = 0; g < 50; g += 1) {
      const id = packKeyId({ senderId: 8n, stream: 0n, generation: BigInt(g) });
      c.set(unpackKeyId(id), id, key, wrap, G);
    }
    expect(c.has(unpackKeyId(mine), mine)).toBe(true);
  });

  it('clear() drops everything, so nothing outlives a session', () => {
    const c = new TransmitKeyCache();
    c.set(unpackKeyId(kid), kid, key, wrap, G);
    c.clear();
    expect(c.has(unpackKeyId(kid), kid)).toBe(false);
  });

  it('clear() overwrites the key buffer, not just the reference (ADR-0028 §5)', () => {
    const c = new TransmitKeyCache();
    const live = new Uint8Array(32).fill(0x55);
    c.set(unpackKeyId(kid), kid, live, wrap, G);
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
    c.set(unpackKeyId(idA), idA, first, wrap, G);
    c.set(unpackKeyId(idB), idB, new Uint8Array(32).fill(0x66), wrap, G);
    expect([...first].every((b) => b === 0)).toBe(true);
  });
});

describe('ReplayWindow.clear', () => {
  it('resets both the window and the generation high-water mark', () => {
    const w = new ReplayWindow();
    expect(w.admit(G, parts(1n, 0n, 5n), 7)).toBe(true);
    // Before clear: a below-high-water generation is rejected, and the exact
    // (kid, seq) is a replay.
    expect(w.admit(G, parts(1n, 0n, 1n), 1)).toBe(false);
    expect(w.admit(G, parts(1n, 0n, 5n), 7)).toBe(false);

    w.clear();

    // After clear: the window forgot seq 7, and the high-water forgot gen 5 — so
    // both the replay and the below-high-water generation are accepted again.
    expect(w.admit(G, parts(1n, 0n, 5n), 7)).toBe(true);
    const fresh = new ReplayWindow();
    expect(fresh.admit(G, parts(1n, 0n, 1n), 1)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Story 2 R-16 under ruling (A'): state scoped by (kek_generation, sender_id).
// Case ids are the ones named in the devloop plan ("sdk-core test cases"); each
// records the MUTATION it goes red against.
// ---------------------------------------------------------------------------

describe('replay window — scoped by KEK generation', () => {
  const g = 1;
  const next = 2;

  it('A1/A2 — scopes partition: the same frame in the next scope is new; in its own scope it is a replay', () => {
    // A1 red against: no scoping. A2 red against: scoping by CLOBBERING (a new
    // scope replacing the old one) rather than partitioning.
    const w = new ReplayWindow();
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(true);
    expect(w.admit(next, parts(1n, 0n, 5n), 7)).toBe(true);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(false);
    expect(w.admit(next, parts(1n, 0n, 5n), 7)).toBe(false);
  });

  it('A3 — the reverse order partitions too', () => {
    // Red against: scope taken from "the current generation" instead of the
    // frame's provenance, which puts both orders in one bucket.
    const w = new ReplayWindow();
    expect(w.admit(next, parts(1n, 0n, 5n), 7)).toBe(true);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(true);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(false);
  });

  it('A4 — the sliding BITMAP is scoped, not only the high-water', () => {
    // Red against: scoping `#generationHighWater` but not the contexts. The new
    // holder would inherit the departed holder's bitmap and its early frames
    // would read as DUPLICATES — a different symptom from A5, same cause.
    const w = new ReplayWindow();
    for (let seq = 5; seq <= 10; seq += 1) w.admit(g, parts(1n, 0n, 9n), seq);
    expect(w.admit(next, parts(1n, 0n, 9n), 5)).toBe(true);
  });

  it('A5 — the HIGH-WATER is scoped: transmit generation 0 is accepted in a new scope', () => {
    // Red against: the shipped unscoped high-water (S-1 at unit level). Its
    // accepting twin one field apart: the same frame in the SAME scope is refused.
    const w = new ReplayWindow();
    expect(w.admit(g, parts(1n, 0n, 100n), 1)).toBe(true);
    expect(w.admit(g, parts(1n, 0n, 0n), 1)).toBe(false);
    expect(w.admit(next, parts(1n, 0n, 0n), 1)).toBe(true);
  });

  it('A6 — within one scope, the high-water still survives context eviction', () => {
    // Red against: scoping that weakened the survive-eviction property.
    const w = new ReplayWindow(2, 64);
    expect(w.admit(next, parts(1n, 0n, 100n), 1)).toBe(true);
    for (let t = 101; t < 200; t += 1) w.admit(next, parts(1n, 0n, BigInt(t)), 1);
    expect(w.admit(next, parts(1n, 0n, 100n), 1)).toBe(false);
  });

  it('A8 — flooding a sender s NEW scope cannot evict its RETAINED scope', () => {
    // Red against: a context budget shared across scopes. That re-opens the F1
    // attack with `generation` in place of `stream`: flood g+1, evict g, replay a
    // captured g frame.
    const w = new ReplayWindow(4, 64);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(true);
    for (let t = 0; t < 500; t += 1) w.admit(next, parts(1n, BigInt(t % 256), BigInt(t)), t);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(false);
  });

  it('discardGeneration drops exactly that scope', () => {
    const w = new ReplayWindow();
    w.admit(g, parts(1n, 0n, 5n), 7);
    w.admit(next, parts(1n, 0n, 5n), 7);
    w.discardGeneration(g);
    expect(w.admit(g, parts(1n, 0n, 5n), 7)).toBe(true);
    expect(w.admit(next, parts(1n, 0n, 5n), 7)).toBe(false);
  });
});

describe('transmit key cache — one entry per key id (E-1) and F-8', () => {
  const kid = hexToBytes('0102030405060708');
  const wrap = new Uint8Array(48).fill(0x30);

  it('B1 (unit) — a set under a DIFFERENT generation is refused; the entry, its scope and its key are untouched', () => {
    // Red against: the pre-E-1 unconditional overwrite, which moves the scope.
    const c = new TransmitKeyCache();
    const held = new Uint8Array(32).fill(0x20);
    expect(c.set(unpackKeyId(kid), kid, held, wrap, 1)).toBe(true);
    const incoming = new Uint8Array(32).fill(0x20);
    expect(c.set(unpackKeyId(kid), kid, incoming, new Uint8Array(48).fill(0x31), 2)).toBe(false);
    expect(c.scopeOf(unpackKeyId(kid), kid)).toBe(1);
    expect(c.get(unpackKeyId(kid), kid)).toBe(held);
    expect(held.every((b) => b === 0x20)).toBe(true);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, wrap)).toBe(true);
    // The refused copy is not left lying around.
    expect(incoming.every((b) => b === 0)).toBe(true);
  });

  it('B5 — same generation, identical key: scope kept, no zeroization of the kept key', () => {
    // The part of the old no-zero-on-replace rule that stays true.
    const c = new TransmitKeyCache();
    const held = new Uint8Array(32).fill(0x20);
    c.set(unpackKeyId(kid), kid, held, wrap, 1);
    const duplicate = new Uint8Array(32).fill(0x20);
    const rewrap = new Uint8Array(48).fill(0x32);
    expect(c.set(unpackKeyId(kid), kid, duplicate, rewrap, 1)).toBe(true);
    expect(c.get(unpackKeyId(kid), kid)).toBe(held);
    expect(held.every((b) => b === 0x20)).toBe(true);
    expect(c.scopeOf(unpackKeyId(kid), kid)).toBe(1);
    expect(c.matchesCachedWrap(unpackKeyId(kid), kid, rewrap)).toBe(true);
  });

  it('B5 — same generation, DIFFERENT key: the superseded buffer is zeroized (defence in depth)', () => {
    // FORECLOSED for honest parties by ADR-0036 §4 invariant 1 plus transmit-key
    // monotonicity (two keys under one key id under one KEK). Kept so that, were
    // it ever reached, no superseded key survives un-zeroized. Not dead code.
    const c = new TransmitKeyCache();
    const old = new Uint8Array(32).fill(0x20);
    c.set(unpackKeyId(kid), kid, old, wrap, 1);
    const replacement = new Uint8Array(32).fill(0x40);
    expect(c.set(unpackKeyId(kid), kid, replacement, wrap, 1)).toBe(true);
    expect(old.every((b) => b === 0)).toBe(true);
    expect(c.get(unpackKeyId(kid), kid)).toBe(replacement);
  });

  it('B3 — purgeSender purges EVERY scope for that sender, zeroizing each, and no other sender', () => {
    // Red against: the narrow reading, "purge this sender in the current scope",
    // which leaves the DEPARTED holder's keys resident under the older, retained
    // generation. The assertion is therefore on the OLD scope — an assertion on
    // the current scope alone passes that bug.
    const c = new TransmitKeyCache();
    const oldScope = packKeyId({ senderId: 9n, stream: 0n, generation: 100n });
    const newScope = packKeyId({ senderId: 9n, stream: 0n, generation: 0n });
    const other = packKeyId({ senderId: 10n, stream: 0n, generation: 1n });
    const oldKey = new Uint8Array(32).fill(0x21);
    const newKey = new Uint8Array(32).fill(0x22);
    const otherKey = new Uint8Array(32).fill(0x23);
    c.set(unpackKeyId(oldScope), oldScope, oldKey, wrap, 1);
    c.set(unpackKeyId(newScope), newScope, newKey, wrap, 2);
    c.set(unpackKeyId(other), other, otherKey, wrap, 1);

    c.purgeSender(9n);

    expect(c.has(unpackKeyId(oldScope), oldScope)).toBe(false);
    expect(oldKey.every((b) => b === 0)).toBe(true);
    expect(c.has(unpackKeyId(newScope), newScope)).toBe(false);
    expect(newKey.every((b) => b === 0)).toBe(true);
    expect(c.has(unpackKeyId(other), other)).toBe(true);
    expect(otherKey.every((b) => b === 0x23)).toBe(true);
  });

  it('discardGeneration zeroizes and drops exactly that generation, for every sender', () => {
    const c = new TransmitKeyCache();
    const a = packKeyId({ senderId: 1n, stream: 0n, generation: 1n });
    const b = packKeyId({ senderId: 2n, stream: 0n, generation: 1n });
    const keep = packKeyId({ senderId: 1n, stream: 0n, generation: 2n });
    const ka = new Uint8Array(32).fill(0x31);
    const kb = new Uint8Array(32).fill(0x32);
    const kk = new Uint8Array(32).fill(0x33);
    c.set(unpackKeyId(a), a, ka, wrap, 1);
    c.set(unpackKeyId(b), b, kb, wrap, 1);
    c.set(unpackKeyId(keep), keep, kk, wrap, 2);

    c.discardGeneration(1);

    expect(c.has(unpackKeyId(a), a)).toBe(false);
    expect(c.has(unpackKeyId(b), b)).toBe(false);
    expect([...ka, ...kb].every((x) => x === 0)).toBe(true);
    expect(c.scopeOf(unpackKeyId(keep), keep)).toBe(2);
    expect(kk.every((x) => x === 0x33)).toBe(true);
  });
});
