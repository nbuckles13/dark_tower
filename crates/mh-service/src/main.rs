//! Media Handler
//!
//! SFU (Selective Forwarding Unit) for real-time media routing.
//!
//! # Servers
//!
//! The Media Handler runs multiple servers:
//! - gRPC server for MC→MH communication (default: 0.0.0.0:50053)
//! - HTTP server for health endpoints (default: 0.0.0.0:8083)
//! - WebTransport server for client media (default: 0.0.0.0:4434) — stub
//!
//! # Startup Flow (ADR-0010)
//!
//! 1. Load configuration from environment
//! 2. Initialize Prometheus metrics recorder (ADR-0011)
//! 3. Spawn `TokenManager` for OAuth token acquisition from AC
//! 4. Start health HTTP server (liveness, readiness, metrics)
//! 5. Start gRPC server for MC→MH communication
//! 6. Create `GcClient` and spawn GC task (registration + load reports)
//! 7. Wait for shutdown signal

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use common::jwt::JwksClient;
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use mh_service::auth::MhJwtValidator;
use mh_service::config::Config;
use mh_service::errors::MhError;
use mh_service::grpc::{GcClient, MhAuthLayer, MhMediaService};
use mh_service::observability::{health_router, HealthState};
use mh_service::session::SessionManager;
use mh_service::webtransport::WebTransportServer;
use proto_gen::internal::media_handler_service_server::MediaHandlerServiceServer;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Default timeout for initial token acquisition.
const TOKEN_ACQUISITION_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with JSON structured logging
    // JSON format enables robust parsing in Promtail without brittle regex
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mh_service=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Starting Media Handler");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!(
        region = %config.region,
        handler_id = %config.handler_id,
        grpc_bind_address = %config.grpc_bind_address,
        health_bind_address = %config.health_bind_address,
        webtransport_bind_address = %config.webtransport_bind_address,
        grpc_advertise_address = %config.grpc_advertise_address,
        webtransport_advertise_address = %config.webtransport_advertise_address,
        max_streams = config.max_streams,
        max_connections = config.max_connections,
        register_meeting_timeout_seconds = config.register_meeting_timeout_seconds,
        "Configuration loaded successfully"
    );

    // Initialize Prometheus metrics recorder (ADR-0011)
    // This must happen before any metrics are recorded
    info!("Initializing Prometheus metrics recorder...");
    let prometheus_handle =
        mh_service::observability::metrics::init_metrics_recorder().map_err(|e| {
            error!(error = %e, "Failed to install Prometheus metrics recorder");
            e
        })?;
    info!("Prometheus metrics recorder initialized");

    // Initialize health state
    let health_state = Arc::new(HealthState::new());

    // Create shutdown token
    let shutdown_token = CancellationToken::new();

    // Spawn TokenManager for OAuth token acquisition (ADR-0003)
    info!(
        ac_endpoint = %config.ac_endpoint,
        client_id = %config.client_id,
        "Spawning TokenManager for AC authentication..."
    );

    let token_config = TokenManagerConfig::from_url(
        config.ac_endpoint.clone(),
        config.client_id.clone(),
        config.client_secret.clone(),
    )
    .map_err(|e| {
        error!(error = %e, "Failed to create TokenManager config");
        MhError::TokenAcquisition(format!("TokenManager config error: {e}"))
    })?
    .with_on_refresh(Arc::new(|event| {
        let status = if event.success { "success" } else { "error" };
        let error_type = event.error_category;
        mh_service::observability::metrics::record_token_refresh(
            status,
            error_type,
            event.duration,
        );
    }));

    let (token_task_handle, token_rx) =
        tokio::time::timeout(TOKEN_ACQUISITION_TIMEOUT, spawn_token_manager(token_config))
            .await
            .map_err(|_| {
                error!(
                    timeout_secs = TOKEN_ACQUISITION_TIMEOUT.as_secs(),
                    "Token acquisition timed out - AC may be unreachable"
                );
                MhError::TokenAcquisitionTimeout
            })?
            .map_err(|e| {
                error!(error = %e, "Failed to acquire initial token from AC");
                MhError::TokenAcquisition(format!("Initial token acquisition failed: {e}"))
            })?;

    info!("TokenManager spawned successfully, initial token acquired");

    // Initialize JWKS client for JWT validation (meeting tokens + service tokens)
    info!(
        ac_jwks_url = %config.ac_jwks_url,
        "Initializing JWKS client..."
    );
    let jwks_client = Arc::new(JwksClient::new(config.ac_jwks_url.clone()).map_err(|e| {
        error!(error = %e, "Failed to create JWKS client");
        MhError::Config(format!("JWKS client creation failed: {e}"))
    })?);

    // Create MH JWT validator for meeting tokens (WebTransport connections)
    let jwt_validator = Arc::new(MhJwtValidator::new(Arc::clone(&jwks_client), 300));
    info!("JWKS client and JWT validator initialized");

    // Create session manager for meeting registration and connection tracking
    let session_manager = Arc::new(SessionManager::new());
    info!("Session manager initialized");

    // Start health HTTP server (MUST succeed - fail startup if it doesn't)
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
    // This prevents race condition where MC tries to call MH before server is ready
    let grpc_addr: SocketAddr = config.grpc_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.grpc_bind_address, "Invalid gRPC bind address");
        format!("Invalid gRPC bind address: {e}")
    })?;

    let mh_media_service = MhMediaService::new();
    let auth_layer = MhAuthLayer::new(Arc::clone(&jwks_client), 300);

    let grpc_shutdown_token = shutdown_token.child_token();
    let grpc_server = tonic::transport::Server::builder()
        .layer(auth_layer)
        .add_service(MediaHandlerServiceServer::new(mh_media_service))
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

    // Start WebTransport server BEFORE GC registration (ADR-0010 ordering)
    // This ensures MH can accept client connections before GC starts routing traffic here
    let wt_server = WebTransportServer::new(
        config.webtransport_bind_address.clone(),
        config.tls_cert_path.clone(),
        config.tls_key_path.clone(),
        Arc::clone(&jwt_validator),
        Arc::clone(&session_manager),
        Duration::from_secs(config.register_meeting_timeout_seconds),
        config.max_connections,
        shutdown_token.child_token(),
    );

    let wt_endpoint = wt_server.bind().await.map_err(|e| {
        error!(error = %e, "Failed to bind WebTransport server");
        format!("WebTransport bind failed: {e}")
    })?;

    info!(
        addr = %config.webtransport_bind_address,
        "WebTransport server bound successfully"
    );

    tokio::spawn(async move {
        wt_server.accept_loop(wt_endpoint).await;
    });
    info!(
        addr = %config.webtransport_bind_address,
        "WebTransport accept loop started"
    );

    // Connect to Global Controller
    info!("Connecting to Global Controller...");
    let gc_client = GcClient::new(config.gc_grpc_url.clone(), token_rx.clone(), config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to GC");
            e
        })?;
    info!("Connected to Global Controller");

    // Spawn GC task (registration + load report heartbeats)
    let gc_task_token = shutdown_token.child_token();
    let gc_task_health = Arc::clone(&health_state);
    tokio::spawn(async move {
        run_gc_task(gc_client, gc_task_health, gc_task_token).await;
    });
    info!("GC task started");

    info!("Media Handler running - press Ctrl+C to shutdown");

    // Wait for shutdown signal
    shutdown_signal().await;

    // Trigger graceful shutdown
    info!("Shutdown signal received, initiating graceful shutdown...");

    // Mark as not ready immediately so k8s stops sending traffic
    health_state.set_not_ready();

    shutdown_token.cancel();

    // Give tasks time to shut down (drain window)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Abort TokenManager background task
    info!("Stopping TokenManager...");
    token_task_handle.abort();

    info!("Media Handler shutdown complete");
    Ok(())
}

/// Unified GC task: registration + load report heartbeat loop.
///
/// Never exits on GC connectivity issues - keeps retrying.
async fn run_gc_task(
    gc_client: GcClient,
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
                        health_state.set_ready();
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "GC task: Initial registration failed, will retry");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    // Load report heartbeat loop
    let interval_ms = gc_client.load_report_interval_ms();
    let interval = Duration::from_millis(interval_ms);

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        interval_ms = interval_ms,
        "GC task: Entering load report heartbeat loop"
    );

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Shutting down");
                break;
            }
            _ = ticker.tick() => {
                if let Err(e) = gc_client.send_load_report().await {
                    match e {
                        MhError::NotRegistered => {
                            warn!("Load report failed: MH not registered with GC, attempting re-registration");
                            if let Err(re) = gc_client.attempt_reregistration().await {
                                warn!(error = %re, "Re-registration failed, will retry on next heartbeat");
                            } else {
                                info!("Re-registration successful");
                            }
                        }
                        other => {
                            warn!(error = %other, "Load report failed");
                        }
                    }
                }
            }
        }
    }

    info!("GC task: Stopped");
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
