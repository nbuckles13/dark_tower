//! The fuzz seed table, shared by `corpus_seeds.rs` (which asserts each seed's
//! outcome and writes the gitignored corpus) and `header_prefix.rs` (which
//! asserts split-invariance over every seed).

use super::{
    build, build_with_gop, ext_length_offset, ext_region_offset, flip_bit, overwrite, set_byte,
    truncated_to, with_trailing, Shape, ALL_SHAPES, SENTINEL_PAYLOAD,
};
use media_protocol::codec::RejectReason;
use media_protocol::frame::{
    FLAGS_OFFSET, FLAG_DISCARDABLE, LEGAL_FLAG_MASK, MAX_EXT_BYTES, MAX_PAYLOAD_BYTES,
    PAYLOAD_LENGTH_OFFSET, PROTOCOL_VERSION, VERSION_OFFSET,
};

/// What a seed must do when decoded as a datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Decodes cleanly.
    Accept,
    /// Rejects with this specific reason.
    Reject(RejectReason),
}

pub struct Seed {
    /// Corpus file name.
    pub name: String,
    /// The bytes.
    pub bytes: Vec<u8>,
    /// Expected datagram outcome.
    pub outcome: Outcome,
}

const MAXIMAL: Shape = Shape {
    key_bearing: true,
    extensions: true,
};

/// Every seed.
#[must_use]
pub fn seeds() -> Vec<Seed> {
    let mut out: Vec<Seed> = Vec::new();
    let mut push = |name: String, bytes: Vec<u8>, outcome: Outcome| {
        out.push(Seed {
            name,
            bytes,
            outcome,
        });
    };

    // --- Accepting seeds: the full shape matrix, at several payload sizes ---
    for shape in ALL_SHAPES {
        for (label, payload) in [
            ("empty", [].as_slice()),
            ("small", SENTINEL_PAYLOAD.as_slice()),
            ("large", &[0x5Au8; 1024]),
        ] {
            let name = format!(
                "accept_key{}_ext{}_{label}",
                u8::from(shape.key_bearing),
                u8::from(shape.extensions)
            );
            push(name, build(shape, payload), Outcome::Accept);
        }
    }
    // Salience at both ends of the declared accepted range.
    for value in [0u8, 100] {
        push(
            format!("accept_salience_{value}"),
            super::build_with_salience(MAXIMAL, &SENTINEL_PAYLOAD, value),
            Outcome::Accept,
        );
    }

    // GoP counter at both ends of u32: every value decodes (ADR-0036 §2 step 8).
    for value in [0u32, u32::MAX] {
        push(
            format!("accept_gop_{value}"),
            build_with_gop(MAXIMAL, &SENTINEL_PAYLOAD, value),
            Outcome::Accept,
        );
    }

    let frame = build(MAXIMAL, &SENTINEL_PAYLOAD);

    push(
        "reject_v2_labelled".into(),
        set_byte(&frame, VERSION_OFFSET, 2),
        Outcome::Reject(RejectReason::UnknownVersion),
    );
    push(
        "reject_key_bearing_discardable".into(),
        set_byte(&frame, FLAGS_OFFSET, frame[FLAGS_OFFSET] | FLAG_DISCARDABLE),
        Outcome::Reject(RejectReason::KeyBearingDiscardable),
    );

    // --- One seed per reject reason ---
    push(
        "reject_unknown_version".into(),
        set_byte(&frame, VERSION_OFFSET, PROTOCOL_VERSION + 1),
        Outcome::Reject(RejectReason::UnknownVersion),
    );
    // Each undefined flag bit individually.
    for bit in 0..8u32 {
        if LEGAL_FLAG_MASK & (1u8 << bit) == 0 {
            push(
                format!("reject_flag_bit_{bit}"),
                flip_bit(&frame, FLAGS_OFFSET, bit),
                Outcome::Reject(RejectReason::ReservedFlagBitSet),
            );
        }
    }
    push(
        "reject_payload_over_max".into(),
        overwrite(
            &frame,
            PAYLOAD_LENGTH_OFFSET,
            &u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::PayloadLengthExceedsMax),
    );
    push(
        "accept_payload_at_max_declared_but_absent".into(),
        overwrite(
            &frame,
            PAYLOAD_LENGTH_OFFSET,
            &u32::try_from(MAX_PAYLOAD_BYTES).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::PayloadLengthExceedsAvailable),
    );
    push(
        "reject_ext_too_large".into(),
        overwrite(
            &frame,
            ext_length_offset(&frame),
            &u16::try_from(MAX_EXT_BYTES + 1).unwrap().to_be_bytes(),
        ),
        Outcome::Reject(RejectReason::ExtensionsTooLarge),
    );
    push(
        "reject_ext_unknown_type".into(),
        set_byte(&frame, ext_region_offset(&frame), 0x7F),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    push(
        "reject_ext_wrong_value_length".into(),
        set_byte(&frame, ext_region_offset(&frame) + 1, 2),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    push(
        "reject_ext_value_out_of_range".into(),
        set_byte(&frame, ext_region_offset(&frame) + 2, 101),
        Outcome::Reject(RejectReason::ExtensionsMalformed),
    );
    for extra in [1usize, 8] {
        push(
            format!("reject_trailing_{extra}"),
            with_trailing(&frame, extra),
            Outcome::Reject(RejectReason::TrailingBytes),
        );
    }

    // --- Truncation at every byte boundary, for both conditional shapes ---
    for shape in ALL_SHAPES {
        let full = build(shape, &SENTINEL_PAYLOAD);
        for len in 0..full.len() {
            push(
                format!(
                    "truncate_key{}_ext{}_{len}",
                    u8::from(shape.key_bearing),
                    u8::from(shape.extensions)
                ),
                truncated_to(&full, len),
                // Every prefix of a valid frame is short of some region, and on
                // the datagram path a shortfall is either `truncated` or, for
                // the payload specifically, `payload_length_exceeds_available`.
                // Both are asserted below by class rather than pinned here.
                Outcome::Reject(RejectReason::Truncated),
            );
        }
    }
    out
}
