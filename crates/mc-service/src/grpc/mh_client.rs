//! Media Handler gRPC Client.
//!
//! Provides a client for MC->MH communication:
//! - `RegisterMeeting` - Notify MH about a new meeting assignment
//!
//! # Security
//!
//! - OAuth 2.0 tokens authenticate MC to MH (via TokenReceiver)
//! - Tokens are automatically refreshed by TokenManager background task
//! - Each call creates a new Channel (MH endpoints vary per meeting)
//!
//! # Connection Pattern
//!
//! Unlike GcClient (singleton channel), MhClient creates a Channel per call
//! because different meetings may be assigned to different MH instances.

use crate::errors::McError;
use crate::observability::metrics::record_register_meeting;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::internal::media_handler_service_client::MediaHandlerServiceClient;
use proto_gen::internal::RegisterMeetingRequest;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tonic::transport::Endpoint;
use tonic::Request;
use tracing::{debug, error, instrument, warn};

/// Default timeout for MH RPC calls.
const MH_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout for MH.
const MH_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Trait for MC->MH meeting registration.
///
/// Abstraction over the gRPC call used to notify MH instances about
/// new meeting assignments. Production code uses `MhClient`;
/// tests can inject a mock to verify call arguments and simulate failures.
pub trait MhRegistrationClient: Send + Sync {
    /// Register a meeting with an MH instance.
    fn register_meeting<'a>(
        &'a self,
        mh_grpc_endpoint: &'a str,
        meeting_id: &'a str,
        mc_id: &'a str,
        mc_grpc_endpoint: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>>;
}

/// MH client for RegisterMeeting RPCs.
///
/// Holds a `TokenReceiver` for Bearer auth. Creates a new gRPC channel
/// per call since MH endpoints vary per meeting assignment.
pub struct MhClient {
    /// Token receiver for dynamically refreshed OAuth tokens.
    token_rx: TokenReceiver,
}

impl MhClient {
    /// Create a new MH client.
    ///
    /// # Arguments
    ///
    /// * `token_rx` - Token receiver for dynamically refreshed OAuth tokens
    #[must_use]
    pub fn new(token_rx: TokenReceiver) -> Self {
        Self { token_rx }
    }

    /// Register a meeting with an MH instance.
    ///
    /// Creates a new gRPC channel to the specified MH endpoint and sends
    /// a `RegisterMeeting` RPC. The MH validates the Bearer token
    /// cryptographically via JWKS.
    ///
    /// # Arguments
    ///
    /// * `mh_grpc_endpoint` - gRPC endpoint of the target MH
    /// * `meeting_id` - Meeting being registered
    /// * `mc_id` - This MC's identifier
    /// * `mc_grpc_endpoint` - This MC's gRPC endpoint (for MH->MC callbacks)
    ///
    /// # Errors
    ///
    /// Returns `McError::Config` if the endpoint is invalid.
    /// Returns `McError::Grpc` if the connection or RPC fails.
    #[instrument(skip_all, fields(meeting_id = %meeting_id), target = "mc.grpc.mh_client")]
    pub async fn register_meeting(
        &self,
        mh_grpc_endpoint: &str,
        meeting_id: &str,
        mc_id: &str,
        mc_grpc_endpoint: &str,
    ) -> Result<(), McError> {
        // Create channel to the specific MH endpoint
        let channel = Endpoint::from_shared(mh_grpc_endpoint.to_string())
            .map_err(|e| {
                error!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    "Invalid MH endpoint"
                );
                McError::Config(format!("Invalid MH endpoint: {e}"))
            })?
            .connect_timeout(MH_CONNECT_TIMEOUT)
            .timeout(MH_RPC_TIMEOUT)
            .connect()
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "Failed to connect to MH"
                );
                McError::Grpc(format!("Failed to connect to MH: {e}"))
            })?;

        let mut client = MediaHandlerServiceClient::new(channel);

        let request = RegisterMeetingRequest {
            meeting_id: meeting_id.to_string(),
            mc_id: mc_id.to_string(),
            mc_grpc_endpoint: mc_grpc_endpoint.to_string(),
        };

        let grpc_request = self.add_auth(request)?;

        let start = Instant::now();
        match client.register_meeting(grpc_request).await {
            Ok(response) => {
                let duration = start.elapsed();
                let inner = response.into_inner();
                if inner.accepted {
                    record_register_meeting("success", duration);
                    debug!(
                        target: "mc.grpc.mh_client",
                        meeting_id = %meeting_id,
                        "MH accepted meeting registration"
                    );
                    Ok(())
                } else {
                    record_register_meeting("error", duration);
                    warn!(
                        target: "mc.grpc.mh_client",
                        meeting_id = %meeting_id,
                        "MH rejected meeting registration"
                    );
                    Err(McError::Grpc(
                        "MH rejected meeting registration".to_string(),
                    ))
                }
            }
            Err(e) => {
                let duration = start.elapsed();
                record_register_meeting("error", duration);

                warn!(
                    target: "mc.grpc.mh_client",
                    error = %e,
                    meeting_id = %meeting_id,
                    "RegisterMeeting RPC failed"
                );
                Err(McError::Grpc(format!("RegisterMeeting RPC failed: {e}")))
            }
        }
    }

    /// Add authorization header to a request.
    fn add_auth<T>(&self, request: T) -> Result<Request<T>, McError> {
        let mut grpc_request = Request::new(request);
        let current_token = self.token_rx.token();
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", current_token.expose_secret())
                .parse()
                .map_err(|e| {
                    error!(
                        target: "mc.grpc.mh_client",
                        error = %e,
                        "Authorization header parse failed"
                    );
                    McError::Config(format!("Authorization header parse failed: {e}"))
                })?,
        );
        Ok(grpc_request)
    }
}

impl MhRegistrationClient for MhClient {
    fn register_meeting<'a>(
        &'a self,
        mh_grpc_endpoint: &'a str,
        meeting_id: &'a str,
        mc_id: &'a str,
        mc_grpc_endpoint: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>> {
        Box::pin(self.register_meeting(mh_grpc_endpoint, meeting_id, mc_id, mc_grpc_endpoint))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mh_rpc_timeout_constants() {
        assert_eq!(MH_RPC_TIMEOUT, Duration::from_secs(10));
        assert_eq!(MH_CONNECT_TIMEOUT, Duration::from_secs(5));
    }

    /// Helper to create a mock TokenReceiver for testing.
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
    fn test_mh_client_creation() {
        let token_rx = mock_token_receiver();
        let _client = MhClient::new(token_rx);
    }

    #[tokio::test]
    async fn test_register_meeting_invalid_endpoint() {
        let token_rx = mock_token_receiver();
        let client = MhClient::new(token_rx);

        let result = client
            .register_meeting("", "meeting-1", "mc-1", "http://mc:50052")
            .await;

        assert!(
            matches!(&result, Err(McError::Config(_)) | Err(McError::Grpc(_))),
            "Expected Config or Grpc error, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_register_meeting_unreachable_endpoint() {
        let token_rx = mock_token_receiver();
        let client = MhClient::new(token_rx);

        let result = client
            .register_meeting(
                "http://127.0.0.1:59998",
                "meeting-1",
                "mc-1",
                "http://mc:50052",
            )
            .await;

        assert!(
            matches!(&result, Err(McError::Grpc(msg)) if msg.contains("Failed to connect")),
            "Expected Grpc connection error, got: {:?}",
            result
        );
    }
}
