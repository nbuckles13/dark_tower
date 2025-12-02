use crate::config::Config;
use crate::errors::AcError;
use crate::models::TokenResponse;
use crate::services::token_service;
use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct UserTokenRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ServiceTokenRequest {
    pub grant_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
}

/// Handle user token request
///
/// POST /api/v1/auth/user/token
pub async fn handle_user_token(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UserTokenRequest>,
) -> Result<Json<TokenResponse>, AcError> {
    let token = token_service::issue_user_token(
        &state.pool,
        &state.config.master_key,
        &payload.username,
        &payload.password,
    )
    .await?;

    Ok(Json(token))
}

/// Handle service token request (OAuth 2.0 Client Credentials)
///
/// POST /api/v1/auth/service/token
///
/// Accepts credentials via:
/// - HTTP Basic Auth (preferred)
/// - Request body (client_id, client_secret)
pub async fn handle_service_token(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<ServiceTokenRequest>,
) -> Result<Json<TokenResponse>, AcError> {
    // Validate grant_type
    if payload.grant_type != "client_credentials" {
        return Err(AcError::InvalidCredentials);
    }

    // Extract client credentials from Basic Auth or request body
    let (client_id, client_secret) = extract_client_credentials(&headers, &payload)?;

    // Extract IP address and User-Agent
    let ip_address = Some(addr.ip().to_string());
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Parse requested scopes
    let requested_scopes = payload.scope.map(|s| {
        s.split_whitespace()
            .map(|scope| scope.to_string())
            .collect()
    });

    // Issue token
    let token = token_service::issue_service_token(
        &state.pool,
        &state.config.master_key,
        &client_id,
        &client_secret,
        &payload.grant_type,
        requested_scopes,
        ip_address.as_deref(),
        user_agent.as_deref(),
    )
    .await?;

    Ok(Json(token))
}

/// Extract client credentials from Basic Auth header or request body
fn extract_client_credentials(
    headers: &HeaderMap,
    payload: &ServiceTokenRequest,
) -> Result<(String, String), AcError> {
    // Try Basic Auth first
    if let Some(auth_header) = headers.get("authorization") {
        let auth_str = auth_header
            .to_str()
            .map_err(|_| AcError::InvalidCredentials)?;

        if let Some(basic_auth) = auth_str.strip_prefix("Basic ") {
            let decoded = general_purpose::STANDARD
                .decode(basic_auth)
                .map_err(|_| AcError::InvalidCredentials)?;

            let credentials =
                String::from_utf8(decoded).map_err(|_| AcError::InvalidCredentials)?;

            return match credentials.splitn(2, ':').collect::<Vec<_>>().as_slice() {
                [username, password] => Ok((username.to_string(), password.to_string())),
                _ => Err(AcError::InvalidCredentials),
            };
        }
    }

    // Fall back to request body
    match (&payload.client_id, &payload.client_secret) {
        (Some(id), Some(secret)) => Ok((id.clone(), secret.clone())),
        _ => Err(AcError::InvalidCredentials),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::AUTHORIZATION;

    #[test]
    fn test_extract_credentials_from_basic_auth() {
        let mut headers = HeaderMap::new();
        // Base64 encoding of "client_id:client_secret"
        headers.insert(
            AUTHORIZATION,
            "Basic Y2xpZW50X2lkOmNsaWVudF9zZWNyZXQ=".parse().unwrap(),
        );

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "client_id");
        assert_eq!(secret, "client_secret");
    }

    #[test]
    fn test_extract_credentials_from_body() {
        let headers = HeaderMap::new();
        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("test_client".to_string()),
            client_secret: Some("test_secret".to_string()),
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "test_client");
        assert_eq!(secret, "test_secret");
    }

    #[test]
    fn test_extract_credentials_basic_auth_priority() {
        let mut headers = HeaderMap::new();
        // Base64 encoding of "auth_client:auth_secret"
        headers.insert(
            AUTHORIZATION,
            "Basic YXV0aF9jbGllbnQ6YXV0aF9zZWNyZXQ=".parse().unwrap(),
        );

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("body_client".to_string()),
            client_secret: Some("body_secret".to_string()),
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        // Should use Basic Auth, not body
        assert_eq!(id, "auth_client");
        assert_eq!(secret, "auth_secret");
    }

    #[test]
    fn test_extract_credentials_invalid_base64() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Basic not-valid-base64!".parse().unwrap());

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcError::InvalidCredentials));
    }

    #[test]
    fn test_extract_credentials_invalid_utf8() {
        let mut headers = HeaderMap::new();
        // Base64 encoding of invalid UTF-8 bytes
        headers.insert(
            AUTHORIZATION,
            "Basic /////w==".parse().unwrap(), // Decodes to invalid UTF-8
        );

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcError::InvalidCredentials));
    }

    #[test]
    fn test_extract_credentials_missing_colon() {
        let mut headers = HeaderMap::new();
        // Base64 encoding of "nocredentials" (no colon separator)
        headers.insert(AUTHORIZATION, "Basic bm9jcmVkZW50aWFscw==".parse().unwrap());

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcError::InvalidCredentials));
    }

    #[test]
    fn test_extract_credentials_bearer_token_ignored() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer some-jwt-token".parse().unwrap());

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("body_client".to_string()),
            client_secret: Some("body_secret".to_string()),
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_ok());
        // Should fall back to body since Bearer is not Basic
        let (id, secret) = result.unwrap();
        assert_eq!(id, "body_client");
        assert_eq!(secret, "body_secret");
    }

    #[test]
    fn test_extract_credentials_missing_all() {
        let headers = HeaderMap::new();
        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcError::InvalidCredentials));
    }

    #[test]
    fn test_extract_credentials_partial_body_credentials() {
        let headers = HeaderMap::new();
        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: Some("test_client".to_string()),
            client_secret: None, // Missing secret
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AcError::InvalidCredentials));
    }

    #[test]
    fn test_extract_credentials_with_special_characters() {
        let mut headers = HeaderMap::new();
        // Base64 encoding of "client:pass@word:with:colons"
        headers.insert(
            AUTHORIZATION,
            "Basic Y2xpZW50OnBhc3NAd29yZDp3aXRoOmNvbG9ucw=="
                .parse()
                .unwrap(),
        );

        let payload = ServiceTokenRequest {
            grant_type: "client_credentials".to_string(),
            client_id: None,
            client_secret: None,
            scope: None,
        };

        let result = extract_client_credentials(&headers, &payload);
        assert!(result.is_ok());
        let (id, secret) = result.unwrap();
        assert_eq!(id, "client");
        // splitn(2, ':') should preserve remaining colons in password
        assert_eq!(secret, "pass@word:with:colons");
    }
}
