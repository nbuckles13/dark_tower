//! Global Controller
//!
//! Entry point for the Dark Tower video conferencing platform.
//! Handles global routing, meeting discovery, and load balancing.

mod config;
mod errors;
mod handlers;
mod models;
mod routes;

use config::Config;
use routes::AppState;
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
        jwt_clock_skew_seconds = config.jwt_clock_skew_seconds,
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

    // Parse bind address before moving config
    let bind_address = config.bind_address.clone();

    // Create application state
    let state = Arc::new(AppState {
        pool: db_pool,
        config,
    });

    // Build application routes
    let app = routes::build_routes(state);

    // Parse bind address
    let addr: SocketAddr = bind_address.parse().map_err(|e| {
        error!("Invalid bind address: {}", e);
        e
    })?;

    info!("Global Controller listening on {}", addr);

    // Start server with graceful shutdown support
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    info!("Global Controller shutdown complete");

    Ok(())
}

/// Listens for shutdown signals (SIGTERM, SIGINT).
/// Returns when a shutdown signal is received and drain period is complete.
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
