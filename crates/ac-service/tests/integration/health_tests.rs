//! P1 Integration tests for health probes (ADR-0012)
//!
//! These tests validate the liveness and readiness probe endpoints that K8s
//! uses to determine service health and traffic routing eligibility.

use ac_service::services::key_management_service;
use ac_test_utils::TestAuthServer;
use reqwest::StatusCode;
use sqlx::PgPool;

// ============================================================================
// Liveness Probe Tests
// ============================================================================

/// P1-1: Test /health returns 200 OK
///
/// The liveness probe should always return 200 OK as long as the process is
/// running and able to handle HTTP requests.
#[sqlx::test(migrations = "../../migrations")]
async fn test_health_endpoint_returns_ok(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health", server.url()))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Health check should return 200 OK"
    );

    let body = response.text().await?;
    assert_eq!(body, "OK", "Health check body should be 'OK'");

    Ok(())
}

// ============================================================================
// Readiness Probe Tests
// ============================================================================

/// P1-2: Test /ready returns 200 when DB is healthy and signing key exists
///
/// The readiness probe verifies that:
/// 1. Database is reachable (can execute queries)
/// 2. An active signing key is available (can issue tokens)
///
/// Only when both checks pass should K8s route traffic to the pod.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_endpoint_returns_ok_when_healthy(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;

    // Act
    let client = reqwest::Client::new();
    let response = client.get(format!("{}/ready", server.url())).send().await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Readiness check should return 200 OK when healthy"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["status"].as_str(),
        Some("ready"),
        "Status should be 'ready'"
    );
    assert_eq!(
        body["database"].as_str(),
        Some("healthy"),
        "Database should be 'healthy'"
    );
    assert_eq!(
        body["signing_key"].as_str(),
        Some("available"),
        "Signing key should be 'available'"
    );
    assert!(body["error"].is_null(), "Error field should not be present");

    Ok(())
}

/// P1-3: Test /ready returns 503 when no signing key exists
///
/// If there is no active signing key, the service cannot issue tokens.
/// It should report as not ready so K8s doesn't route traffic to it.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_returns_503_when_no_signing_key(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange - Spawn server but manually clear signing keys
    let server = TestAuthServer::spawn(pool.clone()).await?;

    // Delete all signing keys to simulate "no signing key" state
    sqlx::query("DELETE FROM signing_keys")
        .execute(&pool)
        .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client.get(format!("{}/ready", server.url())).send().await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Readiness check should return 503 when no signing key"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["status"].as_str(),
        Some("not_ready"),
        "Status should be 'not_ready'"
    );
    assert_eq!(
        body["database"].as_str(),
        Some("healthy"),
        "Database should still be 'healthy'"
    );
    assert_eq!(
        body["signing_key"].as_str(),
        Some("unavailable"),
        "Signing key should be 'unavailable'"
    );
    assert!(
        body["error"].as_str().is_some(),
        "Error message should be present"
    );

    Ok(())
}

/// P1-4: Test readiness recovers after signing key is created
///
/// Validates that if the service starts without a signing key, it can
/// recover and become ready once a key is initialized.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_recovers_after_key_creation(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange - Spawn server
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Delete all signing keys
    sqlx::query("DELETE FROM signing_keys")
        .execute(&pool)
        .await?;

    // Verify not ready
    let client = reqwest::Client::new();
    let response = client.get(format!("{}/ready", server.url())).send().await?;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "Should be not ready without signing key"
    );

    // Act - Initialize signing key
    key_management_service::initialize_signing_key(&pool, &master_key, "test-cluster").await?;

    // Assert - Should now be ready
    let response = client.get(format!("{}/ready", server.url())).send().await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Should become ready after key creation"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(body["status"].as_str(), Some("ready"));

    Ok(())
}

/// P1-5: Test readiness response includes structured JSON
///
/// Validates that the readiness response is proper JSON that operators
/// and monitoring systems can parse and act on.
#[sqlx::test(migrations = "../../migrations")]
async fn test_ready_returns_structured_json(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool).await?;

    // Act
    let client = reqwest::Client::new();
    let response = client.get(format!("{}/ready", server.url())).send().await?;

    // Assert - Verify JSON structure
    let body: serde_json::Value = response.json().await?;

    // Required fields when ready
    assert!(body["status"].is_string(), "status must be a string");
    assert!(body["database"].is_string(), "database must be a string");
    assert!(
        body["signing_key"].is_string(),
        "signing_key must be a string"
    );

    // Error should be absent on success (skip_serializing_if)
    assert!(
        body["error"].is_null(),
        "error should not be present on success"
    );

    Ok(())
}
