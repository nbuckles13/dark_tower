//! OAuth 2.0 Client Credentials Token Manager.
//!
//! Provides automatic token acquisition and refresh for service-to-service
//! authentication via OAuth 2.0 client credentials flow.
//!
//! # Features
//!
//! - Automatic token refresh before expiration (configurable threshold)
//! - Exponential backoff on refresh failures (1s, 2s, 4s, ..., max 30s)
//! - Thread-safe access via `tokio::sync::watch`
//! - Background refresh task (no contention on reads)
//! - Infinite retry on failures (caller controls startup timeout)
//!
//! # Example
//!
//! ```rust,ignore
//! use common::token_manager::{spawn_token_manager, TokenManagerConfig};
//! use common::secret::SecretString;
//! use std::time::Duration;
//!
//! let config = TokenManagerConfig::new(
//!     "http://localhost:8082".to_string(),
//!     "my-service".to_string(),
//!     SecretString::from("secret"),
//! );
//!
//! // Spawn manager - blocks until first token acquired
//! let (task_handle, token_rx) = spawn_token_manager(config).await?;
//!
//! // Get token (always valid after spawn returns)
//! let token = token_rx.token();
//!
//! // Use token in Authorization header
//! let header = format!("Bearer {}", token.expose_secret());
//!
//! // To shutdown: drop task_handle or abort it
//! task_handle.abort();
//! ```
//!
//! # Security
//!
//! - Client secret is stored as `SecretString` (never logged)
//! - Token is stored as `SecretString` internally
//! - Token acquisition/refresh events are logged (without values)
//! - HTTP timeouts prevent hanging connections
//!
//! **ADRs**: ADR-0003 (Service Auth)

use crate::secret::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument, trace, warn};

// =============================================================================
// Constants
// =============================================================================

/// Default refresh threshold (5 minutes before expiration).
pub const DEFAULT_REFRESH_THRESHOLD: Duration = Duration::from_secs(300);

/// Default HTTP request timeout.
pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Initial backoff delay for retry.
const INITIAL_BACKOFF_MS: u64 = 1000;

/// Maximum backoff delay.
const MAX_BACKOFF_MS: u64 = 30_000;

/// Default connection timeout for HTTP client.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Clock drift safety margin (30 seconds).
///
/// This margin accounts for clock differences between the `TokenManager` host
/// and the AC server. We refresh tokens slightly earlier than mathematically
/// required to handle cases where:
/// - System clocks are slightly out of sync
/// - Network latency delays token delivery
/// - Token expiration calculations have rounding differences
///
/// **Note**: Proper NTP synchronization on both hosts is strongly recommended
/// for production deployments.
const CLOCK_DRIFT_MARGIN_SECS: i64 = 30;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during token management.
#[derive(Error, Debug, Clone)]
pub enum TokenError {
    /// Token acquisition failed.
    #[error("Token acquisition failed: {0}")]
    AcquisitionFailed(String),

    /// HTTP client error.
    #[error("HTTP client error: {0}")]
    HttpError(String),

    /// Authentication rejected by AC (401, 400).
    #[error("Authentication rejected: {0}")]
    AuthenticationRejected(String),

    /// Token response parsing failed.
    #[error("Invalid token response: {0}")]
    InvalidResponse(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Watch channel closed unexpectedly.
    #[error("Token channel closed")]
    ChannelClosed,
}

// =============================================================================
// Refresh Callback Types
// =============================================================================

/// Event emitted after each token refresh attempt.
///
/// Contains only non-sensitive metadata about the refresh operation.
/// No tokens, secrets, or credentials are included.
#[derive(Debug, Clone)]
pub struct TokenRefreshEvent {
    /// Whether the refresh was successful.
    pub success: bool,

    /// Duration of the refresh operation (`acquire_token` call only, excludes backoff).
    pub duration: Duration,

    /// Error category for failed refreshes (bounded static string, not raw error message).
    ///
    /// Values: `"http"`, `"auth_rejected"`, `"invalid_response"`,
    /// `"acquisition_failed"`, `"configuration"`, `"channel_closed"`.
    pub error_category: Option<&'static str>,
}

/// Callback type for observing token refresh events.
///
/// Services can inject a callback to record metrics, emit events, etc.
/// without the `common` crate needing to depend on any metrics library.
pub type TokenRefreshCallback = Arc<dyn Fn(TokenRefreshEvent) + Send + Sync>;

/// Map a `TokenError` to a bounded error category string.
///
/// Returns a `&'static str` to ensure label cardinality is bounded.
/// Never returns raw error messages.
fn error_category(err: &TokenError) -> &'static str {
    match err {
        TokenError::HttpError(_) => "http",
        TokenError::AuthenticationRejected(_) => "auth_rejected",
        TokenError::InvalidResponse(_) => "invalid_response",
        TokenError::AcquisitionFailed(_) => "acquisition_failed",
        TokenError::Configuration(_) => "configuration",
        TokenError::ChannelClosed => "channel_closed",
    }
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the token manager.
#[derive(Clone)]
pub struct TokenManagerConfig {
    /// AC endpoint URL (e.g., `http://localhost:8082`).
    pub ac_endpoint: String,

    /// OAuth client ID.
    pub client_id: String,

    /// OAuth client secret (as `SecretString`).
    pub client_secret: SecretString,

    /// Refresh token this many seconds before expiration.
    pub refresh_threshold: Duration,

    /// HTTP request timeout.
    pub http_timeout: Duration,

    /// Optional callback invoked after each token refresh attempt.
    ///
    /// Enables services to record metrics without the `common` crate
    /// depending on any metrics library.
    pub on_refresh: Option<TokenRefreshCallback>,
}

impl std::fmt::Debug for TokenManagerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenManagerConfig")
            .field("ac_endpoint", &self.ac_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("refresh_threshold", &self.refresh_threshold)
            .field("http_timeout", &self.http_timeout)
            .field(
                "on_refresh",
                &self.on_refresh.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

impl TokenManagerConfig {
    /// Create a new configuration with default timeouts.
    ///
    /// # Arguments
    ///
    /// * `ac_endpoint` - The AC endpoint URL. **Should use HTTPS in production.**
    /// * `client_id` - The OAuth client ID.
    /// * `client_secret` - The OAuth client secret.
    ///
    /// # Security Warning
    ///
    /// Using HTTP URLs in production is insecure as credentials are sent in
    /// plain text. Use [`TokenManagerConfig::new_secure`] to enforce HTTPS.
    #[must_use]
    pub fn new(ac_endpoint: String, client_id: String, client_secret: SecretString) -> Self {
        Self {
            ac_endpoint,
            client_id,
            client_secret,
            refresh_threshold: DEFAULT_REFRESH_THRESHOLD,
            http_timeout: DEFAULT_HTTP_TIMEOUT,
            on_refresh: None,
        }
    }

    /// Create a new configuration requiring HTTPS.
    ///
    /// This is the recommended constructor for production use.
    ///
    /// # Errors
    ///
    /// Returns `TokenError::Configuration` if the URL doesn't use HTTPS.
    pub fn new_secure(
        ac_endpoint: String,
        client_id: String,
        client_secret: SecretString,
    ) -> Result<Self, TokenError> {
        if !ac_endpoint.starts_with("https://") {
            return Err(TokenError::Configuration(
                "AC endpoint must use HTTPS in production".into(),
            ));
        }
        Ok(Self::new(ac_endpoint, client_id, client_secret))
    }

    /// Create a configuration that automatically selects HTTP or HTTPS based on the URL.
    ///
    /// This is a convenience method that:
    /// - Allows HTTP for local development (http://...)
    /// - Enforces HTTPS for production (https://...)
    ///
    /// # Arguments
    ///
    /// * `ac_endpoint` - The AC endpoint URL (http:// or https://)
    /// * `client_id` - The OAuth client ID
    /// * `client_secret` - The OAuth client secret
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Local development - HTTP allowed
    /// let config = TokenManagerConfig::from_url(
    ///     "http://localhost:8082".to_string(),
    ///     "service".to_string(),
    ///     secret,
    /// );
    ///
    /// // Production - HTTPS enforced
    /// let config = TokenManagerConfig::from_url(
    ///     "https://ac-service:8082".to_string(),
    ///     "service".to_string(),
    ///     secret,
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `TokenError::Configuration` if the URL scheme is neither http nor https.
    pub fn from_url(
        ac_endpoint: String,
        client_id: String,
        client_secret: SecretString,
    ) -> Result<Self, TokenError> {
        if ac_endpoint.starts_with("https://") {
            // Production HTTPS - use secure constructor
            Self::new_secure(ac_endpoint, client_id, client_secret)
        } else if ac_endpoint.starts_with("http://") {
            // Local development HTTP - use regular constructor
            Ok(Self::new(ac_endpoint, client_id, client_secret))
        } else {
            Err(TokenError::Configuration(format!(
                "Invalid AC endpoint URL scheme (expected http:// or https://): {ac_endpoint}"
            )))
        }
    }

    /// Set the refresh threshold.
    #[must_use]
    pub fn with_refresh_threshold(mut self, threshold: Duration) -> Self {
        self.refresh_threshold = threshold;
        self
    }

    /// Set the HTTP timeout.
    #[must_use]
    pub fn with_http_timeout(mut self, timeout: Duration) -> Self {
        self.http_timeout = timeout;
        self
    }

    /// Set a callback to be invoked after each token refresh attempt.
    ///
    /// The callback receives a [`TokenRefreshEvent`] with non-sensitive metadata
    /// about the refresh operation (success/failure, duration, error category).
    ///
    /// # Panics
    ///
    /// The callback must not panic. A panic inside the callback will
    /// abort the token refresh loop, causing token expiration.
    #[must_use]
    pub fn with_on_refresh(mut self, callback: TokenRefreshCallback) -> Self {
        self.on_refresh = Some(callback);
        self
    }
}

// =============================================================================
// Token Receiver
// =============================================================================

/// Wrapper around watch receiver that provides safe token access.
///
/// This wrapper ensures that callers don't hold the borrow lock longer than
/// necessary, which would block the sender from updating the token.
#[derive(Clone)]
pub struct TokenReceiver(watch::Receiver<SecretString>);

impl TokenReceiver {
    /// Create a `TokenReceiver` from a watch receiver.
    ///
    /// This is primarily useful for testing purposes where you want to
    /// create a `TokenReceiver` without spawning a full `TokenManager`.
    ///
    /// # Arguments
    ///
    /// * `receiver` - The watch receiver to wrap
    #[must_use]
    pub fn from_watch_receiver(receiver: watch::Receiver<SecretString>) -> Self {
        Self(receiver)
    }

    /// Get the current token.
    ///
    /// This always clones the token to avoid blocking the sender.
    /// After `spawn_token_manager` returns, this is guaranteed to return
    /// a valid (non-empty) token.
    #[must_use]
    pub fn token(&self) -> SecretString {
        self.0.borrow().clone()
    }

    /// Wait for the token to change.
    ///
    /// This is useful for reacting to token refresh events, though most
    /// callers should just use `token()` directly.
    ///
    /// # Errors
    ///
    /// Returns `TokenError::ChannelClosed` if the sender is dropped.
    pub async fn changed(&mut self) -> Result<(), TokenError> {
        self.0
            .changed()
            .await
            .map_err(|_| TokenError::ChannelClosed)
    }
}

impl std::fmt::Debug for TokenReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenReceiver")
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl TokenReceiver {
    /// Create a `TokenReceiver` from a watch channel for testing purposes.
    ///
    /// **Note**: This is only for testing. In production, use `spawn_token_manager`
    /// which ensures the token is valid before returning.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use common::secret::SecretString;
    /// use common::token_manager::TokenReceiver;
    /// use tokio::sync::watch;
    ///
    /// let (tx, rx) = watch::channel(SecretString::from("test-token"));
    /// let receiver = TokenReceiver::from_test_channel(rx);
    /// ```
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn from_test_channel(rx: watch::Receiver<SecretString>) -> Self {
        Self(rx)
    }
}

// =============================================================================
// OAuth Response Types
// =============================================================================

/// OAuth 2.0 token response from AC.
#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    expires_in: u64,
    #[allow(dead_code)]
    #[serde(default)]
    scope: Option<String>,
}

impl std::fmt::Debug for OAuthTokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokenResponse")
            .field("access_token", &"[REDACTED]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .finish()
    }
}

// =============================================================================
// Token Manager Function
// =============================================================================

/// Spawn the token manager background task.
///
/// This function:
/// 1. Creates a watch channel with an empty string sentinel
/// 2. Spawns a background task that acquires the initial token (infinite retry)
/// 3. Waits for the first real token before returning
/// 4. Returns `(JoinHandle, TokenReceiver)` where receiver always has a valid token
///
/// The background task runs forever, refreshing tokens before expiration.
/// To stop it, either drop the `JoinHandle` or call `abort()` on it.
///
/// # Arguments
///
/// * `config` - Token manager configuration
///
/// # Errors
///
/// - `TokenError::Configuration` - If the HTTP client cannot be built
/// - `TokenError::ChannelClosed` - If the channel closes before first token (shouldn't happen)
///
/// # Panics
///
/// This function does not panic. All errors are returned via `Result`.
#[instrument(skip_all)]
pub async fn spawn_token_manager(
    config: TokenManagerConfig,
) -> Result<(JoinHandle<()>, TokenReceiver), TokenError> {
    // Build HTTP client
    let http_client = reqwest::Client::builder()
        .timeout(config.http_timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .build()
        .map_err(|e| TokenError::Configuration(format!("Failed to build HTTP client: {e}")))?;

    // Create watch channel with empty string sentinel
    let (sender, mut receiver) = watch::channel(SecretString::from(""));

    // Spawn background task - it owns config, http_client, and sender
    let task_handle = tokio::spawn(async move {
        token_refresh_loop(config, http_client, sender).await;
    });

    // Wait for first real token
    receiver
        .changed()
        .await
        .map_err(|_| TokenError::ChannelClosed)?;

    // Verify token is not empty (defensive check)
    if receiver.borrow().expose_secret().is_empty() {
        return Err(TokenError::AcquisitionFailed(
            "Token is empty after acquisition".into(),
        ));
    }

    Ok((task_handle, TokenReceiver(receiver)))
}

/// Background token refresh loop.
///
/// This function runs forever, acquiring and refreshing tokens as needed.
/// It handles all retry logic with exponential backoff.
#[instrument(skip_all)]
async fn token_refresh_loop(
    config: TokenManagerConfig,
    http_client: reqwest::Client,
    sender: watch::Sender<SecretString>,
) {
    let mut backoff = INITIAL_BACKOFF_MS;
    let mut expires_at: Option<i64> = None;
    let mut initial_acquisition = true;

    loop {
        // Check if we need to refresh
        // We add CLOCK_DRIFT_MARGIN_SECS to account for potential clock differences
        // between this host and the AC server
        let needs_refresh = match expires_at {
            Some(exp) => {
                let now = chrono::Utc::now().timestamp();
                #[allow(clippy::cast_possible_wrap)]
                let threshold_secs = config.refresh_threshold.as_secs() as i64;
                // Add clock drift margin for safety
                exp - now <= threshold_secs + CLOCK_DRIFT_MARGIN_SECS
            }
            None => true, // No token yet
        };

        if needs_refresh {
            // Time the acquire_token call only (excludes backoff sleep)
            let refresh_start = Instant::now();
            let result = acquire_token(&config, &http_client).await;
            let refresh_duration = refresh_start.elapsed();

            match result {
                Ok((token, new_expires_at)) => {
                    // Invoke callback for successful refresh
                    if let Some(ref callback) = config.on_refresh {
                        callback(TokenRefreshEvent {
                            success: true,
                            duration: refresh_duration,
                            error_category: None,
                        });
                    }

                    // Update expiration tracking
                    expires_at = Some(new_expires_at);

                    // Send new token to receivers
                    if sender.send(token).is_err() {
                        // All receivers dropped, exit loop
                        debug!(
                            target: "common.token_manager",
                            client_id = %config.client_id,
                            "All receivers dropped, stopping refresh loop"
                        );
                        break;
                    }

                    if initial_acquisition {
                        info!(
                            target: "common.token_manager",
                            client_id = %config.client_id,
                            "Initial token acquired successfully"
                        );
                        initial_acquisition = false;
                    } else {
                        debug!(
                            target: "common.token_manager",
                            client_id = %config.client_id,
                            "Token refreshed successfully"
                        );
                    }

                    // Reset backoff on success
                    backoff = INITIAL_BACKOFF_MS;
                }
                Err(e) => {
                    // Invoke callback for failed refresh
                    if let Some(ref callback) = config.on_refresh {
                        callback(TokenRefreshEvent {
                            success: false,
                            duration: refresh_duration,
                            error_category: Some(error_category(&e)),
                        });
                    }

                    warn!(
                        target: "common.token_manager",
                        client_id = %config.client_id,
                        error = %e,
                        backoff_ms = backoff,
                        "Token acquisition failed, will retry"
                    );

                    // Wait with exponential backoff, then retry
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF_MS);
                    continue;
                }
            }
        }

        // Calculate sleep duration until next refresh
        // We subtract clock drift margin to ensure we wake up early enough
        let sleep_duration = match expires_at {
            Some(exp) => {
                let now = chrono::Utc::now().timestamp();
                #[allow(clippy::cast_possible_wrap)]
                let threshold_secs = config.refresh_threshold.as_secs() as i64;
                // Account for clock drift margin when calculating wake time
                let refresh_at = exp - threshold_secs - CLOCK_DRIFT_MARGIN_SECS;
                let sleep_secs = (refresh_at - now).max(1);
                #[allow(clippy::cast_sign_loss)]
                Duration::from_secs(sleep_secs as u64)
            }
            None => {
                // Should not happen after successful acquisition, but be safe
                Duration::from_secs(1)
            }
        };

        tokio::time::sleep(sleep_duration).await;
    }
}

/// Acquire a new token from AC.
///
/// Returns the token and its expiration time (Unix timestamp).
#[instrument(skip_all)]
async fn acquire_token(
    config: &TokenManagerConfig,
    http_client: &reqwest::Client,
) -> Result<(SecretString, i64), TokenError> {
    let url = format!("{}/api/v1/auth/service/token", config.ac_endpoint);

    debug!(
        target: "common.token_manager",
        client_id = %config.client_id,
        url = %url,
        "Requesting token from AC"
    );

    // Build JSON body for client credentials grant
    // AC expects JSON (application/json), not form-urlencoded
    let json_body = serde_json::json!({
        "grant_type": "client_credentials",
        "client_id": config.client_id,
        "client_secret": config.client_secret.expose_secret(),
    });

    let response = http_client
        .post(&url)
        .json(&json_body)
        .send()
        .await
        .map_err(|e| {
            debug!(target: "common.token_manager", error = %e, "HTTP request failed");
            TokenError::HttpError(e.to_string())
        })?;

    let status = response.status();

    if status.is_success() {
        let token_response: OAuthTokenResponse = response.json().await.map_err(|e| {
            warn!(target: "common.token_manager", error = %e, "Failed to parse token response");
            TokenError::InvalidResponse(e.to_string())
        })?;

        // Calculate expiration time
        let now = chrono::Utc::now().timestamp();
        #[allow(clippy::cast_possible_wrap)]
        let expires_at = now + token_response.expires_in as i64;

        debug!(
            target: "common.token_manager",
            expires_in_secs = token_response.expires_in,
            "Token acquired successfully"
        );

        Ok((SecretString::from(token_response.access_token), expires_at))
    } else if status.as_u16() == 401 || status.as_u16() == 400 {
        // Read response body for diagnostics, but only log at trace level
        // to avoid leaking sensitive information in production logs
        let body = response.text().await.unwrap_or_else(|e| {
            trace!(target: "common.token_manager", error = %e, "Failed to read error response body");
            "<failed to read body>".to_string()
        });
        warn!(
            target: "common.token_manager",
            status = %status,
            "Authentication rejected by AC"
        );
        // Log body at trace level only (not included in error message for security)
        trace!(
            target: "common.token_manager",
            body = %body,
            "Authentication rejection response body"
        );
        Err(TokenError::AuthenticationRejected(format!(
            "Status {status}"
        )))
    } else if status.is_server_error() {
        warn!(
            target: "common.token_manager",
            status = %status,
            "AC returned server error"
        );
        Err(TokenError::HttpError(format!("AC server error: {status}")))
    } else {
        warn!(
            target: "common.token_manager",
            status = %status,
            "Unexpected response from AC"
        );
        Err(TokenError::HttpError(format!(
            "Unexpected status: {status}"
        )))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(base_url: &str) -> TokenManagerConfig {
        TokenManagerConfig::new(
            base_url.to_string(),
            "test-client".to_string(),
            SecretString::from("test-secret"),
        )
    }

    // =========================================================================
    // Configuration Tests
    // =========================================================================

    #[test]
    fn test_config_defaults() {
        let config = TokenManagerConfig::new(
            "http://localhost:8082".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        );

        assert_eq!(config.refresh_threshold, DEFAULT_REFRESH_THRESHOLD);
        assert_eq!(config.http_timeout, DEFAULT_HTTP_TIMEOUT);
    }

    #[test]
    fn test_config_builder() {
        let config = TokenManagerConfig::new(
            "http://localhost:8082".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        )
        .with_refresh_threshold(Duration::from_secs(60))
        .with_http_timeout(Duration::from_secs(5));

        assert_eq!(config.refresh_threshold, Duration::from_secs(60));
        assert_eq!(config.http_timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_config_debug_redacts_secret() {
        let config = TokenManagerConfig::new(
            "http://localhost:8082".to_string(),
            "client".to_string(),
            SecretString::from("super-secret-value"),
        );

        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("super-secret-value"));
    }

    #[test]
    fn test_constants_are_durations() {
        // Verify constants are Duration type and have correct values
        assert_eq!(DEFAULT_REFRESH_THRESHOLD.as_secs(), 300);
        assert_eq!(DEFAULT_HTTP_TIMEOUT.as_secs(), 10);
    }

    // =========================================================================
    // Token Receiver Tests
    // =========================================================================

    #[test]
    fn test_token_receiver_debug_redacts() {
        let (_tx, rx) = watch::channel(SecretString::from("secret-token"));
        let receiver = TokenReceiver(rx);

        let debug_str = format!("{receiver:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("secret-token"));
    }

    #[test]
    fn test_token_receiver_clone() {
        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let receiver = TokenReceiver(rx);
        let cloned = receiver.clone();

        assert_eq!(
            receiver.token().expose_secret(),
            cloned.token().expose_secret()
        );
    }

    // =========================================================================
    // Token Acquisition Tests
    // =========================================================================

    #[tokio::test]
    async fn test_spawn_token_manager_success() {
        let mock_server = MockServer::start().await;

        // TokenManager now sends JSON instead of form-urlencoded
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .and(body_string_contains(
                "\"grant_type\":\"client_credentials\"",
            ))
            .and(body_string_contains("\"client_id\":\"test-client\""))
            .and(body_string_contains("\"client_secret\":\"test-secret\""))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "acquired-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .expect(1..)
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());
        let result = spawn_token_manager(config).await;

        assert!(result.is_ok(), "spawn_token_manager should succeed");

        let (handle, receiver) = result.unwrap();

        // Token should be immediately available
        let token = receiver.token();
        assert_eq!(token.expose_secret(), "acquired-token");

        // Cleanup
        handle.abort();
    }

    #[tokio::test]
    async fn test_token_receiver_always_valid_after_spawn() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "valid-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());
        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        // Multiple calls should all return valid token
        for _ in 0..10 {
            let token = receiver.token();
            assert!(!token.expose_secret().is_empty());
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_token_receiver_clone_works() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "cloned-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());
        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        // Clone and use in multiple places
        let rx1 = receiver.clone();
        let rx2 = receiver.clone();

        assert_eq!(rx1.token().expose_secret(), "cloned-token");
        assert_eq!(rx2.token().expose_secret(), "cloned-token");

        handle.abort();
    }

    #[tokio::test]
    async fn test_retry_on_500_error() {
        let mock_server = MockServer::start().await;

        // First requests fail with 500, then succeed
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "retry-success-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());

        // Should eventually succeed after retries
        let result = spawn_token_manager(config).await;
        assert!(result.is_ok());

        let (handle, receiver) = result.unwrap();
        assert_eq!(receiver.token().expose_secret(), "retry-success-token");

        handle.abort();
    }

    #[tokio::test]
    async fn test_token_refresh_after_expiry() {
        let mock_server = MockServer::start().await;

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::Relaxed);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": format!("token-{}", count),
                    "token_type": "Bearer",
                    // Very short expiry to trigger refresh
                    "expires_in": 2
                }))
            })
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_refresh_threshold(Duration::from_secs(1)); // Refresh 1 second before expiry

        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        // First token
        let token1 = receiver.token();
        assert!(token1.expose_secret().starts_with("token-"));

        // Wait for refresh (token expires in 2s, refresh threshold is 1s)
        // So refresh should happen after ~1 second
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Should have refreshed
        assert!(
            call_count.load(Ordering::Relaxed) >= 2,
            "Expected at least 2 token requests, got {}",
            call_count.load(Ordering::Relaxed)
        );

        // Can still get token
        let token2 = receiver.token();
        assert!(!token2.expose_secret().is_empty());

        handle.abort();
    }

    #[tokio::test]
    async fn test_changed_notification() {
        let mock_server = MockServer::start().await;

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::Relaxed);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": format!("token-{}", count),
                    "token_type": "Bearer",
                    "expires_in": 1 // Very short for quick refresh
                }))
            })
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_refresh_threshold(Duration::from_secs(0)); // Immediate refresh

        let (handle, mut receiver) = spawn_token_manager(config).await.unwrap();

        // Wait for change notification
        let timeout = tokio::time::timeout(Duration::from_secs(5), receiver.changed()).await;

        assert!(timeout.is_ok(), "Should receive change notification");
        assert!(timeout.unwrap().is_ok(), "Changed should succeed");

        handle.abort();
    }

    // =========================================================================
    // Error Type Tests
    // =========================================================================

    #[test]
    fn test_token_error_display() {
        let err = TokenError::AcquisitionFailed("test error".to_string());
        assert!(err.to_string().contains("test error"));

        let err = TokenError::HttpError("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));

        let err = TokenError::AuthenticationRejected("401 Unauthorized".to_string());
        assert!(err.to_string().contains("401 Unauthorized"));

        let err = TokenError::InvalidResponse("invalid json".to_string());
        assert!(err.to_string().contains("invalid json"));

        let err = TokenError::Configuration("bad config".to_string());
        assert!(err.to_string().contains("bad config"));

        let err = TokenError::ChannelClosed;
        assert!(err.to_string().contains("closed"));
    }

    #[test]
    fn test_token_error_clone() {
        let err = TokenError::AcquisitionFailed("test".to_string());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    // =========================================================================
    // Abort/Cleanup Tests
    // =========================================================================

    #[tokio::test]
    async fn test_abort_handle_stops_task() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());
        let (handle, _receiver) = spawn_token_manager(config).await.unwrap();

        // Abort the task
        handle.abort();

        // Wait a bit and verify it stopped (no panic)
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // =========================================================================
    // Security Tests (Code Review Findings)
    // =========================================================================

    #[test]
    fn test_new_secure_requires_https() {
        // HTTPS should work
        let result = TokenManagerConfig::new_secure(
            "https://ac.example.com".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        );
        assert!(result.is_ok());

        // HTTP should fail
        let result = TokenManagerConfig::new_secure(
            "http://ac.example.com".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenError::Configuration(_)));
    }

    #[test]
    fn test_oauth_response_debug_redacts_token() {
        let response = OAuthTokenResponse {
            access_token: "super-secret-access-token".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            scope: Some("read write".to_string()),
        };

        let debug_str = format!("{response:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("super-secret-access-token"));
        // Other fields should still be visible
        assert!(debug_str.contains("Bearer"));
        assert!(debug_str.contains("3600"));
    }

    #[tokio::test]
    async fn test_401_authentication_rejected() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string(r#"{"error": "invalid_client"}"#),
            )
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(500));

        // The spawn will retry forever on 401, so we need to use a timeout
        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        // Should timeout because 401 triggers infinite retry
        assert!(result.is_err(), "Should timeout on 401 (infinite retry)");
    }

    #[tokio::test]
    async fn test_400_authentication_rejected() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_string(r#"{"error": "invalid_request"}"#),
            )
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(500));

        // The spawn will retry forever on 400, so we need to use a timeout
        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        // Should timeout because 400 triggers infinite retry
        assert!(result.is_err(), "Should timeout on 400 (infinite retry)");
    }

    #[tokio::test]
    async fn test_invalid_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json at all"))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(500));

        // Invalid JSON triggers retry with backoff
        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        // Should timeout because invalid JSON triggers infinite retry
        assert!(
            result.is_err(),
            "Should timeout on invalid JSON (infinite retry)"
        );
    }

    #[tokio::test]
    async fn test_missing_oauth_fields() {
        let mock_server = MockServer::start().await;

        // Missing required field: access_token
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token_type": "Bearer",
                "expires_in": 3600
                // missing: "access_token"
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(500));

        // Missing fields trigger retry with backoff
        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        // Should timeout because missing fields triggers infinite retry
        assert!(
            result.is_err(),
            "Should timeout on missing fields (infinite retry)"
        );
    }

    #[tokio::test]
    async fn test_zero_expires_in_handled() {
        let mock_server = MockServer::start().await;

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(move |_: &wiremock::Request| {
                let count = call_count_clone.fetch_add(1, Ordering::Relaxed);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": format!("token-{}", count),
                    "token_type": "Bearer",
                    // Edge case: zero expiry
                    "expires_in": 0
                }))
            })
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_refresh_threshold(Duration::from_secs(0));

        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        // Token should still be acquired
        let token = receiver.token();
        assert!(token.expose_secret().starts_with("token-"));

        // With zero expires_in, should refresh immediately
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should have refreshed multiple times
        assert!(
            call_count.load(Ordering::Relaxed) >= 2,
            "Expected at least 2 token requests with zero expires_in, got {}",
            call_count.load(Ordering::Relaxed)
        );

        handle.abort();
    }

    #[tokio::test]
    async fn test_channel_closed_error() {
        let (tx, rx) = watch::channel(SecretString::from("test"));
        let mut receiver = TokenReceiver(rx);

        // Drop the sender
        drop(tx);

        // changed() should return ChannelClosed
        let result = receiver.changed().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenError::ChannelClosed));
    }

    #[tokio::test]
    async fn test_http_timeout_error() {
        let mock_server = MockServer::start().await;

        // Simulate slow response (longer than timeout)
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "access_token": "slow-token",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                    .set_delay(Duration::from_secs(5)), // Delay longer than timeout
            )
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(100)); // Very short timeout

        // Should timeout and retry forever
        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        // Should timeout because HTTP timeout triggers retry
        assert!(
            result.is_err(),
            "Should timeout on HTTP timeout (infinite retry)"
        );
    }

    #[tokio::test]
    async fn test_backoff_timing() {
        let mock_server = MockServer::start().await;

        let request_times = Arc::new(std::sync::Mutex::new(Vec::new()));
        let request_times_clone = request_times.clone();

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(move |_: &wiremock::Request| {
                let now = std::time::Instant::now();
                request_times_clone.lock().unwrap().push(now);

                // First 3 requests fail, 4th succeeds
                let times = request_times_clone.lock().unwrap();
                if times.len() <= 3 {
                    ResponseTemplate::new(500)
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "access_token": "backoff-success",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                }
            })
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());

        let start = std::time::Instant::now();
        let result = spawn_token_manager(config).await;
        let total_duration = start.elapsed();

        assert!(result.is_ok(), "Should eventually succeed");

        let (handle, receiver) = result.unwrap();
        assert_eq!(receiver.token().expose_secret(), "backoff-success");

        // With exponential backoff (1s, 2s, 4s), total should be at least 3s
        // (after 3 failures before success)
        // Note: We're lenient here because timing can vary
        assert!(
            total_duration >= Duration::from_secs(3),
            "Expected at least 3s for backoff, got {total_duration:?}"
        );

        handle.abort();
    }

    // =========================================================================
    // Constants Tests
    // =========================================================================

    #[test]
    fn test_connect_timeout_constant() {
        // Verify the constant exists and has reasonable value
        assert_eq!(DEFAULT_CONNECT_TIMEOUT.as_secs(), 5);
    }

    #[test]
    fn test_clock_drift_margin_constant() {
        // Verify clock drift margin is reasonable (not too large, not zero)
        // Using assert_eq! to avoid clippy::assertions_on_constants
        assert_eq!(
            CLOCK_DRIFT_MARGIN_SECS, 30,
            "Clock drift margin should be 30 seconds"
        );
    }

    // =========================================================================
    // from_url() Tests (Critical for OAuth flow - PR coverage requirement)
    // =========================================================================

    #[test]
    fn test_from_url_with_https() {
        let config = TokenManagerConfig::from_url(
            "https://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect("HTTPS URL should be valid");

        assert_eq!(config.ac_endpoint, "https://ac-service:8082");
        assert_eq!(config.client_id, "client-id");
    }

    #[test]
    fn test_from_url_with_http() {
        let config = TokenManagerConfig::from_url(
            "http://localhost:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect("HTTP URL should be valid for local dev");

        assert_eq!(config.ac_endpoint, "http://localhost:8082");
        assert_eq!(config.client_id, "client-id");
    }

    #[test]
    fn test_from_url_with_invalid_scheme() {
        let err = TokenManagerConfig::from_url(
            "ftp://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("FTP scheme should be rejected");

        assert!(matches!(err, TokenError::Configuration(_)));
    }

    #[test]
    fn test_from_url_with_no_scheme() {
        let err = TokenManagerConfig::from_url(
            "ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("URL without scheme should be rejected");

        assert!(matches!(err, TokenError::Configuration(_)));
        assert!(err.to_string().contains("expected http:// or https://"));
    }

    #[test]
    fn test_from_url_with_empty_string() {
        let err = TokenManagerConfig::from_url(
            String::new(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("Empty URL should be rejected");

        assert!(matches!(err, TokenError::Configuration(_)));
    }

    #[test]
    fn test_from_url_with_websocket_scheme() {
        let err = TokenManagerConfig::from_url(
            "ws://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("ws:// scheme should be rejected");
        assert!(matches!(err, TokenError::Configuration(_)));

        let err = TokenManagerConfig::from_url(
            "wss://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("wss:// scheme should be rejected");
        assert!(matches!(err, TokenError::Configuration(_)));
    }

    #[test]
    fn test_from_url_case_sensitivity() {
        let err = TokenManagerConfig::from_url(
            "HTTP://localhost:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("HTTP (uppercase) should be rejected");
        assert!(matches!(err, TokenError::Configuration(_)));

        let err = TokenManagerConfig::from_url(
            "HTTPS://localhost:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect_err("HTTPS (uppercase) should be rejected");
        assert!(matches!(err, TokenError::Configuration(_)));
    }

    #[test]
    fn test_from_url_preserves_defaults() {
        let config = TokenManagerConfig::from_url(
            "https://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect("Valid HTTPS URL");

        assert_eq!(config.refresh_threshold, DEFAULT_REFRESH_THRESHOLD);
        assert_eq!(config.http_timeout, DEFAULT_HTTP_TIMEOUT);
    }

    #[test]
    fn test_from_url_builder_pattern_works() {
        let config = TokenManagerConfig::from_url(
            "https://ac-service:8082".to_string(),
            "client-id".to_string(),
            SecretString::from("secret"),
        )
        .expect("Valid HTTPS URL")
        .with_refresh_threshold(Duration::from_secs(120))
        .with_http_timeout(Duration::from_secs(5));

        assert_eq!(config.refresh_threshold, Duration::from_secs(120));
        assert_eq!(config.http_timeout, Duration::from_secs(5));
    }

    // =========================================================================
    // TokenReceiver::from_watch_receiver() Tests
    // =========================================================================

    #[test]
    fn test_from_watch_receiver_creates_working_receiver() {
        let (_tx, rx) = watch::channel(SecretString::from("test-token-123"));
        let receiver = TokenReceiver::from_watch_receiver(rx);

        assert_eq!(receiver.token().expose_secret(), "test-token-123");
    }

    #[test]
    fn test_from_watch_receiver_clone_works() {
        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let receiver = TokenReceiver::from_watch_receiver(rx);
        let cloned = receiver.clone();

        assert_eq!(
            receiver.token().expose_secret(),
            cloned.token().expose_secret()
        );
    }

    #[tokio::test]
    async fn test_from_watch_receiver_changed_works() {
        let (tx, rx) = watch::channel(SecretString::from("initial"));
        let mut receiver = TokenReceiver::from_watch_receiver(rx);

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = tx.send(SecretString::from("updated"));
        });

        tokio::time::timeout(Duration::from_secs(1), receiver.changed())
            .await
            .expect("Should receive change notification within timeout")
            .expect("Changed should succeed");

        assert_eq!(receiver.token().expose_secret(), "updated");
    }

    // =========================================================================
    // Additional Error Path Coverage
    // =========================================================================

    #[tokio::test]
    async fn test_unexpected_status_code() {
        let mock_server = MockServer::start().await;

        // Test various unexpected status codes (should all trigger retry)
        for status in [418, 301, 302, 403, 404, 429] {
            Mock::given(method("POST"))
                .and(path("/api/v1/auth/service/token"))
                .respond_with(ResponseTemplate::new(status))
                .up_to_n_times(1)
                .mount(&mock_server)
                .await;
        }

        let config = test_config(&mock_server.uri()).with_http_timeout(Duration::from_millis(500));

        let result =
            tokio::time::timeout(Duration::from_secs(2), spawn_token_manager(config)).await;

        assert!(
            result.is_err(),
            "Should timeout on unexpected status codes (infinite retry)"
        );
    }

    #[tokio::test]
    async fn test_empty_token_in_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let config = test_config(&mock_server.uri());
        let err = spawn_token_manager(config)
            .await
            .expect_err("Empty token should be rejected");

        assert!(matches!(err, TokenError::AcquisitionFailed(_)));
    }

    // =========================================================================
    // Callback Mechanism Tests
    // =========================================================================

    #[test]
    fn test_error_category_mapping() {
        assert_eq!(
            error_category(&TokenError::HttpError("test".to_string())),
            "http"
        );
        assert_eq!(
            error_category(&TokenError::AuthenticationRejected("test".to_string())),
            "auth_rejected"
        );
        assert_eq!(
            error_category(&TokenError::InvalidResponse("test".to_string())),
            "invalid_response"
        );
        assert_eq!(
            error_category(&TokenError::AcquisitionFailed("test".to_string())),
            "acquisition_failed"
        );
        assert_eq!(
            error_category(&TokenError::Configuration("test".to_string())),
            "configuration"
        );
        assert_eq!(error_category(&TokenError::ChannelClosed), "channel_closed");
    }

    #[tokio::test]
    async fn test_callback_invoked_on_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "callback-test-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let events: Arc<std::sync::Mutex<Vec<TokenRefreshEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let config = test_config(&mock_server.uri()).with_on_refresh(Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
        }));

        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        // Token should be available
        assert_eq!(receiver.token().expose_secret(), "callback-test-token");

        // Callback should have been invoked at least once with success
        let captured = events.lock().unwrap();
        assert!(!captured.is_empty(), "Callback should have been invoked");

        let success_event = captured
            .iter()
            .find(|e| e.success)
            .expect("Expected a success event");
        assert!(success_event.duration > Duration::ZERO);
        assert!(success_event.error_category.is_none());

        handle.abort();
    }

    #[tokio::test]
    async fn test_callback_invoked_on_failure_then_success() {
        let mock_server = MockServer::start().await;

        // First 2 requests fail with 500, then succeed
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "recovered-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let events: Arc<std::sync::Mutex<Vec<TokenRefreshEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let config = test_config(&mock_server.uri()).with_on_refresh(Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
        }));

        let (handle, receiver) = spawn_token_manager(config).await.unwrap();

        assert_eq!(receiver.token().expose_secret(), "recovered-token");

        let captured = events.lock().unwrap();

        // Should have failure events (from 500s) and at least one success
        let failures: Vec<_> = captured.iter().filter(|e| !e.success).collect();
        let successes: Vec<_> = captured.iter().filter(|e| e.success).collect();

        assert!(
            !failures.is_empty(),
            "Should have at least one failure event"
        );
        assert!(
            !successes.is_empty(),
            "Should have at least one success event"
        );

        // Verify failure events have correct error category
        for failure in &failures {
            assert_eq!(
                failure.error_category,
                Some("http"),
                "500 errors should map to 'http' category"
            );
            assert!(failure.duration > Duration::ZERO);
        }

        handle.abort();
    }

    #[tokio::test]
    async fn test_no_callback_still_works() {
        // Verify existing behavior is unchanged when on_refresh is None
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/auth/service/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "no-callback-token",
                "token_type": "Bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        // Config without on_refresh (default None)
        let config = test_config(&mock_server.uri());
        assert!(config.on_refresh.is_none());

        let (handle, receiver) = spawn_token_manager(config).await.unwrap();
        assert_eq!(receiver.token().expose_secret(), "no-callback-token");

        handle.abort();
    }

    #[test]
    fn test_config_debug_includes_callback_info() {
        let config_without = TokenManagerConfig::new(
            "http://localhost:8082".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        );
        let debug_str = format!("{config_without:?}");
        assert!(debug_str.contains("on_refresh: None"));

        let config_with = config_without.with_on_refresh(Arc::new(|_| {}));
        let debug_str = format!("{config_with:?}");
        assert!(debug_str.contains("<callback>"));
    }

    #[test]
    fn test_with_on_refresh_builder() {
        let config = TokenManagerConfig::new(
            "http://localhost:8082".to_string(),
            "client".to_string(),
            SecretString::from("secret"),
        )
        .with_on_refresh(Arc::new(|_| {}));

        assert!(config.on_refresh.is_some());
    }

    #[test]
    fn test_token_refresh_event_debug() {
        let event = TokenRefreshEvent {
            success: true,
            duration: Duration::from_millis(42),
            error_category: None,
        };
        let debug_str = format!("{event:?}");
        assert!(debug_str.contains("success: true"));
        assert!(debug_str.contains("error_category: None"));

        let error_event = TokenRefreshEvent {
            success: false,
            duration: Duration::from_millis(100),
            error_category: Some("http"),
        };
        let debug_str = format!("{error_event:?}");
        assert!(debug_str.contains("success: false"));
        assert!(debug_str.contains("http"));
    }
}
