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
      kekGeneration: KEK_GENERATION,
      wrappedKeyWithTag: await aesGcmSeal({
        key: KEK,
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
  const signingKey = await importSigningKeyFromSeed(SEED);
  return {
    bytes: finishFrame(unsigned, await signFrame(signingKey, unsigned.signedRange)),
    publicKey: await exportPublicKey(signingKey),
    keyId,
    plaintext,
  };
}

const keysWithKek: ReceiverKeys = {
  kekForGeneration: (g) => (g === KEK_GENERATION ? KEK : undefined),
};
const keysWithoutKek: ReceiverKeys = { kekForGeneration: () => undefined };

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
    const second = await makeFrame({
      streamSequence: 2,
      wrapUnderKeyId: hexToBytes('0000000000000009'),
    });
    const opened = await openVerifiedFrame(
      await verifyFrame(decodeFrame(second.bytes), second.publicKey),
      cache,
      keysWithoutKek,
    );
    expect(bytesToHex(opened.plaintext)).toBe(bytesToHex(second.plaintext));
    // The rotation-lag signal must be OBSERVABLE, not collapsed into `'absent'`
    // (which means "no wrap at all"). This is the KEK-generation-not-held state:
    // a wrap was present under a generation we do not hold, we played off the
    // cache, and task 19 must be able to count it.
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
    cache.set(unpackKeyId(f.keyId), f.keyId, new Uint8Array(32).fill(0x77), new Uint8Array(48));
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
