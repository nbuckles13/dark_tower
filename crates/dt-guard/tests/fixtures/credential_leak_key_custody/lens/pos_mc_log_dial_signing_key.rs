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

use tracing::debug;

/// An MC-shaped diagnostic that dumps the meeting's private `media_dial`
/// signing key (raw seed bytes) into a log line.
pub fn log_dial_key(meeting_id: &str, dial_signing_key: &[u8; 32]) {
    debug!(meeting_id, ?dial_signing_key, "dial key provisioned");
}

// Invariant: FIRE — check item 12 (key material into an MC sink). The private
// `media_dial` signing key reaches a tracing macro. checks.md's custody table
// lists "any log, metric, span or error/panic payload" under "Never" for the
// private half (ADR-0039 §5: "never persisted, logged or Debug-printed").
// Expected verdict: FIRE.
