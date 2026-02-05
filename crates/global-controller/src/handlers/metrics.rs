//! Prometheus metrics endpoint handler.
//!
//! Provides `/metrics` endpoint for Prometheus scraping per ADR-0011.
//!
//! # Security
//!
//! This endpoint is unauthenticated to allow Prometheus to scrape metrics.
//! No PII or secrets are exposed in metrics. Only operational data with
//! bounded cardinality labels.

use axum::{extract::State, response::IntoResponse};
use metrics_exporter_prometheus::PrometheusHandle;

/// Handler for GET /metrics
///
/// Returns Prometheus-formatted metrics for scraping.
/// This is an operational endpoint, not versioned under /api/v1.
///
/// # Response
///
/// Returns 200 OK with Prometheus text format:
/// ```text
/// # HELP gc_http_requests_total Total HTTP requests
/// # TYPE gc_http_requests_total counter
/// gc_http_requests_total{method="GET",endpoint="/health",status_code="200"} 42
/// ```
#[tracing::instrument(skip_all, name = "gc.metrics.scrape")]
pub async fn metrics_handler(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

#[cfg(test)]
mod tests {
    // Note: Testing the metrics endpoint requires a PrometheusHandle,
    // which can only be created once per process via PrometheusBuilder.
    // Integration tests in health_tests.rs verify the full endpoint.
    //
    // Unit test coverage is provided by the metrics module tests
    // which verify metric recording without the handle.
}
