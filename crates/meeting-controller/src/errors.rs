//! Meeting Controller error types.
//!
//! Error types map to appropriate signaling `ErrorCode` values for client responses.
//! Internal details are logged server-side but not exposed to clients.

use thiserror::Error;

/// Meeting Controller error type.
///
/// Maps to signaling `ErrorCode` values:
/// - `SessionBinding` errors: `UNAUTHORIZED` (2)
/// - `NotFound`: `NOT_FOUND` (4)
/// - `Conflict`: `CONFLICT` (5)
/// - Internal, Redis, Config, Grpc: `INTERNAL_ERROR` (6)
/// - `CapacityExceeded`: `CAPACITY_EXCEEDED` (7)
#[derive(Debug, Error)]
#[allow(dead_code)] // Error types used in Phase 6b+
pub enum McError {
    /// Redis operation failed.
    #[error("Redis error: {0}")]
    Redis(String),

    /// gRPC communication error (MC<->GC, MC<->MH).
    #[error("gRPC error: {0}")]
    Grpc(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Session binding token validation failed.
    #[error("Session binding error: {0}")]
    SessionBinding(SessionBindingError),

    /// Meeting not found.
    #[error("Meeting not found: {0}")]
    MeetingNotFound(String),

    /// Participant not found.
    #[error("Participant not found: {0}")]
    ParticipantNotFound(String),

    /// Meeting is at capacity.
    #[error("Meeting at capacity: {0}")]
    MeetingCapacityExceeded(String),

    /// MC is at capacity (load shedding).
    #[error("MC at capacity")]
    McCapacityExceeded,

    /// MC is draining (graceful shutdown).
    #[error("MC is draining")]
    Draining,

    /// Meeting is migrating to another MC.
    #[error("Meeting is migrating: {new_mc_endpoint}")]
    Migrating { new_mc_endpoint: String },

    /// Fenced out by another MC (split-brain recovery).
    #[error("Fenced out: {0}")]
    FencedOut(String),

    /// Conflict error (e.g., participant already exists).
    #[error("Conflict: {0}")]
    Conflict(String),

    /// JWT validation failed.
    #[error("JWT validation failed: {0}")]
    JwtValidation(String),

    /// Permission denied.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Internal error.
    #[error("Internal error")]
    Internal,
}

/// Session binding token validation errors (ADR-0023).
#[derive(Debug, Error)]
#[allow(dead_code)] // Error variants used in Phase 6b+
pub enum SessionBindingError {
    /// Binding token has expired (>30s TTL).
    #[error("Binding token expired")]
    TokenExpired,

    /// Binding token HMAC verification failed.
    #[error("Invalid binding token")]
    InvalidToken,

    /// Nonce has already been used (replay attack prevention).
    #[error("Nonce already used")]
    NonceReused,

    /// Correlation ID not found (session doesn't exist).
    #[error("Session not found")]
    SessionNotFound,

    /// User ID mismatch (JWT `user_id` doesn't match binding).
    #[error("User ID mismatch")]
    UserIdMismatch,
}

impl McError {
    /// Returns the signaling `ErrorCode` value for this error.
    #[allow(dead_code)] // Used in Phase 6b+
    pub fn error_code(&self) -> i32 {
        match self {
            McError::Redis(_)
            | McError::Grpc(_)
            | McError::Config(_)
            | McError::Internal
            | McError::FencedOut(_) => {
                6 // INTERNAL_ERROR
            }
            McError::SessionBinding(_) | McError::JwtValidation(_) => 2, // UNAUTHORIZED
            McError::PermissionDenied(_) => 3,                           // FORBIDDEN
            McError::MeetingNotFound(_) | McError::ParticipantNotFound(_) => 4, // NOT_FOUND
            McError::Conflict(_) => 5,                                   // CONFLICT
            McError::MeetingCapacityExceeded(_)
            | McError::McCapacityExceeded
            | McError::Draining
            | McError::Migrating { .. } => 7, // CAPACITY_EXCEEDED
        }
    }

    /// Returns a client-safe error message (no internal details).
    #[allow(dead_code)] // Used in Phase 6b+
    pub fn client_message(&self) -> String {
        match self {
            McError::Redis(_) | McError::Grpc(_) | McError::Config(_) | McError::Internal => {
                "An internal error occurred".to_string()
            }
            McError::SessionBinding(e) => e.to_string(),
            McError::MeetingNotFound(_) => "Meeting not found".to_string(),
            McError::ParticipantNotFound(_) => "Participant not found".to_string(),
            McError::MeetingCapacityExceeded(_) => "Meeting is at capacity".to_string(),
            McError::McCapacityExceeded => "Server is at capacity, please try again".to_string(),
            McError::Draining => "Server is shutting down, please reconnect".to_string(),
            McError::Migrating { .. } => "Meeting is being migrated, please reconnect".to_string(),
            McError::FencedOut(_) => "An internal error occurred".to_string(),
            McError::JwtValidation(_) => "Invalid or expired token".to_string(),
            McError::Conflict(msg) | McError::PermissionDenied(msg) => msg.clone(),
        }
    }
}

impl From<SessionBindingError> for McError {
    fn from(err: SessionBindingError) -> Self {
        McError::SessionBinding(err)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_mapping() {
        // Internal errors -> 6
        assert_eq!(McError::Redis("conn failed".to_string()).error_code(), 6);
        assert_eq!(
            McError::Grpc("connection refused".to_string()).error_code(),
            6
        );
        assert_eq!(McError::Config("bad config".to_string()).error_code(), 6);
        assert_eq!(McError::Internal.error_code(), 6);
        assert_eq!(McError::FencedOut("stale".to_string()).error_code(), 6);

        // Auth errors -> 2
        assert_eq!(
            McError::SessionBinding(SessionBindingError::TokenExpired).error_code(),
            2
        );
        assert_eq!(
            McError::JwtValidation("expired".to_string()).error_code(),
            2
        );

        // Forbidden -> 3
        assert_eq!(
            McError::PermissionDenied("not host".to_string()).error_code(),
            3
        );

        // Not found -> 4
        assert_eq!(
            McError::MeetingNotFound("meeting-123".to_string()).error_code(),
            4
        );
        assert_eq!(
            McError::ParticipantNotFound("participant-456".to_string()).error_code(),
            4
        );

        // Conflict -> 5
        assert_eq!(
            McError::Conflict("already exists".to_string()).error_code(),
            5
        );

        // Capacity exceeded -> 7
        assert_eq!(
            McError::MeetingCapacityExceeded("max 100".to_string()).error_code(),
            7
        );
        assert_eq!(McError::McCapacityExceeded.error_code(), 7);
        assert_eq!(McError::Draining.error_code(), 7);
        assert_eq!(
            McError::Migrating {
                new_mc_endpoint: "mc2:4433".to_string()
            }
            .error_code(),
            7
        );
    }

    #[test]
    fn test_client_messages_hide_internal_details() {
        // Internal errors should not leak details
        let redis_err = McError::Redis("connection refused at 192.168.1.100:6379".to_string());
        assert!(!redis_err.client_message().contains("192.168"));
        assert_eq!(redis_err.client_message(), "An internal error occurred");

        let config_err = McError::Config("missing secret key".to_string());
        assert!(!config_err.client_message().contains("secret"));
        assert_eq!(config_err.client_message(), "An internal error occurred");
    }

    #[test]
    fn test_session_binding_error_conversion() {
        let binding_err = SessionBindingError::TokenExpired;
        let mc_err: McError = binding_err.into();

        assert!(matches!(mc_err, McError::SessionBinding(_)));
        assert_eq!(mc_err.error_code(), 2);
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(
            format!("{}", McError::Redis("timeout".to_string())),
            "Redis error: timeout"
        );

        assert_eq!(
            format!(
                "{}",
                McError::SessionBinding(SessionBindingError::NonceReused)
            ),
            "Session binding error: Nonce already used"
        );

        assert_eq!(
            format!(
                "{}",
                McError::Migrating {
                    new_mc_endpoint: "mc2.example.com:4433".to_string()
                }
            ),
            "Meeting is migrating: mc2.example.com:4433"
        );
    }
}
