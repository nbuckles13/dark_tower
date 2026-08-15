//! Global Controller models.
//!
//! Contains data types used across the Global Controller service.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
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
#[serde(rename_all = "camelCase")]
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
// Participant Models
// ============================================================================

/// Meeting participant record from database.
///
/// Represents a participant in a meeting, tracking their type (member/external),
/// role (host/participant), and active status via `left_at`.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // Used by integration tests and future join handler
pub struct Participant {
    /// Unique participant record identifier.
    pub participant_id: Uuid,

    /// Meeting the participant belongs to.
    pub meeting_id: Uuid,

    /// User identifier (None for anonymous guests).
    pub user_id: Option<Uuid>,

    /// Participant's display name.
    pub display_name: String,

    /// Whether participant is a same-org member or external/guest.
    pub participant_type: String,

    /// Participant role: host or participant.
    pub role: String,

    /// When the participant joined.
    pub joined_at: DateTime<Utc>,

    /// When the participant left (None = still active).
    pub left_at: Option<DateTime<Utc>>,
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
/// `Debug` is hand-rolled below rather than derived — `join_token_secret` is a
/// 256-bit CSPRNG credential from which join tokens are minted, and a derived
/// `Debug` puts it one `?row` away from the log stream. Same treatment as
/// `Config` (`config.rs`) and `UserContext` (`auth/claims.rs`) in this crate.
#[derive(Clone)]
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

/// Custom Debug implementation that redacts `join_token_secret`.
///
/// The secret is a 256-bit CSPRNG credential from which meeting join tokens are
/// minted; a derived `Debug` would put it one `tracing::debug!(?row, …)` or
/// `format!("{row:?}")` away from a log stream that ships to a collector.
/// `no-secrets-in-logs` cannot see this — it is a lexical rule over identifiers
/// and string literals, and cannot follow a type through a `Debug` impl.
///
/// Every other field is passed through: redacting the struct wholesale would
/// make the type useless for the debugging it exists to serve. That pass-through
/// is a decision rather than an omission — `join_token_secret` is the only
/// credential in this struct. `meeting_code` in particular is deliberately in the
/// clear: it is a public identifier distributed in join URLs, and it does not
/// admit a join on its own (`require_auth` defaults true, guest access requires
/// `allow_guests`, and the join path still enforces org and role checks).
///
/// A derive has no per-field intent to misread; the moment it is hand-rolled,
/// every unredacted field silently asserts "considered, and safe" in a form a
/// reader cannot distinguish from "copied across without thinking". So: a field
/// added here that *is* secret belongs on the redaction list above.
impl fmt::Debug for MeetingRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeetingRow")
            .field("meeting_id", &self.meeting_id)
            .field("org_id", &self.org_id)
            .field("created_by_user_id", &self.created_by_user_id)
            .field("display_name", &self.display_name)
            .field("meeting_code", &self.meeting_code)
            .field("join_token_secret", &"[REDACTED]")
            .field("max_participants", &self.max_participants)
            .field("enable_e2e_encryption", &self.enable_e2e_encryption)
            .field("require_auth", &self.require_auth)
            .field("recording_enabled", &self.recording_enabled)
            .field("meeting_controller_id", &self.meeting_controller_id)
            .field("meeting_controller_region", &self.meeting_controller_region)
            .field("status", &self.status)
            .field("scheduled_start_time", &self.scheduled_start_time)
            .field("actual_start_time", &self.actual_start_time)
            .field("actual_end_time", &self.actual_end_time)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("allow_guests", &self.allow_guests)
            .field(
                "allow_external_participants",
                &self.allow_external_participants,
            )
            .field("waiting_room_enabled", &self.waiting_room_enabled)
            .finish()
    }
}

/// Response for joining a meeting.
///
/// Returned by `GET /v1/meetings/{code}` and `POST /v1/meetings/{code}/guest-token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

    /// `MeetingRow`'s `Debug` must never render `join_token_secret`.
    ///
    /// Without this, the redaction is a comment: re-adding `Debug` to the derive
    /// list would silently restore the leak, and `no-secrets-in-logs` cannot see
    /// it (a lexical rule cannot follow a type through a `Debug` impl).
    #[test]
    fn meeting_row_debug_redacts_join_token_secret() {
        let secret = "d34db33fcafebabe0123456789abcdef0123456789abcdef0123456789abcdef";
        let row = MeetingRow {
            meeting_id: Uuid::new_v4(),
            org_id: Uuid::new_v4(),
            created_by_user_id: Uuid::new_v4(),
            display_name: "Redaction Probe".to_string(),
            meeting_code: "REDACT00001A".to_string(),
            join_token_secret: secret.to_string(),
            max_participants: 10,
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

        let rendered = format!("{row:?}");
        assert!(
            !rendered.contains(secret),
            "Debug output must not contain the join token secret"
        );
        assert!(
            rendered.contains("join_token_secret: \"[REDACTED]\""),
            "the field must still appear, redacted, so its absence is not mistaken \
             for the struct lacking one: {rendered}"
        );
        // Non-secret fields still render — a wholesale redaction would make the
        // type useless for the debugging it exists to serve.
        assert!(rendered.contains("REDACT00001A"));
        assert!(rendered.contains("Redaction Probe"));
    }

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
        assert!(json.contains("\"expiresIn\":900"));
        assert!(json.contains("\"meetingName\":\"Test Meeting\""));
        assert!(json.contains("\"mcId\":\"mc-001\""));
        assert!(json.contains("\"grpcEndpoint\":\"https://mc.example.com:50051\""));
    }

    #[test]
    fn test_mc_assignment_info_serialization() {
        let assignment = McAssignmentInfo {
            mc_id: "mc-test".to_string(),
            webtransport_endpoint: Some("https://mc:443".to_string()),
            grpc_endpoint: "https://mc:50051".to_string(),
        };

        let json = serde_json::to_string(&assignment).expect("serialization should succeed");
        assert!(json.contains("\"mcId\":\"mc-test\""));
        assert!(json.contains("\"webtransportEndpoint\":\"https://mc:443\""));
        assert!(json.contains("\"grpcEndpoint\":\"https://mc:50051\""));
    }

    #[test]
    fn test_mc_assignment_info_serialization_no_webtransport() {
        let assignment = McAssignmentInfo {
            mc_id: "mc-test".to_string(),
            webtransport_endpoint: None,
            grpc_endpoint: "https://mc:50051".to_string(),
        };

        let json = serde_json::to_string(&assignment).expect("serialization should succeed");
        assert!(json.contains("\"mcId\":\"mc-test\""));
        // webtransportEndpoint should be omitted when None
        assert!(!json.contains("webtransportEndpoint"));
        assert!(json.contains("\"grpcEndpoint\":\"https://mc:50051\""));
    }

    #[test]
    fn test_guest_join_request_deserialization() {
        let json = r#"{"displayName":"John Doe","captchaToken":"abc123"}"#;
        let request: GuestJoinRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "John Doe");
        assert_eq!(request.captcha_token, "abc123");
    }

    #[test]
    fn test_guest_join_request_rejects_unknown_fields() {
        let json = r#"{"displayName":"John","captchaToken":"abc","extra":"field"}"#;
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
        let json = r#"{"allowGuests":true,"waitingRoomEnabled":false}"#;
        let request: UpdateMeetingSettingsRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.allow_guests, Some(true));
        assert_eq!(request.allow_external_participants, None);
        assert_eq!(request.waiting_room_enabled, Some(false));
    }

    #[test]
    fn test_update_meeting_settings_request_rejects_unknown_fields() {
        let json = r#"{"allowGuests":true,"extra":"field"}"#;
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
        let json = r#"{"displayName":"Team Standup","maxParticipants":10}"#;
        let request: CreateMeetingRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "Team Standup");
        assert_eq!(request.max_participants, Some(10));
        assert_eq!(request.enable_e2e_encryption, None);
        assert_eq!(request.require_auth, None);
    }

    #[test]
    fn test_create_meeting_request_minimal() {
        let json = r#"{"displayName":"Quick Call"}"#;
        let request: CreateMeetingRequest =
            serde_json::from_str(json).expect("deserialization should succeed");

        assert_eq!(request.display_name, "Quick Call");
        assert_eq!(request.max_participants, None);
        assert_eq!(request.scheduled_start_time, None);
    }

    #[test]
    fn test_create_meeting_request_rejects_unknown_fields() {
        let json = r#"{"displayName":"Test","extra_field":"value"}"#;
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

        assert!(json.contains("\"meetingCode\":\"ABC123def456\""));
        assert!(json.contains("\"status\":\"scheduled\""));
        assert!(json.contains("\"enableE2eEncryption\":true"));
        assert!(json.contains("\"requireAuth\":true"));
        assert!(json.contains("\"recordingEnabled\":false"));
        assert!(json.contains("\"allowGuests\":false"));
        assert!(json.contains("\"waitingRoomEnabled\":true"));
        // Must NOT contain join_token_secret (neither snake_case nor camelCase form).
        assert!(!json.contains("join_token_secret"));
        assert!(!json.contains("joinTokenSecret"));
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

        // Serialize and verify no join_token_secret (neither snake_case nor camelCase form).
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("join_token_secret"));
        assert!(!json.contains("joinTokenSecret"));
        assert!(!json.contains("should_not_appear_in_response"));
    }

    // ========================================================================
    // WIRE-SHAPE LOCKS (R-53 / task #23; GC mirror of AC task #46 `f5fc4b4`)
    //
    // RENAME TRIPWIRE. GC's wire structs are `#[serde(rename_all = "camelCase")]`
    // (task #23, commit `92d963b`). Unlike AC, GC has NO OAuth/RFC-6749 endpoints,
    // so there is NO snake_case carve-out — EVERY GC wire response is camelCase
    // (including `JoinMeetingResponse.expires_in` -> `expiresIn`; GC's join /
    // guest-token are NOT RFC 6749 token endpoints). These tests serialize a real
    // instance and pin the EXACT key-set via two independent checks per struct:
    //   (a) full `BTreeSet` key-set EQUALITY  — catches an added / removed / renamed key;
    //   (b) "no serialized key contains `_`"  — catches a PARTIAL rename sweep that
    //       leaves a single field snake_case while the rest go camel.
    // They complement (do not duplicate) the substring serialization tests above,
    // which spot-check individual keys + VALUES but do NOT assert closed-set
    // equality and so would miss an added/removed key or a sibling snake-revert.
    //
    // IF ONE FAILS DURING A RENAME SWEEP: the SDK + env-tests fixtures + every HTTP
    // client depend on these exact camelCase keys (R-53). DO NOT silently re-baseline
    // — confirm the wire contract and update the lock + SDK + env-tests fixtures in
    // lockstep. The all-camelCase, no-mixed-scheme rule is owned by task #51.
    // ========================================================================

    /// Collect the top-level serialized JSON object key-set of `value`.
    fn wire_key_set(value: &serde_json::Value) -> std::collections::BTreeSet<String> {
        value
            .as_object()
            .expect("wire shape must be a JSON object")
            .keys()
            .cloned()
            .collect()
    }

    /// Assert no top-level serialized key contains `_` (i.e. all-camelCase,
    /// no surviving snake_case field after a rename sweep).
    fn assert_no_snake_keys(value: &serde_json::Value, ctx: &str) {
        for key in value
            .as_object()
            .expect("wire shape must be a JSON object")
            .keys()
        {
            assert!(
                !key.contains('_'),
                "{ctx} wire key `{key}` contains `_` — a snake_case field survived. \
                 GC wire shape is ALL camelCase (R-53, no OAuth carve-out). \
                 DO NOT silently re-baseline; re-camelCase the field (rule owned by task #51)."
            );
        }
    }

    #[test]
    fn test_create_meeting_response_wire_shape_stays_camel() {
        let response = CreateMeetingResponse {
            meeting_id: Uuid::nil(),
            meeting_code: "ABC123def456".to_string(),
            display_name: "Lock".to_string(),
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
        let value = serde_json::to_value(&response).expect("should serialize");
        let expected: std::collections::BTreeSet<String> = [
            "meetingId",                 // camelCase (R-53)
            "meetingCode",               // camelCase (R-53)
            "displayName",               // camelCase (R-53)
            "status",                    // scheme-invariant single word
            "maxParticipants",           // camelCase (R-53)
            "enableE2eEncryption",       // camelCase (R-53)
            "requireAuth",               // camelCase (R-53)
            "recordingEnabled",          // camelCase (R-53)
            "allowGuests",               // camelCase (R-53)
            "allowExternalParticipants", // camelCase (R-53)
            "waitingRoomEnabled",        // camelCase (R-53)
            "createdAt",                 // camelCase (R-53)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "CreateMeetingResponse wire key-set drifted from the R-53 camelCase shape"
        );
        assert_no_snake_keys(&value, "CreateMeetingResponse");
        // Credential-non-leak invariant (both forms), preserved at struct scope.
        let json = serde_json::to_string(&response).expect("should serialize");
        assert!(!json.contains("join_token_secret"));
        assert!(!json.contains("joinTokenSecret"));
    }

    #[test]
    fn test_meeting_response_wire_shape_stays_camel() {
        let response = MeetingResponse {
            meeting_id: Uuid::nil(),
            display_name: "Lock".to_string(),
            meeting_code: "ABC123def456".to_string(),
            status: "scheduled".to_string(),
            allow_guests: true,
            allow_external_participants: false,
            waiting_room_enabled: true,
            updated_at: Utc::now(),
        };
        let value = serde_json::to_value(&response).expect("should serialize");
        let expected: std::collections::BTreeSet<String> = [
            "meetingId",                 // camelCase (R-53)
            "displayName",               // camelCase (R-53)
            "meetingCode",               // camelCase (R-53)
            "status",                    // scheme-invariant single word
            "allowGuests",               // camelCase (R-53)
            "allowExternalParticipants", // camelCase (R-53)
            "waitingRoomEnabled",        // camelCase (R-53)
            "updatedAt",                 // camelCase (R-53)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "MeetingResponse (settings PATCH) wire key-set drifted from the R-53 camelCase shape"
        );
        assert_no_snake_keys(&value, "MeetingResponse");
    }

    #[test]
    fn test_join_meeting_response_wire_shape_stays_camel() {
        let response = JoinMeetingResponse {
            token: "eyJ.test.sig".to_string(),
            expires_in: 900,
            meeting_id: Uuid::nil(),
            meeting_name: "Lock".to_string(),
            mc_assignment: McAssignmentInfo {
                mc_id: "mc-001".to_string(),
                webtransport_endpoint: Some("https://mc:443".to_string()),
                grpc_endpoint: "https://mc:50051".to_string(),
            },
        };
        let value = serde_json::to_value(&response).expect("should serialize");
        let expected: std::collections::BTreeSet<String> = [
            "token",        // scheme-invariant single word
            "expiresIn",    // camelCase (R-53) — NOT RFC 6749; GC join is not an OAuth endpoint
            "meetingId",    // camelCase (R-53)
            "meetingName",  // camelCase (R-53)
            "mcAssignment", // camelCase (R-53)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "JoinMeetingResponse wire key-set drifted from the R-53 camelCase shape — \
             expiresIn is camelCase (GC has NO OAuth carve-out; rule owned by task #51)"
        );
        assert_no_snake_keys(&value, "JoinMeetingResponse");
    }

    #[test]
    fn test_mc_assignment_info_wire_shape_stays_camel() {
        // `webtransport_endpoint` is `skip_serializing_if = "Option::is_none"`, so the
        // `webtransportEndpoint` key is ABSENT when None. Populate it `Some(..)` here
        // to lock the FULL key-set (all 3) — a rename sweep can hit McAssignmentInfo
        // independently of JoinMeetingResponse.
        let assignment = McAssignmentInfo {
            mc_id: "mc-001".to_string(),
            webtransport_endpoint: Some("https://mc:443".to_string()),
            grpc_endpoint: "https://mc:50051".to_string(),
        };
        let value = serde_json::to_value(&assignment).expect("should serialize");
        let expected: std::collections::BTreeSet<String> = [
            "mcId",                 // camelCase (R-53)
            "webtransportEndpoint", // camelCase (R-53); present here because Some(..)
            "grpcEndpoint",         // camelCase (R-53)
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "McAssignmentInfo wire key-set drifted from the R-53 camelCase shape"
        );
        assert_no_snake_keys(&value, "McAssignmentInfo");
    }

    #[test]
    fn test_readiness_response_wire_shape_stays_camel() {
        let response = ReadinessResponse {
            status: "ready",
            database: Some("healthy"),
            ac_jwks: Some("available"),
            error: None,
        };
        let value = serde_json::to_value(&response).expect("should serialize");
        // `error` is `skip_serializing_if = Option::is_none` -> absent here (None).
        let expected: std::collections::BTreeSet<String> = [
            "status",   // scheme-invariant single word
            "database", // scheme-invariant single word
            "acJwks",   // camelCase (R-53) — NOT `ac_jwks`
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "ReadinessResponse wire key-set drifted — `acJwks` must stay camelCase (R-53)"
        );
        assert_no_snake_keys(&value, "ReadinessResponse");
    }

    /// Request-struct deserialize locks: the camelCase wire body MUST deserialize,
    /// and the snake_case body MUST be rejected (`deny_unknown_fields` makes a
    /// snake key "unknown"). This pins the request side of the R-53 contract.
    #[test]
    fn test_create_meeting_request_wire_shape_is_camel() {
        // camelCase body accepted.
        let camel = r#"{"displayName":"X","maxParticipants":10,"enableE2eEncryption":false}"#;
        serde_json::from_str::<CreateMeetingRequest>(camel)
            .expect("camelCase CreateMeetingRequest body must deserialize (R-53)");
        // snake_case body rejected (deny_unknown_fields: snake keys are "unknown").
        let snake = r#"{"display_name":"X","max_participants":10}"#;
        assert!(
            serde_json::from_str::<CreateMeetingRequest>(snake).is_err(),
            "snake_case CreateMeetingRequest body must be REJECTED (R-53 camelCase contract)"
        );
    }

    #[test]
    fn test_guest_join_request_wire_shape_is_camel() {
        let camel = r#"{"displayName":"John Doe","captchaToken":"abc123"}"#;
        serde_json::from_str::<GuestJoinRequest>(camel)
            .expect("camelCase GuestJoinRequest body must deserialize (R-53)");
        let snake = r#"{"display_name":"John Doe","captcha_token":"abc123"}"#;
        assert!(
            serde_json::from_str::<GuestJoinRequest>(snake).is_err(),
            "snake_case GuestJoinRequest body must be REJECTED (R-53 camelCase contract)"
        );
    }

    #[test]
    fn test_update_meeting_settings_request_wire_shape_is_camel() {
        let camel =
            r#"{"allowGuests":true,"allowExternalParticipants":false,"waitingRoomEnabled":true}"#;
        serde_json::from_str::<UpdateMeetingSettingsRequest>(camel)
            .expect("camelCase UpdateMeetingSettingsRequest body must deserialize (R-53)");
        let snake = r#"{"allow_guests":true}"#;
        assert!(
            serde_json::from_str::<UpdateMeetingSettingsRequest>(snake).is_err(),
            "snake_case UpdateMeetingSettingsRequest body must be REJECTED (R-53 camelCase contract)"
        );
    }

    /// `HealthResponse` has NO `rename_all`; its fields (`status`/`region`/`database`)
    /// are scheme-invariant SINGLE WORDS, so the all-camelCase invariant trivially
    /// holds — this is NOT a snake_case carve-out. It is DEAD CODE (`/health` returns
    /// plain text "OK" per ADR-0012) -> struct-only lock, no HTTP-integration lock.
    /// Kept for uniformity with the other response locks; its only forward value is
    /// catching a future MULTI-WORD field addition that would need camelCasing.
    /// (The `rename_all="snake_case"` at models/mod.rs:13 is on the `MeetingStatus`
    /// ENUM, NOT this struct — a different type.)
    #[test]
    fn test_health_response_wire_shape_has_no_snake_keys() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            region: "us-east-1".to_string(),
            database: Some("healthy".to_string()),
        };
        let value = serde_json::to_value(&response).expect("should serialize");
        let expected: std::collections::BTreeSet<String> = ["status", "region", "database"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            wire_key_set(&value),
            expected,
            "HealthResponse wire key-set drifted (dead-code struct; single-word fields)"
        );
        assert_no_snake_keys(&value, "HealthResponse");
    }
}
