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

use crate::config::{
    QuicTransportParams, CONNECTION_RECEIVE_WINDOW_BYTES, CONNECTION_SEND_WINDOW_BYTES,
    DATAGRAM_RECEIVE_BUFFER_BYTES, STREAM_RECEIVE_WINDOW_BYTES,
};
use crate::grpc::McClient;
use crate::media::MediaSetup;
use crate::observability::metrics;
use crate::session::SessionManagerHandle;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Endpoint, Identity, ServerConfig};

use super::connection::{self, ClientAdmission};

/// WebTransport server that accepts client connections.
pub struct WebTransportServer {
    /// Bind address for the WebTransport endpoint.
    bind_address: String,
    /// Path to TLS certificate (PEM).
    tls_cert_path: String,
    /// Path to TLS private key (PEM).
    tls_key_path: String,
    /// Client `media_dial` admission settings (story 4 MH-1).
    admission: ClientAdmission,
    /// Session manager handle for meeting registration and connection tracking.
    session_manager: SessionManagerHandle,
    /// MC notification client for MH→MC participant notifications.
    mc_client: Arc<McClient>,
    /// This MH incarnation's handler ID — the `media_dial` `aud` it admits.
    handler_id: String,
    /// Accept-time resource-exhaustion guard, never a capacity figure
    /// (`MH_MAX_CONNECTIONS`). Enforcement below is unchanged by ADR-0036 §1.
    max_connections: usize,
    /// Explicit QUIC transport parameters (ADR-0036 §1).
    quic_transport: QuicTransportParams,
    /// Media forward-path setup: handles resolved once at process start, and
    /// the latency sample ratio. Cloned per connection, never re-resolved.
    media: MediaSetup,
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
        reason = "WebTransport server requires all config fields at construction"
    )]
    pub fn new(
        bind_address: String,
        tls_cert_path: String,
        tls_key_path: String,
        admission: ClientAdmission,
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
        max_connections: usize,
        quic_transport: QuicTransportParams,
        media: MediaSetup,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            bind_address,
            tls_cert_path,
            tls_key_path,
            admission,
            session_manager,
            mc_client,
            handler_id,
            max_connections,
            quic_transport,
            media,
            active_connections: Arc::new(AtomicUsize::new(0)),
            cancel_token,
        }
    }

    /// The live count of connections this server is holding — the same counter
    /// the `MH_MAX_CONNECTIONS` gate reads. A handler's slot is released (and
    /// its `status` outcome recorded, in the same synchronous block) only when
    /// `handle_connection` returns, which is AFTER its session close and
    /// linger. Read-only; exposed so a test can wait for a handler to finish.
    #[must_use]
    pub fn active_connection_count(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.active_connections)
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
                target: "mh.webtransport",
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
                    target: "mh.webtransport",
                    error = %e,
                    cert_path = %self.tls_cert_path,
                    key_path = %self.tls_key_path,
                    "Failed to load TLS certificate"
                );
                format!("Failed to load TLS certificate: {e}")
            })?;

        let config = ServerConfig::builder()
            .with_bind_address(bind_addr)
            .with_custom_transport(identity, build_transport_config(&self.quic_transport))
            .build();

        let endpoint = Endpoint::server(config).map_err(|e| {
            error!(
                target: "mh.webtransport",
                error = %e,
                "Failed to create WebTransport endpoint"
            );
            format!("Failed to create WebTransport endpoint: {e}")
        })?;

        info!(
            target: "mh.webtransport",
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
                        target: "mh.webtransport",
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
                            target: "mh.webtransport",
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
                    let admission = self.admission;
                    let session_manager = self.session_manager.clone();
                    let mc_client = Arc::clone(&self.mc_client);
                    let handler_id = self.handler_id.clone();
                    let media = self.media.clone();
                    let connection_token = self.cancel_token.child_token();

                    tokio::spawn(async move {
                        let result = connection::handle_connection(
                            incoming_session,
                            admission,
                            session_manager,
                            mc_client,
                            handler_id,
                            media,
                            connection_token,
                        )
                        .await;

                        let count = active_connections.fetch_sub(1, Ordering::Relaxed) - 1;
                        #[expect(clippy::cast_precision_loss, reason = "connection counts << 2^52, no precision loss")]
                        metrics::set_active_connections(count as f64);

                        if let Err(e) = result {
                            metrics::record_webtransport_connection("error");
                            warn!(
                                target: "mh.webtransport",
                                error = %e,
                                "Connection handler completed with error"
                            );
                        }
                    });

                    // Update gauge after incrementing
                    let count = self.active_connections.load(Ordering::Relaxed);
                    #[expect(clippy::cast_precision_loss, reason = "connection counts << 2^52, no precision loss")]
                    metrics::set_active_connections(count as f64);
                }
            }
        }
    }
}

/// Build the explicit QUIC transport configuration MH declares (ADR-0036 §1).
///
/// # Why this exists at all
///
/// Until this function, MH built its server config with **no transport
/// configuration**, so quinn's defaults were live: a 1 MiB datagram send buffer
/// (≈89 seconds of queued audio on a realtime path), no keepalive at all, and
/// an unbounded connection receive window. ADR-0036 §1's framing is the point —
/// "a bound has to be declared to be assertable, and the default being adequate
/// is not the same as the default being chosen."
///
/// Split out from [`WebTransportServer::bind`] so every field can be asserted
/// without binding a socket or loading a TLS identity.
///
/// # Provenance of each setting
///
/// Env-driven (from [`QuicTransportParams`], all required): the uni-stream
/// bound, the datagram **send** buffer, the keepalive interval and the idle
/// timeout (`MH_MAX_IDLE_TIMEOUT_MS`, ADR-0039 §7). Compile-time constants
/// (see [`crate::config`]): both receive windows and the datagram **receive**
/// buffer. Everything not set here remains a quinn
/// default, and that set should only ever shrink.
fn build_transport_config(params: &QuicTransportParams) -> quinn::TransportConfig {
    let mut transport = quinn::TransportConfig::default();

    transport.max_concurrent_uni_streams(params.max_concurrent_uni_streams.into());

    // Egress backstop. MH's own bounded egress queue is validated at startup to
    // trip before this, so back-pressure is countable in MH's code rather than
    // silently discarded by quinn's drop-oldest eviction (ADR-0036 §1, §11).
    transport.datagram_send_buffer_size(params.datagram_send_buffer_bytes);

    // MUST be `Some`. quinn derives the advertised `max_datagram_frame_size`
    // from this field, so `None` would leave datagrams un-negotiated and the
    // audio path would not exist. See `config::DATAGRAM_RECEIVE_BUFFER_BYTES`
    // for the floor this value has to clear and why it is not derived from the
    // send-side frame budget.
    transport.datagram_receive_buffer_size(Some(DATAGRAM_RECEIVE_BUFFER_BYTES));

    // What refreshes a MUTED participant's NAT binding, since a muted
    // participant sends no media by definition (ADR-0036 §1, §5). The idle
    // timeout is declared alongside it because a keepalive is only meaningful
    // as a fraction of one; startup validation enforces the ratio.
    //
    // The idle timeout is also how a client notices a SILENTLY dead MH
    // (ADR-0039 §7: sized ≈ MC's N, no app-level keepalive). Startup bounds it
    // to `MAX_IDLE_TIMEOUT_MS_RANGE` (≤ 30 s), far inside quinn's `VarInt`
    // range, so the conversion cannot fail; if it ever did, NO idle timeout is
    // never the fallback — the bounded maximum is.
    transport.keep_alive_interval(Some(Duration::from_millis(params.keepalive_interval_ms)));
    transport.max_idle_timeout(Some(
        quinn::IdleTimeout::try_from(Duration::from_millis(params.max_idle_timeout_ms))
            .unwrap_or_else(|_| {
                // Unreachable for a validated config (pinned by a unit test);
                // never silent if it ever runs: MH would then serve a value
                // other than the one its startup log and gauge report.
                error!(
                    target: "mh.webtransport",
                    configured_max_idle_timeout_ms = params.max_idle_timeout_ms,
                    substituted_max_idle_timeout_ms =
                        common::config::MH_HEARTBEAT_TIMEOUT_CEILING_MS,
                    "QUIC max_idle_timeout out of quinn's range; substituting the bounded maximum"
                );
                quinn::IdleTimeout::from(quinn::VarInt::from_u32(
                    common::config::MH_HEARTBEAT_TIMEOUT_CEILING_MS,
                ))
            }),
    ));

    // Receive-side flow control. Without these, quinn leaves `receive_window`
    // at `VarInt::MAX`, so a peer could buffer without bound before MH's
    // stream accept loop (story 3 MH-A2) reads — making `max_connections` an
    // admission bound in name only. See `config::CONNECTION_RECEIVE_WINDOW_BYTES` for the arithmetic.
    transport.receive_window(CONNECTION_RECEIVE_WINDOW_BYTES.into());
    transport.stream_receive_window(STREAM_RECEIVE_WINDOW_BYTES.into());

    // Send-side bound (story 3 MH-A2). quinn's default is 10 MB per
    // connection, which would sit UNDER each subscriber's backlog cap so the
    // cap never tripped; see `config::CONNECTION_SEND_WINDOW_BYTES`. It is a
    // term of the boot memory check.
    transport.send_window(u64::from(CONNECTION_SEND_WINDOW_BYTES));

    transport
}

#[cfg(test)]
mod tests {
    //! Coverage for the transport configuration MH declares.
    //!
    //! `quinn::TransportConfig` exposes no getters, so these assert through its
    //! `Debug` rendering — which prints every field this function sets. That is
    //! weaker than reading the fields back, and it is the strongest check the
    //! upstream API allows; it still fails if a setter is dropped, because the
    //! rendered value reverts to quinn's default.

    use super::build_transport_config;
    use crate::config::{
        QuicTransportParams, DATAGRAM_RECEIVE_BUFFER_BYTES, NOMINAL_AUDIO_FRAME_BYTES,
    };

    fn params() -> QuicTransportParams {
        QuicTransportParams {
            max_concurrent_uni_streams: 64,
            datagram_send_buffer_audio_frames: 32,
            datagram_send_buffer_bytes: 32 * NOMINAL_AUDIO_FRAME_BYTES,
            keepalive_interval_ms: 2_000,
            max_idle_timeout_ms: 6_000,
        }
    }

    #[test]
    fn declares_every_adr_0036_section_1_setting() {
        let rendered = format!("{:?}", build_transport_config(&params()));

        // Env-driven values reach quinn unmodified.
        assert!(
            rendered.contains("max_concurrent_uni_streams: 64"),
            "uni-stream bound not applied: {rendered}"
        );
        assert!(
            rendered.contains(&format!(
                "datagram_send_buffer_size: {}",
                32 * NOMINAL_AUDIO_FRAME_BYTES
            )),
            "datagram send buffer not applied (or not the converted byte value): {rendered}"
        );
        assert!(
            rendered.contains("keep_alive_interval: Some(2s)"),
            "keepalive not applied: {rendered}"
        );
    }

    #[test]
    fn datagram_receive_buffer_is_some_so_datagrams_stay_negotiated() {
        // `None` here would leave `max_datagram_frame_size` unadvertised and
        // the entire audio path would not exist. This is the one setting whose
        // absence is silent rather than degraded.
        let rendered = format!("{:?}", build_transport_config(&params()));
        assert!(
            rendered.contains(&format!(
                "datagram_receive_buffer_size: Some({DATAGRAM_RECEIVE_BUFFER_BYTES})"
            )),
            "datagram receive buffer must be explicitly Some: {rendered}"
        );
    }

    #[test]
    fn receive_side_flow_control_is_declared_not_inherited() {
        // Undeclared, quinn leaves `receive_window` at VarInt::MAX, which makes
        // `MH_MAX_CONNECTIONS` an admission bound in name only.
        let rendered = format!("{:?}", build_transport_config(&params()));
        assert!(
            rendered.contains("receive_window: 262144"),
            "connection receive window not declared: {rendered}"
        );
        assert!(
            rendered.contains("stream_receive_window: 65536"),
            "stream receive window not declared: {rendered}"
        );
        assert!(
            rendered.contains("send_window: 524288"),
            "connection send window not declared (quinn's 10 MB default would sit under the \
             egress backlog cap): {rendered}"
        );
    }

    #[test]
    fn every_legal_idle_timeout_converts_without_the_fallback() {
        // Pins the "cannot fail" claim: both edges of the validated range fit
        // quinn's IdleTimeout, so the substitution arm is unreachable.
        let (floor, cap) = crate::config::MAX_IDLE_TIMEOUT_MS_RANGE;
        for ms in [floor, cap] {
            assert!(quinn::IdleTimeout::try_from(std::time::Duration::from_millis(ms)).is_ok());
        }
    }

    #[test]
    fn the_idle_timeout_is_the_configured_value_never_the_retired_constant() {
        // 4321 ms: neither the deployed 6000 nor the retired 30 s constant, so
        // only the config field can produce it.
        let mut p = params();
        p.max_idle_timeout_ms = 4_321;
        let rendered = format!("{:?}", build_transport_config(&p));
        assert!(
            // quinn renders the idle timeout in milliseconds, not as a Duration.
            rendered.contains("max_idle_timeout: Some(4321)"),
            "idle timeout not taken from config: {rendered}"
        );
        assert!(
            !rendered.contains("max_idle_timeout: Some(30000)"),
            "{rendered}"
        );
    }

    #[test]
    fn buffer_bytes_are_taken_from_config_never_recomputed_here() {
        // The frames -> bytes conversion has exactly one home (config load).
        // If this function ever recomputed it, a caller passing a deliberately
        // odd byte value would silently have it overwritten.
        let mut odd = params();
        odd.datagram_send_buffer_bytes = 12_345;
        let rendered = format!("{:?}", build_transport_config(&odd));
        assert!(
            rendered.contains("datagram_send_buffer_size: 12345"),
            "byte value was recomputed rather than taken from config: {rendered}"
        );
    }
}
