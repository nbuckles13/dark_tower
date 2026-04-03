//! Shared types for internal meeting/guest token requests between GC and AC.
//!
//! These types define the API contract for the GC -> AC internal token
//! endpoints (ADR-0020). Both services import from here to ensure
//! compile-time type agreement and prevent serialization mismatches.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Request to AC for a meeting token (authenticated user).
///
/// Sent by GC to `POST /api/v1/auth/internal/meeting-token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTokenRequest {
    /// User ID of the participant.
    pub subject_user_id: Uuid,

    /// Meeting ID.
    pub meeting_id: Uuid,

    /// Organization that owns the meeting.
    pub meeting_org_id: Uuid,

    /// User's home organization. Equal to `meeting_org_id` for same-org
    /// joins; different for cross-org joins.
    pub home_org_id: Uuid,

    /// Type of participant.
    #[serde(default)]
    pub participant_type: ParticipantType,

    /// Role in the meeting.
    #[serde(default)]
    pub role: MeetingRole,

    /// Capabilities granted to this participant.
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Token TTL in seconds (max 900 = 15 minutes).
    #[serde(default = "default_meeting_ttl")]
    pub ttl_seconds: u32,
}

/// Request to AC for a guest token (anonymous user).
///
/// Sent by GC to `POST /api/v1/auth/internal/guest-token`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_waiting_room")]
    pub waiting_room: bool,

    /// Token TTL in seconds (max 900 = 15 minutes).
    #[serde(default = "default_meeting_ttl")]
    pub ttl_seconds: u32,
}

/// Response from AC for token requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// The issued JWT token.
    pub token: String,

    /// Token expiration in seconds from now.
    pub expires_in: u32,
}

/// Participant type in a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantType {
    /// Member of the meeting organization.
    #[default]
    Member,
    /// User from a different organization.
    External,
    /// Anonymous guest (no authentication).
    Guest,
}

impl ParticipantType {
    /// Convert to string for JWT claims.
    #[must_use]
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

/// Role within a meeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRole {
    /// Meeting host with full control.
    Host,
    /// Regular participant.
    #[default]
    Participant,
    /// Guest (waiting room or limited privileges).
    Guest,
}

impl MeetingRole {
    /// Convert to string for JWT claims.
    #[must_use]
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

/// Default TTL for meeting tokens (15 minutes).
fn default_meeting_ttl() -> u32 {
    900
}

/// Default waiting room setting (true).
fn default_waiting_room() -> bool {
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // =========================================================================
    // ParticipantType tests
    // =========================================================================

    #[test]
    fn test_participant_type_as_str() {
        assert_eq!(ParticipantType::Member.as_str(), "member");
        assert_eq!(ParticipantType::External.as_str(), "external");
        assert_eq!(ParticipantType::Guest.as_str(), "guest");
    }

    #[test]
    fn test_participant_type_display() {
        assert_eq!(format!("{}", ParticipantType::Member), "member");
        assert_eq!(format!("{}", ParticipantType::External), "external");
        assert_eq!(format!("{}", ParticipantType::Guest), "guest");
    }

    #[test]
    fn test_participant_type_default() {
        assert_eq!(ParticipantType::default(), ParticipantType::Member);
    }

    #[test]
    fn test_participant_type_serde_roundtrip() {
        for variant in [
            ParticipantType::Member,
            ParticipantType::External,
            ParticipantType::Guest,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: ParticipantType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    #[test]
    fn test_participant_type_deserialize_from_string() {
        let member: ParticipantType = serde_json::from_str("\"member\"").unwrap();
        assert_eq!(member, ParticipantType::Member);
        let external: ParticipantType = serde_json::from_str("\"external\"").unwrap();
        assert_eq!(external, ParticipantType::External);
        let guest: ParticipantType = serde_json::from_str("\"guest\"").unwrap();
        assert_eq!(guest, ParticipantType::Guest);
    }

    // =========================================================================
    // MeetingRole tests
    // =========================================================================

    #[test]
    fn test_meeting_role_as_str() {
        assert_eq!(MeetingRole::Host.as_str(), "host");
        assert_eq!(MeetingRole::Participant.as_str(), "participant");
        assert_eq!(MeetingRole::Guest.as_str(), "guest");
    }

    #[test]
    fn test_meeting_role_display() {
        assert_eq!(format!("{}", MeetingRole::Host), "host");
        assert_eq!(format!("{}", MeetingRole::Participant), "participant");
        assert_eq!(format!("{}", MeetingRole::Guest), "guest");
    }

    #[test]
    fn test_meeting_role_default() {
        assert_eq!(MeetingRole::default(), MeetingRole::Participant);
    }

    #[test]
    fn test_meeting_role_serde_roundtrip() {
        for variant in [
            MeetingRole::Host,
            MeetingRole::Participant,
            MeetingRole::Guest,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: MeetingRole = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }

    // =========================================================================
    // MeetingTokenRequest tests
    // =========================================================================

    #[test]
    fn test_meeting_token_request_serde_roundtrip() {
        let request = MeetingTokenRequest {
            subject_user_id: Uuid::from_u128(1),
            meeting_id: Uuid::from_u128(2),
            meeting_org_id: Uuid::from_u128(3),
            home_org_id: Uuid::from_u128(3), // same-org
            participant_type: ParticipantType::Member,
            role: MeetingRole::Host,
            capabilities: vec!["audio".to_string(), "video".to_string()],
            ttl_seconds: 600,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: MeetingTokenRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.subject_user_id, request.subject_user_id);
        assert_eq!(deserialized.meeting_id, request.meeting_id);
        assert_eq!(deserialized.meeting_org_id, request.meeting_org_id);
        assert_eq!(deserialized.home_org_id, request.home_org_id);
        assert_eq!(deserialized.participant_type, request.participant_type);
        assert_eq!(deserialized.role, request.role);
        assert_eq!(deserialized.capabilities, request.capabilities);
        assert_eq!(deserialized.ttl_seconds, request.ttl_seconds);
    }

    #[test]
    fn test_meeting_token_request_home_org_id_always_serialized() {
        let request = MeetingTokenRequest {
            subject_user_id: Uuid::from_u128(1),
            meeting_id: Uuid::from_u128(2),
            meeting_org_id: Uuid::from_u128(3),
            home_org_id: Uuid::from_u128(3),
            participant_type: ParticipantType::Member,
            role: MeetingRole::Participant,
            capabilities: vec![],
            ttl_seconds: 900,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(
            json.contains("home_org_id"),
            "home_org_id must always be present in serialized output"
        );
    }

    #[test]
    fn test_meeting_token_request_defaults_on_minimal_json() {
        let json = r#"{
            "subject_user_id": "550e8400-e29b-41d4-a716-446655440001",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003",
            "home_org_id": "550e8400-e29b-41d4-a716-446655440004"
        }"#;

        let req: MeetingTokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.participant_type,
            ParticipantType::Member,
            "default participant_type"
        );
        assert_eq!(req.role, MeetingRole::Participant, "default role");
        assert!(req.capabilities.is_empty(), "default capabilities");
        assert_eq!(req.ttl_seconds, 900, "default ttl_seconds");
    }

    // =========================================================================
    // GuestTokenRequest tests
    // =========================================================================

    #[test]
    fn test_guest_token_request_serde_roundtrip() {
        let request = GuestTokenRequest {
            guest_id: Uuid::from_u128(100),
            display_name: "Test Guest".to_string(),
            meeting_id: Uuid::from_u128(2),
            meeting_org_id: Uuid::from_u128(3),
            waiting_room: true,
            ttl_seconds: 600,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: GuestTokenRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.guest_id, request.guest_id);
        assert_eq!(deserialized.display_name, request.display_name);
        assert_eq!(deserialized.meeting_id, request.meeting_id);
        assert_eq!(deserialized.meeting_org_id, request.meeting_org_id);
        assert_eq!(deserialized.waiting_room, request.waiting_room);
        assert_eq!(deserialized.ttl_seconds, request.ttl_seconds);
    }

    #[test]
    fn test_guest_token_request_defaults_on_minimal_json() {
        let json = r#"{
            "guest_id": "550e8400-e29b-41d4-a716-446655440001",
            "display_name": "Alice",
            "meeting_id": "550e8400-e29b-41d4-a716-446655440002",
            "meeting_org_id": "550e8400-e29b-41d4-a716-446655440003"
        }"#;

        let req: GuestTokenRequest = serde_json::from_str(json).unwrap();
        assert!(req.waiting_room, "default waiting_room should be true");
        assert_eq!(req.ttl_seconds, 900, "default ttl_seconds should be 900");
    }

    // =========================================================================
    // TokenResponse tests
    // =========================================================================

    #[test]
    fn test_token_response_serde_roundtrip() {
        let response = TokenResponse {
            token: "eyJ.test.token".to_string(),
            expires_in: 900,
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: TokenResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.token, response.token);
        assert_eq!(deserialized.expires_in, response.expires_in);
    }
}
