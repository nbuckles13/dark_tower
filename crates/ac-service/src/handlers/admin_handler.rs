use crate::errors::AcError;
use crate::models::RegisterServiceResponse;
use crate::repositories::signing_keys;
use crate::services::{key_management_service, registration_service};
use axum::{
    extract::{Request, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::auth_handler::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterServiceRequest {
    pub service_type: String,
    pub region: Option<String>,
}

/// Handle service registration
///
/// POST /api/v1/admin/services/register
///
/// Generates client_id and client_secret, stores in database
pub async fn handle_register_service(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterServiceRequest>,
) -> Result<Json<RegisterServiceResponse>, AcError> {
    // Validate service_type
    let valid_types = ["global-controller", "meeting-controller", "media-handler"];
    if !valid_types.contains(&payload.service_type.as_str()) {
        return Err(AcError::Database(format!(
            "Invalid service_type: '{}'. Must be one of: {}",
            payload.service_type,
            valid_types.join(", ")
        )));
    }

    // Register the service
    let response =
        registration_service::register_service(&state.pool, &payload.service_type, payload.region)
            .await?;

    Ok(Json(response))
}

#[derive(Debug, Serialize)]
pub struct RotateKeysResponse {
    pub rotated: bool,
    pub new_key_id: String,
    pub old_key_id: String,
    pub old_key_valid_until: String,
}

/// Handle key rotation request
///
/// POST /internal/rotate-keys
///
/// Requires scope: service.rotate-keys.ac OR admin.force-rotate-keys.ac
///
/// Implements database-driven rate limiting:
/// - Normal rotation (service.rotate-keys.ac): 1 per 6 days
/// - Force rotation (admin.force-rotate-keys.ac): 1 per hour
///
/// SECURITY: Uses database transactions with SELECT FOR UPDATE to prevent
/// TOCTOU race conditions in concurrent rotation requests.
pub async fn handle_rotate_keys(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Result<Json<RotateKeysResponse>, AcError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(AcError::InvalidToken(
            "Missing Authorization header".to_string(),
        ))?;

    // Extract Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AcError::InvalidToken(
            "Invalid Authorization header format".to_string(),
        ))?;

    // Get active signing key for verification
    let signing_key = signing_keys::get_active_key(&state.pool)
        .await?
        .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Verify JWT and extract claims
    let claims = crate::crypto::verify_jwt(token, &signing_key.public_key)?;

    // Check for rotation scopes
    let token_scopes: Vec<&str> = claims.scope.split_whitespace().collect();
    let has_normal_scope = token_scopes.contains(&"service.rotate-keys.ac");
    let has_force_scope = token_scopes.contains(&"admin.force-rotate-keys.ac");

    if !has_normal_scope && !has_force_scope {
        // SECURITY FIX: Audit log failed authorization attempts
        tracing::warn!(
            target: "audit",
            event = "key_rotation_denied",
            client_id = %claims.sub,
            success = false,
            reason = "insufficient_scope",
            required_scope = "service.rotate-keys.ac or admin.force-rotate-keys.ac",
            provided_scopes = ?token_scopes,
            "Key rotation denied: insufficient scope"
        );

        return Err(AcError::InsufficientScope {
            required: "service.rotate-keys.ac".to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        });
    }

    // Get cluster name from environment, default to "default" for development
    // SECURITY FIX: Make cluster name configurable instead of hardcoded
    let cluster_name = std::env::var("AC_CLUSTER_NAME").unwrap_or_else(|_| "default".to_string());

    // SECURITY FIX: Use database transaction with SELECT FOR UPDATE to prevent TOCTOU race condition
    // This ensures rate limit check and rotation are atomic
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AcError::Database(format!("Failed to begin transaction: {}", e)))?;

    // Lock the signing_keys table during rate limit check
    // This prevents concurrent requests from bypassing rate limiting
    let last_rotation: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        r#"
        SELECT created_at
        FROM signing_keys
        ORDER BY created_at DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AcError::Database(format!("Failed to query last rotation: {}", e)))?;

    // Determine minimum interval based on scope
    let (min_interval_days, min_interval_hours) = if has_force_scope {
        (0, 1) // Force rotation: 1 hour minimum
    } else {
        (6, 0) // Normal rotation: 6 days minimum
    };

    // Check if enough time has passed since last rotation
    if let Some(last) = last_rotation {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(last);

        let min_duration =
            chrono::Duration::days(min_interval_days) + chrono::Duration::hours(min_interval_hours);

        if elapsed < min_duration {
            let remaining = min_duration - elapsed;
            let retry_after_seconds = remaining.num_seconds();

            // SECURITY FIX: Audit log rate-limited attempts
            tracing::warn!(
                target: "audit",
                event = "key_rotation_denied",
                client_id = %claims.sub,
                success = false,
                reason = "rate_limit_exceeded",
                forced = has_force_scope,
                retry_after_seconds = retry_after_seconds,
                elapsed_seconds = elapsed.num_seconds(),
                min_interval_seconds = min_duration.num_seconds(),
                "Key rotation denied: rate limit exceeded"
            );

            // SECURITY FIX: Use generic error message to avoid information leakage
            return Err(AcError::TooManyRequests {
                retry_after_seconds,
                message: "Key rotation temporarily unavailable".to_string(),
            });
        }
    }

    // Get old active key before rotation (within same transaction)
    let old_key = sqlx::query_as::<_, crate::models::SigningKey>(
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
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AcError::Database(format!("Failed to fetch active key: {}", e)))?
    .ok_or_else(|| AcError::Crypto("No active signing key available".to_string()))?;

    // Perform rotation within same transaction
    let new_key_id = key_management_service::rotate_signing_key_tx(
        &mut tx,
        &state.config.master_key,
        &cluster_name,
    )
    .await?;

    // Commit transaction - if this fails, all changes (including rotation) are rolled back
    tx.commit()
        .await
        .map_err(|e| AcError::Database(format!("Failed to commit rotation transaction: {}", e)))?;

    // Get the updated old key to retrieve its valid_until (after transaction commit)
    let old_key_updated = signing_keys::get_by_key_id(&state.pool, &old_key.key_id)
        .await?
        .ok_or_else(|| AcError::Crypto("Failed to retrieve old key after rotation".to_string()))?;

    // Log successful rotation AFTER transaction commits
    // This ensures we only log events that actually happened
    tracing::info!(
        target: "audit",
        event = "key_rotation_success",
        client_id = %claims.sub,
        success = true,
        forced = has_force_scope,
        new_key_id = %new_key_id,
        old_key_id = %old_key.key_id,
        cluster_name = %cluster_name,
        "Key rotation successful"
    );

    Ok(Json(RotateKeysResponse {
        rotated: true,
        new_key_id,
        old_key_id: old_key.key_id.clone(),
        old_key_valid_until: old_key_updated.valid_until.to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use base64::{engine::general_purpose, Engine};
    use std::collections::HashMap;

    /// Create a test config with required environment variables
    fn test_config() -> Config {
        let master_key = general_purpose::STANDARD.encode([0u8; 32]);
        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://localhost/test".to_string(),
            ),
            ("AC_MASTER_KEY".to_string(), master_key),
        ]);
        Config::from_vars(&vars).expect("Test config should be valid")
    }

    #[test]
    fn test_register_service_request_deserialization() {
        let json = r#"{"service_type": "global-controller", "region": "us-west-2"}"#;
        let req: RegisterServiceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.service_type, "global-controller");
        assert_eq!(req.region, Some("us-west-2".to_string()));
    }

    #[test]
    fn test_register_service_request_without_region() {
        let json = r#"{"service_type": "meeting-controller"}"#;
        let req: RegisterServiceRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.service_type, "meeting-controller");
        assert_eq!(req.region, None);
    }

    #[test]
    fn test_valid_service_types() {
        let valid_types = ["global-controller", "meeting-controller", "media-handler"];

        for service_type in valid_types {
            let json = format!(r#"{{"service_type": "{}"}}"#, service_type);
            let req: RegisterServiceRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(req.service_type, service_type);
        }
    }

    #[test]
    fn test_invalid_service_type_format() {
        // Note: This tests deserialization, not handler validation
        let json = r#"{"service_type": "invalid-service"}"#;
        let req: RegisterServiceRequest = serde_json::from_str(json).unwrap();
        // Deserialization succeeds (it's just a string)
        assert_eq!(req.service_type, "invalid-service");
        // Validation happens in the handler, not during deserialization
    }

    // ============================================================================
    // Handler Integration Tests - Error Paths
    // ============================================================================

    /// Test handle_register_service rejects invalid service_type
    ///
    /// Validates that the handler properly validates service_type against
    /// the allowed list before calling the registration service.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_invalid_type(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "invalid-service-type".to_string(),
            region: None,
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        // Should return error
        assert!(result.is_err(), "Invalid service_type should be rejected");

        let err = result.unwrap_err();
        match err {
            AcError::Database(msg) => {
                assert!(
                    msg.contains("Invalid service_type"),
                    "Error should mention invalid service_type, got: {}",
                    msg
                );
                assert!(
                    msg.contains("invalid-service-type"),
                    "Error should include the invalid value"
                );
            }
            _ => panic!("Expected Database error, got: {:?}", err),
        }
    }

    /// Test handle_register_service succeeds for valid global-controller
    ///
    /// Tests the happy path for service registration with all valid inputs.
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_valid_global_controller(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "global-controller".to_string(),
            region: Some("us-west-2".to_string()),
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Valid registration should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap().0;
        assert_eq!(response.service_type, "global-controller");
        assert!(!response.client_id.is_empty());
        assert!(!response.client_secret.is_empty());
        assert!(!response.scopes.is_empty());
    }

    /// Test handle_register_service succeeds for meeting-controller
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_valid_meeting_controller(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "meeting-controller".to_string(),
            region: None,
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        assert!(result.is_ok(), "Valid registration should succeed");

        let response = result.unwrap().0;
        assert_eq!(response.service_type, "meeting-controller");
        assert!(!response.client_id.is_empty());
    }

    /// Test handle_register_service succeeds for media-handler
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_valid_media_handler(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "media-handler".to_string(),
            region: Some("eu-west-1".to_string()),
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        assert!(result.is_ok(), "Valid registration should succeed");

        let response = result.unwrap().0;
        assert_eq!(response.service_type, "media-handler");
        assert_eq!(response.scopes.len(), 3); // media-handler has 3 default scopes
    }

    /// Test handle_register_service validates service_type case-sensitively
    ///
    /// Ensures that service_type matching is case-sensitive for security
    /// (prevents "Global-Controller" from being accepted).
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_case_sensitive(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "Global-Controller".to_string(), // Wrong case
            region: None,
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        // Should fail - case-sensitive check
        assert!(result.is_err(), "Case-sensitive validation should reject");
    }

    /// Test handle_register_service with empty string service_type
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_empty_service_type(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: "".to_string(),
            region: None,
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        assert!(result.is_err(), "Empty service_type should be rejected");
    }

    /// Test handle_register_service with whitespace in service_type
    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_register_service_whitespace_service_type(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = RegisterServiceRequest {
            service_type: " global-controller ".to_string(),
            region: None,
        };

        let result = handle_register_service(State(state), Json(payload)).await;

        // Should fail - whitespace not trimmed
        assert!(
            result.is_err(),
            "service_type with whitespace should be rejected"
        );
    }

    /// Test RegisterServiceRequest Debug implementation
    ///
    /// Ensures Debug trait is properly derived for logging and debugging.
    #[test]
    fn test_register_service_request_debug() {
        let req = RegisterServiceRequest {
            service_type: "global-controller".to_string(),
            region: Some("us-west-2".to_string()),
        };

        let debug_str = format!("{:?}", req);
        assert!(debug_str.contains("global-controller"));
        assert!(debug_str.contains("us-west-2"));
    }
}
