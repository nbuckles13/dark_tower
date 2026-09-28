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
use crate::routing::RoutingTable;
use common::observability::otel_grpc::client_interceptor;
use common::secret::ExposeSecret;
use common::token_manager::TokenReceiver;
use proto_gen::dark_tower::internal::v1::media_handler_registry_service_client::MediaHandlerRegistryServiceClient;
use proto_gen::dark_tower::internal::v1::{RegisterMhRequest, SendLoadReportRequest};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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

/// The largest `current_streams` GC accepts: `media_handlers.current_streams`
/// is an `INTEGER`, and GC's load-report handler rejects anything above
/// `i32::MAX` with `InvalidArgument` (`crates/gc-service/src/grpc/mh_service.rs`,
/// `send_load_report`) rather than wrapping it negative.
const GC_MAX_REPORTABLE_STREAMS: u32 = i32::MAX.unsigned_abs();

/// Clamp an installed-stream count to what GC's load report accepts.
///
/// Saturates toward FULL, and to GC's column maximum rather than `u32::MAX`.
/// Saturating to `u32::MAX` would NOT make GC "place less": GC refuses the
/// whole load report above `i32::MAX`, so the heartbeat goes stale, the handler
/// drops out through staleness instead, and GC's logs fill with
/// `InvalidArgument`. Any value this high is still >= every possible stream
/// ceiling, so GC excludes the handler from placement — the fail-closed intent
/// — while the report itself is accepted.
///
/// Unreachable today (installed streams are bounded by
/// `MH_MAX_TOTAL_EGRESS_EDGES`, at most 2^20); kept correct regardless.
fn saturate_for_gc(installed_streams: usize) -> u32 {
    u32::try_from(installed_streams)
        .unwrap_or(u32::MAX)
        .min(GC_MAX_REPORTABLE_STREAMS)
}

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
    /// The live routing table, read at every load report for
    /// `current_streams`: the installed egress streams on this handler —
    /// the SAME `total_edges()` the admission check compares against the
    /// ceiling and the session actor publishes as `mh_media_egress_edges`.
    routing: Arc<RoutingTable>,
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
        routing: Arc<RoutingTable>,
    ) -> Result<Self, MhError> {
        let channel = Endpoint::from_shared(gc_endpoint.clone())
            .map_err(|e| {
                error!(
                    target: "mh.grpc.gc_client",
                    error = %e,
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
            routing,
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
                    // Same fault and same reasoning as `mc_client`'s: `Config`
                    // means MH's STARTUP configuration, and a per-call
                    // credential failure behind a name that says
                    // "misconfigured at boot" is a false triage pointer.
                    MhError::OutboundAuthUnavailable(format!(
                        "Authorization header parse failed: {e}"
                    ))
                })?,
        );
        Ok(grpc_request)
    }

    /// The value advertised to GC as `max_streams`: the derived egress STREAM
    /// ceiling (story 2 R-19).
    ///
    /// The SAME field admission enforces and `mh_media_egress_stream_ceiling`
    /// publishes — read, never re-derived. GC's placement filter reads it
    /// unchanged and stays SOFT; MH admission is the hard backstop. Replaces the
    /// retired `MH_MAX_STREAMS`, which was advertised here and enforced nowhere.
    #[must_use]
    pub fn advertised_max_streams(&self) -> u32 {
        self.config.egress_admission.stream_ceiling
    }

    /// The value reported to GC as `current_streams`: installed egress streams
    /// on this handler, in the same unit as [`Self::advertised_max_streams`].
    ///
    /// Before story 2 task 8 this was a hardcoded 0, which made GC's soft filter
    /// `current_streams < max_streams` always true. Now it is real, so a handler
    /// at its ceiling drops out of GC placement.
    ///
    /// Streams are released when a meeting's MC calls `EndMeeting` on meeting
    /// end (story 2 R-20). A meeting whose MC never COMPLETES it (an MC that
    /// crashes or is killed mid-teardown, whose `EndMeeting` exhausts its
    /// retries or reaches an MH predating the RPC, or that is rolled back to a
    /// build predating teardown; `docs/TODO.md`, "A meeting whose MC never
    /// sends `EndMeeting` is never reclaimed") keeps its streams, so this can
    /// sit at the ceiling with no live meeting behind it. When every
    /// handler is there, the first join to a NEW meeting fails at GC (503). The
    /// recovery is under "The ratchet" in `docs/observability/metrics/mh-service.md`
    /// (pointed at, not restated: deployment names are infra topology);
    /// `mh_media_egress_edges` is the same value, as a gauge.
    ///
    /// Saturates toward FULL rather than truncating — see
    /// [`saturate_for_gc`] for why the saturation point is GC's column
    /// maximum and not `u32::MAX`.
    #[must_use]
    pub fn current_streams(&self) -> u32 {
        saturate_for_gc(self.routing.load().total_edges())
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
            webtransport_endpoint: self.config.webtransport_advertise_address.clone(),
            grpc_endpoint: self.config.grpc_advertise_address.clone(),
            max_streams: self.advertised_max_streams(),
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
    ) -> Result<proto_gen::dark_tower::internal::v1::RegisterMhResponse, MhError> {
        let grpc_request = self.add_auth(request.clone())?;

        // R-56: inject the active OTel context as outbound traceparent/tracestate.
        let mut client = MediaHandlerRegistryServiceClient::with_interceptor(
            self.channel.clone(),
            client_interceptor(),
        );

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

        let request = SendLoadReportRequest {
            handler_id: self.config.handler_id.clone(),
            current_streams: self.current_streams(),
            health: 1, // HEALTHY
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            bandwidth_usage_percent: 0.0,
        };

        let start = Instant::now();
        let grpc_request = self.add_auth(request)?;

        // R-56: inject the active OTel context as outbound traceparent/tracestate.
        let mut client = MediaHandlerRegistryServiceClient::with_interceptor(
            self.channel.clone(),
            client_interceptor(),
        );

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
    #[instrument(skip_all, fields(handler_id = %self.config.handler_id, region = %self.config.region))]
    pub async fn attempt_reregistration(&self) -> Result<(), MhError> {
        let request = RegisterMhRequest {
            handler_id: self.config.handler_id.clone(),
            region: self.config.region.clone(),
            webtransport_endpoint: self.config.webtransport_advertise_address.clone(),
            grpc_endpoint: self.config.grpc_advertise_address.clone(),
            max_streams: self.advertised_max_streams(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_stops_at_the_largest_value_gc_accepts() {
        // Within range: reported exactly.
        assert_eq!(saturate_for_gc(40), 40);
        // At and above GC's INTEGER maximum: clamped to it, never beyond. A value
        // above i32::MAX makes GC reject the whole load report.
        let gc_max = usize::try_from(i32::MAX).unwrap_or(usize::MAX);
        assert_eq!(saturate_for_gc(gc_max), GC_MAX_REPORTABLE_STREAMS);
        assert_eq!(saturate_for_gc(gc_max + 1), GC_MAX_REPORTABLE_STREAMS);
        assert_eq!(saturate_for_gc(usize::MAX), GC_MAX_REPORTABLE_STREAMS);
        assert!(i32::try_from(GC_MAX_REPORTABLE_STREAMS).is_ok());
    }
}
