//! Socket protocol types for the devloop helper.
//!
//! Newline-delimited JSON protocol. Each request is a single JSON object
//! terminated by `\n`. Responses use a streaming protocol:
//!
//! - Pre-execution errors: `{"success":false,"message":"...","error_kind":"..."}`
//! - Command started:      `{"started":true,"cmd":"provision","ts":"..."}`
//! - Stream lines:         `{"stream":"out","line":"...","ts":"..."}`
//! - Final result:         `{"result":"ok","exit_code":0,"duration_ms":42000}`

use crate::error::HelperError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum request size in bytes (1 MB).
pub const MAX_REQUEST_SIZE: u64 = 1_048_576;

/// Every verb the helper accepts on the wire — the ADR-0030 allowlist, and the
/// SSoT `scripts/guards/simple/validate-dev-cluster-verbs.sh` compares the
/// `infra/devloop/dev-cluster` client's `VERBS=` line against. Keep it on ONE
/// line (the guard reads it as a single literal; `rustfmt::skip` holds it there). `parse_command` accepts
/// exactly this set (pinned by `test_parse_command_accepts_exactly_verbs`).
#[rustfmt::skip]
pub const VERBS: &[&str] = &["provision", "deploy", "teardown", "recreate", "restore-kubeconfig", "status", "cancel"];

/// Commands the helper can execute. None takes an argument: the wire request
/// is `{token, command}` and nothing else (ADR-0038 step 3), so no
/// client-supplied string can reach a script's argv or env.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperCommand {
    /// Make sure a cluster matching the blueprint exists: allocate ports,
    /// render kind-config, run `infra/kind/scripts/provision.sh` (which
    /// destroys and rebuilds the cluster only when its recorded blueprint
    /// differs or is missing — ADR-0038 §1).
    Provision,
    /// Converge the application to the tree: `infra/kind/scripts/deploy.sh`
    /// (content-tagged images, migration Job, the one environment root —
    /// ADR-0038 §2). Rolls only what changed.
    Deploy,
    /// Delete Kind cluster, clean up all state.
    Teardown,
    /// Self-heal: destroy + recreate this slug's cluster, guarded by a host-side
    /// re-confirmation that the control plane is genuinely down (ADR-0030 trust
    /// boundary). Takes NO target argument — destroys only `ctx.cluster_name`.
    Recreate,
    /// Self-heal: regenerate ONLY the container kubeconfig from the live cluster
    /// (non-destructive — reaches no teardown/delete/create path).
    RestoreKubeconfig,
    /// Read-only health check: cluster exists, pods healthy, ports.json.
    Status,
    /// Interrupt the in-flight write handler (idempotent: no-op when idle).
    Cancel,
    /// Test-only: sleep for N seconds via /bin/sh.
    /// Variant exists only under `cfg(test)`; there is NO `Request::parse_command`
    /// arm so the wire surface stays clean. Tests construct it directly.
    #[cfg(test)]
    TestSleep { seconds: u64 },
    /// Test-only: run a /bin/sh stub that traps SIGTERM (`trap "" TERM`) and
    /// then sleeps. Used by `test_sigkill_escalation_logged` to exercise the
    /// 2s SIGTERM-grace-then-SIGKILL path in `run_command_streaming`. Same
    /// `cfg(test)`-only / no-parse-arm posture as `TestSleep`.
    #[cfg(test)]
    TestSleepIgnoringTerm { seconds: u64 },
    /// Test-only: spawn `bash -c 'sleep <N> & wait'` so a forked grandchild
    /// inherits stdout/stderr — exercising the iter-2 process-group cancel
    /// path. Without `process_group(0)` + `kill(-pgid, ...)`, killing the
    /// immediate `bash` would leave `sleep` holding the pipes open.
    #[cfg(test)]
    TestSleepWithChild { seconds: u64 },
    /// Test-only: spawn `bash -c 'trap "" TERM; sleep <N> & wait'`. SIGTERM-
    /// trapped bash forces the SIGKILL escalation branch AND there's a
    /// grandchild — covers both the escalation path and the grandchild reach
    /// via process-group SIGKILL.
    #[cfg(test)]
    TestSleepWithChildIgnoringTerm { seconds: u64 },
}

impl HelperCommand {
    /// Get the command name for logging (and the wire spelling — every
    /// non-test name is a member of [`VERBS`]).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Provision => "provision",
            Self::Deploy => "deploy",
            Self::Teardown => "teardown",
            Self::Recreate => "recreate",
            Self::RestoreKubeconfig => "restore-kubeconfig",
            Self::Status => "status",
            Self::Cancel => "cancel",
            #[cfg(test)]
            Self::TestSleep { .. } => "test-sleep",
            #[cfg(test)]
            Self::TestSleepIgnoringTerm { .. } => "test-sleep-ignoring-term",
            #[cfg(test)]
            Self::TestSleepWithChild { .. } => "test-sleep-with-child",
            #[cfg(test)]
            Self::TestSleepWithChildIgnoringTerm { .. } => "test-sleep-with-child-ignoring-term",
        }
    }

    /// Get the arguments for logging. Wire verbs carry none.
    pub fn args_for_log(&self) -> Vec<String> {
        match self {
            Self::Provision
            | Self::Deploy
            | Self::Teardown
            | Self::Recreate
            | Self::RestoreKubeconfig
            | Self::Status
            | Self::Cancel => vec![],
            #[cfg(test)]
            Self::TestSleep { seconds } => vec![seconds.to_string()],
            #[cfg(test)]
            Self::TestSleepIgnoringTerm { seconds } => vec![seconds.to_string()],
            #[cfg(test)]
            Self::TestSleepWithChild { seconds } => vec![seconds.to_string()],
            #[cfg(test)]
            Self::TestSleepWithChildIgnoringTerm { seconds } => vec![seconds.to_string()],
        }
    }

    /// Classify this command as a write that must serialize on the per-helper
    /// write mutex. Reads (`Status`) skip the lock entirely; control commands
    /// (`Cancel`) signal an in-flight write without acquiring the lock.
    pub fn is_write(&self) -> bool {
        match self {
            Self::Provision
            | Self::Deploy
            | Self::Teardown
            | Self::Recreate
            | Self::RestoreKubeconfig => true,
            Self::Status | Self::Cancel => false,
            // TestSleep is a write so the test stub exercises the real
            // write-lock + cancel-token + child-kill paths.
            #[cfg(test)]
            Self::TestSleep { .. }
            | Self::TestSleepIgnoringTerm { .. }
            | Self::TestSleepWithChild { .. }
            | Self::TestSleepWithChildIgnoringTerm { .. } => true,
        }
    }
}

impl fmt::Display for HelperCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())?;
        for arg in self.args_for_log() {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// Request from the client to the helper: a token and a verb, NOTHING else.
///
/// `deny_unknown_fields` is the argument allowlist: there is no argument field,
/// so a client that sends one (the retired `service` / `skip_observability`,
/// or anything new) is rejected at deserialization — before `parse_command`,
/// before any exec. Adding a field here reopens the ADR-0030 argument surface
/// and needs @security's review.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Authentication token.
    pub token: String,
    /// Command name.
    pub command: String,
}

/// Hand-written so a `{:?}` of a request can never print the token.
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("token", &"<redacted>")
            .field("command", &self.command)
            .finish()
    }
}

impl Request {
    /// Validate and parse the request into a typed command.
    pub fn parse_command(&self) -> Result<HelperCommand, HelperError> {
        // Reject null bytes and control characters in all string fields
        validate_no_control_chars(&self.command, "command")?;
        validate_no_control_chars(&self.token, "token")?;

        match self.command.as_str() {
            "provision" => Ok(HelperCommand::Provision),
            "deploy" => Ok(HelperCommand::Deploy),
            "teardown" => Ok(HelperCommand::Teardown),
            "recreate" => Ok(HelperCommand::Recreate),
            "restore-kubeconfig" => Ok(HelperCommand::RestoreKubeconfig),
            "status" => Ok(HelperCommand::Status),
            "cancel" => Ok(HelperCommand::Cancel),
            // Names the valid set so a client from a different tree (a retired
            // verb like `rebuild-all`, or a newer one this helper predates)
            // gets the fix, not just a rejection: client and helper must come
            // from the same tree (restart the devloop / rebuild the helper).
            other => Err(HelperError::InvalidCommand(format!(
                "{other} (this helper accepts: {}; a client/helper from a different tree — \
                 rebuild and restart the helper via infra/devloop/devloop.sh)",
                VERBS.join(", ")
            ))),
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
    /// Optional structured data. On success: command-specific result (e.g.,
    /// port map on provision). On failure with `error_kind == "busy"`: a
    /// `{op, args}` object naming the in-flight write that blocked this
    /// request. This is intentionally dual-role to keep the wire schema
    /// additive — older clients ignore unknown JSON keys.
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
        // For Busy errors, surface the in-flight op/args as structured data so
        // clients can render `helper busy with <op>` (per Obs O1; uses the
        // dual-role `data` field documented above).
        let data = match error {
            HelperError::Busy { op, args } => Some(serde_json::json!({
                "op": op,
                "args": args,
            })),
            _ => None,
        };
        Self {
            success: false,
            message: error.to_string(),
            error_kind: Some(error.kind().to_string()),
            data,
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
    /// Machine-readable error kind (matches `HelperError::kind()`) on failure.
    /// Lets the connection handler emit the right audit-log shape (e.g.,
    /// `rejected_busy` for `kind == "busy"`, `cancelled` outcome for
    /// `kind == "cancelled"`) without parsing the human-readable error string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// The decision lines the write's scripts printed ([`WriteDecisions`]),
    /// for the audit log ONLY: never serialized, so the client wire shape is
    /// unchanged.
    #[serde(skip)]
    pub decisions: WriteDecisions,
}

/// The two operator-facing decision lines a cluster-building write can print,
/// captured from its child's output for the audit log (helper.log persists
/// across runs; the client stream does not). ONLY these two line shapes are
/// ever recorded — they carry hash prefixes, section labels, reason tokens and
/// workload names, never secret material — and each is truncated to
/// [`MAX_LINE_LEN`] like any streamed line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteDecisions {
    /// The provision decision (`BLUEPRINT ACTION=… REASON=…`). A real
    /// provision/rebuild line is kept over a later `ACTION=check` one (deploy's
    /// guard re-checks the blueprint; the decision that cost minutes is the
    /// provision one).
    pub blueprint: Option<String>,
    /// The last `DEPLOY_FAILED REASON=… WORKLOADS=…` line, when deploy failed.
    pub deploy_failed: Option<String>,
}

impl WriteDecisions {
    /// Record `line` if it is one of the two decision shapes; ignore anything
    /// else.
    pub fn observe(&mut self, line: &str) {
        if line.starts_with("BLUEPRINT ") {
            let is_check = line.contains(" ACTION=check ");
            let have_real = self
                .blueprint
                .as_deref()
                .is_some_and(|b| !b.contains(" ACTION=check "));
            if !(is_check && have_real) {
                self.blueprint = Some(truncate_line(line.to_string(), MAX_LINE_LEN));
            }
        } else if line.starts_with("DEPLOY_FAILED ") {
            self.deploy_failed = Some(truncate_line(line.to_string(), MAX_LINE_LEN));
        }
    }
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

    fn req(command: &str) -> Request {
        Request {
            token: "abc123".to_string(),
            command: command.to_string(),
        }
    }

    /// The wire allowlist: `parse_command` accepts EXACTLY [`VERBS`], and each
    /// accepted command's `name()` is the wire spelling it was parsed from.
    #[test]
    fn test_parse_command_accepts_exactly_verbs() {
        assert_eq!(
            VERBS.len(),
            7,
            "positive control: the verb set is non-empty and pinned"
        );
        for verb in VERBS {
            let parsed = req(verb).parse_command();
            assert!(parsed.is_ok(), "{verb} must parse: {parsed:?}");
            let cmd = parsed.unwrap();
            assert_eq!(cmd.name(), *verb, "name() must round-trip the wire verb");
            assert_eq!(cmd.to_string(), *verb, "Display carries no args");
        }
        // Every non-test variant is reachable from the wire (no verb exists
        // that the allowlist forgot).
        for cmd in [
            HelperCommand::Provision,
            HelperCommand::Deploy,
            HelperCommand::Teardown,
            HelperCommand::Recreate,
            HelperCommand::RestoreKubeconfig,
            HelperCommand::Status,
            HelperCommand::Cancel,
        ] {
            assert!(
                VERBS.contains(&cmd.name()),
                "{} missing from VERBS",
                cmd.name()
            );
        }
    }

    /// Retired by ADR-0038 step 3: no alias survives, and the rejection names
    /// the valid set plus the version-skew fix.
    #[test]
    fn test_retired_verbs_are_invalid_command() {
        for retired in ["setup", "rebuild", "rebuild-all"] {
            let err = req(retired).parse_command().unwrap_err();
            assert_eq!(err.kind(), "invalid_command", "{retired}");
            let msg = err.to_string();
            assert!(msg.contains(retired), "{retired}: {msg}");
            assert!(msg.contains("provision, deploy"), "{retired}: {msg}");
            assert!(msg.contains("devloop.sh"), "{retired}: {msg}");
        }
    }

    /// The retired argument fields — and any other field — are rejected at
    /// deserialization, before parse and before any exec.
    #[test]
    fn test_argument_fields_rejected_at_deserialization() {
        for json in [
            r#"{"token":"abc","command":"deploy","service":"gc"}"#,
            r#"{"token":"abc","command":"provision","skip_observability":true}"#,
            r#"{"token":"abc","command":"provision","skip_observability":false}"#,
            r#"{"token":"abc","command":"status","unknown_field":"evil"}"#,
            r#"{"token":"abc","command":"recreate","service":null}"#,
        ] {
            let err = serde_json::from_str::<Request>(json)
                .expect_err(&format!("must be rejected: {json}"));
            assert!(err.to_string().contains("unknown field"), "{json}: {err}");
        }
    }

    /// Injection shapes against the `{token, command}` wire: every one is
    /// rejected by `parse_command` (the gate in front of any exec).
    #[test]
    fn test_command_injection_shapes_rejected() {
        for command in [
            "deploy mc",
            "provision ",
            " provision",
            "deploy;rm -rf /",
            "provision && id",
            "$(whoami)",
            "`id`",
            "deploy | cat /etc/passwd",
            "../provision",
            "PROVISION",
            "",
        ] {
            let err = req(command).parse_command().unwrap_err();
            assert_eq!(err.kind(), "invalid_command", "{command:?}");
        }
        for command in ["provision\0", "deploy\nteardown", "status\r", "cancel\t"] {
            let err = req(command).parse_command().unwrap_err();
            assert!(
                err.to_string().contains("control character"),
                "{command:?}: {err}"
            );
        }
    }

    #[test]
    fn test_parse_command_rejects_tab_in_token() {
        let r = Request {
            token: "abc\t123".to_string(),
            command: "provision".to_string(),
        };
        let err = r.parse_command().unwrap_err();
        assert!(err.to_string().contains("control character"), "got: {err}");
    }

    /// `{:?}` of a request never prints the token (semantic-guard
    /// credential-leak).
    #[test]
    fn test_request_debug_redacts_token() {
        let r = Request {
            token: "s3cr3t-token-value".to_string(),
            command: "deploy".to_string(),
        };
        let dbg = format!("{r:?}");
        assert!(!dbg.contains("s3cr3t-token-value"), "got: {dbg}");
        assert!(
            dbg.contains("<redacted>") && dbg.contains("deploy"),
            "got: {dbg}"
        );
    }

    /// Security S6 + CR5 + Test #1 regression: the cfg(test) `TestSleep`,
    /// `TestSleepIgnoringTerm`, `TestSleepWithChild`, and
    /// `TestSleepWithChildIgnoringTerm` variants exist in the enum but MUST
    /// NOT have parse arms. Any client sending the matching `command` strings
    /// over the socket gets `invalid_command`, even with `cfg(test)` enabled.
    /// The fall-through arm in `parse_command` already provides the structural
    /// guarantee; this test pins all four literal names so a future refactor
    /// that accidentally adds a parse arm is caught explicitly.
    #[test]
    fn test_release_does_not_expose_test_commands() {
        for name in [
            "test-sleep",
            "test-sleep-ignoring-term",
            "test-sleep-with-child",
            "test-sleep-with-child-ignoring-term",
        ] {
            let json = format!(r#"{{"token":"abcdef","command":"{name}"}}"#);
            let req: Request = serde_json::from_str(&json).unwrap();
            let err = req.parse_command().unwrap_err();
            assert_eq!(err.kind(), "invalid_command", "name={name}");
            assert!(err.to_string().contains(name), "name={name}, got: {err}");
        }
    }

    #[test]
    fn test_is_write_classification() {
        assert!(HelperCommand::Provision.is_write());
        assert!(HelperCommand::Deploy.is_write());
        assert!(HelperCommand::Teardown.is_write());
        assert!(HelperCommand::Recreate.is_write());
        assert!(HelperCommand::RestoreKubeconfig.is_write());
        assert!(!HelperCommand::Status.is_write());
        assert!(!HelperCommand::Cancel.is_write());
        // TestSleep variants are classified as writes so the stubs exercise
        // the real write-lock + cancel paths.
        assert!(HelperCommand::TestSleep { seconds: 1 }.is_write());
        assert!(HelperCommand::TestSleepIgnoringTerm { seconds: 1 }.is_write());
        assert!(HelperCommand::TestSleepWithChild { seconds: 1 }.is_write());
        assert!(HelperCommand::TestSleepWithChildIgnoringTerm { seconds: 1 }.is_write());
    }

    #[test]
    fn test_busy_response_data_carries_op_and_args() {
        let err = HelperError::Busy {
            op: "test-sleep".to_string(),
            args: vec!["30".to_string()],
        };
        let resp = Response::err(&err);
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("busy"));
        let data = resp.data.expect("busy response must include data");
        assert_eq!(data["op"], "test-sleep");
        assert_eq!(data["args"][0], "30");
    }

    #[test]
    fn test_non_busy_response_omits_data() {
        let err = HelperError::AuthFailed;
        let resp = Response::err(&err);
        assert!(resp.data.is_none());
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
            cmd: "provision".to_string(),
            ts: "2026-04-08T14:23:45.000Z".to_string(),
        };
        let json = serde_json::to_string(&started).unwrap();
        assert!(json.contains("\"started\":true"));
        assert!(json.contains("\"cmd\":\"provision\""));
    }

    #[test]
    fn test_command_result_ok_serialization() {
        let result = CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms: 42000,
            error: None,
            error_kind: None,
            data: None,
            decisions: WriteDecisions {
                blueprint: Some("BLUEPRINT ACTION=none REASON=match".to_string()),
                deploy_failed: None,
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        // The decision lines are for the audit log only — never on the wire.
        assert!(!json.contains("BLUEPRINT"), "{json}");
        assert!(!json.contains("decisions"), "{json}");
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
            error: Some("deploy.sh failed".to_string()),
            error_kind: Some("command_failed".to_string()),
            data: None,
            decisions: WriteDecisions::default(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"result\":\"error\""));
        assert!(json.contains("\"exit_code\":1"));
        assert!(json.contains("\"error\":\"deploy.sh failed\""));
    }

    #[test]
    fn test_command_result_with_data() {
        let data = serde_json::json!({"port": 8080});
        let result = CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms: 1000,
            error: None,
            error_kind: None,
            data: Some(data.clone()),
            decisions: WriteDecisions::default(),
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
            error_kind: Some("command_failed".to_string()),
            data: None,
            decisions: WriteDecisions::default(),
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

    #[test]
    fn write_decisions_record_only_the_two_decision_shapes() {
        let mut d = WriteDecisions::default();
        d.observe("building localhost/gc-service ...");
        d.observe("  BLUEPRINT indented is not a decision line");
        d.observe("password=hunter2");
        assert_eq!(d, WriteDecisions::default());
        d.observe(
            "BLUEPRINT ACTION=rebuild REASON=changed RECORDED=a CURRENT=b CHANGED=kind-config",
        );
        d.observe("DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/deployment/mc-0");
        assert_eq!(
            d.blueprint.as_deref(),
            Some(
                "BLUEPRINT ACTION=rebuild REASON=changed RECORDED=a CURRENT=b CHANGED=kind-config"
            )
        );
        assert_eq!(
            d.deploy_failed.as_deref(),
            Some("DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/deployment/mc-0")
        );
    }

    /// deploy's guard re-checks the blueprint after a recreate's provision:
    /// the provision decision (the one that cost minutes) is kept over the
    /// later `ACTION=check` line — but a check is recorded when it is all
    /// there is (a plain deploy).
    #[test]
    fn write_decisions_keep_the_real_provision_line_over_a_later_check() {
        let mut d = WriteDecisions::default();
        d.observe("BLUEPRINT ACTION=check REASON=match RECORDED=a CURRENT=a CHANGED=-");
        assert!(d.blueprint.as_deref().unwrap().contains("ACTION=check"));
        d.observe("BLUEPRINT ACTION=rebuild REASON=missing RECORDED=none CURRENT=b CHANGED=-");
        d.observe("BLUEPRINT ACTION=check REASON=match RECORDED=b CURRENT=b CHANGED=-");
        assert!(d.blueprint.as_deref().unwrap().contains("ACTION=rebuild"));
    }

    #[test]
    fn write_decisions_truncate_like_any_streamed_line() {
        let mut d = WriteDecisions::default();
        d.observe(&format!(
            "DEPLOY_FAILED REASON=rollout-failed WORKLOADS={}",
            "x".repeat(MAX_LINE_LEN * 2)
        ));
        assert!(d.deploy_failed.unwrap().len() <= MAX_LINE_LEN + 32);
    }
}
