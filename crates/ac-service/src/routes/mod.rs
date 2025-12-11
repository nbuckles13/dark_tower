use crate::handlers::{admin_handler, auth_handler, jwks_handler};
use crate::middleware::auth::{require_admin_scope, AuthMiddlewareState};
use crate::repositories::signing_keys;
use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

/// Initialize Prometheus metrics recorder and return the handle
/// for serving metrics via HTTP.
///
/// ADR-0011: Must be called before any metrics are recorded.
/// Configures histogram buckets aligned with SLO targets:
/// - Token issuance p99 < 350ms
/// - DB queries p99 < 50ms
///
/// # Errors
///
/// Returns error if Prometheus recorder fails to install (e.g., already installed).
pub fn init_metrics_recorder() -> Result<PrometheusHandle, String> {
    use metrics_exporter_prometheus::Matcher;

    PrometheusBuilder::new()
        // Token issuance buckets aligned with 350ms SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("ac_token_issuance".to_string()),
            &[
                0.010, 0.025, 0.050, 0.100, 0.150, 0.200, 0.250, 0.300, 0.350, 0.500, 1.000, 2.000,
            ],
        )
        .map_err(|e| format!("Failed to set token issuance buckets: {}", e))?
        // DB query buckets aligned with 50ms SLO target
        .set_buckets_for_metric(
            Matcher::Prefix("ac_db_query".to_string()),
            &[
                0.001, 0.002, 0.005, 0.010, 0.020, 0.050, 0.100, 0.250, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set DB query buckets: {}", e))?
        // Bcrypt buckets - coarse (50ms minimum) to prevent timing side-channel attacks
        .set_buckets_for_metric(
            Matcher::Prefix("ac_bcrypt".to_string()),
            &[
                0.050, 0.100, 0.150, 0.200, 0.250, 0.300, 0.400, 0.500, 1.000,
            ],
        )
        .map_err(|e| format!("Failed to set bcrypt buckets: {}", e))?
        .install_recorder()
        .map_err(|e| format!("Failed to install Prometheus recorder: {}", e))
}

pub fn build_routes(
    state: Arc<auth_handler::AppState>,
    metrics_handle: PrometheusHandle,
) -> Router {
    // Create auth middleware state
    let auth_state = Arc::new(AuthMiddlewareState {
        pool: state.pool.clone(),
    });

    // Admin routes that require authentication with admin:services scope
    let admin_routes = Router::new()
        .route(
            "/api/v1/admin/services/register",
            post(admin_handler::handle_register_service),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            require_admin_scope,
        ))
        .with_state(state.clone());

    // Internal routes (key rotation) - authentication handled in handler
    // (requires rotation-specific scopes, not admin:services)
    let internal_routes = Router::new()
        .route(
            "/internal/rotate-keys",
            post(admin_handler::handle_rotate_keys),
        )
        .with_state(state.clone());

    // Metrics route with its own state (ADR-0011)
    let metrics_routes = Router::new()
        .route("/metrics", get(metrics_endpoint))
        .with_state(metrics_handle);

    // Public routes (no authentication required)
    let public_routes = Router::new()
        // OAuth 2.0 authentication endpoints
        .route(
            "/api/v1/auth/user/token",
            post(auth_handler::handle_user_token),
        )
        .route(
            "/api/v1/auth/service/token",
            post(auth_handler::handle_service_token),
        )
        // JWKS endpoint (RFC 8414 well-known path, no /api/v1 prefix)
        .route("/.well-known/jwks.json", get(jwks_handler::handle_get_jwks))
        // Health check (liveness probe) - simple response, always returns OK if process is running
        .route("/health", get(health_check))
        // Readiness probe - verifies DB connectivity and signing key availability
        // ADR-0012: K8s should only route traffic when service is ready
        .route("/ready", get(readiness_check))
        .with_state(state);

    // Merge routes with global layers
    admin_routes
        .merge(internal_routes)
        .merge(metrics_routes)
        .merge(public_routes)
        .layer(TraceLayer::new_for_http())
        // ADR-0012: 30s HTTP request timeout to prevent hung connections
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
}

/// Liveness probe - returns OK if the process is running
/// Used by K8s livenessProbe to detect hung processes
async fn health_check() -> &'static str {
    "OK"
}

/// Readiness response structure
#[derive(Serialize)]
struct ReadinessResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signing_key: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Readiness probe - verifies service dependencies are available
/// Used by K8s readinessProbe to gate traffic routing
///
/// Checks:
/// 1. Database connectivity (can execute simple query)
/// 2. Signing key availability (active key exists for token issuance)
///
/// Returns 200 OK if all checks pass, 503 Service Unavailable otherwise
///
/// Security: Error messages are intentionally generic to avoid leaking
/// infrastructure details. Actual errors are logged server-side.
async fn readiness_check(State(state): State<Arc<auth_handler::AppState>>) -> impl IntoResponse {
    // Check 1: Database connectivity
    let db_check = sqlx::query("SELECT 1").fetch_one(&state.pool).await;

    if let Err(e) = db_check {
        // Log actual error server-side for operators
        tracing::warn!("Readiness check failed: database error: {}", e);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                status: "not_ready",
                database: Some("unhealthy"),
                signing_key: None,
                // Generic error - don't leak infrastructure details
                error: Some("Service dependencies unavailable".to_string()),
            }),
        );
    }

    // Check 2: Active signing key availability
    let key_check = signing_keys::get_active_key(&state.pool).await;

    match key_check {
        Ok(Some(_)) => {
            // All checks passed
            (
                StatusCode::OK,
                Json(ReadinessResponse {
                    status: "ready",
                    database: Some("healthy"),
                    signing_key: Some("available"),
                    error: None,
                }),
            )
        }
        Ok(None) => {
            // Log actual issue server-side
            tracing::warn!("Readiness check failed: no active signing key");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadinessResponse {
                    status: "not_ready",
                    database: Some("healthy"),
                    signing_key: Some("unavailable"),
                    // Generic error - don't leak key rotation state
                    error: Some("Service dependencies unavailable".to_string()),
                }),
            )
        }
        Err(e) => {
            // Log actual error server-side
            tracing::warn!("Readiness check failed: signing key error: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadinessResponse {
                    status: "not_ready",
                    database: Some("healthy"),
                    signing_key: Some("error"),
                    // Generic error - don't leak infrastructure details
                    error: Some("Service dependencies unavailable".to_string()),
                }),
            )
        }
    }
}

/// Prometheus metrics endpoint (ADR-0011)
///
/// Returns metrics in Prometheus text format for scraping.
/// This endpoint is unauthenticated to allow Prometheus to scrape metrics.
///
/// Security: No PII or secrets are exposed in metrics. Only
/// operational data with bounded cardinality labels.
async fn metrics_endpoint(State(handle): State<PrometheusHandle>) -> String {
    handle.render()
}
