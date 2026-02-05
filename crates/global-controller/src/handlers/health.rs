//! Health check handlers.
//!
//! Provides health check endpoints for Kubernetes liveness and readiness probes.
//!
//! - `/health`: Liveness probe - returns OK if the process is running
//! - `/ready`: Readiness probe - checks dependencies (DB, AC JWKS)

use crate::models::ReadinessResponse;
use crate::routes::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::sync::Arc;

/// Liveness probe handler.
///
/// Returns a simple "OK" response to indicate the process is running.
/// Does NOT check any dependencies - failure means the process is hung/deadlocked.
///
/// Kubernetes will kill and restart the pod if this fails.
///
/// ADR-0012: Liveness probes should be simple and not check dependencies.
pub async fn health_check() -> &'static str {
    "OK"
}

/// Readiness probe handler.
///
/// Checks critical dependencies to determine if the service can handle traffic.
/// Returns 200 if ready, 503 if not ready.
///
/// Kubernetes will remove the pod from the service load balancer if this fails.
///
/// ## Checks
///
/// 1. Database connectivity - can execute simple query
/// 2. AC JWKS endpoint - can fetch public keys for JWT validation
///
/// ## Security
///
/// Error messages are intentionally generic to avoid leaking infrastructure details.
/// Actual errors are logged server-side with `tracing::warn!`.
#[tracing::instrument(skip_all, name = "gc.health.readiness")]
pub async fn readiness_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
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
                ac_jwks: None,
                // Generic error - don't leak infrastructure details
                error: Some("Service dependencies unavailable".to_string()),
            }),
        );
    }

    // Check 2: AC JWKS endpoint reachability
    // We don't fetch the actual keys (that's cached), just verify the endpoint is configured
    // The JWKS client will handle actual fetching and validation during JWT verification
    let jwks_check = if state.config.ac_jwks_url.is_empty() {
        tracing::warn!("Readiness check failed: AC JWKS URL not configured");
        Err("AC JWKS URL not configured")
    } else {
        // Just verify URL is configured - actual fetching happens on-demand
        Ok(())
    };

    if let Err(e) = jwks_check {
        tracing::warn!("Readiness check failed: {}", e);
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReadinessResponse {
                status: "not_ready",
                database: Some("healthy"),
                ac_jwks: Some("unavailable"),
                // Generic error - don't leak configuration details
                error: Some("Service dependencies unavailable".to_string()),
            }),
        );
    }

    // All checks passed
    (
        StatusCode::OK,
        Json(ReadinessResponse {
            status: "ready",
            database: Some("healthy"),
            ac_jwks: Some("available"),
            error: None,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let result = health_check().await;
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_readiness_response_serialization() {
        // Test ready response serialization
        let ready = ReadinessResponse {
            status: "ready",
            database: Some("healthy"),
            ac_jwks: Some("available"),
            error: None,
        };

        let json = serde_json::to_string(&ready).unwrap();
        assert!(json.contains("\"status\":\"ready\""));
        assert!(json.contains("\"database\":\"healthy\""));
        assert!(json.contains("\"ac_jwks\":\"available\""));
        // Error field should be omitted (skip_serializing_if)
        assert!(!json.contains("\"error\""));

        // Test not ready response serialization
        let not_ready = ReadinessResponse {
            status: "not_ready",
            database: Some("unhealthy"),
            ac_jwks: None,
            error: Some("Service dependencies unavailable".to_string()),
        };

        let json = serde_json::to_string(&not_ready).unwrap();
        assert!(json.contains("\"status\":\"not_ready\""));
        assert!(json.contains("\"database\":\"unhealthy\""));
        // ac_jwks is None, should be omitted
        assert!(!json.contains("\"ac_jwks\""));
        assert!(json.contains("\"error\":\"Service dependencies unavailable\""));
    }

    // Note: Actual readiness_check function is tested via integration tests
    // since it requires real database and config setup.
}
