// FIXTURE — NOT PRODUCTION CODE, NOT COMPILED.
//
// A must-NOT-fire counterpart for the credential-leak semantic check
// (scripts/guards/semantic/checks.md, items 11-13). It carries key-adjacent
// vocabulary in shapes that are LEGITIMATE, so a check that name-matches rather
// than judging the value is caught reporting a finding here.
//
// Not a member of any cargo target; excluded from the Rust scanners twice over
// by common::test_code_filter::is_scan_exempt. See ../README.md.

use tracing::debug;

/// Control-plane diagnostics carrying key METADATA — which key, never the key.
pub fn log_generation(kek_generation: u64, sender_id: &str) {
    debug!(kek_generation, sender_id, key_custody = "operator", "media policy applied");
}

// Invariant: CLEAR — must NOT fire.
//
// checks.md's SAFE list names all three explicitly: `kek_generation` is
// "metadata that identifies WHICH key, never the key"; `sender_id` is out of
// scope for this check (barred from metric labels as a stable per-participant
// identifier, which is linkability, not leakage); and `key_custody` is the
// fixed `operator` label that exists in place of an end-to-end boolean.
//
// This fixture is the false-positive pole. A check that fires here is matching
// on the token `kek` rather than judging the value — and note the mechanical
// floor gets this right for a reason worth knowing: bare `kek` is DELIBERATELY
// ABSENT from CATEGORY_A precisely so `kek_generation` is not a false positive.
// Expected verdict: CLEAR.
