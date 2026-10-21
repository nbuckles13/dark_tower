//! Contract tests for the REAL [`MediaTransport`] implementation over a live
//! `wtransport` connection.
//!
//! # Why this suite has to exist
//!
//! Before it, **nothing in the tree sent a datagram or opened a unidirectional
//! stream over the real transport.** `tests/common/wt_client.rs` is bidi-only,
//! and every `finish` / datagram assertion in the workspace runs against
//! `mh_test_utils::transport_shim::LossDelayTransport`. So
//! `tests/transport_seam_reachability.rs` proves the **double** honours the
//! seam's contracts, and nothing proved `wtransport` does.
//!
//! That is precisely the silent-divergence window
//! `mh_service::transport::MediaSendStream::finish` elevates to a *contract*
//! rather than leaving to each implementation: the forward path (story task 16)
//! runs against the double and will trust it exactly there. A real-impl pin is
//! therefore the acceptance test for "provide the REAL implementation", not an
//! optional extra.
//!
//! # What is deliberately NOT asserted here
//!
//! **No datagram-delivery assertion.** QUIC datagrams are unreliable by
//! definition, so a "sent N, received N" check over loopback is a latent
//! ADR-0028 zero-retry flake even if it is green 999 times in 1000 — and a
//! flaky gate on the media path is worse than no gate, because it teaches
//! people to re-run. The happy-path delivery check below therefore runs over
//! the **reliable** unidirectional stream, which exercises the same
//! `open_uni` / `write_all` / `finish` code path with guaranteed delivery.
//!
//! Datagram *error* mapping (peer does not support datagrams, datagram too
//! large) is covered deterministically by the free-function unit tests in
//! `webtransport/media_transport.rs`; those arms cannot be driven on demand
//! over a real loopback connection.
//!
//! # Determinism
//!
//! Every assertion here turns on **local** state, never on delivery timing:
//! `finish`-then-`write` is local quinn stream state, connection close is
//! driven by the test, and the stream round-trip is reliable-ordered by QUIC.

// Integration-test convention (matches `webtransport_integration.rs`): a failed
// rig step should abort the test loudly at the line that failed, not propagate a
// `Result` that obscures which setup step broke.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod common;

use bytes::Bytes;
use mh_service::transport::{MediaSendStream, MediaTransport, TransportCause, TransportError};
use mh_service::webtransport::WtMediaTransport;
use mh_test_utils::transport_shim::LossDelayTransport;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use wtransport::{Endpoint, Identity, ServerConfig};

/// How long a test waits for loopback progress before declaring the rig broken.
///
/// Generous on purpose: this is a rig-liveness backstop, never a timing
/// assertion. No test here passes or fails on how long something took.
const RIG_TIMEOUT: Duration = Duration::from_secs(10);

/// A live client↔server WebTransport pair, with the **server** side wrapped in
/// the production [`WtMediaTransport`].
///
/// The server side is the one under test because that is the side MH is: the
/// forward path opens egress streams and sends datagrams from here.
struct RealTransportPair {
    /// The production implementation, over the accepted server-side connection.
    server: WtMediaTransport,
    /// The raw client connection, used to observe what the server sent and to
    /// drive connection close.
    client: wtransport::Connection,
}

impl RealTransportPair {
    /// Bind a self-signed `wtransport` server on an ephemeral loopback port,
    /// connect one client, and hand back both ends.
    ///
    /// Deliberately **not** built on `common::accept_loop_rig`: that rig drives
    /// MH's full accept loop including the framed-JWT gate, which is a
    /// different subject. What these tests need is the narrowest possible thing
    /// — an accepted `wtransport::Connection` to wrap.
    async fn establish() -> Self {
        let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
            .expect("self-signed identity generation must succeed");

        let config = ServerConfig::builder()
            .with_bind_address("127.0.0.1:0".parse().expect("valid loopback address"))
            .with_identity(identity)
            .build();

        let endpoint = Endpoint::server(config).expect("server endpoint must bind");
        let port = endpoint
            .local_addr()
            .expect("bound endpoint must report its address")
            .port();

        let (conn_tx, conn_rx) = oneshot::channel();
        tokio::spawn(async move {
            let session = endpoint.accept().await;
            let request = session
                .await
                .expect("incoming session must produce a request");
            let connection = request.accept().await.expect("session must be acceptable");
            // If the receiver is gone the test already finished; nothing to do.
            let _ = conn_tx.send(connection);
        });

        let client = common::wt_client::build_client()
            .connect(format!("https://127.0.0.1:{port}"))
            .await
            .expect("client must connect to the test server");

        let server = tokio::time::timeout(RIG_TIMEOUT, conn_rx)
            .await
            .expect("server must accept the session within the rig timeout")
            .expect("server task must not drop the connection");

        Self {
            server: WtMediaTransport::new(Arc::new(server)),
            client,
        }
    }
}

/// The seam's finish-then-write contract, written **once** and applied to both
/// implementations.
///
/// `mh_service::transport::MediaSendStream::finish` states as a contract that a
/// `write_all` after a successful `finish` MUST return
/// [`TransportError::StreamClosed`]. Writing the assertion generically is what
/// turns "the real implementation was tested" into "the two implementations
/// agree on the one contract the forward path relies on" — which is the claim
/// that actually matters, and it costs nothing extra.
///
/// This is a shared **assertion body**, deliberately not a conformance harness:
/// no implementation registry, no growing parity set, no `mh-test-utils`
/// surface. Those are a separate design owned by the test specialist.
async fn assert_write_after_finish_is_stream_closed<S: MediaSendStream>(mut stream: S) {
    stream
        .write_all(b"a frame before the stream is finished")
        .await
        .expect("writing to a live stream must succeed");

    stream.finish().await.expect("finish must succeed");

    let after = stream.write_all(b"a frame after finish").await;

    assert_eq!(
        after,
        Err(TransportError::StreamClosed(TransportCause::local(
            "expected"
        ))),
        "write after finish must report StreamClosed: the forward path relies on this to \
         distinguish a dead STREAM from a dead CONNECTION, and a double diverging from the real \
         transport here would be invisible exactly where task 16 trusts it"
    );
}

#[tokio::test]
async fn real_transport_reports_stream_closed_on_write_after_finish() {
    let pair = RealTransportPair::establish().await;

    let stream = tokio::time::timeout(RIG_TIMEOUT, pair.server.open_uni())
        .await
        .expect("open_uni must not hang")
        .expect("open_uni must succeed on a live connection");

    assert_write_after_finish_is_stream_closed(stream).await;
}

#[tokio::test]
async fn the_test_double_agrees_with_the_real_transport_on_write_after_finish() {
    // Same assertion body, other implementation. If these two ever diverge, one
    // of these tests fails — rather than the forward path silently inheriting a
    // behaviour the double invented.
    let shim = LossDelayTransport::new();
    let stream = shim
        .open_uni()
        .await
        .expect("shim open_uni must succeed by default");

    assert_write_after_finish_is_stream_closed(stream).await;
}

#[tokio::test]
async fn real_transport_reports_connection_closed_when_the_peer_goes_away() {
    // This is the forward path's LOOP-TERMINATION signal. If `recv_datagram`
    // resolved to anything else on close — or never resolved — the ingress loop
    // would spin or hang on every disconnect.
    //
    // Deterministic: the test drives the close, and the assertion is on the
    // resulting error, not on how quickly it arrived.
    let pair = RealTransportPair::establish().await;

    let recv = tokio::spawn({
        let server = pair.server.clone();
        async move { server.recv_datagram().await }
    });

    pair.client.close(0u32.into(), b"test over");

    let result = tokio::time::timeout(RIG_TIMEOUT, recv)
        .await
        .expect("recv_datagram must resolve once the peer closes, not hang")
        .expect("the receive task must not panic");

    assert_eq!(
        result,
        Err(TransportError::ConnectionClosed(TransportCause::local(
            "expected"
        ))),
        "a closed connection must surface as ConnectionClosed"
    );
}

#[tokio::test]
async fn real_transport_delivers_over_the_reliable_stream_path() {
    // Happy path, over the RELIABLE path on purpose — see the module docs on
    // why there is no datagram-delivery assertion. This is what proves
    // `open_uni` / `write_all` / `finish` are genuinely wired to wtransport
    // rather than merely compiling and returning something plausible.
    let pair = RealTransportPair::establish().await;
    let payload = Bytes::from_static(b"one group of pictures, notionally");

    let mut stream = tokio::time::timeout(RIG_TIMEOUT, pair.server.open_uni())
        .await
        .expect("open_uni must not hang")
        .expect("open_uni must succeed on a live connection");

    stream
        .write_all(&payload)
        .await
        .expect("write must succeed");
    stream.finish().await.expect("finish must succeed");

    let mut received = tokio::time::timeout(RIG_TIMEOUT, pair.client.accept_uni())
        .await
        .expect("the client must observe the opened stream")
        .expect("accepting the uni stream must succeed");

    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];
    // Read to end-of-stream. `finish` is what makes this terminate; without it
    // this loop would be the hang that proves `finish` did nothing.
    while let Some(n) = tokio::time::timeout(RIG_TIMEOUT, received.read(&mut chunk))
        .await
        .expect("reads must make progress")
        .expect("reading the uni stream must succeed")
    {
        buf.extend_from_slice(&chunk[..n]);
    }

    assert_eq!(
        Bytes::from(buf),
        payload,
        "bytes written through the real MediaSendStream must arrive intact"
    );
}

/// **Measurement, not a contract**: does `wtransport::Datagram::payload()`
/// hand out a *uniquely-owned* `Bytes`?
///
/// The forward path's fan-out takes a zero-copy path for one edge per frame:
/// `Bytes::try_into_mut()` → rewrite the 6-byte relay region in place →
/// `freeze()`. Whether that path is ever taken **in production** depends
/// entirely on the vendor, because MH does not allocate the ingress buffer.
/// Correctness does not: `media::forward` falls back to the per-connection
/// arena when the buffer is shared, and the arena path is exercised by the
/// N>=2 fan-out gates.
///
/// So this pins the *observed* answer rather than a required one. If a
/// `wtransport` or `quinn` upgrade flips it, this fails loudly and the
/// zero-copy claim in `docs/devloop-outputs/2026-09-05-mh-audio-datagram-forward-path/main.md`
/// gets re-measured instead of quietly becoming false — which is exactly the
/// failure mode `mh_service::transport`'s own docs warn about for properties
/// asserted *through* the seam rather than measured at it.
///
/// Delivery is treated as a rig precondition, not as an assertion about QUIC:
/// the datagram is sent repeatedly until one arrives, because (as this file's
/// header records) QUIC datagrams are unreliable and no test here may assert
/// delivery. If none arrives within the rig timeout the rig is broken and the
/// test says so.
#[tokio::test]
async fn measure_whether_a_received_datagram_payload_is_uniquely_owned() {
    let pair = RealTransportPair::establish().await;

    let received = tokio::time::timeout(RIG_TIMEOUT, async {
        loop {
            // Repeated sends: unreliable transport, rig precondition.
            let _ = pair
                .client
                .send_datagram(Bytes::from_static(b"measurement"));
            tokio::time::sleep(Duration::from_millis(20)).await;
            if let Ok(payload) =
                tokio::time::timeout(Duration::from_millis(50), pair.server.recv_datagram()).await
            {
                return payload.expect("connection must stay alive during the measurement");
            }
        }
    })
    .await
    .expect("the rig must deliver at least one loopback datagram within the timeout");

    // Consumed, NOT cloned: `received.clone().try_into_mut()` can never
    // succeed, because the clone itself makes the refcount 2. Measuring
    // through a clone would "prove" the shared answer no matter what the
    // vendor does — a green measurement of nothing.
    let uniquely_owned = received.try_into_mut().is_ok();

    assert!(
        uniquely_owned,
        "MEASURED 2026-09-05 against wtransport 0.7.2 / quinn 0.11.11: the payload handed out \
         by `WtMediaTransport::recv_datagram` IS uniquely owned, so the forward path's \
         zero-copy edge takes `try_into_mut()` in production. The reason is load-bearing and \
         fragile: `Datagram::payload()` returns `quic_dgram.slice(payload_offset..)`, which \
         SHARES the refcount while the `Datagram` lives — it is unique only because \
         `recv_datagram` drops the `Datagram` before returning. Holding the `Datagram` (to \
         read `session_id`, say) would silently move every frame onto the arena copy. If this \
         now fails, correctness is unaffected (the arena path is taken) but the zero-copy \
         claim in main.md's §Devloop Verification Steps must be re-measured, not deleted."
    );
}

/// Read a client-side uni stream to its end, reporting how it ended.
async fn read_to_end(
    mut received: wtransport::RecvStream,
) -> (Vec<u8>, Result<(), wtransport::error::StreamReadError>) {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match tokio::time::timeout(RIG_TIMEOUT, received.read(&mut chunk))
            .await
            .expect("reads must make progress")
        {
            Ok(Some(n)) => buf.extend_from_slice(chunk.get(..n).unwrap_or(&[])),
            Ok(None) => return (buf, Ok(())),
            Err(e) => return (buf, Err(e)),
        }
    }
}

/// The seam's ABANDONMENT contract on the PRODUCTION implementation (story 3
/// MH-A2): a send stream dropped without `finish`/`reset` RESETS — the peer
/// sees `RESET_STREAM`, never a clean FIN on a possibly half-written frame.
/// quinn's own `Drop` would FINISH; this is what makes the shim's
/// drop-equals-reset model faithful rather than a property production lacks.
///
/// The peer first accepts the stream and reads its first bytes: a reset that
/// overtakes the WebTransport stream header leaves the peer unable to
/// attribute the stream to the session at all, so it would never surface.
#[tokio::test]
async fn a_dropped_real_send_stream_resets_rather_than_finishing() {
    let pair = RealTransportPair::establish().await;
    let mut stream = tokio::time::timeout(RIG_TIMEOUT, pair.server.open_uni())
        .await
        .expect("open_uni must not hang")
        .expect("open_uni must succeed");
    stream.write_all(b"half a frame").await.expect("write");
    let mut received = tokio::time::timeout(RIG_TIMEOUT, pair.client.accept_uni())
        .await
        .expect("the client must observe the stream")
        .expect("accept_uni");
    let mut first = [0_u8; 4];
    received.read_exact(&mut first).await.expect("first bytes");
    drop(stream);
    let (_, end) = read_to_end(received).await;
    assert_reset_with_group_abandoned(end, "a dropped stream must arrive RESET, not finished");
}

/// Fail CLOSED: only a stream RESET carrying MH's `GROUP_ABANDONED` code
/// passes — a connection loss, a stop or a rig race does not.
fn assert_reset_with_group_abandoned(
    end: Result<(), wtransport::error::StreamReadError>,
    what: &str,
) {
    let code = match end {
        Err(wtransport::error::StreamReadError::Reset(code)) => Some(code.into_inner()),
        _ => None,
    };
    assert_eq!(
        code,
        Some(u64::from(mh_service::transport::GROUP_ABANDONED)),
        "{what}: expected RESET_STREAM(GROUP_ABANDONED), got {end:?}"
    );
}

/// An explicit reset reaches the peer as a reset; a finish as a clean end.
#[tokio::test]
async fn real_reset_and_finish_reach_the_peer_as_such() {
    let pair = RealTransportPair::establish().await;
    let mut reset = pair.server.open_uni().await.expect("open");
    reset.write_all(b"xxxx").await.expect("write");
    let mut received = pair.client.accept_uni().await.expect("accept");
    let mut first = [0_u8; 4];
    received.read_exact(&mut first).await.expect("first bytes");
    reset
        .reset(mh_service::transport::GROUP_ABANDONED)
        .expect("reset a live stream");
    assert_reset_with_group_abandoned(read_to_end(received).await.1, "an explicit reset");

    let mut finished = pair.server.open_uni().await.expect("open");
    finished.set_priority(mh_service::media::downlink::quic_priority(2));
    finished.write_all(b"whole group").await.expect("write");
    finished.finish().await.expect("finish");
    drop(finished); // finished: the drop contract must NOT reset it
    let received = pair.client.accept_uni().await.expect("accept");
    let (bytes, end) = read_to_end(received).await;
    assert!(end.is_ok(), "a finished stream ends cleanly: {end:?}");
    assert_eq!(bytes, b"whole group");
}

/// The receive side over real QUIC: `accept_uni` + `read_chunk` deliver the
/// bytes and the FIN, and a peer reset surfaces as `StreamReadError::Reset`.
#[tokio::test]
async fn real_accept_uni_reads_chunks_fin_and_reset() {
    use mh_service::transport::{MediaRecvStream, StreamReadError};
    let pair = RealTransportPair::establish().await;

    let mut sent = pair
        .client
        .open_uni()
        .await
        .expect("open")
        .await
        .expect("opened");
    sent.write_all(b"group bytes").await.expect("write");
    sent.finish().await.expect("finish");
    let mut stream = tokio::time::timeout(RIG_TIMEOUT, pair.server.accept_uni())
        .await
        .expect("accept must not hang")
        .expect("accept_uni");
    let mut got = Vec::new();
    while let Some(chunk) = tokio::time::timeout(RIG_TIMEOUT, stream.read_chunk(1024))
        .await
        .expect("progress")
        .expect("read")
    {
        got.extend_from_slice(&chunk);
    }
    assert_eq!(got, b"group bytes");

    let mut abandoned = pair
        .client
        .open_uni()
        .await
        .expect("open")
        .await
        .expect("opened");
    abandoned.write_all(b"partial").await.expect("write");
    let mut stream = tokio::time::timeout(RIG_TIMEOUT, pair.server.accept_uni())
        .await
        .expect("accept must not hang")
        .expect("accept_uni");
    let first = stream.read_chunk(1024).await.expect("read").expect("bytes");
    assert!(!first.is_empty());
    abandoned.reset(0x5254_u32.into()).expect("reset");
    let outcome = loop {
        match tokio::time::timeout(RIG_TIMEOUT, stream.read_chunk(1024))
            .await
            .expect("progress")
        {
            Ok(Some(_)) => {}
            other => break other,
        }
    };
    assert_eq!(outcome, Err(StreamReadError::Reset(0x5254)));
    stream.stop(mh_service::transport::INGRESS_REFUSED);
}
