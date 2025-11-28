use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AcError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Insufficient scope: required {required}, provided {provided:?}")]
    InsufficientScope {
        required: String,
        provided: Vec<String>,
    },

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Internal server error")]
    Internal,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    required_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provided_scopes: Option<Vec<String>>,
}

impl IntoResponse for AcError {
    fn into_response(self) -> Response {
        let (status, code, message, required_scope, provided_scopes) = match &self {
            AcError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "An internal database error occurred".to_string(),
                None,
                None,
            ),
            AcError::Crypto(err) => {
                // Log the actual error server-side, but don't expose to client
                tracing::error!(target: "crypto", error = %err, "Cryptographic operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "CRYPTO_ERROR",
                    "An internal error occurred".to_string(),
                    None,
                    None,
                )
            }
            AcError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "Invalid client credentials".to_string(),
                None,
                None,
            ),
            AcError::InsufficientScope { required, provided } => (
                StatusCode::FORBIDDEN,
                "INSUFFICIENT_SCOPE",
                format!("Requires scope: {}", required),
                Some(required.clone()),
                Some(provided.clone()),
            ),
            AcError::InvalidToken(reason) => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                reason.clone(),
                None,
                None,
            ),
            AcError::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMIT_EXCEEDED",
                "Too many requests. Please try again later.".to_string(),
                None,
                None,
            ),
            AcError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "An internal error occurred".to_string(),
                None,
                None,
            ),
        };

        let error_response = ErrorResponse {
            error: ErrorDetail {
                code: code.to_string(),
                message,
                required_scope: required_scope.clone(),
                provided_scopes: provided_scopes.clone(),
            },
        };

        let mut response = (status, Json(error_response)).into_response();

        // Add WWW-Authenticate header for 401/403 responses
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let realm = "dark-tower-api";
            let www_auth_value = if let Some(req_scope) = required_scope {
                format!(
                    "Bearer realm=\"{}\", error=\"insufficient_scope\", error_description=\"Requires scope: {}\"",
                    realm, req_scope
                )
            } else {
                format!("Bearer realm=\"{}\", error=\"invalid_token\"", realm)
            };

            if let Ok(header_value) = www_auth_value.parse() {
                response
                    .headers_mut()
                    .insert("WWW-Authenticate", header_value);
            }
        }

        response
    }
}
