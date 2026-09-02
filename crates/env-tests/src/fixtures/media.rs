//! Media-path test fixtures (ADR-0036).

/// A syntactically valid Ed25519 identity public key for join requests.
///
/// **Not real key material and not a real curve point.** MC's join path checks
/// length only — exactly 32 bytes, no curve validation — so a fixed synthetic
/// pattern is sufficient and no keypair generation is needed.
///
/// ANCHOR (DRY): the source of this value is
/// `mc_test_utils::media::sample_identity_public_key`. env-tests deliberately
/// does NOT take a dev-dependency on `mc-test-utils` to share it: that crate
/// depends on `mc-service`, and linking a service crate into a suite whose whole
/// premise is black-box validation against a deployed cluster inverts the
/// ADR-0028 layering. env-tests links exactly one local crate (`proto-gen`) and
/// zero service crates, and that thinness is the point. Two homes is the
/// accepted cost; keep them equal in shape, not by import.
///
/// Passing this asserts nothing about identity: MC performs no `cnf` binding,
/// so an accepted key gives same-keyholder consistency and never a verified
/// identity.
pub fn sample_identity_public_key() -> Vec<u8> {
    vec![0x42; 32]
}
