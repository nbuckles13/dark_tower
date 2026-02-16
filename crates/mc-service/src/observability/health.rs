//! Health endpoints for Meeting Controller (ADR-0023)
//!
//! Provides Kubernetes-compatible health endpoints (matching AC pattern):
//! - `GET /health` - Liveness probe (is the process running?)
//! - `GET /ready` - Readiness probe (can we serve traffic?)
//!
//! Note: The `/metrics` endpoint is served separately via `metrics-exporter-prometheus`.
//!
//! # Health State
//!
//! The `HealthState` tracks:
//! - `live`: Always true after startup (process is running)
//! - `ready`: True when MC is registered with GC and can accept meetings
//!
//! # Metrics Endpoint
//!
//! The `/metrics` endpoint is handled by `metrics-exporter-prometheus` and
//! renders all registered metrics in Prometheus text format.

use axum::{extract::State, http::StatusCode, routing::get, Router};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Health state for the Meeting Controller.
///
/// Tracks liveness and readiness for Kubernetes probes.
#[derive(Debug)]
pub struct HealthState {
    /// Whether the service is live (process running).
    /// Always true after startup initialization.
    live: AtomicBool,
    /// Whether the service is ready to serve traffic.
    /// True when registered with GC and can accept meetings.
    ready: AtomicBool,
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthState {
    /// Create a new health state (live=true, ready=false).
    #[must_use]
    pub fn new() -> Self {
        Self {
            live: AtomicBool::new(true),
            ready: AtomicBool::new(false),
        }
    }

    /// Mark the service as ready to serve traffic.
    pub fn set_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Mark the service as not ready (e.g., during shutdown).
    pub fn set_not_ready(&self) {
        self.ready.store(false, Ordering::SeqCst);
    }

    /// Check if the service is live.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }

    /// Check if the service is ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}

/// Create the health router with liveness and readiness endpoints.
///
/// # Endpoints
///
/// - `GET /health` - Returns 200 if process is running (liveness)
/// - `GET /ready` - Returns 200 if ready to serve traffic, 503 otherwise (readiness)
///
/// # Arguments
///
/// * `health_state` - Shared health state for checking readiness
pub fn health_router(health_state: Arc<HealthState>) -> Router {
    Router::new()
        .route("/health", get(liveness_handler))
        .route("/ready", get(readiness_handler))
        .with_state(health_state)
}

/// Liveness probe handler.
///
/// Returns 200 OK if the process is running.
/// Kubernetes uses this to determine if the pod should be restarted.
async fn liveness_handler(State(state): State<Arc<HealthState>>) -> StatusCode {
    if state.is_live() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// Readiness probe handler.
///
/// Returns 200 OK if the service is ready to serve traffic.
/// Returns 503 Service Unavailable if not ready.
/// Kubernetes uses this to determine if the pod should receive traffic.
async fn readiness_handler(State(state): State<Arc<HealthState>>) -> StatusCode {
    if state.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_state_default() {
        let state = HealthState::new();
        assert!(state.is_live(), "Should be live by default");
        assert!(!state.is_ready(), "Should not be ready by default");
    }

    #[test]
    fn test_health_state_set_ready() {
        let state = HealthState::new();

        state.set_ready();
        assert!(state.is_ready(), "Should be ready after set_ready()");

        state.set_not_ready();
        assert!(
            !state.is_ready(),
            "Should not be ready after set_not_ready()"
        );
    }

    #[test]
    fn test_health_state_thread_safety() {
        use std::thread;

        let state = Arc::new(HealthState::new());

        let state_clone = Arc::clone(&state);
        let handle = thread::spawn(move || {
            state_clone.set_ready();
        });

        handle.join().expect("Thread should complete");
        assert!(
            state.is_ready(),
            "State should be updated from another thread"
        );
    }

    #[tokio::test]
    async fn test_liveness_handler_returns_ok() {
        let state = Arc::new(HealthState::new());
        let status = liveness_handler(State(state)).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readiness_handler_returns_unavailable_when_not_ready() {
        let state = Arc::new(HealthState::new());
        let status = readiness_handler(State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_readiness_handler_returns_ok_when_ready() {
        let state = Arc::new(HealthState::new());
        state.set_ready();
        let status = readiness_handler(State(Arc::clone(&state))).await;
        assert_eq!(status, StatusCode::OK);
    }

    // ========================================================================
    // Integration tests for health_router
    // ========================================================================

    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_health_router_liveness_endpoint() {
        let state = Arc::new(HealthState::new());
        let app = health_router(state);

        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .expect("Failed to build request");

        let response = app
            .oneshot(request)
            .await
            .expect("Failed to execute request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "/health should return 200 OK when live"
        );
    }

    #[tokio::test]
    async fn test_health_router_readiness_endpoint_not_ready() {
        let state = Arc::new(HealthState::new());
        let app = health_router(state);

        let request = Request::builder()
            .uri("/ready")
            .body(Body::empty())
            .expect("Failed to build request");

        let response = app
            .oneshot(request)
            .await
            .expect("Failed to execute request");

        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "/ready should return 503 when not ready"
        );
    }

    #[tokio::test]
    async fn test_health_router_readiness_endpoint_ready() {
        let state = Arc::new(HealthState::new());
        state.set_ready();
        let app = health_router(state);

        let request = Request::builder()
            .uri("/ready")
            .body(Body::empty())
            .expect("Failed to build request");

        let response = app
            .oneshot(request)
            .await
            .expect("Failed to execute request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "/ready should return 200 when ready"
        );
    }

    #[tokio::test]
    async fn test_health_router_unknown_path_returns_404() {
        let state = Arc::new(HealthState::new());
        let app = health_router(state);

        let request = Request::builder()
            .uri("/unknown")
            .body(Body::empty())
            .expect("Failed to build request");

        let response = app
            .oneshot(request)
            .await
            .expect("Failed to execute request");

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Unknown paths should return 404"
        );
    }
}
