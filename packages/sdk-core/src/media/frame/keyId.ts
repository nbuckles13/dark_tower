// File: packages/sdk-core/src/media/frame/keyId.ts
//
// The SFrame key identifier: `sender_id(16) | stream(8) | generation(40)`,
// packed big-endian into the 8 bytes that live in the SFrame object's clear
// header (ADR-0036 §2, §4).
//
// ---------------------------------------------------------------------------
// THE KEY ID LIVES IN EXACTLY ONE PLACE ON THE WIRE
// ---------------------------------------------------------------------------
//
// It is in the SFrame clear header, never duplicated in the frame header. A
// receiver must be able to read it before decrypting — it selects the key — so
// it cannot be inside the ciphertext.
//
// ---------------------------------------------------------------------------
// WHY EVERY CONVERSION IS CHECKED, AND WHY EVERYTHING IS `bigint`
// ---------------------------------------------------------------------------
//
// A masking pack aliases `sender_id` 65536 onto 0. That is not a cosmetic
// truncation: two senders would share a key id, hence a (key, nonce) pair, and
// under AES-GCM a repeated pair does not merely expose those two frames — it
// leaks the authentication subkey and permits forgery. A bare shift is worse,
// overflowing into the adjacent field silently. Both are refused: every
// component is bounds-checked against its field width and throws.
//
// `bigint` throughout, and this is not stylistic. JavaScript's bitwise operators
// coerce to **32-bit** integers. `generation` is 40 bits, so `<<`, `>>>` and
// `& 0xFFFF` are all WRONG here as well as being banned by the task — `1 << 40`
// evaluates to 256, not 2^40. There is no correct number-typed implementation of
// this packing.
//
// Mirrors `crates/media-vector-gen/src/kid.rs`, whose `pack` makes the same
// checks with the same reasoning.
//
// ---------------------------------------------------------------------------
// WHY 16 BITS OF SENDER AND 40 OF GENERATION
// ---------------------------------------------------------------------------
//
// Chosen over a 32-bit sender so that generation is effectively INEXHAUSTIBLE
// under one KEK. A sender rotates its transmit key on every video group and
// every T for audio (ADR-0036 §4), so generation is the fast counter; at 2^40 it
// cannot be driven to wrap within any meeting lifetime. That is why there is no
// generation-ceiling guard and no ceiling vector: the ceiling is UNREACHABLE
// rather than unchecked.

import { WIRE_CONSTANTS } from './wireConstants.js';
import type { Bytes } from './hex.js';

const {
  sender_id_bits: SENDER_ID_BITS,
  stream_bits: STREAM_BITS,
  generation_bits: GENERATION_BITS,
} = WIRE_CONSTANTS.key_id_layout;

/** Byte width of the packed key id. */
export const KEY_ID_BYTES = WIRE_CONSTANTS.key_id_bytes;

/** One past the largest representable `sender_id` (65536). */
export const SENDER_ID_EXCLUSIVE_BOUND = 1n << BigInt(SENDER_ID_BITS);
/** One past the largest representable `stream` (256). */
export const STREAM_EXCLUSIVE_BOUND = 1n << BigInt(STREAM_BITS);
/** One past the largest representable `generation` (2^40). */
export const GENERATION_EXCLUSIVE_BOUND = 1n << BigInt(GENERATION_BITS);

/** The decomposed key id. */
export interface KeyIdParts {
  /**
   * Meeting-scoped sender identifier, `1..=65535`.
   *
   * Arrives as a proto `uint32` and is range-checked to 65535 by MC — but the
   * client checks it AGAIN at pack time. A client that trusted MC's check would
   * silently truncate on a compromised or buggy controller, which is the
   * key-id collision above.
   */
  readonly senderId: bigint;
  /**
   * Sender-scoped stream number.
   *
   * NOT the relay `stream_id`: that one is subscriber-scoped, two bytes wide,
   * rewritten by a media handler per subscriber, and authenticated by nobody.
   * This one is sender-scoped, one byte, and cryptographically bound through the
   * key derivation.
   */
  readonly stream: bigint;
  /** Monotonic per sender across its membership; never reset. */
  readonly generation: bigint;
}

/** Thrown when a key-id component does not fit its field. Never a silent truncation. */
export class KeyIdRangeError extends RangeError {
  readonly field: 'senderId' | 'stream' | 'generation';

  constructor(field: 'senderId' | 'stream' | 'generation', message: string) {
    super(message);
    this.name = 'KeyIdRangeError';
    this.field = field;
  }
}

// SEND-PATH ERROR MESSAGES ECHO THE OUT-OF-RANGE FIELD VALUE, AND THAT IS A
// BOUNDED EXCEPTION TO `rejectReason.ts`'s absolute "no key id in error messages"
// rule — annotated here so the next author does not read the rule as negotiable.
//
// Why it is allowed HERE: this is `packKeyId`, the SEND path. The value is the
// CLIENT'S OWN sender/stream/generation, it is out of range BY DEFINITION (that
// is why we threw), so it is not a live identifier, and it is the one number the
// caller needs to fix their own bug.
//
// Why it does NOT extend to the receive path: `unpackKeyId` handles a PEER's key
// id, and it reports LENGTH ONLY, never a value — a decomposed `sender_id` there
// is a CATEGORY_B token (`crates/dt-guard/src/common/pii_vocabulary.rs`) and
// label-taxonomy R2 bars stream identity on the media path. The contrast is the
// boundary: value-on-send is fine, value-on-receive is not.
function checkField(
  field: 'senderId' | 'stream' | 'generation',
  value: bigint,
  bound: bigint,
  bits: number,
): void {
  if (typeof value !== 'bigint') {
    throw new KeyIdRangeError(
      field,
      `key id: ${field} must be a bigint (fields are up to 40 bits)`,
    );
  }
  if (value < 0n) {
    throw new KeyIdRangeError(field, `key id: ${field} is negative (${value})`);
  }
  if (value >= bound) {
    throw new KeyIdRangeError(
      field,
      `key id: ${field} = ${value} does not fit in ${bits} bits (max ${bound - 1n}). ` +
        `Refused rather than masked: a truncating pack aliases two senders onto one key id, ` +
        `hence one (key, nonce) pair, which under AES-GCM leaks the authentication subkey.`,
    );
  }
}

/**
 * Pack `(senderId, stream, generation)` into the 8-byte big-endian key id.
 *
 * @throws {KeyIdRangeError} if any component is negative or exceeds its field,
 * or if `senderId` is zero.
 */
export function packKeyId(parts: KeyIdParts): Bytes {
  const { senderId, stream, generation } = parts;

  // `sender_id == 0` is reserved-invalid, mirroring
  // `crates/media-vector-gen/src/kid.rs`. A shared zero is N colliding key ids —
  // the same nonce-reuse hazard as truncation, reached by a different route.
  if (senderId === 0n) {
    throw new KeyIdRangeError(
      'senderId',
      'key id: senderId 0 is reserved-invalid — a shared zero is N colliding key ids',
    );
  }
  checkField('senderId', senderId, SENDER_ID_EXCLUSIVE_BOUND, SENDER_ID_BITS);
  checkField('stream', stream, STREAM_EXCLUSIVE_BOUND, STREAM_BITS);
  checkField('generation', generation, GENERATION_EXCLUSIVE_BOUND, GENERATION_BITS);

  const packed =
    (senderId << BigInt(STREAM_BITS + GENERATION_BITS)) |
    (stream << BigInt(GENERATION_BITS)) |
    generation;

  const out = new Uint8Array(KEY_ID_BYTES);
  new DataView(out.buffer).setBigUint64(0, packed, false);
  return out;
}

/**
 * Decompose a received key id into `(senderId, stream, generation)`.
 *
 * FOR LOOKUP ONLY. The decomposed fields select an identity key and a key-cache
 * entry; they must never be re-packed and fed to a cryptographic input.
 *
 * The RFC's canonical derivation input is the 8-byte big-endian key id AS
 * RECEIVED, and the same run is the KEK-unwrap associated data. A value
 * re-packed from these fields always succeeds and is merely wrong — it produces
 * a different key, a different nonce, or a failing tag with no indication of
 * why. That is the phantom-key-failure signature, and it is why
 * `unwrapTransmitKey` and `sframeInfo` take the raw slice rather than these
 * fields: the wrong thing is not expressible in their signatures.
 *
 * Deliberately does NOT reject `senderId === 0`. Rejection is a PACK-time rule;
 * on receive a frame claiming sender 0 simply finds no roster entry and is
 * dropped as `no_roster_entry`, which is the correct and more informative
 * outcome.
 */
export function unpackKeyId(keyId: Uint8Array): KeyIdParts {
  if (keyId.length !== KEY_ID_BYTES) {
    throw new RangeError(`key id: expected ${KEY_ID_BYTES} bytes, got ${keyId.length}`);
  }
  const packed = new DataView(keyId.buffer, keyId.byteOffset, keyId.byteLength).getBigUint64(
    0,
    false,
  );
  return {
    senderId: packed >> BigInt(STREAM_BITS + GENERATION_BITS),
    stream: (packed >> BigInt(GENERATION_BITS)) & (STREAM_EXCLUSIVE_BOUND - 1n),
    generation: packed & (GENERATION_EXCLUSIVE_BOUND - 1n),
  };
}

/**
 * A stable map key for a received key id.
 *
 * Derived from the RECEIVED BYTES, so two distinct wire key ids can never
 * collide in a cache regardless of how they decompose.
 */
export function keyIdCacheKey(keyId: Uint8Array): string {
  let out = '';
  for (const b of keyId) out += b.toString(16).padStart(2, '0');
  return out;
}
