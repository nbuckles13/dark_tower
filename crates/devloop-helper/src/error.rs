//! Error types for the devloop helper.

use std::fmt;

/// All error types produced by the helper.
#[derive(Debug, thiserror::Error)]
pub enum HelperError {
    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("invalid service: {0}")]
    InvalidService(String),

    #[error("port allocation failed: {0}")]
    PortAllocation(String),

    #[error("command failed: {cmd}: {detail}")]
    CommandFailed { cmd: String, detail: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("authentication failed")]
    AuthFailed,

    #[error("request too large")]
    RequestTooLarge,

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("invalid slug: {0}")]
    InvalidSlug(String),

    #[error("helper already running (pid {0})")]
    AlreadyRunning(u32),
}

/// Machine-readable error kind for socket responses.
impl HelperError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidCommand(_) => "invalid_command",
            Self::InvalidService(_) => "invalid_service",
            Self::PortAllocation(_) => "port_allocation",
            Self::CommandFailed { .. } => "command_failed",
            Self::Io(_) => "io_error",
            Self::Json(_) => "json_error",
            Self::AuthFailed => "auth_failed",
            Self::RequestTooLarge => "request_too_large",
            Self::InvalidRequest(_) => "invalid_request",
            Self::InvalidSlug(_) => "invalid_slug",
            Self::AlreadyRunning(_) => "already_running",
        }
    }
}

/// Validated slug for use in paths and cluster names.
///
/// Guarantees: lowercase alphanumeric and hyphens only, starts and ends with
/// alphanumeric, max 63 characters. Safe for filesystem paths and Kind cluster names.
#[derive(Debug, Clone)]
pub struct ValidSlug(String);

impl ValidSlug {
    /// Validate a slug string.
    ///
    /// Pattern: `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`, max 63 chars.
    pub fn new(s: &str) -> Result<Self, HelperError> {
        if s.is_empty() || s.len() > 63 {
            return Err(HelperError::InvalidSlug(format!(
                "slug must be 1-63 characters, got {}",
                s.len()
            )));
        }

        let bytes = s.as_bytes();

        // Check first character (safe: we already checked non-empty above)
        let first = match bytes.first() {
            Some(&b) => b,
            None => {
                return Err(HelperError::InvalidSlug(
                    "slug must not be empty".to_string(),
                ))
            }
        };
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(HelperError::InvalidSlug(format!(
                "slug must start with lowercase alphanumeric, got '{}'",
                char::from(first)
            )));
        }

        // Check last character (safe: we already checked non-empty above)
        let last = match bytes.last() {
            Some(&b) => b,
            None => {
                return Err(HelperError::InvalidSlug(
                    "slug must not be empty".to_string(),
                ))
            }
        };
        if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
            return Err(HelperError::InvalidSlug(format!(
                "slug must end with lowercase alphanumeric, got '{}'",
                char::from(last)
            )));
        }

        // Check all characters
        for &b in bytes {
            if !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'-' {
                return Err(HelperError::InvalidSlug(format!(
                    "slug contains invalid character '{}' (allowed: a-z, 0-9, -)",
                    char::from(b)
                )));
            }
        }

        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ValidSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
