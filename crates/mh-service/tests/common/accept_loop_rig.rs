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

use mh_service::auth::MhJwtValidator;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use mh_service::webtransport::WebTransportServer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

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
    /// Handle for the spawned accept-loop task.
    accept_loop_handle: Option<JoinHandle<()>>,
    /// TempDir for the PEM files — kept alive for the rig's lifetime so
    /// `Identity::load_pemfiles` can re-read the paths at runtime.
    _tempdir: TempDir,
}

impl AcceptLoopRig {
    /// Start with `max_connections = 32` and `register_meeting_timeout = 30s`.
    pub async fn start(
        jwt_validator: Arc<MhJwtValidator>,
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
    ) -> Self {
        Self::start_with(
            jwt_validator,
            session_manager,
            mc_client,
            handler_id,
            32,
            Duration::from_secs(30),
        )
        .await
    }

    /// Start with an explicit `max_connections` — useful for capacity-rejection tests.
    pub async fn start_with(
        jwt_validator: Arc<MhJwtValidator>,
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
        max_connections: usize,
        register_meeting_timeout: Duration,
    ) -> Self {
        let (tempdir, cert_path, key_path) = Self::write_self_signed_pems();

        let cancel_token = CancellationToken::new();
        let server = WebTransportServer::new(
            "127.0.0.1:0".to_string(),
            cert_path,
            key_path,
            jwt_validator,
            session_manager.clone(),
            mc_client,
            handler_id,
            register_meeting_timeout,
            max_connections,
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
            accept_loop_handle: Some(accept_loop_handle),
            _tempdir: tempdir,
        }
    }

    /// Generate a self-signed Ed25519 cert (SAN `["localhost", "127.0.0.1"]`
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
        std::fs::write(&key_path, cert.key_pair.serialize_pem()).expect("failed to write key PEM");

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

impl Drop for AcceptLoopRig {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        if let Some(h) = self.accept_loop_handle.take() {
            h.abort();
        }
    }
}
