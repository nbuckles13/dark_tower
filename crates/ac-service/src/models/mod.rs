use chrono::{DateTime, Utc};
use common::secret::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use sqlx::FromRow;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// Internal Token Request/Response Types (ADR-0020)
// ============================================================================

/// Request to issue a meeting token for a user via internal endpoint.
///
/// Called by GC (with service token having `internal:meeting-token` scope)
/// to get meeting tokens for authenticated users joining meetings.
#[derive(Debug, Clone, Deserialize)]
pub struct MeetingTokenRequest {
    /// The user ID to issue the token for
    pub subject_user_id: Uuid,
    /// The meeting being joined
    pub meeting_id: Uuid,
    /// The org that owns the meeting
    pub meeting_org_id: Uuid,
    /// The user's home org (may differ for cross-org meetings)
    pub home_org_id: Uuid,
    /// Whether this is a member of the meeting org or external participant
    #[serde(default)]
    pub participant_type: ParticipantType,
    /// Role in the meeting (host or participant)
    #[serde(default)]
    pub role: MeetingRole,
    /// Capabilities granted (e.g., video, audio, screen_share)
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Token TTL in seconds (max 900 = 15 minutes)
    #[serde(default = "default_meeting_ttl")]
    pub ttl_seconds: u32,
}

/// Request to issue a guest token via internal endpoint.
///
/// Called by GC (with service token having `internal:meeting-token` scope)
/// to get guest tokens for unauthenticated users joining meetings.
#[derive(Debug, Clone, Deserialize)]
pub struct GuestTokenRequest {
    /// Generated guest ID
    pub guest_id: Uuid,
    /// Display name for the guest
    pub display_name: String,
    /// The meeting being joined
    pub meeting_id: Uuid,
    /// The org that owns the meeting
    pub meeting_org_id: Uuid,
    /// Whether guest should wait in waiting room
    #[serde(default = "default_waiting_room")]
    pub waiting_room: bool,
    /// Token TTL in seconds (max 900 = 15 minutes)
    #[serde(default = "default_meeting_ttl")]
    pub ttl_seconds: u32,
}

/// Response for internal token endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct InternalTokenResponse {
    /// The issued JWT token
    pub token: String,
    /// Token lifetime in seconds
    pub expires_in: u32,
}

/// Participant type in a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantType {
    /// Member of the meeting organization
    #[default]
    Member,
    /// External user (from a different organization)
    External,
    /// Guest (unauthenticated)
    Guest,
}

impl ParticipantType {
    /// Convert to string for JWT claims
    pub fn as_str(&self) -> &'static str {
        match self {
            ParticipantType::Member => "member",
            ParticipantType::External => "external",
            ParticipantType::Guest => "guest",
        }
    }
}

impl fmt::Display for ParticipantType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Role in a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRole {
    /// Meeting host with elevated privileges
    Host,
    /// Regular participant
    #[default]
    Participant,
    /// Guest (waiting room or limited privileges)
    Guest,
}

impl MeetingRole {
    /// Convert to string for JWT claims
    pub fn as_str(&self) -> &'static str {
        match self {
            MeetingRole::Host => "host",
            MeetingRole::Participant => "participant",
            MeetingRole::Guest => "guest",
        }
    }
}

impl fmt::Display for MeetingRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Default TTL for meeting tokens (15 minutes)
fn default_meeting_ttl() -> u32 {
    900
}

/// Default waiting room setting (true)
fn default_waiting_room() -> bool {
    true
}

/// Service credential model (maps to service_credentials table)
#[derive(Debug, Clone, FromRow)]
pub struct ServiceCredential {
    pub credential_id: Uuid,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub client_id: String,
    pub client_secret_hash: String,
    pub service_type: String,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub region: Option<String>,
    pub scopes: Vec<String>,
    pub is_active: bool,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub created_at: DateTime<Utc>,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub updated_at: DateTime<Utc>,
}

/// Signing key model (maps to signing_keys table)
#[derive(Debug, Clone, FromRow)]
pub struct SigningKey {
    pub key_id: String,
    pub public_key: String,
    pub private_key_encrypted: Vec<u8>,
    pub encryption_nonce: Vec<u8>,
    pub encryption_tag: Vec<u8>,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub encryption_algorithm: String,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub master_key_version: i32,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub algorithm: String,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub is_active: bool,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub valid_from: DateTime<Utc>,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub valid_until: DateTime<Utc>,
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub created_at: DateTime<Utc>,
}

/// Auth event model (maps to auth_events table)
#[derive(Debug, Clone, FromRow)]
pub struct AuthEvent {
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub event_id: Uuid,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub event_type: String,
    #[allow(dead_code)] // Will be used in Phase 4 user auth
    pub user_id: Option<Uuid>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub credential_id: Option<Uuid>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub success: bool,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub failure_reason: Option<String>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub ip_address: Option<String>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub user_agent: Option<String>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub metadata: Option<serde_json::Value>,
    #[allow(dead_code)] // Will be used in Phase 4 audit endpoints
    pub created_at: DateTime<Utc>,
}

/// Token response (OAuth 2.0 compliant)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

/// Service registration response.
///
/// The `client_secret` field is wrapped in `SecretString` which:
/// - Redacts the value in Debug output to prevent accidental logging
/// - Requires explicit `.expose_secret()` to access the value
/// - Uses custom serialization to expose the secret in API responses
///   (this is the ONLY time the client_secret should be visible to the user)
#[derive(Clone, Deserialize)]
pub struct RegisterServiceResponse {
    pub client_id: String,
    #[serde(deserialize_with = "deserialize_secret_string")]
    pub client_secret: SecretString,
    pub service_type: String,
    pub scopes: Vec<String>,
}

/// Custom Debug that redacts client_secret
impl fmt::Debug for RegisterServiceResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterServiceResponse")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("service_type", &self.service_type)
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Custom Serialize that exposes client_secret for API response.
/// This is intentional: the registration response is the ONLY time
/// the plaintext client_secret is shown to the user.
impl Serialize for RegisterServiceResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RegisterServiceResponse", 4)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.serialize_field("service_type", &self.service_type)?;
        state.serialize_field("scopes", &self.scopes)?;
        state.end()
    }
}

/// Helper to deserialize SecretString from JSON string
fn deserialize_secret_string<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(SecretString::from(s))
}

/// JWKS response (RFC 7517)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwks {
    pub keys: Vec<JsonWebKey>,
}

/// JSON Web Key (RFC 7517)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWebKey {
    pub kid: String, // Key ID
    pub kty: String, // Key Type (e.g., "OKP" for EdDSA)
    pub crv: String, // Curve (e.g., "Ed25519")
    pub x: String,   // Public key (base64url encoded)
    #[serde(rename = "use")]
    pub use_: String, // Public key use (e.g., "sig")
    pub alg: String, // Algorithm (e.g., "EdDSA")
}

/// Service type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceType {
    GlobalController,
    MeetingController,
    MediaHandler,
}

impl ServiceType {
    /// Get default scopes for a service type
    pub fn default_scopes(&self) -> Vec<String> {
        match self {
            ServiceType::GlobalController => vec![
                "meeting:create".to_string(),
                "meeting:list".to_string(),
                "meeting:read".to_string(),
                "service:register".to_string(),
                "internal:meeting-token".to_string(),
            ],
            ServiceType::MeetingController => vec![
                "meeting:read".to_string(),
                "meeting:update".to_string(),
                "participant:manage".to_string(),
                "media:route".to_string(),
            ],
            ServiceType::MediaHandler => vec![
                "media:process".to_string(),
                "media:forward".to_string(),
                "participant:read".to_string(),
            ],
        }
    }

    /// Convert to string
    #[allow(dead_code)] // Will be used in Phase 4 admin endpoints
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceType::GlobalController => "global-controller",
            ServiceType::MeetingController => "meeting-controller",
            ServiceType::MediaHandler => "media-handler",
        }
    }
}

impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "global-controller" => Ok(ServiceType::GlobalController),
            "meeting-controller" => Ok(ServiceType::MeetingController),
            "media-handler" => Ok(ServiceType::MediaHandler),
            _ => Err(format!("Invalid service type: {}", s)),
        }
    }
}

/// Auth event type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthEventType {
    #[allow(dead_code)] // Will be used in Phase 4 user auth
    UserLogin,
    #[allow(dead_code)] // Will be used in Phase 4 user auth
    UserLoginFailed,
    ServiceTokenIssued,
    ServiceTokenFailed,
    ServiceRegistered,
    KeyGenerated,
    #[allow(dead_code)] // Will be used in Phase 4 key rotation
    KeyRotated,
    #[allow(dead_code)] // Will be used in Phase 4 key rotation
    KeyExpired,
    #[allow(dead_code)] // Will be used in Phase 4 token validation
    TokenValidationFailed,
    #[allow(dead_code)] // Will be used in Phase 4 rate limiting
    RateLimitExceeded,
}

impl AuthEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthEventType::UserLogin => "user_login",
            AuthEventType::UserLoginFailed => "user_login_failed",
            AuthEventType::ServiceTokenIssued => "service_token_issued",
            AuthEventType::ServiceTokenFailed => "service_token_failed",
            AuthEventType::ServiceRegistered => "service_registered",
            AuthEventType::KeyGenerated => "key_generated",
            AuthEventType::KeyRotated => "key_rotated",
            AuthEventType::KeyExpired => "key_expired",
            AuthEventType::TokenValidationFailed => "token_validation_failed",
            AuthEventType::RateLimitExceeded => "rate_limit_exceeded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_scopes() {
        let gc_scopes = ServiceType::GlobalController.default_scopes();
        assert!(gc_scopes.contains(&"meeting:create".to_string()));
        assert!(
            gc_scopes.contains(&"internal:meeting-token".to_string()),
            "GC must have internal:meeting-token scope for POST /api/v1/auth/internal/meeting-token"
        );

        let mc_scopes = ServiceType::MeetingController.default_scopes();
        assert!(mc_scopes.contains(&"participant:manage".to_string()));

        let mh_scopes = ServiceType::MediaHandler.default_scopes();
        assert!(mh_scopes.contains(&"media:process".to_string()));
    }

    #[test]
    fn test_service_type_parsing() {
        assert_eq!(
            ServiceType::from_str("global-controller").ok(),
            Some(ServiceType::GlobalController)
        );
        assert_eq!(
            ServiceType::from_str("meeting-controller").ok(),
            Some(ServiceType::MeetingController)
        );
        assert_eq!(
            ServiceType::from_str("media-handler").ok(),
            Some(ServiceType::MediaHandler)
        );
        assert!(ServiceType::from_str("invalid").is_err());
    }

    // ============================================================================
    // Internal Token Types Tests (ADR-0020)
    // ============================================================================

    #[test]
    fn test_participant_type_display() {
        assert_eq!(format!("{}", ParticipantType::Member), "member");
        assert_eq!(format!("{}", ParticipantType::External), "external");
        assert_eq!(format!("{}", ParticipantType::Guest), "guest");
    }

    #[test]
    fn test_participant_type_default() {
        let default: ParticipantType = Default::default();
        assert_eq!(default, ParticipantType::Member);
    }

    #[test]
    fn test_participant_type_serde() {
        // Serialize
        let member = ParticipantType::Member;
        let json = serde_json::to_string(&member).expect("Should serialize");
        assert_eq!(json, "\"member\"");

        // Deserialize
        let deserialized: ParticipantType =
            serde_json::from_str("\"external\"").expect("Should deserialize");
        assert_eq!(deserialized, ParticipantType::External);
    }

    #[test]
    fn test_meeting_role_display() {
        assert_eq!(format!("{}", MeetingRole::Host), "host");
        assert_eq!(format!("{}", MeetingRole::Participant), "participant");
        assert_eq!(format!("{}", MeetingRole::Guest), "guest");
    }

    #[test]
    fn test_meeting_role_default() {
        let default: MeetingRole = Default::default();
        assert_eq!(default, MeetingRole::Participant);
    }

    #[test]
    fn test_meeting_role_serde() {
        // Serialize
        let host = MeetingRole::Host;
        let json = serde_json::to_string(&host).expect("Should serialize");
        assert_eq!(json, "\"host\"");

        // Deserialize
        let deserialized: MeetingRole =
            serde_json::from_str("\"guest\"").expect("Should deserialize");
        assert_eq!(deserialized, MeetingRole::Guest);
    }

    #[test]
    fn test_internal_token_response_serialization() {
        let response = InternalTokenResponse {
            token: "eyJhbGciOiJFZERTQSJ9.test.sig".to_string(),
            expires_in: 900,
        };

        let json = serde_json::to_string(&response).expect("Should serialize");
        assert!(json.contains("\"token\":"));
        assert!(json.contains("\"expires_in\":900"));
    }

    #[test]
    fn test_meeting_token_request_full() {
        let json = r#"{
            "subject_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "home_org_id": "550e8400-e29b-41d4-a716-446655440004",
            "participant_type": "external",
            "role": "host",
            "capabilities": ["video", "audio", "screen_share"],
            "ttl_seconds": 600
        }"#;

        let req: MeetingTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(req.participant_type, ParticipantType::External);
        assert_eq!(req.role, MeetingRole::Host);
        assert_eq!(req.capabilities.len(), 3);
        assert_eq!(req.ttl_seconds, 600);
    }

    #[test]
    fn test_guest_token_request_full() {
        let json = r#"{
            "guest_id": "550e8400-e29b-41d4-a716-446655440001",
            "display_name": "Test Guest",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "waiting_room": false,
            "ttl_seconds": 300
        }"#;

        let req: GuestTokenRequest = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(req.display_name, "Test Guest");
        assert!(!req.waiting_room);
        assert_eq!(req.ttl_seconds, 300);
    }

    #[test]
    fn test_default_ttl_value() {
        assert_eq!(default_meeting_ttl(), 900);
    }

    #[test]
    fn test_default_waiting_room_value() {
        assert!(default_waiting_room());
    }
}
