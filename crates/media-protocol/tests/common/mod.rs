//! Shared frame construction for the `media-protocol` integration tests.
//!
//! Both `byte_coverage.rs` and `corpus_seeds.rs` need the same two
//! capabilities — build a canonical valid v2 frame, and mutate one at a named
//! constant offset — so they live here rather than as two private builders
//! that would have to agree about the v2 layout and would diverge on the first
//! header change.
//!
//! **Every frame here is produced by the encoder**, and every invalid frame is
//! produced by mutating an encoder-produced valid one. No test writes a
//! literal byte array: the negative cases are unencodable by construction, and
//! hand-writing them would make the test files a second home for the layout.

#![allow(dead_code)]
// Test code: a panic on a bad index IS the failure signal here, and these
// helpers deliberately index at named constant offsets.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

use media_protocol::codec::{encode_frame, MediaFrameParts};
use media_protocol::extensions::{Extension, EXT_TYPE_SALIENCE};
use media_protocol::frame::{
    FrameFlags, WrappedTransmitKey, AEAD_TAG_BYTES, SIGNATURE_SIZE,
    WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES,
};

/// Recognisable byte patterns, so a redaction test can assert that none of
/// them appear in any rendered output.
pub static SENTINEL_WRAPPED_KEY: [u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES] =
    [0xA1; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES];
/// Sentinel wrap tag.
pub static SENTINEL_WRAP_TAG: [u8; AEAD_TAG_BYTES] = [0xB2; AEAD_TAG_BYTES];
/// Sentinel signature.
pub static SENTINEL_SIGNATURE: [u8; SIGNATURE_SIZE] = [0xC3; SIGNATURE_SIZE];
/// Sentinel payload.
pub static SENTINEL_PAYLOAD: [u8; 24] = [0xD4; 24];

/// The KEK generation carried by every built frame.
pub const TEST_KEK_GENERATION: u16 = 0x0107;
/// The stream sequence carried by every built frame.
pub const TEST_STREAM_SEQUENCE: u32 = 0x1234_5678;
/// The relay slot carried by every built frame.
pub const TEST_STREAM_ID: u16 = 0x2A2B;
/// The hop sequence carried by every built frame.
pub const TEST_HOP_SEQUENCE: u32 = 0x0000_04D2;
/// A salience value inside the registry's declared `0..=100` range.
pub const TEST_SALIENCE: u8 = 42;

/// Which of the four header shapes to build.
///
/// Key-bearing and extensions-present are independent, and the resulting four
/// combinations are exactly the four associated-data spans the cross-language
/// vectors must cover. Three of them never appear in loopback traffic, which
/// is why they are enumerated here rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// Whether the wrapped-transmit-key field is present.
    pub key_bearing: bool,
    /// Whether a salience extension is carried.
    pub extensions: bool,
}

/// All four header shapes.
pub const ALL_SHAPES: [Shape; 4] = [
    Shape {
        key_bearing: false,
        extensions: false,
    },
    Shape {
        key_bearing: true,
        extensions: false,
    },
    Shape {
        key_bearing: false,
        extensions: true,
    },
    Shape {
        key_bearing: true,
        extensions: true,
    },
];

/// Build a valid frame of the given shape with the given payload.
#[must_use]
pub fn build(shape: Shape, payload: &[u8]) -> Vec<u8> {
    build_with_salience(shape, payload, TEST_SALIENCE)
}

/// Build a valid frame carrying a specific salience value.
///
/// Values outside the registry's declared accepted set make this return the
/// encoder's rejection instead; use [`try_build_with_salience`] for that case.
#[must_use]
pub fn build_with_salience(shape: Shape, payload: &[u8], salience: u8) -> Vec<u8> {
    try_build_with_salience(shape, payload, salience).expect("valid frame must encode")
}

/// Build a frame, surfacing encoder rejections.
///
/// # Errors
///
/// Propagates the encoder's error so tests can assert the encoder enforces the
/// same grammar the decoder does.
pub fn try_build_with_salience(
    shape: Shape,
    payload: &[u8],
    salience: u8,
) -> Result<Vec<u8>, media_protocol::codec::EncodeError> {
    let salience_value = [salience];
    let extensions: Vec<Extension<'_>> = if shape.extensions {
        vec![Extension {
            ext_type: EXT_TYPE_SALIENCE,
            value: &salience_value,
        }]
    } else {
        Vec::new()
    };
    let wrapped = if shape.key_bearing {
        Some(WrappedTransmitKey::new(
            TEST_KEK_GENERATION,
            &SENTINEL_WRAPPED_KEY,
            &SENTINEL_WRAP_TAG,
        ))
    } else {
        None
    };
    let parts = MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: shape.key_bearing,
        },
        stream_sequence: TEST_STREAM_SEQUENCE,
        wrapped_transmit_key: wrapped,
        extensions: &extensions,
        stream_id: TEST_STREAM_ID,
        hop_sequence: TEST_HOP_SEQUENCE,
        payload,
        signature: &SENTINEL_SIGNATURE,
    };
    encode_frame(&parts).map(|bytes| bytes.to_vec())
}

/// The canonical frame used by the coverage and corpus tests: key-bearing with
/// one extension, so both optional regions are exercised.
#[must_use]
pub fn canonical_frame() -> Vec<u8> {
    build(
        Shape {
            key_bearing: true,
            extensions: true,
        },
        &SENTINEL_PAYLOAD,
    )
}

/// The fixed-size audio shape: key-bearing, no extensions.
#[must_use]
pub fn audio_frame() -> Vec<u8> {
    build(
        Shape {
            key_bearing: true,
            extensions: false,
        },
        &SENTINEL_PAYLOAD,
    )
}

/// Overwrite `len` bytes at `offset` with `value`, returning the mutated copy.
#[must_use]
pub fn overwrite(frame: &[u8], offset: usize, bytes: &[u8]) -> Vec<u8> {
    let mut out = frame.to_vec();
    let end = offset + bytes.len();
    assert!(end <= out.len(), "mutation would extend the frame");
    out[offset..end].copy_from_slice(bytes);
    out
}

/// Set `frame[offset]` to `value`, returning the mutated copy.
#[must_use]
pub fn set_byte(frame: &[u8], offset: usize, value: u8) -> Vec<u8> {
    overwrite(frame, offset, &[value])
}

/// Flip bit `bit` of `frame[offset]`, returning the mutated copy.
#[must_use]
pub fn flip_bit(frame: &[u8], offset: usize, bit: u32) -> Vec<u8> {
    let mut out = frame.to_vec();
    out[offset] ^= 1u8 << bit;
    out
}

/// Truncate to `len` bytes.
#[must_use]
pub fn truncated_to(frame: &[u8], len: usize) -> Vec<u8> {
    frame[..len].to_vec()
}

/// Append `extra` bytes after a complete frame.
#[must_use]
pub fn with_trailing(frame: &[u8], extra: usize) -> Vec<u8> {
    let mut out = frame.to_vec();
    out.extend(std::iter::repeat_n(0xEEu8, extra));
    out
}

/// Offset of the extension-length field, read from the decoded frame itself.
///
/// **Deliberately not computed from `Shape`.** An earlier version applied the
/// rule "publisher prefix, then the wrapped-key field iff key-bearing" here — a
/// *parallel derivation of a conditional layout rule*, the same mechanism as
/// the two offset defects already fixed in this changeset. Built from named
/// constants it would still diverge silently the first time a conditional field
/// is added ahead of the extension region; and because these helpers mutate
/// bytes to *provoke rejections*, most tests would still see a rejection and
/// still pass — for a different reason than the one they name. A test that goes
/// quietly wrong is worse than one that breaks. The decoder is the single
/// authority for every conditional offset; ask it.
#[must_use]
pub fn ext_length_offset(frame: &[u8]) -> usize {
    media_protocol::codec::decode_datagram(frame)
        .expect("offset helpers take a valid frame")
        .ext_length_field_range()
        .start
}

/// Offset of the first extension entry's type byte, read from the decoded
/// frame. See [`ext_length_offset`] for why this is not computed.
#[must_use]
pub fn ext_region_offset(frame: &[u8]) -> usize {
    media_protocol::codec::decode_datagram(frame)
        .expect("offset helpers take a valid frame")
        .extensions_range()
        .start
}

/// The four canonical frames: the cross product of the layout's two
/// conditional regions, key-bearing x extensions-present.
///
/// This is the **one** fixture matrix. Three consumers draw from it — the
/// producibility cross-check, the every-byte-prefix invariant test, and the
/// byte-coverage proof — so "is the fixture set complete?" is one place to
/// check rather than three sentences that happen to agree. If a third
/// conditional region is ever added, this matrix becomes 2x2x2 in one place
/// instead of silently leaving two tests at 2x2.
///
/// # Provenance rule
///
/// These are **generated through the v2 encoder**, never hand-written byte
/// arrays. A hand-written array is a second implementation of the layout, and
/// a wrong one would make the prefix-invariant test assert `Ok(None)` over
/// bytes that are not a valid frame at all — passing while proving nothing.
///
/// **Deliberate exception, stated so nobody "improves" it**: the
/// cross-language vectors must be **frozen bytes**, because TypeScript cannot
/// call the Rust encoder — that is the entire point of a pinned vector.
/// Regenerating them at test time would make them self-confirming and unable
/// to catch the divergence they exist to catch. The split is: Rust-side
/// fixtures generated here, and those generated bytes frozen once into the
/// vectors file with a drift guard asserting the two still agree.
#[must_use]
pub fn canonical_frame_matrix() -> Vec<(Shape, Vec<u8>)> {
    ALL_SHAPES
        .iter()
        .map(|&shape| (shape, build(shape, &SENTINEL_PAYLOAD)))
        .collect()
}
