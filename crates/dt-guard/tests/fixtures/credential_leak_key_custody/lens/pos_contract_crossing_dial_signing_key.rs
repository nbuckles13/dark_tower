// DELIBERATE-LEAK FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// This file plants a key-custody leak on purpose so the credential-leak
// semantic check (scripts/guards/semantic/checks.md, items 11-13) can be shown
// to catch it. Every byte value here is synthetic — `vec![0xC3u8; 32]`, the
// in-tree convention from crates/proto-gen/tests/signaling_roundtrip.rs — and
// no value is derived from any real KDF, capture, or deployment.
//
// COPYING ANY LINE OF THIS FILE INTO PRODUCTION IS THE DEFECT THIS FILE EXISTS
// TO CATCH. It is not a template.
//
// Not a member of any cargo target: unreferenced .rs under tests/ is compiled
// by nothing, so fmt and clippy never see it. Excluded from the Rust scanners
// twice over by common::test_code_filter::is_scan_exempt (the /fixtures/ path
// segment, and crates/dt-guard/**). See ../README.md before adding a fixture.

use proto_gen::dark_tower::internal::v1::RegisterMeetingRequest;

/// The meeting actor's dial key state, held as raw private bytes.
pub struct MeetingDialState {
    dial_signing_key: [u8; 32],
}

/// The crossing, BY NAME: the private half is put on the one allow-listed
/// bytes field instead of the public half.
pub fn register(state: &MeetingDialState, meeting_id: &str) -> RegisterMeetingRequest {
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        media_dial_public_key: state.dial_signing_key.to_vec(),
        ..Default::default()
    }
}

// Invariant: FIRE — check item 11 (key material crossing the MC→MH contract),
// VALUE axis. `media_dial_public_key` is the one (message, field) ADR-0039 §5
// allows, but only for bytes from the PUBLIC half; the private signing key on
// it is item 11 even though `internal-proto-no-key-material` stays green (the
// guard reads declarations, not values). The field name is satisfied, the
// value is not.
// Expected verdict: FIRE.
