// File: packages/sdk-core/src/media/frame/wireConstants.ts
//
// GENERATED — DO NOT EDIT.
//
//   Renderer: packages/sdk-core/scripts/gen-wire-constants.mjs
//   Source:   proto/test-vectors/frame-v2.vectors.json
//   Regenerate: node packages/sdk-core/scripts/gen-wire-constants.mjs --write
//
// ---------------------------------------------------------------------------
// THE CHANGE YOU WANT TO MAKE ALMOST CERTAINLY BELONGS AT THE FAR END OF THIS
// CHAIN, IN RUST. READ THIS BEFORE EDITING A NUMBER BELOW.
// ---------------------------------------------------------------------------
//
//   crates/media-protocol/src/frame.rs        <- the origin; the constants are declared here
//        |  guard g2 / g3 / g4 (scripts/guards/simple/validate-frame-vectors.sh)
//        v
//   proto/test-vectors/frame-v2.vectors.json
//        |  __tests__/wireConstants.drift.test.ts  (re-render + byte-compare)
//        v
//   this file
//
// Every hop is guarded, which is what makes three hops acceptable rather than
// three drift surfaces. Editing THIS file makes the byte-compare red, and the
// tempting repair — re-rendering afterwards — silently discards the edit. Change
// the Rust constant, regenerate the vectors, then regenerate this.
//
// ---------------------------------------------------------------------------
// PROPERTY NAMES ARE THE SSoT's OWN SPELLINGS, ON PURPOSE
// ---------------------------------------------------------------------------
//
// `snake_case` rather than `SCREAMING_SNAKE` so a consumer site reads
// `WIRE_CONSTANTS.max_payload_bytes` — the exact key name in the source file.
// That keeps the traceability visible at the call site, and it is what guard
// g12 greps for in the two enumerated production sites (`frameCodec.ts`,
// `sframe.ts`). Renaming these to TypeScript casing would break that link.

/** Frame header version. ADR-0036 §2; mirrors `frame.rs::PROTOCOL_VERSION`. */
export const HEADER_VERSION = 2;

/**
 * Maximum payload length, enforced BEFORE the payload slice is taken.
 *
 * ADR-0036 §2 calls the length field a parsing trust boundary: a reader that
 * pre-allocates from this attacker-controlled field is the defect. Cross-language
 * SSoT — guard g2 pins it against `frame.rs::MAX_PAYLOAD_BYTES`.
 */
export const MAX_PAYLOAD_BYTES = 1048576;

/**
 * RFC 9605 ciphersuite id. `AES_256_GCM_SHA512_128` (RFC 9605 §8.1), which
 * ADR-0036 §4 selects. The suite fixes the hash, so HKDF is SHA-512 throughout
 * and the extract PRK is 64 bytes, not 32.
 */
export const CIPHER_SUITE_ID = 5;

/** Wire field widths and region sizes, keyed by their SSoT spellings. */
export const WIRE_CONSTANTS = {
  legal_flag_mask: 7,
  signature_bytes: 64,
  aead_tag_bytes: 16,
  key_id_bytes: 8,
  relay_region_bytes: 6,
  publisher_fixed_prefix_bytes: 10,
  wrapped_transmit_key_bytes: 50,
  kek_generation_field_bytes: 2,
  wrapped_transmit_key_material_bytes: 32,
  ext_length_field_bytes: 2,
  max_ext_bytes: 3,
  max_payload_bytes: 1048576,
  cipher_suite_id: 5,
  key_id_layout: {
    sender_id_bits: 16,
    stream_bits: 8,
    generation_bits: 40,
  },
} as const;

/**
 * The publisher-set TLV registry.
 *
 * The rejection semantics travel WITH the table, not just the entries: a decoder
 * could pin an identical registry and still skip unknown types, which is the
 * ADR-0036 §2 defect behind a perfectly matching table.
 */
export const EXTENSION_REGISTRY = {
  entries: [
    {
      ext_type: 1,
      value_len: 1,
      accepted_min: 0,
      accepted_max: 100,
    },
  ],
  unknown_type_is_rejected: true,
  duplicate_type_is_rejected: true,
  ascending_type_order_required: true,
  value_outside_accepted_set_is_rejected: true,
} as const;
