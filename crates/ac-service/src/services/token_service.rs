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
        let failed_count = auth_events::get_failed_attempts_count(
            pool,
            &cred.credential_id,
            fifteen_mins_ago
        ).await?;

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
            Some(if !credential.is_active { "Credential is inactive" } else { "Invalid client secret" }),
            ip_address,
            user_agent,
            None,
        )
        .await {
            tracing::warn!("Failed to log auth event: {}", e);
        }

        return Err(AcError::InvalidCredentials);
    }

    // Determine scopes (use requested scopes if provided and valid, otherwise use default)
    let scopes = if let Some(req_scopes) = requested_scopes {
        // Verify requested scopes are subset of allowed scopes
        let all_valid = req_scopes
            .iter()
            .all(|s| credential.scopes.contains(s));

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
    .await {
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
    pool: &PgPool,
    master_key: &[u8],
    username: &str,
    password: &str,
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
