use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;
use uuid::Uuid;

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
}
