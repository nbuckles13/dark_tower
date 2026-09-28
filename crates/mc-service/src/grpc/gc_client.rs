//! Global Controller gRPC Client.
//!
//! Provides a client for MC→GC communication per ADR-0023 Phase 6c:
//! - Registration on startup
//! - Fast heartbeat (10s) - capacity updates
//! - Comprehensive heartbeat (30s) - full metrics
//!
//! # Security (ADR-0010)
//!
//! - OAuth 2.0 tokens authenticate MC to GC (acquired via TokenManager)
//! - Tokens are automatically refreshed by TokenManager background task
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
use crate::observability::{record_gc_heartbeat, record_gc_heartbeat_latency};
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    ComprehensiveHeartbeatRequest, ControllerCapacity, FastHeartbeatRequest, HealthStatus,
    NotifyMeetingEndedRequest, RegisterMcRequest,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
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
///
/// With exponential backoff (1s -> 2s -> 4s -> ... -> 30s max), this gives
/// approximately 5 minutes of retry time, sufficient for GC rolling updates.
const MAX_REGISTRATION_RETRIES: u32 = 20;

/// Maximum duration for registration attempts.
///
/// Registration will stop after this duration even if retries remain.
/// This handles cases where backoff is slower than expected.
const MAX_REGISTRATION_DURATION: Duration = Duration::from_secs(300); // 5 minutes

/// Base delay for exponential backoff.
const BACKOFF_BASE: Duration = Duration::from_secs(1);

/// Maximum backoff delay.
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Attempts to deliver one meeting-ended notification. Only an attempt that
/// provably never reached GC ([`is_provably_unsent`]: the connect phase failed)
/// is followed by another; any other failure ends the loop (see
/// [`GcClient::notify_meeting_ended`]). With the registration backoff
/// (`BACKOFF_BASE` doubling: 1+2+4+8+16 s between the six attempts) this rides
/// out roughly half a minute of GC refusing connections, which covers a GC pod
/// restart.
pub const MAX_NOTIFY_ATTEMPTS: u32 = 6;

/// Whether a failed GC call provably never reached GC: its error chain
/// contains tonic's [`tonic::ConnectError`], i.e. the TCP/HTTP2 connect phase
/// failed, so no request bytes were written.
///
/// This is decided from the RETURNED `Status`, never from a readiness probe
/// beforehand. After the first successful connect tonic's reconnecting channel
/// reports ready even when a reconnect fails: it parks the connect error and
/// returns it from `call()` (tonic 0.12 `transport/channel/service/reconnect.rs`,
/// the `has_been_connected || is_lazy` branch). A readiness pre-check therefore
/// never sees a GC restart and would leave the retry unreachable.
/// `Status::try_from_error` keeps the
/// transport error as the status's `source`, and that chain carries the
/// `ConnectError`.
#[must_use]
pub fn is_provably_unsent(status: &tonic::Status) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(status);
    while let Some(err) = source {
        if err.is::<tonic::ConnectError>() {
            return true;
        }
        source = err.source();
    }
    false
}

/// GC client with connection management.
///
/// Uses a tonic `Channel` which is cheaply cloneable and handles
/// connection management internally. No locking needed.
///
/// # Token Management (ADR-0010)
///
/// Tokens are obtained from `TokenReceiver` which provides automatically
/// refreshed OAuth 2.0 tokens from the `TokenManager` background task.
/// The receiver is cloneable and thread-safe.
pub struct GcClient {
    /// gRPC channel to GC (cheaply cloneable, handles reconnection).
    channel: Channel,
    /// Token receiver for dynamically refreshed OAuth tokens (ADR-0010).
    token_rx: TokenReceiver,
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
    /// * `token_rx` - Token receiver for dynamically refreshed OAuth tokens (ADR-0010)
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
    ///
    /// # Token Refresh (ADR-0010)
    ///
    /// The `TokenReceiver` provides automatically refreshed tokens from the
    /// `TokenManager` background task. Tokens are acquired on each request
    /// via `token_rx.token()`, ensuring the latest valid token is always used.
    pub async fn new(
        gc_endpoint: String,
        token_rx: TokenReceiver,
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

        Ok(Self::with_channel(channel, token_rx, config))
    }

    /// Build a client over an existing channel (the constructor seam; `new`
    /// is this plus the eager connect). A lazily-connected channel takes the
    /// SAME tonic reconnect branch as an eagerly-connected one whose peer has
    /// since gone away, so tests use it to drive an unreachable GC
    /// deterministically.
    #[must_use]
    pub fn with_channel(channel: Channel, token_rx: TokenReceiver, config: Config) -> Self {
        Self {
            channel,
            token_rx,
            config,
            is_registered: AtomicBool::new(false),
            fast_heartbeat_interval_ms: AtomicU64::new(
                DEFAULT_FAST_HEARTBEAT_INTERVAL.as_millis() as u64
            ),
            comprehensive_heartbeat_interval_ms: AtomicU64::new(
                DEFAULT_COMPREHENSIVE_HEARTBEAT_INTERVAL.as_millis() as u64,
            ),
        }
    }

    /// Add authorization header to a request.
    ///
    /// Retrieves the current token from `TokenReceiver` which provides
    /// automatically refreshed tokens from the `TokenManager` background task.
    fn add_auth<T>(&self, request: T) -> Result<Request<T>, McError> {
        let mut grpc_request = Request::new(request);
        // Get current token from TokenReceiver (always valid after spawn_token_manager)
        let current_token = self.token_rx.token();
        grpc_request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", current_token.expose_secret())
                .parse()
                .map_err(|e| {
                    // Log parse error without token content (e only contains "invalid header value" type message)
                    error!(target: "mc.grpc.gc_client", error = %e, "Authorization header parse failed");
                    McError::Config(format!("Authorization header parse failed: {e}"))
                })?,
        );
        Ok(grpc_request)
    }

    /// Tell GC a meeting has ended, so it stops reusing the meeting's assignment
    /// (ADR-0010 §3: "last participant leaves, MC notifies GC").
    ///
    /// # At-most-once for anything GC may have processed
    ///
    /// `NotifyMeetingEndedRequest` carries only `(meeting_id, region)` and GC
    /// ends whatever assignment row is ACTIVE for it. A duplicate that landed
    /// after the meeting's NEXT incarnation was assigned would end that live
    /// row and split the meeting. So a retry happens ONLY where the request
    /// provably never reached GC: the call failed in its connect phase
    /// ([`is_provably_unsent`]), before anything was sent. Every other failure is
    /// ambiguous — GC may have committed it — and is NOT retried; it is counted,
    /// and the meeting id then reads `MeetingNotFound` until MC restarts
    /// (`MCNotifyMeetingEndedFailing`). An exhausted connect-failure budget is
    /// counted the same way, once. The incarnation-safe fix is tracked in
    /// `docs/TODO.md`, "`NotifyMeetingEnded` is not incarnation-safe".
    ///
    /// Called only after the meeting's handlers are released, by the controller.
    pub async fn notify_meeting_ended(&self, meeting_id: &str) {
        let mut backoff = BACKOFF_BASE;
        for attempt in 1..=MAX_NOTIFY_ATTEMPTS {
            let request = match self.add_auth(NotifyMeetingEndedRequest {
                meeting_id: meeting_id.to_string(),
                region: self.config.region.clone(),
            }) {
                Ok(request) => request,
                Err(e) => {
                    crate::observability::metrics::record_gc_notify_meeting_ended("error");
                    error!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %meeting_id,
                        error = %e,
                        "Could not build NotifyMeetingEnded"
                    );
                    return;
                }
            };
            let mut client = GlobalControllerServiceClient::with_interceptor(
                self.channel.clone(),
                common::observability::otel_grpc::client_interceptor(),
            );
            match client.notify_meeting_ended(request).await {
                Ok(_) => {
                    crate::observability::metrics::record_gc_notify_meeting_ended("success");
                    info!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %meeting_id,
                        attempts = attempt,
                        "GC notified that the meeting ended"
                    );
                }
                Err(status) if is_provably_unsent(&status) => {
                    // Connect phase failed: nothing reached GC, so a retry
                    // cannot duplicate a processed notification.
                    if attempt == MAX_NOTIFY_ATTEMPTS {
                        crate::observability::metrics::record_gc_notify_meeting_ended("error");
                        error!(
                            target: "mc.grpc.gc_client",
                            meeting_id = %meeting_id,
                            attempts = attempt,
                            code = ?status.code(),
                            error = %status.message(),
                            "Could not reach GC to report a meeting ended; GC keeps reusing its \
                             assignment, and joins to this meeting id fail until MC restarts"
                        );
                        return;
                    }
                    warn!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %meeting_id,
                        attempt,
                        code = ?status.code(),
                        error = %status.message(),
                        "GC not reachable for NotifyMeetingEnded; retrying (the request was not sent)"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                    continue;
                }
                Err(status) => {
                    // Possibly sent: GC may have committed it. Never retried.
                    crate::observability::metrics::record_gc_notify_meeting_ended("error");
                    error!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %meeting_id,
                        code = ?status.code(),
                        error = %status.message(),
                        "NotifyMeetingEnded failed after it may have been sent; not retried (GC \
                         may have processed it, and a duplicate could end a later incarnation's \
                         live assignment). If GC did not commit it, joins to this meeting id fail \
                         until MC restarts."
                    );
                }
            }
            return;
        }
    }

    /// Register with the Global Controller.
    ///
    /// Called on startup. Retries with exponential backoff on failure.
    /// Registration continues until:
    /// - Success (GC accepts)
    /// - GC explicitly rejects (e.g., duplicate ID)
    /// - Max retries exceeded (20 retries)
    /// - Max duration exceeded (5 minutes)
    ///
    /// Note: Since the channel handles reconnection internally, we just
    /// retry the RPC on failure without clearing the channel.
    ///
    /// # Errors
    ///
    /// Returns `McError::Config` if registration fails after all retries or timeout.
    #[instrument(skip_all, fields(mc_id = %self.config.mc_id, region = %self.config.region))]
    pub async fn register(&self) -> Result<(), McError> {
        let request = RegisterMcRequest {
            id: self.config.mc_id.clone(),
            region: self.config.region.clone(),
            grpc_endpoint: self.config.grpc_advertise_address.clone(),
            webtransport_endpoint: self.config.webtransport_advertise_address.clone(),
            max_meetings: self.config.max_meetings,
            max_participants: self.config.max_participants,
        };

        let mut retry_count = 0;
        let mut delay = BACKOFF_BASE;
        let deadline = tokio::time::Instant::now() + MAX_REGISTRATION_DURATION;

        loop {
            // Check duration deadline
            if tokio::time::Instant::now() >= deadline {
                error!(
                    target: "mc.grpc.gc_client",
                    duration_secs = MAX_REGISTRATION_DURATION.as_secs(),
                    retries = retry_count,
                    "Registration failed: exceeded maximum duration"
                );
                return Err(McError::Config(format!(
                    "Registration failed after {}s deadline",
                    MAX_REGISTRATION_DURATION.as_secs()
                )));
            }

            match self.try_register(&request).await {
                Ok(response) => {
                    if response.accepted {
                        info!(
                            target: "mc.grpc.gc_client",
                            message = %response.message,
                            fast_heartbeat_ms = response.fast_heartbeat_interval_ms,
                            comprehensive_heartbeat_ms = response.comprehensive_heartbeat_interval_ms,
                            retries = retry_count,
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
                        max_retries = MAX_REGISTRATION_RETRIES,
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
    ) -> Result<proto_gen::dark_tower::internal::v1::RegisterMcResponse, McError> {
        // Clone the channel (cheap operation) for this request
        // R-56: inject the active W3C trace context into outbound GC metadata.
        let mut client = GlobalControllerServiceClient::with_interceptor(
            self.channel.clone(),
            common::observability::otel_grpc::client_interceptor(),
        );
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
    #[instrument(skip_all, fields(mc_id = %self.config.mc_id))]
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
        // R-56: inject the active W3C trace context into outbound GC metadata.
        let mut client = GlobalControllerServiceClient::with_interceptor(
            self.channel.clone(),
            common::observability::otel_grpc::client_interceptor(),
        );
        let grpc_request = self.add_auth(request)?;

        // Start timer for latency measurement (ADR-0011)
        let start = Instant::now();

        match client.fast_heartbeat(grpc_request).await {
            Ok(response) => {
                let duration = start.elapsed();
                // Record success metrics per ADR-0011: both counter and histogram
                record_gc_heartbeat("success", "fast");
                record_gc_heartbeat_latency("fast", duration);

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
                let duration = start.elapsed();
                // Record error metrics per ADR-0011: both counter and histogram
                record_gc_heartbeat("error", "fast");
                record_gc_heartbeat_latency("fast", duration);

                // Check for NOT_FOUND status - means GC doesn't recognize this MC
                // (e.g., after GC restart or network partition)
                if e.code() == tonic::Code::NotFound {
                    warn!(
                        target: "mc.grpc.gc_client",
                        "GC returned NOT_FOUND - MC not registered"
                    );
                    self.is_registered.store(false, Ordering::SeqCst);
                    return Err(McError::NotRegistered);
                }

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
    #[instrument(skip_all, fields(mc_id = %self.config.mc_id))]
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
        // R-56: inject the active W3C trace context into outbound GC metadata.
        let mut client = GlobalControllerServiceClient::with_interceptor(
            self.channel.clone(),
            common::observability::otel_grpc::client_interceptor(),
        );
        let grpc_request = self.add_auth(request)?;

        // Start timer for latency measurement (ADR-0011)
        let start = Instant::now();

        match client.comprehensive_heartbeat(grpc_request).await {
            Ok(response) => {
                let duration = start.elapsed();
                // Record success metrics per ADR-0011: both counter and histogram
                record_gc_heartbeat("success", "comprehensive");
                record_gc_heartbeat_latency("comprehensive", duration);

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
                let duration = start.elapsed();
                // Record error metrics per ADR-0011: both counter and histogram
                record_gc_heartbeat("error", "comprehensive");
                record_gc_heartbeat_latency("comprehensive", duration);

                // Check for NOT_FOUND status - means GC doesn't recognize this MC
                // (e.g., after GC restart or network partition)
                if e.code() == tonic::Code::NotFound {
                    warn!(
                        target: "mc.grpc.gc_client",
                        "GC returned NOT_FOUND - MC not registered"
                    );
                    self.is_registered.store(false, Ordering::SeqCst);
                    return Err(McError::NotRegistered);
                }

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

    /// Attempt re-registration with GC (single attempt, used by heartbeat loop).
    ///
    /// Unlike `register()`, this does not retry internally - the caller handles retry logic.
    /// Used when heartbeat returns NOT_FOUND.
    ///
    /// # Errors
    ///
    /// Returns `McError::Grpc` if RPC fails.
    /// Returns `McError::Config` if GC rejects registration.
    #[instrument(skip_all, fields(mc_id = %self.config.mc_id))]
    pub async fn attempt_reregistration(&self) -> Result<(), McError> {
        info!(target: "mc.grpc.gc_client", "Attempting re-registration with GC");

        let request = RegisterMcRequest {
            id: self.config.mc_id.clone(),
            region: self.config.region.clone(),
            grpc_endpoint: self.config.grpc_advertise_address.clone(),
            webtransport_endpoint: self.config.webtransport_advertise_address.clone(),
            max_meetings: self.config.max_meetings,
            max_participants: self.config.max_participants,
        };

        match self.try_register(&request).await {
            Ok(response) => {
                if response.accepted {
                    info!(
                        target: "mc.grpc.gc_client",
                        message = %response.message,
                        "Successfully re-registered with GC"
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
                    Ok(())
                } else {
                    warn!(
                        target: "mc.grpc.gc_client",
                        message = %response.message,
                        "GC rejected re-registration"
                    );
                    Err(McError::Config(format!(
                        "GC rejected re-registration: {}",
                        response.message
                    )))
                }
            }
            Err(e) => {
                warn!(target: "mc.grpc.gc_client", error = %e, "Re-registration attempt failed");
                Err(e)
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
        // Verify retry configuration provides ~5 minutes of resilience
        // for GC rolling updates or temporary unavailability
        assert_eq!(MAX_REGISTRATION_RETRIES, 20);
        assert_eq!(MAX_REGISTRATION_DURATION, Duration::from_secs(300));
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
            quic_max_idle_timeout_seconds: 10,
            binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
            ac_endpoint: "https://ac.example.com".to_string(),
            client_id: "mc-service".to_string(),
            client_secret: SecretString::from("test-client-secret"),
            ac_jwks_url: "https://ac.example.com/.well-known/jwks.json".to_string(),
            tls_cert_path: "/dev/null".to_string(),
            tls_key_path: "/dev/null".to_string(),
            grpc_advertise_address: "http://localhost:50052".to_string(),
            webtransport_advertise_address: "https://localhost:4433".to_string(),
            otel_enabled: false,
            otel_endpoint: String::new(),
            otel_sample_rate: 1.0,
            environment: "development".to_string(),
            max_receive_slots: 8,
            media_connect_settle_ms: 1500,
            kek_rotation_debounce_seconds: 60,
            max_receive_capability_declarations: 64,
            audio_encoding: crate::media_signaling::AudioEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Opus,
                48_000,
                50,
            )
            .expect("fixture audio encoding is in band"),
        };

        let token_rx = mc_test_utils::test_token_receiver();

        // Empty endpoint should fail with Config error
        let result = GcClient::new(
            String::new(), // Empty string is clearly invalid
            token_rx,
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
            quic_max_idle_timeout_seconds: 10,
            binding_token_secret: SecretString::from("dGVzdC1zZWNyZXQ="),
            ac_endpoint: "https://ac.example.com".to_string(),
            client_id: "mc-service".to_string(),
            client_secret: SecretString::from("test-client-secret"),
            ac_jwks_url: "https://ac.example.com/.well-known/jwks.json".to_string(),
            tls_cert_path: "/dev/null".to_string(),
            tls_key_path: "/dev/null".to_string(),
            grpc_advertise_address: "http://localhost:50052".to_string(),
            webtransport_advertise_address: "https://localhost:4433".to_string(),
            otel_enabled: false,
            otel_endpoint: String::new(),
            otel_sample_rate: 1.0,
            environment: "development".to_string(),
            max_receive_slots: 8,
            media_connect_settle_ms: 1500,
            kek_rotation_debounce_seconds: 60,
            max_receive_capability_declarations: 64,
            audio_encoding: crate::media_signaling::AudioEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Opus,
                48_000,
                50,
            )
            .expect("fixture audio encoding is in band"),
        };

        let token_rx = mc_test_utils::test_token_receiver();

        // Valid endpoint but no server running - should fail with Grpc error
        let result = GcClient::new(
            "http://127.0.0.1:59999".to_string(), // Unlikely to have a server
            token_rx,
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

    #[test]
    fn test_total_retry_duration_sufficient() {
        // Verify that total retry duration is at least 3 minutes
        // This is important for surviving GC rolling updates
        let mut total_delay = Duration::ZERO;
        let mut delay = BACKOFF_BASE;

        for _ in 0..MAX_REGISTRATION_RETRIES {
            total_delay += delay;
            delay = (delay * 2).min(BACKOFF_MAX);
        }

        // Should provide at least 3 minutes of retry time
        assert!(
            total_delay >= Duration::from_secs(180),
            "Total retry duration should be at least 3 minutes, got {:?}",
            total_delay
        );

        // MAX_REGISTRATION_DURATION should be the primary limit
        assert!(
            MAX_REGISTRATION_DURATION >= Duration::from_secs(180),
            "MAX_REGISTRATION_DURATION should be at least 3 minutes"
        );
    }

    #[test]
    fn test_backoff_eventually_caps() {
        // Verify backoff caps at BACKOFF_MAX and stays there
        let mut delay = BACKOFF_BASE;

        for _ in 0..100 {
            delay = (delay * 2).min(BACKOFF_MAX);
        }

        assert_eq!(
            delay, BACKOFF_MAX,
            "Backoff should cap at BACKOFF_MAX after many iterations"
        );
    }
}

/// Queue depth for meeting-ended notifications to GC.
///
/// Sized far above any realistic burst (it would take this many meetings ending
/// within one GC round trip to fill it), so a full queue is a defect signal
/// rather than load: a drop is counted on its own series and logged at ERROR,
/// never silent.
pub const MEETING_ENDED_QUEUE_CAPACITY: usize = 1024;

/// The production `MeetingEndedNotifier`: queue the meeting id for
/// [`drain_meeting_ended`]. `try_send`, because the controller's loop must
/// never block on GC.
#[derive(Debug)]
pub struct MeetingEndedQueue {
    tx: tokio::sync::mpsc::Sender<String>,
}

impl MeetingEndedQueue {
    /// A queue of `capacity`, and the receiving end [`drain_meeting_ended`]
    /// consumes.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> (Self, tokio::sync::mpsc::Receiver<String>) {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

impl crate::actors::controller::MeetingEndedNotifier for MeetingEndedQueue {
    fn meeting_ended(&self, meeting_id: &str) {
        if let Err(e) = self.tx.try_send(meeting_id.to_string()) {
            crate::observability::metrics::record_gc_meeting_ended_notification_dropped();
            error!(
                target: "mc.grpc.gc_client",
                meeting_id = %meeting_id,
                error = %e,
                "Meeting-ended notification DROPPED before it was sent (queue full or closed); \
                 GC keeps reusing this meeting's assignment and joins to it fail until MC restarts"
            );
        }
    }
}

/// Drain meeting-ended notifications to GC, one at a time, until `cancel`.
///
/// Sequential on purpose: while GC is unreachable every notification would
/// fail the same way, and a burst of parallel connect attempts helps nobody.
///
/// # Shutdown: FLUSH what is queued, then count only what does not fit
///
/// `cancel` fires AFTER the shutdown release window, because a meeting whose
/// teardown completes inside that window produces its notification at that
/// moment — so cancelling with the shutdown token would guarantee the loss of
/// exactly the notifications the release window exists to produce. On `cancel`
/// the queue is closed to new entries and what is already in it is SENT, until
/// [`teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET`] expires. Only the remainder is
/// counted as DROPPED, with the same ERROR as a full queue — never lost
/// silently. A notification produced after the close finds the queue closed and
/// is counted by [`MeetingEndedQueue`] the same way.
pub async fn drain_meeting_ended(
    gc_client: std::sync::Arc<GcClient>,
    mut rx: tokio::sync::mpsc::Receiver<String>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            // Cancel first: once the flush window opens, take no new work into
            // the unbounded path — the flush below is the bounded one.
            biased;
            () = cancel.cancelled() => break,
            next = rx.recv() => match next {
                Some(meeting_id) => gc_client.notify_meeting_ended(&meeting_id).await,
                None => return,
            },
        }
    }

    // Closed first, so the set to flush is fixed and cannot grow under us.
    rx.close();
    let flush_deadline =
        tokio::time::Instant::now() + crate::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET;
    while let Ok(meeting_id) = rx.try_recv() {
        // Each send is bounded by the SAME deadline, so one unreachable-GC
        // notification cannot consume the whole flush. A send cut off here is
        // AMBIGUOUS — it may have been committed — so it is not counted as
        // dropped and, per the at-most-once policy, never retried; the ERROR
        // below names the remainder that was provably never sent.
        if tokio::time::timeout_at(flush_deadline, gc_client.notify_meeting_ended(&meeting_id))
            .await
            .is_err()
        {
            error!(
                target: "mc.grpc.gc_client",
                meeting_id = %meeting_id,
                "Meeting-ended notification was still in flight when the shutdown flush window \
                 expired; whether GC committed it is UNKNOWN and it is not retried"
            );
            break;
        }
    }
    while let Ok(meeting_id) = rx.try_recv() {
        crate::observability::metrics::record_gc_meeting_ended_notification_dropped();
        error!(
            target: "mc.grpc.gc_client",
            meeting_id = %meeting_id,
            "Meeting-ended notification DROPPED at shutdown before it was sent; GC keeps reusing \
             this meeting's assignment and joins to it fail until MC restarts"
        );
    }
}
