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

#![warn(clippy::pedantic)]

mod config;
mod errors;

use config::Config;
use tracing::{error, info};
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

    // TODO (Phase 6b): Initialize Redis connection pool
    // TODO (Phase 6b): Initialize actor system
    // TODO (Phase 6c): Register with GC
    // TODO (Phase 6g): Start WebTransport server
    // TODO (Phase 6h): Start health endpoints

    info!("Meeting Controller Phase 6a: Foundation complete");
    info!("Skeleton implementation - full functionality in Phase 6b+");

    Ok(())
}
