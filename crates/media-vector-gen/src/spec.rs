//! **The three computations this crate exists for.**
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # These are derived from ADR-0036, never from the TypeScript codec
//!
//! 1. [`PublisherRegion::aad`] — the AEAD associated-data span (§4).
//! 2. [`signed_input`] — the signed range (§3).
//! 3. [`SframeObject`] — the detached-tag split (§2).
//!
//! Nothing else in the tree checks them. `media-protocol`'s own rustdoc says so
//! at `MediaFrameView::publisher_region`: every fixture in that crate is built
//! with its own encoder, so an encoder and decoder that compute the region
//! **identically but wrongly** produce signatures that verify successfully over
//! the wrong bytes, with the round-trip byte-identical and every byte still
//! load-bearing — under the wrong span. That failure is silent, and this module
//! is the outside derivation that closes it.
//!
//! So the arithmetic below is written from §2's field list. It imports field
//! *sizes* from `media_protocol::frame` — sizes are not among the protected
//! computations, and duplicating them would be duplication with no compensating
//! value — but it computes the *spans* itself and then
//! [`PublisherRegion::cross_check`] asserts agreement with `media-protocol`'s
//! parser walk. Three independent derivations pinned to one value: this
//! arithmetic, that parser walk, and the TypeScript codec.
//!
//! **@dry-reviewer has ruled this duplication load-bearing. Do not collapse it**
//! by calling `publisher_region()` here: that would reduce three derivations to
//! two and delete the only outside check on the span.
//!
//! # Construction order is enforced by the type system
//!
//! The publisher region must be filled **completely, including
//! `payload_length`, before the AAD is sliced**. Get that wrong and the frame is
//! unopenable by any other implementation — while a single-language round-trip
//! still passes, because the encoder verifies against the AAD buffer it
//! remembers rather than the one on the wire.
//!
//! A comment cannot catch that. So [`PublisherRegionDraft::finalize`] is the
//! only way to obtain a [`PublisherRegion`], it requires the payload length,
//! and [`PublisherRegion::aad`] exists only on the finalized type. **Slicing the
//! AAD early does not typecheck.** The length itself comes from
//! [`sframe_object_len`] so it cannot be guessed — and it is knowable before
//! sealing only because GCM is length-preserving and the tag is detached, which
//! is computation 3 making computation 1 possible.

use crate::error::GenError;
use media_protocol::codec::MediaFrameParts;
use media_protocol::frame::{
    FrameFlags, MediaFrameView, AEAD_TAG_BYTES, EXT_LENGTH_FIELD_SIZE, KEY_ID_BYTES,
    PUBLISHER_FIXED_PREFIX_SIZE, SIGNATURE_SIZE, WRAPPED_TRANSMIT_KEY_SIZE,
};

/// **Computation 3: the detached-tag split.**
///
/// ADR-0036 §2: the payload is an *"`SFrame` object: key id and authentication
/// tag in its own clear header (the receiver must read the key id to select a
/// key before decrypting)"*. So both the key id and the tag sit **ahead** of the
/// ciphertext, and the tag is detached from the AEAD output rather than
/// trailing it as RFC 9605's own framing would place it.
///
/// ```text
/// key_id (8) || tag (16) || ciphertext (n)
/// ```
///
/// Corroborated by `media_protocol::frame::SFRAME_OBJECT_OVERHEAD_BYTES`, which
/// is `KEY_ID_BYTES + AEAD_TAG_BYTES` — 24 bytes of clear header, with no room
/// for a trailing tag.
pub struct SframeObject {
    bytes: Vec<u8>,
}

impl SframeObject {
    /// Assemble the object from its three parts, in wire order.
    #[must_use]
    pub fn new(key_id: &[u8; KEY_ID_BYTES], tag: &[u8; AEAD_TAG_BYTES], ciphertext: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(KEY_ID_BYTES + AEAD_TAG_BYTES + ciphertext.len());
        bytes.extend_from_slice(key_id);
        bytes.extend_from_slice(tag);
        bytes.extend_from_slice(ciphertext);
        Self { bytes }
    }

    /// The object's bytes: exactly the frame payload.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The key-id bytes **as they sit in the object**.
    ///
    /// Every nonce and every wrap AAD in the generated file is computed from
    /// this slice — taken from the encoded frame — and never from a value
    /// re-packed out of the decomposed `sender_id` / `stream` / `generation`
    /// fields. That is deliberate: a receive-side decomposition or re-pack bug
    /// then surfaces as a **crypto mismatch** instead of cancelling out against
    /// an identically-buggy re-pack on the other side.
    /// # Errors
    ///
    /// [`GenError::Encode`] if the object is shorter than a key id. **Fallible
    /// rather than panicking**: this crate computes byte spans for a living, so
    /// a panic inside span arithmetic is the failure mode it exists to *detect*
    /// and must not be one it exhibits (ADR-0002).
    pub fn key_id(&self) -> Result<[u8; KEY_ID_BYTES], GenError> {
        let head = self
            .bytes
            .get(..KEY_ID_BYTES)
            .ok_or_else(|| GenError::Encode {
                detail: format!(
                    "SFrame object is {} bytes, shorter than the {KEY_ID_BYTES}-byte key id \
                     that must precede the detached tag",
                    self.bytes.len()
                ),
            })?;
        let mut out = [0u8; KEY_ID_BYTES];
        out.copy_from_slice(head);
        Ok(out)
    }
}

/// Total on-wire payload length for a given plaintext length.
///
/// Knowable **before** sealing: GCM is length-preserving, so the ciphertext is
/// exactly the plaintext length, and the tag is a fixed 16 bytes. This is what
/// lets `payload_length` be written into the publisher region before the AAD is
/// sliced — the load-bearing construction order.
#[must_use]
pub const fn sframe_object_len(plaintext_len: usize) -> usize {
    KEY_ID_BYTES + AEAD_TAG_BYTES + plaintext_len
}

/// A publisher region that is **not yet complete**. It has no `aad()`.
pub struct PublisherRegionDraft {
    flags: FrameFlags,
    stream_sequence: u32,
    wrapped: Option<Vec<u8>>,
    extensions: Vec<u8>,
}

impl PublisherRegionDraft {
    /// Begin a publisher region. `wrapped` is the 50-byte wrapped-transmit-key
    /// field iff the key-bearing flag is set; `extensions` is the encoded TLV
    /// region, empty if absent.
    #[must_use]
    pub fn new(
        flags: FrameFlags,
        stream_sequence: u32,
        wrapped: Option<Vec<u8>>,
        extensions: Vec<u8>,
    ) -> Self {
        Self {
            flags,
            stream_sequence,
            wrapped,
            extensions,
        }
    }

    /// **Computation 1: the AEAD associated-data span.**
    ///
    /// Materialises the complete publisher region, `payload_length` included.
    /// This is the only route to a [`PublisherRegion`], and therefore the only
    /// route to an AAD.
    ///
    /// The span, from §2's field list:
    ///
    /// ```text
    /// version(1) + flags(1) + payload_length(4) + stream_sequence(4)
    ///   + wrapped_transmit_key(50)   iff key-bearing
    ///   + ext_length(2)
    ///   + extensions(ext_length)
    /// ```
    ///
    /// The `ext_length` field is **inside** the span. It must be: the signed
    /// input is discontiguous (the relay region sits between its two halves),
    /// and `ext_length` together with `payload_length` is what authenticates the
    /// split point. Leaving the length prefix outside would let a relay rewrite
    /// where the two halves divide.
    ///
    /// The relay region and the signature are **outside** it. That is the whole
    /// premise of §2's split: a relay rewrites `stream_id` and `hop_sequence`
    /// per subscriber, so those six bytes cannot be authenticated by anyone.
    #[must_use]
    pub fn finalize(self, payload_len: usize) -> PublisherRegion {
        let mut bytes = Vec::new();
        bytes.push(media_protocol::frame::PROTOCOL_VERSION);
        bytes.push(self.flags.to_u8());
        // Truncation is structurally impossible here: callers derive
        // `payload_len` from `sframe_object_len`, and `encode_frame` enforces
        // MAX_PAYLOAD_BYTES independently. Saturating rather than masking keeps
        // the crate's `cast_possible_truncation` deny intact without a silent
        // wrap.
        let declared = u32::try_from(payload_len).unwrap_or(u32::MAX);
        bytes.extend_from_slice(&declared.to_be_bytes());
        bytes.extend_from_slice(&self.stream_sequence.to_be_bytes());
        if let Some(w) = &self.wrapped {
            bytes.extend_from_slice(w);
        }
        let ext_len = u16::try_from(self.extensions.len()).unwrap_or(u16::MAX);
        bytes.extend_from_slice(&ext_len.to_be_bytes());
        bytes.extend_from_slice(&self.extensions);
        PublisherRegion { bytes }
    }

    /// The span length this draft will produce, computed arithmetically from
    /// §2's field list rather than by measuring the emitted bytes.
    ///
    /// Kept separate from [`Self::finalize`] so that
    /// [`PublisherRegion::cross_check`] compares an *arithmetic* derivation
    /// against `media-protocol`'s *parser walk*, rather than comparing a buffer
    /// against its own length — which would be a tautology wearing a check's
    /// clothes.
    #[must_use]
    pub fn spec_span_len(&self) -> usize {
        PUBLISHER_FIXED_PREFIX_SIZE
            + if self.flags.key_bearing {
                WRAPPED_TRANSMIT_KEY_SIZE
            } else {
                0
            }
            + EXT_LENGTH_FIELD_SIZE
            + self.extensions.len()
    }
}

/// A **complete** publisher region. Only this type has an AAD.
pub struct PublisherRegion {
    bytes: Vec<u8>,
}

impl PublisherRegion {
    /// **The AEAD associated data: the publisher region, alone.**
    ///
    /// Deliberately a strict **subset** of the signed range — the payload is
    /// signed but is not associated data, because it is the AEAD's own
    /// ciphertext. The generated vectors pin `aead_aad_hex` and
    /// `signed_input_hex` as separate named fields precisely so the two cannot
    /// be conflated by a reader or by an implementation.
    #[must_use]
    pub fn aad(&self) -> &[u8] {
        &self.bytes
    }

    /// Cross-check this ADR-derived span against `media-protocol`'s
    /// parser-walk-derived span for the same frame.
    ///
    /// # Errors
    ///
    /// [`GenError::SpanDisagreement`] if they differ — meaning one of the two
    /// derivations of a protected computation is wrong. Do not reconcile by
    /// editing whichever is easier to change.
    pub fn cross_check(&self, view: &MediaFrameView<'_>) -> Result<(), GenError> {
        let codec_span = view.publisher_region();
        if codec_span != self.bytes.as_slice() {
            return Err(GenError::SpanDisagreement {
                span: "publisher_region",
                spec_derived: self.bytes.len(),
                codec_derived: codec_span.len(),
            });
        }
        Ok(())
    }
}

/// **Computation 2: the signed range.** Publisher region, then payload.
///
/// ADR-0036 §3: the signature covers *"the publisher region and payload"*. The
/// two spans are **not contiguous** — the relay region sits between them and is
/// excluded, which is the point of §2's split. Concatenating them is unambiguous
/// only because `payload_length` and `ext_length` are themselves inside the
/// signed publisher region, so the split point is itself authenticated.
///
/// This is a strict **superset** of the associated data. Pinning both spans as
/// separately named fields in the vectors is what makes the AAD-versus-signature
/// distinction un-conflatable.
#[must_use]
pub fn signed_input(publisher_region: &PublisherRegion, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(publisher_region.bytes.len() + payload.len());
    out.extend_from_slice(&publisher_region.bytes);
    out.extend_from_slice(payload);
    out
}

/// The bytes a naive `frame[0..payload_end]` implementation would have signed.
///
/// **Never used to sign anything.** It is pinned in the vectors as
/// `naive_contiguous_signed_input_hex` and asserted to **differ** from
/// [`signed_input`], so the near-miss is documented and demonstrably
/// non-degenerate rather than merely avoided. The difference is exactly the six
/// relay bytes: a contiguous slice would authenticate the very field a relay
/// rewrites, so every forwarded frame would fail verification — loudly, but only
/// once a second implementation exists.
///
/// # Errors
///
/// [`GenError::Encode`] if `payload_end` is past the end of `frame`. Checked
/// rather than indexed: a panic here would abort the generator mid-run, and the
/// value being sliced is derived from a decoded header — exactly the
/// attacker-influenced quantity the codec treats as a trust boundary.
pub fn naive_contiguous_signed_input(
    frame: &[u8],
    payload_end: usize,
) -> Result<Vec<u8>, GenError> {
    frame
        .get(..payload_end)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| GenError::Encode {
            detail: format!(
                "payload_end {payload_end} is past the end of a {}-byte frame",
                frame.len()
            ),
        })
}

/// Encode a frame through `media-protocol`, then cross-check both spans.
///
/// Framing is **not** re-implemented here: this delegates to
/// `media_protocol::codec::encode_frame`. Framing is not one of the three
/// protected computations, and a second Rust frame encoder would be duplication
/// with no compensating value.
///
/// # Errors
///
/// [`GenError::Encode`] if the encoder rejects the parts,
/// [`GenError::SpanDisagreement`] if either derivation disagrees.
pub fn encode_and_cross_check(
    parts: &MediaFrameParts<'_>,
    region: &PublisherRegion,
    payload: &[u8],
) -> Result<Vec<u8>, GenError> {
    let bytes = media_protocol::codec::encode_frame(parts).map_err(|e| GenError::Encode {
        detail: format!("{e:?}"),
    })?;
    let view = media_protocol::codec::decode_datagram(&bytes).map_err(|e| GenError::Encode {
        detail: format!("re-decode of our own frame failed: {e:?}"),
    })?;

    region.cross_check(&view)?;

    // Computation 2, cross-checked the same way: `signed_ranges()` is the
    // codec's parser-walk answer, ours is the ADR-derived concatenation.
    let ours = signed_input(region, payload);
    let [codec_pub, codec_payload] = view.signed_ranges();
    let mut theirs = Vec::with_capacity(codec_pub.len() + codec_payload.len());
    theirs.extend_from_slice(codec_pub);
    theirs.extend_from_slice(codec_payload);
    if ours != theirs {
        return Err(GenError::SpanDisagreement {
            span: "signed_input",
            spec_derived: ours.len(),
            codec_derived: theirs.len(),
        });
    }

    // Guard the near-miss is genuinely a near-miss: if the relay region were
    // empty the naive slice would coincide and the pinned negative row would be
    // vacuous.
    let naive = naive_contiguous_signed_input(&bytes, view.payload_range().end)?;
    if naive == ours {
        return Err(GenError::SpanDisagreement {
            span: "naive_contiguous_signed_input coincides with signed_input",
            spec_derived: ours.len(),
            codec_derived: naive.len(),
        });
    }

    debug_assert_eq!(view.signature_range().len(), SIGNATURE_SIZE);
    Ok(bytes.to_vec())
}
