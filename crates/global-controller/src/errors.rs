//! Global Controller error types.
//!
//! All errors map to appropriate HTTP status codes via the `IntoResponse` impl.
//! Error messages returned to clients are intentionally generic to avoid
//! leaking internal details. Actual errors are logged server-side.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Global Controller error type.
///
/// Maps to appropriate HTTP status codes:
/// - Database, Internal: 500 Internal Server Error
/// - InvalidToken: 401 Unauthorized
/// - NotFound: 404 Not Found
/// - Conflict: 409 Conflict
/// - RateLimitExceeded: 429 Too Many Requests
/// - Forbidden: 403 Forbidden
/// - BadRequest: 400 Bad Request
/// - ServiceUnavailable: 503 Service Unavailable
#[derive(Debug, Error)]
#[allow(dead_code)] // Variants will be used in Phase 2+
pub enum GcError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal server error")]
    Internal,
}

impl GcError {
    /// Returns the HTTP status code for this error (for metrics recording).
    #[allow(dead_code)] // Will be used for metrics in Phase 2+
    pub fn status_code(&self) -> u16 {
        match self {
            GcError::Database(_) | GcError::Internal => 500,
            GcError::InvalidToken(_) => 401,
            GcError::NotFound(_) => 404,
            GcError::Conflict(_) => 409,
            GcError::RateLimitExceeded => 429,
            GcError::Forbidden(_) => 403,
            GcError::BadRequest(_) => 400,
            GcError::ServiceUnavailable(_) => 503,
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
}

impl IntoResponse for GcError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            GcError::Database(err) => {
                // Log actual error server-side, return generic message to client
                tracing::error!(target: "gc.database", error = %err, "Database operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    "An internal database error occurred".to_string(),
                )
            }
            GcError::InvalidToken(reason) => {
                (StatusCode::UNAUTHORIZED, "INVALID_TOKEN", reason.clone())
            }
            GcError::NotFound(resource) => (StatusCode::NOT_FOUND, "NOT_FOUND", resource.clone()),
            GcError::Conflict(reason) => (StatusCode::CONFLICT, "CONFLICT", reason.clone()),
            GcError::RateLimitExceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMIT_EXCEEDED",
                "Too many requests. Please try again later.".to_string(),
            ),
            GcError::Forbidden(reason) => (StatusCode::FORBIDDEN, "FORBIDDEN", reason.clone()),
            GcError::BadRequest(reason) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", reason.clone()),
            GcError::ServiceUnavailable(reason) => {
                // Log actual reason server-side
                tracing::warn!(target: "gc.availability", reason = %reason, "Service unavailable");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "SERVICE_UNAVAILABLE",
                    "Service temporarily unavailable".to_string(),
                )
            }
            GcError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "An internal error occurred".to_string(),
            ),
        };

        let error_response = ErrorResponse {
            error: ErrorDetail {
                code: code.to_string(),
                message,
            },
        };

        let mut response = (status, Json(error_response)).into_response();

        // Add WWW-Authenticate header for 401 responses
        if status == StatusCode::UNAUTHORIZED {
            if let Ok(header_value) =
                "Bearer realm=\"dark-tower-api\", error=\"invalid_token\"".parse()
            {
                response
                    .headers_mut()
                    .insert("WWW-Authenticate", header_value);
            }
        }

        response
    }
}

/// Convert sqlx errors to GcError
impl From<sqlx::Error> for GcError {
    fn from(err: sqlx::Error) -> Self {
        GcError::Database(err.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;

    // Helper function to read the response body as JSON
    async fn read_body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn test_display_database_error() {
        let error = GcError::Database("connection failed".to_string());
        assert_eq!(format!("{}", error), "Database error: connection failed");
    }

    #[test]
    fn test_display_invalid_token() {
        let error = GcError::InvalidToken("expired".to_string());
        assert_eq!(format!("{}", error), "Invalid token: expired");
    }

    #[test]
    fn test_display_not_found() {
        let error = GcError::NotFound("meeting".to_string());
        assert_eq!(format!("{}", error), "Not found: meeting");
    }

    #[test]
    fn test_display_conflict() {
        let error = GcError::Conflict("resource already exists".to_string());
        assert_eq!(format!("{}", error), "Conflict: resource already exists");
    }

    #[test]
    fn test_display_rate_limit() {
        let error = GcError::RateLimitExceeded;
        assert_eq!(format!("{}", error), "Rate limit exceeded");
    }

    #[test]
    fn test_display_forbidden() {
        let error = GcError::Forbidden("insufficient permissions".to_string());
        assert_eq!(format!("{}", error), "Forbidden: insufficient permissions");
    }

    #[test]
    fn test_display_bad_request() {
        let error = GcError::BadRequest("invalid input".to_string());
        assert_eq!(format!("{}", error), "Bad request: invalid input");
    }

    #[test]
    fn test_display_service_unavailable() {
        let error = GcError::ServiceUnavailable("database down".to_string());
        assert_eq!(format!("{}", error), "Service unavailable: database down");
    }

    #[test]
    fn test_display_internal() {
        let error = GcError::Internal;
        assert_eq!(format!("{}", error), "Internal server error");
    }

    #[test]
    fn test_status_codes() {
        assert_eq!(GcError::Database("test".to_string()).status_code(), 500);
        assert_eq!(GcError::InvalidToken("test".to_string()).status_code(), 401);
        assert_eq!(GcError::NotFound("test".to_string()).status_code(), 404);
        assert_eq!(GcError::Conflict("test".to_string()).status_code(), 409);
        assert_eq!(GcError::RateLimitExceeded.status_code(), 429);
        assert_eq!(GcError::Forbidden("test".to_string()).status_code(), 403);
        assert_eq!(GcError::BadRequest("test".to_string()).status_code(), 400);
        assert_eq!(
            GcError::ServiceUnavailable("test".to_string()).status_code(),
            503
        );
        assert_eq!(GcError::Internal.status_code(), 500);
    }

    #[tokio::test]
    async fn test_into_response_database_error() {
        let error = GcError::Database("connection failed".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "DATABASE_ERROR");
        assert_eq!(
            body_json["error"]["message"],
            "An internal database error occurred"
        );
    }

    #[tokio::test]
    async fn test_into_response_invalid_token() {
        let error = GcError::InvalidToken("token expired".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Check WWW-Authenticate header
        let www_auth = response.headers().get("WWW-Authenticate");
        assert!(www_auth.is_some());
        let www_auth_str = www_auth.unwrap().to_str().unwrap();
        assert!(www_auth_str.contains("Bearer realm=\"dark-tower-api\""));

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INVALID_TOKEN");
        assert_eq!(body_json["error"]["message"], "token expired");
    }

    #[tokio::test]
    async fn test_into_response_not_found() {
        let error = GcError::NotFound("Meeting not found".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "NOT_FOUND");
        assert_eq!(body_json["error"]["message"], "Meeting not found");
    }

    #[tokio::test]
    async fn test_into_response_conflict() {
        let error = GcError::Conflict("Meeting already exists".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "CONFLICT");
        assert_eq!(body_json["error"]["message"], "Meeting already exists");
    }

    #[tokio::test]
    async fn test_into_response_rate_limit() {
        let error = GcError::RateLimitExceeded;
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "RATE_LIMIT_EXCEEDED");
    }

    #[tokio::test]
    async fn test_into_response_forbidden() {
        let error = GcError::Forbidden("Access denied".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "FORBIDDEN");
        assert_eq!(body_json["error"]["message"], "Access denied");
    }

    #[tokio::test]
    async fn test_into_response_bad_request() {
        let error = GcError::BadRequest("Invalid meeting ID format".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "BAD_REQUEST");
        assert_eq!(body_json["error"]["message"], "Invalid meeting ID format");
    }

    #[tokio::test]
    async fn test_into_response_service_unavailable() {
        let error = GcError::ServiceUnavailable("database maintenance".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "SERVICE_UNAVAILABLE");
        // Generic message returned to client
        assert_eq!(
            body_json["error"]["message"],
            "Service temporarily unavailable"
        );
    }

    #[tokio::test]
    async fn test_into_response_internal() {
        let error = GcError::Internal;
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(body_json["error"]["message"], "An internal error occurred");
    }
}
