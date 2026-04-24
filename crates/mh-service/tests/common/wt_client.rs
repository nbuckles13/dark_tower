//! WebTransport client utilities for mh-service integration tests.
//!
//! Uses `with_no_cert_validation()` on the client to accept the test rig's
//! self-signed server cert. This is a client-only relaxation; the server TLS
//! code path is the production one (see `accept_loop_rig::AcceptLoopRig`).

use wtransport::{ClientConfig, Endpoint};

/// Build a fresh wtransport client configured to trust self-signed certs.
pub fn build_client() -> Endpoint<wtransport::endpoint::endpoint_side::Client> {
    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    Endpoint::client(client_config).expect("failed to build wtransport client endpoint")
}

/// Connect to `wt_url` and open a bidi stream.
pub async fn connect_and_open_bi(
    wt_url: &str,
) -> (
    wtransport::Connection,
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
    let client = build_client();
    let conn = client
        .connect(wt_url)
        .await
        .expect("failed to connect to WT test server");
    let (send, recv) = conn
        .open_bi()
        .await
        .expect("failed to open bi stream")
        .await
        .expect("bi stream never became ready");
    (conn, send, recv)
}

/// Write a length-prefixed frame (4-byte big-endian length + payload).
pub async fn write_framed(
    send: &mut wtransport::stream::SendStream,
    payload: &[u8],
) -> Result<(), wtransport::error::StreamWriteError> {
    let len = u32::try_from(payload.len()).expect("payload length must fit in u32");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    send.write_all(&frame).await
}
