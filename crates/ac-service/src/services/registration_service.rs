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
    /// Verifies that Unicode characters and special characters are handled safely.
    ///
    /// **Note on NULL bytes**: PostgreSQL TEXT fields reject NULL bytes (\0) by design,
    /// returning a database error "invalid byte sequence for encoding". This is acceptable
    /// behavior - the database enforces data integrity. We don't test NULL bytes here
    /// because the database-level rejection is the correct security boundary.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_unicode_and_special_characters_handled_safely(
        pool: PgPool,
    ) -> Result<(), AcError> {
        // Test various problematic strings
        // Note: NULL bytes (\0) intentionally omitted - PostgreSQL rejects them at DB level
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

    /// P1-3: Test UNION SELECT injection prevention
    ///
    /// Verifies that UNION SELECT attacks (attempting to combine results from
    /// multiple tables to leak data) are prevented by sqlx parameterization.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_union_select_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        let union_attacks = vec![
            "test' UNION SELECT NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL--",
            "test' UNION SELECT credential_id,client_id,client_secret_hash,service_type,region,scopes::text,is_active::text,created_at::text,updated_at::text FROM service_credentials--",
            "' UNION ALL SELECT table_name,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL FROM information_schema.tables--",
        ];

        for malicious_id in union_attacks {
            let secret_hash = crypto::hash_client_secret("test-secret")?;

            // SQLx should safely parameterize, treating this as a literal string
            let credential = service_credentials::create_service_credential(
                &pool,
                malicious_id,
                &secret_hash,
                "global-controller",
                None,
                &["test:scope".to_string()],
            )
            .await?;

            // Verify the malicious string was stored as-is (safely escaped)
            assert_eq!(credential.client_id, malicious_id);

            // Verify retrieval returns exact match only (no UNION executed)
            let result = service_credentials::get_by_client_id(&pool, malicious_id).await?;
            assert!(result.is_some(), "Should find exact client_id");

            let retrieved = result.unwrap();
            assert_eq!(
                retrieved.client_id, malicious_id,
                "Should return exact match, not UNION results"
            );

            // Verify only one credential exists (UNION didn't leak other rows)
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM service_credentials WHERE client_id = $1")
                    .bind(malicious_id)
                    .fetch_one(&pool)
                    .await
                    .map_err(|e| AcError::Database(format!("Failed to count: {}", e)))?;
            assert_eq!(count.0, 1, "Exactly 1 credential should exist");
        }

        Ok(())
    }

    /// P1-3: Test second-order SQL injection prevention
    ///
    /// Verifies that malicious data stored in the database cannot be exploited
    /// when retrieved and used in subsequent queries. This tests that sqlx
    /// parameterization is used consistently throughout the codebase.
    ///
    /// Second-order SQL injection occurs when:
    /// 1. Malicious input is safely stored in the database
    /// 2. That data is retrieved and used in a new query
    /// 3. The new query is vulnerable to SQL injection
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_second_order_sql_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        // Step 1: Store malicious data in a scope
        let malicious_scope = "admin:all'; DROP TABLE service_credentials; --";
        let client_id = "second-order-test";
        let secret_hash = crypto::hash_client_secret("test-secret")?;

        service_credentials::create_service_credential(
            &pool,
            client_id,
            &secret_hash,
            "global-controller",
            None,
            &[malicious_scope.to_string()],
        )
        .await?;

        // Step 2: Retrieve the credential (malicious scope is now in memory)
        let credential = service_credentials::get_by_client_id(&pool, client_id)
            .await?
            .expect("Should retrieve credential");

        assert!(
            credential.scopes.contains(&malicious_scope.to_string()),
            "Malicious scope should be stored safely"
        );

        // Step 3: Verify table still exists (no DROP TABLE executed)
        let table_exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM information_schema.tables
             WHERE table_name = 'service_credentials'",
        )
        .fetch_one(&pool)
        .await
        .expect("Should query information_schema");

        assert_eq!(
            table_exists.0, 1,
            "service_credentials table should still exist"
        );

        // Step 4: Use the retrieved scope in a hypothetical query context
        // This simulates using stored data in a new query.
        // If scope validation used string concatenation instead of parameterization,
        // the malicious scope could execute SQL.
        for scope in &credential.scopes {
            // Simulate a query that uses the scope (properly parameterized)
            let scope_check: (bool,) = sqlx::query_as("SELECT $1::text = $1::text") // Dummy query using parameter
                .bind(scope)
                .fetch_one(&pool)
                .await
                .expect("Parameterized query should succeed");

            assert!(scope_check.0, "Query should execute safely");
        }

        // Final verification: Table and data intact
        let final_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM service_credentials")
            .fetch_one(&pool)
            .await
            .expect("Should count credentials");

        assert_eq!(
            final_count.0, 1,
            "Exactly 1 credential should exist (no second-order injection)"
        );

        Ok(())
    }

    /// P1-3: Test time-based blind SQL injection prevention
    ///
    /// Verifies that time-based blind SQL injection attacks (using pg_sleep() to infer
    /// information based on query timing) are prevented by sqlx parameterization.
    ///
    /// Time-based blind SQL injection is a technique where attackers inject database
    /// sleep functions (pg_sleep in PostgreSQL, SLEEP in MySQL) to extract information
    /// based on whether queries complete quickly or are delayed. This is particularly
    /// dangerous when error messages are suppressed or when other injection techniques
    /// don't produce visible results.
    ///
    /// Attack vectors tested:
    /// - Direct pg_sleep injection: `'; SELECT pg_sleep(5); --`
    /// - Boolean-based sleep: `' OR 1=1; SELECT pg_sleep(5); --`
    /// - Nested sleep in region parameter
    ///
    /// Expected behavior:
    /// With proper parameterization, pg_sleep() is treated as literal string data,
    /// not executed as SQL. Queries complete in milliseconds, not seconds.
    ///
    /// NOTE: This test uses timing assertions to verify pg_sleep(5) wasn't executed.
    /// We use a 4-second threshold (well under the 5-second sleep) to accommodate
    /// slow CI environments while still detecting injection. Skipped under coverage
    /// because instrumentation adds unpredictable overhead.
    #[sqlx::test(migrations = "../../migrations")]
    #[cfg_attr(coverage, ignore)]
    async fn test_time_based_sql_injection_prevented(pool: PgPool) -> Result<(), AcError> {
        use tokio::time::Instant;

        // Attack vectors that attempt to execute pg_sleep() for 5 seconds
        let time_based_attacks = vec![
            (
                "service_type",
                "global-controller'; SELECT pg_sleep(5); --",
                None,
            ),
            (
                "service_type",
                "media-handler' OR 1=1; SELECT pg_sleep(5); --",
                None,
            ),
            (
                "region",
                "global-controller",
                Some("us-east'; SELECT pg_sleep(5); --".to_string()),
            ),
            (
                "region",
                "meeting-controller",
                Some("eu-west' OR '1'='1'; SELECT pg_sleep(5); --".to_string()),
            ),
        ];

        for (attack_location, service_type, region) in time_based_attacks {
            let start_time = Instant::now();

            // Attempt registration with time-based injection
            let result = register_service(&pool, service_type, region.clone()).await;

            let elapsed = start_time.elapsed();

            // Query should complete in well under 5 seconds, proving pg_sleep didn't execute.
            // We use 4 seconds as threshold to accommodate slow CI while still detecting
            // the 5-second sleep attack.
            assert!(
                elapsed.as_millis() < 4000,
                "Query completed in {:?} (expected <4s). Time-based SQL injection in {} may have executed!",
                elapsed,
                attack_location
            );

            // The attack should either fail validation (bad service_type)
            // or succeed with the payload safely stored as literal text
            match attack_location {
                "service_type" => {
                    // Invalid service_type should fail
                    assert!(
                        result.is_err(),
                        "Malicious service_type should fail validation"
                    );
                }
                "region" => {
                    // Region is just text, should succeed with safe parameterization
                    assert!(
                        result.is_ok(),
                        "Region with pg_sleep should be safely stored as text"
                    );

                    if let Ok(response) = result {
                        // Verify the malicious string was stored as-is
                        let credential =
                            service_credentials::get_by_client_id(&pool, &response.client_id)
                                .await?
                                .expect("Credential should exist");

                        assert_eq!(
                            credential.region, region,
                            "pg_sleep payload should be stored as literal text"
                        );
                    }
                }
                _ => unreachable!(),
            }
        }

        // Additional test: Verify scopes with pg_sleep are also safe
        let malicious_scopes = vec![
            "admin:all'; SELECT pg_sleep(5); --".to_string(),
            "test:scope".to_string(),
        ];

        let client_id = "time-based-test";
        let secret_hash = crypto::hash_client_secret("test-secret")?;

        let start_time = Instant::now();

        let credential = service_credentials::create_service_credential(
            &pool,
            client_id,
            &secret_hash,
            "global-controller",
            None,
            &malicious_scopes,
        )
        .await?;

        let elapsed = start_time.elapsed();

        // Should complete quickly (under 4 seconds to accommodate slow CI)
        assert!(
            elapsed.as_millis() < 4000,
            "Scope insertion completed in {:?} (expected <4s)",
            elapsed
        );

        // Verify the malicious scope was stored as literal text
        assert!(credential.scopes.contains(&malicious_scopes[0]));

        // Test retrieval timing (verify pg_sleep doesn't execute on SELECT)
        let start_time = Instant::now();

        let retrieved = service_credentials::get_by_client_id(&pool, client_id).await?;

        let elapsed = start_time.elapsed();

        assert!(
            elapsed.as_millis() < 4000,
            "Retrieval completed in {:?} (expected <4s)",
            elapsed
        );

        assert!(
            retrieved.is_some(),
            "Should retrieve credential with pg_sleep payload"
        );

        Ok(())
    }
}
