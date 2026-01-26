//! Pre-configured test data fixtures for MC testing.
//!
//! Provides builders and test data for:
//! - Meetings with various configurations
//! - Participants with different roles
//! - Session binding tokens
//! - JoinRequest/JoinResponse messages

// TODO (Phase 6b): Full implementation with more fixtures
// pub mod meetings;
// pub mod participants;
// pub mod binding_tokens;
// pub mod messages;

use uuid::Uuid;

/// Test meeting fixture.
#[derive(Debug, Clone)]
pub struct TestMeeting {
    /// Meeting ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Maximum participants.
    pub max_participants: u32,
    /// E2E encryption enabled.
    pub e2e_enabled: bool,
}

impl TestMeeting {
    /// Create a new test meeting with the given ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: "Test Meeting".to_string(),
            max_participants: 100,
            e2e_enabled: false,
        }
    }

    /// Create a test meeting with a random ID.
    #[must_use]
    pub fn random() -> Self {
        Self::new(format!("meeting-{}", Uuid::new_v4()))
    }

    /// Set the display name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = name.into();
        self
    }

    /// Set the maximum participants.
    #[must_use]
    pub fn with_max_participants(mut self, max: u32) -> Self {
        self.max_participants = max;
        self
    }

    /// Enable E2E encryption.
    #[must_use]
    pub fn with_e2e(mut self) -> Self {
        self.e2e_enabled = true;
        self
    }
}

/// Test participant fixture.
#[derive(Debug, Clone)]
pub struct TestParticipant {
    /// Participant ID.
    pub participant_id: String,
    /// User ID.
    pub user_id: String,
    /// Display name.
    pub name: String,
    /// Whether this is a guest.
    pub is_guest: bool,
}

impl TestParticipant {
    /// Create a new test participant with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            participant_id: format!("part-{}", Uuid::new_v4()),
            user_id: format!("user-{}", Uuid::new_v4()),
            name,
            is_guest: false,
        }
    }

    /// Create a test participant with a random name.
    #[must_use]
    pub fn random() -> Self {
        Self::new(format!("Participant-{}", &Uuid::new_v4().to_string()[..8]))
    }

    /// Create a guest participant.
    #[must_use]
    pub fn guest(name: impl Into<String>) -> Self {
        let mut p = Self::new(name);
        p.is_guest = true;
        p.user_id = format!("guest-{}", Uuid::new_v4());
        p
    }

    /// Set explicit IDs (for reproducible tests).
    #[must_use]
    pub fn with_ids(mut self, participant_id: &str, user_id: &str) -> Self {
        self.participant_id = participant_id.to_string();
        self.user_id = user_id.to_string();
        self
    }
}

/// Test binding token fixture (ADR-0023).
#[derive(Debug, Clone)]
pub struct TestBindingToken {
    /// Correlation ID (UUIDv7).
    pub correlation_id: String,
    /// User ID (from JWT).
    pub user_id: String,
    /// Participant ID.
    pub participant_id: String,
    /// Current nonce.
    pub nonce: String,
    /// The binding token value (HMAC).
    pub token: String,
}

impl TestBindingToken {
    /// Create a new test binding token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            correlation_id: format!("corr-{}", Uuid::new_v4()),
            user_id: format!("user-{}", Uuid::new_v4()),
            participant_id: format!("part-{}", Uuid::new_v4()),
            nonce: format!("nonce-{}", Uuid::new_v4()),
            // In a real implementation, this would be HMAC-SHA256
            // For testing, we use a placeholder
            token: format!("test-token-{}", Uuid::new_v4()),
        }
    }

    /// Set the correlation ID.
    #[must_use]
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = id.into();
        self
    }

    /// Set the user ID.
    #[must_use]
    pub fn with_user_id(mut self, id: impl Into<String>) -> Self {
        self.user_id = id.into();
        self
    }

    /// Set the participant ID.
    #[must_use]
    pub fn with_participant_id(mut self, id: impl Into<String>) -> Self {
        self.participant_id = id.into();
        self
    }

    /// Set the nonce.
    #[must_use]
    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = nonce.into();
        self
    }

    /// Set explicit token value (for testing specific scenarios).
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }
}

impl Default for TestBindingToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meeting_builder() {
        let meeting = TestMeeting::new("meeting-123")
            .with_name("My Meeting")
            .with_max_participants(50)
            .with_e2e();

        assert_eq!(meeting.id, "meeting-123");
        assert_eq!(meeting.display_name, "My Meeting");
        assert_eq!(meeting.max_participants, 50);
        assert!(meeting.e2e_enabled);
    }

    #[test]
    fn test_participant_builder() {
        let participant = TestParticipant::new("Alice").with_ids("part-1", "user-1");

        assert_eq!(participant.name, "Alice");
        assert_eq!(participant.participant_id, "part-1");
        assert_eq!(participant.user_id, "user-1");
        assert!(!participant.is_guest);
    }

    #[test]
    fn test_guest_participant() {
        let guest = TestParticipant::guest("Guest User");

        assert!(guest.is_guest);
        assert!(guest.user_id.starts_with("guest-"));
    }

    #[test]
    fn test_binding_token_builder() {
        let token = TestBindingToken::new()
            .with_correlation_id("corr-test")
            .with_user_id("user-test")
            .with_nonce("nonce-test");

        assert_eq!(token.correlation_id, "corr-test");
        assert_eq!(token.user_id, "user-test");
        assert_eq!(token.nonce, "nonce-test");
    }
}
