#![no_main]

//! Malformed-input fuzzing for the version-3 frame codec.
//!
//! Drives the whole-frame entry points, because they differ in exactly the
//! places a bug hides: `decode_datagram` adds an exactness condition,
//! `decode_stream_frame` must return `Ok(None)` rather than an error for a
//! prefix, `parse_header_prefix` is the reader-side pre-allocation bound that
//! ADR-0036 §2 says a fuzz test of the decode function alone structurally
//! misses, and `rewrite_relay_region` writes. The cut-through header API has
//! its own targets: `header_prefix` and `header_reader_split`.
//!
//! No header size and no maximum is hardcoded here; everything comes from the
//! crate's exported constants.

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{
    decode_datagram, decode_stream_frame, parse_header_prefix, rewrite_relay_region, HeaderPrefix,
};
use media_protocol::frame::{MAX_FRAME_BYTES, MAX_HEADER_BYTES};

fuzz_target!(|data: &[u8]| {
    // Every entry point must return, never panic, for any input.
    if let Ok(view) = decode_datagram(data) {
        // A successfully decoded datagram is consumed exactly.
        assert_eq!(view.encoded_len(), data.len());
        assert!(view.encoded_len() <= MAX_FRAME_BYTES);
        // The regions must tile the frame with no gap and no overlap.
        assert_eq!(view.publisher_region_range().end, view.relay_region_range().start);
        assert_eq!(view.relay_region_range().end, view.payload_range().start);
        assert_eq!(view.payload_range().end, view.signature_range().start);
        assert_eq!(view.signature_range().end, view.encoded_len());
    }

    match decode_stream_frame(data) {
        Ok(Some(view)) => {
            assert!(view.encoded_len() <= data.len());
            // The stream path agrees with the header parse about the length.
            match parse_header_prefix(data) {
                Ok(HeaderPrefix::Complete(h)) => assert_eq!(h.frame_len(), view.encoded_len()),
                other => panic!("stream decoded but header parse said {other:?}"),
            }
        }
        Ok(None) => {
            // Incomplete: the header parse never rejects what the stream path
            // holds open. It is incomplete while the header is short, and once
            // the header is complete it names a length the buffer does not yet
            // reach — its whole job as the pre-allocation bound.
            match parse_header_prefix(data) {
                Ok(HeaderPrefix::Incomplete { needed }) => {
                    assert!(data.len() + needed <= MAX_HEADER_BYTES);
                }
                Ok(HeaderPrefix::Complete(h)) => {
                    assert!(h.frame_len() > data.len());
                    assert!(h.frame_len() <= MAX_FRAME_BYTES);
                }
                Err(err) => panic!("incomplete on stream, terminal on header parse: {err:?}"),
            }
            assert!(data.len() < MAX_FRAME_BYTES);
        }
        Err(err) => {
            // A terminal stream rejection is terminal on the datagram path too,
            // with the same reason: `decode_datagram` is the stream path plus an
            // exactness condition, never a weaker check.
            let datagram = decode_datagram(data).expect_err("terminal on stream, ok on datagram");
            assert_eq!(datagram.reason(), err.reason());
        }
    }

    // The header parse must decide within a header's worth of bytes.
    if data.len() >= MAX_HEADER_BYTES {
        assert!(!matches!(parse_header_prefix(data), Ok(HeaderPrefix::Incomplete { .. })));
    }

    // The write path, on a copy: it must validate to the decoder's standard
    // before touching a byte.
    let mut writable = data.to_vec();
    if rewrite_relay_region(&mut writable, 0x1234, 0x89AB_CDEF).is_ok() {
        let view = decode_stream_frame(&writable)
            .expect("a successful rewrite leaves a decodable frame")
            .expect("a successful rewrite leaves a complete frame");
        assert_eq!(view.stream_id(), 0x1234);
        assert_eq!(view.hop_sequence(), 0x89AB_CDEF);
        // Only the relay region may have changed.
        let relay = view.relay_region_range();
        for (index, (before, after)) in data.iter().zip(writable.iter()).enumerate() {
            assert!(
                before == after || relay.contains(&index),
                "rewrite touched byte {index} outside the relay region"
            );
        }
    } else {
        assert_eq!(writable, data, "a rejected rewrite must not have written anything");
    }
});
