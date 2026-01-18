//! Global Controller client fixture for cross-service e2e tests.
//!
//! Provides HTTP client for interacting with the Global Controller API,
//! including meeting join, guest token, and settings management endpoints.

use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use thiserror::Error;
use uuid::Uuid;

/// Maximum length for error body in error messages.
const MAX_ERROR_BODY_LEN: usize = 256;

/// Regex pattern for JWT tokens (header.payload.signature).
static JWT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]*)?").unwrap()
});

/// Regex pattern for Bearer tokens in text.
static BEARER_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)bearer\s+[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]*)?").unwrap()
});

/// Sanitize error response body to remove sensitive data.
///
/// This function:
/// - Removes Bearer token patterns (must be first to capture "Bearer <JWT>" as a whole)
/// - Removes JWT patterns (eyJ...)
/// - Truncates long bodies to MAX_ERROR_BODY_LEN
fn sanitize_error_body(body: &str) -> String {
    // Remove Bearer token patterns first (captures "Bearer <JWT>" as a whole)
    let sanitized = BEARER_PATTERN.replace_all(body, "[BEARER_REDACTED]");
    // Remove standalone JWT patterns
    let sanitized = JWT_PATTERN.replace_all(&sanitized, "[JWT_REDACTED]");

    // Truncate if too long
    if sanitized.len() > MAX_ERROR_BODY_LEN {
        format!("{}...[truncated]", &sanitized[..MAX_ERROR_BODY_LEN])
    } else {
        sanitized.into_owned()
    }
}

/// Global Controller client errors.
#[derive(Debug, Error)]
pub enum GcClientError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Request failed with status {status}: {body}")]
    RequestFailed { status: u16, body: String },

    #[error("JSON deserialization failed: {0}")]
    JsonError(#[from] serde_json::Error),
}

/// Request body for guest token endpoint.
#[derive(Clone, Serialize)]
pub struct GuestTokenRequest {
    /// Display name for the guest.
    pub display_name: String,

    /// CAPTCHA verification token.
    pub captcha_token: String,
}

impl std::fmt::Debug for GuestTokenRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuestTokenRequest")
            .field("display_name", &self.display_name)
            .field("captcha_token", &"[REDACTED]")
            .finish()
    }
}

/// Response from meeting join or guest token endpoints.
#[derive(Clone, Deserialize)]
pub struct JoinMeetingResponse {
    /// The issued meeting token (JWT).
    pub token: String,

    /// Token expiration in seconds.
    pub expires_in: u32,

    /// Meeting UUID.
    pub meeting_id: Uuid,

    /// Meeting display name.
    pub meeting_name: String,
}

impl std::fmt::Debug for JoinMeetingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JoinMeetingResponse")
            .field("token", &"[REDACTED]")
            .field("expires_in", &self.expires_in)
            .field("meeting_id", &self.meeting_id)
            .field("meeting_name", &self.meeting_name)
            .finish()
    }
}

/// Response from `/v1/me` endpoint.
#[derive(Clone, Deserialize)]
pub struct MeResponse {
    /// Subject (user or client ID).
    pub sub: String,

    /// Token scopes.
    pub scopes: Vec<String>,

    /// Service type (if service token).
    #[serde(default)]
    pub service_type: Option<String>,

    /// Token expiration timestamp.
    pub exp: i64,

    /// Token issued-at timestamp.
    pub iat: i64,
}

impl std::fmt::Debug for MeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeResponse")
            .field("sub", &"[REDACTED]")
            .field("scopes", &self.scopes)
            .field("service_type", &self.service_type)
            .field("exp", &self.exp)
            .field("iat", &self.iat)
            .finish()
    }
}

/// Request body for updating meeting settings.
#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateMeetingSettingsRequest {
    /// Allow anonymous guests to join.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_guests: Option<bool>,

    /// Allow users from other organizations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_external_participants: Option<bool>,

    /// Enable waiting room for guests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_room_enabled: Option<bool>,
}

impl UpdateMeetingSettingsRequest {
    /// Create a new settings request with allow_guests.
    pub fn with_allow_guests(allow_guests: bool) -> Self {
        Self {
            allow_guests: Some(allow_guests),
            ..Default::default()
        }
    }

    /// Create a new settings request with waiting_room_enabled.
    pub fn with_waiting_room(waiting_room_enabled: bool) -> Self {
        Self {
            waiting_room_enabled: Some(waiting_room_enabled),
            ..Default::default()
        }
    }
}

/// Response from meeting settings endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct MeetingResponse {
    /// Meeting UUID.
    pub meeting_id: Uuid,

    /// Organization UUID.
    pub org_id: Uuid,

    /// Meeting display name.
    pub display_name: String,

    /// Unique meeting code for joining.
    pub meeting_code: String,

    /// Meeting status.
    pub status: String,

    /// Allow anonymous guests.
    pub allow_guests: bool,

    /// Allow external organization participants.
    pub allow_external_participants: bool,

    /// Enable waiting room.
    pub waiting_room_enabled: bool,
}

/// Client for interacting with the Global Controller service.
pub struct GcClient {
    base_url: String,
    http_client: Client,
}

impl GcClient {
    /// Create a new GC client.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http_client: Client::new(),
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the HTTP client for custom requests.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    /// Check GC health endpoint.
    ///
    /// Returns Ok(()) if healthy, Err otherwise.
    pub async fn health_check(&self) -> Result<(), GcClientError> {
        let url = format!("{}/v1/health", self.base_url);

        let response = self.http_client.get(&url).send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GcClientError::RequestFailed {
                status: status.as_u16(),
                body: sanitize_error_body(&body),
            });
        }

        Ok(())
    }

    /// Get current user info from `/v1/me` endpoint.
    ///
    /// # Arguments
    ///
    /// * `token` - Bearer token for authentication
    pub async fn get_me(&self, token: &str) -> Result<MeResponse, GcClientError> {
        let url = format!("{}/v1/me", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Join a meeting as an authenticated user.
    ///
    /// # Arguments
    ///
    /// * `meeting_code` - The meeting code to join
    /// * `token` - Bearer token for authentication
    ///
    /// # Endpoint
    ///
    /// `GET /v1/meetings/{code}`
    pub async fn join_meeting(
        &self,
        meeting_code: &str,
        token: &str,
    ) -> Result<JoinMeetingResponse, GcClientError> {
        let url = format!("{}/v1/meetings/{}", self.base_url, meeting_code);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Request a guest token for anonymous meeting access.
    ///
    /// # Arguments
    ///
    /// * `meeting_code` - The meeting code to join
    /// * `request` - Guest token request with display name and captcha
    ///
    /// # Endpoint
    ///
    /// `POST /v1/meetings/{code}/guest-token`
    pub async fn get_guest_token(
        &self,
        meeting_code: &str,
        request: &GuestTokenRequest,
    ) -> Result<JoinMeetingResponse, GcClientError> {
        let url = format!("{}/v1/meetings/{}/guest-token", self.base_url, meeting_code);

        let response = self.http_client.post(&url).json(request).send().await?;

        self.handle_response(response).await
    }

    /// Update meeting settings.
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Meeting UUID
    /// * `token` - Bearer token for authentication (must be host)
    /// * `request` - Settings to update
    ///
    /// # Endpoint
    ///
    /// `PATCH /v1/meetings/{id}/settings`
    pub async fn update_meeting_settings(
        &self,
        meeting_id: Uuid,
        token: &str,
        request: &UpdateMeetingSettingsRequest,
    ) -> Result<MeetingResponse, GcClientError> {
        let url = format!("{}/v1/meetings/{}/settings", self.base_url, meeting_id);

        let response = self
            .http_client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Make a raw request and return the response for testing error cases.
    ///
    /// Useful for testing authentication failures, permission errors, etc.
    pub async fn raw_join_meeting(
        &self,
        meeting_code: &str,
        token: Option<&str>,
    ) -> Result<reqwest::Response, GcClientError> {
        let url = format!("{}/v1/meetings/{}", self.base_url, meeting_code);

        let mut request = self.http_client.get(&url);

        if let Some(t) = token {
            request = request.header("Authorization", format!("Bearer {}", t));
        }

        Ok(request.send().await?)
    }

    /// Make a raw request to update settings and return the response.
    ///
    /// Useful for testing permission errors (non-host trying to update).
    pub async fn raw_update_settings(
        &self,
        meeting_id: Uuid,
        token: Option<&str>,
        request: &UpdateMeetingSettingsRequest,
    ) -> Result<reqwest::Response, GcClientError> {
        let url = format!("{}/v1/meetings/{}/settings", self.base_url, meeting_id);

        let mut http_request = self.http_client.patch(&url).json(request);

        if let Some(t) = token {
            http_request = http_request.header("Authorization", format!("Bearer {}", t));
        }

        Ok(http_request.send().await?)
    }

    /// Handle response and parse JSON body.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, GcClientError> {
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(GcClientError::RequestFailed {
                status: status.as_u16(),
                body: sanitize_error_body(&body),
            });
        }

        let parsed = response.json().await?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guest_token_request_serialization() {
        let request = GuestTokenRequest {
            display_name: "Test Guest".to_string(),
            captcha_token: "captcha123".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"display_name\":\"Test Guest\""));
        assert!(json.contains("\"captcha_token\":\"captcha123\""));
    }

    #[test]
    fn test_update_settings_request_with_allow_guests() {
        let request = UpdateMeetingSettingsRequest::with_allow_guests(true);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"allow_guests\":true"));
        // Other fields should be omitted
        assert!(!json.contains("allow_external"));
        assert!(!json.contains("waiting_room"));
    }

    #[test]
    fn test_update_settings_request_with_waiting_room() {
        let request = UpdateMeetingSettingsRequest::with_waiting_room(false);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"waiting_room_enabled\":false"));
    }

    #[test]
    fn test_join_meeting_response_deserialization() {
        let json = r#"{
            "token": "eyJ...",
            "expires_in": 900,
            "meeting_id": "00000000-0000-0000-0000-000000000001",
            "meeting_name": "Test Meeting"
        }"#;

        let response: JoinMeetingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.token, "eyJ...");
        assert_eq!(response.expires_in, 900);
        assert_eq!(response.meeting_name, "Test Meeting");
    }

    #[test]
    fn test_me_response_deserialization() {
        let json = r#"{
            "sub": "user123",
            "scopes": ["read", "write"],
            "service_type": "global-controller",
            "exp": 1234567890,
            "iat": 1234567800
        }"#;

        let response: MeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.sub, "user123");
        assert_eq!(response.scopes, vec!["read", "write"]);
        assert_eq!(response.service_type, Some("global-controller".to_string()));
    }

    #[test]
    fn test_me_response_without_service_type() {
        let json = r#"{
            "sub": "user123",
            "scopes": ["read"],
            "exp": 1234567890,
            "iat": 1234567800
        }"#;

        let response: MeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.service_type, None);
    }

    #[test]
    fn test_meeting_response_deserialization() {
        let json = r#"{
            "meeting_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "00000000-0000-0000-0000-000000000002",
            "display_name": "Team Standup",
            "meeting_code": "abc-def-ghi",
            "status": "scheduled",
            "allow_guests": true,
            "allow_external_participants": false,
            "waiting_room_enabled": true
        }"#;

        let response: MeetingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.display_name, "Team Standup");
        assert_eq!(response.meeting_code, "abc-def-ghi");
        assert!(response.allow_guests);
        assert!(!response.allow_external_participants);
        assert!(response.waiting_room_enabled);
    }

    #[test]
    fn test_guest_token_request_debug_redacts_captcha_token() {
        let request = GuestTokenRequest {
            display_name: "Test Guest".to_string(),
            captcha_token: "secret-captcha-token-12345".to_string(),
        };

        let debug_output = format!("{:?}", request);

        // display_name should be visible
        assert!(
            debug_output.contains("Test Guest"),
            "display_name should be visible in debug output"
        );
        // captcha_token should be redacted
        assert!(
            !debug_output.contains("secret-captcha-token-12345"),
            "captcha_token should NOT be visible in debug output"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "debug output should contain [REDACTED] for captcha_token"
        );
    }

    #[test]
    fn test_join_meeting_response_debug_redacts_token() {
        let response = JoinMeetingResponse {
            token: "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.secret.signature".to_string(),
            expires_in: 900,
            meeting_id: Uuid::nil(),
            meeting_name: "Test Meeting".to_string(),
        };

        let debug_output = format!("{:?}", response);

        // Non-sensitive fields should be visible
        assert!(
            debug_output.contains("Test Meeting"),
            "meeting_name should be visible in debug output"
        );
        assert!(
            debug_output.contains("900"),
            "expires_in should be visible in debug output"
        );
        // token should be redacted
        assert!(
            !debug_output.contains("eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9"),
            "JWT token should NOT be visible in debug output"
        );
        assert!(
            !debug_output.contains("secret"),
            "JWT payload should NOT be visible in debug output"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "debug output should contain [REDACTED] for token"
        );
    }

    #[test]
    fn test_error_body_sanitizes_jwt_tokens() {
        // Test JWT token in error body
        let body_with_jwt = r#"{"error": "Invalid token", "token": "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIn0.signature"}"#;
        let sanitized = sanitize_error_body(body_with_jwt);

        // JWT should be redacted
        assert!(
            !sanitized.contains("eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9"),
            "JWT header should be redacted"
        );
        assert!(
            !sanitized.contains("eyJzdWIiOiJ1c2VyMTIzIn0"),
            "JWT payload should be redacted"
        );
        assert!(
            sanitized.contains("[JWT_REDACTED]"),
            "Should contain JWT_REDACTED marker"
        );

        // Non-sensitive parts should remain
        assert!(
            sanitized.contains("Invalid token"),
            "Non-sensitive error message should remain"
        );
    }

    #[test]
    fn test_error_body_sanitizes_bearer_tokens() {
        let body_with_bearer = r#"Authorization header: Bearer eyJhbGciOiJFZERTQSJ9.eyJzdWIiOiJ0ZXN0In0.sig was invalid"#;
        let sanitized = sanitize_error_body(body_with_bearer);

        // Bearer token should be redacted
        assert!(
            sanitized.contains("[BEARER_REDACTED]"),
            "Should contain BEARER_REDACTED marker"
        );
        assert!(
            !sanitized.contains("eyJhbGciOiJFZERTQSJ9"),
            "JWT in Bearer token should be redacted"
        );
    }

    #[test]
    fn test_error_body_truncates_long_responses() {
        // Create a body longer than MAX_ERROR_BODY_LEN (256)
        let long_body = "a".repeat(500);
        let sanitized = sanitize_error_body(&long_body);

        // Should be truncated
        assert!(
            sanitized.len() < 500,
            "Long body should be truncated, got len: {}",
            sanitized.len()
        );
        assert!(
            sanitized.ends_with("...[truncated]"),
            "Truncated body should end with truncation marker"
        );
        // Should be approximately MAX_ERROR_BODY_LEN + marker length
        assert!(
            sanitized.len() <= MAX_ERROR_BODY_LEN + 15,
            "Truncated length should be around max + marker"
        );
    }

    #[test]
    fn test_error_body_preserves_short_safe_messages() {
        let safe_body = r#"{"error": "Not found", "code": 404}"#;
        let sanitized = sanitize_error_body(safe_body);

        // Should be unchanged (no sensitive data, not too long)
        assert_eq!(sanitized, safe_body, "Safe short body should be unchanged");
    }

    #[test]
    fn test_me_response_debug_redacts_sub() {
        let response = MeResponse {
            sub: "user-secret-id-12345".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            service_type: Some("global-controller".to_string()),
            exp: 1234567890,
            iat: 1234567800,
        };

        let debug_output = format!("{:?}", response);

        // sub should be redacted
        assert!(
            !debug_output.contains("user-secret-id-12345"),
            "sub field should NOT be visible in debug output"
        );
        assert!(
            debug_output.contains("[REDACTED]"),
            "debug output should contain [REDACTED] for sub"
        );

        // Other fields should be visible
        assert!(
            debug_output.contains("read"),
            "scopes should be visible in debug output"
        );
        assert!(
            debug_output.contains("global-controller"),
            "service_type should be visible in debug output"
        );
        assert!(
            debug_output.contains("1234567890"),
            "exp should be visible in debug output"
        );
    }
}
