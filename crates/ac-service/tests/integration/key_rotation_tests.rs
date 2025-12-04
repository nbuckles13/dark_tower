//! P0 Integration tests for key rotation (ADR-0009)
//!
//! These tests validate the key rotation endpoint with proper authentication,
//! authorization, and rate limiting enforcement.

use ac_service::crypto;
use ac_service::repositories::{service_credentials, signing_keys};
use ac_service::services::token_service;
use ac_test_utils::{rotation_time, TestAuthServer};
use chrono::Utc;
use reqwest::StatusCode;
use sqlx::PgPool;

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper to create a service with specific scopes and get a token
async fn create_service_with_token(
    pool: &PgPool,
    master_key: &[u8],
    client_id: &str,
    scopes: Vec<String>,
) -> Result<String, anyhow::Error> {
    // Create service credentials
    let client_secret = "test-secret-12345";
    let client_secret_hash = crypto::hash_client_secret(client_secret)?;
    service_credentials::create_service_credential(
        pool,
        client_id,
        &client_secret_hash,
        "global-controller",
        None,
        &scopes,
    )
    .await?;

    // Issue service token
    let token_response = token_service::issue_service_token(
        pool,
        master_key,
        client_id,
        client_secret,
        "client_credentials",
        None,
        None,
        None,
    )
    .await?;

    Ok(token_response.access_token)
}

/// Helper to create a user token (simulated by creating expired service token)
///
/// Since issue_user_token is not fully implemented, we simulate a user token
/// by creating a service token with no service_type (which will fail validation).
async fn create_user_token(pool: &PgPool, master_key: &[u8]) -> Result<String, anyhow::Error> {
    // Get signing key
    let signing_key_model = signing_keys::get_active_key(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No active signing key"))?;

    // Decrypt private key
    let encrypted_key = crypto::EncryptedKey {
        encrypted_data: signing_key_model.private_key_encrypted.clone(),
        nonce: signing_key_model.encryption_nonce.clone(),
        tag: signing_key_model.encryption_tag.clone(),
    };
    let private_key = crypto::decrypt_private_key(&encrypted_key, master_key)?;

    // Create user claims (no service_type, which indicates user token)
    let claims = crypto::Claims {
        sub: "user-alice".to_string(),
        exp: Utc::now().timestamp() + 3600,
        iat: Utc::now().timestamp(),
        scope: "service.rotate-keys.ac".to_string(),
        service_type: None, // User tokens don't have service_type
    };

    // Sign token
    let token = crypto::sign_jwt(&claims, &private_key, &signing_key_model.key_id)?;

    Ok(token)
}

// ============================================================================
// P0 Integration Tests
// ============================================================================

/// P0-1: Test rotate keys with valid scope succeeds
///
/// Verifies that a service with service.rotate-keys.ac scope can successfully
/// rotate keys when rate limit has passed (7 days since last rotation).
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_with_valid_scope_succeeds(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation eligible (7 days ago)
    rotation_time::set_eligible(&pool).await?;

    // Create service with rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "rotation-client",
        vec!["service.rotate-keys.ac".to_string()],
    )
    .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Rotation with valid scope should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["rotated"].as_bool(),
        Some(true),
        "Response should indicate successful rotation"
    );
    assert!(
        body["new_key_id"].is_string(),
        "Response should include new_key_id"
    );
    assert!(
        body["old_key_id"].is_string(),
        "Response should include old_key_id"
    );

    Ok(())
}

/// P0-2: Test rotate keys without scope returns 403
///
/// Verifies that a service without the required rotation scope is denied
/// with a 403 Forbidden error.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_without_scope_returns_403(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation eligible (7 days ago)
    rotation_time::set_eligible(&pool).await?;

    // Create service WITHOUT rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "limited-client",
        vec!["meeting:create".to_string()], // Wrong scope
    )
    .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Rotation without required scope should return 403 Forbidden"
    );

    let body: serde_json::Value = response.json().await?;
    assert!(
        body["error"]["message"].as_str().is_some(),
        "Error response should include error message"
    );
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INSUFFICIENT_SCOPE"),
        "Error code should be INSUFFICIENT_SCOPE"
    );

    Ok(())
}

/// P0-3: Test rotate keys with user token returns 401
///
/// Verifies that user tokens (tokens without service_type) cannot rotate keys,
/// even if they have the rotation scope. Only service tokens are allowed.
///
/// This is a security control - key rotation is a privileged operation that
/// should only be performed by authenticated services, not end users.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_user_token_returns_401(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation eligible (7 days ago)
    rotation_time::set_eligible(&pool).await?;

    // Create a user token (no service_type) with the rotation scope
    let user_token = create_user_token(&pool, &master_key).await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&user_token)
        .send()
        .await?;

    // Assert
    // User tokens should be rejected with 401 Unauthorized
    // The handler validates service_type and rejects user tokens
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "User tokens should be rejected with 401 Unauthorized"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// P0-4: Test rotate keys with expired token returns 401
///
/// Verifies that expired tokens are rejected with 401 Unauthorized during
/// JWT verification.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_expired_token_returns_401(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation eligible (7 days ago)
    rotation_time::set_eligible(&pool).await?;

    // Get signing key to create expired token
    let signing_key_model = signing_keys::get_active_key(&pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No active signing key"))?;

    // Decrypt private key
    let encrypted_key = crypto::EncryptedKey {
        encrypted_data: signing_key_model.private_key_encrypted.clone(),
        nonce: signing_key_model.encryption_nonce.clone(),
        tag: signing_key_model.encryption_tag.clone(),
    };
    let private_key = crypto::decrypt_private_key(&encrypted_key, &master_key)?;

    // Create expired claims
    let expired_claims = crypto::Claims {
        sub: "expired-client".to_string(),
        exp: Utc::now().timestamp() - 3600, // Expired 1 hour ago
        iat: Utc::now().timestamp() - 7200, // Issued 2 hours ago
        scope: "service.rotate-keys.ac".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    // Sign token
    let expired_token = crypto::sign_jwt(&expired_claims, &private_key, &signing_key_model.key_id)?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&expired_token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expired token should return 401 Unauthorized"
    );

    Ok(())
}

/// P0-5: Test rotate keys within 6 days returns 429
///
/// Verifies that normal rotation (service.rotate-keys.ac scope) is rate limited
/// to once per 6 days. Requests within the rate limit window should return 429.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_within_6_days_returns_429(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation to 30 minutes ago (within 6 day window)
    rotation_time::set_rate_limited(&pool).await?;

    // Create service with normal rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "rate-limited-client",
        vec!["service.rotate-keys.ac".to_string()],
    )
    .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Normal rotation within 6 days should return 429 Too Many Requests"
    );

    let body: serde_json::Value = response.json().await?;
    assert!(
        body["error"]["message"].as_str().is_some(),
        "Error response should include error message"
    );
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("TOO_MANY_REQUESTS"),
        "Error code should be TOO_MANY_REQUESTS"
    );

    Ok(())
}

/// P0-6: Test force rotate within 1 hour returns 429
///
/// Verifies that even force rotation (admin.force-rotate-keys.ac scope) is
/// rate limited to once per hour. Requests within the rate limit window should
/// return 429, even with admin privileges.
#[sqlx::test(migrations = "../../migrations")]
async fn test_force_rotate_within_1_hour_returns_429(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation to 30 minutes ago (within 1 hour force window)
    rotation_time::set_rate_limited(&pool).await?;

    // Create service with force rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "force-client",
        vec!["admin.force-rotate-keys.ac".to_string()],
    )
    .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Force rotation within 1 hour should return 429 Too Many Requests"
    );

    Ok(())
}

/// P0-7: Test force rotate after 1 hour succeeds
///
/// Verifies that force rotation with admin.force-rotate-keys.ac scope succeeds
/// after the 1-hour rate limit has passed, even though normal rotation would
/// still be blocked (< 6 days).
#[sqlx::test(migrations = "../../migrations")]
async fn test_force_rotate_after_1_hour_succeeds(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation to 2 hours ago (past 1 hour force limit, but within 6 day normal limit)
    rotation_time::set_force_eligible(&pool).await?;

    // Create service with force rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "force-success-client",
        vec!["admin.force-rotate-keys.ac".to_string()],
    )
    .await?;

    // Act
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&token)
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Force rotation after 1 hour should succeed"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["rotated"].as_bool(),
        Some(true),
        "Response should indicate successful rotation"
    );
    assert!(
        body["new_key_id"].is_string(),
        "Response should include new_key_id"
    );
    assert!(
        body["old_key_id"].is_string(),
        "Response should include old_key_id"
    );

    Ok(())
}

// ============================================================================
// Additional Security Tests (P0 gaps identified in security review)
// ============================================================================

/// P0-8: Test rotate keys without Authorization header returns 401
///
/// Verifies that requests without an Authorization header are rejected
/// with 401 Unauthorized.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_missing_auth_header_returns_401(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    rotation_time::set_eligible(&pool).await?;

    // Act - No bearer_auth() call, missing Authorization header
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Missing Authorization header should return 401 Unauthorized"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

/// P0-9: Test rotate keys with malformed Authorization header returns 401
///
/// Verifies that requests with a malformed Authorization header (not "Bearer <token>")
/// are rejected with 401 Unauthorized.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rotate_keys_malformed_auth_header_returns_401(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    rotation_time::set_eligible(&pool).await?;

    // Act - Malformed header (missing "Bearer " prefix)
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .header("Authorization", "InvalidFormat some-token-without-bearer")
        .send()
        .await?;

    // Assert
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Malformed Authorization header should return 401 Unauthorized"
    );

    let body: serde_json::Value = response.json().await?;
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("INVALID_TOKEN"),
        "Error code should be INVALID_TOKEN"
    );

    Ok(())
}

// ============================================================================
// P1 Security Tests (TOCTOU Protection)
// ============================================================================

/// P1: Test concurrent rotation requests honor rate limiting (TOCTOU protection)
///
/// Validates that the advisory lock prevents TOCTOU race where multiple
/// concurrent requests could bypass rate limiting by reading the same
/// last_rotation timestamp before any commits.
///
/// Expected behavior: Exactly ONE request succeeds, all others are rate limited.
/// The advisory lock serializes all rotation requests, ensuring deterministic behavior.
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_rotation_enforces_rate_limit(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let master_key = server.config().master_key.clone();

    // Set rotation eligible (7 days ago)
    rotation_time::set_eligible(&pool).await?;

    // Create service with rotation scope
    let token = create_service_with_token(
        &pool,
        &master_key,
        "concurrent-client",
        vec!["service.rotate-keys.ac".to_string()],
    )
    .await?;

    let url = server.url();
    let token_clone = token.clone();

    // Launch 5 concurrent rotation requests
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let url = url.clone();
            let token = token_clone.clone();
            tokio::spawn(async move {
                reqwest::Client::new()
                    .post(format!("{}/internal/rotate-keys", url))
                    .bearer_auth(&token)
                    .send()
                    .await
            })
        })
        .collect();

    // Wait for all requests to complete
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("Task should not panic"))
        .collect::<Result<Vec<_>, _>>()?;

    // Count successes and rate-limited responses
    let success_count = results
        .iter()
        .filter(|r| r.status() == StatusCode::OK)
        .count();

    let rate_limited_count = results
        .iter()
        .filter(|r| r.status() == StatusCode::TOO_MANY_REQUESTS)
        .count();

    // Assert: Exactly ONE request should succeed
    // The advisory lock serializes requests, so only the first one wins
    assert_eq!(
        success_count, 1,
        "Exactly one concurrent request should succeed (got {})",
        success_count
    );

    // Assert: All other requests should be rate limited
    assert_eq!(
        rate_limited_count, 4,
        "Four requests should be rate limited (got {})",
        rate_limited_count
    );

    Ok(())
}
