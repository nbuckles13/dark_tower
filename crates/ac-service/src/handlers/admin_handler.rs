use crate::errors::AcError;
use crate::models::RegisterServiceResponse;
use crate::services::registration_service;
use axum::{extract::State, Json};
use serde::Deserialize;
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
    let response = registration_service::register_service(
        &state.pool,
        &payload.service_type,
        payload.region,
    )
    .await?;

    Ok(Json(response))
}
