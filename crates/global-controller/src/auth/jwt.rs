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
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use std::sync::Arc;
use tracing::instrument;

/// Maximum allowed JWT size in bytes (8KB).
///
/// This limit prevents Denial-of-Service (DoS) attacks via oversized tokens.
/// JWTs larger than this size are rejected BEFORE any parsing or cryptographic
/// operations, providing defense-in-depth against resource exhaustion attacks.
///
/// Per OWASP API Security Top 10 - API4:2023 (Unrestricted Resource Consumption)
const MAX_JWT_SIZE_BYTES: usize = 8192; // 8KB

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
    #[instrument(skip(self, token))]
    pub async fn validate(&self, token: &str) -> Result<Claims, GcError> {
        // 1. Check token size BEFORE any parsing (DoS prevention)
        if token.len() > MAX_JWT_SIZE_BYTES {
            tracing::debug!(
                target: "gc.auth.jwt",
                token_size = token.len(),
                max_size = MAX_JWT_SIZE_BYTES,
                "Token rejected: size exceeds maximum allowed"
            );
            return Err(GcError::InvalidToken(
                "The access token is invalid or expired".to_string(),
            ));
        }

        // 2. Extract kid from JWT header
        let kid = extract_kid(token).ok_or_else(|| {
            tracing::debug!(target: "gc.auth.jwt", "Token missing kid header");
            GcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

        // 3. Fetch public key from JWKS
        let jwk = self.jwks_client.get_key(&kid).await?;

        // 4. Verify signature and extract claims
        let claims = verify_token(token, &jwk)?;

        // 5. Validate iat claim with clock skew tolerance
        let now = chrono::Utc::now().timestamp();
        let max_iat = now + self.clock_skew_seconds;

        if claims.iat > max_iat {
            tracing::debug!(
                target: "gc.auth.jwt",
                iat = claims.iat,
                now = now,
                max_allowed = max_iat,
                clock_skew_seconds = self.clock_skew_seconds,
                "Token rejected: iat too far in the future"
            );
            return Err(GcError::InvalidToken(
                "The access token is invalid or expired".to_string(),
            ));
        }

        tracing::debug!(target: "gc.auth.jwt", "Token validated successfully");
        Ok(claims)
    }
}

/// Extract the `kid` (key ID) from a JWT header without verifying the signature.
///
/// # Security Note
///
/// This function does NOT validate the token. It only extracts the `kid` claim
/// for key lookup. The token MUST still be verified after fetching the key.
fn extract_kid(token: &str) -> Option<String> {
    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode the header (first part)
    let header_bytes = URL_SAFE_NO_PAD.decode(parts.first()?).ok()?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes).ok()?;

    // Extract kid as string
    header.get("kid")?.as_str().map(|s| s.to_string())
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

    // Decode public key from base64url
    let public_key_bytes = URL_SAFE_NO_PAD.decode(public_key_b64).map_err(|e| {
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

    #[test]
    fn test_extract_kid_valid_token() {
        // Create a valid JWT header with kid
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key-01"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert_eq!(kid, Some("test-key-01".to_string()));
    }

    #[test]
    fn test_extract_kid_missing_kid() {
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let token = format!("{}.payload.signature", header_b64);

        let kid = extract_kid(&token);
        assert!(kid.is_none());
    }

    #[test]
    fn test_extract_kid_malformed_token() {
        // Wrong number of parts
        assert!(extract_kid("not.a.valid.jwt.format").is_none());
        assert!(extract_kid("only.two").is_none());
        assert!(extract_kid("single").is_none());
        assert!(extract_kid("").is_none());
    }

    #[test]
    fn test_extract_kid_invalid_base64() {
        let token = "!!!invalid!!!.payload.signature";
        assert!(extract_kid(token).is_none());
    }

    #[test]
    fn test_extract_kid_invalid_json() {
        let header_b64 = URL_SAFE_NO_PAD.encode("not valid json".as_bytes());
        let token = format!("{}.payload.signature", header_b64);
        assert!(extract_kid(&token).is_none());
    }

    #[test]
    fn test_max_jwt_size_constant() {
        assert_eq!(
            MAX_JWT_SIZE_BYTES, 8192,
            "Max JWT size should be 8KB for DoS protection"
        );
    }
}
