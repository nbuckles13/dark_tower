//! Structural properties of the v2 codec: zero-copy, canonical encoding,
//! derived constants, the relay rewrite, and redaction.

// Test code: a panic on a bad index IS the failure signal here, and these
// helpers deliberately index at named constant offsets.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod common;

use common::{
    audio_frame, build, canonical_frame, Shape, ALL_SHAPES, SENTINEL_PAYLOAD, SENTINEL_SIGNATURE,
    SENTINEL_WRAPPED_KEY, SENTINEL_WRAP_TAG, TEST_HOP_SEQUENCE, TEST_KEK_GENERATION,
    TEST_STREAM_ID, TEST_STREAM_SEQUENCE,
};
use media_protocol::codec::{
    decode_datagram, decode_stream_frame, encode_frame, peek_frame_len, rewrite_relay_region,
    DecodeError, EncodeError, MediaFrameParts,
};
use media_protocol::extensions::{Extension, EXT_ENTRY_HEADER_BYTES, EXT_TYPE_SALIENCE};
use media_protocol::frame::{
    FrameFlags, WrappedTransmitKey, AEAD_TAG_BYTES, EXT_LENGTH_FIELD_SIZE, KEY_ID_BYTES,
    MAX_EXT_BYTES, MAX_FRAME_BYTES, MAX_HEADER_BYTES, MAX_PAYLOAD_BYTES,
    PUBLISHER_FIXED_PREFIX_SIZE, RELAY_REGION_SIZE, SFRAME_OBJECT_OVERHEAD_BYTES, SIGNATURE_SIZE,
    WRAPPED_TRANSMIT_KEY_SIZE,
};

// ---------------------------------------------------------------------------
// Zero-copy, asserted structurally
// ---------------------------------------------------------------------------

fn within(outer: &[u8], inner: &[u8]) -> bool {
    let base = outer.as_ptr() as usize;
    let end = base + outer.len();
    let start = inner.as_ptr() as usize;
    start >= base && start + inner.len() <= end
}

/// Every returned slice must alias the input buffer.
///
/// Asserted by pointer, not by equality: a copy compares equal and would pass
/// a naive test while defeating the entire point of a zero-copy decode.
#[test]
fn decoded_slices_alias_the_input_buffer() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let view = decode_datagram(&frame).expect("fixture must decode");

        assert!(within(&frame, view.payload()), "payload was copied");
        assert!(within(&frame, view.signature()), "signature was copied");
        assert!(
            within(&frame, view.publisher_region()),
            "publisher region was copied"
        );
        assert!(
            within(&frame, view.extensions().as_bytes()),
            "extensions were copied"
        );
        assert!(within(&frame, view.as_bytes()), "frame slice was copied");
        for span in view.signed_ranges() {
            assert!(within(&frame, span), "a signed span was copied");
        }
        if let Some(wrapped) = view.wrapped_transmit_key() {
            assert!(
                within(&frame, wrapped.expose_wrapped_key()),
                "wrapped key was copied"
            );
            assert!(
                within(&frame, wrapped.expose_wrap_tag()),
                "wrap tag was copied"
            );
        }
    }
}

/// The ranges must address the same bytes the slices do.
#[test]
fn ranges_and_slices_agree() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let view = decode_datagram(&frame).expect("fixture must decode");
        assert_eq!(&frame[view.payload_range()], view.payload());
        assert_eq!(&frame[view.signature_range()], view.signature().as_slice());
        assert_eq!(
            &frame[view.publisher_region_range()],
            view.publisher_region()
        );
        assert_eq!(view.relay_region_offset(), view.publisher_region().len());
        assert_eq!(view.encoded_len(), frame.len());
    }
}

/// The associated data is a strict subset of the signed input, and the relay
/// region is in neither.
#[test]
fn signed_and_aad_spans_exclude_the_relay_region() {
    let frame = canonical_frame();
    let view = decode_datagram(&frame).expect("fixture must decode");

    let [signed_first, signed_second] = view.signed_ranges();
    assert_eq!(
        signed_first,
        view.publisher_region(),
        "AAD must be the first signed span"
    );
    assert_eq!(signed_second, view.payload());

    let relay = view.relay_region_range();
    assert!(relay.start >= view.publisher_region_range().end);
    assert!(relay.end <= view.payload_range().start);
    assert_eq!(relay.len(), RELAY_REGION_SIZE);

    // The two signed spans are non-contiguous precisely because the relay
    // region sits between them.
    assert_eq!(
        view.publisher_region_range().end + RELAY_REGION_SIZE,
        view.payload_range().start
    );
}

// ---------------------------------------------------------------------------
// Canonical encoding and injectivity
// ---------------------------------------------------------------------------

/// `encode(decode(x)) == x`, byte for byte.
///
/// Canonical encoding plus byte-identical roundtrip means decode is
/// **injective** over the accepted language: no two distinct byte strings
/// decode to the same view. That is the no-covert-channel property, proved
/// independently of `byte_coverage.rs` — two proofs of one control from
/// different directions. Do not weaken this to field equality.
#[test]
fn decode_then_encode_is_byte_identical() {
    for shape in ALL_SHAPES {
        for payload in [SENTINEL_PAYLOAD.as_slice(), &[], &[0u8; 300]] {
            let frame = build(shape, payload);
            let view = decode_datagram(&frame).expect("fixture must decode");

            let salience_value = view.extensions().declared_salience().map(|v| [v]);
            let extensions: Vec<Extension<'_>> = salience_value
                .as_ref()
                .map(|value| {
                    vec![Extension {
                        ext_type: EXT_TYPE_SALIENCE,
                        value,
                    }]
                })
                .unwrap_or_default();

            let parts = MediaFrameParts {
                flags: view.flags(),
                stream_sequence: view.stream_sequence(),
                wrapped_transmit_key: view.wrapped_transmit_key(),
                extensions: &extensions,
                stream_id: view.stream_id(),
                hop_sequence: view.hop_sequence(),
                payload: view.payload(),
                signature: view.signature(),
            };
            let reencoded = encode_frame(&parts).expect("re-encode must succeed");
            assert_eq!(
                reencoded.as_ref(),
                frame.as_slice(),
                "roundtrip changed bytes for {shape:?}"
            );

            let review = decode_datagram(&reencoded).expect("re-encoded frame must decode");
            assert_eq!(
                view, review,
                "roundtrip changed the decoded view for {shape:?}"
            );
        }
    }
}

#[test]
fn decoded_fields_match_what_was_encoded() {
    let frame = canonical_frame();
    let view = decode_datagram(&frame).expect("fixture must decode");
    assert_eq!(view.version(), media_protocol::frame::PROTOCOL_VERSION);
    assert_eq!(view.stream_sequence(), TEST_STREAM_SEQUENCE);
    assert_eq!(view.stream_id(), TEST_STREAM_ID);
    assert_eq!(view.hop_sequence(), TEST_HOP_SEQUENCE);
    assert_eq!(view.payload(), SENTINEL_PAYLOAD.as_slice());
    assert_eq!(view.payload_length(), SENTINEL_PAYLOAD.len());
    assert_eq!(view.signature(), &SENTINEL_SIGNATURE);
    let wrapped = view
        .wrapped_transmit_key()
        .expect("canonical frame is key-bearing");
    assert_eq!(wrapped.kek_generation(), TEST_KEK_GENERATION);
    assert_eq!(wrapped.expose_wrapped_key(), &SENTINEL_WRAPPED_KEY);
    assert_eq!(wrapped.expose_wrap_tag(), &SENTINEL_WRAP_TAG);
}

#[test]
fn encoder_rejects_a_flag_field_mismatch() {
    // The flag *is* the presence signal, so the two cannot differ.
    let parts = MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: true,
        },
        stream_sequence: 1,
        wrapped_transmit_key: None,
        extensions: &[],
        stream_id: 0,
        hop_sequence: 0,
        payload: &[],
        signature: &SENTINEL_SIGNATURE,
    };
    assert!(matches!(
        encode_frame(&parts),
        Err(EncodeError::KeyBearingFlagMismatch {
            flag_set: true,
            field_present: false
        })
    ));
}

#[test]
fn encoder_emits_extensions_in_ascending_order_regardless_of_input_order() {
    // Only one registry type exists today, so ordering is asserted through the
    // duplicate rule: two entries of the same type must be refused rather than
    // silently emitted in input order.
    let a = [10u8];
    let b = [20u8];
    let parts = MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: false,
        },
        stream_sequence: 1,
        wrapped_transmit_key: None,
        extensions: &[
            Extension {
                ext_type: EXT_TYPE_SALIENCE,
                value: &a,
            },
            Extension {
                ext_type: EXT_TYPE_SALIENCE,
                value: &b,
            },
        ],
        stream_id: 0,
        hop_sequence: 0,
        payload: &[],
        signature: &SENTINEL_SIGNATURE,
    };
    assert!(matches!(
        encode_frame(&parts),
        Err(EncodeError::Extensions(_))
    ));
}

// ---------------------------------------------------------------------------
// Derived constants, exercised behaviourally as well as pinned
// ---------------------------------------------------------------------------

/// The key-bearing field's size is observable in the encoded length.
#[test]
fn key_bearing_costs_exactly_the_wrapped_key_size() {
    let with_key = build(
        Shape {
            key_bearing: true,
            extensions: false,
        },
        &SENTINEL_PAYLOAD,
    );
    let without = build(
        Shape {
            key_bearing: false,
            extensions: false,
        },
        &SENTINEL_PAYLOAD,
    );
    assert_eq!(with_key.len() - without.len(), WRAPPED_TRANSMIT_KEY_SIZE);
}

/// One extension entry costs exactly its header plus its declared value length.
#[test]
fn one_extension_costs_its_entry_size() {
    let with_ext = build(
        Shape {
            key_bearing: false,
            extensions: true,
        },
        &SENTINEL_PAYLOAD,
    );
    let without = build(
        Shape {
            key_bearing: false,
            extensions: false,
        },
        &SENTINEL_PAYLOAD,
    );
    assert_eq!(with_ext.len() - without.len(), EXT_ENTRY_HEADER_BYTES + 1);
}

/// The registry-derived extension bound is exactly one of every registry type.
#[test]
fn max_ext_bytes_is_the_full_registry() {
    let full_region: usize = media_protocol::extensions::EXT_REGISTRY
        .iter()
        .map(|spec| EXT_ENTRY_HEADER_BYTES + spec.value_len)
        .sum();
    assert_eq!(MAX_EXT_BYTES, full_region);
    // A maximal frame's extension region reaches the bound, so the bound is
    // live rather than a check that can never fire.
    let maximal = build(
        Shape {
            key_bearing: true,
            extensions: true,
        },
        &SENTINEL_PAYLOAD,
    );
    let view = decode_datagram(&maximal).expect("fixture must decode");
    assert_eq!(view.extensions().len(), MAX_EXT_BYTES);
}

/// Intentional value pins. These are the single literal appearance of each
/// number; the definitions are the derived `const` expressions.
#[test]
fn derived_constants_have_their_expected_values() {
    assert_eq!(PUBLISHER_FIXED_PREFIX_SIZE, 10);
    assert_eq!(EXT_LENGTH_FIELD_SIZE, 2);
    assert_eq!(RELAY_REGION_SIZE, 6);
    assert_eq!(SIGNATURE_SIZE, 64);
    assert_eq!(WRAPPED_TRANSMIT_KEY_SIZE, 50);
    assert_eq!(SFRAME_OBJECT_OVERHEAD_BYTES, 24);
    assert_eq!(AEAD_TAG_BYTES, 16);
    assert_eq!(KEY_ID_BYTES, 8);
    assert_eq!(MAX_PAYLOAD_BYTES, 1_048_576);
    assert_eq!(MAX_EXT_BYTES, 3);
    assert_eq!(MAX_HEADER_BYTES, 71);
    assert_eq!(MAX_FRAME_BYTES, 71 + 1_048_576 + 64);
}

// ---------------------------------------------------------------------------
// The reader-side pre-allocation boundary
// ---------------------------------------------------------------------------

#[test]
fn peek_answers_once_the_header_is_present_and_never_before() {
    let frame = canonical_frame();
    let view = decode_datagram(&frame).expect("fixture must decode");
    let header_end = view.payload_range().start;

    for len in 0..header_end {
        assert_eq!(
            peek_frame_len(&frame[..len]).expect("a prefix is never a rejection"),
            None,
            "peek answered at {len}, before the header was complete"
        );
    }
    assert_eq!(
        peek_frame_len(&frame[..header_end]).unwrap(),
        Some(frame.len())
    );
    assert_eq!(peek_frame_len(&frame).unwrap(), Some(frame.len()));
    assert!(
        header_end <= MAX_HEADER_BYTES,
        "a header exceeded MAX_HEADER_BYTES"
    );
}

#[test]
fn peek_enforces_the_maximum_before_a_caller_could_reserve() {
    let frame = canonical_frame();
    let over = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap();
    let mut bytes = frame.clone();
    bytes[media_protocol::frame::PAYLOAD_LENGTH_OFFSET
        ..media_protocol::frame::PAYLOAD_LENGTH_OFFSET + 4]
        .copy_from_slice(&over.to_be_bytes());
    // Only the header is offered — a reader has not yet buffered a payload.
    let header_only = &bytes[..MAX_HEADER_BYTES];
    let err = peek_frame_len(header_only).expect_err("must reject before any reservation");
    assert_eq!(
        err.reason(),
        media_protocol::codec::RejectReason::PayloadLengthExceedsMax
    );
}

#[test]
fn payload_length_at_u32_max_rejects_rather_than_wrapping() {
    // Accumulating in the field's own width would wrap `u32::MAX + 50 + 64 + 3`
    // to a small number that passes a naive bounds check. Offsets accumulate in
    // usize via checked_add for exactly this reason.
    let frame = canonical_frame();
    let mut bytes = frame.clone();
    bytes[media_protocol::frame::PAYLOAD_LENGTH_OFFSET
        ..media_protocol::frame::PAYLOAD_LENGTH_OFFSET + 4]
        .copy_from_slice(&u32::MAX.to_be_bytes());
    let err = decode_datagram(&bytes).expect_err("must reject");
    assert_eq!(
        err.reason(),
        media_protocol::codec::RejectReason::PayloadLengthExceedsMax
    );
    assert!(matches!(
        peek_frame_len(&bytes),
        Err(DecodeError::PayloadLengthExceedsMax { .. })
    ));
}

// ---------------------------------------------------------------------------
// The relay rewrite
// ---------------------------------------------------------------------------

/// The rewrite must touch the relay region and nothing else.
///
/// Run over all four shapes. The zero-extension shape is the one that hid the
/// deleted fixed-offset constant's bug: it is the only shape for which a
/// hardcoded offset happens to be right.
#[test]
fn relay_rewrite_leaves_the_publisher_region_and_signature_byte_identical() {
    for shape in ALL_SHAPES {
        let original = build(shape, &SENTINEL_PAYLOAD);
        let view = decode_datagram(&original).expect("fixture must decode");
        let publisher_before = view.publisher_region().to_vec();
        let signature_before = *view.signature();
        let payload_before = view.payload().to_vec();
        let relay_range = view.relay_region_range();

        let mut rewritten = original.clone();
        rewrite_relay_region(&mut rewritten, 0xBEEF, 0x0102_0304).expect("rewrite must succeed");

        let after = decode_datagram(&rewritten).expect("rewritten frame must still decode");
        assert_eq!(after.publisher_region(), publisher_before.as_slice());
        assert_eq!(after.signature(), &signature_before);
        assert_eq!(after.payload(), payload_before.as_slice());
        assert_eq!(after.stream_id(), 0xBEEF);
        assert_eq!(after.hop_sequence(), 0x0102_0304);
        assert_eq!(after.relay_region_range(), relay_range);

        // Byte-level: only the six relay bytes differ.
        let differing: Vec<usize> = (0..original.len())
            .filter(|&i| original[i] != rewritten[i])
            .collect();
        assert!(
            differing.iter().all(|i| relay_range.contains(i)),
            "{shape:?}: rewrite touched bytes outside the relay region: {differing:?}"
        );
    }
}

/// The write offset and the read offset are the same derived value.
///
/// Not two call sites that happen to agree: `rewrite_relay_region` re-derives
/// through the same parser the decoder uses. A differential here would be
/// invisible to a media handler's own telemetry — it writes six bytes, every
/// counter reports success, and the frame is malformed only from the
/// receiver's point of view.
#[test]
fn rewrite_offset_equals_the_decoder_read_offset() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let read_offset = decode_datagram(&frame).unwrap().relay_region_offset();

        // Rewrite a value distinguishable from every other byte in the frame,
        // then find where it actually landed.
        let mut rewritten = frame.clone();
        rewrite_relay_region(&mut rewritten, 0xFFFF, 0xFFFF_FFFF).unwrap();
        let write_offset = (0..frame.len())
            .find(|&i| frame[i] != rewritten[i])
            .expect("rewrite changed nothing");
        assert_eq!(
            write_offset, read_offset,
            "{shape:?}: write and read offsets diverged"
        );
    }
}

#[test]
fn rewrite_validates_before_writing() {
    // A relay must not write into a frame it has not validated to the same
    // standard as decode.
    let frame = canonical_frame();
    let mut malformed = frame.clone();
    let ext_type_offset = common::ext_region_offset(&frame);
    malformed[ext_type_offset] = 0x7F;
    let before = malformed.clone();
    assert!(rewrite_relay_region(&mut malformed, 1, 2).is_err());
    assert_eq!(
        malformed, before,
        "a rejected rewrite must not have written anything"
    );
}

#[test]
fn rewrite_accepts_a_buffer_holding_further_frames() {
    let first = audio_frame();
    let mut buffer = first.clone();
    buffer.extend_from_slice(&canonical_frame());
    let tail_before = buffer[first.len()..].to_vec();

    rewrite_relay_region(&mut buffer, 7, 9).expect("rewrite must succeed");
    assert_eq!(
        &buffer[first.len()..],
        tail_before.as_slice(),
        "the next frame was touched"
    );

    let view = decode_stream_frame(&buffer).unwrap().unwrap();
    assert_eq!(view.stream_id(), 7);
    assert_eq!(view.hop_sequence(), 9);
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// A hand-rolled `Debug` is a convention, and conventions get "cleaned up"
/// into `#[derive(Debug)]`. This makes that a failing build.
///
/// Precedent: `crates/common/src/secret.rs::test_debug_is_redacted`.
#[test]
fn rendered_output_never_contains_key_signature_or_payload_bytes() {
    let frame = canonical_frame();
    let view = decode_datagram(&frame).expect("fixture must decode");
    let wrapped = view
        .wrapped_transmit_key()
        .expect("canonical frame is key-bearing");

    // `MediaFrameParts` is included deliberately: it is the encode-side
    // container holding the same payload and signature bytes, it is `pub`, and
    // it crosses into crates that DO have a `tracing` dependency. Omitting it
    // is how a derived `Debug` on it passed this test while rendering the very
    // sentinels the test exists to catch.
    let salience = view.extensions().declared_salience().map(|v| [v]);
    let extensions: Vec<Extension<'_>> = salience
        .as_ref()
        .map(|value| {
            vec![Extension {
                ext_type: EXT_TYPE_SALIENCE,
                value,
            }]
        })
        .unwrap_or_default();
    let parts = MediaFrameParts {
        flags: view.flags(),
        stream_sequence: view.stream_sequence(),
        wrapped_transmit_key: view.wrapped_transmit_key(),
        extensions: &extensions,
        stream_id: view.stream_id(),
        hop_sequence: view.hop_sequence(),
        payload: view.payload(),
        signature: view.signature(),
    };

    let rendered = format!("{view:?} {wrapped:?} {:?} {parts:?}", view.extensions());

    for (name, sentinel) in [
        ("wrapped key", SENTINEL_WRAPPED_KEY.as_slice()),
        ("wrap tag", SENTINEL_WRAP_TAG.as_slice()),
        ("signature", SENTINEL_SIGNATURE.as_slice()),
        ("payload", SENTINEL_PAYLOAD.as_slice()),
    ] {
        // Sentinels are runs of one repeated byte, so their decimal rendering
        // in a derived Debug would appear as a repeated numeral sequence.
        let repeated = format!("{}, {}, {}", sentinel[0], sentinel[0], sentinel[0]);
        assert!(
            !rendered.contains(&repeated),
            "{name} bytes appear in rendered output"
        );
        let hex = format!("{:02x}{:02x}", sentinel[0], sentinel[0]);
        assert!(
            !rendered.contains(&hex),
            "{name} bytes appear in rendered output as hex"
        );
    }

    // The KEK generation is *not* redacted: a receiver inspects it for §4
    // rotation, and it is not key material.
    assert!(rendered.contains(&TEST_KEK_GENERATION.to_string()));
}

#[test]
fn wrapped_key_debug_is_redacted_even_when_pulled_out_of_the_view() {
    let key = [0x5Au8; 32];
    let tag = [0x6Bu8; 16];
    let wrapped = WrappedTransmitKey::new(3, &key, &tag);
    let rendered = format!("{wrapped:?}");
    assert!(rendered.contains("redacted"));
    assert!(!rendered.contains("90, 90"), "key bytes rendered");
}
