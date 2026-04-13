//! JWT validation for Media Handler.
//!
//! Thin wrapper around `common::jwt::JwtValidator` that provides MH-specific
//! `validate_meeting_token()` with automatic `JwtError` -> `MhError` mapping.
//!
//! # Token Types
//!
//! - **Meeting tokens**: Authenticated participant tokens with `token_type == "meeting"`
//!
//! MH only accepts meeting tokens. Guest tokens are rejected at both the
//! deserialization level (incompatible field types) and the explicit
//! `token_type` check.
//!
//! # Security
//!
//! - Token type is checked after signature verification to prevent token confusion
//! - Error messages are generic to prevent information leakage

use crate::errors::MhError;
use common::jwt::{JwksClient, MeetingTokenClaims};
use std::sync::Arc;
use tracing::instrument;

/// Re-export the common `JwtValidator` for direct generic usage.
pub use common::jwt::JwtValidator as CommonJwtValidator;

/// MH JWT validator wrapping the common `JwtValidator`.
///
/// Provides typed methods that return `MhError` and enforce
/// token-type constraints after signature verification.
pub struct MhJwtValidator {
    inner: CommonJwtValidator,
}

impl MhJwtValidator {
    /// Create a new MH JWT validator.
    ///
    /// # Arguments
    ///
    /// * `jwks_client` - Client for fetching public keys from AC's JWKS endpoint
    /// * `clock_skew_seconds` - Clock skew tolerance for iat validation
    #[must_use]
    pub fn new(jwks_client: Arc<JwksClient>, clock_skew_seconds: i64) -> Self {
        Self {
            inner: CommonJwtValidator::new(jwks_client, clock_skew_seconds),
        }
    }

    /// Validate a meeting JWT and return the claims.
    ///
    /// # Security Checks
    ///
    /// 1. Common checks: size, kid extraction, JWKS lookup, `EdDSA` signature, exp, iat
    /// 2. Token type: `token_type` must be `"meeting"` (prevents token confusion)
    ///
    /// # Errors
    ///
    /// Returns `MhError::JwtValidation` for all validation failures with a generic
    /// message to prevent information leakage.
    #[instrument(skip_all)]
    pub async fn validate_meeting_token(&self, token: &str) -> Result<MeetingTokenClaims, MhError> {
        let claims = self.inner.validate::<MeetingTokenClaims>(token).await?;

        if claims.token_type != "meeting" {
            tracing::warn!(
                target: "mh.auth",
                token_type = %claims.token_type,
                "Token type mismatch: expected meeting token"
            );
            return Err(MhError::JwtValidation(
                "The access token is invalid or expired".to_string(),
            ));
        }

        Ok(claims)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use chrono::Utc;
    use common::jwt::JwksClient;
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde::Serialize;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Test keypair for signing tokens.
    // NOTE: Duplicated from mc-service tests. Extraction to common-test-utils
    // is a future cleanup opportunity to avoid cross-service dev-dependency.
    struct TestKeypair {
        kid: String,
        public_key_bytes: Vec<u8>,
        private_key_pkcs8: Vec<u8>,
    }

    impl TestKeypair {
        fn new(seed: u8, kid: &str) -> Self {
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

        fn sign_token<T: Serialize>(&self, claims: &T) -> String {
            let encoding_key = EncodingKey::from_ed_der(&self.private_key_pkcs8);
            let mut header = Header::new(Algorithm::EdDSA);
            header.typ = Some("JWT".to_string());
            header.kid = Some(self.kid.clone());

            encode(&header, claims, &encoding_key).expect("Failed to sign token")
        }

        fn jwk_json(&self) -> serde_json::Value {
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

    /// Build PKCS#8 v1 document from Ed25519 seed.
    fn build_pkcs8_from_seed(seed: &[u8; 32]) -> Vec<u8> {
        let mut pkcs8 = Vec::new();
        pkcs8.push(0x30);
        pkcs8.push(0x2e);
        pkcs8.extend_from_slice(&[0x02, 0x01, 0x00]);
        pkcs8.push(0x30);
        pkcs8.push(0x05);
        pkcs8.extend_from_slice(&[0x06, 0x03, 0x2b, 0x65, 0x70]);
        pkcs8.push(0x04);
        pkcs8.push(0x22);
        pkcs8.push(0x04);
        pkcs8.push(0x20);
        pkcs8.extend_from_slice(seed);
        pkcs8
    }

    /// Set up wiremock JWKS server and return (`mock_server`, keypair, validator).
    async fn setup_test_validator() -> (MockServer, TestKeypair, MhJwtValidator) {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "mh-test-key-01");

        let jwks_response = serde_json::json!({
            "keys": [keypair.jwk_json()]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_url = format!("{}/.well-known/jwks.json", mock_server.uri());
        let jwks_client =
            Arc::new(JwksClient::new(jwks_url).expect("Failed to create JWKS client"));
        let validator = MhJwtValidator::new(jwks_client, 300);

        (mock_server, keypair, validator)
    }

    fn make_meeting_claims() -> MeetingTokenClaims {
        let now = Utc::now().timestamp();
        MeetingTokenClaims {
            sub: "user-001".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-123".to_string(),
            home_org_id: None,
            meeting_org_id: "org-456".to_string(),
            participant_type: common::jwt::ParticipantType::Member,
            role: common::jwt::MeetingRole::Participant,
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: now,
            exp: now + 3600,
            jti: "jti-001".to_string(),
        }
    }

    #[test]
    fn test_mh_jwt_validator_creation() {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost:8082/.well-known/jwks.json".to_string())
                .expect("Failed to create JWKS client"),
        );
        let _validator = MhJwtValidator::new(jwks_client, 300);
    }

    #[tokio::test]
    async fn test_validate_meeting_token_success() {
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let claims = make_meeting_claims();
        let token = keypair.sign_token(&claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let validated = result.unwrap();
        assert_eq!(validated.meeting_id, "meeting-123");
        assert_eq!(validated.token_type, "meeting");
        assert_eq!(validated.meeting_org_id, "org-456");
        assert_eq!(validated.sub, "user-001");
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_wrong_token_type() {
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let mut claims = make_meeting_claims();
        claims.token_type = "guest".to_string();

        let token = keypair.sign_token(&claims);
        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(msg)) if msg.contains("invalid or expired")),
            "Expected JwtValidation error for wrong token_type, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_guest_token() {
        // Token confusion: a guest token should be rejected when used as meeting token.
        // GuestTokenClaims has string participant_type vs enum — deserialization rejects it.
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let now = Utc::now().timestamp();
        let guest_claims = common::jwt::GuestTokenClaims {
            sub: "guest-001".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-123".to_string(),
            meeting_org_id: "org-456".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Test Guest".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string()],
            iat: now,
            exp: now + 3600,
            jti: "jti-guest-001".to_string(),
        };
        let token = keypair.sign_token(&guest_claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(_))),
            "Expected JwtValidation error for guest token, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_invalid_token() {
        let (_mock_server, _keypair, validator) = setup_test_validator().await;

        let result = validator.validate_meeting_token("not-a-valid-jwt").await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(_))),
            "Expected JwtValidation error, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_wrong_signature() {
        let (_mock_server, _keypair, validator) = setup_test_validator().await;

        let wrong_keypair = TestKeypair::new(99, "wrong-key");
        let claims = make_meeting_claims();
        let token = wrong_keypair.sign_token(&claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(_))),
            "Expected JwtValidation error for wrong key, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_expired() {
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let mut claims = make_meeting_claims();
        claims.exp = Utc::now().timestamp() - 3600; // Expired 1 hour ago

        let token = keypair.sign_token(&claims);
        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(_))),
            "Expected JwtValidation error for expired token, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_oversized() {
        let (_mock_server, _keypair, validator) = setup_test_validator().await;

        // Create a token string > 8KB
        let oversized = "a".repeat(8193);
        let result = validator.validate_meeting_token(&oversized).await;
        assert!(
            matches!(&result, Err(MhError::JwtValidation(_))),
            "Expected JwtValidation error for oversized token, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_jwks_unreachable() {
        // JWKS endpoint is unreachable — should map to MhError::Internal
        let jwks_client = Arc::new(
            JwksClient::new("http://127.0.0.1:1/.well-known/jwks.json".to_string())
                .expect("Failed to create JWKS client"),
        );
        let validator = MhJwtValidator::new(jwks_client, 300);

        // Use a structurally valid JWT (3 dot-separated parts with a kid header)
        // so validation gets past size/format checks and actually hits JWKS lookup
        let keypair = TestKeypair::new(42, "unreachable-key");
        let claims = make_meeting_claims();
        let token = keypair.sign_token(&claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(MhError::Internal(_))),
            "Expected Internal error for unreachable JWKS, got {:?}",
            result
        );
    }
}
