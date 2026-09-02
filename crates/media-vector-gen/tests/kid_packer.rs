//! The key-id packer's refusal behaviour.
//!
//! # Why an error rather than a mask is a security property, not style
//!
//! A masking pack aliases `sender_id` 65536 to **0**. Two senders would then
//! share a key id, hence a (key, nonce) pair — and under AES-GCM a repeated
//! pair does not merely expose those two frames: it leaks the authentication
//! subkey and permits forgery. A bare shift is worse still, overflowing
//! silently into the adjacent field. Both are refused.
//!
//! Each row here sits **adjacent to an accepted maximum** that the
//! `key_id_all_max_roundtrip` vector pins, so the boundary is nailed from both
//! sides: the largest legal value round-trips through a real frame, and one
//! more is an error rather than a silently colliding key id.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "Test code: a panic is the failure report."
)]

use media_protocol::frame::{KEY_ID_GENERATION_BITS, KEY_ID_SENDER_ID_BITS, KEY_ID_STREAM_BITS};
use media_vector_gen::error::GenError;
use media_vector_gen::kid::KeyIdParts;

fn parts(sender_id: u64, stream: u64, generation: u64) -> KeyIdParts {
    KeyIdParts {
        sender_id,
        stream,
        generation,
    }
}

#[test]
fn sender_id_one_past_max_errors_rather_than_aliasing_to_zero() {
    let err = parts(1 << KEY_ID_SENDER_ID_BITS, 1, 1).pack().unwrap_err();
    match err {
        GenError::KeyIdFieldOverflow { field, value, .. } => {
            assert_eq!(field, "sender_id");
            // The error carries the offending VALUE, not just "out of range".
            // Dropping it would drop the evidence for which field collided.
            assert_eq!(value, 65536);
        }
        other => panic!("expected KeyIdFieldOverflow, got {other:?}"),
    }
    // The alias it would have produced is itself rejected, so even a masking
    // implementation could not reach a usable key id by this route.
    assert!(matches!(
        parts(0, 1, 1).pack().unwrap_err(),
        GenError::SenderIdZero
    ));
}

#[test]
fn stream_one_past_max_errors() {
    let err = parts(1, 1 << KEY_ID_STREAM_BITS, 1).pack().unwrap_err();
    match err {
        GenError::KeyIdFieldOverflow { field, value, .. } => {
            assert_eq!(field, "stream");
            assert_eq!(value, 256);
        }
        other => panic!("expected KeyIdFieldOverflow, got {other:?}"),
    }
}

#[test]
fn generation_one_past_max_errors() {
    let err = parts(1, 1, 1 << KEY_ID_GENERATION_BITS).pack().unwrap_err();
    match err {
        GenError::KeyIdFieldOverflow { field, value, .. } => {
            assert_eq!(field, "generation");
            assert_eq!(value, 1 << 40);
        }
        other => panic!("expected KeyIdFieldOverflow, got {other:?}"),
    }
}

#[test]
fn every_field_at_its_maximum_packs_and_is_the_all_ones_key_id() {
    let packed = parts(
        (1 << KEY_ID_SENDER_ID_BITS) - 1,
        (1 << KEY_ID_STREAM_BITS) - 1,
        (1 << KEY_ID_GENERATION_BITS) - 1,
    )
    .pack()
    .expect("all-max must be ACCEPTED; the error rows above are worthless otherwise");
    assert_eq!(
        packed, [0xFF; 8],
        "the three fields must exactly fill the key id with no gap and no overlap"
    );
}

#[test]
fn fields_do_not_bleed_into_one_another() {
    // A shift-based pack with a too-large stream would corrupt `generation`.
    // Setting each field alone must leave the others zero.
    assert_eq!(
        parts(1, 0, 0).pack().unwrap(),
        [0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        parts(1, 0xFF, 0).pack().unwrap(),
        [0x00, 0x01, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        parts(1, 0, 0xFF).pack().unwrap(),
        [0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF]
    );
}
