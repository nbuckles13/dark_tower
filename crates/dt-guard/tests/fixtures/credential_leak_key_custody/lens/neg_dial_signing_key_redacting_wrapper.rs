// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

/// The in-tree redacting wrapper (crates/mc-service/src/media_dial/key.rs):
/// its own hand-written `Debug` prints a constant, never a byte, length or
/// fingerprint of either half.
pub struct DialSigningKey(ring::signature::Ed25519KeyPair);

impl std::fmt::Debug for DialSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DialSigningKey([REDACTED])")
    }
}

/// Identical to pos_derive_debug_raw_dial_key_bytes.rs except for the field TYPE.
#[derive(Debug)]
pub struct MeetingDialState {
    pub meeting_id: String,
    pub dial_signing_key: DialSigningKey,
}

// Invariant: CLEAR — must NOT fire. checks.md item 13: a `#[derive(Debug)]` is
// SAFE when every field on the path to key bytes is a redacting wrapper whose
// own `Debug` redacts — and item 13 requires verifying that it DOES (a crate
// type is not SAFE by name): here the wrapper's `Debug` is hand-written and
// prints a constant, and crates/mc-service/src/media_dial/key.rs has a test
// asserting exactly that. THE PAIRING IS THE TEST: same field name
// `dial_signing_key` as the must-fire, so the wrapper TYPE is the only
// variable.
// Expected verdict: CLEAR.
