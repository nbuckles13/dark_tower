use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Service credential model (maps to service_credentials table)
#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)] // Some fields used only in Phase 4 admin endpoints
pub struct ServiceCredential {
    pub credential_id: Uuid,
    pub client_id: String,
    pub client_secret_hash: String,
    pub service_type: String,
    pub region: Option<String>,
    pub scopes: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Signing key model (maps to signing_keys table)
#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)] // Some fields used only in Phase 4 admin/JWKS endpoints
pub struct SigningKey {
    pub key_id: String,
    pub public_key: String,
    pub private_key_encrypted: Vec<u8>,
    pub encryption_nonce: Vec<u8>,
    pub encryption_tag: Vec<u8>,
    pub encryption_algorithm: String,
    pub master_key_version: i32,
    pub algorithm: String,
    pub is_active: bool,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Auth event model (maps to auth_events table)
#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)] // Used only in Phase 4 audit/monitoring endpoints
pub struct AuthEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub user_id: Option<Uuid>,
    pub credential_id: Option<Uuid>,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: Option<serde_json::Value>,
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

/// Service registration response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterServiceResponse {
    pub client_id: String,
    pub client_secret: String,
    pub service_type: String,
    pub scopes: Vec<String>,
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

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "global-controller" => Some(ServiceType::GlobalController),
            "meeting-controller" => Some(ServiceType::MeetingController),
            "media-handler" => Some(ServiceType::MediaHandler),
            _ => None,
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

/// Auth event type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // All variants used in Phase 4 event logging
pub enum AuthEventType {
    UserLogin,
    UserLoginFailed,
    ServiceTokenIssued,
    ServiceTokenFailed,
    ServiceRegistered,
    KeyGenerated,
    KeyRotated,
    KeyExpired,
    TokenValidationFailed,
    RateLimitExceeded,
}

impl AuthEventType {
    #[allow(dead_code)] // Used in Phase 4 event logging
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

        let mc_scopes = ServiceType::MeetingController.default_scopes();
        assert!(mc_scopes.contains(&"participant:manage".to_string()));

        let mh_scopes = ServiceType::MediaHandler.default_scopes();
        assert!(mh_scopes.contains(&"media:process".to_string()));
    }

    #[test]
    fn test_service_type_parsing() {
        assert_eq!(
            ServiceType::from_str("global-controller"),
            Some(ServiceType::GlobalController)
        );
        assert_eq!(
            ServiceType::from_str("meeting-controller"),
            Some(ServiceType::MeetingController)
        );
        assert_eq!(
            ServiceType::from_str("media-handler"),
            Some(ServiceType::MediaHandler)
        );
        assert_eq!(ServiceType::from_str("invalid"), None);
    }
}
