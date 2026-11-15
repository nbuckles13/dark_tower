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
use crate::media_routing::{
    AssignmentRejected, HandlerEndpoint, HandlerId, ReleaseCauseExt, RemovalTrigger,
};
use crate::observability::{record_gc_heartbeat, record_gc_heartbeat_latency};
use common::meeting_lifecycle::AssignmentId;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::global_controller_service_client::GlobalControllerServiceClient;
use proto_gen::dark_tower::internal::v1::{
    AcquireMediaHandlerRequest, AcquireMediaHandlerResponse, ComprehensiveHeartbeatRequest,
    ControllerCapacity, FastHeartbeatRequest, HealthStatus, NotifyMeetingEndedOutcome,
    NotifyMeetingEndedRequest, RegisterMcRequest, RegisterMcResponse,
};
use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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

/// Bound on ONE `NotifyMeetingEnded` attempt. A GC that never answers must not
/// hold the drain on one notification: an attempt past this is a failed
/// attempt like any other (retried — safe, because the notify is fenced by
/// `assignment_id`).
pub const NOTIFY_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the drain republishes the pending gauges between events, so
/// `mc_gc_notify_meeting_ended_oldest_pending_age_seconds` tracks a stuck
/// entry's true age while it waits out its backoff (the `MCNotifyMeetingEndedStuck`
/// input).
pub const NOTIFY_PENDING_PUBLISH_INTERVAL: Duration = Duration::from_secs(5);

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
    /// GC's cap on `AcquireMediaHandlerRequest.exclude_handler_ids`
    /// (`RegisterMCResponse.max_exclude_handler_ids`, ADR-0039 §3). 0 until the
    /// first accepted registration; refreshed by every re-registration (a GC
    /// config change reaches MC only then). Never 0 after an accepted
    /// registration: [`Self::accept_registration`] refuses 0.
    max_exclude_handler_ids: AtomicU32,
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
            max_exclude_handler_ids: AtomicU32::new(0),
        }
    }

    /// Apply an ACCEPTED registration response — the ONE path shared by
    /// [`Self::register`] and [`Self::attempt_reregistration`], so a
    /// re-registration refreshes exactly what the first registration set.
    ///
    /// # Errors
    ///
    /// [`McError::Config`] if GC sent `max_exclude_handler_ids = 0` (absent):
    /// MC derives its per-meeting replacement attempt bound from it (ADR-0039
    /// §3), so it fails registration rather than run with no bound. Not
    /// retried by `register` (the same arm as a GC rejection): a GC that
    /// predates the field sends 0 on every attempt.
    fn accept_registration(&self, response: &RegisterMcResponse) -> Result<(), McError> {
        let Some(cap) = NonZeroU32::new(response.max_exclude_handler_ids) else {
            error!(
                target: "mc.grpc.gc_client",
                field = "max_exclude_handler_ids",
                "GC sent max_exclude_handler_ids=0 (absent): GC predates story 4 GC-3. Roll GC \
                 forward (or this MC back) - see docs/runbooks/mc-deployment.md \
                 \"Pre-Deployment Coordination\" and the GC-3 rollback floor in \
                 docs/runbooks/gc-deployment.md"
            );
            return Err(McError::Config(
                "GC sent max_exclude_handler_ids=0 (absent); GC predates story 4 GC-3 - roll GC \
                 forward or MC back"
                    .to_string(),
            ));
        };
        // Store intervals from GC (0 keeps the default, as before).
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
        self.max_exclude_handler_ids
            .store(cap.get(), Ordering::SeqCst);
        self.is_registered.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// GC's exclude-list cap, once an accepted registration delivered it.
    #[must_use]
    pub fn max_exclude_handler_ids(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(self.max_exclude_handler_ids.load(Ordering::SeqCst))
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

    /// ONE attempt to tell GC that the incarnation `ended` names has ended, so
    /// GC stops routing joins for its meeting id here (ADR-0010 §3).
    ///
    /// # Fenced, so retried
    ///
    /// The request names MC's `controller_id` and the `assignment_id` GC sent
    /// with the assignment, and GC ends ONLY the active row matching both
    /// (`internal.proto`, `NotifyMeetingEndedRequest`). A duplicate can
    /// therefore end nothing but its own incarnation, so the caller
    /// ([`drain_meeting_ended`]) retries every [`NotifyAttempt::Retry`] until
    /// GC answers. `INVALID_ARGUMENT` and an `UNSPECIFIED` outcome are defects
    /// and final ([`NotifyAttempt::Rejected`]).
    ///
    /// Records the attempt's `mc_gc_notify_meeting_ended_total{outcome}` and
    /// logs it; never logs the request (it carries the auth metadata).
    pub async fn notify_meeting_ended(&self, ended: &EndedMeeting) -> NotifyAttempt {
        use crate::observability::metrics::{
            record_gc_notify_meeting_ended, NotifyMeetingEndedOutcome as Recorded,
        };
        let request = match self.add_auth(NotifyMeetingEndedRequest {
            meeting_id: ended.meeting_id.clone(),
            region: self.config.region.clone(),
            controller_id: self.config.mc_id.clone(),
            assignment_id: ended.assignment_id.to_string(),
        }) {
            Ok(request) => request,
            Err(e) => {
                // A token that cannot form a header now may next time (the
                // token manager refreshes it): retry.
                record_gc_notify_meeting_ended(Recorded::Retried);
                warn!(
                    target: "mc.grpc.gc_client",
                    meeting_id = %ended.meeting_id,
                    assignment_id = %ended.assignment_id,
                    error = %e,
                    "Could not build NotifyMeetingEnded; will retry"
                );
                return NotifyAttempt::Retry;
            }
        };
        let mut client = GlobalControllerServiceClient::with_interceptor(
            self.channel.clone(),
            common::observability::otel_grpc::client_interceptor(),
        );
        let result =
            tokio::time::timeout(NOTIFY_ATTEMPT_TIMEOUT, client.notify_meeting_ended(request))
                .await;
        match result {
            Ok(Ok(response)) => match response.into_inner().outcome() {
                outcome @ (NotifyMeetingEndedOutcome::Ended
                | NotifyMeetingEndedOutcome::NotCurrent) => {
                    record_gc_notify_meeting_ended(Recorded::Acked);
                    info!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %ended.meeting_id,
                        assignment_id = %ended.assignment_id,
                        outcome = notify_outcome_label(outcome),
                        "GC notified that the meeting ended"
                    );
                    NotifyAttempt::Acked
                }
                NotifyMeetingEndedOutcome::Unspecified => {
                    record_gc_notify_meeting_ended(Recorded::Rejected);
                    error!(
                        target: "mc.grpc.gc_client",
                        meeting_id = %ended.meeting_id,
                        assignment_id = %ended.assignment_id,
                        "GC answered NotifyMeetingEnded with no outcome (a GC defect or version \
                         skew); not retried — GC may keep routing this meeting id here until \
                         this MC stops heartbeating"
                    );
                    NotifyAttempt::Rejected
                }
            },
            Ok(Err(status)) if status.code() == tonic::Code::InvalidArgument => {
                record_gc_notify_meeting_ended(Recorded::Rejected);
                error!(
                    target: "mc.grpc.gc_client",
                    meeting_id = %ended.meeting_id,
                    assignment_id = %ended.assignment_id,
                    code = ?status.code(),
                    error = %status.message(),
                    "GC refused NotifyMeetingEnded as invalid (an MC defect); not retried — GC \
                     may keep routing this meeting id here until this MC stops heartbeating"
                );
                NotifyAttempt::Rejected
            }
            Ok(Err(status)) => {
                record_gc_notify_meeting_ended(Recorded::Retried);
                warn!(
                    target: "mc.grpc.gc_client",
                    meeting_id = %ended.meeting_id,
                    assignment_id = %ended.assignment_id,
                    code = ?status.code(),
                    error = %status.message(),
                    "NotifyMeetingEnded failed; will retry (fenced: a duplicate cannot end another \
                     incarnation)"
                );
                NotifyAttempt::Retry
            }
            Err(_) => {
                record_gc_notify_meeting_ended(Recorded::Retried);
                warn!(
                    target: "mc.grpc.gc_client",
                    meeting_id = %ended.meeting_id,
                    assignment_id = %ended.assignment_id,
                    timeout_ms = u64::try_from(NOTIFY_ATTEMPT_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
                    "NotifyMeetingEnded got no answer in time; will retry"
                );
                NotifyAttempt::Retry
            }
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
                        self.accept_registration(&response)?;
                        info!(
                            target: "mc.grpc.gc_client",
                            message = %response.message,
                            fast_heartbeat_ms = response.fast_heartbeat_interval_ms,
                            comprehensive_heartbeat_ms = response.comprehensive_heartbeat_interval_ms,
                            max_exclude_handler_ids = response.max_exclude_handler_ids,
                            retries = retry_count,
                            "Successfully registered with GC"
                        );
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
    ) -> Result<RegisterMcResponse, McError> {
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
                    self.accept_registration(&response)?;
                    info!(
                        target: "mc.grpc.gc_client",
                        message = %response.message,
                        max_exclude_handler_ids = response.max_exclude_handler_ids,
                        "Successfully re-registered with GC"
                    );
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

// =============================================================================
// ADR-0039 §3: AcquireMediaHandler (MC -> GC replacement grant)
// =============================================================================

/// One replacement request, as the meeting actor builds it. Ids and the
/// trigger only: no key material, no token, no request metadata (the bearer
/// is added by [`GcClient`] at send time and never stored here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireCall {
    /// The meeting needing a handler.
    pub meeting_id: String,
    /// `exclude_handler_ids`: current ∪ removed (refused-at-admission grants
    /// are removed, so they are in it). A set, so the wire list is deduplicated
    /// by construction (GC counts duplicates toward its cap).
    pub exclude: BTreeSet<HandlerId>,
    /// Why the replaced handler left (projected through
    /// [`ReleaseCauseExt::release_cause`], the ONE projection).
    pub trigger: RemovalTrigger,
    /// The handler being replaced, when there is exactly one: label/log
    /// context for GC only (`gc_view`), never a swap target. `None` omits the
    /// field.
    pub replaced: Option<HandlerId>,
}

/// Why GC's answer is refused outright (never retried, ERROR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireRejection {
    /// `FAILED_PRECONDITION`: GC holds no active ownership row for (meeting,
    /// this MC). Anti-misroute only.
    NotOwner,
    /// `INVALID_ARGUMENT`: an MC bug (over-cap list, malformed id, bad
    /// trigger).
    InvalidRequest,
    /// An `OK` that is not a grant (fail closed): which field was missing.
    Malformed(MalformedGrant),
}

impl AcquireRejection {
    /// Bounded log value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotOwner => "not_owner",
            Self::InvalidRequest => "invalid_request",
            Self::Malformed(m) => m.label(),
        }
    }
}

/// The missing piece of an `OK` that is not a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedGrant {
    /// `assignment` absent.
    NoAssignment,
    /// The assignment failed THE intake predicate shared with the hand-off.
    Assignment(AssignmentRejected),
}

impl MalformedGrant {
    /// Bounded log value.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NoAssignment => "no_assignment",
            Self::Assignment(r) => r.label(),
        }
    }
}

/// GC's answer to one [`AcquireCall`], classified by status CODE (never the
/// message). The code and GC's message are KEPT on every failure arm, so the
/// caller's log and any error built from it carry them (the message goes to
/// logs only, never to a client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireResult {
    /// `OK` with a well-formed assignment.
    Granted(HandlerEndpoint),
    /// `RESOURCE_EXHAUSTED`: nothing eligible after exclusion. Retried.
    NoCapacity {
        /// GC's status message (logs only).
        message: String,
    },
    /// GC could not answer: transport errors, `UNAVAILABLE`,
    /// `DEADLINE_EXCEEDED`, `INTERNAL`, `UNIMPLEMENTED` (rollout window),
    /// `UNAUTHENTICATED` / `PERMISSION_DENIED` (token expiry; TokenManager
    /// refreshes) and every other code. Retried with backoff.
    Unavailable {
        /// The status code.
        code: tonic::Code,
        /// GC's status message (logs only).
        message: String,
    },
    /// Refused outright: never retried, logged at ERROR.
    Rejected {
        /// Why.
        reason: AcquireRejection,
        /// The status code (`Ok` for a malformed grant).
        code: tonic::Code,
        /// GC's status message (logs only).
        message: String,
    },
}

/// Classify one `AcquireMediaHandler` reply. One explicit arm per code GC is
/// documented to return (`internal.proto` `AcquireMediaHandlerRequest`
/// STATUSES), so no code reaches a bucket by accident.
#[must_use]
pub fn classify_acquire(
    reply: Result<AcquireMediaHandlerResponse, tonic::Status>,
) -> AcquireResult {
    let status = match reply {
        Ok(response) => {
            let Some(assignment) = response.assignment else {
                return AcquireResult::Rejected {
                    reason: AcquireRejection::Malformed(MalformedGrant::NoAssignment),
                    code: tonic::Code::Ok,
                    message: String::new(),
                };
            };
            return match HandlerEndpoint::try_from(&assignment) {
                Ok(endpoint) => AcquireResult::Granted(endpoint),
                Err(rejected) => AcquireResult::Rejected {
                    reason: AcquireRejection::Malformed(MalformedGrant::Assignment(rejected)),
                    code: tonic::Code::Ok,
                    message: String::new(),
                },
            };
        }
        Err(status) => status,
    };
    let code = status.code();
    let message = status.message().to_string();
    match code {
        tonic::Code::ResourceExhausted => AcquireResult::NoCapacity { message },
        tonic::Code::FailedPrecondition => AcquireResult::Rejected {
            reason: AcquireRejection::NotOwner,
            code,
            message,
        },
        tonic::Code::InvalidArgument => AcquireResult::Rejected {
            reason: AcquireRejection::InvalidRequest,
            code,
            message,
        },
        // Transient by contract: GC really emits INTERNAL on a DB error.
        tonic::Code::Unavailable
        | tonic::Code::DeadlineExceeded
        | tonic::Code::Internal
        // Only from a GC predating the rpc (rollout window).
        | tonic::Code::Unimplemented
        // GC's auth layer (token expiry): TokenManager refreshes; the caller
        // WARNs on these two every attempt so a lasting credential fault is
        // never silent.
        | tonic::Code::Unauthenticated
        | tonic::Code::PermissionDenied
        // Every other code: retried with backoff (fail toward survivors).
        | tonic::Code::Ok
        | tonic::Code::Cancelled
        | tonic::Code::Unknown
        | tonic::Code::NotFound
        | tonic::Code::AlreadyExists
        | tonic::Code::Aborted
        | tonic::Code::OutOfRange
        | tonic::Code::DataLoss => AcquireResult::Unavailable { code, message },
    }
}

/// MC→GC replacement grants (ADR-0039 §3). Abstraction over the gRPC call so
/// the meeting actor's migration path is testable with a scripted GC.
/// Production: [`GcClient`].
pub trait HandlerAcquirer: Send + Sync {
    /// Ask GC for ONE replacement handler. Never panics; every failure is an
    /// [`AcquireResult`] arm.
    fn acquire<'a>(
        &'a self,
        call: &'a AcquireCall,
    ) -> Pin<Box<dyn std::future::Future<Output = AcquireResult> + Send + 'a>>;

    /// GC's exclude-list cap from the latest accepted registration (`None`
    /// before one).
    fn max_exclude_handler_ids(&self) -> Option<NonZeroU32>;
}

impl HandlerAcquirer for GcClient {
    fn acquire<'a>(
        &'a self,
        call: &'a AcquireCall,
    ) -> Pin<Box<dyn std::future::Future<Output = AcquireResult> + Send + 'a>> {
        Box::pin(async move {
            let request = AcquireMediaHandlerRequest {
                meeting_id: call.meeting_id.clone(),
                controller_id: self.config.mc_id.clone(),
                exclude_handler_ids: call
                    .exclude
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                trigger: call.trigger.release_cause().into(),
                replaced_handler_id: call.replaced.as_ref().map(|id| id.as_str().to_string()),
            };
            let grpc_request = match self.add_auth(request) {
                Ok(r) => r,
                Err(e) => {
                    return AcquireResult::Unavailable {
                        code: tonic::Code::Unauthenticated,
                        message: format!("could not build the request: {e}"),
                    }
                }
            };
            // The SAME channel as registration: `.timeout(GC_RPC_TIMEOUT)` and
            // `connect_timeout(GC_CONNECT_TIMEOUT)` bound every call, so a hung
            // GC resolves as DEADLINE_EXCEEDED and never wedges the meeting's
            // one in-flight acquisition.
            let mut client = GlobalControllerServiceClient::with_interceptor(
                self.channel.clone(),
                common::observability::otel_grpc::client_interceptor(),
            );
            classify_acquire(
                client
                    .acquire_media_handler(grpc_request)
                    .await
                    .map(tonic::Response::into_inner),
            )
        })
    }

    fn max_exclude_handler_ids(&self) -> Option<NonZeroU32> {
        GcClient::max_exclude_handler_ids(self)
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
            clock_skew_seconds: 5,
            disconnect_grace_period_seconds: 30,
            meeting_join_window_seconds: crate::actors::meeting_timers::MAX_JOIN_WINDOW.as_secs(),
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
            receive_slot_caps: [8, 4],
            media_connect_settle_ms: 1500,
            mh_heartbeat_timeout_ms: 6000,
            kek_rotation_debounce_seconds: 60,
            media_dial_token_ttl_seconds: 300,
            mh_replacement_min_interval_ms: 1000,
            mh_replacement_backoff_max_ms: 30000,
            planned_transfer_margin_ms: 5000,
            client_loss_disagreement: crate::media_routing::DisagreementThreshold::new(
                crate::media_routing::DisagreementFraction::new(0.5).unwrap(),
                std::num::NonZeroU32::new(3).unwrap(),
            ),
            max_receive_capability_declarations: 64,
            audio_encoding: crate::media_signaling::AudioEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Opus,
                48_000,
                50,
            )
            .expect("fixture audio encoding is in band"),
            video_encoding: crate::media_signaling::VideoEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Vp8,
                640,
                360,
                30,
                800_000,
                500,
            )
            .expect("fixture video encoding is in band"),
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
            clock_skew_seconds: 5,
            disconnect_grace_period_seconds: 30,
            meeting_join_window_seconds: crate::actors::meeting_timers::MAX_JOIN_WINDOW.as_secs(),
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
            receive_slot_caps: [8, 4],
            media_connect_settle_ms: 1500,
            mh_heartbeat_timeout_ms: 6000,
            kek_rotation_debounce_seconds: 60,
            media_dial_token_ttl_seconds: 300,
            mh_replacement_min_interval_ms: 1000,
            mh_replacement_backoff_max_ms: 30000,
            planned_transfer_margin_ms: 5000,
            client_loss_disagreement: crate::media_routing::DisagreementThreshold::new(
                crate::media_routing::DisagreementFraction::new(0.5).unwrap(),
                std::num::NonZeroU32::new(3).unwrap(),
            ),
            max_receive_capability_declarations: 64,
            audio_encoding: crate::media_signaling::AudioEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Opus,
                48_000,
                50,
            )
            .expect("fixture audio encoding is in band"),
            video_encoding: crate::media_signaling::VideoEncoding::new(
                proto_gen::dark_tower::signaling::v1::Codec::Vp8,
                640,
                360,
                30,
                800_000,
                500,
            )
            .expect("fixture video encoding is in band"),
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

/// Bound on meeting-ended notifications MC holds unanswered: the drain holds
/// at most this many PENDING retry and stops receiving at that point, and the
/// channel buffers at most this many more QUEUED behind it (so up to 2 x this
/// in total before the producer's `try_send` drops).
///
/// Sized far above any realistic burst. While MC->GC fails, MC's heartbeats
/// fail too, so GC stops assigning new meetings here within its staleness
/// threshold; the pending set is therefore bounded in practice by meetings
/// that ended around the outage. A notification beyond this is DROPPED on its
/// own counter and logged at ERROR, never silently.
pub const MEETING_ENDED_QUEUE_CAPACITY: usize = 1024;

/// One ended incarnation GC must be told about.
#[derive(Debug, Clone)]
pub struct EndedMeeting {
    /// The meeting that ended.
    pub meeting_id: String,
    /// The incarnation GC assigned (what GC's end is fenced on).
    pub assignment_id: AssignmentId,
    /// When the controller queued it (the pending-age gauge's origin).
    pub enqueued_at: tokio::time::Instant,
}

/// What one notify attempt concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyAttempt {
    /// GC answered ENDED or NOT_CURRENT: done.
    Acked,
    /// Failed; retry after backoff.
    Retry,
    /// A defect (`INVALID_ARGUMENT` or an `UNSPECIFIED` outcome): final.
    Rejected,
}

fn notify_outcome_label(outcome: NotifyMeetingEndedOutcome) -> &'static str {
    match outcome {
        NotifyMeetingEndedOutcome::Ended => "ended",
        NotifyMeetingEndedOutcome::NotCurrent => "not_current",
        NotifyMeetingEndedOutcome::Unspecified => "unspecified",
    }
}

/// The production `MeetingEndedNotifier`: queue the ended incarnation for
/// [`drain_meeting_ended`]. `try_send`, because the controller's loop must
/// never block on GC.
#[derive(Debug)]
pub struct MeetingEndedQueue {
    tx: tokio::sync::mpsc::Sender<EndedMeeting>,
}

impl MeetingEndedQueue {
    /// A queue of `capacity`, and the receiving end [`drain_meeting_ended`]
    /// consumes.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> (Self, tokio::sync::mpsc::Receiver<EndedMeeting>) {
        let (tx, rx) = tokio::sync::mpsc::channel(capacity);
        (Self { tx }, rx)
    }
}

impl crate::actors::controller::MeetingEndedNotifier for MeetingEndedQueue {
    fn meeting_ended(&self, meeting_id: &str, assignment_id: AssignmentId) {
        let ended = EndedMeeting {
            meeting_id: meeting_id.to_string(),
            assignment_id,
            enqueued_at: tokio::time::Instant::now(),
        };
        if let Err(e) = self.tx.try_send(ended) {
            crate::observability::metrics::record_gc_meeting_ended_notification_dropped();
            error!(
                target: "mc.grpc.gc_client",
                meeting_id = %meeting_id,
                assignment_id = %assignment_id,
                error = %e,
                "Meeting-ended notification DROPPED before it was sent (queue full or closed); \
                 GC keeps routing joins for this meeting id here until this MC stops heartbeating"
            );
        }
    }
}

/// A notification awaiting its next attempt.
struct PendingNotify {
    ended: EndedMeeting,
    next_attempt: tokio::time::Instant,
    backoff: Duration,
}

/// The unanswered notifications, each on its OWN retry schedule: one entry GC
/// keeps failing never delays another beyond the one in-flight attempt (bounded
/// by [`NOTIFY_ATTEMPT_TIMEOUT`]); it waits out its own backoff while the others
/// are sent.
struct PendingNotifies {
    entries: Vec<PendingNotify>,
}

impl PendingNotifies {
    fn push(&mut self, ended: EndedMeeting) {
        self.entries.push(PendingNotify {
            ended,
            next_attempt: tokio::time::Instant::now(),
            backoff: BACKOFF_BASE,
        });
    }

    /// Index of the entry due soonest (ties: oldest first, the push order).
    fn next_due(&self) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .min_by_key(|(i, p)| (p.next_attempt, *i))
            .map(|(i, _)| i)
    }

    /// Publish the pending count and the oldest entry's age.
    fn publish(&self) {
        let now = tokio::time::Instant::now();
        let oldest = self
            .entries
            .iter()
            .map(|p| now.saturating_duration_since(p.ended.enqueued_at))
            .max()
            .unwrap_or(Duration::ZERO);
        crate::observability::metrics::set_gc_notify_meeting_ended_pending(
            self.entries.len(),
            oldest,
        );
    }

    /// Run the attempt for entry `i` and apply its verdict.
    async fn attempt(&mut self, gc_client: &GcClient, i: usize) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let verdict = gc_client.notify_meeting_ended(&entry.ended).await;
        match verdict {
            NotifyAttempt::Acked | NotifyAttempt::Rejected => {
                self.entries.remove(i);
            }
            NotifyAttempt::Retry => {
                if let Some(entry) = self.entries.get_mut(i) {
                    entry.next_attempt = tokio::time::Instant::now() + entry.backoff;
                    entry.backoff = (entry.backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
        self.publish();
    }

    /// The soonest-due entry: its index and when it is due.
    fn due(&self) -> Option<(usize, tokio::time::Instant)> {
        self.next_due()
            .and_then(|i| self.entries.get(i).map(|p| (i, p.next_attempt)))
    }

    /// Count and log everything still pending as DROPPED (shutdown).
    fn drop_all(&mut self) {
        for p in self.entries.drain(..) {
            crate::observability::metrics::record_gc_meeting_ended_notification_dropped();
            error!(
                target: "mc.grpc.gc_client",
                meeting_id = %p.ended.meeting_id,
                assignment_id = %p.ended.assignment_id,
                "Meeting-ended notification DROPPED at shutdown before GC answered it; GC keeps \
                 routing joins for this meeting id here until this MC stops heartbeating"
            );
        }
        self.publish();
    }
}

/// Deliver meeting-ended notifications to GC until `cancel`, then flush.
///
/// Each notification is retried on its OWN capped-backoff schedule until GC
/// answers (see [`GcClient::notify_meeting_ended`]). Attempts are sequential,
/// so a notification GC keeps failing delays the others by at most its one
/// in-flight attempt ([`NOTIFY_ATTEMPT_TIMEOUT`]), never by its backoff. At
/// most `capacity` are held pending (plus up to `capacity` queued in the
/// channel; beyond that the producer's `try_send` counts the excess as
/// dropped). The pending gauges are also republished every
/// [`NOTIFY_PENDING_PUBLISH_INTERVAL`], so the oldest-pending age stays fresh
/// while an entry waits out its backoff. An in-flight attempt yields to
/// `cancel` (its entry stays pending for the flush — safe, the notify is fenced).
///
/// # Shutdown: FLUSH what is held, then count only what does not get through
///
/// `cancel` fires AFTER the shutdown release window, because a meeting whose
/// teardown completes inside that window produces its notification at that
/// moment. On `cancel` the queue is closed and everything held is sent — still
/// on each entry's schedule — until
/// [`teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET`](crate::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET)
/// expires; the remainder is counted as DROPPED with an ERROR each.
pub async fn drain_meeting_ended(
    gc_client: std::sync::Arc<GcClient>,
    mut rx: tokio::sync::mpsc::Receiver<EndedMeeting>,
    cancel: tokio_util::sync::CancellationToken,
) {
    let capacity = rx.max_capacity();
    let mut pending = PendingNotifies {
        entries: Vec::new(),
    };
    pending.publish();
    let mut closed = false;
    let mut publish_tick = tokio::time::interval(NOTIFY_PENDING_PUBLISH_INTERVAL);
    publish_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let due = pending.due();
        let room = pending.entries.len() < capacity;
        tokio::select! {
            // Cancel first: once the flush window opens, take no new work into
            // the unbounded path — the flush below is the bounded one.
            biased;
            () = cancel.cancelled() => break,
            _ = publish_tick.tick() => pending.publish(),
            next = rx.recv(), if room && !closed => match next {
                Some(ended) => {
                    pending.push(ended);
                    pending.publish();
                }
                None => closed = true,
            },
            () = sleep_until_due(due.map(|(_, at)| at)) => {
                if let Some((i, _)) = due {
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => break,
                        () = pending.attempt(&gc_client, i) => {}
                    }
                }
            }
        }
        if closed && pending.entries.is_empty() {
            return;
        }
    }

    // Closed first, so the set to flush is fixed and cannot grow under us.
    rx.close();
    while let Ok(ended) = rx.try_recv() {
        pending.push(ended);
    }
    pending.publish();
    let flush_deadline =
        tokio::time::Instant::now() + crate::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET;
    while let Some((i, at)) = pending.due() {
        if at >= flush_deadline {
            break;
        }
        tokio::time::sleep_until(at).await;
        // Each attempt is bounded by the SAME deadline, so one unanswered
        // attempt cannot consume the whole flush; a cut-off attempt leaves its
        // entry pending (and so counted as dropped below).
        if tokio::time::timeout_at(flush_deadline, pending.attempt(&gc_client, i))
            .await
            .is_err()
        {
            break;
        }
    }
    pending.drop_all();
}

/// Sleep until `at`, or forever when nothing is due.
async fn sleep_until_due(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
