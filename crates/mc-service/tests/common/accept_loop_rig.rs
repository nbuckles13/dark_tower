//! Accept-loop component-test rig: drives the real
//! `WebTransportServer::bind() → accept_loop()` under self-signed TLS.
//!
//! # Provenance — divergences from MH (`crates/mh-service/tests/common/accept_loop_rig.rs`)
//!
//! This rig is structurally a port of MH's canonical rig, with three principled
//! divergences driven by MC's stack genuinely differing from MH's:
//!
//! 1. **Mock injection at `redis_client` and `mh_client` seams.** MC's
//!    `WebTransportServer::new` takes an `Arc<dyn MhAssignmentStore>` and an
//!    `Arc<dyn MhRegistrationClient>` — the Redis seam isn't in MH at all.
//!    The rig accepts pre-built `Arc`s of these traits so callers can inject
//!    `MockMhAssignmentStore` / `MockMhRegistrationClient` from
//!    `tests/common/mod.rs`.
//!
//! 2. **No `SessionManagerHandle`.** MC has no equivalent type; sessions live
//!    inside the `MeetingControllerActorHandle` actor hierarchy. The rig
//!    surfaces `controller_handle` for test assertions on actor state and
//!    `mh_reg_client` for verifying RegisterMeeting fanout.
//!
//! 3. **`mc_id` and `mc_grpc_endpoint` constructor args.** MC's accept loop
//!    forwards these to `connection::handle_connection` so the spawned
//!    `register_meeting_with_handlers` task can identify itself to MH; MH has
//!    no equivalent. Defaulted to `"mc-test"` / `"http://mc-test:50052"`
//!    matching the deleted `join_tests.rs:213-246` accept-loop fork.
//!
//! Otherwise: byte-identical `WebTransportServer::bind() → accept_loop()`
//! invocation matching `main.rs:359-388`. Self-signed PEMs use the same
//! `rcgen + tempfile` pattern as MH's `write_self_signed_pems`. ADR-0032
//! §Decision rejects accept-loop forks that bypass production code; this rig
//! retires the fork at `crates/mc-service/tests/join_tests.rs:213-246`.
//!
//! # What this rig does NOT own
//!
//! No `mpsc::Receiver<Result<(), McError>>` for per-connection handler
//! results — see ADR-0032 §Decision. Tests observe handler outcomes via
//! metric emissions (`mc_webtransport_connections_total`,
//! `mc_jwt_validations_total`, `mc_session_join_failures_total`),
//! actor-system state, and the `MockMhRegistrationClient` call-recording
//! channel.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use mc_service::actors::MeetingControllerActorHandle;
use mc_service::auth::McJwtValidator;
use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::MhAssignmentStore;
use mc_service::webtransport::WebTransportServer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Component-test rig for the real `WebTransportServer::accept_loop`.
pub struct AcceptLoopRig {
    /// Advertise-style URL for clients (e.g. `https://127.0.0.1:PORT`).
    pub url: String,
    /// Bound local address of the accept endpoint.
    pub addr: SocketAddr,
    /// Controller handle shared with the accept loop — tests assert actor state via this.
    pub controller_handle: Arc<MeetingControllerActorHandle>,
    /// Cancellation token wired into the accept loop.
    cancel_token: CancellationToken,
    /// Handle for the spawned accept-loop task.
    accept_loop_handle: Option<JoinHandle<()>>,
    /// TempDir for the PEM files — kept alive for the rig's lifetime so
    /// `Identity::load_pemfiles` can re-read the paths at runtime.
    _tempdir: TempDir,
}

impl AcceptLoopRig {
    /// Start with `max_connections = 32` — convenience wrapper.
    pub async fn start(
        controller_handle: Arc<MeetingControllerActorHandle>,
        jwt_validator: Arc<McJwtValidator>,
        mh_store: Arc<dyn MhAssignmentStore>,
        mh_reg_client: Arc<dyn MhRegistrationClient>,
    ) -> Self {
        Self::start_with(
            controller_handle,
            jwt_validator,
            mh_store,
            mh_reg_client,
            "mc-test".to_string(),
            "http://mc-test:50052".to_string(),
            32,
        )
        .await
    }

    /// Start with explicit `max_connections` — used by capacity-rejection tests.
    ///
    /// Mirrors `WebTransportServer::new` shape; collapsing to a config struct
    /// would obscure the byte-identical-to-main.rs invariant.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_with(
        controller_handle: Arc<MeetingControllerActorHandle>,
        jwt_validator: Arc<McJwtValidator>,
        mh_store: Arc<dyn MhAssignmentStore>,
        mh_reg_client: Arc<dyn MhRegistrationClient>,
        mc_id: String,
        mc_grpc_endpoint: String,
        max_connections: usize,
    ) -> Self {
        let (tempdir, cert_path, key_path) = Self::write_self_signed_pems();

        let cancel_token = CancellationToken::new();
        let server = WebTransportServer::new(
            "127.0.0.1:0".to_string(),
            cert_path,
            key_path,
            Arc::clone(&controller_handle),
            jwt_validator,
            mh_store,
            mh_reg_client,
            mc_id,
            mc_grpc_endpoint,
            max_connections,
            cancel_token.clone(),
        );

        // Byte-identical to `main.rs:376-388` — real `bind()` then real
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
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Self {
            url,
            addr,
            controller_handle,
            cancel_token,
            accept_loop_handle: Some(accept_loop_handle),
            _tempdir: tempdir,
        }
    }

    /// Generate a self-signed Ed25519 cert (SAN `["localhost", "127.0.0.1"]`)
    /// and write PEMs into a temp dir.
    ///
    /// Ported from `crates/mh-service/tests/common/accept_loop_rig.rs`. Uses
    /// `rcgen` (dev-dep) because `wtransport::Identity::self_signed` returns
    /// an opaque `Identity` object with no stable PEM-serialization public
    /// API — and the production `WebTransportServer::bind()` reads PEMs from
    /// disk via `Identity::load_pemfiles`. The on-disk round trip is
    /// necessary to exercise the same code path as runtime.
    ///
    /// Consolidate to a shared test-utils crate after AC + GC backfills land
    /// in ADR-0032 Steps 4-5 (when three call sites exist).
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
