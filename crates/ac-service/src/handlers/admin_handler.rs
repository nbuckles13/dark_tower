use crate::crypto;
use crate::errors::AcError;
use crate::models::RegisterServiceResponse;
use crate::observability::metrics::{record_error, record_key_rotation};
use crate::observability::ErrorCategory;
use crate::repositories::{service_credentials, signing_keys};
use crate::services::{key_management_service, registration_service};
use axum::{
    extract::{Path, Request, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

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
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
/// Only safe fields (service_type, status) are recorded.
#[instrument(
    name = "ac.admin.register_service",
    skip_all,
    fields(service_type, status)
)]
pub async fn handle_register_service(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterServiceRequest>,
) -> Result<Json<RegisterServiceResponse>, AcError> {
    // Record service_type for tracing (safe field per ADR-0011)
    tracing::Span::current().record("service_type", &payload.service_type);

    // Validate service_type
    let valid_types = ["global-controller", "meeting-controller", "media-handler"];
    if !valid_types.contains(&payload.service_type.as_str()) {
        tracing::Span::current().record("status", "error");
        let err = AcError::Database(format!(
            "Invalid service_type: '{}'. Must be one of: {}",
            payload.service_type,
            valid_types.join(", ")
        ));
        record_error(
            "register_service",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );
        return Err(err);
    }

    // Register the service
    let result =
        registration_service::register_service(&state.pool, &payload.service_type, payload.region)
            .await;

    let status = if result.is_ok() { "success" } else { "error" };
    tracing::Span::current().record("status", status);

    // ADR-0011: Record error category for failed requests
    match result {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            let category = ErrorCategory::from(&e);
            record_error("register_service", category.as_str(), e.status_code());
            Err(e)
        }
    }
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
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
/// Only safe fields (forced, status) are recorded. client_id is NOT logged.
#[instrument(name = "ac.admin.rotate_keys", skip_all, fields(forced, status))]
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

    // Extract kid from JWT header to look up the correct signing key
    // This is required for key rotation support: during the overlap period,
    // tokens signed with the old key are still valid but we need to verify
    // them with the old key, not the new "active" key.
    let kid = crate::crypto::extract_jwt_kid(token).ok_or(AcError::InvalidToken(
        "Missing or invalid key ID in token header".to_string(),
    ))?;

    // Look up the signing key by kid (not just "active" key)
    // This ensures tokens signed with old keys (still in validity window) work
    let signing_key = signing_keys::get_by_key_id(&state.pool, &kid)
        .await?
        .ok_or_else(|| {
            tracing::debug!(
                target: "crypto",
                kid = %kid,
                "Token references unknown key ID"
            );
            AcError::InvalidToken("The access token is invalid or expired".to_string())
        })?;

    // SECURITY: Verify the key is still within its validity window
    let now = Utc::now();
    if now < signing_key.valid_from || now >= signing_key.valid_until {
        tracing::debug!(
            target: "crypto",
            kid = %kid,
            valid_from = %signing_key.valid_from,
            valid_until = %signing_key.valid_until,
            now = %now,
            "Token signed with key outside validity window"
        );
        let err = AcError::InvalidToken("The access token is invalid or expired".to_string());
        tracing::Span::current().record("status", "error");
        record_key_rotation("error");
        record_error(
            "rotate_keys",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );
        return Err(err);
    }

    // Verify JWT and extract claims with configured clock skew tolerance
    let claims = crate::crypto::verify_jwt(
        token,
        &signing_key.public_key,
        state.config.jwt_clock_skew_seconds,
    )?;

    // SECURITY: Require service token (must have service_type)
    // User tokens (no service_type) are not authorized for key rotation
    if claims.service_type.is_none() {
        tracing::warn!(
            target: "audit",
            event = "key_rotation_denied",
            client_id = %claims.sub,
            success = false,
            reason = "user_token_not_allowed",
            "Key rotation denied: user tokens cannot rotate keys"
        );

        let err =
            AcError::InvalidToken("User tokens are not authorized for key rotation".to_string());
        tracing::Span::current().record("status", "error");
        record_key_rotation("error");
        record_error(
            "rotate_keys",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );
        return Err(err);
    }

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

        let err = AcError::InsufficientScope {
            required: "service.rotate-keys.ac".to_string(),
            provided: token_scopes.iter().map(|s| s.to_string()).collect(),
        };
        tracing::Span::current().record("status", "error");
        record_key_rotation("error");
        record_error(
            "rotate_keys",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );
        return Err(err);
    }

    // Get cluster name from environment, default to "default" for development
    // SECURITY FIX: Make cluster name configurable instead of hardcoded
    let cluster_name = std::env::var("AC_CLUSTER_NAME").unwrap_or_else(|_| "default".to_string());

    // SECURITY FIX: Use database transaction with advisory lock to prevent TOCTOU race condition
    // This ensures rate limit check and rotation are atomic
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AcError::Database(format!("Failed to begin transaction: {}", e)))?;

    // SECURITY: Acquire advisory lock to serialize all key rotation requests
    // This prevents TOCTOU race conditions where multiple concurrent requests
    // could bypass rate limiting by reading the same last_rotation timestamp.
    // The lock is transaction-scoped and automatically released on commit/rollback.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('key_rotation'))")
        .execute(&mut *tx)
        .await
        .map_err(|e| AcError::Database(format!("Failed to acquire rotation lock: {}", e)))?;

    // Query last rotation timestamp
    // The advisory lock ensures only ONE request at a time can perform this check
    let last_rotation: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        r#"
        SELECT created_at
        FROM signing_keys
        ORDER BY created_at DESC
        LIMIT 1
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
            let err = AcError::TooManyRequests {
                retry_after_seconds,
                message: "Key rotation temporarily unavailable".to_string(),
            };
            tracing::Span::current().record("status", "error");
            tracing::Span::current().record("forced", has_force_scope);
            record_key_rotation("error");
            record_error(
                "rotate_keys",
                ErrorCategory::from(&err).as_str(),
                err.status_code(),
            );
            return Err(err);
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

    // ADR-0011: Record metrics and span fields
    tracing::Span::current().record("forced", has_force_scope);
    tracing::Span::current().record("status", "success");
    record_key_rotation("success");

    Ok(Json(RotateKeysResponse {
        rotated: true,
        new_key_id,
        old_key_id: old_key.key_id.clone(),
        old_key_valid_until: old_key_updated.valid_until.to_rfc3339(),
    }))
}

// ============================================================================
// OAuth Client Management CRUD Endpoints
// ============================================================================

/// Client list item response (excludes client_secret_hash)
#[derive(Debug, Serialize)]
pub struct ClientListItem {
    pub id: Uuid,
    pub client_id: String,
    pub service_type: String,
    pub scopes: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Client detail response (excludes client_secret_hash)
#[derive(Debug, Serialize)]
pub struct ClientDetailResponse {
    pub id: Uuid,
    pub client_id: String,
    pub service_type: String,
    pub region: Option<String>,
    pub scopes: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create client request
#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub service_type: String,
    pub region: Option<String>,
}

/// Create client response (ONLY time client_secret is returned)
#[derive(Debug, Serialize)]
pub struct CreateClientResponse {
    pub id: Uuid,
    pub client_id: String,
    pub client_secret: String, // ONLY returned at creation time
    pub service_type: String,
    pub scopes: Vec<String>,
}

/// Update client request
#[derive(Debug, Deserialize)]
pub struct UpdateClientRequest {
    pub scopes: Option<Vec<String>>,
}

/// Rotate secret response (ONLY time new client_secret is returned)
#[derive(Debug, Serialize)]
pub struct RotateSecretResponse {
    pub client_id: String,
    pub client_secret: String, // New secret - ONLY returned at rotation time
}

/// List all OAuth clients
///
/// GET /api/v1/admin/clients
///
/// Returns all registered OAuth clients (excludes client_secret)
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.list_clients", skip_all, fields(status))]
pub async fn handle_list_clients(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ClientListItem>>, AcError> {
    // Fetch all credentials
    let result = service_credentials::get_all(&state.pool).await;

    match result {
        Ok(credentials) => {
            // Map to response type (exclude client_secret_hash)
            let clients: Vec<ClientListItem> = credentials
                .into_iter()
                .map(|c| ClientListItem {
                    id: c.credential_id,
                    client_id: c.client_id,
                    service_type: c.service_type,
                    scopes: c.scopes,
                    is_active: c.is_active,
                    created_at: c.created_at,
                })
                .collect();

            tracing::Span::current().record("status", "success");

            // Audit log successful operation
            tracing::info!(
                target: "audit",
                event = "clients_listed",
                success = true,
                count = clients.len(),
                "Clients listed successfully"
            );

            Ok(Json(clients))
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("list_clients", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "clients_listed",
                success = false,
                "Failed to list clients"
            );

            Err(e)
        }
    }
}

/// Get specific client details
///
/// GET /api/v1/admin/clients/{id}
///
/// Returns detailed information about a specific client (excludes client_secret)
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.get_client", skip_all, fields(status))]
pub async fn handle_get_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<ClientDetailResponse>, AcError> {
    // Fetch credential by ID
    let result = service_credentials::get_by_credential_id(&state.pool, id).await;

    match result {
        Ok(Some(credential)) => {
            // Map to response type (exclude client_secret_hash)
            let response = ClientDetailResponse {
                id: credential.credential_id,
                client_id: credential.client_id.clone(),
                service_type: credential.service_type,
                region: credential.region,
                scopes: credential.scopes,
                is_active: credential.is_active,
                created_at: credential.created_at,
                updated_at: credential.updated_at,
            };

            tracing::Span::current().record("status", "success");

            // Audit log successful operation
            tracing::info!(
                target: "audit",
                event = "client_retrieved",
                success = true,
                credential_id = %id,
                "Client retrieved successfully"
            );

            Ok(Json(response))
        }
        Ok(None) => {
            tracing::Span::current().record("status", "error");
            let err = AcError::NotFound(format!("Client with ID {} not found", id));
            record_error(
                "get_client",
                ErrorCategory::from(&err).as_str(),
                err.status_code(),
            );

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_retrieved",
                success = false,
                credential_id = %id,
                "Client not found"
            );

            Err(err)
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("get_client", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_retrieved",
                success = false,
                credential_id = %id,
                "Failed to retrieve client"
            );

            Err(e)
        }
    }
}

/// Create new OAuth client
///
/// POST /api/v1/admin/clients
///
/// Generates client_id and client_secret, stores in database.
/// This is the ONLY time the plaintext client_secret is returned.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
/// Only safe fields (service_type, status) are recorded.
#[instrument(
    name = "ac.admin.create_client",
    skip_all,
    fields(service_type, status)
)]
pub async fn handle_create_client(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateClientRequest>,
) -> Result<Json<CreateClientResponse>, AcError> {
    // Record service_type for tracing (safe field per ADR-0011)
    tracing::Span::current().record("service_type", &payload.service_type);

    // Validate service_type
    let valid_types = ["global-controller", "meeting-controller", "media-handler"];
    if !valid_types.contains(&payload.service_type.as_str()) {
        tracing::Span::current().record("status", "error");
        let err = AcError::Database(format!(
            "Invalid service_type: '{}'. Must be one of: {}",
            payload.service_type,
            valid_types.join(", ")
        ));
        record_error(
            "create_client",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );

        // Audit log failed operation
        tracing::warn!(
            target: "audit",
            event = "client_created",
            success = false,
            service_type = %payload.service_type,
            "Client creation failed: invalid service_type"
        );

        return Err(err);
    }

    // Use existing registration service to create the client
    let result =
        registration_service::register_service(&state.pool, &payload.service_type, payload.region)
            .await;

    match result {
        Ok(registration) => {
            // Fetch the created credential to get credential_id
            let credential =
                service_credentials::get_by_client_id(&state.pool, &registration.client_id)
                    .await?
                    .ok_or_else(|| {
                        AcError::Database("Failed to retrieve created credential".to_string())
                    })?;

            // Map to response type
            let response = CreateClientResponse {
                id: credential.credential_id,
                client_id: registration.client_id.clone(),
                client_secret: registration.client_secret, // ONLY returned here
                service_type: registration.service_type,
                scopes: registration.scopes,
            };

            tracing::Span::current().record("status", "success");

            // Audit log successful operation
            tracing::info!(
                target: "audit",
                event = "client_created",
                success = true,
                credential_id = %credential.credential_id,
                client_id = %registration.client_id,
                "Client created successfully"
            );

            Ok(Json(response))
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("create_client", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_created",
                success = false,
                "Failed to create client"
            );

            Err(e)
        }
    }
}

/// Update client metadata
///
/// PUT /api/v1/admin/clients/{id}
///
/// Updates client scopes. Cannot update client_id or regenerate secret.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.update_client", skip_all, fields(status))]
pub async fn handle_update_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateClientRequest>,
) -> Result<Json<ClientDetailResponse>, AcError> {
    // Verify credential exists
    let result = service_credentials::get_by_credential_id(&state.pool, id).await;

    let credential = match result {
        Ok(Some(c)) => c,
        Ok(None) => {
            tracing::Span::current().record("status", "error");
            let err = AcError::NotFound(format!("Client with ID {} not found", id));
            record_error(
                "update_client",
                ErrorCategory::from(&err).as_str(),
                err.status_code(),
            );

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_updated",
                success = false,
                credential_id = %id,
                "Client not found"
            );

            return Err(err);
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("update_client", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_updated",
                success = false,
                credential_id = %id,
                "Failed to update client"
            );

            return Err(e);
        }
    };

    // Update scopes if provided
    let updated_credential = if let Some(new_scopes) = payload.scopes {
        // Validate scopes format
        for scope in &new_scopes {
            // Basic scope validation: non-empty, reasonable length, allowed characters
            if scope.is_empty() {
                tracing::Span::current().record("status", "error");
                let err = AcError::Database("Scope cannot be empty".to_string());
                record_error(
                    "update_client",
                    ErrorCategory::from(&err).as_str(),
                    err.status_code(),
                );

                // Audit log failed operation
                tracing::warn!(
                    target: "audit",
                    event = "client_updated",
                    success = false,
                    credential_id = %id,
                    "Invalid scope: empty"
                );

                return Err(err);
            }

            if scope.len() > 100 {
                tracing::Span::current().record("status", "error");
                let err = AcError::Database(format!(
                    "Scope '{}' exceeds maximum length of 100 characters",
                    scope
                ));
                record_error(
                    "update_client",
                    ErrorCategory::from(&err).as_str(),
                    err.status_code(),
                );

                // Audit log failed operation
                tracing::warn!(
                    target: "audit",
                    event = "client_updated",
                    success = false,
                    credential_id = %id,
                    "Invalid scope: too long"
                );

                return Err(err);
            }

            // Allow alphanumeric, hyphens, dots, colons (common in OAuth scopes)
            if !scope
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == ':')
            {
                tracing::Span::current().record("status", "error");
                let err = AcError::Database(format!("Scope '{}' contains invalid characters. Only alphanumeric, hyphens, dots, and colons are allowed", scope));
                record_error(
                    "update_client",
                    ErrorCategory::from(&err).as_str(),
                    err.status_code(),
                );

                // Audit log failed operation
                tracing::warn!(
                    target: "audit",
                    event = "client_updated",
                    success = false,
                    credential_id = %id,
                    "Invalid scope: invalid characters"
                );

                return Err(err);
            }
        }

        service_credentials::update_metadata(&state.pool, id, &new_scopes).await?
    } else {
        // No updates requested, return current credential
        credential
    };

    // Map to response type
    let response = ClientDetailResponse {
        id: updated_credential.credential_id,
        client_id: updated_credential.client_id.clone(),
        service_type: updated_credential.service_type,
        region: updated_credential.region,
        scopes: updated_credential.scopes,
        is_active: updated_credential.is_active,
        created_at: updated_credential.created_at,
        updated_at: updated_credential.updated_at,
    };

    tracing::Span::current().record("status", "success");

    // Audit log successful operation
    tracing::info!(
        target: "audit",
        event = "client_updated",
        success = true,
        credential_id = %id,
        "Client updated successfully"
    );

    Ok(Json(response))
}

/// Delete client
///
/// DELETE /api/v1/admin/clients/{id}
///
/// Hard delete - removes credentials from database.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.delete_client", skip_all, fields(status))]
pub async fn handle_delete_client(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AcError> {
    // First check if credential exists
    let result = service_credentials::get_by_credential_id(&state.pool, id).await;

    match result {
        Ok(Some(_)) => {
            // Credential exists, proceed with deletion
            let delete_result = service_credentials::delete(&state.pool, id).await;

            match delete_result {
                Ok(_) => {
                    tracing::Span::current().record("status", "success");

                    // Audit log successful operation
                    tracing::info!(
                        target: "audit",
                        event = "client_deleted",
                        success = true,
                        credential_id = %id,
                        "Client deleted successfully"
                    );

                    Ok(Json(serde_json::json!({ "deleted": true })))
                }
                Err(e) => {
                    tracing::Span::current().record("status", "error");
                    let category = ErrorCategory::from(&e);
                    record_error("delete_client", category.as_str(), e.status_code());

                    // Audit log failed operation
                    tracing::warn!(
                        target: "audit",
                        event = "client_deleted",
                        success = false,
                        credential_id = %id,
                        "Failed to delete client"
                    );

                    Err(e)
                }
            }
        }
        Ok(None) => {
            tracing::Span::current().record("status", "error");
            let err = AcError::NotFound(format!("Client with ID {} not found", id));
            record_error(
                "delete_client",
                ErrorCategory::from(&err).as_str(),
                err.status_code(),
            );

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_deleted",
                success = false,
                credential_id = %id,
                "Client not found"
            );

            Err(err)
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("delete_client", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_deleted",
                success = false,
                credential_id = %id,
                "Failed to delete client"
            );

            Err(e)
        }
    }
}

/// Rotate client secret
///
/// POST /api/v1/admin/clients/{id}/rotate-secret
///
/// Generates new client_secret, invalidates old one.
/// This is the ONLY time the new plaintext client_secret is returned.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.rotate_client_secret", skip_all, fields(status))]
pub async fn handle_rotate_client_secret(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<RotateSecretResponse>, AcError> {
    // Verify credential exists
    let result = service_credentials::get_by_credential_id(&state.pool, id).await;

    let credential = match result {
        Ok(Some(c)) => c,
        Ok(None) => {
            tracing::Span::current().record("status", "error");
            let err = AcError::NotFound(format!("Client with ID {} not found", id));
            record_error(
                "rotate_client_secret",
                ErrorCategory::from(&err).as_str(),
                err.status_code(),
            );

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_secret_rotated",
                success = false,
                credential_id = %id,
                "Client not found"
            );

            return Err(err);
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("rotate_client_secret", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_secret_rotated",
                success = false,
                credential_id = %id,
                "Failed to rotate client secret"
            );

            return Err(e);
        }
    };

    // Generate new client_secret (32 bytes, CSPRNG, base64)
    let new_client_secret = crypto::generate_client_secret()?;

    // Hash new client_secret with bcrypt (cost factor 12)
    let new_secret_hash = crypto::hash_client_secret(&new_client_secret)?;

    // Update database with new hash
    let rotate_result = service_credentials::rotate_secret(&state.pool, id, &new_secret_hash).await;

    match rotate_result {
        Ok(_) => {
            // Return response with new secret (ONLY time it's shown)
            let response = RotateSecretResponse {
                client_id: credential.client_id.clone(),
                client_secret: new_client_secret, // ONLY returned here
            };

            tracing::Span::current().record("status", "success");

            // Audit log successful operation
            tracing::info!(
                target: "audit",
                event = "client_secret_rotated",
                success = true,
                credential_id = %id,
                client_id = %credential.client_id,
                "Client secret rotated successfully"
            );

            Ok(Json(response))
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("rotate_client_secret", category.as_str(), e.status_code());

            // Audit log failed operation
            tracing::warn!(
                target: "audit",
                event = "client_secret_rotated",
                success = false,
                credential_id = %id,
                "Failed to rotate client secret"
            );

            Err(e)
        }
    }
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
            other => {
                // Use expect to fail with a clear message if it's not the expected error type
                let _ = matches!(other, AcError::Database(_));
                assert!(
                    matches!(other, AcError::Database(_)),
                    "Expected Database error, got: {:?}",
                    other
                );
            }
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

    // ============================================================================
    // Handler Tests for OAuth Client Management CRUD Endpoints
    // ============================================================================

    // ----------------------------------------------------------------------------
    // Create Client Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_create_client_success(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: Some("us-west-2".to_string()),
        };

        let result = handle_create_client(State(state), Json(payload)).await;

        assert!(
            result.is_ok(),
            "Valid request should succeed: {:?}",
            result.err()
        );
        let response = result.unwrap().0;
        assert_eq!(response.service_type, "global-controller");
        assert!(!response.client_id.is_empty());
        assert!(!response.client_secret.is_empty());
        assert!(!response.scopes.is_empty());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_create_client_invalid_service_type(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = CreateClientRequest {
            service_type: "invalid-service".to_string(),
            region: None,
        };

        let result = handle_create_client(State(state), Json(payload)).await;

        assert!(result.is_err(), "Invalid service_type should be rejected");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::Database(msg) if msg.contains("Invalid service_type")),
            "Expected Database error with 'Invalid service_type', got: {:?}",
            err
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_create_client_empty_service_type(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let payload = CreateClientRequest {
            service_type: "".to_string(),
            region: None,
        };

        let result = handle_create_client(State(state), Json(payload)).await;

        assert!(result.is_err(), "Empty service_type should be rejected");
    }

    // ----------------------------------------------------------------------------
    // List Clients Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_list_clients_empty(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let result = handle_list_clients(State(state)).await;

        assert!(result.is_ok(), "List should succeed even when empty");
        let response = result.unwrap().0;
        assert_eq!(response.len(), 0, "Empty database should return empty list");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_list_clients_multiple(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create multiple clients
        let payload1 = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: Some("us-west-2".to_string()),
        };
        let _ = handle_create_client(State(state.clone()), Json(payload1))
            .await
            .unwrap();

        let payload2 = CreateClientRequest {
            service_type: "meeting-controller".to_string(),
            region: None,
        };
        let _ = handle_create_client(State(state.clone()), Json(payload2))
            .await
            .unwrap();

        // List all clients
        let result = handle_list_clients(State(state)).await;

        assert!(result.is_ok(), "List should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.len(), 2, "Should return all 2 clients");

        // Verify response structure (excludes client_secret_hash)
        assert!(!response[0].client_id.is_empty());
        assert!(!response[0].service_type.is_empty());
        assert!(!response[0].scopes.is_empty());
    }

    // ----------------------------------------------------------------------------
    // Get Client Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_get_client_success(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: Some("us-west-2".to_string()),
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        // Get the client by ID
        let result = handle_get_client(State(state), Path(create_response.id)).await;

        assert!(result.is_ok(), "Get should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.id, create_response.id);
        assert_eq!(response.client_id, create_response.client_id);
        assert_eq!(response.service_type, "global-controller");
        assert_eq!(response.region, Some("us-west-2".to_string()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_get_client_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        // Try to get nonexistent client
        let random_uuid = Uuid::new_v4();
        let result = handle_get_client(State(state), Path(random_uuid)).await;

        assert!(result.is_err(), "Should return error for unknown UUID");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::NotFound(_)),
            "Expected NotFound error, got: {:?}",
            err
        );
    }

    // ----------------------------------------------------------------------------
    // Update Client Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_update_client_success(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        // Update scopes
        let update_payload = UpdateClientRequest {
            scopes: Some(vec![
                "meeting:create".to_string(),
                "meeting:update".to_string(),
                "meeting:delete".to_string(),
            ]),
        };

        let result =
            handle_update_client(State(state), Path(create_response.id), Json(update_payload))
                .await;

        assert!(result.is_ok(), "Update should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.id, create_response.id);
        assert_eq!(
            response.scopes,
            vec![
                "meeting:create".to_string(),
                "meeting:update".to_string(),
                "meeting:delete".to_string(),
            ]
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_update_client_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let update_payload = UpdateClientRequest {
            scopes: Some(vec!["scope1".to_string()]),
        };

        let result =
            handle_update_client(State(state), Path(random_uuid), Json(update_payload)).await;

        assert!(result.is_err(), "Should return error for unknown UUID");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::NotFound(msg) if msg.contains(&random_uuid.to_string())),
            "Expected NotFound error containing UUID, got: {:?}",
            err
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_update_client_invalid_scope_format(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        // Try to update with invalid scope (contains special characters)
        let update_payload = UpdateClientRequest {
            scopes: Some(vec!["invalid@scope#value".to_string()]),
        };

        let result =
            handle_update_client(State(state), Path(create_response.id), Json(update_payload))
                .await;

        assert!(result.is_err(), "Should reject invalid scope format");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::Database(msg) if msg.contains("invalid characters")),
            "Expected Database error with 'invalid characters', got: {:?}",
            err
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_update_client_no_changes(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        let original_scopes = create_response.scopes.clone();

        // Update with no scopes provided (no-op)
        let update_payload = UpdateClientRequest { scopes: None };

        let result =
            handle_update_client(State(state), Path(create_response.id), Json(update_payload))
                .await;

        assert!(result.is_ok(), "No-op update should succeed");
        let response = result.unwrap().0;
        assert_eq!(
            response.scopes, original_scopes,
            "Scopes should be unchanged"
        );
    }

    // ----------------------------------------------------------------------------
    // Delete Client Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_delete_client_success(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client directly via repository (avoid creating auth_events)
        let credential = crate::repositories::service_credentials::create_service_credential(
            &pool,
            "test-delete-client",
            "hash",
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await
        .unwrap();

        // Delete the client
        let result =
            handle_delete_client(State(state.clone()), Path(credential.credential_id)).await;

        assert!(result.is_ok(), "Delete should succeed: {:?}", result.err());
        let response = result.unwrap().0;
        assert_eq!(response.get("deleted"), Some(&serde_json::json!(true)));

        // Verify it's actually deleted
        let get_result = handle_get_client(State(state), Path(credential.credential_id)).await;
        assert!(get_result.is_err(), "Client should be deleted");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_delete_client_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let result = handle_delete_client(State(state), Path(random_uuid)).await;

        assert!(result.is_err(), "Should return error for unknown UUID");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::NotFound(msg) if msg.contains(&random_uuid.to_string())),
            "Expected NotFound error containing UUID, got: {:?}",
            err
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_delete_client_idempotent(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client directly via repository (avoid creating auth_events)
        let credential = crate::repositories::service_credentials::create_service_credential(
            &pool,
            "test-delete-idempotent",
            "hash",
            "global-controller",
            None,
            &["meeting:create".to_string()],
        )
        .await
        .unwrap();

        // First delete
        let result1 =
            handle_delete_client(State(state.clone()), Path(credential.credential_id)).await;
        assert!(
            result1.is_ok(),
            "First delete should succeed: {:?}",
            result1.err()
        );

        // Second delete (should return 404)
        let result2 = handle_delete_client(State(state), Path(credential.credential_id)).await;
        assert!(result2.is_err(), "Second delete should return error");
        let err = result2.unwrap_err();
        assert!(
            matches!(&err, AcError::NotFound(_)),
            "Expected NotFound error, got: {:?}",
            err
        );
    }

    // ----------------------------------------------------------------------------
    // Rotate Secret Tests
    // ----------------------------------------------------------------------------

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_rotate_client_secret_success(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        let original_secret = create_response.client_secret.clone();

        // Rotate secret
        let result = handle_rotate_client_secret(State(state), Path(create_response.id)).await;

        assert!(result.is_ok(), "Rotate should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.client_id, create_response.client_id);
        assert!(!response.client_secret.is_empty());
        assert_ne!(
            response.client_secret, original_secret,
            "New secret should differ from original"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_rotate_client_secret_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let result = handle_rotate_client_secret(State(state), Path(random_uuid)).await;

        assert!(result.is_err(), "Should return error for unknown UUID");
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::NotFound(msg) if msg.contains(&random_uuid.to_string())),
            "Expected NotFound error containing UUID, got: {:?}",
            err
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_rotate_client_secret_changes_hash(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState {
            pool: pool.clone(),
            config,
        });

        // Create a client
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let create_response = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        // Get original credential from database to check hash
        let original_cred = crate::repositories::service_credentials::get_by_credential_id(
            &state.pool,
            create_response.id,
        )
        .await
        .unwrap()
        .unwrap();

        // Rotate secret
        let rotate_result =
            handle_rotate_client_secret(State(state.clone()), Path(create_response.id)).await;
        assert!(rotate_result.is_ok(), "Rotate should succeed");

        // Get updated credential to verify hash changed
        let updated_cred = crate::repositories::service_credentials::get_by_credential_id(
            &state.pool,
            create_response.id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_ne!(
            updated_cred.client_secret_hash, original_cred.client_secret_hash,
            "Secret hash should change after rotation"
        );
    }
}
