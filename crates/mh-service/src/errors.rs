//! Media Handler error types.
//!
//! Error types map to appropriate status codes for gRPC responses.
//! Internal details are logged server-side but not exposed to clients.

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
        }
    }

    /// Returns a gRPC-compatible status code for this error.
    ///
    /// Maps to `tonic::Code` values as u16 for the `status_code` label
    /// in `mh_errors_total`.
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            MhError::Grpc(_) | MhError::Internal(_) => 13, // INTERNAL
            MhError::NotRegistered => 5,                   // NOT_FOUND
            MhError::Config(_) => 3,                       // INVALID_ARGUMENT
            MhError::TokenAcquisition(_) | MhError::TokenAcquisitionTimeout => 14, // UNAVAILABLE
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
            | MhError::TokenAcquisitionTimeout => "An internal error occurred",
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
    }
}
