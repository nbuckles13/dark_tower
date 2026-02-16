//! HTTP routes for Global Controller.
//!
//! Defines the Axum router and application state.

use crate::auth::{JwksClient, JwtValidator};
use crate::config::Config;
use crate::handlers;
use crate::middleware::{http_metrics_middleware, require_auth, AuthState};
use crate::services::mc_client::McClientTrait;
use axum::{
    middleware,
    routing::{get, patch, post},
    Router,
};
use common::token_manager::TokenReceiver;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
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

    /// Token receiver for dynamically refreshed OAuth tokens from TokenManager.
    pub token_receiver: TokenReceiver,
}

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets:
/// - HTTP request p95 < 200ms
/// - MC assignment p95 < 20ms
/// - DB queries p99 < 50ms
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    use metrics_exporter_prometheus::Matcher;

    PrometheusBuilder::new()
        // HTTP request buckets aligned with 200ms p95 SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("gc_http_request".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.300, 0.500, 1.000, 2.000,
            ],
        )
        .map_err(|e| format!("Failed to set HTTP request buckets: {e}"))?
        // MC assignment buckets aligned with 20ms p95 SLO target (ADR-0010)
        .set_buckets_for_metric(
            Matcher::Prefix("gc_mc_assignment".to_string()),
            &[
                0.005, 0.010, 0.015, 0.020, 0.030, 0.050, 0.100, 0.250, 0.500,
            ],
        )
        .map_err(|e| format!("Failed to set MC assignment buckets: {e}"))?
        // DB query buckets aligned with 50ms p99 SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("gc_db_query".to_string()),
            &[
                0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set DB query buckets: {e}"))?
        // gRPC MC call buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_grpc_mc".to_string()),
            &[
                0.005, 0.010, 0.025, 0.050, 0.100, 0.200, 0.500, 1.000, 2.500,
            ],
        )
        .map_err(|e| format!("Failed to set gRPC MC buckets: {e}"))?
        // MH selection buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_mh_selection".to_string()),
            &[0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250],
        )
        .map_err(|e| format!("Failed to set MH selection buckets: {e}"))?
        // Token refresh buckets
        .set_buckets_for_metric(
            Matcher::Prefix("gc_token_refresh".to_string()),
            &[0.010, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000],
        )
        .map_err(|e| format!("Failed to set token refresh buckets: {e}"))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {e}"))
}

/// Build the application routes.
///
/// Creates an Axum router with:
/// - `/health` - Liveness probe (simple "OK") - public, unversioned
/// - `/ready` - Readiness probe (checks DB + AC JWKS) - public, unversioned
/// - `/metrics` - Prometheus metrics endpoint (ADR-0011) - public, unversioned
/// - `/api/v1/me` - Current user endpoint - requires authentication
/// - `/api/v1/meetings/{code}` - Join meeting (authenticated)
/// - `/api/v1/meetings/{code}/guest-token` - Get guest token (public)
/// - `/api/v1/meetings/{id}/settings` - Update meeting settings (authenticated, host only)
/// - TraceLayer for request logging
/// - HTTP metrics middleware (ADR-0011)
/// - 30 second request timeout
pub fn build_routes(state: Arc<AppState>, metrics_handle: PrometheusHandle) -> Router {
    // Create JWKS client and JWT validator
    let jwks_client = Arc::new(JwksClient::new(state.config.ac_jwks_url.clone()));
    let jwt_validator = Arc::new(JwtValidator::new(
        jwks_client,
        state.config.jwt_clock_skew_seconds,
    ));
    let auth_state = Arc::new(AuthState { jwt_validator });

    // Public routes (no authentication required)
    let public_routes = Router::new()
        // Health check endpoints (unversioned operational endpoints)
        .route("/health", get(handlers::health_check))
        .route("/ready", get(handlers::readiness_check))
        // Guest token endpoint (public, rate limited)
        .route(
            "/api/v1/meetings/:code/guest-token",
            post(handlers::get_guest_token),
        )
        .with_state(state.clone());

    // Metrics route with its own state (ADR-0011)
    let metrics_routes = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(metrics_handle);

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
    // 3. http_metrics_middleware - Record ALL responses (outermost)
    public_routes
        .merge(metrics_routes)
        .merge(protected_routes)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        // HTTP metrics layer (outermost) - captures ALL responses including
        // framework-level errors like 415, 400, 404, 405
        .layer(middleware::from_fn(http_metrics_middleware))
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
