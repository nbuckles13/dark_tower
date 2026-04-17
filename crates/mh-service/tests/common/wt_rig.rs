//! WebTransport test rig: self-signed TLS + direct spawn of the production
//! `handle_connection` so per-connection results are observable.
//!
//! # TLS invariant
//!
//! This rig constructs `wtransport::Identity::self_signed(...)` and passes it
//! through the production `ServerConfig::builder().with_identity()` API — the
//! same code path `WebTransportServer::bind()` takes at runtime (see
//! `src/webtransport/server.rs:110-126`). A grep for `#[cfg(test)]` under
//! `src/webtransport/` returns zero matches — there is no test-only branch
//! on the server.
//!
//! The test client uses `with_no_cert_validation()`, a client-only relaxation
//! so the self-signed server cert is accepted.
//!
//! # Why bypass `WebTransportServer::accept_loop`?
//!
//! Production `accept_loop` spawns `connection::handle_connection` as a
//! `tokio::spawn` task and drops the `Result`. Tests need to assert on the
//! actual `MhError` variant, so this rig runs the same `handle_connection`
//! directly per-incoming and reports each result on an `mpsc::Receiver`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mh_service::auth::MhJwtValidator;
use mh_service::errors::MhError;
use mh_service::grpc::McClient;
use mh_service::session::SessionManagerHandle;
use mh_service::webtransport::connection::handle_connection;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wtransport::{Identity, ServerConfig};

pub struct WtRig {
    pub addr: SocketAddr,
    pub url: String,
    pub results_rx: mpsc::Receiver<Result<(), MhError>>,
    cancel_token: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl WtRig {
    pub async fn start(
        jwt_validator: Arc<MhJwtValidator>,
        session_manager: SessionManagerHandle,
        mc_client: Arc<McClient>,
        handler_id: String,
        register_meeting_timeout: Duration,
    ) -> Self {
        let identity = Identity::self_signed(["localhost", "127.0.0.1"])
            .expect("failed to build self-signed identity");

        let bind_addr: SocketAddr = "127.0.0.1:0"
            .parse()
            .expect("127.0.0.1:0 is a valid SocketAddr");
        let server_config = ServerConfig::builder()
            .with_bind_address(bind_addr)
            .with_identity(&identity)
            .build();

        let endpoint = wtransport::Endpoint::server(server_config)
            .expect("failed to create WebTransport endpoint");
        let addr = endpoint
            .local_addr()
            .expect("failed to read local addr for WT endpoint");
        let url = format!("https://127.0.0.1:{}", addr.port());

        let cancel_token = CancellationToken::new();
        let (results_tx, results_rx) = mpsc::channel(8);

        let cancel_clone = cancel_token.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = cancel_clone.cancelled() => break,
                    incoming = endpoint.accept() => {
                        let jwt_validator = Arc::clone(&jwt_validator);
                        let session_manager = session_manager.clone();
                        let mc_client = Arc::clone(&mc_client);
                        let handler_id = handler_id.clone();
                        let connection_token = cancel_clone.child_token();
                        let results_tx = results_tx.clone();
                        tokio::spawn(async move {
                            let result = handle_connection(
                                incoming,
                                jwt_validator,
                                session_manager,
                                mc_client,
                                handler_id,
                                register_meeting_timeout,
                                connection_token,
                            )
                            .await;
                            let _ = results_tx.send(result).await;
                        });
                    }
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            addr,
            url,
            results_rx,
            cancel_token,
            handle: Some(handle),
        }
    }

    /// Wait for the next per-connection result, bounded by `timeout`.
    pub async fn next_result(&mut self, timeout: Duration) -> Option<Result<(), MhError>> {
        tokio::time::timeout(timeout, self.results_rx.recv())
            .await
            .ok()
            .flatten()
    }
}

impl Drop for WtRig {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}
