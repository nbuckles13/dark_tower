//! Meeting Controller
//!
//! Stateful WebTransport signaling server for real-time meeting coordination.
//!
//! # Servers
//!
//! The Meeting Controller runs multiple servers:
//! - WebTransport server for client signaling (default: 0.0.0.0:4433)
//! - gRPC server for GC communication (default: 0.0.0.0:50052)
//! - HTTP server for health endpoints (default: 0.0.0.0:8081)
//!
//! # Architecture (ADR-0023)
//!
//! Uses an actor model hierarchy:
//! - `MeetingControllerActor` (singleton): Supervises meetings
//! - `MeetingActor` (per meeting): Owns meeting state
//! - `ConnectionActor` (per connection): Handles one WebTransport connection
//!
//! # State Management
//!
//! - Live state in Redis with sync writes for critical data
//! - Fencing tokens prevent split-brain during failover
//! - Session binding tokens enable secure reconnection
//!
//! # Startup Flow (ADR-0023 Phase 6c, ADR-0010)
//!
//! 1. Load configuration from environment
//! 2. Initialize Prometheus metrics recorder (ADR-0011)
//! 3. Initialize Redis connection (`FencedRedisClient`)
//! 4. Spawn `TokenManager` for OAuth token acquisition from AC (ADR-0010)
//! 5. Initialize actor system (`MeetingControllerActorHandle`)
//! 6. Start health HTTP server (liveness, readiness, metrics)
//! 7. Start gRPC server for GC->MC communication
//! 8. Create `GcClient` with `TokenReceiver` and spawn GC task (registration + heartbeats)
//! 9. Wait for shutdown signal

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)] // main.rs orchestrates startup, naturally longer

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use common::secret::{ExposeSecret, SecretBox};
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use meeting_controller::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use meeting_controller::config::Config;
use meeting_controller::errors::McError;
use meeting_controller::grpc::{GcClient, McAssignmentService};
use meeting_controller::observability::{health_router, HealthState};
use meeting_controller::redis::FencedRedisClient;
use meeting_controller::system_info::gather_system_info;
use metrics_exporter_prometheus::PrometheusBuilder;
use proto_gen::internal::meeting_controller_service_server::MeetingControllerServiceServer;
use proto_gen::internal::HealthStatus;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Default timeout for initial token acquisition.
const TOKEN_ACQUISITION_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum secret length for HMAC-SHA256 (32 bytes).
const MIN_SECRET_LENGTH: usize = 32;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "meeting_controller=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Meeting Controller");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!(
        region = %config.region,
        mc_id = %config.mc_id,
        webtransport_bind_address = %config.webtransport_bind_address,
        grpc_bind_address = %config.grpc_bind_address,
        health_bind_address = %config.health_bind_address,
        max_meetings = config.max_meetings,
        max_participants = config.max_participants,
        binding_token_ttl_seconds = config.binding_token_ttl_seconds,
        "Configuration loaded successfully"
    );

    // Initialize Prometheus metrics recorder (ADR-0011)
    // This must happen before any metrics are recorded
    info!("Initializing Prometheus metrics recorder...");
    let prometheus_handle = PrometheusBuilder::new().install_recorder().map_err(|e| {
        error!(error = %e, "Failed to install Prometheus metrics recorder");
        format!("Failed to install Prometheus metrics recorder: {e}")
    })?;
    info!("Prometheus metrics recorder initialized");

    // Initialize health state
    let health_state = Arc::new(HealthState::new());

    // Initialize Redis connection (Phase 6b)
    info!("Connecting to Redis...");
    let redis_client = FencedRedisClient::new(config.redis_url.expose_secret())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to Redis");
            e
        })?;
    let redis_client = Arc::new(redis_client);
    info!("Redis connection established");

    // Spawn TokenManager for OAuth token acquisition (ADR-0010)
    info!(
        ac_endpoint = %config.ac_endpoint,
        client_id = %config.client_id,
        "Spawning TokenManager for AC authentication..."
    );

    // Use from_url() to automatically handle HTTP (local dev) or HTTPS (production)
    let token_config = TokenManagerConfig::from_url(
        config.ac_endpoint.clone(),
        config.client_id.clone(),
        config.client_secret.clone(),
    )
    .map_err(|e| {
        error!(error = %e, "Failed to create TokenManager config");
        McError::TokenAcquisition(format!("TokenManager config error: {e}"))
    })?;

    let (token_task_handle, token_rx) =
        tokio::time::timeout(TOKEN_ACQUISITION_TIMEOUT, spawn_token_manager(token_config))
            .await
            .map_err(|_| {
                error!(
                    timeout_secs = TOKEN_ACQUISITION_TIMEOUT.as_secs(),
                    "Token acquisition timed out - AC may be unreachable"
                );
                McError::TokenAcquisitionTimeout
            })?
            .map_err(|e| {
                error!(error = %e, "Failed to acquire initial token from AC");
                McError::TokenAcquisition(format!("Initial token acquisition failed: {e}"))
            })?;

    info!("TokenManager spawned successfully, initial token acquired");

    // Initialize shared metrics for heartbeat reporting
    let controller_metrics = ControllerMetrics::new();

    // Initialize actor system (Phase 6b)
    info!("Initializing actor system...");
    let actor_metrics = ActorMetrics::new();

    // Decode master secret for session binding tokens from base64 config
    let master_secret = {
        use base64::Engine;
        let decoder = base64::engine::general_purpose::STANDARD;
        let secret_bytes = decoder
            .decode(config.binding_token_secret.expose_secret())
            .map_err(|e| {
                error!(error = %e, "MC_BINDING_TOKEN_SECRET is not valid base64");
                format!("Invalid base64 in MC_BINDING_TOKEN_SECRET: {e}")
            })?;

        if secret_bytes.len() < MIN_SECRET_LENGTH {
            error!(
                length = secret_bytes.len(),
                min_length = MIN_SECRET_LENGTH,
                "MC_BINDING_TOKEN_SECRET is too short"
            );
            return Err(format!(
                "MC_BINDING_TOKEN_SECRET must be at least {MIN_SECRET_LENGTH} bytes, got {}",
                secret_bytes.len()
            )
            .into());
        }

        SecretBox::new(Box::new(secret_bytes))
    };

    let controller_handle = Arc::new(MeetingControllerActorHandle::new(
        config.mc_id.clone(),
        Arc::clone(&actor_metrics),
        Arc::clone(&controller_metrics),
        master_secret,
    ));
    info!("Actor system initialized");

    // Create shutdown token as child of controller's token
    // This ensures all tasks are cancelled when the controller shuts down
    let shutdown_token = controller_handle.child_token();

    // Start health HTTP server (MUST succeed - fail startup if it doesn't)
    // This provides liveness/readiness probes and Prometheus /metrics endpoint
    let health_addr: SocketAddr = config.health_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.health_bind_address, "Invalid health bind address");
        format!("Invalid health bind address: {e}")
    })?;

    let health_router = health_router(Arc::clone(&health_state));

    // Add /metrics endpoint served by Prometheus exporter
    let metrics_router = Router::new().route(
        "/metrics",
        axum::routing::get(move || {
            let handle = prometheus_handle.clone();
            async move { handle.render() }
        }),
    );

    let app = health_router.merge(metrics_router);

    // Bind listener BEFORE spawning to fail fast on bind errors
    let listener = tokio::net::TcpListener::bind(health_addr)
        .await
        .map_err(|e| {
            error!(error = %e, addr = %health_addr, "Failed to bind health server");
            format!("Failed to bind health server to {health_addr}: {e}")
        })?;
    info!(addr = %health_addr, "Health server bound successfully");

    // Spawn health server task
    let health_shutdown_token = shutdown_token.child_token();
    tokio::spawn(async move {
        info!(addr = %health_addr, "Health server starting");
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            health_shutdown_token.cancelled().await;
            info!("Health server shutting down");
        });
        if let Err(e) = server.await {
            error!(error = %e, "Health server failed");
        }
    });
    info!(addr = %health_addr, "Health server started");

    // Start gRPC server BEFORE GC registration (correct ordering)
    // This prevents race condition where GC tries to call MC before server is ready
    let grpc_addr = config.grpc_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.grpc_bind_address, "Invalid gRPC bind address");
        e
    })?;

    let mc_assignment_service = McAssignmentService::new(
        Arc::clone(&controller_handle),
        Arc::clone(&redis_client),
        config.mc_id.clone(),
        config.max_meetings,
        config.max_participants,
    );

    let grpc_shutdown_token = shutdown_token.child_token();
    let grpc_server = tonic::transport::Server::builder()
        .add_service(MeetingControllerServiceServer::new(mc_assignment_service))
        .serve_with_shutdown(grpc_addr, async move {
            grpc_shutdown_token.cancelled().await;
            info!("gRPC server shutting down");
        });

    // Spawn gRPC server task
    tokio::spawn(async move {
        info!(addr = %grpc_addr, "gRPC server starting");
        if let Err(e) = grpc_server.await {
            error!(error = %e, "gRPC server failed");
        }
    });
    info!(addr = %grpc_addr, "gRPC server started");

    // Create GcClient with TokenReceiver and spawn unified GC task (Phase 6c, ADR-0010)
    // This task owns gc_client directly (no Arc needed)
    info!("Connecting to Global Controller...");
    let gc_client = GcClient::new(config.gc_grpc_url.clone(), token_rx.clone(), config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to GC");
            e
        })?;
    info!("Connected to Global Controller");

    // Spawn unified GC task (registration + dual heartbeats)
    let gc_task_token = shutdown_token.child_token();
    let gc_task_metrics = Arc::clone(&controller_metrics);
    let gc_task_health = Arc::clone(&health_state);
    tokio::spawn(async move {
        run_gc_task(gc_client, gc_task_metrics, gc_task_health, gc_task_token).await;
    });
    info!("GC task started");

    info!("Meeting Controller Phase 6c: GC integration complete");

    // TODO (Phase 6g): Start WebTransport server
    // TODO (Phase 6h): Start health endpoints

    // Wait for shutdown signal
    info!("Meeting Controller running - press Ctrl+C to shutdown");
    shutdown_signal().await;

    // Trigger graceful shutdown via cancellation token
    // This propagates to all child tokens (GC task, gRPC server, health server)
    info!("Shutdown signal received, initiating graceful shutdown...");

    // Mark as not ready immediately so k8s stops sending traffic
    health_state.set_not_ready();

    shutdown_token.cancel();

    // Give tasks time to shut down
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown actor system (also cancels via its token)
    if let Err(e) = controller_handle.shutdown(Duration::from_secs(30)).await {
        warn!(error = %e, "Actor system shutdown error");
    }

    // Abort TokenManager background task (ADR-0010)
    info!("Stopping TokenManager...");
    token_task_handle.abort();

    info!("Meeting Controller shutdown complete");
    Ok(())
}

/// Unified GC task: registration + dual heartbeat loop.
///
/// This task owns `gc_client` directly (no Arc needed).
/// It never exits on GC connectivity issues - keeps retrying to protect active meetings.
///
/// Operational model:
/// - Initial registration: Retry forever until success (with exponential backoff)
/// - Dual heartbeats: Fast (10s) + comprehensive (30s) in single select loop
/// - Re-registration: Detect `NOT_FOUND` from heartbeat, automatically re-register
/// - Never exit: Protects active meetings during GC outages/restarts
async fn run_gc_task(
    gc_client: GcClient,
    metrics: Arc<ControllerMetrics>,
    health_state: Arc<HealthState>,
    cancel_token: CancellationToken,
) {
    info!("GC task: Starting initial registration");

    // Initial registration (retry forever, never exit)
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Cancelled before registration completed");
                return;
            }
            result = gc_client.register() => {
                match result {
                    Ok(()) => {
                        info!("GC task: Initial registration successful");
                        // Mark as ready now that we're registered with GC
                        health_state.set_ready();
                        break; // Proceed to heartbeat loop
                    }
                    Err(e) => {
                        // Log but never exit - keep retrying
                        // GC may be temporarily unavailable during rolling updates
                        warn!(error = %e, "GC task: Initial registration failed, will retry");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    // Dual heartbeat loop (fast + comprehensive in one select)
    let fast_interval = Duration::from_millis(gc_client.fast_heartbeat_interval_ms());
    let comprehensive_interval =
        Duration::from_millis(gc_client.comprehensive_heartbeat_interval_ms());

    let mut fast_ticker = tokio::time::interval(fast_interval);
    fast_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut comprehensive_ticker = tokio::time::interval(comprehensive_interval);
    comprehensive_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        fast_interval_ms = fast_interval.as_millis(),
        comprehensive_interval_ms = comprehensive_interval.as_millis(),
        "GC task: Entering dual heartbeat loop"
    );

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Shutting down");
                break;
            }
            _ = fast_ticker.tick() => {
                let snapshot = metrics.snapshot();

                if let Err(e) = gc_client
                    .fast_heartbeat(snapshot.meetings, snapshot.participants, HealthStatus::Healthy)
                    .await
                {
                    handle_heartbeat_error(&gc_client, e).await;
                }
            }
            _ = comprehensive_ticker.tick() => {
                let snapshot = metrics.snapshot();
                let sys_info = gather_system_info();

                // CPU and memory are 0-100, no precision loss in f32 range
                #[allow(clippy::cast_precision_loss)]
                let cpu = sys_info.cpu_percent as f32;
                #[allow(clippy::cast_precision_loss)]
                let memory = sys_info.memory_percent as f32;

                if let Err(e) = gc_client
                    .comprehensive_heartbeat(
                        snapshot.meetings,
                        snapshot.participants,
                        HealthStatus::Healthy,
                        cpu,
                        memory,
                    )
                    .await
                {
                    handle_heartbeat_error(&gc_client, e).await;
                }
            }
        }
    }

    info!("GC task: Stopped");
}

/// Handle heartbeat errors, including re-registration on `NOT_FOUND`.
///
/// Never exits - logs error and attempts re-registration if needed.
async fn handle_heartbeat_error(gc_client: &GcClient, error: McError) {
    match error {
        McError::NotRegistered => {
            // GC doesn't recognize this MC (e.g., after GC restart)
            // Attempt re-registration (single attempt, task loop will retry)
            warn!("Heartbeat failed: MC not registered with GC, attempting re-registration");

            if let Err(e) = gc_client.attempt_reregistration().await {
                warn!(error = %e, "Re-registration failed, will retry on next heartbeat");
            } else {
                info!("Re-registration successful");
            }
        }
        other => {
            // Other errors (network, timeout, etc.) - log and continue
            warn!(error = %other, "Heartbeat failed");
        }
    }
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM).
///
/// # Panics
///
/// Panics if signal handlers cannot be installed. This is acceptable because
/// without signal handlers, we cannot gracefully shut down the service.
async fn shutdown_signal() {
    let ctrl_c = async {
        #[expect(
            clippy::expect_used,
            reason = "Signal handler installation is critical - panic is appropriate if it fails"
        )]
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        #[expect(
            clippy::expect_used,
            reason = "Signal handler installation is critical - panic is appropriate if it fails"
        )]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
