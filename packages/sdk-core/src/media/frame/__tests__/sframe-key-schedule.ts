// File: packages/sdk-core/src/media/frame/__tests__/sframe-key-schedule.ts
//
// RFC 9605 §4.4 SFrame key schedule and the AEAD primitive, implemented from
// the specification text. This is the code the vendored external sframe-wg
// vectors gate (`external-anchor.test.ts`).
//
// ---------------------------------------------------------------------------
// DERIVED FROM THE SPEC, NOT FROM THE RUST REFERENCE GENERATOR
// ---------------------------------------------------------------------------
//
// Written from RFC 9605 §4.4 and the ADR-0036 §4 ciphersuite choice. The Rust
// reference generator in this same task was NOT read while authoring it. That
// independence is the entire point: if the two implementations converge by one
// mirroring the other, the vectors validate a shared error instead of catching
// it. See `main.md` §Divergences.
//
// ---------------------------------------------------------------------------
// SINGLE HOME — AND A SCOPE LIMIT WORTH KNOWING
// ---------------------------------------------------------------------------
//
// Like `hex.ts`, this lives in the test tree because story task 8 has no
// production consumer; the production crypto stack lands at story task 15.
// **MOVE it to `src/` at task 15 — do not copy it.**
//
// The scope limit that matters more: until that move happens, the external
// anchor gates THIS module, which is test-tier code — not the shipping key
// schedule, because at task 8 no shipping key schedule exists. So "the external
// gate is live at task 8" is true and narrower than it sounds. At task 15 the
// gate must be re-pointed at the production module by promoting this file, not
// by writing a second implementation and leaving the anchor aimed at the first.
// If that happens, the external anchor silently stops gating anything that
// ships, while continuing to pass. Recorded as a task-15 obligation in
// `main.md`.

import { bigUintToBytesBE, concatBytes, xorBytes, type Bytes } from './hex.js';

/** Hashes the SFrame ciphersuites in scope here use. */
export type HashName = 'SHA-256' | 'SHA-512';

/** HashLen (RFC 5869's terminology) — the HMAC output width, in bytes. */
const HASH_OUTPUT_BYTES: Readonly<Record<HashName, number>> = {
  'SHA-256': 32,
  'SHA-512': 64,
};

/**
 * `HKDF-Extract(salt, IKM)` — RFC 5869 §2.2, i.e. `HMAC-Hash(salt, IKM)`.
 *
 * Exposed as its own function ON PURPOSE. WebCrypto's one-shot HKDF
 * (`deriveBits`) performs extract-then-expand internally and never surfaces the
 * PRK, so the explicit PRK length/hash assertion this task requires is
 * impossible through it. `expandThenCompareWithWebCryptoHkdf` re-checks this
 * implementation against the platform's, so we get the assertion AND a
 * cross-check rather than choosing between them.
 */
export async function hkdfExtract(
  hash: HashName,
  salt: Uint8Array,
  ikm: Uint8Array,
): Promise<Bytes> {
  return hmac(hash, salt, ikm);
}

/**
 * The default HKDF salt for `hash`: `HashLen` zero bytes.
 *
 * RFC 5869 §2.2 defines the absent salt as "a string of HashLen zeros", so this
 * IS the specified construction rather than a substitute for one. That matters
 * practically as well as pedantically: WebCrypto rejects a zero-LENGTH HMAC key
 * outright (`DataError: Zero-length key is not supported`), so an implementer
 * who reads RFC 9605's "empty salt" literally hits a throw and reaches for a
 * workaround. There is nothing to work around — HMAC zero-pads a short key to
 * the hash block size, so an empty key, `HashLen` zeros and `BlockLen` zeros
 * are all the same key by construction, not merely equal in practice.
 *
 * Do not "fix" this to an empty array. It will throw, and the equivalence that
 * makes this correct is not obvious from the call site.
 */
export function defaultHkdfSalt(hash: HashName): Bytes {
  return new Uint8Array(HASH_OUTPUT_BYTES[hash]);
}

/** `HKDF-Expand(PRK, info, L)` — RFC 5869 §2.3. */
export async function hkdfExpand(
  hash: HashName,
  prk: Uint8Array,
  info: Uint8Array,
  length: number,
): Promise<Bytes> {
  const hashLen = HASH_OUTPUT_BYTES[hash];
  const blocks = Math.ceil(length / hashLen);
  if (blocks > 255) throw new RangeError(`HKDF-Expand: L=${length} exceeds 255*HashLen`);
  const out = new Uint8Array(length);
  let previous = new Uint8Array(0);
  let written = 0;
  for (let counter = 1; counter <= blocks; counter += 1) {
    previous = await hmac(hash, prk, concatBytes(previous, info, Uint8Array.of(counter)));
    const take = Math.min(previous.length, length - written);
    out.set(previous.subarray(0, take), written);
    written += take;
  }
  return out;
}

/**
 * The RFC 9605 §4.4 `info` string for a derivation.
 *
 * `label || KID(8, big-endian) || cipher_suite(2, big-endian)`. The trailing
 * space in each label is significant and part of the literal.
 *
 * `kid` is taken as an OPAQUE 8-byte run, never as a number and never re-packed
 * from decomposed fields. Our key id decomposes as
 * `sender_id(16) | stream(8) | generation(40)`, but that decomposition is a
 * lookup aid on the receive side and has no business here: feeding a re-packed
 * value into the derivation always succeeds and is merely wrong, which is the
 * phantom-key-failure signature. It also keeps this function usable against the
 * external vectors, whose `kid` decomposes to a `sender_id` of 0 that our own
 * layout treats as invalid.
 */
export function sframeInfo(label: string, kid: Uint8Array, cipherSuiteId: number): Bytes {
  if (kid.length !== 8) throw new RangeError(`SFrame info: KID must be 8 bytes, got ${kid.length}`);
  return concatBytes(
    new TextEncoder().encode(label),
    kid,
    bigUintToBytesBE(BigInt(cipherSuiteId), 2, 'cipher_suite'),
  );
}

/** RFC 9605 §4.4 key-derivation label. The trailing space is part of it. */
export const SFRAME_KEY_LABEL = 'SFrame 1.0 Secret key ';
/** RFC 9605 §4.4 salt-derivation label. The trailing space is part of it. */
export const SFRAME_SALT_LABEL = 'SFrame 1.0 Secret salt ';

/**
 * The nonce for a frame: `sframe_salt XOR big-endian(counter)`, right-aligned
 * into the salt width (RFC 9605 §4.4.1).
 *
 * `counter` is a `bigint` because the field is 64-bit in the RFC. Our own
 * counter is the 32-bit stream sequence, and zero-extending a big-endian 32-bit
 * value into 12 bytes IS the big-endian 12-byte encoding of the same integer —
 * so this one construction covers both, and the external vectors genuinely
 * exercise it (their counter exceeds 16 bits, so multi-byte placement is not
 * accidentally trivial).
 */
export function sframeNonce(salt: Uint8Array, counter: bigint): Bytes {
  return xorBytes(salt, bigUintToBytesBE(counter, salt.length, 'counter'));
}

/** Full RFC 9605 §4.4 derivation for one (base key, KID, ciphersuite). */
export interface SframeDerivation {
  /** HKDF-Extract output. 64 bytes under SHA-512, 32 under SHA-256. */
  readonly secret: Bytes;
  readonly keyInfo: Bytes;
  readonly saltInfo: Bytes;
  readonly key: Bytes;
  readonly salt: Bytes;
}

export async function deriveSframeKeys(params: {
  readonly hash: HashName;
  readonly baseKey: Uint8Array;
  readonly kid: Uint8Array;
  readonly cipherSuiteId: number;
  readonly keyBytes: number;
  readonly saltBytes: number;
}): Promise<SframeDerivation> {
  const { hash, baseKey, kid, cipherSuiteId, keyBytes, saltBytes } = params;
  const secret = await hkdfExtract(hash, defaultHkdfSalt(hash), baseKey);
  const keyInfo = sframeInfo(SFRAME_KEY_LABEL, kid, cipherSuiteId);
  const saltInfo = sframeInfo(SFRAME_SALT_LABEL, kid, cipherSuiteId);
  return {
    secret,
    keyInfo,
    saltInfo,
    key: await hkdfExpand(hash, secret, keyInfo, keyBytes),
    salt: await hkdfExpand(hash, secret, saltInfo, saltBytes),
  };
}

/**
 * The same derivation via WebCrypto's one-shot HKDF, for cross-checking this
 * module's explicit extract-then-expand against the platform's implementation.
 */
export async function webCryptoHkdf(
  hash: HashName,
  ikm: Uint8Array,
  salt: Uint8Array,
  info: Uint8Array,
  length: number,
): Promise<Bytes> {
  const key = await crypto.subtle.importKey('raw', Uint8Array.from(ikm), 'HKDF', false, [
    'deriveBits',
  ]);
  const bits = await crypto.subtle.deriveBits(
    { name: 'HKDF', hash, salt: Uint8Array.from(salt), info: Uint8Array.from(info) },
    key,
    length * 8,
  );
  return new Uint8Array(bits);
}

/**
 * The only AES key length this construction accepts.
 *
 * ADR-0036 §4 fixes the transmit key at AES-256, and RFC 9605 ciphersuite
 * 0x0005 is AES-256-GCM. Nothing here is suite-parameterised.
 */
export const AES_256_KEY_BYTES = 32;

/**
 * AES-GCM seal in COMBINED mode: returns `ciphertext || tag`.
 *
 * The detached-tag split that ADR-0036 §2 requires (tag moved into the SFrame
 * object's clear header) is deliberately NOT done here — it is framing, it is
 * one of the three computations the cross-language vectors exist to gate, and
 * the external vectors do not cover it. Keeping this primitive in combined mode
 * means the external anchor tests exactly what it can honestly claim to test.
 */
export async function aesGcmSeal(params: {
  readonly key: Uint8Array;
  readonly nonce: Uint8Array;
  readonly aad: Uint8Array;
  readonly plaintext: Uint8Array;
  readonly tagBytes: number;
}): Promise<Bytes> {
  const { key, nonce, aad, plaintext, tagBytes } = params;
  // The key-length check is OURS because the platform does not make it.
  // `crypto.subtle.importKey('raw', <16 bytes>, 'AES-GCM', ...)` succeeds
  // without complaint (verified on Node 22 and specified by WebCrypto), so
  // there is no backstop underneath this line. Without it, the AES-128
  // contrast suite in the vendored external vectors could be driven through
  // this function — which would mean building a weak-key acceptance path into
  // our crypto in order to test that we do not have one.
  if (key.length !== AES_256_KEY_BYTES) {
    throw new RangeError(
      `aesGcmSeal: key must be ${AES_256_KEY_BYTES} bytes (AES-256), got ${key.length}`,
    );
  }
  const cryptoKey = await crypto.subtle.importKey('raw', Uint8Array.from(key), 'AES-GCM', false, [
    'encrypt',
  ]);
  const sealed = await crypto.subtle.encrypt(
    {
      name: 'AES-GCM',
      iv: Uint8Array.from(nonce),
      additionalData: Uint8Array.from(aad),
      tagLength: tagBytes * 8,
    },
    cryptoKey,
    Uint8Array.from(plaintext),
  );
  return new Uint8Array(sealed);
}

/** HMAC-`hash` over `data` under `key`. */
async function hmac(hash: HashName, key: Uint8Array, data: Uint8Array): Promise<Bytes> {
  // `Uint8Array.from` re-backs the view on a plain `ArrayBuffer`. WebCrypto's
  // `BufferSource` rejects the `ArrayBufferLike` a bare `Uint8Array` may carry,
  // and copying at the boundary is cheaper to read than a cast that asserts
  // something the type system cannot see.
  const cryptoKey = await crypto.subtle.importKey(
    'raw',
    Uint8Array.from(key),
    { name: 'HMAC', hash },
    false,
    ['sign'],
  );
  return new Uint8Array(await crypto.subtle.sign('HMAC', cryptoKey, Uint8Array.from(data)));
}
