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

use proto_gen::dark_tower::internal::v1::MediaPolicyUpdate;

/// A builder for an MH-client call, taking unwrapped transmit-key bytes.
pub struct MediaPolicyBuilder {
    inner: MediaPolicyUpdate,
}

impl MediaPolicyBuilder {
    /// The crossing: transmit-key bytes passed to a builder for an
    /// `internal.v1` message. Per checks.md's custody table the unwrapped
    /// transmit key is held by the sending client ONLY — never MC, never MH,
    /// never anything on the wire.
    pub fn with_transmit_key(mut self, transmit_key_bytes: Vec<u8>) -> Self {
        self.inner.transmit_key_bytes = transmit_key_bytes;
        self
    }
}

// Invariant: FIRE — check item 11 (key material crossing the MC->MH contract),
// via a builder rather than a struct literal, because item 11 names "assigned
// into, or passed to a builder or constructor for" that contract.
// Expected verdict: FIRE.
