use crate::errors::AcError;
use crate::models::SigningKey;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Create a new signing key
#[expect(clippy::too_many_arguments)] // Represents all signing_keys table columns
pub async fn create_signing_key(
    pool: &PgPool,
    key_id: &str,
    public_key: &str,
    private_key_encrypted: &[u8],
    encryption_nonce: &[u8],
    encryption_tag: &[u8],
    master_key_version: i32,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
) -> Result<SigningKey, AcError> {
    let key = sqlx::query_as::<_, SigningKey>(
        r#"
        INSERT INTO signing_keys (
            key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag,
            encryption_algorithm, master_key_version, algorithm,
            is_active, valid_from, valid_until
        )
        VALUES ($1, $2, $3, $4, $5, 'AES-256-GCM', $6, 'EdDSA', true, $7, $8)
        RETURNING
            key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag,
            encryption_algorithm, master_key_version, algorithm,
            is_active, valid_from, valid_until, created_at
        "#,
    )
    .bind(key_id)
    .bind(public_key)
    .bind(private_key_encrypted)
    .bind(encryption_nonce)
    .bind(encryption_tag)
    .bind(master_key_version)
    .bind(valid_from)
    .bind(valid_until)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to create signing key: {}", e)))?;

    Ok(key)
}

/// Get the currently active signing key
pub async fn get_active_key(pool: &PgPool) -> Result<Option<SigningKey>, AcError> {
    let key = sqlx::query_as::<_, SigningKey>(
        r#"
        SELECT
            key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag,
            encryption_algorithm, master_key_version, algorithm,
            is_active, valid_from, valid_until, created_at
        FROM signing_keys
        WHERE is_active = true
            AND valid_from <= NOW()
            AND valid_until > NOW()
        ORDER BY valid_from DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch active key: {}", e)))?;

    Ok(key)
}

/// Get signing key by key_id
#[expect(dead_code)] // Will be used in Phase 4 JWKS/admin endpoints
pub async fn get_by_key_id(pool: &PgPool, key_id: &str) -> Result<Option<SigningKey>, AcError> {
    let key = sqlx::query_as::<_, SigningKey>(
        r#"
        SELECT
            key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag,
            encryption_algorithm, master_key_version, algorithm,
            is_active, valid_from, valid_until, created_at
        FROM signing_keys
        WHERE key_id = $1
        "#,
    )
    .bind(key_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch key by id: {}", e)))?;

    Ok(key)
}

/// Mark old keys as inactive and new key as active (key rotation)
pub async fn rotate_key(pool: &PgPool, new_key_id: &str) -> Result<(), AcError> {
    // Start transaction
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AcError::Database(format!("Failed to start transaction: {}", e)))?;

    // Deactivate all existing active keys
    sqlx::query(
        r#"
        UPDATE signing_keys
        SET is_active = false
        WHERE is_active = true
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AcError::Database(format!("Failed to deactivate old keys: {}", e)))?;

    // Activate the new key
    sqlx::query(
        r#"
        UPDATE signing_keys
        SET is_active = true
        WHERE key_id = $1
        "#,
    )
    .bind(new_key_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AcError::Database(format!("Failed to activate new key: {}", e)))?;

    // Commit transaction
    tx.commit()
        .await
        .map_err(|e| AcError::Database(format!("Failed to commit rotation: {}", e)))?;

    Ok(())
}

/// Get all active public keys (for JWKS endpoint)
pub async fn get_all_active_keys(pool: &PgPool) -> Result<Vec<SigningKey>, AcError> {
    let keys = sqlx::query_as::<_, SigningKey>(
        r#"
        SELECT
            key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag,
            encryption_algorithm, master_key_version, algorithm,
            is_active, valid_from, valid_until, created_at
        FROM signing_keys
        WHERE is_active = true
            AND valid_from <= NOW()
            AND valid_until > NOW()
        ORDER BY valid_from DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch active keys: {}", e)))?;

    Ok(keys)
}

/// Mark a key as inactive
#[expect(dead_code)] // Will be used in Phase 4 key management
pub async fn deactivate_key(pool: &PgPool, key_id: &str) -> Result<(), AcError> {
    sqlx::query(
        r#"
        UPDATE signing_keys
        SET is_active = false
        WHERE key_id = $1
        "#,
    )
    .bind(key_id)
    .execute(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to deactivate key: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    // Helper to create test key data
    fn test_key_data(seed: u8) -> (String, String, Vec<u8>, Vec<u8>, Vec<u8>) {
        let key_id = format!("test-key-{}", seed);
        let public_key = format!("public_key_data_{}", seed);
        let private_key_encrypted = vec![seed; 32];
        let encryption_nonce = vec![seed + 1; 12];
        let encryption_tag = vec![seed + 2; 16];
        (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag)
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_create_signing_key(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();
        let valid_from = now;
        let valid_until = now + Duration::days(30);

        let key = create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1, // master_key_version
            valid_from,
            valid_until,
        )
        .await?;

        // Verify all fields
        assert_eq!(key.key_id, key_id);
        assert_eq!(key.public_key, public_key);
        assert_eq!(key.private_key_encrypted, private_key_encrypted);
        assert_eq!(key.encryption_nonce, encryption_nonce);
        assert_eq!(key.encryption_tag, encryption_tag);
        assert_eq!(key.encryption_algorithm, "AES-256-GCM");
        assert_eq!(key.master_key_version, 1);
        assert_eq!(key.algorithm, "EdDSA");
        assert!(key.is_active);
        assert_eq!(key.valid_from.timestamp(), valid_from.timestamp());
        assert_eq!(key.valid_until.timestamp(), valid_until.timestamp());

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_active_key_none_exists(pool: PgPool) -> Result<(), AcError> {
        let result = get_active_key(&pool).await?;
        assert!(result.is_none());
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_active_key_returns_current(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();
        let valid_from = now - Duration::days(1); // Started yesterday
        let valid_until = now + Duration::days(30); // Valid for 30 more days

        create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1,
            valid_from,
            valid_until,
        )
        .await?;

        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_some());
        let active_key = active_key.unwrap();
        assert_eq!(active_key.key_id, key_id);
        assert!(active_key.is_active);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_active_key_respects_validity_window(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();

        // Create a key that will be valid in the future
        let valid_from = now + Duration::days(1); // Starts tomorrow
        let valid_until = now + Duration::days(31);

        create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1,
            valid_from,
            valid_until,
        )
        .await?;

        // Should not return the future key
        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_none(), "Should not return key that's not yet valid");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_active_key_expired_not_returned(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();

        // Create an expired key
        let valid_from = now - Duration::days(31);
        let valid_until = now - Duration::days(1); // Expired yesterday

        create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1,
            valid_from,
            valid_until,
        )
        .await?;

        // Should not return the expired key
        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_none(), "Should not return expired key");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_key_id(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();
        create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1,
            now,
            now + Duration::days(30),
        )
        .await?;

        let found_key = get_by_key_id(&pool, &key_id).await?;
        assert!(found_key.is_some());
        assert_eq!(found_key.unwrap().key_id, key_id);

        let not_found = get_by_key_id(&pool, "nonexistent-key").await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rotate_key_deactivates_old_activates_new(pool: PgPool) -> Result<(), AcError> {
        let now = Utc::now();

        // Create two keys
        let (key_id_1, public_key_1, private_key_encrypted_1, encryption_nonce_1, encryption_tag_1) = test_key_data(1);
        let (key_id_2, public_key_2, private_key_encrypted_2, encryption_nonce_2, encryption_tag_2) = test_key_data(2);

        create_signing_key(
            &pool,
            &key_id_1,
            &public_key_1,
            &private_key_encrypted_1,
            &encryption_nonce_1,
            &encryption_tag_1,
            1,
            now - Duration::days(1),
            now + Duration::days(30),
        )
        .await?;

        create_signing_key(
            &pool,
            &key_id_2,
            &public_key_2,
            &private_key_encrypted_2,
            &encryption_nonce_2,
            &encryption_tag_2,
            1,
            now,
            now + Duration::days(30),
        )
        .await?;

        // Both keys are initially active
        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_some());

        // Rotate to key 2
        rotate_key(&pool, &key_id_2).await?;

        // Verify key 2 is now the only active key
        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_some());
        assert_eq!(active_key.unwrap().key_id, key_id_2);

        // Verify key 1 is now inactive
        let key_1 = get_by_key_id(&pool, &key_id_1).await?;
        assert!(key_1.is_some());
        assert!(!key_1.unwrap().is_active, "Old key should be inactive");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_all_active_keys(pool: PgPool) -> Result<(), AcError> {
        let now = Utc::now();

        // Create multiple keys with different validity periods
        let (key_id_1, public_key_1, private_key_encrypted_1, encryption_nonce_1, encryption_tag_1) = test_key_data(1);
        let (key_id_2, public_key_2, private_key_encrypted_2, encryption_nonce_2, encryption_tag_2) = test_key_data(2);
        let (key_id_3, public_key_3, private_key_encrypted_3, encryption_nonce_3, encryption_tag_3) = test_key_data(3);

        // Current valid key
        create_signing_key(
            &pool,
            &key_id_1,
            &public_key_1,
            &private_key_encrypted_1,
            &encryption_nonce_1,
            &encryption_tag_1,
            1,
            now - Duration::days(1),
            now + Duration::days(30),
        )
        .await?;

        // Future key (not yet valid)
        create_signing_key(
            &pool,
            &key_id_2,
            &public_key_2,
            &private_key_encrypted_2,
            &encryption_nonce_2,
            &encryption_tag_2,
            1,
            now + Duration::days(1),
            now + Duration::days(31),
        )
        .await?;

        // Expired key
        create_signing_key(
            &pool,
            &key_id_3,
            &public_key_3,
            &private_key_encrypted_3,
            &encryption_nonce_3,
            &encryption_tag_3,
            1,
            now - Duration::days(31),
            now - Duration::days(1),
        )
        .await?;

        let active_keys = get_all_active_keys(&pool).await?;

        // Should only return currently valid key
        assert_eq!(active_keys.len(), 1);
        assert_eq!(active_keys[0].key_id, key_id_1);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_deactivate_key(pool: PgPool) -> Result<(), AcError> {
        let (key_id, public_key, private_key_encrypted, encryption_nonce, encryption_tag) = test_key_data(1);

        let now = Utc::now();
        create_signing_key(
            &pool,
            &key_id,
            &public_key,
            &private_key_encrypted,
            &encryption_nonce,
            &encryption_tag,
            1,
            now,
            now + Duration::days(30),
        )
        .await?;

        // Verify it's active
        let key = get_by_key_id(&pool, &key_id).await?;
        assert!(key.is_some());
        assert!(key.unwrap().is_active);

        // Deactivate it
        deactivate_key(&pool, &key_id).await?;

        // Verify it's inactive
        let key = get_by_key_id(&pool, &key_id).await?;
        assert!(key.is_some());
        assert!(!key.unwrap().is_active);

        // Should not appear in active keys
        let active_key = get_active_key(&pool).await?;
        assert!(active_key.is_none());

        Ok(())
    }
}
