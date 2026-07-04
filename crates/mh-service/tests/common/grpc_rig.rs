//! gRPC test rig running `MhAuthLayer` + `MhMediaService` as in `main.rs`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use common::jwt::JwksClient;
use common::observability::otel_grpc::server_interceptor;
use mh_service::grpc::{MhAuthLayer, MhMediaService, SpanLayer};
use mh_service::session::SessionManagerHandle;
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerServiceServer;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;

/// Running mh-service gRPC rig. The `Drop` impl cancels and aborts the spawned
/// server task, so panicking tests do not leak tasks between binaries.
pub struct GrpcRig {
    pub addr: SocketAddr,
    pub session_manager: SessionManagerHandle,
    cancel_token: CancellationToken,
    handle: Option<JoinHandle<()>>,
}

impl GrpcRig {
    /// Build and spawn a real mh-service gRPC stack on `127.0.0.1:0`.
    ///
    /// The caller supplies a `JwksClient` (already pointing at a wiremock JWKS)
    /// and the `SessionManagerHandle` the service should share.
    pub async fn start(
        jwks_client: Arc<JwksClient>,
        session_manager: SessionManagerHandle,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind gRPC TcpListener");
        let addr = listener
            .local_addr()
            .expect("failed to read local addr for gRPC server");

        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();

        let auth_layer = MhAuthLayer::new(jwks_client, 300);
        let mh_media_service = MhMediaService::new(session_manager.clone());

        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

        // R-56: mirror main.rs — inbound W3C trace-context extraction via
        // server_interceptor(), independent of auth_layer (different headers).
        // SpanLayer supplies the ambient span the interceptor attaches to
        // (see `grpc::span_layer` module docs) — required for the interceptor
        // to have any effect.
        let server = Server::builder()
            .layer(SpanLayer)
            .layer(auth_layer)
            .add_service(MediaHandlerServiceServer::with_interceptor(
                mh_media_service,
                server_interceptor(),
            ))
            .serve_with_incoming_shutdown(incoming, async move {
                cancel_token_clone.cancelled().await;
            });

        let handle = tokio::spawn(async move {
            let _ = server.await;
        });

        // Give tonic a moment to bring HTTP/2 online.
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            addr,
            session_manager,
            cancel_token,
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for GrpcRig {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}
