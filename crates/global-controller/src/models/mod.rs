//! Global Controller models.
//!
//! Contains data types used across the Global Controller service.

use serde::{Deserialize, Serialize};

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
}
