//! End-to-end proof that a generated frame is openable by the receive path:
//! decode → verify → unwrap → decrypt → plaintext.
//!
//! This is the check that the load-bearing construction order is actually
//! right. A frame built with the AAD sliced before `payload_length` was written
//! would still round-trip through its own encoder — but it would fail *here*,
//! because this opens the frame using the AAD taken from the **wire bytes**,
//! exactly as a foreign implementation would.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "Test code: a panic is the failure report."
)]

use media_protocol::frame::{AEAD_TAG_BYTES, KEY_ID_BYTES};
use media_vector_gen::{crypto, fixtures, rows, schedule, CIPHER_SUITE_ID};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::signature;

#[test]
fn generated_frame_opens_from_wire_bytes_alone() {
    let spec = rows::FrameSpec::base("smoke", "smoke", "full_frame");
    let built = rows::build_frame(&spec).expect("build");
    let frame = &built.frame;

    // Everything below uses ONLY the wire bytes and the fixture keys a receiver
    // would hold — never a value remembered from the build.
    let view = media_protocol::codec::decode_datagram(frame).expect("decode");

    // 1. Verify the signature over publisher region ‖ payload.
    let identity = crypto::Identity::from_seed(&fixtures::ALICE_IDENTITY_SEED).unwrap();
    let public = signature::UnparsedPublicKey::new(&signature::ED25519, identity.public_key());
    let [pubreg, payload] = view.signed_ranges();
    let mut signed = Vec::new();
    signed.extend_from_slice(pubreg);
    signed.extend_from_slice(payload);
    public
        .verify(&signed, view.signature())
        .expect("signature must verify over publisher region then payload");

    // 2. Read the key id from the payload's clear header and unwrap the
    //    transmit key under the meeting KEK, bound to that key id.
    let kid: [u8; KEY_ID_BYTES] = payload[..KEY_ID_BYTES].try_into().unwrap();
    let wrapped = view.wrapped_transmit_key().expect("key-bearing");
    let mut wrap_buf = wrapped.expose_wrapped_key().to_vec();
    wrap_buf.extend_from_slice(wrapped.expose_wrap_tag());
    let kek = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &fixtures::MEETING_KEK).unwrap());
    let transmit_key = kek
        .open_in_place(
            Nonce::assume_unique_for_key(crypto::wrap_nonce(&kid)),
            Aad::from(kid),
            &mut wrap_buf,
        )
        .expect("KEK unwrap must succeed with nonce = 0x00000000||kid and aad = kid");
    assert_eq!(
        transmit_key,
        fixtures::ALICE_TRANSMIT_KEY,
        "unwrapped transmit key must equal the sender's base key"
    );

    // 3. Derive, and decrypt with the AAD taken from the WIRE.
    let derived = schedule::derive(transmit_key, &kid, CIPHER_SUITE_ID).unwrap();
    let nonce = schedule::nonce_from(&derived.salt, u64::from(view.stream_sequence()));
    let tag = &payload[KEY_ID_BYTES..KEY_ID_BYTES + AEAD_TAG_BYTES];
    let ct = &payload[KEY_ID_BYTES + AEAD_TAG_BYTES..];

    let mut buf = ct.to_vec();
    buf.extend_from_slice(tag);
    let sframe = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &derived.key).unwrap());
    let plaintext = sframe
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(view.publisher_region()),
            &mut buf,
        )
        .expect(
            "decrypt must succeed with AAD = the publisher region taken from the wire. \
             Failure here means payload_length was not written before the AAD was sliced.",
        );
    assert_eq!(plaintext, fixtures::PLAINTEXT);
}

#[test]
fn aad_is_a_strict_subset_of_the_signed_range() {
    let spec = rows::FrameSpec::base("subset", "subset", "roundtrip");
    let built = rows::build_frame(&spec).expect("build");
    let aad = hex::decode(&built.row.derived.aead_aad_hex).unwrap();
    let signed = hex::decode(&built.row.derived.signed_input_hex).unwrap();
    let naive = hex::decode(&built.row.derived.naive_contiguous_signed_input_hex).unwrap();

    assert!(signed.starts_with(&aad), "AAD must prefix the signed input");
    assert!(
        aad.len() < signed.len(),
        "AAD must be a STRICT subset: equal lengths would mean the payload is unsigned"
    );
    assert_ne!(
        naive, signed,
        "the naive contiguous slice must DIFFER from the signed input, or the pinned \
         near-miss row is vacuous"
    );
    assert_eq!(
        naive.len() - signed.len(),
        media_protocol::frame::RELAY_REGION_SIZE,
        "the difference must be exactly the relay region"
    );
}
