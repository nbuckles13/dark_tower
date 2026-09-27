//! Command execution for the devloop helper.
//!
//! All external commands use `Command::new().arg()` — no shell interpolation.
//! User-facing commands stream stdout/stderr line-by-line to the client via
//! an mpsc channel. Internal commands (`cluster_already_exists`, `which`) use
//! buffered `.output()` since they need to inspect output programmatically.

use crate::error::HelperError;
use crate::fs_atomic::atomic_write_secret;
use crate::logging::now_rfc3339;
use crate::ports::{self, PortAllocation, PortOffsets};
use crate::protocol::{
    CommandOutcome, CommandResult, CommandStarted, HelperCommand, Service, StreamKind, StreamLine,
    StreamMsg, MAX_LINE_LEN,
};
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Bounded timeouts for the apiserver-reachability probe (per Security pt.3).
/// The probe runs on EVERY `status` call, and `status` is `timeout`-wrapped by
/// `scripts/layer7.sh`; the SUM of these two host-subprocess bounds plus a
/// margin must stay strictly under that shell `timeout`, or a healthy-but-slow
/// probe trips rc-124 → `helper-unreachable` and the operator restarts a helper
/// that is fine. Current sum: inspect (5s) + TCP connect (3s) = 8s < the shell's
/// `DEVLOOP_SELF_HEAL_STATUS_TIMEOUT` (20s) in `layer7.sh`. Keep that inequality
/// when tuning either side.
const INSPECT_TIMEOUT_SECS: &str = "5";
const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Hostname that dev containers use to reach the host via podman's gateway.
/// This is a well-known podman/slirp4netns convention — resolves to the
/// host-gateway IP (e.g., 10.255.255.254) inside containers.
const CONTAINER_HOST: &str = "host.containers.internal";

/// Default host-gateway IP for Kind NodePort listenAddress (ADR-0030).
/// Used as fallback when `--host-gateway-ip` is not provided.
const DEFAULT_HOST_GATEWAY_IP: &str = "10.255.255.254";

/// Graceful-shutdown window between SIGTERM and SIGKILL on cancel
/// (per security S5). The 2 s window is for the cooperative shell/kubectl
/// tree spawned by writer commands — `setup.sh` running `kubectl wait`,
/// `kubectl apply`, `kind create cluster`, etc. — all of which respond
/// promptly to SIGTERM. We do NOT signal the Kind cluster's
/// containerd/kubelet/etcd: those run inside Kind's container as detached
/// components managed by the cluster's PID 1, not as descendants of
/// setup.sh — they were never in our process group. Partial-Kind state
/// from a cancelled `setup` is recovered by setup.sh's "cluster exists,
/// reusing" branch on the next setup. Process-wide shutdown stays
/// SIGKILL-immediate; see the `IMPORTANT:` comment in
/// `run_command_streaming`.
pub const CANCEL_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(2);

/// Lock a `Mutex` recovering from poison (per ADR-0002 + CR7).
/// Centralized helper so all callsites are panic-free; single change point
/// if we ever switch to `parking_lot::Mutex`.
fn lock_recovered<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Send `sig` to the process group led by `child`. Returns
/// `Err(HelperError::CommandFailed)` (per ADR-0002 — never silently swallow
/// the overflow case, even though it is defensive-only) if the child's PID
/// exceeds `i32` range or equals `i32::MIN` (so its `checked_neg` is
/// undefined). Caller pattern at all signalling sites is:
///
/// ```ignore
/// if signal_process_group(&child, libc::SIGKILL).is_err() {
///     // Pathological PID — fall back to single-PID kill of the leader.
///     // Best effort; any grandchildren leak until next sweep.
///     let _ = child.kill();
/// }
/// ```
///
/// SAFETY contract (S3-(a) continuity): the caller MUST be the sole signaller
/// of `child`, MUST NOT have called `child.wait()` yet, AND MUST own the
/// child via mutable borrow / move (not via shared `Arc<Mutex<Child>>` etc.) —
/// the borrow checker is what enforces sole-signaller-ness. All three
/// invariants hold inside `run_command_streaming` because the writer thread
/// owns `Child` on its own stack and `wait()` is the very last call. The PID
/// derives from `child.id()` and never crosses thread boundaries —
/// `cmd_cancel` only flips an atomic; the writer thread alone signals the
/// group.
fn signal_process_group(child: &Child, sig: libc::c_int) -> Result<(), HelperError> {
    // Guard against pid 0: `libc::kill(0, sig)` means "every process in the
    // calling process's group" — which would SIGKILL the helper itself.
    // `Child::id()` shouldn't ever return 0 (Linux PIDs start at 1), but the
    // FFI surface is sharp enough that a guard is cheap insurance.
    debug_assert!(
        child.id() > 0,
        "child.id() must be >0 (libc::kill(0,...) would target the helper's own group)"
    );
    let pid_i32 = i32::try_from(child.id()).map_err(|_| HelperError::CommandFailed {
        cmd: "kill".to_string(),
        detail: format!("PID {} exceeds i32 range", child.id()),
    })?;
    let neg_pid = pid_i32
        .checked_neg()
        .ok_or_else(|| HelperError::CommandFailed {
            cmd: "kill".to_string(),
            detail: format!("PID {pid_i32} cannot be negated for pgid signal"),
        })?;
    debug_assert!(
        neg_pid < 0,
        "checked_neg should yield negative for pgid signal"
    );
    // SAFETY: libc::kill is FFI; neg_pid is a valid pgid (negative of leader PID
    // for a child spawned with `Command::process_group(0)`).
    unsafe {
        libc::kill(neg_pid, sig);
    }
    Ok(())
}

/// Per-write cancellation signal. Wraps the helper's process-wide shutdown
/// flag and a per-write cancel token; either being set cancels the in-flight
/// child (per CR3).
#[derive(Clone)]
pub struct CancelSignal {
    pub shutdown: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
}

impl CancelSignal {
    pub fn new(shutdown: Arc<AtomicBool>, cancel: Arc<AtomicBool>) -> Self {
        Self { shutdown, cancel }
    }

    /// True if either shutdown OR per-write cancel is set. Available for
    /// callers that want a single check; `run_command_streaming` checks the
    /// two flags separately to drive distinct kill paths (SIGTERM-then-SIGKILL
    /// vs SIGKILL-immediate).
    #[allow(dead_code)]
    pub fn is_cancelled(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed) || self.cancel.load(Ordering::Relaxed)
    }

    /// True if the per-write cancel token (not process shutdown) is set.
    /// Used to map command-failed errors to `HelperError::Cancelled`.
    pub fn cancel_set(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Construct from process shutdown only (no per-write token). Used by
    /// tests; production sites always own a per-write token via
    /// `run_with_write_slot`.
    #[allow(dead_code)]
    pub fn shutdown_only(shutdown: Arc<AtomicBool>) -> Self {
        Self {
            shutdown,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Information about a write command in flight on this helper.
///
/// Per Security S3 option (a): NO `child_pid` field. The PID stays on the
/// writer's stack inside `run_command_streaming`; signals are issued from
/// there before any `wait()` call, so the kernel cannot recycle the PID.
pub struct InFlightOp {
    pub op: String,
    pub args: Vec<String>,
    pub started_at: String,
    pub cancel_token: Arc<AtomicBool>,
}

/// Shared write-mutex state. The `Mutex` guards the `Option`, NOT the running
/// command — claiming the slot is `lock → check None → insert`, releasing is
/// `lock → set None`. The writer holds NEITHER the mutex nor any lock during
/// the long-running command body, so reads (`status`) and `cancel` always
/// observe state without contention.
pub struct WriteState {
    pub in_flight: Option<InFlightOp>,
}

impl WriteState {
    pub fn new() -> Self {
        Self { in_flight: None }
    }
}

impl Default for WriteState {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that clears `WriteState.in_flight` on drop. Ensures the slot is
/// released on ANY exit path (success, error, panic). Cancel never lock-steals
/// the slot — only the writer (via this guard) clears it.
struct WriteSlotGuard<'a> {
    state: &'a Mutex<WriteState>,
}

impl Drop for WriteSlotGuard<'_> {
    fn drop(&mut self) {
        let mut g = lock_recovered(self.state);
        g.in_flight = None;
    }
}

/// Snapshot of the in-flight op for a `status` response (per Security S4
/// snapshot consistency). Cloned under lock, lock dropped, snapshot returned —
/// the rest of `cmd_status` proceeds without holding the mutex.
///
/// `cancel_pending` is loaded from the in-flight op's `cancel_token` while
/// the lock is still held, so the snapshot is a single-acquisition consistent
/// view of (op, args, started_at, cancel_pending).
pub struct BusyHint {
    pub op: String,
    pub args: Vec<String>,
    pub started_at: String,
    pub cancel_pending: bool,
}

/// Runtime context for command execution.
pub struct Context {
    pub slug: String,
    pub cluster_name: String,
    pub project_root: PathBuf,
    pub runtime_dir: PathBuf,
    pub registry_path: PathBuf,
    pub container_runtime: ContainerRuntime,
    /// Host-gateway IP for Kind NodePort listenAddress (ADR-0030).
    /// Detected by devloop.sh and passed via --host-gateway-ip.
    pub host_gateway_ip: Option<String>,
    /// Cargo parallelism cap passed to every service image build as
    /// `--build-arg CARGO_BUILD_JOBS` (validated at startup; forwarded by devloop.sh with
    /// the pipeline default, see scripts/lang/_common.sh).
    pub cargo_build_jobs: String,
    /// Shutdown flag — set by signal handler, checked during long-running commands.
    pub shutdown: Arc<AtomicBool>,
    /// Shared write-mutex state — `None` when idle, `Some(InFlightOp)` when a
    /// write command is running. Reads/cancel observe; only the writer sets it.
    pub write_state: Arc<Mutex<WriteState>>,
    /// Self-heal recreate bound (Delta 1): set true (via `compare_exchange`) the
    /// moment before the FIRST automated `recreate` destroys, so at most ONE
    /// automated recreate happens per helper PROCESS LIFETIME. This is the
    /// BOUNDED safety control, and it lives in process memory ON PURPOSE — the
    /// earlier design keyed a file marker in `runtime_dir`, which the dev
    /// container can `rm` (it is bind-mounted RW at the same uid, devloop.sh:583),
    /// defeating the bound with one command. An `AtomicBool` in the helper's own
    /// address space is unforgeable by the container. Per-process == per-lifetime:
    /// the Gate-2 outer retry re-enters the same live helper (suppressed); a host
    /// crash restarts the helper (a fresh, legitimate allowance). `Context` is
    /// held behind `Arc` and never cloned by value, so every connection observes
    /// this same atom.
    pub recreate_bound: AtomicBool,
}

impl Context {
    /// Snapshot the in-flight op (or `None` if idle). Lock is held only for
    /// the duration of the clone; the snapshot is returned by value.
    pub fn snapshot_busy(&self) -> Option<BusyHint> {
        let g = lock_recovered(&self.write_state);
        g.in_flight.as_ref().map(|o| BusyHint {
            op: o.op.clone(),
            args: o.args.clone(),
            started_at: o.started_at.clone(),
            cancel_pending: o.cancel_token.load(Ordering::Relaxed),
        })
    }
}

/// Detected container runtime.
#[derive(Debug, Clone, Copy)]
pub enum ContainerRuntime {
    Podman,
    Docker,
}

impl ContainerRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
        }
    }

    pub fn kind_provider_env(self) -> (&'static str, &'static str) {
        match self {
            Self::Podman => ("KIND_EXPERIMENTAL_PROVIDER", "podman"),
            Self::Docker => ("KIND_EXPERIMENTAL_PROVIDER", "docker"),
        }
    }
}

/// Detect the available container runtime.
pub fn detect_container_runtime() -> Result<ContainerRuntime, HelperError> {
    if which("podman") {
        Ok(ContainerRuntime::Podman)
    } else if which("docker") {
        Ok(ContainerRuntime::Docker)
    } else {
        Err(HelperError::CommandFailed {
            cmd: "detect-runtime".to_string(),
            detail: "neither podman nor docker found in PATH".to_string(),
        })
    }
}

/// Check if a command exists in PATH.
fn which(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check that required tools are available.
pub fn check_prerequisites() -> Result<ContainerRuntime, HelperError> {
    if !which("kind") {
        return Err(HelperError::CommandFailed {
            cmd: "prerequisites".to_string(),
            detail: "kind not found in PATH".to_string(),
        });
    }
    if !which("kubectl") {
        return Err(HelperError::CommandFailed {
            cmd: "prerequisites".to_string(),
            detail: "kubectl not found in PATH".to_string(),
        });
    }
    detect_container_runtime()
}

/// Execute a helper command, streaming output to the client via the writer.
///
/// Sends a `CommandStarted` message, streams stdout/stderr as `StreamLine` messages,
/// and returns a `CommandResult` with exit code, duration, and optional data.
/// The caller is responsible for writing the `CommandResult` to the socket.
///
/// Write commands serialize on `ctx.write_state` (per Operations + Security).
/// Reads (`Status`) and control (`Cancel`) bypass the lock entirely.
pub fn execute(cmd: &HelperCommand, ctx: &Context, writer: &mut dyn Write) -> CommandResult {
    let start = Instant::now();
    let cmd_name = cmd.name();

    // Send started message
    let started = CommandStarted {
        started: true,
        cmd: cmd_name.to_string(),
        ts: now_rfc3339(),
    };
    if let Err(e) = send_json_line(writer, &started) {
        eprintln!("[devloop-helper] failed to send started message: {e}");
    }

    let result = if cmd.is_write() {
        run_with_write_slot(cmd, ctx, writer)
    } else {
        // Reads + control commands bypass the write mutex.
        match cmd {
            HelperCommand::Status => cmd_status(ctx),
            HelperCommand::Cancel => cmd_cancel(ctx),
            other => Err(HelperError::InvalidCommand(format!(
                "non-write command not handled: {}",
                other.name()
            ))),
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(data) => CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms,
            error: None,
            error_kind: None,
            data,
        },
        Err(e) => {
            // For Busy errors, also surface the structured op/args via data
            // so the streaming protocol matches what `Response::err` would
            // emit on the unary side (per Obs O1 wire-shape consistency).
            let data = match &e {
                HelperError::Busy { op, args } => Some(serde_json::json!({
                    "op": op,
                    "args": args,
                })),
                _ => None,
            };
            CommandResult {
                result: CommandOutcome::Error,
                exit_code: None,
                duration_ms,
                error: Some(e.to_string()),
                error_kind: Some(e.kind().to_string()),
                data,
            }
        }
    }
}

/// Try to claim the write slot, run the command, release the slot via
/// `WriteSlotGuard` on any exit path. Returns `HelperError::Busy` if another
/// write is already in flight (per Security S1 + Obs O1 collision-pair).
/// Returns `HelperError::Cancelled` if the per-write cancel token was set
/// during execution.
fn run_with_write_slot(
    cmd: &HelperCommand,
    ctx: &Context,
    writer: &mut dyn Write,
) -> Result<Option<serde_json::Value>, HelperError> {
    let cancel_token = Arc::new(AtomicBool::new(false));

    // Try to claim the slot.
    {
        let mut g = lock_recovered(&ctx.write_state);
        if let Some(in_flight) = g.in_flight.as_ref() {
            return Err(HelperError::Busy {
                op: in_flight.op.clone(),
                args: in_flight.args.clone(),
            });
        }
        g.in_flight = Some(InFlightOp {
            op: cmd.name().to_string(),
            args: cmd.args_for_log(),
            started_at: now_rfc3339(),
            cancel_token: Arc::clone(&cancel_token),
        });
    }
    // RAII: clear the slot on any exit path (success/error/panic).
    let _guard = WriteSlotGuard {
        state: &ctx.write_state,
    };

    let signal = CancelSignal::new(Arc::clone(&ctx.shutdown), Arc::clone(&cancel_token));

    let result = match cmd {
        HelperCommand::Setup { skip_observability } => {
            cmd_setup(ctx, *skip_observability, writer, &signal)
        }
        HelperCommand::Rebuild(svc) => cmd_rebuild(ctx, *svc, writer, &signal).map(|()| None),
        HelperCommand::RebuildAll => cmd_rebuild_all(ctx, writer, &signal).map(|()| None),
        HelperCommand::Deploy(svc) => cmd_deploy(ctx, *svc, writer, &signal).map(|()| None),
        HelperCommand::Teardown => cmd_teardown(ctx, writer, &signal).map(|()| None),
        HelperCommand::Recreate => cmd_recreate(ctx, writer, &signal),
        HelperCommand::RestoreKubeconfig => cmd_restore_kubeconfig(ctx, writer, &signal),
        #[cfg(test)]
        HelperCommand::TestSleep { seconds } => {
            cmd_test_sleep(ctx, *seconds, writer, &signal).map(|()| None)
        }
        #[cfg(test)]
        HelperCommand::TestSleepIgnoringTerm { seconds } => {
            cmd_test_sleep_ignoring_term(ctx, *seconds, writer, &signal).map(|()| None)
        }
        #[cfg(test)]
        HelperCommand::TestSleepWithChild { seconds } => {
            cmd_test_sleep_with_child(ctx, *seconds, writer, &signal).map(|()| None)
        }
        #[cfg(test)]
        HelperCommand::TestSleepWithChildIgnoringTerm { seconds } => {
            cmd_test_sleep_with_child_ignoring_term(ctx, *seconds, writer, &signal).map(|()| None)
        }
        // The match in `execute` above ensures non-write commands never reach here.
        other => Err(HelperError::InvalidRequest(format!(
            "internal: write dispatcher reached non-write command {}",
            other.name()
        ))),
    };

    // Map command-failed errors to HelperError::Cancelled when the per-write
    // cancel token was set during execution. Process-shutdown does NOT remap
    // (we're going down; the helper's exit path swallows it).
    result.map_err(|e| {
        if signal.cancel_set() {
            // The escalation flag will be set by run_command_streaming when
            // SIGKILL was needed. Default to false here; the streaming path
            // sets it via a more direct mechanism if needed (we lose the
            // signal-vs-escalated distinction across the error boundary in
            // the rare case where command_failed fires before SIGTERM was
            // even sent, which is fine — clean cancel string).
            match &e {
                HelperError::Cancelled { .. } => e,
                _ => HelperError::Cancelled { escalated: false },
            }
        } else {
            e
        }
    })
}

/// Test-only: run `sleep N` via run_command_streaming so the concurrency
/// tests exercise the real cancel-token + child-kill path. Spawn `sleep`
/// directly (NOT via /bin/sh) so SIGTERM goes to the actual sleeping
/// process — some shells fork+wait instead of exec, which would mask
/// signal delivery.
#[cfg(test)]
fn cmd_test_sleep(
    ctx: &Context,
    seconds: u64,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] test-sleep: sleeping for {seconds}s");
    let _ = ctx; // unused
    let mut cmd = Command::new("sleep");
    cmd.arg(seconds.to_string());
    run_command_streaming(&mut cmd, "test-sleep", writer, signal)
}

/// Test-only: shell stub that traps SIGTERM and then sleeps. Forces
/// `run_command_streaming` to wait `CANCEL_GRACEFUL_TIMEOUT` and escalate
/// to SIGKILL — exercising the `escalated: true` branch of
/// `HelperError::Cancelled` (Security S5).
#[cfg(test)]
fn cmd_test_sleep_ignoring_term(
    ctx: &Context,
    seconds: u64,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] test-sleep-ignoring-term: blocking for up to {seconds}s with TERM trapped");
    let _ = ctx;
    // Busy-loop in the shell itself (no `sleep` child) so SIGKILL on the
    // shell terminates everything that holds the stdout/stderr pipes —
    // otherwise an orphaned `sleep` keeps the pipes open and the reader
    // threads never see EOF.
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(format!(
        "trap '' TERM; i=0; max=$(( {seconds} * 1000000 )); \
         while [ $i -lt $max ]; do i=$(( i + 1 )); done"
    ));
    run_command_streaming(&mut cmd, "test-sleep-ignoring-term", writer, signal)
}

/// Test-only: spawn `bash -c 'sleep N & wait'` so a forked grandchild
/// inherits stdout/stderr. Without process-group SIGTERM, killing only the
/// immediate `bash` would leave the orphaned `sleep` holding the pipes
/// open until natural exit (~N seconds). With `process_group(0)` +
/// `kill(-pgid, SIGTERM)` the grandchild is reached and cancel completes
/// promptly. This stub validates the iter-2 process-group cancel path.
#[cfg(test)]
fn cmd_test_sleep_with_child(
    ctx: &Context,
    seconds: u64,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] test-sleep-with-child: bash forking sleep {seconds}");
    let _ = ctx;
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(format!("sleep {seconds} & wait"));
    run_command_streaming(&mut cmd, "test-sleep-with-child", writer, signal)
}

/// Test-only: TERM-trapped bash with a backgrounded `sleep` grandchild.
/// Forces both the SIGKILL escalation branch AND the grandchild reach via
/// process-group SIGKILL — so a successful cancel proves the SIGKILL path
/// also signals via pgid (not just the cooperative SIGTERM path).
#[cfg(test)]
fn cmd_test_sleep_with_child_ignoring_term(
    ctx: &Context,
    seconds: u64,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] test-sleep-with-child-ignoring-term: trapped bash forking sleep {seconds}");
    let _ = ctx;
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(format!("trap '' TERM; sleep {seconds} & wait"));
    run_command_streaming(
        &mut cmd,
        "test-sleep-with-child-ignoring-term",
        writer,
        signal,
    )
}

/// Cancel: signal the in-flight write to abort. Idempotent — returns no-op
/// success when no write is running. Auth-gated like every other command
/// (the parse layer + handle_connection enforce token validation per CR9).
///
/// Cancel does NOT lock-steal the write slot — only the writer (via
/// `WriteSlotGuard::drop`) clears it. Cancel just flips the per-write cancel
/// token; the writer thread (which alone holds `Child`) does the SIGTERM
/// inside `run_command_streaming`. This closes Security S3's PID-recycle
/// TOCTOU because no PID ever crosses thread boundaries.
fn cmd_cancel(ctx: &Context) -> Result<Option<serde_json::Value>, HelperError> {
    let snapshot = {
        let g = lock_recovered(&ctx.write_state);
        g.in_flight.as_ref().map(|o| {
            (
                o.op.clone(),
                o.args.clone(),
                o.started_at.clone(),
                Arc::clone(&o.cancel_token),
            )
        })
    };
    match snapshot {
        Some((op, args, started_at, cancel_token)) => {
            cancel_token.store(true, Ordering::SeqCst);
            eprintln!("[devloop-helper] cancel: signalled in-flight {op}");
            Ok(Some(serde_json::json!({
                "cancelled": true,
                "op": op,
                "args": args,
                "started_at": started_at,
            })))
        }
        None => {
            eprintln!("[devloop-helper] cancel: no write in flight (no-op)");
            Ok(Some(serde_json::json!({
                "cancelled": false,
                "reason": "no-op",
            })))
        }
    }
}

/// Validate that a host-gateway IP is a valid, non-unspecified address (ADR-0030).
///
/// Rejects `0.0.0.0` and `::` which would expose Kind NodePorts to the LAN.
fn validate_gateway_ip(ip: &str) -> Result<(), HelperError> {
    let addr: std::net::IpAddr = ip
        .parse()
        .map_err(|_| HelperError::InvalidRequest(format!("invalid host-gateway-ip: '{ip}'")))?;
    if addr.is_unspecified() {
        return Err(HelperError::InvalidRequest(
            "host-gateway-ip must not be 0.0.0.0 or :: (ADR-0030 prohibits binding to all interfaces)".to_string(),
        ));
    }
    Ok(())
}

// =============================================================================
// Self-heal: apiserver-reachability probe (ADR-0030 §self-heal, Deltas 1 & 2)
// =============================================================================

/// Positive tri-state reachability of the cluster's apiserver.
///
/// Only `Unreachable` licenses a destroy, and (Delta 2) it comes from EXACTLY
/// one place: the control-plane container reported stopped by `<runtime>
/// inspect` (`rc==0 && "false"`). The TCP step can only confirm life
/// (`Reachable`) or fail to determine (`Unknown`) — it NEVER yields
/// `Unreachable`, because its inputs (`ports.json`'s port, the gateway IP) are
/// influenceable by the container-writable mount / a compile-time default, and
/// a refusal at a possibly-wrong address is not evidence anything died.
/// Everything indeterminate ⇒ `Unknown` ⇒ escalate, never destroy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiserverReachability {
    Reachable,
    Unreachable,
    Unknown,
}

impl ApiserverReachability {
    /// Serialized additively as a string in `status` JSON (`reachable |
    /// unreachable | unknown`). The shell's `__self_heal_cluster` classifies off
    /// this; `cmd_recreate` re-confirms host-side.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reachable => "reachable",
            Self::Unreachable => "unreachable",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify a `<runtime> inspect -f '{{.State.Running}}'` result into a
/// container-running tri-state, WITHOUT parsing any runtime error string.
///
/// - `Some(true)`  — rc 0, stdout `"true"`  (container running).
/// - `Some(false)` — rc 0, stdout `"false"` (container STOPPED — the sole
///   `Unreachable`/destroy license per Delta 2 / R3-final).
/// - `None`        — everything else ⇒ `Unknown`: spawn/exec failure, any
///   non-zero exit (timeout 124/137, container ABSENT [Docker exit 1 / Podman
///   125], daemon down — indistinguishable by exit code, and all correctly
///   `Unknown`), or rc 0 with unexpected stdout. Container-absence is licensed
///   separately and positively via `cluster_already_exists() == Ok(false)`, NOT
///   here — so a runtime hiccup can never read as a destroy license.
///
/// Pure over `(spawned_ok, exit_code, stdout)` so every cell is unit-testable
/// without a container runtime.
fn inspect_signal(spawned_ok: bool, exit_code: Option<i32>, stdout: &str) -> Option<bool> {
    if !spawned_ok {
        return None; // spawn/exec failure (incl. `timeout` binary missing) ⇒ Unknown
    }
    if exit_code != Some(0) {
        return None; // any non-zero (timeout / absent / daemon-down) ⇒ Unknown
    }
    match stdout.trim() {
        "true" => Some(true),
        "false" => Some(false),
        // rc 0 but not a clean bool: DO NOT default to `false` — that would
        // destroy on garbage. Unknown.
        _ => None,
    }
}

/// Pure reachability classifier over the three probe signals. `Some(false)`
/// container-running is the SOLE `Unreachable`; TCP yields only `Reachable`
/// (connected) or `Unknown` (anything else). Exhaustive by construction.
fn reachability_from_signals(
    container_running: Option<bool>,
    port: Option<u16>,
    tcp_connected: Option<bool>,
) -> ApiserverReachability {
    match container_running {
        // Container STOPPED — the only positive destroy license from the probe.
        Some(false) => ApiserverReachability::Unreachable,
        // inspect indeterminate (spawn-err / timeout / absent / garbage) ⇒ Unknown.
        None => ApiserverReachability::Unknown,
        // Container running: only a successful TCP connect proves the apiserver
        // is listening. Refused / no-route (Some(false)) and timeout (None) and
        // an undeterminable port (None) ALL ⇒ Unknown, never Unreachable.
        Some(true) => match (port, tcp_connected) {
            (Some(_), Some(true)) => ApiserverReachability::Reachable,
            _ => ApiserverReachability::Unknown,
        },
    }
}

/// Read the live apiserver port from the persisted port map (`ports.json`
/// `.ports.k8s_api`) — the SAME value setup wrote (S4-convergent). Returns
/// `None` if the file is missing/unparseable or the port is absent/zero.
///
/// This is also `restore-kubeconfig`'s port source (S4): NEVER a fresh
/// `allocate_ports`, which on a restarted helper could hand a different slot and
/// point a cluster-admin kubeconfig at another devloop's apiserver.
fn read_k8s_api_port(ctx: &Context) -> Option<u16> {
    let ports_path = ctx.runtime_dir.join("ports.json");
    let contents = fs::read_to_string(&ports_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let port = value.pointer("/ports/k8s_api").and_then(|v| v.as_u64())?;
    u16::try_from(port).ok().filter(|&p| p != 0)
}

/// Read the persisted `observability_deployed` SSoT from the live `ports.json`
/// (written from `!skip_observability` by setup). A cluster brought up with
/// `--skip-observability` must be RECREATED without observability too — else the
/// self-heal silently restores a stack the operator explicitly opted out of
/// (@code-reviewer, config-over-hardcoding). Defaults to `true` (full stack) when
/// the file is missing/unparseable — there is then no SSoT to honour, and that
/// matches the pre-existing behaviour.
fn read_observability_deployed(ctx: &Context) -> bool {
    let ports_path = ctx.runtime_dir.join("ports.json");
    fs::read_to_string(&ports_path)
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v.get("observability_deployed").and_then(|b| b.as_bool()))
        .unwrap_or(true)
}

/// `<runtime> inspect -f '{{.State.Running}}' <cluster>-control-plane`, bounded
/// by coreutils `timeout` (Security pt.3 — a wedged runtime must not hang the
/// probe/recreate). Reads from the container runtime, NOT the RW mount, so the
/// container cannot influence this observation (ADR-0030:468 forbids mounting
/// the runtime socket into the container).
fn inspect_container_running(ctx: &Context) -> Option<bool> {
    let node = format!("{}-control-plane", ctx.cluster_name);
    let runtime = ctx.container_runtime.as_str();
    match Command::new("timeout")
        .arg(INSPECT_TIMEOUT_SECS)
        .arg(runtime)
        .arg("inspect")
        .arg("-f")
        .arg("{{.State.Running}}")
        .arg(&node)
        .output()
    {
        Ok(out) => inspect_signal(
            true,
            out.status.code(),
            &String::from_utf8_lossy(&out.stdout),
        ),
        // spawn failure (incl. `timeout` not on PATH) ⇒ Unknown, never a license.
        Err(_) => inspect_signal(false, None, ""),
    }
}

/// Bounded TCP connect to `host:port`. `Some(true)` connected, `Some(false)`
/// refused / no-route, `None` timeout / resolve error. Only `Some(true)` feeds
/// `Reachable`; the other two are `Unknown` (Delta 2), but the distinction is
/// kept for the unit tests and future use.
fn tcp_probe(host: &str, port: u16) -> Option<bool> {
    let mut addrs = match (host, port).to_socket_addrs() {
        Ok(a) => a,
        Err(_) => return None,
    };
    let addr = addrs.next()?;
    match TcpStream::connect_timeout(&addr, TCP_CONNECT_TIMEOUT) {
        Ok(_) => Some(true),
        // Refused/reset = a definite "not listening" answer. Everything else
        // (TimedOut / WouldBlock / no-route / other) ⇒ None. Under Delta 2 both
        // Some(false) and None classify to Unknown, so only `Some(true)` is
        // load-bearing; the distinction is kept for the unit tests.
        Err(e) => match e.kind() {
            ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset => Some(false),
            _ => None,
        },
    }
}

/// Gather the three signals and classify. Kubeconfig-free, HTTP-status-agnostic,
/// no stderr parsing. Used by BOTH `cmd_status` (the shell's classification
/// input) and `cmd_recreate` (the host-side re-confirmation at the moment of
/// action) — one definition, so the two can differ only in timing, never in what
/// `Unreachable` means.
pub fn probe_apiserver_reachable(ctx: &Context) -> ApiserverReachability {
    let container_running = inspect_container_running(ctx);
    let port = read_k8s_api_port(ctx);
    let tcp_connected = match (container_running, port) {
        (Some(true), Some(p)) => {
            let host = ctx
                .host_gateway_ip
                .as_deref()
                .unwrap_or(DEFAULT_HOST_GATEWAY_IP);
            tcp_probe(host, p)
        }
        // No point probing TCP unless the container is running and we know the port.
        _ => None,
    };
    reachability_from_signals(container_running, port, tcp_connected)
}

// =============================================================================
// Self-heal: recreate + restore-kubeconfig outcome vocabulary
// =============================================================================

/// Structured outcome the helper reports for a self-heal verb, in
/// `CommandResult.data.self_heal.outcome`. Typed (not a `json!` string literal)
/// so the exact token strings are compile-checked — the shell greps them, and a
/// typo would ship a silently-wrong token no compiler catches (@code-reviewer).
///
/// The FAILURE values are BYTE-IDENTICAL to the shell's `DETAIL=` set and the
/// runbook §8 rows, so `grep 'SELF_HEAL '` puts the helper's raw report beside
/// the shell's rendering with no near-miss to reconcile by eye. See
/// `test_self_heal_outcome_failure_tokens_match_detail_set` for the pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum SelfHealOutcome {
    Recreated,
    Restored,
    /// Refused because the host re-confirmed the control plane `Reachable`
    /// (`reach == Reachable`) — the two host-side observations DISAGREE (a flap,
    /// or the cluster came up between the classify and re-confirm probes). NOT a
    /// container-visibility or kubeconfig problem: `probe_apiserver_reachable` is
    /// host-side and reads no kubeconfig anywhere in this path (@observability
    /// F3/F5 — the definition site, kept explicitly negated because this exact
    /// wrong belief has regenerated in several artifacts).
    RecreateRefusedClusterAlive,
    /// Refused because the host could NOT determine the control-plane state
    /// (`reach == Unknown`, e.g. the container runtime is down / `inspect`
    /// errored) — nothing was confirmed, alive OR dead (Security F2). Distinct
    /// from `RecreateRefusedClusterAlive` because the two demand OPPOSITE
    /// operator actions (kubeconfig vs container-runtime), and this is the most
    /// likely refusal in practice.
    RecreateRefusedInconclusive,
    RecreateAlreadyAttemptedThisHelperLifetime,
    RecreateFailed,
    RestoreFailed,
}

/// Build the `{ "self_heal": { outcome, attempt, max, evidence } }` data payload
/// the client renders as `SELF_HEAL HELPER_OUTCOME=… ATTEMPT=<n>/<max>
/// EVIDENCE_LEAF=…`.
///
/// `max` is the LITERAL `1`, NOT a `SELF_HEAL_MAX_RECREATE` const: the bound is
/// the `AtomicBool` on `Context`, which permits exactly one transition, so `1`
/// is derived from the type's arity — a standalone const would be a decorative
/// second encoding that could drift (set it to 2 and the token reports `1/2`
/// while the code still enforces 1). Do NOT reintroduce a tunable const here.
fn self_heal_data(
    outcome: SelfHealOutcome,
    attempt: u8,
    evidence_leaf: &str,
) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "self_heal": {
            "outcome": outcome,
            "attempt": attempt,
            "max": 1,
            "evidence": evidence_leaf,
        }
    }))
}

/// Setup: allocate ports, generate kind-config, create cluster, run setup.sh.
fn cmd_setup(
    ctx: &Context,
    skip_observability: bool,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<Option<serde_json::Value>, HelperError> {
    eprintln!("[devloop-helper] setup: allocating ports...");
    let alloc = ports::allocate_ports(&ctx.slug, &ctx.registry_path)?;

    eprintln!(
        "[devloop-helper] setup: allocated base port {} (slot {})",
        alloc.base_port, alloc.slot_index
    );

    // Verify critical ports are available
    ports::verify_ports_available(&alloc)?;

    // Generate kind-config from template
    let template_path = ctx.project_root.join("infra/kind/kind-config.yaml.tmpl");
    let template = fs::read_to_string(&template_path).map_err(|e| HelperError::CommandFailed {
        cmd: "setup".to_string(),
        detail: format!("failed to read kind-config template: {e}"),
    })?;

    let gateway_ip = ctx
        .host_gateway_ip
        .as_deref()
        .unwrap_or(DEFAULT_HOST_GATEWAY_IP);
    validate_gateway_ip(gateway_ip)?;
    let mut vars = ports::template_env_vars(&alloc, gateway_ip);
    vars.insert("CLUSTER_NAME".to_string(), ctx.cluster_name.clone());

    let config_content = ports::substitute_template(&template, &vars);
    let config_path = ctx.runtime_dir.join("kind-config.yaml");
    {
        // not secret-bearing (port mappings + gateway IP) — ordinary write; the
        // atomic-secret-write helper (`fs_atomic`) is for credential files only.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&config_path)?;
        file.write_all(config_content.as_bytes())?;
        file.flush()?;
    }

    // Generate port map
    let host = CONTAINER_HOST;
    let host_fallback = "172.17.0.1";
    let port_map = ports::generate_port_map(
        &alloc,
        &ctx.cluster_name,
        host,
        host_fallback,
        !skip_observability,
    );
    let port_map_path = ctx.runtime_dir.join("ports.json");
    ports::write_port_map(&port_map_path, &port_map)?;

    // Create Kind cluster (skip if it already exists for idempotent setup)
    let (env_key, env_val) = ctx.container_runtime.kind_provider_env();
    let cluster_exists = cluster_already_exists(&ctx.cluster_name)?;
    if cluster_exists {
        eprintln!(
            "[devloop-helper] setup: Kind cluster '{}' already exists, reusing",
            ctx.cluster_name
        );
    } else {
        eprintln!(
            "[devloop-helper] setup: creating Kind cluster '{}'...",
            ctx.cluster_name
        );
        run_command_streaming(
            Command::new("kind")
                .arg("create")
                .arg("cluster")
                .arg("--config")
                .arg(&config_path)
                .arg("--name")
                .arg(&ctx.cluster_name)
                .env(env_key, env_val),
            "kind create cluster",
            writer,
            signal,
        )?;
    }

    // Generate DT_PORT_MAP file for setup.sh
    let port_map_shell_path = ctx.runtime_dir.join("port-map.env");
    write_port_map_shell(&port_map_shell_path, &alloc)?;

    // Run setup.sh
    // DT_HOST_GATEWAY_IP enables ConfigMap patching in setup.sh for devloop clusters
    // (advertise addresses use gateway IP + dynamic ports instead of localhost defaults).
    eprintln!("[devloop-helper] setup: running setup.sh...");
    let mut setup_cmd = Command::new(ctx.project_root.join("infra/kind/scripts/setup.sh"));
    setup_cmd
        .arg("--yes")
        .env("DT_CLUSTER_NAME", &ctx.cluster_name)
        .env("DT_PORT_MAP", &port_map_shell_path)
        .env("DT_HOST_GATEWAY_IP", gateway_ip)
        .env(env_key, env_val);
    if skip_observability {
        // TODO: setup.sh does not yet support --skip-observability;
        // once it does, this will suppress observability stack deployment.
        setup_cmd.arg("--skip-observability");
    }

    run_command_streaming(&mut setup_cmd, "setup.sh", writer, signal)?;

    // Generate kubeconfig for container access (ADR-0030/0031).
    // Rewrites the API server URL to the single `HOST_GATEWAY_IP:HOST_PORT_K8S_API`
    // binding (offset K8S_API). Shares the impl with `restore-kubeconfig`.
    generate_container_kubeconfig(ctx, alloc.port(PortOffsets::K8S_API))?;

    // Return port map as data
    let data = serde_json::to_value(&port_map).map_err(|e| HelperError::CommandFailed {
        cmd: "setup".to_string(),
        detail: format!("failed to serialize port map: {e}"),
    })?;

    Ok(Some(data))
}

/// Write a shell-sourceable port map file for setup.sh.
fn write_port_map_shell(path: &Path, alloc: &PortAllocation) -> Result<(), HelperError> {
    // not secret-bearing (port numbers) — ordinary write; the atomic-secret-write
    // helper (`fs_atomic`) is for credential files only.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;

    writeln!(file, "# Generated by devloop-helper")?;
    writeln!(file, "AC_HTTP_PORT={}", alloc.port(PortOffsets::AC_HTTP))?;
    writeln!(file, "GC_HTTP_PORT={}", alloc.port(PortOffsets::GC_HTTP))?;
    writeln!(
        file,
        "MH_HEALTH_PORT={}",
        alloc.port(PortOffsets::MH_0_HEALTH)
    )?;
    writeln!(file, "POSTGRES_PORT={}", alloc.port(PortOffsets::POSTGRES))?;
    writeln!(
        file,
        "PROMETHEUS_PORT={}",
        alloc.port(PortOffsets::PROMETHEUS)
    )?;
    writeln!(file, "GRAFANA_PORT={}", alloc.port(PortOffsets::GRAFANA))?;
    writeln!(file, "LOKI_PORT={}", alloc.port(PortOffsets::LOKI))?;
    writeln!(
        file,
        "MC_0_WEBTRANSPORT_PORT={}",
        alloc.port(PortOffsets::MC_0_WEBTRANSPORT)
    )?;
    writeln!(
        file,
        "MC_1_WEBTRANSPORT_PORT={}",
        alloc.port(PortOffsets::MC_1_WEBTRANSPORT)
    )?;
    writeln!(
        file,
        "MH_0_WEBTRANSPORT_PORT={}",
        alloc.port(PortOffsets::MH_0_WEBTRANSPORT)
    )?;
    writeln!(
        file,
        "MH_1_WEBTRANSPORT_PORT={}",
        alloc.port(PortOffsets::MH_1_WEBTRANSPORT)
    )?;

    file.flush()?;
    Ok(())
}

/// Rewrite kubeconfig `server:` URL for container access (ADR-0030).
///
/// The apiserver binds a SINGLE address, `HOST_GATEWAY_IP:HOST_PORT_K8S_API`
/// (`infra/kind/kind-config.yaml.tmpl`: `apiServerAddress`/`apiServerPort`).
/// There is NO 127.0.0.1 binding and NO separate `6443` extraPortMapping — the
/// old "two-port" design (a 127.0.0.1 apiServerPort plus a gateway extraPortMap)
/// never existed in the template, so this function targets the one real binding.
/// Kind emits `server: https://<gateway-ip>:<port>`; this replaces host+port with
/// the container-reachable `<target_host>:<target_port>`.
fn rewrite_kubeconfig_server(
    kubeconfig: &str,
    target_host: &str,
    target_port: u16,
) -> Result<String, HelperError> {
    // Replace `server: https://ANY_HOST:ANY_PORT` with `server: https://TARGET:PORT`.
    // Kind may put 127.0.0.1 or the gateway IP depending on apiServerAddress config.
    const PATTERN: &str = "server: https://";
    if !kubeconfig.contains(PATTERN) {
        return Err(HelperError::CommandFailed {
            cmd: "kubeconfig rewrite".to_string(),
            detail: "kubeconfig does not contain 'server: https://' pattern".to_string(),
        });
    }
    let mut result = String::with_capacity(kubeconfig.len());
    let mut remaining = kubeconfig;
    while let Some(pos) = remaining.find(PATTERN) {
        result.push_str(&remaining[..pos]);
        let after_pattern = &remaining[pos + PATTERN.len()..];
        // Skip old host:port (everything until whitespace/newline)
        let end = after_pattern
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_pattern.len());
        result.push_str(&format!("server: https://{target_host}:{target_port}"));
        remaining = &after_pattern[end..];
    }
    result.push_str(remaining);
    Ok(result)
}

/// Generate a kubeconfig file for use inside the dev container (ADR-0030/0031).
///
/// Runs `kind get kubeconfig`, rewrites the API server URL to
/// `https://<gateway-ip>:<k8s_api_port>` — the single `HOST_GATEWAY_IP:
/// HOST_PORT_K8S_API` binding — so the container reaches the Kind cluster's K8s
/// API through the host-gateway binding.
///
/// Takes the k8s_api port as a `u16` so setup + `restore-kubeconfig` share ONE
/// impl (@dry-reviewer): setup passes `alloc.port(PortOffsets::K8S_API)`;
/// `cmd_restore_kubeconfig` passes the port read from the LIVE `ports.json` (S4 —
/// never a fresh `allocate_ports`).
///
/// NOTE: this is the CONTAINER kubeconfig (`runtime_dir/kubeconfig`), distinct
/// from (a)'s host `$KUBECONFIG` written by `infra/kind/scripts/setup.sh`'s
/// `write_kubeconfig()` (:393-397). Host kubeconfig vs container kubeconfig — do
/// NOT collapse the two writers; they target different files for different
/// consumers.
fn generate_container_kubeconfig(ctx: &Context, k8s_api_port: u16) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] setup: generating container kubeconfig...");

    let (env_key, env_val) = ctx.container_runtime.kind_provider_env();
    let output = Command::new("kind")
        .arg("get")
        .arg("kubeconfig")
        .arg("--name")
        .arg(&ctx.cluster_name)
        .env(env_key, env_val)
        .output()
        .map_err(|e| HelperError::CommandFailed {
            cmd: "kind get kubeconfig".to_string(),
            detail: format!("failed to execute: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HelperError::CommandFailed {
            cmd: "kind get kubeconfig".to_string(),
            detail: format!("exit {}: {}", output.status, stderr.trim()),
        });
    }

    let kubeconfig = String::from_utf8_lossy(&output.stdout);
    // Use the gateway IP directly (not host.containers.internal) because the K8s API
    // server's TLS cert includes the IP as a SAN but not the DNS name.
    // HTTP services (AC, GC, etc.) can use host.containers.internal since they don't
    // do TLS cert validation.
    let gw_ip = ctx
        .host_gateway_ip
        .as_deref()
        .unwrap_or(DEFAULT_HOST_GATEWAY_IP);
    let kubeconfig = rewrite_kubeconfig_server(&kubeconfig, gw_ip, k8s_api_port)?;

    // Kubeconfig is cluster-admin credential material in the container-RW mount:
    // write it atomically (S3) via the ONE secret-write home. temp+rename carries
    // 0600 (no looser-mode preservation), is symlink-safe, and never leaves a
    // partial key.
    let kubeconfig_path = ctx.runtime_dir.join("kubeconfig");
    atomic_write_secret(&kubeconfig_path, kubeconfig.as_bytes())?;

    eprintln!(
        "[devloop-helper] setup: kubeconfig written to {}",
        kubeconfig_path.display()
    );

    Ok(())
}

/// Per-item timeout for evidence-bundle host commands (Security pt.3). Each
/// item is `timeout`-bounded so a wedged runtime — the very thing we're often
/// recovering from — cannot hang the capture into a silent, `SELF_HEAL`-less
/// stall.
const EVIDENCE_TIMEOUT_SECS: &str = "10";

/// The closed evidence-bundle item allowlist (item file names). The individual
/// `EV_*` consts are the production SSoT — `capture_evidence_bundle` writes each
/// bundle file through them, so a new item requires a new `EV_*` const + a writer
/// (@test B2 / @security). Do NOT add `kubectl config view`, `kind get
/// kubeconfig`, `-o yaml`, `describe`, or `kubectl logs` (see the
/// `capture_evidence_bundle` doc for why each is excluded).
const EV_META: &str = "meta.txt";
const EV_STATUS: &str = "status.json";
const EV_KIND_CLUSTERS: &str = "kind-clusters.txt";
const EV_CP_INSPECT: &str = "control-plane-inspect.txt";
const EV_PODS: &str = "pods.txt";
/// The `EV_*` set aggregated for the set-equality test's expected value. Built
/// from the SAME `EV_*` consts production writes through, so the test's real
/// capture (which writes via those consts) vs this set catches any drift in
/// either direction. `#[cfg(test)]` because production writes via the individual
/// consts, not this slice — so in the bin build the aggregate is genuinely unused
/// (marking it, not `#[allow(dead_code)]`, keeps that honest).
#[cfg(test)]
const EVIDENCE_BUNDLE_ITEMS: &[&str] =
    &[EV_META, EV_STATUS, EV_KIND_CLUSTERS, EV_CP_INSPECT, EV_PODS];

/// Current charged state of the recreate bound as the `ATTEMPT=n` numerator
/// (0 or 1). `max` is always 1 (the `AtomicBool`'s arity).
fn recreate_bound_charged(ctx: &Context) -> u8 {
    u8::from(ctx.recreate_bound.load(Ordering::SeqCst))
}

/// Write one evidence-bundle file (0600). Diagnostics, not secret-bearing;
/// best-effort (ordinary write — the atomic-secret-write helper is for
/// credential files only).
fn write_bundle_file(dir: &Path, filename: &str, bytes: &[u8]) -> std::io::Result<()> {
    let path = dir.join(filename);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

/// Run `timeout <n> <program> <args…>`, writing combined stdout+stderr into
/// `dir/filename`. Returns `Err(())` ONLY when the `timeout` binary itself
/// cannot spawn (⇒ the caller aborts the whole bundle with
/// `capture-failed:timeout-unavailable`); every other failure (the inner program
/// missing, non-zero, etc.) is written into the file as a diagnostic and is
/// non-fatal.
fn capture_cmd(dir: &Path, filename: &str, program: &str, args: &[&str]) -> Result<(), ()> {
    match Command::new("timeout")
        .arg(EVIDENCE_TIMEOUT_SECS)
        .arg(program)
        .args(args)
        .output()
    {
        Ok(out) => {
            let mut body = out.stdout;
            body.extend_from_slice(&out.stderr);
            let _ = write_bundle_file(dir, filename, &body);
            Ok(())
        }
        Err(_) => Err(()), // `timeout` not on PATH / unspawnable ⇒ abort bundle
    }
}

/// Capture the pre-destroy evidence bundle, HOST-SIDE, BEFORE `cmd_recreate`
/// destroys anything. Returns the bundle's LEAF name (`self-heal-evidence-<ts>`,
/// no `/`) which the shell composes into a container path, or
/// `capture-failed:<reason>` from the closed reason enum
/// (`bundle-dir-unwritable`, `timeout-unavailable`). NEVER aborts recovery —
/// capture failure only annotates the leaf.
///
/// CLOSED ALLOWLIST (P3 / Security R2 — do NOT extend without re-reviewing).
/// Items: `meta.txt` (timestamp, helper pid, cluster name, fresh reachability
/// verdict); `status.json` (a FRESH host-side status snapshot — this helper's own
/// observation at the moment of action, more trustworthy than the container's
/// triggering classification; embeds `ports.json`, which is not credentials);
/// `kind-clusters.txt` (`kind get clusters`); `control-plane-inspect.txt`
/// (`<runtime> inspect <cluster>-control-plane`, the S2 second observation
/// captured before deciding); and `pods.txt` (`kubectl get pods` DEFAULT TABLE
/// output only).
///
/// DELIBERATELY EXCLUDED: `kind get kubeconfig` and `kubectl config view [--raw]`
/// (cluster-admin `client-certificate-data`/`client-key-data`), `kubectl describe
/// pod` and `-o yaml`/`-o json` (container env var names + ConfigMap-inlined
/// values + secret references), and `kubectl logs` (can hang AND can carry pod
/// tokens). The realistic risk is a future contributor finding table output
/// insufficient and reaching for `describe`/`-o yaml` — that is why this list is
/// closed and named.
///
/// The dir is 0700 and files 0600 — that guards the bundle from OTHER host
/// users, NOT from the dev container (same uid, inside the RW mount). This is a
/// diagnostic aid, not a tamper-evident forensic record.
///
/// ANCHOR (DRY): the `capture-failed:<reason>` values this produces
/// (`bundle-dir-unwritable`, `timeout-unavailable`) are two of the closed
/// 5-member set mirrored in `scripts/layer7.sh::__self_heal_compose_evidence`
/// (which emits the other three and validates ALL of them) and enumerated in
/// `docs/runbooks/devloop-validation.md` §8. Add a member to all three together.
fn capture_evidence_bundle(ctx: &Context) -> String {
    let leaf = format!(
        "self-heal-evidence-{}",
        now_rfc3339().replace([':', '.'], "-")
    );
    let dir = ctx.runtime_dir.join(&leaf);
    if fs::DirBuilder::new().mode(0o700).create(&dir).is_err() {
        return "capture-failed:bundle-dir-unwritable".to_string();
    }

    // meta.txt — no subprocess, always safe.
    let meta = format!(
        "timestamp={}\nhelper_pid={}\ncluster={}\nreachability={}\n",
        now_rfc3339(),
        std::process::id(),
        ctx.cluster_name,
        probe_apiserver_reachable(ctx).as_str(),
    );
    let _ = write_bundle_file(&dir, EV_META, meta.as_bytes());

    // status.json — fresh host-side snapshot (best-effort).
    if let Ok(Some(status)) = cmd_status(ctx) {
        if let Ok(s) = serde_json::to_string_pretty(&status) {
            let _ = write_bundle_file(&dir, EV_STATUS, s.as_bytes());
        }
    }

    // Bounded host commands. A missing `timeout` binary is the one capture
    // failure that aborts the (subprocess-based) items — surface it as the enum
    // value; meta/status above are already written.
    let node = format!("{}-control-plane", ctx.cluster_name);
    let kube_ctx = format!("kind-{}", ctx.cluster_name);
    let runtime = ctx.container_runtime.as_str();
    let items: [(&str, &str, Vec<&str>); 3] = [
        (EV_KIND_CLUSTERS, "kind", vec!["get", "clusters"]),
        (
            EV_CP_INSPECT,
            runtime,
            // BOUNDED format (Security F1): capture ONLY the lifecycle fields
            // that answer why/when the control plane died — NOT the raw full
            // object, whose `Config.Env`/`HostConfig.Binds`/`Mounts` would
            // auto-capture anything that later lands in the kind node's env or
            // mounts (a registry cred, a proxy token) with no code change for
            // review to catch. Keeping this a format string keeps the item
            // bounded like its allowlist neighbours; do NOT restore the wildcard.
            vec![
                "inspect",
                "-f",
                "{{.State.Status}} {{.State.Running}} {{.State.ExitCode}} \
                 {{.State.OOMKilled}} {{.State.StartedAt}} {{.State.FinishedAt}}",
                node.as_str(),
            ],
        ),
        (
            EV_PODS,
            "kubectl",
            vec![
                "get",
                "pods",
                "-n",
                "dark-tower",
                "--context",
                kube_ctx.as_str(),
            ],
        ),
    ];
    for (filename, program, args) in &items {
        if capture_cmd(&dir, filename, program, args).is_err() {
            return "capture-failed:timeout-unavailable".to_string();
        }
    }

    leaf
}

/// The two-armed destroy license (R3-final). Extracted pure so the license
/// logic is unit-provable without running teardown/setup. `Err` (daemon down /
/// wedged `kind`) is NOT `Ok(false)`, so it does NOT license — the pin against a
/// future `unwrap_or(false)` that would make every daemon outage a destroy
/// license.
fn recreate_licensed(reach: ApiserverReachability, exists: &Result<bool, HelperError>) -> bool {
    reach == ApiserverReachability::Unreachable || matches!(exists, Ok(false))
}

/// The gate + bound decision, pure and hermetically testable WITHOUT running
/// teardown/setup (@test B1). Separating this from the IO makes both the LICENSE
/// (may we destroy at all) and the BOUND (have we already destroyed this
/// lifetime, and is it charged ONLY after a positive license) unit-provable.
#[derive(Debug, PartialEq, Eq)]
enum RecreateDecision {
    /// Refused: host re-confirmed the control plane Reachable (a raced/flap
    /// observation). Bound NOT charged.
    RefusedClusterAlive,
    /// Refused: host could not determine (Unknown / `exists == Err`). Bound NOT
    /// charged.
    RefusedInconclusive,
    /// Licensed, but the one recreate for this helper lifetime was already spent.
    /// Bound stays charged; NOT re-charged.
    AlreadyAttempted,
    /// Licensed and this call charged the bound — proceed to evidence + destroy.
    Proceed,
}

/// Decide whether to recreate, and charge the once-per-lifetime bound EXACTLY
/// when (and only when) a positive license lets us proceed.
///
/// Ordering is load-bearing (Delta 1, @test B1): the license check comes FIRST,
/// so a refusal NEVER charges the bound (a later legitimate `Unreachable` still
/// gets its one recreate); the `compare_exchange` runs only on the licensed
/// path, BEFORE the caller captures evidence or destroys — so a failed capture
/// or a failed destroy cannot buy a SECOND destroy.
fn decide_recreate(
    reach: ApiserverReachability,
    exists: &Result<bool, HelperError>,
    bound: &AtomicBool,
) -> RecreateDecision {
    if !recreate_licensed(reach, exists) {
        return if reach == ApiserverReachability::Reachable {
            RecreateDecision::RefusedClusterAlive
        } else {
            RecreateDecision::RefusedInconclusive
        };
    }
    if bound
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return RecreateDecision::AlreadyAttempted;
    }
    RecreateDecision::Proceed
}

/// `dev-cluster recreate` — the guarded, once-per-helper-lifetime cluster
/// recreate, and the ENFORCEMENT POINT of the whole self-heal safety argument at
/// the ADR-0030 trust boundary.
///
/// This runs on the PRIVILEGED host side and RE-CONFIRMS, at the moment of
/// action, that a destroy is licensed — the semi-trusted container only
/// *requested* it. The re-confirmation is TWO-armed (R3-final):
///   License 1: `probe_apiserver_reachable(ctx) == Unreachable` (control-plane
///              container reported stopped by `inspect` — an observation the
///              container cannot influence; the runtime socket is not mounted
///              into it, ADR-0030:468).
///   License 2: `cluster_already_exists() == Ok(false)` (cluster positively
///              ABSENT — nothing to destroy).
/// Anything else — including `cluster_already_exists() == Err` (a daemon outage,
/// which `inspect`'s exit code cannot distinguish from absence) — REFUSES.
///
/// LOAD-BEARING, do NOT "optimize away": this re-confirmation is not redundant
/// with the shell's probe. The shell classifies; the host RE-VERIFIES against an
/// observation the container cannot forge. Removing it collapses the safety
/// argument. Containment invariant: destroys ONLY `ctx.cluster_name` — there is
/// no client-supplied target arg (see `protocol::Request::reject_all_args`), so
/// the worst case is "one devloop destroys its OWN cluster" (the blast radius
/// devloop.sh:577-582 already accepts). The container's *assertion* is never the
/// authority; this gate + the containment invariant are.
///
/// NON-COLLAPSE (Finding-5 RECIPROCAL with `scripts/layer7.sh`'s infra-change
/// branch): that branch does an UNCONDITIONAL `teardown`+`setup` and MUST destroy
/// a HEALTHY cluster (the `infra/kind/` blueprint changed, so the running cluster
/// is stale by definition). This verb is its INVERSE — it MUST REFUSE to destroy
/// a healthy cluster. Two semantically-opposite operations; do NOT let a DRY pass
/// merge them (the forward half of this note lives at that shell branch).
fn cmd_recreate(
    ctx: &Context,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<Option<serde_json::Value>, HelperError> {
    // --- Host-side re-confirmation + bound (the enforcement point) -------------
    // `exists` consumes the `Result` DIRECTLY — `Err` (rc≠0 / wedged `kind`) MUST
    // stay distinguishable from `Ok(false)` (an `unwrap_or(false)` would turn
    // every daemon outage into a destroy license). The gate + bound decision is
    // pure (`decide_recreate`); it charges the bound ONLY on the licensed path,
    // after the re-confirm and before any destroy.
    let reach = probe_apiserver_reachable(ctx);
    let exists = cluster_already_exists(&ctx.cluster_name);
    match decide_recreate(reach, &exists, &ctx.recreate_bound) {
        RecreateDecision::RefusedClusterAlive | RecreateDecision::RefusedInconclusive => {
            // Split the refusal at the point the distinction EXISTS (Security F2):
            // `Reachable` is a POSITIVE "alive" observation (a raced re-confirm);
            // anything else (`Unknown` / `exists == Err`, runtime down) is "could
            // not determine", which demands the OPPOSITE operator action. The
            // bound is untouched, so a later legitimate `Unreachable` still heals.
            let outcome = if reach == ApiserverReachability::Reachable {
                SelfHealOutcome::RecreateRefusedClusterAlive
            } else {
                SelfHealOutcome::RecreateRefusedInconclusive
            };
            eprintln!(
                "[devloop-helper] recreate: REFUSED ({outcome:?}) — control plane not confirmed dead \
                 (reachability={}, cluster_exists={:?}); escalating, not destroying",
                reach.as_str(),
                exists
            );
            return Ok(self_heal_data(outcome, recreate_bound_charged(ctx), "none"));
        }
        RecreateDecision::AlreadyAttempted => {
            eprintln!(
                "[devloop-helper] recreate: bound already spent this helper lifetime; escalating"
            );
            return Ok(self_heal_data(
                SelfHealOutcome::RecreateAlreadyAttemptedThisHelperLifetime,
                1,
                "none",
            ));
        }
        RecreateDecision::Proceed => { /* licensed + bound charged — fall through */ }
    }

    // --- Evidence BEFORE the destroy -------------------------------------------
    let evidence_leaf = capture_evidence_bundle(ctx);
    // Honour the persisted observability-deploy SSoT BEFORE teardown wipes
    // ports.json (@code-reviewer): recreate the cluster with the SAME
    // observability choice it was originally brought up with, not a hardcoded
    // full stack.
    let skip_observability = !read_observability_deployed(ctx);
    eprintln!(
        "[devloop-helper] recreate: control plane confirmed down; evidence={evidence_leaf}; \
         recreating {} (skip_observability={skip_observability})",
        ctx.cluster_name
    );

    // --- Destroy + recreate (reuse cmd_teardown/cmd_setup) ---------------------
    // Called DIRECTLY (not via the dispatcher): they run inside the write slot
    // this command already holds; routing through `execute`/`run_with_write_slot`
    // would self-`Busy`. cmd_teardown swallows non-cancel kind errors internally,
    // so its `?` only propagates a genuine `Cancelled`.
    cmd_teardown(ctx, writer, signal)?;
    match cmd_setup(ctx, skip_observability, writer, signal) {
        Ok(_) => Ok(self_heal_data(
            SelfHealOutcome::Recreated,
            1,
            &evidence_leaf,
        )),
        Err(e @ HelperError::Cancelled { .. }) => Err(e),
        Err(e) => {
            eprintln!("[devloop-helper] recreate: setup after teardown failed: {e}");
            Ok(self_heal_data(
                SelfHealOutcome::RecreateFailed,
                1,
                &evidence_leaf,
            ))
        }
    }
}

/// `dev-cluster restore-kubeconfig` — regenerate ONLY the container kubeconfig
/// from the LIVE cluster. NON-DESTRUCTIVE: reaches no teardown/delete/create
/// path (this is the whole point — a stale/missing kubeconfig on a HEALTHY
/// cluster must never trigger a destroy). NOT bound-gated.
///
/// Refuses loudly (S4/P6) — explicit early return, never a fallthrough to
/// `allocate_ports` — if the cluster is absent or `ports.json` has no k8s_api
/// port. The port comes from the persisted live port map ONLY: a restarted
/// helper re-allocating could hand a different slot and point a cluster-admin
/// kubeconfig at another devloop's apiserver.
fn cmd_restore_kubeconfig(
    ctx: &Context,
    _writer: &mut dyn Write,
    _signal: &CancelSignal,
) -> Result<Option<serde_json::Value>, HelperError> {
    let attempt = recreate_bound_charged(ctx); // report the actual bound state
    match cluster_already_exists(&ctx.cluster_name) {
        Ok(true) => {}
        other => {
            eprintln!(
                "[devloop-helper] restore-kubeconfig: refusing — cluster not present ({other:?})"
            );
            return Ok(self_heal_data(
                SelfHealOutcome::RestoreFailed,
                attempt,
                "none",
            ));
        }
    }
    let port = match read_k8s_api_port(ctx) {
        Some(p) => p,
        None => {
            eprintln!(
                "[devloop-helper] restore-kubeconfig: refusing — ports.json missing/unparseable \
                 or no k8s_api port (NOT re-allocating)"
            );
            return Ok(self_heal_data(
                SelfHealOutcome::RestoreFailed,
                attempt,
                "none",
            ));
        }
    };
    match generate_container_kubeconfig(ctx, port) {
        Ok(()) => Ok(self_heal_data(SelfHealOutcome::Restored, attempt, "none")),
        Err(e) => {
            eprintln!("[devloop-helper] restore-kubeconfig: kubeconfig regen failed: {e}");
            Ok(self_heal_data(
                SelfHealOutcome::RestoreFailed,
                attempt,
                "none",
            ))
        }
    }
}

/// Summary of pod health in the dark-tower namespace.
#[derive(Debug, serde::Serialize)]
struct PodHealthSummary {
    total: usize,
    ready: usize,
    not_ready: Vec<PodStatus>,
}

/// Status of a single pod.
#[derive(Debug, serde::Serialize)]
struct PodStatus {
    name: String,
    phase: String,
    ready: bool,
}

/// Parse kubectl `get pods -o json` output into a health summary.
///
/// Pure function — takes raw JSON string, returns structured summary.
/// Designed for unit testing without a real cluster.
fn parse_pod_health(json_str: &str) -> Result<PodHealthSummary, String> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("invalid JSON: {e}"))?;

    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'items' array".to_string())?;

    let mut total = 0;
    let mut ready_count = 0;
    let mut not_ready = Vec::new();

    for item in items {
        let name = item
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let phase = item
            .pointer("/status/phase")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");

        // A pod is ready if phase is Running AND all containers have ready=true
        let containers_ready = item
            .pointer("/status/containerStatuses")
            .and_then(|v| v.as_array())
            .map(|statuses| {
                !statuses.is_empty()
                    && statuses
                        .iter()
                        .all(|s| s.get("ready").and_then(|r| r.as_bool()).unwrap_or(false))
            })
            .unwrap_or(false);

        let is_ready = phase == "Running" && containers_ready;

        total += 1;
        if is_ready {
            ready_count += 1;
        } else {
            not_ready.push(PodStatus {
                name: name.to_string(),
                phase: phase.to_string(),
                ready: false,
            });
        }
    }

    Ok(PodHealthSummary {
        total,
        ready: ready_count,
        not_ready,
    })
}

/// Status: read-only health check — cluster exists, pods healthy, ports.json.
///
/// Snapshots `WriteState` at request time (per Security S4) so the busy hint
/// is consistent: lock held only for the field clones, dropped before the
/// kubectl call. Status NEVER blocks on the write mutex.
fn cmd_status(ctx: &Context) -> Result<Option<serde_json::Value>, HelperError> {
    eprintln!("[devloop-helper] status: checking cluster health...");

    // 0. Snapshot busy hint BEFORE any blocking work (lock dropped immediately).
    let busy = ctx.snapshot_busy();

    // 1. Check if Kind cluster exists
    let cluster_exists = match cluster_already_exists(&ctx.cluster_name) {
        Ok(exists) => exists,
        Err(e) => {
            eprintln!("[devloop-helper] status: cluster check failed: {e}");
            false
        }
    };

    // 2. Check pod health (only if cluster exists)
    let (pods_healthy, pod_summary, pod_error) = if cluster_exists {
        let kubectl_ctx = format!("kind-{}", ctx.cluster_name);
        match Command::new("kubectl")
            .arg("get")
            .arg("pods")
            .arg("-n")
            .arg("dark-tower")
            .arg("--context")
            .arg(&kubectl_ctx)
            .arg("-o")
            .arg("json")
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                match parse_pod_health(&stdout) {
                    Ok(summary) => {
                        let healthy = summary.not_ready.is_empty() && summary.total > 0;
                        (healthy, Some(summary), None)
                    }
                    Err(e) => (false, None, Some(format!("pod health parse error: {e}"))),
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                (
                    false,
                    None,
                    Some(format!("kubectl failed: {}", stderr.trim())),
                )
            }
            Err(e) => (false, None, Some(format!("kubectl spawn failed: {e}"))),
        }
    } else {
        (false, None, None)
    };

    // 3. Read ports.json if available
    let ports_path = ctx.runtime_dir.join("ports.json");
    let ports = if ports_path.exists() {
        match fs::read_to_string(&ports_path) {
            Ok(contents) => serde_json::from_str::<serde_json::Value>(&contents).ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    // 4. Setup-in-progress is true iff a setup write is currently holding the
    // mutex. Sourcing from the mutex (instead of the legacy `setup.pid` heuristic
    // owned by devloop.sh's eager-setup wrapper) means the field reflects what
    // the helper actually sees in flight — including manual `dev-cluster setup`
    // invocations that never wrote a setup.pid. The `setup.pid` file still
    // exists and is still maintained by devloop.sh for its own wrapper-lifecycle
    // tracking; cmd_status simply doesn't consult it any more.
    // Vocabulary matched by literal op name. `recreate` calls `cmd_setup`
    // INTERNALLY, so a status served mid-`recreate` (reads bypass the write
    // mutex) would otherwise report `setup_in_progress=false` while a genuine
    // setup runs inside it — and `Cluster exists: false` + `Setup in progress:
    // false` is dispatch row 1 (`apiserver-unreachable`), i.e. it fails toward a
    // destroy. Both write ops that run setup must be in this set; adding a THIRD
    // verb that runs setup requires revisiting this matcher.
    let setup_in_progress = busy
        .as_ref()
        .is_some_and(|b| matches!(b.op.as_str(), "setup" | "recreate"));

    // Build response data — construct fully to avoid indexing (clippy::indexing_slicing)
    let pod_summary_val = pod_summary.map(|summary| {
        serde_json::json!({
            "total": summary.total,
            "ready": summary.ready,
            "not_ready": summary.not_ready,
        })
    });

    // Per CR11: busy/in_flight always emitted (no skip_serializing_if), so
    // newer clients can distinguish "old helper, field absent" from "new
    // helper, idle" deterministically. `cancel_pending` follows the same
    // wire-determinism rule and is emitted at the top level (not nested
    // under in_flight) so the idle case has nowhere awkward for it to live.
    let in_flight_val = busy.as_ref().map(|b| {
        serde_json::json!({
            "op": b.op,
            "args": b.args,
            "started_at": b.started_at,
        })
    });
    let cancel_pending = busy.as_ref().is_some_and(|b| b.cancel_pending);

    // Apiserver reachability (self-heal input). ADDITIVE string field — the three
    // COUPLED health lines + NDJSON shape are untouched, and a probe failure
    // degrades to `unknown` (never errors), so `__cluster_ready` (which greps
    // only the three) is unaffected. The probe is internally `timeout`-bounded;
    // its total bound stays under the shell `timeout` on `dev-cluster status`
    // (see INSPECT_TIMEOUT_SECS / TCP_CONNECT_TIMEOUT).
    let apiserver_reachable = probe_apiserver_reachable(ctx).as_str();

    let data = serde_json::json!({
        "cluster_exists": cluster_exists,
        "pods_healthy": pods_healthy,
        "apiserver_reachable": apiserver_reachable,
        "setup_in_progress": setup_in_progress,
        "checked_at": now_rfc3339(),
        "pod_summary": pod_summary_val,
        "pod_error": pod_error,
        "ports": ports,
        "busy": busy.is_some(),
        "cancel_pending": cancel_pending,
        "in_flight": in_flight_val,
    });

    Ok(Some(data))
}

/// Rebuild: build one service image, load into Kind, restart deployment.
fn cmd_rebuild(
    ctx: &Context,
    svc: Service,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] rebuild: building {}...", svc);

    // Build image
    run_command_streaming(
        Command::new(ctx.container_runtime.as_str())
            .arg("build")
            .arg("--build-arg")
            .arg(format!("CARGO_BUILD_JOBS={}", ctx.cargo_build_jobs))
            .arg("-t")
            .arg(svc.image_tag())
            .arg("-f")
            .arg(svc.dockerfile())
            .arg(&ctx.project_root),
        &format!("{} build {}", ctx.container_runtime.as_str(), svc),
        writer,
        signal,
    )?;

    // Load into Kind
    load_image_to_kind(ctx, svc.image_tag(), writer, signal)?;

    // Restart deployment
    restart_deployment(ctx, svc, writer, signal)?;

    Ok(())
}

/// Rebuild all service images.
fn cmd_rebuild_all(
    ctx: &Context,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    for svc in &Service::ALL {
        cmd_rebuild(ctx, *svc, writer, signal)?;
    }
    Ok(())
}

/// Deploy: apply manifests only via setup.sh --skip-build --only.
fn cmd_deploy(
    ctx: &Context,
    svc: Service,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] deploy: applying manifests for {}...", svc);

    let (env_key, env_val) = ctx.container_runtime.kind_provider_env();

    // Validate and pass gateway IP so setup.sh can patch ConfigMap advertise addresses.
    let gateway_ip = ctx
        .host_gateway_ip
        .as_deref()
        .unwrap_or(DEFAULT_HOST_GATEWAY_IP);
    validate_gateway_ip(gateway_ip)?;

    let port_map_shell_path = ctx.runtime_dir.join("port-map.env");

    run_command_streaming(
        Command::new(ctx.project_root.join("infra/kind/scripts/setup.sh"))
            .arg("--yes")
            .arg("--skip-build")
            .arg("--only")
            .arg(svc.as_str())
            .env("DT_CLUSTER_NAME", &ctx.cluster_name)
            .env("DT_PORT_MAP", &port_map_shell_path)
            .env("DT_HOST_GATEWAY_IP", gateway_ip)
            .env(env_key, env_val),
        &format!("setup.sh --skip-build --only {svc}"),
        writer,
        signal,
    )?;

    Ok(())
}

/// Teardown: delete Kind cluster, clean up all state.
fn cmd_teardown(
    ctx: &Context,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    eprintln!(
        "[devloop-helper] teardown: deleting cluster '{}'...",
        ctx.cluster_name
    );

    let (env_key, env_val) = ctx.container_runtime.kind_provider_env();

    // Delete Kind cluster (idempotent — succeeds even if cluster doesn't exist)
    let result = run_command_streaming(
        Command::new("kind")
            .arg("delete")
            .arg("cluster")
            .arg("--name")
            .arg(&ctx.cluster_name)
            .env(env_key, env_val),
        "kind delete cluster",
        writer,
        signal,
    );

    propagate_teardown_kind_result(result)?;

    // Remove from port registry
    if let Err(e) = ports::deallocate_ports(&ctx.slug, &ctx.registry_path) {
        eprintln!("[devloop-helper] teardown: port deallocation warning: {e}");
    }

    Ok(())
}

/// Decide whether `kind delete cluster`'s outcome should abort teardown or be
/// swallowed so port-deallocation can proceed. `Cancelled` MUST propagate —
/// port-deallocation is destructive, and the audit log / outcome must reflect
/// that the op was cancelled, not completed. Every other error is logged and
/// swallowed so the user can recover from a partial Kind state.
fn propagate_teardown_kind_result(result: Result<(), HelperError>) -> Result<(), HelperError> {
    match result {
        Err(e @ HelperError::Cancelled { .. }) => Err(e),
        Err(ref e) => {
            eprintln!("[devloop-helper] teardown: kind delete cluster warning: {e}");
            Ok(())
        }
        Ok(()) => Ok(()),
    }
}

/// Load a container image into the Kind cluster.
fn load_image_to_kind(
    ctx: &Context,
    image_tag: &str,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    let (env_key, env_val) = ctx.container_runtime.kind_provider_env();

    match ctx.container_runtime {
        ContainerRuntime::Podman => {
            // Podman requires save/load workaround
            let tmp_path = ctx.runtime_dir.join("kind-image-load.tar");
            run_command_streaming(
                Command::new("podman")
                    .arg("save")
                    .arg(image_tag)
                    .arg("-o")
                    .arg(&tmp_path),
                &format!("podman save {image_tag}"),
                writer,
                signal,
            )?;
            let result = run_command_streaming(
                Command::new("kind")
                    .arg("load")
                    .arg("image-archive")
                    .arg(&tmp_path)
                    .arg("--name")
                    .arg(&ctx.cluster_name)
                    .env(env_key, env_val),
                "kind load image-archive",
                writer,
                signal,
            );
            let _ = fs::remove_file(&tmp_path);
            result?;
        }
        ContainerRuntime::Docker => {
            run_command_streaming(
                Command::new("kind")
                    .arg("load")
                    .arg("docker-image")
                    .arg(image_tag)
                    .arg("--name")
                    .arg(&ctx.cluster_name)
                    .env(env_key, env_val),
                "kind load docker-image",
                writer,
                signal,
            )?;
        }
    }
    Ok(())
}

/// Restart the deployment(s) for a service.
fn restart_deployment(
    ctx: &Context,
    svc: Service,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    let kubectl_ctx = format!("kind-{}", ctx.cluster_name);

    match svc {
        Service::Ac => {
            run_command_streaming(
                Command::new("kubectl")
                    .arg("--context")
                    .arg(&kubectl_ctx)
                    .arg("rollout")
                    .arg("restart")
                    .arg("statefulset/ac-service")
                    .arg("-n")
                    .arg("dark-tower"),
                "kubectl rollout restart ac-service",
                writer,
                signal,
            )?;
        }
        Service::Gc => {
            run_command_streaming(
                Command::new("kubectl")
                    .arg("--context")
                    .arg(&kubectl_ctx)
                    .arg("rollout")
                    .arg("restart")
                    .arg("deployment/gc-service")
                    .arg("-n")
                    .arg("dark-tower"),
                "kubectl rollout restart gc-service",
                writer,
                signal,
            )?;
        }
        Service::Mc => {
            for i in 0..2 {
                run_command_streaming(
                    Command::new("kubectl")
                        .arg("--context")
                        .arg(&kubectl_ctx)
                        .arg("rollout")
                        .arg("restart")
                        .arg(format!("deployment/mc-{i}"))
                        .arg("-n")
                        .arg("dark-tower"),
                    &format!("kubectl rollout restart mc-{i}"),
                    writer,
                    signal,
                )?;
            }
        }
        Service::Mh => {
            for i in 0..2 {
                run_command_streaming(
                    Command::new("kubectl")
                        .arg("--context")
                        .arg(&kubectl_ctx)
                        .arg("rollout")
                        .arg("restart")
                        .arg(format!("deployment/mh-{i}"))
                        .arg("-n")
                        .arg("dark-tower"),
                    &format!("kubectl rollout restart mh-{i}"),
                    writer,
                    signal,
                )?;
            }
        }
    }

    Ok(())
}

/// Check if a Kind cluster with the given name already exists.
fn cluster_already_exists(cluster_name: &str) -> Result<bool, HelperError> {
    let output = Command::new("kind")
        .arg("get")
        .arg("clusters")
        .output()
        .map_err(|e| HelperError::CommandFailed {
            cmd: "kind get clusters".to_string(),
            detail: format!("failed to execute: {e}"),
        })?;

    if !output.status.success() {
        return Err(HelperError::CommandFailed {
            cmd: "kind get clusters".to_string(),
            detail: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|line| line.trim() == cluster_name))
}

/// Read lines from a pipe and send them as `StreamMsg` values to the channel.
///
/// This is the shared reader function used by both stdout and stderr threads.
/// It never panics — all IO errors are caught and logged.
fn pipe_reader(
    reader: impl BufRead + Send,
    kind: StreamKind,
    sender: mpsc::Sender<StreamMsg>,
) -> Result<(), String> {
    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                let truncated = crate::protocol::truncate_line(line, MAX_LINE_LEN);
                let msg = StreamLine {
                    stream: kind,
                    line: truncated,
                    ts: now_rfc3339(),
                };
                if sender.send(StreamMsg::Line(msg)).is_err() {
                    // Receiver dropped — main thread is shutting down
                    break;
                }
            }
            Err(e) => {
                // Pipe closed or read error — child is done with this stream
                if e.kind() != ErrorKind::BrokenPipe {
                    eprintln!("[devloop-helper] pipe read error: {e}");
                }
                break;
            }
        }
    }
    let _ = sender.send(StreamMsg::Done);
    Ok(())
}

/// Run a command, streaming stdout/stderr line-by-line to the client writer.
///
/// Uses an mpsc channel: two reader threads send `StreamMsg` values, and this
/// function (on the calling thread) receives them and writes JSON lines to the
/// writer. The calling thread checks the `shutdown` flag for SIGTERM and the
/// per-write cancel token for `cancel`-driven termination.
///
/// Cancel/shutdown send signals to the child's *process group* (negative-pgid
/// `libc::kill`) so they reach grandchildren that inherited stdout/stderr —
/// e.g. `kubectl wait` spawned by `setup.sh`. Without this, an immediate-child
/// SIGKILL would leave grandchildren holding the pipes open, blocking the
/// reader threads on `read()` until the orphan exited (~60 s in practice).
/// The child is spawned with `Command::process_group(0)` so it becomes the
/// leader of a new group with `pgid == child.id()`; helper's own pgid is
/// unaffected.
///
/// SAFETY: Child processes must not receive the auth token in their environment.
/// Only DT_CLUSTER_NAME, DT_PORT_MAP (file path), and KIND_EXPERIMENTAL_PROVIDER
/// are passed. The auth token stays in the helper process only.
fn run_command_streaming(
    cmd: &mut Command,
    description: &str,
    writer: &mut dyn Write,
    signal: &CancelSignal,
) -> Result<(), HelperError> {
    // Configure the child to call setpgid(0, 0) post-fork pre-exec, making
    // it the leader of a new group with pgid == child.id(). Helper's own
    // pgid is unaffected. We then signal -pgid to reach grandchildren that
    // inherit our stdout/stderr pipes (kubectl wait, etc.).
    cmd.process_group(0);
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HelperError::CommandFailed {
            cmd: description.to_string(),
            detail: format!("failed to execute: {e}"),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HelperError::CommandFailed {
            cmd: description.to_string(),
            detail: "failed to capture stdout".to_string(),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| HelperError::CommandFailed {
            cmd: description.to_string(),
            detail: "failed to capture stderr".to_string(),
        })?;

    let (sender, receiver) = mpsc::channel::<StreamMsg>();

    // Spawn stdout reader thread
    let stdout_sender = sender.clone();
    let stdout_handle = std::thread::spawn(move || {
        pipe_reader(BufReader::new(stdout), StreamKind::Out, stdout_sender)
    });

    // Spawn stderr reader thread
    let stderr_handle =
        std::thread::spawn(move || pipe_reader(BufReader::new(stderr), StreamKind::Err, sender));

    // Main loop: receive stream messages and write to client.
    let mut done_count = 0u8;
    let mut write_failed = false;
    let mut child_killed = false;
    // Cancellation tracking. When the per-write cancel token (NOT process
    // shutdown) is set, we send SIGTERM, wait up to CANCEL_GRACEFUL_TIMEOUT,
    // then escalate to SIGKILL. `cancel_termed` records that we sent SIGTERM
    // (so we don't re-send), `term_at` records when, `escalated` records
    // whether SIGKILL was needed.
    let mut cancel_termed = false;
    let mut term_at: Option<Instant> = None;
    let mut escalated = false;

    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(StreamMsg::Line(stream_line)) => {
                if !write_failed {
                    if let Err(e) = send_json_line(writer, &stream_line) {
                        eprintln!("[devloop-helper] stream write failed: {e}");
                        write_failed = true;
                        // Broken-pipe path: the client disconnected. Process-group SIGKILL —
                        // same mechanic as process-shutdown — to ensure grandchildren don't
                        // hold pipes open and wedge the reader threads. No graceful-grace
                        // window (client is gone; no need to be polite); this is strictly
                        // more aggressive than cancel, not less.
                        if !child_killed {
                            if let Err(e) = signal_process_group(&child, libc::SIGKILL) {
                                eprintln!("[devloop-helper] signal_process_group broken-pipe fallback: {e}");
                                let _ = child.kill();
                            }
                            child_killed = true;
                        }
                    }
                }
                // If write failed, keep draining the channel so reader threads aren't blocked
            }
            Ok(StreamMsg::Done) => {
                done_count += 1;
                if done_count >= 2 {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Both senders dropped — threads exited
                break;
            }
        }

        // IMPORTANT: cancel/shutdown kill semantics (per CR12 + Security S5).
        //
        // 1. WHAT GETS KILLED: the whole process group of the write child —
        //    `setup.sh` plus its kubectl/kind/shell descendants. We do NOT
        //    reach the Kind cluster's containerd/kubelet/etcd: those run
        //    inside Kind's container as detached components managed by the
        //    cluster's PID 1, not as descendants of setup.sh, and were
        //    never in our process group. (Iter-1's framing claimed they
        //    were children of setup.sh — that was wrong; iter-2 corrects
        //    the model and the 2 s grace window's justification.)
        //
        // 2. WHY PROCESS-GROUP, NOT IMMEDIATE-CHILD: grandchildren inherit
        //    stdout/stderr from the bash/setup.sh leader. Killing only the
        //    leader leaves orphan grandchildren holding the pipes open;
        //    `read()` on the reader threads then blocks until the orphans
        //    exit naturally (~60 s observed in pre-iter-2 dev-cluster
        //    cancel). pgid-signalling reaches the whole shell tree, the
        //    pipes close, the reader threads see EOF, and cancel completes
        //    promptly.
        //
        // 3. CANCEL vs SHUTDOWN: same kill mechanic (process-group signal),
        //    different timing.
        //    - Cancel-token (per-write, set by `cmd_cancel`): SIGTERM to
        //      the group, CANCEL_GRACEFUL_TIMEOUT (2 s) grace, then
        //      escalate to SIGKILL of the group. The 2 s window matches
        //      the cooperative shell/kubectl tree's typical SIGTERM
        //      response. Partial-Kind state is recovered via setup.sh's
        //      "cluster exists, reusing" branch on the next setup.
        //    - Process-shutdown (helper-wide, set by SIGTERM/SIGINT to
        //      the helper): SIGKILL of the group immediately. We're going
        //      down; no time for a 2 s grace window.
        if !child_killed {
            if signal.cancel.load(Ordering::Relaxed) && !cancel_termed {
                // Sole signaller of `child` lives on this stack; PID cannot
                // be recycled because we have not called wait() (Security
                // S3-(a)). Signal the process group so kubectl-and-friends
                // grandchildren are reached.
                match signal_process_group(&child, libc::SIGTERM) {
                    Ok(()) => {
                        cancel_termed = true;
                        term_at = Some(Instant::now());
                    }
                    Err(e) => {
                        // Pathological PID (>i32::MAX or i32::MIN) — fall back
                        // to single-PID SIGKILL of the leader. Best effort;
                        // any grandchildren leak until next sweep.
                        eprintln!("[devloop-helper] signal_process_group failed, falling back to child.kill(): {e}");
                        let _ = child.kill();
                        child_killed = true;
                        escalated = true;
                    }
                }
            } else if cancel_termed {
                // Check escalation window.
                if let Some(t0) = term_at {
                    if t0.elapsed() >= CANCEL_GRACEFUL_TIMEOUT {
                        // Child still alive after SIGTERM grace window — escalate.
                        match child.try_wait() {
                            Ok(Some(_)) => {
                                // Child exited cleanly within the window.
                                child_killed = true;
                            }
                            Ok(None) | Err(_) => {
                                if let Err(e) = signal_process_group(&child, libc::SIGKILL) {
                                    eprintln!("[devloop-helper] signal_process_group SIGKILL fallback: {e}");
                                    let _ = child.kill();
                                }
                                child_killed = true;
                                escalated = true;
                            }
                        }
                    } else if let Ok(Some(_)) = child.try_wait() {
                        // Child exited gracefully before the window expired.
                        child_killed = true;
                    }
                }
            } else if signal.shutdown.load(Ordering::Relaxed) {
                // Process shutdown — SIGKILL-immediate (asymmetry, see above).
                // Signal the group so grandchildren don't outlive the helper.
                if let Err(e) = signal_process_group(&child, libc::SIGKILL) {
                    eprintln!("[devloop-helper] signal_process_group shutdown fallback: {e}");
                    let _ = child.kill();
                }
                child_killed = true;
            }
        }
    }

    // Wait for child to exit (reap zombie). After kill, this returns immediately.
    let status = child.wait().map_err(|e| HelperError::CommandFailed {
        cmd: description.to_string(),
        detail: format!("failed to wait for child: {e}"),
    })?;

    // Join reader threads — child exit closed the pipes, so threads see EOF promptly
    match stdout_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(msg)) => eprintln!("[devloop-helper] stdout reader error: {msg}"),
        Err(_) => eprintln!("[devloop-helper] stdout reader thread panicked"),
    }
    match stderr_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(msg)) => eprintln!("[devloop-helper] stderr reader error: {msg}"),
        Err(_) => eprintln!("[devloop-helper] stderr reader thread panicked"),
    }

    if write_failed {
        return Err(HelperError::CommandFailed {
            cmd: description.to_string(),
            detail: "stream write failed: client disconnected".to_string(),
        });
    }

    // If the cancel token fired, return Cancelled (not generic CommandFailed)
    // so the audit log carries the prefix-`cancelled` invariant.
    if signal.cancel_set() {
        return Err(HelperError::Cancelled { escalated });
    }

    if !status.success() {
        let mut detail = format!("exit code: {}", status);
        if child_killed {
            detail.push_str(" (killed due to SIGTERM)");
        }
        return Err(HelperError::CommandFailed {
            cmd: description.to_string(),
            detail,
        });
    }

    Ok(())
}

/// Send a serializable value as a JSON line to the writer.
///
/// Each line is flushed immediately after writing to ensure the client
/// sees output in real time. Without flush, the client would see nothing
/// until the buffer fills or the child exits.
pub fn send_json_line(
    writer: &mut dyn Write,
    value: &impl serde::Serialize,
) -> std::io::Result<()> {
    let json = serde_json::to_string(value).map_err(std::io::Error::other)?;
    writeln!(writer, "{json}")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CommandStarted, StreamKind, StreamLine};

    /// Helper: run a command via run_command_streaming, collecting all output
    /// written to the writer as individual JSON lines.
    fn collect_streaming_output(args: &[&str]) -> (Result<(), HelperError>, Vec<String>) {
        let signal = CancelSignal::shutdown_only(Arc::new(AtomicBool::new(false)));
        let mut output = Vec::new();
        let result = run_command_streaming(
            Command::new(args[0]).args(&args[1..]),
            "test command",
            &mut output,
            &signal,
        );
        let lines: Vec<String> = output
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| String::from_utf8_lossy(l).to_string())
            .collect();
        (result, lines)
    }

    /// Parse collected lines into StreamLines and find the stream lines by kind.
    fn parse_stream_lines(lines: &[String]) -> Vec<StreamLine> {
        lines
            .iter()
            .filter_map(|l| serde_json::from_str::<StreamLine>(l).ok())
            .collect()
    }

    #[test]
    fn test_streaming_stdout_lines() {
        let (result, lines) =
            collect_streaming_output(&["/bin/sh", "-c", "echo line1; echo line2; echo line3"]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        let out_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Out)
            .map(|l| l.line.as_str())
            .collect();
        assert_eq!(out_lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_streaming_stderr_lines() {
        let (result, lines) =
            collect_streaming_output(&["/bin/sh", "-c", "echo err1 >&2; echo err2 >&2"]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        let err_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Err)
            .map(|l| l.line.as_str())
            .collect();
        assert_eq!(err_lines, vec!["err1", "err2"]);
    }

    #[test]
    fn test_streaming_interleaved_stdout_stderr() {
        // Interleaving is non-deterministic between threads, so we check
        // sets of lines by kind, not ordering between kinds.
        let (result, lines) = collect_streaming_output(&[
            "/bin/sh",
            "-c",
            "echo out1; echo err1 >&2; echo out2; echo err2 >&2",
        ]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        let mut out_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Out)
            .map(|l| l.line.as_str())
            .collect();
        let mut err_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Err)
            .map(|l| l.line.as_str())
            .collect();
        out_lines.sort();
        err_lines.sort();
        assert_eq!(out_lines, vec!["out1", "out2"]);
        assert_eq!(err_lines, vec!["err1", "err2"]);
    }

    #[test]
    fn test_streaming_empty_output() {
        let (result, lines) = collect_streaming_output(&["/bin/true"]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        assert!(
            stream_lines.is_empty(),
            "expected zero stream lines for /bin/true, got {}",
            stream_lines.len()
        );
    }

    #[test]
    fn test_streaming_nonzero_exit() {
        let (result, lines) = collect_streaming_output(&["/bin/sh", "-c", "echo partial; exit 42"]);
        assert!(result.is_err());

        // Verify the partial output was still streamed
        let stream_lines = parse_stream_lines(&lines);
        let out_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Out)
            .map(|l| l.line.as_str())
            .collect();
        assert!(
            out_lines.contains(&"partial"),
            "expected 'partial' in output"
        );

        // Verify the error mentions exit code
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("exit code"),
            "expected exit code in error: {err_str}"
        );
    }

    #[test]
    fn test_streaming_signal_death() {
        let (result, lines) =
            collect_streaming_output(&["/bin/sh", "-c", "echo before; kill -9 $$"]);
        assert!(result.is_err());

        // The "before" line should have been streamed
        let stream_lines = parse_stream_lines(&lines);
        let out_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Out)
            .map(|l| l.line.as_str())
            .collect();
        assert!(out_lines.contains(&"before"), "expected 'before' in output");
    }

    #[test]
    fn test_streaming_nonexistent_binary() {
        let signal = CancelSignal::shutdown_only(Arc::new(AtomicBool::new(false)));
        let mut output = Vec::new();
        let result = run_command_streaming(
            &mut Command::new("/nonexistent/binary/that/does/not/exist"),
            "nonexistent",
            &mut output,
            &signal,
        );
        assert!(result.is_err());

        // No stream lines should have been written
        assert!(
            output.is_empty(),
            "expected no output for nonexistent binary"
        );

        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("failed to execute"),
            "expected spawn failure: {}",
            err
        );
    }

    #[test]
    fn test_streaming_line_truncation() {
        // Generate a line that exceeds MAX_LINE_LEN (64KB)
        let long_len = MAX_LINE_LEN + 1000;
        let cmd = format!("printf '%0{long_len}d' 0");
        let (result, lines) = collect_streaming_output(&["/bin/sh", "-c", &cmd]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        assert!(
            !stream_lines.is_empty(),
            "expected at least one stream line"
        );
        let line = &stream_lines[0].line;
        assert!(
            line.ends_with("[truncated]"),
            "expected truncation marker, got line of len {}",
            line.len()
        );
        // The truncated line should be around MAX_LINE_LEN + " [truncated]".len()
        assert!(
            line.len() <= MAX_LINE_LEN + 20,
            "truncated line too long: {}",
            line.len()
        );
    }

    #[test]
    fn test_streaming_line_under_limit() {
        // Generate a line just under MAX_LINE_LEN — should pass through intact
        let len = MAX_LINE_LEN - 10;
        let cmd = format!("printf '%0{len}d' 0");
        let (result, lines) = collect_streaming_output(&["/bin/sh", "-c", &cmd]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        assert!(!stream_lines.is_empty());
        let line = &stream_lines[0].line;
        assert_eq!(line.len(), len, "expected line of exactly {len} chars");
        assert!(
            !line.contains("[truncated]"),
            "line under limit should not be truncated"
        );
    }

    #[test]
    fn test_streaming_timestamps_present() {
        let (result, lines) = collect_streaming_output(&["/bin/sh", "-c", "echo hello"]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        assert!(!stream_lines.is_empty());
        for sl in &stream_lines {
            assert!(
                !sl.ts.is_empty(),
                "stream line should have a non-empty timestamp"
            );
            // Basic format check: starts with year
            assert!(
                sl.ts.starts_with("20"),
                "timestamp should start with '20': {}",
                sl.ts
            );
        }
    }

    #[test]
    fn test_streaming_special_chars_in_output() {
        // Child output containing JSON-like content and special chars
        let (result, lines) = collect_streaming_output(&[
            "/bin/sh",
            "-c",
            r#"echo '{"fake":"json"}'; echo 'line with "quotes" and \backslash'"#,
        ]);
        assert!(result.is_ok());

        let stream_lines = parse_stream_lines(&lines);
        let out_lines: Vec<&str> = stream_lines
            .iter()
            .filter(|l| l.stream == StreamKind::Out)
            .map(|l| l.line.as_str())
            .collect();
        assert!(
            out_lines.iter().any(|l| l.contains(r#"{"fake":"json"}"#)),
            "JSON-like output should be preserved: {:?}",
            out_lines
        );
    }

    #[test]
    fn test_execute_sends_command_started() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context {
            slug: "test".to_string(),
            cluster_name: "devloop-test".to_string(),
            project_root: std::path::PathBuf::from("/tmp/devloop-test-nonexistent"),
            runtime_dir: dir.path().to_path_buf(),
            registry_path: dir.path().join("port-registry.json"),
            container_runtime: ContainerRuntime::Podman,
            host_gateway_ip: None,
            cargo_build_jobs: "6".to_string(),
            shutdown: Arc::new(AtomicBool::new(false)),
            write_state: Arc::new(Mutex::new(WriteState::new())),
            recreate_bound: AtomicBool::new(false),
        };

        let mut output = Vec::new();
        // Teardown will fail (no kind binary) but CommandStarted is sent first
        let _result = execute(&HelperCommand::Teardown, &ctx, &mut output);

        let lines: Vec<String> = output
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| String::from_utf8_lossy(l).to_string())
            .collect();

        assert!(
            !lines.is_empty(),
            "expected at least one line (CommandStarted)"
        );

        // First line must be CommandStarted
        let started: CommandStarted =
            serde_json::from_str(&lines[0]).expect("first line should parse as CommandStarted");
        assert!(started.started);
        assert_eq!(started.cmd, "teardown");
        assert!(!started.ts.is_empty());
    }

    #[test]
    fn test_rewrite_kubeconfig_standard_kind_output() {
        // Kind generates apiServerPort (43721) but container uses gateway port (24303)
        let kubeconfig = r#"apiVersion: v1
clusters:
- cluster:
    certificate-authority-data: LS0tLS1...
    server: https://127.0.0.1:43721
  name: kind-devloop-test
contexts:
- context:
    cluster: kind-devloop-test
    user: kind-devloop-test
  name: kind-devloop-test
current-context: kind-devloop-test
"#;
        let result =
            rewrite_kubeconfig_server(kubeconfig, "host.containers.internal", 24303).unwrap();
        assert!(result.contains("server: https://host.containers.internal:24303"));
        assert!(!result.contains("127.0.0.1"));
        assert!(!result.contains("43721"));
    }

    #[test]
    fn test_rewrite_kubeconfig_replaces_host_and_port() {
        let kubeconfig = "    server: https://127.0.0.1:6443\n";
        let result =
            rewrite_kubeconfig_server(kubeconfig, "host.containers.internal", 20103).unwrap();
        assert!(result.contains("server: https://host.containers.internal:20103"));
        assert!(!result.contains("6443"));
    }

    #[test]
    fn test_rewrite_kubeconfig_no_match_returns_error() {
        // A kubeconfig with no server: https:// line at all
        let kubeconfig = "    server: http://localhost:6443\n";
        let result = rewrite_kubeconfig_server(kubeconfig, "host.containers.internal", 24303);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("does not contain"),
            "error should explain the pattern mismatch: {err}"
        );
    }

    #[test]
    fn test_rewrite_kubeconfig_handles_gateway_ip() {
        // Kind may put the gateway IP instead of 127.0.0.1 when apiServerAddress is set
        let kubeconfig = "    server: https://10.255.255.254:25103\n";
        let result =
            rewrite_kubeconfig_server(kubeconfig, "host.containers.internal", 24303).unwrap();
        assert!(result.contains("server: https://host.containers.internal:24303"));
        assert!(!result.contains("10.255.255.254"));
    }

    #[test]
    fn test_rewrite_kubeconfig_empty_input_returns_error() {
        let result = rewrite_kubeconfig_server("", "host.containers.internal", 24303);
        assert!(result.is_err());
    }

    #[test]
    fn test_rewrite_kubeconfig_preserves_rest_of_content() {
        let kubeconfig = "before\n    server: https://127.0.0.1:9999\nafter\n";
        let result =
            rewrite_kubeconfig_server(kubeconfig, "host.containers.internal", 24303).unwrap();
        assert!(result.starts_with("before\n"));
        assert!(result.ends_with("after\n"));
        assert!(result.contains("server: https://host.containers.internal:24303"));
        assert!(!result.contains("9999"));
    }

    #[test]
    fn test_validate_gateway_ip_valid() {
        assert!(validate_gateway_ip("10.255.255.254").is_ok());
        assert!(validate_gateway_ip("192.168.1.1").is_ok());
        assert!(validate_gateway_ip("127.0.0.1").is_ok());
    }

    #[test]
    fn test_validate_gateway_ip_rejects_unspecified() {
        let err = validate_gateway_ip("0.0.0.0").unwrap_err().to_string();
        assert!(err.contains("must not be 0.0.0.0"), "got: {err}");
    }

    #[test]
    fn test_validate_gateway_ip_rejects_ipv6_unspecified() {
        let err = validate_gateway_ip("::").unwrap_err().to_string();
        assert!(err.contains("must not be"), "got: {err}");
    }

    #[test]
    fn test_validate_gateway_ip_rejects_invalid() {
        assert!(validate_gateway_ip("not-an-ip").is_err());
        assert!(validate_gateway_ip("").is_err());
        assert!(validate_gateway_ip("999.999.999.999").is_err());
    }

    #[test]
    fn test_write_port_map_shell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("port-map.env");

        let alloc = ports::PortAllocation {
            base_port: 24200,
            slot_index: 21,
        };
        write_port_map_shell(&path, &alloc).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();

        // Verify all expected variables are present with correct values
        let expected = [
            ("AC_HTTP_PORT", alloc.port(PortOffsets::AC_HTTP)),
            ("GC_HTTP_PORT", alloc.port(PortOffsets::GC_HTTP)),
            ("MH_HEALTH_PORT", alloc.port(PortOffsets::MH_0_HEALTH)),
            ("POSTGRES_PORT", alloc.port(PortOffsets::POSTGRES)),
            ("PROMETHEUS_PORT", alloc.port(PortOffsets::PROMETHEUS)),
            ("GRAFANA_PORT", alloc.port(PortOffsets::GRAFANA)),
            ("LOKI_PORT", alloc.port(PortOffsets::LOKI)),
            (
                "MC_0_WEBTRANSPORT_PORT",
                alloc.port(PortOffsets::MC_0_WEBTRANSPORT),
            ),
            (
                "MC_1_WEBTRANSPORT_PORT",
                alloc.port(PortOffsets::MC_1_WEBTRANSPORT),
            ),
            (
                "MH_0_WEBTRANSPORT_PORT",
                alloc.port(PortOffsets::MH_0_WEBTRANSPORT),
            ),
            (
                "MH_1_WEBTRANSPORT_PORT",
                alloc.port(PortOffsets::MH_1_WEBTRANSPORT),
            ),
        ];
        for (name, port) in &expected {
            let line = format!("{name}={port}");
            assert!(
                contents.contains(&line),
                "port-map.env missing '{line}', contents:\n{contents}"
            );
        }

        // Verify every non-comment, non-empty line matches setup.sh validation regex
        #[expect(
            clippy::disallowed_methods,
            reason = "test-only static-literal Regex; pattern compiles or test panics — outside dt-guard canonical-home discipline since this is a one-off test assertion, not a guard kernel. ADR-0034 §6 + ADR-0002"
        )]
        let re = regex::Regex::new(r"^[A-Z_][A-Z0-9_]*=[0-9]+$").unwrap();
        for line in contents.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            assert!(
                re.is_match(line),
                "line does not match setup.sh validation regex: '{line}'"
            );
        }
    }

    // --- parse_pod_health tests ---

    #[test]
    fn test_parse_pod_health_all_running() {
        let json = r#"{
            "items": [
                {
                    "metadata": {"name": "ac-service-0"},
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{"ready": true}]
                    }
                },
                {
                    "metadata": {"name": "gc-service-abc123"},
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{"ready": true}]
                    }
                }
            ]
        }"#;
        let summary = parse_pod_health(json).unwrap();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.ready, 2);
        assert!(summary.not_ready.is_empty());
    }

    #[test]
    fn test_parse_pod_health_partial() {
        let json = r#"{
            "items": [
                {
                    "metadata": {"name": "ac-service-0"},
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{"ready": true}]
                    }
                },
                {
                    "metadata": {"name": "gc-service-abc123"},
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{"ready": false}]
                    }
                },
                {
                    "metadata": {"name": "mc-0-xyz"},
                    "status": {
                        "phase": "CrashLoopBackOff",
                        "containerStatuses": [{"ready": false}]
                    }
                }
            ]
        }"#;
        let summary = parse_pod_health(json).unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.ready, 1);
        assert_eq!(summary.not_ready.len(), 2);
        assert_eq!(summary.not_ready[0].name, "gc-service-abc123");
        assert_eq!(summary.not_ready[1].name, "mc-0-xyz");
        assert_eq!(summary.not_ready[1].phase, "CrashLoopBackOff");
    }

    #[test]
    fn test_parse_pod_health_empty_pods() {
        let json = r#"{"items": []}"#;
        let summary = parse_pod_health(json).unwrap();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.ready, 0);
        assert!(summary.not_ready.is_empty());
    }

    #[test]
    fn test_parse_pod_health_malformed_json() {
        let result = parse_pod_health("not json at all");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid JSON"));
    }

    #[test]
    fn test_parse_pod_health_missing_items() {
        let result = parse_pod_health(r#"{"kind": "PodList"}"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("items"));
    }

    #[test]
    fn test_parse_pod_health_pending_pod() {
        let json = r#"{
            "items": [
                {
                    "metadata": {"name": "gc-service-pending"},
                    "status": {
                        "phase": "Pending"
                    }
                }
            ]
        }"#;
        let summary = parse_pod_health(json).unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.ready, 0);
        assert_eq!(summary.not_ready.len(), 1);
        assert_eq!(summary.not_ready[0].phase, "Pending");
    }

    /// Cancel-during-teardown invariant (semantic-guard fix at
    /// `cmd_teardown`): `Cancelled` must NOT be swallowed alongside other
    /// kind-delete errors. Port-deallocation is destructive, so a swallowed
    /// Cancelled would falsely report `outcome=completed` and proceed to
    /// release ports.
    #[test]
    fn test_cancel_during_teardown_propagates() {
        // Cancelled (clean) propagates.
        let err = propagate_teardown_kind_result(Err(HelperError::Cancelled { escalated: false }))
            .unwrap_err();
        assert!(matches!(err, HelperError::Cancelled { escalated: false }));

        // Cancelled (escalated) propagates.
        let err = propagate_teardown_kind_result(Err(HelperError::Cancelled { escalated: true }))
            .unwrap_err();
        assert!(matches!(err, HelperError::Cancelled { escalated: true }));
    }

    /// Sibling invariant: NON-cancel errors are intentionally swallowed so
    /// teardown can finish port-deallocation. Pin this so a future "always
    /// propagate" simplification breaks the test instead of port cleanup.
    #[test]
    fn test_teardown_swallows_non_cancel_errors() {
        let result = propagate_teardown_kind_result(Err(HelperError::CommandFailed {
            cmd: "kind delete cluster".to_string(),
            detail: "exit code: 1".to_string(),
        }));
        assert!(result.is_ok(), "non-cancel error must be swallowed");

        let result = propagate_teardown_kind_result(Ok(()));
        assert!(result.is_ok());
    }

    // --- Self-heal: inspect_signal (License 1 classifier, R3-final) -----------

    #[test]
    fn test_inspect_signal_running_stopped() {
        // rc0 + "true" ⇒ running; rc0 + "false" ⇒ stopped (the SOLE Unreachable).
        assert_eq!(inspect_signal(true, Some(0), "true\n"), Some(true));
        assert_eq!(inspect_signal(true, Some(0), "false\n"), Some(false));
        assert_eq!(inspect_signal(true, Some(0), " false "), Some(false));
    }

    #[test]
    fn test_inspect_signal_garbage_is_unknown_not_destroy() {
        // rc0 but not a clean bool ⇒ None (Unknown). Guards a future
        // `!= "true" ⇒ false` simplification that would destroy on garbage.
        assert_eq!(inspect_signal(true, Some(0), "maybe"), None);
        assert_eq!(inspect_signal(true, Some(0), ""), None);
    }

    #[test]
    fn test_inspect_signal_runtime_errors_are_unknown() {
        // spawn/exec failure (e.g. `timeout` missing) ⇒ None.
        assert_eq!(inspect_signal(false, None, ""), None);
        // timeout / kill ⇒ None.
        assert_eq!(inspect_signal(true, Some(124), ""), None);
        assert_eq!(inspect_signal(true, Some(137), ""), None);
        // container ABSENT (Docker exit 1) and daemon-down (also exit 1) ⇒ None:
        // an exit code cannot tell them apart, so neither licenses a destroy.
        // Absence is licensed separately via cluster_already_exists()==Ok(false).
        assert_eq!(inspect_signal(true, Some(1), ""), None);
        // Podman "no such object" exit 125 ⇒ None.
        assert_eq!(inspect_signal(true, Some(125), ""), None);
    }

    // --- Self-heal: reachability_from_signals (Delta 2) -----------------------

    #[test]
    fn test_reachability_container_stopped_is_unreachable() {
        // The ONLY Unreachable source.
        assert_eq!(
            reachability_from_signals(Some(false), Some(6443), Some(true)),
            ApiserverReachability::Unreachable
        );
        assert_eq!(
            reachability_from_signals(Some(false), None, None),
            ApiserverReachability::Unreachable
        );
    }

    #[test]
    fn test_reachability_inspect_indeterminate_is_unknown() {
        assert_eq!(
            reachability_from_signals(None, Some(6443), Some(true)),
            ApiserverReachability::Unknown
        );
    }

    #[test]
    fn test_reachability_running_tcp_cases() {
        // running + connected ⇒ Reachable (healthy-cluster probe-target pin:
        // this must NOT be aimed at a defunct 127.0.0.1).
        assert_eq!(
            reachability_from_signals(Some(true), Some(6443), Some(true)),
            ApiserverReachability::Reachable
        );
        // running + refused ⇒ Unknown (Delta 2 — the flipped row; NOT Unreachable).
        assert_eq!(
            reachability_from_signals(Some(true), Some(6443), Some(false)),
            ApiserverReachability::Unknown
        );
        // running + timeout ⇒ Unknown.
        assert_eq!(
            reachability_from_signals(Some(true), Some(6443), None),
            ApiserverReachability::Unknown
        );
        // running + port undeterminable ⇒ Unknown.
        assert_eq!(
            reachability_from_signals(Some(true), None, None),
            ApiserverReachability::Unknown
        );
    }

    // --- Self-heal: two-armed recreate gate (License proof, R3-final) ---------
    // These prove the LICENSE (distinct from the classifier). The shell's
    // `Cluster exists: false ⇒ recreate` case proves only ROUTING; the license
    // that a real cmd_recreate permits the destroy lives here.

    #[test]
    fn test_recreate_gate_license_1_unreachable_proceeds() {
        // Container stopped ⇒ licensed regardless of the exists probe.
        assert!(recreate_licensed(
            ApiserverReachability::Unreachable,
            &Ok(true)
        ));
    }

    #[test]
    fn test_recreate_gate_license_2_absent_proceeds() {
        // Cluster positively absent ⇒ licensed (nothing to destroy).
        assert!(recreate_licensed(
            ApiserverReachability::Unknown,
            &Ok(false)
        ));
        assert!(recreate_licensed(
            ApiserverReachability::Reachable,
            &Ok(false)
        ));
    }

    #[test]
    fn test_recreate_gate_present_and_alive_refuses() {
        // Reachable / Unknown with the cluster present ⇒ NOT licensed.
        assert!(!recreate_licensed(
            ApiserverReachability::Reachable,
            &Ok(true)
        ));
        assert!(!recreate_licensed(
            ApiserverReachability::Unknown,
            &Ok(true)
        ));
    }

    #[test]
    fn test_recreate_gate_daemon_down_err_refuses_not_absence() {
        // THE PIN: `Err` (daemon down / wedged kind) must NOT license — it is
        // NOT `Ok(false)`. An `unwrap_or(false)` collapse would flip this to
        // licensed (every daemon outage a destroy license) and red this test.
        let err = || HelperError::CommandFailed {
            cmd: "kind get clusters".to_string(),
            detail: "daemon unreachable".to_string(),
        };
        assert!(!recreate_licensed(
            ApiserverReachability::Unknown,
            &Err(err())
        ));
        // Even with a Reachable probe (which alone wouldn't license), Err stays refused.
        assert!(!recreate_licensed(
            ApiserverReachability::Reachable,
            &Err(err())
        ));
        // And Unreachable still licenses via arm 1 even if exists is Err.
        assert!(recreate_licensed(
            ApiserverReachability::Unreachable,
            &Err(err())
        ));
    }

    // --- Self-heal: the AtomicBool once-per-lifetime BOUND (B1) ---------------
    // Distinct from the license: license = "may we destroy at all"; bound =
    // "have we already destroyed once this lifetime". `decide_recreate` is the
    // pure gate+bound decision; these drive it hermetically (no teardown/setup).

    #[test]
    fn test_decide_recreate_refusal_does_not_charge_bound() {
        let err = || HelperError::CommandFailed {
            cmd: "kind get clusters".to_string(),
            detail: "daemon down".to_string(),
        };
        // Reachable + present ⇒ refused-cluster-alive, bound untouched.
        let b = AtomicBool::new(false);
        assert_eq!(
            decide_recreate(ApiserverReachability::Reachable, &Ok(true), &b),
            RecreateDecision::RefusedClusterAlive
        );
        assert!(
            !b.load(Ordering::SeqCst),
            "refusal must not charge the bound"
        );
        // Unknown + Err (runtime down) ⇒ refused-inconclusive, bound untouched —
        // so a LATER legitimate Unreachable still gets its one recreate.
        let b = AtomicBool::new(false);
        assert_eq!(
            decide_recreate(ApiserverReachability::Unknown, &Err(err()), &b),
            RecreateDecision::RefusedInconclusive
        );
        assert!(!b.load(Ordering::SeqCst));
        // Proven: after that inconclusive refusal, Unreachable now proceeds+charges.
        assert_eq!(
            decide_recreate(ApiserverReachability::Unreachable, &Err(err()), &b),
            RecreateDecision::Proceed
        );
        assert!(
            b.load(Ordering::SeqCst),
            "licensed destroy must charge the bound"
        );
    }

    #[test]
    fn test_decide_recreate_bound_is_once_per_lifetime() {
        // A shared bound models the daemon's Arc<Context> across accept threads.
        let b = AtomicBool::new(false);
        // First licensed recreate proceeds and charges.
        assert_eq!(
            decide_recreate(ApiserverReachability::Unreachable, &Ok(true), &b),
            RecreateDecision::Proceed
        );
        // Second in the same lifetime is suppressed — the no-infinite-loop guard.
        assert_eq!(
            decide_recreate(ApiserverReachability::Unreachable, &Ok(true), &b),
            RecreateDecision::AlreadyAttempted
        );
        // License 2 (Ok(false)) is likewise bound-suppressed after the spend.
        assert_eq!(
            decide_recreate(ApiserverReachability::Unknown, &Ok(false), &b),
            RecreateDecision::AlreadyAttempted
        );
    }

    // --- Self-heal: evidence-bundle closed allowlist (B2) ---------------------

    /// Runs the REAL `capture_evidence_bundle` and set-compares the PRODUCED
    /// files against the EXPECTED set (`EVIDENCE_BUNDLE_ITEMS`, the test-side
    /// aggregate of the production `EV_*` filename consts). NOT a static
    /// list-equals-itself check (@test): production writes each bundle file
    /// THROUGH those same `EV_*` consts, so a 6th capture with a new filename, or
    /// a slice-only add, both red — drift caught bidirectionally, fails closed,
    /// which a content grep can't — plus a credential-marker defence-in-depth
    /// (no `client-key-data`/`client-certificate-data`/PEM; `certificate-authority-data`
    /// is public and deliberately NOT asserted). Requires coreutils `timeout`
    /// (a hard dependency of the capture path); runs the real bounded capture,
    /// whose subprocesses fail harmlessly if kind/podman/kubectl are absent.
    #[test]
    fn test_evidence_bundle_matches_allowlist_and_has_no_credentials() {
        use std::collections::BTreeSet;
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        let leaf = capture_evidence_bundle(&ctx);
        assert!(
            leaf.starts_with("self-heal-evidence-"),
            "capture failed ({leaf}) — is coreutils `timeout` on PATH?"
        );
        let bundle_dir = dir.path().join(&leaf);
        let produced: BTreeSet<String> = fs::read_dir(&bundle_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let expected: BTreeSet<String> = EVIDENCE_BUNDLE_ITEMS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            produced, expected,
            "evidence bundle drifted from EVIDENCE_BUNDLE_ITEMS"
        );
        // Positive control (not vacuous) + no credential markers.
        for item in EVIDENCE_BUNDLE_ITEMS {
            let body = fs::read_to_string(bundle_dir.join(item)).unwrap_or_default();
            for marker in ["client-key-data", "client-certificate-data", "-----BEGIN"] {
                assert!(
                    !body.contains(marker),
                    "credential marker {marker:?} found in evidence item {item}"
                );
            }
        }
    }

    // --- Self-heal: outcome token vocabulary (identity-mapping pin) -----------

    /// The helper's `HELPER_OUTCOME=` wire tokens, pinned EXHAUSTIVELY: the
    /// `wire` match below is compile-forced to cover every `SelfHealOutcome`
    /// variant, so a NEW variant fails to compile until its token is pinned —
    /// not a vacuity-prone per-variant literal list (@team-lead A3, @dry-reviewer
    /// F4). serde and the hand map must agree (catches a serde-rename drift), and
    /// the produced set must equal the frozen 7-member `HELPER_OUTCOME=` set that
    /// @paired-operations catalogues in §8 (the cross-encoding assertion the old
    /// name overclaimed but never actually made). The shell `DETAIL=` half is
    /// pinned independently by `layer7.test.sh`'s SH cases + the `ANCHOR (DRY):`.
    #[test]
    fn test_self_heal_outcome_wire_tokens_exhaustive() {
        use std::collections::BTreeSet;
        fn wire(o: &SelfHealOutcome) -> &'static str {
            match o {
                SelfHealOutcome::Recreated => "recreated",
                SelfHealOutcome::Restored => "restored",
                SelfHealOutcome::RecreateRefusedClusterAlive => "recreate-refused-cluster-alive",
                SelfHealOutcome::RecreateRefusedInconclusive => "recreate-refused-inconclusive",
                SelfHealOutcome::RecreateAlreadyAttemptedThisHelperLifetime => {
                    "recreate-already-attempted-this-helper-lifetime"
                }
                SelfHealOutcome::RecreateFailed => "recreate-failed",
                SelfHealOutcome::RestoreFailed => "restore-failed",
            }
        }
        let all = [
            SelfHealOutcome::Recreated,
            SelfHealOutcome::Restored,
            SelfHealOutcome::RecreateRefusedClusterAlive,
            SelfHealOutcome::RecreateRefusedInconclusive,
            SelfHealOutcome::RecreateAlreadyAttemptedThisHelperLifetime,
            SelfHealOutcome::RecreateFailed,
            SelfHealOutcome::RestoreFailed,
        ];
        let expected: BTreeSet<&str> = [
            "recreated",
            "restored",
            "recreate-refused-cluster-alive",
            "recreate-refused-inconclusive",
            "recreate-already-attempted-this-helper-lifetime",
            "recreate-failed",
            "restore-failed",
        ]
        .into_iter()
        .collect();
        let mut produced: BTreeSet<&str> = BTreeSet::new();
        for o in &all {
            assert_eq!(
                serde_json::to_value(o).unwrap(),
                wire(o),
                "serde vs hand map"
            );
            produced.insert(wire(o));
        }
        assert_eq!(
            produced, expected,
            "HELPER_OUTCOME wire set drifted from the frozen §8 set"
        );
    }

    /// `self_heal_data` emits `max: 1` (literal, derived from the AtomicBool's
    /// arity — not a tunable const).
    #[test]
    fn test_self_heal_data_max_is_one() {
        let data = self_heal_data(SelfHealOutcome::Recreated, 1, "self-heal-evidence-x").unwrap();
        assert_eq!(data["self_heal"]["max"], 1);
        assert_eq!(data["self_heal"]["attempt"], 1);
        assert_eq!(data["self_heal"]["outcome"], "recreated");
        assert_eq!(data["self_heal"]["evidence"], "self-heal-evidence-x");
    }

    /// S4: cmd_restore_kubeconfig derives the apiserver port from the persisted
    /// ports.json, and `read_k8s_api_port` returns None (⇒ refuse loudly, never
    /// allocate) when the file is missing or the port absent/zero.
    #[test]
    fn test_read_k8s_api_port_from_ports_json() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = test_ctx(dir.path());
        // Missing ports.json ⇒ None.
        assert_eq!(read_k8s_api_port(&ctx), None);
        // Present with a real port ⇒ Some.
        fs::write(
            dir.path().join("ports.json"),
            r#"{"ports":{"k8s_api":24303}}"#,
        )
        .unwrap();
        assert_eq!(read_k8s_api_port(&ctx), Some(24303));
        // Zero port ⇒ None (treated as absent).
        fs::write(dir.path().join("ports.json"), r#"{"ports":{"k8s_api":0}}"#).unwrap();
        assert_eq!(read_k8s_api_port(&ctx), None);
    }

    /// Minimal Context for the pure-fn tests that need one (`read_k8s_api_port`).
    fn test_ctx(dir: &std::path::Path) -> Context {
        Context {
            slug: "test".to_string(),
            cluster_name: "devloop-test".to_string(),
            project_root: PathBuf::from("/tmp/devloop-test-nonexistent"),
            runtime_dir: dir.to_path_buf(),
            registry_path: dir.join("port-registry.json"),
            container_runtime: ContainerRuntime::Podman,
            host_gateway_ip: None,
            cargo_build_jobs: "6".to_string(),
            shutdown: Arc::new(AtomicBool::new(false)),
            write_state: Arc::new(Mutex::new(WriteState::new())),
            recreate_bound: AtomicBool::new(false),
        }
    }
}
