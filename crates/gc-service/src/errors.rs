//! Global Controller error types.
//!
//! All errors map to appropriate HTTP status codes via the `IntoResponse` impl.
//! Error messages returned to clients are intentionally generic to avoid
//! leaking internal details. Actual errors are logged server-side.

use crate::repositories::MeetingRefusal;
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
pub enum GcError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Not found: {0}")]
    NotFound(String),

    /// A resource conflict (409).
    // No constructor yet — the only variant of this enum without one. Scoped here
    // rather than the enum-wide `#[allow(dead_code)]` this replaces, which masked
    // all fifteen. `#[expect]` is unusable: `main.rs` re-declares these modules
    // privately, so the lint fires for the bin target but not the lib, where the
    // enum is `pub` and never dead — an `#[expect]` would be unfulfilled in the
    // lib build and warn there instead.
    #[allow(
        dead_code,
        reason = "Conflict has no constructor; all other variants are live"
    )]
    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    /// The organization is at its concurrent-meeting cap (403).
    ///
    /// Unit variant deliberately: `IntoResponse` echoes the payload of
    /// `String`-carrying variants verbatim to the client, so carrying one here
    /// would put an org identifier, cap value or live meeting count one careless
    /// call site away from the wire. The message is fixed.
    #[error("Organization meeting limit exceeded")]
    OrgMeetingLimitExceeded,

    /// The organization row exists with `is_active = false` (403).
    ///
    /// Unit variant for the same reason as [`GcError::OrgMeetingLimitExceeded`].
    #[error("Organization is not active")]
    OrgInactive,

    /// A valid token names an organization with no row (500).
    ///
    /// Distinct from [`GcError::Internal`] because this endpoint already emits
    /// `INTERNAL_ERROR` for RNG failure and meeting-code-collision exhaustion:
    /// reusing it would leave the cause indistinguishable on the wire, which is
    /// the defect story R-6 exists to remove. Client-visible message stays
    /// generic; the organization identifier goes to the log only.
    #[error("Organization is not provisioned")]
    OrgNotProvisioned,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("Unsupported media type: {0}")]
    UnsupportedMediaType(String),

    #[error("Bad gateway: {0}")]
    BadGateway(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl GcError {
    /// Returns the HTTP status code for this error (for metrics recording).
    pub fn status_code(&self) -> u16 {
        match self {
            GcError::Database(_) | GcError::Internal(_) => 500,
            GcError::InvalidToken(_) => 401,
            GcError::NotFound(_) => 404,
            GcError::Conflict(_) => 409,
            GcError::RateLimitExceeded => 429,
            GcError::Forbidden(_) | GcError::OrgMeetingLimitExceeded | GcError::OrgInactive => 403,
            GcError::OrgNotProvisioned => 500,
            GcError::BadRequest(_) => 400,
            GcError::ServiceUnavailable(_) => 503,
            GcError::PayloadTooLarge(_) => 413,
            GcError::UnsupportedMediaType(_) => 415,
            GcError::BadGateway(_) => 502,
        }
    }

    /// Returns a bounded label string for the error variant (for metrics).
    ///
    /// Uses enum variant names, not error message content.
    pub fn error_type_label(&self) -> &'static str {
        match self {
            GcError::Database(_) => "database",
            GcError::InvalidToken(_) => "invalid_token",
            GcError::NotFound(_) => "not_found",
            GcError::Conflict(_) => "conflict",
            GcError::RateLimitExceeded => "rate_limit",
            GcError::Forbidden(_) => "forbidden",
            // These three intentionally equal `MeetingRefusal::metric_label()`
            // so `gc_meeting_creation_failures_total` and the HTTP error metrics
            // correlate mechanically for whoever is on call.
            GcError::OrgMeetingLimitExceeded => "org_limit",
            GcError::OrgInactive => "org_inactive",
            GcError::OrgNotProvisioned => "org_not_provisioned",
            GcError::BadRequest(_) => "bad_request",
            GcError::ServiceUnavailable(_) => "service_unavailable",
            GcError::PayloadTooLarge(_) => "payload_too_large",
            GcError::UnsupportedMediaType(_) => "unsupported_media_type",
            GcError::BadGateway(_) => "bad_gateway",
            GcError::Internal(_) => "internal",
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
            GcError::OrgMeetingLimitExceeded => (
                StatusCode::FORBIDDEN,
                "ORGANIZATION_MEETING_LIMIT_EXCEEDED",
                "Organization meeting limit exceeded".to_string(),
            ),
            GcError::OrgInactive => (
                StatusCode::FORBIDDEN,
                "ORGANIZATION_INACTIVE",
                "Organization is not active".to_string(),
            ),
            // Deliberately does NOT log here. Unlike `Database`/`Internal`, this
            // is a unit variant carrying no detail to preserve, and the handler
            // already emits an `error!` with `org_id` and `user_id`. A second
            // line here would add no context while double-counting the event in
            // any log-derived error rate.
            GcError::OrgNotProvisioned => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "ORGANIZATION_NOT_PROVISIONED",
                "An internal error occurred".to_string(),
            ),
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
            GcError::PayloadTooLarge(reason) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "PAYLOAD_TOO_LARGE",
                reason.clone(),
            ),
            GcError::UnsupportedMediaType(reason) => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "UNSUPPORTED_MEDIA_TYPE",
                reason.clone(),
            ),
            GcError::BadGateway(reason) => {
                // Log actual reason server-side (e.g. collector status/URL), return
                // generic message to client. Mirrors ServiceUnavailable: never echo
                // upstream collector details to the caller.
                tracing::warn!(target: "gc.telemetry", reason = %reason, "Telemetry collector unreachable");
                (
                    StatusCode::BAD_GATEWAY,
                    "BAD_GATEWAY",
                    "Telemetry collector is unavailable".to_string(),
                )
            }
            GcError::Internal(reason) => {
                // Log actual reason server-side, return generic message to client
                tracing::error!(target: "gc.internal", reason = %reason, "Internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred".to_string(),
                )
            }
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

/// Map a repository-level refusal cause to its HTTP-level error.
///
/// The single place the two taxonomies meet. Exhaustive, so adding a fourth
/// refusal cause is a compile error here rather than a silently-defaulted
/// status code.
impl From<MeetingRefusal> for GcError {
    fn from(refusal: MeetingRefusal) -> Self {
        match refusal {
            MeetingRefusal::OrganizationNotProvisioned => GcError::OrgNotProvisioned,
            MeetingRefusal::OrganizationInactive => GcError::OrgInactive,
            MeetingRefusal::CapacityExhausted => GcError::OrgMeetingLimitExceeded,
        }
    }
}

/// Convert sqlx errors to GcError
impl From<sqlx::Error> for GcError {
    fn from(err: sqlx::Error) -> Self {
        GcError::Database(err.to_string())
    }
}

/// Convert common JWT errors to GcError
impl From<common::jwt::JwtError> for GcError {
    fn from(err: common::jwt::JwtError) -> Self {
        match err {
            common::jwt::JwtError::ServiceUnavailable(msg) => GcError::ServiceUnavailable(msg),
            _ => GcError::InvalidToken("The access token is invalid or expired".to_string()),
        }
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
        let error = GcError::Internal("test reason".to_string());
        assert_eq!(format!("{}", error), "Internal server error: test reason");
    }

    /// One representative value per `GcError` variant.
    ///
    /// **What the `match` below does and does not enforce.** Adding a variant to
    /// `GcError` is a compile error here until someone adds an arm — so a new
    /// variant cannot be introduced without a maintainer visiting this function.
    /// It does **not** enforce that the variant was also added to `all`: the
    /// `match` only ever sees values already in the list, so handling the arm
    /// while forgetting the list leaves the uniqueness test silently covering
    /// N-1 of N variants. Rust cannot close that gap without a derive
    /// (`strum` is not in the workspace; `std::mem::variant_count` is nightly),
    /// so **adding the value to `all` is on the author.**
    ///
    /// Stated precisely because an overclaiming comment on *this* test would be
    /// worse than none — it is the anti-re-collapse guard for the whole change,
    /// so it is what a future reader trusts instead of checking (@test, T-9
    /// follow-up). Still stronger than the hand-enumerated house style at the
    /// `test_status_codes` / `test_error_type_labels` pair below, which can be
    /// narrowed with no signal at all.
    fn one_of_each_variant() -> Vec<GcError> {
        let all = vec![
            GcError::Database("t".into()),
            GcError::InvalidToken("t".into()),
            GcError::NotFound("t".into()),
            GcError::Conflict("t".into()),
            GcError::RateLimitExceeded,
            GcError::Forbidden("t".into()),
            GcError::OrgMeetingLimitExceeded,
            GcError::OrgInactive,
            GcError::OrgNotProvisioned,
            GcError::BadRequest("t".into()),
            GcError::ServiceUnavailable("t".into()),
            GcError::PayloadTooLarge("t".into()),
            GcError::UnsupportedMediaType("t".into()),
            GcError::BadGateway("t".into()),
            GcError::Internal("t".into()),
        ];

        // Duplicate guard only; see the doc comment above for what this match
        // does and does not enforce.
        let mut seen = std::collections::BTreeSet::new();
        for err in &all {
            let discriminant = match err {
                GcError::Database(_) => "Database",
                GcError::InvalidToken(_) => "InvalidToken",
                GcError::NotFound(_) => "NotFound",
                GcError::Conflict(_) => "Conflict",
                GcError::RateLimitExceeded => "RateLimitExceeded",
                GcError::Forbidden(_) => "Forbidden",
                GcError::OrgMeetingLimitExceeded => "OrgMeetingLimitExceeded",
                GcError::OrgInactive => "OrgInactive",
                GcError::OrgNotProvisioned => "OrgNotProvisioned",
                GcError::BadRequest(_) => "BadRequest",
                GcError::ServiceUnavailable(_) => "ServiceUnavailable",
                GcError::PayloadTooLarge(_) => "PayloadTooLarge",
                GcError::UnsupportedMediaType(_) => "UnsupportedMediaType",
                GcError::BadGateway(_) => "BadGateway",
                GcError::Internal(_) => "Internal",
            };
            assert!(
                seen.insert(discriminant),
                "{discriminant} listed twice in one_of_each_variant"
            );
        }

        all
    }

    /// Every `GcError` variant renders a distinct envelope `code`.
    ///
    /// This is the anti-re-collapse guard for story R-6: the three refusal
    /// causes are told apart by (status, code), and `POST /api/v1/meetings`
    /// already emitted `INTERNAL_ERROR` from two unrelated paths before this
    /// change. A future variant sharing a code would silently re-merge causes
    /// that this work separated.
    #[tokio::test]
    async fn every_variant_has_a_distinct_error_code() {
        let mut codes: std::collections::BTreeMap<String, &'static str> =
            std::collections::BTreeMap::new();

        for err in one_of_each_variant() {
            let label = err.error_type_label();
            let response = err.into_response();
            let body_json = read_body_json(response.into_body()).await;
            let code = body_json["error"]["code"].as_str().unwrap().to_string();

            let previous = codes.get(&code).copied().unwrap_or_default();
            assert!(
                previous.is_empty(),
                "error code {code} is emitted by both {previous} and {label} — \
                 two variants sharing a code silently re-merge causes"
            );
            codes.insert(code, label);
        }
    }

    /// The three refusal causes are pairwise distinct at the (status, code)
    /// level — the tuple the story runner and task #3 branch on.
    #[tokio::test]
    async fn refusal_causes_are_pairwise_distinct() {
        let mut seen = std::collections::BTreeSet::new();

        for err in [
            GcError::OrgMeetingLimitExceeded,
            GcError::OrgInactive,
            GcError::OrgNotProvisioned,
        ] {
            let status = err.status_code();
            let response = err.into_response();
            let body_json = read_body_json(response.into_body()).await;
            let code = body_json["error"]["code"].as_str().unwrap().to_string();
            assert!(
                seen.insert((status, code.clone())),
                "({status}, {code}) is not distinct across the three refusal causes"
            );
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn refusal_maps_to_its_error_variant() {
        use crate::repositories::meetings::MeetingRefusal;

        // Label agreement between the two taxonomies is load-bearing: the
        // creation-failure metric and the HTTP error metric must correlate.
        for (refusal, expected_status) in [
            (MeetingRefusal::CapacityExhausted, 403),
            (MeetingRefusal::OrganizationInactive, 403),
            (MeetingRefusal::OrganizationNotProvisioned, 500),
        ] {
            let err: GcError = refusal.into();
            assert_eq!(err.status_code(), expected_status);
            assert_eq!(err.error_type_label(), refusal.metric_label());
        }
    }

    #[tokio::test]
    async fn org_not_provisioned_body_carries_no_identifier() {
        let response = GcError::OrgNotProvisioned.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "ORGANIZATION_NOT_PROVISIONED");
        // Generic message: the org identifier is a log field, never a body field.
        assert_eq!(body_json["error"]["message"], "An internal error occurred");
    }

    #[tokio::test]
    async fn org_inactive_response_shape() {
        let response = GcError::OrgInactive.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "ORGANIZATION_INACTIVE");
        assert_eq!(body_json["error"]["message"], "Organization is not active");
    }

    #[tokio::test]
    async fn org_meeting_limit_exceeded_response_shape() {
        let response = GcError::OrgMeetingLimitExceeded.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(
            body_json["error"]["code"],
            "ORGANIZATION_MEETING_LIMIT_EXCEEDED"
        );
        assert_eq!(
            body_json["error"]["message"],
            "Organization meeting limit exceeded"
        );
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
        assert_eq!(
            GcError::PayloadTooLarge("test".to_string()).status_code(),
            413
        );
        assert_eq!(
            GcError::UnsupportedMediaType("test".to_string()).status_code(),
            415
        );
        assert_eq!(GcError::BadGateway("test".to_string()).status_code(), 502);
        assert_eq!(GcError::Internal("test".to_string()).status_code(), 500);
    }

    #[test]
    fn test_error_type_labels() {
        assert_eq!(GcError::Database("t".into()).error_type_label(), "database");
        assert_eq!(
            GcError::InvalidToken("t".into()).error_type_label(),
            "invalid_token"
        );
        assert_eq!(
            GcError::NotFound("t".into()).error_type_label(),
            "not_found"
        );
        assert_eq!(GcError::Conflict("t".into()).error_type_label(), "conflict");
        assert_eq!(GcError::RateLimitExceeded.error_type_label(), "rate_limit");
        assert_eq!(
            GcError::Forbidden("t".into()).error_type_label(),
            "forbidden"
        );
        assert_eq!(
            GcError::BadRequest("t".into()).error_type_label(),
            "bad_request"
        );
        assert_eq!(
            GcError::ServiceUnavailable("t".into()).error_type_label(),
            "service_unavailable"
        );
        assert_eq!(
            GcError::PayloadTooLarge("t".into()).error_type_label(),
            "payload_too_large"
        );
        assert_eq!(
            GcError::UnsupportedMediaType("t".into()).error_type_label(),
            "unsupported_media_type"
        );
        assert_eq!(
            GcError::BadGateway("t".into()).error_type_label(),
            "bad_gateway"
        );
        assert_eq!(GcError::Internal("t".into()).error_type_label(), "internal");
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
        let error = GcError::Internal("test reason".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "INTERNAL_ERROR");
        assert_eq!(body_json["error"]["message"], "An internal error occurred");
    }

    #[tokio::test]
    async fn test_into_response_payload_too_large() {
        let error = GcError::PayloadTooLarge("Payload exceeds 256 KiB limit".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "PAYLOAD_TOO_LARGE");
        assert_eq!(
            body_json["error"]["message"],
            "Payload exceeds 256 KiB limit"
        );
    }

    #[tokio::test]
    async fn test_into_response_unsupported_media_type() {
        let error = GcError::UnsupportedMediaType(
            "Content-Type must be application/x-protobuf".to_string(),
        );
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "UNSUPPORTED_MEDIA_TYPE");
        assert_eq!(
            body_json["error"]["message"],
            "Content-Type must be application/x-protobuf"
        );
    }

    #[tokio::test]
    async fn test_into_response_bad_gateway() {
        // Collector detail logged server-side; generic message returned to client.
        let error = GcError::BadGateway("collector returned 503".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);

        let body_json = read_body_json(response.into_body()).await;
        assert_eq!(body_json["error"]["code"], "BAD_GATEWAY");
        // Generic message — collector detail not echoed.
        assert_eq!(
            body_json["error"]["message"],
            "Telemetry collector is unavailable"
        );
        assert!(!body_json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("503"));
    }
}
