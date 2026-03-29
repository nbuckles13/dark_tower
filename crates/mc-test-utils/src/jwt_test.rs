//! Reusable JWT test utilities for Ed25519 token signing and JWKS mocking.
//!
//! Provides:
//! - `TestKeypair`: Ed25519 keypair for signing JWTs in tests
//! - `setup_jwks_mock`: wiremock-based JWKS endpoint
//! - `make_meeting_claims`: Helper to create valid MeetingTokenClaims

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use common::jwt::{MeetingRole, MeetingTokenClaims, ParticipantType};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::Serialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Ed25519 test keypair for signing JWTs.
pub struct TestKeypair {
    /// Key ID (kid) used in JWT header and JWKS.
    pub kid: String,
    /// Raw Ed25519 public key bytes.
    pub public_key_bytes: Vec<u8>,
    /// PKCS#8-encoded private key for `jsonwebtoken`.
    pub private_key_pkcs8: Vec<u8>,
}

impl TestKeypair {
    /// Create a deterministic Ed25519 keypair from a seed byte and key ID.
    pub fn new(seed: u8, kid: &str) -> Self {
        let mut seed_bytes = [0u8; 32];
        seed_bytes[0] = seed;
        for (i, byte) in seed_bytes.iter_mut().enumerate().skip(1) {
            *byte = seed.wrapping_mul(i as u8).wrapping_add(i as u8);
        }

        let key_pair = Ed25519KeyPair::from_seed_unchecked(&seed_bytes)
            .expect("Failed to create test keypair");

        let public_key_bytes = key_pair.public_key().as_ref().to_vec();
        let private_key_pkcs8 = build_pkcs8_from_seed(&seed_bytes);

        Self {
            kid: kid.to_string(),
            public_key_bytes,
            private_key_pkcs8,
        }
    }

    /// Sign arbitrary claims as a JWT with EdDSA (Ed25519).
    pub fn sign_token<T: Serialize>(&self, claims: &T) -> String {
        let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
        let mut header = Header::new(Algorithm::EdDSA);
        header.typ = Some("JWT".to_string());
        header.kid = Some(self.kid.clone());

        encode(&header, claims, &encoding_key).expect("Failed to sign token")
    }

    /// Produce the JWK JSON representation for JWKS responses.
    pub fn jwk_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kty": "OKP",
            "kid": self.kid,
            "crv": "Ed25519",
            "x": URL_SAFE_NO_PAD.encode(&self.public_key_bytes),
            "alg": "EdDSA",
            "use": "sig"
        })
    }
}

/// Build a PKCS#8 v1 document from a raw Ed25519 32-byte seed.
///
/// This produces the DER encoding expected by `jsonwebtoken::EncodingKey::from_ed_der`.
pub(crate) fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
    let mut pkcs8 = Vec::new();
    // SEQUENCE (outer)
    pkcs8.push(0x30);
    pkcs8.push(0x2e);
    // INTEGER version = 0
    pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);
    // SEQUENCE (AlgorithmIdentifier)
    pkcs8.push(0x30);
    pkcs8.push(0x05);
    // OID 1.3.101.112 (Ed25519)
    pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);
    // OCTET STRING (privateKey wrapper)
    pkcs8.push(0x04);
    pkcs8.push(0x22);
    // OCTET STRING (actual seed)
    pkcs8.push(0x04);
    pkcs8.push(0x20);
    pkcs8.extend_from_slice(seed);
    pkcs8
}

/// Mount a JWKS mock on the given `MockServer` with the provided keypair.
///
/// Returns the full JWKS URL (e.g., `http://127.0.0.1:PORT/.well-known/jwks.json`).
pub async fn mount_jwks_mock(mock_server: &MockServer, keypair: &TestKeypair) -> String {
    let jwks_response = serde_json::json!({
        "keys": [keypair.jwk_json()]
    });

    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
        .mount(mock_server)
        .await;

    format!("{}/.well-known/jwks.json", mock_server.uri())
}

/// Create a valid `MeetingTokenClaims` for the given meeting_id.
///
/// Defaults: 1-hour expiry, participant role, member type.
pub fn make_meeting_claims(meeting_id: &str) -> MeetingTokenClaims {
    let now = Utc::now().timestamp();
    MeetingTokenClaims {
        sub: "user-001".to_string(),
        token_type: "meeting".to_string(),
        meeting_id: meeting_id.to_string(),
        home_org_id: None,
        meeting_org_id: "org-456".to_string(),
        participant_type: ParticipantType::Member,
        role: MeetingRole::Participant,
        capabilities: vec!["video".to_string(), "audio".to_string()],
        iat: now,
        exp: now + 3600,
        jti: format!("jti-{}", uuid::Uuid::new_v4()),
    }
}

/// Create an expired `MeetingTokenClaims` for the given meeting_id.
pub fn make_expired_meeting_claims(meeting_id: &str) -> MeetingTokenClaims {
    let past = Utc::now().timestamp() - 7200;
    MeetingTokenClaims {
        sub: "user-001".to_string(),
        token_type: "meeting".to_string(),
        meeting_id: meeting_id.to_string(),
        home_org_id: None,
        meeting_org_id: "org-456".to_string(),
        participant_type: ParticipantType::Member,
        role: MeetingRole::Participant,
        capabilities: vec!["video".to_string(), "audio".to_string()],
        iat: past,
        exp: past + 3600, // expired 1 hour ago
        jti: format!("jti-{}", uuid::Uuid::new_v4()),
    }
}

/// Create a host `MeetingTokenClaims` for the given meeting_id.
pub fn make_host_meeting_claims(meeting_id: &str) -> MeetingTokenClaims {
    let mut claims = make_meeting_claims(meeting_id);
    claims.role = MeetingRole::Host;
    claims
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_creation() {
        let kp = TestKeypair::new(42, "test-key");
        assert_eq!(kp.kid, "test-key");
        assert_eq!(kp.public_key_bytes.len(), 32);
        assert!(!kp.private_key_pkcs8.is_empty());
    }

    #[test]
    fn test_sign_token() {
        let kp = TestKeypair::new(42, "test-key");
        let claims = make_meeting_claims("meeting-123");
        let token = kp.sign_token(&claims);

        // JWT has 3 dot-separated parts
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn test_jwk_json_format() {
        let kp = TestKeypair::new(42, "test-key");
        let jwk = kp.jwk_json();

        assert_eq!(jwk["kty"], "OKP");
        assert_eq!(jwk["kid"], "test-key");
        assert_eq!(jwk["crv"], "Ed25519");
        assert_eq!(jwk["alg"], "EdDSA");
    }

    #[test]
    fn test_make_meeting_claims_defaults() {
        let claims = make_meeting_claims("m-1");
        assert_eq!(claims.meeting_id, "m-1");
        assert_eq!(claims.token_type, "meeting");
        assert!(claims.exp > Utc::now().timestamp());
    }

    #[test]
    fn test_make_expired_claims() {
        let claims = make_expired_meeting_claims("m-1");
        assert!(claims.exp < Utc::now().timestamp());
    }
}
