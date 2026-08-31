//! Executable proof that the v2 header has no skipped bytes and no skipped
//! bits.
//!
//! ADR-0036 §2 requires that *"every byte and every bit is either decoded into
//! a field the receiver inspects, or rejected if set — nothing is skipped"*,
//! and the ADR separately insists that a control's coverage be **demonstrated,
//! not asserted**. "We deleted the six reserved bytes" is only the assertion
//! half; this is the demonstration.
//!
//! Method: take a valid frame, mutate **every byte position** in turn, and
//! assert each position either changes an observable decoded value or produces
//! a rejection. A position that does neither is a byte nobody inspects — a
//! covert channel, which at ~180 B/s/stream is the exact defect §2 names in
//! the v1 codec.
//!
//! The signature and the payload are covered only because the view exposes
//! them as observable values: decode does not validate either, so mutating one
//! changes no header field and triggers no rejection. Without those accessors
//! they would be a 64-byte and a payload-sized hole in this proof.

// Test code: a panic on a bad index IS the failure signal here, and these
// helpers deliberately index at named constant offsets.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]

mod common;

use common::{build, flip_bit, set_byte, Shape, ALL_SHAPES, SENTINEL_PAYLOAD};
use media_protocol::codec::decode_datagram;
use media_protocol::frame::{MediaFrameView, FLAGS_OFFSET, LEGAL_FLAG_MASK};

/// Everything a receiver can observe about a decoded frame, as one comparable
/// value. If a mutation changes no element of this tuple and does not reject,
/// the mutated byte is unobservable.
fn observable(view: &MediaFrameView<'_>) -> String {
    format!(
        "{:?}|{}|{}|{:?}|{}|{}|{}|{:?}|{:?}|{:?}",
        view.flags(),
        view.payload_length(),
        view.stream_sequence(),
        view.wrapped_transmit_key().map(|w| (
            w.kek_generation(),
            *w.expose_wrapped_key(),
            *w.expose_wrap_tag()
        )),
        view.stream_id(),
        view.hop_sequence(),
        view.encoded_len(),
        view.extensions().as_bytes(),
        view.payload(),
        view.signature(),
    )
}

#[test]
fn every_byte_position_is_decoded_or_rejected() {
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let baseline_view = decode_datagram(&frame).expect("fixture must decode");
        let baseline = observable(&baseline_view);

        for offset in 0..frame.len() {
            // Two mutations per position: XOR-with-0xFF and set-to-zero. One
            // alone can be a no-op when the original byte happens to equal the
            // replacement.
            let candidates = [
                set_byte(&frame, offset, frame[offset] ^ 0xFF),
                set_byte(&frame, offset, 0),
            ];
            let mut covered = false;
            for mutated in &candidates {
                if mutated == &frame {
                    continue;
                }
                match decode_datagram(mutated) {
                    Err(_) => covered = true,
                    Ok(view) => {
                        if observable(&view) != baseline {
                            covered = true;
                        }
                    }
                }
            }
            assert!(
                covered,
                "byte {offset} of the {shape:?} frame is neither decoded into an observable \
                 value nor rejected when changed: it is a covert channel"
            );
        }
    }
}

#[test]
fn every_flag_bit_is_decoded_or_rejected() {
    let shape = Shape {
        key_bearing: true,
        extensions: true,
    };
    let frame = build(shape, &SENTINEL_PAYLOAD);
    let baseline = observable(&decode_datagram(&frame).expect("fixture must decode"));

    for bit in 0..8u32 {
        let mutated = flip_bit(&frame, FLAGS_OFFSET, bit);
        let defined = LEGAL_FLAG_MASK & (1u8 << bit) != 0;
        match decode_datagram(&mutated) {
            Err(err) => {
                if defined {
                    // Flipping a defined bit may still reject: clearing
                    // key-bearing reinterprets the wrapped-key field as
                    // extensions. That is a decode of the bit, not a skip.
                    let _ = err;
                } else {
                    assert_eq!(
                        err.reason(),
                        media_protocol::codec::RejectReason::ReservedFlagBitSet,
                        "undefined flag bit {bit} must reject as reserved_flag_bit_set"
                    );
                }
            }
            Ok(view) => {
                assert!(
                    defined,
                    "undefined flag bit {bit} was accepted — it is a covert channel"
                );
                assert_ne!(
                    observable(&view),
                    baseline,
                    "defined flag bit {bit} changed nothing observable"
                );
            }
        }
    }
}

#[test]
fn no_byte_of_the_frame_is_outside_a_named_region() {
    // The regions must tile the frame exactly, with no gap and no overlap.
    // A gap would be bytes belonging to no field; an overlap would mean a byte
    // is authenticated by one region and rewritable through another.
    for shape in ALL_SHAPES {
        let frame = build(shape, &SENTINEL_PAYLOAD);
        let view = decode_datagram(&frame).expect("fixture must decode");
        let publisher = view.publisher_region_range();
        let relay = view.relay_region_range();
        let payload = view.payload_range();
        let signature = view.signature_range();

        assert_eq!(publisher.start, 0);
        assert_eq!(
            publisher.end, relay.start,
            "gap or overlap between publisher and relay"
        );
        assert_eq!(
            relay.end, payload.start,
            "gap or overlap between relay and payload"
        );
        assert_eq!(
            payload.end, signature.start,
            "gap or overlap between payload and signature"
        );
        assert_eq!(
            signature.end,
            frame.len(),
            "frame does not end at the signature"
        );
    }
}
