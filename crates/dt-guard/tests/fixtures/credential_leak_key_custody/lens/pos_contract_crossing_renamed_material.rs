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

/// The meeting actor's in-memory key state. Only MC holds this.
pub struct MeetingKeyState {
    kek: Vec<u8>,
}

impl MeetingKeyState {
    /// Reads the raw KEK out of the actor's state. Note the return value is
    /// named for its SHAPE, not its meaning.
    fn raw(&self) -> Vec<u8> {
        self.kek.clone()
    }
}

/// The laundering hop. Nothing in this signature names a credential.
fn collect_payload(state: &MeetingKeyState) -> Vec<u8> {
    let material = state.raw();
    material
}

/// The crossing, under a name no vocabulary contains.
pub fn register(state: &MeetingKeyState, meeting_id: &str) -> RegisterMeetingRequest {
    let blob = collect_payload(state);
    RegisterMeetingRequest {
        meeting_id: meeting_id.to_string(),
        // `blob` IS the meeting KEK. Renaming did not change what it is.
        opaque_bootstrap: blob,
        ..Default::default()
    }
}

// `opaque_bootstrap` IS A HYPOTHETICAL FIELD on a real message, and the reason
// it has to be hypothetical is worth more than the fixture.
//
// Since story 4 P1, `dark_tower.internal.v1` carries exactly ONE `bytes`
// field, `RegisterMeetingRequest.media_dial_public_key`: a 32-byte PUBLIC
// verification key, the single `ALLOWED_BYTES_FIELDS` entry (ADR-0039 §5).
// Every other `bytes` declaration still reds `internal-proto-no-key-material`,
// so the contract still has no declared place for secret or media key bytes,
// and `opaque_bootstrap` still has no home there. That is the "confirm the
// premise against the real artifact" half of ADR-0036 §11 holding by
// construction rather than by vigilance -- but the structural block is now
// NARROWER: the guard reads declarations, not values, and a 32-byte check
// cannot tell a public key from the meeting KEK, a private seed or any other
// 32-byte secret; any of them assigned into that one field is green there and
// is caught only by item 11's value axis (checks.md).
//
// The asymmetry is designed, not accidental: `signaling.v1` carries
// `bytes meeting_kek` twice — `JoinResponse` (line 412) and `MeetingKekUpdate`
// (line 871), §4's delivery to an ENTITLED holder — while `internal.v1` carries
// no MEDIA-key `bytes` field (only the allow-listed public verification key).
// That is the structural expression of "the client is entitled, MH is not."
//
// TRIGGER FIRED AT STORY 4 P1: the structural block now has one narrow opening
// (`media_dial_public_key`). This fixture keeps the hypothetical field so it
// still exercises follow-the-value through an undeclared home. A real-field
// variant (KEK or private seed into `media_dial_public_key`) is the natural
// plant for that opening; it belongs with the story-4 poles MC-D owes
// (checks.md "Story-4 poles are owed"). The original trigger, for the record:
// **the moment a `bytes` field is added to `internal.v1`, that structural
// enforcement is gone and this fixture stops being hypothetical.** `proto/**`
// is an enumerated ADR-0024 §6.4 Guarded Shared Area, so such an addition
// already routes through owner co-sign — which is where this note is meant to
// be read. Do not read the remaining absence (of any other `bytes` field) as a
// permanent property.
//
// **What remains at that moment is items 11-13.** The structural block and the
// semantic check are not ranked alternatives — the block is why the check has had
// nothing to catch here yet, and the check is what catches it the day the block
// goes. A structural block removed without the check behind it is the silent
// regression §11 warns about.

// Invariant: FIRE — check item 11, reached only by FOLLOWING THE VALUE.
//
// This is the strongest plant in the set and the one that distinguishes the
// semantic check from the mechanical floor. checks.md §"Judge the value, not
// the name" names exactly this shape: "A Vec<u8> read out of the meeting
// actor's KEK field, moved through a helper, and renamed bytes / payload /
// material / blob is the same finding under a name no vocabulary contains."
//
// No word-list can reach it: `raw`, `material`, `blob` and
// `opaque_bootstrap` are in no vocabulary, and the only occurrence of `kek`
// is a private field two function hops away from the crossing. A hunk showing
// only `collect_payload` does not clear it.
//
// IF THIS FIXTURE DOES NOT FIRE, THAT IS A FINDING ABOUT THE CHECK, recorded
// and escalated — never a cue to make the plant more obvious until it passes.
// Expected verdict: FIRE.
