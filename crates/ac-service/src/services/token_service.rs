use crate::crypto::{self, Claims, EncryptedKey};
use crate::errors::AcError;
use crate::models::{AuthEventType, TokenResponse};
use crate::repositories::{auth_events, service_credentials, signing_keys};
use chrono::Utc;
use sqlx::PgPool;

// Token configuration
const TOKEN_EXPIRY_SECONDS: u64 = 3600; // 1 hour
const TOKEN_EXPIRY_SECONDS_I64: i64 = 3600; // 1 hour (for timestamp calculations)

// Rate limiting configuration (per ADR-0003)
const RATE_LIMIT_WINDOW_MINUTES: i64 = 15; // 15-minute sliding window
const RATE_LIMIT_MAX_ATTEMPTS: i64 = 5; // Maximum failed attempts before lockout

// Security test configuration
#[cfg(test)]
const MAX_TIMING_VARIANCE_PERCENT: f64 = 30.0; // Timing attack tolerance threshold

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
        let rate_limit_window_ago =
            Utc::now() - chrono::Duration::minutes(RATE_LIMIT_WINDOW_MINUTES);
        let failed_count = auth_events::get_failed_attempts_count(
            pool,
            &cred.credential_id,
            rate_limit_window_ago,
        )
        .await?;

        if failed_count >= RATE_LIMIT_MAX_ATTEMPTS {
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
        exp: now + TOKEN_EXPIRY_SECONDS_I64,
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
        expires_in: TOKEN_EXPIRY_SECONDS,
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

        // Timing difference should be less than 30% of the longer operation
        // Tightened from 50% based on security review to reduce timing attack surface.
        // bcrypt operations have inherent variance, but 30% tolerance is acceptable
        // while still catching timing vulnerabilities.

        assert!(
            diff_percentage < MAX_TIMING_VARIANCE_PERCENT,
            "Timing difference too large: {}ms ({:.1}% of {}ms) - potential timing attack vulnerability.\n  \
             Valid client: {:?}\n  \
             Invalid client: {:?}\n  \
             Max allowed variance: {:.1}%",
            time_diff.as_millis(),
            diff_percentage,
            max_time.as_millis(),
            valid_client_duration,
            invalid_client_duration,
            MAX_TIMING_VARIANCE_PERCENT
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

        // Attempt RATE_LIMIT_MAX_ATTEMPTS failed logins
        for _ in 0..RATE_LIMIT_MAX_ATTEMPTS {
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
        assert_eq!(result.expires_in, TOKEN_EXPIRY_SECONDS);
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
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
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
            exp: Utc::now().timestamp() - TOKEN_EXPIRY_SECONDS_I64, // Expired 1 hour ago
            iat: Utc::now().timestamp() - (TOKEN_EXPIRY_SECONDS_I64 * 2), // Issued 2 hours ago
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

    /// P1-SECURITY: Test JWT with far-future iat is rejected (beyond clock skew)
    ///
    /// Verifies that tokens with iat more than 5 minutes (JWT_CLOCK_SKEW_SECONDS)
    /// in the future are rejected. This prevents token pre-generation attacks
    /// and detects compromised systems with incorrect clocks.
    ///
    /// This test validates the custom iat validation implemented in crypto::verify_jwt()
    /// which supplements the jsonwebtoken library's standard validation.
    ///
    /// Security rationale:
    /// - Prevents attackers from pre-generating tokens for future use
    /// - Detects systems with severely incorrect clocks (potential compromise)
    /// - Allows reasonable clock skew (±5 minutes) for distributed systems
    ///
    /// Per NIST SP 800-63B: Clock synchronization should be maintained within
    /// reasonable bounds (typically 5 minutes) for time-based security controls.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_future_iat_beyond_clock_skew_rejected(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Create a token with future iat (issued in the future, beyond clock skew)
        let future_claims = crypto::Claims {
            sub: "future-client".to_string(),
            exp: Utc::now().timestamp() + (TOKEN_EXPIRY_SECONDS_I64 * 2), // Expires in 2 hours
            iat: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64, // Issued 1 hour from now (way beyond 5 min skew!)
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

        // Verify the token - should be REJECTED due to custom iat validation
        let result = crypto::verify_jwt(&future_token, &signing_key_model.public_key);

        // Should be rejected - iat too far in the future
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with iat 1 hour in future should be rejected (beyond clock skew tolerance)"
        );

        Ok(())
    }

    /// P1-SECURITY: Test JWT with future iat within clock skew is accepted
    ///
    /// Verifies that tokens with iat slightly in the future (2 minutes) are accepted,
    /// allowing for reasonable clock drift between distributed servers.
    ///
    /// The 5-minute clock skew tolerance (JWT_CLOCK_SKEW_SECONDS) balances security
    /// with operational reliability for systems that may have minor time synchronization
    /// differences.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_future_iat_within_clock_skew_accepted(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Create a token with iat 2 minutes in the future (within 5 min clock skew)
        let claims_within_skew = crypto::Claims {
            sub: "clock-skew-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64, // Expires in 1 hour
            iat: Utc::now().timestamp() + 120, // Issued 2 minutes from now (within tolerance)
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

        // Sign the token
        let token = crypto::sign_jwt(&claims_within_skew, &private_key)?;

        // Verify the token - should be accepted (within clock skew)
        let result = crypto::verify_jwt(&token, &signing_key_model.public_key);

        assert!(
            result.is_ok(),
            "JWT with iat 2 minutes in future should be accepted (within clock skew tolerance)"
        );

        let verified_claims = result.unwrap();
        assert_eq!(verified_claims.sub, "clock-skew-client");
        assert_eq!(verified_claims.scope, "meeting:create");

        Ok(())
    }

    /// P1-SECURITY: Test JWT iat at exact clock skew boundary is accepted
    ///
    /// Verifies that tokens with iat exactly at the 5-minute boundary are accepted.
    /// This is a boundary condition test to ensure the validation uses <= not <.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_iat_at_clock_skew_boundary_accepted(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        const JWT_CLOCK_SKEW_SECONDS: i64 = 300; // 5 minutes (same as crypto module constant)

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Create a token with iat exactly at the clock skew boundary (5 minutes)
        let claims_at_boundary = crypto::Claims {
            sub: "boundary-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp() + JWT_CLOCK_SKEW_SECONDS, // Exactly at 5 min boundary
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

        // Sign the token
        let token = crypto::sign_jwt(&claims_at_boundary, &private_key)?;

        // Verify the token - should be accepted (exactly at boundary)
        let result = crypto::verify_jwt(&token, &signing_key_model.public_key);

        assert!(
            result.is_ok(),
            "JWT with iat exactly at 5 minute boundary should be accepted"
        );

        let verified_claims = result.unwrap();
        assert_eq!(verified_claims.sub, "boundary-client");

        Ok(())
    }

    /// P1-SECURITY: Test JWT iat just beyond clock skew boundary is rejected
    ///
    /// Verifies that tokens with iat just 1 second beyond the 5-minute boundary
    /// are rejected. This ensures the boundary validation is strict.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_iat_just_beyond_clock_skew_rejected(pool: PgPool) -> Result<(), AcError> {
        use chrono::Utc;

        const JWT_CLOCK_SKEW_SECONDS: i64 = 300; // 5 minutes (same as crypto module constant)

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get the signing key
        let signing_key_model = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Create a token with iat 1 second beyond the clock skew boundary
        let claims_beyond_boundary = crypto::Claims {
            sub: "beyond-boundary-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp() + JWT_CLOCK_SKEW_SECONDS + 1, // 1 second beyond 5 min
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

        // Sign the token
        let token = crypto::sign_jwt(&claims_beyond_boundary, &private_key)?;

        // Verify the token - should be rejected (beyond boundary)
        let result = crypto::verify_jwt(&token, &signing_key_model.public_key);

        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with iat 1 second beyond 5 minute boundary should be rejected"
        );

        Ok(())
    }

    // ============================================================================
    // P1 Security Tests - JWT Header Injection
    // ============================================================================

    /// P1-SECURITY: Test JWT header typ claim tampering
    ///
    /// Verifies that tokens with various `typ` header values are handled correctly.
    /// Per RFC 7519 Section 5.1, the `typ` (type) header is OPTIONAL and is used
    /// by JWT applications to declare the media type of the complete JWT.
    ///
    /// Common values:
    /// - "JWT" (uppercase) - Traditional JWT type
    /// - "at+jwt" - RFC 9068 OAuth 2.0 Access Token type
    /// - null/missing - Valid per RFC 7519 (typ is optional)
    ///
    /// Security rationale:
    /// The `typ` header is NOT security-critical. It's a hint for parsers and
    /// is not part of the signature verification or claims validation.
    /// We verify that our implementation correctly:
    /// 1. Accepts tokens regardless of typ value (since it's optional)
    /// 2. Does not use typ for security decisions
    /// 3. Validates tokens based on signature and claims, not header metadata
    ///
    /// This test documents expected behavior and prevents regressions if
    /// we add typ validation in the future.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_header_typ_tampering(pool: PgPool) -> Result<(), AcError> {
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

        // Create standard claims
        let claims = crypto::Claims {
            sub: "typ-test-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp(),
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let encoding_key = EncodingKey::from_ed_der(&private_key);

        // Test 1: typ = "at+jwt" (RFC 9068 access token type)
        let mut header_at_jwt = Header::new(Algorithm::EdDSA);
        header_at_jwt.typ = Some("at+jwt".to_string());
        let token_at_jwt = encode(&header_at_jwt, &claims, &encoding_key)
            .expect("Failed to encode token with typ=at+jwt");

        let result = crypto::verify_jwt(&token_at_jwt, &signing_key_model.public_key);
        assert!(
            result.is_ok(),
            "JWT with typ='at+jwt' should be accepted (RFC 9068 standard)"
        );

        // Test 2: typ = "jwt" (lowercase)
        let mut header_lowercase = Header::new(Algorithm::EdDSA);
        header_lowercase.typ = Some("jwt".to_string());
        let token_lowercase = encode(&header_lowercase, &claims, &encoding_key)
            .expect("Failed to encode token with typ=jwt");

        let result = crypto::verify_jwt(&token_lowercase, &signing_key_model.public_key);
        assert!(
            result.is_ok(),
            "JWT with typ='jwt' (lowercase) should be accepted"
        );

        // Test 3: typ = "CUSTOM" (arbitrary value)
        let mut header_custom = Header::new(Algorithm::EdDSA);
        header_custom.typ = Some("CUSTOM".to_string());
        let token_custom = encode(&header_custom, &claims, &encoding_key)
            .expect("Failed to encode token with typ=CUSTOM");

        let result = crypto::verify_jwt(&token_custom, &signing_key_model.public_key);
        assert!(
            result.is_ok(),
            "JWT with typ='CUSTOM' should be accepted (typ is optional, any value allowed)"
        );

        // Test 4: typ = null/missing (manually construct header without typ)
        // The default Header::new() sets typ to Some("JWT"), so we need to manually clear it
        let mut header_no_typ = Header::new(Algorithm::EdDSA);
        header_no_typ.typ = None;
        let token_no_typ = encode(&header_no_typ, &claims, &encoding_key)
            .expect("Failed to encode token without typ");

        let result = crypto::verify_jwt(&token_no_typ, &signing_key_model.public_key);
        assert!(
            result.is_ok(),
            "JWT without typ header should be accepted (typ is optional per RFC 7519)"
        );

        // Verify the verified claims are correct regardless of typ
        let verified_claims = result.unwrap();
        assert_eq!(verified_claims.sub, "typ-test-client");
        assert_eq!(verified_claims.scope, "meeting:create");

        Ok(())
    }

    /// P1-SECURITY: Test JWT header alg mismatch is rejected
    ///
    /// Verifies that tampering with the `alg` header (e.g., changing EdDSA to HS256)
    /// is rejected. This tests defense against algorithm confusion attacks.
    ///
    /// Algorithm confusion (CVE-2015-2951, CVE-2016-5431) occurs when:
    /// 1. Attacker changes alg from asymmetric (EdDSA/RS256) to symmetric (HS256)
    /// 2. Server's public key is used as HMAC secret
    /// 3. Attacker signs token with HMAC-SHA256 using the public key
    /// 4. Server incorrectly validates with HS256 instead of EdDSA
    ///
    /// Our defense:
    /// - jsonwebtoken library enforces algorithm on decoding
    /// - crypto::verify_jwt() specifies EdDSA algorithm explicitly
    /// - Signature verification will fail if algorithm doesn't match
    ///
    /// Security impact: CRITICAL - prevents complete authentication bypass
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_header_alg_mismatch_rejected(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;

        service_credentials::create_service_credential(
            &pool,
            "alg-mismatch-client",
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
            "alg-mismatch-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Tamper with the algorithm header: EdDSA -> HS256
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts");

        // Decode the header
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let mut header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        // Verify original algorithm is EdDSA
        assert_eq!(
            header["alg"].as_str().unwrap(),
            "EdDSA",
            "Original token should use EdDSA algorithm"
        );

        // Tamper: change algorithm from EdDSA to HS256
        header["alg"] = serde_json::Value::String("HS256".to_string());

        // Re-encode the tampered header
        let tampered_header = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).expect("Failed to serialize header"));

        // Reconstruct JWT with tampered header (same payload and signature)
        let tampered_token = format!("{}.{}.{}", tampered_header, parts[1], parts[2]);

        // Attempt to verify with EdDSA public key
        let signing_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");
        let result = crypto::verify_jwt(&tampered_token, &signing_key.public_key);

        // Should be rejected - algorithm mismatch
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with tampered algorithm header (EdDSA->HS256) should be rejected"
        );

        // Also test other algorithm confusions
        header["alg"] = serde_json::Value::String("RS256".to_string());
        let tampered_header_rs256 = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).expect("Failed to serialize header"));
        let tampered_token_rs256 = format!("{}.{}.{}", tampered_header_rs256, parts[1], parts[2]);

        let result = crypto::verify_jwt(&tampered_token_rs256, &signing_key.public_key);
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with tampered algorithm header (EdDSA->RS256) should be rejected"
        );

        Ok(())
    }

    /// P1-SECURITY: Test JWT header kid injection
    ///
    /// Verifies that a token with a manipulated `kid` (key ID) header pointing to
    /// an "attacker-controlled" key ID is still validated against our actual public key,
    /// not some attacker's key.
    ///
    /// The `kid` (Key ID) header parameter (RFC 7515 Section 4.1.4) is a hint
    /// indicating which key was used to sign the JWT. It's commonly used in:
    /// - Multi-key environments (key rotation)
    /// - JWKS (JSON Web Key Set) endpoints
    ///
    /// Attack scenario:
    /// 1. Attacker generates their own keypair
    /// 2. Attacker creates JWT with kid="attacker-key-123"
    /// 3. Attacker signs JWT with their private key
    /// 4. Vulnerable server fetches key from JWKS using kid
    /// 5. Server validates with attacker's public key -> success!
    ///
    /// Our defense:
    /// - We do NOT use kid to fetch keys dynamically
    /// - crypto::verify_jwt() requires explicit public_key parameter
    /// - Verification always uses our trusted signing key, ignoring kid
    /// - kid is informational only, not used for security decisions
    ///
    /// Security impact: CRITICAL - prevents authentication bypass via key substitution
    ///
    /// Note: When we implement key rotation with get_by_key_id(), we must ensure:
    /// - Only keys from our database are trusted
    /// - kid cannot reference external/attacker-controlled keys
    /// - Whitelist approach: validate kid against known key IDs before lookup
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_header_kid_injection(pool: PgPool) -> Result<(), AcError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use chrono::Utc;
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Get our legitimate signing key
        let legitimate_key = signing_keys::get_active_key(&pool)
            .await?
            .expect("No active signing key found");

        // Generate attacker's keypair
        let (attacker_public_key, attacker_private_key) = crypto::generate_signing_key()?;

        // Create claims
        let claims = crypto::Claims {
            sub: "kid-injection-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp(),
            scope: "admin:all meeting:delete".to_string(), // Escalated privileges!
            service_type: Some("global-controller".to_string()),
        };

        // Create header with attacker's kid
        let mut attacker_header = Header::new(Algorithm::EdDSA);
        attacker_header.kid = Some("attacker-controlled-key-12345".to_string());

        // Sign with attacker's private key
        let attacker_encoding_key = EncodingKey::from_ed_der(&attacker_private_key);
        let attacker_token = encode(&attacker_header, &claims, &attacker_encoding_key)
            .expect("Failed to encode attacker token");

        // Attempt to verify with OUR legitimate public key (not attacker's)
        // This simulates the secure behavior: we ignore kid and use our trusted key
        let result = crypto::verify_jwt(&attacker_token, &legitimate_key.public_key);

        // Should be REJECTED - signature won't match our public key
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT signed with attacker's key should be rejected even with spoofed kid header"
        );

        // Verify that the token WOULD be valid if verified with attacker's key
        // (to prove the token itself is well-formed, just signed by wrong key)
        let attacker_verification = crypto::verify_jwt(&attacker_token, &attacker_public_key);
        assert!(
            attacker_verification.is_ok(),
            "Token should be valid when verified with the key it was signed with (attacker's key)"
        );

        // Test 2: Legitimate token with manipulated kid header
        // Issue a real token
        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "kid-test-client",
            &valid_hash,
            "media-handler",
            None,
            &["media:process".to_string()],
        )
        .await?;

        let token_response = issue_service_token(
            &pool,
            &master_key,
            "kid-test-client",
            valid_secret,
            "client_credentials",
            None,
            None,
            None,
        )
        .await?;

        // Manually add/modify kid header
        let parts: Vec<&str> = token_response.access_token.split('.').collect();
        let header_bytes = URL_SAFE_NO_PAD
            .decode(parts[0])
            .expect("Failed to decode header");
        let mut header: serde_json::Value =
            serde_json::from_slice(&header_bytes).expect("Failed to parse header JSON");

        // Inject malicious kid
        header["kid"] = serde_json::Value::String("../../../etc/passwd".to_string());

        let tampered_header = URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).expect("Failed to serialize header"));
        let tampered_token = format!("{}.{}.{}", tampered_header, parts[1], parts[2]);

        // Verify still succeeds because kid is ignored (only signature matters)
        let result = crypto::verify_jwt(&tampered_token, &legitimate_key.public_key);

        // Token should be REJECTED because changing the header invalidates the signature
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "JWT with tampered kid header should be rejected due to signature mismatch"
        );

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
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
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
            exp: chrono::Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
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

    // ============================================================================
    // P1 Security Tests - JWT Size Limit (DoS Prevention)
    // ============================================================================

    /// P1-SECURITY: Test JWT size limit enforcement
    ///
    /// Verifies that oversized JWTs are rejected before any parsing or signature
    /// verification to prevent Denial-of-Service (DoS) attacks via resource exhaustion.
    ///
    /// Attack scenario:
    /// - Attacker sends extremely large JWT (e.g., 10MB) to /token/verify endpoint
    /// - Without size limit: System allocates large buffers, wastes CPU on base64 decode
    ///   and signature verification, potentially exhausting memory/CPU resources
    /// - With size limit: Token rejected immediately with minimal resource usage
    ///
    /// Defense-in-depth rationale:
    /// - Size check happens BEFORE base64 decode (prevents allocation attacks)
    /// - Size check happens BEFORE signature verification (prevents CPU exhaustion)
    /// - 4KB limit is generous (typical JWTs are 200-500 bytes) but prevents abuse
    ///
    /// Security impact: HIGH - prevents resource exhaustion DoS attacks
    ///
    /// Per OWASP API Security Top 10 - API4:2023 (Unrestricted Resource Consumption)
    /// Per CWE-400 (Uncontrolled Resource Consumption)
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_jwt_oversized_token_rejected(pool: PgPool) -> Result<(), AcError> {
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

        // Create a JWT with an EXTREMELY large payload (10KB of extra claims data)
        // This simulates an attacker trying to cause resource exhaustion
        #[derive(serde::Serialize)]
        struct OversizedClaims {
            sub: String,
            exp: i64,
            iat: i64,
            scope: String,
            service_type: Option<String>,
            // Add a massive payload to exceed 4KB limit
            bloat: String,
        }

        // Create 10KB of junk data (far exceeding the 4KB limit)
        // This will cause the final JWT to be much larger than MAX_JWT_SIZE_BYTES
        let bloat_data = "A".repeat(10_000);

        let oversized_claims = OversizedClaims {
            sub: "oversized-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp(),
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
            bloat: bloat_data,
        };

        // Sign the oversized token (this will succeed, signature is valid)
        let encoding_key = EncodingKey::from_ed_der(&private_key);
        let header = Header::new(Algorithm::EdDSA);
        let oversized_token = encode(&header, &oversized_claims, &encoding_key)
            .expect("Failed to encode oversized token");

        // Verify the token is indeed oversized (should be > 4KB)
        assert!(
            oversized_token.len() > 4096,
            "Test token must exceed 4KB limit for this test to be valid. Got {} bytes",
            oversized_token.len()
        );

        tracing::info!(
            "Test generated oversized JWT: {} bytes (limit: 4096 bytes)",
            oversized_token.len()
        );

        // Attempt to verify the oversized token
        // Should be rejected BEFORE signature verification (efficiency)
        let result = crypto::verify_jwt(&oversized_token, &signing_key_model.public_key);

        // Should be rejected due to size limit
        assert!(
            matches!(result, Err(AcError::InvalidToken(_))),
            "Oversized JWT should be rejected to prevent DoS attacks"
        );

        // Verify that a normal-sized token still works (regression test)
        let normal_claims = crypto::Claims {
            sub: "normal-client".to_string(),
            exp: Utc::now().timestamp() + TOKEN_EXPIRY_SECONDS_I64,
            iat: Utc::now().timestamp(),
            scope: "meeting:create".to_string(),
            service_type: Some("global-controller".to_string()),
        };

        let normal_token = crypto::sign_jwt(&normal_claims, &private_key)?;

        // Verify the normal token is under the size limit
        assert!(
            normal_token.len() <= 4096,
            "Normal token should be under 4KB. Got {} bytes",
            normal_token.len()
        );

        // Normal token should verify successfully
        let result = crypto::verify_jwt(&normal_token, &signing_key_model.public_key);
        assert!(
            result.is_ok(),
            "Normal-sized JWT should still be accepted (regression check)"
        );

        let verified = result.unwrap();
        assert_eq!(verified.sub, "normal-client");
        assert_eq!(verified.scope, "meeting:create");

        Ok(())
    }

    // NOTE: JWT size limit boundary testing (4095, 4096, 4097 bytes) is complex
    // because base64 encoding affects final size unpredictably. The unit tests
    // in crypto::tests::test_jwt_size_limit_enforcement and
    // crypto::tests::test_jwt_size_limit_allows_normal_tokens provide adequate
    // coverage for the size limit logic. The integration test above
    // (test_jwt_oversized_token_rejected) validates the full flow with a 10KB token.

    // ============================================================================
    // P1 Security Tests - Key Rotation & Lifecycle
    // ============================================================================
    //
    // TODO(security): Add key rotation tests when supporting repository methods exist:
    // - signing_keys::get_by_key_id() - Retrieve specific key by ID
    // - signing_keys::list_all_keys() - List all keys (active and inactive)
    //
    // Required tests:
    // 1. test_jwt_signed_with_deactivated_key_rejected - Verify old keys don't work after rotation
    // 2. test_key_rotation_lifecycle - Verify only one key active at a time

    // ============================================================================
    // P1 Security Tests - Error Message Information Leakage Prevention
    // ============================================================================

    /// P1-SECURITY: Test that error messages don't leak sensitive information
    ///
    /// Verifies that error messages don't reveal:
    /// - Whether a client_id exists in the database (username enumeration)
    /// - Database implementation details
    /// - Stack traces or internal paths
    /// - Specific authentication failure reasons (password vs client_id)
    ///
    /// Per OWASP A05:2021 (Security Misconfiguration), CWE-209 (Information Exposure)
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_error_messages_no_info_leakage(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        // Create a valid credential
        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "existing-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Test various failure scenarios
        let scenarios = vec![
            (
                "nonexistent-client",
                "any-password",
                "nonexistent client_id should not be revealed",
            ),
            (
                "existing-client",
                "wrong-password",
                "invalid password should not be revealed",
            ),
        ];

        for (client_id, password, expectation) in scenarios {
            let result = issue_service_token(
                &pool,
                &master_key,
                client_id,
                password,
                "client_credentials",
                None,
                None,
                None,
            )
            .await;

            // Both scenarios should return InvalidCredentials
            assert!(
                matches!(result, Err(AcError::InvalidCredentials)),
                "Expected InvalidCredentials for: {}",
                expectation
            );

            if let Err(e) = result {
                let error_msg = e.to_string();

                // Error message should NOT contain sensitive information
                assert!(
                    !error_msg.to_lowercase().contains("not found"),
                    "Error should not reveal 'not found': {}",
                    expectation
                );
                assert!(
                    !error_msg.to_lowercase().contains("password"),
                    "Error should not reveal 'password': {}",
                    expectation
                );
                assert!(
                    !error_msg.to_lowercase().contains("database"),
                    "Error should not reveal 'database': {}",
                    expectation
                );
                assert!(
                    !error_msg.to_lowercase().contains("sql"),
                    "Error should not reveal 'sql': {}",
                    expectation
                );
                assert!(
                    !error_msg.contains("client_id"),
                    "Error should not reveal 'client_id': {}",
                    expectation
                );
                // Allow "credentials" (plural, generic) but not "credential_id" or specific details
                assert!(
                    !error_msg.contains("credential_id")
                        && !error_msg.contains("service_credential"),
                    "Error should not reveal specific credential details: {}",
                    expectation
                );
                assert!(
                    !error_msg.contains("/home/") && !error_msg.contains("C:\\"),
                    "Error should not reveal file paths: {}",
                    expectation
                );

                // Error message should be generic
                assert!(
                    error_msg.contains("Invalid") || error_msg.contains("invalid"),
                    "Error message should be generic: {}",
                    error_msg
                );
            }
        }

        Ok(())
    }

    /// P1-SECURITY: Test error message consistency for timing attack prevention
    ///
    /// Ensures that different failure types return the same error type and similar messages
    /// to prevent information leakage via error message differences.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_error_message_consistency(pool: PgPool) -> Result<(), AcError> {
        let master_key = crypto::generate_random_bytes(32)?;
        key_management_service::initialize_signing_key(&pool, &master_key, "test").await?;

        let valid_secret = "valid-secret-12345";
        let valid_hash = crypto::hash_client_secret(valid_secret)?;
        service_credentials::create_service_credential(
            &pool,
            "valid-client",
            &valid_hash,
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await?;

        // Both scenarios should return the same error type
        let invalid_client_result = issue_service_token(
            &pool,
            &master_key,
            "nonexistent-client",
            "any-password",
            "client_credentials",
            None,
            None,
            None,
        )
        .await;

        let invalid_password_result = issue_service_token(
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

        // Both should return InvalidCredentials
        assert!(matches!(
            invalid_client_result,
            Err(AcError::InvalidCredentials)
        ));
        assert!(matches!(
            invalid_password_result,
            Err(AcError::InvalidCredentials)
        ));

        // Error messages should be similar in content and length
        let err1 = invalid_client_result.unwrap_err().to_string();
        let err2 = invalid_password_result.unwrap_err().to_string();

        // Messages should have similar length (within 50%) to prevent information leakage
        let length_ratio = if err1.len() > err2.len() {
            err1.len() as f64 / err2.len() as f64
        } else {
            err2.len() as f64 / err1.len() as f64
        };

        assert!(
            length_ratio < 1.5,
            "Error message lengths should be similar: {} chars vs {} chars",
            err1.len(),
            err2.len()
        );

        Ok(())
    }
}
