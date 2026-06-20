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

/// OAuth 2.0 token response (service-token / client_credentials endpoint).
///
/// Mirror of AC's `crates/ac-service/src/models/mod.rs:TokenResponse`. Under task #51's single rule,
/// the wire is uniform camelCase (`accessToken`/`tokenType`/`expiresIn`/`scope`) — flipped in lockstep
/// with the server struct AND the production consumer `common::token_manager::OAuthTokenResponse`.
/// Rust field idents stay snake_case (`rename_all` flips only the wire keys).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
/// Contains an auto-login user JWT in `access_token` (Rust ident; wire key is `accessToken`).
///
/// Mirror of `crates/ac-service/src/handlers/auth_handler.rs:UserRegistrationResponse`;
/// uses the SAME uniform camelCase wire shape — task #51 removed the former per-field
/// OAuth snake_case carve-out on this user-flow endpoint, so all wire keys are camelCase
/// (`userId`/`email`/`displayName`/`accessToken`/`tokenType`/`expiresIn`). Rust field idents
/// stay snake_case (the `rename_all` flips only the wire keys).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
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

    /// WIRE-SHAPE ROUND-TRIP LOCK — `UserRegistrationResponse` mirror (task #51).
    ///
    /// DB-free guard that the env-tests fixture deserializes AC's CURRENT register
    /// wire shape: uniform camelCase, no per-field OAuth carve-out. The live
    /// round-trip (via `register_user()`) only runs in the cluster-gated env-tests;
    /// this asserts the mirror tracks the server shape without a DB — the same
    /// always-on protection the AC-side struct locks provide. If the AC server flips
    /// the wire and this mirror lags, the cluster round-trip (matrix c) would fail at
    /// deserialize; this test trips first, in-clone.
    #[test]
    fn test_user_registration_response_camel_wire_round_trip() {
        // CURRENT camelCase wire (post-task-#51) deserializes and populates all fields.
        let camel = r#"{
            "userId": "00000000-0000-0000-0000-000000000000",
            "email": "alice@example.com",
            "displayName": "Alice",
            "accessToken": "FAKE_TOKEN_FOR_TEST",
            "tokenType": "Bearer",
            "expiresIn": 3600
        }"#;
        let resp: UserRegistrationResponse =
            serde_json::from_str(camel).expect("camelCase register wire must deserialize");
        assert_eq!(resp.user_id, Uuid::nil());
        assert_eq!(resp.email, "alice@example.com");
        assert_eq!(resp.display_name, "Alice");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, 3600);

        // Wire-break: the legacy mixed snake_case OAuth keys no longer populate the
        // token fields. With `rename_all = "camelCase"` and no per-field overrides,
        // `access_token`/`token_type`/`expires_in` are unknown keys; since the struct
        // does not `deny_unknown_fields`, they're ignored and the required `accessToken`
        // is then missing → deserialize error. This pins the carve-out's removal.
        let snake = r#"{
            "userId": "00000000-0000-0000-0000-000000000000",
            "email": "alice@example.com",
            "displayName": "Alice",
            "access_token": "FAKE_TOKEN_FOR_TEST",
            "token_type": "Bearer",
            "expires_in": 3600
        }"#;
        let res: Result<UserRegistrationResponse, _> = serde_json::from_str(snake);
        assert!(
            res.is_err(),
            "legacy snake_case OAuth keys must no longer satisfy the register response \
             (task #51 removed the per-field carve-out; the mirror must track the camel wire)"
        );
    }
}
