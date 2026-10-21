// File: packages/sdk-core/src/media/frame/frameCodec.ts
//
// MODULE THREE of the SFrame stack: the ADR-0036 §2 binary frame codec — header
// v3, the only version — and the two authenticated byte ranges the rest of the
// stack consumes.
//
// ---------------------------------------------------------------------------
// PROVENANCE OF THE v3 CODEC (R-22). READ BEFORE "SIMPLIFYING" IT.
// ---------------------------------------------------------------------------
//
// The v3 encode and decode below were authored by task C-v3 from ADR-0036 §2
// "Header v3 — the normative layout" and the story's Design §Wire freeze ONLY,
// before any v3 vector or v3 Rust codec existed. Nothing here was read off
// `crates/media-protocol` or the vector generator.
//
// That independence is the whole value of the cross-language vectors: they catch
// a defect only when the TypeScript and Rust codecs are two separate readings of
// the same table. REPLACING EITHER SIDE WITH A PORT OF THE OTHER HOLLOWS OUT THE
// R-22 GATE — both would then share every misreading, and the vectors would
// compare one implementation with itself. Fix a disagreement against the ADR
// table (and get the table fixed if it is the table), never by copying the
// other language.
//
// ANCHOR (DRY): every wire constant comes from `./wireConstants.js`, which is
// RENDERED from `proto/test-vectors/frame-v3.vectors.json` — the cross-language
// SSoT. Sites below read `WIRE_CONSTANTS.max_payload_bytes` and
// `WIRE_CONSTANTS.legal_flag_mask` by their SSoT spellings. Never hardcode a
// second copy: the origin is `crates/media-protocol/src/frame.rs`, the vectors
// file mirrors it under guards g2/g3/g4, and
// `__tests__/wireConstants.drift.test.ts` byte-compares the render. `legal_flag_mask`
// matters most here — it is 7, so no banned-literal check could ever catch a
// hardcode of it, and the derivation is the only enforcement there is.
//
// What the SSoT pins is the WIDTHS; the OFFSETS are this file's reading of the
// ADR table (`GOP_COUNTER_OFFSET` = `stream_sequence`'s end), checked against the
// SSoT's prefix width at module load and against the vectors' pinned offsets by
// the conformance harness.
//
// ---------------------------------------------------------------------------
// LAYOUT (ADR-0036 §2 Header v3 table), BIG-ENDIAN THROUGHOUT
// ---------------------------------------------------------------------------
//
//   off  size  field                      region
//     0     1  version (= 3)              publisher  \
//     1     1  flags                      publisher   |
//     2     4  payload_length             publisher   |  AEAD associated data
//     6     4  stream_sequence            publisher   |  AND signed
//    10     4  gop_counter (new in v3)    publisher   |
//    14    50  wrapped transmit key       publisher   |  (iff key-bearing)
//     .     2  ext_length                 publisher   |
//     .     n  extension TLVs             publisher  /
//     .     2  stream_id                  relay      \  NEITHER signed
//     .     4  hop_sequence               relay      /  NOR in the AAD
//     .     m  payload (SFrame object)               <- signed, NOT in the AAD
//     .    64  Ed25519 signature
//
// v3 is the only version (R-22): v2 was an intentional wire break with no dual
// decode, and any other version byte rejects `unknown_version`.
//
// The relay `stream_id` is HOP-LOCAL (ADR-0036 §2): on the uplink the publisher
// stamps MC's send id there, on the downlink MH rewrites it to the receiver's
// slot id. The codec takes it as an input and never interprets it.
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
// `gop_counter` sits inside the fixed publisher prefix, so it is covered by the
// AAD and by the signature exactly as `stream_sequence` is; the relay region is
// the one excluded run.
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
  gop_counter_field_bytes: GOP_COUNTER_FIELD_BYTES,
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
 * Relay-region field widths: `stream_id(2) || hop_sequence(4)`.
 *
 * Named rather than left as literals at the two `DataView` calls that use them,
 * because {@link writeHopSequence} needs the hop-sequence OFFSET and deriving it
 * from an unnamed `+ 2` is how a field width drifts from its reader.
 * `__tests__/frameCodec.test.ts` asserts the two sum to `relay_region_bytes`.
 */
const STREAM_ID_FIELD_BYTES = 2;
const HOP_SEQUENCE_FIELD_BYTES = 4;

/**
 * Protocol version, from the SSoT (`header_version`, guard g3 pins it against
 * `frame.rs::PROTOCOL_VERSION`). The ONLY version this codec encodes or decodes,
 * and therefore the only one a join declares and the publish path sends.
 */
export const PROTOCOL_VERSION = HEADER_VERSION;

/** Field positions in the fixed prefix (ADR-0036 §2 table). */
const VERSION_OFFSET = 0;
const FLAGS_OFFSET = 1;
const PAYLOAD_LENGTH_OFFSET = 2;
const STREAM_SEQUENCE_OFFSET = 6;
/** u32, like `payload_length` and `hop_sequence`. */
const STREAM_SEQUENCE_FIELD_BYTES = 4;

/**
 * Where `gop_counter` sits: immediately after `stream_sequence` — the table's
 * own relation, offset 10. `frameCodec.v3.test.ts` pins 10 and 14 literally.
 */
const GOP_COUNTER_OFFSET = STREAM_SEQUENCE_OFFSET + STREAM_SEQUENCE_FIELD_BYTES;

// The prefix is `gop_counter`'s end. Checked against the SSoT's declared width
// once, at load, so a vectors file and this layout cannot disagree silently.
if (GOP_COUNTER_OFFSET + GOP_COUNTER_FIELD_BYTES !== PUBLISHER_FIXED_PREFIX_BYTES) {
  throw new Error(
    'frameCodec: the ADR-0036 §2 prefix layout disagrees with wire_constants.publisher_fixed_prefix_bytes',
  );
}

/**
 * The header versions this SDK encodes, decodes, SENDS and therefore declares at
 * join (`ParticipantCapabilities.supported_header_versions`): exactly
 * {@link PROTOCOL_VERSION}. One set for all four, because with one version
 * there is nothing to distinguish them — no allowlist, no floor (R-22).
 */
export const SUPPORTED_HEADER_VERSIONS: readonly number[] = [PROTOCOL_VERSION];

/** The one membership predicate for {@link SUPPORTED_HEADER_VERSIONS}. */
export function isSupportedHeaderVersion(version: number): boolean {
  return SUPPORTED_HEADER_VERSIONS.includes(version);
}

/**
 * Refuse an encoder input that `DataView` would silently wrap.
 *
 * `setUint32(…, 2 ** 32)` writes 0, `-1` writes 0xFFFFFFFF and `NaN` writes 0 —
 * and `stream_sequence` is the AEAD nonce input, so a silent wrap is a nonce
 * repeat. The ADR's encode-side rule is that an encoder MUST refuse what the
 * decoder would reject; a value that cannot be represented is the limiting case.
 * The bound is derived from the field width, never written as a literal.
 */
function assertUintField(name: string, value: number, widthBytes: number): void {
  const max = 2 ** (8 * widthBytes) - 1;
  if (!Number.isInteger(value) || value < 0 || value > max) {
    // The field NAME and the width only. Never the value: `gop_counter` and the
    // relay ids are stream identity (ADR-0036 §11), and a sequence is a nonce.
    throw new RangeError(`${name} must be an integer in 0..2^${8 * widthBytes}-1`);
  }
}

/** Flag bit: the frame can be decoded without predecessors. */
export const FLAG_INDEPENDENTLY_DECODABLE = 0b0000_0001;
/** Flag bit: the sender believes this frame can be dropped without affecting others. */
export const FLAG_DISCARDABLE = 0b0000_0010;
/** Flag bit: a fixed-size wrapped transmit key follows the fixed publisher prefix. */
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
  /**
   * The v3 GoP counter (ADR-0036 §2), read from the signed publisher prefix.
   * Every u32 is accepted on the wire; its rules bind the sender. It is stream
   * identity: never a log field, metric label, span attribute or test-bus value
   * (§11).
   */
  readonly gopCounter: number;
  readonly wrappedTransmitKey: WrappedTransmitKey | null;
  readonly extensions: readonly FrameExtension[];
  /**
   * The hop-local relay stream id (ADR-0036 §2). On a RECEIVED frame
   * (downlink): which of the subscriber's slots this frame fills. On a SENT
   * frame (uplink): the publisher's send id, `SendStream.stream_number`.
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

/** What a per-version prefix parse hands the shared, version-blind tail. */
interface ParsedPrefix {
  readonly version: number;
  readonly flags: FrameFlags;
  readonly payloadLength: number;
  readonly streamSequence: number;
  readonly gopCounter: number;
  /** Where the cursor stands once the fixed prefix is consumed. */
  readonly prefixLen: number;
}

/**
 * Decode the flag byte.
 *
 * Every bit is decoded into a field the receiver inspects, or rejected if set.
 * There is deliberately no masking variant of this parser: masking-and-continuing
 * turns the reserved bits into a covert channel out of a compromised media
 * handler that no test would fail.
 */
function parseFlags(rawFlags: number): FrameFlags {
  if ((rawFlags & ~LEGAL_FLAG_MASK) !== 0) {
    throw codecReject(
      'reserved_flag_bit_set',
      `reserved flag bit set in 0b${rawFlags.toString(2).padStart(8, '0')}`,
      { at: FLAGS_OFFSET },
    );
  }
  return {
    independentlyDecodable: (rawFlags & FLAG_INDEPENDENTLY_DECODABLE) !== 0,
    discardable: (rawFlags & FLAG_DISCARDABLE) !== 0,
    keyBearing: (rawFlags & FLAG_KEY_BEARING) !== 0,
  };
}

/**
 * Read `payload_length` and bound it. BEFORE any allocation, and before the
 * availability check: the parsing trust boundary of ADR-0036 §2.
 */
function parsePayloadLength(view: DataView): number {
  const payloadLength = view.getUint32(PAYLOAD_LENGTH_OFFSET, false);
  if (payloadLength > MAX_PAYLOAD_BYTES) {
    throw codecReject(
      'payload_length_exceeds_max',
      `declared payload length ${payloadLength} exceeds the wire-format maximum ${MAX_PAYLOAD_BYTES}`,
      { at: PAYLOAD_LENGTH_OFFSET, declared: payloadLength, limit: MAX_PAYLOAD_BYTES },
    );
  }
  return payloadLength;
}

/**
 * The v3 fixed prefix, ADR-0036 §2 decode-order steps 1–8, in that order.
 */
function parsePrefix(data: Uint8Array, view: DataView): ParsedPrefix {
  // Step 1: the version byte is present.
  need(data.length, VERSION_OFFSET + 1, 'the version byte', VERSION_OFFSET);
  // Step 2: version == 3, terminal even on a one-byte buffer.
  const version = data[VERSION_OFFSET] as number;
  if (version !== PROTOCOL_VERSION) {
    // No other version has a decode path, deliberately (R-22): v3 is the only
    // version, a v2 frame is refused here like any other, and a live branch for
    // an older version would be a downgrade surface with no legitimate caller.
    throw codecReject('unknown_version', `unsupported frame version ${version}`, {
      at: VERSION_OFFSET,
    });
  }
  // Step 3: the 14-byte fixed prefix is present.
  need(data.length, PUBLISHER_FIXED_PREFIX_BYTES, 'the fixed publisher prefix', 0);
  // Step 4: no flag bit outside the legal mask.
  const flags = parseFlags(data[FLAGS_OFFSET] as number);
  // Step 5: never key-bearing AND discardable (ADR-0036 §4: a frame carrying a
  // new key must never be droppable, or a receiver that dropped it could not
  // open what follows). A grammar check on the flag byte alone, so it is
  // terminal here, after the broader reserved-bit check and before any length
  // or cursor work.
  if (flags.keyBearing && flags.discardable) {
    throw codecReject('key_bearing_discardable', 'a key-bearing frame is marked discardable', {
      at: FLAGS_OFFSET,
    });
  }
  // Step 6: the payload bound, before anything is sized from it.
  const payloadLength = parsePayloadLength(view);
  return {
    version,
    flags,
    payloadLength,
    // Step 7: every value accepted.
    streamSequence: view.getUint32(STREAM_SEQUENCE_OFFSET, false),
    // Step 8: every u32 accepted, 0 and u32::MAX included. No check, because no
    // value is illegal on the wire; the rules bind the sender.
    gopCounter: view.getUint32(GOP_COUNTER_OFFSET, false),
    prefixLen: PUBLISHER_FIXED_PREFIX_BYTES,
  };
}

/**
 * Everything after the fixed prefix — ADR-0036 §2 decode-order steps 9–14 —
 * working only from the prefix's cursor and flags.
 *
 * Check ORDER is load-bearing and is pinned by the vectors:
 *   * `ext_length` is bounded BEFORE the region's availability is checked.
 *   * the relay-region completeness check fires BEFORE the payload availability
 *     check, so a frame cut inside the relay region reports `truncated` (a
 *     structural fault) rather than an availability arithmetic artifact.
 *   * step 13 is arithmetic, not "are the 64 bytes present": a datagram with the
 *     whole payload but a short signature reports
 *     `payload_length_exceeds_available`.
 */
function decodeAfterPrefix(data: Uint8Array, view: DataView, prefix: ParsedPrefix): DecodedFrame {
  const { flags, payloadLength } = prefix;
  let cursor = prefix.prefixLen;

  // Step 9.
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

  // Step 10: present, then the limit, then (step 11) availability and grammar.
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

  // Step 12.
  need(data.length - cursor, RELAY_REGION_BYTES, 'the relay region', cursor);
  const streamId = view.getUint16(cursor, false);
  const hopSequence = view.getUint32(cursor + STREAM_ID_FIELD_BYTES, false);
  cursor += RELAY_REGION_BYTES;

  // Step 13.
  const payloadOffset = cursor;
  need(data.length - cursor, SIGNATURE_BYTES, 'the trailing signature', cursor);
  const availableForPayload = data.length - cursor - SIGNATURE_BYTES;
  if (payloadLength > availableForPayload) {
    throw codecReject(
      'payload_length_exceeds_available',
      `declared payload length ${payloadLength} exceeds the ${availableForPayload} bytes this datagram carries`,
      { at: PAYLOAD_LENGTH_OFFSET, declared: payloadLength, available: availableForPayload },
    );
  }

  // Step 14 (datagram only — this codec is datagram-only; a stream reader's
  // "need more bytes" split lives with that reader).
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
    version: prefix.version,
    flags,
    payloadLength,
    streamSequence: prefix.streamSequence,
    gopCounter: prefix.gopCounter,
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

function viewOf(data: Uint8Array): DataView {
  return new DataView(data.buffer, data.byteOffset, data.byteLength);
}

/**
 * Decode a frame from a received buffer: ADR-0036 §2 steps 1–14 for ANY input.
 * Slices, never copies.
 *
 * @throws {FrameRejectedError} with a bounded `rejectReason`.
 */
export function decodeFrame(data: Uint8Array): DecodedFrame {
  const view = viewOf(data);
  return decodeAfterPrefix(data, view, parsePrefix(data, view));
}

/** Fields an encoder needs to build a frame. */
export interface EncodeFrameInput extends PublisherRegionInput {
  /**
   * The hop-local relay `stream_id` (ADR-0036 §2). An opaque input: the codec
   * never interprets it. Choosing it (MC's send id on the uplink) is the
   * publish path's job.
   */
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
  /**
   * Byte offset of the relay region within `bytes` (and within the finished
   * frame, since {@link finishFrame} only appends).
   *
   * Exposed so a sender can write the hop sequence AFTER signing — see
   * {@link writeHopSequence}. Returned rather than recomputed by the caller,
   * because recomputing it means re-deriving the publisher-region length from
   * the flags and extension sizes, which is a second implementation of the
   * layout this module owns.
   */
  readonly relayRegionOffset: number;
}

/** Fields of the publisher region. A subset of {@link EncodeFrameInput}. */
export interface PublisherRegionInput {
  /**
   * The header version to build. REQUIRED, with no default: the only runtime
   * source is MC's `SendDirective.header_version` (ADR-0036 §2, meeting-wide and
   * MC-directed). An unsupported value is refused, never coerced.
   */
  readonly headerVersion: number;
  readonly flags: FrameFlags;
  readonly streamSequence: number;
  /**
   * The v3 GoP counter. An opaque input: its semantics (ADR-0036 §2 — per sender
   * and stream, increments on every independently-decodable frame, never resets
   * or wraps) belong to the sender — `TransmitKeyManager.nextGopCounter` —
   * not the codec.
   */
  readonly gopCounter: number;
  readonly wrappedTransmitKey: WrappedTransmitKey | null;
  readonly extensions: readonly FrameExtension[];
}

/**
 * Build the publisher region for a frame whose payload will be `payloadLength`
 * bytes.
 *
 * ---------------------------------------------------------------------------
 * WHY THIS IS EXPORTED, AND WHY IT IS NOT A SECOND SERIALIZER
 * ---------------------------------------------------------------------------
 *
 * The SFrame seal needs the AEAD associated data; the associated data IS the
 * publisher region; the region carries `payload_length`, which is a property of
 * the sealed output. That reads circular and is not: AES-GCM is
 * length-preserving, so the sealed SFrame object's length is known before
 * sealing — `key_id(8) + tag(16) + plaintext.length` — and the region can
 * therefore be built first.
 *
 * The alternative was for the egress path to lay out the region itself and hand
 * the bytes to `buildUnsignedFrame`. That would be a SECOND WRITER of header
 * bytes, i.e. a second wire format that agrees with this one only by review.
 * Instead {@link buildUnsignedFrame} calls this function, so there is exactly
 * one place that knows the layout and the AAD a sender seals under is
 * byte-identical to the region the frame ships with by construction rather than
 * by comparison.
 *
 * THE ENTIRE REGION IS POPULATED BEFORE IT IS RETURNED. That is the encode-side
 * mirror of the decode rule: a range taken over a half-filled buffer
 * authenticates zeros, and the resulting frame verifies against nothing.
 *
 * @throws {FrameRejectedError} `unknown_version` (unsupported `headerVersion`),
 *   `payload_length_exceeds_max`, `extensions_too_large`,
 *   `key_bearing_discardable` and `extensions_malformed`.
 * @throws {RangeError} if the key-bearing flag and the wrapped key disagree, or
 *   if a field value does not fit its width.
 */
export function buildPublisherRegion(input: PublisherRegionInput, payloadLength: number): Bytes {
  const { headerVersion, flags, wrappedTransmitKey, extensions } = input;

  if (!isSupportedHeaderVersion(headerVersion)) {
    throw codecReject('unknown_version', `cannot encode frame header version ${headerVersion}`);
  }
  if (flags.keyBearing !== (wrappedTransmitKey !== null)) {
    throw new RangeError(
      'buildPublisherRegion: the key-bearing flag and the wrapped transmit key must agree',
    );
  }
  if (payloadLength > MAX_PAYLOAD_BYTES) {
    throw codecReject(
      'payload_length_exceeds_max',
      `payload of ${payloadLength} bytes exceeds the wire-format maximum ${MAX_PAYLOAD_BYTES}`,
      { declared: payloadLength, limit: MAX_PAYLOAD_BYTES },
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
  assertEncodeInput(input, extBytes);

  const prefixLen = PUBLISHER_FIXED_PREFIX_BYTES;
  const regionLen =
    prefixLen +
    (wrappedTransmitKey ? WRAPPED_TRANSMIT_KEY_BYTES : 0) +
    EXT_LENGTH_FIELD_BYTES +
    extBytes.length;

  const region = new Uint8Array(regionLen);
  const view = new DataView(region.buffer);
  writeFixedPrefix(region, view, input, payloadLength);

  // Everything after the fixed prefix, laid out ONCE from the cursor — the
  // mirror of the decoder's version-blind tail.
  let cursor = prefixLen;
  if (wrappedTransmitKey) {
    view.setUint16(cursor, wrappedTransmitKey.kekGeneration, false);
    region.set(wrappedTransmitKey.wrappedKeyWithTag, cursor + KEK_GENERATION_FIELD_BYTES);
    cursor += WRAPPED_TRANSMIT_KEY_BYTES;
  }
  view.setUint16(cursor, extBytes.length, false);
  cursor += EXT_LENGTH_FIELD_BYTES;
  region.set(extBytes, cursor);

  return region as Bytes;
}

/**
 * The encode-side refusals (ADR-0036 §2 "Encode side": refuse every frame the
 * decoder rejects) beyond the version, flag/wrap pairing and size bounds
 * checked by the caller. Called once.
 */
function assertEncodeInput(input: PublisherRegionInput, extBytes: Uint8Array): void {
  if (input.flags.keyBearing && input.flags.discardable) {
    throw codecReject('key_bearing_discardable', 'a key-bearing frame must not be discardable');
  }
  assertUintField('stream_sequence', input.streamSequence, STREAM_SEQUENCE_FIELD_BYTES);
  assertUintField('gop_counter', input.gopCounter, GOP_COUNTER_FIELD_BYTES);
  if (input.wrappedTransmitKey) {
    assertUintField(
      'kek_generation',
      input.wrappedTransmitKey.kekGeneration,
      KEK_GENERATION_FIELD_BYTES,
    );
  }
  // "an extension region ... outside the registry grammar": checked by the
  // DECODER'S OWN grammar function over the bytes about to be written, so encode
  // and decode cannot disagree on the grammar.
  parseExtensions(extBytes, 0);
}

/** Write the fixed prefix for `input.headerVersion`; the caller lays out the rest. */
function writeFixedPrefix(
  region: Uint8Array,
  view: DataView,
  input: PublisherRegionInput,
  payloadLength: number,
): void {
  const { flags } = input;
  region[VERSION_OFFSET] = input.headerVersion;
  region[FLAGS_OFFSET] =
    (flags.independentlyDecodable ? FLAG_INDEPENDENTLY_DECODABLE : 0) |
    (flags.discardable ? FLAG_DISCARDABLE : 0) |
    (flags.keyBearing ? FLAG_KEY_BEARING : 0);
  view.setUint32(PAYLOAD_LENGTH_OFFSET, payloadLength, false);
  view.setUint32(STREAM_SEQUENCE_OFFSET, input.streamSequence, false);
  view.setUint32(GOP_COUNTER_OFFSET, input.gopCounter, false);
}

/**
 * Overwrite the relay region's hop sequence in a built frame.
 *
 * ---------------------------------------------------------------------------
 * SAFE AFTER SIGNING, AND ASSIGNED AT DEQUEUE FOR A REASON THAT SPANS SERVICES
 * ---------------------------------------------------------------------------
 *
 * The relay region is excluded from BOTH the AEAD associated data (which is the
 * publisher region only) and the Ed25519 signed range (publisher ‖ payload), so
 * rewriting it invalidates nothing. That exclusion is exactly what ADR-0036 §2's
 * publisher/relay split exists to permit, and it is what a media handler does to
 * this same region on every forward.
 *
 * ADR-0036 requires the hop sequence to count ONLY FRAMES ACTUALLY SENT. A
 * sender with a bounded drop-oldest egress queue must therefore assign it at
 * DEQUEUE, not at enqueue, and the difference has a two-service consequence:
 *
 *   * At dequeue, a client-side queue drop leaves the hop sequence CONTIGUOUS on
 *     the wire and the gap lands on the stream sequence instead. The receiving
 *     media handler's uplink hop-gap detector therefore sees no gap — which is
 *     CORRECT, because the client already counted that drop on
 *     `dt_client_media_send_dropped_total`, and ADR-0036 §11's whole point is
 *     that the client-side drop is the one the handler structurally cannot see.
 *   * At enqueue, the same drop would punch a hop gap too, and the handler would
 *     report it as network loss on the client-to-handler path — the same drop
 *     counted twice, in two services, under two different causes, one of which
 *     is wrong.
 *
 * Nothing in the tree would catch an inversion, so this comment is the control,
 * named as the weaker form it is. It is the reciprocal of the far-end obligation
 * recorded in `docs/TODO.md` (the media handler's missing INGRESS hop-gap
 * counter): that counter's meaning depends on this choice, so the choice is
 * written down where the number is generated.
 *
 * @param frame the built frame, mutated in place.
 * @param relayRegionOffset from {@link UnsignedFrame.relayRegionOffset}.
 */
export function writeHopSequence(
  frame: Uint8Array,
  relayRegionOffset: number,
  hopSequence: number,
): void {
  assertUintField('hop_sequence', hopSequence, HOP_SEQUENCE_FIELD_BYTES);
  const hopOffset = relayRegionOffset + STREAM_ID_FIELD_BYTES;
  if (hopOffset + HOP_SEQUENCE_FIELD_BYTES > frame.length) {
    throw new RangeError(
      `writeHopSequence: relay region at ${relayRegionOffset} does not fit in a ` +
        `${frame.length}-byte frame`,
    );
  }
  new DataView(frame.buffer, frame.byteOffset, frame.byteLength).setUint32(
    hopOffset,
    hopSequence,
    false,
  );
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
  const { payload } = input;

  // ONE writer of header bytes. The region is built by `buildPublisherRegion`
  // and copied in whole; this function never lays out publisher fields itself,
  // so a sender that pre-computed the AAD gets a byte-identical region here by
  // construction rather than by comparison.
  const publisherRegionBytes = buildPublisherRegion(input, payload.length);
  // The relay ids are opaque but must still be representable.
  assertUintField('stream_id', input.streamId, STREAM_ID_FIELD_BYTES);
  assertUintField('hop_sequence', input.hopSequence, HOP_SEQUENCE_FIELD_BYTES);
  const publisherRegionLen = publisherRegionBytes.length;
  const total = publisherRegionLen + RELAY_REGION_BYTES + payload.length;

  const bytes = new Uint8Array(total);
  bytes.set(publisherRegionBytes, 0);
  const view = new DataView(bytes.buffer);

  // The publisher region is now COMPLETE. Only here may it be sliced.
  const publisherRegion = bytes.subarray(0, publisherRegionLen) as Bytes;

  let cursor = publisherRegionLen;
  view.setUint16(cursor, input.streamId, false);
  view.setUint32(cursor + STREAM_ID_FIELD_BYTES, input.hopSequence, false);
  cursor += RELAY_REGION_BYTES;
  bytes.set(payload, cursor);

  return {
    bytes: bytes as Bytes,
    aeadAad: publisherRegion,
    signedRange: concatBytes(publisherRegion, bytes.subarray(cursor, cursor + payload.length)),
    relayRegionOffset: publisherRegionLen,
  };
}

/** Append a signature to an unsigned frame, producing the wire bytes. */
export function finishFrame(unsigned: UnsignedFrame, signature: Uint8Array): Bytes {
  if (signature.length !== SIGNATURE_BYTES) {
    throw new RangeError(`frame signature must be ${SIGNATURE_BYTES} bytes`);
  }
  return concatBytes(unsigned.bytes, signature);
}
