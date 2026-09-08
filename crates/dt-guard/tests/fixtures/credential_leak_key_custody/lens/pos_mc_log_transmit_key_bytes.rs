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

/// An MC-shaped diagnostic that debug-formats transmit-key bytes.
pub fn log_rotation(transmit_key_bytes: &[u8]) {
    debug!(?transmit_key_bytes, "rotating sender transmit key");
}

// Invariant: FIRE — check item 12 (key material into an MC sink), via a tracing
// FIELD rather than a format string.
//
// This fixture is also the one that carries the mechanical floor's known gap:
// `transmit_key` IS a CATEGORY_A entry, but the word-boundary matchers cannot
// see past the trailing `_bytes` (`_` is a word character, so \btransmit_key\b
// has no boundary there). Confirmed by running the shipped guard, not inferred.
// The semantic check is what covers this line; the vocabulary is a floor under
// it, and this spelling is below the floor's reach at a log site.
// Expected verdict: FIRE.
