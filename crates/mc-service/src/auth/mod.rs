//! JWT validation for Meeting Controller.
//!
//! Thin wrapper around `common::jwt::JwtValidator` that provides MC-specific
//! `validate_meeting_token()` and `validate_guest_token()` methods with
//! automatic `JwtError` -> `McError` error mapping.
//!
//! # Token Types
//!
//! - **Meeting tokens**: Authenticated participant tokens with `token_type == "meeting"`
//! - **Guest tokens**: Unauthenticated guest tokens with `token_type == "guest"`
//!
//! Each method validates the token signature, expiration, iat clock skew,
//! and then enforces token-type-specific field constraints.
//!
//! # Security
//!
//! - Token type is checked after signature verification to prevent token confusion
//!   (e.g., a guest token being accepted as a meeting token)
//! - Error messages are generic to prevent information leakage

use crate::errors::McError;
use common::jwt::{GuestTokenClaims, JwksClient, MeetingTokenClaims};
use std::sync::Arc;
use tracing::instrument;

/// Re-export the common JwtValidator for direct generic usage.
pub use common::jwt::JwtValidator as CommonJwtValidator;

/// MC JWT validator wrapping the common `JwtValidator`.
///
/// Provides typed methods that return `McError` and enforce
/// token-type constraints after signature verification.
pub struct McJwtValidator {
    inner: CommonJwtValidator,
}

impl McJwtValidator {
    /// Create a new MC JWT validator.
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
    /// 1. Common checks: size, kid extraction, JWKS lookup, EdDSA signature, exp, iat
    /// 2. Token type: `token_type` must be `"meeting"` (prevents token confusion)
    ///
    /// # Errors
    ///
    /// Returns `McError::JwtValidation` for all validation failures with a generic
    /// message to prevent information leakage.
    #[instrument(skip_all)]
    pub async fn validate_meeting_token(&self, token: &str) -> Result<MeetingTokenClaims, McError> {
        let claims = self.inner.validate::<MeetingTokenClaims>(token).await?;

        if claims.token_type != common::jwt::MEETING_TOKEN_TYPE {
            tracing::warn!(
                target: "mc.auth",
                token_type = %claims.token_type,
                "Token type mismatch: expected meeting token"
            );
            return Err(McError::JwtValidation(
                "The access token is invalid or expired".to_string(),
            ));
        }

        Ok(claims)
    }

    /// Validate a guest JWT and return the claims.
    ///
    /// # Security Checks
    ///
    /// 1. Common checks: size, kid extraction, JWKS lookup, EdDSA signature, exp, iat
    /// 2. Guest field enforcement: `token_type`, `participant_type`, and `role` must all be `"guest"`
    ///
    /// # Errors
    ///
    /// Returns `McError::JwtValidation` for all validation failures.
    #[instrument(skip_all)]
    pub async fn validate_guest_token(&self, token: &str) -> Result<GuestTokenClaims, McError> {
        let claims = self.inner.validate::<GuestTokenClaims>(token).await?;

        claims.validate().map_err(|reason| {
            tracing::warn!(target: "mc.auth", reason = reason, "Guest token field validation failed");
            McError::JwtValidation("The access token is invalid or expired".to_string())
        })?;

        Ok(claims)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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

    /// Test keypair for signing tokens.
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

    /// Set up wiremock JWKS server and return (mock_server, keypair, validator).
    async fn setup_test_validator() -> (MockServer, TestKeypair, McJwtValidator) {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(42, "mc-test-key-01");

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
        let validator = McJwtValidator::new(jwks_client, 300);

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
            display_name: "Test Participant".to_string(),
            iat: now,
            exp: now + 3600,
            jti: "jti-001".to_string(),
        }
    }

    fn make_guest_claims() -> GuestTokenClaims {
        let now = Utc::now().timestamp();
        GuestTokenClaims {
            sub: "guest-001".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-123".to_string(),
            meeting_org_id: "org-456".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Test Guest".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: now,
            exp: now + 3600,
            jti: "jti-guest-001".to_string(),
        }
    }

    #[test]
    fn test_mc_jwt_validator_creation() {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost:8082/.well-known/jwks.json".to_string())
                .expect("Failed to create JWKS client"),
        );
        let _validator = McJwtValidator::new(jwks_client, 300);
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
    async fn test_validate_guest_token_success() {
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let claims = make_guest_claims();
        let token = keypair.sign_token(&claims);

        let result = validator.validate_guest_token(&token).await;
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);

        let validated = result.unwrap();
        assert_eq!(validated.meeting_id, "meeting-123");
        assert_eq!(validated.token_type, "guest");
        assert_eq!(validated.display_name, "Test Guest");
        assert!(validated.waiting_room);
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_guest_token() {
        // Token confusion prevention: a valid guest token must not be accepted
        // as a meeting token. This tests deserialization-level rejection (guest
        // participant_type "guest" doesn't match MeetingTokenClaims enum).
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let guest_claims = make_guest_claims();
        let token = keypair.sign_token(&guest_claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(msg)) if msg.contains("invalid or expired")),
            "Expected JwtValidation error for guest token used as meeting token, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_wrong_token_type() {
        // Directly exercises the token_type != "meeting" guard (line 66).
        // Uses a structurally valid MeetingTokenClaims but with token_type "guest".
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let mut claims = make_meeting_claims();
        claims.token_type = "guest".to_string();

        let token = keypair.sign_token(&claims);
        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(msg)) if msg.contains("invalid or expired")),
            "Expected JwtValidation error for wrong token_type, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_guest_token_rejects_meeting_token() {
        // Bidirectional token confusion: a valid meeting token must not be
        // accepted as a guest token.
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let claims = make_meeting_claims();
        let token = keypair.sign_token(&claims);

        let result = validator.validate_guest_token(&token).await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(_))),
            "Expected JwtValidation error for meeting token used as guest token, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_guest_token_rejects_invalid_role() {
        let (_mock_server, keypair, validator) = setup_test_validator().await;
        let mut claims = make_guest_claims();
        claims.role = "host".to_string(); // tampered role

        let token = keypair.sign_token(&claims);
        let result = validator.validate_guest_token(&token).await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(_))),
            "Expected JwtValidation error for tampered guest role, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_invalid_token() {
        let (_mock_server, _keypair, validator) = setup_test_validator().await;

        let result = validator.validate_meeting_token("not-a-valid-jwt").await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(_))),
            "Expected JwtValidation error, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_validate_meeting_token_rejects_wrong_signature() {
        let (_mock_server, _keypair, validator) = setup_test_validator().await;

        // Sign with a different key
        let wrong_keypair = TestKeypair::new(99, "wrong-key");
        let claims = make_meeting_claims();
        let token = wrong_keypair.sign_token(&claims);

        let result = validator.validate_meeting_token(&token).await;
        assert!(
            matches!(&result, Err(McError::JwtValidation(_))),
            "Expected JwtValidation error for wrong key, got {:?}",
            result
        );
    }

    // ---------------------------------------------------------------------
    // Cross-meeting KEK guard BY CONSTRUCTION (ADR-0039 §5): MC's join path
    // verifies only AC-signed tokens (AC JWKS, kid REQUIRED, EdDSA pinned), so
    // an MC-minted `media_dial` token — kid-less, signed by a key in no JWKS —
    // is refused there. These arms use the REAL mint.
    // ---------------------------------------------------------------------

    fn mc_minted_media_dial() -> (crate::media_dial::key::DialSigningKey, String) {
        let key = crate::media_dial::key::DialSigningKey::from_seed_for_test(&[0x3c; 32]);
        let token = key
            .mint(
                "user-001",
                "meeting-123",
                "mh-1",
                Utc::now().timestamp(),
                300,
            )
            .unwrap()
            .into_wire();
        (key, token)
    }

    fn claims_of(token: &str) -> common::jwt::MediaDialClaims {
        let payload = token.split('.').nth(1).unwrap();
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
    }

    async fn jwks_hits(server: &MockServer) -> usize {
        server.received_requests().await.unwrap_or_default().len()
    }

    /// Arm a: a kid-less MC-minted `media_dial` is `MissingKid` — decided
    /// before the JWKS is consulted (hit count 0) — and both join validators
    /// refuse it. The token is under the size cap, so the size check passed
    /// first and is not what refused it.
    #[tokio::test]
    async fn join_rejects_a_kidless_media_dial_before_consulting_the_jwks() {
        let (server, _ac, validator) = setup_test_validator().await;
        let (_key, token) = mc_minted_media_dial();
        assert!(token.len() < common::jwt::MAX_JWT_SIZE_BYTES);

        let inner = CommonJwtValidator::new(
            Arc::new(JwksClient::new(format!("{}/.well-known/jwks.json", server.uri())).unwrap()),
            300,
        );
        assert!(matches!(
            inner.validate::<common::jwt::MediaDialClaims>(&token).await,
            Err(common::jwt::JwtError::MissingKid)
        ));
        assert!(matches!(
            validator.validate_meeting_token(&token).await,
            Err(McError::JwtValidation(_))
        ));
        assert!(matches!(
            validator.validate_guest_token(&token).await,
            Err(McError::JwtValidation(_))
        ));
        assert_eq!(
            jwks_hits(&server).await,
            0,
            "refused before any JWKS lookup"
        );
    }

    /// Arm b: the SAME MC-minted claim set, re-signed by a foreign key under a
    /// kid that IS live in the served JWKS. (b1) The join path consults the
    /// JWKS and refuses. (b2) The cause is the signature: a direct decode
    /// against the served key with `aud` validation OFF is `InvalidSignature`,
    /// while the forging key decodes it — so the signature is the only
    /// difference.
    ///
    /// NOTE: `common::jwt::verify_token` also REJECTS any token carrying `aud`
    /// (jsonwebtoken validates `aud` against an empty expected set) and maps
    /// that to `InvalidSignature` too. That is a second wall against a
    /// `media_dial` token, not a bug: do not "fix" it.
    #[tokio::test]
    async fn join_rejects_a_media_dial_forged_under_a_live_kid_by_signature() {
        use jsonwebtoken::{decode, errors::ErrorKind, DecodingKey, Validation};
        let (server, ac, validator) = setup_test_validator().await;
        let (_key, minted) = mc_minted_media_dial();
        let claims = claims_of(&minted);
        let forger = TestKeypair::new(99, &ac.kid);
        let forged = forger.sign_token(&claims);

        // b1
        assert!(matches!(
            validator.validate_meeting_token(&forged).await,
            Err(McError::JwtValidation(_))
        ));
        assert!(
            jwks_hits(&server).await >= 1,
            "past extract_kid: the JWKS was consulted"
        );

        // b1': attributable to the SIGNATURE on MC's own verifier. The
        // aud-LESS MC claim set (so verify_token's aud wall is not what
        // refuses it), forged under the live kid, is exactly InvalidSignature
        // through the inner validator the join uses; the same claims signed by
        // the live key pass it — the signature is the only difference.
        #[derive(serde::Deserialize, Serialize)]
        struct AudlessDial {
            sub: String,
            token_type: String,
            meeting_id: String,
            iat: i64,
            exp: i64,
        }
        impl common::jwt::HasIat for AudlessDial {
            fn iat(&self) -> i64 {
                self.iat
            }
        }
        let audless = AudlessDial {
            sub: claims.sub.clone(),
            token_type: claims.token_type.clone(),
            meeting_id: claims.meeting_id.clone(),
            iat: claims.iat,
            exp: claims.exp,
        };
        let inner = CommonJwtValidator::new(
            Arc::new(JwksClient::new(format!("{}/.well-known/jwks.json", server.uri())).unwrap()),
            300,
        );
        assert!(matches!(
            inner
                .validate::<AudlessDial>(&forger.sign_token(&audless))
                .await,
            Err(common::jwt::JwtError::InvalidSignature)
        ));
        assert!(
            inner
                .validate::<AudlessDial>(&ac.sign_token(&audless))
                .await
                .is_ok(),
            "control: the live key's signature over the same claims verifies"
        );

        // b2
        let mut no_aud = Validation::new(Algorithm::EdDSA);
        no_aud.validate_aud = false;
        let served = DecodingKey::from_ed_der(&ac.public_key_bytes);
        let err = decode::<common::jwt::MediaDialClaims>(&forged, &served, &no_aud).unwrap_err();
        assert_eq!(*err.kind(), ErrorKind::InvalidSignature);
        let forging = DecodingKey::from_ed_der(&forger.public_key_bytes);
        assert!(decode::<common::jwt::MediaDialClaims>(&forged, &forging, &no_aud).is_ok());
    }

    /// Arm d: even with a GOOD signature (the live AC key, a served kid), a
    /// `media_dial` claim set is never accepted where an AC token is expected:
    /// the `aud` wall refuses it, and with `aud` removed the missing
    /// meeting/guest fields and the `token_type` check still do. Arm c (positive): an AC meeting token is accepted
    /// by the same validator.
    #[tokio::test]
    async fn a_media_dial_claim_set_is_refused_even_under_the_live_ac_key() {
        let (_server, ac, validator) = setup_test_validator().await;
        let (_key, minted) = mc_minted_media_dial();
        let signed_by_ac = ac.sign_token(&claims_of(&minted));
        assert!(matches!(
            validator.validate_meeting_token(&signed_by_ac).await,
            Err(McError::JwtValidation(_))
        ));
        assert!(matches!(
            validator.validate_guest_token(&signed_by_ac).await,
            Err(McError::JwtValidation(_))
        ));

        // The shape and token_type walls on their own: the SAME claim set with
        // `aud` removed (so verify_token's aud wall is not what refuses it),
        // signed by the live AC key, is still refused by both validators.
        let mut no_aud = serde_json::to_value(claims_of(&minted)).unwrap();
        no_aud.as_object_mut().unwrap().remove("aud");
        let signed_no_aud = ac.sign_token(&no_aud);
        assert!(matches!(
            validator.validate_meeting_token(&signed_no_aud).await,
            Err(McError::JwtValidation(_))
        ));
        assert!(matches!(
            validator.validate_guest_token(&signed_no_aud).await,
            Err(McError::JwtValidation(_))
        ));

        let ac_token = ac.sign_token(&make_meeting_claims());
        assert!(validator.validate_meeting_token(&ac_token).await.is_ok());
    }

    /// MC's dial key is never in AC's JWKS (the fixture's served set).
    #[tokio::test]
    async fn the_dial_public_key_is_not_in_the_served_jwks() {
        let (_server, ac, _validator) = setup_test_validator().await;
        let (key, _token) = mc_minted_media_dial();
        assert_ne!(
            ac.jwk_json()["x"].as_str().unwrap(),
            URL_SAFE_NO_PAD.encode(key.public_key())
        );
    }
}
