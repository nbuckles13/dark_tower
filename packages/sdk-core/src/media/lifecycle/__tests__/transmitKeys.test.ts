// File: packages/sdk-core/src/media/lifecycle/__tests__/transmitKeys.test.ts
//
// THE (KEY, NONCE) REUSE INVARIANT.
//
// ADR-0036 §4 permits a stream-sequence reset only because the key id
// distinguishes generations. If a path ever existed where a sequence returned to
// zero while the generation did NOT advance, that is a repeated (key, nonce)
// pair under AES-GCM — which does not merely expose those two frames, it leaks
// the authentication subkey and permits forgery.
//
// These assertions are about REACHABILITY, not about a happy path: the point is
// that no sequence of public calls can produce the hazard.

import { describe, expect, it } from 'vitest';

import { AES_256_KEY_BYTES } from '../../frame/sframe.js';
import { bytesToHex } from '../../frame/hex.js';
import { unpackKeyId } from '../../frame/keyId.js';
import { JoinResponseKekSource } from '../../setup/kekSource.js';
import { TransmitKeyError, TransmitKeyManager } from '../transmitKeys.js';

const KEK = new Uint8Array(AES_256_KEY_BYTES).fill(0x11);

function kekFor(): { source: JoinResponseKekSource; generation: number } {
  const source = new JoinResponseKekSource();
  source.set(KEK, 3);
  return { source, generation: 3 };
}

describe('generation is monotonic per sender and is never reset', () => {
  it('only ever increases, and rotate() is unconditional', () => {
    const mgr = new TransmitKeyManager(258);
    expect(mgr.generation).toBe(0n);
    for (let i = 1; i <= 5; i += 1) {
      mgr.rotate();
      expect(mgr.generation).toBe(BigInt(i));
    }
    // `clear()` is teardown, not a reset: a manager that survived teardown with
    // a reset generation would be the hazard this module exists to prevent.
    mgr.clear();
    expect(mgr.generation).toBe(5n);
  });

  it('exposes no way to reset a stream sequence', () => {
    const mgr = new TransmitKeyManager(258) as unknown as Record<string, unknown>;
    // Asserted on the SURFACE rather than on behaviour: the property that
    // matters is that no caller can ask for a reset, not that today's callers
    // happen not to.
    for (const name of Object.getOwnPropertyNames(Object.getPrototypeOf(mgr))) {
      expect(name).not.toMatch(/reset|seek|rewind/i);
    }
  });

  it('keeps counting the stream sequence across a rotation', () => {
    const mgr = new TransmitKeyManager(258);
    expect(mgr.nextStreamSequence(1)).toBe(0);
    expect(mgr.nextStreamSequence(1)).toBe(1);
    mgr.rotate();
    // §2: keep counting regardless. A reset here would be legal under §4 but is
    // deliberately not taken, so a reset and a generation bump can never become
    // independently reachable by a later refactor.
    expect(mgr.nextStreamSequence(1)).toBe(2);
  });

  it('counts each stream independently', () => {
    const mgr = new TransmitKeyManager(258);
    expect(mgr.nextStreamSequence(1)).toBe(0);
    expect(mgr.nextStreamSequence(2)).toBe(0);
    expect(mgr.nextStreamSequence(1)).toBe(1);
  });
});

describe('key material', () => {
  it('packs the generation into the key id and mints a new key on rotation', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const first = await mgr.materialFor(1, kek);
    expect(unpackKeyId(first.keyId).generation).toBe(0n);
    expect(unpackKeyId(first.keyId).senderId).toBe(258n);
    expect(unpackKeyId(first.keyId).stream).toBe(1n);

    mgr.rotate();
    const second = await mgr.materialFor(1, kek);
    expect(unpackKeyId(second.keyId).generation).toBe(1n);
    expect([...second.key]).not.toEqual([...first.key]);
  });

  it('wraps ONCE per generation — the block is byte-identical between frames', async () => {
    // ADR-0036 §4: "One transmit key therefore wraps to one ciphertext —
    // byte-identical from frame to frame within a generation." That determinism
    // is what the RECEIVER's cached-wrap skip depends on; re-wrapping per frame
    // would be correct but would make every receiver unwrap on every frame.
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const a = await mgr.materialFor(1, kek);
    const b = await mgr.materialFor(1, kek);
    expect(a.wrapped.wrappedKeyWithTag).toBe(b.wrapped.wrappedKeyWithTag);
  });

  it('fails loudly when no meeting KEK is held rather than sending unannounced', async () => {
    const mgr = new TransmitKeyManager(258);
    const empty = { source: new JoinResponseKekSource(), generation: 0 };
    await expect(mgr.materialFor(1, empty)).rejects.toBeInstanceOf(TransmitKeyError);
  });

  it('refuses a sender id of 0 — a shared zero is N colliding key ids', () => {
    expect(() => new TransmitKeyManager(0)).toThrow(TransmitKeyError);
  });

  it('overwrites key buffers on clear', async () => {
    const mgr = new TransmitKeyManager(258);
    const material = await mgr.materialFor(1, kekFor());
    expect(material.key.some((b) => b !== 0)).toBe(true);
    mgr.clear();
    // ADR-0028 §5 names the PRACTICE, not a guarantee: JavaScript cannot promise
    // no engine-internal copy survives. This is the cheap place to honour it.
    expect(material.key.every((b) => b === 0)).toBe(true);
  });
});

describe('minting is single-flight, because two mints for one key id is (key, nonce) reuse', () => {
  // SEQUENTIAL TESTS CANNOT SEE THIS. Every other case in this file drives
  // `materialFor` one call at a time, and the suite was green with the defect
  // present. `AudioPipeline` calls `submit` fire-and-forget from the encoder's
  // output callback, so concurrent entry is the normal case, and the mint path
  // is entered at EVERY rotation boundary — every T, every unmute, every
  // resume-from-empty.
  it('gives concurrent callers ONE key, ONE key id and ONE wrap', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const [a, b, c] = await Promise.all([
      mgr.materialFor(1, kek),
      mgr.materialFor(1, kek),
      mgr.materialFor(1, kek),
    ]);
    expect([...a!.keyId]).toEqual([...b!.keyId]);
    expect([...a!.key]).toEqual([...b!.key]);
    expect([...a!.key]).toEqual([...c!.key]);
    // The wrap is the observable the RECEIVER depends on: `matchesCachedWrap`
    // skips the unwrap only when the block is byte-identical.
    expect([...a!.wrapped.wrappedKeyWithTag]).toEqual([...b!.wrapped.wrappedKeyWithTag]);
  });

  it('never produces two distinct wraps under one key id', async () => {
    // The negative that names the hazard, so a refactor reintroducing it fails
    // with the reason attached rather than as a bare inequality.
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const materials = await Promise.all(Array.from({ length: 8 }, () => mgr.materialFor(1, kek)));
    const keyIds = new Set(materials.map((m) => bytesToHex(m.keyId)));
    const wraps = new Set(materials.map((m) => bytesToHex(m.wrapped.wrappedKeyWithTag)));
    expect(keyIds.size).toBe(1);
    expect(
      wraps.size,
      'two wraps under one key id means two AES-GCM seals under the same (KEK, nonce) with ' +
        'different plaintexts — ADR-0036 §4: catastrophic, yielding authentication-key recovery. ' +
        'The nonce is a pure function of the key id, so the mint MUST be single-flight per stream.',
    ).toBe(1);
  });

  it('mints per stream independently, so the guard does not over-serialise', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const [one, two] = await Promise.all([mgr.materialFor(1, kek), mgr.materialFor(2, kek)]);
    expect(bytesToHex(one!.keyId)).not.toBe(bytesToHex(two!.keyId));
    expect([...one!.key]).not.toEqual([...two!.key]);
  });

  it('does not wedge the stream when a mint rejects', async () => {
    const mgr = new TransmitKeyManager(258);
    const empty = { source: new JoinResponseKekSource(), generation: 0 };
    await expect(mgr.materialFor(1, empty)).rejects.toBeInstanceOf(TransmitKeyError);
    // The in-flight slot must be released on rejection, or the stream is dead
    // for the rest of the session once a KEK arrives.
    await expect(mgr.materialFor(1, kekFor())).resolves.toBeDefined();
  });
});

describe('a rotation is not undone by a mint that was already in flight', () => {
  it('does not install material minted at a superseded generation', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    // Start a mint, rotate before it settles, then let it finish.
    const inFlight = mgr.materialFor(1, kek);
    mgr.rotate();
    const stale = await inFlight;

    // Its caller still gets material — that frame was committed to the
    // pre-rotation generation before the rotation happened, and emitting it is
    // self-consistent and monotonic.
    expect(unpackKeyId(stale.keyId).generation).toBe(0n);

    // But the rotation TOOK EFFECT: the next mint is at the new generation, not
    // the reinstated old one. Otherwise the leaked-key window `rotate()` exists
    // to bound would not be bounded.
    const next = await mgr.materialFor(1, kek);
    expect(unpackKeyId(next.keyId).generation).toBe(1n);
    expect([...next.key]).not.toEqual([...stale.key]);
  });

  it('lets no later caller join a mint orphaned by a rotation', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const orphaned = mgr.materialFor(1, kek);
    mgr.rotate();
    const fresh = await mgr.materialFor(1, kek);
    const stale = await orphaned;
    expect(unpackKeyId(fresh.keyId).generation).toBe(1n);
    expect(unpackKeyId(stale.keyId).generation).toBe(0n);
  });

  it('does NOT overwrite the just-superseded key, but DOES overwrite it later', async () => {
    // A transmit key is read AFTER AN AWAIT by the frame being sealed with it
    // (`deriveSframeKeys` -> `hkdfExtract` -> `hmac`, which reads the material at
    // its `sign()` call, downstream of `await importKey`). Zeroing at the moment
    // of supersession would seal that frame under zeros while its wrap announces
    // the real key — a hygiene measure presenting as `decrypt_failed`, whose
    // triage points at the key schedule or the sender.
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const gen0 = await mgr.materialFor(1, kek);

    mgr.rotate();
    expect(
      gen0.key.some((b) => b !== 0),
      'the just-superseded key must survive one rotation period, because a frame may still be ' +
        'mid-derivation with it',
    ).toBe(true);

    // One more rotation, and the deferred overwrite lands.
    await mgr.materialFor(1, kek);
    mgr.rotate();
    expect(gen0.key.every((b) => b === 0)).toBe(true);
  });

  it('overwrites everything still held at teardown, retired or current', async () => {
    const mgr = new TransmitKeyManager(258);
    const kek = kekFor();
    const gen0 = await mgr.materialFor(1, kek);
    mgr.rotate();
    const gen1 = await mgr.materialFor(1, kek);
    mgr.clear();
    expect(gen0.key.every((b) => b === 0)).toBe(true);
    expect(gen1.key.every((b) => b === 0)).toBe(true);
  });
});
