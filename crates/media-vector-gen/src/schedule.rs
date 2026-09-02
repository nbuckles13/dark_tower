//! RFC 9605 §4.4 `SFrame` key schedule, and the AEAD primitives.
//!
//! NON-PRODUCTION crate — see crate docs.
//!
//! # This module is NOT independently authored, and that is deliberate
//!
//! Everything here is **anchored externally** to the sframe-wg test vectors at
//! commit `025d568`, vendored under `proto/test-vectors/external/sframe-wg/`.
//! `tests/external_sframe_gate.rs` feeds the external row's `base_key` / `kid` /
//! `ctr` into these functions and compares against the external expected
//! `sframe_secret` / `sframe_key` / `sframe_salt` / `nonce`.
//!
//! Independent authorship of a key schedule would be worth far less than
//! agreement with the specification's own vectors: it is the only part of this
//! crate where an outside oracle exists, so using it is strictly better than
//! being clever. The three computations that have **no** outside oracle live in
//! [`crate::spec`].
//!
//! # Ciphersuite 0x0005 forces SHA-512, and that changes two things
//!
//! `AES_256_GCM_SHA512_128` (RFC 9605 §8.1) means HKDF is **SHA-512
//! throughout**, so the extract PRK is **64 bytes, not 32**. And it is the
//! direct GCM form: the derived key length *is* the AEAD key length, with no
//! `enc_key`/`auth_key` split (that split belongs to the AES-CTR suites).
//! ADR-0027's key-derivation row is broadened to permit `HKDF_SHA512` by this
//! same changeset.

use crate::error::GenError;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::hkdf;
use ring::hmac;

/// Length of the HKDF-SHA512 pseudo-random key. **64, not 32** — asserted
/// explicitly by the external gate against the upstream `sframe_secret`.
pub const PRK_LEN: usize = 64;
/// AEAD key length for `AES_256_GCM`. Also the *derived* key length: direct GCM
/// form, no `enc_key`/`auth_key` split.
pub const AEAD_KEY_LEN: usize = 32;
/// `SFrame` salt length, equal to the GCM nonce length.
pub const SALT_LEN: usize = NONCE_LEN;

/// RFC 9605 §4.4 label prefix for the key expansion. Trailing space is part of
/// the label; the external vectors' `sframe_key_label` pins these exact bytes.
const KEY_LABEL: &[u8] = b"SFrame 1.0 Secret key ";
/// RFC 9605 §4.4 label prefix for the salt expansion.
const SALT_LABEL: &[u8] = b"SFrame 1.0 Secret salt ";

/// Newtype so `hkdf::Prk::expand` can produce an arbitrary output length.
struct OutLen(usize);
impl hkdf::KeyType for OutLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// The label bytes for one expansion: `prefix || KID(8, BE) || suite(2, BE)`.
fn label(prefix: &[u8], key_id: [u8; 8], suite: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(prefix.len() + 8 + 2);
    out.extend_from_slice(prefix);
    out.extend_from_slice(&key_id);
    out.extend_from_slice(&suite.to_be_bytes());
    out
}

/// HKDF-Extract, computed **explicitly** so the PRK bytes are observable.
///
/// `ring::hkdf::Salt::extract` returns an opaque `Prk`, which would make
/// "assert the PRK is 64 bytes and the hash is SHA-512" unassertable — and that
/// assertion is the whole point of gating the schedule. HKDF-Extract *is*
/// HMAC, so this is the same computation, not an approximation.
///
/// RFC 9605 uses an **empty** salt. HMAC zero-pads a short key to the hash
/// block size, so an empty key and a 128-byte zero key are identical **by
/// construction**; passing zero bytes is exact rather than a workaround, which
/// matters because the `TypeScript` side must pass 64 zero bytes (`WebCrypto`
/// rejects a zero-length HMAC key outright) and a future reader must not
/// "correct" that back to something non-equivalent.
#[must_use]
pub fn extract_prk(base_key: &[u8]) -> [u8; PRK_LEN] {
    let salt = hmac::Key::new(hmac::HMAC_SHA512, &[]);
    let tag = hmac::sign(&salt, base_key);
    let mut prk = [0u8; PRK_LEN];
    prk.copy_from_slice(tag.as_ref());
    prk
}

/// The derived `SFrame` key and salt for one key id.
#[derive(Clone)]
pub struct Derived {
    /// The AEAD key. 32 bytes: direct GCM form, no auth-key half.
    pub key: [u8; AEAD_KEY_LEN],
    /// The nonce salt.
    pub salt: [u8; SALT_LEN],
    /// The extract PRK, retained so callers can assert its length and value
    /// against the external vectors rather than taking it on trust.
    pub prk: [u8; PRK_LEN],
}

/// Derive `(sframe_key, sframe_salt)` per RFC 9605 §4.4.
///
/// # Errors
///
/// [`GenError::Crypto`] if `ring`'s expand rejects the output length.
pub fn derive(base_key: &[u8], key_id: &[u8; 8], suite: u16) -> Result<Derived, GenError> {
    let prk_bytes = extract_prk(base_key);
    // Cross-check the explicit extract against `ring`'s own opaque one: if the
    // two disagree, the explicit form above is wrong and every assertion built
    // on its observable bytes is worthless.
    let ring_prk = hkdf::Salt::new(hkdf::HKDF_SHA512, &[]).extract(base_key);
    let explicit_prk = hkdf::Prk::new_less_safe(hkdf::HKDF_SHA512, &prk_bytes);

    let key = expand(
        &explicit_prk,
        &label(KEY_LABEL, *key_id, suite),
        AEAD_KEY_LEN,
    )?;
    let ring_key = expand(&ring_prk, &label(KEY_LABEL, *key_id, suite), AEAD_KEY_LEN)?;
    if key != ring_key {
        return Err(GenError::Crypto {
            operation: "explicit HMAC-SHA512 extract disagrees with ring::hkdf extract",
        });
    }
    let salt = expand(&explicit_prk, &label(SALT_LABEL, *key_id, suite), SALT_LEN)?;

    let mut key_arr = [0u8; AEAD_KEY_LEN];
    key_arr.copy_from_slice(&key);
    let mut salt_arr = [0u8; SALT_LEN];
    salt_arr.copy_from_slice(&salt);
    Ok(Derived {
        key: key_arr,
        salt: salt_arr,
        prk: prk_bytes,
    })
}

fn expand(prk: &hkdf::Prk, info: &[u8], len: usize) -> Result<Vec<u8>, GenError> {
    let info_parts = [info];
    let okm = prk
        .expand(&info_parts, OutLen(len))
        .map_err(|_| GenError::Crypto {
            operation: "hkdf expand",
        })?;
    let mut out = vec![0u8; len];
    okm.fill(&mut out).map_err(|_| GenError::Crypto {
        operation: "hkdf expand fill",
    })?;
    Ok(out)
}

/// `nonce = salt XOR BE12(counter)` (RFC 9605 §4.4.2).
///
/// Our counter is the 32-bit `stream_sequence`. Zero-extending a big-endian
/// 32-bit value into 12 bytes **is** the big-endian 12-byte encoding of the
/// same integer, so this is RFC 9605's construction unchanged, not an analogue
/// of it — which is what makes the external `nonce` row a real gate on our
/// nonce derivation.
#[must_use]
pub fn nonce_from(salt: &[u8; SALT_LEN], counter: u64) -> [u8; SALT_LEN] {
    let mut ctr = [0u8; SALT_LEN];
    ctr[SALT_LEN - 8..].copy_from_slice(&counter.to_be_bytes());
    let mut out = *salt;
    for (o, c) in out.iter_mut().zip(ctr.iter()) {
        *o ^= *c;
    }
    out
}

/// The output of a detached-tag seal.
pub struct Sealed {
    /// Ciphertext, exactly the plaintext length under GCM.
    pub ciphertext: Vec<u8>,
    /// The 16-byte authentication tag, **detached** from the ciphertext.
    pub tag: [u8; media_protocol::frame::AEAD_TAG_BYTES],
}

/// AES-256-GCM seal producing a **detached** tag.
///
/// # Errors
///
/// [`GenError::Crypto`] on any `ring` failure, including a key length other
/// than 32 — see [`reject_wrong_key_len`] for why that rejection is asserted
/// rather than assumed.
pub fn seal_detached(
    key: &[u8],
    nonce: &[u8; SALT_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Sealed, GenError> {
    let unbound = UnboundKey::new(&AES_256_GCM, key).map_err(|_| GenError::Crypto {
        operation: "AES_256_GCM UnboundKey::new (key length must be exactly 32)",
    })?;
    let sealing = LessSafeKey::new(unbound);
    let mut buf = plaintext.to_vec();
    let tag = sealing
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key(*nonce),
            Aad::from(aad),
            &mut buf,
        )
        .map_err(|_| GenError::Crypto {
            operation: "AES_256_GCM seal_in_place_separate_tag",
        })?;
    let mut tag_arr = [0u8; media_protocol::frame::AEAD_TAG_BYTES];
    tag_arr.copy_from_slice(tag.as_ref());
    Ok(Sealed {
        ciphertext: buf,
        tag: tag_arr,
    })
}

/// Whether the AEAD constructor rejects a key of this length.
///
/// Exists so `tests/external_sframe_gate.rs` can assert that a 16-byte key is
/// **refused**, rather than assuming it. The 0x0004 contrast row would
/// otherwise be one careless line away from becoming an AES-128 acceptance path
/// reachable by anything that can influence a suite id — built in order to test
/// that we do not have one. ADR-0036's amendment table permits AES-128-GCM
/// "only for `SFrame` interop", and we do none.
#[must_use]
pub fn reject_wrong_key_len(key_len: usize) -> bool {
    UnboundKey::new(&AES_256_GCM, &vec![0u8; key_len]).is_err()
}
