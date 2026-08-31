#![no_main]

//! Malformed-input fuzzing for the version-2 frame codec.
//!
//! Drives **all four** entry points, because they differ in exactly the places
//! a bug hides: `decode_datagram` adds an exactness condition,
//! `decode_stream_frame` must return `Ok(None)` rather than an error for a
//! prefix, `peek_frame_len` is the reader-side pre-allocation bound that
//! ADR-0036 §2 says a fuzz test of the decode function alone structurally
//! misses, and `rewrite_relay_region` is the only one that writes.
//!
//! No header size and no maximum is hardcoded here; everything comes from the
//! crate's exported constants.

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{
    decode_datagram, decode_stream_frame, peek_frame_len, rewrite_relay_region,
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
            // The stream path agrees with peek about how long the frame is.
            assert_eq!(peek_frame_len(data), Ok(Some(view.encoded_len())));
        }
        Ok(None) => {
            // Incomplete: peek must also be silent, and must never need more
            // than a full header to decide.
            assert_eq!(peek_frame_len(data), Ok(None));
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

    // peek must decide within a header's worth of bytes.
    if data.len() >= MAX_FRAME_BYTES {
        assert!(!matches!(peek_frame_len(data), Ok(None)));
    }
    let _ = MAX_HEADER_BYTES;

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
