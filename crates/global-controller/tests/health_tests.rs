//! Health endpoint integration tests.
//!
//! Tests the `/health` endpoint using the `TestGcServer` harness.

use gc_test_utils::TestGcServer;
use sqlx::PgPool;

/// Test that health endpoint returns 200 and healthy status.
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_returns_200(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", server.url()))
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["region"], "test-region");
    assert_eq!(body["database"], "healthy");

    Ok(())
}

/// Test that health endpoint returns JSON content type.
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_returns_json(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", server.url()))
        .send()
        .await?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());

    assert!(
        content_type.is_some_and(|ct| ct.contains("application/json")),
        "Expected application/json content type, got {:?}",
        content_type
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
