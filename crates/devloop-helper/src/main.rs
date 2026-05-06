//! Devloop helper binary — host-side cluster manager per ADR-0030.
//!
//! Listens on a unix socket and handles build/deploy commands for a Kind cluster.
//! Each devloop gets its own helper process managing a dedicated cluster.
//!
//! Usage: devloop-helper <slug> [--project-root <path>]

mod auth;
mod commands;
mod error;
mod logging;
mod ports;
mod protocol;

use error::{HelperError, ValidSlug};
use protocol::{Request, Response, MAX_REQUEST_SIZE};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{fs, process};

/// Maximum number of concurrent connection-handler threads (per Operations
/// Gate 1 + Security S4). Beyond this, accepted sockets are closed
/// immediately and a rate-limited audit entry is written.
const MAX_CONCURRENT_CONNECTIONS: usize = 32;

/// Minimum interval between connection-cap-rejection audit entries
/// (per Security S4 + Obs cancel-related-noise dedup). Avoids audit-log spam
/// under retry storms.
const REJECT_LOG_DEDUP_SECS: i64 = 1;

fn main() {
    if let Err(e) = run() {
        eprintln!("[devloop-helper] fatal: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), HelperError> {
    let args = parse_args()?;
    let slug = ValidSlug::new(&args.slug)?;
    let cluster_name = format!("devloop-{}", slug);

    // Setup runtime directory
    let runtime_dir = PathBuf::from(format!("/tmp/devloop-{slug}"));
    fs::create_dir_all(&runtime_dir)?;
    set_dir_permissions(&runtime_dir)?;

    // Acquire startup lock and handle stale PID
    let lock_path = runtime_dir.join("helper.lock");
    let _lock = acquire_startup_lock(&lock_path)?;
    let pid_path = runtime_dir.join("helper.pid");
    handle_stale_pid(&pid_path, &runtime_dir)?;

    // Write PID file
    write_pid_file(&pid_path)?;

    // Generate auth token
    let token = auth::generate_token()?;
    let token_path = runtime_dir.join("auth-token");
    auth::write_token(&token_path, &token)?;

    // Setup audit log
    let log_path = runtime_dir.join("helper.log");
    let audit_log = logging::AuditLog::new(&log_path)?;

    // Check prerequisites
    let container_runtime = commands::check_prerequisites()?;

    // Determine project root
    let project_root = args
        .project_root
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Create execution context
    // Install signal handlers
    let shutdown = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&shutdown))?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&shutdown))?;

    let ctx = Arc::new(commands::Context {
        slug: slug.as_str().to_string(),
        cluster_name: cluster_name.clone(),
        project_root,
        runtime_dir: runtime_dir.clone(),
        registry_path: ports::registry_path(),
        container_runtime,
        host_gateway_ip: args.host_gateway_ip,
        shutdown: Arc::clone(&shutdown),
        write_state: Arc::new(Mutex::new(commands::WriteState::new())),
    });

    // Bind socket
    let socket_path = runtime_dir.join("helper.sock");
    // Remove stale socket file if it exists
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }
    let listener = UnixListener::bind(&socket_path)?;
    set_file_permissions(&socket_path, 0o600)?;

    // Set socket timeout so we can check the shutdown flag periodically
    listener.set_nonblocking(false)?;

    // Log startup (base_port is not yet known — allocation happens in cmd_setup)
    audit_log.log_startup(&cluster_name, &socket_path.to_string_lossy(), process::id());

    eprintln!(
        "[devloop-helper] ready: slug={slug} cluster={cluster_name} socket={} pid={}",
        socket_path.display(),
        process::id()
    );

    // Accept loop — thread-per-connection (per Plan §"Threading Model").
    // Bounded by MAX_CONCURRENT_CONNECTIONS. Reads run concurrently with
    // writes; writes serialize on ctx.write_state.
    listener.set_nonblocking(true)?;

    let conn_count = Arc::new(AtomicUsize::new(0));
    // RFC3339-millis-encoded last-rejection timestamp (i64 milliseconds since
    // UNIX epoch); 0 means never rejected. Used to dedup connection-cap
    // rejection audit entries to ≤1/sec.
    let last_reject_ms = Arc::new(AtomicI64::new(0));
    // Audit log shared across handler threads (interior `&self`, append-only).
    let audit_log = Arc::new(audit_log);
    let token = Arc::new(token);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            eprintln!("[devloop-helper] shutdown requested, exiting...");
            break;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                if let Err(e) = stream.set_nonblocking(false) {
                    eprintln!("[devloop-helper] failed to set blocking mode: {e}");
                    continue;
                }

                // Try to claim a connection slot (per Security S4 + CR10).
                let current = conn_count.fetch_add(1, Ordering::SeqCst);
                if current >= MAX_CONCURRENT_CONNECTIONS {
                    // Roll back and reject.
                    conn_count.fetch_sub(1, Ordering::SeqCst);
                    maybe_log_cap_reject(&audit_log, &last_reject_ms);
                    drop(stream); // close the socket; client sees EOF
                    continue;
                }

                let token_clone = Arc::clone(&token);
                let ctx_clone = Arc::clone(&ctx);
                let audit_clone = Arc::clone(&audit_log);
                let conn_count_clone = Arc::clone(&conn_count);
                std::thread::spawn(move || {
                    let _guard = ConnGuard {
                        counter: conn_count_clone,
                    };
                    if let Err(e) =
                        handle_connection(stream, &token_clone, &ctx_clone, &audit_clone)
                    {
                        eprintln!("[devloop-helper] connection error: {e}");
                    }
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                eprintln!("[devloop-helper] accept error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    // Cleanup on exit
    cleanup(&pid_path, &socket_path, &runtime_dir);
    Ok(())
}

/// RAII connection-counter guard. Decrements on drop so panics in the
/// handler don't leak the count (per Security S4).
struct ConnGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Extract `(op, args)` from a `CommandResult.data` payload that follows the
/// `{op, args}` envelope. `op` defaults to `"unknown"`; `args` is the
/// space-joined string of the array's string elements (defaults to empty).
/// Used by the busy-collision and cancel-target audit shapes.
fn extract_op_and_args(data: Option<&serde_json::Value>) -> (String, String) {
    let op = data
        .and_then(|d| d.get("op"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let args = data
        .and_then(|d| d.get("args"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    (op, args)
}

/// Write a single connection-cap-rejection audit entry, deduped to
/// ≤1 per `REJECT_LOG_DEDUP_SECS` (per Security S4).
fn maybe_log_cap_reject(audit_log: &logging::AuditLog, last_reject_ms: &AtomicI64) {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let last = last_reject_ms.load(Ordering::Relaxed);
    if now_ms - last < REJECT_LOG_DEDUP_SECS * 1000 {
        return;
    }
    if last_reject_ms
        .compare_exchange(last, now_ms, Ordering::SeqCst, Ordering::Relaxed)
        .is_ok()
    {
        audit_log.log_command(
            "rejected",
            &[],
            0,
            1,
            Some("connection_cap_reached"),
            Some(logging::OUTCOME_REJECTED),
        );
    }
}

/// Handle a single client connection.
fn handle_connection(
    stream: std::os::unix::net::UnixStream,
    expected_token: &str,
    ctx: &commands::Context,
    audit_log: &logging::AuditLog,
) -> Result<(), HelperError> {
    let mut writer = stream.try_clone()?;

    // Read request with size limit
    let mut line = String::new();
    let limited = stream.take(MAX_REQUEST_SIZE);
    let mut limited_buf = BufReader::new(limited);

    let bytes_read = limited_buf.read_line(&mut line)?;
    if bytes_read == 0 {
        let resp = Response::err(&HelperError::InvalidRequest("empty request".to_string()));
        send_response(&mut writer, &resp)?;
        return Ok(());
    }

    // Check if we hit the size limit (no newline found within MAX_REQUEST_SIZE)
    if !line.ends_with('\n') && line.len() as u64 >= MAX_REQUEST_SIZE {
        let err = HelperError::RequestTooLarge;
        audit_log.log_command(
            "unknown",
            &[],
            0,
            1,
            Some(&err.to_string()),
            Some(logging::OUTCOME_ERROR),
        );
        let resp = Response::err(&err);
        send_response(&mut writer, &resp)?;
        return Ok(());
    }

    // Parse JSON request
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(req) => req,
        Err(e) => {
            let err = HelperError::InvalidRequest(format!("invalid JSON: {e}"));
            audit_log.log_command(
                "unknown",
                &[],
                0,
                1,
                Some(&err.to_string()),
                Some(logging::OUTCOME_ERROR),
            );
            let resp = Response::err(&err);
            send_response(&mut writer, &resp)?;
            return Ok(());
        }
    };

    // Validate auth token (every command including cancel — per CR9)
    if let Err(e) = auth::validate_token(&request.token, expected_token) {
        audit_log.log_command(
            "auth",
            &[],
            0,
            1,
            Some(&e.to_string()),
            Some(logging::OUTCOME_ERROR),
        );
        let resp = Response::err(&e);
        send_response(&mut writer, &resp)?;
        return Ok(());
    }

    // Parse and validate command
    let cmd = match request.parse_command() {
        Ok(cmd) => cmd,
        Err(e) => {
            audit_log.log_command(
                &request.command,
                &[],
                0,
                1,
                Some(&e.to_string()),
                Some(logging::OUTCOME_ERROR),
            );
            let resp = Response::err(&e);
            send_response(&mut writer, &resp)?;
            return Ok(());
        }
    };

    // Execute command with streaming output
    let cmd_name = cmd.name().to_string();
    let cmd_args = cmd.args_for_log();

    let result = commands::execute(&cmd, ctx, &mut writer);

    // Send final result to client
    if let Err(e) = commands::send_json_line(&mut writer, &result) {
        eprintln!("[devloop-helper] failed to send command result: {e}");
    }

    // Audit log per Obs schema:
    // - Busy errors → "rejected_busy" entry with collision pair in args (option A).
    // - Cancel command → "cancel" entry with target=<op> in args.
    // - Cancelled writes → keep cmd_name (e.g. "setup"), error prefix "cancelled".
    // - Everything else → standard cmd entry with appropriate outcome.
    let exit_code = result.exit_code.unwrap_or(-1);
    let error_str = result.error.as_deref();
    let kind = result.error_kind.as_deref();

    if kind == Some("busy") {
        // Per Obs option A: collision pair lives in `args`, not `error`.
        let (in_flight_op, in_flight_args) = extract_op_and_args(result.data.as_ref());
        let rejected_args = cmd_args.join(" ");
        let collision_args = vec![
            format!("rejected={cmd_name}"),
            format!("rejected_args={rejected_args}"),
            format!("in_flight={in_flight_op}"),
            format!("in_flight_args={in_flight_args}"),
        ];
        audit_log.log_command(
            "rejected_busy",
            &collision_args,
            result.duration_ms,
            exit_code,
            None,
            Some(logging::OUTCOME_REJECTED),
        );
    } else if cmd_name == "cancel" {
        // Per Obs O2: cancel audit entry names what got cancelled.
        let cancelled = result
            .data
            .as_ref()
            .and_then(|d| d.get("cancelled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if cancelled {
            let (target_op, target_args) = extract_op_and_args(result.data.as_ref());
            let cancel_args = vec![
                format!("target={target_op}"),
                format!("target_args={target_args}"),
            ];
            audit_log.log_command(
                "cancel",
                &cancel_args,
                result.duration_ms,
                exit_code,
                Some(target_op.as_str()),
                Some(logging::OUTCOME_COMPLETED),
            );
        } else {
            audit_log.log_command(
                "cancel",
                &[],
                result.duration_ms,
                exit_code,
                Some("none"),
                Some(logging::OUTCOME_NO_OP),
            );
        }
    } else {
        // Standard write/status entry. Outcome derived from kind/exit.
        let outcome = if kind == Some("cancelled") {
            Some(logging::OUTCOME_CANCELLED)
        } else if exit_code == 0 {
            Some(logging::OUTCOME_COMPLETED)
        } else {
            Some(logging::OUTCOME_ERROR)
        };
        audit_log.log_command(
            &cmd_name,
            &cmd_args,
            result.duration_ms,
            exit_code,
            error_str,
            outcome,
        );
    }

    Ok(())
}

/// Send a JSON response followed by a newline.
fn send_response(writer: &mut impl Write, resp: &Response) -> Result<(), HelperError> {
    let json = serde_json::to_string(resp)?;
    writeln!(writer, "{json}")?;
    writer.flush()?;
    Ok(())
}

/// CLI arguments.
struct Args {
    slug: String,
    project_root: Option<PathBuf>,
    host_gateway_ip: Option<String>,
}

/// Parse CLI arguments.
fn parse_args() -> Result<Args, HelperError> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: devloop-helper <slug> [--project-root <path>] [--host-gateway-ip <ip>]");
        return Err(HelperError::InvalidRequest(
            "missing required argument: slug".to_string(),
        ));
    }

    let slug = args
        .get(1)
        .ok_or_else(|| HelperError::InvalidRequest("missing slug argument".to_string()))?
        .clone();

    let mut project_root = None;
    let mut host_gateway_ip = None;
    let mut i = 2;
    while i < args.len() {
        let arg = args.get(i).ok_or_else(|| {
            HelperError::InvalidRequest("unexpected end of arguments".to_string())
        })?;
        match arg.as_str() {
            "--project-root" => {
                i += 1;
                let val = args.get(i).ok_or_else(|| {
                    HelperError::InvalidRequest("--project-root requires a value".to_string())
                })?;
                project_root = Some(PathBuf::from(val));
            }
            "--host-gateway-ip" => {
                i += 1;
                let val = args.get(i).ok_or_else(|| {
                    HelperError::InvalidRequest("--host-gateway-ip requires a value".to_string())
                })?;
                host_gateway_ip = Some(val.clone());
            }
            other => {
                return Err(HelperError::InvalidRequest(format!(
                    "unknown argument: {other}"
                )));
            }
        }
        i += 1;
    }

    Ok(Args {
        slug,
        project_root,
        host_gateway_ip,
    })
}

/// Set directory permissions to 0700.
fn set_dir_permissions(path: &Path) -> Result<(), HelperError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o700);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Set file permissions.
fn set_file_permissions(path: &Path, mode: u32) -> Result<(), HelperError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(mode);
    fs::set_permissions(path, perms)?;
    Ok(())
}

/// Acquire a startup lock to prevent concurrent helper launches for the same slug.
fn acquire_startup_lock(lock_path: &Path) -> Result<fs::File, HelperError> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock_path)?;

    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::WouldBlock {
            return Err(HelperError::AlreadyRunning(0));
        }
        return Err(HelperError::Io(err));
    }
    Ok(file)
}

/// Check for a stale PID file and clean up if the process is dead.
fn handle_stale_pid(pid_path: &Path, runtime_dir: &Path) -> Result<(), HelperError> {
    if !pid_path.exists() {
        return Ok(());
    }

    let pid_str = fs::read_to_string(pid_path)?;
    let pid: u32 = pid_str.trim().parse().map_err(|_| {
        HelperError::InvalidRequest(format!("invalid PID file content: {}", pid_str.trim()))
    })?;

    // Check if process is alive and is a devloop-helper
    if ports::is_helper_alive(pid) {
        return Err(HelperError::AlreadyRunning(pid));
    }
    // Process is dead or PID was recycled — proceed with cleanup

    // Process is dead or PID was recycled — clean up stale files
    eprintln!("[devloop-helper] cleaning up stale files from PID {pid}");
    let stale_files = ["helper.pid", "helper.sock", "auth-token"];
    for file in &stale_files {
        let path = runtime_dir.join(file);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }

    Ok(())
}

/// Write the current PID to the PID file.
fn write_pid_file(pid_path: &Path) -> Result<(), HelperError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(pid_path)?;
    write!(file, "{}", process::id())?;
    file.flush()?;
    Ok(())
}

/// Clean up PID file, socket, and auth token on exit.
fn cleanup(pid_path: &Path, socket_path: &Path, runtime_dir: &Path) {
    let _ = fs::remove_file(pid_path);
    let _ = fs::remove_file(socket_path);
    let _ = fs::remove_file(runtime_dir.join("auth-token"));
}

/// Test-only: build a stock `commands::Context` rooted at `dir` (typically a
/// `TempDir` path). Centralizes the setup repeated by socket-roundtrip and
/// concurrency tests so a future field addition only edits one place.
#[cfg(test)]
fn build_test_context(dir: &Path) -> commands::Context {
    commands::Context {
        slug: "test".to_string(),
        cluster_name: "devloop-test".to_string(),
        project_root: PathBuf::from("/tmp/devloop-test-nonexistent"),
        runtime_dir: dir.to_path_buf(),
        registry_path: dir.join("port-registry.json"),
        container_runtime: commands::ContainerRuntime::Podman,
        host_gateway_ip: None,
        shutdown: Arc::new(AtomicBool::new(false)),
        write_state: Arc::new(Mutex::new(commands::WriteState::new())),
    }
}

/// Test-only: read all newline-delimited JSON lines from `reader` until EOF.
/// Errors are treated as EOF — callers want best-effort drain. Shared by the
/// socket-roundtrip helpers in `tests` and the `TestHelper` methods in
/// `concurrency_tests`.
#[cfg(test)]
fn read_all_ndjson(reader: &mut impl BufRead) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => lines.push(line),
            Err(_) => break,
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_slug() {
        assert!(ValidSlug::new("my-task").is_ok());
        assert!(ValidSlug::new("task123").is_ok());
        assert!(ValidSlug::new("a").is_ok());
        assert!(ValidSlug::new("a-b-c").is_ok());
        assert!(ValidSlug::new("1").is_ok());
    }

    #[test]
    fn test_invalid_slug_path_traversal() {
        assert!(ValidSlug::new("../../etc").is_err());
        assert!(ValidSlug::new("../..").is_err());
        assert!(ValidSlug::new("test/../passwd").is_err());
    }

    #[test]
    fn test_invalid_slug_special_chars() {
        assert!(ValidSlug::new("").is_err());
        assert!(ValidSlug::new("-starts-with-dash").is_err());
        assert!(ValidSlug::new("ends-with-dash-").is_err());
        assert!(ValidSlug::new("has spaces").is_err());
        assert!(ValidSlug::new("has.dots").is_err());
        assert!(ValidSlug::new("has/slashes").is_err());
        assert!(ValidSlug::new("has\\backslashes").is_err());
        assert!(ValidSlug::new("HAS_UPPERCASE").is_err());
        assert!(ValidSlug::new("has_underscore").is_err());
        assert!(ValidSlug::new("has\0null").is_err());
        assert!(ValidSlug::new("has\nnewline").is_err());
    }

    #[test]
    fn test_invalid_slug_too_long() {
        let long_slug = "a".repeat(64);
        assert!(ValidSlug::new(&long_slug).is_err());
    }

    #[test]
    fn test_slug_max_length() {
        let slug = "a".repeat(63);
        assert!(ValidSlug::new(&slug).is_ok());
    }

    // --- Socket-level injection regression tests ---
    // These create a real UnixListener + UnixStream pair and exercise the full
    // request-handling path: socket read -> size check -> JSON parse -> command
    // validation -> rejection. Per ADR-0030: "tests that send malformed inputs
    // through the socket and verify they are all rejected before any command execution."

    use std::os::unix::net::UnixStream;

    /// Core test helper: set up a socket pair, send raw bytes, run handle_connection,
    /// return all response lines.
    fn socket_roundtrip_raw(request_bytes: &[u8]) -> (String, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let token = auth::generate_token().unwrap();
        let ctx = build_test_context(dir.path());
        let log_path = dir.path().join("helper.log");
        let audit_log = logging::AuditLog::new(&log_path).unwrap();

        let token_clone = token.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).unwrap();
            let _ = handle_connection(stream, &token_clone, &ctx, &audit_log);
        });

        let mut client = UnixStream::connect(&sock_path).unwrap();
        // Write may fail with BrokenPipe if the handler closes after reading its limit
        let _ = client.write_all(request_bytes);
        let _ = client.shutdown(std::net::Shutdown::Write);

        let lines = read_all_ndjson(&mut BufReader::new(&client));

        handle.join().unwrap();

        (token, lines)
    }

    /// Helper: send raw bytes and parse the first response line as a Response.
    /// For pre-execution errors, there is exactly one line.
    fn socket_roundtrip(request_bytes: &[u8]) -> protocol::Response {
        let (_token, lines) = socket_roundtrip_raw(request_bytes);
        if lines.is_empty() || lines[0].trim().is_empty() {
            protocol::Response {
                success: false,
                message: "no response received".to_string(),
                error_kind: Some("no_response".to_string()),
                data: None,
            }
        } else {
            serde_json::from_str(lines[0].trim()).unwrap()
        }
    }

    /// Helper: send a JSON command with a valid token pre-filled.
    /// Returns the first response line parsed as a Response (for pre-execution errors).
    fn socket_roundtrip_with_token(command_json: &str) -> protocol::Response {
        let (_token, lines) = socket_roundtrip_with_token_raw(command_json);
        serde_json::from_str(lines[0].trim()).unwrap()
    }

    /// Helper: send a JSON command with a valid token, return all response lines.
    fn socket_roundtrip_with_token_raw(command_json: &str) -> (String, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();

        let token = auth::generate_token().unwrap();
        let ctx = build_test_context(dir.path());
        let log_path = dir.path().join("helper.log");
        let audit_log = logging::AuditLog::new(&log_path).unwrap();

        let full_json = command_json.replace("TOKEN_PLACEHOLDER", &token);

        let token_clone = token.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).unwrap();
            let _ = handle_connection(stream, &token_clone, &ctx, &audit_log);
        });

        let mut client = UnixStream::connect(&sock_path).unwrap();
        writeln!(client, "{full_json}").unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let lines = read_all_ndjson(&mut BufReader::new(&client));

        handle.join().unwrap();

        (token, lines)
    }

    #[test]
    fn test_socket_shell_metacharacters_in_service() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"rebuild","service":"; rm -rf /"}"#,
        );
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_service"));
    }

    #[test]
    fn test_socket_command_substitution_in_service() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"rebuild","service":"$(whoami)"}"#,
        );
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_service"));
    }

    #[test]
    fn test_socket_backtick_injection_in_service() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"rebuild","service":"`id`"}"#,
        );
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_service"));
    }

    #[test]
    fn test_socket_pipe_in_service() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"rebuild","service":"ac | cat /etc/passwd"}"#,
        );
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_service"));
    }

    #[test]
    fn test_socket_null_bytes_in_command() {
        // Null byte in the middle of a command
        let mut request = br#"{"token":"fake","command":"setup"#.to_vec();
        request.push(0x00);
        request.extend_from_slice(br#"extra"}"#);
        request.push(b'\n');
        let resp = socket_roundtrip(&request);
        assert!(!resp.success);
    }

    #[test]
    fn test_socket_newline_in_service_field() {
        let resp = socket_roundtrip_with_token(
            "{\"token\":\"TOKEN_PLACEHOLDER\",\"command\":\"rebuild\",\"service\":\"ac\\nmalicious\"}",
        );
        assert!(!resp.success);
    }

    #[test]
    fn test_socket_invalid_auth_token() {
        let request = br#"{"token":"wrong_token_definitely_not_valid","command":"setup"}"#;
        let mut full = request.to_vec();
        full.push(b'\n');
        let resp = socket_roundtrip(&full);
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("auth_failed"));
    }

    #[test]
    fn test_socket_missing_token_field() {
        let request = b"{\"command\":\"setup\"}\n";
        let resp = socket_roundtrip(request);
        assert!(!resp.success);
        // Missing required field should cause JSON parse error
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn test_socket_unknown_command() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"hack-the-planet"}"#,
        );
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_command"));
    }

    #[test]
    fn test_socket_malformed_json() {
        let request = b"this is not json at all\n";
        let resp = socket_roundtrip(request);
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn test_socket_empty_request() {
        let request = b"\n";
        let resp = socket_roundtrip(request);
        assert!(!resp.success);
    }

    #[test]
    fn test_socket_unknown_fields_rejected() {
        let resp = socket_roundtrip_with_token(
            r#"{"token":"TOKEN_PLACEHOLDER","command":"setup","evil_field":"payload"}"#,
        );
        assert!(!resp.success);
        // deny_unknown_fields should reject this at JSON parse time
        assert_eq!(resp.error_kind.as_deref(), Some("invalid_request"));
    }

    #[test]
    fn test_socket_oversized_payload() {
        // Send 1.5 MB of 'a' characters without a newline
        let mut payload = vec![b'a'; 1_536_000];
        payload.push(b'\n');
        let (_token, lines) = socket_roundtrip_raw(&payload);

        assert!(
            !lines.is_empty() && !lines[0].trim().is_empty(),
            "oversized payload must receive a rejection response, not a dropped connection"
        );
        let resp: protocol::Response = serde_json::from_str(lines[0].trim()).unwrap();
        assert!(!resp.success);
        assert!(
            resp.error_kind.as_deref() == Some("request_too_large")
                || resp.error_kind.as_deref() == Some("invalid_request"),
            "unexpected error kind: {:?}",
            resp.error_kind
        );
    }

    #[test]
    fn test_socket_oversized_valid_json() {
        // Construct a >1MB valid JSON object with padding
        let padding = "a".repeat(1_200_000);
        let json = format!(r#"{{"token":"fake","command":"setup","padding":"{padding}"}}"#);
        let mut payload = json.into_bytes();
        payload.push(b'\n');
        let (_token, lines) = socket_roundtrip_raw(&payload);

        assert!(
            !lines.is_empty() && !lines[0].trim().is_empty(),
            "oversized JSON payload must receive a rejection response"
        );
        let resp: protocol::Response = serde_json::from_str(lines[0].trim()).unwrap();
        assert!(!resp.success);
        // The take adapter truncates the JSON, causing a parse error
        assert!(
            resp.error_kind.as_deref() == Some("request_too_large")
                || resp.error_kind.as_deref() == Some("invalid_request"),
            "unexpected error kind: {:?}",
            resp.error_kind
        );
    }
}

/// Concurrency tests (a)-(d) per Plan §Tests, plus audit-log assertions
/// per Security S1/S2 + Obs O1/O2/O3/O4.
///
/// These tests use a shared `Context` + multi-accept server thread to
/// exercise the real thread-per-connection + write-mutex + cancel paths.
/// They use `HelperCommand::TestSleep` (cfg-test) to avoid Kind cluster
/// dependency.
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use protocol::CommandResult;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Test harness: long-lived helper accepting connections on a unix socket
    /// using the same thread-per-connection model as production. Returns the
    /// socket path, auth token, audit-log path, shared context, and a
    /// shutdown handle.
    struct TestHelper {
        sock_path: PathBuf,
        token: String,
        log_path: PathBuf,
        shutdown: Arc<AtomicBool>,
        _dir: tempfile::TempDir,
        accept_thread: Option<std::thread::JoinHandle<()>>,
    }

    impl TestHelper {
        /// Send a JSON request, read all NDJSON response lines, return them.
        fn request(&self, json: &str) -> Vec<String> {
            let mut client = UnixStream::connect(&self.sock_path).unwrap();
            writeln!(client, "{json}").unwrap();
            client.shutdown(std::net::Shutdown::Write).unwrap();
            read_all_ndjson(&mut BufReader::new(&client))
        }

        /// Spawn a request on a background thread; return the join handle.
        fn spawn_request(&self, json: String) -> std::thread::JoinHandle<Vec<String>> {
            let sock_path = self.sock_path.clone();
            std::thread::spawn(move || {
                let mut client = UnixStream::connect(&sock_path).unwrap();
                writeln!(client, "{json}").unwrap();
                client.shutdown(std::net::Shutdown::Write).unwrap();
                read_all_ndjson(&mut BufReader::new(&client))
            })
        }

        fn read_audit_log(&self) -> Vec<serde_json::Value> {
            let contents = fs::read_to_string(&self.log_path).unwrap_or_default();
            contents
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                .collect()
        }
    }

    impl Drop for TestHelper {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(t) = self.accept_thread.take() {
                let _ = t.join();
            }
        }
    }

    /// Start a long-running write via the helper's internal API directly
    /// (TestSleep has no parse arm — by design). Returns a join handle that
    /// completes when the write finishes (cancel/error/normal).
    fn spawn_test_sleep_write(
        ctx: Arc<commands::Context>,
        seconds: u64,
    ) -> std::thread::JoinHandle<protocol::CommandResult> {
        std::thread::spawn(move || {
            let mut output = Vec::new();
            commands::execute(
                &protocol::HelperCommand::TestSleep { seconds },
                &ctx,
                &mut output,
            )
        })
    }

    /// Build a TestHelper that exposes its Context for in-process write
    /// dispatch (TestSleep bypasses the wire layer).
    struct TestHelperWithCtx {
        helper: TestHelper,
        ctx: Arc<commands::Context>,
    }

    impl TestHelperWithCtx {
        fn start() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let sock_path = dir.path().join("test.sock");
            let log_path = dir.path().join("helper.log");
            let token = auth::generate_token().unwrap();

            let listener = UnixListener::bind(&sock_path).unwrap();
            listener.set_nonblocking(true).unwrap();

            let ctx = Arc::new(build_test_context(dir.path()));
            let audit_log = Arc::new(logging::AuditLog::new(&log_path).unwrap());
            let token_arc = Arc::new(token.clone());
            let shutdown = Arc::new(AtomicBool::new(false));
            let conn_count = Arc::new(AtomicUsize::new(0));
            let last_reject_ms = Arc::new(AtomicI64::new(0));

            let shutdown_clone = Arc::clone(&shutdown);
            let ctx_for_thread = Arc::clone(&ctx);
            let accept_thread = std::thread::spawn(move || loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let current = conn_count.fetch_add(1, Ordering::SeqCst);
                        if current >= MAX_CONCURRENT_CONNECTIONS {
                            conn_count.fetch_sub(1, Ordering::SeqCst);
                            maybe_log_cap_reject(&audit_log, &last_reject_ms);
                            drop(stream);
                            continue;
                        }
                        let token_clone = Arc::clone(&token_arc);
                        let ctx_clone = Arc::clone(&ctx_for_thread);
                        let audit_clone = Arc::clone(&audit_log);
                        let conn_count_clone = Arc::clone(&conn_count);
                        std::thread::spawn(move || {
                            let _guard = ConnGuard {
                                counter: conn_count_clone,
                            };
                            let _ =
                                handle_connection(stream, &token_clone, &ctx_clone, &audit_clone);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            });

            let helper = TestHelper {
                sock_path,
                token,
                log_path,
                shutdown,
                _dir: dir,
                accept_thread: Some(accept_thread),
            };
            Self { helper, ctx }
        }

        fn status_json(&self) -> String {
            format!(r#"{{"token":"{}","command":"status"}}"#, self.helper.token)
        }

        fn cancel_json(&self) -> String {
            format!(r#"{{"token":"{}","command":"cancel"}}"#, self.helper.token)
        }

        /// Parse the LAST line of an NDJSON response stream as a CommandResult.
        /// Helpers in `#[cfg(test)]` are allowed to panic on assertion-style
        /// failures via `expect`; this helper turns a missing CommandResult
        /// line into a failed test.
        fn parse_result(lines: &[String]) -> CommandResult {
            lines
                .iter()
                .rev()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    serde_json::from_str::<CommandResult>(trimmed).ok()
                })
                .next()
                .unwrap_or_else(|| {
                    let dump = lines.join("");
                    eprintln!("test failure: no CommandResult line found in {dump}");
                    // Test-only failure path; allow panic in cfg(test) helper.
                    #[allow(clippy::panic)]
                    {
                        panic!("no CommandResult line found")
                    }
                })
        }

        /// Parse the first non-stream line as a Response with `error_kind`
        /// set (used for pre-execution errors like auth_failed, where the
        /// helper writes a single `Response` and closes).
        fn parse_response(lines: &[String]) -> protocol::Response {
            lines
                .iter()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    serde_json::from_str::<protocol::Response>(trimmed)
                        .ok()
                        .filter(|r| r.error_kind.is_some())
                })
                .next()
                .unwrap_or_else(|| {
                    let dump = lines.join("");
                    eprintln!("test failure: no Response line found in {dump}");
                    #[allow(clippy::panic)]
                    {
                        panic!("no Response line found")
                    }
                })
        }
    }

    // --- Test (a): concurrent reads during long-running write ---

    #[test]
    fn test_concurrent_status_during_long_write() {
        let h = TestHelperWithCtx::start();
        // Spawn a 10s TestSleep write directly (TestSleep has no parse arm).
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 10);

        // Wait for the write to claim the slot.
        let mut claimed = false;
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                claimed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(claimed, "write did not claim the slot in time");

        // Issue 5 parallel status requests via socket.
        let status_json = h.status_json();
        let start = Instant::now();
        let handles: Vec<_> = (0..5)
            .map(|_| h.helper.spawn_request(status_json.clone()))
            .collect();

        let results: Vec<Vec<String>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let total_elapsed = start.elapsed();

        // Each status returned in well under the spec's 5s ceiling. Tighter
        // bound: 2s for slow CI (per Test #8).
        assert!(
            total_elapsed < Duration::from_secs(2),
            "status calls took {:?}, expected <2s",
            total_elapsed
        );

        for lines in &results {
            let result = TestHelperWithCtx::parse_result(lines);
            let data = result.data.expect("status data missing");
            assert_eq!(data["busy"], serde_json::Value::Bool(true), "got: {data}");
            let in_flight = &data["in_flight"];
            assert_eq!(in_flight["op"], "test-sleep", "got: {data}");
            assert_eq!(
                in_flight["args"][0], "10",
                "in_flight.args mismatch: {data}"
            );
            // started_at parses as RFC3339.
            let started_at = in_flight["started_at"].as_str().unwrap();
            chrono::DateTime::parse_from_rfc3339(started_at).expect("started_at not RFC3339");
        }

        // Cancel the write to clean up quickly.
        let cancel = h.helper.request(&h.cancel_json());
        let _ = TestHelperWithCtx::parse_result(&cancel);
        let _ = write_handle.join().unwrap();
    }

    // --- Test (b): write-while-write returns busy ---

    #[test]
    fn test_write_while_write_returns_busy() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 5);

        // Wait for the write to claim the slot.
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());

        // Try a real wire write (rebuild ac).
        let rebuild_json = format!(
            r#"{{"token":"{}","command":"rebuild","service":"ac"}}"#,
            h.helper.token
        );
        let lines = h.helper.request(&rebuild_json);
        let result = TestHelperWithCtx::parse_result(&lines);
        assert_eq!(result.error_kind.as_deref(), Some("busy"));
        let data = result.data.expect("busy data missing");
        assert_eq!(data["op"], "test-sleep");
        assert_eq!(data["args"][0], "5");

        // Cleanup.
        let _ = h.helper.request(&h.cancel_json());
        let _ = write_handle.join().unwrap();
    }

    // --- Test (c): cancel mid-write terminates the handler ---

    #[test]
    fn test_cancel_mid_write_terminates_handler() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 30);

        // Wait for the write to claim the slot, then 200ms of "real work".
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        std::thread::sleep(Duration::from_millis(200));

        // Cancel returns immediately.
        let cancel_start = Instant::now();
        let cancel_lines = h.helper.request(&h.cancel_json());
        let cancel_elapsed = cancel_start.elapsed();
        let cancel_result = TestHelperWithCtx::parse_result(&cancel_lines);
        let cancel_data = cancel_result.data.expect("cancel data missing");
        assert_eq!(cancel_data["cancelled"], serde_json::Value::Bool(true));
        assert!(
            cancel_elapsed < Duration::from_secs(1),
            "cancel took too long: {cancel_elapsed:?}"
        );

        // Write resolves with cancelled within ~3s (SIGTERM-then-2s-SIGKILL slack).
        let write_start = Instant::now();
        let write_result = write_handle.join().unwrap();
        let write_elapsed = write_start.elapsed();
        assert!(
            write_elapsed < Duration::from_secs(3),
            "write didn't terminate within 3s: {write_elapsed:?}"
        );
        assert_eq!(write_result.error_kind.as_deref(), Some("cancelled"));
        assert!(
            write_result
                .error
                .as_deref()
                .unwrap_or("")
                .starts_with("cancelled"),
            "got: {:?}",
            write_result.error
        );
    }

    // --- Test (d): cancel when idle is no-op ---

    #[test]
    fn test_cancel_when_idle_is_noop() {
        let h = TestHelperWithCtx::start();
        let lines = h.helper.request(&h.cancel_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        assert_eq!(result.error_kind, None);
        assert_eq!(result.exit_code, Some(0));
        let data = result.data.expect("no-op data missing");
        assert_eq!(data["cancelled"], serde_json::Value::Bool(false));
        assert_eq!(data["reason"], "no-op");
    }

    // --- Additional tests: race coverage and audit-log assertions ---

    #[test]
    fn test_cancel_after_write_completes_is_noop() {
        let h = TestHelperWithCtx::start();
        // Run a 1s write to completion.
        let write_result = spawn_test_sleep_write(Arc::clone(&h.ctx), 1)
            .join()
            .unwrap();
        assert_eq!(write_result.exit_code, Some(0));
        // Slot is cleared by WriteSlotGuard::drop.
        assert!(h.ctx.snapshot_busy().is_none());
        // Cancel sees nothing in flight.
        let lines = h.helper.request(&h.cancel_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.expect("no-op data missing");
        assert_eq!(data["cancelled"], serde_json::Value::Bool(false));
        assert_eq!(data["reason"], "no-op");
    }

    #[test]
    fn test_repeated_cancel_when_idle() {
        let h = TestHelperWithCtx::start();
        for _ in 0..5 {
            let lines = h.helper.request(&h.cancel_json());
            let result = TestHelperWithCtx::parse_result(&lines);
            let data = result.data.expect("no-op data missing");
            assert_eq!(data["cancelled"], serde_json::Value::Bool(false));
        }
    }

    #[test]
    fn test_status_when_idle_busy_false() {
        let h = TestHelperWithCtx::start();
        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.expect("status data missing");
        assert_eq!(data["busy"], serde_json::Value::Bool(false), "got: {data}");
        // Per CR11: in_flight is literally null (not absent) when idle.
        assert_eq!(data["in_flight"], serde_json::Value::Null, "got: {data}");
        // setup_in_progress is sourced from the mutex; idle => false.
        assert_eq!(
            data["setup_in_progress"],
            serde_json::Value::Bool(false),
            "got: {data}"
        );
    }

    /// `setup_in_progress` must reflect the mutex view, not the legacy
    /// `setup.pid` heuristic. Asserts that a non-setup write in flight
    /// (TestSleep) leaves `setup_in_progress=false` while `busy=true`.
    #[test]
    fn test_status_setup_in_progress_only_for_setup_op() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 5);
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());

        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.expect("status data missing");
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "got: {data}");
        assert_eq!(
            data["setup_in_progress"],
            serde_json::Value::Bool(false),
            "non-setup write in flight should not show setup_in_progress=true: {data}",
        );

        // Cancel and reap.
        let _ = h.helper.request(&h.cancel_json());
        let _ = write_handle.join();
    }

    #[test]
    fn test_busy_rejection_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 5);
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());

        let rebuild_json = format!(
            r#"{{"token":"{}","command":"rebuild","service":"ac"}}"#,
            h.helper.token
        );
        let _ = h.helper.request(&rebuild_json);

        // Give the audit log a moment to flush.
        std::thread::sleep(Duration::from_millis(100));
        let entries = h.helper.read_audit_log();
        let busy_entries: Vec<_> = entries
            .iter()
            .filter(|e| e["cmd"] == "rejected_busy")
            .collect();
        assert!(
            !busy_entries.is_empty(),
            "no rejected_busy entry in {entries:#?}"
        );
        let entry = busy_entries[0];
        assert_eq!(entry["outcome"], "rejected");
        assert!(entry.get("error").map(|v| v.is_null()).unwrap_or(true));
        // Args should contain all four collision-pair keyed strings.
        let args: Vec<&str> = entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            args.iter().any(|a| a.starts_with("rejected=rebuild")),
            "got: {args:?}"
        );
        assert!(args.contains(&"rejected_args=ac"), "got: {args:?}");
        assert!(
            args.iter().any(|a| a.starts_with("in_flight=test-sleep")),
            "got: {args:?}"
        );
        assert!(args.contains(&"in_flight_args=5"), "got: {args:?}");

        let _ = h.helper.request(&h.cancel_json());
        let _ = write_handle.join().unwrap();
    }

    #[test]
    fn test_cancel_completed_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 30);
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = h.helper.request(&h.cancel_json());
        std::thread::sleep(Duration::from_millis(100));
        let entries = h.helper.read_audit_log();
        let cancel_entry = entries
            .iter()
            .find(|e| e["cmd"] == "cancel")
            .expect("no cancel entry");
        assert_eq!(cancel_entry["outcome"], "completed");
        assert_eq!(cancel_entry["error"], "test-sleep");
        let args: Vec<&str> = cancel_entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            args.iter().any(|a| a.starts_with("target=test-sleep")),
            "got: {args:?}"
        );
        let _ = write_handle.join().unwrap();
    }

    #[test]
    fn test_cancel_noop_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let _ = h.helper.request(&h.cancel_json());
        std::thread::sleep(Duration::from_millis(100));
        let entries = h.helper.read_audit_log();
        let cancel_entry = entries
            .iter()
            .find(|e| e["cmd"] == "cancel")
            .expect("no cancel entry");
        assert_eq!(cancel_entry["outcome"], "no-op");
        assert_eq!(cancel_entry["error"], "none");
    }

    /// Obs O4: status's in-flight view and the audit log's record agree.
    /// `cancel`'s audit entry includes `target_args=<args>` derived from the
    /// in-flight op — that's the same source as status's `in_flight.args`,
    /// so they MUST match. (test-sleep itself is invoked in-process via
    /// `commands::execute`, bypassing handle_connection's audit log; the
    /// cancel entry below is the cross-source pin.)
    #[test]
    fn test_status_audit_drift_invariant() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 10);
        for _ in 0..50 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Read status to capture in_flight.args.
        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.expect("status data");
        let status_args: Vec<String> = data["in_flight"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        // Cancel writes an audit entry whose `args` carries `target_args=<args>`
        // — that string IS the join of the in-flight op's args.
        let _ = h.helper.request(&h.cancel_json());
        let _ = write_handle.join().unwrap();
        std::thread::sleep(Duration::from_millis(100));

        let entries = h.helper.read_audit_log();
        let cancel_entry = entries
            .iter()
            .find(|e| e["cmd"] == "cancel" && e["outcome"] == "completed")
            .expect("no completed cancel entry");
        let cancel_args: Vec<&str> = cancel_entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let target_args = cancel_args
            .iter()
            .find(|s| s.starts_with("target_args="))
            .expect("no target_args in cancel entry")
            .strip_prefix("target_args=")
            .unwrap();
        // Status reported `["10"]`; audit's `target_args=` is the joined string.
        assert_eq!(
            target_args,
            status_args.join(" "),
            "status's in_flight.args does not match cancel audit's target_args"
        );
    }

    #[test]
    fn test_cancel_requires_auth_token() {
        let h = TestHelperWithCtx::start();
        let bad = r#"{"token":"wrong_token_definitely_not_valid","command":"cancel"}"#;
        let lines = h.helper.request(bad);
        let resp = TestHelperWithCtx::parse_response(&lines);
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("auth_failed"));
    }

    /// Security S4: connection cap rejects the 33rd concurrent connection,
    /// audit log records exactly one `cmd:"rejected"` /
    /// `error:"connection_cap_reached"` / `outcome:"rejected"` entry.
    #[test]
    fn test_connection_cap_rejects_at_32() {
        let h = TestHelperWithCtx::start();

        // Hold MAX_CONCURRENT_CONNECTIONS streams open without writing a
        // newline. handle_connection's read_line blocks until EOF/newline
        // arrives, so each accepted connection consumes a slot until we drop
        // the client end at test cleanup.
        let mut holders: Vec<UnixStream> = Vec::with_capacity(MAX_CONCURRENT_CONNECTIONS);
        for _ in 0..MAX_CONCURRENT_CONNECTIONS {
            let client = UnixStream::connect(&h.helper.sock_path).unwrap();
            holders.push(client);
            // Short sleep lets the accept loop pick up the connection and
            // increment the counter before we try the next one.
            std::thread::sleep(Duration::from_millis(5));
        }
        // Extra slack so all 32 are definitely accepted server-side.
        std::thread::sleep(Duration::from_millis(100));

        // The 33rd connection should be accepted then immediately closed by
        // the server (drop(stream) after fetch_sub). Reading from it returns
        // EOF (0 bytes) without any response payload.
        let rejected = UnixStream::connect(&h.helper.sock_path).unwrap();
        rejected
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut buf = [0u8; 64];
        let n = (&rejected).read(&mut buf).unwrap_or(usize::MAX);
        assert_eq!(
            n,
            0,
            "33rd connection should see EOF, got {n} bytes: {:?}",
            &buf[..n.min(64)]
        );

        // Audit log: one cap-rejection entry. The dedup window is 1s, but
        // there's only one over-cap connection in this test so dedup doesn't
        // matter.
        std::thread::sleep(Duration::from_millis(100));
        let entries = h.helper.read_audit_log();
        let cap_rejections: Vec<_> = entries
            .iter()
            .filter(|e| e["cmd"] == "rejected" && e["error"] == "connection_cap_reached")
            .collect();
        assert_eq!(
            cap_rejections.len(),
            1,
            "expected exactly one cap-rejection entry, got: {entries:#?}"
        );
        assert_eq!(cap_rejections[0]["outcome"], "rejected");

        // Drop holders so handler threads exit and slots are released before
        // teardown.
        drop(holders);
    }

    /// Security S5 + Obs O3: a child that ignores SIGTERM forces the
    /// 2s-grace-then-SIGKILL escalation path. The error string carries the
    /// `(sigterm timeout, escalated to sigkill)` suffix from
    /// `HelperError::Cancelled { escalated: true }`.
    #[test]
    fn test_sigkill_escalation_logged() {
        let h = TestHelperWithCtx::start();

        // Sleep duration must comfortably exceed CANCEL_GRACEFUL_TIMEOUT (2s)
        // so the child is still alive when the grace window expires.
        let write_handle = std::thread::spawn({
            let ctx = Arc::clone(&h.ctx);
            move || {
                let mut output = Vec::new();
                commands::execute(
                    &protocol::HelperCommand::TestSleepIgnoringTerm { seconds: 30 },
                    &ctx,
                    &mut output,
                )
            }
        });

        // Wait for the write to claim the slot.
        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        // Let the shell install the trap before we cancel.
        std::thread::sleep(Duration::from_millis(200));

        // Cancel — SIGTERM is delivered but ignored; after 2s the streaming
        // loop escalates to SIGKILL.
        let cancel_lines = h.helper.request(&h.cancel_json());
        let cancel_result = TestHelperWithCtx::parse_result(&cancel_lines);
        assert_eq!(
            cancel_result
                .data
                .as_ref()
                .and_then(|d| d["cancelled"].as_bool()),
            Some(true),
            "cancel did not report cancelled=true: {cancel_result:?}"
        );

        // Write resolves with escalated cancellation. Must complete within
        // grace + slack (≈3s).
        let start = Instant::now();
        let write_result = write_handle.join().unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "escalation path took too long: {elapsed:?}"
        );
        assert_eq!(write_result.error_kind.as_deref(), Some("cancelled"));
        let err_str = write_result
            .error
            .as_deref()
            .expect("cancelled write must carry an error string");
        assert!(
            err_str.starts_with("cancelled (sigterm timeout, escalated to sigkill)"),
            "expected SIGKILL-escalated suffix, got: {err_str:?}"
        );
    }

    // --- Iter-2: process-group cancel reaches grandchildren ---

    /// Iter-2 headline test: a `bash -c 'sleep 30 & wait'` write spawns a
    /// grandchild `sleep` that inherits stdout/stderr. Without
    /// `Command::process_group(0)` + `kill(-pgid, SIGTERM)`, killing only
    /// `bash` would leave `sleep` holding the pipes for ~30s. The pgid
    /// signal reaches the grandchild and cancel completes promptly.
    #[test]
    fn test_cancel_kills_grandchild_holding_pipes() {
        let h = TestHelperWithCtx::start();

        let write_handle = std::thread::spawn({
            let ctx = Arc::clone(&h.ctx);
            move || {
                let mut output = Vec::new();
                commands::execute(
                    &protocol::HelperCommand::TestSleepWithChild { seconds: 30 },
                    &ctx,
                    &mut output,
                )
            }
        });

        // Wait for the write to claim the slot + bash to fork the sleep.
        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        // Let bash fork the `sleep` grandchild before we cancel.
        std::thread::sleep(Duration::from_millis(200));

        let cancel_start = Instant::now();
        let cancel_lines = h.helper.request(&h.cancel_json());
        let cancel_result = TestHelperWithCtx::parse_result(&cancel_lines);
        assert_eq!(
            cancel_result
                .data
                .as_ref()
                .and_then(|d| d["cancelled"].as_bool()),
            Some(true),
            "cancel did not report cancelled=true: {cancel_result:?}"
        );

        // Write resolves with cancellation. Without pgid signalling this
        // would block ~30s (waiting for the orphaned `sleep` grandchild
        // to release the pipes). With pgid the bash + sleep both die on
        // SIGTERM and the read loop sees EOF promptly.
        let write_result = write_handle.join().unwrap();
        let elapsed = cancel_start.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "process-group cancel took too long (regression: grandchild orphaned?): {elapsed:?}"
        );
        assert_eq!(
            write_result.error_kind.as_deref(),
            Some("cancelled"),
            "expected cancelled error_kind, got: {write_result:?}"
        );
        // Obs O3 prefix invariant — clean-SIGTERM error must start with
        // "cancelled". This is the same string handle_connection copies into
        // the audit-log entry's `error` field (see main.rs `outcome` mapping).
        let err_str = write_result
            .error
            .as_deref()
            .expect("cancelled write must carry an error string");
        assert!(
            err_str.starts_with("cancelled"),
            "Obs O3 prefix invariant: clean-SIGTERM error must start with `cancelled`, got: {err_str:?}"
        );
        // Pin the absence of the SIGKILL-escalation suffix — clean PG-SIGTERM
        // should NOT escalate. If a future regression breaks the SIGTERM-pgid
        // path (e.g., setpgid not configured correctly on the child), the
        // SIGKILL-pgid escalation would silently take over and the timing
        // assertion alone would still pass. This negative assertion forces
        // the SIGTERM path to remain load-bearing.
        assert!(
            !err_str.starts_with("cancelled (sigterm timeout, escalated to sigkill)"),
            "PG-SIGTERM should kill bash+sleep promptly without SIGKILL escalation, got: {err_str:?}"
        );
    }

    /// Iter-2: TERM-trapped bash with a backgrounded `sleep` grandchild
    /// forces both the SIGKILL escalation branch AND the grandchild reach
    /// via process-group SIGKILL. Cancel must still complete within ~grace
    /// + slack (~7s upper bound).
    #[test]
    fn test_cancel_kills_grandchild_with_sigkill_escalation() {
        let h = TestHelperWithCtx::start();

        let write_handle = std::thread::spawn({
            let ctx = Arc::clone(&h.ctx);
            move || {
                let mut output = Vec::new();
                commands::execute(
                    &protocol::HelperCommand::TestSleepWithChildIgnoringTerm { seconds: 30 },
                    &ctx,
                    &mut output,
                )
            }
        });

        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        // Let the shell install the trap and fork the `sleep`.
        std::thread::sleep(Duration::from_millis(200));

        let cancel_start = Instant::now();
        let cancel_lines = h.helper.request(&h.cancel_json());
        let cancel_result = TestHelperWithCtx::parse_result(&cancel_lines);
        assert_eq!(
            cancel_result
                .data
                .as_ref()
                .and_then(|d| d["cancelled"].as_bool()),
            Some(true),
            "cancel did not report cancelled=true: {cancel_result:?}"
        );

        let write_result = write_handle.join().unwrap();
        let elapsed = cancel_start.elapsed();
        assert!(
            elapsed < Duration::from_secs(7),
            "SIGKILL-pgid path took too long (regression): {elapsed:?}"
        );
        assert_eq!(write_result.error_kind.as_deref(), Some("cancelled"));
        let err_str = write_result
            .error
            .as_deref()
            .expect("cancelled write must carry an error string");
        assert!(
            err_str.starts_with("cancelled (sigterm timeout, escalated to sigkill)"),
            "expected SIGKILL-escalated suffix, got: {err_str:?}"
        );
    }

    // --- Iter-2: cancel_pending field on status response ---

    /// Iter-2: while a cancel is in flight (between `cmd_cancel` storing
    /// `true` into the cancel token and the writer thread releasing the
    /// slot), `status` returns `cancel_pending: true`. Once the slot is
    /// released, `cancel_pending` returns to false (and `busy` to false,
    /// `in_flight` to null).
    ///
    /// Uses `TestSleepIgnoringTerm` (5s busy-loop) to widen the SIGTERM-
    /// grace + drain observation window — the writer enters the 2s grace
    /// after SIGTERM, then escalates to SIGKILL. Status during that 2s
    /// window observes `cancel_pending=true`.
    #[test]
    fn test_cancel_pending_visible_in_status() {
        let h = TestHelperWithCtx::start();

        let write_handle = std::thread::spawn({
            let ctx = Arc::clone(&h.ctx);
            move || {
                let mut output = Vec::new();
                commands::execute(
                    &protocol::HelperCommand::TestSleepIgnoringTerm { seconds: 5 },
                    &ctx,
                    &mut output,
                )
            }
        });

        // Wait for the write to claim the slot.
        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        // Let the shell install the TERM trap.
        std::thread::sleep(Duration::from_millis(150));

        // Status before cancel: busy=true, cancel_pending=false.
        {
            let lines = h.helper.request(&h.status_json());
            let result = TestHelperWithCtx::parse_result(&lines);
            let data = result.data.as_ref().expect("status data");
            assert_eq!(data["busy"], serde_json::Value::Bool(true));
            assert_eq!(data["cancel_pending"], serde_json::Value::Bool(false));
        }

        // Send cancel (returns immediately).
        let cancel_lines = h.helper.request(&h.cancel_json());
        let cancel_result = TestHelperWithCtx::parse_result(&cancel_lines);
        assert_eq!(
            cancel_result
                .data
                .as_ref()
                .and_then(|d| d["cancelled"].as_bool()),
            Some(true),
        );

        // Status during the SIGTERM-grace window: cancel_pending=true.
        // The trap-ignore + 2s grace gives a wide observation window.
        {
            let lines = h.helper.request(&h.status_json());
            let result = TestHelperWithCtx::parse_result(&lines);
            let data = result.data.as_ref().expect("status data");
            assert_eq!(data["busy"], serde_json::Value::Bool(true));
            assert_eq!(
                data["cancel_pending"],
                serde_json::Value::Bool(true),
                "expected cancel_pending=true during grace window, got: {data}"
            );
        }

        // Wait for the write to terminate.
        let write_result = write_handle.join().unwrap();
        assert_eq!(write_result.error_kind.as_deref(), Some("cancelled"));

        // Status after release: busy=false, cancel_pending=false, in_flight=null.
        {
            let lines = h.helper.request(&h.status_json());
            let result = TestHelperWithCtx::parse_result(&lines);
            let data = result.data.as_ref().expect("status data");
            assert_eq!(data["busy"], serde_json::Value::Bool(false));
            assert_eq!(data["cancel_pending"], serde_json::Value::Bool(false));
            assert_eq!(data["in_flight"], serde_json::Value::Null);
        }
    }

    /// Iter-2: when busy but no cancel has been issued, status reports
    /// `cancel_pending=false`. Confirms cancel_pending is sourced from the
    /// in-flight op's cancel_token, not derived from the busy flag alone.
    #[test]
    fn test_cancel_pending_false_when_busy_not_cancelling() {
        let h = TestHelperWithCtx::start();
        let write_handle = spawn_test_sleep_write(Arc::clone(&h.ctx), 3);

        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());

        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.as_ref().expect("status data");
        assert_eq!(data["busy"], serde_json::Value::Bool(true));
        assert_eq!(data["cancel_pending"], serde_json::Value::Bool(false));

        // Let the write complete naturally.
        let _ = write_handle.join().unwrap();
    }

    /// Iter-2: idle status emits `cancel_pending=false` (CR11 wire-determinism).
    #[test]
    fn test_cancel_pending_false_when_idle() {
        let h = TestHelperWithCtx::start();
        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.as_ref().expect("status data");
        assert_eq!(data["busy"], serde_json::Value::Bool(false));
        assert_eq!(data["cancel_pending"], serde_json::Value::Bool(false));
        // Per CR11: cancel_pending is literally present (not absent) when idle.
        assert!(
            data.get("cancel_pending").is_some(),
            "cancel_pending must be literally present (not absent) per CR11 wire-determinism"
        );
    }

    /// Iter-2 (Obs O4 cancel-path extension): status's pending view, the
    /// cancel-command audit entry, and the writer's returned `CommandResult`
    /// MUST agree across the cancel path. Specifically:
    ///   (i)   status's `in_flight.{op,args}` during the SIGTERM-grace window
    ///         matches the cancel audit entry's `target_args=<args>` token.
    ///   (ii)  writer returns `error_kind == "cancelled"` and the eventual
    ///         outcome translates to `OUTCOME_CANCELLED` per the
    ///         handle_connection mapping at main.rs:402-417.
    ///   (iii) writer's error string satisfies the Obs O3 prefix-`cancelled`
    ///         invariant — both clean SIGTERM and SIGKILL-escalated produce
    ///         a message starting with "cancelled". This stub forces the
    ///         SIGKILL path (TERM-trapped shell) so the
    ///         "(sigterm timeout, escalated to sigkill)" suffix is also
    ///         covered against the prefix invariant.
    ///
    /// Note: the writer is invoked via `commands::execute()` directly because
    /// `TestSleepIgnoringTerm` has no parse arm (release-only invariant).
    /// The audit-log entry that handle_connection would emit for the writer
    /// is therefore not present in this test's helper.log; (ii) and (iii)
    /// are asserted against the writer's `CommandResult`, which is the
    /// SAME source `handle_connection` reads to populate its audit entry
    /// (main.rs:402-417). The agreement enforced here transitively pins
    /// the audit-entry shape.
    #[test]
    fn test_status_audit_drift_invariant_cancel_path() {
        let h = TestHelperWithCtx::start();
        let write_handle = std::thread::spawn({
            let ctx = Arc::clone(&h.ctx);
            move || {
                let mut output = Vec::new();
                commands::execute(
                    &protocol::HelperCommand::TestSleepIgnoringTerm { seconds: 5 },
                    &ctx,
                    &mut output,
                )
            }
        });

        // Wait for the slot claim.
        for _ in 0..100 {
            if h.ctx.snapshot_busy().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(h.ctx.snapshot_busy().is_some());
        // Let the shell install the trap.
        std::thread::sleep(Duration::from_millis(150));

        // Cancel; immediately capture status's pending view during the
        // SIGTERM-grace window.
        let _ = h.helper.request(&h.cancel_json());
        let lines = h.helper.request(&h.status_json());
        let result = TestHelperWithCtx::parse_result(&lines);
        let data = result.data.expect("status data");
        assert_eq!(data["busy"], serde_json::Value::Bool(true));
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(true),
            "cancel_pending should be true during SIGTERM grace, got: {data}"
        );
        let status_args: Vec<String> = data["in_flight"]["args"]
            .as_array()
            .expect("in_flight.args present during pending cancel")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let status_op = data["in_flight"]["op"]
            .as_str()
            .expect("in_flight.op present during pending cancel")
            .to_string();

        // Wait for the writer to terminate (SIGKILL escalation).
        let write_result = write_handle.join().unwrap();
        std::thread::sleep(Duration::from_millis(100));

        // (i) args agreement — status's in_flight.args == cancel audit's
        // target_args. The cancel command goes over the socket, so its
        // audit entry IS recorded by handle_connection.
        let entries = h.helper.read_audit_log();
        let cancel_entry = entries
            .iter()
            .find(|e| e["cmd"] == "cancel" && e["outcome"] == "completed")
            .expect("no completed cancel entry in audit log");
        let cancel_target = cancel_entry["error"]
            .as_str()
            .expect("cancel entry's error carries cancelled-op identifier");
        assert_eq!(
            cancel_target, status_op,
            "cancel audit entry's `error` (cancelled-op identifier) must match status's in_flight.op"
        );
        let cancel_args: Vec<&str> = cancel_entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let target_args = cancel_args
            .iter()
            .find(|s| s.starts_with("target_args="))
            .expect("no target_args in cancel entry")
            .strip_prefix("target_args=")
            .unwrap();
        assert_eq!(
            target_args,
            status_args.join(" "),
            "status's in_flight.args ({status_args:?}) does not match cancel audit's target_args ({target_args:?})"
        );

        // (ii) writer returns `error_kind == "cancelled"`. handle_connection
        // (main.rs:402-417) maps this to OUTCOME_CANCELLED in the audit
        // entry it would emit; the in-process invocation here can't trip
        // that path (no parse arm), so we assert the source-of-truth
        // CommandResult instead.
        assert_eq!(
            write_result.error_kind.as_deref(),
            Some("cancelled"),
            "writer must return error_kind=cancelled, got: {write_result:?}"
        );

        // (iii) Obs O3 prefix invariant — writer's error string starts with
        // "cancelled". This is the string `handle_connection` would copy
        // verbatim into the audit entry's `error` field at main.rs:411-418.
        let writer_error = write_result
            .error
            .as_deref()
            .expect("cancelled writer must carry an error string");
        assert!(
            writer_error.starts_with("cancelled"),
            "writer error must start with `cancelled` (Obs O3 prefix invariant), got: {writer_error:?}"
        );
        // This stub forces the SIGKILL escalation path; confirm the
        // suffix-distinguished message also satisfies the prefix invariant.
        // Clean-SIGTERM coverage lives in test_cancel_kills_grandchild_holding_pipes
        // (which also returns error_kind=cancelled with prefix-`cancelled`
        // — message is "cancelled by client request" without the suffix).
        assert!(
            writer_error.starts_with("cancelled (sigterm timeout, escalated to sigkill)"),
            "expected SIGKILL-escalated suffix in writer error, got: {writer_error:?}"
        );
    }
}
