use crate::errors::AcError;
use crate::models::SigningKey;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Create a new signing key
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
pub async fn rotate_key(
    pool: &PgPool,
    new_key_id: &str,
) -> Result<(), AcError> {
    // Start transaction
    let mut tx = pool.begin().await
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
    tx.commit().await
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
