#![no_main]

use libfuzzer_sys::fuzz_target;
use media_protocol::codec::decode_frame;
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    // Convert raw bytes to Bytes type
    let buf = Bytes::copy_from_slice(data);

    // Try to decode the frame
    // This should never panic, only return Err for invalid input
    let _ = decode_frame(&mut buf.clone());

    // The fuzzer explores all code paths in decode_frame
    // looking for panics, infinite loops, or crashes
});
