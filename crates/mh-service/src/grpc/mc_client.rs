//! Meeting Controller gRPC Client for Media Handler.
//!
//! Provides a client for MH→MC communication per R-16/R-17:
//! - `NotifyParticipantConnected` — inform MC when a participant establishes
//!   a WebTransport connection to this MH
//! - `NotifyParticipantDisconnected` — inform MC when a participant's
//!   WebTransport connection drops
//!
//! # Security (ADR-0003)
//!
//! - OAuth 2.0 tokens authenticate MH to MC (via `TokenReceiver`)
//! - Tokens are automatically refreshed by `TokenManager` background task
//! - Token values are never logged
//!
//! # Connection Pattern
//!
//! Like MC's `MhClient`, `McClient` creates a new Channel per call because
//! different meetings may be assigned to different MC instances, each with
//! its own gRPC endpoint.

use crate::errors::MhError;
use crate::observability::metrics;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::internal::media_coordination_service_client::MediaCoordinationServiceClient;
use proto_gen::internal::{ParticipantMediaConnected, ParticipantMediaDisconnected};
use std::time::Duration;
use tonic::transport::Endpoint;
use tonic::Request;
use tracing::{debug, error, instrument, warn};

/// Default timeout for MC RPC calls.
const MC_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout for MC.
const MC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of retry attempts for notification delivery.
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Base delay for exponential backoff between retries.
const RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// MC client for MH→MC notification RPCs.
///
/// Holds a `TokenReceiver` for Bearer auth. Creates a new gRPC channel
/// per call since MC endpoints vary per meeting assignment.
pub struct McClient {
    /// Token receiver for dynamically refreshed OAuth tokens.
    token_rx: TokenReceiver,
}

impl McClient {
    /// Create a new MC client.
    ///
    /// # Arguments
    ///
    /// * `token_rx` - Token receiver for dynamically refreshed OAuth tokens
    #[must_use]
    pub fn new(token_rx: TokenReceiver) -> Self {
        Self { token_rx }
    }

    /// Notify MC that a participant has connected to this MH.
    ///
    /// Creates a new gRPC channel to the specified MC endpoint and sends
    /// a `NotifyParticipantConnected` RPC. The MC validates the Bearer token
    /// cryptographically via JWKS.
    ///
    /// # Arguments
    ///
    /// * `mc_grpc_endpoint` - gRPC endpoint of the target MC
    /// * `meeting_id` - Meeting the participant connected to
    /// * `participant_id` - Participant who connected (from JWT `sub` claim)
    /// * `handler_id` - This MH instance's identifier
    ///
    /// # Errors
    ///
    /// Returns `MhError::Config` if the endpoint is invalid.
    /// Returns `MhError::Grpc` if the connection or RPC fails.
    #[instrument(skip_all, fields(meeting_id = %meeting_id), target = "mh.grpc.mc_client")]
    pub async fn notify_participant_connected(
        &self,
        mc_grpc_endpoint: &str,
        meeting_id: &str,
        participant_id: &str,
        handler_id: &str,
    ) -> Result<(), MhError> {
        let request = ParticipantMediaConnected {
            meeting_id: meeting_id.to_string(),
            participant_id: participant_id.to_string(),
            handler_id: handler_id.to_string(),
        };

        self.send_with_retry(
            mc_grpc_endpoint,
            meeting_id,
            "connected",
            |mut client, req| {
                Box::pin(async move { client.notify_participant_connected(req).await })
            },
            request,
        )
        .await
    }

    /// Notify MC that a participant has disconnected from this MH.
    ///
    /// Creates a new gRPC channel to the specified MC endpoint and sends
    /// a `NotifyParticipantDisconnected` RPC. The MC validates the Bearer token
    /// cryptographically via JWKS.
    ///
    /// # Arguments
    ///
    /// * `mc_grpc_endpoint` - gRPC endpoint of the target MC
    /// * `meeting_id` - Meeting the participant disconnected from
    /// * `participant_id` - Participant who disconnected
    /// * `handler_id` - This MH instance's identifier
    /// * `reason` - Disconnect reason (proto `DisconnectReason` enum value)
    ///
    /// # Errors
    ///
    /// Returns `MhError::Config` if the endpoint is invalid.
    /// Returns `MhError::Grpc` if the connection or RPC fails.
    #[instrument(skip_all, fields(meeting_id = %meeting_id), target = "mh.grpc.mc_client")]
    pub async fn notify_participant_disconnected(
        &self,
        mc_grpc_endpoint: &str,
        meeting_id: &str,
        participant_id: &str,
        handler_id: &str,
        reason: i32,
    ) -> Result<(), MhError> {
        let request = ParticipantMediaDisconnected {
            meeting_id: meeting_id.to_string(),
            participant_id: participant_id.to_string(),
            handler_id: handler_id.to_string(),
            reason,
        };

        self.send_with_retry(
            mc_grpc_endpoint,
            meeting_id,
            "disconnected",
            |mut client, req| {
                Box::pin(async move { client.notify_participant_disconnected(req).await })
            },
            request,
        )
        .await
    }

    /// Send an RPC with retry and exponential backoff.
    ///
    /// Retries up to `MAX_RETRY_ATTEMPTS` times with delays of 1s, 2s, 4s.
    /// Does not retry on `UNAUTHENTICATED` or `PERMISSION_DENIED` (security:
    /// retrying won't fix auth issues, and repeated attempts could trigger
    /// rate limiting).
    async fn send_with_retry<T, R, F>(
        &self,
        mc_grpc_endpoint: &str,
        meeting_id: &str,
        event: &str,
        rpc_fn: F,
        request: T,
    ) -> Result<(), MhError>
    where
        T: Clone + prost::Message,
        R: std::fmt::Debug,
        F: Fn(
            MediaCoordinationServiceClient<tonic::transport::Channel>,
            Request<T>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>> + Send>,
        >,
    {
        let mut delay = RETRY_BASE_DELAY;

        for attempt in 1..=MAX_RETRY_ATTEMPTS {
            match self.try_send(mc_grpc_endpoint, &request, &rpc_fn).await {
                Ok(()) => {
                    metrics::record_mc_notification(event, "success");
                    return Ok(());
                }
                Err(e) => {
                    // Do not retry on auth errors — retrying won't fix them
                    if is_auth_error(&e) {
                        warn!(
                            target: "mh.grpc.mc_client",
                            error = %e,
                            meeting_id = %meeting_id,
                            event = %event,
                            "MC notification auth failure, not retrying"
                        );
                        metrics::record_mc_notification(event, "error");
                        return Err(e);
                    }

                    if attempt == MAX_RETRY_ATTEMPTS {
                        warn!(
                            target: "mh.grpc.mc_client",
                            error = %e,
                            meeting_id = %meeting_id,
                            event = %event,
                            attempts = MAX_RETRY_ATTEMPTS,
                            "MC notification failed after all retries"
                        );
                        metrics::record_mc_notification(event, "error");
                        return Err(e);
                    }

                    debug!(
                        target: "mh.grpc.mc_client",
                        error = %e,
                        meeting_id = %meeting_id,
                        event = %event,
                        attempt = attempt,
                        retry_delay_ms = delay.as_millis(),
                        "MC notification failed, will retry"
                    );

                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }

        // Unreachable: loop always returns
        Ok(())
    }

    /// Attempt a single RPC call to MC.
    async fn try_send<T, R, F>(
        &self,
        mc_grpc_endpoint: &str,
        request: &T,
        rpc_fn: &F,
    ) -> Result<(), MhError>
    where
        T: Clone + prost::Message,
        R: std::fmt::Debug,
        F: Fn(
            MediaCoordinationServiceClient<tonic::transport::Channel>,
            Request<T>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>> + Send>,
        >,
    {
        // Create channel to the specific MC endpoint
        let channel = Endpoint::from_shared(mc_grpc_endpoint.to_string())
            .map_err(|e| {
                error!(
                    target: "mh.grpc.mc_client",
                    error = %e,
                    "Invalid MC endpoint"
                );
                MhError::Config(format!("Invalid MC endpoint: {e}"))
            })?
            .connect_timeout(MC_CONNECT_TIMEOUT)
            .timeout(MC_RPC_TIMEOUT)
            .connect()
            .await
            .map_err(|e| {
                debug!(
                    target: "mh.grpc.mc_client",
                    error = %e,
                    "Failed to connect to MC"
                );
                MhError::Grpc(format!("Failed to connect to MC: {e}"))
            })?;

        let client = MediaCoordinationServiceClient::new(channel);

        let grpc_request = self.add_auth(request.clone())?;

        rpc_fn(client, grpc_request).await.map_err(|status| {
            debug!(
                target: "mh.grpc.mc_client",
                error = %status,
                code = ?status.code(),
                "MC notification RPC failed"
            );
            // Map auth status codes to JwtValidation so retry logic can
            // distinguish them without fragile string matching (S-1).
            if status.code() == tonic::Code::Unauthenticated
                || status.code() == tonic::Code::PermissionDenied
            {
                return MhError::JwtValidation("MC rejected service token".to_string());
            }
            MhError::Grpc(format!("MC notification RPC failed: {status}"))
        })?;

        Ok(())
    }

    /// Add authorization header to a request.
    fn add_auth<T>(&self, request: T) -> Result<Request<T>, MhError> {
        let mut grpc_request = Request::new(request);
        let current_token = self.token_rx.token();
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", current_token.expose_secret())
                .parse()
                .map_err(|e| {
                    error!(
                        target: "mh.grpc.mc_client",
                        error = %e,
                        "Authorization header parse failed"
                    );
                    MhError::Config(format!("Authorization header parse failed: {e}"))
                })?,
        );
        Ok(grpc_request)
    }
}

/// Check if an error is an authentication/authorization failure.
///
/// Returns `true` for `JwtValidation` errors (mapped from tonic
/// `UNAUTHENTICATED`/`PERMISSION_DENIED` status codes in `try_send`).
/// These should not be retried — retrying won't fix auth issues.
fn is_auth_error(err: &MhError) -> bool {
    matches!(err, MhError::JwtValidation(_))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mc_rpc_timeout_constants() {
        assert_eq!(MC_RPC_TIMEOUT, Duration::from_secs(10));
        assert_eq!(MC_CONNECT_TIMEOUT, Duration::from_secs(5));
    }

    #[test]
    fn test_retry_constants() {
        assert_eq!(MAX_RETRY_ATTEMPTS, 3);
        assert_eq!(RETRY_BASE_DELAY, Duration::from_secs(1));
    }

    /// Helper to create a mock `TokenReceiver` for testing.
    fn mock_token_receiver() -> TokenReceiver {
        use common::secret::SecretString;
        use std::sync::OnceLock;
        use tokio::sync::watch;

        static TOKEN_SENDER: OnceLock<watch::Sender<SecretString>> = OnceLock::new();

        let sender = TOKEN_SENDER.get_or_init(|| {
            let (tx, _rx) = watch::channel(SecretString::from("test-token"));
            tx
        });

        TokenReceiver::from_test_channel(sender.subscribe())
    }

    #[test]
    fn test_mc_client_creation() {
        let token_rx = mock_token_receiver();
        let _client = McClient::new(token_rx);
    }

    #[tokio::test]
    async fn test_notify_connected_invalid_endpoint() {
        let token_rx = mock_token_receiver();
        let client = McClient::new(token_rx);

        let result = client
            .notify_participant_connected("", "meeting-1", "user-1", "mh-1")
            .await;

        assert!(
            matches!(&result, Err(MhError::Config(_) | MhError::Grpc(_))),
            "Expected Config or Grpc error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_notify_disconnected_invalid_endpoint() {
        let token_rx = mock_token_receiver();
        let client = McClient::new(token_rx);

        let result = client
            .notify_participant_disconnected("", "meeting-1", "user-1", "mh-1", 1)
            .await;

        assert!(
            matches!(&result, Err(MhError::Config(_) | MhError::Grpc(_))),
            "Expected Config or Grpc error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_notify_connected_unreachable_endpoint() {
        let token_rx = mock_token_receiver();
        let client = McClient::new(token_rx);

        let result = client
            .notify_participant_connected("http://127.0.0.1:59997", "meeting-1", "user-1", "mh-1")
            .await;

        assert!(
            matches!(&result, Err(MhError::Grpc(msg)) if msg.contains("Failed to connect")),
            "Expected Grpc connection error, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_notify_disconnected_unreachable_endpoint() {
        let token_rx = mock_token_receiver();
        let client = McClient::new(token_rx);

        let result = client
            .notify_participant_disconnected(
                "http://127.0.0.1:59996",
                "meeting-1",
                "user-1",
                "mh-1",
                1,
            )
            .await;

        assert!(
            matches!(&result, Err(MhError::Grpc(msg)) if msg.contains("Failed to connect")),
            "Expected Grpc connection error, got: {result:?}"
        );
    }

    #[test]
    fn test_is_auth_error_jwt_validation() {
        let err = MhError::JwtValidation("MC rejected service token".to_string());
        assert!(is_auth_error(&err));
    }

    #[test]
    fn test_is_auth_error_other_grpc() {
        let err = MhError::Grpc("RPC failed: connection refused".to_string());
        assert!(!is_auth_error(&err));
    }

    #[test]
    fn test_is_auth_error_non_grpc() {
        let err = MhError::Config("bad config".to_string());
        assert!(!is_auth_error(&err));
    }
}
