//! Shared WebTransport signalling client for the cluster media-flow suites.
//!
//! # One home for the MC/MH client plumbing
//!
//! The join-flow binaries (`24_join_flow.rs`, `26_mh_quic.rs`,
//! `27_mc_slot_placement.rs`) all speak
//! the same wire discipline: a wtransport connection to a self-signed Kind
//! endpoint, a 4-byte big-endian length prefix per protobuf message, an MC
//! `ClientMessage{JoinRequest{…}}` opener, and an MH
//! `MhClientMessage{ConnectRequest{…}}` opener. Each binary used to carry its
//! own copy of every one of those; this module is the single home so the
//! `JoinRequest` field list, the framing, and the MH connect envelope live in
//! exactly one place in env-tests.
//!
//! This lives in the LIB (`src/fixtures`), so `wtransport`, `proto-gen`,
//! `prost` and `bytes` are ordinary `[dependencies]` of `env-tests`, not only
//! `[dev-dependencies]` — the lib target has to be able to name `ServerMessage`
//! and friends. That is a deliberate reversal of the earlier "keep them
//! dev-only" decision: the structural duplication it was protecting against is
//! the thing being removed here. The suite still links ZERO service crates
//! (only `proto-gen`, a wire-contract crate), so the ADR-0028 black-box
//! layering the media fixtures record is intact.

use bytes::{BufMut, BytesMut};
use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    client_message, mh_client_message, server_message, ClientMessage, JoinRequest, JoinResponse,
    MhClientMessage, MhConnectRequest, ServerMessage,
};
use std::time::Duration;

use super::media::sample_identity_public_key;

/// Upper bound on a single framed MC/MH message, in bytes. Frames larger than
/// this are a wire defect, not a payload.
const MAX_FRAME_BYTES: usize = 65_536;

/// How long [`mc_join`] waits for the `JoinResponse` before giving up.
const JOIN_RESPONSE_BOUND: Duration = Duration::from_secs(10);

/// Connect a wtransport client to a WebTransport URL.
///
/// Uses `with_no_cert_validation()` for Kind's self-signed dev certs (the dev CA
/// cert is not committed to the repo; it is generated at Kind setup time by
/// `scripts/generate-dev-certs.sh`).
pub async fn connect_wt(url: &str) -> wtransport::Connection {
    let config = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .build();
    wtransport::Endpoint::client(config)
        .expect("create WebTransport client")
        .connect(url)
        .await
        .unwrap_or_else(|e| panic!("connect to WebTransport at {url} failed: {e}"))
}

/// 4-byte big-endian length prefix + `encoded` — the one framing used by both
/// the MC signalling stream and the MH connect envelope.
#[must_use]
pub fn frame(encoded: &[u8]) -> Vec<u8> {
    let mut f = BytesMut::with_capacity(4 + encoded.len());
    f.put_u32(u32::try_from(encoded.len()).expect("frame fits u32"));
    f.put_slice(encoded);
    f.to_vec()
}

/// One live MC signalling session: the WebTransport connection and its bidi
/// stream, kept open so post-join signalling (ADR-0036 §6 `ReceiveCapability`
/// in, §5 `SendDirective` + `StreamAssignments` out) can be driven over it.
///
/// Dropping it closes the session.
pub struct McSession {
    conn: wtransport::Connection,
    send: wtransport::SendStream,
    recv: wtransport::RecvStream,
}

impl McSession {
    /// Connect to an MC WebTransport URL and open the bidi signalling stream.
    /// No `JoinRequest` is sent yet — see [`mc_join`].
    pub async fn connect(url: &str) -> Self {
        let conn = connect_wt(url).await;
        let (send, recv) = conn
            .open_bi()
            .await
            .unwrap_or_else(|e| panic!("open bi stream to MC at {url} failed: {e}"))
            .await
            .unwrap_or_else(|e| panic!("MC bi stream to {url} not ready: {e}"));
        Self { conn, send, recv }
    }

    /// The underlying connection, e.g. to assert on `closed()`.
    #[must_use]
    pub fn connection(&self) -> &wtransport::Connection {
        &self.conn
    }

    /// Write a length-prefixed `ClientMessage`.
    pub async fn write(&mut self, message: &ClientMessage) {
        self.send
            .write_all(&frame(&message.encode_to_vec()))
            .await
            .expect("write ClientMessage to MC");
    }

    /// Read the next length-prefixed `ServerMessage`, or `None` if none arrives
    /// within `bound`. Callers that treat a timeout as fatal `.expect()` the
    /// result; callers draining until quiet use `None` as the quiet signal.
    ///
    /// `bound` covers BOTH halves of the frame — the length prefix and the body
    /// — because a peer that writes a length and then stalls would otherwise
    /// park this read with nothing to notice but Layer 7's whole-suite
    /// `timeout` (exit 124), which reports "the env-tests hung" with no phase,
    /// no test name and the rest of the suite unrun. A half-written frame is a
    /// DEFECT rather than quiet, so it panics here instead of returning `None`:
    /// `None` means "nothing arrived", which the drain-until-quiet callers read
    /// as their stop condition, and a partial frame read as quiet would turn a
    /// wire fault into a green settle.
    pub async fn try_read(&mut self, bound: Duration) -> Option<ServerMessage> {
        let mut len = [0u8; 4];
        tokio::time::timeout(bound, self.recv.read_exact(&mut len))
            .await
            .ok()?
            .expect("read MC frame length");
        let n = u32::from_be_bytes(len) as usize;
        assert!(
            n > 0 && n <= MAX_FRAME_BYTES,
            "MC frame length out of range: {n}"
        );
        let mut buf = vec![0u8; n];
        tokio::time::timeout(bound, self.recv.read_exact(&mut buf))
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "MC frame body ({n} bytes) did not arrive within {bound:?} after its length \
                     prefix — a half-written frame, not quiet"
                )
            })
            .expect("read MC frame body");
        Some(ServerMessage::decode(buf.as_slice()).expect("decode ServerMessage"))
    }
}

/// Build the MC `JoinRequest` — the ONE place its field list lives in env-tests.
///
/// `identity_public_key` is the shared synthetic Ed25519 fixture: MC checks
/// length only (ADR-0036 §4) and performs no attestation this story.
#[must_use]
pub fn join_request(meeting_id: &str, join_token: &str, participant_name: &str) -> JoinRequest {
    JoinRequest {
        meeting_id: meeting_id.to_string(),
        join_token: join_token.to_string(),
        participant_name: participant_name.to_string(),
        capabilities: None,
        correlation_id: String::new(),
        binding_token: String::new(),
        identity_public_key: sample_identity_public_key(),
    }
}

/// Wrap [`join_request`] in the `ClientMessage` envelope — what goes on the wire.
#[must_use]
pub fn join_message(meeting_id: &str, join_token: &str, participant_name: &str) -> ClientMessage {
    ClientMessage {
        message: Some(client_message::Message::JoinRequest(join_request(
            meeting_id,
            join_token,
            participant_name,
        ))),
        trace_parent: String::new(),
        trace_state: String::new(),
    }
}

/// Send a `JoinRequest` on `session` and await the `JoinResponse`. Driving a
/// real MC join is what makes MC fire `RegisterMeeting` to every assigned MH
/// (R-12), the precondition for MH-side coverage.
///
/// Panics loudly (naming `label`) on an `Error` message, a non-`JoinResponse`
/// variant, or no reply within the bound. Only the variant NAME is printed on
/// the unexpected-variant path, never the Debug payload, to avoid echoing
/// PII-bearing roster fields into CI logs.
///
/// # Positive control (shared-fixture failure mode)
///
/// This helper backs BOTH `26_mh_quic.rs` and `27_mc_slot_placement.rs`. If its
/// setup silently yielded a shape that "join did not really happen" — no
/// `sender_id`, no assigned handler, an empty handler url — both suites could go
/// green for one upstream reason, and two correlated greens read as stronger
/// evidence than one. So a wholesale-empty join is made structurally impossible
/// to mistake for a pass: it PANICS here, naming `label`, rather than being
/// returned as a usable-looking default.
pub async fn mc_join(
    session: &mut McSession,
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    label: &str,
) -> JoinResponse {
    session
        .write(&join_message(meeting_id, join_token, participant_name))
        .await;

    let join = match session
        .try_read(JOIN_RESPONSE_BOUND)
        .await
        .and_then(|m| m.message)
    {
        Some(server_message::Message::JoinResponse(j)) => j,
        Some(server_message::Message::Error(e)) => panic!(
            "MC refused {label}'s join: code={} message={}",
            e.code, e.message
        ),
        Some(_) => {
            panic!("expected a JoinResponse for {label}, got a different ServerMessage variant")
        }
        None => panic!("expected a JoinResponse for {label}, got no reply within the bound"),
    };

    assert!(
        join.sender_id.is_some(),
        "{label}: MC JoinResponse carried no sender_id — the join did not really happen \
         (positive control: a wholesale-empty setup must not read as a pass)"
    );
    assert!(
        !join.media_servers.is_empty(),
        "{label}: MC JoinResponse carried no media_servers — no handler was assigned"
    );
    for (idx, server) in join.media_servers.iter().enumerate() {
        assert!(
            !server.media_handler_url.is_empty(),
            "{label}: MC JoinResponse media_servers[{idx}] has an empty media_handler_url"
        );
    }
    join
}

/// Encode `jwt` as the typed `MhClientMessage{ConnectRequest{join_token}}`
/// envelope and frame it. MH's wire format on the first message of a bidi
/// stream is this envelope, mirroring MC's `ClientMessage{JoinRequest{…}}`.
/// Negative tests pass malformed/oversized `jwt` strings through unchanged so
/// the validator observes the intended failure mode.
#[must_use]
pub fn mh_connect_frame(jwt: &str) -> Vec<u8> {
    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: jwt.to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    frame(&envelope.encode_to_vec())
}

/// Open a bidi stream on `conn`, write the MH connect frame, and return the live
/// streams so the caller controls read timing and disconnect.
pub async fn mh_open_connect(
    conn: &wtransport::Connection,
    jwt: &str,
) -> (wtransport::SendStream, wtransport::RecvStream) {
    let (mut send, recv) = conn
        .open_bi()
        .await
        .expect("open bi stream to MH")
        .await
        .expect("MH bi stream ready");
    send.write_all(&mh_connect_frame(jwt))
        .await
        .expect("write MH connect request");
    (send, recv)
}

/// One live MH media session: the connection AND the connect (carrier) stream.
///
/// # The carrier stream is the session's liveness signal — it must be HELD
///
/// MH watches the client's send half of the connect stream for the lifetime of
/// the media session: a FIN (`read` → `Ok(None)`) is "client disconnected", a
/// reset is "disconnected with error", and either one tears the media session
/// down — ingress, egress and the sender binding
/// (`crates/mh-service/src/webtransport/connection.rs`, the hold-open loop after
/// the sender binding). Dropping a `wtransport::SendStream` FINISHES it, so a
/// helper that returned only the connection and let the streams fall out of
/// scope would end every media session the moment it was bound, and no datagram
/// would ever be forwarded. The streams are therefore owned here and live
/// exactly as long as this value; drop it to disconnect.
pub struct MhSession {
    conn: wtransport::Connection,
    // Held, never read: their only job is to keep the carrier stream open.
    _send: wtransport::SendStream,
    _recv: wtransport::RecvStream,
}

impl MhSession {
    /// The media connection, for `send_datagram` / `receive_datagram`.
    #[must_use]
    pub fn connection(&self) -> &wtransport::Connection {
        &self.conn
    }
}

/// Connect to an MH URL, send the connect request, and return the live media
/// session — connection plus the held carrier stream (see [`MhSession`]).
pub async fn mh_connect(url: &str, jwt: &str) -> MhSession {
    let conn = connect_wt(url).await;
    let (send, recv) = mh_open_connect(&conn, jwt).await;
    MhSession {
        conn,
        _send: send,
        _recv: recv,
    }
}
