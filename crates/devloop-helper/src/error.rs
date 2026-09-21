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

/// KIND cluster-name length bounds — single source of truth, DERIVED not typed.
///
/// KIND names the control-plane node `<cluster>-control-plane`, a Kubernetes DNS
/// label capped at 63 chars (RFC 1123). Exactly one control-plane node is declared
/// in `infra/kind/kind-config.yaml.tmpl` (the config this helper renders from — see the
/// NOTE above its `nodes:` block),
/// so `-control-plane` is the longest node-name suffix. That makes the operative
/// caps: cluster name ≤ `63 − len("-control-plane")` = 49, and — because this helper
/// forms every cluster name as `devloop-<slug>` (see [`crate`] `run()`) — slug ≤
/// `49 − len("devloop-")` = 41. Deriving from the two string constants (rather than
/// typing `49`/`41`) keeps every bound honest if the prefix or suffix ever changes.
pub const DNS_LABEL_MAX: usize = 63;
pub const NODE_SUFFIX: &str = "-control-plane";
pub const CLUSTER_PREFIX: &str = "devloop-";
pub const CLUSTER_NAME_MAX: usize = DNS_LABEL_MAX - NODE_SUFFIX.len();
pub const SLUG_MAX: usize = CLUSTER_NAME_MAX - CLUSTER_PREFIX.len();

/// Validated slug for use in paths and cluster names.
///
/// Guarantees: lowercase alphanumeric and hyphens only, starts and ends with
/// alphanumeric, and **at most [`SLUG_MAX`] (41) characters**. The length cap is
/// NOT an arbitrary path limit: the helper forms the KIND cluster name as
/// `devloop-<slug>`, whose control-plane node `devloop-<slug>-control-plane` must
/// fit the 63-char DNS label ([`DNS_LABEL_MAX`]) — so `slug ≤ 63 − 14 − 8 = 41`.
/// A longer slug produces a node name KIND cannot create; capping here (defence in
/// depth behind `devloop.sh`'s launch check) rejects it loudly instead of letting
/// the cluster silently fail to come up.
#[derive(Debug, Clone)]
pub struct ValidSlug(String);

impl ValidSlug {
    /// Validate a slug string.
    ///
    /// Pattern: `^[a-z0-9]([a-z0-9-]*[a-z0-9])?$`, max [`SLUG_MAX`] chars.
    pub fn new(s: &str) -> Result<Self, HelperError> {
        if s.is_empty() || s.len() > SLUG_MAX {
            return Err(HelperError::InvalidSlug(format!(
                "slug must be 1-{SLUG_MAX} characters, got {} \
                 (cluster name is 'devloop-<slug>', whose '-control-plane' node name \
                 must fit the {DNS_LABEL_MAX}-char DNS label)",
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

    // --- #1 name-length: the slug cap is the reachable Rust bypass of devloop.sh's
    //     launch check (main.rs forms `devloop-<slug>`), so it must enforce slug ≤ 41.

    /// The bounds are DERIVED from 63 (not typed), so pin the computed values — a
    /// change to the prefix/suffix that shifted them would red this, which is the
    /// point of the derivation.
    #[test]
    fn test_slug_bounds_are_derived_from_63() {
        assert_eq!(CLUSTER_NAME_MAX, 49, "63 - len(\"-control-plane\")");
        assert_eq!(SLUG_MAX, 41, "49 - len(\"devloop-\")");
    }

    /// Boundary: a slug of exactly SLUG_MAX is accepted (produces a 63-char node name).
    #[test]
    fn test_valid_slug_accepts_max_length() {
        let s = "a".repeat(SLUG_MAX); // 41 chars, all valid
        assert!(ValidSlug::new(&s).is_ok(), "len {} should pass", s.len());
    }

    /// Boundary: SLUG_MAX + 1 is REJECTED — a 42-char slug yields a 50-char cluster
    /// name (`devloop-` + 42) and a 64-char node name (+ `-control-plane`), which
    /// exceeds the 63-char DNS label and KIND cannot create. This is the hole that a
    /// literal `63` cap here would have left open.
    #[test]
    fn test_valid_slug_rejects_over_max_length() {
        let s = "a".repeat(SLUG_MAX + 1); // 42 chars
        let err = ValidSlug::new(&s).expect_err("42-char slug must be rejected");
        assert!(
            matches!(&err, HelperError::InvalidSlug(_)),
            "expected InvalidSlug, got {err:?}"
        );
        // Message must name the computed cap + the 63 origin (self-documenting,
        // since there is no §8 catalogue row in task (a)).
        if let HelperError::InvalidSlug(msg) = &err {
            assert!(msg.contains(&SLUG_MAX.to_string()), "cap not named: {msg}");
            assert!(
                msg.contains(&DNS_LABEL_MAX.to_string()),
                "63 origin not named: {msg}"
            );
        }
    }
}
