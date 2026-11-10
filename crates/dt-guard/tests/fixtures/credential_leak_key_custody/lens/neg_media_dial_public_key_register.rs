// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

use proto_gen::dark_tower::internal::v1::RegisterMeetingRequest;

/// The in-tree shape: a redacting wrapper exposing only the public half.
pub struct DialSigningKey(ring::signature::Ed25519KeyPair);

impl DialSigningKey {
    pub fn public_key(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(ring::signature::KeyPair::public_key(&self.0).as_ref());
        out
    }
}

pub fn register(dial_signing_key: &DialSigningKey, meeting_id: &str) -> RegisterMeetingRequest {
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        media_dial_public_key: dial_signing_key.public_key().to_vec(),
        ..Default::default()
    }
}

// Invariant: CLEAR — must NOT fire. The ONE permitted key bytes on the MC→MH
// contract: the 32-byte PUBLIC verification key, on the one ADR-0039 §5 pair
// (RegisterMeetingRequest, media_dial_public_key), read through the wrapper's
// public accessor. checks.md item 11's exception, both axes satisfied (field
// AND value). A check that fires here is name-matching `dial_signing_key`
// rather than judging the value; the paired must-fire is
// pos_contract_crossing_dial_signing_key.rs (private half, same field).
// Expected verdict: CLEAR.
