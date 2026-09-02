//! Media-path test fixtures (ADR-0036).

/// A syntactically valid Ed25519 identity public key for tests.
///
/// **Not real key material and not a real curve point.** A fixed, obviously
/// synthetic byte pattern: MC's join path performs an exact-length check on an
/// opaque 32-byte blob and no curve validation, so a pattern is sufficient and
/// no keypair generation is needed.
///
/// One home for the value rather than eleven inline literals across the MC test
/// suite. env-tests keeps its own copy in `crates/env-tests/src/fixtures/media.rs`
/// — it must not take a dev-dependency on this crate, which depends on
/// `mc-service`, because linking the service into a black-box cluster-test suite
/// inverts the ADR-0028 layering.
///
/// Using this in a test asserts nothing about identity: MC performs no `cnf`
/// binding, so a key accepted at join yields same-keyholder consistency only,
/// never a verified identity.
pub fn sample_identity_public_key() -> Vec<u8> {
    vec![0x42; 32]
}
