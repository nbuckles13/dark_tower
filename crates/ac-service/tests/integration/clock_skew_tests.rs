//! P1 Integration tests for JWT clock skew configuration
//!
//! These tests validate the configurable JWT_CLOCK_SKEW_SECONDS feature,
//! ensuring the entire pipeline works correctly with custom clock skew values.

use ac_service::config::DEFAULT_JWT_CLOCK_SKEW;
use ac_service::crypto;
use ac_service::models::SigningKey;
use ac_service::repositories::signing_keys;
use ac_service::services::key_management_service;
use chrono::Utc;
use common::secret::SecretBox;
use sqlx::PgPool;
use std::time::Duration;

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper to get the signing key and decrypt the private key
async fn get_signing_key_and_private(
    pool: &PgPool,
    master_key: &[u8],
) -> Result<(SigningKey, Vec<u8>), anyhow::Error> {
    let signing_key = signing_keys::get_active_key(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No active signing key"))?;

    let encrypted_key = crypto::EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(signing_key.private_key_encrypted.clone())),
        nonce: signing_key.encryption_nonce.clone(),
        tag: signing_key.encryption_tag.clone(),
    };
    let private_key = crypto::decrypt_private_key(&encrypted_key, master_key)?;

    Ok((signing_key, private_key))
}

// ============================================================================
// P1 Integration Tests - Custom Clock Skew Configuration
// ============================================================================

/// P1-1: Test custom clock skew value (60 seconds) accepts tokens within that skew
///
/// Verifies that when clock_skew_seconds is set to 60 seconds:
/// - A token with iat 30 seconds in the future (within 60s skew) is accepted
/// - This proves the custom configuration is correctly propagated through the pipeline
#[sqlx::test(migrations = "../../migrations")]
async fn test_custom_clock_skew_accepts_within_tolerance(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange: Initialize signing key
    let master_key = crypto::generate_random_bytes(32)?;
    key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

    let (signing_key, private_key) = get_signing_key_and_private(&pool, &master_key).await?;

    let now = Utc::now().timestamp();

    // Create token with iat 30 seconds in the future
    // This is within 60 second custom clock skew but outside 5 minute default
    let claims = crypto::Claims {
        sub: "clock-skew-test-client".to_string(),
        exp: now + 3600, // Expires in 1 hour
        iat: now + 30,   // Issued 30 seconds from now (within 60s skew)
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token = crypto::sign_jwt(&claims, &private_key, &signing_key.key_id)?;

    // Act: Verify with custom clock skew of 60 seconds
    let custom_clock_skew = Duration::from_secs(60);
    let result = crypto::verify_jwt(&token, &signing_key.public_key, custom_clock_skew);

    // Assert: Token should be accepted (iat is within 60 second tolerance)
    assert!(
        result.is_ok(),
        "Token with iat 30 seconds in future should be accepted with 60s clock skew"
    );

    let verified_claims = result.unwrap();
    assert_eq!(verified_claims.sub, "clock-skew-test-client");
    assert_eq!(verified_claims.scope, "test:scope");

    Ok(())
}

/// P1-2: Test custom clock skew value (60 seconds) rejects tokens beyond that skew
///
/// Verifies that when clock_skew_seconds is set to 60 seconds:
/// - A token with iat 90 seconds in the future (beyond 60s skew) is rejected
/// - This proves the custom configuration is correctly enforced
#[sqlx::test(migrations = "../../migrations")]
async fn test_custom_clock_skew_rejects_beyond_tolerance(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    // Arrange: Initialize signing key
    let master_key = crypto::generate_random_bytes(32)?;
    key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

    let (signing_key, private_key) = get_signing_key_and_private(&pool, &master_key).await?;

    let now = Utc::now().timestamp();

    // Create token with iat 90 seconds in the future
    // This is beyond 60 second custom clock skew
    let claims = crypto::Claims {
        sub: "clock-skew-test-client".to_string(),
        exp: now + 3600, // Expires in 1 hour
        iat: now + 90,   // Issued 90 seconds from now (beyond 60s skew)
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token = crypto::sign_jwt(&claims, &private_key, &signing_key.key_id)?;

    // Act: Verify with custom clock skew of 60 seconds
    let custom_clock_skew = Duration::from_secs(60);
    let result = crypto::verify_jwt(&token, &signing_key.public_key, custom_clock_skew);

    // Assert: Token should be rejected (iat is beyond 60 second tolerance)
    assert!(
        result.is_err(),
        "Token with iat 90 seconds in future should be rejected with 60s clock skew"
    );

    Ok(())
}

/// P1-3: Test default clock skew (300 seconds) behavior is unchanged
///
/// Regression test to ensure the default clock skew behavior is preserved.
/// A token with iat 120 seconds in the future should be accepted with default 300s skew.
#[sqlx::test(migrations = "../../migrations")]
async fn test_default_clock_skew_behavior_unchanged(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange: Initialize signing key
    let master_key = crypto::generate_random_bytes(32)?;
    key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

    let (signing_key, private_key) = get_signing_key_and_private(&pool, &master_key).await?;

    let now = Utc::now().timestamp();

    // Create token with iat 120 seconds in the future (within default 300s skew)
    let claims = crypto::Claims {
        sub: "default-skew-test-client".to_string(),
        exp: now + 3600,
        iat: now + 120, // Within default 300 second clock skew
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token = crypto::sign_jwt(&claims, &private_key, &signing_key.key_id)?;

    // Act: Verify with DEFAULT clock skew
    let result = crypto::verify_jwt(&token, &signing_key.public_key, DEFAULT_JWT_CLOCK_SKEW);

    // Assert: Token should be accepted with default clock skew
    assert!(
        result.is_ok(),
        "Token with iat 120 seconds in future should be accepted with default 300s clock skew"
    );

    let verified_claims = result.unwrap();
    assert_eq!(verified_claims.sub, "default-skew-test-client");

    Ok(())
}

/// P1-4: Test minimum clock skew (1 second) edge case
///
/// Verifies the system behaves correctly with the minimum valid clock skew of 1 second.
#[sqlx::test(migrations = "../../migrations")]
async fn test_minimum_clock_skew_edge_case(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange: Initialize signing key
    let master_key = crypto::generate_random_bytes(32)?;
    key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

    let (signing_key, private_key) = get_signing_key_and_private(&pool, &master_key).await?;

    let now = Utc::now().timestamp();

    // Create token with iat exactly at the 1 second boundary
    let claims_at_boundary = crypto::Claims {
        sub: "min-skew-boundary-client".to_string(),
        exp: now + 3600,
        iat: now + 1, // Exactly at 1 second boundary
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token_at_boundary =
        crypto::sign_jwt(&claims_at_boundary, &private_key, &signing_key.key_id)?;

    // Act & Assert: Token at boundary should be accepted
    let min_clock_skew = Duration::from_secs(1);
    let result = crypto::verify_jwt(&token_at_boundary, &signing_key.public_key, min_clock_skew);
    assert!(
        result.is_ok(),
        "Token with iat at exact 1 second boundary should be accepted"
    );

    // Create token with iat 2 seconds in the future (beyond 1 second skew)
    let claims_beyond = crypto::Claims {
        sub: "min-skew-beyond-client".to_string(),
        exp: now + 3600,
        iat: now + 2, // 1 second beyond the boundary
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token_beyond = crypto::sign_jwt(&claims_beyond, &private_key, &signing_key.key_id)?;

    // Act & Assert: Token beyond boundary should be rejected
    let result = crypto::verify_jwt(&token_beyond, &signing_key.public_key, min_clock_skew);
    assert!(
        result.is_err(),
        "Token with iat 2 seconds in future should be rejected with 1s clock skew"
    );

    Ok(())
}

/// P1-5: Test maximum clock skew (600 seconds) edge case
///
/// Verifies the system behaves correctly with the maximum valid clock skew of 600 seconds.
#[sqlx::test(migrations = "../../migrations")]
async fn test_maximum_clock_skew_edge_case(pool: PgPool) -> Result<(), anyhow::Error> {
    // Arrange: Initialize signing key
    let master_key = crypto::generate_random_bytes(32)?;
    key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

    let (signing_key, private_key) = get_signing_key_and_private(&pool, &master_key).await?;

    let now = Utc::now().timestamp();

    // Create token with iat at the 600 second boundary
    let claims_at_boundary = crypto::Claims {
        sub: "max-skew-boundary-client".to_string(),
        exp: now + 7200, // Expires in 2 hours
        iat: now + 600,  // Exactly at 600 second (10 minute) boundary
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token_at_boundary =
        crypto::sign_jwt(&claims_at_boundary, &private_key, &signing_key.key_id)?;

    // Act & Assert: Token at max boundary should be accepted
    let max_clock_skew = Duration::from_secs(600);
    let result = crypto::verify_jwt(&token_at_boundary, &signing_key.public_key, max_clock_skew);
    assert!(
        result.is_ok(),
        "Token with iat at exact 600 second boundary should be accepted"
    );

    // Create token with iat 601 seconds in the future (beyond max skew)
    let claims_beyond = crypto::Claims {
        sub: "max-skew-beyond-client".to_string(),
        exp: now + 7200,
        iat: now + 601, // 1 second beyond max boundary
        scope: "test:scope".to_string(),
        service_type: Some("global-controller".to_string()),
    };

    let token_beyond = crypto::sign_jwt(&claims_beyond, &private_key, &signing_key.key_id)?;

    // Act & Assert: Token beyond max boundary should be rejected
    let result = crypto::verify_jwt(
        &token_beyond,
        &signing_key.public_key,
        Duration::from_secs(600),
    );
    assert!(
        result.is_err(),
        "Token with iat 601 seconds in future should be rejected with 600s clock skew"
    );

    Ok(())
}
