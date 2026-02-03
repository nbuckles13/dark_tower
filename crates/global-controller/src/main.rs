//! Global Controller
//!
//! Entry point for the Dark Tower video conferencing platform.
//! Handles global routing, meeting discovery, and load balancing.
//!
//! # Servers
//!
//! The Global Controller runs two servers:
//! - HTTP/REST API server for client requests (default: 0.0.0.0:8080)
//! - gRPC server for MC registration and heartbeat (default: 0.0.0.0:50051)
//!
//! # Background Tasks
//!
//! - Health checker: Monitors MC heartbeats and marks stale controllers unhealthy

mod auth;
mod config;
mod errors;
mod grpc;
mod handlers;
mod middleware;
mod models;
mod repositories;
mod routes;
mod services;
mod tasks;

use auth::{JwksClient, JwtValidator};
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use config::Config;
use grpc::auth_layer::async_auth::GrpcAuthLayer;
use grpc::{McService, MhService};
use proto_gen::internal::global_controller_service_server::GlobalControllerServiceServer;
use proto_gen::internal::media_handler_registry_service_server::MediaHandlerRegistryServiceServer;
use routes::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tasks::{
    start_assignment_cleanup, start_health_checker, start_mh_health_checker,
    AssignmentCleanupConfig,
};
use tokio::signal;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server as TonicServer;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "global_controller=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Global Controller");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!(
        region = %config.region,
        bind_address = %config.bind_address,
        grpc_bind_address = %config.grpc_bind_address,
        jwt_clock_skew_seconds = config.jwt_clock_skew_seconds,
        mc_staleness_threshold_seconds = config.mc_staleness_threshold_seconds,
        "Configuration loaded successfully"
    );

    // Initialize database connection pool with query timeout
    info!("Connecting to database...");
    let db_url_with_timeout = add_query_timeout(&config.database_url, 5);
    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(&db_url_with_timeout)
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            e
        })?;

    info!("Database connection established");

    // Parse bind addresses before moving config
    let http_bind_address = config.bind_address.clone();
    let grpc_bind_address = config.grpc_bind_address.clone();
    let mc_staleness_threshold = config.mc_staleness_threshold_seconds;

    // Spawn TokenManager for OAuth 2.0 client credentials flow
    let token_config = TokenManagerConfig::new(
        config.ac_internal_url.clone(),
        config.gc_client_id.clone(),
        config.gc_client_secret.clone(),
    );

    info!("Starting token manager...");
    let (token_task_handle, token_rx): (JoinHandle<()>, _) =
        tokio::time::timeout(Duration::from_secs(30), spawn_token_manager(token_config))
            .await
            .map_err(|_| "Token manager startup timed out after 30 seconds")?
            .map_err(|e| format!("Token manager failed to start: {}", e))?;

    info!("Token manager started successfully");

    // Create MC client for GC->MC communication
    let mc_client: Arc<dyn services::McClientTrait> =
        Arc::new(services::McClient::new(token_rx.clone()));

    // Create application state
    let state = Arc::new(AppState {
        pool: db_pool.clone(),
        config,
        mc_client,
        token_receiver: token_rx,
    });

    // Create JWT validator for gRPC auth
    let jwks_client = Arc::new(JwksClient::new(state.config.ac_jwks_url.clone()));
    let jwt_validator = Arc::new(JwtValidator::new(
        jwks_client,
        state.config.jwt_clock_skew_seconds,
    ));

    // Build HTTP application routes
    let http_app = routes::build_routes(state.clone());

    // Create gRPC services with auth layer
    let mc_service = McService::new(state.clone());
    let mh_service = MhService::new(Arc::new(db_pool.clone()));
    let grpc_auth_layer = GrpcAuthLayer::new(jwt_validator);

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // Start health checker background task
    let health_checker_pool = db_pool.clone();
    let health_checker_token = cancel_token.clone();
    let health_checker_handle = tokio::spawn(async move {
        start_health_checker(
            health_checker_pool,
            mc_staleness_threshold,
            health_checker_token,
        )
        .await;
    });

    // Start assignment cleanup background task
    let cleanup_pool = db_pool.clone();
    let cleanup_token = cancel_token.clone();
    let cleanup_config = AssignmentCleanupConfig::from_env();
    info!(
        cleanup_interval_seconds = cleanup_config.check_interval_seconds,
        inactivity_hours = cleanup_config.inactivity_hours,
        retention_days = cleanup_config.retention_days,
        "Assignment cleanup configuration loaded"
    );
    let cleanup_handle = tokio::spawn(async move {
        start_assignment_cleanup(cleanup_pool, cleanup_config, cleanup_token).await;
    });

    // Start MH health checker background task
    let mh_health_checker_pool = db_pool.clone();
    let mh_health_checker_token = cancel_token.clone();
    let mh_staleness_threshold = mc_staleness_threshold; // Use same threshold as MC health checker
    let mh_health_checker_handle = tokio::spawn(async move {
        start_mh_health_checker(
            mh_health_checker_pool,
            mh_staleness_threshold,
            mh_health_checker_token,
        )
        .await;
    });

    // Parse HTTP bind address
    let http_addr: SocketAddr = http_bind_address.parse().map_err(|e| {
        error!("Invalid HTTP bind address: {}", e);
        e
    })?;

    // Parse gRPC bind address
    let grpc_addr: SocketAddr = grpc_bind_address.parse().map_err(|e| {
        error!("Invalid gRPC bind address: {}", e);
        e
    })?;

    info!("Global Controller HTTP server listening on {}", http_addr);
    info!("Global Controller gRPC server listening on {}", grpc_addr);

    // Start HTTP server
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    let http_server = axum::serve(
        http_listener,
        http_app.into_make_service_with_connect_info::<SocketAddr>(),
    );

    // Start gRPC server with auth layer and both MC and MH services
    let grpc_server = TonicServer::builder()
        .layer(grpc_auth_layer)
        .add_service(GlobalControllerServiceServer::new(mc_service))
        .add_service(MediaHandlerRegistryServiceServer::new(mh_service))
        .serve(grpc_addr);

    // Run both servers concurrently with graceful shutdown
    let cancel_for_shutdown = cancel_token.clone();
    tokio::select! {
        result = http_server.with_graceful_shutdown(shutdown_signal(cancel_for_shutdown.clone())) => {
            if let Err(e) = result {
                error!("HTTP server error: {}", e);
            }
        }
        result = grpc_server => {
            if let Err(e) = result {
                error!("gRPC server error: {}", e);
            }
        }
    }

    // Cancel background tasks
    cancel_token.cancel();

    // Abort token manager task (it doesn't use CancellationToken)
    token_task_handle.abort();

    // Wait for background tasks to finish
    info!("Waiting for background tasks to complete...");
    if let Err(e) = health_checker_handle.await {
        error!("Health checker task error: {}", e);
    }
    if let Err(e) = cleanup_handle.await {
        error!("Assignment cleanup task error: {}", e);
    }
    if let Err(e) = mh_health_checker_handle.await {
        error!("MH health checker task error: {}", e);
    }

    info!("Global Controller shutdown complete");

    Ok(())
}

/// Listens for shutdown signals (SIGTERM, SIGINT).
/// Returns when a shutdown signal is received and drain period is complete.
/// Also triggers the cancellation token for coordinated shutdown.
async fn shutdown_signal(cancel_token: CancellationToken) {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => info!("Received SIGINT, starting graceful shutdown..."),
            Err(e) => error!("Failed to listen for SIGINT: {}", e),
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
                info!("Received SIGTERM, starting graceful shutdown...");
            }
            Err(e) => {
                error!("Failed to listen for SIGTERM: {}", e);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    // Trigger cancellation for all tasks
    cancel_token.cancel();

    // Graceful shutdown drain period
    let drain_secs: u64 = std::env::var("GC_DRAIN_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    if drain_secs > 0 {
        warn!("Draining connections for {} seconds...", drain_secs);
        tokio::time::sleep(Duration::from_secs(drain_secs)).await;
        info!("Drain period complete");
    } else {
        info!("Skipping drain period (GC_DRAIN_SECONDS=0)");
    }
}

/// Adds statement_timeout to the database URL.
/// This ensures queries don't hang indefinitely.
fn add_query_timeout(url: &str, timeout_secs: u32) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{}{}options=-c%20statement_timeout%3D{}s",
        url, separator, timeout_secs
    )
}
