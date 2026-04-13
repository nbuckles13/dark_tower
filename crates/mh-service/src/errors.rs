//! Media Handler error types.
//!
//! Error types map to appropriate status codes for gRPC responses.
//! Internal details are logged server-side but not exposed to clients.

use common::jwt::JwtError;
use thiserror::Error;

/// Media Handler error type.
#[derive(Debug, Error)]
pub enum MhError {
    /// gRPC communication error (MH<->GC).
    #[error("gRPC error: {0}")]
    Grpc(String),

    /// MH is not registered with GC (heartbeat returned `NOT_FOUND`).
    #[error("Not registered with GC")]
    NotRegistered,

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal error with context.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Token acquisition failed during startup.
    #[error("Token acquisition failed: {0}")]
    TokenAcquisition(String),

    /// Token acquisition timed out during startup.
    #[error("Token acquisition timed out")]
    TokenAcquisitionTimeout,

    /// JWT validation failed (meeting token or service token).
    #[error("JWT validation failed: {0}")]
    JwtValidation(String),

    /// WebTransport server or connection error.
    #[error("WebTransport error: {0}")]
    WebTransportError(String),

    /// Client connected to a meeting not registered on this MH.
    #[error("Meeting not registered: {0}")]
    MeetingNotRegistered(String),
}

impl MhError {
    /// Returns a bounded label string for the error variant (for metrics).
    ///
    /// Uses enum variant names, not error message content.
    /// Ensures label cardinality is bounded (ADR-0011).
    #[must_use]
    pub fn error_type_label(&self) -> &'static str {
        match self {
            MhError::Grpc(_) => "grpc",
            MhError::NotRegistered => "not_registered",
            MhError::Config(_) => "config",
            MhError::Internal(_) => "internal",
            MhError::TokenAcquisition(_) => "token_acquisition",
            MhError::TokenAcquisitionTimeout => "token_acquisition_timeout",
            MhError::JwtValidation(_) => "jwt_validation",
            MhError::WebTransportError(_) => "webtransport",
            MhError::MeetingNotRegistered(_) => "meeting_not_registered",
        }
    }

    /// Returns a gRPC-compatible status code for this error.
    ///
    /// Maps to `tonic::Code` values as u16 for the `status_code` label
    /// in `mh_errors_total`.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            MhError::Grpc(_) | MhError::Internal(_) | MhError::WebTransportError(_) => 13, // INTERNAL
            MhError::NotRegistered | MhError::MeetingNotRegistered(_) => 5, // NOT_FOUND
            MhError::Config(_) => 3,                                        // INVALID_ARGUMENT
            MhError::TokenAcquisition(_) | MhError::TokenAcquisitionTimeout => 14, // UNAVAILABLE
            MhError::JwtValidation(_) => 16,                                // UNAUTHENTICATED
        }
    }

    /// Returns a client-safe error message (no internal details).
    #[must_use]
    pub fn client_message(&self) -> &'static str {
        match self {
            MhError::Grpc(_)
            | MhError::Config(_)
            | MhError::Internal(_)
            | MhError::NotRegistered
            | MhError::TokenAcquisition(_)
            | MhError::TokenAcquisitionTimeout
            | MhError::WebTransportError(_) => "An internal error occurred",
            MhError::JwtValidation(_) => "Invalid or expired token",
            MhError::MeetingNotRegistered(_) => "Meeting not available",
        }
    }
}

impl From<JwtError> for MhError {
    fn from(err: JwtError) -> Self {
        match err {
            JwtError::ServiceUnavailable(_) => {
                MhError::Internal("Authentication service unavailable".to_string())
            }
            _ => MhError::JwtValidation("The access token is invalid or expired".to_string()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_type_label_exhaustive() {
        assert_eq!(MhError::Grpc("test".to_string()).error_type_label(), "grpc");
        assert_eq!(MhError::NotRegistered.error_type_label(), "not_registered");
        assert_eq!(
            MhError::Config("test".to_string()).error_type_label(),
            "config"
        );
        assert_eq!(
            MhError::Internal("test".to_string()).error_type_label(),
            "internal"
        );
        assert_eq!(
            MhError::TokenAcquisition("test".to_string()).error_type_label(),
            "token_acquisition"
        );
        assert_eq!(
            MhError::TokenAcquisitionTimeout.error_type_label(),
            "token_acquisition_timeout"
        );
        assert_eq!(
            MhError::JwtValidation("test".to_string()).error_type_label(),
            "jwt_validation"
        );
        assert_eq!(
            MhError::WebTransportError("test".to_string()).error_type_label(),
            "webtransport"
        );
        assert_eq!(
            MhError::MeetingNotRegistered("test".to_string()).error_type_label(),
            "meeting_not_registered"
        );
    }

    #[test]
    fn test_status_code_mapping() {
        assert_eq!(MhError::Grpc("test".to_string()).status_code(), 13);
        assert_eq!(MhError::Internal("test".to_string()).status_code(), 13);
        assert_eq!(MhError::NotRegistered.status_code(), 5);
        assert_eq!(MhError::Config("test".to_string()).status_code(), 3);
        assert_eq!(
            MhError::TokenAcquisition("test".to_string()).status_code(),
            14
        );
        assert_eq!(MhError::TokenAcquisitionTimeout.status_code(), 14);
        assert_eq!(MhError::JwtValidation("test".to_string()).status_code(), 16);
        assert_eq!(
            MhError::WebTransportError("test".to_string()).status_code(),
            13
        );
        assert_eq!(
            MhError::MeetingNotRegistered("test".to_string()).status_code(),
            5
        );
    }

    #[test]
    fn test_client_messages_hide_internal_details() {
        let grpc_err = MhError::Grpc("connection refused at 192.168.1.100".to_string());
        assert!(!grpc_err.client_message().contains("192.168"));
        assert_eq!(grpc_err.client_message(), "An internal error occurred");

        let config_err = MhError::Config("missing secret key".to_string());
        assert!(!config_err.client_message().contains("secret"));

        let token_err =
            MhError::TokenAcquisition("AC connection refused at 192.168.1.1".to_string());
        assert!(!token_err.client_message().contains("192.168"));

        let jwt_err = MhError::JwtValidation("EdDSA signature mismatch".to_string());
        assert!(!jwt_err.client_message().contains("EdDSA"));
        assert_eq!(jwt_err.client_message(), "Invalid or expired token");

        let wt_err = MhError::WebTransportError("TLS handshake failed".to_string());
        assert!(!wt_err.client_message().contains("TLS"));
        assert_eq!(wt_err.client_message(), "An internal error occurred");

        let meeting_err = MhError::MeetingNotRegistered("meeting-123".to_string());
        assert!(!meeting_err.client_message().contains("meeting-123"));
        assert_eq!(meeting_err.client_message(), "Meeting not available");
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(
            format!("{}", MhError::Grpc("timeout".to_string())),
            "gRPC error: timeout"
        );
        assert_eq!(
            format!("{}", MhError::NotRegistered),
            "Not registered with GC"
        );
        assert_eq!(
            format!("{}", MhError::TokenAcquisitionTimeout),
            "Token acquisition timed out"
        );
        assert_eq!(
            format!("{}", MhError::JwtValidation("bad token".to_string())),
            "JWT validation failed: bad token"
        );
        assert_eq!(
            format!("{}", MhError::WebTransportError("bind failed".to_string())),
            "WebTransport error: bind failed"
        );
        assert_eq!(
            format!(
                "{}",
                MhError::MeetingNotRegistered("meeting-xyz".to_string())
            ),
            "Meeting not registered: meeting-xyz"
        );
    }

    #[test]
    fn test_from_jwt_error_service_unavailable() {
        let jwt_err = JwtError::ServiceUnavailable("AC down".to_string());
        let mh_err: MhError = jwt_err.into();
        assert!(matches!(mh_err, MhError::Internal(_)));
        assert_eq!(mh_err.client_message(), "An internal error occurred");
    }

    #[test]
    fn test_from_jwt_error_validation_failures() {
        let errors = [
            JwtError::TokenTooLarge,
            JwtError::MalformedToken,
            JwtError::MissingKid,
            JwtError::IatTooFarInFuture,
            JwtError::InvalidSignature,
            JwtError::KeyNotFound,
        ];

        for jwt_err in errors {
            let mh_err: MhError = jwt_err.into();
            assert!(
                matches!(&mh_err, MhError::JwtValidation(_)),
                "Expected JwtValidation, got {mh_err:?}"
            );
            assert_eq!(mh_err.client_message(), "Invalid or expired token");
        }
    }
}
