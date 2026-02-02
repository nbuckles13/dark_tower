//! HTTP routes for Global Controller.
//!
//! Defines the Axum router and application state.

use crate::auth::{JwksClient, JwtValidator};
use crate::config::Config;
use crate::handlers;
use crate::middleware::{require_auth, AuthState};
use crate::services::mc_client::McClientTrait;
use axum::{
    middleware,
    routing::{get, patch, post},
    Router,
};
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

    /// MC client for GC->MC communication.
    pub mc_client: Arc<dyn McClientTrait>,
}

/// Build the application routes.
///
/// Creates an Axum router with:
/// - `/health` - Health check endpoint (database ping) - public, unversioned
/// - `/api/v1/me` - Current user endpoint - requires authentication
/// - `/api/v1/meetings/{code}` - Join meeting (authenticated)
/// - `/api/v1/meetings/{code}/guest-token` - Get guest token (public)
/// - `/api/v1/meetings/{id}/settings` - Update meeting settings (authenticated, host only)
/// - TraceLayer for request logging
/// - 30 second request timeout
pub fn build_routes(state: Arc<AppState>) -> Router {
    // Create JWKS client and JWT validator
    let jwks_client = Arc::new(JwksClient::new(state.config.ac_jwks_url.clone()));
    let jwt_validator = Arc::new(JwtValidator::new(
        jwks_client,
        state.config.jwt_clock_skew_seconds,
    ));
    let auth_state = Arc::new(AuthState { jwt_validator });

    // Public routes (no authentication required)
    let public_routes = Router::new()
        // Health check endpoint (unversioned operational endpoint)
        .route("/health", get(handlers::health_check))
        // Guest token endpoint (public, rate limited)
        .route(
            "/api/v1/meetings/:code/guest-token",
            post(handlers::get_guest_token),
        )
        .with_state(state.clone());

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        // Current user endpoint
        .route("/api/v1/me", get(handlers::get_me))
        // Meeting join endpoint
        .route("/api/v1/meetings/:code", get(handlers::join_meeting))
        // Meeting settings endpoint
        .route(
            "/api/v1/meetings/:id/settings",
            patch(handlers::update_meeting_settings),
        )
        .route_layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_auth,
        ))
        .with_state(state);

    // Merge routes and apply global middleware layers
    // Layer order (bottom-to-top execution):
    // 1. TimeoutLayer - Timeout the request (innermost)
    // 2. TraceLayer - Log request details
    public_routes
        .merge(protected_routes)
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
