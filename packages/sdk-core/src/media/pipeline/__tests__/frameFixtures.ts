// File: packages/sdk-core/src/media/pipeline/__tests__/frameFixtures.ts
//
// Test-tier frame builder. REAL CRYPTO THROUGHOUT.
//
// Ed25519 signing, the KEK wrap, the RFC 9605 key schedule and the SFrame seal
// are the production primitives. Nothing here is a stub, and no test built on it
// has a verify path that can return `true` without real signature math.
//
// The builder exists so a test can construct a frame with a DELIBERATELY WRONG
// pairing — a key id claiming one sender, signed by another's key — which is the
// insider-forgery shape the attribution test asserts against and which a
// production encoder cannot produce.

import { buildPublisherRegion, buildUnsignedFrame, finishFrame } from '../../frame/frameCodec.js';
import { importSigningKeyFromSeed, signFrame } from '../../frame/ed25519.js';
import { packKeyId } from '../../frame/keyId.js';
import {
  AES_256_KEY_BYTES,
  SFRAME_CIPHER_SUITE_ID,
  SFRAME_SALT_BYTES,
  aesGcmSeal,
  sealSframe,
  serializeSframe,
  sframeObjectLength,
  wrapNonce,
} from '../../frame/sframe.js';
import { deriveSframeKeys, sframeNonce } from '../../frame/sframeKeySchedule.js';
import type { Bytes } from '../../frame/hex.js';

/** A test identity: a seed, its signing key, and its raw public half. */
export interface TestIdentity {
  readonly seed: Uint8Array;
  readonly signingKey: CryptoKey;
  readonly publicKey: Uint8Array;
}

/** Build a deterministic test identity from a one-byte seed fill. */
export async function makeIdentity(fill: number): Promise<TestIdentity> {
  const seed = new Uint8Array(32).fill(fill);
  const signingKey = await importSigningKeyFromSeed(seed);
  // Derive the public half through a verify-capable import of the same seed:
  // WebCrypto exposes it on the JWK of the imported private key.
  const jwk = await crypto.subtle.exportKey('jwk', signingKey);
  const b64 = (jwk.x ?? '').replace(/-/g, '+').replace(/_/g, '/');
  const raw = atob(b64);
  const publicKey = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i += 1) publicKey[i] = raw.charCodeAt(i);
  return { seed, signingKey, publicKey };
}

/** Inputs for {@link buildTestFrame}. */
export interface TestFrameSpec {
  /** The `sender_id` the KEY ID claims. Attribution derives from this. */
  readonly keyIdSenderId: number;
  readonly stream?: number;
  readonly generation?: number;
  readonly streamSequence?: number;
  /** The 32-byte transmit key. Defaults to a deterministic fill. */
  readonly transmitKey?: Uint8Array;
  /** The 32-byte meeting KEK the wrap opens under. */
  readonly kek: Uint8Array;
  /** The KEK generation announced in the wrapped block. */
  readonly kekGeneration?: number;
  /**
   * WHO ACTUALLY SIGNS. Deliberately independent of `keyIdSenderId` so a test
   * can build the insider-forgery frame: a frame under another sender's key id,
   * authored so it WOULD decrypt if verification were skipped.
   */
  readonly signer: TestIdentity;
  readonly plaintext: Uint8Array;
  /** Relay region. Unauthenticated: rewritten by a handler, signed by nobody. */
  readonly streamId?: number;
  readonly hopSequence?: number;
  /** Omit the wrap and clear the key-bearing flag. */
  readonly keyBearing?: boolean;
}

/** Build one complete v2 frame with real crypto. */
export async function buildTestFrame(spec: TestFrameSpec): Promise<Bytes> {
  const streamSequence = spec.streamSequence ?? 0;
  const keyId = packKeyId({
    senderId: BigInt(spec.keyIdSenderId),
    stream: BigInt(spec.stream ?? 1),
    generation: BigInt(spec.generation ?? 0),
  });
  const transmitKey = spec.transmitKey ?? new Uint8Array(AES_256_KEY_BYTES).fill(0x2a);
  const keyBearing = spec.keyBearing ?? true;

  const wrapped = keyBearing
    ? {
        kekGeneration: spec.kekGeneration ?? 0,
        wrappedKeyWithTag: await aesGcmSeal({
          key: spec.kek,
          nonce: wrapNonce(keyId),
          aad: keyId,
          plaintext: transmitKey,
        }),
      }
    : null;

  const flags = { independentlyDecodable: true, discardable: false, keyBearing } as const;
  const region = buildPublisherRegion(
    { flags, streamSequence, wrappedTransmitKey: wrapped, extensions: [] },
    sframeObjectLength(spec.plaintext.length),
  );

  const derived = await deriveSframeKeys({
    hash: 'SHA-512',
    baseKey: transmitKey,
    kid: keyId,
    cipherSuiteId: SFRAME_CIPHER_SUITE_ID,
    keyBytes: AES_256_KEY_BYTES,
    saltBytes: SFRAME_SALT_BYTES,
  });
  const sframe = await sealSframe({
    key: derived.key,
    nonce: sframeNonce(derived.salt, BigInt(streamSequence)),
    aad: region,
    plaintext: spec.plaintext,
    keyId,
  });

  const unsigned = buildUnsignedFrame({
    flags,
    streamSequence,
    wrappedTransmitKey: wrapped,
    extensions: [],
    streamId: spec.streamId ?? 0,
    hopSequence: spec.hopSequence ?? 0,
    payload: serializeSframe(sframe),
  });
  const signature = await signFrame(spec.signer.signingKey, unsigned.signedRange);
  return finishFrame(unsigned, signature);
}
