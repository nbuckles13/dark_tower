use crate::errors::AcError;
use crate::models::ServiceCredential;
use sqlx::PgPool;
use uuid::Uuid;

/// Create a new service credential
pub async fn create_service_credential(
    pool: &PgPool,
    client_id: &str,
    client_secret_hash: &str,
    service_type: &str,
    region: Option<&str>,
    scopes: &[String],
) -> Result<ServiceCredential, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        INSERT INTO service_credentials (client_id, client_secret_hash, service_type, region, scopes)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        "#,
    )
    .bind(client_id)
    .bind(client_secret_hash)
    .bind(service_type)
    .bind(region)
    .bind(scopes)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to create service credential: {}", e)))?;

    Ok(credential)
}

/// Get service credential by client_id
pub async fn get_by_client_id(
    pool: &PgPool,
    client_id: &str,
) -> Result<Option<ServiceCredential>, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        SELECT
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        FROM service_credentials
        WHERE client_id = $1
        "#,
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch service credential: {}", e)))?;

    Ok(credential)
}

/// Update scopes for a service credential
pub async fn update_scopes(
    pool: &PgPool,
    credential_id: Uuid,
    scopes: &[String],
) -> Result<ServiceCredential, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        UPDATE service_credentials
        SET scopes = $2, updated_at = NOW()
        WHERE credential_id = $1
        RETURNING
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        "#,
    )
    .bind(credential_id)
    .bind(scopes)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to update scopes: {}", e)))?;

    Ok(credential)
}

/// Deactivate a service credential
pub async fn deactivate(pool: &PgPool, credential_id: Uuid) -> Result<ServiceCredential, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        UPDATE service_credentials
        SET is_active = false, updated_at = NOW()
        WHERE credential_id = $1
        RETURNING
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        "#,
    )
    .bind(credential_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to deactivate credential: {}", e)))?;

    Ok(credential)
}

/// Get all active service credentials by service type
#[expect(dead_code)] // Will be used in Phase 4 admin endpoints
pub async fn get_active_by_service_type(
    pool: &PgPool,
    service_type: &str,
) -> Result<Vec<ServiceCredential>, AcError> {
    let credentials = sqlx::query_as::<_, ServiceCredential>(
        r#"
        SELECT
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        FROM service_credentials
        WHERE service_type = $1 AND is_active = true
        ORDER BY created_at DESC
        "#,
    )
    .bind(service_type)
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch credentials by type: {}", e)))?;

    Ok(credentials)
}
