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

/// The meeting actor's dial key state. Only MC holds this.
pub struct DialKeyState {
    seed: Vec<u8>,
}

impl DialKeyState {
    /// Reads the raw seed out of the actor's state, named for its SHAPE.
    fn raw(&self) -> Vec<u8> {
        self.seed.clone()
    }
}

/// The laundering hop. Nothing in this signature names a credential.
fn collect_payload(state: &DialKeyState) -> Vec<u8> {
    let material = state.raw();
    material
}

/// The crossing, under a name no vocabulary contains, onto the ALLOWED field.
pub fn register(state: &DialKeyState, meeting_id: &str) -> RegisterMeetingRequest {
    let bytes = collect_payload(state);
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        // `bytes` IS the private dial seed: 32 bytes, so a length check on the
        // field passes too. Renaming did not change what it is.
        media_dial_public_key: bytes,
        ..Default::default()
    }
}

// Invariant: FIRE — check item 11, judge the VALUE, not the name. The private
// `media_dial` seed, renamed `bytes` through a helper, is assigned into the one
// allow-listed field. No vocabulary term appears on the path and the field is
// the sanctioned one, so only a check that follows the value catches it; a
// names-only check demonstrates the floor, not the check (checks.md §Fixture
// poles, "Highest-value plant").
// Expected verdict: FIRE.
