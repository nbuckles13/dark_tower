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
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, process};

/// Global shutdown flag, set by SIGTERM handler.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

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
    let ctx = commands::Context {
        slug: slug.as_str().to_string(),
        cluster_name: cluster_name.clone(),
        project_root,
        runtime_dir: runtime_dir.clone(),
        registry_path: ports::registry_path(),
        container_runtime,
        host_gateway_ip: args.host_gateway_ip,
    };

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

    // Install SIGTERM handler
    install_signal_handler()?;

    // Log startup (base_port is not yet known — allocation happens in cmd_setup)
    audit_log.log_startup(&cluster_name, &socket_path.to_string_lossy(), process::id());

    eprintln!(
        "[devloop-helper] ready: slug={slug} cluster={cluster_name} socket={} pid={}",
        socket_path.display(),
        process::id()
    );

    // Accept loop
    // Use a timeout on accept so we can check the shutdown flag
    listener.set_nonblocking(true)?;

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            eprintln!("[devloop-helper] shutdown requested, exiting...");
            break;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                // Set blocking mode for the connection
                if let Err(e) = stream.set_nonblocking(false) {
                    eprintln!("[devloop-helper] failed to set blocking mode: {e}");
                    continue;
                }

                if let Err(e) = handle_connection(stream, &token, &ctx, &audit_log) {
                    eprintln!("[devloop-helper] connection error: {e}");
                }

                if SHUTDOWN.load(Ordering::Relaxed) {
                    eprintln!("[devloop-helper] shutdown requested after command, exiting...");
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection pending — sleep briefly and check shutdown flag
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
        audit_log.log_command("unknown", &[], 0, 1, Some(&err.to_string()));
        let resp = Response::err(&err);
        send_response(&mut writer, &resp)?;
        return Ok(());
    }

    // Parse JSON request
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(req) => req,
        Err(e) => {
            let err = HelperError::InvalidRequest(format!("invalid JSON: {e}"));
            audit_log.log_command("unknown", &[], 0, 1, Some(&err.to_string()));
            let resp = Response::err(&err);
            send_response(&mut writer, &resp)?;
            return Ok(());
        }
    };

    // Validate auth token
    if let Err(e) = auth::validate_token(&request.token, expected_token) {
        audit_log.log_command("auth", &[], 0, 1, Some(&e.to_string()));
        let resp = Response::err(&e);
        send_response(&mut writer, &resp)?;
        return Ok(());
    }

    // Parse and validate command
    let cmd = match request.parse_command() {
        Ok(cmd) => cmd,
        Err(e) => {
            audit_log.log_command(&request.command, &[], 0, 1, Some(&e.to_string()));
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

    // Log to audit log
    let exit_code = result.exit_code.unwrap_or(-1);
    let error_str = result.error.as_deref();
    audit_log.log_command(
        &cmd_name,
        &cmd_args,
        result.duration_ms,
        exit_code,
        error_str,
    );

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

/// Install SIGTERM signal handler.
fn install_signal_handler() -> Result<(), HelperError> {
    // SAFETY: Setting an atomic bool from a signal handler is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, sigterm_handler as libc::sighandler_t);
    }
    Ok(())
}

/// Signal handler — sets the shutdown flag.
extern "C" fn sigterm_handler(_sig: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

/// Clean up PID file, socket, and auth token on exit.
fn cleanup(pid_path: &Path, socket_path: &Path, runtime_dir: &Path) {
    let _ = fs::remove_file(pid_path);
    let _ = fs::remove_file(socket_path);
    let _ = fs::remove_file(runtime_dir.join("auth-token"));
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
        let project_root = PathBuf::from("/tmp/devloop-test-nonexistent");
        let runtime_dir = dir.path().to_path_buf();
        let registry_path = dir.path().join("port-registry.json");
        let ctx = commands::Context {
            slug: "test".to_string(),
            cluster_name: "devloop-test".to_string(),
            project_root,
            runtime_dir,
            registry_path,
            container_runtime: commands::ContainerRuntime::Podman,
            host_gateway_ip: None,
        };
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

        // Read ALL lines until EOF
        let mut lines = Vec::new();
        let mut reader = BufReader::new(&client);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => lines.push(line),
                Err(_) => break,
            }
        }

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
        let project_root = PathBuf::from("/tmp/devloop-test-nonexistent");
        let runtime_dir = dir.path().to_path_buf();
        let registry_path = dir.path().join("port-registry.json");
        let ctx = commands::Context {
            slug: "test".to_string(),
            cluster_name: "devloop-test".to_string(),
            project_root,
            runtime_dir,
            registry_path,
            container_runtime: commands::ContainerRuntime::Podman,
            host_gateway_ip: None,
        };
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

        // Read ALL lines until EOF
        let mut lines = Vec::new();
        let mut reader = BufReader::new(&client);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => lines.push(line),
                Err(_) => break,
            }
        }

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
