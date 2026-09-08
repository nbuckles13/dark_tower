// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

use proto_gen::dark_tower::signaling::v1::JoinResponse;

/// MC delivering the meeting KEK to a client it has just admitted.
pub fn build_join_response(meeting_id: &str, meeting_kek: Vec<u8>) -> JoinResponse {
    JoinResponse {
        meeting_id: meeting_id.to_string(),
        // Delivery to an ENTITLED holder. This is ADR-0036 §4's mechanism, not
        // a leak — and note the message is `signaling.v1`, the client-facing
        // contract, NOT `internal.v1`, which is what item 11 scopes to.
        meeting_kek,
        ..Default::default()
    }
}

// Invariant: CLEAR — must NOT fire.
//
// checks.md's SAFE list: "`JoinResponse.meeting_kek` and the KEK-push message.
// These ARE §4's delivery mechanism to a client MC has just admitted; delivery
// to an entitled holder is not leakage." The custody table lists "every client
// MC admits, via the join response and the KEK-push message" as an entitled
// holder of the meeting KEK.
//
// This is the fixture that distinguishes "the KEK moved" from "the KEK reached
// someone not in the custody table". A check firing here has collapsed the two.
// It also pairs against pos_contract_crossing_meeting_kek.rs on the axis that
// actually matters: same value, same spelling, different DESTINATION.
// Expected verdict: CLEAR.
