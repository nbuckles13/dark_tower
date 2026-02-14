//! Media Handler
//!
//! Handles media routing, transcoding, and mixing (SFU architecture).

#![warn(clippy::pedantic)]

use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialize tracing with JSON structured logging
    // JSON format enables robust parsing in Promtail without brittle regex
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "media_handler=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Dark Tower Media Handler");
    info!("Phase 1: Foundation - In Development");
}
