//! The reject-reason vocabulary: every reason, its precedence, and which entry
//! points can produce it.
//!
//! Three families of test here, each proving something the others do not:
//!
//! 1. **One test per reason**, asserting the *specific* `RejectReason` rather
//!    than `is_err()`.
//! 2. **Precedence**, one test per adjacent pair in the normative order. Two
//!    implementations can both "correctly reject" the same bytes with
//!    different tokens while a cross-language drift guard reports clean.
//! 3. **Producibility**, cross-checked against behaviour rather than declared
//!    twice. See the two tests at the bottom, which compose deliberately:
//!    `declared_producibility_matches_behaviour` proves each declaration is
//!    *true* at sampled points, and `no_prefix_of_a_valid_frame_ever_errs`
//!    proves the underlying invariant across the whole prefix space. Neither
//!    subsumes the other — the sampled test constructs a *complete* malformed
//!    extension region and so never exercises a truncation landing mid-TLV.
//!    Do not delete either as redundant.

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
    audio_frame, build, canonical_frame, ext_length_offset, ext_region_offset, flip_bit, overwrite,
    set_byte, truncated_to, try_build_with_salience, with_trailing, Shape, ALL_SHAPES,
    SENTINEL_PAYLOAD,
};
use media_protocol::codec::{
    decode_datagram, decode_stream_frame, peek_frame_len, rewrite_relay_region, DecodeError,
    ProducibleBy, RejectReason, ALL_REJECT_REASONS,
};
use media_protocol::frame::{
    MAX_EXT_BYTES, MAX_PAYLOAD_BYTES, PROTOCOL_VERSION, PUBLISHER_FIXED_PREFIX_SIZE,
    SIGNATURE_SIZE, VERSION_OFFSET,
};

const MAXIMAL: Shape = Shape {
    key_bearing: true,
    extensions: true,
};

fn reason_of(bytes: &[u8]) -> RejectReason {
    decode_datagram(bytes)
        .expect_err("expected a rejection")
        .reason()
}

// ---------------------------------------------------------------------------
// One test per reason
// ---------------------------------------------------------------------------

/// Bytes that produce each reason on the datagram path.
///
/// Every entry mutates an encoder-produced valid frame at a named constant
/// offset. Nothing here is a literal byte array: the negative cases are
/// unencodable by construction, and hand-writing them would make this file a
/// second home for the layout.
fn datagram_case(reason: RejectReason) -> Vec<u8> {
    let frame = canonical_frame();
    match reason {
        RejectReason::UnknownVersion => set_byte(&frame, VERSION_OFFSET, PROTOCOL_VERSION + 1),
        RejectReason::ReservedFlagBitSet => {
            flip_bit(&frame, media_protocol::frame::FLAGS_OFFSET, 3)
        }
        RejectReason::PayloadLengthExceedsMax => {
            let over = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap();
            overwrite(
                &frame,
                media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
                &over.to_be_bytes(),
            )
        }
        RejectReason::PayloadLengthExceedsAvailable => {
            // Within the wire-format maximum, so step 5 passes; larger than the
            // bytes present, so step 11 fires.
            let declared = u32::try_from(MAX_PAYLOAD_BYTES).unwrap();
            overwrite(
                &frame,
                media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
                &declared.to_be_bytes(),
            )
        }
        RejectReason::Truncated => truncated_to(&frame, PUBLISHER_FIXED_PREFIX_SIZE - 1),
        RejectReason::ExtensionsTooLarge => {
            let over = u16::try_from(MAX_EXT_BYTES + 1).unwrap();
            overwrite(&frame, ext_length_offset(&frame), &over.to_be_bytes())
        }
        RejectReason::ExtensionsMalformed => {
            // Known length, correct order, region consumed exactly — only the
            // type byte is unregistered.
            set_byte(&frame, ext_region_offset(&frame), 0x7F)
        }
        RejectReason::TrailingBytes => with_trailing(&frame, 1),
    }
}

#[test]
fn every_reason_has_a_case_and_produces_exactly_itself() {
    for &reason in ALL_REJECT_REASONS {
        let bytes = datagram_case(reason);
        assert_eq!(
            reason_of(&bytes),
            reason,
            "case for {} produced the wrong reason",
            reason.as_str()
        );
    }
}

#[test]
fn reason_tokens_are_the_frozen_spellings() {
    // A value pin, not a second definition: the enum owns the strings and this
    // asserts the wire tokens have not silently changed.
    let tokens: Vec<&str> = ALL_REJECT_REASONS.iter().map(|r| r.as_str()).collect();
    assert_eq!(
        tokens,
        vec![
            "unknown_version",
            "reserved_flag_bit_set",
            "payload_length_exceeds_max",
            "payload_length_exceeds_available",
            "truncated",
            "extensions_too_large",
            "extensions_malformed",
            "trailing_bytes",
        ]
    );
}

#[test]
fn all_reject_reasons_is_complete() {
    // `ALL_REJECT_REASONS` is generated by the same macro that defines the
    // enum and its tokens, so a variant cannot exist without an entry — the
    // omission path is closed by construction rather than by this test.
    //
    // What remains checkable here, and what this asserts: the generated list
    // has no duplicate, and its length matches an independent count of the
    // variants reachable through a wildcard-free match. If a future edit
    // replaces the macro with hand-written lists, `index_of` stops compiling
    // when a variant is added, and the contiguity assertion catches a variant
    // added to the enum but not to the const.
    const fn index_of(reason: RejectReason) -> usize {
        match reason {
            RejectReason::UnknownVersion => 0,
            RejectReason::ReservedFlagBitSet => 1,
            RejectReason::PayloadLengthExceedsMax => 2,
            RejectReason::PayloadLengthExceedsAvailable => 3,
            RejectReason::Truncated => 4,
            RejectReason::ExtensionsTooLarge => 5,
            RejectReason::ExtensionsMalformed => 6,
            RejectReason::TrailingBytes => 7,
        }
    }

    let mut indices: Vec<usize> = ALL_REJECT_REASONS.iter().copied().map(index_of).collect();
    indices.sort_unstable();
    let expected: Vec<usize> = (0..indices.len()).collect();
    assert_eq!(
        indices, expected,
        "ALL_REJECT_REASONS must contain every variant exactly once, with no gap"
    );

    let mut tokens: Vec<&str> = ALL_REJECT_REASONS.iter().map(|r| r.as_str()).collect();
    let before = tokens.len();
    tokens.sort_unstable();
    tokens.dedup();
    assert_eq!(
        before,
        tokens.len(),
        "ALL_REJECT_REASONS contains a duplicate"
    );
}

// ---------------------------------------------------------------------------
// Precedence: one test per adjacent pair in the normative order
// ---------------------------------------------------------------------------

#[test]
fn version_precedes_flags() {
    let frame = canonical_frame();
    let both = set_byte(
        &flip_bit(&frame, media_protocol::frame::FLAGS_OFFSET, 7),
        VERSION_OFFSET,
        9,
    );
    assert_eq!(reason_of(&both), RejectReason::UnknownVersion);
}

#[test]
fn version_precedes_truncation() {
    // One byte, wrong version: version dispatch happens before the prefix
    // length check, which is why the version sits at offset 0.
    let bytes = set_byte(&canonical_frame(), VERSION_OFFSET, 9);
    assert_eq!(
        reason_of(&truncated_to(&bytes, 1)),
        RejectReason::UnknownVersion
    );
}

#[test]
fn flags_precede_payload_length() {
    let frame = canonical_frame();
    let over = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap();
    let both = flip_bit(
        &overwrite(
            &frame,
            media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
            &over.to_be_bytes(),
        ),
        media_protocol::frame::FLAGS_OFFSET,
        6,
    );
    assert_eq!(reason_of(&both), RejectReason::ReservedFlagBitSet);
}

#[test]
fn payload_length_max_precedes_extension_length() {
    let frame = canonical_frame();
    let over_payload = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap();
    let over_ext = u16::try_from(MAX_EXT_BYTES + 1).unwrap();
    let both = overwrite(
        &overwrite(&frame, ext_length_offset(&frame), &over_ext.to_be_bytes()),
        media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
        &over_payload.to_be_bytes(),
    );
    assert_eq!(reason_of(&both), RejectReason::PayloadLengthExceedsMax);
}

#[test]
fn extension_length_limit_precedes_extension_grammar() {
    // An over-large declared length is terminal before any TLV byte is walked.
    let frame = canonical_frame();
    let over = u16::try_from(MAX_EXT_BYTES + 1).unwrap();
    let bytes = set_byte(
        &overwrite(&frame, ext_length_offset(&frame), &over.to_be_bytes()),
        ext_region_offset(&frame),
        0x7F,
    );
    assert_eq!(reason_of(&bytes), RejectReason::ExtensionsTooLarge);
}

#[test]
fn extension_grammar_precedes_payload_availability() {
    let frame = canonical_frame();
    let declared = u32::try_from(MAX_PAYLOAD_BYTES).unwrap();
    let bytes = set_byte(
        &overwrite(
            &frame,
            media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
            &declared.to_be_bytes(),
        ),
        ext_region_offset(&frame),
        0x7F,
    );
    assert_eq!(reason_of(&bytes), RejectReason::ExtensionsMalformed);
}

#[test]
fn signature_shortfall_precedes_payload_availability() {
    // Fewer than SIGNATURE_SIZE bytes after the relay region: the signature is
    // a fixed-size region, so this is `truncated`, not a payload complaint.
    let frame = canonical_frame();
    let short = truncated_to(&frame, frame.len() - SIGNATURE_SIZE);
    assert_eq!(reason_of(&short), RejectReason::Truncated);
}

#[test]
fn payload_availability_precedes_trailing_bytes() {
    let frame = canonical_frame();
    let declared = u32::try_from(MAX_PAYLOAD_BYTES).unwrap();
    let bytes = with_trailing(
        &overwrite(
            &frame,
            media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
            &declared.to_be_bytes(),
        ),
        4,
    );
    assert_eq!(
        reason_of(&bytes),
        RejectReason::PayloadLengthExceedsAvailable
    );
}

// ---------------------------------------------------------------------------
// The extension grammar, one case per rejection rule
// ---------------------------------------------------------------------------

#[test]
fn unknown_extension_type_is_rejected_not_skipped() {
    let frame = canonical_frame();
    let bytes = set_byte(&frame, ext_region_offset(&frame), 0x7F);
    assert_eq!(reason_of(&bytes), RejectReason::ExtensionsMalformed);
}

#[test]
fn wrong_extension_value_length_is_rejected() {
    let frame = canonical_frame();
    let bytes = set_byte(&frame, ext_region_offset(&frame) + 1, 0);
    assert_eq!(reason_of(&bytes), RejectReason::ExtensionsMalformed);
}

#[test]
fn salience_at_the_top_of_the_declared_range_decodes() {
    let frame = try_build_with_salience(MAXIMAL, &SENTINEL_PAYLOAD, 100)
        .expect("100 is inside the declared accepted set");
    let view = decode_datagram(&frame).expect("must decode");
    assert_eq!(view.extensions().declared_salience(), Some(100));
}

#[test]
fn salience_one_past_the_declared_range_is_rejected() {
    // Both sides of the boundary, because an off-by-one here is a silently
    // widened accepted set — the thing the value-set clause exists to prevent.
    let valid = try_build_with_salience(MAXIMAL, &SENTINEL_PAYLOAD, 100).unwrap();
    let bytes = set_byte(&valid, ext_region_offset(&valid) + 2, 101);
    assert_eq!(reason_of(&bytes), RejectReason::ExtensionsMalformed);
}

#[test]
fn encoder_enforces_the_same_value_set_as_the_decoder() {
    // An encoder able to emit bytes the decoder rejects would break the
    // canonical-roundtrip identity and report the failure in the wrong place.
    assert!(try_build_with_salience(MAXIMAL, &SENTINEL_PAYLOAD, 101).is_err());
}

// ---------------------------------------------------------------------------
// Producibility: declared against observed
// ---------------------------------------------------------------------------

/// Cross-check every declared producibility against actual behaviour.
///
/// Exhaustiveness over `ALL_REJECT_REASONS` proves the classification is
/// *complete*; feeding the same bytes to both entry points proves it is *true*.
/// A declaration that is merely self-consistent would pass the first and fail
/// this.
///
/// Composes with `no_prefix_of_a_valid_frame_ever_errs`, which proves the
/// underlying invariant over the whole prefix space rather than at these
/// sampled points.
#[test]
fn declared_producibility_matches_behaviour() {
    for &reason in ALL_REJECT_REASONS {
        let bytes = datagram_case(reason);
        assert_eq!(
            reason_of(&bytes),
            reason,
            "datagram case mismatch for {reason}"
        );

        // `rewrite_relay_region` is the fourth fallible entry point, and a
        // media handler calls it on stream-carried video. Checking only the
        // two decode paths would assert two thirds of the claim — which is how
        // `Truncated` and `PayloadLengthExceedsAvailable` came to be labelled
        // datagram-only while being reachable on the video path.
        let mut writable = bytes.clone();
        let rewrite = rewrite_relay_region(&mut writable, 1, 2);
        let stream = decode_stream_frame(&bytes);
        let peek = peek_frame_len(&bytes);

        match reason.producible_by() {
            ProducibleBy::AllEntryPoints => {
                for (name, produced) in [
                    ("decode_stream_frame", stream.err().map(|e| e.reason())),
                    ("peek_frame_len", peek.err().map(|e| e.reason())),
                    ("rewrite_relay_region", rewrite.err().map(|e| e.reason())),
                ] {
                    assert_eq!(
                        produced,
                        Some(reason),
                        "{reason} is declared AllEntryPoints but {name} did not produce it"
                    );
                }
            }
            ProducibleBy::CompleteFrameRequired => {
                // The prefix-tolerant entry points must NOT reject.
                assert!(
                    stream.is_ok(),
                    "{reason} is declared CompleteFrameRequired but decode_stream_frame rejected"
                );
                assert!(
                    peek.is_ok(),
                    "{reason} is declared CompleteFrameRequired but peek_frame_len rejected"
                );
                // The relay rewrite must, and this is the half the old
                // `DatagramOnly` label denied.
                assert_eq!(
                    rewrite.err().map(|e| e.reason()),
                    Some(reason),
                    "{reason} is declared CompleteFrameRequired but rewrite_relay_region did not \
                     produce it — this is the claim that must hold for MH's video path"
                );
            }
            ProducibleBy::DecodeDatagramOnly => {
                for (name, outcome_is_err) in [
                    ("decode_stream_frame", stream.is_err()),
                    ("peek_frame_len", peek.is_err()),
                    ("rewrite_relay_region", rewrite.is_err()),
                ] {
                    assert!(
                        !outcome_is_err,
                        "{reason} is declared decode_datagram-only but {name} rejected"
                    );
                }
            }
        }

        // The media claim, checked rather than inferred from a variant name.
        assert_eq!(
            reason.producible_by().reachable_on_stream_carried_frames(),
            !matches!(reason.producible_by(), ProducibleBy::DecodeDatagramOnly),
            "reachable_on_stream_carried_frames disagrees with the entry-point set"
        );
    }
}

/// A limit or grammar violation must be TERMINAL even on a buffer too short to
/// hold a whole frame.
///
/// This is the boundary that makes the datagram/stream split safe, and every
/// other stream-path `Err` assertion misses it: `declared_producibility_matches_behaviour`
/// feeds *complete* frames, and `no_prefix_of_a_valid_frame_ever_errs` uses
/// prefixes of *valid* frames, so neither exercises a buffer that is both short
/// **and** already self-contradicting.
///
/// If a limit violation were reported as `Ok(None)` on a truncated buffer, a
/// peer could stall a stream indefinitely with bytes that can never complete,
/// and the only thing catching it would be the caller's buffer/timeout
/// backstop rather than the codec's own terminal decision.
#[test]
fn a_violation_visible_in_a_short_prefix_is_terminal_on_the_stream_path() {
    let frame = canonical_frame();

    // Over-max payload length, truncated to just the publisher prefix: the
    // limit check at step 5 fires before any body-availability check.
    let over = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap();
    let short_over_max = truncated_to(
        &overwrite(
            &frame,
            media_protocol::frame::PAYLOAD_LENGTH_OFFSET,
            &over.to_be_bytes(),
        ),
        PUBLISHER_FIXED_PREFIX_SIZE,
    );
    let err = decode_stream_frame(&short_over_max)
        .expect_err("a limit violation is terminal, never Ok(None)");
    assert_eq!(err.reason(), RejectReason::PayloadLengthExceedsMax);

    // Undefined flag bit, truncated to exactly the publisher prefix — far
    // short of a complete frame, but past the point the parser inspects flags.
    let short_bad_flag = truncated_to(
        &flip_bit(&frame, media_protocol::frame::FLAGS_OFFSET, 5),
        PUBLISHER_FIXED_PREFIX_SIZE,
    );
    let err = decode_stream_frame(&short_bad_flag)
        .expect_err("a grammar violation is terminal, never Ok(None)");
    assert_eq!(err.reason(), RejectReason::ReservedFlagBitSet);

    // The boundary of the property, asserted so it is not mistaken for a gap:
    // a *two-byte* buffer carrying the same bad flag byte is `Ok(None)`, not a
    // rejection. The parser requires the whole fixed publisher prefix before
    // inspecting any field inside it — the prefix-length row precedes the flag
    // row in the normative order — so at two bytes it has not yet reached the
    // flag check. "Terminal" means *once the violation has been reached*, not
    // "as soon as the offending byte is present in the buffer". A TypeScript
    // port that checked flags before the prefix length would diverge here, and
    // this is the assertion that would catch it.
    let two_bytes = truncated_to(&flip_bit(&frame, media_protocol::frame::FLAGS_OFFSET, 5), 2);
    assert!(
        matches!(decode_stream_frame(&two_bytes), Ok(None)),
        "a buffer short of the publisher prefix is incomplete, whatever it contains"
    );
}

/// The P3 invariant, proved across the whole prefix space.
///
/// **On the stream path, no strict prefix of a valid frame may produce `Err`.**
/// A prefix contains no limit or grammar violation by construction — every
/// length and every flag in it came from a valid frame — so any `Err` is the
/// defect. If this ever fails, a frame spanning a read boundary is being
/// reported as a permanent structural fault, which under this crate's own
/// consumer guidance means reset-the-stream with no resync possible: a
/// remotely-triggerable stream kill.
///
/// Run over **all four shapes** — the cross product of the layout's two
/// conditional regions, key-bearing x extensions-present. This is the same 2x2
/// the cross-language vectors pin as the four associated-data spans, reused
/// rather than a second hand-written list. The shape this story actually
/// carries in production (key-bearing audio, zero extensions) never places a
/// prefix boundary inside a TLV region, so testing only it would pass
/// vacuously over exactly the boundary this test is best placed to catch.
#[test]
fn no_prefix_of_a_valid_frame_ever_errs() {
    let mut frames: Vec<Vec<u8>> = Vec::new();
    for shape in ALL_SHAPES {
        frames.push(build(shape, &SENTINEL_PAYLOAD));
        // Zero-length payload: the signature boundary lands immediately after
        // the relay region.
        frames.push(build(shape, &[]));
    }
    for frame in frames {
        for len in 0..frame.len() {
            let prefix = truncated_to(&frame, len);
            let outcome = decode_stream_frame(&prefix);
            assert!(
                matches!(outcome, Ok(None)),
                "prefix of length {len} produced {outcome:?}; a prefix of a valid frame must be \
                 incomplete, never a rejection"
            );
            // peek must never reject a prefix either, and must stay silent
            // until the length is genuinely knowable.
            assert!(
                peek_frame_len(&prefix).is_ok(),
                "peek rejected a valid prefix at {len}"
            );
        }
        // The whole frame decodes, so the prefixes above really were prefixes
        // of something valid.
        assert!(decode_datagram(&frame).is_ok());
    }
}

#[test]
fn trailing_bytes_are_a_datagram_concern_only() {
    // On a stream the bytes after the signature *are the next frame*. If the
    // exactness check ever migrated to the stream path, every video stream
    // would reject everything after its first frame — which presents as "video
    // works for exactly one frame then dies" and reads like an encoder bug.
    let first = canonical_frame();
    let second = audio_frame();
    let mut concatenated = first.clone();
    concatenated.extend_from_slice(&second);

    let view = decode_stream_frame(&concatenated)
        .expect("two concatenated frames must not be a rejection")
        .expect("the first frame is complete");
    assert_eq!(view.encoded_len(), first.len());

    let rest = &concatenated[view.encoded_len()..];
    let second_view = decode_stream_frame(rest).unwrap().unwrap();
    assert_eq!(second_view.encoded_len(), second.len());

    // The same bytes on the datagram path are a rejection.
    assert_eq!(reason_of(&concatenated), RejectReason::TrailingBytes);
}

#[test]
fn decode_error_display_leaks_no_buffer_bytes() {
    for &reason in ALL_REJECT_REASONS {
        let bytes = datagram_case(reason);
        let err: DecodeError = decode_datagram(&bytes).unwrap_err();
        let rendered = format!("{err} {err:?}");
        assert!(!rendered.contains("A1A1"), "wrapped key material rendered");
        assert!(!rendered.contains("C3C3"), "signature material rendered");
    }
}
