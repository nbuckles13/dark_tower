use crate::errors::AcError;
use crate::models::ServiceCredential;
use crate::observability::metrics::record_db_query;
use sqlx::PgPool;
use std::time::Instant;
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
    let start = Instant::now();
    let result = sqlx::query_as::<_, ServiceCredential>(
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
    .await;

    let status = if result.is_ok() { "success" } else { "error" };
    record_db_query("select", "service_credentials", status, start.elapsed());

    let credential = result
        .map_err(|e| AcError::Database(format!("Failed to fetch service credential: {}", e)))?;

    Ok(credential)
}

/// Update scopes for a service credential
#[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
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
#[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
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
#[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
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

/// Get all service credentials (for admin listing)
pub async fn get_all(pool: &PgPool) -> Result<Vec<ServiceCredential>, AcError> {
    let credentials = sqlx::query_as::<_, ServiceCredential>(
        r#"
        SELECT
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        FROM service_credentials
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch all credentials: {}", e)))?;

    Ok(credentials)
}

/// Get service credential by credential_id
pub async fn get_by_credential_id(
    pool: &PgPool,
    credential_id: Uuid,
) -> Result<Option<ServiceCredential>, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        SELECT
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        FROM service_credentials
        WHERE credential_id = $1
        "#,
    )
    .bind(credential_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch service credential: {}", e)))?;

    Ok(credential)
}

/// Update service credential metadata (name is stored in service_type for now)
pub async fn update_metadata(
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
    .map_err(|e| AcError::Database(format!("Failed to update metadata: {}", e)))?;

    Ok(credential)
}

/// Delete service credential (hard delete)
pub async fn delete(pool: &PgPool, credential_id: Uuid) -> Result<(), AcError> {
    let result = sqlx::query(
        r#"
        DELETE FROM service_credentials
        WHERE credential_id = $1
        "#,
    )
    .bind(credential_id)
    .execute(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to delete credential: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AcError::Database("Credential not found".to_string()));
    }

    Ok(())
}

/// Rotate client secret (update hash)
pub async fn rotate_secret(
    pool: &PgPool,
    credential_id: Uuid,
    new_secret_hash: &str,
) -> Result<ServiceCredential, AcError> {
    let credential = sqlx::query_as::<_, ServiceCredential>(
        r#"
        UPDATE service_credentials
        SET client_secret_hash = $2, updated_at = NOW()
        WHERE credential_id = $1
        RETURNING
            credential_id, client_id, client_secret_hash, service_type, region, scopes,
            is_active, created_at, updated_at
        "#,
    )
    .bind(credential_id)
    .bind(new_secret_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| AcError::Database(format!("Failed to rotate secret: {}", e)))?;

    Ok(credential)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_create_and_get_service_credential(pool: PgPool) -> Result<(), AcError> {
        // Create a service credential
        let scopes = vec!["meeting:create".to_string(), "meeting:read".to_string()];
        let credential = create_service_credential(
            &pool,
            "gc-test-client-001",
            "hashed_secret_123",
            "global-controller",
            Some("us-west-2"),
            &scopes,
        )
        .await?;

        // Verify fields
        assert_eq!(credential.client_id, "gc-test-client-001");
        assert_eq!(credential.client_secret_hash, "hashed_secret_123");
        assert_eq!(credential.service_type, "global-controller");
        assert_eq!(credential.region, Some("us-west-2".to_string()));
        assert_eq!(credential.scopes, scopes);
        assert!(credential.is_active);

        // Retrieve by client_id
        let retrieved = get_by_client_id(&pool, "gc-test-client-001").await?;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.credential_id, credential.credential_id);
        assert_eq!(retrieved.client_id, "gc-test-client-001");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_nonexistent_credential(pool: PgPool) -> Result<(), AcError> {
        let result = get_by_client_id(&pool, "nonexistent-client").await?;
        assert!(result.is_none());
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_scopes(pool: PgPool) -> Result<(), AcError> {
        // Create credential
        let initial_scopes = vec!["meeting:create".to_string()];
        let credential = create_service_credential(
            &pool,
            "test-client-update",
            "hash",
            "meeting-controller",
            None,
            &initial_scopes,
        )
        .await?;

        // Update scopes
        let new_scopes = vec![
            "meeting:create".to_string(),
            "meeting:update".to_string(),
            "participant:manage".to_string(),
        ];
        let updated = update_scopes(&pool, credential.credential_id, &new_scopes).await?;

        assert_eq!(updated.scopes, new_scopes);
        assert_eq!(updated.credential_id, credential.credential_id);
        // updated_at should be more recent than created_at
        assert!(updated.updated_at >= updated.created_at);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_deactivate_credential(pool: PgPool) -> Result<(), AcError> {
        // Create active credential
        let credential = create_service_credential(
            &pool,
            "test-deactivate",
            "hash",
            "media-handler",
            None,
            &["media:process".to_string()],
        )
        .await?;
        assert!(credential.is_active);

        // Deactivate
        let deactivated = deactivate(&pool, credential.credential_id).await?;
        assert!(!deactivated.is_active);
        assert_eq!(deactivated.credential_id, credential.credential_id);

        // Verify it's still in database but inactive
        let retrieved = get_by_client_id(&pool, "test-deactivate").await?;
        assert!(retrieved.is_some());
        assert!(!retrieved.unwrap().is_active);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_active_by_service_type(pool: PgPool) -> Result<(), AcError> {
        // Create multiple credentials of different types
        create_service_credential(
            &pool,
            "gc-1",
            "hash1",
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        let gc2 = create_service_credential(
            &pool,
            "gc-2",
            "hash2",
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        create_service_credential(
            &pool,
            "mc-1",
            "hash3",
            "meeting-controller",
            None,
            &["participant:manage".to_string()],
        )
        .await?;

        // Deactivate one global-controller
        deactivate(&pool, gc2.credential_id).await?;

        // Get active global-controller credentials
        let active_gc = get_active_by_service_type(&pool, "global-controller").await?;
        assert_eq!(active_gc.len(), 1);
        assert_eq!(active_gc[0].client_id, "gc-1");

        // Get active meeting-controller credentials
        let active_mc = get_active_by_service_type(&pool, "meeting-controller").await?;
        assert_eq!(active_mc.len(), 1);
        assert_eq!(active_mc[0].client_id, "mc-1");

        // No active media-handler credentials
        let active_mh = get_active_by_service_type(&pool, "media-handler").await?;
        assert_eq!(active_mh.len(), 0);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_duplicate_client_id_fails(pool: PgPool) -> Result<(), AcError> {
        // Create first credential
        create_service_credential(
            &pool,
            "duplicate-test",
            "hash1",
            "global-controller",
            None,
            &["scope1".to_string()],
        )
        .await?;

        // Try to create duplicate client_id (should fail due to unique constraint)
        let result = create_service_credential(
            &pool,
            "duplicate-test", // Same client_id
            "hash2",
            "meeting-controller",
            None,
            &["scope2".to_string()],
        )
        .await;

        let err = result.expect_err("Expected Database error for duplicate client_id");
        assert!(matches!(err, AcError::Database(_)));

        Ok(())
    }

    // ============================================================================
    // Tests for new CRUD repository functions
    // ============================================================================

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_all_empty(pool: PgPool) -> Result<(), AcError> {
        // Get all credentials when database is empty
        let credentials = get_all(&pool).await?;
        assert_eq!(
            credentials.len(),
            0,
            "Empty database should return empty vec"
        );
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_all_multiple(pool: PgPool) -> Result<(), AcError> {
        // Create multiple credentials
        let cred1 = create_service_credential(
            &pool,
            "client-1",
            "hash1",
            "global-controller",
            Some("us-west-2"),
            &["meeting:create".to_string()],
        )
        .await?;

        let cred2 = create_service_credential(
            &pool,
            "client-2",
            "hash2",
            "meeting-controller",
            None,
            &["participant:manage".to_string()],
        )
        .await?;

        let cred3 = create_service_credential(
            &pool,
            "client-3",
            "hash3",
            "media-handler",
            Some("eu-west-1"),
            &["media:process".to_string()],
        )
        .await?;

        // Get all credentials
        let all = get_all(&pool).await?;
        assert_eq!(all.len(), 3, "Should return all 3 credentials");

        // Verify they're ordered by created_at DESC (newest first)
        assert_eq!(all[0].credential_id, cred3.credential_id);
        assert_eq!(all[1].credential_id, cred2.credential_id);
        assert_eq!(all[2].credential_id, cred1.credential_id);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_credential_id_found(pool: PgPool) -> Result<(), AcError> {
        // Create a credential
        let created = create_service_credential(
            &pool,
            "test-get-by-id",
            "hash",
            "global-controller",
            Some("us-west-2"),
            &["meeting:create".to_string()],
        )
        .await?;

        // Retrieve by credential_id
        let result = get_by_credential_id(&pool, created.credential_id).await?;
        assert!(result.is_some(), "Should find credential by ID");

        let retrieved = result.unwrap();
        assert_eq!(retrieved.credential_id, created.credential_id);
        assert_eq!(retrieved.client_id, "test-get-by-id");
        assert_eq!(retrieved.service_type, "global-controller");
        assert_eq!(retrieved.region, Some("us-west-2".to_string()));
        assert_eq!(retrieved.scopes, vec!["meeting:create".to_string()]);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_get_by_credential_id_not_found(pool: PgPool) -> Result<(), AcError> {
        // Try to get nonexistent credential by random UUID
        let random_uuid = Uuid::new_v4();
        let result = get_by_credential_id(&pool, random_uuid).await?;
        assert!(result.is_none(), "Should return None for unknown UUID");
        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_metadata_success(pool: PgPool) -> Result<(), AcError> {
        // Create credential with initial scopes
        let created = create_service_credential(
            &pool,
            "test-update-meta",
            "hash",
            "meeting-controller",
            None,
            &["scope1".to_string()],
        )
        .await?;

        // Update scopes
        let new_scopes = vec![
            "scope1".to_string(),
            "scope2".to_string(),
            "scope3".to_string(),
        ];
        let updated = update_metadata(&pool, created.credential_id, &new_scopes).await?;

        assert_eq!(updated.credential_id, created.credential_id);
        assert_eq!(updated.scopes, new_scopes);
        assert!(
            updated.updated_at >= created.updated_at,
            "updated_at should be >= created_at"
        );

        // Verify in database
        let retrieved = get_by_credential_id(&pool, created.credential_id).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().scopes, new_scopes);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_metadata_not_found(pool: PgPool) -> Result<(), AcError> {
        // Try to update nonexistent credential
        let random_uuid = Uuid::new_v4();
        let result = update_metadata(&pool, random_uuid, &["scope".to_string()]).await;

        assert!(result.is_err(), "Should error for unknown credential");
        let err = result.unwrap_err();
        assert!(
            matches!(err, AcError::Database(_)),
            "Should be Database error"
        );

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_delete_success(pool: PgPool) -> Result<(), AcError> {
        // Create credential
        let created = create_service_credential(
            &pool,
            "test-delete",
            "hash",
            "media-handler",
            None,
            &["media:process".to_string()],
        )
        .await?;

        // Verify it exists
        let before_delete = get_by_credential_id(&pool, created.credential_id).await?;
        assert!(before_delete.is_some());

        // Delete credential
        let delete_result = delete(&pool, created.credential_id).await;
        assert!(delete_result.is_ok(), "Should delete successfully");

        // Verify it's gone
        let after_delete = get_by_credential_id(&pool, created.credential_id).await?;
        assert!(after_delete.is_none(), "Credential should be deleted");

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_delete_not_found(pool: PgPool) -> Result<(), AcError> {
        // Try to delete nonexistent credential
        let random_uuid = Uuid::new_v4();
        let result = delete(&pool, random_uuid).await;

        assert!(result.is_err(), "Should error for unknown credential");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::Database(msg) if msg.contains("not found")),
            "Expected Database error with 'not found', got: {:?}",
            err
        );

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rotate_secret_success(pool: PgPool) -> Result<(), AcError> {
        // Create credential
        let created = create_service_credential(
            &pool,
            "test-rotate",
            "original-hash",
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        assert_eq!(created.client_secret_hash, "original-hash");

        // Rotate secret
        let new_hash = "new-hash-value";
        let rotated = rotate_secret(&pool, created.credential_id, new_hash).await?;

        assert_eq!(rotated.credential_id, created.credential_id);
        assert_eq!(rotated.client_secret_hash, new_hash);
        assert_ne!(rotated.client_secret_hash, created.client_secret_hash);
        assert!(
            rotated.updated_at >= created.updated_at,
            "updated_at should be >= created_at"
        );

        // Verify in database
        let retrieved = get_by_credential_id(&pool, created.credential_id).await?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().client_secret_hash, new_hash);

        Ok(())
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rotate_secret_not_found(pool: PgPool) -> Result<(), AcError> {
        // Try to rotate secret for nonexistent credential
        let random_uuid = Uuid::new_v4();
        let result = rotate_secret(&pool, random_uuid, "new-hash").await;

        assert!(result.is_err(), "Should error for unknown credential");
        let err = result.unwrap_err();
        assert!(
            matches!(err, AcError::Database(_)),
            "Should be Database error"
        );

        Ok(())
    }
}
