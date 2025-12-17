//! Authentication client fixture for token issuance and JWKS operations.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Authentication client errors.
#[derive(Debug, Error)]
pub enum AuthClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Token issuance failed with status {status}: {body}")]
    IssuanceFailed { status: u16, body: String },

    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// OAuth 2.0 token request.
#[derive(Debug, Serialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl TokenRequest {
    /// Create a new client credentials token request with optional scope.
    pub fn client_credentials(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        let scope_str = scope.into();
        Self {
            grant_type: "client_credentials".to_string(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: if scope_str.is_empty() {
                None
            } else {
                Some(scope_str)
            },
        }
    }
}

/// OAuth 2.0 token response.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

/// JWKS (JSON Web Key Set) response.
#[derive(Debug, Deserialize, Clone)]
pub struct JwksResponse {
    pub keys: Vec<JwkKey>,
}

/// A single JWK (JSON Web Key).
#[derive(Debug, Deserialize, Clone)]
pub struct JwkKey {
    pub kty: String,
    pub kid: String,
    pub alg: Option<String>,
    pub crv: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    #[serde(rename = "use")]
    pub key_use: Option<String>,
}

/// Client for interacting with the Authentication Controller service.
pub struct AuthClient {
    base_url: String,
    http_client: Client,
}

impl AuthClient {
    /// Create a new authentication client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: Client::new(),
        }
    }

    /// Issue a token using client credentials.
    pub async fn issue_token(
        &self,
        request: TokenRequest,
    ) -> Result<TokenResponse, AuthClientError> {
        // AC service token endpoint is at /api/v1/auth/service/token
        let token_url = format!("{}/api/v1/auth/service/token", self.base_url);

        let response = self
            .http_client
            .post(&token_url)
            .json(&request)
            .send()
            .await?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AuthClientError::IssuanceFailed {
                status: status.as_u16(),
                body,
            });
        }

        let token_response = response.json::<TokenResponse>().await?;
        Ok(token_response)
    }

    /// Fetch the JWKS (JSON Web Key Set) from the service.
    pub async fn fetch_jwks(&self) -> Result<JwksResponse, AuthClientError> {
        // AC service JWKS endpoint is at /.well-known/jwks.json
        let jwks_url = format!("{}/.well-known/jwks.json", self.base_url);

        let response = self.http_client.get(&jwks_url).send().await?;

        if !response.status().is_success() {
            return Err(AuthClientError::JwksFetchFailed(format!(
                "Status: {}",
                response.status()
            )));
        }

        let jwks = response.json::<JwksResponse>().await?;
        Ok(jwks)
    }

    /// Get the HTTP client for custom requests.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    /// Get the base URL for the authentication service.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
