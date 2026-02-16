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

    /// MC is not registered with GC (heartbeat returned NOT_FOUND).
    #[error("Not registered with GC")]
    NotRegistered,

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

    /// Internal error with context.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Token acquisition failed during startup.
    #[error("Token acquisition failed: {0}")]
    TokenAcquisition(String),

    /// Token acquisition timed out during startup.
    #[error("Token acquisition timed out")]
    TokenAcquisitionTimeout,
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
            | McError::Internal(_)
            | McError::FencedOut(_)
            | McError::NotRegistered
            | McError::TokenAcquisition(_)
            | McError::TokenAcquisitionTimeout => {
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

    /// Returns the signaling error code as a u16 (for metrics recording).
    ///
    /// Maps signaling `ErrorCode` values to u16 for the `status_code` label
    /// in `mc_errors_total`. MC uses signaling codes (not HTTP status codes)
    /// since it communicates via WebTransport, not HTTP.
    #[allow(dead_code)] // Used in Phase 6b+
    #[allow(clippy::cast_sign_loss)] // error_code() returns well-known positive values 2-7
    pub fn status_code(&self) -> u16 {
        self.error_code() as u16
    }

    /// Returns a bounded label string for the error variant (for metrics).
    ///
    /// Uses enum variant names, not error message content.
    /// Ensures label cardinality is bounded (ADR-0011).
    #[allow(dead_code)] // Used in Phase 6b+
    pub fn error_type_label(&self) -> &'static str {
        match self {
            McError::Redis(_) => "redis",
            McError::Grpc(_) => "grpc",
            McError::NotRegistered => "not_registered",
            McError::Config(_) => "config",
            McError::SessionBinding(_) => "session_binding",
            McError::MeetingNotFound(_) => "meeting_not_found",
            McError::ParticipantNotFound(_) => "participant_not_found",
            McError::MeetingCapacityExceeded(_) => "meeting_capacity_exceeded",
            McError::McCapacityExceeded => "mc_capacity_exceeded",
            McError::Draining => "draining",
            McError::Migrating { .. } => "migrating",
            McError::FencedOut(_) => "fenced_out",
            McError::Conflict(_) => "conflict",
            McError::JwtValidation(_) => "jwt_validation",
            McError::PermissionDenied(_) => "permission_denied",
            McError::Internal(_) => "internal",
            McError::TokenAcquisition(_) => "token_acquisition",
            McError::TokenAcquisitionTimeout => "token_acquisition_timeout",
        }
    }

    /// Returns a client-safe error message (no internal details).
    #[allow(dead_code)] // Used in Phase 6b+
    pub fn client_message(&self) -> String {
        match self {
            McError::Redis(_)
            | McError::Grpc(_)
            | McError::Config(_)
            | McError::Internal(_)
            | McError::NotRegistered
            | McError::TokenAcquisition(_)
            | McError::TokenAcquisitionTimeout => "An internal error occurred".to_string(),
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
        assert_eq!(McError::Internal("test".to_string()).error_code(), 6);
        assert_eq!(McError::FencedOut("stale".to_string()).error_code(), 6);
        assert_eq!(McError::NotRegistered.error_code(), 6);

        // Token acquisition errors -> 6 (INTERNAL_ERROR)
        assert_eq!(
            McError::TokenAcquisition("failed".to_string()).error_code(),
            6
        );
        assert_eq!(McError::TokenAcquisitionTimeout.error_code(), 6);

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

        // NotRegistered should also hide internal details
        let not_registered_err = McError::NotRegistered;
        assert_eq!(
            not_registered_err.client_message(),
            "An internal error occurred"
        );

        // Token errors should hide details
        let token_err =
            McError::TokenAcquisition("AC connection refused at 192.168.1.1".to_string());
        assert!(!token_err.client_message().contains("192.168"));
        assert_eq!(token_err.client_message(), "An internal error occurred");

        let timeout_err = McError::TokenAcquisitionTimeout;
        assert_eq!(timeout_err.client_message(), "An internal error occurred");
    }

    #[test]
    fn test_session_binding_error_conversion() {
        let binding_err = SessionBindingError::TokenExpired;
        let mc_err: McError = binding_err.into();

        assert!(matches!(mc_err, McError::SessionBinding(_)));
        assert_eq!(mc_err.error_code(), 2);
    }

    #[test]
    fn test_status_code_mapping() {
        // status_code() returns signaling error codes as u16 (not HTTP codes)
        // MC uses WebTransport, not HTTP (see status_code() doc comment)

        // Internal errors -> 6
        assert_eq!(McError::Redis("test".to_string()).status_code(), 6);
        assert_eq!(McError::Grpc("test".to_string()).status_code(), 6);
        assert_eq!(McError::Config("test".to_string()).status_code(), 6);
        assert_eq!(McError::Internal("test".to_string()).status_code(), 6);
        assert_eq!(McError::FencedOut("test".to_string()).status_code(), 6);
        assert_eq!(McError::NotRegistered.status_code(), 6);
        assert_eq!(
            McError::TokenAcquisition("test".to_string()).status_code(),
            6
        );
        assert_eq!(McError::TokenAcquisitionTimeout.status_code(), 6);

        // Auth errors -> 2
        assert_eq!(
            McError::SessionBinding(SessionBindingError::TokenExpired).status_code(),
            2
        );
        assert_eq!(McError::JwtValidation("test".to_string()).status_code(), 2);

        // Forbidden -> 3
        assert_eq!(
            McError::PermissionDenied("test".to_string()).status_code(),
            3
        );

        // Not found -> 4
        assert_eq!(
            McError::MeetingNotFound("test".to_string()).status_code(),
            4
        );
        assert_eq!(
            McError::ParticipantNotFound("test".to_string()).status_code(),
            4
        );

        // Conflict -> 5
        assert_eq!(McError::Conflict("test".to_string()).status_code(), 5);

        // Capacity exceeded -> 7
        assert_eq!(
            McError::MeetingCapacityExceeded("test".to_string()).status_code(),
            7
        );
        assert_eq!(McError::McCapacityExceeded.status_code(), 7);
        assert_eq!(McError::Draining.status_code(), 7);
        assert_eq!(
            McError::Migrating {
                new_mc_endpoint: "mc2:4433".to_string()
            }
            .status_code(),
            7
        );
    }

    #[test]
    fn test_error_type_label_exhaustive() {
        // Verify all 18 McError variants map to bounded &'static str labels
        assert_eq!(
            McError::Redis("test".to_string()).error_type_label(),
            "redis"
        );
        assert_eq!(McError::Grpc("test".to_string()).error_type_label(), "grpc");
        assert_eq!(McError::NotRegistered.error_type_label(), "not_registered");
        assert_eq!(
            McError::Config("test".to_string()).error_type_label(),
            "config"
        );
        assert_eq!(
            McError::SessionBinding(SessionBindingError::TokenExpired).error_type_label(),
            "session_binding"
        );
        assert_eq!(
            McError::MeetingNotFound("test".to_string()).error_type_label(),
            "meeting_not_found"
        );
        assert_eq!(
            McError::ParticipantNotFound("test".to_string()).error_type_label(),
            "participant_not_found"
        );
        assert_eq!(
            McError::MeetingCapacityExceeded("test".to_string()).error_type_label(),
            "meeting_capacity_exceeded"
        );
        assert_eq!(
            McError::McCapacityExceeded.error_type_label(),
            "mc_capacity_exceeded"
        );
        assert_eq!(McError::Draining.error_type_label(), "draining");
        assert_eq!(
            McError::Migrating {
                new_mc_endpoint: "mc2:4433".to_string()
            }
            .error_type_label(),
            "migrating"
        );
        assert_eq!(
            McError::FencedOut("test".to_string()).error_type_label(),
            "fenced_out"
        );
        assert_eq!(
            McError::Conflict("test".to_string()).error_type_label(),
            "conflict"
        );
        assert_eq!(
            McError::JwtValidation("test".to_string()).error_type_label(),
            "jwt_validation"
        );
        assert_eq!(
            McError::PermissionDenied("test".to_string()).error_type_label(),
            "permission_denied"
        );
        assert_eq!(
            McError::Internal("test".to_string()).error_type_label(),
            "internal"
        );
        assert_eq!(
            McError::TokenAcquisition("test".to_string()).error_type_label(),
            "token_acquisition"
        );
        assert_eq!(
            McError::TokenAcquisitionTimeout.error_type_label(),
            "token_acquisition_timeout"
        );
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

        // Token error display formatting
        assert_eq!(
            format!(
                "{}",
                McError::TokenAcquisition("AC unreachable".to_string())
            ),
            "Token acquisition failed: AC unreachable"
        );
        assert_eq!(
            format!("{}", McError::TokenAcquisitionTimeout),
            "Token acquisition timed out"
        );
    }
}
