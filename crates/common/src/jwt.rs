//! JWT utilities shared across Dark Tower services.
//!
//! This module provides common JWT validation utilities including:
//! - Size limits for DoS prevention
//! - Clock skew constants for iat validation
//! - Key ID extraction from JWT headers
//! - iat validation logic
//! - Service token claims structure
//! - User token claims structure (ADR-0020)
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

/// User token claims structure per ADR-0020.
///
/// Used for user authentication tokens issued by the Auth Controller.
/// Contains organization membership and role information.
///
/// # Fields
///
/// - `sub`: Subject (user UUID)
/// - `org_id`: Organization ID the user belongs to
/// - `email`: User's email address
/// - `roles`: User roles (e.g., \["user"\], \["user", "admin"\])
/// - `iat`: Issued-at timestamp (Unix epoch seconds)
/// - `exp`: Expiration timestamp (Unix epoch seconds)
/// - `jti`: Unique token identifier for revocation
///
/// # Security
///
/// The `sub`, `email`, and `jti` fields are redacted in Debug output
/// to prevent accidental logging of personally identifiable information.
#[derive(Clone, Serialize, Deserialize)]
pub struct UserClaims {
    /// Subject (user UUID) - redacted in Debug output.
    pub sub: String,
    /// Organization ID the user belongs to.
    pub org_id: String,
    /// User's email address - redacted in Debug output.
    pub email: String,
    /// User roles (e.g., \["user"\], \["user", "admin"\]).
    pub roles: Vec<String>,
    /// Issued-at timestamp (Unix epoch seconds).
    pub iat: i64,
    /// Expiration timestamp (Unix epoch seconds).
    pub exp: i64,
    /// Unique token identifier for revocation - redacted in Debug output.
    pub jti: String,
}

impl fmt::Debug for UserClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserClaims")
            .field("sub", &"[REDACTED]")
            .field("org_id", &self.org_id)
            .field("email", &"[REDACTED]")
            .field("roles", &self.roles)
            .field("iat", &self.iat)
            .field("exp", &self.exp)
            .field("jti", &"[REDACTED]")
            .finish()
    }
}

// =============================================================================
// Meeting Token Enums
// =============================================================================

/// Participant type for authenticated meeting participants.
///
/// Used in `MeetingTokenClaims` to distinguish same-org members from
/// cross-org external participants. Guest participants use a separate
/// `GuestTokenClaims` type with a fixed `"guest"` string.
///
/// Serializes to lowercase strings for JWT encoding (e.g., `"member"`, `"external"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParticipantType {
    /// Same-org participant.
    Member,
    /// Cross-org participant (invited from another organization).
    External,
}

/// Role for authenticated meeting participants.
///
/// Used in `MeetingTokenClaims` to distinguish hosts from regular participants.
/// Guest participants use a separate `GuestTokenClaims` type with a fixed
/// `"guest"` string.
///
/// Serializes to lowercase strings for JWT encoding (e.g., `"host"`, `"participant"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingRole {
    /// Meeting host with administrative capabilities.
    Host,
    /// Regular meeting participant.
    Participant,
}

// =============================================================================
// Meeting Token Claims
// =============================================================================

/// Meeting token claims structure per ADR-0020.
///
/// Used for authenticated participant tokens issued by the Auth Controller
/// via GC request. MC deserializes these from validated JWTs.
///
/// # Fields
///
/// - `sub`: Subject (participant UUID)
/// - `token_type`: Token discriminator (must be `"meeting"`)
/// - `meeting_id`: Meeting UUID
/// - `home_org_id`: Participant's home organization (None for same-org joins)
/// - `meeting_org_id`: Meeting's organization UUID
/// - `participant_type`: Member or External
/// - `role`: Host or Participant
/// - `capabilities`: Granted capabilities (e.g., `["video", "audio", "screen_share"]`)
/// - `iat`: Issued-at timestamp (Unix epoch seconds)
/// - `exp`: Expiration timestamp (Unix epoch seconds)
/// - `jti`: Unique token identifier for revocation
///
/// # Security
///
/// The `sub` and `jti` fields are redacted in Debug output to prevent
/// accidental logging of personally identifiable information.
#[derive(Clone, Serialize, Deserialize)]
pub struct MeetingTokenClaims {
    /// Subject (participant UUID) - redacted in Debug output.
    pub sub: String,
    /// Token type discriminator (must be `"meeting"`).
    pub token_type: String,
    /// Meeting UUID.
    pub meeting_id: String,
    /// Participant's home organization (None for same-org joins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_org_id: Option<String>,
    /// Meeting's organization UUID.
    pub meeting_org_id: String,
    /// Participant type (member or external).
    pub participant_type: ParticipantType,
    /// Meeting role (host or participant).
    pub role: MeetingRole,
    /// Granted capabilities (e.g., `["video", "audio", "screen_share"]`).
    pub capabilities: Vec<String>,
    /// Issued-at timestamp (Unix epoch seconds).
    pub iat: i64,
    /// Expiration timestamp (Unix epoch seconds).
    pub exp: i64,
    /// Unique token identifier for revocation - redacted in Debug output.
    pub jti: String,
}

impl fmt::Debug for MeetingTokenClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeetingTokenClaims")
            .field("sub", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("meeting_id", &self.meeting_id)
            .field("home_org_id", &self.home_org_id)
            .field("meeting_org_id", &self.meeting_org_id)
            .field("participant_type", &self.participant_type)
            .field("role", &self.role)
            .field("capabilities", &self.capabilities)
            .field("iat", &self.iat)
            .field("exp", &self.exp)
            .field("jti", &"[REDACTED]")
            .finish()
    }
}

/// Guest token claims structure per ADR-0020.
///
/// Used for unauthenticated guest tokens issued by the Auth Controller
/// via GC request. MC deserializes these from validated JWTs.
///
/// # Fields
///
/// - `sub`: Subject (guest UUID, CSPRNG-generated)
/// - `token_type`: Token discriminator (must be `"guest"`)
/// - `meeting_id`: Meeting UUID
/// - `meeting_org_id`: Meeting's organization UUID
/// - `participant_type`: Always `"guest"` per ADR-0020
/// - `role`: Always `"guest"` per ADR-0020
/// - `display_name`: Guest's self-reported display name
/// - `waiting_room`: Whether guest must wait for host approval
/// - `capabilities`: Granted capabilities (e.g., `["video", "audio"]`)
/// - `iat`: Issued-at timestamp (Unix epoch seconds)
/// - `exp`: Expiration timestamp (Unix epoch seconds)
/// - `jti`: Unique token identifier for revocation
///
/// # Security
///
/// The `sub`, `display_name`, and `jti` fields are redacted in Debug output
/// to prevent accidental logging of personally identifiable information.
#[derive(Clone, Serialize, Deserialize)]
pub struct GuestTokenClaims {
    /// Subject (guest UUID) - redacted in Debug output.
    pub sub: String,
    /// Token type discriminator (must be `"guest"`).
    pub token_type: String,
    /// Meeting UUID.
    pub meeting_id: String,
    /// Meeting's organization UUID.
    pub meeting_org_id: String,
    /// Participant type (always `"guest"` per ADR-0020).
    pub participant_type: String,
    /// Role (always `"guest"` per ADR-0020).
    pub role: String,
    /// Guest's self-reported display name - redacted in Debug output.
    pub display_name: String,
    /// Whether guest must wait for host approval before joining.
    pub waiting_room: bool,
    /// Granted capabilities (e.g., `["video", "audio"]`).
    pub capabilities: Vec<String>,
    /// Issued-at timestamp (Unix epoch seconds).
    pub iat: i64,
    /// Expiration timestamp (Unix epoch seconds).
    pub exp: i64,
    /// Unique token identifier for revocation - redacted in Debug output.
    pub jti: String,
}

impl GuestTokenClaims {
    /// Validate that the guest token claims contain expected fixed values.
    ///
    /// Guest tokens must have `token_type`, `participant_type`, and `role`
    /// all set to `"guest"`. This prevents a tampered or malformed token
    /// from carrying elevated privileges (e.g., `role: "host"`).
    ///
    /// # Errors
    ///
    /// Returns an error message if any of the fixed fields have unexpected values.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.token_type != "guest" {
            return Err("Guest token must have token_type 'guest'");
        }
        if self.participant_type != "guest" {
            return Err("Guest token must have participant_type 'guest'");
        }
        if self.role != "guest" {
            return Err("Guest token must have role 'guest'");
        }
        Ok(())
    }
}

impl fmt::Debug for GuestTokenClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GuestTokenClaims")
            .field("sub", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("meeting_id", &self.meeting_id)
            .field("meeting_org_id", &self.meeting_org_id)
            .field("participant_type", &self.participant_type)
            .field("role", &self.role)
            .field("display_name", &"[REDACTED]")
            .field("waiting_room", &self.waiting_room)
            .field("capabilities", &self.capabilities)
            .field("iat", &self.iat)
            .field("exp", &self.exp)
            .field("jti", &"[REDACTED]")
            .finish()
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
    // UserClaims Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_user_claims_debug_redacts_sensitive_fields() {
        let claims = UserClaims {
            sub: "user-secret-id".to_string(),
            org_id: "org-456".to_string(),
            email: "secret@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_571_490,
            jti: "secret-jti-token".to_string(),
        };

        let debug_str = format!("{claims:?}");

        // Sensitive fields must be redacted
        assert!(
            !debug_str.contains("user-secret-id"),
            "sub should be redacted"
        );
        assert!(
            !debug_str.contains("secret@example.com"),
            "email should be redacted"
        );
        assert!(
            !debug_str.contains("secret-jti-token"),
            "jti should be redacted"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Should contain [REDACTED] markers"
        );

        // Non-sensitive fields should be visible
        assert!(debug_str.contains("org-456"), "org_id should be visible");
        assert!(debug_str.contains("user"), "roles should be visible");
        assert!(debug_str.contains("admin"), "roles should be visible");
        assert!(debug_str.contains("1234567890"), "iat should be visible");
        assert!(debug_str.contains("1234571490"), "exp should be visible");
    }

    #[test]
    fn test_user_claims_serialization_roundtrip() {
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_571_490,
            jti: "jti-789".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: UserClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.org_id, claims.org_id);
        assert_eq!(deserialized.email, claims.email);
        assert_eq!(deserialized.roles, claims.roles);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.jti, claims.jti);
    }

    #[test]
    fn test_user_claims_clone() {
        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_571_490,
            jti: "jti-789".to_string(),
        };

        let cloned = claims.clone();

        assert_eq!(cloned.sub, claims.sub);
        assert_eq!(cloned.org_id, claims.org_id);
        assert_eq!(cloned.email, claims.email);
        assert_eq!(cloned.roles, claims.roles);
        assert_eq!(cloned.iat, claims.iat);
        assert_eq!(cloned.exp, claims.exp);
        assert_eq!(cloned.jti, claims.jti);
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

    // -------------------------------------------------------------------------
    // ParticipantType / MeetingRole Enum Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_participant_type_serialization() {
        let member_json = serde_json::to_string(&ParticipantType::Member).unwrap();
        assert_eq!(member_json, "\"member\"");

        let external_json = serde_json::to_string(&ParticipantType::External).unwrap();
        assert_eq!(external_json, "\"external\"");
    }

    #[test]
    fn test_participant_type_deserialization() {
        let member: ParticipantType = serde_json::from_str("\"member\"").unwrap();
        assert_eq!(member, ParticipantType::Member);

        let external: ParticipantType = serde_json::from_str("\"external\"").unwrap();
        assert_eq!(external, ParticipantType::External);
    }

    #[test]
    fn test_meeting_role_serialization() {
        let host_json = serde_json::to_string(&MeetingRole::Host).unwrap();
        assert_eq!(host_json, "\"host\"");

        let participant_json = serde_json::to_string(&MeetingRole::Participant).unwrap();
        assert_eq!(participant_json, "\"participant\"");
    }

    #[test]
    fn test_meeting_role_deserialization() {
        let host: MeetingRole = serde_json::from_str("\"host\"").unwrap();
        assert_eq!(host, MeetingRole::Host);

        let participant: MeetingRole = serde_json::from_str("\"participant\"").unwrap();
        assert_eq!(participant, MeetingRole::Participant);
    }

    #[test]
    fn test_participant_type_rejects_invalid_value() {
        let result = serde_json::from_str::<ParticipantType>("\"admin\"");
        assert!(
            result.is_err(),
            "Invalid participant type should be rejected"
        );

        let result = serde_json::from_str::<ParticipantType>("\"MEMBER\"");
        assert!(result.is_err(), "Uppercase variant should be rejected");
    }

    #[test]
    fn test_meeting_role_rejects_invalid_value() {
        let result = serde_json::from_str::<MeetingRole>("\"moderator\"");
        assert!(result.is_err(), "Invalid meeting role should be rejected");

        let result = serde_json::from_str::<MeetingRole>("\"HOST\"");
        assert!(result.is_err(), "Uppercase variant should be rejected");
    }

    // -------------------------------------------------------------------------
    // MeetingTokenClaims Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_meeting_token_claims_serialization_roundtrip() {
        let claims = MeetingTokenClaims {
            sub: "user-uuid-123".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid-456".to_string(),
            home_org_id: Some("home-org-789".to_string()),
            meeting_org_id: "meeting-org-012".to_string(),
            participant_type: ParticipantType::External,
            role: MeetingRole::Participant,
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-unique-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: MeetingTokenClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.token_type, "meeting");
        assert_eq!(deserialized.meeting_id, claims.meeting_id);
        assert_eq!(deserialized.home_org_id, Some("home-org-789".to_string()));
        assert_eq!(deserialized.meeting_org_id, claims.meeting_org_id);
        assert_eq!(deserialized.participant_type, ParticipantType::External);
        assert_eq!(deserialized.role, MeetingRole::Participant);
        assert_eq!(deserialized.capabilities, vec!["video", "audio"]);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.jti, claims.jti);
    }

    #[test]
    fn test_meeting_token_claims_same_org_omits_home_org_id() {
        let claims = MeetingTokenClaims {
            sub: "user-uuid".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            home_org_id: None,
            meeting_org_id: "org-uuid".to_string(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Host,
            capabilities: vec![
                "video".to_string(),
                "audio".to_string(),
                "screen_share".to_string(),
            ],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(
            !json.contains("home_org_id"),
            "home_org_id should be omitted when None"
        );

        // Deserialize JSON without home_org_id field (from own serialization)
        let deserialized: MeetingTokenClaims = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.home_org_id, None);
    }

    #[test]
    fn test_meeting_token_claims_deserialize_missing_home_org_id() {
        // Simulate AC-issued JSON that completely omits home_org_id for same-org joins
        let json = r#"{"sub":"u","token_type":"meeting","meeting_id":"m","meeting_org_id":"o","participant_type":"member","role":"host","capabilities":[],"iat":0,"exp":0,"jti":"j"}"#;
        let claims: MeetingTokenClaims = serde_json::from_str(json).unwrap();
        assert_eq!(claims.home_org_id, None);
    }

    #[test]
    fn test_meeting_token_claims_empty_capabilities() {
        let claims = MeetingTokenClaims {
            sub: "user-uuid".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            home_org_id: None,
            meeting_org_id: "org-uuid".to_string(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Host,
            capabilities: vec![],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: MeetingTokenClaims = serde_json::from_str(&json).unwrap();
        assert!(deserialized.capabilities.is_empty());
    }

    #[test]
    fn test_guest_token_claims_empty_capabilities() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: true,
            capabilities: vec![],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: GuestTokenClaims = serde_json::from_str(&json).unwrap();
        assert!(deserialized.capabilities.is_empty());
    }

    #[test]
    fn test_meeting_token_claims_debug_redacts_pii() {
        let claims = MeetingTokenClaims {
            sub: "secret-user-uuid".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid-456".to_string(),
            home_org_id: Some("home-org-789".to_string()),
            meeting_org_id: "meeting-org-012".to_string(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Host,
            capabilities: vec!["video".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "secret-jti-value".to_string(),
        };

        let debug_str = format!("{claims:?}");

        // Sensitive fields must be redacted
        assert!(
            !debug_str.contains("secret-user-uuid"),
            "sub should be redacted"
        );
        assert!(
            !debug_str.contains("secret-jti-value"),
            "jti should be redacted"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Should contain [REDACTED] markers"
        );

        // Non-sensitive fields should be visible
        assert!(
            debug_str.contains("meeting"),
            "token_type should be visible"
        );
        assert!(
            debug_str.contains("meeting-uuid-456"),
            "meeting_id should be visible"
        );
        assert!(
            debug_str.contains("home-org-789"),
            "home_org_id should be visible"
        );
        assert!(
            debug_str.contains("meeting-org-012"),
            "meeting_org_id should be visible"
        );
        assert!(
            debug_str.contains("Member"),
            "participant_type should be visible"
        );
        assert!(debug_str.contains("Host"), "role should be visible");
        assert!(
            debug_str.contains("video"),
            "capabilities should be visible"
        );
        assert!(debug_str.contains("1234567890"), "iat should be visible");
        assert!(debug_str.contains("1234568790"), "exp should be visible");
    }

    #[test]
    fn test_meeting_token_claims_clone() {
        let claims = MeetingTokenClaims {
            sub: "user-uuid".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            home_org_id: None,
            meeting_org_id: "org-uuid".to_string(),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Host,
            capabilities: vec!["video".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let cloned = claims.clone();
        assert_eq!(cloned.sub, claims.sub);
        assert_eq!(cloned.meeting_id, claims.meeting_id);
        assert_eq!(cloned.participant_type, ParticipantType::Member);
        assert_eq!(cloned.role, MeetingRole::Host);
    }

    // -------------------------------------------------------------------------
    // GuestTokenClaims Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_guest_token_claims_serialization_roundtrip() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid-123".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid-456".to_string(),
            meeting_org_id: "org-uuid-789".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string(), "audio".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-unique-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: GuestTokenClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.token_type, "guest");
        assert_eq!(deserialized.meeting_id, claims.meeting_id);
        assert_eq!(deserialized.meeting_org_id, claims.meeting_org_id);
        assert_eq!(deserialized.participant_type, "guest");
        assert_eq!(deserialized.role, "guest");
        assert_eq!(deserialized.display_name, "Alice");
        assert!(deserialized.waiting_room);
        assert_eq!(deserialized.capabilities, vec!["video", "audio"]);
        assert_eq!(deserialized.iat, claims.iat);
        assert_eq!(deserialized.exp, claims.exp);
        assert_eq!(deserialized.jti, claims.jti);
    }

    #[test]
    fn test_guest_token_claims_waiting_room_false() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Bob".to_string(),
            waiting_room: false,
            capabilities: vec!["audio".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: GuestTokenClaims = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.waiting_room);
    }

    #[test]
    fn test_guest_token_claims_debug_redacts_pii() {
        let claims = GuestTokenClaims {
            sub: "secret-guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid-456".to_string(),
            meeting_org_id: "org-uuid-789".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Secret Alice Name".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "secret-jti-value".to_string(),
        };

        let debug_str = format!("{claims:?}");

        // Sensitive fields must be redacted
        assert!(
            !debug_str.contains("secret-guest-uuid"),
            "sub should be redacted"
        );
        assert!(
            !debug_str.contains("Secret Alice Name"),
            "display_name should be redacted"
        );
        assert!(
            !debug_str.contains("secret-jti-value"),
            "jti should be redacted"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Should contain [REDACTED] markers"
        );

        // Non-sensitive fields should be visible
        assert!(debug_str.contains("guest"), "token_type should be visible");
        assert!(
            debug_str.contains("meeting-uuid-456"),
            "meeting_id should be visible"
        );
        assert!(
            debug_str.contains("org-uuid-789"),
            "meeting_org_id should be visible"
        );
        assert!(debug_str.contains("true"), "waiting_room should be visible");
        assert!(
            debug_str.contains("video"),
            "capabilities should be visible"
        );
        assert!(debug_str.contains("1234567890"), "iat should be visible");
        assert!(debug_str.contains("1234568790"), "exp should be visible");
    }

    #[test]
    fn test_guest_token_claims_clone() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let cloned = claims.clone();
        assert_eq!(cloned.sub, claims.sub);
        assert_eq!(cloned.display_name, claims.display_name);
        assert!(cloned.waiting_room);
    }

    #[test]
    fn test_guest_token_claims_validate_success() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: true,
            capabilities: vec!["video".to_string()],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        assert!(claims.validate().is_ok());
    }

    #[test]
    fn test_guest_token_claims_validate_rejects_wrong_token_type() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "meeting".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: false,
            capabilities: vec![],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let result = claims.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Guest token must have token_type 'guest'"
        );
    }

    #[test]
    fn test_guest_token_claims_validate_rejects_wrong_participant_type() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "member".to_string(),
            role: "guest".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: false,
            capabilities: vec![],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let result = claims.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Guest token must have participant_type 'guest'"
        );
    }

    #[test]
    fn test_guest_token_claims_validate_rejects_wrong_role() {
        let claims = GuestTokenClaims {
            sub: "guest-uuid".to_string(),
            token_type: "guest".to_string(),
            meeting_id: "meeting-uuid".to_string(),
            meeting_org_id: "org-uuid".to_string(),
            participant_type: "guest".to_string(),
            role: "host".to_string(),
            display_name: "Alice".to_string(),
            waiting_room: false,
            capabilities: vec![],
            iat: 1_234_567_890,
            exp: 1_234_568_790,
            jti: "jti-id".to_string(),
        };

        let result = claims.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Guest token must have role 'guest'");
    }
}
