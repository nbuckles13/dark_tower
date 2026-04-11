//! Socket protocol types for the devloop helper.
//!
//! Newline-delimited JSON protocol. Each request is a single JSON object
//! terminated by `\n`. Responses use a streaming protocol:
//!
//! - Pre-execution errors: `{"success":false,"message":"...","error_kind":"..."}`
//! - Command started:      `{"started":true,"cmd":"setup","ts":"..."}`
//! - Stream lines:         `{"stream":"out","line":"...","ts":"..."}`
//! - Final result:         `{"result":"ok","exit_code":0,"duration_ms":42000}`

use crate::error::HelperError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Maximum request size in bytes (1 MB).
pub const MAX_REQUEST_SIZE: u64 = 1_048_576;

/// Service names — exhaustive enum match prevents injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Ac,
    Gc,
    Mc,
    Mh,
}

impl Service {
    /// All valid service variants.
    pub const ALL: [Service; 4] = [Service::Ac, Service::Gc, Service::Mc, Service::Mh];

    /// Get the service name as used in container image tags and deployment names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ac => "ac",
            Self::Gc => "gc",
            Self::Mc => "mc",
            Self::Mh => "mh",
        }
    }

    /// Get the full crate/image name.
    pub fn image_tag(self) -> &'static str {
        match self {
            Self::Ac => "localhost/ac-service:latest",
            Self::Gc => "localhost/gc-service:latest",
            Self::Mc => "localhost/mc-service:latest",
            Self::Mh => "localhost/mh-service:latest",
        }
    }

    /// Get the Dockerfile path relative to project root.
    pub fn dockerfile(self) -> &'static str {
        match self {
            Self::Ac => "infra/docker/ac-service/Dockerfile",
            Self::Gc => "infra/docker/gc-service/Dockerfile",
            Self::Mc => "infra/docker/mc-service/Dockerfile",
            Self::Mh => "infra/docker/mh-service/Dockerfile",
        }
    }
}

impl FromStr for Service {
    type Err = HelperError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ac" => Ok(Self::Ac),
            "gc" => Ok(Self::Gc),
            "mc" => Ok(Self::Mc),
            "mh" => Ok(Self::Mh),
            other => Err(HelperError::InvalidService(other.to_string())),
        }
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Commands the helper can execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperCommand {
    /// Allocate ports, generate kind-config, create cluster, run setup.sh.
    Setup { skip_observability: bool },
    /// Build one service image, load into Kind, restart deployment.
    Rebuild(Service),
    /// Rebuild all service images.
    RebuildAll,
    /// Apply manifests only (no image rebuild).
    Deploy(Service),
    /// Delete Kind cluster, clean up all state.
    Teardown,
}

impl HelperCommand {
    /// Get the command name for logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Setup { .. } => "setup",
            Self::Rebuild(_) => "rebuild",
            Self::RebuildAll => "rebuild-all",
            Self::Deploy(_) => "deploy",
            Self::Teardown => "teardown",
        }
    }

    /// Get the arguments for logging.
    pub fn args_for_log(&self) -> Vec<String> {
        match self {
            Self::Setup { skip_observability } => {
                if *skip_observability {
                    vec!["--skip-observability".to_string()]
                } else {
                    vec![]
                }
            }
            Self::Rebuild(svc) | Self::Deploy(svc) => vec![svc.to_string()],
            Self::RebuildAll | Self::Teardown => vec![],
        }
    }
}

impl fmt::Display for HelperCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup { skip_observability } => {
                write!(f, "setup")?;
                if *skip_observability {
                    write!(f, " --skip-observability")?;
                }
                Ok(())
            }
            Self::Rebuild(svc) => write!(f, "rebuild {svc}"),
            Self::RebuildAll => write!(f, "rebuild-all"),
            Self::Deploy(svc) => write!(f, "deploy {svc}"),
            Self::Teardown => write!(f, "teardown"),
        }
    }
}

/// Request from the client to the helper.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Authentication token.
    pub token: String,
    /// Command name.
    pub command: String,
    /// Optional service name for rebuild/deploy.
    #[serde(default)]
    pub service: Option<String>,
    /// Optional flags.
    #[serde(default)]
    pub skip_observability: bool,
}

impl Request {
    /// Validate and parse the request into a typed command.
    pub fn parse_command(&self) -> Result<HelperCommand, HelperError> {
        // Reject null bytes and control characters in all string fields
        validate_no_control_chars(&self.command, "command")?;
        validate_no_control_chars(&self.token, "token")?;
        if let Some(ref svc) = self.service {
            validate_no_control_chars(svc, "service")?;
        }

        match self.command.as_str() {
            "setup" => {
                if self.service.is_some() {
                    return Err(HelperError::InvalidRequest(
                        "setup command does not accept a service argument".to_string(),
                    ));
                }
                Ok(HelperCommand::Setup {
                    skip_observability: self.skip_observability,
                })
            }
            "rebuild" => {
                let svc_str = self.service.as_deref().ok_or_else(|| {
                    HelperError::InvalidRequest(
                        "rebuild command requires a service argument".to_string(),
                    )
                })?;
                let svc = Service::from_str(svc_str)?;
                Ok(HelperCommand::Rebuild(svc))
            }
            "rebuild-all" => {
                if self.service.is_some() {
                    return Err(HelperError::InvalidRequest(
                        "rebuild-all command does not accept a service argument".to_string(),
                    ));
                }
                Ok(HelperCommand::RebuildAll)
            }
            "deploy" => {
                let svc_str = self.service.as_deref().ok_or_else(|| {
                    HelperError::InvalidRequest(
                        "deploy command requires a service argument".to_string(),
                    )
                })?;
                let svc = Service::from_str(svc_str)?;
                Ok(HelperCommand::Deploy(svc))
            }
            "teardown" => {
                if self.service.is_some() {
                    return Err(HelperError::InvalidRequest(
                        "teardown command does not accept a service argument".to_string(),
                    ));
                }
                Ok(HelperCommand::Teardown)
            }
            other => Err(HelperError::InvalidCommand(other.to_string())),
        }
    }
}

/// Response from the helper to the client.
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub success: bool,
    pub message: String,
    /// Machine-readable error kind (only present on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// Optional structured data (e.g., port map on setup success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Response {
    #[cfg(test)]
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_kind: None,
            data: None,
        }
    }

    pub fn err(error: &HelperError) -> Self {
        Self {
            success: false,
            message: error.to_string(),
            error_kind: Some(error.kind().to_string()),
            data: None,
        }
    }
}

/// Maximum length of a single output line before truncation (64 KB).
pub const MAX_LINE_LEN: usize = 65_536;

/// Which stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Out,
    Err,
}

/// A single line of streaming output from a child process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamLine {
    pub stream: StreamKind,
    pub line: String,
    pub ts: String,
}

/// Emitted once before streaming begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStarted {
    pub started: bool,
    pub cmd: String,
    pub ts: String,
}

/// Outcome of a command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandOutcome {
    Ok,
    Error,
}

/// Final result message sent after all stream lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub result: CommandOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Message sent from reader threads to the main thread via mpsc channel.
pub enum StreamMsg {
    /// A line of output to forward to the client.
    Line(StreamLine),
    /// Indicates a reader thread has finished reading its pipe.
    Done,
}

/// Truncate a line to the maximum allowed length, appending a marker if truncated.
pub fn truncate_line(line: String, max_len: usize) -> String {
    if line.len() <= max_len {
        return line;
    }
    // Find a valid UTF-8 boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = line[..end].to_string();
    truncated.push_str(" [truncated]");
    truncated
}

/// Reject strings containing null bytes or ASCII control characters.
fn validate_no_control_chars(s: &str, field_name: &str) -> Result<(), HelperError> {
    for (i, b) in s.bytes().enumerate() {
        if b < 0x20 || b == 0x7f {
            return Err(HelperError::InvalidRequest(format!(
                "{field_name} contains control character at byte {i} (0x{b:02x})"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_from_str_valid() {
        assert_eq!(Service::from_str("ac").unwrap(), Service::Ac);
        assert_eq!(Service::from_str("gc").unwrap(), Service::Gc);
        assert_eq!(Service::from_str("mc").unwrap(), Service::Mc);
        assert_eq!(Service::from_str("mh").unwrap(), Service::Mh);
    }

    #[test]
    fn test_service_from_str_rejects_invalid() {
        let invalid = [
            "AC",
            "Ac",
            "ac ",
            " ac",
            "ac-service",
            "db",
            "web",
            "",
            "ac\0",
            "ac\n",
            "ac;rm -rf /",
            "$(whoami)",
        ];
        for s in &invalid {
            assert!(
                Service::from_str(s).is_err(),
                "expected rejection for {:?}",
                s
            );
        }
    }

    #[test]
    fn test_service_round_trip() {
        for svc in &Service::ALL {
            assert_eq!(Service::from_str(svc.as_str()).unwrap(), *svc);
        }
    }

    #[test]
    fn test_parse_command_setup() {
        let req = Request {
            token: "abc123".to_string(),
            command: "setup".to_string(),
            service: None,
            skip_observability: false,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(
            cmd,
            HelperCommand::Setup {
                skip_observability: false
            }
        );
    }

    #[test]
    fn test_parse_command_setup_skip_obs() {
        let req = Request {
            token: "abc123".to_string(),
            command: "setup".to_string(),
            service: None,
            skip_observability: true,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(
            cmd,
            HelperCommand::Setup {
                skip_observability: true
            }
        );
    }

    #[test]
    fn test_parse_command_rebuild() {
        let req = Request {
            token: "abc123".to_string(),
            command: "rebuild".to_string(),
            service: Some("ac".to_string()),
            skip_observability: false,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(cmd, HelperCommand::Rebuild(Service::Ac));
    }

    #[test]
    fn test_parse_command_rebuild_missing_service() {
        let req = Request {
            token: "abc123".to_string(),
            command: "rebuild".to_string(),
            service: None,
            skip_observability: false,
        };
        assert!(req.parse_command().is_err());
    }

    #[test]
    fn test_parse_command_rebuild_invalid_service() {
        let req = Request {
            token: "abc123".to_string(),
            command: "rebuild".to_string(),
            service: Some("; rm -rf /".to_string()),
            skip_observability: false,
        };
        assert!(req.parse_command().is_err());
    }

    #[test]
    fn test_parse_command_rebuild_all() {
        let req = Request {
            token: "abc123".to_string(),
            command: "rebuild-all".to_string(),
            service: None,
            skip_observability: false,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(cmd, HelperCommand::RebuildAll);
    }

    #[test]
    fn test_parse_command_deploy() {
        let req = Request {
            token: "abc123".to_string(),
            command: "deploy".to_string(),
            service: Some("gc".to_string()),
            skip_observability: false,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(cmd, HelperCommand::Deploy(Service::Gc));
    }

    #[test]
    fn test_parse_command_teardown() {
        let req = Request {
            token: "abc123".to_string(),
            command: "teardown".to_string(),
            service: None,
            skip_observability: false,
        };
        let cmd = req.parse_command().unwrap();
        assert_eq!(cmd, HelperCommand::Teardown);
    }

    #[test]
    fn test_parse_command_unknown() {
        let req = Request {
            token: "abc123".to_string(),
            command: "hack".to_string(),
            service: None,
            skip_observability: false,
        };
        assert!(req.parse_command().is_err());
    }

    #[test]
    fn test_parse_command_rejects_null_bytes_in_command() {
        let req = Request {
            token: "abc123".to_string(),
            command: "setup\0extra".to_string(),
            service: None,
            skip_observability: false,
        };
        let err = req.parse_command().unwrap_err();
        assert!(
            err.to_string().contains("control character"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_parse_command_rejects_newlines_in_service() {
        let req = Request {
            token: "abc123".to_string(),
            command: "rebuild".to_string(),
            service: Some("ac\nmalicious".to_string()),
            skip_observability: false,
        };
        let err = req.parse_command().unwrap_err();
        assert!(
            err.to_string().contains("control character"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_parse_command_rejects_tab_in_token() {
        let req = Request {
            token: "abc\t123".to_string(),
            command: "setup".to_string(),
            service: None,
            skip_observability: false,
        };
        let err = req.parse_command().unwrap_err();
        assert!(
            err.to_string().contains("control character"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_setup_rejects_service_arg() {
        let req = Request {
            token: "abc123".to_string(),
            command: "setup".to_string(),
            service: Some("ac".to_string()),
            skip_observability: false,
        };
        assert!(req.parse_command().is_err());
    }

    #[test]
    fn test_teardown_rejects_service_arg() {
        let req = Request {
            token: "abc123".to_string(),
            command: "teardown".to_string(),
            service: Some("ac".to_string()),
            skip_observability: false,
        };
        assert!(req.parse_command().is_err());
    }

    #[test]
    fn test_serde_rejects_unknown_fields() {
        let json = r#"{"token":"abc","command":"setup","unknown_field":"evil"}"#;
        let result: Result<Request, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_response_ok_serialization() {
        let resp = Response::ok("done");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(!json.contains("error_kind"));
    }

    #[test]
    fn test_response_err_serialization() {
        let err = HelperError::AuthFailed;
        let resp = Response::err(&err);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error_kind\":\"auth_failed\""));
    }

    #[test]
    fn test_command_display() {
        assert_eq!(
            HelperCommand::Setup {
                skip_observability: false
            }
            .to_string(),
            "setup"
        );
        assert_eq!(
            HelperCommand::Setup {
                skip_observability: true
            }
            .to_string(),
            "setup --skip-observability"
        );
        assert_eq!(
            HelperCommand::Rebuild(Service::Ac).to_string(),
            "rebuild ac"
        );
        assert_eq!(HelperCommand::RebuildAll.to_string(), "rebuild-all");
        assert_eq!(HelperCommand::Deploy(Service::Gc).to_string(), "deploy gc");
        assert_eq!(HelperCommand::Teardown.to_string(), "teardown");
    }

    // --- Streaming protocol type tests ---

    #[test]
    fn test_stream_line_serialization_out() {
        let line = StreamLine {
            stream: StreamKind::Out,
            line: "Building ac-service...".to_string(),
            ts: "2026-04-08T14:23:45.123Z".to_string(),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"stream\":\"out\""));
        assert!(json.contains("\"line\":\"Building ac-service...\""));
        assert!(json.contains("\"ts\":\"2026-04-08T14:23:45.123Z\""));
    }

    #[test]
    fn test_stream_line_serialization_err() {
        let line = StreamLine {
            stream: StreamKind::Err,
            line: "warning: unused variable".to_string(),
            ts: "2026-04-08T14:23:45.200Z".to_string(),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"stream\":\"err\""));
        assert!(json.contains("\"line\":\"warning: unused variable\""));
    }

    #[test]
    fn test_stream_line_round_trip() {
        let line = StreamLine {
            stream: StreamKind::Out,
            line: "hello world".to_string(),
            ts: "2026-04-08T14:23:45.000Z".to_string(),
        };
        let json = serde_json::to_string(&line).unwrap();
        let parsed: StreamLine = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, line);
    }

    #[test]
    fn test_stream_line_with_special_chars() {
        let line = StreamLine {
            stream: StreamKind::Out,
            line: r#"{"fake":"json"} with "quotes" and \backslash"#.to_string(),
            ts: "2026-04-08T14:23:45.000Z".to_string(),
        };
        let json = serde_json::to_string(&line).unwrap();
        let parsed: StreamLine = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, line);
    }

    #[test]
    fn test_stream_kind_rejects_invalid() {
        let json = r#"{"stream":"invalid","line":"x","ts":"t"}"#;
        let result: Result<StreamLine, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_command_started_serialization() {
        let started = CommandStarted {
            started: true,
            cmd: "setup".to_string(),
            ts: "2026-04-08T14:23:45.000Z".to_string(),
        };
        let json = serde_json::to_string(&started).unwrap();
        assert!(json.contains("\"started\":true"));
        assert!(json.contains("\"cmd\":\"setup\""));
    }

    #[test]
    fn test_command_result_ok_serialization() {
        let result = CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms: 42000,
            error: None,
            data: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"result\":\"ok\""));
        assert!(json.contains("\"exit_code\":0"));
        assert!(json.contains("\"duration_ms\":42000"));
        assert!(!json.contains("\"error\""));
        assert!(!json.contains("\"data\""));
    }

    #[test]
    fn test_command_result_error_serialization() {
        let result = CommandResult {
            result: CommandOutcome::Error,
            exit_code: Some(1),
            duration_ms: 5000,
            error: Some("setup.sh failed".to_string()),
            data: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"result\":\"error\""));
        assert!(json.contains("\"exit_code\":1"));
        assert!(json.contains("\"error\":\"setup.sh failed\""));
    }

    #[test]
    fn test_command_result_with_data() {
        let data = serde_json::json!({"port": 8080});
        let result = CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms: 1000,
            error: None,
            data: Some(data.clone()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: CommandResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.data, Some(data));
    }

    #[test]
    fn test_command_result_round_trip() {
        let result = CommandResult {
            result: CommandOutcome::Error,
            exit_code: None,
            duration_ms: 100,
            error: Some("killed by signal".to_string()),
            data: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: CommandResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, result);
    }

    #[test]
    fn test_command_outcome_rejects_invalid() {
        let json = r#"{"result":"invalid","duration_ms":0}"#;
        let result: Result<CommandResult, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncate_line_short() {
        let line = "hello".to_string();
        assert_eq!(truncate_line(line, 100), "hello");
    }

    #[test]
    fn test_truncate_line_exact_limit() {
        let line = "a".repeat(100);
        assert_eq!(truncate_line(line, 100), "a".repeat(100));
    }

    #[test]
    fn test_truncate_line_over_limit() {
        let line = "a".repeat(200);
        let result = truncate_line(line, 100);
        assert!(result.starts_with(&"a".repeat(100)));
        assert!(result.ends_with(" [truncated]"));
        assert_eq!(result.len(), 100 + " [truncated]".len());
    }

    #[test]
    fn test_truncate_line_utf8_boundary() {
        // Multi-byte UTF-8 character (emoji is 4 bytes)
        let mut line = "a".repeat(98);
        line.push('\u{1F600}'); // 4-byte emoji
        line.push_str("aaaa");
        // Truncate at 100 — should not split the emoji
        let result = truncate_line(line, 100);
        assert!(result.ends_with(" [truncated]"));
        // The truncation should back up to byte 98 (before the emoji)
        assert!(result.starts_with(&"a".repeat(98)));
    }
}
