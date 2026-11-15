//! Accept-loop component-test rig: drives the real
//! `WebTransportServer::bind() → accept_loop()` under self-signed TLS.
//!
//! # What this rig owns
//!
//! - Self-signed TLS PEMs written to a `TempDir` (cleaned up on drop).
//! - A live `Endpoint<Server>` bound via production `WebTransportServer::bind()`.
//! - The `tokio::spawn`'d `accept_loop` task and its `CancellationToken`.
//! - A `SessionManagerHandle` tests can observe to verify accept-path
//!   side-effects (active-connection count, meeting registration, …).
//!
//! # What this rig does NOT own
//!
//! No `mpsc::Receiver<Result<(), MhError>>` for per-connection handler
//! results. The production `accept_loop` at
//! `crates/mh-service/src/webtransport/server.rs:188-212` `tokio::spawn`s
//! `handle_connection` and drops the `Result`. ADR-0032 §Decision rejects
//! both production-code modification (to forward the Result) and rig-side
//! forks that call `handle_connection` directly. Tests observe handler
//! outcomes via metric emissions (`mh_webtransport_connections_total`,
//! `mh_jwt_validations_total`), session-manager state, and mock-MC channels —
//! see `webtransport_integration.rs` and `webtransport_accept_loop_integration.rs`.
//!
//! # Replaces `wt_rig.rs`
//!
//! The prior `wt_rig.rs` bypassed `accept_loop` by calling `handle_connection`
//! directly so tests could assert on `Result<(), MhError>`. ADR-0032 §Decision
//! explicitly retires that bypass — this rig drives the real loop and tests
//! adapt to observable side-effects. See
//! `docs/devloop-outputs/2026-04-24-adr-0032-step-2-mh-metric-test-backfill/main.md`.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use mh_service::webtransport::connection::ClientAdmission;
use mh_service::webtransport::WebTransportServer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The deployed admission values (`infra/services/mh-service/config.env`):
/// skew 30 s, two connections awaiting their bind per `(meeting, sub)`.
pub const DEPLOYED_ADMISSION: ClientAdmission = ClientAdmission {
    clock_skew: Duration::from_secs(30),
    max_pending_per_participant: 2,
};

/// Component-test rig for the real `WebTransportServer::accept_loop`.
pub struct AcceptLoopRig {
    /// Advertise-style URL for clients (e.g. `https://127.0.0.1:PORT`).
    pub url: String,
    /// Bound local address of the accept endpoint.
    pub addr: SocketAddr,
    /// Session manager shared with the accept loop — tests observe state here.
    pub session_manager: SessionManagerHandle,
    /// Cancellation token wired into the accept loop.
    cancel_token: CancellationToken,
    /// The server's held-connection counter (`MH_MAX_CONNECTIONS` gate).
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Handle for the spawned accept-loop task.
    accept_loop_handle: Option<JoinHandle<()>>,
    /// TempDir for the PEM files — kept alive for the rig's lifetime so
    /// `Identity::load_pemfiles` can re-read the paths at runtime.
    _tempdir: TempDir,
}

impl AcceptLoopRig {
    /// Start with `max_connections = 32` and [`DEPLOYED_ADMISSION`].
    pub async fn start(
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
    ) -> Self {
        Self::start_with(
            session_manager,
            mc_client,
            handler_id,
            32,
            DEPLOYED_ADMISSION,
        )
        .await
    }

    /// Start with an explicit `max_connections` and admission settings —
    /// capacity-rejection, skew and pending-cap tests.
    pub async fn start_with(
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
        max_connections: usize,
        admission: ClientAdmission,
    ) -> Self {
        let (tempdir, cert_path, key_path) = Self::write_self_signed_pems();

        let cancel_token = CancellationToken::new();
        let server = WebTransportServer::new(
            "127.0.0.1:0".to_string(),
            cert_path,
            key_path,
            admission,
            session_manager.clone(),
            mc_client,
            handler_id,
            max_connections,
            // Deployed values (the fixture is pinned to
            // infra/services/mh-service/config.env). The rig drives the REAL
            // `bind()`, so these are the transport parameters the test
            // endpoint actually negotiates with.
            mh_test_utils::admission::fixture_quic_transport(),
            // Handles resolved once, exactly as `main` does — the rig drives
            // the real accept loop, so a media session started here uses the
            // production setup path.
            mh_service::media::MediaSetup {
                handles: std::sync::Arc::new(
                    mh_service::observability::metrics::resolve_media_handles(),
                ),
                latency_sample_ratio: 0.01,
                stream_path: mh_test_utils::admission::fixture_stream_path_limits(),
                downlink_channel_capacity: mh_service::media::downlink_channel_capacity(12),
            },
            cancel_token.clone(),
        );

        // Byte-identical to `main.rs:258-260` — real `bind()` then real
        // `accept_loop()` on the returned endpoint.
        let endpoint = server
            .bind()
            .await
            .expect("WebTransportServer::bind() failed in accept-loop rig");
        let addr = endpoint
            .local_addr()
            .expect("endpoint local_addr() must be available after bind()");
        let url = format!("https://127.0.0.1:{}", addr.port());
        let active = server.active_connection_count();

        let accept_loop_handle = tokio::spawn(async move {
            server.accept_loop(endpoint).await;
        });

        // Give the accept loop a moment to reach its `endpoint.accept()` await.
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            url,
            addr,
            session_manager,
            cancel_token,
            active,
            accept_loop_handle: Some(accept_loop_handle),
            _tempdir: tempdir,
        }
    }

    /// Generate a self-signed ECDSA P-256 cert (SAN `["localhost", "127.0.0.1"]`
    /// — same scope as `wt_rig.rs` precedent) and write PEMs into a temp dir.
    ///
    /// Uses `rcgen` (dev-dep) because `wtransport::Identity::self_signed`
    /// returns an opaque `Identity` object with no stable PEM-serialization
    /// public API — and the production `WebTransportServer::bind()` reads
    /// PEMs from disk via `Identity::load_pemfiles`. The on-disk round trip
    /// is necessary to exercise the same code path as runtime.
    fn write_self_signed_pems() -> (TempDir, String, String) {
        let cert = rcgen::generate_simple_self_signed(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ])
        .expect("rcgen failed to generate self-signed cert for accept-loop rig");

        let tempdir = tempfile::tempdir().expect("tempdir creation failed");
        let cert_path: PathBuf = tempdir.path().join("cert.pem");
        let key_path: PathBuf = tempdir.path().join("key.pem");

        std::fs::write(&cert_path, cert.cert.pem()).expect("failed to write cert PEM");
        std::fs::write(&key_path, cert.signing_key.serialize_pem())
            .expect("failed to write key PEM");

        let cert_path_s = cert_path
            .to_str()
            .expect("cert path must be UTF-8")
            .to_string();
        let key_path_s = key_path
            .to_str()
            .expect("key path must be UTF-8")
            .to_string();

        (tempdir, cert_path_s, key_path_s)
    }
}

impl AcceptLoopRig {
    /// Wait (bounded poll, never a sleep-and-hope) until the server holds no
    /// connection: every `handle_connection` has RETURNED. The accept loop
    /// releases the slot and records the connection's `status` outcome in one
    /// synchronous block, so once this returns on `current_thread`, a
    /// `mh_webtransport_connections_total{status="error"}` that WOULD be
    /// recorded already has been — the sync point the F9 (decline is not an
    /// error) delta-0 assertions need.
    pub async fn wait_idle(&self, within: Duration) {
        self.wait_held(0, within).await;
    }

    /// As [`Self::wait_idle`], but until exactly `held` connections remain
    /// (e.g. an incumbent that stays connected).
    pub async fn wait_held(&self, held: usize, within: Duration) {
        let stop = std::time::Instant::now() + within;
        while self.active.load(std::sync::atomic::Ordering::SeqCst) != held {
            assert!(
                std::time::Instant::now() < stop,
                "a connection handler did not return within {within:?}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Server shutdown: cancel the accept loop's token (every connection's
    /// parent) WITHOUT aborting the loop task, exactly as `main` does on
    /// SIGTERM — so the connection tasks run their shutdown arms.
    pub fn shutdown(&self) {
        self.cancel_token.cancel();
    }

    /// The server's shutdown token itself, for a caller that cancels it as
    /// `main` does — the planned drain (`mh_service::drain::run`, story 4 MH-3).
    pub fn shutdown_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}

impl Drop for AcceptLoopRig {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        if let Some(h) = self.accept_loop_handle.take() {
            h.abort();
        }
    }
}
