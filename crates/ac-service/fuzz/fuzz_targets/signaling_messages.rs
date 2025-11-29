#![no_main]

use libfuzzer_sys::fuzz_target;
use proto_gen::signaling::ClientMessage;
use proto_gen::Message; // Re-exported from prost

fuzz_target!(|data: &[u8]| {
    // Try to decode a ClientMessage from the fuzz input
    // Protocol Buffers should handle all malformed input gracefully
    let _ = ClientMessage::decode(data);

    // The fuzzer will explore:
    // - Invalid field numbers
    // - Truncated messages
    // - Invalid varint encodings
    // - Nested message overflow
    // - String encoding issues
    // - Enum value validation
});
