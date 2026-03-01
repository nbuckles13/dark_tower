//! Authentication client fixture for token issuance and JWKS operations.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Default password for test users (meets AC's 8-char minimum).
pub const TEST_USER_PASSWORD: &str = "test-env-password-42";

/// Default subdomain for the seeded dev organization.
const TEST_ORG_SUBDOMAIN: &str = "devtest";

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

    /// Register a new user and return the registration response with auto-login token.
    ///
    /// The registration endpoint requires org context from the `Host` header.
    /// We use the seeded `devtest` organization subdomain.
    ///
    /// # Arguments
    ///
    /// * `request` - Registration request with email, password, display_name
    pub async fn register_user(
        &self,
        request: &UserRegistrationRequest,
    ) -> Result<UserRegistrationResponse, AuthClientError> {
        let register_url = format!("{}/api/v1/auth/register", self.base_url);

        // Extract host and port from base_url for the Host header.
        // AC's org extraction middleware requires subdomain.domain format.
        let host_header = build_org_host_header(&self.base_url, TEST_ORG_SUBDOMAIN);

        let response = self
            .http_client
            .post(&register_url)
            .header("Host", &host_header)
            .json(request)
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

        let registration_response = response.json::<UserRegistrationResponse>().await?;
        Ok(registration_response)
    }
}

/// Build a Host header with org subdomain for AC's org extraction middleware.
///
/// Given base_url `http://localhost:8082` and subdomain `devtest`,
/// produces `devtest.localhost:8082`.
fn build_org_host_header(base_url: &str, subdomain: &str) -> String {
    // Strip scheme
    let without_scheme = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
        .unwrap_or(base_url);

    // Strip trailing slash
    let host = without_scheme.trim_end_matches('/');

    // Format as subdomain.host[:port]
    format!("{}.{}", subdomain, host)
}

/// User registration request.
///
/// Sent to AC's `POST /api/v1/auth/register` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct UserRegistrationRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

impl UserRegistrationRequest {
    /// Create a registration request with a unique UUID-based email.
    ///
    /// Uses a UUID in the email to prevent collisions across test runs.
    pub fn unique(display_name: impl Into<String>) -> Self {
        Self {
            email: format!("test-{}@envtest.dev", Uuid::new_v4()),
            password: TEST_USER_PASSWORD.to_string(),
            display_name: display_name.into(),
        }
    }
}

/// User registration response.
///
/// Returned by AC's `POST /api/v1/auth/register` endpoint.
/// Contains an auto-login user JWT in `access_token`.
#[derive(Clone, Deserialize)]
pub struct UserRegistrationResponse {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

impl std::fmt::Debug for UserRegistrationResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserRegistrationResponse")
            .field("user_id", &self.user_id)
            .field("email", &self.email)
            .field("display_name", &self.display_name)
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_org_host_header_http() {
        let result = build_org_host_header("http://localhost:8082", "devtest");
        assert_eq!(result, "devtest.localhost:8082");
    }

    #[test]
    fn test_build_org_host_header_https() {
        let result = build_org_host_header("https://example.com:443", "acme");
        assert_eq!(result, "acme.example.com:443");
    }

    #[test]
    fn test_build_org_host_header_trailing_slash() {
        let result = build_org_host_header("http://localhost:8082/", "devtest");
        assert_eq!(result, "devtest.localhost:8082");
    }

    #[test]
    fn test_user_registration_request_unique_email() {
        let req1 = UserRegistrationRequest::unique("User 1");
        let req2 = UserRegistrationRequest::unique("User 2");

        assert_ne!(req1.email, req2.email, "Emails should be unique");
        assert!(req1.email.ends_with("@envtest.dev"));
        assert_eq!(req1.password, TEST_USER_PASSWORD);
    }

    #[test]
    fn test_user_registration_response_debug_redacts_token() {
        let response = UserRegistrationResponse {
            user_id: Uuid::nil(),
            email: "test@example.com".to_string(),
            display_name: "Test User".to_string(),
            access_token: "eyJhbGciOiJFZERTQSJ9.secret.sig".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
        };

        let debug_output = format!("{:?}", response);

        assert!(
            !debug_output.contains("eyJhbGciOiJFZERTQSJ9"),
            "access_token should be redacted"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "Should contain [REDACTED]"
        );
        assert!(
            debug_output.contains("test@example.com"),
            "Email should be visible"
        );
    }
}
