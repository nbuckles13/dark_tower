// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

use std::fmt;

/// The in-tree safe form: a hand-rolled `Debug` over key-adjacent bytes that
/// prints a byte COUNT and never content. Modelled on `RedactedLen` at
/// crates/proto-gen/src/lib.rs.
pub struct RedactedLen<'a>(pub &'a [u8]);

impl fmt::Debug for RedactedLen<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted {} bytes>", self.0.len())
    }
}

/// Logging the LENGTH of transmit-key material, never the material.
pub fn log_key_len(transmit_key_bytes: &[u8]) {
    tracing::debug!(len = ?RedactedLen(transmit_key_bytes), "transmit key installed");
}

// Invariant: CLEAR — must NOT fire.
//
// checks.md's SAFE list: "Key LENGTHS and length constants as metadata.
// `RedactedLen` in crates/proto-gen/src/lib.rs is the in-tree safe form — it
// prints a byte count, never content."
//
// The subtlety this pole exists for: item 13 says "a hand-rolled Debug/Display
// that prints key bytes is ALWAYS a finding, wrapper or not". This is a
// hand-rolled `Debug` over key material that is nonetheless safe, because it
// prints a count. So the item-13 rule is about what the impl PRINTS, not about
// whether it was hand-rolled — and a check that keys on "hand-rolled Debug near
// key material" rather than on what it emits fires here and is wrong.
// Note also `transmit_key_bytes` appears here in a SAFE shape, so a check that
// pattern-matched that spelling from the positive fixtures would misfire.
// Expected verdict: CLEAR.
