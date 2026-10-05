use crate::crypto;
use crate::errors::AcError;
use crate::models::{RegisterServiceResponse, ServiceCredential, ServiceType};
use crate::observability::metrics::{
    record_credential_operation, record_error, record_key_rotation, set_active_signing_keys,
    set_key_rotation_last_success, set_signing_key_age_days,
};
use crate::observability::ErrorCategory;
use crate::repositories::{service_credentials, signing_keys};
use crate::services::{key_management_service, registration_service};
use axum::{
    extract::{Path, Request, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common::secret::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
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
    if ServiceType::from_str(&payload.service_type).is_err() {
        tracing::Span::current().record("status", "error");
        let err = AcError::BadRequest(registration_service::INVALID_SERVICE_TYPE_MESSAGE);
        record_error(
            "register_service",
            ErrorCategory::from(&err).as_str(),
            err.status_code(),
        );
        record_credential_operation("create", "error");
        return Err(err);
    }

    // Register the service using configured bcrypt cost
    let result = registration_service::register_service(
        &state.pool,
        &payload.service_type,
        payload.region,
        state.config.bcrypt_cost,
    )
    .await;

    let status = if result.is_ok() { "success" } else { "error" };
    tracing::Span::current().record("status", status);

    // ADR-0011: Record error category for failed requests
    match result {
        Ok(response) => {
            record_credential_operation("create", "success");
            Ok(Json(response))
        }
        Err(e) => {
            let category = ErrorCategory::from(&e);
            record_error("register_service", category.as_str(), e.status_code());
            record_credential_operation("create", "error");
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
    let kid = common::jwt::extract_kid(token).map_err(|_| {
        AcError::InvalidToken("Missing or invalid key ID in token header".to_string())
    })?;

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
        std::time::Duration::from_secs(state.config.jwt_clock_skew_seconds as u64),
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
        state.config.master_key.expose_secret(),
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

    // Update key management gauges after successful rotation via admin API.
    // rotate_signing_key_tx does not set gauges, so we set them here.
    set_active_signing_keys(1);
    set_signing_key_age_days(0.0);
    set_key_rotation_last_success(chrono::Utc::now().timestamp() as f64);

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
///
/// The `client_secret` field is wrapped in `SecretString` which:
/// - Redacts the value in Debug output to prevent accidental logging
/// - Requires explicit `.expose_secret()` to access the value
/// - Uses custom serialization to expose the secret in API responses
pub struct CreateClientResponse {
    pub id: Uuid,
    pub client_id: String,
    pub client_secret: common::secret::SecretString, // ONLY returned at creation time
    pub service_type: String,
    pub scopes: Vec<String>,
}

/// Custom Debug that redacts client_secret
impl std::fmt::Debug for CreateClientResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateClientResponse")
            .field("id", &self.id)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("service_type", &self.service_type)
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Custom Serialize that exposes client_secret for API response.
/// This is intentional: the create client response is the ONLY time
/// the plaintext client_secret is shown to the user.
impl Serialize for CreateClientResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("CreateClientResponse", 5)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.serialize_field("service_type", &self.service_type)?;
        state.serialize_field("scopes", &self.scopes)?;
        state.end()
    }
}

/// Update client request
#[derive(Debug, Deserialize)]
pub struct UpdateClientRequest {
    pub scopes: Option<Vec<String>>,
}

/// Rotate secret response (ONLY time new client_secret is returned)
///
/// The `client_secret` field is wrapped in `SecretString` which:
/// - Redacts the value in Debug output to prevent accidental logging
/// - Requires explicit `.expose_secret()` to access the value
/// - Uses custom serialization to expose the secret in API responses
pub struct RotateSecretResponse {
    pub client_id: String,
    pub client_secret: common::secret::SecretString, // New secret - ONLY returned at rotation time
}

/// Custom Debug that redacts client_secret
impl std::fmt::Debug for RotateSecretResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RotateSecretResponse")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .finish()
    }
}

/// Custom Serialize that exposes client_secret for API response.
/// This is intentional: the rotate secret response is the ONLY time
/// the new plaintext client_secret is shown to the user.
impl Serialize for RotateSecretResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RotateSecretResponse", 2)?;
        state.serialize_field("client_id", &self.client_id)?;
        state.serialize_field("client_secret", self.client_secret.expose_secret())?;
        state.end()
    }
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

            record_credential_operation("list", "success");
            Ok(Json(clients))
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("list_clients", category.as_str(), e.status_code());
            record_credential_operation("list", "error");

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
    match registration_service::get_service(&state.pool, id).await {
        Ok(credential) => {
            tracing::Span::current().record("status", "success");

            // Audit log successful operation
            tracing::info!(
                target: "audit",
                event = "client_retrieved",
                success = true,
                credential_id = %id,
                "Client retrieved successfully"
            );

            record_credential_operation("get", "success");
            // Map to response type (exclude client_secret_hash)
            Ok(Json(client_detail(credential)))
        }
        Err(e) => {
            record_client_op_failure("get_client", "get", "client_retrieved", id, &e);
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
    if ServiceType::from_str(&payload.service_type).is_err() {
        tracing::Span::current().record("status", "error");
        let err = AcError::BadRequest(registration_service::INVALID_SERVICE_TYPE_MESSAGE);
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
    let result = registration_service::register_service(
        &state.pool,
        &payload.service_type,
        payload.region,
        state.config.bcrypt_cost,
    )
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

            record_credential_operation("create", "success");
            Ok(Json(response))
        }
        Err(e) => {
            tracing::Span::current().record("status", "error");
            let category = ErrorCategory::from(&e);
            record_error("create_client", category.as_str(), e.status_code());
            record_credential_operation("create", "error");

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

/// Record the metrics + tracing audit event for a failed admin client
/// operation. `credential_op` is the `ac_credential_operations_total`
/// operation label.
fn record_client_op_failure(
    metric_operation: &'static str,
    credential_op: &'static str,
    audit_event: &'static str,
    id: Uuid,
    err: &AcError,
) {
    tracing::Span::current().record("status", "error");
    record_error(
        metric_operation,
        ErrorCategory::from(err).as_str(),
        err.status_code(),
    );
    record_credential_operation(credential_op, "error");
    tracing::warn!(
        target: "audit",
        event = audit_event,
        success = false,
        credential_id = %id,
        error_category = ErrorCategory::from(err).as_str(),
        "Client operation failed"
    );
}

fn client_detail(credential: ServiceCredential) -> ClientDetailResponse {
    ClientDetailResponse {
        id: credential.credential_id,
        client_id: credential.client_id,
        service_type: credential.service_type,
        region: credential.region,
        scopes: credential.scopes,
        is_active: credential.is_active,
        created_at: credential.created_at,
        updated_at: credential.updated_at,
    }
}

/// Update client scopes
///
/// PUT /api/v1/admin/clients/{id}
///
/// Narrowing only: every requested scope must belong to the client's
/// `ServiceType::default_scopes()` (ADR-0003 Component 2). Cannot change
/// client_id, the secret or `is_active` (a deactivated client stays
/// deactivated). Validation, the state change and the `service_scopes_updated`
/// audit row live in `registration_service::update_service_scopes`.
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.update_client", skip_all, fields(status))]
pub async fn handle_update_client(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crypto::Claims>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateClientRequest>,
) -> Result<Json<ClientDetailResponse>, AcError> {
    let result = match payload.scopes {
        Some(new_scopes) => {
            registration_service::update_service_scopes(&state.pool, id, new_scopes, &claims.sub)
                .await
        }
        // No updates requested: return the current credential.
        None => registration_service::get_service(&state.pool, id).await,
    };

    match result {
        Ok(credential) => {
            tracing::Span::current().record("status", "success");
            tracing::info!(
                target: "audit",
                event = "client_updated",
                success = true,
                credential_id = %id,
                "Client updated successfully"
            );
            record_credential_operation("update", "success");
            Ok(Json(client_detail(credential)))
        }
        Err(e) => {
            record_client_op_failure("update_client", "update", "client_updated", id, &e);
            Err(e)
        }
    }
}

/// Revoke (deactivate) client
///
/// DELETE /api/v1/admin/clients/{id}
///
/// Soft delete: sets `is_active = false`, after which token issuance refuses
/// the credential. Idempotent — a repeat returns 200 with no new audit row.
/// Response shape `{"deleted": true}` is kept. Tokens already issued remain
/// valid until `exp` (stateless JWTs, ADR-0007).
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.delete_client", skip_all, fields(status))]
pub async fn handle_delete_client(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crypto::Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AcError> {
    match registration_service::deactivate_service(&state.pool, id, &claims.sub).await {
        Ok(_) => {
            tracing::Span::current().record("status", "success");
            tracing::info!(
                target: "audit",
                event = "client_deleted",
                success = true,
                credential_id = %id,
                "Client deactivated successfully"
            );
            record_credential_operation("delete", "success");
            Ok(Json(serde_json::json!({ "deleted": true })))
        }
        Err(e) => {
            record_client_op_failure("delete_client", "delete", "client_deleted", id, &e);
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
/// A deactivated client is refused with 409 and no secret is generated.
/// Tokens already issued remain valid until `exp` (ADR-0007).
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.admin.rotate_client_secret", skip_all, fields(status))]
pub async fn handle_rotate_client_secret(
    State(state): State<Arc<AppState>>,
    Extension(claims): Extension<crypto::Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<RotateSecretResponse>, AcError> {
    let result = registration_service::rotate_service_secret(
        &state.pool,
        id,
        &claims.sub,
        state.config.bcrypt_cost,
    )
    .await;

    match result {
        Ok((credential, new_client_secret)) => {
            tracing::Span::current().record("status", "success");
            tracing::info!(
                target: "audit",
                event = "client_secret_rotated",
                success = true,
                credential_id = %id,
                client_id = %credential.client_id,
                "Client secret rotated successfully"
            );
            record_credential_operation("rotate_secret", "success");
            Ok(Json(RotateSecretResponse {
                client_id: credential.client_id,
                client_secret: new_client_secret, // ONLY returned here
            }))
        }
        Err(e) => {
            record_client_op_failure(
                "rotate_client_secret",
                "rotate_secret",
                "client_secret_rotated",
                id,
                &e,
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

    /// Claims the admin middleware would have inserted.
    fn admin_claims() -> Extension<crypto::Claims> {
        Extension(crypto::Claims::new(
            "admin-test".to_string(),
            0,
            0,
            "admin:services".to_string(),
            None,
        ))
    }

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

        // 400 with a fixed message: the caller's input is never echoed back.
        let err = result.unwrap_err();
        assert!(
            matches!(&err, AcError::BadRequest(msg)
                if *msg == registration_service::INVALID_SERVICE_TYPE_MESSAGE),
            "Expected the fixed invalid-service-type BadRequest, got: {:?}",
            err
        );
        assert_eq!(err.status_code(), 400);
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
        assert!(!response.client_secret.expose_secret().is_empty());
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
        assert_eq!(response.scopes.len(), 2); // media-handler has 2 default scopes (ADR-0003)
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
        assert!(!response.client_secret.expose_secret().is_empty());
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
            matches!(&err, AcError::BadRequest(msg)
                if *msg == registration_service::INVALID_SERVICE_TYPE_MESSAGE),
            "Expected the fixed invalid-service-type BadRequest, got: {:?}",
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
        // Narrow to a subset of the GC defaults (PUT may only narrow).
        let update_payload = UpdateClientRequest {
            scopes: Some(vec!["service.write.mc".to_string()]),
        };

        let result = handle_update_client(
            State(state),
            admin_claims(),
            Path(create_response.id),
            Json(update_payload),
        )
        .await;

        assert!(result.is_ok(), "Update should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.id, create_response.id);
        assert_eq!(response.scopes, vec!["service.write.mc".to_string()]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_update_client_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let update_payload = UpdateClientRequest {
            scopes: Some(vec!["scope1".to_string()]),
        };

        let result = handle_update_client(
            State(state),
            admin_claims(),
            Path(random_uuid),
            Json(update_payload),
        )
        .await;

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

        let result = handle_update_client(
            State(state),
            admin_claims(),
            Path(create_response.id),
            Json(update_payload),
        )
        .await;

        assert!(result.is_err(), "Should reject invalid scope format");
        let err = result.unwrap_err();
        assert!(
            matches!(
                &err,
                AcError::BadRequest("Scope contains invalid characters")
            ),
            "Expected BadRequest 'Scope contains invalid characters', got: {:?}",
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

        let result = handle_update_client(
            State(state),
            admin_claims(),
            Path(create_response.id),
            Json(update_payload),
        )
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

        // Seed through the create handler so a `service_registered` auth_events
        // row references the credential (the FK a hard delete used to violate).
        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let created = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0;

        let result =
            handle_delete_client(State(state.clone()), admin_claims(), Path(created.id)).await;

        assert!(result.is_ok(), "Delete should succeed: {:?}", result.err());
        let response = result.unwrap().0;
        assert_eq!(response.get("deleted"), Some(&serde_json::json!(true)));

        // Soft delete: the client is still readable, now inactive.
        let detail = handle_get_client(State(state), Path(created.id))
            .await
            .unwrap()
            .0;
        assert!(!detail.is_active, "Client should be deactivated");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_delete_client_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let result = handle_delete_client(State(state), admin_claims(), Path(random_uuid)).await;

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

        let payload = CreateClientRequest {
            service_type: "global-controller".to_string(),
            region: None,
        };
        let credential_id = handle_create_client(State(state.clone()), Json(payload))
            .await
            .unwrap()
            .0
            .id;

        // First delete
        let result1 =
            handle_delete_client(State(state.clone()), admin_claims(), Path(credential_id)).await;
        assert!(
            result1.is_ok(),
            "First delete should succeed: {:?}",
            result1.err()
        );

        // Second delete: idempotent revoke, still 200.
        let result2 = handle_delete_client(State(state), admin_claims(), Path(credential_id)).await;
        assert!(
            result2.is_ok(),
            "Second delete should succeed (idempotent): {:?}",
            result2.err()
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

        let original_secret = create_response.client_secret.expose_secret().to_string();

        // Rotate secret
        let result =
            handle_rotate_client_secret(State(state), admin_claims(), Path(create_response.id))
                .await;

        assert!(result.is_ok(), "Rotate should succeed");
        let response = result.unwrap().0;
        assert_eq!(response.client_id, create_response.client_id);
        assert!(!response.client_secret.expose_secret().is_empty());
        assert_ne!(
            response.client_secret.expose_secret(),
            &original_secret,
            "New secret should differ from original"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn test_handle_rotate_client_secret_not_found(pool: sqlx::PgPool) {
        let config = test_config();
        let state = Arc::new(AppState { pool, config });

        let random_uuid = Uuid::new_v4();
        let result =
            handle_rotate_client_secret(State(state), admin_claims(), Path(random_uuid)).await;

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
        let rotate_result = handle_rotate_client_secret(
            State(state.clone()),
            admin_claims(),
            Path(create_response.id),
        )
        .await;
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
