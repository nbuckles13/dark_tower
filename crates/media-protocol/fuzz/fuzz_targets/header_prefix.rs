#![no_main]

//! Fuzzes the cut-through header parse (`parse_header_prefix`, R-21) on its
//! own, because ADR-0036 §2 names the reader-side length bound as the defect a
//! whole-frame decode fuzz target structurally misses.
//!
//! Properties, for every input:
//! * it returns, never panics;
//! * `Incomplete { needed }` is at least one and never asks past
//!   `MAX_HEADER_BYTES` in total — so a reader never buffers more than a header
//!   before deciding, whatever `payload_length` says;
//! * a `Complete` header's declared payload is within `MAX_PAYLOAD_BYTES`, its
//!   lengths are self-consistent, and it agrees with `decode_stream_frame`;
//! * a rejection is the same reason `decode_stream_frame` reports;
//! * the header-slice rewrite accepts exactly what the parse accepts and
//!   writes only relay bytes.

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{
    decode_stream_frame, parse_header_prefix, rewrite_relay_region_header, HeaderPrefix,
};
use media_protocol::frame::{
    MAX_FRAME_BYTES, MAX_HEADER_BYTES, MAX_PAYLOAD_BYTES, RELAY_REGION_SIZE, SIGNATURE_SIZE,
};

fuzz_target!(|data: &[u8]| {
    let parsed = parse_header_prefix(data);
    let stream = decode_stream_frame(data);
    match parsed {
        Ok(HeaderPrefix::Incomplete { needed }) => {
            assert!(needed >= 1);
            assert!(data.len() + needed <= MAX_HEADER_BYTES);
            assert!(matches!(stream, Ok(None)));
        }
        Ok(HeaderPrefix::Complete(h)) => {
            assert!(h.payload_length() <= MAX_PAYLOAD_BYTES);
            assert!(h.header_len() <= MAX_HEADER_BYTES);
            assert!(h.header_len() <= data.len());
            assert_eq!(h.frame_len(), h.header_len() + h.payload_length() + SIGNATURE_SIZE);
            assert_eq!(h.tail_len(), h.frame_len() - h.header_len());
            assert!(h.frame_len() <= MAX_FRAME_BYTES);
            match stream {
                Ok(Some(view)) => {
                    assert_eq!(view.encoded_len(), h.frame_len());
                    assert_eq!(view.gop_counter(), h.gop_counter());
                    assert_eq!(view.stream_id(), h.stream_id());
                    assert_eq!(view.hop_sequence(), h.hop_sequence());
                }
                Ok(None) => assert!(data.len() < h.frame_len()),
                Err(e) => panic!("header valid, stream decode rejected: {e:?}"),
            }
        }
        Err(e) => {
            let s = stream.expect_err("header rejected, stream decode did not");
            assert_eq!(s.reason(), e.reason());
        }
    }

    let mut writable = data.to_vec();
    match (rewrite_relay_region_header(&mut writable, 0x0BAD, 0xFEED_F00D), parsed) {
        (Ok(h), Ok(HeaderPrefix::Complete(p))) => {
            assert_eq!(h.header_len(), p.header_len());
            assert_eq!((h.stream_id(), h.hop_sequence()), (0x0BAD, 0xFEED_F00D));
            let relay = h.header_len() - RELAY_REGION_SIZE..h.header_len();
            for (i, (a, b)) in data.iter().zip(&writable).enumerate() {
                assert!(a == b || relay.contains(&i), "rewrite touched byte {i}");
            }
        }
        (Err(_), Ok(HeaderPrefix::Complete(_))) => panic!("rewrite refused a valid header"),
        (Ok(_), _) => panic!("rewrite accepted a header the parse did not"),
        (Err(_), _) => assert_eq!(writable, data, "a refused rewrite wrote"),
    }
});
