// File: packages/sdk-core/src/media/frame/__tests__/receivePathIntegration.test.ts
//
// The receive path driven end to end over frames this test builds, so the arms
// the pinned vectors never reach — key-layer rejects, the audio re-send cadence,
// non-key-bearing frames — are exercised rather than assumed.

import { describe, expect, it } from 'vitest';

import { bytesToHex, hexToBytes } from '../hex.js';
import { buildUnsignedFrame, decodeFrame, finishFrame } from '../frameCodec.js';
import { exportPublicKey, importSigningKeyFromSeed, signFrame } from '../ed25519.js';
import { packKeyId, unpackKeyId } from '../keyId.js';
import {
  SFRAME_CIPHER_SUITE_ID,
  aesGcmSeal,
  sealSframe,
  serializeSframe,
  wrapNonce,
} from '../sframe.js';
import { deriveSframeKeys, sframeNonce } from '../sframeKeySchedule.js';
import { codecReject, cryptoReject, keyReject } from '../rejectReason.js';
import {
  ReplayWindow,
  TransmitKeyCache,
  openVerifiedFrame,
  verifyFrame,
  type ReceiverKeys,
} from '../receivePath.js';

const SEED = hexToBytes('404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f');
const KEK = new Uint8Array(32).fill(0x10);
const TRANSMIT = new Uint8Array(32).fill(0x20);
const KEK_GENERATION = 1;

/** Build a complete, correctly signed frame. */
async function makeFrame(
  over: {
    senderId?: bigint;
    stream?: bigint;
    generation?: bigint;
    streamSequence?: number;
    keyBearing?: boolean;
    transmitKey?: Uint8Array;
    wrapUnderKeyId?: Uint8Array;
    plaintext?: string;
    /** KEK generation announced on the wrap. Defaults to `KEK_GENERATION`. */
    kekGeneration?: number;
    /** KEK the transmit key is wrapped under. Defaults to `KEK`. */
    kek?: Uint8Array;
    /** Ed25519 signing seed: the sender's IDENTITY. Defaults to `SEED`. */
    seed?: Uint8Array;
  } = {},
) {
  const senderId = over.senderId ?? 258n;
  const keyId = packKeyId({
    senderId,
    stream: over.stream ?? 3n,
    generation: over.generation ?? 0x0405060708n,
  });
  const transmitKey = over.transmitKey ?? TRANSMIT;
  const streamSequence = over.streamSequence ?? 7;
  const keyBearing = over.keyBearing ?? true;

  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey: transmitKey,
    kid: keyId,
    cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
    keyBytes: 32,
    saltBytes: 12,
  });

  let wrappedTransmitKey = null;
  if (keyBearing) {
    // The wrap is bound to `wrapUnderKeyId` if supplied — which is how a
    // mis-bound wrap is constructed, since nothing on the wire declares the
    // binding.
    const bindTo = over.wrapUnderKeyId ?? keyId;
    wrappedTransmitKey = {
      kekGeneration: over.kekGeneration ?? KEK_GENERATION,
      wrappedKeyWithTag: await aesGcmSeal({
        key: over.kek ?? KEK,
        nonce: wrapNonce(bindTo),
        aad: bindTo,
        plaintext: transmitKey,
      }),
    };
  }

  // The publisher region must be complete before the AAD is taken, so the frame
  // is built in two passes: once to obtain the AAD, then again with the sealed
  // payload. The payload length is identical across both because GCM is
  // length-preserving, so the AAD from the first pass is the AAD of the second.
  const plaintext = new TextEncoder().encode(over.plaintext ?? 'dark-tower');
  const skeleton = buildUnsignedFrame({
    flags: { independentlyDecodable: true, discardable: false, keyBearing },
    streamSequence,
    wrappedTransmitKey,
    extensions: [],
    streamId: 2,
    hopSequence: 11,
    payload: new Uint8Array(8 + 16 + plaintext.length),
  });

  const sealed = await sealSframe({
    key: derived.key,
    nonce: sframeNonce(derived.salt, BigInt(streamSequence)),
    aad: skeleton.aeadAad,
    plaintext,
    keyId,
  });

  const unsigned = buildUnsignedFrame({
    flags: { independentlyDecodable: true, discardable: false, keyBearing },
    streamSequence,
    wrappedTransmitKey,
    extensions: [],
    streamId: 2,
    hopSequence: 11,
    payload: serializeSframe(sealed),
  });
  const signingKey = await importSigningKeyFromSeed(over.seed ?? SEED);
  return {
    bytes: finishFrame(unsigned, await signFrame(signingKey, unsigned.signedRange)),
    publicKey: await exportPublicKey(signingKey),
    keyId,
    plaintext,
  };
}

const keysWithKek: ReceiverKeys = {
  kekForGeneration: (g) => (g === KEK_GENERATION ? KEK : undefined),
  isOlderThanRetained: (g) => g < KEK_GENERATION,
};
// Nothing held at all: every unheld generation is the NEWER case, because there
// is no current generation for it to be older than.
const keysWithoutKek: ReceiverKeys = {
  kekForGeneration: () => undefined,
  isOlderThanRetained: () => false,
};

describe('the happy path', () => {
  it('decodes, verifies, unwraps, caches and decrypts', async () => {
    const f = await makeFrame();
    const verified = await verifyFrame(decodeFrame(f.bytes), f.publicKey);
    const cache = new TransmitKeyCache();
    const opened = await openVerifiedFrame(verified, cache, keysWithKek);
    expect(opened.wrapOutcome).toBe('cached');
    expect(bytesToHex(opened.plaintext)).toBe(bytesToHex(f.plaintext));
    expect(opened.senderId).toBe(258n);
  });

  it('does no per-frame unwrap once the key is held', async () => {
    // ADR-0036 §4:413. Audio carries the wrap on EVERY frame, and §4 guarantees
    // the block is byte-identical within a generation — so the second frame takes
    // the comparison arm, not the AEAD.
    const cache = new TransmitKeyCache();
    const first = await makeFrame({ streamSequence: 1 });
    await openVerifiedFrame(
      await verifyFrame(decodeFrame(first.bytes), first.publicKey),
      cache,
      keysWithKek,
    );
    const second = await makeFrame({ streamSequence: 2 });
    const opened = await openVerifiedFrame(
      await verifyFrame(decodeFrame(second.bytes), second.publicKey),
      cache,
      keysWithKek,
    );
    expect(opened.wrapOutcome).toBe('already_held');
  });

  it('plays a non-key-bearing frame from a cached key', async () => {
    const cache = new TransmitKeyCache();
    const bearing = await makeFrame({ streamSequence: 1 });
    await openVerifiedFrame(
      await verifyFrame(decodeFrame(bearing.bytes), bearing.publicKey),
      cache,
      keysWithKek,
    );
    const plain = await makeFrame({ streamSequence: 2, keyBearing: false });
    const opened = await openVerifiedFrame(
      await verifyFrame(decodeFrame(plain.bytes), plain.publicKey),
      cache,
      keysWithKek,
    );
    expect(opened.wrapOutcome).toBe('absent');
  });
});

describe('key-layer rejects', () => {
  it('drops a frame whose KEK generation it cannot resolve', async () => {
    const f = await makeFrame();
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        new TransmitKeyCache(),
        keysWithoutKek,
      ),
    ).rejects.toMatchObject({ rejectReason: 'no_kek_for_generation' });
  });

  it('plays through a KEK rotation it has not received, if it already holds the key', async () => {
    // The transient §4 describes at join and at rotation. Dropping a playable
    // frame because a key we do not need is unavailable would turn a benign race
    // into an audio gap.
    const cache = new TransmitKeyCache();
    const first = await makeFrame({ streamSequence: 1 });
    await openVerifiedFrame(
      await verifyFrame(decodeFrame(first.bytes), first.publicKey),
      cache,
      keysWithKek,
    );
    // The same key id re-wrapped under the NEXT generation, which this receiver
    // has not received. It still holds the generation the key was unwrapped
    // under (the key id's scope): a receiver holding NO generation at all cannot
    // play off the cache, because a scope whose generation left retention never
    // opens a frame (story 2 C-3).
    const second = await makeFrame({
      streamSequence: 2,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
    });
    const opened = await openVerifiedFrame(
      await verifyFrame(decodeFrame(second.bytes), second.publicKey),
      cache,
      keysWithKek,
    );
    expect(bytesToHex(opened.plaintext)).toBe(bytesToHex(second.plaintext));
    // The condition must be OBSERVABLE, not collapsed into `'absent'` (which
    // means "no wrap at all"): a wrap was present under a generation we do not
    // hold, we played off the cache, and it is counted. A sustained rate means a
    // sender re-wrapping rather than rotating — see the `WrapOutcome` doc.
    expect(opened.wrapOutcome).toBe('kek_generation_not_held');
  });

  it('drops a non-key-bearing frame for an unknown key id', async () => {
    // ADR-0036 §4: under the cadence this cannot occur — audio carries the key on
    // every frame and a video subscriber enters at a group start. The only paths
    // to it are defects, so it lands in the decode-reject bucket where firing
    // means an invariant broke rather than a state the system passes through.
    const f = await makeFrame({ keyBearing: false });
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        new TransmitKeyCache(),
        keysWithKek,
      ),
    ).rejects.toMatchObject({ rejectReason: 'no_transmit_key' });
  });

  it('drops when the wrap does not open and nothing is cached', async () => {
    const f = await makeFrame({ wrapUnderKeyId: hexToBytes('0000000000000009') });
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        new TransmitKeyCache(),
        keysWithKek,
      ),
    ).rejects.toMatchObject({ rejectReason: 'unwrap_failed' });
  });
});

describe('an unheld KEK generation: accepted vs dropped, and newer vs older', () => {
  // A receiver that has moved PAST this suite's frames: it holds generations
  // above `KEK_GENERATION` and retains nothing at or below it.
  const keysMovedOn: ReceiverKeys = {
    kekForGeneration: () => undefined,
    isOlderThanRetained: (g) => g < KEK_GENERATION + 4,
  };

  it('drops as kek_generation_stale when the generation is older than retention keeps', async () => {
    const f = await makeFrame();
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        new TransmitKeyCache(),
        keysMovedOn,
      ),
    ).rejects.toMatchObject({ rejectReason: 'kek_generation_stale', layer: 'key' });
  });

  it('keeps no_kek_for_generation for the NEWER case, on the same frame bytes', async () => {
    // Same frame, same receiver shape, only the direction flipped: the token
    // follows receiver state, never anything in the frame.
    const f = await makeFrame();
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        new TransmitKeyCache(),
        { kekForGeneration: () => undefined, isOlderThanRetained: () => false },
      ),
    ).rejects.toMatchObject({ rejectReason: 'no_kek_for_generation' });
  });

  it('ACCEPTS an OLDER unheld generation when the transmit key is cached', async () => {
    // One key id resolves to one scope — the generation it was unwrapped under —
    // so if it arrives under a second wrap announcing an OLDER unheld
    // generation, the frame still plays off the cache. Direction splits only the
    // DROP tokens. NOT an honest-sender pattern for this SDK (it rotates to a new
    // key id on every KEK change, R-13); this pins the receiver's behaviour for a
    // sender that re-wraps instead.
    const cache = new TransmitKeyCache();
    const first = await makeFrame({ streamSequence: 1 });
    await openVerifiedFrame(
      await verifyFrame(decodeFrame(first.bytes), first.publicKey),
      cache,
      keysWithKek,
    );
    // Wrapped under the generation BELOW the key id's scope, which this
    // receiver retains nothing of; it still holds the scope's own generation.
    const second = await makeFrame({
      streamSequence: 2,
      kekGeneration: KEK_GENERATION - 1,
      kek: KEK_NEXT,
    });
    const opened = await openVerifiedFrame(
      await verifyFrame(decodeFrame(second.bytes), second.publicKey),
      cache,
      keysWithKek,
    );
    expect(bytesToHex(opened.plaintext)).toBe(bytesToHex(second.plaintext));
    expect(opened.wrapOutcome).toBe('kek_generation_not_held');
  });
});

describe('TransmitKeyCache.purgeSender', () => {
  it('zeroizes and drops ONE sender only, leaving every other sender cached', async () => {
    // Both senders' keys enter the cache the only way production allows: through
    // a VERIFIED open. No test primes the cache through `set()`.
    const cache = new TransmitKeyCache();
    const a = await makeFrame({ senderId: 258n, streamSequence: 1 });
    const b = await makeFrame({ senderId: 259n, streamSequence: 1 });
    const va = await verifyFrame(decodeFrame(a.bytes), a.publicKey);
    const vb = await verifyFrame(decodeFrame(b.bytes), b.publicKey);
    await openVerifiedFrame(va, cache, keysWithKek);
    await openVerifiedFrame(vb, cache, keysWithKek);
    const heldA = cache.get(va.keyIdParts, va.keyId);
    const heldB = cache.get(vb.keyIdParts, vb.keyId);
    expect(heldA).toBeDefined();
    expect(heldB).toBeDefined();

    cache.purgeSender(va.keyIdParts.senderId);

    expect(cache.has(va.keyIdParts, va.keyId)).toBe(false);
    // Overwritten, not merely unreferenced: the buffer we held is now zeros.
    expect(heldA!.every((byte) => byte === 0)).toBe(true);
    // The other sender is untouched, bytes and all.
    expect(cache.has(vb.keyIdParts, vb.keyId)).toBe(true);
    expect(heldB!.some((byte) => byte !== 0)).toBe(true);
  });
});

describe('crypto-layer rejects', () => {
  it('fails verification against a different sender key', async () => {
    const f = await makeFrame();
    const otherSeed = hexToBytes(
      '505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f',
    );
    const other = await exportPublicKey(await importSigningKeyFromSeed(otherSeed));
    await expect(verifyFrame(decodeFrame(f.bytes), other)).rejects.toMatchObject({
      rejectReason: 'signature_invalid',
    });
  });

  it('fails decryption when the cached transmit key is wrong', async () => {
    const cache = new TransmitKeyCache();
    const f = await makeFrame({ keyBearing: false });
    cache.set(
      unpackKeyId(f.keyId),
      f.keyId,
      new Uint8Array(32).fill(0x77),
      new Uint8Array(48),
      KEK_GENERATION,
    );
    await expect(
      openVerifiedFrame(await verifyFrame(decodeFrame(f.bytes), f.publicKey), cache, keysWithKek),
    ).rejects.toMatchObject({ rejectReason: 'decrypt_failed' });
  });

  it('rejects a replay through the composed path', async () => {
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const f = await makeFrame();
    await openVerifiedFrame(
      await verifyFrame(decodeFrame(f.bytes), f.publicKey),
      cache,
      keysWithKek,
      replay,
    );
    await expect(
      openVerifiedFrame(
        await verifyFrame(decodeFrame(f.bytes), f.publicKey),
        cache,
        keysWithKek,
        replay,
      ),
    ).rejects.toMatchObject({ rejectReason: 'replay_detected' });
  });
});

describe('reject constructors', () => {
  it('tag each error with its layer', () => {
    expect(codecReject('truncated', 'x').layer).toBe('codec');
    expect(cryptoReject('decrypt_failed', 'x').layer).toBe('crypto');
    expect(keyReject('no_roster_entry', 'x').layer).toBe('key');
    expect(keyReject('no_roster_entry', 'x').rejectReason).toBe('no_roster_entry');
  });
});

// ---------------------------------------------------------------------------
// Story 2 R-16 under ruling (A'): receiver state scoped by (kek_generation,
// sender_id). Case ids (D1, B1′, B2, ...) are the ones named in the devloop plan
// (`docs/devloop-outputs/2026-09-26-mc-kek-lifecycle/main.md`, "sdk-core test
// cases"). Each case records the MUTATION it goes red against.
// ---------------------------------------------------------------------------

/** A second identity: the NEW holder of a reissued sender id. */
const SEED_B = hexToBytes('606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f');
/** The KEK for generation `KEK_GENERATION + 1`. */
const KEK_NEXT = new Uint8Array(32).fill(0x11);
const TRANSMIT_B = new Uint8Array(32).fill(0x21);

/**
 * A receiver holding exactly the generations in `held`. Anything below the
 * newest held generation and not itself held is "older than retained".
 */
function holding(held: ReadonlyMap<number, Uint8Array>): ReceiverKeys {
  const newest = Math.max(...held.keys());
  return {
    kekForGeneration: (g) => held.get(g),
    isOlderThanRetained: (g) => !held.has(g) && g < newest,
  };
}

const BOTH_GENERATIONS = new Map([
  [KEK_GENERATION, KEK],
  [KEK_GENERATION + 1, KEK_NEXT],
]);

async function open(
  f: Awaited<ReturnType<typeof makeFrame>>,
  cache: TransmitKeyCache,
  keys: ReceiverKeys,
  replay: ReplayWindow,
) {
  return openVerifiedFrame(
    await verifyFrame(decodeFrame(f.bytes), f.publicKey),
    cache,
    keys,
    replay,
  );
}

describe('S-1: a reissued sender id after an epoch reset', () => {
  it('D1 — ACCEPTS the new holder at transmit generation 0 under the new KEK generation', async () => {
    // Red against: the shipped unscoped high-water. The departed holder drove
    // (sender 258, stream 3) to transmit generation 100; the new holder of the
    // same id starts at 0 and, unscoped, is dropped as a replay FOREVER.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);

    const departed = await makeFrame({ generation: 100n, streamSequence: 5 });
    await expect(open(departed, cache, keys, replay)).resolves.toBeDefined();

    // The departed holder's ParticipantLeft: the roster forgets it and the
    // resolver purges its transmit keys (story 2 R-18). Replay state is KEPT.
    cache.purgeSender(258n);

    const reissued = await makeFrame({
      seed: SEED_B,
      transmitKey: TRANSMIT_B,
      generation: 0n,
      streamSequence: 1,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
      plaintext: 'new-holder',
    });
    const opened = await open(reissued, cache, keys, replay);
    expect(new TextDecoder().decode(opened.plaintext)).toBe('new-holder');
  });
});

describe('E-1: one key id resolves to one scope', () => {
  it('B2 — a replayed wrapless frame does NOT reach a bucket that never saw it after a cross-generation re-wrap', async () => {
    // Red against: scoping WITH the pre-E-1 unconditional `cache.set`, which
    // drags the key id's provenance to the re-wrap's generation, so the replay
    // resolves to a fresh bucket and is admitted — valid signature, valid tag.
    // (NOT red against the shipped unscoped code: there the replay window is not
    // scoped at all, so the drag has nothing to drag the frame into. The defect
    // is latent in shipped code and live only once scoping lands.)
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);

    await open(await makeFrame({ streamSequence: 1 }), cache, keys, replay);
    const wrapless = await makeFrame({ streamSequence: 2, keyBearing: false });
    // Positive control: the frame IS admitted once, so the rejection below is a
    // replay rejection and not a receiver that rejects everything.
    await expect(open(wrapless, cache, keys, replay)).resolves.toBeDefined();

    // The SAME transmit key re-wrapped under the next generation (a sender that
    // re-wraps instead of rotating — what R-13 ends). Plays either way.
    const rewrap = await makeFrame({
      streamSequence: 3,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
    });
    await expect(open(rewrap, cache, keys, replay)).resolves.toBeDefined();

    await expect(open(wrapless, cache, keys, replay)).rejects.toMatchObject({
      rejectReason: 'replay_detected',
    });
  });

  it('B1′(i) — a NEW sender s first key id under a newer held generation is cached, not refused', async () => {
    // Red against: "refuse any wrap newer than the newest cached scope" (m1),
    // which breaks every joiner after the first rotation.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    await open(await makeFrame({ streamSequence: 1 }), cache, keys, replay);

    const joiner = await makeFrame({
      senderId: 259n,
      seed: SEED_B,
      transmitKey: TRANSMIT_B,
      streamSequence: 1,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
    });
    const opened = await open(joiner, cache, keys, replay);
    expect(opened.wrapOutcome).toBe('cached');
  });

  it('B1′(ii) — the SAME sender rotating to a new key id under a newer held generation is cached, not refused', async () => {
    // Red against: "refuse a newer-generation wrap for a SENDER with any cached
    // entry" (m2). (i) cannot see m2 — a new sender has no entry — and m2
    // silently breaks every honest R-13 rotation, which is exactly this frame.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    await open(await makeFrame({ streamSequence: 1 }), cache, keys, replay);

    const rotated = await makeFrame({
      generation: 0x0405060709n,
      transmitKey: TRANSMIT_B,
      streamSequence: 2,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
      plaintext: 'rotated',
    });
    const opened = await open(rotated, cache, keys, replay);
    expect(opened.wrapOutcome).toBe('cached');
    expect(new TextDecoder().decode(opened.plaintext)).toBe('rotated');

    // The old key id's entry is untouched: a second key id was ADDED, not
    // substituted. (Asserted on cache state, not by opening an old-key frame: that
    // would couple this case to cross-generation replay admission, which C7 owns.)
    const oldKeyId = (await makeFrame()).keyId;
    expect(cache.has(unpackKeyId(oldKeyId), oldKeyId)).toBe(true);
  });
});

describe('E-1 — the refusal itself', () => {
  it('B1 — a cross-generation re-wrap of a cached key id is refused; entry, scope and key untouched; the frame plays', async () => {
    // Red against: the pre-E-1 unconditional `cache.set` (outcome `cached`,
    // scope dragged to the re-wrap's generation, key object replaced).
    //
    // "Not unwrapped into a second scope" is asserted through its CONSEQUENCES —
    // same scope, same key object — not by counting KEK lookups: the refusal
    // path legitimately QUERIES the announced generation (to fork conflict from
    // not-held), so a lookup count cannot separate "queried" from "unwrapped".
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    const first = await makeFrame({ streamSequence: 1 });
    await open(first, cache, keys, replay);
    const parts = unpackKeyId(first.keyId);
    const heldKey = cache.get(parts, first.keyId);

    const rewrap = await makeFrame({
      streamSequence: 2,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
      plaintext: 'still-plays',
    });
    const opened = await open(rewrap, cache, keys, replay);

    expect(opened.wrapOutcome).toBe('wrap_generation_conflict');
    expect(new TextDecoder().decode(opened.plaintext)).toBe('still-plays');
    expect(cache.scopeOf(parts, first.keyId)).toBe(KEK_GENERATION);
    expect(cache.get(parts, first.keyId)).toBe(heldKey);
  });

  it('B4 — a roster purge keeps replay state: a re-sent key-bearing frame is a replay, and caches nothing', async () => {
    // Red against: a purge that also clears replay state (rebind-BACK replay),
    // or a replay that re-populates the cache the purge emptied.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    const f = await makeFrame({ streamSequence: 4 });
    await open(f, cache, keys, replay);
    cache.purgeSender(258n);
    await expect(open(f, cache, keys, replay)).rejects.toMatchObject({
      rejectReason: 'replay_detected',
    });
    expect(cache.has(unpackKeyId(f.keyId), f.keyId)).toBe(false);
  });
});

describe('provenance: a captured frame lands in the bucket that saw it (D2, C7)', () => {
  /** A departed/rotating sender's frames under generation 1 at transmit gen 7. */
  const underG = (seq: number) => makeFrame({ generation: 7n, streamSequence: seq });
  /** The same sender after rotating (R-13): transmit gen 8 under generation 2. */
  const underNext = (seq: number) =>
    makeFrame({
      generation: 8n,
      transmitKey: TRANSMIT_B,
      streamSequence: seq,
      kekGeneration: KEK_GENERATION + 1,
      kek: KEK_NEXT,
    });

  it('C7 — a g frame reordered after the first g+1 frame is ACCEPTED while g is retained', async () => {
    // Red against: the shipped unscoped high-water, which dropped it the moment
    // the sender's g+1 frame landed — negating what ADR-0036 §4 buys retention
    // for ("frames in flight across a rotation").
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    await open(await underG(1), cache, keys, replay);
    await open(await underNext(3), cache, keys, replay);
    await expect(open(await underG(2), cache, keys, replay)).resolves.toBeDefined();
  });

  it('C7 twin / D2 — the SAME frame replayed after g+1 began is still replay_detected from g s bucket', async () => {
    // Red against: scope recomputed from the CURRENT generation at admit time,
    // which drops the replay into g+1's bucket (fresh for this key id) and
    // admits it. Also red against a scoping that broke the replay check and
    // admits anything under an old generation — which C7 alone cannot tell apart.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const keys = holding(BOTH_GENERATIONS);
    const captured = await underG(1);
    await open(captured, cache, keys, replay);
    await open(await underNext(2), cache, keys, replay);
    await expect(open(captured, cache, keys, replay)).rejects.toMatchObject({
      rejectReason: 'replay_detected',
    });
  });

  it('D2 / C7 expired — after g leaves retention the same replay is kek_generation_stale, never a fresh bucket', async () => {
    // The token is the assertion: `kek_generation_stale` proves the frame was
    // refused BEFORE replay state was consulted, so no discarded scope is
    // re-created. The discard is what the pipeline's one listener does on the
    // holder's generation-dropped event (see the lifecycle suite for that wiring).
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const captured = await underG(1);
    await open(captured, cache, holding(BOTH_GENERATIONS), replay);
    await open(await underNext(2), cache, holding(BOTH_GENERATIONS), replay);

    replay.discardGeneration(KEK_GENERATION);
    cache.discardGeneration(KEK_GENERATION);
    const onlyNext = holding(new Map([[KEK_GENERATION + 1, KEK_NEXT]]));

    await expect(open(captured, cache, onlyNext, replay)).rejects.toMatchObject({
      rejectReason: 'kek_generation_stale',
    });
    const wrapless = await makeFrame({ generation: 7n, streamSequence: 5, keyBearing: false });
    await expect(open(wrapless, cache, onlyNext, replay)).rejects.toMatchObject({
      rejectReason: 'no_transmit_key',
    });
  });
});

describe('retention expiry DURING frame processing (@security Gate-3 check a)', () => {
  /**
   * A receiver whose `kekForGeneration` lookup itself expires the generation and
   * discards its scope — exactly what a wired `MeetingKekHolder` does, since the
   * lookup is its lazy backstop and the drop event fires the pipeline's listener
   * synchronously inside it. Reproduced here without the pipeline so the receive
   * path's own ordering is what is under test.
   */
  function expiringOnLookup(cache: TransmitKeyCache, replay: ReplayWindow, expire: number) {
    let held = new Map(BOTH_GENERATIONS);
    return {
      kekForGeneration(generation: number) {
        // EXPIRE FIRST, THEN LOOK UP — the order `MeetingKekHolder` uses
        // (`#expireIfDue` runs ahead of the `find`), which is why it can never
        // return a KEK for a generation whose scope it has just discarded.
        if (held.has(expire)) {
          held = new Map([...held].filter(([g]) => g !== expire));
          replay.discardGeneration(expire);
          cache.discardGeneration(expire);
        }
        return held.get(generation);
      },
      isOlderThanRetained: (g: number) => !held.has(g) && g < Math.max(...held.keys()),
    } satisfies ReceiverKeys;
  }

  it('reports kek_generation_stale, NOT decrypt_failed, for a frame whose scope expires mid-processing', async () => {
    // A wrapless frame carries no generation, so the CACHED scope is the only
    // thing that can name one. Captured before the lookup and used for the drop,
    // it yields `kek_generation_stale`; re-read after the lookup it would be gone
    // and the frame would report `no_transmit_key`, downgrading a retention
    // expiry to a protocol violation. The token is the whole assertion.
    //
    // ON THE ZEROIZE-THEN-OPEN HAZARD @security asked about: it is NOT reachable
    // through `MeetingKekHolder`, because its lazy expiry runs BEFORE the lookup,
    // so it cannot return a usable KEK for a generation whose entry the listener
    // just zeroized (the fake above copies that order deliberately). The
    // re-read-after-the-check in `openVerifiedFrame` is therefore defence for any
    // OTHER `ReceiverKeys` implementation reaching the seam, not a live bug — and
    // it is what keeps the drop token honest if one ever does.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const primed = await makeFrame({ streamSequence: 1 });
    await open(primed, cache, holding(BOTH_GENERATIONS), replay);
    expect(cache.scopeOf(unpackKeyId(primed.keyId), primed.keyId)).toBe(KEK_GENERATION);

    const wrapless = await makeFrame({ streamSequence: 2, keyBearing: false });
    await expect(
      open(wrapless, cache, expiringOnLookup(cache, replay, KEK_GENERATION), replay),
    ).rejects.toMatchObject({ rejectReason: 'kek_generation_stale', layer: 'key' });
  });

  it('leaves no replay state behind for a scope discarded mid-processing', async () => {
    // The C-3 "never leaked" half at the frame level: a frame refused because its
    // scope expired must not have created a bucket in the scope that was just
    // discarded — nothing would ever discard it again.
    const cache = new TransmitKeyCache();
    const replay = new ReplayWindow();
    const primed = await makeFrame({ streamSequence: 1 });
    await open(primed, cache, holding(BOTH_GENERATIONS), replay);
    const wrapless = await makeFrame({ streamSequence: 2, keyBearing: false });
    await expect(
      open(wrapless, cache, expiringOnLookup(cache, replay, KEK_GENERATION), replay),
    ).rejects.toMatchObject({ rejectReason: 'kek_generation_stale' });

    // A fresh window would admit sequence 2 in that scope; so must this one, if
    // the refused frame recorded nothing.
    expect(replay.admit(KEK_GENERATION, unpackKeyId(wrapless.keyId), 2)).toBe(true);
  });
});
