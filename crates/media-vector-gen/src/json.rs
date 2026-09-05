//! Serde shapes for `proto/test-vectors/frame-v2.vectors.json`.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! Field **declaration order is emission order** under `serde_json`, so the
//! output is byte-stable: regenerating with nothing changed leaves `git diff`
//! empty. A churny generator makes the drift guard useless within two stories.
//!
//! Every integer wider than 32 bits is a lowercase unprefixed even-length hex
//! **string** (`key_id`, `generation`); everything 32 bits or narrower is a JSON
//! number. A 64-bit key id as a JSON number would lose precision in every
//! JavaScript reader, silently, and only for large values.
//!
//! There are no `null`s: an absent field beats a null for a TypeScript
//! discriminated union over `kind`.

use serde::Serialize;

/// Top-level document.
#[derive(Serialize)]
pub struct VectorFile {
    /// Loud non-production banner; guard check g1 asserts it is present.
    ///
    /// The three fields below are renamed rather than named with a leading
    /// underscore in Rust. The JSON keys keep the underscore — it sorts them
    /// above the data and marks them as commentary — but a `_`-prefixed Rust
    /// field reads to the compiler as deliberately unused, which is false: g1
    /// and g13 both assert on them.
    #[serde(rename = "_non_production")]
    pub non_production: String,
    /// `kind` is a fixture classifier and never a metric label value.
    #[serde(rename = "_comment_kind_is_not_a_metric_label")]
    pub comment_kind_is_not_a_metric_label: String,
    /// Fixture-key legibility note; the property g13 enforces.
    #[serde(rename = "_comment_fixture_keys")]
    pub comment_fixture_keys: String,
    /// What `derived` and `decoded` mean on a row whose frame does not decode.
    #[serde(rename = "_comment_derived_on_reject_rows")]
    pub comment_derived_on_reject_rows: String,
    /// Schema version of this document.
    pub schema_version: u32,
    /// Equals `media_protocol::frame::PROTOCOL_VERSION`; guard check g3.
    pub header_version: u8,
    /// RFC 9605 §8.1 ciphersuite id.
    pub cipher_suite_id: u16,
    /// Human-readable ciphersuite name.
    pub cipher_suite_name: String,
    /// Cross-language `SSoT` for `MAX_PAYLOAD_BYTES`; guard check g2.
    pub max_payload_bytes: u64,
    /// Per-codec gating claim. The file never claims a green it does not have.
    pub gated_by: GatedBy,
    /// True only when every [`GatedBy`] value is true; guard check g14.
    pub cross_language_property_established: bool,
    /// Wire constants neither codec may hardcode independently.
    pub wire_constants: WireConstants,
    /// The TLV registry, including its rejection semantics.
    pub extension_registry: ExtensionRegistry,
    /// The complete R-25 reason vocabulary.
    pub reject_reasons: Vec<RejectReasonEntry>,
    /// Which `crypto`-block members are secret-bearing; guard check g13.
    pub key_material_fields: Vec<String>,
    /// Which `crypto`-block members are derived or public; guard check g13.
    pub derived_public_fields: Vec<String>,
    /// The vector rows.
    pub vectors: Vec<Row>,
}

/// Per-codec conformance claim.
#[derive(Serialize)]
pub struct GatedBy {
    /// The Rust reference conformance, live from task 8.
    pub rust: bool,
    /// The `TypeScript` codec conformance. `false` until story task 15.
    pub typescript: bool,
}

/// Wire constants pinned cross-language.
#[derive(Serialize)]
pub struct WireConstants {
    /// Every defined flag bit `OR`ed together.
    pub legal_flag_mask: u8,
    /// Ed25519 signature length.
    pub signature_bytes: u32,
    /// AEAD tag length.
    pub aead_tag_bytes: u32,
    /// `SFrame` key-id length.
    pub key_id_bytes: u32,
    /// Relay-region length.
    pub relay_region_bytes: u32,
    /// Unconditional publisher-region prefix length.
    pub publisher_fixed_prefix_bytes: u32,
    /// Wrapped-transmit-key field length when key-bearing. Equals
    /// `kek_generation_field_bytes + wrapped_transmit_key_material_bytes +
    /// aead_tag_bytes`; carried alongside its parts so the cross-language codec
    /// slices the block from the `SSoT` instead of deriving one part by
    /// subtraction (which would collapse two `media-protocol` constants that
    /// agree only by coincidence).
    pub wrapped_transmit_key_bytes: u32,
    /// KEK-generation field width, first field inside the wrapped-key block.
    pub kek_generation_field_bytes: u32,
    /// Wrapped transmit-key material length (AES-256), second field inside the
    /// wrapped-key block, ahead of the 16-byte wrap tag.
    pub wrapped_transmit_key_material_bytes: u32,
    /// Extension-length field width.
    pub ext_length_field_bytes: u32,
    /// Registry-derived maximum extension-region size.
    pub max_ext_bytes: u32,
    /// Key-id field widths.
    pub key_id_layout: KeyIdLayout,
}

/// `sender_id(16) | stream(8) | generation(40)`, big-endian.
#[derive(Serialize)]
pub struct KeyIdLayout {
    /// Width of `sender_id` in bits.
    pub sender_id_bits: u32,
    /// Width of the sender-scoped `stream` in bits. **Not** the relay
    /// `stream_id`, which is subscriber-scoped and two bytes wide.
    pub stream_bits: u32,
    /// Width of `generation` in bits.
    pub generation_bits: u32,
}

/// The TLV registry and, crucially, its rejection semantics.
///
/// Four `bool`s is over clippy's `struct_excessive_bools` threshold. The
/// `expect` is deliberate and the alternative was tried and reverted: grouping
/// them into a nested struct silenced the lint but **changed the published JSON
/// shape** — a cross-language contract altered to satisfy a style lint, which
/// is the wrong trade, and it did not even remove the lint (the nested struct
/// still held four `bool`s).
///
/// This is a serde wire shape: each field is a JSON key read by name across two
/// languages, and the four are independent facts about the registry with no
/// natural grouping between them. The lint's real hazard — positional
/// construction where adjacent `bool`s get transposed — cannot occur here:
/// there is exactly one construction site and it names every field.
#[expect(
    clippy::struct_excessive_bools,
    reason = "serde wire shape; each bool is a named JSON key read cross-language, \
              constructed once by name, never positionally. Nesting them to satisfy \
              the lint would change a published contract and would not silence it."
)]
#[derive(Serialize)]
pub struct ExtensionRegistry {
    /// Why the rejection semantics travel with the table.
    #[serde(rename = "_comment")]
    pub comment: String,
    /// Registry entries.
    pub entries: Vec<ExtensionEntry>,
    /// A type not in the registry is rejected, never skipped.
    pub unknown_type_is_rejected: bool,
    /// A repeated type is rejected.
    pub duplicate_type_is_rejected: bool,
    /// Entries must appear in ascending type order.
    pub ascending_type_order_required: bool,
    /// A value outside the declared accepted set is rejected.
    pub value_outside_accepted_set_is_rejected: bool,
}

/// One registry entry.
#[derive(Serialize)]
pub struct ExtensionEntry {
    /// The type byte.
    pub ext_type: u8,
    /// Exact on-wire value length.
    pub value_len: u32,
    /// Lowest accepted byte value.
    pub accepted_min: u8,
    /// Highest accepted byte value.
    pub accepted_max: u8,
}

/// One entry in the reason vocabulary.
#[derive(Serialize)]
pub struct RejectReasonEntry {
    /// The wire/metric token.
    pub token: String,
    /// Owning layer: `codec`, `crypto` or `key`.
    pub layer: String,
    /// Whether the frame is dropped. Only `wrap_key_id_mismatch` is false.
    pub drops_frame: bool,
    /// Whether a byte string can determine this outcome, hence whether a vector
    /// row can exist for it.
    ///
    /// **Orthogonal to `layer`.** `unwrap_failed` is byte-determined while the
    /// remedy is key distribution; `no_transmit_key` is codec-layer while
    /// firing on key-store membership. Neither field may be derived from the
    /// other in either direction.
    pub has_vector: bool,
    /// The fleet label this token shares, if any; guard check g16.
    pub fleet_spelling: Option<String>,
    /// A path whose text must contain this token verbatim, if any; g16.
    pub spec_anchor: Option<String>,
}

/// One vector row.
#[derive(Serialize, Clone)]
pub struct Row {
    /// Structural, stable name.
    pub name: String,
    /// What this row proves that no other row proves.
    pub description: String,
    /// Row kind. **Never a metric label value.**
    pub kind: String,
    /// The full on-wire frame.
    pub frame_hex: String,
    /// Named offsets, so a span disagreement names an offset instead of
    /// surfacing as an opaque tag mismatch hundreds of bytes later.
    pub offsets: Offsets,
    /// Decoded field breakdown.
    pub decoded: Decoded,
    /// The pinned derived spans and crypto inputs.
    pub derived: Derived,
    /// Self-contained reproduction material.
    pub crypto: Crypto,
    /// Present on reject rows only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    /// Row-kind-specific expectations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
}

/// Byte offsets within `frame_hex`.
#[derive(Serialize, Clone)]
pub struct Offsets {
    /// Length of the publisher region, which starts at 0.
    pub publisher_region_len: u32,
    /// Offset of the relay region.
    pub relay_region_offset: u32,
    /// Offset of the payload.
    pub payload_offset: u32,
    /// Offset of the trailing signature.
    pub signature_offset: u32,
}

/// Decoded header fields.
#[derive(Serialize, Clone)]
pub struct Decoded {
    /// Always `header_version`.
    pub version: u8,
    /// The three defined flags.
    pub flags: Flags,
    /// Declared payload length.
    pub payload_length: u32,
    /// The AEAD nonce input.
    pub stream_sequence: u32,
    /// Present iff key-bearing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kek_generation: Option<u16>,
    /// Relay: subscriber slot.
    pub stream_id: u16,
    /// Relay: transmit count.
    pub hop_sequence: u32,
    /// Encoded TLV entries.
    pub extensions: Vec<DecodedExtension>,
    /// **A 16-hex-char string, never a JS number.**
    pub key_id: String,
    /// The decomposed key id.
    pub key_id_decomposed: KeyIdDecomposed,
}

/// One decoded extension.
#[derive(Serialize, Clone)]
pub struct DecodedExtension {
    /// Type byte.
    pub ext_type: u8,
    /// Value bytes.
    pub value_hex: String,
}

/// Three flag bits.
#[derive(Serialize, Clone)]
pub struct Flags {
    /// Decodable without predecessors.
    pub independently_decodable: bool,
    /// Droppable; publisher-declared and untrusted.
    pub discardable: bool,
    /// A wrapped transmit key follows the stream sequence.
    pub key_bearing: bool,
}

/// The key id, decomposed.
///
/// **Every nonce and wrap AAD in this file is computed from the KID bytes taken
/// from the encoded frame, never from a value re-packed out of these fields** —
/// so a receive-side re-pack or decomposition bug surfaces as a crypto mismatch
/// rather than cancelling out.
#[derive(Serialize, Clone)]
pub struct KeyIdDecomposed {
    /// Meeting-scoped sender, `1..=65535`.
    pub sender_id: u16,
    /// Sender-scoped stream number.
    pub stream: u8,
    /// Generation, as hex because it exceeds 32 bits.
    pub generation: String,
}

/// Pinned derived values.
#[derive(Serialize, Clone)]
pub struct Derived {
    /// Publisher region ‖ payload (§3).
    pub signed_input_hex: String,
    /// The publisher region **alone** (§4) — a strict subset of the above.
    pub aead_aad_hex: String,
    /// What a `frame[0..payload_end]` bug would have signed. Asserted to
    /// **differ** from `signed_input_hex`.
    pub naive_contiguous_signed_input_hex: String,
    /// `sframe_salt XOR BE12(stream_sequence)`.
    pub sframe_nonce_hex: String,
    /// Derived AEAD key for this key id.
    pub sframe_key_hex: String,
    /// Derived nonce salt for this key id.
    pub sframe_salt_hex: String,
    /// The `SFrame` payload's detached tag.
    pub payload_tag_hex: String,
    /// Ciphertext, without key id or tag.
    pub payload_ciphertext_hex: String,
    /// The trailing Ed25519 signature.
    pub signature_hex: String,
    /// KEK-wrap nonce: `0x00000000 || key_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_nonce_hex: Option<String>,
    /// KEK-wrap associated data: the key-id bytes alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrap_aad_hex: Option<String>,
}

/// Self-contained reproduction material. Every member must appear in exactly
/// one of `key_material_fields` or `derived_public_fields`; g13.
#[derive(Serialize, Clone)]
pub struct Crypto {
    /// Ed25519 public key — derived from the seed, hence necessarily
    /// random-looking. Classified derived-public.
    pub identity_public_hex: String,
    /// Ed25519 seed. **Test-only private material.**
    pub identity_private_seed_hex: String,
    /// The meeting KEK.
    pub kek_hex: String,
    /// The `SFrame` **base key**, not the derived AEAD key.
    pub transmit_key_hex: String,
    /// Legible ASCII fixture plaintext.
    pub plaintext_hex: String,
}
