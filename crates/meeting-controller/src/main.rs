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
//! # Startup Flow (ADR-0023 Phase 6c)
//!
//! 1. Load configuration from environment
//! 2. Initialize Redis connection (`FencedRedisClient`)
//! 3. Initialize actor system (`MeetingControllerActorHandle`)
//! 4. Create `GcClient` and register with GC
//! 5. Spawn heartbeat tasks (fast: 10s, comprehensive: 30s)
//! 6. Start gRPC server for GC->MC communication
//! 7. Wait for shutdown signal

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)] // main.rs orchestrates startup, naturally longer

use std::sync::Arc;
use std::time::Duration;

use common::secret::{ExposeSecret, SecretBox};
use meeting_controller::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use meeting_controller::config::Config;
use meeting_controller::grpc::{GcClient, McAssignmentService};
use meeting_controller::redis::FencedRedisClient;
use meeting_controller::system_info::gather_system_info;
use proto_gen::internal::meeting_controller_service_server::MeetingControllerServiceServer;
use proto_gen::internal::HealthStatus;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    // Initialize shared metrics for heartbeat reporting
    let controller_metrics = ControllerMetrics::new();

    // Initialize actor system (Phase 6b)
    info!("Initializing actor system...");
    let actor_metrics = ActorMetrics::new();

    // Create master secret for session binding tokens
    // In production, this should be loaded from secure config
    let master_secret = SecretBox::new(Box::new(vec![0u8; 32])); // TODO: Load from config

    let controller_handle = Arc::new(MeetingControllerActorHandle::new(
        config.mc_id.clone(),
        Arc::clone(&actor_metrics),
        master_secret,
    ));
    info!("Actor system initialized");

    // Create GcClient and register with GC (Phase 6c)
    info!("Connecting to Global Controller...");
    let gc_client = GcClient::new(
        config.gc_grpc_url.clone(),
        config.service_token.clone(),
        config.clone(),
    )
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to connect to GC");
        e
    })?;
    info!("Connected to Global Controller");

    // Register with GC (has exponential backoff built in)
    info!("Registering with Global Controller...");
    gc_client.register().await.map_err(|e| {
        error!(error = %e, "Failed to register with GC");
        e
    })?;
    info!("Registered with Global Controller");

    // Create shutdown token as child of controller's token
    // This ensures heartbeat tasks are cancelled when the controller shuts down
    let shutdown_token = controller_handle.child_token();

    // Spawn heartbeat tasks (Phase 6c)
    // Use child tokens so they're cancelled when controller shuts down
    let gc_client_for_fast = Arc::new(gc_client);
    let gc_client_for_comprehensive = Arc::clone(&gc_client_for_fast);
    let metrics_for_fast = Arc::clone(&controller_metrics);
    let metrics_for_comprehensive = Arc::clone(&controller_metrics);
    let fast_heartbeat_token = shutdown_token.child_token();
    let comprehensive_heartbeat_token = shutdown_token.child_token();

    // Fast heartbeat task (every 10s)
    let fast_interval_ms = gc_client_for_fast.fast_heartbeat_interval_ms();
    tokio::spawn(async move {
        let interval = Duration::from_millis(fast_interval_ms);
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                () = fast_heartbeat_token.cancelled() => {
                    info!("Fast heartbeat task shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    let meetings = metrics_for_fast.meetings();
                    let participants = metrics_for_fast.participants();

                    if let Err(e) = gc_client_for_fast
                        .fast_heartbeat(meetings, participants, HealthStatus::Healthy)
                        .await
                    {
                        warn!(error = %e, "Fast heartbeat failed");
                    }
                }
            }
        }
    });
    info!(
        "Fast heartbeat task started (interval: {}ms)",
        fast_interval_ms
    );

    // Comprehensive heartbeat task (every 30s)
    let comprehensive_interval_ms =
        gc_client_for_comprehensive.comprehensive_heartbeat_interval_ms();
    tokio::spawn(async move {
        let interval = Duration::from_millis(comprehensive_interval_ms);
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                () = comprehensive_heartbeat_token.cancelled() => {
                    info!("Comprehensive heartbeat task shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    let meetings = metrics_for_comprehensive.meetings();
                    let participants = metrics_for_comprehensive.participants();
                    let sys_info = gather_system_info();

                    // CPU and memory are 0-100, no precision loss in f32 range
                    #[allow(clippy::cast_precision_loss)]
                    let cpu = sys_info.cpu_percent as f32;
                    #[allow(clippy::cast_precision_loss)]
                    let memory = sys_info.memory_percent as f32;

                    if let Err(e) = gc_client_for_comprehensive
                        .comprehensive_heartbeat(
                            meetings,
                            participants,
                            HealthStatus::Healthy,
                            cpu,
                            memory,
                        )
                        .await
                    {
                        warn!(error = %e, "Comprehensive heartbeat failed");
                    }
                }
            }
        }
    });
    info!(
        "Comprehensive heartbeat task started (interval: {}ms)",
        comprehensive_interval_ms
    );

    // Create and start gRPC server for GC->MC communication (Phase 6c)
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

    info!("Meeting Controller Phase 6c: GC integration complete");

    // TODO (Phase 6g): Start WebTransport server
    // TODO (Phase 6h): Start health endpoints

    // Wait for shutdown signal
    info!("Meeting Controller running - press Ctrl+C to shutdown");
    shutdown_signal().await;

    // Trigger graceful shutdown via cancellation token
    // This propagates to all child tokens (heartbeats, gRPC server)
    info!("Shutdown signal received, initiating graceful shutdown...");
    shutdown_token.cancel();

    // Give tasks time to shut down
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Shutdown actor system (also cancels via its token)
    if let Err(e) = controller_handle.shutdown(Duration::from_secs(30)).await {
        warn!(error = %e, "Actor system shutdown error");
    }

    info!("Meeting Controller shutdown complete");
    Ok(())
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
