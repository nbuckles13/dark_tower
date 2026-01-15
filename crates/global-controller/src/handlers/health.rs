//! Health check handler.
//!
//! Provides health check endpoints for liveness and readiness probes.

use crate::errors::GcError;
use crate::models::HealthResponse;
use crate::routes::AppState;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use tracing::instrument;

/// Health check handler.
///
/// Pings the database to verify connectivity and returns the service status.
///
/// ## Response
///
/// Returns a JSON response with:
/// - `status`: "healthy" if database is reachable, "unhealthy" otherwise
/// - `region`: The deployment region from configuration
/// - `database`: "healthy" if DB ping succeeds (omitted on failure)
///
/// ## Example Response
///
/// ```json
/// {
///   "status": "healthy",
///   "region": "us-east-1",
///   "database": "healthy"
/// }
/// ```
#[instrument(skip_all, name = "gc.health.check")]
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthResponse>, GcError> {
    // Ping database to verify connectivity
    let db_healthy = sqlx::query("SELECT 1").fetch_one(&state.pool).await.is_ok();

    let response = if db_healthy {
        HealthResponse {
            status: "healthy".to_string(),
            region: state.config.region.clone(),
            database: Some("healthy".to_string()),
        }
    } else {
        // Return unhealthy status but don't error out - K8s needs to see the response
        HealthResponse {
            status: "unhealthy".to_string(),
            region: state.config.region.clone(),
            database: Some("unhealthy".to_string()),
        }
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Unit test with mocked state would require more infrastructure.
    // The actual handler is tested via integration tests in health_tests.rs.

    #[test]
    fn test_health_response_structure() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            region: "us-east-1".to_string(),
            database: Some("healthy".to_string()),
        };

        assert_eq!(response.status, "healthy");
        assert_eq!(response.region, "us-east-1");
        assert_eq!(response.database, Some("healthy".to_string()));
    }
}
