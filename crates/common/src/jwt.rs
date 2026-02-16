//! JWT utilities shared across Dark Tower services.
//!
//! This module provides common JWT validation utilities including:
//! - Size limits for DoS prevention
//! - Clock skew constants for iat validation
//! - Key ID extraction from JWT headers
//! - iat validation logic
//! - Service token claims structure
//!
//! # Security
//!
//! - Tokens are size-checked BEFORE parsing (DoS prevention)
//! - Only EdDSA (Ed25519) algorithm is accepted
//! - Generic error messages prevent information leakage
//! - The `sub` field in Claims is redacted in Debug output
//!
//! # Usage
//!
//! ```rust,ignore
//! use common::jwt::{extract_kid, validate_iat, MAX_JWT_SIZE_BYTES, ServiceClaims};
//!
//! // Check token size before parsing
//! if token.len() > MAX_JWT_SIZE_BYTES {
//!     return Err("Token too large");
//! }
//!
//! // Extract key ID for JWKS lookup
//! let kid = extract_kid(token)?;
//!
//! // After signature verification, validate iat
//! validate_iat(claims.iat, DEFAULT_CLOCK_SKEW)?;
//! ```
//!
//! **ADRs**: ADR-0003 (Service Auth), ADR-0007 (Token Lifetime)

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use thiserror::Error;

// =============================================================================
// Constants
// =============================================================================

/// Maximum allowed JWT size in bytes (8KB).
///
/// This limit prevents denial-of-service attacks via oversized tokens.
/// JWTs larger than this size are rejected BEFORE any parsing or cryptographic
/// operations, providing defense-in-depth against resource exhaustion attacks.
///
/// # Rationale
///
/// - Typical JWTs are 200-500 bytes (header + claims + signature)
/// - Standard service token: ~350 bytes (`EdDSA` sig, basic claims)
/// - 8KB limit allows for reasonable expansion while preventing abuse
/// - Checked BEFORE base64 decode and signature verification for efficiency
///
/// # Attack Scenario
///
/// - Attacker sends 10MB JWT to /token/verify endpoint
/// - Without size limit: Base64 decode allocates large buffer, wastes CPU/memory
/// - With size limit: Rejected immediately with minimal resource usage
///
/// Per OWASP API Security Top 10 - API4:2023 (Unrestricted Resource Consumption)
pub const MAX_JWT_SIZE_BYTES: usize = 8192; // 8KB

/// Default JWT clock skew tolerance (5 minutes per NIST SP 800-63B).
///
/// This tolerance accounts for clock drift between servers. Tokens with `iat`
/// (issued-at) timestamps more than this amount in the future are rejected.
pub const DEFAULT_CLOCK_SKEW: Duration = Duration::from_secs(300);

/// Maximum allowed JWT clock skew tolerance (10 minutes).
///
/// This prevents misconfiguration that could weaken security by allowing
/// excessively large clock skew tolerance.
pub const MAX_CLOCK_SKEW: Duration = Duration::from_secs(600);

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during JWT validation.
///
/// Note: Error messages are intentionally generic to prevent information leakage.
/// Detailed information is logged at debug level for troubleshooting.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum JwtValidationError {
    /// Token size exceeds maximum allowed.
    #[error("The access token is invalid or expired")]
    TokenTooLarge,

    /// Token format is invalid (not a valid JWT structure).
    #[error("The access token is invalid or expired")]
    MalformedToken,

    /// Token is missing required `kid` header.
    #[error("The access token is invalid or expired")]
    MissingKid,

    /// Token `iat` claim is too far in the future.
    #[error("The access token is invalid or expired")]
    IatTooFarInFuture,
}

// =============================================================================
// Claims Types
// =============================================================================

/// Service token claims structure.
///
/// Used for service-to-service authentication tokens. The `sub` field contains
/// service identifiers which are redacted in Debug output.
///
/// # Fields
///
/// - `sub`: Subject (service identifier)
/// - `exp`: Expiration timestamp (Unix epoch seconds)
/// - `iat`: Issued-at timestamp (Unix epoch seconds)
/// - `scope`: Space-separated permissions
/// - `service_type`: Optional service type identifier
///
/// # Security
///
/// The `sub` field is redacted in Debug output to prevent accidental logging
/// of service identifiers.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServiceClaims {
    /// Subject (service identifier) - redacted in Debug output.
    pub sub: String,

    /// Expiration timestamp (Unix epoch seconds).
    pub exp: i64,

    /// Issued-at timestamp (Unix epoch seconds).
    pub iat: i64,

    /// Space-separated scopes granted to this token.
    pub scope: String,

    /// Optional service type for service-to-service tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,
}

impl fmt::Debug for ServiceClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceClaims")
            .field("sub", &"[REDACTED]")
            .field("exp", &self.exp)
            .field("iat", &self.iat)
            .field("scope", &self.scope)
            .field("service_type", &self.service_type)
            .finish()
    }
}

impl ServiceClaims {
    /// Creates a new `ServiceClaims` instance.
    ///
    /// # Arguments
    ///
    /// * `sub` - Subject (service identifier)
    /// * `exp` - Expiration timestamp (Unix epoch seconds)
    /// * `iat` - Issued-at timestamp (Unix epoch seconds)
    /// * `scope` - Space-separated permissions
    /// * `service_type` - Optional service type identifier
    #[must_use]
    pub fn new(
        sub: String,
        exp: i64,
        iat: i64,
        scope: String,
        service_type: Option<String>,
    ) -> Self {
        Self {
            sub,
            exp,
            iat,
            scope,
            service_type,
        }
    }

    /// Check if the token has a specific scope.
    ///
    /// Scopes are space-separated in the JWT claims.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if claims.has_scope("meetings:create") {
    ///     // Allow meeting creation
    /// }
    /// ```
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }

    /// Get all scopes as a vector.
    #[must_use]
    pub fn scopes(&self) -> Vec<&str> {
        self.scope.split_whitespace().collect()
    }
}

// =============================================================================
// Functions
// =============================================================================

/// Extract the `kid` (key ID) from a JWT header without verifying the signature.
///
/// This is used to look up the correct signing key for verification when
/// multiple keys may be valid (e.g., during key rotation).
///
/// # Security
///
/// - Token size is checked BEFORE any parsing (denial-of-service prevention)
/// - This function does NOT validate the token signature
/// - The token MUST still be verified after fetching the key
/// - The `kid` value should only be used for key lookup in a trusted JWKS
///
/// # Arguments
///
/// * `token` - The JWT string to extract the kid from
///
/// # Returns
///
/// - `Ok(kid)` - The key ID from the JWT header
/// - `Err(TokenTooLarge)` - Token exceeds `MAX_JWT_SIZE_BYTES`
/// - `Err(MalformedToken)` - Token is not valid JWT format
/// - `Err(MissingKid)` - Token header doesn't contain a `kid` field
///
/// # Errors
///
/// Returns `JwtValidationError` variants:
/// - `TokenTooLarge` - Token exceeds size limit (denial-of-service protection)
/// - `MalformedToken` - Token format invalid (wrong structure, bad base64, invalid JSON)
/// - `MissingKid` - Token header missing `kid` field or `kid` is not a string
///
/// # Example
///
/// ```rust,ignore
/// use common::jwt::extract_kid;
///
/// match extract_kid(token) {
///     Ok(kid) => {
///         // Use kid to look up public key from JWKS
///         let key = jwks_client.get_key(&kid)?;
///     }
///     Err(e) => {
///         // Handle error (token rejected)
///     }
/// }
/// ```
pub fn extract_kid(token: &str) -> Result<String, JwtValidationError> {
    // Check token size first (DoS prevention)
    if token.len() > MAX_JWT_SIZE_BYTES {
        tracing::debug!(
            target: "common.jwt",
            token_size = token.len(),
            max_size = MAX_JWT_SIZE_BYTES,
            "Token rejected: size exceeds maximum allowed"
        );
        return Err(JwtValidationError::TokenTooLarge);
    }

    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        tracing::debug!(
            target: "common.jwt",
            parts = parts.len(),
            "Token rejected: invalid JWT format"
        );
        return Err(JwtValidationError::MalformedToken);
    }

    // Decode the header (first part) - safe indexing since we verified length above
    let header_part = parts.first().ok_or(JwtValidationError::MalformedToken)?;
    let header_bytes = URL_SAFE_NO_PAD.decode(header_part).map_err(|e| {
        tracing::debug!(target: "common.jwt", error = %e, "Failed to decode JWT header base64");
        JwtValidationError::MalformedToken
    })?;

    let header: serde_json::Value = serde_json::from_slice(&header_bytes).map_err(|e| {
        tracing::debug!(target: "common.jwt", error = %e, "Failed to parse JWT header JSON");
        JwtValidationError::MalformedToken
    })?;

    // Extract kid as string, rejecting empty values for defense-in-depth
    let kid = header
        .get("kid")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .ok_or(JwtValidationError::MissingKid)?;

    Ok(kid)
}

/// Validate the `iat` (issued-at) claim with clock skew tolerance.
///
/// Rejects tokens with `iat` too far in the future, which could indicate:
/// - Token pre-generation attack
/// - Clock synchronization issues
/// - Token manipulation
///
/// # Arguments
///
/// * `iat` - The issued-at timestamp from the JWT claims (Unix epoch seconds)
/// * `clock_skew` - Maximum allowed clock skew tolerance
///
/// # Returns
///
/// - `Ok(())` - The iat claim is valid
/// - `Err(IatTooFarInFuture)` - The iat is more than `clock_skew` in the future
///
/// # Errors
///
/// Returns `JwtValidationError::IatTooFarInFuture` if the iat timestamp is more than
/// `clock_skew` in the future.
///
/// # Example
///
/// ```rust,ignore
/// use common::jwt::{validate_iat, DEFAULT_CLOCK_SKEW};
///
/// // After verifying signature and extracting claims
/// validate_iat(claims.iat, DEFAULT_CLOCK_SKEW)?;
/// ```
pub fn validate_iat(iat: i64, clock_skew: Duration) -> Result<(), JwtValidationError> {
    let now = chrono::Utc::now().timestamp();
    validate_iat_at(iat, clock_skew, now)
}

/// Deterministic `iat` validation against an explicit `now` timestamp.
///
/// Prefer [`validate_iat`] in production code. This variant exists so that
/// boundary conditions can be unit-tested without wall-clock dependence.
pub(crate) fn validate_iat_at(
    iat: i64,
    clock_skew: Duration,
    now: i64,
) -> Result<(), JwtValidationError> {
    // Safe cast: clock_skew is bounded to MAX_CLOCK_SKEW (600 seconds), well within i64 range
    #[allow(clippy::cast_possible_wrap)]
    let clock_skew_secs = clock_skew.as_secs() as i64;
    let max_iat = now + clock_skew_secs;

    if iat > max_iat {
        tracing::debug!(
            target: "common.jwt",
            iat = iat,
            now = now,
            max_allowed = max_iat,
            clock_skew_secs = clock_skew_secs,
            "Token rejected: iat too far in the future"
        );
        return Err(JwtValidationError::IatTooFarInFuture);
    }

    Ok(())
}

/// Decode an Ed25519 public key from PEM format.
///
/// Strips PEM header/footer lines and decodes the base64 content.
///
/// # Arguments
///
/// * `pem` - The public key in PEM format (with or without header/footer)
///
/// # Returns
///
/// - `Ok(bytes)` - The raw public key bytes (DER format)
/// - `Err(...)` - If the base64 decoding fails
///
/// # Errors
///
/// Returns `base64::DecodeError` if the base64 content cannot be decoded.
///
/// # Example
///
/// ```rust,ignore
/// use common::jwt::decode_ed25519_public_key_pem;
///
/// let pem = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK...\n-----END PUBLIC KEY-----";
/// let der_bytes = decode_ed25519_public_key_pem(pem)?;
/// let decoding_key = DecodingKey::from_ed_der(&der_bytes);
/// ```
pub fn decode_ed25519_public_key_pem(pem: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // Extract base64 from PEM format (strip header/footer lines)
    let b64: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();

    base64::engine::general_purpose::STANDARD.decode(b64)
}

/// Decode an Ed25519 public key from JWK `x` field (base64url format).
///
/// The `x` field in an OKP (Octet Key Pair) JWK contains the public key
/// in base64url encoding without padding.
///
/// # Arguments
///
/// * `x_b64url` - The base64url-encoded public key from JWK `x` field
///
/// # Returns
///
/// - `Ok(bytes)` - The raw public key bytes
/// - `Err(...)` - If the base64url decoding fails
///
/// # Errors
///
/// Returns `base64::DecodeError` if the base64url content cannot be decoded.
///
/// # Example
///
/// ```rust,ignore
/// use common::jwt::decode_ed25519_public_key_jwk;
///
/// let x = jwk.x.as_ref().ok_or("missing x field")?;
/// let der_bytes = decode_ed25519_public_key_jwk(x)?;
/// let decoding_key = DecodingKey::from_ed_der(&der_bytes);
/// ```
pub fn decode_ed25519_public_key_jwk(x_b64url: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(x_b64url)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_possible_wrap)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Constants Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_max_jwt_size_is_8kb() {
        assert_eq!(MAX_JWT_SIZE_BYTES, 8192);
    }

    #[test]
    fn test_default_clock_skew_is_5_minutes() {
        assert_eq!(DEFAULT_CLOCK_SKEW, Duration::from_secs(300));
    }

    #[test]
    fn test_max_clock_skew_is_10_minutes() {
        assert_eq!(MAX_CLOCK_SKEW, Duration::from_secs(600));
    }

    // -------------------------------------------------------------------------
    // extract_kid Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_extract_kid_valid_token() {
        // Create a valid JWT header with kid
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key-01"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header);
        let token = format!("{header_b64}.payload.signature");

        let result = extract_kid(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-key-01");
    }

    #[test]
    fn test_extract_kid_missing_kid() {
        // Header without kid
        let header = r#"{"alg":"EdDSA","typ":"JWT"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header);
        let token = format!("{header_b64}.payload.signature");

        let result = extract_kid(&token);
        assert!(matches!(result, Err(JwtValidationError::MissingKid)));
    }

    #[test]
    fn test_extract_kid_malformed_token() {
        // Not a valid JWT format
        let result = extract_kid("not-a-jwt");
        assert!(matches!(result, Err(JwtValidationError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_empty_token() {
        // Empty string should be rejected as malformed
        let result = extract_kid("");
        assert!(matches!(result, Err(JwtValidationError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_invalid_base64() {
        let result = extract_kid("!!!invalid!!!.payload.signature");
        assert!(matches!(result, Err(JwtValidationError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_invalid_json() {
        let header_b64 = URL_SAFE_NO_PAD.encode("not-json");
        let token = format!("{header_b64}.payload.signature");

        let result = extract_kid(&token);
        assert!(matches!(result, Err(JwtValidationError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_oversized_token() {
        // Create a token larger than MAX_JWT_SIZE_BYTES
        let oversized = "a".repeat(MAX_JWT_SIZE_BYTES + 1);
        let result = extract_kid(&oversized);
        assert!(matches!(result, Err(JwtValidationError::TokenTooLarge)));
    }

    #[test]
    fn test_extract_kid_at_size_limit() {
        // Token exactly at size limit should be accepted
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header);
        // Need 3 parts: header.payload.signature (2 dots)
        let remaining = MAX_JWT_SIZE_BYTES - header_b64.len() - 2; // -2 for two dots
        let payload_len = remaining / 2;
        let sig_len = remaining - payload_len;
        let token = format!(
            "{}.{}.{}",
            header_b64,
            "a".repeat(payload_len),
            "b".repeat(sig_len)
        );

        // Verify token is exactly at size limit
        assert_eq!(
            token.len(),
            MAX_JWT_SIZE_BYTES,
            "Token should be exactly at size limit"
        );

        // Should succeed - token at limit is accepted
        let result = extract_kid(&token);
        assert!(result.is_ok(), "Token at size limit should be accepted");
        assert_eq!(result.unwrap(), "key");
    }

    #[test]
    fn test_extract_kid_non_string_kid() {
        // kid is a number, not a string
        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":12345}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header);
        let token = format!("{header_b64}.payload.signature");

        let result = extract_kid(&token);
        assert!(matches!(result, Err(JwtValidationError::MissingKid)));
    }

    // -------------------------------------------------------------------------
    // validate_iat Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_iat_current_time() {
        let now = chrono::Utc::now().timestamp();
        let result = validate_iat(now, DEFAULT_CLOCK_SKEW);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_iat_past_time() {
        let past = chrono::Utc::now().timestamp() - 3600; // 1 hour ago
        let result = validate_iat(past, DEFAULT_CLOCK_SKEW);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_iat_within_clock_skew() {
        let future = chrono::Utc::now().timestamp() + 200; // 200s in future (< 300s skew)
        let result = validate_iat(future, DEFAULT_CLOCK_SKEW);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_iat_at_clock_skew_boundary() {
        let future = chrono::Utc::now().timestamp() + DEFAULT_CLOCK_SKEW.as_secs() as i64;
        let result = validate_iat(future, DEFAULT_CLOCK_SKEW);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_iat_beyond_clock_skew() {
        let future = chrono::Utc::now().timestamp() + DEFAULT_CLOCK_SKEW.as_secs() as i64 + 10;
        let result = validate_iat(future, DEFAULT_CLOCK_SKEW);
        assert!(matches!(result, Err(JwtValidationError::IatTooFarInFuture)));
    }

    #[test]
    fn test_validate_iat_far_future() {
        let far_future = chrono::Utc::now().timestamp() + 86400; // 1 day in future
        let result = validate_iat(far_future, DEFAULT_CLOCK_SKEW);
        assert!(matches!(result, Err(JwtValidationError::IatTooFarInFuture)));
    }

    #[test]
    fn test_validate_iat_at_minimum_skew_boundary() {
        let now = 1_700_000_000_i64;
        let one_sec = Duration::from_secs(1);

        // iat exactly at boundary (now + skew) — accepted
        assert!(validate_iat_at(now + 1, one_sec, now).is_ok());

        // iat one second beyond boundary — rejected
        assert!(matches!(
            validate_iat_at(now + 2, one_sec, now),
            Err(JwtValidationError::IatTooFarInFuture)
        ));
    }

    #[test]
    fn test_validate_iat_at_boundary_exact() {
        let now = 1_700_000_000_i64;

        // iat == now + skew is the last accepted value
        assert!(validate_iat_at(now + 300, DEFAULT_CLOCK_SKEW, now).is_ok());

        // iat == now + skew + 1 is the first rejected value
        assert!(matches!(
            validate_iat_at(now + 301, DEFAULT_CLOCK_SKEW, now),
            Err(JwtValidationError::IatTooFarInFuture)
        ));
    }

    // -------------------------------------------------------------------------
    // ServiceClaims Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_service_claims_debug_redacts_sub() {
        let claims = ServiceClaims {
            sub: "secret-service-id".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: "read write".to_string(),
            service_type: None,
        };

        let debug_str = format!("{claims:?}");

        assert!(
            !debug_str.contains("secret-service-id"),
            "Debug output should not contain actual sub value"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain [REDACTED]"
        );
    }

    #[test]
    fn test_service_claims_has_scope() {
        let claims = ServiceClaims {
            sub: "service".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: "read write admin".to_string(),
            service_type: None,
        };

        assert!(claims.has_scope("read"));
        assert!(claims.has_scope("write"));
        assert!(claims.has_scope("admin"));
        assert!(!claims.has_scope("delete"));
        assert!(!claims.has_scope("rea")); // Partial match should not work
    }

    #[test]
    fn test_service_claims_scopes() {
        let claims = ServiceClaims {
            sub: "service".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: "read write admin".to_string(),
            service_type: None,
        };

        let scopes = claims.scopes();
        assert_eq!(scopes, vec!["read", "write", "admin"]);
    }

    #[test]
    fn test_service_claims_empty_scope() {
        let claims = ServiceClaims {
            sub: "service".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: String::new(),
            service_type: None,
        };

        assert!(!claims.has_scope("read"));
        assert!(claims.scopes().is_empty());
    }

    #[test]
    fn test_service_claims_serialization() {
        let claims = ServiceClaims {
            sub: "service123".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: "read write".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: ServiceClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.scope, claims.scope);
        assert_eq!(deserialized.service_type, claims.service_type);
    }

    #[test]
    fn test_service_claims_without_service_type_omits_field() {
        let claims = ServiceClaims {
            sub: "service".to_string(),
            exp: 1_234_567_890,
            iat: 1_234_567_800,
            scope: "read".to_string(),
            service_type: None,
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(
            !json.contains("service_type"),
            "service_type should be omitted when None"
        );
    }

    // -------------------------------------------------------------------------
    // Key Decoding Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_ed25519_public_key_pem() {
        // A minimal PEM-like format with valid base64
        // This is just "test" in base64 to verify header stripping works
        let pem = "-----BEGIN PUBLIC KEY-----\ndGVzdA==\n-----END PUBLIC KEY-----";
        // Just verify it strips headers correctly and decodes
        let result = decode_ed25519_public_key_pem(pem);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"test");
    }

    #[test]
    fn test_decode_ed25519_public_key_pem_without_headers() {
        // "hello" in standard base64
        let b64 = "aGVsbG8=";
        let result = decode_ed25519_public_key_pem(b64);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"hello");
    }

    #[test]
    fn test_decode_ed25519_public_key_pem_invalid_base64() {
        // Invalid base64 should fail
        let pem = "-----BEGIN PUBLIC KEY-----\n!!!invalid!!!\n-----END PUBLIC KEY-----";
        let result = decode_ed25519_public_key_pem(pem);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_ed25519_public_key_jwk() {
        // base64url encoded value
        let x = "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo";
        let result = decode_ed25519_public_key_jwk(x);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 32); // Ed25519 public key is 32 bytes
    }

    #[test]
    fn test_decode_ed25519_public_key_jwk_invalid() {
        let invalid = "not-valid-base64url!!!";
        let result = decode_ed25519_public_key_jwk(invalid);
        assert!(result.is_err());
    }
}
