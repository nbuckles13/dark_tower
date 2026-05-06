//! Error types for the devloop helper.

use std::fmt;

/// All error types produced by the helper.
///
/// Marked `#[non_exhaustive]` so future variants do not break downstream
/// `match` sites (per code-reviewer CR8).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// Write rejected because another write is in flight on this helper.
    /// `op` and `args` identify the in-flight write so the client can render a
    /// "helper busy with <op>" message and the audit log can record the
    /// collision pair via `Display`.
    #[error(
        "helper busy with {}; run dev-cluster cancel to abort it",
        format_busy_op(op, args)
    )]
    Busy { op: String, args: Vec<String> },

    /// In-flight write that exited because a `cancel` arrived. `Display`
    /// always starts with the literal string `"cancelled"` so the audit log
    /// stays greppable by `^"error":"cancelled` (Obs O3 invariant).
    /// `escalated == true` when SIGKILL escalation was needed; the suffix
    /// lets operators triage clean-vs-forced shutdown.
    #[error("{}", if *escalated {
        "cancelled (sigterm timeout, escalated to sigkill)"
    } else {
        "cancelled by client request"
    })]
    Cancelled { escalated: bool },
}

/// Format the in-flight op + args for `Busy` Display.
fn format_busy_op(op: &str, args: &[String]) -> String {
    if args.is_empty() {
        op.to_string()
    } else {
        format!("{} {}", op, args.join(" "))
    }
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
            Self::Busy { .. } => "busy",
            Self::Cancelled { .. } => "cancelled",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_busy_kind() {
        let err = HelperError::Busy {
            op: "setup".to_string(),
            args: vec!["--skip-observability".to_string()],
        };
        assert_eq!(err.kind(), "busy");
    }

    #[test]
    fn test_busy_display_contains_op_and_args() {
        let err = HelperError::Busy {
            op: "setup".to_string(),
            args: vec!["--skip-observability".to_string()],
        };
        let s = err.to_string();
        assert!(s.contains("setup"), "got: {s}");
        assert!(s.contains("--skip-observability"), "got: {s}");
        assert!(s.contains("dev-cluster cancel"), "got: {s}");
    }

    #[test]
    fn test_busy_display_no_args() {
        let err = HelperError::Busy {
            op: "teardown".to_string(),
            args: vec![],
        };
        let s = err.to_string();
        assert!(s.contains("teardown"), "got: {s}");
        assert!(s.contains("dev-cluster cancel"), "got: {s}");
    }

    #[test]
    fn test_cancelled_kind() {
        assert_eq!(
            HelperError::Cancelled { escalated: false }.kind(),
            "cancelled"
        );
        assert_eq!(
            HelperError::Cancelled { escalated: true }.kind(),
            "cancelled"
        );
    }

    /// Obs O3 prefix invariant: both Cancelled Display outputs MUST start
    /// with the literal string "cancelled" so `^"error":"cancelled` greps work.
    #[test]
    fn test_cancelled_display_prefix() {
        assert!(HelperError::Cancelled { escalated: false }
            .to_string()
            .starts_with("cancelled"));
        assert!(HelperError::Cancelled { escalated: true }
            .to_string()
            .starts_with("cancelled"));
    }

    /// Obs O3: escalated form has the SIGKILL suffix so operators can
    /// distinguish "child shut down cleanly" from "we had to force-kill".
    #[test]
    fn test_cancelled_display_escalation_suffix() {
        let escalated = HelperError::Cancelled { escalated: true }.to_string();
        assert!(
            escalated.contains("escalated to sigkill"),
            "got: {escalated}"
        );
        let clean = HelperError::Cancelled { escalated: false }.to_string();
        assert!(!clean.contains("sigkill"), "got: {clean}");
    }
}
