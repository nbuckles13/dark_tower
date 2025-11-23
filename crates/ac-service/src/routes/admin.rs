use axum::{
    Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct RegisterServiceRequest {
    pub service_type: String,
    pub region: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterServiceResponse {
    pub client_id: String,
    pub client_secret: String,
    pub service_type: String,
    pub scopes: Vec<String>,
}

/// Service registration endpoint
/// POST /api/v1/admin/services/register
///
/// TODO Phase 3: Implement service registration
/// - Generate client_id (UUID)
/// - Generate client_secret using CSPRNG (ring::rand::SystemRandom)
/// - Hash client_secret with bcrypt (cost factor 12+)
/// - Assign scopes based on service_type
/// - Store credentials in database
/// - Log registration event
/// - Return plaintext client_secret (only time it's visible)
pub async fn register_service(
    Json(_payload): Json<RegisterServiceRequest>,
) -> Result<Json<RegisterServiceResponse>, StatusCode> {
    // Placeholder implementation
    todo!("Phase 3: Implement service registration")
}
