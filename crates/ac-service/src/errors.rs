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

    #[error("Too many requests. Retry after {retry_after_seconds} seconds: {message}")]
    TooManyRequests {
        retry_after_seconds: i64,
        message: String,
    },

    #[error("Internal server error")]
    Internal,
}

impl AcError {
    /// Returns the HTTP status code for this error (ADR-0011: for metrics recording)
    pub fn status_code(&self) -> u16 {
        match self {
            AcError::Database(_) | AcError::Crypto(_) | AcError::Internal => 500,
            AcError::InvalidCredentials | AcError::InvalidToken(_) => 401,
            AcError::InsufficientScope { .. } => 403,
            AcError::RateLimitExceeded | AcError::TooManyRequests { .. } => 429,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_seconds: Option<i64>,
}

impl IntoResponse for AcError {
    fn into_response(self) -> Response {
        let (status, code, message, required_scope, provided_scopes, retry_after) = match &self {
            AcError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "An internal database error occurred".to_string(),
                None,
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
                    None,
                )
            }
            AcError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "Invalid client credentials".to_string(),
                None,
                None,
                None,
            ),
            AcError::InsufficientScope { required, provided } => (
                StatusCode::FORBIDDEN,
                "INSUFFICIENT_SCOPE",
                format!("Requires scope: {}", required),
                Some(required.clone()),
                Some(provided.clone()),
                None,
            ),
            AcError::InvalidToken(reason) => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                reason.clone(),
                None,
                None,
                None,
            ),
            AcError::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMIT_EXCEEDED",
                "Too many requests. Please try again later.".to_string(),
                None,
                None,
                None,
            ),
            AcError::TooManyRequests {
                retry_after_seconds,
                message,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
                message.clone(),
                None,
                None,
                Some(*retry_after_seconds),
            ),
            AcError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "An internal error occurred".to_string(),
                None,
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
                retry_after_seconds: retry_after,
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

        // Add Retry-After header for 429 responses
        if let Some(retry_after_secs) = retry_after {
            if let Ok(header_value) = retry_after_secs.to_string().parse() {
                response.headers_mut().insert("Retry-After", header_value);
            }
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;

    // Helper function to read the response body as JSON
    async fn read_body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    // Display tests
    #[test]
    fn test_display_database_error() {
        let error = AcError::Database("connection failed".to_string());
        assert_eq!(format!("{}", error), "Database error: connection failed");
    }

    #[test]
    fn test_display_crypto_error() {
        let error = AcError::Crypto("key generation failed".to_string());
        assert_eq!(
            format!("{}", error),
            "Cryptographic error: key generation failed"
        );
    }

    #[test]
    fn test_display_invalid_credentials() {
        let error = AcError::InvalidCredentials;
        assert_eq!(format!("{}", error), "Invalid credentials");
    }

    #[test]
    fn test_display_insufficient_scope() {
        let error = AcError::InsufficientScope {
            required: "admin".to_string(),
            provided: vec!["read".to_string(), "write".to_string()],
        };
        assert_eq!(
            format!("{}", error),
            "Insufficient scope: required admin, provided [\"read\", \"write\"]"
        );
    }

    #[test]
    fn test_display_invalid_token() {
        let error = AcError::InvalidToken("expired".to_string());
        assert_eq!(format!("{}", error), "Invalid token: expired");
    }

    #[test]
    fn test_display_rate_limit_exceeded() {
        let error = AcError::RateLimitExceeded;
        assert_eq!(format!("{}", error), "Rate limit exceeded");
    }

    #[test]
    fn test_display_internal() {
        let error = AcError::Internal;
        assert_eq!(format!("{}", error), "Internal server error");
    }

    // IntoResponse tests
    #[tokio::test]
    async fn test_into_response_database_error() {
        let error = AcError::Database("connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "DATABASE_ERROR");
        assert_eq!(
            body_json["error"]["message"],
            "An internal database error occurred"
        );
        assert!(body_json["error"]["required_scope"].is_null());
        assert!(body_json["error"]["provided_scopes"].is_null());
    }

    #[tokio::test]
    async fn test_into_response_crypto_error() {
        let error = AcError::Crypto("key generation failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "CRYPTO_ERROR");
        assert_eq!(body_json["error"]["message"], "An internal error occurred");
        assert!(body_json["error"]["required_scope"].is_null());
        assert!(body_json["error"]["provided_scopes"].is_null());
    }

    #[tokio::test]
    async fn test_into_response_invalid_credentials() {
        let error = AcError::InvalidCredentials;
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Check WWW-Authenticate header before consuming the body
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_some());
        let www_auth_str = www_auth.unwrap().to_str().unwrap();
        assert!(www_auth_str.contains("Bearer realm=\"dark-tower-api\""));
        assert!(www_auth_str.contains("error=\"invalid_token\""));

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INVALID_CREDENTIALS");
        assert_eq!(body_json["error"]["message"], "Invalid client credentials");
    }

    #[tokio::test]
    async fn test_into_response_insufficient_scope() {
        let error = AcError::InsufficientScope {
            required: "admin".to_string(),
            provided: vec!["read".to_string(), "write".to_string()],
        };
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        // Check WWW-Authenticate header before consuming the body
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_some());
        let www_auth_str = www_auth.unwrap().to_str().unwrap();
        assert!(www_auth_str.contains("Bearer realm=\"dark-tower-api\""));
        assert!(www_auth_str.contains("error=\"insufficient_scope\""));
        assert!(www_auth_str.contains("Requires scope: admin"));

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INSUFFICIENT_SCOPE");
        assert_eq!(body_json["error"]["message"], "Requires scope: admin");
        assert_eq!(body_json["error"]["required_scope"], "admin");
        assert_eq!(
            body_json["error"]["provided_scopes"],
            serde_json::json!(["read", "write"])
        );
    }

    #[tokio::test]
    async fn test_into_response_invalid_token() {
        let error = AcError::InvalidToken("token expired".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Check WWW-Authenticate header before consuming the body
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_some());
        let www_auth_str = www_auth.unwrap().to_str().unwrap();
        assert!(www_auth_str.contains("Bearer realm=\"dark-tower-api\""));
        assert!(www_auth_str.contains("error=\"invalid_token\""));

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INVALID_TOKEN");
        assert_eq!(body_json["error"]["message"], "token expired");
    }

    #[tokio::test]
    async fn test_into_response_rate_limit_exceeded() {
        let error = AcError::RateLimitExceeded;
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        // Rate limit errors should not have WWW-Authenticate header
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_none());

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "RATE_LIMIT_EXCEEDED");
        assert_eq!(
            body_json["error"]["message"],
            "Too many requests. Please try again later."
        );
    }

    #[tokio::test]
    async fn test_into_response_internal() {
        let error = AcError::Internal;
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Internal errors should not have WWW-Authenticate header
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_none());

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(body_json["error"]["message"], "An internal error occurred");
    }
}
