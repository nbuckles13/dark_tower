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
    let response =
        registration_service::register_service(&state.pool, &payload.service_type, payload.region)
            .await?;

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
