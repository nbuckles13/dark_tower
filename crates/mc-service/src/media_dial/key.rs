//! The per-meeting `media_dial` signing key and the token mint (ADR-0039 §5).
//!
//! # Custody (`scripts/guards/semantic/checks.md` credential-leak items 11-13)
//!
//! - ONE Ed25519 keypair per meeting, drawn from the system CSPRNG when the
//!   meeting actor is created, held only in that actor's memory as its own
//!   field, beside and SEPARATE from the KEK (never derived from it, never
//!   touched by any KEK path). Never persisted, logged, traced, serialized or
//!   Debug-printed; dies with the meeting; never rotated; never in any JWKS.
//! - Only the 32-byte PUBLIC half leaves this type ([`DialSigningKey::public_key`]),
//!   onto `RegisterMeetingRequest.media_dial_public_key`.
//! - [`DialSigningKey`] has no `Clone`, `Serialize`, `Display` or raw-bytes
//!   accessor, and a hand-written `Debug` printing a constant. The PKCS#8
//!   document `ring` generates through is dropped inside
//!   [`DialSigningKey::generate`]; it is never stored or handed to another
//!   encoder (a jsonwebtoken `EncodingKey` would be a second, non-redacting
//!   copy of the private half — which is why the JWS is signed here by hand).
//!
//! # Token
//!
//! A compact JWS: `b64url(header).b64url(claims).b64url(signature)`, with
//! the header the CONSTANT bytes `{"alg":"EdDSA","typ":"JWT"}` — so a `kid`
//! cannot appear by construction (a kid-less token fails every AC-JWKS
//! verifier with `MissingKid`: the cross-meeting KEK guard by construction).
//! Claims are `common::jwt::MediaDialClaims`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use common::jwt::{MediaDialClaims, MEDIA_DIAL_TOKEN_TYPE};
use ring::rand::SecureRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

/// The JOSE header of every `media_dial` token: EdDSA, JWT, and nothing else.
pub const MEDIA_DIAL_JWS_HEADER: &[u8] = br#"{"alg":"EdDSA","typ":"JWT"}"#;

/// The system CSPRNG could not produce the meeting's dial key. Carries
/// nothing: the failure is that there IS no key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialKeyGenerationFailed;

/// Minting a token failed. No payload, fixed text: neither a claim, a token
/// nor key material can reach an error or a log through this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MintError {
    /// `iat + TTL` overflowed.
    ExpOverflow,
    /// The claims could not be encoded.
    Encode,
    /// The participant is not on the roster, or no join has installed the
    /// meeting's `media_dial` timing yet: nothing is minted.
    Unavailable,
}

impl std::fmt::Display for MintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ExpOverflow => "media_dial expiry overflowed",
            Self::Encode => "media_dial claims could not be encoded",
            Self::Unavailable => "media_dial minting is unavailable for this participant",
        })
    }
}

impl std::error::Error for MintError {}

/// The meeting's private `media_dial` signing key (see the module doc).
pub struct DialSigningKey(Ed25519KeyPair);

impl std::fmt::Debug for DialSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A constant: no byte, length or fingerprint of either half.
        f.write_str("DialSigningKey([REDACTED])")
    }
}

impl DialSigningKey {
    /// Draw a fresh keypair from `rng` (the system CSPRNG).
    ///
    /// # Errors
    ///
    /// [`DialKeyGenerationFailed`] if the CSPRNG fails. Fail closed: there is
    /// no fallback and no deterministic key.
    pub fn generate(rng: &dyn SecureRandom) -> Result<Self, DialKeyGenerationFailed> {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(rng).map_err(|_| DialKeyGenerationFailed)?;
        let pair =
            Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| DialKeyGenerationFailed)?;
        // `pkcs8` drops here; the keypair is the only holder of the private half.
        Ok(Self(pair))
    }

    /// A key from a fixed seed. **Test builds only** — a deterministic key in
    /// production would defeat the credential.
    #[cfg(any(test, feature = "test-seams"))]
    #[must_use]
    #[expect(clippy::missing_panics_doc, reason = "test-only constructor")]
    #[expect(
        clippy::expect_used,
        reason = "from_seed_unchecked only fails on a non-32-byte seed; the type is [u8; 32]"
    )]
    pub fn from_seed_for_test(seed: &[u8; 32]) -> Self {
        Self(Ed25519KeyPair::from_seed_unchecked(seed).expect("32-byte seed"))
    }

    /// The 32-byte PUBLIC verification key — the only bytes that leave this
    /// type (`RegisterMeetingRequest.media_dial_public_key`).
    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(self.0.public_key().as_ref());
        out
    }

    /// Sign a claim set as a compact JWS with the constant header.
    ///
    /// # Errors
    ///
    /// [`MintError::Encode`] if the claims cannot be serialized.
    pub fn sign_media_dial(&self, claims: &MediaDialClaims) -> Result<MediaDialToken, MintError> {
        let claims_json = serde_json::to_vec(claims).map_err(|_| MintError::Encode)?;
        let mut token = URL_SAFE_NO_PAD.encode(MEDIA_DIAL_JWS_HEADER);
        token.push('.');
        token.push_str(&URL_SAFE_NO_PAD.encode(claims_json));
        let signature = self.0.sign(token.as_bytes());
        token.push('.');
        token.push_str(&URL_SAFE_NO_PAD.encode(signature.as_ref()));
        Ok(MediaDialToken(token))
    }

    /// Mint the token for one (participant, handler): `sub` is the
    /// participant's validated meeting-token `sub` from MC's roster (never a
    /// request field), `aud` the handler's `handler_id`.
    ///
    /// # Errors
    ///
    /// [`MintError::ExpOverflow`] if `iat + ttl` overflows; [`MintError::Encode`].
    pub fn mint(
        &self,
        sub: &str,
        meeting_id: &str,
        aud: &str,
        iat: i64,
        ttl_seconds: u32,
    ) -> Result<MediaDialToken, MintError> {
        let exp = iat
            .checked_add(i64::from(ttl_seconds))
            .ok_or(MintError::ExpOverflow)?;
        self.sign_media_dial(&MediaDialClaims {
            sub: sub.to_string(),
            token_type: MEDIA_DIAL_TOKEN_TYPE.to_string(),
            meeting_id: meeting_id.to_string(),
            aud: aud.to_string(),
            iat,
            exp,
        })
    }
}

/// A minted `media_dial` token. Redacting `Debug`; the string leaves only by
/// [`Self::into_wire`], into `MediaServerInfo.media_dial_token`.
pub struct MediaDialToken(String);

impl std::fmt::Debug for MediaDialToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MediaDialToken([REDACTED])")
    }
}

impl MediaDialToken {
    /// The compact JWS, for the one wire field that carries it.
    #[must_use]
    pub fn into_wire(self) -> String {
        self.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use ring::signature::{UnparsedPublicKey, ED25519};

    const SEED: [u8; 32] = [7u8; 32];

    fn split(token: &str) -> (Vec<u8>, Vec<u8>, Vec<u8>, String) {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "compact JWS has three segments");
        (
            URL_SAFE_NO_PAD.decode(parts[0]).unwrap(),
            URL_SAFE_NO_PAD.decode(parts[1]).unwrap(),
            URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
            format!("{}.{}", parts[0], parts[1]),
        )
    }

    #[test]
    fn minted_token_has_the_exact_header_claims_and_a_verifying_signature() {
        let key = DialSigningKey::from_seed_for_test(&SEED);
        let token = key
            .mint("user-a", "meeting-1", "mh-1", 1_000, 120)
            .unwrap()
            .into_wire();
        let (header, claims, signature, input) = split(&token);

        // Header: exactly two keys, no kid.
        let header: serde_json::Value = serde_json::from_slice(&header).unwrap();
        assert_eq!(header, serde_json::json!({"alg": "EdDSA", "typ": "JWT"}));

        // Claims: exactly the ADR-0039 §5 names, iat/exp integers.
        let claims: serde_json::Value = serde_json::from_slice(&claims).unwrap();
        let mut keys: Vec<&str> = claims
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["aud", "exp", "iat", "meeting_id", "sub", "token_type"]
        );
        assert_eq!(claims["sub"], "user-a");
        assert_eq!(claims["token_type"], "media_dial");
        assert_eq!(claims["meeting_id"], "meeting-1");
        assert_eq!(claims["aud"], "mh-1");
        assert_eq!(claims["iat"].as_i64(), Some(1_000));
        assert_eq!(claims["exp"].as_i64(), Some(1_120));

        // Independent oracle: ring verification against the public bytes.
        let public = key.public_key();
        UnparsedPublicKey::new(&ED25519, public)
            .verify(input.as_bytes(), &signature)
            .expect("verifies under the meeting's public key");
        // Negative arm: another key's public half does not verify it.
        let other = DialSigningKey::from_seed_for_test(&[8u8; 32]);
        assert!(UnparsedPublicKey::new(&ED25519, other.public_key())
            .verify(input.as_bytes(), &signature)
            .is_err());
    }

    /// Interop with the consumer side: the library MH-1's verifier uses
    /// decodes the hand-signed token (EdDSA pinned, aud = the handler).
    #[test]
    fn jsonwebtoken_decodes_the_minted_token_with_the_handler_as_audience() {
        use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
        let key = DialSigningKey::from_seed_for_test(&SEED);
        let iat = chrono::Utc::now().timestamp();
        let token = key
            .mint("user-a", "meeting-1", "mh-1", iat, 300)
            .unwrap()
            .into_wire();
        let decoding = DecodingKey::from_ed_der(&key.public_key());
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&["mh-1"]);
        let data = decode::<MediaDialClaims>(&token, &decoding, &validation).unwrap();
        assert_eq!(data.claims.aud, "mh-1");
        assert!(data.claims.validate().is_ok());
        assert!(data.header.kid.is_none());

        let mut wrong_aud = Validation::new(Algorithm::EdDSA);
        wrong_aud.set_audience(&["mh-2"]);
        assert!(decode::<MediaDialClaims>(&token, &decoding, &wrong_aud).is_err());
    }

    #[test]
    fn expiry_overflow_fails_closed() {
        let key = DialSigningKey::from_seed_for_test(&SEED);
        assert_eq!(
            key.mint("u", "m", "h", i64::MAX - 10, 60).unwrap_err(),
            MintError::ExpOverflow
        );
    }

    /// The wire key is the PUBLIC half — distinguishing inputs: not the seed,
    /// and not inside the PKCS#8 document of a generated key.
    #[test]
    fn the_public_key_is_neither_the_seed_nor_private_material() {
        let key = DialSigningKey::from_seed_for_test(&SEED);
        assert_ne!(key.public_key(), SEED);

        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        assert_ne!(pkcs8.as_ref(), public.as_slice());
        // The PKCS#8 v2 document embeds the public key AND the seed; the seed
        // (the 32 bytes after the 16-byte prefix) is not the public key.
        assert_ne!(&pkcs8.as_ref()[16..48], public.as_slice());

        // Two generated keys differ (per meeting, never shared).
        let a = DialSigningKey::generate(&rng).unwrap();
        let b = DialSigningKey::generate(&rng).unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn debug_never_renders_key_or_token_material() {
        let key = DialSigningKey::from_seed_for_test(&SEED);
        let rendered = format!("{key:?}");
        assert_eq!(rendered, "DialSigningKey([REDACTED])");
        for needle in [
            hex::encode(SEED),
            hex::encode(key.public_key()),
            URL_SAFE_NO_PAD.encode(SEED),
        ] {
            assert!(!rendered.contains(&needle));
        }

        let token = key.mint("user-a", "m", "h", 1, 60).unwrap();
        let rendered = format!("{token:?}");
        let wire = token.into_wire();
        assert!(!wire.is_empty());
        assert!(!rendered.contains(&wire));
        assert!(!rendered.contains(wire.split('.').nth(2).unwrap()));
    }
}
