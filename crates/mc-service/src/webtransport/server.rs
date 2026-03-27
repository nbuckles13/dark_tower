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
use crate::observability::metrics;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Endpoint, Identity, ServerConfig};

use super::connection;

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
    /// Maximum concurrent connections (bounds resource exhaustion).
    max_connections: usize,
    /// Active connection count.
    active_connections: Arc<AtomicUsize>,
    /// Cancellation token for graceful shutdown.
    cancel_token: CancellationToken,
}

impl WebTransportServer {
    /// Create a new WebTransport server.
    #[must_use]
    pub fn new(
        bind_address: String,
        tls_cert_path: String,
        tls_key_path: String,
        controller_handle: Arc<MeetingControllerActorHandle>,
        jwt_validator: Arc<McJwtValidator>,
        max_connections: usize,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            bind_address,
            tls_cert_path,
            tls_key_path,
            controller_handle,
            jwt_validator,
            max_connections,
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

        let config = ServerConfig::builder()
            .with_bind_address(bind_addr)
            .with_identity(&identity)
            .build();

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
                    let connection_token = self.cancel_token.child_token();

                    tokio::spawn(async move {
                        let result = connection::handle_connection(
                            incoming_session,
                            controller_handle,
                            jwt_validator,
                            connection_token,
                        )
                        .await;

                        active_connections.fetch_sub(1, Ordering::Relaxed);

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
