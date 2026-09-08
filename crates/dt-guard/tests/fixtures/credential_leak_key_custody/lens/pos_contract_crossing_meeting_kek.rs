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

/// MC building the MH registration request, with the meeting KEK assigned onto
/// it. Shaped after the real call site at crates/mc-service/src/grpc/mh_client.rs.
pub fn register_meeting(meeting_id: &str, meeting_kek: Vec<u8>) -> RegisterMeetingRequest {
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        // The crossing. MH is never an entitled holder of this value.
        meeting_kek,
        ..Default::default()
    }
}

// Invariant: FIRE — check item 11 (key material crossing the MC->MH contract).
// The meeting KEK is assigned into a `dark_tower.internal.v1` message
// construction, which checks.md's custody table lists under "Never" for MH.
// Expected verdict: FIRE.
