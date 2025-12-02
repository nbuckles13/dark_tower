use crate::crypto;
use crate::errors::AcError;
use crate::models::{AuthEventType, JsonWebKey, Jwks};
use crate::repositories::{auth_events, signing_keys};
use chrono::{Duration, Utc};
use sqlx::PgPool;

const KEY_VALIDITY_DAYS: i64 = 365; // 1 year

/// Get the next sequence number for a key with given prefix
async fn get_next_key_sequence(pool: &PgPool, prefix: &str) -> Result<u32, AcError> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM signing_keys
        WHERE key_id LIKE $1
        "#,
    )
    .bind(format!("{}%", prefix))
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to count keys: {}", e)))?;

    Ok((count.0 + 1) as u32)
}

/// Initialize the first signing key if none exists
pub async fn initialize_signing_key(
    pool: &PgPool,
    master_key: &[u8],
    cluster_name: &str,
) -> Result<(), AcError> {
    // Check if any active keys exist
    let active_key = signing_keys::get_active_key(pool).await?;

    if active_key.is_some() {
        // Key already exists, no need to initialize
        return Ok(());
    }

    // Generate key_id: Format 'auth-{cluster}-{YYYY}-{NN}'
    let now = Utc::now();
    let key_prefix = format!("auth-{}-{}-", cluster_name, now.format("%Y"));
    let sequence = get_next_key_sequence(pool, &key_prefix).await?;
    let key_id = format!("{}{:02}", key_prefix, sequence);

    // Generate EdDSA keypair
    let (public_key_pem, private_key_pkcs8) = crypto::generate_signing_key()?;

    // Encrypt private key with master key
    let encrypted = crypto::encrypt_private_key(&private_key_pkcs8, master_key)?;

    // Set validity period
    let valid_from = now;
    let valid_until = now + Duration::days(KEY_VALIDITY_DAYS);

    // Store in database
    signing_keys::create_signing_key(
        pool,
        &key_id,
        &public_key_pem,
        &encrypted.encrypted_data,
        &encrypted.nonce,
        &encrypted.tag,
        1, // master_key_version
        valid_from,
        valid_until,
    )
    .await?;

    // Log key generation
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::KeyGenerated.as_str(),
        None,
        None,
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "key_id": key_id,
            "valid_from": valid_from.to_rfc3339(),
            "valid_until": valid_until.to_rfc3339(),
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}

/// Rotate signing keys (generate new key, mark old keys as inactive)
#[allow(dead_code)] // Library function - will be used in Phase 4 key rotation endpoints
pub async fn rotate_signing_key(
    pool: &PgPool,
    master_key: &[u8],
    cluster_name: &str,
) -> Result<String, AcError> {
    // Generate new key_id
    let now = Utc::now();
    let key_prefix = format!("auth-{}-{}-", cluster_name, now.format("%Y"));
    let sequence = get_next_key_sequence(pool, &key_prefix).await?;
    let key_id = format!("{}{:02}", key_prefix, sequence);

    // Generate new EdDSA keypair
    let (public_key_pem, private_key_pkcs8) = crypto::generate_signing_key()?;

    // Encrypt private key
    let encrypted = crypto::encrypt_private_key(&private_key_pkcs8, master_key)?;

    // Set validity period
    let valid_from = now;
    let valid_until = now + Duration::days(KEY_VALIDITY_DAYS);

    // Create new key
    signing_keys::create_signing_key(
        pool,
        &key_id,
        &public_key_pem,
        &encrypted.encrypted_data,
        &encrypted.nonce,
        &encrypted.tag,
        1, // master_key_version
        valid_from,
        valid_until,
    )
    .await?;

    // Rotate keys (mark old as inactive, new as active)
    signing_keys::rotate_key(pool, &key_id).await?;

    // Log key rotation
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::KeyRotated.as_str(),
        None,
        None,
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "new_key_id": key_id,
            "valid_from": valid_from.to_rfc3339(),
            "valid_until": valid_until.to_rfc3339(),
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(key_id)
}

/// Get JWKS (JSON Web Key Set) for public key distribution
///
/// Returns all active public keys in RFC 7517 format
pub async fn get_jwks(pool: &PgPool) -> Result<Jwks, AcError> {
    // Fetch all active keys
    let keys = signing_keys::get_all_active_keys(pool).await?;

    // Convert to JWKS format
    let json_web_keys: Vec<JsonWebKey> = keys
        .into_iter()
        .map(|key| {
            // Extract base64 from PEM format
            let public_key_b64 = key
                .public_key
                .lines()
                .filter(|line| !line.starts_with("-----"))
                .collect::<String>();

            JsonWebKey {
                kid: key.key_id,
                kty: "OKP".to_string(),     // Octet Key Pair for EdDSA
                crv: "Ed25519".to_string(), // Curve
                x: public_key_b64,          // Public key
                use_: "sig".to_string(),    // Signature use
                alg: "EdDSA".to_string(),   // Algorithm
            }
        })
        .collect();

    Ok(Jwks {
        keys: json_web_keys,
    })
}

/// Check and mark expired keys as inactive
#[allow(dead_code)] // Library function - will be used in Phase 4 background tasks
pub async fn expire_old_keys(_pool: &PgPool) -> Result<Vec<String>, AcError> {
    // This would be called periodically by a background task
    // For now, it's a placeholder that could be implemented in Phase 4

    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    /// Test master key for tests (32 bytes for AES-256)
    fn test_master_key() -> Vec<u8> {
        vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]
    }

    // ============================================================================
    // Key Initialization Tests
    // ============================================================================

    /// Test initialize_signing_key creates a key when none exists
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_initialize_signing_key_creates_key(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Verify no keys exist initially
        let active_key = signing_keys::get_active_key(&pool).await?;
        assert!(active_key.is_none(), "No key should exist initially");

        // Initialize
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;

        // Verify key was created
        let active_key = signing_keys::get_active_key(&pool).await?;
        assert!(
            active_key.is_some(),
            "Key should exist after initialization"
        );

        let key = active_key.unwrap();
        assert!(key.key_id.starts_with("auth-test-cluster-"));
        assert!(key.is_active);

        Ok(())
    }

    /// Test initialize_signing_key is idempotent (doesn't create duplicate keys)
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_initialize_signing_key_idempotent(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize twice
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;

        // Should still have only one key
        let all_keys = signing_keys::get_all_active_keys(&pool).await?;
        assert_eq!(
            all_keys.len(),
            1,
            "Should have exactly one key after multiple initializations"
        );

        Ok(())
    }

    // ============================================================================
    // Key Rotation Tests
    // ============================================================================

    /// Test rotate_signing_key creates a new key
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rotate_signing_key(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize first key
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;

        let original_key = signing_keys::get_active_key(&pool).await?.unwrap();

        // Rotate to new key
        let new_key_id = rotate_signing_key(&pool, &master_key, "test-cluster").await?;

        // Verify new key is active
        let active_key = signing_keys::get_active_key(&pool).await?.unwrap();
        assert_eq!(active_key.key_id, new_key_id);
        assert_ne!(active_key.key_id, original_key.key_id);

        Ok(())
    }

    /// Test rotate_signing_key deactivates old key
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rotate_signing_key_deactivates_old(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize first key
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;
        let original_key_id = signing_keys::get_active_key(&pool).await?.unwrap().key_id;

        // Rotate
        rotate_signing_key(&pool, &master_key, "test-cluster").await?;

        // Verify original key is deactivated
        let original_key = signing_keys::get_by_key_id(&pool, &original_key_id)
            .await?
            .unwrap();
        assert!(
            !original_key.is_active,
            "Original key should be deactivated"
        );

        Ok(())
    }

    /// Test key_id sequence increments correctly
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_key_sequence_increments(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize and rotate multiple times
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;
        let key1_id = signing_keys::get_active_key(&pool).await?.unwrap().key_id;

        let key2_id = rotate_signing_key(&pool, &master_key, "test-cluster").await?;
        let key3_id = rotate_signing_key(&pool, &master_key, "test-cluster").await?;

        // Verify sequence numbers increment
        assert!(
            key1_id.ends_with("-01"),
            "First key should end with 01: {}",
            key1_id
        );
        assert!(
            key2_id.ends_with("-02"),
            "Second key should end with 02: {}",
            key2_id
        );
        assert!(
            key3_id.ends_with("-03"),
            "Third key should end with 03: {}",
            key3_id
        );

        Ok(())
    }

    // ============================================================================
    // JWKS Tests
    // ============================================================================

    /// Test get_jwks returns empty when no keys exist
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_jwks_empty(pool: PgPool) -> Result<(), AcError> {
        let jwks = get_jwks(&pool).await?;
        assert!(
            jwks.keys.is_empty(),
            "JWKS should be empty when no keys exist"
        );
        Ok(())
    }

    /// Test get_jwks returns keys in correct format
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_jwks_format(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize a key
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;

        // Get JWKS
        let jwks = get_jwks(&pool).await?;

        assert_eq!(jwks.keys.len(), 1, "JWKS should have one key");

        let key = &jwks.keys[0];
        assert!(key.kid.starts_with("auth-test-cluster-"));
        assert_eq!(key.kty, "OKP");
        assert_eq!(key.crv, "Ed25519");
        assert_eq!(key.use_, "sig");
        assert_eq!(key.alg, "EdDSA");
        assert!(!key.x.is_empty(), "Public key should not be empty");

        Ok(())
    }

    /// Test get_jwks only returns active keys
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_jwks_only_active_keys(pool: PgPool) -> Result<(), AcError> {
        let master_key = test_master_key();

        // Initialize and rotate (creates 2 keys, one active one inactive)
        initialize_signing_key(&pool, &master_key, "test-cluster").await?;
        rotate_signing_key(&pool, &master_key, "test-cluster").await?;

        // Get JWKS
        let jwks = get_jwks(&pool).await?;

        // Should only return the active key
        assert_eq!(jwks.keys.len(), 1, "JWKS should only return active keys");

        Ok(())
    }

    // ============================================================================
    // Expire Keys Tests
    // ============================================================================

    /// Test expire_old_keys placeholder returns empty
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_expire_old_keys_placeholder(pool: PgPool) -> Result<(), AcError> {
        let expired = expire_old_keys(&pool).await?;
        assert!(expired.is_empty(), "Placeholder should return empty vector");
        Ok(())
    }
}
