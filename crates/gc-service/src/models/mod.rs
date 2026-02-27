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
/// Returned by the `/health` endpoint (liveness probe).
/// Note: Currently unused as /health returns plain text "OK" per ADR-0012.
/// Kept for potential future use if detailed health check is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HealthResponse {
    /// Service health status ("healthy" or "unhealthy").
    pub status: String,

    /// Deployment region.
    pub region: String,

    /// Database connectivity status (optional, for detailed health).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// Readiness check response.
///
/// Returned by the `/ready` endpoint (readiness probe).
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessResponse {
    /// Service readiness status ("ready" or "not_ready").
    pub status: &'static str,

    /// Database connectivity status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<&'static str>,

    /// AC JWKS endpoint reachability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ac_jwks: Option<&'static str>,

    /// Error message (generic, no infrastructure details).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

// ============================================================================
// Meeting Create API Models
// ============================================================================

/// Maximum display name length for meetings (in bytes).
pub const MAX_MEETING_DISPLAY_NAME_LENGTH: usize = 255;

/// Minimum display name length for meetings (in bytes, after trimming).
pub const MIN_MEETING_DISPLAY_NAME_LENGTH: usize = 1;

/// Minimum number of participants for a meeting.
pub const MIN_PARTICIPANTS: i32 = 2;

/// Default maximum participants if not specified in request.
pub const DEFAULT_MAX_PARTICIPANTS: i32 = 100;

/// Request to create a new meeting.
///
/// Sent by authenticated users to create a meeting in their organization.
/// All settings fields are optional; secure defaults are applied server-side.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMeetingRequest {
    /// Meeting display name (required, 1-255 bytes after trimming).
    pub display_name: String,

    /// Maximum number of participants (optional, default 100, min 2).
    pub max_participants: Option<i32>,

    /// Scheduled start time (optional, NULL = ad-hoc meeting).
    pub scheduled_start_time: Option<DateTime<Utc>>,

    /// Whether end-to-end encryption is enabled (default: true).
    pub enable_e2e_encryption: Option<bool>,

    /// Whether authentication is required to join (default: true).
    pub require_auth: Option<bool>,

    /// Whether recording is enabled (default: false).
    pub recording_enabled: Option<bool>,

    /// Whether anonymous guests can join (default: false).
    pub allow_guests: Option<bool>,

    /// Whether external org users can join (default: false).
    pub allow_external_participants: Option<bool>,

    /// Whether waiting room is enabled (default: true).
    pub waiting_room_enabled: Option<bool>,
}

impl CreateMeetingRequest {
    /// Validate the request fields.
    ///
    /// # Errors
    ///
    /// Returns an error message if validation fails.
    pub fn validate(&self) -> Result<(), &'static str> {
        let display_name = self.display_name.trim();

        if display_name.len() < MIN_MEETING_DISPLAY_NAME_LENGTH {
            return Err("Display name is required");
        }

        if display_name.len() > MAX_MEETING_DISPLAY_NAME_LENGTH {
            return Err("Display name must be at most 255 characters");
        }

        if let Some(max_participants) = self.max_participants {
            if max_participants < MIN_PARTICIPANTS {
                return Err("Maximum participants must be at least 2");
            }
        }

        Ok(())
    }
}

/// Response after creating a meeting.
///
/// Returned by `POST /api/v1/meetings` with status 201 Created.
/// Excludes `join_token_secret` and other internal fields.
#[derive(Debug, Clone, Serialize)]
pub struct CreateMeetingResponse {
    /// Unique meeting identifier.
    pub meeting_id: Uuid,

    /// Short meeting code for joining.
    pub meeting_code: String,

    /// Meeting display name.
    pub display_name: String,

    /// Current meeting status.
    pub status: String,

    /// Maximum number of participants.
    pub max_participants: i32,

    /// Whether end-to-end encryption is enabled.
    pub enable_e2e_encryption: bool,

    /// Whether authentication is required to join.
    pub require_auth: bool,

    /// Whether recording is enabled.
    pub recording_enabled: bool,

    /// Whether anonymous guests can join.
    pub allow_guests: bool,

    /// Whether external org users can join.
    pub allow_external_participants: bool,

    /// Whether waiting room is enabled.
    pub waiting_room_enabled: bool,

    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl From<MeetingRow> for CreateMeetingResponse {
    fn from(row: MeetingRow) -> Self {
        Self {
            meeting_id: row.meeting_id,
            meeting_code: row.meeting_code,
            display_name: row.display_name,
            status: row.status,
            max_participants: row.max_participants,
            enable_e2e_encryption: row.enable_e2e_encryption,
            require_auth: row.require_auth,
            recording_enabled: row.recording_enabled,
            allow_guests: row.allow_guests,
            allow_external_participants: row.allow_external_participants,
            waiting_room_enabled: row.waiting_room_enabled,
            created_at: row.created_at,
        }
    }
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

    // ========================================================================
    // CreateMeetingRequest Tests
    // ========================================================================

    #[test]
    fn test_create_meeting_request_deserialization() {
        let json = r#"{"display_name":"Team Standup","max_participants":10}"#;
        let request: CreateMeetingRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "Team Standup");
        assert_eq!(request.max_participants, Some(10));
        assert_eq!(request.enable_e2e_encryption, None);
        assert_eq!(request.require_auth, None);
    }

    #[test]
    fn test_create_meeting_request_minimal() {
        let json = r#"{"display_name":"Quick Call"}"#;
        let request: CreateMeetingRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "Quick Call");
        assert_eq!(request.max_participants, None);
        assert_eq!(request.scheduled_start_time, None);
    }

    #[test]
    fn test_create_meeting_request_rejects_unknown_fields() {
        let json = r#"{"display_name":"Test","extra_field":"value"}"#;
        let result: Result<CreateMeetingRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject unknown fields");
    }

    #[test]
    fn test_create_meeting_request_validation_success() {
        let request = CreateMeetingRequest {
            display_name: "Team Meeting".to_string(),
            max_participants: Some(10),
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_create_meeting_request_validation_empty_name() {
        let request = CreateMeetingRequest {
            display_name: "".to_string(),
            max_participants: None,
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Display name is required");
    }

    #[test]
    fn test_create_meeting_request_validation_whitespace_name() {
        let request = CreateMeetingRequest {
            display_name: "   ".to_string(),
            max_participants: None,
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        let result = request.validate();
        assert!(result.is_err(), "Should reject whitespace-only name");
    }

    #[test]
    fn test_create_meeting_request_validation_long_name() {
        let request = CreateMeetingRequest {
            display_name: "a".repeat(256),
            max_participants: None,
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Display name must be at most 255 characters"
        );
    }

    #[test]
    fn test_create_meeting_request_validation_max_participants_too_low() {
        let request = CreateMeetingRequest {
            display_name: "Test".to_string(),
            max_participants: Some(1),
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        let result = request.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Maximum participants must be at least 2"
        );
    }

    #[test]
    fn test_create_meeting_request_validation_max_participants_minimum() {
        let request = CreateMeetingRequest {
            display_name: "Test".to_string(),
            max_participants: Some(2),
            scheduled_start_time: None,
            enable_e2e_encryption: None,
            require_auth: None,
            recording_enabled: None,
            allow_guests: None,
            allow_external_participants: None,
            waiting_room_enabled: None,
        };
        assert!(request.validate().is_ok(), "max_participants=2 should pass");
    }

    // ========================================================================
    // CreateMeetingResponse Tests
    // ========================================================================

    #[test]
    fn test_create_meeting_response_serialization() {
        let response = CreateMeetingResponse {
            meeting_id: Uuid::nil(),
            meeting_code: "ABC123def456".to_string(),
            display_name: "Test Meeting".to_string(),
            status: "scheduled".to_string(),
            max_participants: 100,
            enable_e2e_encryption: true,
            require_auth: true,
            recording_enabled: false,
            allow_guests: false,
            allow_external_participants: false,
            waiting_room_enabled: true,
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&response).expect("serialization should succeed");

        assert!(json.contains("\"meeting_code\":\"ABC123def456\""));
        assert!(json.contains("\"status\":\"scheduled\""));
        assert!(json.contains("\"enable_e2e_encryption\":true"));
        assert!(json.contains("\"require_auth\":true"));
        assert!(json.contains("\"recording_enabled\":false"));
        assert!(json.contains("\"allow_guests\":false"));
        assert!(json.contains("\"waiting_room_enabled\":true"));
        // Must NOT contain join_token_secret
        assert!(!json.contains("join_token_secret"));
    }

    #[test]
    fn test_create_meeting_response_from_meeting_row() {
        let row = MeetingRow {
            meeting_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            created_by_user_id: Uuid::new_v4(),
            display_name: "From Row".to_string(),
            meeting_code: "TestCode1234".to_string(),
            join_token_secret: "should_not_appear_in_response".to_string(),
            max_participants: 50,
            enable_e2e_encryption: true,
            require_auth: true,
            recording_enabled: false,
            meeting_controller_id: None,
            meeting_controller_region: None,
            status: "scheduled".to_string(),
            scheduled_start_time: None,
            actual_start_time: None,
            actual_end_time: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            allow_guests: false,
            allow_external_participants: false,
            waiting_room_enabled: true,
        };

        let response = CreateMeetingResponse::from(row.clone());

        assert_eq!(response.meeting_id, row.meeting_id);
        assert_eq!(response.meeting_code, "TestCode1234");
        assert_eq!(response.display_name, "From Row");
        assert_eq!(response.max_participants, 50);
        assert_eq!(response.status, "scheduled");

        // Serialize and verify no join_token_secret
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("join_token_secret"));
        assert!(!json.contains("should_not_appear_in_response"));
    }
}
