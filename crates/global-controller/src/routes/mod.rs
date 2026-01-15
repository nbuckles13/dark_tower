//! HTTP routes for Global Controller.
//!
//! Defines the Axum router and application state.

use crate::config::Config;
use crate::handlers;
use axum::{routing::get, Router};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

/// Application state shared across all handlers.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub pool: PgPool,

    /// Service configuration.
    pub config: Config,
}

/// Build the application routes.
///
/// Creates an Axum router with:
/// - `/v1/health` - Health check endpoint (database ping)
/// - TraceLayer for request logging
/// - 30 second request timeout
pub fn build_routes(state: Arc<AppState>) -> Router {
    // Public routes (no authentication required)
    let public_routes = Router::new()
        // Health check endpoint
        .route("/v1/health", get(handlers::health_check))
        .with_state(state);

    // Apply global middleware layers
    // Layer order (bottom-to-top execution):
    // 1. TimeoutLayer - Timeout the request (innermost)
    // 2. TraceLayer - Log request details
    public_routes
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_is_clone() {
        // This test verifies that AppState implements Clone,
        // which is required for Axum's State extractor.
        fn assert_clone<T: Clone>() {}
        assert_clone::<AppState>();
    }

    #[test]
    fn test_config_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<Config>();
    }
}
