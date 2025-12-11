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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use sqlx::PgPool;

    /// Test-only version of ReadinessResponse with owned strings for deserialization
    #[derive(serde::Deserialize)]
    struct ReadinessResponseOwned {
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        database: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        signing_key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    }

    #[tokio::test]
    async fn test_health_check() {
        let result = health_check().await;
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_init_metrics_recorder_does_not_panic() {
        // NOTE: init_metrics_recorder() can only succeed once per process
        // due to global Prometheus recorder installation. Subsequent calls
        // will return Err() once a recorder is installed.
        //
        // This test verifies the function doesn't panic, regardless of
        // whether it succeeds or fails due to already-installed recorder.
        let result = init_metrics_recorder();

        // Either succeeds (first call in process) or fails gracefully
        match result {
            Ok(handle) => {
                // Verify handle can render (basic smoke test)
                let metrics = handle.render();
                assert!(
                    metrics.is_empty() || metrics.contains('#'),
                    "Metrics should be empty or contain Prometheus format markers"
                );
            }
            Err(e) => {
                // Verify error message is descriptive
                assert!(
                    e.contains("Prometheus") || e.contains("install") || e.contains("bucket"),
                    "Error message should mention Prometheus or installation: {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        // Initialize metrics recorder (may fail if already installed in process)
        let handle = match init_metrics_recorder() {
            Ok(h) => h,
            Err(_) => {
                // If recorder already installed, we can't test this in isolation.
                // This is expected when running multiple tests in the same process.
                // The function is also covered by E2E tests in server_harness.
                return;
            }
        };

        // Call the endpoint handler
        let result = metrics_endpoint(State(handle)).await;

        // Verify metrics are in Prometheus text format
        // Empty is valid (no metrics recorded yet)
        // Non-empty should contain Prometheus format markers (# for comments/metadata)
        assert!(
            result.is_empty() || result.contains('#'),
            "Metrics should be in Prometheus text format"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_readiness_check_healthy(pool: PgPool) -> Result<(), anyhow::Error> {
        use crate::services::key_management_service;
        use ac_test_utils::crypto_fixtures::test_master_key;

        // Initialize signing key so readiness check passes
        let master_key = test_master_key();
        key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster").await?;

        // Create app state
        let config = Config {
            database_url: String::new(),
            bind_address: "127.0.0.1:0".to_string(),
            master_key: master_key.clone(),
            hash_secret: master_key.clone(),
            otlp_endpoint: None,
        };
        let state = Arc::new(auth_handler::AppState {
            pool: pool.clone(),
            config,
        });

        // Call readiness check - it returns impl IntoResponse
        // We need to convert it to a response to inspect it
        let response_impl = readiness_check(State(state)).await;

        // Convert to HTTP response to extract status and body
        use axum::response::IntoResponse;
        let response = response_impl.into_response();

        // Verify status code
        assert_eq!(response.status(), StatusCode::OK);

        // Extract and parse body
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let body_str = String::from_utf8(body_bytes.to_vec())?;
        let body: ReadinessResponseOwned = serde_json::from_str(&body_str)?;

        // Verify response indicates healthy state
        assert_eq!(body.status, "ready");
        assert_eq!(body.database, Some("healthy".to_string()));
        assert_eq!(body.signing_key, Some("available".to_string()));
        assert_eq!(body.error, None);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_readiness_check_no_signing_key(pool: PgPool) -> Result<(), anyhow::Error> {
        // DO NOT initialize signing key - this simulates missing key scenario

        // Create app state
        let config = Config {
            database_url: String::new(),
            bind_address: "127.0.0.1:0".to_string(),
            master_key: vec![0u8; 32],  // Dummy key (won't be used)
            hash_secret: vec![0u8; 32], // Dummy hash secret for tests
            otlp_endpoint: None,
        };
        let state = Arc::new(auth_handler::AppState {
            pool: pool.clone(),
            config,
        });

        // Call readiness check - it returns impl IntoResponse
        let response_impl = readiness_check(State(state)).await;

        // Convert to HTTP response to extract status and body
        use axum::response::IntoResponse;
        let response = response_impl.into_response();

        // Verify status code
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        // Extract and parse body
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let body_str = String::from_utf8(body_bytes.to_vec())?;
        let body: ReadinessResponseOwned = serde_json::from_str(&body_str)?;

        // Verify response indicates unhealthy state due to missing signing key
        assert_eq!(body.status, "not_ready");
        assert_eq!(body.database, Some("healthy".to_string()));
        assert_eq!(body.signing_key, Some("unavailable".to_string()));
        assert_eq!(
            body.error,
            Some("Service dependencies unavailable".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_readiness_response_serialization() {
        // Test healthy response serialization
        let healthy = ReadinessResponse {
            status: "ready",
            database: Some("healthy"),
            signing_key: Some("available"),
            error: None,
        };

        let json = serde_json::to_string(&healthy).unwrap();
        assert!(json.contains("\"status\":\"ready\""));
        assert!(json.contains("\"database\":\"healthy\""));
        assert!(json.contains("\"signing_key\":\"available\""));
        // Error field should be omitted (skip_serializing_if)
        assert!(!json.contains("\"error\""));

        // Test unhealthy response serialization
        let unhealthy = ReadinessResponse {
            status: "not_ready",
            database: Some("unhealthy"),
            signing_key: None,
            error: Some("Service dependencies unavailable".to_string()),
        };

        let json = serde_json::to_string(&unhealthy).unwrap();
        assert!(json.contains("\"status\":\"not_ready\""));
        assert!(json.contains("\"database\":\"unhealthy\""));
        // signing_key is None, should be omitted
        assert!(!json.contains("\"signing_key\""));
        assert!(json.contains("\"error\":\"Service dependencies unavailable\""));
    }

    // Note: build_routes() is tested via E2E tests in ac-test-utils/server_harness.rs
    // Integration testing with actual HTTP server is more appropriate for route assembly.
}
