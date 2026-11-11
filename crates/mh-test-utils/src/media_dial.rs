//! The ONE `media_dial` test forge for MH (story 4 MH-1; ADR-0039 §5).
//!
//! MC mints with its per-meeting Ed25519 key (`mc-service` `media_dial::key`).
//! MH's tests need the same compact JWS plus inputs a real minter never emits:
//! an arbitrary header (`alg` none/HS256, `kid`/`jwk`, an attacker's `jwk`), an
//! attacker key, and size-exact padding. Well-formed claims are serialized from
//! `common::jwt::MediaDialClaims` — the shared shape — and only malformed shapes
//! are raw JSON built from that.
//!
//! Test-only key material: the seeds are fixed constants, never production
//! keys.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use common::jwt::{MediaDialClaims, MEDIA_DIAL_TOKEN_TYPE};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};

/// Seed of "the meeting's" key in tests.
pub const MEETING_SEED: [u8; 32] = [0x11; 32];
/// Seed of a second meeting's (or an attacker's) key.
pub const ATTACKER_SEED: [u8; 32] = [0x22; 32];

/// A seeded Ed25519 signer producing compact JWS strings.
pub struct DialForge {
    key: Ed25519KeyPair,
}

impl DialForge {
    #[must_use]
    pub fn new(seed: [u8; 32]) -> Self {
        let key = Ed25519KeyPair::from_seed_unchecked(&seed).expect("valid Ed25519 seed");
        Self { key }
    }

    /// The raw 32-byte public key, as `RegisterMeetingRequest.media_dial_public_key`.
    #[must_use]
    pub fn public_key(&self) -> Vec<u8> {
        self.key.public_key().as_ref().to_vec()
    }

    /// This key as a well-formed OKP JWK (for "an attacker announces its key in
    /// the header").
    #[must_use]
    pub fn jwk(&self) -> Value {
        json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(self.key.public_key().as_ref()),
        })
    }

    /// Well-formed claims via the shared `MediaDialClaims` shape.
    #[must_use]
    pub fn claims_json(meeting_id: &str, aud: &str, sub: &str, iat: i64, ttl_secs: i64) -> Value {
        serde_json::to_value(MediaDialClaims {
            sub: sub.to_string(),
            token_type: MEDIA_DIAL_TOKEN_TYPE.to_string(),
            meeting_id: meeting_id.to_string(),
            aud: aud.to_string(),
            iat,
            exp: iat + ttl_secs,
        })
        .expect("claims serialize")
    }

    /// Sign with MC's header shape: `{"alg":"EdDSA","typ":"JWT"}`.
    #[must_use]
    pub fn sign(&self, claims: &Value) -> String {
        self.sign_with_header(&json!({"alg": "EdDSA", "typ": "JWT"}), claims)
    }

    /// Sign `claims` under an arbitrary `header` (whatever `alg` it names, the
    /// signature is always this key's Ed25519 signature).
    #[must_use]
    pub fn sign_with_header(&self, header: &Value, claims: &Value) -> String {
        let mut token = URL_SAFE_NO_PAD.encode(header.to_string());
        token.push('.');
        token.push_str(&URL_SAFE_NO_PAD.encode(claims.to_string()));
        let sig = self.key.sign(token.as_bytes());
        token.push('.');
        token.push_str(&URL_SAFE_NO_PAD.encode(sig.as_ref()));
        token
    }

    /// A validly signed token of EXACTLY `len` bytes, padded through extra
    /// (ignored) header and claim members. For the size-cap boundary.
    ///
    /// # Panics
    ///
    /// If `len` is shorter than the unpadded token.
    #[must_use]
    pub fn sign_padded_to(&self, claims: &Value, len: usize) -> String {
        for claim_pad in 0..len {
            for header_pad in 0..4 {
                let mut header = json!({"alg": "EdDSA", "typ": "JWT"});
                header["pad"] = Value::String("h".repeat(header_pad));
                let mut padded = claims.clone();
                padded["pad"] = Value::String("p".repeat(claim_pad));
                let token = self.sign_with_header(&header, &padded);
                if token.len() == len {
                    return token;
                }
                if token.len() > len + 8 {
                    panic!("cannot pad a media_dial token to exactly {len} bytes");
                }
            }
        }
        panic!("cannot pad a media_dial token to exactly {len} bytes");
    }
}

/// Tamper one signature byte (the result keeps its shape, fails the check).
#[must_use]
pub fn tamper_signature(token: &str) -> String {
    let mut bytes = token.as_bytes().to_vec();
    let last = bytes.len() - 2;
    bytes[last] = if bytes[last] == b'A' { b'B' } else { b'A' };
    String::from_utf8(bytes).expect("ascii")
}

/// The PUBLIC key every test registration carries by default
/// (`RegisterMeetingRequest.media_dial_public_key`): [`MEETING_SEED`]'s, so a
/// token from `DialForge::new(MEETING_SEED)` verifies against it.
#[must_use]
pub fn meeting_public_key() -> Vec<u8> {
    DialForge::new(MEETING_SEED).public_key()
}

/// [`meeting_public_key`] as the registered-key type, for fixtures that build a
/// `MeetingRegistration` directly.
#[must_use]
pub fn meeting_dial_key() -> mh_service::auth::MediaDialKey {
    mh_service::auth::MediaDialKey::from_wire(&meeting_public_key()).expect("32-byte key")
}
