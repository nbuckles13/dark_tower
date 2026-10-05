#[cfg(test)]
use crate::config::DEFAULT_BCRYPT_COST;
use crate::crypto;
use crate::errors::AcError;
use crate::models::{AuthEventType, RegisterServiceResponse, ServiceCredential, ServiceType};
use crate::observability::metrics::record_audit_log_failure;
use crate::repositories::{auth_events, service_credentials};
use common::secret::{ExposeSecret, SecretString};
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

/// Register a new service and generate credentials
///
/// Generates client_id (UUID), generates and hashes client_secret (bcrypt),
/// assigns default scopes based on service_type, stores in database
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `service_type` - Type of service (e.g., "global-controller", "media-handler")
/// * `region` - Optional region identifier
/// * `bcrypt_cost` - Bcrypt cost factor for password hashing (10-14, default 12)
pub async fn register_service(
    pool: &PgPool,
    service_type: &str,
    region: Option<String>,
    bcrypt_cost: u32,
) -> Result<RegisterServiceResponse, AcError> {
    // Validate and parse service_type
    let svc_type = ServiceType::from_str(service_type)
        .map_err(|_| AcError::BadRequest(INVALID_SERVICE_TYPE_MESSAGE))?;

    // Generate client_id (UUID)
    let client_id = Uuid::new_v4().to_string();

    // Generate client_secret (32 bytes, CSPRNG, base64)
    let client_secret = crypto::generate_client_secret()?;

    // Hash client_secret with bcrypt using configured cost factor
    let client_secret_hash =
        crypto::hash_client_secret(client_secret.expose_secret(), bcrypt_cost)?;

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
    log_admin_event(
        pool,
        AuthEventType::ServiceRegistered,
        credential.credential_id,
        true,
        None,
        serde_json::json!({
            "service_type": service_type,
            "region": region,
            "scopes": scopes,
        }),
    )
    .await;

    // Return credentials (this is the ONLY time the plaintext client_secret is shown)
    Ok(RegisterServiceResponse {
        client_id,
        client_secret, // Plaintext secret - store this securely!
        service_type: service_type.to_string(),
        scopes,
    })
}

/// Maximum number of scopes a client credential may hold. Bounds the PUT
/// payload and the requested-scopes list recorded on a refused audit row.
pub const MAX_SCOPES_PER_CLIENT: usize = 32;

/// Maximum length of a single scope string.
const MAX_SCOPE_LEN: usize = 100;

/// Body message for an unknown `service_type`. Lists every valid
/// [`ServiceType`] (pinned by `tests::invalid_service_type_message_names_every_type`).
pub const INVALID_SERVICE_TYPE_MESSAGE: &str =
    "Invalid service_type. Must be one of: global-controller, meeting-controller, media-handler";

/// Body message when a credential is not active (rotate on a deactivated client).
pub const CLIENT_DEACTIVATED_MESSAGE: &str = "client is deactivated";

/// Why a scope-update request was refused, with its fixed audit
/// `failure_reason` token and body message. Never carries the raw input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeRejection {
    TooMany,
    Empty,
    TooLong,
    InvalidCharacters,
    NotPermitted,
}

impl ScopeRejection {
    fn failure_reason(self) -> &'static str {
        match self {
            ScopeRejection::TooMany => "too_many_scopes",
            ScopeRejection::Empty | ScopeRejection::TooLong | ScopeRejection::InvalidCharacters => {
                "invalid_scope_format"
            }
            ScopeRejection::NotPermitted => "scope_not_permitted",
        }
    }

    fn message(self) -> &'static str {
        match self {
            ScopeRejection::TooMany => "Too many scopes",
            ScopeRejection::Empty => "Scope cannot be empty",
            ScopeRejection::TooLong => "Scope exceeds maximum length of 100 characters",
            ScopeRejection::InvalidCharacters => "Scope contains invalid characters",
            ScopeRejection::NotPermitted => "Scope not permitted for this client's service type",
        }
    }
}

/// Validate a requested scope list against format rules and the scope policy.
///
/// Policy (ADR-0003 Component 2, auth-controller ruling A1): PUT may only
/// NARROW — every requested scope must be in the credential's
/// `ServiceType::default_scopes()` (the SSoT). An unparseable stored
/// `service_type` permits nothing (fail closed). An empty list is allowed.
fn check_scopes(service_type: &str, requested: &[String]) -> Result<(), ScopeRejection> {
    if requested.len() > MAX_SCOPES_PER_CLIENT {
        return Err(ScopeRejection::TooMany);
    }
    for scope in requested {
        if scope.is_empty() {
            return Err(ScopeRejection::Empty);
        }
        if scope.len() > MAX_SCOPE_LEN {
            return Err(ScopeRejection::TooLong);
        }
        // Alphanumeric, hyphens, dots, colons (common in OAuth scopes).
        if !scope
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == ':')
        {
            return Err(ScopeRejection::InvalidCharacters);
        }
    }
    let permitted = ServiceType::from_str(service_type)
        .map(|t| t.default_scopes())
        .unwrap_or_default();
    if requested.iter().any(|s| !permitted.contains(s)) {
        return Err(ScopeRejection::NotPermitted);
    }
    Ok(())
}

/// Write one audit row for a credential-scoped event (registration and the
/// admin mutations) on `credential_id`. Best-effort (ADR-0032): a
/// failed insert logs a warning and bumps `ac_audit_log_failures_total`; the
/// caller's operation still succeeds.
async fn log_admin_event(
    pool: &PgPool,
    event_type: AuthEventType,
    credential_id: Uuid,
    success: bool,
    failure_reason: Option<&str>,
    metadata: serde_json::Value,
) {
    if let Err(e) = auth_events::log_event(
        pool,
        event_type.as_str(),
        None,
        Some(credential_id),
        success,
        failure_reason,
        None,
        None,
        Some(metadata),
    )
    .await
    {
        tracing::warn!(event_type = event_type.as_str(), error = %e, "Failed to log auth event");
        record_audit_log_failure(event_type, "db_write_failed");
    }
}

/// Fetch a credential by id, mapping absence to `NotFound`. The single home of
/// the admin "client not found" error (handlers and the mutations below).
pub async fn get_service(pool: &PgPool, credential_id: Uuid) -> Result<ServiceCredential, AcError> {
    service_credentials::get_by_credential_id(pool, credential_id)
        .await?
        .ok_or_else(|| not_found(credential_id))
}

fn not_found(credential_id: Uuid) -> AcError {
    AcError::NotFound(format!("Client with ID {} not found", credential_id))
}

/// Narrow a credential's scopes (PUT `/api/v1/admin/clients/{id}`).
///
/// The single writer of the scope change, its `service_scopes_updated` audit
/// row and the audit-failure metric. A refused request on an existing
/// credential writes a `success=false` row with a fixed `failure_reason`
/// (`scope_not_permitted` also records the requested scopes — format-valid and
/// at most [`MAX_SCOPES_PER_CLIENT`]; format failures record nothing of the
/// input). Does not touch `is_active`. Unknown id → `NotFound`, no row.
pub async fn update_service_scopes(
    pool: &PgPool,
    credential_id: Uuid,
    new_scopes: Vec<String>,
    actor_sub: &str,
) -> Result<ServiceCredential, AcError> {
    let credential = get_service(pool, credential_id).await?;

    if let Err(rejection) = check_scopes(&credential.service_type, &new_scopes) {
        let metadata = if rejection == ScopeRejection::NotPermitted {
            serde_json::json!({ "actor_sub": actor_sub, "requested_scopes": new_scopes })
        } else {
            serde_json::json!({ "actor_sub": actor_sub })
        };
        log_admin_event(
            pool,
            AuthEventType::ServiceScopesUpdated,
            credential_id,
            false,
            Some(rejection.failure_reason()),
            metadata,
        )
        .await;
        return Err(AcError::BadRequest(rejection.message()));
    }

    // `old_scopes` comes from the row the UPDATE locked, not the read above,
    // so a concurrent PUT can never make the audit row record a stale value.
    let update = service_credentials::update_scopes(pool, credential_id, &new_scopes)
        .await?
        .ok_or_else(|| not_found(credential_id))?;
    log_admin_event(
        pool,
        AuthEventType::ServiceScopesUpdated,
        credential_id,
        true,
        None,
        serde_json::json!({
            "actor_sub": actor_sub,
            "old_scopes": update.old_scopes,
            "new_scopes": new_scopes,
        }),
    )
    .await;
    Ok(update.credential)
}

/// Revoke a credential (DELETE `/api/v1/admin/clients/{id}`).
///
/// Soft delete: `is_active = false`. A hard delete is impossible —
/// `auth_events.credential_id` references the row with no `ON DELETE` (SET
/// NULL would violate `event_has_subject`, CASCADE would destroy the audit
/// trail). Idempotent: an already-inactive credential returns `Ok` with no new
/// audit row; `service_deactivated` is written only on the active→inactive
/// transition. Tokens already issued stay valid until `exp` (stateless JWTs,
/// ADR-0007). Unknown id → `NotFound`.
pub async fn deactivate_service(
    pool: &PgPool,
    credential_id: Uuid,
    actor_sub: &str,
) -> Result<ServiceCredential, AcError> {
    // The conditional UPDATE is authoritative: only the call that performs the
    // active→inactive transition gets a row back and writes the audit row, so
    // concurrent DELETEs record the transition exactly once.
    let Some(deactivated) = service_credentials::deactivate(pool, credential_id).await? else {
        // Already inactive (or gone): idempotent, no audit row.
        return get_service(pool, credential_id).await;
    };
    log_admin_event(
        pool,
        AuthEventType::ServiceDeactivated,
        credential_id,
        true,
        None,
        serde_json::json!({ "actor_sub": actor_sub }),
    )
    .await;
    Ok(deactivated)
}

/// Rotate a credential's secret (POST `/api/v1/admin/clients/{id}/rotate-secret`).
///
/// Returns the credential and the new plaintext secret — the only time it is
/// ever shown. A deactivated credential is refused with `Conflict` BEFORE any
/// secret is generated (the stored hash is untouched; the UPDATE is also
/// conditional on `is_active`, so a concurrent revoke cannot slip through) and a `success=false`
/// row with `failure_reason = "client_deactivated"`. The audit row never
/// carries the secret or its hash. Tokens already issued under the old secret
/// stay valid until `exp` (ADR-0007). Unknown id → `NotFound`.
pub async fn rotate_service_secret(
    pool: &PgPool,
    credential_id: Uuid,
    actor_sub: &str,
    bcrypt_cost: u32,
) -> Result<(ServiceCredential, SecretString), AcError> {
    let credential = get_service(pool, credential_id).await?;
    if !credential.is_active {
        return Err(refuse_rotation_of_inactive(pool, credential_id, actor_sub).await);
    }

    // Generate new client_secret (32 bytes, CSPRNG, base64) and hash it.
    let new_secret = crypto::generate_client_secret()?;
    let new_secret_hash = crypto::hash_client_secret(new_secret.expose_secret(), bcrypt_cost)?;
    // The UPDATE is conditional on `is_active`: if the credential was revoked
    // after the check above, nothing is stored and the new secret is dropped
    // here, never returned.
    let Some(rotated) =
        service_credentials::rotate_secret(pool, credential_id, &new_secret_hash).await?
    else {
        return Err(refuse_rotation_of_inactive(pool, credential_id, actor_sub).await);
    };
    log_admin_event(
        pool,
        AuthEventType::ServiceSecretRotated,
        credential_id,
        true,
        None,
        serde_json::json!({ "actor_sub": actor_sub, "client_id": credential.client_id }),
    )
    .await;
    Ok((rotated, new_secret))
}

/// Record a refused rotation of an inactive credential and build its error.
async fn refuse_rotation_of_inactive(
    pool: &PgPool,
    credential_id: Uuid,
    actor_sub: &str,
) -> AcError {
    log_admin_event(
        pool,
        AuthEventType::ServiceSecretRotated,
        credential_id,
        false,
        Some("client_deactivated"),
        serde_json::json!({ "actor_sub": actor_sub }),
    )
    .await;
    AcError::Conflict(CLIENT_DEACTIVATED_MESSAGE)
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
        let result =
            register_service(&pool, malicious_service_type, None, DEFAULT_BCRYPT_COST).await;

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
            DEFAULT_BCRYPT_COST,
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
            "scope-a'; DROP TABLE auth_events; --".to_string(),
            "admin:all' OR '1'='1".to_string(),
            "valid:scope".to_string(),
        ];

        let client_id = "test-sql-scopes";
        let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
            let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
        let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
            let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
            let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
            let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
        let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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
            let result =
                register_service(&pool, service_type, region.clone(), DEFAULT_BCRYPT_COST).await;

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
        let secret_hash = crypto::hash_client_secret("test-secret", DEFAULT_BCRYPT_COST)?;

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

    // ============================================================================
    // Coverage Tests - Service Lifecycle
    // ============================================================================

    /// Test successful service registration for all service types
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_service_all_types(pool: PgPool) -> Result<(), AcError> {
        let service_types = ["global-controller", "meeting-controller", "media-handler"];

        for service_type in service_types {
            let result = register_service(
                &pool,
                service_type,
                Some("us-west-2".to_string()),
                DEFAULT_BCRYPT_COST,
            )
            .await;

            assert!(
                result.is_ok(),
                "Registration for {} should succeed: {:?}",
                service_type,
                result.err()
            );

            let response = result.unwrap();

            // Verify response fields
            assert!(
                !response.client_id.is_empty(),
                "client_id should not be empty"
            );
            assert!(
                !response.client_secret.expose_secret().is_empty(),
                "client_secret should not be empty"
            );
            assert_eq!(response.service_type, service_type);
            assert!(!response.scopes.is_empty(), "scopes should not be empty");

            // Verify credential was stored
            let credential = service_credentials::get_by_client_id(&pool, &response.client_id)
                .await?
                .expect("Credential should exist");

            assert!(credential.is_active);
            assert_eq!(credential.service_type, service_type);
            assert_eq!(credential.region.as_deref(), Some("us-west-2"));
        }

        Ok(())
    }

    /// Test service registration without region
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_service_no_region(pool: PgPool) -> Result<(), AcError> {
        let result =
            register_service(&pool, "global-controller", None, DEFAULT_BCRYPT_COST).await?;

        let credential = service_credentials::get_by_client_id(&pool, &result.client_id)
            .await?
            .expect("Credential should exist");

        assert!(credential.region.is_none(), "Region should be None");

        Ok(())
    }

    /// Narrowing to a subset of the type's default scopes succeeds, persists,
    /// and leaves `is_active` alone.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_service_scopes(pool: PgPool) -> Result<(), AcError> {
        let response =
            register_service(&pool, "meeting-controller", None, DEFAULT_BCRYPT_COST).await?;
        let credential = service_credentials::get_by_client_id(&pool, &response.client_id)
            .await?
            .expect("Credential should exist");

        let new_scopes = vec!["service.write.gc".to_string()];
        let updated =
            update_service_scopes(&pool, credential.credential_id, new_scopes.clone(), "admin")
                .await?;

        assert_eq!(updated.scopes, new_scopes);
        assert_ne!(updated.scopes, response.scopes);
        assert!(updated.is_active);

        Ok(())
    }

    /// Unknown credential id → NotFound.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_update_service_scopes_not_found(pool: PgPool) -> Result<(), AcError> {
        let result = update_service_scopes(&pool, Uuid::new_v4(), vec![], "admin").await;
        assert!(matches!(result, Err(AcError::NotFound(_))));
        Ok(())
    }

    #[test]
    fn check_scopes_enforces_type_subset_and_format() {
        let ok = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // Subset of the GC defaults (and empty) are allowed.
        assert_eq!(
            check_scopes("global-controller", &ok(&["service.write.mc"])),
            Ok(())
        );
        assert_eq!(check_scopes("global-controller", &[]), Ok(()));
        // Escalation attempts are refused.
        for scope in [
            "admin:services",
            "admin.force-rotate-keys.ac",
            "service.rotate-keys.ac",
            "service.write.mh",
        ] {
            assert_eq!(
                check_scopes("global-controller", &ok(&[scope])),
                Err(ScopeRejection::NotPermitted),
                "{scope}"
            );
        }
        assert_eq!(
            check_scopes("media-handler", &ok(&["internal:meeting-token"])),
            Err(ScopeRejection::NotPermitted)
        );
        // Unparseable stored service_type permits nothing (fail closed), but
        // still allows the empty list.
        assert_eq!(
            check_scopes("not-a-type", &ok(&["service.write.mc"])),
            Err(ScopeRejection::NotPermitted)
        );
        assert_eq!(check_scopes("not-a-type", &[]), Ok(()));
        // Format.
        assert_eq!(
            check_scopes("global-controller", &ok(&[""])),
            Err(ScopeRejection::Empty)
        );
        assert_eq!(
            check_scopes("global-controller", &ok(&[&"a".repeat(101)])),
            Err(ScopeRejection::TooLong)
        );
        assert_eq!(
            check_scopes("global-controller", &ok(&["bad scope!"])),
            Err(ScopeRejection::InvalidCharacters)
        );
        let too_many: Vec<String> = (0..=MAX_SCOPES_PER_CLIENT)
            .map(|i| format!("s{i}"))
            .collect();
        assert_eq!(
            check_scopes("global-controller", &too_many),
            Err(ScopeRejection::TooMany)
        );
    }

    /// Deactivation flips `is_active`, and repeating it is a no-op `Ok`.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_deactivate_service(pool: PgPool) -> Result<(), AcError> {
        let response = register_service(&pool, "media-handler", None, DEFAULT_BCRYPT_COST).await?;
        let credential = service_credentials::get_by_client_id(&pool, &response.client_id)
            .await?
            .expect("Credential should exist");
        assert!(credential.is_active);

        let deactivated = deactivate_service(&pool, credential.credential_id, "admin").await?;
        assert!(!deactivated.is_active);
        let again = deactivate_service(&pool, credential.credential_id, "admin").await?;
        assert!(!again.is_active);

        Ok(())
    }

    /// Unknown credential id → NotFound.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_deactivate_service_not_found(pool: PgPool) -> Result<(), AcError> {
        let result = deactivate_service(&pool, Uuid::new_v4(), "admin").await;
        assert!(matches!(result, Err(AcError::NotFound(_))));
        Ok(())
    }

    #[test]
    fn invalid_service_type_message_names_every_type() {
        // `ServiceType::ALL` is generated from the same list as the enum
        // (`string_enum!`), so a new variant is covered here automatically.
        for t in ServiceType::ALL {
            assert!(
                INVALID_SERVICE_TYPE_MESSAGE.contains(t.as_str()),
                "{} missing from the message",
                t.as_str()
            );
        }
    }

    /// Test registration with invalid service type
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_invalid_service_type(pool: PgPool) -> Result<(), AcError> {
        let result = register_service(&pool, "invalid-service", None, DEFAULT_BCRYPT_COST).await;

        // A fixed message: the caller's input is never echoed into the body.
        assert!(
            matches!(result, Err(AcError::BadRequest(msg)) if msg == INVALID_SERVICE_TYPE_MESSAGE),
            "Should fail with the fixed invalid-service-type BadRequest"
        );

        Ok(())
    }

    // ============================================================================
    // Bcrypt Cost Propagation Tests
    // ============================================================================

    /// Test that register_service uses the provided bcrypt cost factor.
    /// This is critical for verifying that config.bcrypt_cost is properly propagated.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_register_service_uses_provided_bcrypt_cost(pool: PgPool) -> Result<(), AcError> {
        use crate::config::{MAX_BCRYPT_COST, MIN_BCRYPT_COST};

        // Test with minimum cost (10)
        let response_min =
            register_service(&pool, "global-controller", None, MIN_BCRYPT_COST).await?;

        // Retrieve the credential and verify the hash uses cost 10
        let credential_min = service_credentials::get_by_client_id(&pool, &response_min.client_id)
            .await?
            .expect("Credential should exist");

        // Bcrypt hash format: $2b$XX$... where XX is the cost
        let cost_min = credential_min
            .client_secret_hash
            .split('$')
            .nth(2)
            .expect("Hash should have cost field");
        assert_eq!(cost_min, "10", "Hash should use cost 10 (MIN_BCRYPT_COST)");

        // Test with maximum cost (14)
        let response_max = register_service(&pool, "media-handler", None, MAX_BCRYPT_COST).await?;

        let credential_max = service_credentials::get_by_client_id(&pool, &response_max.client_id)
            .await?
            .expect("Credential should exist");

        let cost_max = credential_max
            .client_secret_hash
            .split('$')
            .nth(2)
            .expect("Hash should have cost field");
        assert_eq!(cost_max, "14", "Hash should use cost 14 (MAX_BCRYPT_COST)");

        // Verify both credentials can still authenticate with their original secrets
        assert!(
            crypto::verify_client_secret(
                response_min.client_secret.expose_secret(),
                &credential_min.client_secret_hash
            )?,
            "Should verify with cost 10 hash"
        );
        assert!(
            crypto::verify_client_secret(
                response_max.client_secret.expose_secret(),
                &credential_max.client_secret_hash
            )?,
            "Should verify with cost 14 hash"
        );

        Ok(())
    }
}
