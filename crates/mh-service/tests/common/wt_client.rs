//! WebTransport client utilities for mh-service integration tests.
//!
//! Uses `with_no_cert_validation()` on the client to accept the test rig's
//! self-signed server cert. This is a client-only relaxation; the server TLS
//! code path is the production one (see `accept_loop_rig::AcceptLoopRig`).

use prost::Message;
use proto_gen::dark_tower::signaling::v1::{mh_client_message, MhClientMessage, MhConnectRequest};
use wtransport::{ClientConfig, Endpoint};

/// Build a fresh wtransport client configured to trust self-signed certs.
pub fn build_client() -> Endpoint<wtransport::endpoint::endpoint_side::Client> {
    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    Endpoint::client(client_config).expect("failed to build wtransport client endpoint")
}

/// A client whose per-stream receive window is `window` bytes: the peer can
/// send at most that much on any stream before this client grants more credit.
/// With a blackholed relay that credit never comes — a peer that WITHHOLDS
/// flow-control credit (the close-linger bound must hold against it).
pub fn build_client_with_stream_window(
    window: u32,
) -> Endpoint<wtransport::endpoint::endpoint_side::Client> {
    let mut client_config = ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    let mut transport = wtransport::quinn::TransportConfig::default();
    transport.stream_receive_window(wtransport::quinn::VarInt::from_u32(window));
    client_config
        .quic_config_mut()
        .transport_config(std::sync::Arc::new(transport));
    Endpoint::client(client_config).expect("failed to build wtransport client endpoint")
}

/// Connect with `client` to `wt_url` and open a bidi stream.
pub async fn connect_and_open_bi_with(
    client: &Endpoint<wtransport::endpoint::endpoint_side::Client>,
    wt_url: &str,
) -> (
    wtransport::Connection,
    wtransport::stream::SendStream,
    wtransport::stream::RecvStream,
) {
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

/// Encode `MhClientMessage{ConnectRequest{media_dial_token}}` and frame-write
/// it.
///
/// This is the production wire format on MH's bidi accept path. Use this
/// helper for positive-path tests; use [`write_framed`] directly for negative
/// tests that need raw bytes (malformed envelopes, oversized payloads).
///
/// `trace_parent`/`trace_state` are proto3-default empty strings, matching
/// today's clients that don't yet populate R-58's trace fields — see
/// [`write_mh_connect_with_trace`] for the trace-carrying variant.
pub async fn write_mh_connect(
    send: &mut wtransport::stream::SendStream,
    media_dial_token: &str,
) -> Result<(), wtransport::error::StreamWriteError> {
    write_mh_connect_with_trace(send, media_dial_token, "", "").await
}

/// The connect envelope an OLD client sends: `media_dial_token` plus a string
/// on `MhConnectRequest` tag 1, the reserved tag of the deleted `join_token`
/// (story 4 P3). Hand-encoded, because the generated type no longer has the
/// field; MH must decode tag 1 as an unknown field and never read it.
///
/// Deliberate twin of `crates/env-tests/src/fixtures/mc_session.rs`
/// `mh_connect_frame_with_vacated_tag_1` (env-tests cannot reach MH test
/// support, ADR-0028 layering): a change to the vacated-tag shape changes both.
pub async fn write_mh_connect_with_vacated_tag_1(
    send: &mut wtransport::stream::SendStream,
    tag_1_payload: &str,
    media_dial_token: &str,
) -> Result<(), wtransport::error::StreamWriteError> {
    fn len_field(tag: u32, payload: &[u8], out: &mut Vec<u8>) {
        prost::encoding::encode_key(tag, prost::encoding::WireType::LengthDelimited, out);
        prost::encoding::encode_varint(
            u64::try_from(payload.len()).expect("payload length fits u64"),
            out,
        );
        out.extend_from_slice(payload);
    }
    let mut request = Vec::new();
    len_field(1, tag_1_payload.as_bytes(), &mut request);
    MhConnectRequest {
        media_dial_token: media_dial_token.to_string(),
    }
    .encode(&mut request)
    .expect("Vec has unbounded capacity");
    // `MhClientMessage.connect_request` is oneof tag 1.
    let mut envelope = Vec::new();
    len_field(1, &request, &mut envelope);
    write_framed(send, &envelope).await
}

/// Like [`write_mh_connect`], but sets the envelope's W3C `trace_parent`/
/// `trace_state` fields (R-58) to the given values instead of leaving them
/// proto3-default-empty. Used to exercise the WebTransport trace-context
/// extraction path in `webtransport/connection.rs::handle_connection`.
pub async fn write_mh_connect_with_trace(
    send: &mut wtransport::stream::SendStream,
    media_dial_token: &str,
    trace_parent: &str,
    trace_state: &str,
) -> Result<(), wtransport::error::StreamWriteError> {
    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                media_dial_token: media_dial_token.to_string(),
            },
        )),
        trace_parent: trace_parent.to_string(),
        trace_state: trace_state.to_string(),
    };
    let encoded = envelope.encode_to_vec();
    write_framed(send, &encoded).await
}

// ---------------------------------------------------------------------------
// Session-close observation (ADR-0039 §7)
// ---------------------------------------------------------------------------
//
// The PRIMARY observer is the vendored wtransport accessor
// `Connection::session_close_received()`, filled only by upstream's
// CLOSE_WEBTRANSPORT_SESSION parser (and its bare-CONNECT-FIN branch, as code
// 0). It is NOT `closed()`: a wtransport client reports a QUIC
// CONNECTION_CLOSE carrying N and a capsule carrying N alike on some paths, and
// only the capsule reaches a browser. `closed()` is the secondary assertion.

/// `H3_NO_ERROR` (RFC 9114 §8.1), from wtransport-proto, never retyped.
pub fn h3_no_error() -> u64 {
    wtransport::proto::error::ErrorCode::NoError
        .to_code()
        .into_inner()
}

/// The wire number of `code`, read from the generated enum.
pub fn wire(code: proto_gen::dark_tower::signaling::v1::MhCloseCode) -> u64 {
    u64::from(i32::from(code).unsigned_abs())
}

/// Every `MhCloseCode` number MH can send — from `CloseCode::ALL` (the one
/// enumeration, coupled to the proto by `close.rs`'s own tests): none of them may
/// ever appear as a QUIC application close code (that would reach a browser as
/// transport loss).
fn every_close_code_number() -> Vec<u64> {
    mh_service::webtransport::close::CloseCode::ALL
        .iter()
        .map(|c| u64::from(c.wire()))
        .collect()
}

/// The QUIC-level end the peer saw must be one of the two the patched transport
/// produces — the peer's own close after the capsule, or MH's H3_NO_ERROR — and
/// NEVER a QUIC close carrying an `MhCloseCode` number.
fn assert_quic_end_is_codeless(end: &wtransport::error::ConnectionError, ctx: &str) {
    use wtransport::error::ConnectionError;
    assert!(
        matches!(
            end,
            ConnectionError::LocallyClosed | ConnectionError::ApplicationClosed(_)
        ),
        "{ctx}: unexpected QUIC-level end {end:?}"
    );
    if let ConnectionError::ApplicationClosed(close) = end {
        let code = close.code().into_inner();
        assert!(
            !every_close_code_number().contains(&code),
            "{ctx}: MH put close code {code} on a QUIC CONNECTION_CLOSE — a browser reads that \
             as transport loss; codes travel ONLY in the session-close capsule"
        );
        assert_eq!(
            code,
            h3_no_error(),
            "{ctx}: a QUIC close from MH must be H3_NO_ERROR (never 0x104 or an app code)"
        );
        assert!(
            close.reason().is_empty(),
            "{ctx}: QUIC close reason must be empty"
        );
    }
}

/// Assert MH closed the session with `expected`: a capsule carrying exactly that
/// code and an EMPTY message, then a code-less QUIC end. Returns how long after
/// the call the capsule was observed.
pub async fn expect_session_close(
    conn: &wtransport::Connection,
    expected: proto_gen::dark_tower::signaling::v1::MhCloseCode,
    within: std::time::Duration,
    ctx: &str,
) -> std::time::Duration {
    let start = std::time::Instant::now();
    let received = tokio::time::timeout(within, conn.session_close_received()).await;
    assert!(
        received.is_ok(),
        "{ctx}: no session end observed within {within:?}"
    );
    let elapsed = start.elapsed();
    let received = received.unwrap();
    assert!(
        received.is_some(),
        "{ctx}: the session ended WITHOUT a CLOSE_WEBTRANSPORT_SESSION capsule — a QUIC-only \
         close is transport loss to a browser, never {expected:?}"
    );
    let close = received.unwrap();
    assert_eq!(
        close.code().into_inner(),
        wire(expected),
        "{ctx}: wrong close code (expected {expected:?})"
    );
    assert!(
        close.reason().is_empty(),
        "{ctx}: close message must be empty (numeric only), got {} bytes",
        close.reason().len()
    );
    let end = tokio::time::timeout(within, conn.closed()).await;
    assert!(
        end.is_ok(),
        "{ctx}: QUIC did not end within {within:?} after the capsule"
    );
    assert_quic_end_is_codeless(&end.unwrap(), ctx);
    elapsed
}

/// Assert the session ended with NO session close: no capsule and no bare
/// CONNECT FIN (`session_close_received()` reports both), and a code-less QUIC end.
pub async fn expect_no_session_close(
    conn: &wtransport::Connection,
    within: std::time::Duration,
    ctx: &str,
) -> wtransport::error::ConnectionError {
    let received = tokio::time::timeout(within, conn.session_close_received()).await;
    assert!(
        received.is_ok(),
        "{ctx}: the session did not end within {within:?}"
    );
    let received = received.unwrap();
    assert!(
        received.is_none(),
        "{ctx}: a code-less end must send no capsule and no CONNECT FIN; got {received:?}"
    );
    let end = tokio::time::timeout(within, conn.closed()).await;
    assert!(end.is_ok(), "{ctx}: QUIC did not end within {within:?}");
    let end = end.unwrap();
    assert_quic_end_is_codeless(&end, ctx);
    end
}

/// Assert the session is still open after `for_at_least` (positive controls).
pub async fn expect_still_open(
    conn: &wtransport::Connection,
    for_at_least: std::time::Duration,
    ctx: &str,
) {
    assert!(
        tokio::time::timeout(for_at_least, conn.closed())
            .await
            .is_err(),
        "{ctx}: the session must still be open after {for_at_least:?}"
    );
}
