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
