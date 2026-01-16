//! Auth Controller HTTP client for internal endpoints.
//!
//! This service handles communication with the Authentication Controller
//! for meeting and guest token generation.
//!
//! # Security
//!
//! - GC authenticates using its own service token (client credentials)
//! - All requests use HTTPS in production
//! - Timeouts prevent hanging connections
//! - Errors are logged server-side with generic messages returned

use crate::errors::GcError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, instrument, warn};
use uuid::Uuid;

/// Default timeout for AC requests in seconds.
const AC_REQUEST_TIMEOUT_SECS: u64 = 10;

/// Participant type in a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantType {
    /// Member of the same organization as the meeting.
    Member,
    /// User from a different organization.
    External,
    /// Anonymous guest (no authentication).
    Guest,
}

/// Role within a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRole {
    /// Meeting host with full control.
    Host,
    /// Regular participant.
    Participant,
}

/// Request to AC for a meeting token.
#[derive(Debug, Clone, Serialize)]
pub struct MeetingTokenRequest {
    /// User ID of the participant.
    pub subject_user_id: Uuid,

    /// Meeting ID.
    pub meeting_id: Uuid,

    /// Organization that owns the meeting.
    pub meeting_org_id: Uuid,

    /// User's home organization (None if same as meeting org).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_org_id: Option<Uuid>,

    /// Type of participant.
    pub participant_type: ParticipantType,

    /// Role in the meeting.
    pub role: MeetingRole,

    /// Capabilities granted to this participant.
    pub capabilities: Vec<String>,

    /// Token TTL in seconds (default: 900).
    pub ttl_seconds: u32,
}

/// Request to AC for a guest token.
#[derive(Debug, Clone, Serialize)]
pub struct GuestTokenRequest {
    /// CSPRNG-generated guest identifier.
    pub guest_id: Uuid,

    /// Display name for the guest.
    pub display_name: String,

    /// Meeting ID.
    pub meeting_id: Uuid,

    /// Organization that owns the meeting.
    pub meeting_org_id: Uuid,

    /// Whether the guest should be placed in waiting room.
    pub waiting_room: bool,

    /// Token TTL in seconds (default: 900).
    pub ttl_seconds: u32,
}

/// Response from AC for token requests.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    /// The issued JWT token.
    pub token: String,

    /// Token expiration in seconds from now.
    pub expires_in: u32,
}

/// HTTP client for Auth Controller internal endpoints.
///
/// Handles service-to-service authentication and token requests.
#[derive(Clone)]
pub struct AcClient {
    /// HTTP client with configured timeouts.
    client: Client,

    /// Base URL for AC internal API.
    base_url: String,

    /// GC's service token for authenticating to AC.
    service_token: String,
}

impl AcClient {
    /// Create a new AC client.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL for AC internal API (e.g., "http://localhost:8082")
    /// * `service_token` - GC's service token for client credentials auth
    ///
    /// # Errors
    ///
    /// Returns `GcError::Internal` if the HTTP client cannot be built.
    pub fn new(base_url: String, service_token: String) -> Result<Self, GcError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(AC_REQUEST_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                error!(target: "gc.services.ac_client", error = %e, "Failed to build HTTP client");
                GcError::Internal
            })?;

        Ok(Self {
            client,
            base_url,
            service_token,
        })
    }

    /// Request a meeting token from AC for an authenticated user.
    ///
    /// # Arguments
    ///
    /// * `request` - Meeting token request parameters
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` if AC is unreachable or returns 5xx
    /// - `GcError::Forbidden` if the request is rejected
    /// - `GcError::BadRequest` if the request parameters are invalid
    #[instrument(skip(self, request), fields(meeting_id = %request.meeting_id, user_id = %request.subject_user_id))]
    pub async fn request_meeting_token(
        &self,
        request: &MeetingTokenRequest,
    ) -> Result<TokenResponse, GcError> {
        let url = format!("{}/api/v1/auth/internal/meeting-token", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.service_token))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                warn!(target: "gc.services.ac_client", error = %e, "AC request failed");
                GcError::ServiceUnavailable("Auth Controller is unavailable".to_string())
            })?;

        self.handle_response(response).await
    }

    /// Request a guest token from AC for an anonymous user.
    ///
    /// # Arguments
    ///
    /// * `request` - Guest token request parameters
    ///
    /// # Errors
    ///
    /// - `GcError::ServiceUnavailable` if AC is unreachable or returns 5xx
    /// - `GcError::Forbidden` if the request is rejected
    /// - `GcError::BadRequest` if the request parameters are invalid
    #[instrument(skip(self, request), fields(meeting_id = %request.meeting_id, guest_id = %request.guest_id))]
    pub async fn request_guest_token(
        &self,
        request: &GuestTokenRequest,
    ) -> Result<TokenResponse, GcError> {
        let url = format!("{}/api/v1/auth/internal/guest-token", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.service_token))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                warn!(target: "gc.services.ac_client", error = %e, "AC request failed");
                GcError::ServiceUnavailable("Auth Controller is unavailable".to_string())
            })?;

        self.handle_response(response).await
    }

    /// Handle AC response and map status codes to errors.
    async fn handle_response(&self, response: reqwest::Response) -> Result<TokenResponse, GcError> {
        let status = response.status();

        if status.is_success() {
            response.json().await.map_err(|e| {
                error!(target: "gc.services.ac_client", error = %e, "Failed to parse AC response");
                GcError::Internal
            })
        } else if status.is_server_error() {
            warn!(target: "gc.services.ac_client", status = %status, "AC returned server error");
            Err(GcError::ServiceUnavailable(
                "Auth Controller is unavailable".to_string(),
            ))
        } else if status.as_u16() == 403 {
            Err(GcError::Forbidden(
                "Request denied by Auth Controller".to_string(),
            ))
        } else if status.as_u16() == 400 {
            let error_body = response.text().await.unwrap_or_default();
            warn!(target: "gc.services.ac_client", status = %status, body = %error_body, "AC returned bad request");
            Err(GcError::BadRequest("Invalid token request".to_string()))
        } else if status.as_u16() == 401 {
            error!(target: "gc.services.ac_client", "GC service token rejected by AC");
            Err(GcError::Internal)
        } else {
            warn!(target: "gc.services.ac_client", status = %status, "Unexpected AC response");
            Err(GcError::Internal)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_type_serialization() {
        let member = ParticipantType::Member;
        let json = serde_json::to_string(&member).unwrap();
        assert_eq!(json, "\"member\"");

        let external = ParticipantType::External;
        let json = serde_json::to_string(&external).unwrap();
        assert_eq!(json, "\"external\"");

        let guest = ParticipantType::Guest;
        let json = serde_json::to_string(&guest).unwrap();
        assert_eq!(json, "\"guest\"");
    }

    #[test]
    fn test_meeting_role_serialization() {
        let host = MeetingRole::Host;
        let json = serde_json::to_string(&host).unwrap();
        assert_eq!(json, "\"host\"");

        let participant = MeetingRole::Participant;
        let json = serde_json::to_string(&participant).unwrap();
        assert_eq!(json, "\"participant\"");
    }

    #[test]
    fn test_meeting_token_request_serialization() {
        let request = MeetingTokenRequest {
            subject_user_id: Uuid::nil(),
            meeting_id: Uuid::nil(),
            meeting_org_id: Uuid::nil(),
            home_org_id: None,
            participant_type: ParticipantType::Member,
            role: MeetingRole::Participant,
            capabilities: vec!["audio".to_string(), "video".to_string()],
            ttl_seconds: 900,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"participant_type\":\"member\""));
        assert!(json.contains("\"role\":\"participant\""));
        assert!(json.contains("\"ttl_seconds\":900"));
        // home_org_id should be omitted when None
        assert!(!json.contains("home_org_id"));
    }

    #[test]
    fn test_meeting_token_request_with_home_org() {
        let request = MeetingTokenRequest {
            subject_user_id: Uuid::nil(),
            meeting_id: Uuid::nil(),
            meeting_org_id: Uuid::nil(),
            home_org_id: Some(Uuid::nil()),
            participant_type: ParticipantType::External,
            role: MeetingRole::Participant,
            capabilities: vec![],
            ttl_seconds: 900,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("home_org_id"));
        assert!(json.contains("\"participant_type\":\"external\""));
    }

    #[test]
    fn test_guest_token_request_serialization() {
        let request = GuestTokenRequest {
            guest_id: Uuid::nil(),
            display_name: "Test Guest".to_string(),
            meeting_id: Uuid::nil(),
            meeting_org_id: Uuid::nil(),
            waiting_room: true,
            ttl_seconds: 900,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"display_name\":\"Test Guest\""));
        assert!(json.contains("\"waiting_room\":true"));
        assert!(json.contains("\"ttl_seconds\":900"));
    }

    #[test]
    fn test_token_response_deserialization() {
        let json = r#"{"token":"eyJ...","expires_in":900}"#;
        let response: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.token, "eyJ...");
        assert_eq!(response.expires_in, 900);
    }
}
