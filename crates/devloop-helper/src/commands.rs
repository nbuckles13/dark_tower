//! Command execution for the devloop helper.
//!
//! All external commands use `Command::new().arg()` — no shell interpolation.
//! User-facing commands stream stdout/stderr line-by-line to the client via
//! an mpsc channel. Internal commands (`cluster_already_exists`, `which`) use
//! buffered `.output()` since they need to inspect output programmatically.

use crate::error::HelperError;
use crate::logging::now_rfc3339;
use crate::ports::{self, PortAllocation, PortOffsets};
use crate::protocol::{
    CommandOutcome, CommandResult, CommandStarted, HelperCommand, Service, StreamKind, StreamLine,
    StreamMsg, MAX_LINE_LEN,
};
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hostname that dev containers use to reach the host via podman's gateway.
/// This is a well-known podman/slirp4netns convention — resolves to the
/// host-gateway IP (e.g., 10.255.255.254) inside containers.
const CONTAINER_HOST: &str = "host.containers.internal";

/// Default host-gateway IP for Kind NodePort listenAddress (ADR-0030).
/// Used as fallback when `--host-gateway-ip` is not provided.
const DEFAULT_HOST_GATEWAY_IP: &str = "10.255.255.254";

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
    /// Shutdown flag — set by signal handler, checked during long-running commands.
    pub shutdown: Arc<AtomicBool>,
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

    let result = match cmd {
        HelperCommand::Setup { skip_observability } => cmd_setup(ctx, *skip_observability, writer),
        HelperCommand::Rebuild(svc) => cmd_rebuild(ctx, *svc, writer).map(|()| None),
        HelperCommand::RebuildAll => cmd_rebuild_all(ctx, writer).map(|()| None),
        HelperCommand::Deploy(svc) => cmd_deploy(ctx, *svc, writer).map(|()| None),
        HelperCommand::Teardown => cmd_teardown(ctx, writer).map(|()| None),
        HelperCommand::Status => cmd_status(ctx),
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(data) => CommandResult {
            result: CommandOutcome::Ok,
            exit_code: Some(0),
            duration_ms,
            error: None,
            data,
        },
        Err(e) => CommandResult {
            result: CommandOutcome::Error,
            exit_code: None,
            duration_ms,
            error: Some(e.to_string()),
            data: None,
        },
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

/// Setup: allocate ports, generate kind-config, create cluster, run setup.sh.
fn cmd_setup(
    ctx: &Context,
    skip_observability: bool,
    writer: &mut dyn Write,
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
            &ctx.shutdown,
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

    run_command_streaming(&mut setup_cmd, "setup.sh", writer, &ctx.shutdown)?;

    // Generate kubeconfig for container access (ADR-0030/0031).
    // Rewrites the API server URL to use host.containers.internal and the
    // gateway K8s API port (extraPortMappings), not the apiServerPort.
    generate_container_kubeconfig(ctx, &alloc)?;

    // Return port map as data
    let data = serde_json::to_value(&port_map).map_err(|e| HelperError::CommandFailed {
        cmd: "setup".to_string(),
        detail: format!("failed to serialize port map: {e}"),
    })?;

    Ok(Some(data))
}

/// Write a shell-sourceable port map file for setup.sh.
fn write_port_map_shell(path: &Path, alloc: &PortAllocation) -> Result<(), HelperError> {
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
/// Kind generates `server: https://127.0.0.1:API_SERVER_PORT` where API_SERVER_PORT
/// is the Kind apiServerPort (bound to 127.0.0.1, for host-side kubectl).
///
/// Inside the dev container, the K8s API is reached through a different path:
/// `host.containers.internal:GATEWAY_PORT` where GATEWAY_PORT is the
/// extraPortMappings hostPort for containerPort 6443, bound to the host-gateway IP.
///
/// These are two different ports — the apiServerPort and the gateway port are
/// independently allocated. This function replaces both host and port.
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
/// Runs `kind get kubeconfig`, rewrites the API server URL from
/// `https://127.0.0.1:$apiServerPort` to
/// `https://host.containers.internal:$gatewayK8sPort` so the container
/// can reach the Kind cluster's K8s API through the host-gateway binding.
///
/// The two ports are different: apiServerPort is Kind's host-side port
/// bound to 127.0.0.1, while gatewayK8sPort is the extraPortMappings
/// port bound to HOST_GATEWAY_IP (reachable via host.containers.internal).
fn generate_container_kubeconfig(ctx: &Context, alloc: &PortAllocation) -> Result<(), HelperError> {
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
    let gateway_k8s_port = alloc.port(PortOffsets::K8S_API);
    // Use the gateway IP directly (not host.containers.internal) because the K8s API
    // server's TLS cert includes the IP as a SAN but not the DNS name.
    // HTTP services (AC, GC, etc.) can use host.containers.internal since they don't
    // do TLS cert validation.
    let gw_ip = ctx
        .host_gateway_ip
        .as_deref()
        .unwrap_or(DEFAULT_HOST_GATEWAY_IP);
    let kubeconfig = rewrite_kubeconfig_server(&kubeconfig, gw_ip, gateway_k8s_port)?;

    let kubeconfig_path = ctx.runtime_dir.join("kubeconfig");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&kubeconfig_path)?;
    file.write_all(kubeconfig.as_bytes())?;
    file.flush()?;

    eprintln!(
        "[devloop-helper] setup: kubeconfig written to {}",
        kubeconfig_path.display()
    );

    Ok(())
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
fn cmd_status(ctx: &Context) -> Result<Option<serde_json::Value>, HelperError> {
    eprintln!("[devloop-helper] status: checking cluster health...");

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

    // 4. Check if setup is in progress (PID file from eager setup)
    let setup_in_progress = {
        let setup_pid_path = ctx.runtime_dir.join("setup.pid");
        if let Ok(pid_str) = fs::read_to_string(&setup_pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
            } else {
                false
            }
        } else {
            false
        }
    };

    // Build response data — construct fully to avoid indexing (clippy::indexing_slicing)
    let pod_summary_val = pod_summary.map(|summary| {
        serde_json::json!({
            "total": summary.total,
            "ready": summary.ready,
            "not_ready": summary.not_ready,
        })
    });

    let data = serde_json::json!({
        "cluster_exists": cluster_exists,
        "pods_healthy": pods_healthy,
        "setup_in_progress": setup_in_progress,
        "checked_at": now_rfc3339(),
        "pod_summary": pod_summary_val,
        "pod_error": pod_error,
        "ports": ports,
    });

    Ok(Some(data))
}

/// Rebuild: build one service image, load into Kind, restart deployment.
fn cmd_rebuild(ctx: &Context, svc: Service, writer: &mut dyn Write) -> Result<(), HelperError> {
    eprintln!("[devloop-helper] rebuild: building {}...", svc);

    // Build image
    run_command_streaming(
        Command::new(ctx.container_runtime.as_str())
            .arg("build")
            .arg("-t")
            .arg(svc.image_tag())
            .arg("-f")
            .arg(svc.dockerfile())
            .arg(&ctx.project_root),
        &format!("{} build {}", ctx.container_runtime.as_str(), svc),
        writer,
        &ctx.shutdown,
    )?;

    // Load into Kind
    load_image_to_kind(ctx, svc.image_tag(), writer)?;

    // Restart deployment
    restart_deployment(ctx, svc, writer)?;

    Ok(())
}

/// Rebuild all service images.
fn cmd_rebuild_all(ctx: &Context, writer: &mut dyn Write) -> Result<(), HelperError> {
    for svc in &Service::ALL {
        cmd_rebuild(ctx, *svc, writer)?;
    }
    Ok(())
}

/// Deploy: apply manifests only via setup.sh --skip-build --only.
fn cmd_deploy(ctx: &Context, svc: Service, writer: &mut dyn Write) -> Result<(), HelperError> {
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
        &ctx.shutdown,
    )?;

    Ok(())
}

/// Teardown: delete Kind cluster, clean up all state.
fn cmd_teardown(ctx: &Context, writer: &mut dyn Write) -> Result<(), HelperError> {
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
        &ctx.shutdown,
    );

    if let Err(ref e) = result {
        eprintln!("[devloop-helper] teardown: kind delete cluster warning: {e}");
        // Continue with cleanup even if kind delete fails
    }

    // Remove from port registry
    if let Err(e) = ports::deallocate_ports(&ctx.slug, &ctx.registry_path) {
        eprintln!("[devloop-helper] teardown: port deallocation warning: {e}");
    }

    Ok(())
}

/// Load a container image into the Kind cluster.
fn load_image_to_kind(
    ctx: &Context,
    image_tag: &str,
    writer: &mut dyn Write,
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
                &ctx.shutdown,
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
                &ctx.shutdown,
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
                &ctx.shutdown,
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
                &ctx.shutdown,
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
                &ctx.shutdown,
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
                    &ctx.shutdown,
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
                    &ctx.shutdown,
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
/// writer. The calling thread checks the `shutdown` flag for SIGTERM.
///
/// On broken pipe (client disconnect) or SIGTERM: the child is killed via
/// `Child::kill()` (SIGKILL — immediate, non-catchable). The output is lost
/// anyway when the client disconnects, and idempotent re-runs handle recovery.
/// The launcher (devloop.sh) sends SIGTERM first; if the helper doesn't exit
/// within its timeout, devloop.sh escalates to SIGKILL.
///
/// SAFETY: Child processes must not receive the auth token in their environment.
/// Only DT_CLUSTER_NAME, DT_PORT_MAP (file path), and KIND_EXPERIMENTAL_PROVIDER
/// are passed. The auth token stays in the helper process only.
fn run_command_streaming(
    cmd: &mut Command,
    description: &str,
    writer: &mut dyn Write,
    shutdown: &AtomicBool,
) -> Result<(), HelperError> {
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

    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(StreamMsg::Line(stream_line)) => {
                if !write_failed {
                    if let Err(e) = send_json_line(writer, &stream_line) {
                        eprintln!("[devloop-helper] stream write failed: {e}");
                        write_failed = true;
                        // Kill the child — output is lost, idempotent re-run handles recovery
                        if !child_killed {
                            // SIGKILL (not SIGTERM) — immediate termination
                            let _ = child.kill();
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

        // Kill child on SIGTERM
        if !child_killed && shutdown.load(Ordering::Relaxed) {
            // SIGKILL (not SIGTERM) — immediate termination; idempotent re-run handles recovery
            let _ = child.kill();
            child_killed = true;
            // Don't break — let the reader threads drain and send Done
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
        let shutdown = AtomicBool::new(false);
        let mut output = Vec::new();
        let result = run_command_streaming(
            Command::new(args[0]).args(&args[1..]),
            "test command",
            &mut output,
            &shutdown,
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
        let shutdown = AtomicBool::new(false);
        let mut output = Vec::new();
        let result = run_command_streaming(
            &mut Command::new("/nonexistent/binary/that/does/not/exist"),
            "nonexistent",
            &mut output,
            &shutdown,
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
            shutdown: Arc::new(AtomicBool::new(false)),
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
}
