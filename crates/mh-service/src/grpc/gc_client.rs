//! Global Controller gRPC Client for Media Handler.
//!
//! Provides a client for MH→GC communication per ADR-0010:
//! - Registration on startup (`RegisterMH`)
//! - Periodic load reports (`SendLoadReport`)
//!
//! # Security (ADR-0003)
//!
//! - OAuth 2.0 tokens authenticate MH to GC (acquired via `TokenManager`)
//! - Tokens are automatically refreshed by `TokenManager` background task
//! - Token values are never logged
//!
//! # Connection Pattern
//!
//! The tonic `Channel` is cheaply cloneable and handles reconnection internally.

use crate::config::Config;
use crate::errors::MhError;
use crate::observability::metrics;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::internal::media_handler_registry_service_client::MediaHandlerRegistryServiceClient;
use proto_gen::internal::{MhLoadReportRequest, RegisterMhRequest};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use tracing::{debug, error, info, instrument, warn};

/// Default timeout for GC RPC calls.
const GC_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Default connect timeout.
const GC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default load report interval (10 seconds per ADR-0010).
const DEFAULT_LOAD_REPORT_INTERVAL: Duration = Duration::from_secs(10);

/// Base delay for exponential backoff.
const BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Maximum backoff delay.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// GC client for MH→GC communication.
///
/// Uses a tonic `Channel` which is cheaply cloneable and handles
/// connection management internally.
pub struct GcClient {
    /// gRPC channel to GC.
    channel: Channel,
    /// Token receiver for dynamically refreshed OAuth tokens.
    token_rx: TokenReceiver,
    /// MH configuration.
    config: Config,
    /// Whether registration has succeeded.
    is_registered: AtomicBool,
    /// Load report interval from GC (or default).
    load_report_interval_ms: AtomicU64,
}

impl GcClient {
    /// Create a new GC client with eager channel initialization.
    ///
    /// # Errors
    ///
    /// Returns `MhError::Config` if the endpoint is invalid.
    /// Returns `MhError::Grpc` if the initial connection fails.
    pub async fn new(
        gc_endpoint: String,
        token_rx: TokenReceiver,
        config: Config,
    ) -> Result<Self, MhError> {
        let channel = Endpoint::from_shared(gc_endpoint.clone())
            .map_err(|e| {
                error!(
                    target: "mh.grpc.gc_client",
                    error = %e,
                    endpoint = %gc_endpoint,
                    "Invalid GC endpoint"
                );
                MhError::Config(format!("Invalid GC endpoint: {e}"))
            })?
            .connect_timeout(GC_CONNECT_TIMEOUT)
            .timeout(GC_RPC_TIMEOUT)
            .connect()
            .await
            .map_err(|e| {
                warn!(
                    target: "mh.grpc.gc_client",
                    error = %e,
                    endpoint = %gc_endpoint,
                    "Failed to connect to GC"
                );
                MhError::Grpc(format!("Failed to connect to GC: {e}"))
            })?;

        Ok(Self {
            channel,
            token_rx,
            config,
            is_registered: AtomicBool::new(false),
            #[expect(
                clippy::cast_possible_truncation,
                reason = "10_000ms constant fits in u64 with no truncation risk"
            )]
            load_report_interval_ms: AtomicU64::new(DEFAULT_LOAD_REPORT_INTERVAL.as_millis() as u64),
        })
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
                    error!(target: "mh.grpc.gc_client", error = %e, "Authorization header parse failed");
                    MhError::Config(format!("Authorization header parse failed: {e}"))
                })?,
        );
        Ok(grpc_request)
    }

    /// Register with the Global Controller.
    ///
    /// Retries with exponential backoff until success or cancellation.
    ///
    /// # Errors
    ///
    /// Returns `MhError::Grpc` if GC explicitly rejects the registration.
    #[instrument(skip_all, fields(handler_id = %self.config.handler_id, region = %self.config.region))]
    pub async fn register(&self) -> Result<(), MhError> {
        let request = RegisterMhRequest {
            handler_id: self.config.handler_id.clone(),
            region: self.config.region.clone(),
            webtransport_endpoint: format!(
                "https://{}",
                self.config
                    .webtransport_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            grpc_endpoint: format!(
                "grpc://{}",
                self.config
                    .grpc_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            max_streams: self.config.max_streams,
        };

        let mut delay = BACKOFF_BASE;

        loop {
            let start = Instant::now();
            match self.try_register(&request).await {
                Ok(response) => {
                    let duration = start.elapsed();
                    metrics::record_gc_registration("success");
                    metrics::record_gc_registration_latency(duration);

                    if response.accepted {
                        info!(
                            target: "mh.grpc.gc_client",
                            message = %response.message,
                            load_report_interval_ms = response.load_report_interval_ms,
                            "Successfully registered with GC"
                        );

                        // Store interval from GC
                        if response.load_report_interval_ms > 0 {
                            self.load_report_interval_ms
                                .store(response.load_report_interval_ms, Ordering::SeqCst);
                        }

                        self.is_registered.store(true, Ordering::SeqCst);
                        return Ok(());
                    }

                    // GC rejected registration
                    warn!(
                        target: "mh.grpc.gc_client",
                        message = %response.message,
                        "GC rejected registration"
                    );
                    return Err(MhError::Grpc(format!(
                        "GC rejected registration: {}",
                        response.message
                    )));
                }
                Err(e) => {
                    let duration = start.elapsed();
                    metrics::record_gc_registration("error");
                    metrics::record_gc_registration_latency(duration);

                    warn!(
                        target: "mh.grpc.gc_client",
                        error = %e,
                        retry_delay_ms = delay.as_millis(),
                        "Registration failed, will retry"
                    );

                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    /// Attempt a single registration RPC.
    async fn try_register(
        &self,
        request: &RegisterMhRequest,
    ) -> Result<proto_gen::internal::RegisterMhResponse, MhError> {
        let grpc_request = self.add_auth(request.clone())?;

        let mut client = MediaHandlerRegistryServiceClient::new(self.channel.clone());

        let response = client.register_mh(grpc_request).await.map_err(|e| {
            debug!(
                target: "mh.grpc.gc_client",
                error = %e,
                "RegisterMH RPC failed"
            );
            MhError::Grpc(format!("RegisterMH failed: {e}"))
        })?;

        Ok(response.into_inner())
    }

    /// Send a load report (heartbeat) to GC.
    ///
    /// # Errors
    ///
    /// Returns `MhError::NotRegistered` if GC returns `NOT_FOUND`.
    /// Returns `MhError::Grpc` for other gRPC errors.
    #[instrument(skip_all, fields(handler_id = %self.config.handler_id))]
    pub async fn send_load_report(&self) -> Result<(), MhError> {
        if !self.is_registered.load(Ordering::SeqCst) {
            debug!(target: "mh.grpc.gc_client", "Skipping heartbeat - not registered");
            return Ok(());
        }

        let request = MhLoadReportRequest {
            handler_id: self.config.handler_id.clone(),
            current_streams: 0, // Stub: no active streams
            health: 1,          // HEALTHY
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            bandwidth_usage_percent: 0.0,
        };

        let start = Instant::now();
        let grpc_request = self.add_auth(request)?;

        let mut client = MediaHandlerRegistryServiceClient::new(self.channel.clone());

        match client.send_load_report(grpc_request).await {
            Ok(response) => {
                let duration = start.elapsed();
                metrics::record_gc_heartbeat("success");
                metrics::record_gc_heartbeat_latency(duration);

                let inner = response.into_inner();
                debug!(
                    target: "mh.grpc.gc_client",
                    acknowledged = inner.acknowledged,
                    timestamp = inner.timestamp,
                    "Load report acknowledged"
                );
                Ok(())
            }
            Err(status) => {
                let duration = start.elapsed();
                metrics::record_gc_heartbeat("error");
                metrics::record_gc_heartbeat_latency(duration);

                if status.code() == tonic::Code::NotFound {
                    self.is_registered.store(false, Ordering::SeqCst);
                    return Err(MhError::NotRegistered);
                }

                Err(MhError::Grpc(format!("SendLoadReport failed: {status}")))
            }
        }
    }

    /// Attempt re-registration after a `NOT_FOUND` heartbeat response.
    ///
    /// Single attempt only — the heartbeat loop will retry if this fails.
    ///
    /// # Errors
    ///
    /// Returns `MhError::Grpc` if registration fails or is rejected.
    pub async fn attempt_reregistration(&self) -> Result<(), MhError> {
        let request = RegisterMhRequest {
            handler_id: self.config.handler_id.clone(),
            region: self.config.region.clone(),
            webtransport_endpoint: format!(
                "https://{}",
                self.config
                    .webtransport_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            grpc_endpoint: format!(
                "grpc://{}",
                self.config
                    .grpc_bind_address
                    .replace("0.0.0.0", "localhost")
            ),
            max_streams: self.config.max_streams,
        };

        match self.try_register(&request).await {
            Ok(response) if response.accepted => {
                info!(target: "mh.grpc.gc_client", "Re-registration successful");
                if response.load_report_interval_ms > 0 {
                    self.load_report_interval_ms
                        .store(response.load_report_interval_ms, Ordering::SeqCst);
                }
                self.is_registered.store(true, Ordering::SeqCst);
                Ok(())
            }
            Ok(response) => Err(MhError::Grpc(format!(
                "GC rejected re-registration: {}",
                response.message
            ))),
            Err(e) => Err(e),
        }
    }

    /// Get the load report interval in milliseconds.
    #[must_use]
    pub fn load_report_interval_ms(&self) -> u64 {
        self.load_report_interval_ms.load(Ordering::SeqCst)
    }

    /// Check if currently registered with GC.
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.is_registered.load(Ordering::SeqCst)
    }
}
