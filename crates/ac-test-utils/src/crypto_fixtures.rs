//! Deterministic cryptographic fixtures for testing
//!
//! Provides reproducible Ed25519 keypairs and test vectors.
//! All fixtures are deterministic based on seed values.

use base64::engine::general_purpose;
use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair};
use thiserror::Error;

/// Test fixture error type
#[derive(Error, Debug)]
pub enum FixtureError {
    #[error("Cryptographic operation failed: {0}")]
    Crypto(String),
}

/// Generate a deterministic Ed25519 signing key for testing.
///
/// The same seed always produces the same keypair, ensuring test reproducibility.
///
/// # Arguments
/// * `seed` - Seed value for deterministic key generation (0-255)
///
/// # Returns
/// * `Ok((public_key_pem, private_key_pkcs8))` - Public key in PEM format, private key in PKCS#8 DER
///
/// # Example
/// ```rust,ignore
/// let (public_pem, private_pkcs8) = test_signing_key(1)?;
/// // Same seed always produces same key
/// let (public_pem2, private_pkcs8_2) = test_signing_key(1)?;
/// assert_eq!(public_pem, public_pem2);
/// ```
pub fn test_signing_key(seed: u8) -> Result<(String, Vec<u8>), FixtureError> {
    // Create deterministic 32-byte seed from input
    let mut seed_bytes = [0u8; 32];
    seed_bytes[0] = seed;
    // Fill rest with deterministic pattern
    for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
        *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
    }

    // Generate keypair from seed using ring's from_seed_unchecked
    // Note: This is deterministic and suitable for testing
    let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
        .map_err(|e| FixtureError::Crypto(format!("Failed to generate test keypair: {:?}", e)))?;

    // Get public key
    let public_key_bytes = key_pair.public_key().as_ref();

    // Format public key as PEM
    let public_key_b64 = general_purpose::STANDARD.encode(public_key_bytes);
    let public_key_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        public_key_b64
    );

    // Generate PKCS#8 document manually from the seed
    // Ring doesn't expose a method to get PKCS#8 from Ed25519KeyPair, so we build it
    let pkcs8_bytes = build_pkcs8_from_seed(&seed_bytes);

    Ok((public_key_pem, pkcs8_bytes))
}

/// Build PKCS#8 v1 document from Ed25519 seed
///
/// This is a test-only utility. Production code must use ring::rand::SystemRandom.
fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    // PKCS#8 v1 format for Ed25519 (RFC 5208):
    // SEQUENCE {
    //   version         INTEGER (0),
    //   algorithm       AlgorithmIdentifier,
    //   privateKey      OCTET STRING
    // }
    // Where privateKey for Ed25519 is:
    // OCTET STRING containing OCTET STRING with 32-byte seed

    let mut pkcs8 = Vec::new();

    // Outer SEQUENCE tag
    pkcs8.push(0x30);
    pkcs8.push(0x2e); // Length: 46 bytes

    // Version: INTEGER 0
    pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);

    // Algorithm Identifier: SEQUENCE
    pkcs8.push(0x30);
    pkcs8.push(0x05); // Length: 5 bytes
                      // OID for Ed25519: 1.3.101.112
    pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);

    // Private Key: OCTET STRING
    pkcs8.push(0x04);
    pkcs8.push(0x22); // Length: 34 bytes
                      // Inner OCTET STRING with seed
    pkcs8.push(0x04);
    pkcs8.push(0x20); // Length: 32 bytes
    pkcs8.extend_from_slice(seed);

    pkcs8
}

/// Test master key for encryption/decryption tests
///
/// Returns a deterministic 32-byte master key for AES-256-GCM testing.
pub fn test_master_key() -> Vec<u8> {
    vec![
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_key_is_deterministic() {
        let (pub1, priv1) = test_signing_key(1).unwrap();
        let (pub2, priv2) = test_signing_key(1).unwrap();

        assert_eq!(pub1, pub2, "Public keys should be identical for same seed");
        assert_eq!(
            priv1, priv2,
            "Private keys should be identical for same seed"
        );
    }

    #[test]
    fn test_different_seeds_produce_different_keys() {
        let (pub1, _) = test_signing_key(1).unwrap();
        let (pub2, _) = test_signing_key(2).unwrap();

        assert_ne!(pub1, pub2, "Different seeds should produce different keys");
    }

    #[test]
    fn test_master_key_is_32_bytes() {
        let key = test_master_key();
        assert_eq!(key.len(), 32, "Master key must be 32 bytes for AES-256");
    }
}
