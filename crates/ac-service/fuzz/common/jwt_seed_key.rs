//! The ONE fixed Ed25519 key of the `jwt_validation` fuzz harness, shared by the
//! target (`fuzz_targets/jwt_validation.rs`) and the seed generator
//! (`examples/gen_jwt_seed.rs`) via `#[path]`, so the two cannot drift.
//! Lives outside `fuzz_targets/` on purpose: every `.rs` there must be a `[[bin]]` with a
//! fuzz-nightly matrix entry (`crates/media-protocol/tests/fuzz_wiring.rs`).
//!
//! The seed is a deterministic TEST constant, not a credential: it verifies
//! nothing outside this harness. Fixing it makes the seed token the generator
//! writes at fuzz time identical every run (Ed25519 signing is deterministic).

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};

/// Fixed 32-byte Ed25519 seed (the private scalar's source).
pub const SEED: [u8; 32] = [0x42; 32];

/// RFC 8410 PKCS#8 v1 prefix for an Ed25519 private key; the 32-byte seed
/// follows it. `ring::Ed25519KeyPair::from_seed_unchecked` exports no PKCS#8,
/// and jsonwebtoken's `EncodingKey::from_ed_der` wants one (aws-lc-rs
/// `Ed25519KeyPair::from_pkcs8` accepts v1).
const PKCS8_V1_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// The claims shape the target decodes into.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
}

/// The raw 32-byte public key of [`SEED`] — a real curve point, so verification
/// reaches the curve arithmetic rather than rejecting the key.
pub fn public_key() -> Vec<u8> {
    Ed25519KeyPair::from_seed_unchecked(&SEED)
        .expect("a 32-byte seed is always a valid Ed25519 seed")
        .public_key()
        .as_ref()
        .to_vec()
}

pub fn decoding_key() -> DecodingKey {
    DecodingKey::from_ed_der(&public_key())
}

/// Only the generator signs; the target never needs the private half.
#[allow(
    dead_code,
    reason = "shared via #[path] into the fuzz target (unused) and gen_jwt_seed (used); #[expect] would be unfulfilled in the latter"
)]
pub fn encoding_key() -> EncodingKey {
    let mut pkcs8 = PKCS8_V1_PREFIX.to_vec();
    pkcs8.extend_from_slice(&SEED);
    EncodingKey::from_ed_der(&pkcs8)
}

/// The production validation shape: EdDSA only. Expiry is off so a fixed seed
/// token never ages out of the corpus.
pub fn validation() -> Validation {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = false;
    validation
}
