//! Current user handler.
//!
//! Returns information about the authenticated user/service from JWT claims.

use crate::auth::Claims;
use axum::{Extension, Json};
use serde::Serialize;
use tracing::instrument;

/// Response for `/v1/me` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct MeResponse {
    /// Subject (user or client ID).
    pub sub: String,

    /// Token scopes.
    pub scopes: Vec<String>,

    /// Service type (if service token).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_type: Option<String>,

    /// Token expiration timestamp.
    pub exp: i64,

    /// Token issued-at timestamp.
    pub iat: i64,
}

/// Handler for GET /v1/me
///
/// Returns the authenticated user's claims from the JWT.
/// Requires valid authentication via the auth middleware.
///
/// ## Response
///
/// Returns 200 OK with user claims:
///
/// ```json
/// {
///   "sub": "client_abc123",
///   "scopes": ["read", "write"],
///   "service_type": "global-controller",
///   "exp": 1234567890,
///   "iat": 1234567800
/// }
/// ```
#[instrument(skip_all, name = "gc.handlers.me")]
pub async fn get_me(Extension(claims): Extension<Claims>) -> Json<MeResponse> {
    tracing::debug!(target: "gc.handlers.me", "Returning user claims");

    let scopes = claims.scopes().iter().map(|s| s.to_string()).collect();

    Json(MeResponse {
        sub: claims.sub,
        scopes,
        service_type: claims.service_type,
        exp: claims.exp,
        iat: claims.iat,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_me_response_serialization() {
        let response = MeResponse {
            sub: "user123".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            service_type: Some("global-controller".to_string()),
            exp: 1234567890,
            iat: 1234567800,
        };

        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"sub\":\"user123\""));
        assert!(json.contains("\"scopes\":[\"read\",\"write\"]"));
        assert!(json.contains("\"service_type\":\"global-controller\""));
        assert!(json.contains("\"exp\":1234567890"));
        assert!(json.contains("\"iat\":1234567800"));
    }

    #[test]
    fn test_me_response_without_service_type() {
        let response = MeResponse {
            sub: "user123".to_string(),
            scopes: vec!["read".to_string()],
            service_type: None,
            exp: 1234567890,
            iat: 1234567800,
        };

        let json = serde_json::to_string(&response).unwrap();

        assert!(
            !json.contains("service_type"),
            "service_type should be omitted when None"
        );
    }
}
