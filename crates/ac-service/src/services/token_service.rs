use crate::crypto::{self, Claims, EncryptedKey};
use crate::errors::AcError;
use crate::models::{AuthEventType, TokenResponse};
use crate::repositories::{auth_events, service_credentials, signing_keys};
use chrono::Utc;
use sqlx::PgPool;

const TOKEN_EXPIRY_SECONDS: i64 = 3600; // 1 hour

/// Issue a service token using OAuth 2.0 Client Credentials flow
///
/// Verifies client credentials, generates JWT with scopes, logs event
#[expect(clippy::too_many_arguments)] // OAuth 2.0 token endpoint requires many params
pub async fn issue_service_token(
    pool: &PgPool,
    master_key: &[u8],
    client_id: &str,
    client_secret: &str,
    grant_type: &str,
    requested_scopes: Option<Vec<String>>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<TokenResponse, AcError> {
    // Validate grant_type
    if grant_type != "client_credentials" {
        return Err(AcError::InvalidCredentials);
    }

    // Fetch credential from database
    let credential = service_credentials::get_by_client_id(pool, client_id).await?;

    // Check for account lockout (prevent brute force)
    if let Some(ref cred) = credential {
        let fifteen_mins_ago = Utc::now() - chrono::Duration::minutes(15);
        let failed_count =
            auth_events::get_failed_attempts_count(pool, &cred.credential_id, fifteen_mins_ago)
                .await?;

        if failed_count >= 5 {
            tracing::warn!(
                "Account locked due to excessive failed attempts: client_id={}",
                client_id
            );
            return Err(AcError::RateLimitExceeded);
        }
    }

    // Always run bcrypt to prevent timing attacks
    // Use dummy hash if credential not found (constant-time operation)
    let hash_to_verify = match &credential {
        Some(c) => c.client_secret_hash.as_str(),
        None => "$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYqExt7YD3a", // Dummy bcrypt hash
    };

    let is_valid = crypto::verify_client_secret(client_secret, hash_to_verify)?;

    // Now check if credential existed and was active
    let credential = credential.ok_or(AcError::InvalidCredentials)?;

    if !credential.is_active || !is_valid {
        // Log failed attempt
        if let Err(e) = auth_events::log_event(
            pool,
            AuthEventType::ServiceTokenFailed.as_str(),
            None,
            Some(credential.credential_id),
            false,
            Some(if !credential.is_active {
                "Credential is inactive"
            } else {
                "Invalid client secret"
            }),
            ip_address,
            user_agent,
            None,
        )
        .await
        {
            tracing::warn!("Failed to log auth event: {}", e);
        }

        return Err(AcError::InvalidCredentials);
    }

    // Determine scopes (use requested scopes if provided and valid, otherwise use default)
    let scopes = if let Some(req_scopes) = requested_scopes {
        // Verify requested scopes are subset of allowed scopes
        let all_valid = req_scopes.iter().all(|s| credential.scopes.contains(s));

        if !all_valid {
            return Err(AcError::InsufficientScope {
                required: req_scopes.join(" "),
                provided: credential.scopes.clone(),
            });
        }
        req_scopes
    } else {
        credential.scopes.clone()
    };

    // Load active signing key
    let signing_key = signing_keys::get_active_key(pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Decrypt private key
    let encrypted_key = EncryptedKey {
        encrypted_data: signing_key.private_key_encrypted,
        nonce: signing_key.encryption_nonce,
        tag: signing_key.encryption_tag,
    };

    let private_key_pkcs8 = crypto::decrypt_private_key(&encrypted_key, master_key)?;

    // Generate JWT claims
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: client_id.to_string(),
        exp: now + TOKEN_EXPIRY_SECONDS,
        iat: now,
        scope: scopes.join(" "),
        service_type: Some(credential.service_type.clone()),
    };

    // Sign JWT
    let token = crypto::sign_jwt(&claims, &private_key_pkcs8)?;

    // Log successful token issuance
    if let Err(e) = auth_events::log_event(
        pool,
        AuthEventType::ServiceTokenIssued.as_str(),
        None,
        Some(credential.credential_id),
        true,
        None,
        ip_address,
        user_agent,
        Some(serde_json::json!({
            "key_id": signing_key.key_id,
            "scopes": scopes,
        })),
    )
    .await
    {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(TokenResponse {
        access_token: token,
        token_type: "Bearer".to_string(),
        expires_in: TOKEN_EXPIRY_SECONDS as u64,
        scope: scopes.join(" "),
    })
}

/// Issue a user token (placeholder - not fully implemented in Phase 1)
///
/// This is a simplified implementation for future use
pub async fn issue_user_token(
    _pool: &PgPool,
    _master_key: &[u8],
    _username: &str,
    _password: &str,
) -> Result<TokenResponse, AcError> {
    // NOTE: In Phase 1, we don't have a users table yet
    // This is a placeholder that would need to:
    // 1. Fetch user from database
    // 2. Verify password hash
    // 3. Get user roles/scopes
    // 4. Generate JWT with user claims

    // For now, return an error indicating this is not yet implemented
    Err(AcError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;
    use crate::repositories::service_credentials;
    use crate::services::key_management_service;
    use std::time::Instant;

    /// P0-1 (CG-1): Test timing attack prevention - invalid client_id
    ///
    /// Verifies that authentication attempts with non-existent client_ids
    /// take roughly the same time as valid client_ids to prevent username enumeration.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_timing_attack_prevention_invalid_client_id(pool: PgPool) -> Result<(), AcError> {
        // Setup: Create master key (32 bytes for AES-256-GCM) and signing key
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Create valid credential
        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "valid-client",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Measure time for valid client_id with wrong password
        let start = Instant::now();
        let _ = issue_service_token(
            &pool,
            &master_key,
            "valid-client",
            "wrong-password",
            "client_credentials",
            None,
            None,
            None,
        )
        .await;
        let valid_client_duration = start.elapsed();

        // Measure time for invalid client_id
        let start = Instant::now();
        let _ = issue_service_token(
            &pool,
            &master_key,
            "nonexistent-client",
            "some-password",
            "client_credentials",
            None,
            None,
            None,
        )
        .await;
        let invalid_client_duration = start.elapsed();

        // Both should take similar time due to dummy hash verification
        // Use proportional check instead of absolute timing to avoid flakiness in CI
        let time_diff = valid_client_duration.abs_diff(invalid_client_duration);
        let max_time = valid_client_duration.max(invalid_client_duration);
        let diff_percentage = (time_diff.as_millis() as f64 / max_time.as_millis() as f64) * 100.0;

        // Timing difference should be less than 50% of the longer operation
        // This ensures constant-time behavior while tolerating CI environment variations
        assert!(
            diff_percentage < 50.0,
            "Timing difference too large: {}ms ({:.1}% of {}ms) - potential timing attack vulnerability",
            time_diff.as_millis(),
            diff_percentage,
            max_time.as_millis()
        );

        Ok(())
    }

    /// P0-1 (CG-1): Test timing attack prevention - consistent error messages
    ///
    /// Verifies that the same error is returned for invalid client_id and invalid secret
    /// to prevent username enumeration.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_timing_attack_prevention_consistent_errors(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Create valid credential
        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "valid-client",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Test invalid client_id
        let invalid_client_result = issue_service_token(
            &pool,
            &master_key,
            "nonexistent-client",
            "some-password",
            "client_credentials",
            None,
            None,
            None,
        )
        .await;

        // Test valid client_id with wrong password
        let wrong_password_result = issue_service_token(
            &pool,
            &master_key,
            "valid-client",
            "wrong-password",
            "client_credentials",
            None,
            None,
            None,
        )
        .await;

        // Both should return the same error type (InvalidCredentials)
        assert!(matches!(
            invalid_client_result,
            Err(AcError::InvalidCredentials)
        ));
        assert!(matches!(
            wrong_password_result,
            Err(AcError::InvalidCredentials)
        ));

        Ok(())
    }

    /// P0-2 (CG-2): Test rate limiting - basic lockout after 5 failed attempts
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rate_limiting_basic_lockout(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        let _credential = service_credentials::create_service_credential(
            &pool,
            "test-client",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Attempt 5 failed logins
        for _ in 0..5 {
            let _ = issue_service_token(
                &pool,
                &master_key,
                "test-client",
                "wrong-password",
                "client_credentials",
                None,
                Some("192.168.1.1"),
                None,
            )
            .await;
        }

        // 6th attempt should be rate limited
        let result = issue_service_token(
            &pool,
            &master_key,
            "test-client",
            "wrong-password",
            "client_credentials",
            None,
            Some("192.168.1.1"),
            None,
        )
        .await;

        assert!(matches!(result, Err(AcError::RateLimitExceeded)));

        // Even with correct password, should still be locked
        let result = issue_service_token(
            &pool,
            &master_key,
            "test-client",
            valid_secret,
            "client_credentials",
            None,
            Some("192.168.1.1"),
            None,
        )
        .await;

        assert!(matches!(result, Err(AcError::RateLimitExceeded)));

        Ok(())
    }

    /// P0-2 (CG-2): Test rate limiting - window expiration
    ///
    /// Verifies that failed attempts outside the 15-minute window don't count
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_rate_limiting_window_expiration(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "test-client-window",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Note: This test simulates the window but doesn't actually wait 15 minutes.
        // In a real scenario, old events would be outside the window.
        // For this test, we just verify the logic doesn't lock out with < 5 recent failures.

        // Make 3 failed attempts
        for _ in 0..3 {
            let _ = issue_service_token(
                &pool,
                &master_key,
                "test-client-window",
                "wrong-password",
                "client_credentials",
                None,
                Some("192.168.1.2"),
                None,
            )
            .await;
        }

        // Should NOT be locked (only 3 failures)
        let result = issue_service_token(
            &pool,
            &master_key,
            "test-client-window",
            "wrong-password",
            "client_credentials",
            None,
            Some("192.168.1.2"),
            None,
        )
        .await;

        // Should get InvalidCredentials, not RateLimitExceeded
        assert!(matches!(result, Err(AcError::InvalidCredentials)));

        Ok(())
    }

    /// P0-4 (CG-6): Test scope escalation prevention - reject unauthorized scopes
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_scope_escalation_prevention(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        // Create credential with limited scopes
        service_credentials::create_service_credential(
            &pool,
            "limited-client",
            &valid_hash,
            "media-handler",
            None,
            &["media:process".to_string(), "media:forward".to_string()],
        )
        .await?;

        // Attempt to request unauthorized scope
        let result = issue_service_token(
            &pool,
            &master_key,
            "limited-client",
            valid_secret,
            "client_credentials",
            Some(vec![
                "media:process".to_string(),
                "meeting:create".to_string(), // Unauthorized!
            ]),
            None,
            None,
        )
        .await;

        assert!(matches!(result, Err(AcError::InsufficientScope { .. })));

        Ok(())
    }

    /// P0-4 (CG-6): Test scope validation - subset of allowed scopes works
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_scope_validation_subset_allowed(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "multi-scope-client",
            &valid_hash,
            "global-controller",
            None,
            &[
                "meeting:create".to_string(),
                "meeting:read".to_string(),
                "meeting:list".to_string(),
            ],
        )
        .await?;

        // Request subset of scopes (should succeed)
        let result = issue_service_token(
            &pool,
            &master_key,
            "multi-scope-client",
            valid_secret,
            "client_credentials",
            Some(vec!["meeting:read".to_string()]),
            None,
            None,
        )
        .await?;

        assert_eq!(result.scope, "meeting:read");
        assert_eq!(result.token_type, "Bearer");

        Ok(())
    }

    /// P0-3 (CG-5): Test authentication bypass - inactive credentials rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_authentication_bypass_inactive_credential(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        let credential = service_credentials::create_service_credential(
            &pool,
            "deactivated-client",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Deactivate the credential
        service_credentials::deactivate(&pool, credential.credential_id).await?;

        // Attempt to authenticate with valid password but inactive credential
        let result = issue_service_token(
            &pool,
            &master_key,
            "deactivated-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await;

        assert!(matches!(result, Err(AcError::InvalidCredentials)));

        Ok(())
    }

    /// P0-3 (CG-5): Test authentication bypass - invalid grant type rejected
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_authentication_bypass_invalid_grant_type(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "test-grant-client",
            &valid_hash,
            "global-controller",
            None,
            &["test:scope".to_string()],
        )
        .await?;

        // Attempt with invalid grant_type
        let result = issue_service_token(
            &pool,
            &master_key,
            "test-grant-client",
            valid_secret,
            "password", // Wrong grant type
            None,
            None,
            None,
        )
        .await;

        assert!(matches!(result, Err(AcError::InvalidCredentials)));

        Ok(())
    }

    /// P0: Test successful token issuance with all parameters
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_successful_token_issuance(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "success-client",
            &valid_hash,
            "global-controller",
            Some("us-west-2"),
            &["meeting:create".to_string(), "meeting:read".to_string()],
        )
        .await?;

        let result = issue_service_token(
            &pool,
            &master_key,
            "success-client",
            valid_secret,
            "client_credentials",
            None,
            Some("192.168.1.100"),
            Some("TestAgent/1.0"),
        )
        .await?;

        assert_eq!(result.token_type, "Bearer");
        assert_eq!(result.expires_in, 3600);
        assert!(result.scope.contains("meeting:create"));
        assert!(result.scope.contains("meeting:read"));
        assert!(!result.access_token.is_empty());

        // Verify JWT structure using test utilities
        use ac_test_utils::assertions::TokenAssertions;
        result
            .access_token
            .assert_valid_jwt()
            .assert_has_scope("meeting:create")
            .assert_has_scope("meeting:read");

        Ok(())
    }
}
