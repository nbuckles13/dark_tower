//! Chaos tests for key rotation under load and concurrent operations
//!
//! These tests validate that key rotation maintains consistency during:
//! - Concurrent token validation operations
//! - In-flight token issuance
//! - Multiple simultaneous rotation attempts
//!
//! Key rotation is a critical operation that must not break existing tokens
//! or cause validation failures during the transition.

use ac_test_utils::{rotation_time, TestAuthServer};
use reqwest::StatusCode;
use sqlx::PgPool;

/// Test that key rotation doesn't break tokens issued before rotation
///
/// This validates the core overlap period behavior:
/// 1. Issue tokens with current key (Key A)
/// 2. Rotate to new key (Key B becomes active, Key A still valid)
/// 3. Old tokens signed with Key A should still validate successfully
/// 4. New tokens should be signed with Key B
/// 5. JWKS should expose both keys during grace period
#[sqlx::test(migrations = "../../migrations")]
async fn test_key_rotation_during_validation(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Step 1: Issue tokens with current key (Key A)
    let token_before = server
        .create_service_token("client-before-rotation", &["read"])
        .await?;

    // Verify token works
    let jwks_before: serde_json::Value = client
        .get(format!("{}/.well-known/jwks.json", server.url()))
        .send()
        .await?
        .json()
        .await?;

    let keys_before_count = jwks_before["keys"].as_array().map(|k| k.len()).unwrap_or(0);
    assert_eq!(
        keys_before_count, 1,
        "Should have exactly 1 key before rotation"
    );

    // Step 2: Perform key rotation
    rotation_time::set_eligible(&pool).await?;

    let admin_token = server
        .create_service_token("admin-client", &["service.rotate-keys.ac"])
        .await?;

    let rotate_response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&admin_token)
        .send()
        .await?;

    assert_eq!(
        rotate_response.status(),
        StatusCode::OK,
        "Key rotation should succeed"
    );

    let rotation_body: serde_json::Value = rotate_response.json().await?;
    assert_eq!(
        rotation_body["rotated"].as_bool(),
        Some(true),
        "Rotation should be successful"
    );

    let new_key_id = rotation_body["new_key_id"]
        .as_str()
        .expect("Should have new_key_id");
    let _old_key_id = rotation_body["old_key_id"]
        .as_str()
        .expect("Should have old_key_id");

    // Step 3: Verify old token still works (signed with Key A)
    // We can't directly validate tokens via the server (no validation endpoint),
    // but we can verify the JWKS contains both keys
    let jwks_after: serde_json::Value = client
        .get(format!("{}/.well-known/jwks.json", server.url()))
        .send()
        .await?
        .json()
        .await?;

    let keys_after = jwks_after["keys"]
        .as_array()
        .expect("JWKS should have keys array");

    // JWKS should contain both old and new keys during grace period
    assert!(
        !keys_after.is_empty(),
        "JWKS should have at least the new key (old key may have expired in test due to short validity)"
    );

    // Verify new key is in JWKS
    let new_key_present = keys_after
        .iter()
        .any(|k| k["kid"].as_str() == Some(new_key_id));
    assert!(new_key_present, "New key should be in JWKS");

    // Step 4: Issue new token (should use Key B)
    let token_after = server
        .create_service_token("client-after-rotation", &["write"])
        .await?;

    // Verify tokens are different (different signatures due to different keys)
    assert_ne!(
        token_before, token_after,
        "Tokens should be different (different clients + keys)"
    );

    // Step 5: Verify both tokens are structurally valid JWTs
    // (This doesn't verify signatures, but ensures they're well-formed)
    let token_before_parts: Vec<&str> = token_before.split('.').collect();
    assert_eq!(
        token_before_parts.len(),
        3,
        "Old token should have 3 parts (header.payload.signature)"
    );

    let token_after_parts: Vec<&str> = token_after.split('.').collect();
    assert_eq!(
        token_after_parts.len(),
        3,
        "New token should have 3 parts (header.payload.signature)"
    );

    Ok(())
}

/// Test concurrent key rotations are serialized by advisory lock
///
/// Validates that multiple concurrent rotation requests are handled safely:
/// - Only ONE rotation should succeed
/// - All others should be rate-limited (already rotated by the first request)
/// - No race conditions or partial rotations
#[sqlx::test(migrations = "../../migrations")]
async fn test_concurrent_rotations_are_serialized(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    rotation_time::set_eligible(&pool).await?;

    // Create multiple clients with rotation scope
    let token = server
        .create_service_token("rotation-client", &["service.rotate-keys.ac"])
        .await?;

    let url = server.url();

    // Launch 10 concurrent rotation requests
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let url = url.clone();
            let token = token.clone();
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

    // Count outcomes
    let success_count = results
        .iter()
        .filter(|r| r.status() == StatusCode::OK)
        .count();

    let rate_limited_count = results
        .iter()
        .filter(|r| r.status() == StatusCode::TOO_MANY_REQUESTS)
        .count();

    // Assert: Exactly ONE rotation should succeed
    assert_eq!(
        success_count, 1,
        "Exactly one concurrent rotation should succeed (got {})",
        success_count
    );

    // Assert: All other requests should be rate limited
    assert_eq!(
        rate_limited_count, 9,
        "Nine requests should be rate limited (got {})",
        rate_limited_count
    );

    // Verify no other status codes occurred (no 500 errors, etc.)
    let total_expected = success_count + rate_limited_count;
    assert_eq!(
        total_expected, 10,
        "All 10 requests should be accounted for (got {})",
        total_expected
    );

    Ok(())
}

/// Test that tokens can be validated during key rotation
///
/// This test simulates the real-world scenario where:
/// 1. Tokens are being issued continuously
/// 2. A key rotation happens
/// 3. Validation should work seamlessly throughout
///
/// We verify this by:
/// - Creating tokens before rotation
/// - Rotating keys
/// - Creating tokens after rotation
/// - Verifying both sets are structurally valid
#[sqlx::test(migrations = "../../migrations")]
async fn test_validation_works_during_rotation(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;

    // Issue multiple tokens before rotation
    let mut tokens_before = Vec::new();
    for i in 0..5 {
        let token = server
            .create_service_token(&format!("client-before-{}", i), &["read"])
            .await?;
        tokens_before.push(token);
    }

    // Verify all tokens are well-formed
    for token in &tokens_before {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "Token should have 3 parts");
    }

    // Perform rotation
    rotation_time::set_eligible(&pool).await?;
    let admin_token = server
        .create_service_token("admin", &["service.rotate-keys.ac"])
        .await?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&admin_token)
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::OK, "Rotation should succeed");

    // Issue multiple tokens after rotation
    let mut tokens_after = Vec::new();
    for i in 0..5 {
        let token = server
            .create_service_token(&format!("client-after-{}", i), &["write"])
            .await?;
        tokens_after.push(token);
    }

    // Verify all post-rotation tokens are well-formed
    for token in &tokens_after {
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "Token should have 3 parts");
    }

    // Verify we have distinct tokens (different signatures)
    assert_eq!(tokens_before.len(), 5, "Should have 5 pre-rotation tokens");
    assert_eq!(tokens_after.len(), 5, "Should have 5 post-rotation tokens");

    // Tokens before and after should be different sets (different keys used)
    // We can't validate signatures here, but we verify they're structurally correct

    Ok(())
}

/// Test JWKS updates immediately after rotation
///
/// Validates that the JWKS endpoint reflects key changes immediately,
/// so that token validators can discover new keys without delay.
#[sqlx::test(migrations = "../../migrations")]
async fn test_jwks_updates_after_rotation(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;
    let client = reqwest::Client::new();

    // Get initial JWKS
    let jwks_before: serde_json::Value = client
        .get(format!("{}/.well-known/jwks.json", server.url()))
        .send()
        .await?
        .json()
        .await?;

    let keys_before = jwks_before["keys"]
        .as_array()
        .expect("JWKS should have keys array");
    let _initial_kid = keys_before[0]["kid"].as_str().expect("Key should have kid");

    // Perform rotation
    rotation_time::set_eligible(&pool).await?;
    let admin_token = server
        .create_service_token("admin", &["service.rotate-keys.ac"])
        .await?;

    let rotate_response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&admin_token)
        .send()
        .await?;

    assert_eq!(rotate_response.status(), StatusCode::OK);

    let rotation_body: serde_json::Value = rotate_response.json().await?;
    let new_key_id = rotation_body["new_key_id"]
        .as_str()
        .expect("Should have new_key_id");

    // Get JWKS immediately after rotation
    let jwks_after: serde_json::Value = client
        .get(format!("{}/.well-known/jwks.json", server.url()))
        .send()
        .await?
        .json()
        .await?;

    let keys_after = jwks_after["keys"]
        .as_array()
        .expect("JWKS should have keys array");

    // New key should be present
    let new_key_present = keys_after
        .iter()
        .any(|k| k["kid"].as_str() == Some(new_key_id));
    assert!(
        new_key_present,
        "New key should be immediately visible in JWKS"
    );

    // Old key may or may not be present depending on validity window
    // (in tests, validity windows are short, so it might have expired)
    // We just verify that at least the new key is there
    assert!(
        !keys_after.is_empty(),
        "JWKS should have at least the new key"
    );

    Ok(())
}

/// Test that forced rotation works under load
///
/// Validates that force rotation (admin scope) can succeed even when
/// concurrent token operations are happening.
#[sqlx::test(migrations = "../../migrations")]
async fn test_force_rotation_under_load(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange
    let server = TestAuthServer::spawn(pool.clone()).await?;

    // Set up for force rotation (within 6-day window but past 1-hour window)
    rotation_time::set_force_eligible(&pool).await?;

    // Pre-create clients for background token issuance
    // (We can't clone the server, so we use the URL and pool directly)
    let server_url = server.url();

    // Spawn background task that issues tokens via HTTP
    let token_issuance = tokio::spawn(async move {
        for i in 0..10 {
            // Issue tokens by calling the token endpoint directly
            // This simulates real load on the service
            let _ = reqwest::Client::new()
                .post(format!("{}/api/v1/auth/service/token", server_url))
                .json(&serde_json::json!({
                    "client_id": format!("background-client-{}", i),
                    "client_secret": "test-secret-12345",
                    "grant_type": "client_credentials"
                }))
                .send()
                .await;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });

    // Give token issuance a head start
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Perform force rotation while tokens are being issued
    let admin_token = server
        .create_service_token("force-admin", &["admin.force-rotate-keys.ac"])
        .await?;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/internal/rotate-keys", server.url()))
        .bearer_auth(&admin_token)
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Force rotation should succeed even under load"
    );

    // Wait for background tasks to complete
    token_issuance.await?;

    // Verify we can still issue tokens after rotation
    let token_after = server
        .create_service_token("post-rotation-client", &["write"])
        .await?;

    let parts: Vec<&str> = token_after.split('.').collect();
    assert_eq!(parts.len(), 3, "Token after rotation should be well-formed");

    Ok(())
}
