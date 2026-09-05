// File: packages/sdk-core/src/media/frame/frameCodec.ts
//
// MODULE THREE of the SFrame stack: the ADR-0036 §2 version-2 binary frame
// codec, and the two authenticated byte ranges the rest of the stack consumes.
//
// ANCHOR (DRY): every wire constant here comes from `./wireConstants.js`, which
// is RENDERED from `proto/test-vectors/frame-v2.vectors.json` — the cross-language
// SSoT. Sites below read `WIRE_CONSTANTS.max_payload_bytes` and
// `WIRE_CONSTANTS.legal_flag_mask` by their SSoT spellings. Never hardcode a
// second copy: the origin is `crates/media-protocol/src/frame.rs`, the vectors
// file mirrors it under guards g2/g3/g4, and
// `__tests__/wireConstants.drift.test.ts` byte-compares the render. `legal_flag_mask`
// matters most here — it is 7, so no banned-literal check could ever catch a
// hardcode of it, and the derivation is the only enforcement there is.
//
// ---------------------------------------------------------------------------
// LAYOUT (ADR-0036 §2 + Appendix), BIG-ENDIAN THROUGHOUT
// ---------------------------------------------------------------------------
//
//   off  size  field                      region
//     0     1  version (= 2)              publisher  \
//     1     1  flags                      publisher   |
//     2     4  payload_length             publisher   |  AEAD associated data
//     6     4  stream_sequence            publisher   |  AND signed
//    10    50  wrapped transmit key       publisher   |  (iff key-bearing)
//     .     2  ext_length                 publisher   |
//     .     n  extension TLVs             publisher  /
//     .     2  stream_id                  relay      \  NEITHER signed
//     .     4  hop_sequence               relay      /  NOR in the AAD
//     .     m  payload (SFrame object)               <- signed, NOT in the AAD
//     .    64  Ed25519 signature
//
// ---------------------------------------------------------------------------
// TWO DELIBERATE DEVIATIONS FROM RFC 9605. READ BOTH BEFORE CHANGING EITHER.
// ---------------------------------------------------------------------------
//
// (1) THE AEAD ASSOCIATED DATA IS THE PUBLISHER REGION ONLY — the relay region
//     and the payload are EXCLUDED. RFC 9605's own AAD is its SFrame header, so
//     this is ours, not the RFC's, and nothing outside this repository validates
//     it. The relay region is excluded because a media handler REWRITES it per
//     subscriber; including it would break every decrypt downstream of a relay.
//
// (2) THE ED25519 SIGNED RANGE IS THE PUBLISHER REGION CONCATENATED WITH THE
//     PAYLOAD — SKIPPING the relay region wedged between them. The signed range
//     is therefore NOT CONTIGUOUS in the received buffer. The obvious
//     implementation — sign everything up to the signature — is wrong, and it is
//     wrong in the way that is hardest to catch: both sides agree, MH routes
//     correctly because it never decrypts, and the receiver's verification fails.
//     Black video, silent audio, both sides' unit tests green (ADR-0036 §2). The
//     vectors pin the near-miss explicitly as `naive_contiguous_signed_input_hex`,
//     and the conformance harness asserts it DIFFERS from the real signed range on
//     every row.
//
// BOTH RANGES ARE SLICES OF THE RECEIVED BUFFER AND ARE NEVER RE-SERIALIZED.
// Re-serializing would verify the decoder against itself: a field the decoder
// mis-parsed would be re-emitted with the same mistake and the signature would
// still check out. `subarray` views, always.
//
// On the ENCODE side the mirror rule: the entire publisher region — including the
// wrapped-key block and `payload_length` — must be populated BEFORE it is sliced
// as AAD or signed. A range taken over a half-filled buffer authenticates zeros.
//
// The vendored external sframe-wg vectors gate NONE of this file. See
// `proto/test-vectors/external/sframe-wg/PROVENANCE.md`.

import { concatBytes, type Bytes } from './hex.js';
import { WIRE_CONSTANTS, EXTENSION_REGISTRY, HEADER_VERSION } from './wireConstants.js';
import { codecReject } from './rejectReason.js';

const {
  legal_flag_mask: LEGAL_FLAG_MASK,
  max_payload_bytes: MAX_PAYLOAD_BYTES,
  signature_bytes: SIGNATURE_BYTES,
  relay_region_bytes: RELAY_REGION_BYTES,
  publisher_fixed_prefix_bytes: PUBLISHER_FIXED_PREFIX_BYTES,
  wrapped_transmit_key_bytes: WRAPPED_TRANSMIT_KEY_BYTES,
  kek_generation_field_bytes: KEK_GENERATION_FIELD_BYTES,
  wrapped_transmit_key_material_bytes: WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES,
  ext_length_field_bytes: EXT_LENGTH_FIELD_BYTES,
  max_ext_bytes: MAX_EXT_BYTES,
} = WIRE_CONSTANTS;

/**
 * The AEAD input span inside the wrapped-key block: the wrapped key plus its tag,
 * i.e. everything after the KEK-generation field.
 *
 * Written as MATERIAL + TAG rather than as `WRAPPED_TRANSMIT_KEY_BYTES -
 * KEK_GENERATION_FIELD_BYTES`. The subtraction gives the same number today and
 * would make this span a function of the BLOCK size rather than of its contents,
 * so a future field added to the block would silently be swallowed into the AEAD
 * input. `__tests__/frameCodec.test.ts` asserts the three widths still sum to the
 * block size, which is the cross-check `frame.rs:211` already makes on the Rust
 * side.
 */
const WRAPPED_KEY_AEAD_BYTES = WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES + WIRE_CONSTANTS.aead_tag_bytes;

/**
 * Protocol version, from the SSoT (`header_version`, guard g3 pins it against
 * `frame.rs::PROTOCOL_VERSION`). There is deliberately no v1 decode path — see
 * `decodeFrame`.
 */
export const PROTOCOL_VERSION = HEADER_VERSION;

/** Flag bit: the frame can be decoded without predecessors. */
export const FLAG_INDEPENDENTLY_DECODABLE = 0b0000_0001;
/** Flag bit: the sender believes this frame can be dropped without affecting others. */
export const FLAG_DISCARDABLE = 0b0000_0010;
/** Flag bit: a fixed-size wrapped transmit key follows the stream sequence. */
export const FLAG_KEY_BEARING = 0b0000_0100;

/** Decoded flag bits. */
export interface FrameFlags {
  readonly independentlyDecodable: boolean;
  readonly discardable: boolean;
  readonly keyBearing: boolean;
}

/** One decoded TLV extension. Value is a slice of the received buffer. */
export interface FrameExtension {
  readonly extType: number;
  readonly value: Bytes;
}

/** The wrapped transmit key block, present iff the key-bearing flag is set. */
export interface WrappedTransmitKey {
  readonly kekGeneration: number;
  /** 32-byte wrapped key concatenated with its 16-byte tag — the AEAD input. */
  readonly wrappedKeyWithTag: Bytes;
}

/**
 * A decoded frame. Holds SLICES of the input buffer, never copies.
 *
 * Carries no method that decrypts. Opening a frame requires a `VerifiedFrame`,
 * which only `verifyFrame` in `./receivePath.ts` can construct — so
 * verify-before-decrypt is a type constraint rather than a calling convention.
 */
export interface DecodedFrame {
  readonly version: number;
  readonly flags: FrameFlags;
  readonly payloadLength: number;
  readonly streamSequence: number;
  readonly wrappedTransmitKey: WrappedTransmitKey | null;
  readonly extensions: readonly FrameExtension[];
  /**
   * Which of the subscriber's slots this frame fills.
   *
   * UNAUTHENTICATED. A media handler writes this freely and nobody signs it. A
   * receiver must validate it against its own declared slots before acting on it
   * — otherwise a compromised relay gains 16 free bits per frame and can place a
   * frame in a slot the subscriber never assigned. That check needs the slot
   * declaration, which arrives with the layout subscription, so it lands at story
   * task 19; it is tracked in `docs/TODO.md`. Exposed here so the check is purely
   * additive when it arrives.
   */
  readonly streamId: number;
  /** Per (connection, stream) transmit counter. Unauthenticated; see `streamId`. */
  readonly hopSequence: number;
  /** The SFrame object bytes. A slice. */
  readonly payload: Bytes;
  /** The trailing Ed25519 signature. A slice. */
  readonly signature: Bytes;
  /**
   * The AEAD associated data: the publisher region ONLY. A slice of the input.
   * Deviation (1) in the module header.
   */
  readonly aeadAad: Bytes;
  /**
   * The Ed25519 signed range: publisher region ‖ payload, skipping the relay
   * region. Deviation (2) in the module header.
   *
   * This is the ONE value in this interface that is not a single contiguous
   * slice — it cannot be, because the range is not contiguous on the wire. It is
   * built from two `subarray` views of the received buffer and never from
   * re-serialized fields.
   */
  readonly signedRange: Bytes;
  /** Byte offsets, for assertions and for callers that need to re-slice. */
  readonly offsets: {
    readonly publisherRegionLen: number;
    readonly relayRegionOffset: number;
    readonly payloadOffset: number;
    readonly signatureOffset: number;
  };
}

function need(available: number, wanted: number, what: string, at: number): void {
  if (available < wanted) {
    throw codecReject(
      'truncated',
      `frame ends inside ${what}: ${wanted} bytes required, ${available} available`,
      { at, declared: wanted, available },
    );
  }
}

/**
 * Parse the TLV extension region against the registry.
 *
 * The rejection semantics travel WITH the table, not just the entries: a decoder
 * could pin an identical registry and still SKIP unknown types, which is the
 * ADR-0036 §2 defect behind a perfectly matching table. All four are enforced —
 * unknown type, duplicate type, ascending order, and the accepted VALUE SET.
 *
 * The accepted set matters as much as the length and type: §7 treats
 * publisher-supplied selection signals as untrusted, and a value outside the
 * declared range is a publisher trying to outrank its priority group.
 *
 * NOTHING OUTSIDE THIS REPOSITORY DEFINES THIS GRAMMAR. Unlike the AAD span,
 * which is externally anchored against RFC 9605, the registry is ours alone: no
 * outside authority says type `0x01` means salience or that its accepted set is
 * `0..=100`. The vectors covering it prove internal consistency only.
 */
function parseExtensions(region: Uint8Array, regionOffset: number): FrameExtension[] {
  const out: FrameExtension[] = [];
  const seen = new Set<number>();
  let previousType = -1;
  let i = 0;

  while (i < region.length) {
    if (region.length - i < 2) {
      throw codecReject(
        'extensions_malformed',
        `extension entry header is truncated: ${region.length - i} of 2 bytes`,
        { at: regionOffset + i, available: region.length - i, limit: 2 },
      );
    }
    const extType = region[i] as number;
    const valueLen = region[i + 1] as number;
    i += 2;

    const spec = EXTENSION_REGISTRY.entries.find((e) => e.ext_type === extType);
    if (!spec) {
      // REJECTED, never skipped. Skipping unknown types is the forward-compatible
      // instinct and the ADR-0036 §2 defect: it makes the extension region a
      // covert channel out of a compromised publisher.
      throw codecReject(
        'extensions_malformed',
        `unknown extension type 0x${extType.toString(16)}`,
        {
          at: regionOffset + i - 2,
        },
      );
    }
    if (seen.has(extType)) {
      throw codecReject(
        'extensions_malformed',
        `duplicate extension type 0x${extType.toString(16)}`,
        { at: regionOffset + i - 2 },
      );
    }
    if (extType <= previousType) {
      throw codecReject(
        'extensions_malformed',
        `extension types out of ascending order at 0x${extType.toString(16)}`,
        { at: regionOffset + i - 2 },
      );
    }
    if (valueLen !== spec.value_len) {
      throw codecReject(
        'extensions_malformed',
        `extension type 0x${extType.toString(16)} declares a ${valueLen}-byte value, registry says ${spec.value_len}`,
        { at: regionOffset + i - 1, declared: valueLen, limit: spec.value_len },
      );
    }
    if (region.length - i < valueLen) {
      throw codecReject(
        'extensions_malformed',
        `extension type 0x${extType.toString(16)} value is truncated`,
        { at: regionOffset + i, available: region.length - i, declared: valueLen },
      );
    }
    const value = region.subarray(i, i + valueLen) as Bytes;
    // The accepted SET, not merely the length. `value_len` is 1 for every current
    // entry, so a single byte compared against the declared range.
    const scalar = value[0] as number;
    if (scalar < spec.accepted_min || scalar > spec.accepted_max) {
      throw codecReject(
        'extensions_malformed',
        `extension type 0x${extType.toString(16)} value ${scalar} is outside the accepted set ` +
          `${spec.accepted_min}..=${spec.accepted_max}`,
        { at: regionOffset + i, declared: scalar, limit: spec.accepted_max },
      );
    }

    seen.add(extType);
    previousType = extType;
    i += valueLen;
    out.push({ extType, value });
  }
  return out;
}

/**
 * Decode a v2 frame from a received buffer. Slices, never copies.
 *
 * Check ORDER is load-bearing and is pinned by the vectors:
 *   * `payload_length_exceeds_max` fires BEFORE `payload_length_exceeds_available`,
 *     because the maximum must be enforced before any allocation is sized from an
 *     attacker-controlled field. A reader that pre-allocates from this field is
 *     the defect ADR-0036 §2 names, and it PASSES fuzzing of the decode function
 *     while being wrong in the reader that calls it.
 *   * the relay-region completeness check fires BEFORE the payload availability
 *     check, so a frame cut inside the relay region reports `truncated` (a
 *     structural fault) rather than an availability arithmetic artifact.
 *
 * @throws {FrameRejectedError} with a bounded `rejectReason`.
 */
export function decodeFrame(data: Uint8Array): DecodedFrame {
  need(data.length, PUBLISHER_FIXED_PREFIX_BYTES, 'the fixed publisher prefix', 0);

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  const version = data[0] as number;
  if (version !== PROTOCOL_VERSION) {
    // No v1 decode path exists, deliberately. At the commit that introduced v2
    // nothing deployed spoke v1, so a live v1 branch would have been a downgrade
    // surface with no legitimate caller.
    throw codecReject('unknown_version', `unsupported frame version ${version}`, { at: 0 });
  }

  const rawFlags = data[1] as number;
  if ((rawFlags & ~LEGAL_FLAG_MASK) !== 0) {
    // Every bit is decoded into a field the receiver inspects, or rejected if
    // set. There is deliberately no masking variant of this parser:
    // masking-and-continuing turns the reserved bits into a covert channel out of
    // a compromised media handler that no test would fail.
    throw codecReject(
      'reserved_flag_bit_set',
      `reserved flag bit set in 0b${rawFlags.toString(2).padStart(8, '0')}`,
      { at: 1 },
    );
  }
  const flags: FrameFlags = {
    independentlyDecodable: (rawFlags & FLAG_INDEPENDENTLY_DECODABLE) !== 0,
    discardable: (rawFlags & FLAG_DISCARDABLE) !== 0,
    keyBearing: (rawFlags & FLAG_KEY_BEARING) !== 0,
  };

  const payloadLength = view.getUint32(2, false);
  // BEFORE any allocation, and before the availability check. The parsing trust
  // boundary of ADR-0036 §2.
  if (payloadLength > MAX_PAYLOAD_BYTES) {
    throw codecReject(
      'payload_length_exceeds_max',
      `declared payload length ${payloadLength} exceeds the wire-format maximum ${MAX_PAYLOAD_BYTES}`,
      { at: 2, declared: payloadLength, limit: MAX_PAYLOAD_BYTES },
    );
  }

  const streamSequence = view.getUint32(6, false);
  let cursor = PUBLISHER_FIXED_PREFIX_BYTES;

  let wrappedTransmitKey: WrappedTransmitKey | null = null;
  if (flags.keyBearing) {
    need(data.length - cursor, WRAPPED_TRANSMIT_KEY_BYTES, 'the wrapped transmit key', cursor);
    const kekGeneration = view.getUint16(cursor, false);
    const materialStart = cursor + KEK_GENERATION_FIELD_BYTES;
    wrappedTransmitKey = {
      kekGeneration,
      // The 32-byte wrapped key and its 16-byte tag are handed to the AEAD as one
      // run — combined mode — so they are sliced together rather than split and
      // rejoined.
      wrappedKeyWithTag: data.subarray(
        materialStart,
        materialStart + WRAPPED_KEY_AEAD_BYTES,
      ) as Bytes,
    };
    cursor += WRAPPED_TRANSMIT_KEY_BYTES;
  }

  need(data.length - cursor, EXT_LENGTH_FIELD_BYTES, 'the extension length field', cursor);
  const extLength = view.getUint16(cursor, false);
  cursor += EXT_LENGTH_FIELD_BYTES;
  if (extLength > MAX_EXT_BYTES) {
    // The bound is DERIVED from the registry, not chosen: a grammatically valid
    // region is a subset of the registry with each type at most once, so the
    // maximum is the registry's own total size.
    throw codecReject(
      'extensions_too_large',
      `declared extension region ${extLength} exceeds the registry-derived maximum ${MAX_EXT_BYTES}`,
      { at: cursor - EXT_LENGTH_FIELD_BYTES, declared: extLength, limit: MAX_EXT_BYTES },
    );
  }
  need(data.length - cursor, extLength, 'the extension region', cursor);
  const extensions = parseExtensions(data.subarray(cursor, cursor + extLength), cursor);
  cursor += extLength;

  const publisherRegionLen = cursor;

  need(data.length - cursor, RELAY_REGION_BYTES, 'the relay region', cursor);
  const streamId = view.getUint16(cursor, false);
  const hopSequence = view.getUint32(cursor + 2, false);
  cursor += RELAY_REGION_BYTES;

  const payloadOffset = cursor;
  need(data.length - cursor, SIGNATURE_BYTES, 'the trailing signature', cursor);
  const availableForPayload = data.length - cursor - SIGNATURE_BYTES;
  if (payloadLength > availableForPayload) {
    throw codecReject(
      'payload_length_exceeds_available',
      `declared payload length ${payloadLength} exceeds the ${availableForPayload} bytes this datagram carries`,
      { at: 2, declared: payloadLength, available: availableForPayload },
    );
  }

  const signatureOffset = payloadOffset + payloadLength;
  const expectedLength = signatureOffset + SIGNATURE_BYTES;
  if (data.length > expectedLength) {
    // QUIC preserves datagram boundaries, so this is never a transport artifact —
    // some endpoint wrote it, and the signature covers neither those bytes nor
    // their absence.
    throw codecReject(
      'trailing_bytes',
      `${data.length - expectedLength} byte(s) follow the signature`,
      { at: expectedLength, available: data.length, declared: expectedLength },
    );
  }

  const publisherRegion = data.subarray(0, publisherRegionLen) as Bytes;
  const payload = data.subarray(payloadOffset, signatureOffset) as Bytes;

  return {
    version,
    flags,
    payloadLength,
    streamSequence,
    wrappedTransmitKey,
    extensions,
    streamId,
    hopSequence,
    payload,
    signature: data.subarray(signatureOffset, expectedLength) as Bytes,
    // Deviation (1): the publisher region ONLY. A slice of the received buffer.
    aeadAad: publisherRegion,
    // Deviation (2): publisher region ‖ payload, SKIPPING the relay region that
    // sits between them on the wire. Both halves are `subarray` views; nothing is
    // re-serialized from the decoded fields above.
    signedRange: concatBytes(publisherRegion, payload),
    offsets: {
      publisherRegionLen,
      relayRegionOffset: publisherRegionLen,
      payloadOffset,
      signatureOffset,
    },
  };
}

/** Fields an encoder needs to build a frame. */
export interface EncodeFrameInput {
  readonly flags: FrameFlags;
  readonly streamSequence: number;
  readonly wrappedTransmitKey: WrappedTransmitKey | null;
  readonly extensions: readonly FrameExtension[];
  readonly streamId: number;
  readonly hopSequence: number;
  readonly payload: Uint8Array;
}

/** A frame built up to, but not including, its signature. */
export interface UnsignedFrame {
  /** Everything except the trailing 64 signature bytes. */
  readonly bytes: Bytes;
  /** The AEAD associated data — a slice of `bytes`. */
  readonly aeadAad: Bytes;
  /** The Ed25519 signed range — publisher region ‖ payload. */
  readonly signedRange: Bytes;
}

/**
 * Build a frame up to its signature.
 *
 * THE ENTIRE PUBLISHER REGION IS POPULATED BEFORE EITHER RANGE IS TAKEN. That is
 * the encode-side mirror of the decode rule: the wrapped-key block and
 * `payload_length` are written first, and only then are the AAD and signed range
 * sliced out. A range taken over a half-filled buffer authenticates zeros, and
 * the resulting frame verifies against nothing.
 *
 * The caller seals the payload with `aeadAad`, then signs `signedRange`, then
 * appends the signature. `payload` must therefore already be the sealed SFrame
 * object — which is why the AAD must be available before the payload exists, and
 * why the AAD covers the publisher region only.
 */
export function buildUnsignedFrame(input: EncodeFrameInput): UnsignedFrame {
  const { flags, wrappedTransmitKey, extensions, payload } = input;

  if (flags.keyBearing !== (wrappedTransmitKey !== null)) {
    throw new RangeError(
      'buildUnsignedFrame: the key-bearing flag and the wrapped transmit key must agree',
    );
  }
  if (payload.length > MAX_PAYLOAD_BYTES) {
    throw codecReject(
      'payload_length_exceeds_max',
      `payload of ${payload.length} bytes exceeds the wire-format maximum ${MAX_PAYLOAD_BYTES}`,
      { declared: payload.length, limit: MAX_PAYLOAD_BYTES },
    );
  }

  const extBytes = concatBytes(
    ...extensions.map((e) => concatBytes(Uint8Array.of(e.extType, e.value.length), e.value)),
  );
  if (extBytes.length > MAX_EXT_BYTES) {
    throw codecReject(
      'extensions_too_large',
      `extension region of ${extBytes.length} bytes exceeds the maximum ${MAX_EXT_BYTES}`,
      { declared: extBytes.length, limit: MAX_EXT_BYTES },
    );
  }

  const publisherRegionLen =
    PUBLISHER_FIXED_PREFIX_BYTES +
    (wrappedTransmitKey ? WRAPPED_TRANSMIT_KEY_BYTES : 0) +
    EXT_LENGTH_FIELD_BYTES +
    extBytes.length;
  const total = publisherRegionLen + RELAY_REGION_BYTES + payload.length;

  const bytes = new Uint8Array(total);
  const view = new DataView(bytes.buffer);
  bytes[0] = PROTOCOL_VERSION;
  bytes[1] =
    (flags.independentlyDecodable ? FLAG_INDEPENDENTLY_DECODABLE : 0) |
    (flags.discardable ? FLAG_DISCARDABLE : 0) |
    (flags.keyBearing ? FLAG_KEY_BEARING : 0);
  view.setUint32(2, payload.length, false);
  view.setUint32(6, input.streamSequence, false);

  let cursor = PUBLISHER_FIXED_PREFIX_BYTES;
  if (wrappedTransmitKey) {
    view.setUint16(cursor, wrappedTransmitKey.kekGeneration, false);
    bytes.set(wrappedTransmitKey.wrappedKeyWithTag, cursor + KEK_GENERATION_FIELD_BYTES);
    cursor += WRAPPED_TRANSMIT_KEY_BYTES;
  }
  view.setUint16(cursor, extBytes.length, false);
  cursor += EXT_LENGTH_FIELD_BYTES;
  bytes.set(extBytes, cursor);
  cursor += extBytes.length;

  // The publisher region is now COMPLETE. Only here may it be sliced.
  const publisherRegion = bytes.subarray(0, publisherRegionLen) as Bytes;

  view.setUint16(cursor, input.streamId, false);
  view.setUint32(cursor + 2, input.hopSequence, false);
  cursor += RELAY_REGION_BYTES;
  bytes.set(payload, cursor);

  return {
    bytes: bytes as Bytes,
    aeadAad: publisherRegion,
    signedRange: concatBytes(publisherRegion, bytes.subarray(cursor, cursor + payload.length)),
  };
}

/** Append a signature to an unsigned frame, producing the wire bytes. */
export function finishFrame(unsigned: UnsignedFrame, signature: Uint8Array): Bytes {
  if (signature.length !== SIGNATURE_BYTES) {
    throw new RangeError(`frame signature must be ${SIGNATURE_BYTES} bytes`);
  }
  return concatBytes(unsigned.bytes, signature);
}
