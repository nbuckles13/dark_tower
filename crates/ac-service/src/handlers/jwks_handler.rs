use crate::errors::AcError;
use crate::models::Jwks;
use crate::observability::metrics::record_jwks_request;
use crate::services::key_management_service;
use axum::{
    extract::State,
    http::header::{HeaderMap, HeaderValue, CACHE_CONTROL},
    Json,
};
use std::sync::Arc;
use tracing::instrument;

use super::auth_handler::AppState;

/// Handle JWKS request
///
/// GET /.well-known/jwks.json
///
/// Returns all active public keys in JWKS format (RFC 7517)
/// with Cache-Control header set to max-age=3600 (1 hour)
///
/// ADR-0011: Handler instrumented with skip_all to prevent PII leakage.
#[instrument(name = "ac.jwks.get", skip_all, fields(cache_status = "miss", status))]
pub async fn handle_get_jwks(
    State(state): State<Arc<AppState>>,
) -> Result<(HeaderMap, Json<Jwks>), AcError> {
    // Note: Cache status is always "miss" at the handler level.
    // Upstream caches (CDN, browser) handle caching based on Cache-Control.
    let result = key_management_service::get_jwks(&state.pool).await;

    let status = if result.is_ok() { "success" } else { "error" };
    tracing::Span::current().record("status", status);
    record_jwks_request("miss"); // Handler always fetches from DB

    let jwks = result?;

    // Add Cache-Control header to allow caching for 1 hour
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=3600"));

    Ok((headers, Json(jwks)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwks_serialization() {
        use crate::models::JsonWebKey;

        let jwks = Jwks {
            keys: vec![
                JsonWebKey {
                    kid: "key-1".to_string(),
                    kty: "OKP".to_string(),
                    crv: "Ed25519".to_string(),
                    x: "base64url-encoded-public-key".to_string(),
                    use_: "sig".to_string(),
                    alg: "EdDSA".to_string(),
                },
                JsonWebKey {
                    kid: "key-2".to_string(),
                    kty: "OKP".to_string(),
                    crv: "Ed25519".to_string(),
                    x: "another-base64url-encoded-key".to_string(),
                    use_: "sig".to_string(),
                    alg: "EdDSA".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&jwks).unwrap();
        assert!(json.contains("\"kid\":\"key-1\""));
        assert!(json.contains("\"kid\":\"key-2\""));
        assert!(json.contains("\"kty\":\"OKP\""));
        assert!(json.contains("\"crv\":\"Ed25519\""));
        assert!(json.contains("\"use\":\"sig\""));
        assert!(json.contains("\"alg\":\"EdDSA\""));
    }

    #[test]
    fn test_jwks_deserialization() {
        let json = r#"{
            "keys": [
                {
                    "kid": "test-key",
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": "test-public-key",
                    "use": "sig",
                    "alg": "EdDSA"
                }
            ]
        }"#;

        let jwks: Jwks = serde_json::from_str(json).unwrap();
        assert_eq!(jwks.keys.len(), 1);
        assert_eq!(jwks.keys[0].kid, "test-key");
        assert_eq!(jwks.keys[0].kty, "OKP");
        assert_eq!(jwks.keys[0].crv, "Ed25519");
        assert_eq!(jwks.keys[0].x, "test-public-key");
        assert_eq!(jwks.keys[0].use_, "sig");
        assert_eq!(jwks.keys[0].alg, "EdDSA");
    }

    #[test]
    fn test_empty_jwks() {
        let jwks = Jwks { keys: vec![] };
        let json = serde_json::to_string(&jwks).unwrap();
        assert!(json.contains("\"keys\":[]"));

        let parsed: Jwks = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.keys.len(), 0);
    }

    #[test]
    fn test_jwks_multiple_keys() {
        use crate::models::JsonWebKey;

        let jwks = Jwks {
            keys: vec![
                JsonWebKey {
                    kid: "key-1".to_string(),
                    kty: "OKP".to_string(),
                    crv: "Ed25519".to_string(),
                    x: "first-key".to_string(),
                    use_: "sig".to_string(),
                    alg: "EdDSA".to_string(),
                },
                JsonWebKey {
                    kid: "key-2".to_string(),
                    kty: "OKP".to_string(),
                    crv: "Ed25519".to_string(),
                    x: "second-key".to_string(),
                    use_: "sig".to_string(),
                    alg: "EdDSA".to_string(),
                },
                JsonWebKey {
                    kid: "key-3".to_string(),
                    kty: "OKP".to_string(),
                    crv: "Ed25519".to_string(),
                    x: "third-key".to_string(),
                    use_: "sig".to_string(),
                    alg: "EdDSA".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&jwks).unwrap();
        let parsed: Jwks = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.keys.len(), 3);
        assert_eq!(parsed.keys[0].kid, "key-1");
        assert_eq!(parsed.keys[1].kid, "key-2");
        assert_eq!(parsed.keys[2].kid, "key-3");
    }
}
