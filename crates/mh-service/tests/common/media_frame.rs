//! Frame-v2 fixtures for the media forward-path suites.
//!
//! # One home, and why it is here rather than in `mh-test-utils`
//!
//! Three integration binaries build frame-v2 datagrams
//! (`media_forward_integration`, `media_backpressure_integration`,
//! `media_metrics_integration`), and a copy in each would be three places to
//! find when the header moves. This directory is already the shared home for
//! rigs those same binaries share, and it needs no new crate edge.
//!
//! **Built through `media_protocol::codec::encode_frame`, never by hand.** A
//! hand-rolled builder would be a second spelling of the wire format, and a
//! test asserting against a frame it built from its own understanding of the
//! layout can agree with itself while disagreeing with production.

use bytes::Bytes;
use media_protocol::codec::{encode_frame, MediaFrameParts};
use media_protocol::extensions::{Extension, EXT_TYPE_SALIENCE};
use media_protocol::frame::{
    FrameFlags, WrappedTransmitKey, AEAD_TAG_BYTES, SIGNATURE_SIZE,
    WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES,
};

/// A plausible Opus payload size; the value is irrelevant to every assertion
/// here, only its stability matters.
pub const FIXTURE_PAYLOAD_BYTES: usize = 160;

/// A stand-in signature. MH is keyless and never verifies, so these bytes only
/// have to survive the round trip byte-identically — which is the property the
/// frames-in-equals-frames-out gate asserts.
fn fixture_signature() -> [u8; SIGNATURE_SIZE] {
    let mut signature = [0_u8; SIGNATURE_SIZE];
    for (index, byte) in signature.iter_mut().enumerate() {
        *byte = u8::try_from(index % 251).unwrap_or(0);
    }
    signature
}

/// Which of the three header shapes a fixture frame carries.
///
/// # These are the two movers of the relay-region offset, and that is the point
///
/// `relay_region_offset()` is `publisher_region_end()`, i.e. `ext_start +
/// ext_len`, so it moves with exactly two things: the **key-bearing flag**
/// (which inserts a fixed-size wrapped transmit key) and the **extension TLV
/// length**. ADR-0036 makes both live on audio — §4's in-band key carriage sets
/// the key-bearing flag on audio frames, and §7's salience TLV *is* the
/// extension region.
///
/// A fixture that only ever built [`Self::Minimal`] would give every media test
/// the **minimum-offset** shape, which is precisely the shape a hardcoded
/// `const RELAY_OFFSET` gets right. The byte-identity gate would then pass
/// against the cached-offset optimisation that `media/forward.rs::rewrite`'s
/// doc comment exists to defeat — and because a relay-side offset defect has
/// **no MH-side signal by construction** (MH counts nothing; every receiver
/// counts `signature_invalid`; MH is barred from emitting crypto-layer tokens),
/// the test that pins the structure is the only detector there is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameShape {
    /// No wrapped key, no extensions — the smallest relay-region offset.
    Minimal,
    /// Key-bearing: a wrapped transmit key sits between the stream sequence and
    /// the extension length field (ADR-0036 §4).
    KeyBearing,
    /// Carries a salience TLV — §7's audio selector input (ADR-0036 §7).
    WithExtensions,
}

impl FrameShape {
    /// Every shape, so a parameterised test cannot silently cover fewer.
    pub const ALL: [Self; 3] = [Self::Minimal, Self::KeyBearing, Self::WithExtensions];
}

/// One well-formed audio datagram in the minimal shape.
///
/// `stream_id` and `hop_sequence` are set to values a *publisher* would write
/// on its uplink, deliberately different from what MH must rewrite them to, so
/// a forward path that forgot to rewrite fails the assertion instead of
/// accidentally passing.
#[must_use]
pub fn audio_datagram(stream_sequence: u32, marker: u8) -> Bytes {
    audio_datagram_shaped(stream_sequence, marker, FrameShape::Minimal)
}

/// One well-formed audio datagram in `shape`.
///
/// The key material and the salience value are synthetic and meaningless: MH is
/// keyless, never opens a frame and never reads a TLV, so all these bytes have
/// to do is move the relay-region offset and survive the relay unchanged.
#[must_use]
pub fn audio_datagram_shaped(stream_sequence: u32, marker: u8, shape: FrameShape) -> Bytes {
    let payload = vec![marker; FIXTURE_PAYLOAD_BYTES];
    let signature = fixture_signature();

    // Synthetic, not key material: fixed patterns, never CSPRNG output, and
    // never used to decrypt anything. MH cannot read them and does not try.
    let wrapped_key = [0x11_u8; WRAPPED_TRANSMIT_KEY_MATERIAL_BYTES];
    let wrap_tag = [0x22_u8; AEAD_TAG_BYTES];
    let wrapped = WrappedTransmitKey::new(7, &wrapped_key, &wrap_tag);

    // The one registered extension type, at a value inside its declared
    // accepted range — the encoder enforces the whole grammar, so a value
    // outside it would fail to encode rather than produce a bad fixture.
    let salience = [42_u8];
    let extensions = [Extension {
        ext_type: EXT_TYPE_SALIENCE,
        value: &salience,
    }];

    let parts = MediaFrameParts {
        flags: FrameFlags {
            independently_decodable: true,
            discardable: false,
            key_bearing: matches!(shape, FrameShape::KeyBearing),
        },
        stream_sequence,
        wrapped_transmit_key: match shape {
            FrameShape::KeyBearing => Some(wrapped),
            FrameShape::Minimal | FrameShape::WithExtensions => None,
        },
        extensions: match shape {
            FrameShape::WithExtensions => &extensions,
            FrameShape::Minimal | FrameShape::KeyBearing => &[],
        },
        stream_id: 0xFFFF,
        hop_sequence: 0xDEAD_BEEF,
        payload: &payload,
        signature: &signature,
    };
    encode_frame(&parts).expect("fixture frame must encode")
}

/// The relay-region offset a frame's own header determines.
///
/// Read through the codec's `pub` accessor, never computed here — the accessor's
/// own doc says "This is **not** a constant… Never precompute it", and a fixture
/// that precomputed it would be asserting its own arithmetic.
#[must_use]
pub fn relay_region_offset(frame: &[u8]) -> usize {
    media_protocol::codec::decode_datagram(frame)
        .expect("frame must decode")
        .relay_region_offset()
}

/// The payload marker byte a frame built by [`audio_datagram`] carries.
///
/// Reads the payload out of an encoded frame through the codec, so the fixture
/// never assumes an offset.
#[must_use]
pub fn marker_of(frame: &[u8]) -> u8 {
    let view = media_protocol::codec::decode_datagram(frame).expect("frame must decode");
    view.payload().first().copied().unwrap_or(0)
}
