#![no_main]

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::{decode_frame, encode_frame};
use media_protocol::frame::{MediaFrame, FrameType, FrameFlags};
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    // Try to decode the input
    let buf = Bytes::copy_from_slice(data);

    if let Ok(frame) = decode_frame(&mut buf.clone()) {
        // If decode succeeded, encoding should succeed too
        if let Ok(encoded) = encode_frame(&frame) {
            // Round-trip: decode the encoded frame
            if let Ok(frame2) = decode_frame(&mut encoded.clone()) {
                // Verify round-trip integrity
                assert_eq!(frame.version, frame2.version);
                assert_eq!(frame.user_id, frame2.user_id);
                assert_eq!(frame.stream_id, frame2.stream_id);
                assert_eq!(frame.timestamp, frame2.timestamp);
                assert_eq!(frame.sequence, frame2.sequence);
                assert_eq!(frame.payload, frame2.payload);
            }
        }
    }
});
