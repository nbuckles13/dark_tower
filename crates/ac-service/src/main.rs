mod config;
mod crypto;
mod errors;
mod handlers;
mod middleware;
mod models;
mod observability;
mod repositories;
mod routes;
mod services;

use config::Config;
use handlers::auth_handler::AppState;
use services::key_management_service;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ac_service=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Auth Controller");

    // Initialize Prometheus metrics recorder (ADR-0011)
    // Must be done before any metrics are recorded
    let metrics_handle = routes::init_metrics_recorder().map_err(|e| {
        error!("Failed to initialize metrics recorder: {}", e);
        e
    })?;
    info!("Prometheus metrics recorder initialized");

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");

    // Initialize database connection pool with query timeout
    // ADR-0012: 5s statement timeout to fail fast on hung queries
    info!("Connecting to database...");
    let db_url_with_timeout = add_query_timeout(&config.database_url, 5);
    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20) // ADR-0012: Increased from 5 to 20 for production capacity
        .min_connections(2) // Keep warm connections to reduce latency
        .acquire_timeout(Duration::from_secs(5)) // Fail fast on connection issues
        .idle_timeout(Duration::from_secs(600)) // 10 minutes
        .max_lifetime(Duration::from_secs(1800)) // 30 minutes
        .connect(&db_url_with_timeout)
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            e
        })?;

    info!("Database connection established");

    // Initialize signing key if none exists
    info!("Initializing signing keys...");
    let cluster_name = std::env::var("CLUSTER_NAME").unwrap_or_else(|_| "us".to_string());

    key_management_service::initialize_signing_key(&db_pool, &config.master_key, &cluster_name)
        .await
        .map_err(|e| {
            error!("Failed to initialize signing key: {}", e);
            e
        })?;

    info!("Signing keys initialized");

    // Parse bind address before moving config
    let bind_address = config.bind_address.clone();

    // Create application state
    let state = Arc::new(AppState {
        pool: db_pool,
        config,
    });

    // Build application routes with HTTP request timeout
    // ADR-0012: 30s request timeout to prevent hung connections
    // ADR-0011: metrics_handle enables /metrics endpoint for Prometheus scraping
    let app = routes::build_routes(state, metrics_handle);

    // Parse bind address
    let addr: SocketAddr = bind_address.parse().map_err(|e| {
        error!("Invalid bind address: {}", e);
        e
    })?;

    info!("Auth Controller listening on {}", addr);

    // Start server with graceful shutdown support
    // ADR-0012: 30s graceful shutdown drain period
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    info!("Auth Controller shutdown complete");

    Ok(())
}

/// Listens for shutdown signals (SIGTERM, SIGINT)
/// Returns when a shutdown signal is received and drain period is complete
///
/// ADR-0012: Graceful shutdown with 30s drain period
async fn shutdown_signal() {
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
                // Fall through - ctrl_c will still work
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

    // ADR-0012: Graceful shutdown drain period
    // K8s sends SIGTERM, then waits terminationGracePeriodSeconds (35s in our StatefulSet)
    // We use 30s in production to allow 5s buffer for final cleanup
    //
    // During this period:
    // - axum stops accepting new connections (handled by with_graceful_shutdown)
    // - Existing connections are allowed to complete
    // - K8s removes us from service endpoints (readiness probe fails after SIGTERM)
    //
    // For local development, use AC_DRAIN_SECONDS=0 to exit immediately
    let drain_secs: u64 = std::env::var("AC_DRAIN_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    if drain_secs > 0 {
        warn!("Draining connections for {} seconds...", drain_secs);
        tokio::time::sleep(Duration::from_secs(drain_secs)).await;
        info!("Drain period complete");
    } else {
        info!("Skipping drain period (AC_DRAIN_SECONDS=0)");
    }
}

/// Adds statement_timeout to the database URL
/// This ensures queries don't hang indefinitely
fn add_query_timeout(url: &str, timeout_secs: u32) -> String {
    let separator = if url.contains('?') { '&' } else { '?' };
    format!(
        "{}{}options=-c%20statement_timeout%3D{}s",
        url, separator, timeout_secs
    )
}
