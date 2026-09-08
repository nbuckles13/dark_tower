// File: packages/sdk-core/src/media/frame/sframe.ts
//
// MODULE TWO of the SFrame stack: the AES-256-GCM primitives, the detached-tag
// SFrame object, and the KEK unwrap.
//
// ANCHOR (DRY): every wire constant here comes from `./wireConstants.js`, which
// is RENDERED from `proto/test-vectors/frame-v2.vectors.json` — the cross-language
// SSoT. Sites below read `WIRE_CONSTANTS.cipher_suite_id` and
// `WIRE_CONSTANTS.aead_tag_bytes` by their SSoT spellings. Never hardcode a second
// copy of any of them: the number's origin is `crates/media-protocol/src/frame.rs`,
// the vectors file mirrors it under guard g2/g3/g4, and
// `__tests__/wireConstants.drift.test.ts` byte-compares the render.
//
// ---------------------------------------------------------------------------
// ONE SUITE ONLY. THERE IS NO AES-128 PATH, AND THAT IS LOAD-BEARING
// ---------------------------------------------------------------------------
//
// ADR-0036 §4 selects `AES_256_GCM_SHA512_128`. Nothing here is
// suite-parameterised, and the key-length reject below is the reason:
//
//   VERIFIED IN CHROME (Chrome for Testing 151, over http://127.0.0.1):
//   `crypto.subtle.importKey('raw', new Uint8Array(16), 'AES-GCM', ...)`
//   SUCCEEDS. It reports `algorithm.length === 128` and encrypts happily.
//
// There is NO platform backstop. The refusal must be ours, and it must be on
// SEAL, on OPEN, and on UNWRAP — open is the receive path, the one an attacker
// reaches. Without it, the vendored AES-128 contrast row could be driven through
// this module, which would mean building a weak-key acceptance path into shipping
// crypto in order to test that we do not have one. ADR-0036's amendment table
// permits AES-128-GCM "only for SFrame interop", and we do none.
//
// ---------------------------------------------------------------------------
// THIS MODULE OWNS ONE DEVIATION FROM RFC 9605: THE OBJECT LAYOUT
// ---------------------------------------------------------------------------
//
// Our SFrame object is `key_id(8) || tag(16) || ciphertext`. RFC 9605's is
// `config || KID || CTR` with a TRAILING tag. The tag is moved into the clear
// header on seal and rejoined on open.
//
// The vendored external sframe-wg vectors gate the key schedule and the GCM
// primitive. They gate NEITHER this layout NOR the detached-tag split — the
// upstream rows have no relay region, no signature, no wrap and no extensions.
// See `proto/test-vectors/external/sframe-wg/PROVENANCE.md`. Do not read a green
// external anchor as covering anything in this file below `aesGcmSeal`/`aesGcmOpen`.

import { concatBytes, type Bytes } from './hex.js';
import { WIRE_CONSTANTS } from './wireConstants.js';
import { KEY_ID_BYTES } from './keyId.js';
import { codecReject } from './rejectReason.js';

/**
 * The only AES key length this construction accepts.
 *
 * ADR-0036 §4 fixes the transmit key at AES-256, and the RFC 9605 ciphersuite it
 * selects is AES-256-GCM. The suite id itself is never written here as a
 * literal — it is read from the SSoT as `WIRE_CONSTANTS.cipher_suite_id`, and
 * guard g12 bans the literal spelling in this file for exactly that reason.
 *
 * BOUNDARY NOTE — this 32 is not the same 32 as any other in this diff, and none
 * of them may be hoisted to a shared constant. They agree by coincidence, and
 * collapsing them would couple values that are free to move independently:
 *   * this one       — the AEAD key length for AES-256-GCM;
 *   * `WIRE_CONSTANTS.wrapped_transmit_key_material_bytes` — the wire width of the
 *     wrapped transmit key (`frame.rs::WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES`);
 *   * `MEETING_KEK_BYTES` below — the meeting KEK length
 *     (`crates/mc-service/src/media_admission/kek.rs`);
 *   * `ED25519_PUBLIC_KEY_BYTES` in `./ed25519.ts`
 *     (`crates/mc-service/src/media_admission/identity_key.rs`).
 * Rust records the same non-collapse at each of its definitions.
 */
export const AES_256_KEY_BYTES = 32;

/**
 * Meeting KEK length.
 *
 * BOUNDARY NOTE: see `AES_256_KEY_BYTES`. Equal by coincidence, not by
 * derivation; not hoisted.
 */
export const MEETING_KEK_BYTES = 32;

/**
 * AES-GCM nonce length, and hence the SFrame salt length (RFC 9605 §4.4.1).
 *
 * BOUNDARY NOTE: 12 because that is the GCM nonce width, NOT because
 * `aead_tag_bytes - 4` happens to equal it. An earlier draft of `wrapNonce`
 * wrote exactly that subtraction — it produced the right number and coupled the
 * nonce width to the tag width, two values free to move independently. Deriving
 * one constant from an unrelated one by arithmetic is the same collapse as
 * sharing a constant, and harder to see.
 *
 * Not in `wire_constants`: it is an RFC 9605 property of the ciphersuite, not a
 * Dark Tower frame field, so the vectors file is not its source of truth.
 */
export const GCM_NONCE_BYTES = 12;

/**
 * SFrame salt length. Equal to the GCM nonce width by construction — the salt IS
 * the nonce input (`sframeNonce` XORs it with the counter to produce the iv), so
 * a derived salt of any other width could not form a valid nonce. Exported so
 * callers of `deriveSframeKeys` reference it rather than writing a bare `12`.
 */
export const SFRAME_SALT_BYTES = GCM_NONCE_BYTES;

/** AEAD tag length, from the SSoT. */
const TAG_BYTES = WIRE_CONSTANTS.aead_tag_bytes;

/** The RFC 9605 ciphersuite id, from the SSoT (`cipher_suite_id`). */
export const SFRAME_CIPHER_SUITE_ID = WIRE_CONSTANTS.cipher_suite_id;

/**
 * Reject any AES key length except 32.
 *
 * Exported so the external-anchor gate can assert the refusal rather than assume
 * it — see the module header for why the platform will not do this for us.
 */
export function rejectWrongAesKeyLength(keyLength: number, where: string): void {
  if (keyLength !== AES_256_KEY_BYTES) {
    throw new RangeError(
      `${where}: AES key must be exactly ${AES_256_KEY_BYTES} bytes (AES-256), got ${keyLength}. ` +
        `WebCrypto ACCEPTS a 16-byte AES-GCM key without complaint, so this refusal is the only ` +
        `one there is.`,
    );
  }
}

async function importAesKey(
  key: Uint8Array,
  usage: 'encrypt' | 'decrypt',
  where: string,
): Promise<CryptoKey> {
  rejectWrongAesKeyLength(key.length, where);
  return crypto.subtle.importKey('raw', Uint8Array.from(key), 'AES-GCM', false, [usage]);
}

/**
 * AES-256-GCM seal in COMBINED mode: returns `ciphertext || tag`.
 *
 * The detached-tag split is deliberately NOT done here — it is framing, and it is
 * one of the computations the cross-language vectors exist to gate. Keeping this
 * primitive in combined mode means the external anchor tests exactly what it can
 * honestly claim to test.
 */
export async function aesGcmSeal(params: {
  readonly key: Uint8Array;
  readonly nonce: Uint8Array;
  readonly aad: Uint8Array;
  readonly plaintext: Uint8Array;
}): Promise<Bytes> {
  const cryptoKey = await importAesKey(params.key, 'encrypt', 'aesGcmSeal');
  const sealed = await crypto.subtle.encrypt(
    {
      name: 'AES-GCM',
      iv: Uint8Array.from(params.nonce),
      additionalData: Uint8Array.from(params.aad),
      tagLength: TAG_BYTES * 8,
    },
    cryptoKey,
    Uint8Array.from(params.plaintext),
  );
  return new Uint8Array(sealed);
}

/**
 * AES-256-GCM open in COMBINED mode: takes `ciphertext || tag`.
 *
 * Returns `null` on an authentication failure rather than throwing, so callers
 * choose the reject reason. The same GCM failure means different things at
 * different call sites — `decrypt_failed` for the payload, `unwrap_failed` for
 * the KEK unwrap — and those have OPPOSITE remedies, so the primitive must not
 * pick one.
 */
export async function aesGcmOpen(params: {
  readonly key: Uint8Array;
  readonly nonce: Uint8Array;
  readonly aad: Uint8Array;
  readonly ciphertextWithTag: Uint8Array;
}): Promise<Bytes | null> {
  // The key-length reject is on OPEN as well as seal. Open is the receive path,
  // the one an attacker reaches; a check only on the send side would leave the
  // reachable half unguarded.
  const cryptoKey = await importAesKey(params.key, 'decrypt', 'aesGcmOpen');
  try {
    const opened = await crypto.subtle.decrypt(
      {
        name: 'AES-GCM',
        iv: Uint8Array.from(params.nonce),
        additionalData: Uint8Array.from(params.aad),
        tagLength: TAG_BYTES * 8,
      },
      cryptoKey,
      Uint8Array.from(params.ciphertextWithTag),
    );
    return new Uint8Array(opened);
  } catch {
    // Swallowed deliberately: WebCrypto's OperationError carries nothing an
    // operator can use, and re-throwing it would put a platform string on a path
    // that must produce a bounded token. `null` means "the tag did not verify".
    return null;
  }
}

/** A sealed SFrame object, split into its wire parts. */
export interface SframeObject {
  /** The 8-byte key id, in the clear header. */
  readonly keyId: Bytes;
  /** The 16-byte authentication tag, DETACHED into the clear header. */
  readonly tag: Bytes;
  /** Ciphertext, exactly the plaintext length under GCM. */
  readonly ciphertext: Bytes;
}

/**
 * Seal a plaintext into an SFrame object: `key_id(8) || tag(16) || ciphertext`.
 *
 * `aad` is supplied by the CALLER, not computed here. Our AAD is the frame's
 * publisher region, which this module cannot see — that is framing, and it lives
 * in `./frameCodec.ts`.
 */
export async function sealSframe(params: {
  readonly key: Uint8Array;
  readonly nonce: Uint8Array;
  readonly aad: Uint8Array;
  readonly plaintext: Uint8Array;
  readonly keyId: Uint8Array;
}): Promise<SframeObject> {
  if (params.keyId.length !== KEY_ID_BYTES) {
    throw new RangeError(`sealSframe: key id must be ${KEY_ID_BYTES} bytes`);
  }
  const combined = await aesGcmSeal(params);
  // GCM is length-preserving, so the tag is exactly the trailing TAG_BYTES.
  const split = combined.length - TAG_BYTES;
  if (split < 0) {
    throw new RangeError(`sealSframe: sealed output shorter than the tag (${combined.length})`);
  }
  return {
    keyId: Uint8Array.from(params.keyId),
    tag: combined.subarray(split),
    ciphertext: combined.subarray(0, split),
  };
}

/**
 * The wire length of the SFrame object that sealing `plaintextLength` bytes
 * produces: `key_id(8) || tag(16) || ciphertext`.
 *
 * Exists so the ENCODE path can size `payload_length` BEFORE sealing. That is
 * not a shortcut around sealing first — it is what makes the ordering possible
 * at all: the AEAD associated data is the publisher region, the publisher region
 * carries `payload_length`, and AES-GCM is length-preserving, so the length is
 * knowable in advance and the apparent circularity dissolves.
 *
 * Derived from the same constants `sealSframe` and `parseSframe` use, so a
 * caller cannot compute a different answer than the serializer produces.
 */
export function sframeObjectLength(plaintextLength: number): number {
  return KEY_ID_BYTES + TAG_BYTES + plaintextLength;
}

/** Serialize an SFrame object to its wire form. */
export function serializeSframe(obj: SframeObject): Bytes {
  return concatBytes(obj.keyId, obj.tag, obj.ciphertext);
}

/**
 * Parse an SFrame object from a payload slice. Slices, never copies.
 *
 * @throws {FrameRejectedError} `truncated` if the payload cannot hold the clear
 * header.
 */
export function parseSframe(payload: Uint8Array): SframeObject {
  const minimum = KEY_ID_BYTES + TAG_BYTES;
  if (payload.length < minimum) {
    throw codecReject(
      'truncated',
      `SFrame object is ${payload.length} bytes, shorter than its ${minimum}-byte clear header`,
      { available: payload.length, limit: minimum },
    );
  }
  return {
    keyId: payload.subarray(0, KEY_ID_BYTES) as Bytes,
    tag: payload.subarray(KEY_ID_BYTES, minimum) as Bytes,
    ciphertext: payload.subarray(minimum) as Bytes,
  };
}

/**
 * Open an SFrame object, rejoining the detached tag.
 *
 * Returns `null` on authentication failure; the caller maps that to
 * `decrypt_failed`.
 */
export async function openSframe(params: {
  readonly key: Uint8Array;
  readonly nonce: Uint8Array;
  readonly aad: Uint8Array;
  readonly object: SframeObject;
}): Promise<Bytes | null> {
  // Rejoin: WebCrypto's combined mode wants `ciphertext || tag`, which is the
  // inverse of the split performed on seal.
  return aesGcmOpen({
    key: params.key,
    nonce: params.nonce,
    aad: params.aad,
    ciphertextWithTag: concatBytes(params.object.ciphertext, params.object.tag),
  });
}

/**
 * The 12-byte KEK-unwrap nonce: `0x00000000 || key_id`.
 *
 * ADR-0036 §4 pins this: four zero bytes then the 8-byte big-endian key id,
 * right-aligned. Zero-PREFIX rather than zero-suffix so the codebase carries ONE
 * padding rule — SFrame's own nonce is `salt XOR BE12(counter)`, which
 * right-aligns a big-endian value identically.
 */
export function wrapNonce(keyId: Uint8Array): Bytes {
  if (keyId.length !== KEY_ID_BYTES) {
    throw new RangeError(`wrapNonce: key id must be ${KEY_ID_BYTES} bytes`);
  }
  const out = new Uint8Array(GCM_NONCE_BYTES);
  out.set(keyId, out.length - KEY_ID_BYTES);
  return out;
}

/**
 * Unwrap a transmit key from a frame's wrapped-key block.
 *
 * ---------------------------------------------------------------------------
 * THE KEY-ID BINDING IS ENFORCED BY CONSTRUCTION, NOT BY A CHECK
 * ---------------------------------------------------------------------------
 *
 * The 50-byte block is `kek_generation(2) || wrapped_key(32) || tag(16)`. THERE
 * IS NO BOUND-KEY-ID FIELD. The binding exists only as the AAD used at seal
 * time, so unwrapping under THIS frame's own key id is what enforces ADR-0036
 * §4's "receivers accept a wrapped key only for the key id of the frame carrying
 * it". A wrap bound to a different key id simply does not open. That is the
 * strongest available form and nothing should be added to "improve" it:
 *
 *   * DO NOT add a plaintext bound-kid field with a comparison.
 *   * DO NOT change the AAD to make mis-binding "detectable". NARROWING it is the
 *     dangerous one — it would make a mis-bound wrap SUCCEED, leaving the binding
 *     resting on a comparison, which is strictly weaker than a construction that
 *     simply fails. Widening it breaks interop with every honest wrapper.
 *
 * A consequence worth knowing before reading the receive path: a mis-bound wrap
 * and a wrong-KEK unwrap produce the SAME tag mismatch — one bit, indistinguishable.
 * `wrap_key_id_mismatch` names a condition this code cannot detect. What forks
 * the two outcomes is receiver state, not the failure. See `receivePath.ts`.
 *
 * ---------------------------------------------------------------------------
 * A NONCE COLLISION IS REACHABLE HERE. IT IS NOT EXPLOITABLE, AND THE REASON IT
 * IS NOT IS A DEPENDENCY THAT A FUTURE EDIT COULD REMOVE.
 * ---------------------------------------------------------------------------
 *
 * The nonce is `0x00000000 || key_id` under a MEETING-WIDE KEK. So an insider can
 * wrap arbitrary material under another sender's key id and produce a second GCM
 * ciphertext under a `(KEK, nonce)` pair already used. That is textbook AES-GCM
 * nonce reuse and it reads as a critical finding to anyone — including an external
 * auditor — who meets it without this context. It is not one:
 *
 *   * Only a KEK HOLDER can cause it. Producing the wrap requires the KEK, and a
 *     KEK holder can already encrypt and authenticate anything under it, so
 *     authentication-subkey recovery confers nothing they do not have.
 *   * The one party who could benefit from OBSERVING it cannot use it. MH sees
 *     both ciphertexts and could in principle recover the GHASH subkey — but the
 *     wrapped-key block sits in the PUBLISHER REGION and is covered by the
 *     Ed25519 signature. ADR-0036 §4, in terms: "MH can neither attach, strip, nor
 *     replay it." MH cannot get a forged wrap into a frame at all, so tag-forgery
 *     capability is unreachable.
 *   * Honest senders cannot cause it, because generation is monotonic per sender
 *     and never reset — the caller obligation below.
 *
 * ** THE DEPENDENCY IS THE LOAD-BEARING PART: this is unexploitable BECAUSE §3's
 * signature covers the publisher region. Weakening that coverage turns this from a
 * curiosity into a live forgery path against the KEK. ** Do not copy the
 * conclusion without this clause to a site where it does not hold.
 *
 * CALLER OBLIGATION: never wrap two DIFFERENT transmit keys under the same key
 * id. One transmit key wraps to one ciphertext per key id — byte-identical from
 * frame to frame within a generation — so nonce uniqueness under the KEK reduces
 * to key-id uniqueness, which generation monotonicity gives.
 *
 * @param keyId the RECEIVED 8-byte slice. NOT `(sender, stream, generation)`, and
 * deliberately not reconstructible from them here: a value re-packed from decoded
 * fields always succeeds and is merely wrong, which is the phantom-key-failure
 * signature. Taking the slice makes the wrong thing inexpressible.
 *
 * @returns the unwrapped transmit key, or `null` if the tag did not verify.
 *
 * PURE: returns a key and writes NOTHING. There is deliberately no
 * `unwrapAndCache` and no cache-priming export — the cache write is reachable
 * only from the verified path in `receivePath.ts`, because ADR-0036 §4 requires
 * that a wrap from a frame that fails verification is never cached. A door that
 * exists only for tests is still a door.
 */
export async function unwrapTransmitKey(params: {
  readonly kek: Uint8Array;
  readonly keyId: Uint8Array;
  readonly wrappedKeyWithTag: Uint8Array;
}): Promise<Bytes | null> {
  if (params.keyId.length !== KEY_ID_BYTES) {
    throw new RangeError(`unwrapTransmitKey: key id must be ${KEY_ID_BYTES} bytes`);
  }
  rejectWrongAesKeyLength(params.kek.length, 'unwrapTransmitKey (meeting KEK)');
  const opened = await aesGcmOpen({
    key: params.kek,
    // The AAD is the RECEIVED key id slice, never a re-pack. Same run that feeds
    // the HKDF `info`.
    aad: params.keyId,
    nonce: wrapNonce(params.keyId),
    ciphertextWithTag: params.wrappedKeyWithTag,
  });
  if (opened === null) return null;
  if (opened.length !== AES_256_KEY_BYTES) {
    // A KEK that opens but yields a wrong-width key is a protocol violation, not
    // a wrong key. Refused rather than passed on to the schedule.
    throw new RangeError(
      `unwrapTransmitKey: unwrapped key is ${opened.length} bytes, expected ${AES_256_KEY_BYTES}`,
    );
  }
  return opened;
}
