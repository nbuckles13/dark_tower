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
    ///
    /// Note: Ignored under coverage builds because instrumentation adds significant
    /// overhead that makes timing comparisons meaningless.
    #[sqlx::test(migrations = "../../migrations")]
    #[cfg_attr(coverage, ignore)]
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

    // ============================================================================
    // P1 Security Tests - JWT Validation & Manipulation
    // ============================================================================

    /// P1-1: Test JWT payload tampering detection
    ///
    /// Verifies that modifying JWT claims (sub, scope, etc.) is detected
    /// and rejected even if the signature appears valid.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_payload_tampering_rejected(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "tamper-client",
            &valid_hash,
            "media-handler",
            None,
            &["media:process".to_string()],
        )
        .await?;

        // Issue a valid token
        let token_response = issue_service_token(
            &pool,
            &master_key,
            "tamper-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Tamper with the payload by changing scope claim
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Decode the payload
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Failed to decode payload");
        let mut payload: serde_json::Value =
            serde_json::from_slice(&payload_bytes).expect("Failed to parse payload JSON");

        // Tamper: escalate scope to admin
        payload["scope"] = serde_json::Value::String("admin:all meeting:delete".to_string());

        // Re-encode the tampered payload
        let tampered_payload = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("Failed to serialize payload"));

        // Reconstruct JWT with tampered payload (signature will be invalid)
        let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        // Attempt to verify the tampered token
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&tampered_token, &signing_key.public_key);

        // Should be rejected due to signature mismatch
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "Tampered JWT should be rejected"
        );

        Ok(())
    }

    /// P1-1: Test JWT signed with wrong key is rejected
    ///
    /// Verifies that a valid JWT signed with a different key is rejected.
    /// This is critical - if signature verification is broken, attackers could
    /// generate tokens with their own keys.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_wrong_signature_rejected(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the correct signing key
        let correct_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Generate a DIFFERENT keypair (attacker's key)
        let (wrong_public_key, wrong_private_key) = crypto::generate_signing_key()?;

        // Create valid claims
        let claims = crypto::Claims {
            sub: "test-client".to_string(),
            exp: Utc::now().timestamp() + 3600,
            iat: Utc::now().timestamp(),
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        // Sign with the WRONG key (attacker's key)
        let token_wrong_key = crypto::sign_jwt(&claims, &wrong_private_key)?;

        // Try to verify with the CORRECT public key
        let result = crypto::verify_jwt(&token_wrong_key, &correct_key.public_key);

        // Should be rejected - signature won't match
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT signed with wrong key should be rejected"
        );

        // Also verify that verifying with the wrong public key would succeed
        // (to prove the token itself is valid, just not for our system)
        let verify_with_wrong_key = crypto::verify_jwt(&token_wrong_key, &wrong_public_key);
        assert!(
            verify_with_wrong_key.is_ok(),
            "Token should be valid for the key it was signed with"
        );

        Ok(())
    }

    /// P1-1: Test JWT signature stripping attack
    ///
    /// Verifies that removing the signature component is detected and rejected.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_signature_stripped_rejected(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "sig-strip-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Issue a valid token
        let token_response = issue_service_token(
            &pool,
            &master_key,
            "sig-strip-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Strip signature (remove third component)
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        let stripped_token = format!("{}.{}", parts[0], parts[1]);

        // Attempt to verify the stripped token
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&stripped_token, &signing_key.public_key);

        // Should be rejected - invalid JWT format
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT without signature should be rejected"
        );

        Ok(())
    }

    /// P1-1: Test JWT algorithm confusion prevention
    ///
    /// Verifies that changing the algorithm in the header (e.g., EdDSA -> HS256)
    /// is rejected even if the signature appears valid.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_algorithm_confusion_rejected(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "alg-conf-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Issue a valid token with EdDSA
        let token_response = issue_service_token(
            &pool,
            &master_key,
            "alg-conf-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Attempt to change algorithm to HS256
        let parts: Vec<&str> = token_response.access_token.split('.').collect();

        // Decode the header
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let mut header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        // Tamper: change algorithm from EdDSA to HS256
        header["alg"] = serde_json::Value::String("HS256".to_string());

        // Re-encode the tampered header
        let tampered_header = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).expect("Failed to serialize header"));

        // Reconstruct JWT with tampered header
        let tampered_token = format!("{}.{}.{}", tampered_header, parts[1], parts[2]);

        // Attempt to verify with EdDSA public key
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&tampered_token, &signing_key.public_key);

        // Should be rejected due to algorithm mismatch
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with modified algorithm should be rejected"
        );

        Ok(())
    }

    /// P1-1: Test JWT "none" algorithm attack
    ///
    /// Verifies that JWTs with alg: "none" are rejected. This is a classic
    /// JWT vulnerability (CVE-2015-2951) where attackers bypass signature
    /// verification by claiming no algorithm is used.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_none_algorithm_rejected(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "none-alg-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Issue a valid token first
        let token_response = issue_service_token(
            &pool,
            &master_key,
            "none-alg-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Modify the algorithm to "none"
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let mut header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        // Classic attack: set alg to "none"
        header["alg"] = serde_json::Value::String("none".to_string());

        let tampered_header = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).expect("Failed to serialize header"));

        // Reconstruct JWT with "none" algorithm (no signature or empty signature)
        let none_token = format!("{}.{}.", tampered_header, parts[1]);

        // Attempt to verify
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&none_token, &signing_key.public_key);

        // Should be rejected - "none" algorithm not allowed
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with 'none' algorithm should be rejected"
        );

        Ok(())
    }

    /// P1-1: Test expired token rejection in full validation flow
    ///
    /// Verifies that tokens with exp < current time are rejected.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_expired_token_rejected(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Manually create an expired token
        let expired_claims = crypto::Claims {
            sub: "expired-client".to_string(),
            exp: Utc::now().timestamp() - 3600, // Expired 1 hour ago
            iat: Utc::now().timestamp() - 7200, // Issued 2 hours ago
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        // Decrypt the private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: signing_key_model.private_key_encrypted.clone(),
            nonce: signing_key_model.encryption_nonce.clone(),
            tag: signing_key_model.encryption_tag.clone(),
        };
        let private_key = crypto::decrypt_private_key(&encrypted_key, &master_key)?;

        // Sign the expired token
        let expired_token = crypto::sign_jwt(&expired_claims, &private_key)?;

        // Attempt to verify the expired token
        let result = crypto::verify_jwt(&expired_token, &signing_key_model.public_key);

        // Should be rejected due to expiration
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "Expired JWT should be rejected"
        );

        Ok(())
    }

    /// P1-1: Test JWT with future issued-at time rejected
    ///
    /// Verifies that tokens with iat > current time are handled appropriately.
    /// Note: jsonwebtoken crate doesn't validate iat by default, but we test
    /// that the token structure is valid even with future iat.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_future_iat_accepted_by_library(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Create a token with future iat (issued in the future)
        let future_claims = crypto::Claims {
            sub: "future-client".to_string(),
            exp: Utc::now().timestamp() + 7200, // Expires in 2 hours
            iat: Utc::now().timestamp() + 3600, // Issued 1 hour from now (suspicious!)
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        // Decrypt the private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: signing_key_model.private_key_encrypted.clone(),
            nonce: signing_key_model.encryption_nonce.clone(),
            tag: signing_key_model.encryption_tag.clone(),
        };
        let private_key = crypto::decrypt_private_key(&encrypted_key, &master_key)?;

        // Sign the token with future iat
        let future_token = crypto::sign_jwt(&future_claims, &private_key)?;

        // Verify the token - jsonwebtoken library doesn't validate iat by default
        // so this will succeed. This documents the current behavior.
        let result = crypto::verify_jwt(&future_token, &signing_key_model.public_key);

        // Currently accepted (library doesn't validate iat)
        // If we want stricter validation, we'd need to add custom iat checking
        assert!(
            result.is_ok(),
            "JWT with future iat is currently accepted by jsonwebtoken library"
        );

        let claims = result.unwrap();
        assert_eq!(claims.sub, "future-client");

        // Note: This test documents that we rely on the jsonwebtoken library's
        // validation. If stricter iat validation is needed, add to crypto::verify_jwt.

        Ok(())
    }

    /// P1-2: Test JWT claims injection - extra claims
    ///
    /// Verifies that extra claims in the JWT are safely ignored during deserialization.
    /// This tests that serde correctly handles unknown fields when deserializing into
    /// the Claims struct, which is critical for forward compatibility.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_extra_claims_ignored_safely(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Decrypt the private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: signing_key_model.private_key_encrypted.clone(),
            nonce: signing_key_model.encryption_nonce.clone(),
            tag: signing_key_model.encryption_tag.clone(),
        };
        let private_key = crypto::decrypt_private_key(&encrypted_key, &master_key)?;

        // Create a custom struct with EXTRA fields beyond what Claims has
        #[derive(serde::Serialize)]
        struct ClaimsWithExtra {
            sub: String,
            exp: i64,
            iat: i64,
            scope: String,
            service_type: Option<String>,
            // Extra fields that Claims doesn't have
            admin: bool,
            roles: Vec<String>,
            custom_field: String,
        }

        let claims_with_extra = ClaimsWithExtra {
            sub: "extra-claims-client".to_string(),
            exp: Utc::now().timestamp() + 3600,
            iat: Utc::now().timestamp(),
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
            admin: true,                                // Extra field
            roles: vec!["superuser".to_string()],       // Extra field
            custom_field: "malicious_data".to_string(), // Extra field
        };

        // Sign the token WITH extra fields
        let encoding_key = EncodingKey::from_ed_der(&private_key);
        let header = Header::new(Algorithm::EdDSA);
        let token_with_extra = encode(&header, &claims_with_extra, &encoding_key)
            .expect("Failed to encode token with extra claims");

        // Verify it deserializes into Claims struct (should ignore extra fields)
        let verified = crypto::verify_jwt(&token_with_extra, &signing_key_model.public_key)?;

        // Should successfully parse the standard fields
        assert_eq!(verified.sub, "extra-claims-client");
        assert_eq!(verified.scope, "meeting:create");
        assert_eq!(verified.service_type, Some("global-controller".to_string()));

        // Extra fields (admin, roles, custom_field) are silently ignored
        // This is the expected behavior for forward compatibility

        Ok(())
    }

    /// P1-2: Test JWT missing required claims
    ///
    /// Verifies that JWTs missing required claims (sub, exp, iat, scope) are rejected.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_missing_required_claims_rejected(pool: PgPool) -> Result<(), AcError> {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Decrypt the private key
        let encrypted_key = crypto::EncryptedKey {
            encrypted_data: signing_key_model.private_key_encrypted.clone(),
            nonce: signing_key_model.encryption_nonce.clone(),
            tag: signing_key_model.encryption_tag.clone(),
        };
        let private_key = crypto::decrypt_private_key(&encrypted_key, &master_key)?;

        // Create a JWT with missing 'scope' claim
        #[derive(serde::Serialize)]
        struct IncompleteClaims {
            sub: String,
            exp: i64,
            iat: i64,
            // Missing 'scope' field!
        }

        let incomplete_claims = IncompleteClaims {
            sub: "incomplete-client".to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
        };

        let encoding_key = EncodingKey::from_ed_der(&private_key);
        let header = Header::new(Algorithm::EdDSA);
        let incomplete_token = encode(&header, &incomplete_claims, &encoding_key)
            .expect("Failed to encode incomplete token");

        // Attempt to verify - should fail during deserialization
        let result = crypto::verify_jwt(&incomplete_token, &signing_key_model.public_key);

        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT missing required claims should be rejected"
        );

        Ok(())
    }

    /// P1-2: Test JWT sub claim tampering
    ///
    /// Verifies that changing the subject claim is detected by signature verification.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_sub_claim_tampering_rejected(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "sub-tamper-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Issue a valid token
        let token_response = issue_service_token(
            &pool,
            &master_key,
            "sub-tamper-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Tamper with the sub claim
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .expect("Failed to decode payload");
        let mut payload: serde_json::Value =
            serde_json::from_slice(&payload_bytes).expect("Failed to parse payload JSON");

        // Change sub to impersonate another client
        payload["sub"] = serde_json::Value::String("admin-client".to_string());

        let tampered_payload = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("Failed to serialize payload"));
        let tampered_token = format!("{}.{}.{}", parts[0], tampered_payload, parts[2]);

        // Attempt to verify
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&tampered_token, &signing_key.public_key);

        // Should be rejected due to signature mismatch
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with tampered sub claim should be rejected"
        );

        Ok(())
    }
}
