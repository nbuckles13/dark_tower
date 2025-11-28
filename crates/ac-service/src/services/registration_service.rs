use crate::crypto;
use crate::errors::AcError;
use crate::models::{AuthEventType, RegisterServiceResponse, ServiceType};
use crate::repositories::{auth_events, service_credentials};
use sqlx::PgPool;
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
    let svc_type = ServiceType::from_str(service_type)
        .ok_or_else(|| {
            AcError::Database(format!(
                "Invalid service_type: '{}'. Must be one of: global-controller, meeting-controller, media-handler",
                service_type
            ))
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
    .await {
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
    .await {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}

/// Deactivate a service credential
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
    .await {
        tracing::warn!("Failed to log auth event: {}", e);
    }

    Ok(())
}
