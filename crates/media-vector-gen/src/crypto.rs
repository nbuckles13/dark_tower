//! The composed frame operations: KEK wrap, sign, seal.
//!
//! NON-PRODUCTION crate — see crate docs.

use crate::error::GenError;
use crate::schedule::{self, SALT_LEN};
use media_protocol::frame::{AEAD_TAG_BYTES, KEY_ID_BYTES, SIGNATURE_SIZE};
use ring::signature::{Ed25519KeyPair, KeyPair};

/// Length of the wrapped-transmit-key material.
pub const WRAPPED_KEY_LEN: usize = 32;

/// The KEK-wrap nonce: `0x00000000 || key_id` (12 bytes, key id right-aligned).
///
/// ADR-0036 §4 says the wrap nonce is *"derived from the key id"* and that
/// *"nonce uniqueness under the KEK reduces to key-id uniqueness"*. Any
/// injective derivation satisfies that, so the ADR alone did not fix the bytes;
/// the story contract pins zero-prefixing, and this changeset records it in §4
/// so it has a normative home that is not a manifest prompt.
///
/// Zero-**prefix** rather than zero-suffix so the codebase has **one** padding
/// rule: `SFrame`'s own nonce is `salt XOR BE12(counter)`, which right-aligns a
/// big-endian value in exactly the same way.
#[must_use]
pub fn wrap_nonce(key_id: &[u8; KEY_ID_BYTES]) -> [u8; SALT_LEN] {
    let mut n = [0u8; SALT_LEN];
    n[SALT_LEN - KEY_ID_BYTES..].copy_from_slice(key_id);
    n
}

/// The 50-byte wrapped-transmit-key field: `kek_generation(2) || wrapped(32) || tag(16)`.
pub struct WrappedField {
    /// The wrapped key material.
    pub wrapped_key: [u8; WRAPPED_KEY_LEN],
    /// The detached wrap tag.
    pub wrap_tag: [u8; AEAD_TAG_BYTES],
    /// The nonce used, pinned in the vectors.
    pub nonce: [u8; SALT_LEN],
    /// The associated data used: the key-id bytes alone.
    pub aad: [u8; KEY_ID_BYTES],
}

/// Wrap a transmit key under the meeting KEK, bound to `key_id`.
///
/// The associated data is the **key id alone** (§4: *"the key id as associated
/// data"*), and `key_id` must be the bytes taken from the encoded frame rather
/// than a value re-packed from decomposed fields — so a decomposition bug
/// surfaces as a crypto mismatch instead of cancelling out.
///
/// One transmit key therefore wraps to **one** ciphertext, byte-identical from
/// frame to frame within a generation (§4). Rows `wrap_determinism_seq_lo` /
/// `_hi` assert exactly that.
///
/// # Errors
///
/// [`GenError::Crypto`] on any `ring` failure.
pub fn wrap_transmit_key(
    kek: &[u8],
    key_id: &[u8; KEY_ID_BYTES],
    transmit_key: &[u8; WRAPPED_KEY_LEN],
) -> Result<WrappedField, GenError> {
    let nonce = wrap_nonce(key_id);
    let sealed = schedule::seal_detached(kek, &nonce, key_id, transmit_key)?;
    let mut wrapped_key = [0u8; WRAPPED_KEY_LEN];
    wrapped_key.copy_from_slice(&sealed.ciphertext);
    Ok(WrappedField {
        wrapped_key,
        wrap_tag: sealed.tag,
        nonce,
        aad: *key_id,
    })
}

/// An Ed25519 identity, derived deterministically from a fixed seed.
pub struct Identity {
    pair: Ed25519KeyPair,
}

impl Identity {
    /// Build from a 32-byte seed. Ed25519 is deterministic per RFC 8032, so a
    /// pinned seed makes the signature bytes reproducible — which is what lets
    /// the vectors gate the **send** path and not only verification.
    ///
    /// # Errors
    ///
    /// [`GenError::Crypto`] if the seed is rejected.
    pub fn from_seed(seed: &[u8]) -> Result<Self, GenError> {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| GenError::Crypto {
            operation: "Ed25519KeyPair::from_seed_unchecked",
        })?;
        Ok(Self { pair })
    }

    /// The public key bytes, published on the roster in production.
    #[must_use]
    pub fn public_key(&self) -> Vec<u8> {
        self.pair.public_key().as_ref().to_vec()
    }

    /// Sign the signed input (§3): publisher region, then payload.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_SIZE] {
        let sig = self.pair.sign(message);
        let mut out = [0u8; SIGNATURE_SIZE];
        out.copy_from_slice(sig.as_ref());
        out
    }
}
