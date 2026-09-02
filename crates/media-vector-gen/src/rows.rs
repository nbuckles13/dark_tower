//! Building the vector rows.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # Construction order, which is load-bearing
//!
//! [`build_frame`] follows one order and the type system enforces it:
//!
//! 1. derive the `SFrame` key and salt from (base key, key id);
//! 2. compute `payload_length` **before sealing**, via
//!    [`spec::sframe_object_len`] — possible only because GCM is
//!    length-preserving and the tag is detached;
//! 3. **finalize the entire publisher region**, `payload_length` included;
//! 4. slice the AAD from it;
//! 5. seal;
//! 6. assemble the `SFrame` object, sign, encode.
//!
//! Get (3) and (4) the wrong way round and the frame is unopenable by any other
//! implementation, while a single-language round-trip still passes — because
//! the encoder verifies against the AAD buffer it remembers rather than the one
//! on the wire. [`spec::PublisherRegion`] has no public constructor, so that
//! ordering error does not typecheck.

use crate::crypto::{self, Identity, WrappedField};
use crate::error::GenError;
use crate::fixtures;
use crate::json::{
    Crypto, Decoded, DecodedExtension, Derived, Flags, KeyIdDecomposed, Offsets, Row,
};
use crate::kid::KeyIdParts;
use crate::schedule;
use crate::spec::{self, PublisherRegionDraft, SframeObject};
use crate::CIPHER_SUITE_ID;
use media_protocol::codec::MediaFrameParts;
use media_protocol::extensions::Extension;
use media_protocol::frame::{FrameFlags, WrappedTransmitKey, AEAD_TAG_BYTES, KEY_ID_BYTES};

/// Everything that varies between rows.
pub struct FrameSpec {
    /// Row name.
    pub name: &'static str,
    /// What this row proves that no other row proves.
    pub description: &'static str,
    /// Row kind.
    pub kind: &'static str,
    /// Key-id components.
    pub key: KeyIdParts,
    /// Whether the frame carries a wrapped transmit key.
    pub key_bearing: bool,
    /// Independently-decodable flag.
    pub independently_decodable: bool,
    /// Discardable flag.
    pub discardable: bool,
    /// Salience extension value, if the extension region is present.
    pub salience: Option<u8>,
    /// AEAD nonce input.
    pub stream_sequence: u32,
    /// Relay: subscriber slot.
    pub stream_id: u16,
    /// Relay: transmit count.
    pub hop_sequence: u32,
    /// The `SFrame` base key used for the payload.
    pub transmit_key: [u8; fixtures::KEY_LEN],
    /// The KEK the wrapped key is sealed under.
    pub kek: [u8; fixtures::KEY_LEN],
    /// The identity seed that signs the frame.
    pub identity_seed: [u8; fixtures::KEY_LEN],
    /// Override the key id the wrap is bound to. `None` binds it to the frame's
    /// own key id, which is the correct behaviour; `Some` produces the
    /// `wrap_binding` row.
    pub wrap_for_key_id: Option<KeyIdParts>,
    /// Reject reason, for reject rows.
    pub reject_reason: Option<&'static str>,
    /// Row-kind-specific expectations.
    pub expected: Option<serde_json::Value>,
}

impl FrameSpec {
    /// A well-formed audio-shaped default: key-bearing, no extensions.
    #[must_use]
    pub fn base(name: &'static str, description: &'static str, kind: &'static str) -> Self {
        Self {
            name,
            description,
            kind,
            key: KeyIdParts {
                sender_id: 0x0102,
                stream: 0x03,
                generation: 0x04_0506_0708,
            },
            key_bearing: true,
            independently_decodable: true,
            discardable: false,
            salience: None,
            stream_sequence: 7,
            stream_id: 2,
            hop_sequence: 11,
            transmit_key: fixtures::ALICE_TRANSMIT_KEY,
            kek: fixtures::MEETING_KEK,
            identity_seed: fixtures::ALICE_IDENTITY_SEED,
            wrap_for_key_id: None,
            reject_reason: None,
            expected: None,
        }
    }
}

/// The assembled artifacts of one frame, before JSON shaping.
pub struct BuiltFrame {
    /// Full on-wire bytes.
    pub frame: Vec<u8>,
    /// The row, ready to serialize.
    pub row: Row,
}

/// Build one frame and its row.
///
/// # Errors
///
/// Propagates [`GenError`] from the key-id packer, the crypto layer, the
/// encoder, or either span cross-check.
#[expect(
    clippy::too_many_lines,
    reason = "One linear construction sequence whose ORDER is the invariant under test. \
              Splitting it would scatter the ordering across functions and make the \
              load-bearing sequence harder to verify, not easier."
)]
pub fn build_frame(spec_in: &FrameSpec) -> Result<BuiltFrame, GenError> {
    // (1) Key id, then schedule. The key id is packed with checked conversions
    // and is the input to the derivation, so a decomposition error cannot
    // silently produce a usable key.
    let key_id = spec_in.key.pack()?;
    let derived = schedule::derive(&spec_in.transmit_key, &key_id, CIPHER_SUITE_ID)?;
    let nonce = schedule::nonce_from(&derived.salt, u64::from(spec_in.stream_sequence));

    // (2) Payload length, known BEFORE sealing. This is computation 3 making
    // computation 1 possible.
    let payload_len = spec::sframe_object_len(fixtures::PLAINTEXT.len());

    // Wrapped transmit key, bound to a key id. `wrap_for_key_id` deliberately
    // allows binding to a DIFFERENT key id, which is the wrap_binding row: the
    // receiver must ignore such a wrap rather than cache it (§4).
    let wrapped: Option<WrappedField> = if spec_in.key_bearing {
        let bind_to = match spec_in.wrap_for_key_id {
            Some(other) => other.pack()?,
            None => key_id,
        };
        Some(crypto::wrap_transmit_key(
            &spec_in.kek,
            &bind_to,
            &spec_in.transmit_key,
        )?)
    } else {
        None
    };

    let wrapped_field_bytes: Option<Vec<u8>> = wrapped.as_ref().map(|w| {
        let mut v = Vec::new();
        v.extend_from_slice(&fixtures::KEK_GENERATION.to_be_bytes());
        v.extend_from_slice(&w.wrapped_key);
        v.extend_from_slice(&w.wrap_tag);
        v
    });

    let flags = FrameFlags {
        independently_decodable: spec_in.independently_decodable,
        discardable: spec_in.discardable,
        key_bearing: spec_in.key_bearing,
    };

    let ext_entries: Vec<Extension<'_>> = Vec::new();
    let salience_value = spec_in.salience.map(|v| [v]);
    let ext_entries = match &salience_value {
        Some(v) => vec![Extension {
            ext_type: media_protocol::extensions::EXT_TYPE_SALIENCE,
            value: v.as_slice(),
        }],
        None => ext_entries,
    };
    // The encoded TLV bytes, needed by the ADR-derived span arithmetic.
    let ext_bytes: Vec<u8> = match spec_in.salience {
        Some(v) => vec![media_protocol::extensions::EXT_TYPE_SALIENCE, 1, v],
        None => Vec::new(),
    };

    // (3) Finalize the WHOLE publisher region, payload_length included.
    let draft = PublisherRegionDraft::new(
        flags,
        spec_in.stream_sequence,
        wrapped_field_bytes.clone(),
        ext_bytes.clone(),
    );
    let spec_span_len = draft.spec_span_len();
    let region = draft.finalize(payload_len);

    // (4) Only now can the AAD be sliced. `aad()` exists only on the finalized
    // type, so this cannot be reordered.
    let aad = region.aad().to_vec();
    if aad.len() != spec_span_len {
        return Err(GenError::SpanDisagreement {
            span: "publisher_region (arithmetic vs materialized)",
            spec_derived: spec_span_len,
            codec_derived: aad.len(),
        });
    }

    // (5) Seal, with the publisher region as associated data.
    let sealed = schedule::seal_detached(&derived.key, &nonce, &aad, fixtures::PLAINTEXT)?;

    // (6) Assemble the SFrame object: key_id || tag || ciphertext.
    let object = SframeObject::new(&key_id, &sealed.tag, &sealed.ciphertext);
    let payload = object.as_bytes().to_vec();
    if payload.len() != payload_len {
        return Err(GenError::SpanDisagreement {
            span: "payload length predicted before sealing",
            spec_derived: payload_len,
            codec_derived: payload.len(),
        });
    }

    let identity = Identity::from_seed(&spec_in.identity_seed)?;
    let signature = identity.sign(&spec::signed_input(&region, &payload));

    let wrapped_view = wrapped
        .as_ref()
        .map(|w| WrappedTransmitKey::new(fixtures::KEK_GENERATION, &w.wrapped_key, &w.wrap_tag));
    let parts = MediaFrameParts {
        flags,
        stream_sequence: spec_in.stream_sequence,
        wrapped_transmit_key: wrapped_view,
        extensions: &ext_entries,
        stream_id: spec_in.stream_id,
        hop_sequence: spec_in.hop_sequence,
        payload: &payload,
        signature: &signature,
    };
    let frame = spec::encode_and_cross_check(&parts, &region, &payload)?;

    // The KID for every pinned nonce and wrap AAD comes from HERE — the encoded
    // frame — not from `key_id` above. A receive-side re-pack bug then surfaces
    // as a crypto mismatch instead of cancelling out.
    let view = media_protocol::codec::decode_datagram(&frame).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let payload_range = view.payload_range();
    let mut kid_from_wire = [0u8; KEY_ID_BYTES];
    kid_from_wire.copy_from_slice(
        frame
            .get(payload_range.start..payload_range.start + KEY_ID_BYTES)
            .ok_or(GenError::Encode {
                detail: "payload shorter than a key id".to_string(),
            })?,
    );
    if kid_from_wire != key_id {
        return Err(GenError::SpanDisagreement {
            span: "key id on the wire differs from the packed key id",
            spec_derived: usize::from(key_id[0]),
            codec_derived: usize::from(kid_from_wire[0]),
        });
    }

    let naive = spec::naive_contiguous_signed_input(&frame, payload_range.end)?;

    let row = Row {
        name: spec_in.name.to_string(),
        description: spec_in.description.to_string(),
        kind: spec_in.kind.to_string(),
        frame_hex: hex::encode(&frame),
        offsets: Offsets {
            publisher_region_len: as_u32(view.publisher_region().len()),
            relay_region_offset: as_u32(view.relay_region_offset()),
            payload_offset: as_u32(payload_range.start),
            signature_offset: as_u32(view.signature_range().start),
        },
        decoded: Decoded {
            version: view.version(),
            flags: Flags {
                independently_decodable: flags.independently_decodable,
                discardable: flags.discardable,
                key_bearing: flags.key_bearing,
            },
            payload_length: as_u32(view.payload_length()),
            stream_sequence: view.stream_sequence(),
            kek_generation: wrapped.as_ref().map(|_| fixtures::KEK_GENERATION),
            stream_id: view.stream_id(),
            hop_sequence: view.hop_sequence(),
            extensions: view
                .extensions()
                .iter()
                .map(|e| DecodedExtension {
                    ext_type: e.ext_type,
                    value_hex: hex::encode(e.value),
                })
                .collect(),
            key_id: hex::encode(kid_from_wire),
            key_id_decomposed: KeyIdDecomposed {
                sender_id: u16::try_from(spec_in.key.sender_id).unwrap_or(u16::MAX),
                stream: u8::try_from(spec_in.key.stream).unwrap_or(u8::MAX),
                generation: hex::encode(spec_in.key.generation.to_be_bytes()),
            },
        },
        derived: Derived {
            signed_input_hex: hex::encode(spec::signed_input(&region, &payload)),
            aead_aad_hex: hex::encode(&aad),
            naive_contiguous_signed_input_hex: hex::encode(&naive),
            sframe_nonce_hex: hex::encode(schedule::nonce_from(
                &derived.salt,
                u64::from(view.stream_sequence()),
            )),
            sframe_key_hex: hex::encode(derived.key),
            sframe_salt_hex: hex::encode(derived.salt),
            payload_tag_hex: hex::encode(sealed.tag),
            payload_ciphertext_hex: hex::encode(&sealed.ciphertext),
            signature_hex: hex::encode(signature),
            wrap_nonce_hex: wrapped.as_ref().map(|w| hex::encode(w.nonce)),
            wrap_aad_hex: wrapped.as_ref().map(|w| hex::encode(w.aad)),
        },
        crypto: Crypto {
            identity_public_hex: hex::encode(identity.public_key()),
            identity_private_seed_hex: hex::encode(spec_in.identity_seed),
            kek_hex: hex::encode(spec_in.kek),
            transmit_key_hex: hex::encode(spec_in.transmit_key),
            plaintext_hex: hex::encode(fixtures::PLAINTEXT),
        },
        reject_reason: spec_in.reject_reason.map(ToString::to_string),
        expected: spec_in.expected.clone(),
    };

    debug_assert_eq!(sealed.tag.len(), AEAD_TAG_BYTES);
    Ok(BuiltFrame { frame, row })
}

/// Widths here are bounded by `MAX_FRAME_BYTES`, far below `u32::MAX`;
/// saturating rather than masking keeps the crate's truncation deny intact.
fn as_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
