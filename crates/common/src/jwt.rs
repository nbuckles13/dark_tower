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
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::instrument;

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
pub enum JwtError {
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

    /// Token signature verification failed (wrong key type, algorithm, or signature).
    #[error("The access token is invalid or expired")]
    InvalidSignature,

    /// Signing key not found in JWKS.
    #[error("The access token is invalid or expired")]
    KeyNotFound,

    /// JWKS endpoint or auth service is unavailable.
    #[error("Authentication service unavailable")]
    ServiceUnavailable(String),
}

// =============================================================================
// HasIat Trait
// =============================================================================

/// Trait for claims types that contain an `iat` (issued-at) field.
///
/// Required by [`JwtValidator::validate`] so that iat validation can be
/// performed without re-parsing the JWT payload.
pub trait HasIat {
    /// Return the issued-at timestamp (Unix epoch seconds).
    fn iat(&self) -> i64;
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
// HasIat Implementations
// =============================================================================

impl HasIat for ServiceClaims {
    fn iat(&self) -> i64 {
        self.iat
    }
}

impl HasIat for UserClaims {
    fn iat(&self) -> i64 {
        self.iat
    }
}

impl HasIat for MeetingTokenClaims {
    fn iat(&self) -> i64 {
        self.iat
    }
}

impl HasIat for GuestTokenClaims {
    fn iat(&self) -> i64 {
        self.iat
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
/// Returns `JwtError` variants:
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
pub fn extract_kid(token: &str) -> Result<String, JwtError> {
    // Check token size first (DoS prevention)
    if token.len() > MAX_JWT_SIZE_BYTES {
        tracing::debug!(
            target: "common.jwt",
            token_size = token.len(),
            max_size = MAX_JWT_SIZE_BYTES,
            "Token rejected: size exceeds maximum allowed"
        );
        return Err(JwtError::TokenTooLarge);
    }

    // JWT format: header.payload.signature
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        tracing::debug!(
            target: "common.jwt",
            parts = parts.len(),
            "Token rejected: invalid JWT format"
        );
        return Err(JwtError::MalformedToken);
    }

    // Decode the header (first part) - safe indexing since we verified length above
    let header_part = parts.first().ok_or(JwtError::MalformedToken)?;
    let header_bytes = URL_SAFE_NO_PAD.decode(header_part).map_err(|e| {
        tracing::debug!(target: "common.jwt", error = %e, "Failed to decode JWT header base64");
        JwtError::MalformedToken
    })?;

    let header: serde_json::Value = serde_json::from_slice(&header_bytes).map_err(|e| {
        tracing::debug!(target: "common.jwt", error = %e, "Failed to parse JWT header JSON");
        JwtError::MalformedToken
    })?;

    // Extract kid as string, rejecting empty values for defense-in-depth
    let kid = header
        .get("kid")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .ok_or(JwtError::MissingKid)?;

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
/// Returns `JwtError::IatTooFarInFuture` if the iat timestamp is more than
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
pub fn validate_iat(iat: i64, clock_skew: Duration) -> Result<(), JwtError> {
    let now = chrono::Utc::now().timestamp();
    validate_iat_at(iat, clock_skew, now)
}

/// Deterministic `iat` validation against an explicit `now` timestamp.
///
/// Prefer [`validate_iat`] in production code. This variant exists so that
/// boundary conditions can be unit-tested without wall-clock dependence.
pub(crate) fn validate_iat_at(iat: i64, clock_skew: Duration, now: i64) -> Result<(), JwtError> {
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
        return Err(JwtError::IatTooFarInFuture);
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
// JWKS Types
// =============================================================================

/// Default cache TTL in seconds (5 minutes).
const DEFAULT_CACHE_TTL_SECONDS: u64 = 300;

/// JSON Web Key from JWKS endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key type (always "OKP" for Ed25519).
    pub kty: String,

    /// Key ID - used to select the correct key for verification.
    pub kid: String,

    /// Curve name (always "Ed25519" for `EdDSA`).
    #[serde(default)]
    pub crv: Option<String>,

    /// Public key value (base64url encoded).
    #[serde(default)]
    pub x: Option<String>,

    /// Algorithm (should be "`EdDSA`").
    #[serde(default)]
    pub alg: Option<String>,

    /// Key use (should be "sig" for signing).
    #[serde(default, rename = "use")]
    pub key_use: Option<String>,
}

/// JWKS response from Auth Controller.
#[derive(Debug, Clone, Deserialize)]
pub struct JwksResponse {
    /// List of JSON Web Keys.
    pub keys: Vec<Jwk>,
}

/// Cached JWKS data with expiry time.
struct CachedJwks {
    /// Map of key ID to JWK.
    keys: HashMap<String, Jwk>,

    /// When this cache entry expires.
    expires_at: Instant,
}

// =============================================================================
// JWKS Client
// =============================================================================

/// JWKS client for fetching and caching public keys.
///
/// Thread-safe client that fetches JWKS from Auth Controller and caches
/// the keys with configurable TTL.
pub struct JwksClient {
    /// URL to the JWKS endpoint.
    jwks_url: String,

    /// HTTP client for fetching JWKS.
    http_client: reqwest::Client,

    /// Cached JWKS data.
    cache: Arc<RwLock<Option<CachedJwks>>>,

    /// Cache TTL duration.
    cache_ttl: Duration,
}

impl JwksClient {
    /// Create a new JWKS client.
    ///
    /// # Arguments
    ///
    /// * `jwks_url` - URL to the Auth Controller's JWKS endpoint
    ///
    /// # Errors
    ///
    /// Returns `JwtError::ServiceUnavailable` if the HTTP client cannot be built.
    pub fn new(jwks_url: String) -> Result<Self, JwtError> {
        Self::with_ttl(jwks_url, Duration::from_secs(DEFAULT_CACHE_TTL_SECONDS))
    }

    /// Create a new JWKS client with custom cache TTL.
    ///
    /// # Arguments
    ///
    /// * `jwks_url` - URL to the Auth Controller's JWKS endpoint
    /// * `cache_ttl` - How long to cache JWKS before refreshing
    ///
    /// # Errors
    ///
    /// Returns `JwtError::ServiceUnavailable` if the HTTP client cannot be built.
    pub fn with_ttl(jwks_url: String, cache_ttl: Duration) -> Result<Self, JwtError> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                tracing::error!(target: "common.jwt.jwks", error = %e, "Failed to build HTTP client");
                JwtError::ServiceUnavailable("Failed to initialize JWKS client".to_string())
            })?;

        Ok(Self {
            jwks_url,
            http_client,
            cache: Arc::new(RwLock::new(None)),
            cache_ttl,
        })
    }

    /// Get a JWK by key ID.
    ///
    /// Returns the JWK if found, or fetches from AC if cache is expired/empty.
    ///
    /// # Arguments
    ///
    /// * `kid` - Key ID to look up
    ///
    /// # Errors
    ///
    /// Returns `JwtError::ServiceUnavailable` if JWKS cannot be fetched.
    /// Returns `JwtError::KeyNotFound` if key ID is not found.
    #[instrument(skip_all, fields(kid = %kid))]
    pub async fn get_key(&self, kid: &str) -> Result<Jwk, JwtError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.expires_at > Instant::now() {
                    if let Some(key) = cached.keys.get(kid) {
                        tracing::debug!(target: "common.jwt.jwks", kid = %kid, "JWKS cache hit");
                        return Ok(key.clone());
                    }
                    // Key not found in valid cache
                    tracing::debug!(target: "common.jwt.jwks", kid = %kid, "Key not found in JWKS cache");
                    return Err(JwtError::KeyNotFound);
                }
            }
        }

        // Cache miss or expired - fetch fresh JWKS
        self.refresh_cache().await?;

        // Try to get key from refreshed cache
        let cache = self.cache.read().await;
        if let Some(cached) = cache.as_ref() {
            if let Some(key) = cached.keys.get(kid) {
                return Ok(key.clone());
            }
        }

        // Key not found even after refresh
        tracing::warn!(target: "common.jwt.jwks", kid = %kid, "Key not found in JWKS after refresh");
        Err(JwtError::KeyNotFound)
    }

    /// Refresh the JWKS cache by fetching from Auth Controller.
    #[instrument(skip_all)]
    async fn refresh_cache(&self) -> Result<(), JwtError> {
        tracing::debug!(target: "common.jwt.jwks", url = %self.jwks_url, "Fetching JWKS from AC");

        let response = self
            .http_client
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(target: "common.jwt.jwks", error = %e, "Failed to fetch JWKS");
                JwtError::ServiceUnavailable("Authentication service unavailable".to_string())
            })?;

        if !response.status().is_success() {
            tracing::error!(
                target: "common.jwt.jwks",
                status = %response.status(),
                "JWKS endpoint returned error"
            );
            return Err(JwtError::ServiceUnavailable(
                "Authentication service unavailable".to_string(),
            ));
        }

        let jwks: JwksResponse = response.json().await.map_err(|e| {
            tracing::error!(target: "common.jwt.jwks", error = %e, "Failed to parse JWKS response");
            JwtError::ServiceUnavailable("Authentication service unavailable".to_string())
        })?;

        // Build key map
        let keys: HashMap<String, Jwk> = jwks
            .keys
            .into_iter()
            .map(|key| (key.kid.clone(), key))
            .collect();

        tracing::info!(
            target: "common.jwt.jwks",
            key_count = keys.len(),
            "JWKS cache refreshed"
        );

        // Update cache
        let mut cache = self.cache.write().await;
        *cache = Some(CachedJwks {
            keys,
            expires_at: Instant::now() + self.cache_ttl,
        });

        Ok(())
    }

    /// Force refresh the cache.
    ///
    /// Useful for manual cache invalidation.
    ///
    /// # Errors
    ///
    /// Returns `JwtError::ServiceUnavailable` if JWKS cannot be fetched.
    pub async fn force_refresh(&self) -> Result<(), JwtError> {
        self.refresh_cache().await
    }

    /// Clear the cache.
    ///
    /// Useful for testing.
    #[cfg(test)]
    #[allow(dead_code)] // Test utility for cache invalidation
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }
}

// =============================================================================
// JWT Verification
// =============================================================================

/// Verify JWT signature and extract claims.
///
/// Generic over the claims type — allows both service `Claims` and `UserClaims`
/// to be decoded from the same `EdDSA`-signed JWT. The JWK validation and signature
/// verification are claims-type-independent.
///
/// # Errors
///
/// Returns `JwtError::InvalidSignature` if the JWK key type is not OKP, the algorithm
/// is not `EdDSA`, the public key is missing or invalid, or signature verification fails.
pub fn verify_token<T: DeserializeOwned>(token: &str, jwk: &Jwk) -> Result<T, JwtError> {
    // Validate JWK is EdDSA key
    if jwk.kty != "OKP" {
        tracing::warn!(target: "common.jwt", kty = %jwk.kty, "Unexpected JWK key type");
        return Err(JwtError::InvalidSignature);
    }
    if let Some(alg) = &jwk.alg {
        if alg != "EdDSA" {
            tracing::warn!(target: "common.jwt", alg = %alg, "Unexpected JWK algorithm");
            return Err(JwtError::InvalidSignature);
        }
    }

    // Get public key bytes from JWK
    let public_key_b64 = jwk.x.as_ref().ok_or_else(|| {
        tracing::error!(target: "common.jwt", kid = %jwk.kid, "JWK missing x field");
        JwtError::InvalidSignature
    })?;

    // Decode public key from base64url using common utility
    let public_key_bytes = decode_ed25519_public_key_jwk(public_key_b64).map_err(|e| {
        tracing::error!(target: "common.jwt", error = %e, "Invalid public key encoding");
        JwtError::InvalidSignature
    })?;

    // Create decoding key
    let decoding_key = DecodingKey::from_ed_der(&public_key_bytes);

    // Configure validation
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_exp = true;
    // Don't validate aud - we'll check scopes instead

    // Decode and verify
    let token_data = decode::<T>(token, &decoding_key, &validation).map_err(|e| {
        tracing::debug!(target: "common.jwt", error = %e, "Token verification failed");
        JwtError::InvalidSignature
    })?;

    Ok(token_data.claims)
}

// =============================================================================
// JWT Validator
// =============================================================================

/// JWT validator using JWKS from Auth Controller.
///
/// Provides a full validation pipeline: size check, kid extraction, JWKS lookup,
/// `EdDSA` signature verification, and iat validation. Generic over claims types.
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
    #[must_use]
    #[expect(
        clippy::cast_possible_wrap,
        reason = "MAX_CLOCK_SKEW (600s) is well within i64 range"
    )]
    pub fn new(jwks_client: Arc<JwksClient>, clock_skew_seconds: i64) -> Self {
        let clamped = clock_skew_seconds
            .max(0)
            .min(MAX_CLOCK_SKEW.as_secs() as i64);
        Self {
            jwks_client,
            clock_skew_seconds: clamped,
        }
    }

    /// Validate a JWT and return the claims.
    ///
    /// # Security Checks
    ///
    /// 1. Size check - reject tokens > 8KB before parsing
    /// 2. Extract kid from header to find the correct key
    /// 3. Fetch public key from JWKS
    /// 4. Verify `EdDSA` signature
    /// 5. Validate exp claim (reject expired tokens)
    /// 6. Validate iat claim with clock skew tolerance
    ///
    /// # Arguments
    ///
    /// * `token` - The JWT string to validate
    ///
    /// # Errors
    ///
    /// Returns `JwtError` for all validation failures.
    #[expect(
        clippy::cast_sign_loss,
        reason = "clock_skew_seconds clamped non-negative at construction"
    )]
    #[instrument(skip_all)]
    pub async fn validate<T: DeserializeOwned + HasIat>(&self, token: &str) -> Result<T, JwtError> {
        // 1. Extract kid from JWT header (includes size check)
        let kid = extract_kid(token).map_err(|e| {
            tracing::debug!(target: "common.jwt", error = ?e, "Token kid extraction failed");
            e
        })?;

        // 2. Fetch public key from JWKS
        let jwk = self.jwks_client.get_key(&kid).await?;

        // 3. Verify signature and extract claims
        let claims = verify_token::<T>(token, &jwk)?;

        // 4. Validate iat claim with clock skew tolerance
        validate_iat(
            claims.iat(),
            Duration::from_secs(self.clock_skew_seconds as u64),
        )?;

        tracing::debug!(target: "common.jwt", "Token validated successfully");
        Ok(claims)
    }
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
        assert!(matches!(result, Err(JwtError::MissingKid)));
    }

    #[test]
    fn test_extract_kid_malformed_token() {
        // Not a valid JWT format
        let result = extract_kid("not-a-jwt");
        assert!(matches!(result, Err(JwtError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_empty_token() {
        // Empty string should be rejected as malformed
        let result = extract_kid("");
        assert!(matches!(result, Err(JwtError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_invalid_base64() {
        let result = extract_kid("!!!invalid!!!.payload.signature");
        assert!(matches!(result, Err(JwtError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_invalid_json() {
        let header_b64 = URL_SAFE_NO_PAD.encode("not-json");
        let token = format!("{header_b64}.payload.signature");

        let result = extract_kid(&token);
        assert!(matches!(result, Err(JwtError::MalformedToken)));
    }

    #[test]
    fn test_extract_kid_oversized_token() {
        // Create a token larger than MAX_JWT_SIZE_BYTES
        let oversized = "a".repeat(MAX_JWT_SIZE_BYTES + 1);
        let result = extract_kid(&oversized);
        assert!(matches!(result, Err(JwtError::TokenTooLarge)));
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
        assert!(matches!(result, Err(JwtError::MissingKid)));
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
        assert!(matches!(result, Err(JwtError::IatTooFarInFuture)));
    }

    #[test]
    fn test_validate_iat_far_future() {
        let far_future = chrono::Utc::now().timestamp() + 86400; // 1 day in future
        let result = validate_iat(far_future, DEFAULT_CLOCK_SKEW);
        assert!(matches!(result, Err(JwtError::IatTooFarInFuture)));
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
            Err(JwtError::IatTooFarInFuture)
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
            Err(JwtError::IatTooFarInFuture)
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

    // -------------------------------------------------------------------------
    // JwtError Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_jwt_error_display_messages_are_uniform() {
        // All validation errors should show the same generic message
        let errors = vec![
            JwtError::TokenTooLarge,
            JwtError::MalformedToken,
            JwtError::MissingKid,
            JwtError::IatTooFarInFuture,
            JwtError::InvalidSignature,
            JwtError::KeyNotFound,
        ];
        for err in &errors {
            assert_eq!(
                format!("{err}"),
                "The access token is invalid or expired",
                "All validation errors should have uniform message, got variant: {err:?}"
            );
        }
    }

    #[test]
    fn test_jwt_error_service_unavailable_display() {
        let err = JwtError::ServiceUnavailable("auth down".to_string());
        assert_eq!(format!("{err}"), "Authentication service unavailable");
    }

    #[test]
    fn test_jwt_error_clone_and_eq() {
        let err = JwtError::InvalidSignature;
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    // -------------------------------------------------------------------------
    // Jwk / JwksResponse Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_jwk_deserialization() {
        let json = r#"{
            "kty": "OKP",
            "kid": "test-key-01",
            "crv": "Ed25519",
            "x": "dGVzdC1wdWJsaWMta2V5LWRhdGE",
            "alg": "EdDSA",
            "use": "sig"
        }"#;

        let jwk: Jwk = serde_json::from_str(json).unwrap();

        assert_eq!(jwk.kty, "OKP");
        assert_eq!(jwk.kid, "test-key-01");
        assert_eq!(jwk.crv, Some("Ed25519".to_string()));
        assert_eq!(jwk.x, Some("dGVzdC1wdWJsaWMta2V5LWRhdGE".to_string()));
        assert_eq!(jwk.alg, Some("EdDSA".to_string()));
        assert_eq!(jwk.key_use, Some("sig".to_string()));
    }

    #[test]
    fn test_jwk_deserialization_minimal() {
        let json = r#"{
            "kty": "OKP",
            "kid": "test-key-02"
        }"#;

        let jwk: Jwk = serde_json::from_str(json).unwrap();

        assert_eq!(jwk.kty, "OKP");
        assert_eq!(jwk.kid, "test-key-02");
        assert!(jwk.crv.is_none());
        assert!(jwk.x.is_none());
        assert!(jwk.alg.is_none());
        assert!(jwk.key_use.is_none());
    }

    #[test]
    fn test_jwks_response_deserialization() {
        let json = r#"{
            "keys": [
                {"kty": "OKP", "kid": "key-1"},
                {"kty": "OKP", "kid": "key-2"}
            ]
        }"#;

        let jwks: JwksResponse = serde_json::from_str(json).unwrap();

        assert_eq!(jwks.keys.len(), 2);
        assert_eq!(jwks.keys.first().unwrap().kid, "key-1");
        assert_eq!(jwks.keys.get(1).unwrap().kid, "key-2");
    }

    // -------------------------------------------------------------------------
    // JwksClient Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_jwks_client_creation() {
        let client =
            JwksClient::new("http://localhost:8082/.well-known/jwks.json".to_string()).unwrap();
        assert_eq!(
            client.jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
    }

    #[test]
    fn test_jwks_client_custom_ttl() {
        let client = JwksClient::with_ttl(
            "http://localhost:8082/.well-known/jwks.json".to_string(),
            Duration::from_secs(60),
        )
        .unwrap();
        assert_eq!(client.cache_ttl, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_jwks_get_key_success_from_fetch() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "test-key-01",
                    "crv": "Ed25519",
                    "x": "dGVzdC1wdWJsaWMta2V5LWRhdGE",
                    "alg": "EdDSA",
                    "use": "sig"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let key = client.get_key("test-key-01").await;
        assert!(key.is_ok());
        let jwk = key.unwrap();
        assert_eq!(jwk.kid, "test-key-01");
        assert_eq!(jwk.kty, "OKP");
    }

    #[tokio::test]
    async fn test_jwks_get_key_cache_hit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "cached-key",
                    "crv": "Ed25519",
                    "x": "dGVzdA",
                    "alg": "EdDSA"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(1) // Should only be called once due to caching
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        // First call - fetches from server
        let key1 = client.get_key("cached-key").await;
        assert!(key1.is_ok());

        // Second call - should hit cache
        let key2 = client.get_key("cached-key").await;
        assert!(key2.is_ok());

        assert_eq!(key1.unwrap().kid, key2.unwrap().kid);
    }

    #[tokio::test]
    async fn test_jwks_get_key_not_found_in_valid_cache() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "existing-key",
                    "crv": "Ed25519",
                    "x": "dGVzdA",
                    "alg": "EdDSA"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        // Populate cache
        let _ = client.get_key("existing-key").await;

        // Request non-existent key from valid cache
        let result = client.get_key("non-existent-key").await;
        assert!(matches!(result, Err(JwtError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_jwks_get_key_not_found_after_refresh() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "different-key",
                    "crv": "Ed25519",
                    "x": "dGVzdA",
                    "alg": "EdDSA"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let client = JwksClient::with_ttl(
            format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_millis(1),
        )
        .unwrap();

        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = client.get_key("non-existent-key").await;
        assert!(matches!(result, Err(JwtError::KeyNotFound)));
    }

    #[tokio::test]
    async fn test_jwks_refresh_cache_network_error() {
        let client =
            JwksClient::new("http://127.0.0.1:1/.well-known/jwks.json".to_string()).unwrap();

        let result = client.get_key("any-key").await;
        assert!(
            matches!(result, Err(JwtError::ServiceUnavailable(_))),
            "Expected ServiceUnavailable, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_jwks_refresh_cache_non_success_status() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let result = client.get_key("any-key").await;
        assert!(matches!(result, Err(JwtError::ServiceUnavailable(_))));
    }

    #[tokio::test]
    async fn test_jwks_refresh_cache_invalid_json() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let result = client.get_key("any-key").await;
        assert!(matches!(result, Err(JwtError::ServiceUnavailable(_))));
    }

    #[tokio::test]
    async fn test_jwks_refresh_cache_404() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let result = client.get_key("any-key").await;
        assert!(matches!(result, Err(JwtError::ServiceUnavailable(_))));
    }

    #[tokio::test]
    async fn test_jwks_force_refresh_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "force-refresh-key",
                    "crv": "Ed25519",
                    "x": "dGVzdA",
                    "alg": "EdDSA"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let result = client.force_refresh().await;
        assert!(result.is_ok());

        let key = client.get_key("force-refresh-key").await;
        assert!(key.is_ok());
    }

    #[tokio::test]
    async fn test_jwks_clear_cache() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": "clear-cache-key",
                    "crv": "Ed25519",
                    "x": "dGVzdA",
                    "alg": "EdDSA"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(2) // Should be called twice - once before clear, once after
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let _ = client.get_key("clear-cache-key").await;
        client.clear_cache().await;

        let key = client.get_key("clear-cache-key").await;
        assert!(key.is_ok());
    }

    #[tokio::test]
    async fn test_jwks_multiple_keys() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {"kty": "OKP", "kid": "key-1", "crv": "Ed25519", "x": "a2V5LTE", "alg": "EdDSA"},
                {"kty": "OKP", "kid": "key-2", "crv": "Ed25519", "x": "a2V5LTI", "alg": "EdDSA"},
                {"kty": "OKP", "kid": "key-3", "crv": "Ed25519", "x": "a2V5LTM", "alg": "EdDSA"}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap();

        let key1 = client.get_key("key-1").await.unwrap();
        assert_eq!(key1.kid, "key-1");

        let key2 = client.get_key("key-2").await.unwrap();
        assert_eq!(key2.kid, "key-2");

        let key3 = client.get_key("key-3").await.unwrap();
        assert_eq!(key3.kid, "key-3");
    }

    #[tokio::test]
    async fn test_jwks_cache_expiration_triggers_refresh() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {"kty": "OKP", "kid": "expiring-key", "crv": "Ed25519", "x": "dGVzdA", "alg": "EdDSA"}
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .expect(2) // Should be called twice due to expiration
            .mount(&mock_server)
            .await;

        let client = JwksClient::with_ttl(
            format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_millis(1),
        )
        .unwrap();

        let _ = client.get_key("expiring-key").await;

        tokio::time::sleep(Duration::from_millis(10)).await;

        let key = client.get_key("expiring-key").await;
        assert!(key.is_ok());
    }

    // -------------------------------------------------------------------------
    // verify_token Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_verify_token_rejects_non_okp_key_type() {
        let jwk = Jwk {
            kty: "RSA".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{header_b64}.{payload_b64}.fake_signature");

        let result = verify_token::<serde_json::Value>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_verify_token_rejects_non_eddsa_algorithm() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()),
            alg: Some("RS256".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890,"scope":"read"}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{header_b64}.{payload_b64}.fake_signature");

        let result = verify_token::<serde_json::Value>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_verify_token_rejects_missing_x_field() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: None,
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{header_b64}.{payload_b64}.fake_signature");

        let result = verify_token::<serde_json::Value>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_verify_token_rejects_invalid_base64_public_key() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("!!!invalid-base64!!!".to_string()),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{header_b64}.{payload_b64}.fake_signature");

        let result = verify_token::<serde_json::Value>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[test]
    fn test_verify_token_accepts_jwk_without_alg_field() {
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: "test-key".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some("dGVzdC1wdWJsaWMta2V5".to_string()),
            alg: None,
            key_use: Some("sig".to_string()),
        };

        let header = r#"{"alg":"EdDSA","typ":"JWT","kid":"test-key"}"#;
        let header_b64 = URL_SAFE_NO_PAD.encode(header.as_bytes());
        let payload = r#"{"sub":"test","exp":9999999999,"iat":1234567890}"#;
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let token = format!("{header_b64}.{payload_b64}.fake_signature");

        // This should fail at signature verification, not at JWK validation
        let result = verify_token::<serde_json::Value>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    // -------------------------------------------------------------------------
    // JwtValidator + verify_token Round-Trip Tests (Real Ed25519 Keys)
    // -------------------------------------------------------------------------

    /// Test helper: generates an Ed25519 keypair and returns (kid, pkcs8 bytes, public key bytes).
    fn generate_ed25519_keypair() -> (String, Vec<u8>, Vec<u8>) {
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let kid = "test-kid-01".to_string();
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let public_key = keypair.public_key().as_ref().to_vec();

        (kid, pkcs8.as_ref().to_vec(), public_key)
    }

    /// Sign a JWT with the given claims using the Ed25519 private key.
    fn sign_jwt<T: Serialize>(claims: &T, kid: &str, pkcs8_bytes: &[u8]) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header as JwtHeader};

        let mut header = JwtHeader::new(Algorithm::EdDSA);
        header.kid = Some(kid.to_string());

        let encoding_key = EncodingKey::from_ed_der(pkcs8_bytes);
        encode(&header, claims, &encoding_key).unwrap()
    }

    #[test]
    fn test_verify_token_roundtrip_with_real_keys() {
        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();

        let claims = ServiceClaims::new(
            "test-service".to_string(),
            chrono::Utc::now().timestamp() + 3600,
            chrono::Utc::now().timestamp(),
            "read write".to_string(),
            Some("global-controller".to_string()),
        );

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        // Create JWK from public key
        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid: kid.clone(),
            crv: Some("Ed25519".to_string()),
            x: Some(x_b64),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let result = verify_token::<ServiceClaims>(&token, &jwk);
        assert!(result.is_ok(), "verify_token should succeed: {result:?}");

        let decoded = result.unwrap();
        assert_eq!(decoded.sub, "test-service");
        assert_eq!(decoded.scope, "read write");
        assert_eq!(decoded.service_type, Some("global-controller".to_string()));
    }

    #[test]
    fn test_verify_token_roundtrip_user_claims() {
        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();

        let claims = UserClaims {
            sub: "user-123".to_string(),
            org_id: "org-456".to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-789".to_string(),
        };

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid,
            crv: Some("Ed25519".to_string()),
            x: Some(x_b64),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let result = verify_token::<UserClaims>(&token, &jwk);
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.sub, "user-123");
        assert_eq!(decoded.org_id, "org-456");
        assert_eq!(decoded.email, "user@example.com");
        assert_eq!(decoded.roles, vec!["user", "admin"]);
        assert_eq!(decoded.jti, "jti-789");
    }

    #[test]
    fn test_verify_token_rejects_wrong_key() {
        let (kid, pkcs8_bytes, _) = generate_ed25519_keypair();
        let (_, _, wrong_public_key) = generate_ed25519_keypair();

        let claims = ServiceClaims::new(
            "test".to_string(),
            chrono::Utc::now().timestamp() + 3600,
            chrono::Utc::now().timestamp(),
            "read".to_string(),
            None,
        );

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        // Use wrong public key for verification
        let x_b64 = URL_SAFE_NO_PAD.encode(&wrong_public_key);
        let jwk = Jwk {
            kty: "OKP".to_string(),
            kid,
            crv: Some("Ed25519".to_string()),
            x: Some(x_b64),
            alg: Some("EdDSA".to_string()),
            key_use: Some("sig".to_string()),
        };

        let result = verify_token::<ServiceClaims>(&token, &jwk);
        assert!(matches!(result, Err(JwtError::InvalidSignature)));
    }

    #[tokio::test]
    async fn test_jwt_validator_validate_roundtrip() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();
        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [
                {
                    "kty": "OKP",
                    "kid": kid,
                    "crv": "Ed25519",
                    "x": x_b64,
                    "alg": "EdDSA",
                    "use": "sig"
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_client = Arc::new(
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap(),
        );
        let validator = JwtValidator::new(jwks_client, 300);

        let claims = ServiceClaims::new(
            "test-service".to_string(),
            chrono::Utc::now().timestamp() + 3600,
            chrono::Utc::now().timestamp(),
            "read write".to_string(),
            None,
        );

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        let result: Result<ServiceClaims, JwtError> = validator.validate(&token).await;
        assert!(result.is_ok(), "Validation should succeed: {result:?}");

        let decoded = result.unwrap();
        assert_eq!(decoded.sub, "test-service");
        assert_eq!(decoded.scope, "read write");
    }

    #[tokio::test]
    async fn test_jwt_validator_validate_user_claims_roundtrip() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();
        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [{"kty": "OKP", "kid": kid, "crv": "Ed25519", "x": x_b64, "alg": "EdDSA", "use": "sig"}]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_client = Arc::new(
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap(),
        );
        let validator = JwtValidator::new(jwks_client, 300);

        let claims = UserClaims {
            sub: "user-abc".to_string(),
            org_id: "org-xyz".to_string(),
            email: "test@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: chrono::Utc::now().timestamp(),
            exp: chrono::Utc::now().timestamp() + 3600,
            jti: "jti-unique".to_string(),
        };

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        let result: Result<UserClaims, JwtError> = validator.validate(&token).await;
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.sub, "user-abc");
        assert_eq!(decoded.org_id, "org-xyz");
    }

    #[tokio::test]
    async fn test_jwt_validator_rejects_expired_token() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();
        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [{"kty": "OKP", "kid": kid, "crv": "Ed25519", "x": x_b64, "alg": "EdDSA", "use": "sig"}]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_client = Arc::new(
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap(),
        );
        let validator = JwtValidator::new(jwks_client, 300);

        // Create an expired token (exp in the past)
        let claims = ServiceClaims::new(
            "test".to_string(),
            chrono::Utc::now().timestamp() - 3600, // expired 1 hour ago
            chrono::Utc::now().timestamp() - 7200,
            "read".to_string(),
            None,
        );

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        let result: Result<ServiceClaims, JwtError> = validator.validate(&token).await;
        assert!(
            matches!(result, Err(JwtError::InvalidSignature)),
            "Expired token should be rejected: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_jwt_validator_rejects_future_iat() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let (kid, pkcs8_bytes, public_key_bytes) = generate_ed25519_keypair();
        let x_b64 = URL_SAFE_NO_PAD.encode(&public_key_bytes);

        let mock_server = MockServer::start().await;

        let jwks_response = serde_json::json!({
            "keys": [{"kty": "OKP", "kid": kid, "crv": "Ed25519", "x": x_b64, "alg": "EdDSA", "use": "sig"}]
        });

        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_response))
            .mount(&mock_server)
            .await;

        let jwks_client = Arc::new(
            JwksClient::new(format!("{}/.well-known/jwks.json", mock_server.uri())).unwrap(),
        );
        // Use very small clock skew (1 second) to easily test iat rejection
        let validator = JwtValidator::new(jwks_client, 1);

        // Create token with iat far in the future
        let claims = ServiceClaims::new(
            "test".to_string(),
            chrono::Utc::now().timestamp() + 7200,
            chrono::Utc::now().timestamp() + 3600, // iat 1 hour in the future
            "read".to_string(),
            None,
        );

        let token = sign_jwt(&claims, &kid, &pkcs8_bytes);

        let result: Result<ServiceClaims, JwtError> = validator.validate(&token).await;
        assert!(
            matches!(result, Err(JwtError::IatTooFarInFuture)),
            "Future iat should be rejected: {result:?}"
        );
    }

    // Note: Missing-iat validation is enforced at compile time via the HasIat trait bound
    // on JwtValidator::validate<T: DeserializeOwned + HasIat>. Types without HasIat
    // cannot be passed to validate().

    #[test]
    fn test_jwt_validator_clamps_negative_clock_skew() {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost/.well-known/jwks.json".to_string()).unwrap(),
        );
        let validator = JwtValidator::new(jwks_client, -100);
        assert_eq!(validator.clock_skew_seconds, 0);
    }

    #[test]
    fn test_jwt_validator_clamps_excessive_clock_skew() {
        let jwks_client = Arc::new(
            JwksClient::new("http://localhost/.well-known/jwks.json".to_string()).unwrap(),
        );
        // MAX_CLOCK_SKEW is 600 seconds
        let validator = JwtValidator::new(jwks_client, 99999);
        assert_eq!(
            validator.clock_skew_seconds,
            MAX_CLOCK_SKEW.as_secs() as i64
        );
    }
}
