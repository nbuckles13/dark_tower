//! Global Controller gRPC Client.
//!
//! Provides a client for MC→GC communication per ADR-0023 Phase 6c:
//! - Registration on startup
//! - Fast heartbeat (10s) - capacity updates
//! - Comprehensive heartbeat (30s) - full metrics
//!
//! # Security
//!
//! - Service tokens authenticate MC to GC
//! - Channel is cheaply cloneable (backed by tower_buffer::Buffer with mpsc)
//! - tonic handles reconnection internally
//! - Exponential backoff for retries
//!
//! # Connection Pattern
//!
//! The tonic `Channel` is designed to be cloned cheaply and used concurrently.
//! From the docs: "Channel provides a Clone implementation that is cheap".
//! No locking is needed - just clone the channel for each request.

use crate::config::Config;
use crate::errors::McError;
use common::secret::{ExposeSecret, SecretString};
use proto_gen::internal::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::internal::{
    ComprehensiveHeartbeatRequest, ControllerCapacity, FastHeartbeatRequest, HealthStatus,
    RegisterMcRequest,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::{debug, error, info, instrument, warn};

/// Default timeout for GC RPC calls.
const GC_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout.
const GC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default fast heartbeat interval (10 seconds per ADR-0010).
const DEFAULT_FAST_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// Default comprehensive heartbeat interval (30 seconds per ADR-0010).
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Maximum retries for registration.
const MAX_REGISTRATION_RETRIES: u32 = 5;

/// Base delay for exponential backoff.
const BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Maximum backoff delay.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// GC client with connection management.
///
/// Uses a tonic `Channel` which is cheaply cloneable and handles
/// connection management internally. No locking needed.
pub struct GcClient {
    /// gRPC channel to GC (cheaply cloneable, handles reconnection).
    channel: Channel,
    /// Service token for authenticating to GC (protected by SecretString).
    service_token: SecretString,
    /// MC configuration.
    config: Config,
    /// Whether registration has succeeded.
    is_registered: AtomicBool,
    /// Fast heartbeat interval from GC (or default).
    fast_heartbeat_interval_ms: AtomicU64,
    /// Comprehensive heartbeat interval from GC (or default).
    comprehensive_heartbeat_interval_ms: AtomicU64,
}

impl GcClient {
    /// Create a new GC client with eager channel initialization.
    ///
    /// # Arguments
    ///
    /// * `gc_endpoint` - gRPC endpoint of the Global Controller
    /// * `service_token` - MC's service token for authenticating to GC
    /// * `config` - MC configuration
    ///
    /// # Errors
    ///
    /// Returns `McError::Config` if the endpoint is invalid.
    /// Returns `McError::Grpc` if the initial connection fails.
    ///
    /// # Note
    ///
    /// The channel is created eagerly at startup (fail fast). tonic's `Channel`
    /// is cheaply cloneable and handles reconnection internally, so no locking
    /// is needed for concurrent use.
    pub async fn new(
        gc_endpoint: String,
        service_token: SecretString,
        config: Config,
    ) -> Result<Self, McError> {
        let channel = Endpoint::from_shared(gc_endpoint.clone())
            .map_err(|e| {
                error!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    endpoint = %gc_endpoint,
                    "Invalid GC endpoint"
                );
                McError::Config(format!("Invalid GC endpoint: {e}"))
            })?
            .connect_timeout(GC_CONNECT_TIMEOUT)
            .timeout(GC_RPC_TIMEOUT)
            .connect()
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    endpoint = %gc_endpoint,
                    "Failed to connect to GC"
                );
                McError::Grpc(format!("Failed to connect to GC: {e}"))
            })?;

        Ok(Self {
            channel,
            service_token,
            config,
            is_registered: AtomicBool::new(false),
            fast_heartbeat_interval_ms: AtomicU64::new(
                DEFAULT_FAST_HEARTBEAT_INTERVAL.as_millis() as u64
            ),
            comprehensive_heartbeat_interval_ms: AtomicU64::new(
                DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL.as_millis() as u64,
            ),
        })
    }

    /// Add authorization header to a request.
    fn add_auth<T>(&self, request: T) -> Result<Request<T>, McError> {
        let mut grpc_request = Request::new(request);
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.service_token.expose_secret())
                .parse()
                .map_err(|_| {
                    error!(target: "mc.grpc.gc_client", "Invalid service token format");
                    McError::Config("Invalid service token format".to_string())
                })?,
        );
        Ok(grpc_request)
    }

    /// Register with the Global Controller.
    ///
    /// Called on startup. Retries with exponential backoff on failure.
    /// Note: Since the channel handles reconnection internally, we just
    /// retry the RPC on failure without clearing the channel.
    ///
    /// # Errors
    ///
    /// Returns `McError::Config` if registration fails after all retries.
    #[instrument(skip(self), fields(mc_id = %self.config.mc_id, region = %self.config.region))]
    pub async fn register(&self) -> Result<(), McError> {
        let request = RegisterMcRequest {
            id: self.config.mc_id.clone(),
            region: self.config.region.clone(),
            grpc_endpoint: format!(
                "http://{}",
                self.config
                    .grpc_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            webtransport_endpoint: format!(
                "https://{}",
                self.config
                    .webtransport_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            max_meetings: self.config.max_meetings,
            max_participants: self.config.max_participants,
        };

        let mut retry_count = 0;
        let mut delay = BACKOFF_BASE;

        loop {
            match self.try_register(&request).await {
                Ok(response) => {
                    if response.accepted {
                        info!(
                            target: "mc.grpc.gc_client",
                            message = %response.message,
                            fast_heartbeat_ms = response.fast_heartbeat_interval_ms,
                            comprehensive_heartbeat_ms = response.comprehensive_heartbeat_interval_ms,
                            "Successfully registered with GC"
                        );

                        // Store intervals from GC
                        if response.fast_heartbeat_interval_ms > 0 {
                            self.fast_heartbeat_interval_ms
                                .store(response.fast_heartbeat_interval_ms, Ordering::SeqCst);
                        }
                        if response.comprehensive_heartbeat_interval_ms > 0 {
                            self.comprehensive_heartbeat_interval_ms.store(
                                response.comprehensive_heartbeat_interval_ms,
                                Ordering::SeqCst,
                            );
                        }

                        self.is_registered.store(true, Ordering::SeqCst);
                        return Ok(());
                    } else {
                        error!(
                            target: "mc.grpc.gc_client",
                            message = %response.message,
                            "GC rejected registration"
                        );
                        return Err(McError::Config(format!(
                            "GC rejected registration: {}",
                            response.message
                        )));
                    }
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= MAX_REGISTRATION_RETRIES {
                        error!(
                            target: "mc.grpc.gc_client",
                            error = %e,
                            retries = retry_count,
                            "Registration failed after max retries"
                        );
                        return Err(e);
                    }

                    warn!(
                        target: "mc.grpc.gc_client",
                        error = %e,
                        retry_count = retry_count,
                        delay_ms = delay.as_millis(),
                        "Registration failed, retrying"
                    );

                    // Exponential backoff (tonic Channel handles reconnection internally)
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    /// Attempt a single registration call.
    async fn try_register(
        &self,
        request: &RegisterMcRequest,
    ) -> Result<proto_gen::internal::RegisterMcResponse, McError> {
        // Clone the channel (cheap operation) for this request
        let mut client = GlobalControllerServiceClient::new(self.channel.clone());
        let grpc_request = self.add_auth(request.clone())?;

        client
            .register_mc(grpc_request)
            .await
            .map(|r| r.into_inner())
            .map_err(|e| {
                warn!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    "RegisterMC RPC failed"
                );
                McError::Grpc(format!("RegisterMC RPC failed: {e}"))
            })
    }

    /// Send a fast heartbeat (capacity update).
    ///
    /// Called every 10 seconds (or interval specified by GC).
    ///
    /// # Arguments
    ///
    /// * `current_meetings` - Current number of active meetings
    /// * `current_participants` - Current total participants
    /// * `health` - Current health status
    #[instrument(skip(self), fields(mc_id = %self.config.mc_id))]
    pub async fn fast_heartbeat(
        &self,
        current_meetings: u32,
        current_participants: u32,
        health: HealthStatus,
    ) -> Result<(), McError> {
        if !self.is_registered.load(Ordering::SeqCst) {
            debug!(target: "mc.grpc.gc_client", "Skipping heartbeat - not registered");
            return Ok(());
        }

        let request = FastHeartbeatRequest {
            controller_id: self.config.mc_id.clone(),
            capacity: Some(ControllerCapacity {
                max_meetings: self.config.max_meetings,
                current_meetings,
                max_participants: self.config.max_participants,
                current_participants,
            }),
            health: health.into(),
        };

        // Clone the channel (cheap operation) for this request
        let mut client = GlobalControllerServiceClient::new(self.channel.clone());
        let grpc_request = self.add_auth(request)?;

        match client.fast_heartbeat(grpc_request).await {
            Ok(response) => {
                let inner = response.into_inner();
                if inner.acknowledged {
                    debug!(
                        target: "mc.grpc.gc_client",
                        timestamp = inner.timestamp,
                        "Fast heartbeat acknowledged"
                    );
                }
                Ok(())
            }
            Err(e) => {
                warn!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    "Fast heartbeat failed"
                );
                // tonic Channel handles reconnection internally, no need to clear
                Err(McError::Grpc(format!("Fast heartbeat failed: {e}")))
            }
        }
    }

    /// Send a comprehensive heartbeat (full metrics).
    ///
    /// Called every 30 seconds (or interval specified by GC).
    ///
    /// # Arguments
    ///
    /// * `current_meetings` - Current number of active meetings
    /// * `current_participants` - Current total participants
    /// * `health` - Current health status
    /// * `cpu_usage_percent` - CPU usage (0-100)
    /// * `memory_usage_percent` - Memory usage (0-100)
    #[instrument(skip(self), fields(mc_id = %self.config.mc_id))]
    pub async fn comprehensive_heartbeat(
        &self,
        current_meetings: u32,
        current_participants: u32,
        health: HealthStatus,
        cpu_usage_percent: f32,
        memory_usage_percent: f32,
    ) -> Result<(), McError> {
        if !self.is_registered.load(Ordering::SeqCst) {
            debug!(target: "mc.grpc.gc_client", "Skipping heartbeat - not registered");
            return Ok(());
        }

        let request = ComprehensiveHeartbeatRequest {
            controller_id: self.config.mc_id.clone(),
            capacity: Some(ControllerCapacity {
                max_meetings: self.config.max_meetings,
                current_meetings,
                max_participants: self.config.max_participants,
                current_participants,
            }),
            health: health.into(),
            cpu_usage_percent,
            memory_usage_percent,
        };

        // Clone the channel (cheap operation) for this request
        let mut client = GlobalControllerServiceClient::new(self.channel.clone());
        let grpc_request = self.add_auth(request)?;

        match client.comprehensive_heartbeat(grpc_request).await {
            Ok(response) => {
                let inner = response.into_inner();
                if inner.acknowledged {
                    debug!(
                        target: "mc.grpc.gc_client",
                        timestamp = inner.timestamp,
                        cpu = cpu_usage_percent,
                        memory = memory_usage_percent,
                        "Comprehensive heartbeat acknowledged"
                    );
                }
                Ok(())
            }
            Err(e) => {
                warn!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    "Comprehensive heartbeat failed"
                );
                // tonic Channel handles reconnection internally, no need to clear
                Err(McError::Grpc(format!(
                    "Comprehensive heartbeat failed: {e}"
                )))
            }
        }
    }

    /// Check if registered with GC.
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.is_registered.load(Ordering::SeqCst)
    }

    /// Get the fast heartbeat interval in milliseconds.
    #[must_use]
    pub fn fast_heartbeat_interval_ms(&self) -> u64 {
        self.fast_heartbeat_interval_ms.load(Ordering::SeqCst)
    }

    /// Get the comprehensive heartbeat interval in milliseconds.
    #[must_use]
    pub fn comprehensive_heartbeat_interval_ms(&self) -> u64 {
        self.comprehensive_heartbeat_interval_ms
            .load(Ordering::SeqCst)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // Note: GcClient::new() is now async and connects eagerly.
    // Tests that require a GcClient instance need a running gRPC server.
    // These tests verify constants and calculations that don't require a client.

    #[test]
    fn test_default_intervals() {
        assert_eq!(DEFAULT_FAST_HEARTBEAT_INTERVAL, Duration::from_secs(10));
        assert_eq!(
            DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL,
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_retry_constants() {
        // Verify retry configuration is reasonable
        assert_eq!(MAX_REGISTRATION_RETRIES, 5);
        assert_eq!(BACKOFF_BASE, Duration::from_secs(1));
        assert_eq!(BACKOFF_MAX, Duration::from_secs(30));
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        // Verify the backoff calculation pattern used in register()
        let base = BACKOFF_BASE;
        let max = BACKOFF_MAX;

        // First retry: 1s
        let delay1 = base;
        assert_eq!(delay1, Duration::from_secs(1));

        // Second retry: 2s
        let delay2 = (delay1 * 2).min(max);
        assert_eq!(delay2, Duration::from_secs(2));

        // Third retry: 4s
        let delay3 = (delay2 * 2).min(max);
        assert_eq!(delay3, Duration::from_secs(4));

        // Fourth retry: 8s
        let delay4 = (delay3 * 2).min(max);
        assert_eq!(delay4, Duration::from_secs(8));

        // Fifth retry: 16s
        let delay5 = (delay4 * 2).min(max);
        assert_eq!(delay5, Duration::from_secs(16));

        // Sixth retry: 32s -> capped at 30s
        let delay6 = (delay5 * 2).min(max);
        assert_eq!(delay6, Duration::from_secs(30));

        // Further retries stay at max
        let delay7 = (delay6 * 2).min(max);
        assert_eq!(delay7, Duration::from_secs(30));
    }

    #[test]
    fn test_rpc_timeout_constants() {
        // Verify timeouts are reasonable for production use
        assert_eq!(GC_RPC_TIMEOUT, Duration::from_secs(10));
        assert_eq!(GC_CONNECT_TIMEOUT, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_new_with_invalid_endpoint() {
        use common::secret::SecretString;

        let config = Config {
            mc_id: "mc-test-001".to_string(),
            region: "us-east-1".to_string(),
            webtransport_bind_address: "0.0.0.0:4433".to_string(),
            grpc_bind_address: "0.0.0.0:50052".to_string(),
            health_bind_address: "0.0.0.0:8081".to_string(),
            redis_url: SecretString::from("redis://localhost:6379"),
            gc_grpc_url: "http://localhost:50051".to_string(),
            max_meetings: 1000,
            max_participants: 10000,
            binding_token_ttl_seconds: 30,
            clock_skew_seconds: 5,
            nonce_grace_window_seconds: 5,
            disconnect_grace_period_seconds: 30,
            binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
        };

        // Empty endpoint should fail with Config error
        let result = GcClient::new(
            String::new(), // Empty string is clearly invalid
            SecretString::from("test-token"),
            config.clone(),
        )
        .await;

        // Should fail with either Config or Grpc error depending on tonic's parsing
        let is_expected_error = matches!(result, Err(McError::Config(_)) | Err(McError::Grpc(_)));
        assert!(is_expected_error, "Expected Config or Grpc error");
    }

    #[tokio::test]
    async fn test_new_with_unreachable_endpoint() {
        use common::secret::SecretString;

        let config = Config {
            mc_id: "mc-test-001".to_string(),
            region: "us-east-1".to_string(),
            webtransport_bind_address: "0.0.0.0:4433".to_string(),
            grpc_bind_address: "0.0.0.0:50052".to_string(),
            health_bind_address: "0.0.0.0:8081".to_string(),
            redis_url: SecretString::from("redis://localhost:6379"),
            gc_grpc_url: "http://localhost:50051".to_string(),
            max_meetings: 1000,
            max_participants: 10000,
            binding_token_ttl_seconds: 30,
            clock_skew_seconds: 5,
            nonce_grace_window_seconds: 5,
            disconnect_grace_period_seconds: 30,
            binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
        };

        // Valid endpoint but no server running - should fail with Grpc error
        let result = GcClient::new(
            "http://127.0.0.1:59999".to_string(), // Unlikely to have a server
            SecretString::from("test-token"),
            config,
        )
        .await;

        let is_expected_error = matches!(
            &result,
            Err(McError::Grpc(msg)) if msg.contains("Failed to connect to GC")
        );
        assert!(
            is_expected_error,
            "Expected Grpc error with 'Failed to connect to GC'"
        );
    }
}
