mod config;
mod crypto;
mod errors;
mod handlers;
mod middleware;
mod models;
mod repositories;
mod routes;
mod services;

use config::Config;
use handlers::auth_handler::AppState;
use services::key_management_service;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};
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

    // Initialize database connection pool
    info!("Connecting to database...");
    let db_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
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

    // Build application routes
    let app = routes::build_routes(state);

    // Parse bind address
    let addr: SocketAddr = bind_address
        .parse()
        .map_err(|e| {
            error!("Invalid bind address: {}", e);
            e
        })?;

    info!("Auth Controller listening on {}", addr);

    // Start server with ConnectInfo support
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
