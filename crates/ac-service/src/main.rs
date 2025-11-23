mod config;
mod routes;
mod errors;

use config::Config;
use std::net::SocketAddr;
use tracing::{info, error};
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

    // Load configuration
    let config = Config::from_env().map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");

    // TODO Phase 3: Initialize database connection pool
    // let db_pool = sqlx::postgres::PgPoolOptions::new()
    //     .max_connections(5)
    //     .connect(&config.database_url)
    //     .await?;

    // TODO Phase 3: Initialize actors (JwksManagerActor, TokenIssuerActor, KeyRotationActor)

    // Build application routes
    let app = routes::build_routes();

    // Parse bind address
    let addr: SocketAddr = config.bind_address.parse()
        .map_err(|e| {
            error!("Invalid bind address: {}", e);
            e
        })?;

    info!("Auth Controller listening on {}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
