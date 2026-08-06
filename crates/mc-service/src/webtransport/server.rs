//! WebTransport accept loop with TLS 1.3 termination.
//!
//! Binds a QUIC/HTTP3 endpoint using `wtransport`, accepts WebTransport sessions,
//! and spawns per-connection handler tasks.
//!
//! # Graceful Shutdown
//!
//! The accept loop monitors a `CancellationToken`. On cancellation:
//! 1. Stop accepting new connections
//! 2. Child tokens propagate cancellation to active connection handlers

use crate::actors::MeetingControllerActorHandle;
use crate::auth::McJwtValidator;
use crate::grpc::MhRegistrationClient;
use crate::observability::metrics;
use crate::redis::MhAssignmentStore;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Endpoint, Identity, ServerConfig};

use super::connection;

/// Derive the QUIC keep-alive interval from the max-idle-timeout.
///
/// Keep-alive packets keep a healthy-but-quiet connection alive so it is never
/// spuriously idle-timed-out; to be effective the interval MUST be strictly less
/// than the idle timeout (quinn requirement). Computed in MILLISECONDS as
/// `idle / 3` so it never degenerates to `0s` for small idle timeouts (integer
/// `secs / 3` would floor 1s and 2s to 0): e.g. 1s→333ms, 2s→666ms, 10s→3.333s —
/// always `> 0` and strictly `< idle`. Config rejects `idle == 0`, so `idle` here
/// is always `>= 1s`.
#[must_use]
pub fn keep_alive_for(idle: Duration) -> Duration {
    Duration::from_millis(idle.as_millis() as u64 / 3)
}

/// WebTransport server that accepts client connections.
pub struct WebTransportServer {
    /// Bind address for the WebTransport endpoint.
    bind_address: String,
    /// Path to TLS certificate (PEM).
    tls_cert_path: String,
    /// Path to TLS private key (PEM).
    tls_key_path: String,
    /// Handle to the meeting controller actor.
    controller_handle: Arc<MeetingControllerActorHandle>,
    /// JWT validator for meeting tokens.
    jwt_validator: Arc<McJwtValidator>,
    /// Redis client for reading MH assignment data during join.
    redis_client: Arc<dyn MhAssignmentStore>,
    /// MH registration client for async RegisterMeeting RPCs.
    mh_client: Arc<dyn MhRegistrationClient>,
    /// This MC's identifier.
    mc_id: String,
    /// This MC's gRPC advertise address (for MH->MC callbacks).
    mc_grpc_endpoint: String,
    /// Maximum concurrent connections (bounds resource exhaustion).
    max_connections: usize,
    /// QUIC max-idle-timeout for the WebTransport endpoint (config-driven).
    /// Bounds crash/network-loss departure detection; a keep-alive interval is
    /// derived from it so healthy-but-quiet connections are never idle-timed-out.
    quic_max_idle_timeout: Duration,
    /// Active connection count.
    active_connections: Arc<AtomicUsize>,
    /// Cancellation token for graceful shutdown.
    cancel_token: CancellationToken,
}

impl WebTransportServer {
    /// Create a new WebTransport server.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "Constructor wiring; a config struct would be over-engineering"
    )]
    pub fn new(
        bind_address: String,
        tls_cert_path: String,
        tls_key_path: String,
        controller_handle: Arc<MeetingControllerActorHandle>,
        jwt_validator: Arc<McJwtValidator>,
        redis_client: Arc<dyn MhAssignmentStore>,
        mh_client: Arc<dyn MhRegistrationClient>,
        mc_id: String,
        mc_grpc_endpoint: String,
        max_connections: usize,
        quic_max_idle_timeout: Duration,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            bind_address,
            tls_cert_path,
            tls_key_path,
            controller_handle,
            jwt_validator,
            redis_client,
            mh_client,
            mc_id,
            mc_grpc_endpoint,
            max_connections,
            quic_max_idle_timeout,
            active_connections: Arc::new(AtomicUsize::new(0)),
            cancel_token,
        }
    }

    /// Load TLS identity and bind the QUIC/HTTP3 endpoint.
    ///
    /// Call this **before** spawning the accept loop so that TLS or bind
    /// failures are fail-fast (crash the process) rather than silent.
    ///
    /// # Errors
    ///
    /// Returns an error if the TLS certificate cannot be loaded or the
    /// endpoint fails to bind.
    pub async fn bind(&self) -> Result<Endpoint<Server>, Box<dyn std::error::Error + Send + Sync>> {
        let bind_addr: std::net::SocketAddr = self.bind_address.parse().map_err(|e| {
            error!(
                target: "mc.webtransport",
                error = %e,
                addr = %self.bind_address,
                "Invalid WebTransport bind address"
            );
            format!(
                "Invalid WebTransport bind address '{}': {e}",
                self.bind_address
            )
        })?;

        let identity = Identity::load_pemfiles(&self.tls_cert_path, &self.tls_key_path)
            .await
            .map_err(|e| {
                error!(
                    target: "mc.webtransport",
                    error = %e,
                    cert_path = %self.tls_cert_path,
                    key_path = %self.tls_key_path,
                    "Failed to load TLS certificate"
                );
                format!("Failed to load TLS certificate: {e}")
            })?;

        // Config-driven QUIC idle timeout + derived keep-alive. The idle timeout
        // is the ONLY detector for a crash/network-loss departure (a clean tab
        // close is caught immediately by `Connection::closed()`), and it bounds
        // the documented worst-case crash roster-remove latency. Keep-alive
        // (< idle) prevents a healthy-but-quiet connection from being spuriously
        // idle-timed-out. FAIL-LOUD: an invalid idle timeout (rejected by
        // wtransport) crashes bind rather than silently reverting to the quinn
        // library default, which would re-introduce unbounded detection.
        let keep_alive = keep_alive_for(self.quic_max_idle_timeout);
        let config = ServerConfig::builder()
            .with_bind_address(bind_addr)
            .with_identity(identity)
            .max_idle_timeout(Some(self.quic_max_idle_timeout))
            .map_err(|e| {
                error!(
                    target: "mc.webtransport",
                    error = %e,
                    idle_timeout_secs = self.quic_max_idle_timeout.as_secs(),
                    "Invalid QUIC max_idle_timeout (MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS)"
                );
                format!(
                    "Invalid QUIC max_idle_timeout {}s (MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS): {e}",
                    self.quic_max_idle_timeout.as_secs()
                )
            })?
            .keep_alive_interval(Some(keep_alive))
            .build();

        info!(
            target: "mc.webtransport",
            idle_timeout_secs = self.quic_max_idle_timeout.as_secs(),
            keep_alive_ms = keep_alive.as_millis() as u64,
            "QUIC idle timeout + keep-alive configured"
        );

        let endpoint = Endpoint::server(config).map_err(|e| {
            error!(
                target: "mc.webtransport",
                error = %e,
                "Failed to create WebTransport endpoint"
            );
            format!("Failed to create WebTransport endpoint: {e}")
        })?;

        info!(
            target: "mc.webtransport",
            bind_address = %self.bind_address,
            "WebTransport endpoint bound successfully"
        );

        Ok(endpoint)
    }

    /// Run the accept loop until the cancellation token is triggered.
    ///
    /// Individual connection errors do not stop the loop.
    /// Call [`Self::bind()`] first to obtain the endpoint.
    pub async fn accept_loop(&self, endpoint: Endpoint<Server>) {
        loop {
            tokio::select! {
                () = self.cancel_token.cancelled() => {
                    info!(
                        target: "mc.webtransport",
                        "WebTransport accept loop shutting down"
                    );
                    break;
                }

                incoming = endpoint.accept() => {
                    let incoming_session = incoming;

                    // Capacity check: reject before allocating handler resources
                    let current = self.active_connections.load(Ordering::Relaxed);
                    if current >= self.max_connections {
                        warn!(
                            target: "mc.webtransport",
                            active = current,
                            max = self.max_connections,
                            "Connection rejected: at capacity"
                        );
                        // Drop incoming_session without accepting — client sees connection refused
                        metrics::record_webtransport_connection("rejected");
                        continue;
                    }

                    self.active_connections.fetch_add(1, Ordering::Relaxed);
                    metrics::record_webtransport_connection("accepted");
                    let active_connections = Arc::clone(&self.active_connections);
                    let controller_handle = Arc::clone(&self.controller_handle);
                    let jwt_validator = Arc::clone(&self.jwt_validator);
                    let redis_client = Arc::clone(&self.redis_client);
                    let mh_client = Arc::clone(&self.mh_client);
                    let mc_id = self.mc_id.clone();
                    let mc_grpc_endpoint = self.mc_grpc_endpoint.clone();
                    let connection_token = self.cancel_token.child_token();

                    tokio::spawn(async move {
                        let result = connection::handle_connection(
                            incoming_session,
                            controller_handle,
                            jwt_validator,
                            redis_client,
                            mh_client,
                            mc_id,
                            mc_grpc_endpoint,
                            connection_token,
                        )
                        .await;

                        active_connections.fetch_sub(1, Ordering::Relaxed);

                        // INVARIANT (task #64): `status="error"` counts connection
                        // ESTABLISHMENT failures only. `handle_connection` returns
                        // `Err` ONLY before/at join (session/stream accept, decode,
                        // JWT/join); a MID-SESSION departure returns `Ok(())` and is
                        // observed via `mc_participant_disconnects_total{cause}` /
                        // `mc_participant_leaves_total{reason}` instead. Do NOT route
                        // a post-join disconnect back to an `Err` here — that would
                        // silently miscount peers-going-away as server errors and
                        // revive the reclassified-away bug.
                        if let Err(e) = result {
                            metrics::record_webtransport_connection("error");
                            warn!(
                                target: "mc.webtransport",
                                error = %e,
                                "Connection handler completed with error"
                            );
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::keep_alive_for;
    use std::time::Duration;

    #[test]
    fn keep_alive_is_positive_and_below_idle_for_small_timeouts() {
        // The integer-secs/3 degeneration bug (1s→0, 2s→0) is avoided by the
        // millis derivation. Config rejects idle==0, so 1s is the minimum.
        for idle_secs in [1u64, 2, 3, 10, 30] {
            let idle = Duration::from_secs(idle_secs);
            let ka = keep_alive_for(idle);
            assert!(
                ka > Duration::ZERO,
                "keep-alive must be > 0 for {idle_secs}s"
            );
            assert!(
                ka < idle,
                "keep-alive {ka:?} must be strictly < idle {idle:?} to be effective"
            );
        }
    }

    #[test]
    fn keep_alive_exact_thirds() {
        assert_eq!(
            keep_alive_for(Duration::from_secs(1)),
            Duration::from_millis(333)
        );
        assert_eq!(
            keep_alive_for(Duration::from_secs(2)),
            Duration::from_millis(666)
        );
        assert_eq!(
            keep_alive_for(Duration::from_secs(10)),
            Duration::from_millis(3333)
        );
    }
}
