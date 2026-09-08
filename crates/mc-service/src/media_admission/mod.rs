//! Per-meeting admission facts for the ADR-0036 media path.
//!
//! MC's per-meeting actor is the sole issuing authority for three facts that
//! the media path binds keys to, and this module is their one home. Each
//! carries a different discipline:
//!
//! | Fact | Discipline | Failure if absent |
//! |---|---|---|
//! | [`kek::MeetingKek`] | secret MC generates and must never let escape | the operator-custody claim becomes false |
//! | [`identity_key::IdentityPublicKey`] | client-supplied blob, shape-checked before republication | a malformed blob fans out to every roster |
//! | [`sender_id::SenderId`] | bounded monotonic namespace, never reused | AES-GCM (key, nonce) collision — R-35 |
//!
//! # The security floor, stated once for everything below
//!
//! **This story performs NO `cnf` thumbprint check.** MC does not verify a
//! joiner's identity key against the meeting token's `cnf` claim, and
//! publishes it on the roster as received — **trust on first use**. A frame
//! signature that verifies against a roster key therefore proves only that
//! every frame verifying against it came from **the same keyholder**. It never
//! proves *who* that keyholder is, and for a guest it never proves more than a
//! pseudonym (ADR-0036 §4).
//!
//! Say "same-keyholder consistency", never "verified identity". The
//! client-validated AC attestation that closes this is **story 2**, and when
//! it lands the attestation wins over the roster on disagreement.
//!
//! Naming floor: nothing here may be called `attested`, `verified` or
//! `trusted` — not a field, not a type, not a local, not a test.
//!
//! # Key custody
//!
//! Media is encrypted between clients; MH, transport and storage cannot read
//! it; **MC can** (ADR-0036 §4). That is accepted operator custody, recorded as
//! the user's risk decision. Nothing here may describe it as end-to-end against
//! the operator or as zero-trust, and no metric, log, span or comment may carry
//! an end-to-end or zero-trust boolean. Telemetry carries
//! `key_custody=operator` instead.

pub mod binding_response;
pub mod identity_key;
pub mod kek;
pub mod sender_id;

pub use binding_response::SenderBindingOutcome;
pub use identity_key::{IdentityPublicKey, IdentityPublicKeyRejected, IDENTITY_PUBLIC_KEY_BYTES};
pub use kek::{KekGenerationFailed, MeetingKek, MeetingKeyState, MEETING_KEK_BYTES};
pub use sender_id::{Allocation, SenderId, SenderIdAllocator, SenderIdSpaceExhausted};

/// Admission fixtures for `src/` unit tests.
///
/// One home for the two admission values that `ParticipantInfo` literals need,
/// so the byte pattern has a single place in the crate rather than one copy per
/// test module. Derives the key from `mc_test_utils::media`, which is the
/// fixture home for the suite as a whole, so this is a typed adapter and not a
/// second value.
///
/// `src/` unit tests only. Integration tests under `tests/` go through
/// `tests/common/mod.rs`, which wraps the same `mc-test-utils` helper.
///
/// Using these asserts nothing about identity: MC performs no `cnf` binding, so
/// a key accepted at join yields same-keyholder consistency only.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{IdentityPublicKey, SenderId};

    /// A valid presented identity key. MC republishes the presented key; the
    /// value is a synthetic pattern and not real key material.
    pub(crate) fn sample_identity_key() -> Option<IdentityPublicKey> {
        Some(
            IdentityPublicKey::try_from_bytes(&mc_test_utils::media::sample_identity_public_key())
                .expect("fixture key is exactly 32 bytes"),
        )
    }

    /// A stand-in allocated id. MC allocates a real one at admission.
    pub(crate) fn sample_sender_id() -> SenderId {
        SenderId::from_nonzero(std::num::NonZeroU16::MIN)
    }
}
