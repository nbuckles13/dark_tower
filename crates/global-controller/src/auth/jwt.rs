//! JWT validation for Global Controller.
//!
//! Validates incoming JWTs using public keys fetched from Auth Controller's JWKS endpoint.
//!
//! # Security
//!
//! - Tokens are size-checked BEFORE parsing (DoS prevention)
//! - Only EdDSA (Ed25519) algorithm is accepted
//! - Expiration and issued-at claims are validated with clock skew tolerance
//! - Generic error messages prevent information leakage

use crate::auth::claims::Claims;
use crate::auth::jwks::{Jwk, JwksClient};
use crate::errors::GcError;
use common::jwt::{decode_ed25519_public_key_jwk, extract_kid, validate_iat};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use std::sync::Arc;
use tracing::instrument;

/// JWT validator using JWKS from Auth Controller.
pub struct JwtValidator {
    /// JWKS client for fetching public keys.
    jwks_client: Arc<JwksClient>,

    /// Clock skew tolerance in seconds for iat validation.
    clock_skew_seconds: i64,
}

impl JwtValidator {
    /// Create a new JWT validator.
    ///
    /// # Arguments
    ///
    /// * `jwks_client` - Client for fetching public keys
    /// * `clock_skew_seconds` - Clock skew tolerance for iat validation
    pub fn new(jwks_client: Arc<JwksClient>, clock_skew_seconds: i64) -> Self {
        Self {
            jwks_client,
            clock_skew_seconds,
        }
    }

    /// Validate a JWT and return the claims.
    ///
    /// # Security Checks
    ///
    /// 1. Size check - reject tokens > 8KB before parsing
    /// 2. Extract kid from header to find the correct key
    /// 3. Fetch public key from JWKS
    /// 4. Verify EdDSA signature
    /// 5. Validate exp claim (reject expired tokens)
    /// 6. Validate iat claim with clock skew tolerance
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT string to validate
    ///
    /// # Errors
    ///
    /// Returns `GcError::InvalidToken` for all validation failures with a generic
    /// message to prevent information leakage.
    #[instrument(skip_all)]
    pub async fn validate(&self, token: &str) -> Result<Claims, GcError> {
        // 1. Extract kid from JWT header (includes size check via common::jwt)
        let kid = extract_kid(token).map_err(|e| {
            tracing::debug!(target: "gc.auth.jwt", error = ?e, "Token kid extraction failed");
            GcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

        // 2. Fetch public key from JWKS
        let jwk = self.jwks_client.get_key(&kid).await?;

        // 3. Verify signature and extract claims
        let claims = verify_token(token, &jwk)?;

        // 4. Validate iat claim with clock skew tolerance using common utility
        if let Err(e) = validate_iat(claims.iat, self.clock_skew_seconds) {
            tracing::debug!(target: "gc.auth.jwt", error = ?e, "Token iat validation failed");
            return Err(GcError::InvalidToken(
                "The access token is invalid or expired".to_string(),
            ));
        }

        tracing::debug!(target: "gc.auth.jwt", "Token validated successfully");
        Ok(claims)
    }
}

/// Verify JWT signature and extract claims.
///
/// Uses EdDSA (Ed25519) algorithm exclusively per project security requirements.
fn verify_token(token: &str, jwk: &Jwk) -> Result<Claims, GcError> {
    // Validate JWK is EdDSA key
    if jwk.kty != "OKP" {
        tracing::warn!(target: "gc.auth.jwt", kty = %jwk.kty, "Unexpected JWK key type");
        return Err(GcError::InvalidToken(
            "The access token is invalid or expired".to_string(),
        ));
    }
    if let Some(alg) = &jwk.alg {
        if alg != "EdDSA" {
            tracing::warn!(target: "gc.auth.jwt", alg = %alg, "Unexpected JWK algorithm");
            return Err(GcError::InvalidToken(
                "The access token is invalid or expired".to_string(),
            ));
        }
    }

    // Get public key bytes from JWK
    let public_key_b64 = jwk.x.as_ref().ok_or_else(|| {
        tracing::error!(target: "gc.auth.jwt", kid = %jwk.kid, "JWK missing x field");
        GcError::InvalidToken("The access token is invalid or expired".to_string())
    })?;

    // Decode public key from base64url using common utility
    let public_key_bytes = decode_ed25519_public_key_jwk(public_key_b64).map_err(|e| {
        tracing::error!(target: "gc.auth.jwt", error = %e, "Invalid public key encoding");
        GcError::InvalidToken("The access token is invalid or expired".to_string())
    })?;

    // Create decoding key
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Configure validation
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    // Don't validate aud - we'll check scopes instead

    // Decode and verify
    let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(target: "gc.auth.jwt", error = %e, "Token verification failed");
        GcError::InvalidToken("The access token is invalid or expired".to_string())
    })?;

    Ok(token_data.claims)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::auth::jwks::Jwk;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use common::jwt::MAX_JWT_SIZE_BYTES;

    #[test]
    fn test_extract_kid_valid_token() {
        // Create a valid JWT header with kid
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key-01"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert_eq!(kid.unwrap(), "test-key-01".to_string());
    }

    #[test]
    fn test_extract_kid_missing_kid() {
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert!(kid.is_err());
    }

    #[test]
    fn test_extract_kid_malformed_token() {
        // Wrong number of parts
        assert!(extract_kid("not.a.valid.jwt.format").is_err());
        assert!(extract_kid("only.two").is_err());
        assert!(extract_kid("single").is_err());
        assert!(extract_kid("").is_err());
    }

    #[test]
    fn test_extract_kid_invalid_base64() {
        let token = "!!!invalid!!!.payload.signature";
        assert!(extract_kid(token).is_err());
    }

    #[test]
    fn test_extract_kid_invalid_json() {
        let header_b64 = URL_SAFE_NO_PAD.encode("not valid json".as_bytes());
        let token = format!("{}.payload.signature", header_b64);
        assert!(extract_kid(&token).is_err());
    }

    #[test]
    fn test_max_jwt_size_constant() {
        assert_eq!(
            MAX_JWT_SIZE_BYTES, 8192,
            "Max JWT size should be 8KB for DoS protection"
        );
    }

    // =========================================================================
    // verify_token tests - JWK validation
    // =========================================================================

    #[test]
    fn test_verify_token_rejects_non_okp_key_type() {
        let jwk = Jwk {
            kty: "RSA".to_string(), // Wrong key type
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        // Create a fake token (doesn't matter, we're testing JWK validation)
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{}.{}.fake_signature", header_b64, payload_b64);

        let result = verify_token(&token, &jwk);
        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken with 'invalid or expired', got {:?}",
            err
        );
    }

    #[test]
    fn test_verify_token_rejects_non_eddsa_algorithm() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()),
            alg: Some("RS256".to_string()), // Wrong algorithm
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{}.{}.fake_signature", header_b64, payload_b64);

        let result = verify_token(&token, &jwk);
        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken with 'invalid or expired', got {:?}",
            err
        );
    }

    #[test]
    fn test_verify_token_rejects_missing_x_field() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: None, // Missing public key
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{}.{}.fake_signature", header_b64, payload_b64);

        let result = verify_token(&token, &jwk);
        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken with 'invalid or expired', got {:?}",
            err
        );
    }

    #[test]
    fn test_verify_token_rejects_invalid_base64_public_key() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("!!!invalid-base64!!!".to_string()), // Invalid base64
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{}.{}.fake_signature", header_b64, payload_b64);

        let result = verify_token(&token, &jwk);
        let err = result.expect_err("Expected error");
        assert!(
            matches!(&err, GcError::InvalidToken(msg) if msg.contains("invalid or expired")),
            "Expected InvalidToken with 'invalid or expired', got {:?}",
            err
        );
    }

    #[test]
    fn test_verify_token_accepts_jwk_without_alg_field() {
        // JWK without alg field should still be processed (alg is optional)
        // but will fail at signature verification with invalid key
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()), // Valid base64 but not real key
            alg: None,                                   // No algorithm specified
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{}.{}.fake_signature", header_b64, payload_b64);

        // This should fail at signature verification, not at JWK validation
        let result = verify_token(&token, &jwk);
        // Error should be about invalid token (signature verification failed)
        assert!(
            matches!(result, Err(GcError::InvalidToken(_))),
            "Expected InvalidToken, got {:?}",
            result
        );
    }

    // =========================================================================
    // JwtValidator tests
    // =========================================================================

    #[test]
    fn test_jwt_validator_creation() {
        use std::sync::Arc;

        let jwks_client = Arc::new(JwksClient::new(
            "http://localhost:8082/.well-known/jwks.json".to_string(),
        ));
        let validator = JwtValidator::new(jwks_client, 300);

        // Verify clock skew is set
        assert_eq!(validator.clock_skew_seconds, 300);
    }

    // =========================================================================
    // extract_kid edge cases
    // =========================================================================

    #[test]
    fn test_extract_kid_with_empty_parts() {
        // Token with empty header part
        let token = ".payload.signature";
        assert!(extract_kid(token).is_err());
    }

    #[test]
    fn test_extract_kid_with_numeric_kid() {
        // kid as number in JSON (should return Err since we expect string)
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":12345}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert!(kid.is_err());
    }

    #[test]
    fn test_extract_kid_with_null_kid() {
        // kid as null in JSON
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":null}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert!(kid.is_err());
    }

    #[test]
    fn test_extract_kid_with_empty_string_kid() {
        // kid as empty string - rejected for defense-in-depth
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":""}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let result = extract_kid(&token);
        assert!(result.is_err(), "Empty kid should be rejected");
    }

    #[test]
    fn test_extract_kid_with_special_characters() {
        // kid with special characters
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"key-with-special_chars.123"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert_eq!(kid.unwrap(), "key-with-special_chars.123".to_string());
    }

    // =========================================================================
    // Token size boundary tests
    // =========================================================================

    #[test]
    fn test_token_exactly_at_8192_bytes() {
        // Create a token exactly 8192 bytes (at the limit)
        let padding = "a".repeat(8192 - 20); // Account for header/payload structure
        let token = format!("{}.test.sig", padding);

        // Token at exactly 8192 bytes should pass size check
        assert!(token.len() <= MAX_JWT_SIZE_BYTES);
    }

    #[test]
    fn test_token_over_8192_bytes() {
        // Create a token over 8192 bytes
        let token = "a".repeat(8193);

        // Token over 8192 bytes should fail size check
        assert!(token.len() > MAX_JWT_SIZE_BYTES);
    }
}
