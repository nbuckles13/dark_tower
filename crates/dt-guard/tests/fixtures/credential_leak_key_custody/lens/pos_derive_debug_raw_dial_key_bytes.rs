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

/// Meeting-actor dial state with the private key held as raw bytes and a
/// derived `Debug` over it.
#[derive(Debug)]
pub struct MeetingDialState {
    pub meeting_id: String,
    pub dial_signing_key: [u8; 32],
}

// Invariant: FIRE — check item 13 (redaction defeated at the call site): a
// `#[derive(Debug)]` on a struct that transitively reaches the private
// `media_dial` signing key as raw bytes, so any `{:?}` of it prints the key.
// Paired with neg_dial_signing_key_redacting_wrapper.rs, which differs ONLY in
// the field TYPE (same field name `dial_signing_key`) — do not tidy the names
// apart.
// Expected verdict: FIRE.
