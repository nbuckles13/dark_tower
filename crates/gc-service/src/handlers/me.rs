//! Current user handler.
//!
//! Returns information about the authenticated user/service from JWT claims.
//!
//! **TEMPORARY**: This endpoint exists for Phase 2 auth middleware testing.
//! Remove when real GC endpoints (meeting management, etc.) are implemented.

use crate::auth::Claims;
use axum::{Extension, Json};
use serde::Serialize;
use tracing::instrument;

/// Response for `/v1/me` endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
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
///   "serviceType": "global-controller",
///   "exp": 1234567890,
///   "iat": 1234567800
/// }
/// ```
#[instrument(
    skip_all,
    name = "gc.handlers.me",
    fields(
        method = "GET",
        endpoint = "/api/v1/me",
        status = tracing::field::Empty,
    )
)]
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
        assert!(json.contains("\"serviceType\":\"global-controller\""));
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
            !json.contains("serviceType"),
            "serviceType should be omitted when None"
        );
    }

    /// WIRE-SHAPE LOCK — `MeResponse` (R-53 / task #23; GC mirror of AC `f5fc4b4`).
    ///
    /// RENAME TRIPWIRE. `MeResponse` is `#[serde(rename_all = "camelCase")]` (only
    /// `service_type` -> `serviceType` is multi-word; `sub`/`scopes`/`exp`/`iat` are
    /// scheme-invariant). `/me` is not in task #49's failing-test surface, but it is
    /// exactly the kind of wire surface a blanket rename sweep could silently flip, so
    /// it gets a struct-level lock too. Asserts the EXACT key-set + "no key contains
    /// `_`". `service_type` is `skip_serializing_if = Option::is_none`, so populate it
    /// `Some(..)` to lock the FULL key-set present. Complements the substring
    /// serialization tests above with closed-set key equality.
    ///
    /// IF THIS FAILS DURING A RENAME SWEEP: the SDK + every `/me` client depend on
    /// these camelCase keys (R-53). DO NOT silently re-baseline — the all-camelCase
    /// rule is owned by task #51.
    #[test]
    fn test_me_response_wire_shape_stays_camel() {
        let response = MeResponse {
            sub: "user123".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            service_type: Some("global-controller".to_string()),
            exp: 1234567890,
            iat: 1234567800,
        };

        let value = serde_json::to_value(&response).expect("should serialize");
        let keys: std::collections::BTreeSet<String> = value
            .as_object()
            .expect("MeResponse wire shape must be a JSON object")
            .keys()
            .cloned()
            .collect();

        let expected: std::collections::BTreeSet<String> = [
            "sub",         // scheme-invariant single word
            "scopes",      // scheme-invariant single word
            "serviceType", // camelCase (R-53) — NOT `service_type`
            "exp",         // scheme-invariant single word
            "iat",         // scheme-invariant single word
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(
            keys, expected,
            "MeResponse wire key-set drifted from the R-53 camelCase shape"
        );
        for key in &keys {
            assert!(
                !key.contains('_'),
                "MeResponse wire key `{key}` contains `_` — a snake_case field survived. \
                 GC wire shape is ALL camelCase (R-53). DO NOT silently re-baseline \
                 (rule owned by task #51)."
            );
        }
    }
}
