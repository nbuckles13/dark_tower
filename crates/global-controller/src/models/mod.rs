//! Global Controller models.
//!
//! Contains data types used across the Global Controller service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Meeting status enumeration.
///
/// Represents the lifecycle state of a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // Will be used in Phase 2+ for meeting management
pub enum MeetingStatus {
    /// Meeting is scheduled but not yet active.
    Scheduled,

    /// Meeting is currently in progress.
    Active,

    /// Meeting has ended normally.
    Ended,

    /// Meeting was cancelled before it started.
    Cancelled,
}

impl MeetingStatus {
    /// Returns the string representation of the status.
    #[allow(dead_code)] // Will be used in Phase 2+
    pub fn as_str(&self) -> &'static str {
        match self {
            MeetingStatus::Scheduled => "scheduled",
            MeetingStatus::Active => "active",
            MeetingStatus::Ended => "ended",
            MeetingStatus::Cancelled => "cancelled",
        }
    }
}

/// Health check response.
///
/// Returned by the `/v1/health` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service health status ("healthy" or "unhealthy").
    pub status: String,

    /// Deployment region.
    pub region: String,

    /// Database connectivity status (optional, for detailed health).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

// ============================================================================
// Meeting API Models (Phase 2)
// ============================================================================

/// Maximum display name length for guests.
pub const MAX_GUEST_DISPLAY_NAME_LENGTH: usize = 100;

/// Minimum display name length for guests.
pub const MIN_GUEST_DISPLAY_NAME_LENGTH: usize = 2;

/// Meeting database row.
///
/// Represents a meeting as stored in the database.
/// Some fields are unused currently but will be used in future phases.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields used in database queries and future phases
pub struct MeetingRow {
    /// Unique meeting identifier.
    pub meeting_id: Uuid,

    /// Organization that owns the meeting.
    pub org_id: Uuid,

    /// User who created the meeting.
    pub created_by_user_id: Uuid,

    /// Meeting display name.
    pub display_name: String,

    /// Short meeting code for joining.
    pub meeting_code: String,

    /// Secret for validating join tokens.
    pub join_token_secret: String,

    /// Maximum number of participants.
    pub max_participants: i32,

    /// Whether end-to-end encryption is enabled.
    pub enable_e2e_encryption: bool,

    /// Whether authentication is required to join.
    pub require_auth: bool,

    /// Whether recording is enabled.
    pub recording_enabled: bool,

    /// Assigned meeting controller ID (if active).
    pub meeting_controller_id: Option<String>,

    /// Assigned meeting controller region.
    pub meeting_controller_region: Option<String>,

    /// Current meeting status.
    pub status: String,

    /// Scheduled start time.
    pub scheduled_start_time: Option<DateTime<Utc>>,

    /// Actual start time.
    pub actual_start_time: Option<DateTime<Utc>>,

    /// Actual end time.
    pub actual_end_time: Option<DateTime<Utc>>,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,

    /// Whether anonymous guests can join.
    pub allow_guests: bool,

    /// Whether external org users can join.
    pub allow_external_participants: bool,

    /// Whether waiting room is enabled.
    pub waiting_room_enabled: bool,
}

/// Response for joining a meeting.
///
/// Returned by `GET /v1/meetings/{code}` and `POST /v1/meetings/{code}/guest-token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinMeetingResponse {
    /// The meeting token for connecting to the meeting controller.
    pub token: String,

    /// Token expiration in seconds from now.
    pub expires_in: u32,

    /// Meeting ID.
    pub meeting_id: Uuid,

    /// Meeting display name.
    pub meeting_name: String,

    /// Assigned meeting controller information.
    pub mc_assignment: McAssignmentInfo,
}

/// Meeting controller assignment information.
///
/// Returned as part of the join meeting response to direct the client
/// to the assigned meeting controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McAssignmentInfo {
    /// Assigned meeting controller ID.
    pub mc_id: String,

    /// WebTransport endpoint for client connections (preferred).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webtransport_endpoint: Option<String>,

    /// gRPC endpoint for fallback connections.
    pub grpc_endpoint: String,
}

/// Request for guest token.
///
/// Sent by anonymous users to join a meeting.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestJoinRequest {
    /// Guest's display name.
    pub display_name: String,

    /// Captcha token for bot prevention.
    pub captcha_token: String,
}

impl GuestJoinRequest {
    /// Validate the request.
    ///
    /// # Errors
    ///
    /// Returns an error message if validation fails.
    pub fn validate(&self) -> Result<(), &'static str> {
        let display_name = self.display_name.trim();

        if display_name.len() < MIN_GUEST_DISPLAY_NAME_LENGTH {
            return Err("Display name must be at least 2 characters");
        }

        if display_name.len() > MAX_GUEST_DISPLAY_NAME_LENGTH {
            return Err("Display name must be at most 100 characters");
        }

        if self.captcha_token.is_empty() {
            return Err("Captcha token is required");
        }

        Ok(())
    }
}

/// Request to update meeting settings.
///
/// Sent by meeting host to update meeting configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateMeetingSettingsRequest {
    /// Whether anonymous guests can join.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_guests: Option<bool>,

    /// Whether external org users can join.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_external_participants: Option<bool>,

    /// Whether waiting room is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_room_enabled: Option<bool>,
}

impl UpdateMeetingSettingsRequest {
    /// Check if the request has any changes.
    pub fn has_changes(&self) -> bool {
        self.allow_guests.is_some()
            || self.allow_external_participants.is_some()
            || self.waiting_room_enabled.is_some()
    }
}

/// Response for meeting details.
///
/// Returned by `PATCH /v1/meetings/{id}/settings`.
#[derive(Debug, Clone, Serialize)]
pub struct MeetingResponse {
    /// Meeting ID.
    pub meeting_id: Uuid,

    /// Meeting display name.
    pub display_name: String,

    /// Short meeting code for joining.
    pub meeting_code: String,

    /// Current meeting status.
    pub status: String,

    /// Whether anonymous guests can join.
    pub allow_guests: bool,

    /// Whether external org users can join.
    pub allow_external_participants: bool,

    /// Whether waiting room is enabled.
    pub waiting_room_enabled: bool,

    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<MeetingRow> for MeetingResponse {
    fn from(row: MeetingRow) -> Self {
        Self {
            meeting_id: row.meeting_id,
            display_name: row.display_name,
            meeting_code: row.meeting_code,
            status: row.status,
            allow_guests: row.allow_guests,
            allow_external_participants: row.allow_external_participants,
            waiting_room_enabled: row.waiting_room_enabled,
            updated_at: row.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meeting_status_as_str() {
        assert_eq!(MeetingStatus::Scheduled.as_str(), "scheduled");
        assert_eq!(MeetingStatus::Active.as_str(), "active");
        assert_eq!(MeetingStatus::Ended.as_str(), "ended");
        assert_eq!(MeetingStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn test_meeting_status_serialization() {
        let status = MeetingStatus::Active;
        let json = serde_json::to_string(&status).expect("serialization should succeed");
        assert_eq!(json, "\"active\"");
    }

    #[test]
    fn test_meeting_status_deserialization() {
        let status: MeetingStatus =
            serde_json::from_str("\"scheduled\"").expect("deserialization should succeed");
        assert_eq!(status, MeetingStatus::Scheduled);
    }

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            region: "us-east-1".to_string(),
            database: None,
        };

        let json = serde_json::to_string(&response).expect("serialization should succeed");

        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"region\":\"us-east-1\""));
        // database field should be omitted when None
        assert!(!json.contains("database"));
    }

    #[test]
    fn test_health_response_serialization_with_database() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            region: "eu-west-1".to_string(),
            database: Some("healthy".to_string()),
        };

        let json = serde_json::to_string(&response).expect("serialization should succeed");

        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"region\":\"eu-west-1\""));
        assert!(json.contains("\"database\":\"healthy\""));
    }

    #[test]
    fn test_health_response_deserialization() {
        let json = r#"{"status":"healthy","region":"ap-southeast-1"}"#;
        let response: HealthResponse =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(response.status, "healthy");
        assert_eq!(response.region, "ap-southeast-1");
        assert_eq!(response.database, None);
    }

    // ========================================================================
    // Phase 2 Model Tests
    // ========================================================================

    #[test]
    fn test_join_meeting_response_serialization() {
        let response = JoinMeetingResponse {
            token: "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9...".to_string(),
            expires_in: 900,
            meeting_id: Uuid::nil(),
            meeting_name: "Test Meeting".to_string(),
            mc_assignment: McAssignmentInfo {
                mc_id: "mc-001".to_string(),
                webtransport_endpoint: Some("https://mc.example.com:443".to_string()),
                grpc_endpoint: "https://mc.example.com:50051".to_string(),
            },
        };

        let json = serde_json::to_string(&response).expect("serialization should succeed");

        assert!(json.contains("\"token\":\"eyJ"));
        assert!(json.contains("\"expires_in\":900"));
        assert!(json.contains("\"meeting_name\":\"Test Meeting\""));
        assert!(json.contains("\"mc_id\":\"mc-001\""));
        assert!(json.contains("\"grpc_endpoint\":\"https://mc.example.com:50051\""));
    }

    #[test]
    fn test_mc_assignment_info_serialization() {
        let assignment = McAssignmentInfo {
            mc_id: "mc-test".to_string(),
            webtransport_endpoint: Some("https://mc:443".to_string()),
            grpc_endpoint: "https://mc:50051".to_string(),
        };

        let json = serde_json::to_string(&assignment).expect("serialization should succeed");
        assert!(json.contains("\"mc_id\":\"mc-test\""));
        assert!(json.contains("\"webtransport_endpoint\":\"https://mc:443\""));
        assert!(json.contains("\"grpc_endpoint\":\"https://mc:50051\""));
    }

    #[test]
    fn test_mc_assignment_info_serialization_no_webtransport() {
        let assignment = McAssignmentInfo {
            mc_id: "mc-test".to_string(),
            webtransport_endpoint: None,
            grpc_endpoint: "https://mc:50051".to_string(),
        };

        let json = serde_json::to_string(&assignment).expect("serialization should succeed");
        assert!(json.contains("\"mc_id\":\"mc-test\""));
        // webtransport_endpoint should be omitted when None
        assert!(!json.contains("webtransport_endpoint"));
        assert!(json.contains("\"grpc_endpoint\":\"https://mc:50051\""));
    }

    #[test]
    fn test_guest_join_request_deserialization() {
        let json = r#"{"display_name":"John Doe","captcha_token":"abc123"}"#;
        let request: GuestJoinRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "John Doe");
        assert_eq!(request.captcha_token, "abc123");
    }

    #[test]
    fn test_guest_join_request_rejects_unknown_fields() {
        let json = r#"{"display_name":"John","captcha_token":"abc","extra":"field"}"#;
        let result: Result<GuestJoinRequest, _> = serde_json::from_str(json);

        assert!(result.is_err(), "Should reject unknown fields");
    }

    #[test]
    fn test_guest_join_request_validation_success() {
        let request = GuestJoinRequest {
            display_name: "John Doe".to_string(),
            captcha_token: "abc123".to_string(),
        };

        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_guest_join_request_validation_short_name() {
        let request = GuestJoinRequest {
            display_name: "J".to_string(),
            captcha_token: "abc123".to_string(),
        };

        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Display name must be at least 2 characters"
        );
    }

    #[test]
    fn test_guest_join_request_validation_long_name() {
        let request = GuestJoinRequest {
            display_name: "a".repeat(101),
            captcha_token: "abc123".to_string(),
        };

        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Display name must be at most 100 characters"
        );
    }

    #[test]
    fn test_guest_join_request_validation_empty_captcha() {
        let request = GuestJoinRequest {
            display_name: "John Doe".to_string(),
            captcha_token: String::new(),
        };

        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Captcha token is required");
    }

    #[test]
    fn test_guest_join_request_validation_whitespace_name() {
        let request = GuestJoinRequest {
            display_name: "   ".to_string(),
            captcha_token: "abc123".to_string(),
        };

        let result = request.validate();
        assert!(result.is_err(), "Should reject whitespace-only name");
    }

    #[test]
    fn test_update_meeting_settings_request_deserialization() {
        let json = r#"{"allow_guests":true,"waiting_room_enabled":false}"#;
        let request: UpdateMeetingSettingsRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.allow_guests, Some(true));
        assert_eq!(request.allow_external_participants, None);
        assert_eq!(request.waiting_room_enabled, Some(false));
    }

    #[test]
    fn test_update_meeting_settings_request_rejects_unknown_fields() {
        let json = r#"{"allow_guests":true,"extra":"field"}"#;
        let result: Result<UpdateMeetingSettingsRequest, _> = serde_json::from_str(json);

        assert!(result.is_err(), "Should reject unknown fields");
    }

    #[test]
    fn test_update_meeting_settings_has_changes() {
        let request_with_changes = UpdateMeetingSettingsRequest {
            allow_guests: Some(true),
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        assert!(request_with_changes.has_changes());

        let request_no_changes = UpdateMeetingSettingsRequest {
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        assert!(!request_no_changes.has_changes());
    }
}
