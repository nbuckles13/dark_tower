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

/// A struct whose path to key bytes is a RAW array — no redacting wrapper.
#[derive(Debug)]
pub struct MeetingAdmission {
    pub meeting_id: String,
    /// Raw bytes. `#[derive(Debug)]` will print every one of them.
    pub meeting_kek: [u8; 32],
}

impl MeetingAdmission {
    pub fn synthetic() -> Self {
        Self {
            meeting_id: "m-0".to_string(),
            meeting_kek: [0xC3u8; 32],
        }
    }
}

// Invariant: FIRE — check item 13 (redaction defeated at the call site), via a
// `#[derive(Debug)]` on a struct that transitively reaches key material through
// a NON-redacting field.
//
// The pairing with neg_derive_debug_secretbox_wrapper.rs is the test: that
// fixture is identical but for the field TYPE. checks.md's poles were amended so
// both spell the field `meeting_kek`, precisely so the wrapper type is the sole
// variable — a check that merely name-matched would pass both while never
// exercising the wrapper logic.
// Expected verdict: FIRE.
