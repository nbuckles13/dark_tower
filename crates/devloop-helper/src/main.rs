//! Devloop helper binary — host-side cluster manager per ADR-0030.
//!
//! Listens on a unix socket and handles build/deploy commands for a Kind cluster.
//! Each devloop gets its own helper process managing a dedicated cluster.
//!
//! Usage: devloop-helper <slug> [--project-root <path>]

mod auth;
mod commands;
mod error;
mod fs_atomic;
mod logging;
mod ports;
mod protocol;

use error::{HelperError, ValidSlug, CLUSTER_PREFIX};
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
    // Cluster name shares the `CLUSTER_PREFIX` const with error.rs's slug-cap
    // derivation (Finding 4): the prefix the cap subtracts is the same one that
    // builds the name, so a rename can't silently invalidate the bound. (The
    // runtime-dir path below uses the literal `devloop-` deliberately — it is the
    // /tmp/devloop-<slug> convention shared with devloop.sh, not a cluster name.)
    let cluster_name = format!("{CLUSTER_PREFIX}{slug}");

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
        recreate_bound: AtomicBool::new(false),
        cancel_grace: commands::CANCEL_GRACEFUL_TIMEOUT,
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

    // Log startup (base_port is not yet known — allocation happens in cmd_provision)
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

    // Validate auth token (every command including cancel — per CR9).
    //
    // GUARD-MOTIVATED FORM — written as `match`, not the shorter `if let Err(e) = …`, on
    // purpose; do NOT "simplify" it back. The `if let Err(e) = auth::validate_token(&request.token,
    // expected_token) {` form is a no-secrets-in-logs Check-5 false positive: Check-5 reads the
    // `Err(` of a destructuring PATTERN as if it were an error CONSTRUCTOR, and the secret-NAMED
    // identifiers (request.token / expected_token) on that line are argument names, not logged
    // values. The `match …(…) {` line carries no `Err(`, so it doesn't trip; a REAL future leak
    // inside the Err arm (e.g. `Err(anyhow!("… {} …", request.token))`) lands on its own scanned
    // line and DOES trip — so this form loses no coverage.
    //
    // The token value is NEVER logged regardless: auth::validate_token returns unit-variant
    // HelperError::AuthFailed, whose Display is the static "auth_failed" (error.rs:87), so the
    // e.to_string() below cannot carry a token.
    //
    // This auth line is PRE-EXISTING and is in Check-5 scope only INCIDENTALLY: the guard scans
    // changed files (rust_log_secrets.rs:262, get_all_changed_files), and main.rs is in the
    // changed set only because workstream #1's CLUSTER_PREFIX edit at :45 pulled it in — nothing
    // to do with auth. It has never been scanned before.
    //
    // This form is NOT a safety control: reverting it to `if let` would re-trigger the false
    // positive, NOT create a vulnerability. Do not treat it as a security pattern or cargo-cult
    // it elsewhere. The proper fix (make Check-5 distinguish constructor from destructuring) is
    // tracked in docs/TODO.md § "Guard Coverage Gap — Check-5 (SECRET_IN_ERROR_MSG) cannot
    // distinguish Err( as an error CONSTRUCTOR from Err( as a destructuring PATTERN"; when it
    // lands, this can go back to `if let`.
    //
    // If this is ever inlined back to `if let`, Check-5 fires and Gate 2 reds. That is the
    // INTENDED behaviour, not a regression to suppress — it is what makes this form safe to
    // change without a guard protecting it. Fix Check-5 (see the TODO above), don't add a waiver.
    match auth::validate_token(&request.token, expected_token) {
        Ok(()) => {}
        Err(e) => {
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
    // - Cancelled writes → keep cmd_name (e.g. "deploy"), error prefix "cancelled".
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
        audit_log.log_command_with_decisions(
            &cmd_name,
            &cmd_args,
            result.duration_ms,
            exit_code,
            error_str,
            outcome,
            &result.decisions,
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
///
/// not secret-bearing (an empty flock file), so this keeps an ordinary write —
/// the atomic-secret-write helper is for credential files only. It also
/// deliberately does NOT truncate (`truncate(false)`); the file is only ever
/// flock'd, never read.
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
///
/// not secret-bearing (a PID is not credential material), so this keeps an
/// ordinary write — the atomic-secret-write helper (`fs_atomic::atomic_write_secret`)
/// is for credential files (kubeconfig, auth token) only.
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

    // Slug length-boundary tests live with `ValidSlug` + the derived `SLUG_MAX`
    // constant in `error.rs` (`test_valid_slug_accepts_max_length`,
    // `test_valid_slug_rejects_over_max_length`, `test_slug_bounds_are_derived_from_63`) —
    // one home for the #1 name-length invariant. The charset tests above stay here.

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
        let ctx = commands::test_context(dir.path());
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
        let ctx = commands::test_context(dir.path());
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

    /// O2 (ADR-0038 step 3): a provision/deploy write's decision lines reach the
    /// PERSISTENT audit log (helper.log) — and nothing else the script printed.
    #[test]
    fn test_write_decision_lines_reach_the_audit_log_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        let token = auth::generate_token().unwrap();
        let mut ctx = commands::test_context(dir.path());
        let root = dir.path().join("root");
        let script = root.join("infra/kind/scripts/deploy.sh");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        // Installed by a child process (never an fd this test holds), so a
        // parallel fork cannot make the exec fail with ETXTBSY.
        let src = dir.path().join("deploy.src");
        fs::write(
            &src,
            "#!/bin/sh\n\
             echo 'building images (secret=hunter2)'\n\
             echo 'BLUEPRINT ACTION=check REASON=match RECORDED=aaaaaaaaaaaa CURRENT=aaaaaaaaaaaa CHANGED=-'\n\
             echo 'DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/deployment/mc-0' >&2\n\
             exit 1\n",
        )
        .unwrap();
        assert!(std::process::Command::new("install")
            .arg("-m")
            .arg("0755")
            .arg(&src)
            .arg(&script)
            .status()
            .unwrap()
            .success());
        ctx.project_root = root;
        let log_path = dir.path().join("helper.log");
        let audit_log = logging::AuditLog::new(&log_path).unwrap();
        let token_clone = token.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_nonblocking(false).unwrap();
            let _ = handle_connection(stream, &token_clone, &ctx, &audit_log);
        });
        let mut client = UnixStream::connect(&sock_path).unwrap();
        writeln!(client, r#"{{"token":"{token}","command":"deploy"}}"#).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let _ = read_all_ndjson(&mut BufReader::new(&client));
        handle.join().unwrap();

        let log = fs::read_to_string(&log_path).unwrap();
        let entry: serde_json::Value = log
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|e| e["cmd"] == "deploy")
            .expect("a deploy audit entry");
        assert_eq!(
            entry["deploy_failed"],
            "DEPLOY_FAILED REASON=rollout-failed WORKLOADS=dark-tower/deployment/mc-0"
        );
        assert_eq!(
            entry["blueprint"],
            "BLUEPRINT ACTION=check REASON=match RECORDED=aaaaaaaaaaaa CURRENT=aaaaaaaaaaaa CHANGED=-"
        );
        // Negative: arbitrary script output never reaches the audit log.
        assert!(!log.contains("hunter2"), "{log}");
        assert!(!log.contains("building images"), "{log}");
    }

    /// The retired `service` argument field is rejected at deserialization —
    /// before auth-gated parse, before any exec — whatever it carries.
    #[test]
    fn test_socket_service_field_rejected_whatever_its_value() {
        for value in [
            "; rm -rf /",
            "$(whoami)",
            "`id`",
            "ac | cat /etc/passwd",
            "gc",
        ] {
            let json = format!(
                r#"{{"token":"TOKEN_PLACEHOLDER","command":"deploy","service":{}}}"#,
                serde_json::Value::String(value.to_string())
            );
            let resp = socket_roundtrip_with_token(&json);
            assert!(!resp.success, "{value}");
            assert_eq!(
                resp.error_kind.as_deref(),
                Some("invalid_request"),
                "{value}"
            );
        }
    }

    /// Injection shapes in the ONLY string the wire carries besides the token.
    #[test]
    fn test_socket_command_injection_shapes_rejected() {
        for command in [
            "deploy mc",
            "provision ",
            "deploy; rm -rf /",
            "$(whoami)",
            "`id`",
            "provision | cat /etc/passwd",
            "rebuild-all",
            "setup",
        ] {
            let json = format!(
                r#"{{"token":"TOKEN_PLACEHOLDER","command":{}}}"#,
                serde_json::Value::String(command.to_string())
            );
            let resp = socket_roundtrip_with_token(&json);
            assert!(!resp.success, "{command:?}");
            assert_eq!(
                resp.error_kind.as_deref(),
                Some("invalid_command"),
                "{command:?}"
            );
        }
    }

    #[test]
    fn test_socket_null_bytes_in_command() {
        // Null byte in the middle of a command
        let mut request = br#"{"token":"fake","command":"provision"#.to_vec();
        request.push(0x00);
        request.extend_from_slice(br#"extra"}"#);
        request.push(b'\n');
        let resp = socket_roundtrip(&request);
        assert!(!resp.success);
    }

    #[test]
    fn test_socket_newline_in_command_field() {
        let resp = socket_roundtrip_with_token(
            "{\"token\":\"TOKEN_PLACEHOLDER\",\"command\":\"deploy\\nteardown\"}",
        );
        assert!(!resp.success);
    }

    #[test]
    fn test_socket_invalid_auth_token() {
        let request = br#"{"token":"wrong_token_definitely_not_valid","command":"provision"}"#;
        let mut full = request.to_vec();
        full.push(b'\n');
        let resp = socket_roundtrip(&full);
        assert!(!resp.success);
        assert_eq!(resp.error_kind.as_deref(), Some("auth_failed"));
    }

    #[test]
    fn test_socket_missing_token_field() {
        let request = b"{\"command\":\"provision\"}\n";
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
            r#"{"token":"TOKEN_PLACEHOLDER","command":"provision","evil_field":"payload"}"#,
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
        let json = format!(r#"{{"token":"fake","command":"provision","padding":"{padding}"}}"#);
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
/// exercise the real thread-per-connection + write-mutex + cancel paths,
/// with `HelperCommand::TestHold` (cfg-test) as the write.
///
/// THE RULE OF THIS MODULE: no test synchronizes on a duration. A held write
/// ends only when the test cancels or releases it (never on a clock — so
/// `status`'s real `kind`/`kubectl`/inspect latency, or a loaded runner, can't
/// end it early); every "has X happened yet?" is an event waited on through
/// `poll_until`/`HeldWrite::join`, bounded only by the fail-loud
/// `WAIT_DEADLINE` hang detector. No elapsed-time assertion is made.
#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use protocol::{CommandResult, HelperCommand, HoldShape};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    use commands::{poll_until, WAIT_DEADLINE};

    /// Cancel grace that never expires (`run_command_streaming` only compares
    /// `elapsed() >= grace`; nothing adds to it, so `MAX` cannot overflow).
    /// For tests that must observe a cancelled write still holding the slot
    /// across `status` calls — whose kind/kubectl latency is uncontrolled, so
    /// any FINITE grace would be a duration racing it — or that must prove the
    /// SIGTERM path ALONE ends a write (no SIGKILL can rescue a broken SIGTERM
    /// path; it fails at the `join` deadline instead). Set only through
    /// `commands::test_context_with_grace`.
    const NEVER_ESCALATE: Duration = Duration::MAX;
    // SIGKILL-escalation tests run at the production grace and must see the
    // escalation land inside one wait.
    const _: () = assert!(WAIT_DEADLINE.as_secs() > commands::CANCEL_GRACEFUL_TIMEOUT.as_secs());

    /// Run `cmd` as a write on its own thread, the way `handle_connection`
    /// would (in-process: `TestHold` has no parse arm). The result arrives on
    /// the returned channel.
    fn spawn_write(
        ctx: &Arc<commands::Context>,
        cmd: HelperCommand,
    ) -> mpsc::Receiver<CommandResult> {
        let (tx, rx) = mpsc::channel();
        let ctx = Arc::clone(ctx);
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let _ = tx.send(commands::execute(&cmd, &ctx, &mut output));
        });
        rx
    }

    /// A `TestHold` write in flight. `spawn` returns once the write holds the
    /// slot AND its stub is set up (TERM trap installed / grandchild forked).
    /// It ends only on cancel or `release()`; dropping it releases it, so a
    /// failing test leaves no stub behind (and the stub also exits if this
    /// test process dies — see `commands::hold_command`).
    struct HeldWrite {
        latch: PathBuf,
        result: mpsc::Receiver<CommandResult>,
        _dir: tempfile::TempDir,
    }

    impl HeldWrite {
        fn spawn(ctx: &Arc<commands::Context>, shape: HoldShape) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let latch = dir.path().join("latch");
            let ready = dir.path().join("ready");
            let result = spawn_write(
                ctx,
                HelperCommand::TestHold {
                    latch: latch.clone(),
                    ready: ready.clone(),
                    shape,
                },
            );
            let held = Self {
                latch,
                result,
                _dir: dir,
            };
            poll_until("the write to claim the slot", || {
                ctx.snapshot_busy().is_some()
            });
            poll_until("the hold stub to be ready", || ready.exists());
            held
        }

        /// The write's logged args (`in_flight.args`, audit `*_args=`).
        fn args(&self) -> Vec<String> {
            vec![self.latch.display().to_string()]
        }

        /// Let the stub exit 0 on its own.
        fn release(&self) {
            fs::write(&self.latch, b"").unwrap();
        }

        /// The write's result once it has ended. On the deadline: release the
        /// stub, then fail.
        fn join(self) -> CommandResult {
            match self.result.recv_timeout(WAIT_DEADLINE) {
                Ok(r) => r,
                Err(e) => {
                    self.release();
                    #[expect(clippy::panic, reason = "test harness: fail loudly with context")]
                    {
                        panic!("held write did not end within {WAIT_DEADLINE:?}: {e}")
                    }
                }
            }
        }
    }

    impl Drop for HeldWrite {
        fn drop(&mut self) {
            let _ = fs::write(&self.latch, b"");
            // Let the stub see the latch before `_dir` (which holds it) goes.
            let _ = self.result.recv_timeout(WAIT_DEADLINE);
        }
    }

    /// Test harness: long-lived helper accepting connections on a unix socket
    /// using the same thread-per-connection model as production.
    struct TestHelper {
        sock_path: PathBuf,
        token: String,
        log_path: PathBuf,
        conn_count: Arc<AtomicUsize>,
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

        /// No wait needed before reading: `handle_connection` appends its
        /// audit entry (a synchronous, unbuffered `writeln`) BEFORE it returns
        /// and drops the stream, and `request()` reads to that EOF — so every
        /// entry for a request has hit the file by the time `request()`
        /// returns. (Likewise the accept loop logs a cap rejection before it
        /// drops the rejected stream.)
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

    /// A TestHelper plus its Context, for in-process write dispatch.
    struct TestHelperWithCtx {
        helper: TestHelper,
        ctx: Arc<commands::Context>,
    }

    impl TestHelperWithCtx {
        /// At the production cancel grace.
        fn start() -> Self {
            Self::start_with_grace(commands::CANCEL_GRACEFUL_TIMEOUT)
        }

        fn start_with_grace(cancel_grace: Duration) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let sock_path = dir.path().join("test.sock");
            let log_path = dir.path().join("helper.log");
            let token = auth::generate_token().unwrap();

            let listener = UnixListener::bind(&sock_path).unwrap();
            listener.set_nonblocking(true).unwrap();

            let ctx = Arc::new(commands::test_context_with_grace(dir.path(), cancel_grace));
            let audit_log = Arc::new(logging::AuditLog::new(&log_path).unwrap());
            let token_arc = Arc::new(token.clone());
            let shutdown = Arc::new(AtomicBool::new(false));
            let conn_count = Arc::new(AtomicUsize::new(0));
            let last_reject_ms = Arc::new(AtomicI64::new(0));

            let shutdown_clone = Arc::clone(&shutdown);
            let conn_count_loop = Arc::clone(&conn_count);
            let ctx_for_thread = Arc::clone(&ctx);
            let accept_thread = std::thread::spawn(move || loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let current = conn_count_loop.fetch_add(1, Ordering::SeqCst);
                        if current >= MAX_CONCURRENT_CONNECTIONS {
                            conn_count_loop.fetch_sub(1, Ordering::SeqCst);
                            maybe_log_cap_reject(&audit_log, &last_reject_ms);
                            drop(stream);
                            continue;
                        }
                        let token_clone = Arc::clone(&token_arc);
                        let ctx_clone = Arc::clone(&ctx_for_thread);
                        let audit_clone = Arc::clone(&audit_log);
                        let conn_count_clone = Arc::clone(&conn_count_loop);
                        std::thread::spawn(move || {
                            let _guard = ConnGuard {
                                counter: conn_count_clone,
                            };
                            let _ =
                                handle_connection(stream, &token_clone, &ctx_clone, &audit_clone);
                        });
                    }
                    // Accept-loop idle poll (the production loop's shape), not
                    // a test synchronization.
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
                conn_count,
                shutdown,
                _dir: dir,
                accept_thread: Some(accept_thread),
            };
            Self { helper, ctx }
        }

        /// Send wire verb `verb` with the valid token; its final result.
        fn command(&self, verb: &str) -> CommandResult {
            let json = format!(r#"{{"token":"{}","command":"{verb}"}}"#, self.helper.token);
            Self::parse_result(&self.helper.request(&json))
        }

        fn status(&self) -> serde_json::Value {
            self.command("status").data.expect("status data missing")
        }

        /// Send `cancel` and return its data (`cancelled`, `op`, …).
        fn cancel(&self) -> serde_json::Value {
            self.command("cancel").data.expect("cancel data missing")
        }

        /// Send `cancel` and require that it hit an in-flight write.
        fn cancel_in_flight(&self) {
            let data = self.cancel();
            assert_eq!(
                data["cancelled"],
                serde_json::Value::Bool(true),
                "cancel did not find the held write: {data}"
            );
        }

        /// Parse the LAST line of an NDJSON response stream as a CommandResult.
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
                    #[expect(clippy::panic, reason = "test harness: fail loudly with context")]
                    {
                        panic!("no CommandResult line found in {dump}")
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
                    #[expect(clippy::panic, reason = "test harness: fail loudly with context")]
                    {
                        panic!("no Response line found in {dump}")
                    }
                })
        }
    }

    fn string_args(v: &serde_json::Value) -> Vec<String> {
        v.as_array()
            .expect("args is an array")
            .iter()
            .map(|a| a.as_str().expect("arg is a string").to_string())
            .collect()
    }

    fn audit_args(entry: &serde_json::Value) -> Vec<String> {
        string_args(&entry["args"])
    }

    /// The `cancel` audit entry with outcome `completed` (it hit a write).
    fn completed_cancel_entry(h: &TestHelperWithCtx) -> serde_json::Value {
        let entries = h.helper.read_audit_log();
        entries
            .iter()
            .find(|e| e["cmd"] == "cancel" && e["outcome"] == "completed")
            .cloned()
            .unwrap_or_else(|| {
                #[expect(clippy::panic, reason = "test harness: fail loudly with context")]
                {
                    panic!("no completed cancel entry in {entries:#?}")
                }
            })
    }

    /// The write ended by cancel, with EXACTLY the error string
    /// `handle_connection` would copy into the audit entry — derived from
    /// `HelperError::Cancelled`'s Display (whose Obs O3 `cancelled` prefix is
    /// pinned in error.rs), so no literal can drift. `escalated` says which
    /// path ended it: SIGTERM alone, or the SIGKILL escalation.
    fn assert_cancelled(result: &CommandResult, escalated: bool) {
        assert_eq!(
            result.error_kind.as_deref(),
            Some("cancelled"),
            "{result:?}"
        );
        assert_eq!(
            result.error.as_deref(),
            Some(HelperError::Cancelled { escalated }.to_string().as_str()),
            "expected the {} cancel path: {result:?}",
            if escalated {
                "SIGKILL-escalated"
            } else {
                "SIGTERM-only"
            }
        );
    }

    // --- Test (a): concurrent reads during long-running write ---

    /// Status does not wait for an in-flight write: the write cannot end until
    /// the test cancels it, so a status that waited on it would never return —
    /// and it returns showing the write in flight.
    #[test]
    fn test_status_does_not_block_on_write() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "got: {data}");
        let in_flight = &data["in_flight"];
        assert_eq!(in_flight["op"], "test-hold", "got: {data}");
        assert_eq!(string_args(&in_flight["args"]), w.args(), "got: {data}");
        let started_at = in_flight["started_at"].as_str().unwrap();
        chrono::DateTime::parse_from_rfc3339(started_at).expect("started_at not RFC3339");

        h.cancel_in_flight();
        assert_cancelled(&w.join(), false);
    }

    // --- Test (b): write-while-write returns busy ---

    #[test]
    fn test_write_while_write_returns_busy() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let result = h.command("deploy");
        assert_eq!(result.error_kind.as_deref(), Some("busy"));
        let data = result.data.expect("busy data missing");
        assert_eq!(data["op"], "test-hold");
        assert_eq!(string_args(&data["args"]), w.args());

        w.release();
        assert_eq!(w.join().exit_code, Some(0));
    }

    // --- Test (c): cancel mid-write terminates the handler ---

    /// SIGTERM alone ends a cancelled write: under NEVER_ESCALATE no SIGKILL can
    /// arrive before the wait deadline, so a broken SIGTERM/pgid path fails
    /// here instead of being rescued by escalation.
    #[test]
    fn test_cancel_mid_write_terminates_handler() {
        let h = TestHelperWithCtx::start_with_grace(NEVER_ESCALATE);
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);
        h.cancel_in_flight();
        assert_cancelled(&w.join(), false);
        assert!(h.ctx.snapshot_busy().is_none(), "slot not released");
    }

    // --- Test (d): cancel when idle is no-op ---

    #[test]
    fn test_cancel_when_idle_is_noop() {
        let h = TestHelperWithCtx::start();
        let result = h.command("cancel");
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
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);
        w.release();
        assert_eq!(w.join().exit_code, Some(0));
        // Slot is cleared by WriteSlotGuard::drop.
        assert!(h.ctx.snapshot_busy().is_none());
        // Cancel sees nothing in flight.
        let data = h.cancel();
        assert_eq!(data["cancelled"], serde_json::Value::Bool(false));
        assert_eq!(data["reason"], "no-op");
    }

    #[test]
    fn test_repeated_cancel_when_idle() {
        let h = TestHelperWithCtx::start();
        for _ in 0..5 {
            assert_eq!(h.cancel()["cancelled"], serde_json::Value::Bool(false));
        }
    }

    #[test]
    fn test_status_when_idle_busy_false() {
        let h = TestHelperWithCtx::start();
        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(false), "got: {data}");
        // Per CR11: in_flight is literally null (not absent) when idle.
        assert_eq!(data["in_flight"], serde_json::Value::Null, "got: {data}");
        // cluster_write_in_progress is sourced from the mutex; idle => false.
        assert_eq!(
            data["cluster_write_in_progress"],
            serde_json::Value::Bool(false),
            "got: {data}"
        );
    }

    /// `cluster_write_in_progress` must reflect the mutex view, not the legacy
    /// `setup.pid` heuristic. Asserts that a write that is NOT a cluster write
    /// (TestHold) leaves it false while `busy=true`; the positive set is
    /// pinned by `commands::tests::is_cluster_write_exact_set` and the probe
    /// window by `commands::tests::observe_cluster_writes_around_*`.
    #[test]
    fn test_status_cluster_write_in_progress_only_for_cluster_writes() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "got: {data}");
        assert_eq!(
            data["cluster_write_in_progress"],
            serde_json::Value::Bool(false),
            "a non-cluster write in flight must not show cluster_write_in_progress=true: {data}",
        );

        w.release();
        assert_eq!(w.join().exit_code, Some(0));
    }

    #[test]
    fn test_busy_rejection_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let _ = h.command("deploy");

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
        let args = audit_args(entry);
        assert!(
            args.iter().any(|a| a.starts_with("rejected=deploy")),
            "got: {args:?}"
        );
        assert!(
            args.contains(&"rejected_args=".to_string()),
            "got: {args:?}"
        );
        assert!(
            args.iter().any(|a| a.starts_with("in_flight=test-hold")),
            "got: {args:?}"
        );
        assert!(
            args.contains(&format!("in_flight_args={}", w.args().join(" "))),
            "got: {args:?}"
        );

        w.release();
        assert_eq!(w.join().exit_code, Some(0));
    }

    #[test]
    fn test_cancel_completed_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);
        h.cancel_in_flight();

        let cancel_entry = completed_cancel_entry(&h);
        assert_eq!(cancel_entry["error"], "test-hold");
        let args = audit_args(&cancel_entry);
        assert!(
            args.iter().any(|a| a.starts_with("target=test-hold")),
            "got: {args:?}"
        );
        assert_cancelled(&w.join(), false);
    }

    #[test]
    fn test_cancel_noop_is_audit_logged() {
        let h = TestHelperWithCtx::start();
        let _ = h.cancel();
        let entries = h.helper.read_audit_log();
        let cancel_entry = entries
            .iter()
            .find(|e| e["cmd"] == "cancel")
            .expect("no cancel entry");
        assert_eq!(cancel_entry["outcome"], "no-op");
        assert_eq!(cancel_entry["error"], "none");
    }

    /// The `target_args=<args>` token of a completed cancel's audit entry.
    fn cancel_target_args(cancel_entry: &serde_json::Value) -> String {
        audit_args(cancel_entry)
            .iter()
            .find_map(|s| s.strip_prefix("target_args=").map(str::to_string))
            .expect("no target_args in cancel entry")
    }

    /// Obs O4: status's in-flight view and the audit log's record agree.
    /// `cancel`'s audit entry includes `target_args=<args>` derived from the
    /// in-flight op — that's the same source as status's `in_flight.args`,
    /// so they MUST match. (The write itself is invoked in-process via
    /// `commands::execute`, bypassing handle_connection's audit log; the
    /// cancel entry below is the cross-source pin.)
    #[test]
    fn test_status_audit_drift_invariant() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let status_args = string_args(&h.status()["in_flight"]["args"]);
        assert_eq!(status_args, w.args(), "status reports the write's args");

        h.cancel_in_flight();
        assert_cancelled(&w.join(), false);

        assert_eq!(
            cancel_target_args(&completed_cancel_entry(&h)),
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
        // arrives, so each accepted connection holds a slot until we drop
        // the client end at test cleanup.
        let holders: Vec<UnixStream> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| UnixStream::connect(&h.helper.sock_path).unwrap())
            .collect();
        poll_until("the server to accept every held connection", || {
            h.helper.conn_count.load(Ordering::SeqCst) == MAX_CONCURRENT_CONNECTIONS
        });

        // The 33rd connection is accepted then immediately closed by the
        // server (drop(stream) after fetch_sub). Reading from it returns EOF
        // (0 bytes) without any response payload.
        let rejected = UnixStream::connect(&h.helper.sock_path).unwrap();
        rejected.set_read_timeout(Some(WAIT_DEADLINE)).unwrap();
        let mut buf = [0u8; 64];
        let n = (&rejected).read(&mut buf).unwrap_or(usize::MAX);
        assert_eq!(
            n,
            0,
            "33rd connection should see EOF, got {n} bytes: {:?}",
            &buf[..n.min(64)]
        );

        // Audit log: one cap-rejection entry (logged before the EOF above).
        // The dedup window is 1s, but there's only one over-cap connection.
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
    /// grace-then-SIGKILL escalation path (production grace). The error string
    /// carries the `(sigterm timeout, escalated to sigkill)` suffix from
    /// `HelperError::Cancelled { escalated: true }` — the escalation event.
    #[test]
    fn test_sigkill_escalation_logged() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::IgnoringTerm);
        h.cancel_in_flight();
        assert_cancelled(&w.join(), true);
    }

    // --- Iter-2: process-group cancel reaches grandchildren ---

    /// Iter-2 headline test: the holder is a backgrounded grandchild that
    /// inherits stdout/stderr. Without `Command::process_group(0)` +
    /// `kill(-pgid, SIGTERM)`, killing only the immediate shell would leave the
    /// grandchild holding the pipes, and (latch never released, NEVER_ESCALATE so
    /// no SIGKILL) the write would never end — failing at the wait deadline.
    #[test]
    fn test_cancel_kills_grandchild_holding_pipes() {
        let h = TestHelperWithCtx::start_with_grace(NEVER_ESCALATE);
        let w = HeldWrite::spawn(&h.ctx, HoldShape::WithChild);
        h.cancel_in_flight();
        // Also pins the absence of the SIGKILL suffix: the SIGTERM-pgid path
        // alone must end it.
        assert_cancelled(&w.join(), false);
    }

    /// Iter-2: TERM-trapped shell with a backgrounded TERM-ignoring
    /// grandchild forces both the SIGKILL escalation branch AND the grandchild
    /// reach via process-group SIGKILL (production grace).
    #[test]
    fn test_cancel_kills_grandchild_with_sigkill_escalation() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::WithChildIgnoringTerm);
        h.cancel_in_flight();
        assert_cancelled(&w.join(), true);
    }

    // --- Iter-2: cancel_pending field on status response ---

    /// Iter-2: while a cancel is in flight (between `cmd_cancel` storing
    /// `true` into the cancel token and the writer thread releasing the
    /// slot), `status` returns `cancel_pending: true`. Once the slot is
    /// released, `cancel_pending` returns to false (and `busy` to false,
    /// `in_flight` to null).
    ///
    /// The TERM-ignoring hold under NEVER_ESCALATE stays in flight after cancel
    /// until the test releases it, so the pending window is held open by the
    /// test, not by a clock. That cancel returned while the write still holds
    /// the slot is also the proof that cancel does not wait for the writer.
    #[test]
    fn test_cancel_pending_visible_in_status() {
        let h = TestHelperWithCtx::start_with_grace(NEVER_ESCALATE);
        let w = HeldWrite::spawn(&h.ctx, HoldShape::IgnoringTerm);

        // Status before cancel: busy=true, cancel_pending=false.
        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "{data}");
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(false),
            "{data}"
        );

        h.cancel_in_flight();

        // Status after cancel returned, write still held: cancel_pending=true.
        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "{data}");
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(true),
            "expected cancel_pending=true while the cancelled write is held: {data}"
        );

        w.release();
        assert_cancelled(&w.join(), false);

        // Status after release: busy=false, cancel_pending=false, in_flight=null.
        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(false), "{data}");
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(false),
            "{data}"
        );
        assert_eq!(data["in_flight"], serde_json::Value::Null, "{data}");
    }

    /// Iter-2: when busy but no cancel has been issued, status reports
    /// `cancel_pending=false`. Confirms cancel_pending is sourced from the
    /// in-flight op's cancel_token, not derived from the busy flag alone.
    #[test]
    fn test_cancel_pending_false_when_busy_not_cancelling() {
        let h = TestHelperWithCtx::start();
        let w = HeldWrite::spawn(&h.ctx, HoldShape::Plain);

        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "{data}");
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(false),
            "{data}"
        );

        w.release();
        assert_eq!(w.join().exit_code, Some(0));
    }

    /// Iter-2: idle status emits `cancel_pending=false` (CR11 wire-determinism).
    #[test]
    fn test_cancel_pending_false_when_idle() {
        let h = TestHelperWithCtx::start();
        let data = h.status();
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
    /// MUST agree across the cancel path:
    ///   (i)  status's `in_flight.{op,args}` while the cancel is pending
    ///        matches the cancel audit entry's `error` (op) and
    ///        `target_args=<args>`.
    ///   (ii) the writer returns `error_kind == "cancelled"`, which
    ///        handle_connection maps to `OUTCOME_CANCELLED`, with exactly
    ///        `HelperError::Cancelled`'s string (what handle_connection copies
    ///        into the audit entry's `error`; its Obs O3 `cancelled` prefix is
    ///        pinned in error.rs). The SIGKILL-escalated form is asserted by
    ///        `test_sigkill_escalation_logged`.
    ///
    /// The writer is invoked via `commands::execute()` directly (`TestHold`
    /// has no parse arm), so (ii) is asserted on the `CommandResult` — the
    /// SAME source `handle_connection` reads to populate its audit entry.
    #[test]
    fn test_status_audit_drift_invariant_cancel_path() {
        let h = TestHelperWithCtx::start_with_grace(NEVER_ESCALATE);
        let w = HeldWrite::spawn(&h.ctx, HoldShape::IgnoringTerm);

        h.cancel_in_flight();
        let data = h.status();
        assert_eq!(data["busy"], serde_json::Value::Bool(true), "{data}");
        assert_eq!(
            data["cancel_pending"],
            serde_json::Value::Bool(true),
            "cancel_pending should be true while the cancelled write is held: {data}"
        );
        let status_args = string_args(&data["in_flight"]["args"]);
        assert_eq!(status_args, w.args());
        let status_op = data["in_flight"]["op"]
            .as_str()
            .expect("in_flight.op present during pending cancel")
            .to_string();

        w.release();
        let write_result = w.join();

        // (i) The cancel command went over the socket, so handle_connection
        // recorded its audit entry.
        let cancel_entry = completed_cancel_entry(&h);
        assert_eq!(
            cancel_entry["error"].as_str(),
            Some(status_op.as_str()),
            "cancel audit entry's `error` (cancelled-op identifier) must match status's in_flight.op"
        );
        assert_eq!(
            cancel_target_args(&cancel_entry),
            status_args.join(" "),
            "status's in_flight.args does not match cancel audit's target_args"
        );

        // (ii)
        assert_cancelled(&write_result, false);
    }
}
