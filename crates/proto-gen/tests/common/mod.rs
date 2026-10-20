//! Helpers shared by the proto-gen wire-shape tests (`signaling_roundtrip.rs`,
//! `internal_roundtrip.rs`). Each test binary compiles this module; it carries
//! no lint attributes of its own, so each binary's crate-level lint posture
//! applies to it.

use proto_gen::Message;

/// An enum number neither `signaling.proto` nor `internal.proto` names, for the
/// open-enum tests.
pub const UNKNOWN_ENUM_VALUE: i32 = 99;

/// Encode then decode: a failed decode of bytes we just encoded IS the failure.
pub fn roundtrip<T: Message + Default>(msg: &T) -> T {
    let bytes = msg.encode_to_vec();
    T::decode(bytes.as_slice()).expect("decode must succeed for a message we just encoded")
}

/// Top-level field numbers present in an encoded message, in wire order.
///
/// Derived from the bytes rather than compared against a golden vector, so an
/// assertion cannot be satisfied by a stale expectation. Handles the two wire
/// types the messages under test use at top level (varint and
/// length-delimited) and fails loudly on anything else rather than guessing.
pub fn top_level_field_numbers(mut bytes: &[u8]) -> Vec<u64> {
    /// `None` on a truncated or over-long (more than 64-bit) varint.
    fn varint(buf: &mut &[u8]) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (byte, rest) = buf.split_first()?;
            *buf = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
    let mut fields = Vec::new();
    while !bytes.is_empty() {
        let key = varint(&mut bytes).expect("valid field key");
        let wire_type = key & 0x7;
        assert!(
            wire_type == 0 || wire_type == 2,
            "unexpected wire type {wire_type} at top level"
        );
        fields.push(key >> 3);
        if wire_type == 0 {
            varint(&mut bytes).expect("valid varint field value");
        } else {
            let len = usize::try_from(varint(&mut bytes).expect("valid length prefix"))
                .expect("length fits usize");
            bytes = bytes
                .get(len..)
                .expect("length-delimited field within buffer");
        }
    }
    fields
}
