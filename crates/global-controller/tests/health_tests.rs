//! Health endpoint integration tests.
//!
//! Tests the `/health` (liveness) and `/ready` (readiness) endpoints
//! using the `TestGcServer` harness.
//!
//! Note: `/health` returns plain text "OK" for Kubernetes liveness probes.
//! `/ready` returns JSON with detailed health status for readiness probes.

use gc_test_utils::TestGcServer;
use sqlx::PgPool;

/// Test that /health liveness endpoint returns 200 and plain text "OK".
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_returns_200(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // /health returns plain text "OK" for Kubernetes liveness probes
    let body = response.text().await?;
    assert_eq!(body, "OK");

    Ok(())
}

/// Test that /ready readiness endpoint returns JSON with health details.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_endpoint_returns_json(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client.get(format!("{}/ready", server.url())).send().await?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());

    assert!(
        content_type.is_some_and(|ct| ct.contains("application/json")),
        "Expected application/json content type, got {:?}",
        content_type
    );

    // /ready returns JSON with detailed status
    let body: serde_json::Value = response.json().await?;
    assert!(
        body.get("database").is_some(),
        "Expected 'database' field in response"
    );

    Ok(())
}

/// Test that non-existent routes return 404.
#[sqlx::test(migrations = "../../migrations")]
async fn test_unknown_route_returns_404(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/v1/nonexistent", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 404);

    Ok(())
}
