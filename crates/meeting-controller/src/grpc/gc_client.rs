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
//! - Connection reuse via channel caching
//! - Automatic reconnection on failure
//! - Exponential backoff for retries

use crate::config::Config;
use crate::errors::McError;
use common::secret::{ExposeSecret, SecretString};
use proto_gen::internal::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::internal::{
    ComprehensiveHeartbeatRequest, ControllerCapacity, FastHeartbeatRequest, HealthStatus,
    RegisterMcRequest,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::{debug, error, info, instrument, warn};

/// Default timeout for GC RPC calls in seconds.
const GC_RPC_TIMEOUT_SECS: u64 = 10;

/// Default connect timeout in seconds.
const GC_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Default fast heartbeat interval (10 seconds per ADR-0010).
const DEFAULT_FAST_HEARTBEAT_INTERVAL_MS: u64 = 10_000;

/// Default comprehensive heartbeat interval (30 seconds per ADR-0010).
const DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS: u64 = 30_000;

/// Maximum retries for registration.
const MAX_REGISTRATION_RETRIES: u32 = 5;

/// Base delay for exponential backoff in milliseconds.
const BACKOFF_BASE_MS: u64 = 1000;

/// Maximum backoff delay in milliseconds.
const BACKOFF_MAX_MS: u64 = 30_000;

/// GC client with connection management.
///
/// Maintains a cached gRPC channel and handles registration/heartbeat.
pub struct GcClient {
    /// GC gRPC endpoint.
    gc_endpoint: String,
    /// Cached channel.
    channel: Arc<RwLock<Option<Channel>>>,
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
    /// Create a new GC client.
    ///
    /// # Arguments
    ///
    /// * `gc_endpoint` - gRPC endpoint of the Global Controller
    /// * `service_token` - MC's service token for authenticating to GC
    /// * `config` - MC configuration
    #[must_use]
    pub fn new(gc_endpoint: String, service_token: SecretString, config: Config) -> Self {
        Self {
            gc_endpoint,
            channel: Arc::new(RwLock::new(None)),
            service_token,
            config,
            is_registered: AtomicBool::new(false),
            fast_heartbeat_interval_ms: AtomicU64::new(DEFAULT_FAST_HEARTBEAT_INTERVAL_MS),
            comprehensive_heartbeat_interval_ms: AtomicU64::new(
                DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS,
            ),
        }
    }

    /// Get or create a channel to the GC.
    async fn get_channel(&self) -> Result<Channel, McError> {
        // Check cache first
        {
            let channel_guard = self.channel.read().await;
            if let Some(channel) = channel_guard.as_ref() {
                return Ok(channel.clone());
            }
        }

        // Create new channel
        let channel = Endpoint::from_shared(self.gc_endpoint.clone())
            .map_err(|e| {
                error!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    endpoint = %self.gc_endpoint,
                    "Invalid GC endpoint"
                );
                McError::Config(format!("Invalid GC endpoint: {e}"))
            })?
            .connect_timeout(Duration::from_secs(GC_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(GC_RPC_TIMEOUT_SECS))
            .connect()
            .await
            .map_err(|e| {
                warn!(
                    target: "mc.grpc.gc_client",
                    error = %e,
                    endpoint = %self.gc_endpoint,
                    "Failed to connect to GC"
                );
                McError::Grpc(format!("Failed to connect to GC: {e}"))
            })?;

        // Cache the channel
        {
            let mut channel_guard = self.channel.write().await;
            *channel_guard = Some(channel.clone());
        }

        Ok(channel)
    }

    /// Clear the cached channel (on connection failure).
    async fn clear_channel(&self) {
        let mut channel_guard = self.channel.write().await;
        *channel_guard = None;
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
        let mut delay_ms = BACKOFF_BASE_MS;

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
                        delay_ms = delay_ms,
                        "Registration failed, retrying"
                    );

                    // Clear channel to force reconnection
                    self.clear_channel().await;

                    // Exponential backoff
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms * 2).min(BACKOFF_MAX_MS);
                }
            }
        }
    }

    /// Attempt a single registration call.
    async fn try_register(
        &self,
        request: &RegisterMcRequest,
    ) -> Result<proto_gen::internal::RegisterMcResponse, McError> {
        let channel = self.get_channel().await?;
        let mut client = GlobalControllerServiceClient::new(channel);
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

        let channel = self.get_channel().await?;
        let mut client = GlobalControllerServiceClient::new(channel);
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
                // Clear channel to force reconnection on next call
                self.clear_channel().await;
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

        let channel = self.get_channel().await?;
        let mut client = GlobalControllerServiceClient::new(channel);
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
                // Clear channel to force reconnection on next call
                self.clear_channel().await;
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

    fn test_config() -> Config {
        Config {
            mc_id: "mc-test-001".to_string(),
            region: "us-east-1".to_string(),
            webtransport_bind_address: "0.0.0.0:4433".to_string(),
            grpc_bind_address: "0.0.0.0:50052".to_string(),
            health_bind_address: "0.0.0.0:8081".to_string(),
            redis_url: SecretString::from("redis://localhost:6379"),
            gc_grpc_url: "http://localhost:50051".to_string(),
            gc_grpc_endpoint: "http://localhost:50051".to_string(),
            max_meetings: 1000,
            max_participants: 10000,
            binding_token_ttl_seconds: 30,
            clock_skew_seconds: 5,
            nonce_grace_window_seconds: 5,
            disconnect_grace_period_seconds: 30,
            binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
        }
    }

    #[test]
    fn test_gc_client_new() {
        let client = GcClient::new(
            "http://localhost:50051".to_string(),
            SecretString::from("test-token"),
            test_config(),
        );
        assert!(!client.is_registered());
        assert_eq!(
            client.fast_heartbeat_interval_ms(),
            DEFAULT_FAST_HEARTBEAT_INTERVAL_MS
        );
        assert_eq!(
            client.comprehensive_heartbeat_interval_ms(),
            DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS
        );
    }

    #[test]
    fn test_default_intervals() {
        assert_eq!(DEFAULT_FAST_HEARTBEAT_INTERVAL_MS, 10_000);
        assert_eq!(DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS, 30_000);
    }

    #[test]
    fn test_retry_constants() {
        // Verify retry configuration is reasonable
        assert_eq!(MAX_REGISTRATION_RETRIES, 5);
        assert_eq!(BACKOFF_BASE_MS, 1000);
        assert_eq!(BACKOFF_MAX_MS, 30_000);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        // Verify the backoff calculation pattern used in register()
        let base = BACKOFF_BASE_MS;
        let max = BACKOFF_MAX_MS;

        // First retry: 1000ms
        let delay1 = base;
        assert_eq!(delay1, 1000);

        // Second retry: 2000ms
        let delay2 = (delay1 * 2).min(max);
        assert_eq!(delay2, 2000);

        // Third retry: 4000ms
        let delay3 = (delay2 * 2).min(max);
        assert_eq!(delay3, 4000);

        // Fourth retry: 8000ms
        let delay4 = (delay3 * 2).min(max);
        assert_eq!(delay4, 8000);

        // Fifth retry: 16000ms
        let delay5 = (delay4 * 2).min(max);
        assert_eq!(delay5, 16000);

        // Sixth retry: 32000ms -> capped at 30000ms
        let delay6 = (delay5 * 2).min(max);
        assert_eq!(delay6, 30_000);

        // Further retries stay at max
        let delay7 = (delay6 * 2).min(max);
        assert_eq!(delay7, 30_000);
    }

    #[tokio::test]
    async fn test_heartbeat_skipped_when_not_registered() {
        let client = GcClient::new(
            "http://localhost:50051".to_string(),
            SecretString::from("test-token"),
            test_config(),
        );

        // Not registered yet
        assert!(!client.is_registered());

        // Fast heartbeat should return Ok but not actually do anything
        let result = client.fast_heartbeat(0, 0, HealthStatus::Healthy).await;
        assert!(result.is_ok());

        // Comprehensive heartbeat should also return Ok but skip
        let result = client
            .comprehensive_heartbeat(0, 0, HealthStatus::Healthy, 50.0, 60.0)
            .await;
        assert!(result.is_ok());

        // Still not registered (heartbeats don't change registration state)
        assert!(!client.is_registered());
    }

    #[test]
    fn test_gc_client_interval_accessors() {
        let client = GcClient::new(
            "http://localhost:50051".to_string(),
            SecretString::from("test-token"),
            test_config(),
        );

        // Initial values should be defaults
        assert_eq!(
            client.fast_heartbeat_interval_ms(),
            DEFAULT_FAST_HEARTBEAT_INTERVAL_MS
        );
        assert_eq!(
            client.comprehensive_heartbeat_interval_ms(),
            DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL_MS
        );

        // Manually update via atomics (simulating what register() does)
        client
            .fast_heartbeat_interval_ms
            .store(5000, std::sync::atomic::Ordering::SeqCst);
        client
            .comprehensive_heartbeat_interval_ms
            .store(15000, std::sync::atomic::Ordering::SeqCst);

        assert_eq!(client.fast_heartbeat_interval_ms(), 5000);
        assert_eq!(client.comprehensive_heartbeat_interval_ms(), 15000);
    }

    #[tokio::test]
    async fn test_channel_caching() {
        let client = GcClient::new(
            "http://localhost:50051".to_string(),
            SecretString::from("test-token"),
            test_config(),
        );

        // Initially no channel cached
        {
            let cache = client.channel.read().await;
            assert!(cache.is_none());
        }

        // After clear_channel, still None
        client.clear_channel().await;
        {
            let cache = client.channel.read().await;
            assert!(cache.is_none());
        }
    }

    #[test]
    fn test_rpc_timeout_constants() {
        // Verify timeouts are reasonable for production use
        assert_eq!(GC_RPC_TIMEOUT_SECS, 10);
        assert_eq!(GC_CONNECT_TIMEOUT_SECS, 5);
    }
}
