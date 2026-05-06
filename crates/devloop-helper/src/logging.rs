//! Audit logging for the devloop helper.
//!
//! JSONL format: one JSON object per line. Machine-parseable for diagnostics.

use serde::Serialize;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Audit log writer. Appends JSONL entries to the log file.
pub struct AuditLog {
    path: PathBuf,
}

/// Locked vocabulary for the `outcome` field. Emission sites MUST reference
/// these constants by name (a typo'd literal would otherwise compile). Adding
/// a sixth outcome requires adding a const here, extending `OUTCOMES`, and
/// updating `test_outcome_round_trip`.
pub const OUTCOME_COMPLETED: &str = "completed";
pub const OUTCOME_CANCELLED: &str = "cancelled";
pub const OUTCOME_ERROR: &str = "error";
pub const OUTCOME_REJECTED: &str = "rejected";
pub const OUTCOME_NO_OP: &str = "no-op";

/// Round-trip vocabulary for `test_outcome_round_trip`. Kept as a slice so the
/// test enumerates every const above; not referenced from production code.
#[cfg(test)]
pub const OUTCOMES: &[&str] = &[
    OUTCOME_COMPLETED,
    OUTCOME_CANCELLED,
    OUTCOME_ERROR,
    OUTCOME_REJECTED,
    OUTCOME_NO_OP,
];

/// A single audit log entry.
///
/// Field cross-use note: `error` is also used by `cancel` events to carry the
/// cancelled-op identifier — this is intentional cross-use to avoid schema
/// growth. `rejected_busy` events instead encode the collision pair into
/// `args` per Obs option A; their `error` is `None`.
#[derive(Debug, Serialize)]
struct LogEntry<'a> {
    ts: String,
    cmd: &'a str,
    args: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    /// Outcome classifier. One of `OUTCOMES`; absent on legacy entries (e.g.,
    /// `startup`) so historical grep tooling continues to parse them.
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<&'a str>,
}

impl AuditLog {
    /// Create a new audit log, preserving any existing entries from prior sessions.
    pub fn new(path: &Path) -> Result<Self, std::io::Error> {
        // Create the log file with 0600 permissions (append mode preserves history)
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Log a startup entry.
    pub fn log_startup(&self, cluster_name: &str, socket_path: &str, pid: u32) {
        let args = vec![
            format!("cluster={cluster_name}"),
            format!("socket={socket_path}"),
            format!("pid={pid}"),
        ];
        self.append(&LogEntry {
            ts: now_rfc3339(),
            cmd: "startup",
            args: &args,
            duration_ms: None,
            exit_code: None,
            error: None,
            outcome: None,
        });
    }

    /// Log a command execution result.
    ///
    /// `outcome` MUST be one of the `OUTCOME_*` consts or `None` for entries
    /// where outcome is not applicable (legacy callers; `startup`).
    pub fn log_command(
        &self,
        cmd: &str,
        args: &[String],
        duration_ms: u64,
        exit_code: i32,
        error: Option<&str>,
        outcome: Option<&str>,
    ) {
        self.append(&LogEntry {
            ts: now_rfc3339(),
            cmd,
            args,
            duration_ms: Some(duration_ms),
            exit_code: Some(exit_code),
            error,
            outcome,
        });
    }

    /// Append a log entry to the file.
    fn append(&self, entry: &LogEntry<'_>) {
        let result = (|| -> Result<(), std::io::Error> {
            let mut file = fs::OpenOptions::new().append(true).open(&self.path)?;
            let json = serde_json::to_string(entry).map_err(std::io::Error::other)?;
            writeln!(file, "{json}")?;
            Ok(())
        })();

        if let Err(e) = result {
            eprintln!("[devloop-helper] failed to write audit log: {e}");
        }
    }
}

/// Get current time as RFC 3339 string with millisecond precision.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_creation() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let _log = AuditLog::new(&log_path).unwrap();

        assert!(log_path.exists());
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(&log_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn test_audit_log_startup_entry() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        log.log_startup("devloop-test", "/tmp/devloop-test/helper.sock", 12345);

        let contents = fs::read_to_string(&log_path).unwrap();
        let entry: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(entry["cmd"], "startup");
        assert!(entry["ts"].as_str().is_some());
    }

    #[test]
    fn test_audit_log_command_entry() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        let args = vec!["ac".to_string()];
        log.log_command("rebuild", &args, 45123, 0, None, Some("completed"));

        let contents = fs::read_to_string(&log_path).unwrap();
        let entry: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(entry["cmd"], "rebuild");
        assert_eq!(entry["duration_ms"], 45123);
        assert_eq!(entry["exit_code"], 0);
        assert_eq!(entry["outcome"], "completed");
        assert!(entry.get("error").is_none());
    }

    #[test]
    fn test_audit_log_command_with_error() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        log.log_command(
            "setup",
            &[],
            120000,
            1,
            Some("kind create failed"),
            Some("error"),
        );

        let contents = fs::read_to_string(&log_path).unwrap();
        let entry: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(entry["cmd"], "setup");
        assert_eq!(entry["exit_code"], 1);
        assert_eq!(entry["error"], "kind create failed");
        assert_eq!(entry["outcome"], "error");
    }

    /// Obs follow-up: lock the outcome value vocabulary by enumerating all
    /// five and asserting they round-trip through serialization.
    #[test]
    fn test_outcome_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        for &outcome in OUTCOMES {
            log.log_command("setup", &[], 1, 0, None, Some(outcome));
        }

        let contents = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), OUTCOMES.len());

        for (i, &expected) in OUTCOMES.iter().enumerate() {
            let entry: serde_json::Value = serde_json::from_str(lines[i]).unwrap();
            assert_eq!(
                entry["outcome"], expected,
                "outcome mismatch on entry {i}: {entry}"
            );
        }
    }

    #[test]
    fn test_outcome_absent_is_serialized_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        log.log_command("setup", &[], 1, 0, None, None);

        let contents = fs::read_to_string(&log_path).unwrap();
        let entry: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert!(entry.get("outcome").is_none(), "got: {entry}");
    }

    #[test]
    fn test_audit_log_multiple_entries() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("helper.log");
        let log = AuditLog::new(&log_path).unwrap();

        log.log_startup("devloop-test", "/tmp/test.sock", 1);
        log.log_command("setup", &[], 5000, 0, None, Some("completed"));
        log.log_command(
            "rebuild",
            &["ac".to_string()],
            3000,
            0,
            None,
            Some("completed"),
        );

        let contents = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = contents.trim().lines().collect();
        assert_eq!(lines.len(), 3);

        // Each line should be valid JSON
        for line in &lines {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }
}
