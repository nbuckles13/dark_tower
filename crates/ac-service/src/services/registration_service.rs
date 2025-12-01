use crate::crypto;
use crate::errors::AcError;
use crate::models::{AuthEventType, RegisterServiceResponse, ServiceType};
use crate::repositories::{auth_events, service_credentials};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Register a new service and generate credentials
///
/// Generates client_id (UUID), generates and hashes client_secret (bcrypt),
/// assigns default scopes based on service_type, stores in database
pub async fn register_service(
    pool: &PgPool,
    service_type: &str,
    region: Option<String>,
) -> Result<RegisterServiceResponse, AcError> {
    // Validate and parse service_type
    let svc_type = ServiceType::from_str(service_type).map_err(|e| {
        AcError::Database(format!("Invalid service_type: '{}'. {}", service_type, e))
    })?;

    // Generate client_id (UUID)
    let client_id = Uuid::new_v4().to_string();

    // Generate client_secret (32 bytes, CSPRNG, base64)
    let client_secret = crypto::generate_client_secret()?;

    // Hash client_secret with bcrypt (cost factor 12)
    let client_secret_hash = crypto::hash_client_secret(&client_secret)?;

    // Get default scopes for this service type
    let scopes = svc_type.default_scopes();

    // Store in database
    let credential = service_credentials::create_service_credential(
        pool,
        &client_id,
        &client_secret_hash,
        service_type,
        region.as_deref(),
        &scopes,
    )
    .await?;

    // Log registration event
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::ServiceRegistered.as_str(),
        None,
        Some(credential.credential_id),
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "service_type": service_type,
            "region": region,
            "scopes": scopes,
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    // Return credentials (this is the ONLY time the plaintext client_secret is shown)
    Ok(RegisterServiceResponse {
        client_id,
        client_secret, // Plaintext secret - store this securely!
        service_type: service_type.to_string(),
        scopes,
    })
}

/// Update scopes for an existing service
#[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
pub async fn update_service_scopes(
    pool: &PgPool,
    client_id: &str,
    new_scopes: Vec<String>,
) -> Result<(), AcError> {
    // Fetch credential
    let credential = service_credentials::get_by_client_id(pool, client_id)
        .await?
        .ok_or_else(|| AcError::Database("Service credential not found".to_string()))?;

    // Update scopes
    service_credentials::update_scopes(pool, credential.credential_id, &new_scopes).await?;

    // Log scope update
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::ServiceTokenIssued.as_str(), // Reusing token issued type
        None,
        Some(credential.credential_id),
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "action": "scopes_updated",
            "old_scopes": credential.scopes,
            "new_scopes": new_scopes,
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}

/// Deactivate a service credential
#[allow(dead_code)] // Library function - will be used in Phase 4 admin endpoints
pub async fn deactivate_service(pool: &PgPool, client_id: &str) -> Result<(), AcError> {
    // Fetch credential
    let credential = service_credentials::get_by_client_id(pool, client_id)
        .await?
        .ok_or_else(|| AcError::Database("Service credential not found".to_string()))?;

    // Deactivate
    service_credentials::deactivate(pool, credential.credential_id).await?;

    // Log deactivation
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::ServiceTokenFailed.as_str(), // Reusing failed type
        None,
        Some(credential.credential_id),
        true,
        None,
        None,
        None,
        Some(serde_json::json!({
            "action": "service_deactivated",
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use sqlx::PgPool;

    // ============================================================================
    // P1 Security Tests - SQL Injection Prevention
    // ============================================================================

    /// P1-3: Test client_id with SQL injection metacharacters
    ///
    /// Verifies that SQL metacharacters in client_id are properly escaped
    /// and don't cause SQL injection vulnerabilities.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_client_id_sql_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        // Attempt registration with SQL injection attack in service_type parameter
        // The attack attempts to break out of quotes and execute arbitrary SQL
        let malicious_service_type = "global-controller'; DROP TABLE service_credentials; --";

        // This should either fail validation OR be safely escaped
        let result = register_service(&pool, malicious_service_type, None).await;

        // Should fail due to invalid service_type (doesn't match enum)
        assert!(
            result.is_err(),
            "SQL injection attempt in service_type should be rejected"
        );

        // Verify the table still exists by querying it
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM service_credentials")
            .fetch_one(&pool)
            .await
            .expect("service_credentials table should still exist");

        assert_eq!(
            count.0, 0,
            "No credentials should exist yet (table wasn't dropped)"
        );

        Ok(())
    }

    /// P1-3: Test service registration with special characters in region
    ///
    /// Verifies that special characters in optional fields like region
    /// are properly handled and don't cause SQL injection.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_region_special_characters_sanitized(pool: PgPool) -> Result<(), AcError> {
        // Register with SQL metacharacters in region field (keep under 50 chars for VARCHAR limit)
        let malicious_region = "us'; DROP TABLE service_credentials;--";

        let result = register_service(
            &pool,
            "global-controller",
            Some(malicious_region.to_string()),
        )
        .await;

        // Should succeed (region is just stored as text, sqlx parameterizes it)
        assert!(
            result.is_ok(),
            "Registration with special chars in region should succeed (safely escaped)"
        );

        let response = result.unwrap();

        // Verify the credential was created with the exact region string
        let credential = service_credentials::get_by_client_id(&pool, &response.client_id)
            .await?
            .expect("Credential should exist");

        assert_eq!(
            credential.region.as_deref(),
            Some(malicious_region),
            "Region should be stored exactly as provided (safely escaped)"
        );

        // Verify table still exists and has exactly 1 row
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM service_credentials")
            .fetch_one(&pool)
            .await
            .map_err(|e| AcError::Database(format!("Failed to count credentials: {}", e)))?;

        assert_eq!(count.0, 1, "Exactly 1 credential should exist");

        Ok(())
    }

    /// P1-3: Test scopes array with injection attempts
    ///
    /// Verifies that scopes (stored as ARRAY) can't be exploited for SQL injection.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_scopes_array_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        // Create a credential manually with malicious scope strings
        let malicious_scopes = vec![
            "meeting:create'; DROP TABLE auth_events; --".to_string(),
            "admin:all' OR '1'='1".to_string(),
            "valid:scope".to_string(),
        ];

        let client_id = "test-sql-scopes";
        let secret_hash = crypto::hash_client_secret("test-secret")?;

        // SQLx should safely parameterize array values
        let credential = service_credentials::create_service_credential(
            &pool,
            client_id,
            &secret_hash,
            "global-controller",
            None,
            &malicious_scopes,
        )
        .await?;

        // Verify scopes were stored exactly as provided (not executed as SQL)
        assert_eq!(credential.scopes.len(), 3);
        assert!(credential.scopes.contains(&malicious_scopes[0]));
        assert!(credential.scopes.contains(&malicious_scopes[1]));

        // Verify auth_events table still exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM auth_events")
            .fetch_one(&pool)
            .await
            .expect("auth_events table should still exist");

        assert!(count.0 >= 0, "auth_events table should be accessible");

        Ok(())
    }

    /// P1-3: Test Unicode and special characters in string fields
    ///
    /// Verifies that Unicode characters, NULL bytes, and other edge cases
    /// are handled safely.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_unicode_and_null_bytes_handled_safely(pool: PgPool) -> Result<(), AcError> {
        // Test various problematic strings
        let test_cases = vec![
            ("unicode_emoji", "🔒🚀💾"),         // Emoji
            ("unicode_chinese", "数据库注入"),   // Chinese characters
            ("unicode_arabic", "حقن SQL"),       // Arabic
            ("backslash", "test\\backslash"),    // Backslashes
            ("quotes", "test'quote\"double"),    // Mixed quotes
            ("newline", "test\nwith\nnewlines"), // Newlines
        ];

        for (client_id, region_value) in test_cases {
            let secret_hash = crypto::hash_client_secret("test-secret")?;

            let credential = service_credentials::create_service_credential(
                &pool,
                client_id,
                &secret_hash,
                "media-handler",
                Some(region_value),
                &["test:scope".to_string()],
            )
            .await?;

            // Verify the value was stored exactly as provided
            assert_eq!(
                credential.region.as_deref(),
                Some(region_value),
                "Region '{}' should be stored exactly as provided",
                region_value
            );

            // Verify we can retrieve it
            let retrieved = service_credentials::get_by_client_id(&pool, client_id)
                .await?
                .expect("Should retrieve credential");

            assert_eq!(
                retrieved.region.as_deref(),
                Some(region_value),
                "Retrieved region should match stored value"
            );
        }

        Ok(())
    }

    /// P1-3: Test oversized input handling
    ///
    /// Verifies that extremely long inputs are handled gracefully.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_oversized_input_handling(pool: PgPool) -> Result<(), AcError> {
        // Create an extremely long client_id (reasonable limit is ~255 chars)
        let long_client_id = "a".repeat(1000);
        let secret_hash = crypto::hash_client_secret("test-secret")?;

        // This should either succeed (if DB allows it) or fail gracefully
        let result = service_credentials::create_service_credential(
            &pool,
            &long_client_id,
            &secret_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await;

        // Either way, it shouldn't cause a panic or SQL injection
        match result {
            Ok(_credential) => {
                // If it succeeded, verify we can retrieve it
                let retrieved = service_credentials::get_by_client_id(&pool, &long_client_id)
                    .await?
                    .expect("Should retrieve credential");
                assert_eq!(retrieved.client_id, long_client_id);
            }
            Err(e) => {
                // If it failed, it should be a proper database error, not a panic
                assert!(
                    matches!(e, AcError::Database(_)),
                    "Oversized input should fail with Database error, got: {:?}",
                    e
                );
            }
        }

        Ok(())
    }

    /// P1-3: Test SQL comment injection attempts
    ///
    /// Verifies that SQL comments (--,  /*  */,  #) don't allow SQL injection.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_sql_comment_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        let comment_attacks = vec![
            "test-- comment",
            "test/* block comment */",
            "test' --",
            "test'; --",
        ];

        for client_id in comment_attacks {
            let secret_hash = crypto::hash_client_secret("test-secret")?;

            // Should succeed - comments are just part of the string value
            let credential = service_credentials::create_service_credential(
                &pool,
                client_id,
                &secret_hash,
                "global-controller",
                None,
                &["test:scope".to_string()],
            )
            .await?;

            // Verify the client_id was stored exactly as provided
            assert_eq!(credential.client_id, client_id);

            // Verify we can retrieve it
            let retrieved = service_credentials::get_by_client_id(&pool, client_id)
                .await?
                .expect("Should retrieve credential");

            assert_eq!(retrieved.client_id, client_id);
        }

        Ok(())
    }

    /// P1-3: Test boolean context injection
    ///
    /// Verifies that SQL boolean injection patterns don't work.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_boolean_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        // Classic SQL injection patterns that try to make WHERE clauses always true
        let boolean_attacks = vec![
            "' OR '1'='1",
            "' OR 1=1 --",
            "admin' OR 'a'='a",
            "' OR true --",
        ];

        for malicious_id in boolean_attacks {
            let secret_hash = crypto::hash_client_secret("test-secret")?;

            // Create with malicious client_id
            service_credentials::create_service_credential(
                &pool,
                malicious_id,
                &secret_hash,
                "global-controller",
                None,
                &["test:scope".to_string()],
            )
            .await?;

            // Try to fetch - should only return exact match, not "all rows"
            let result = service_credentials::get_by_client_id(&pool, malicious_id).await?;

            assert!(result.is_some(), "Should find the exact client_id");

            let credential = result.unwrap();
            assert_eq!(
                credential.client_id, malicious_id,
                "Should return exact match only"
            );

            // Verify only one credential exists with this client_id
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM service_credentials WHERE client_id = $1")
                    .bind(malicious_id)
                    .fetch_one(&pool)
                    .await
                    .map_err(|e| {
                        AcError::Database(format!("Failed to count credentials: {}", e))
                    })?;

            assert_eq!(count.0, 1, "Should have exactly 1 matching credential");
        }

        Ok(())
    }
}
