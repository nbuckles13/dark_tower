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
    client_message, mh_client_message, server_message, ClientMessage, ErrorMessage, JoinRequest,
    JoinResponse, MediaKind, MhClientMessage, MhConnectRequest, MuteChange,
    ParticipantCapabilities, ParticipantMuteUpdate, PublishedKind, ReceiveCapability, ReceiveSlot,
    ServerMessage,
};
use std::time::Duration;

use super::media::sample_identity_public_key;

/// Upper bound on a single framed MC/MH message, in bytes (MC's and MH's
/// per-message size bound). Frames larger than this are a wire defect, not a
/// payload, and are refused BEFORE the body is allocated.
pub const MAX_FRAME_BYTES: usize = 65_536;

/// How long [`mc_join`] waits for the `JoinResponse` before giving up.
const JOIN_RESPONSE_BOUND: Duration = Duration::from_secs(10);

/// Connect a wtransport client to a WebTransport URL.
///
/// Uses `with_no_cert_validation()` for Kind's self-signed dev certs (the dev CA
/// cert is not committed to the repo; it is generated at Kind setup time by
/// `scripts/generate-dev-certs.sh`).
pub async fn connect_wt(url: &str) -> wtransport::Connection {
    try_connect_wt(url).await.unwrap_or_else(|e| panic!("{e}"))
}

/// [`connect_wt`], returning the failure instead of panicking, so a caller can
/// classify an unreachable endpoint (e.g. as an ENVIRONMENT failure) rather
/// than letting it read as a product defect.
///
/// # Errors
///
/// A description of the client-construction or connect failure.
pub async fn try_connect_wt(url: &str) -> Result<wtransport::Connection, String> {
    try_connect_wt_with(url, None).await
}

/// [`try_connect_wt`] with a QUIC keep-alive: while the peer is up, the
/// session cannot end by IDLE TIMEOUT. That is what lets a test read a close
/// on such a session as the PEER's doing (e.g. MH releasing a meeting) rather
/// than as a quiet link timing out.
///
/// # Errors
///
/// As [`try_connect_wt`].
pub async fn try_connect_wt_with(
    url: &str,
    keep_alive: Option<Duration>,
) -> Result<wtransport::Connection, String> {
    let config = wtransport::ClientConfig::builder()
        .with_bind_default()
        .with_no_cert_validation()
        .keep_alive_interval(keep_alive)
        .build();
    wtransport::Endpoint::client(config)
        .map_err(|e| format!("create WebTransport client: {e}"))?
        .connect(url)
        .await
        .map_err(|e| format!("connect to WebTransport at {url} failed: {e}"))
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

/// The body length a 4-byte big-endian prefix declares, held to
/// `1..=`[`MAX_FRAME_BYTES`] BEFORE anything is allocated for the body — the
/// ONE bound rule for every carrier/signalling reader in env-tests
/// ([`read_framed`] and the cancel-safe carrier reader in `fixtures::video`).
///
/// # Panics
///
/// On a length outside the bound; the text names `what` only.
pub(crate) fn checked_frame_len(prefix: [u8; 4], what: &str) -> usize {
    let n = usize::try_from(u32::from_be_bytes(prefix)).unwrap_or(usize::MAX);
    assert!(
        n > 0 && n <= MAX_FRAME_BYTES,
        "{what}: length prefix out of range (must be 1..={MAX_FRAME_BYTES})"
    );
    n
}

/// Read the next 4-byte-length-prefixed message body from `recv`, or `None` if
/// no length prefix arrives within `bound` — THE bounded framed read for both
/// MC's signalling stream and MH's carrier stream.
///
/// `bound` covers BOTH halves of the frame (see [`McSession::try_read`]). A
/// length outside `1..=`[`MAX_FRAME_BYTES`] is refused before the body is
/// allocated, and a half-written frame panics rather than reading as quiet.
/// Panic text names `what` and the reason only — never body content.
///
/// # Panics
///
/// On a read error, an out-of-range length, or a body that does not follow
/// its length prefix within `bound`.
pub async fn read_framed(
    recv: &mut wtransport::RecvStream,
    bound: Duration,
    what: &str,
) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    tokio::time::timeout(bound, recv.read_exact(&mut len))
        .await
        .ok()?
        .unwrap_or_else(|e| panic!("{what}: read length prefix failed: {e}"));
    let n = checked_frame_len(len, what);
    let mut buf = vec![0u8; n];
    tokio::time::timeout(bound, recv.read_exact(&mut buf))
        .await
        .unwrap_or_else(|_| {
            panic!(
                "{what}: body did not arrive within {bound:?} after its length prefix — a \
                 half-written frame, not quiet"
            )
        })
        .unwrap_or_else(|e| panic!("{what}: read body failed: {e}"));
    Some(buf)
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
        let buf = read_framed(&mut self.recv, bound, "MC frame").await?;
        Some(ServerMessage::decode(buf.as_slice()).expect("decode ServerMessage"))
    }
}

/// The frame header version this suite declares at join: DERIVED from the
/// wire-format crate (`media_protocol::frame::PROTOCOL_VERSION`), never a
/// literal, so a version bump moves every env-test join with it. MC refuses a
/// join whose `supported_header_versions` lacks it (story 3 R-22).
#[must_use]
pub fn current_header_version() -> u32 {
    u32::from(media_protocol::frame::PROTOCOL_VERSION)
}

/// One `ReceiveCapability` from per-kind slot ids: audio slots first, then
/// video, in the order given (the client's declaration order — MC truncates
/// each kind to its cap keeping the FIRST ids of that kind). The ONE builder
/// for both the join-carried declaration and the post-join redeclaration
/// (`participant::Participant::declare`).
#[must_use]
pub fn receive_capability(audio_slot_ids: &[u32], video_slot_ids: &[u32]) -> ReceiveCapability {
    let slot = |kind: MediaKind| {
        move |id: &u32| ReceiveSlot {
            slot_id: *id,
            media_kind: kind as i32,
            pinned_sender_id: None,
        }
    };
    ReceiveCapability {
        slots: audio_slot_ids
            .iter()
            .map(slot(MediaKind::Audio))
            .chain(video_slot_ids.iter().map(slot(MediaKind::VideoCamera)))
            .collect(),
    }
}

/// What a join DECLARES (story 3 R-2, R-22): the publish set, the initial
/// receive declaration and the frame header versions.
///
/// [`JoinDeclaration::default`] is what every suite that does not care sends:
/// publishes AUDIO, declares no receive slots (absent = zero of each kind, which
/// MC admits because the participant publishes) and supports
/// [`current_header_version`]. That is the story-2 audio participant expressed
/// on the story-3 wire. The negative cases set raw values through the builder
/// methods so MC observes exactly the malformed declaration under test.
#[derive(Clone, Debug)]
pub struct JoinDeclaration {
    /// The published kinds as raw `(media_kind, client_muted)` entries: raw so
    /// an invalid, unknown or duplicate value can be sent unchanged. Empty =
    /// receive-only.
    pub publish: Vec<(i32, bool)>,
    pub receive_capability: Option<ReceiveCapability>,
    pub header_versions: Vec<u32>,
}

impl Default for JoinDeclaration {
    fn default() -> Self {
        Self {
            publish: vec![(MediaKind::Audio as i32, false)],
            receive_capability: None,
            header_versions: vec![current_header_version()],
        }
    }
}

impl JoinDeclaration {
    /// Replace the publish set, each kind live (empty = receive-only).
    #[must_use]
    pub fn publishing(mut self, kinds: &[MediaKind]) -> Self {
        self.publish = kinds.iter().map(|k| (*k as i32, false)).collect();
        self
    }

    /// Replace the publish set with raw kind values, each live (negative
    /// cases).
    #[must_use]
    pub fn publishing_raw(mut self, kinds: &[i32]) -> Self {
        self.publish = kinds.iter().map(|k| (*k, false)).collect();
        self
    }

    /// Mark the already-published `kind` client-muted at join.
    #[must_use]
    pub fn client_muted(mut self, kind: MediaKind) -> Self {
        for (k, muted) in &mut self.publish {
            if *k == kind as i32 {
                *muted = true;
            }
        }
        self
    }

    /// Carry an initial receive declaration on the join (see
    /// [`receive_capability`] for ordering).
    #[must_use]
    pub fn receiving(mut self, audio_slot_ids: &[u32], video_slot_ids: &[u32]) -> Self {
        self.receive_capability = Some(receive_capability(audio_slot_ids, video_slot_ids));
        self
    }

    /// Replace the declared header versions (negative cases).
    #[must_use]
    pub fn header_versions(mut self, versions: &[u32]) -> Self {
        self.header_versions = versions.to_vec();
        self
    }
}

/// `kind`'s advertised receive-slot cap on a `JoinResponse`, if MC sent
/// EXACTLY one entry for it (CONVENTIONS §6: `receive_slot_caps` carries one
/// entry per known kind). `None` = no entry; a duplicate panics.
#[must_use]
pub fn slot_cap(response: &JoinResponse, kind: MediaKind) -> Option<u32> {
    let mut entries = response
        .receive_slot_caps
        .iter()
        .filter(|c| c.media_kind == kind as i32);
    let cap = entries.next().map(|c| c.max_slots);
    assert!(
        entries.next().is_none(),
        "two receive_slot_caps entries for {kind:?}"
    );
    cap
}

/// Is `kind` server-muted on a mute update? Panics unless the snapshot
/// carries EXACTLY one entry for the kind (CONVENTIONS §6).
#[must_use]
pub fn server_muted(update: &ParticipantMuteUpdate, kind: MediaKind) -> bool {
    let mut entries = update
        .media_states
        .iter()
        .filter(|s| s.media_kind == kind as i32);
    let entry = entries
        .next()
        .unwrap_or_else(|| panic!("no media_states entry for {kind:?}"));
    assert!(
        entries.next().is_none(),
        "two media_states entries for {kind:?}"
    );
    entry.server_muted
}

/// The kind-keyed `changes` of a mute request from per-kind asks (`None` = no
/// entry: that kind unchanged).
#[must_use]
pub fn mute_changes(audio: Option<bool>, video: Option<bool>) -> Vec<MuteChange> {
    [(MediaKind::Audio, audio), (MediaKind::VideoCamera, video)]
        .into_iter()
        .filter_map(|(kind, muted)| {
            muted.map(|muted| MuteChange {
                media_kind: kind as i32,
                muted,
            })
        })
        .collect()
}

/// Build the MC `JoinRequest` — the ONE place its field list lives in env-tests.
///
/// `identity_public_key` is the shared synthetic Ed25519 fixture: MC checks
/// length only (ADR-0036 §4) and performs no attestation this story.
#[must_use]
pub fn join_request_with(
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    declaration: &JoinDeclaration,
) -> JoinRequest {
    JoinRequest {
        meeting_id: meeting_id.to_string(),
        join_token: join_token.to_string(),
        participant_name: participant_name.to_string(),
        capabilities: Some(ParticipantCapabilities {
            supported_codecs: Vec::new(),
            supported_header_versions: declaration.header_versions.clone(),
        }),
        correlation_id: String::new(),
        binding_token: String::new(),
        identity_public_key: sample_identity_public_key(),
        receive_capability: declaration.receive_capability.clone(),
        published_kinds: declaration
            .publish
            .iter()
            .map(|&(media_kind, client_muted)| PublishedKind {
                media_kind,
                client_muted,
            })
            .collect(),
    }
}

/// [`join_request_with`] the [`JoinDeclaration::default`] declaration.
#[must_use]
pub fn join_request(meeting_id: &str, join_token: &str, participant_name: &str) -> JoinRequest {
    join_request_with(
        meeting_id,
        join_token,
        participant_name,
        &JoinDeclaration::default(),
    )
}

/// Wrap a `JoinRequest` in the `ClientMessage` envelope — what goes on the wire.
#[must_use]
pub fn join_message_with(
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    declaration: &JoinDeclaration,
) -> ClientMessage {
    ClientMessage {
        message: Some(client_message::Message::JoinRequest(join_request_with(
            meeting_id,
            join_token,
            participant_name,
            declaration,
        ))),
        trace_parent: String::new(),
        trace_state: String::new(),
    }
}

/// [`join_message_with`] the default declaration.
#[must_use]
pub fn join_message(meeting_id: &str, join_token: &str, participant_name: &str) -> ClientMessage {
    join_message_with(
        meeting_id,
        join_token,
        participant_name,
        &JoinDeclaration::default(),
    )
}

/// Send a `JoinRequest` carrying `declaration` and read MC's ONE reply: the
/// `JoinResponse`, or the `Error` MC refused with (`Err`). The refusal path is
/// for the negative cases; MC's refusal text is generic, so a caller attributes
/// a refusal only through a positive twin that differs in one field.
///
/// # Panics
///
/// On no reply within the bound or a reply that is neither variant (only the
/// variant NAME is printed, never the Debug payload — roster fields carry PII).
pub async fn try_mc_join(
    session: &mut McSession,
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    declaration: &JoinDeclaration,
    label: &str,
) -> Result<JoinResponse, ErrorMessage> {
    session
        .write(&join_message_with(
            meeting_id,
            join_token,
            participant_name,
            declaration,
        ))
        .await;
    match session
        .try_read(JOIN_RESPONSE_BOUND)
        .await
        .and_then(|m| m.message)
    {
        Some(server_message::Message::JoinResponse(j)) => Ok(j),
        Some(server_message::Message::Error(e)) => Err(e),
        Some(_) => {
            panic!("expected a JoinResponse or Error for {label}, got a different variant")
        }
        None => panic!("expected a JoinResponse for {label}, got no reply within the bound"),
    }
}

/// Send the default-declaration `JoinRequest` on `session` and await the
/// `JoinResponse`. See [`mc_join_with`].
pub async fn mc_join(
    session: &mut McSession,
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    label: &str,
) -> JoinResponse {
    mc_join_with(
        session,
        meeting_id,
        join_token,
        participant_name,
        &JoinDeclaration::default(),
        label,
    )
    .await
}

/// Send a `JoinRequest` carrying `declaration` on `session` and await the
/// `JoinResponse`. Driving a real MC join is what makes MC fire
/// `RegisterMeeting` to every assigned MH (R-12), the precondition for MH-side
/// coverage.
///
/// Panics loudly (naming `label`) on an `Error` message, a non-`JoinResponse`
/// variant, or no reply within the bound.
///
/// # Positive control (shared-fixture failure mode)
///
/// This helper backs every multi-party suite. If its setup silently yielded a
/// shape that "join did not really happen" — no `sender_id`, no assigned
/// handler, an empty handler url — those suites could go green for one
/// upstream reason, and correlated greens read as stronger evidence than one.
/// So a wholesale-empty join is made structurally impossible to mistake for a
/// pass: it PANICS here, naming `label`, rather than being returned as a
/// usable-looking default.
pub async fn mc_join_with(
    session: &mut McSession,
    meeting_id: &str,
    join_token: &str,
    participant_name: &str,
    declaration: &JoinDeclaration,
    label: &str,
) -> JoinResponse {
    let join = try_mc_join(
        session,
        meeting_id,
        join_token,
        participant_name,
        declaration,
        label,
    )
    .await
    .unwrap_or_else(|e| {
        panic!(
            "MC refused {label}'s join: code={} message={}",
            e.code, e.message
        )
    });

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
    try_mh_open_connect(conn, jwt)
        .await
        .unwrap_or_else(|e| panic!("{e}"))
}

/// [`mh_open_connect`], returning a stream-open or connect-frame write failure
/// instead of panicking. The one encoding of "open MH's carrier bi-stream and
/// write the connect frame".
///
/// # Errors
///
/// A description of the stream-open or connect-frame write failure.
pub async fn try_mh_open_connect(
    conn: &wtransport::Connection,
    jwt: &str,
) -> Result<(wtransport::SendStream, wtransport::RecvStream), String> {
    let (mut send, recv) = conn
        .open_bi()
        .await
        .map_err(|e| format!("open bi stream to MH: {e}"))?
        .await
        .map_err(|e| format!("MH bi stream not ready: {e}"))?;
    send.write_all(&mh_connect_frame(jwt))
        .await
        .map_err(|e| format!("write MH connect request: {e}"))?;
    Ok((send, recv))
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
    // Held for the session's lifetime: their first job is to keep the carrier
    // stream open. Lent out (never moved) by [`MhSession::control`] for the
    // story-3 control messages.
    send: wtransport::SendStream,
    recv: wtransport::RecvStream,
}

impl MhSession {
    /// The media connection, for `send_datagram` / `receive_datagram`.
    #[must_use]
    pub fn connection(&self) -> &wtransport::Connection {
        &self.conn
    }

    /// The carrier stream's halves, BORROWED: the send half carries
    /// `MhClientMessage.recovery_request`, the receive half
    /// `MhServerMessage.keyframe_request` (story 3; both length-framed exactly as
    /// the connect request). Borrow-only on purpose — the session must keep
    /// owning both halves, because finishing or dropping the send half is MH's
    /// "client disconnected" and tears the media session down. Never call
    /// `finish()` or `reset()` on the lent send half.
    pub fn control(&mut self) -> (&mut wtransport::SendStream, &mut wtransport::RecvStream) {
        (&mut self.send, &mut self.recv)
    }
}

/// Connect to an MH URL, send the connect request, and return the live media
/// session — connection plus the held carrier stream (see [`MhSession`]).
pub async fn mh_connect(url: &str, jwt: &str) -> MhSession {
    try_mh_connect(url, jwt)
        .await
        .unwrap_or_else(|e| panic!("{e}"))
}

/// [`mh_connect`], returning a connect or stream-open failure instead of
/// panicking — for suites where an unreachable handler must be reported as an
/// ENVIRONMENT failure, never absorbed into the scenario under test.
///
/// # Errors
///
/// A description of the connect, stream-open or connect-frame write failure.
pub async fn try_mh_connect(url: &str, jwt: &str) -> Result<MhSession, String> {
    try_mh_connect_with(url, jwt, None).await
}

/// [`try_mh_connect`] with a QUIC keep-alive (see [`try_connect_wt_with`]).
///
/// # Errors
///
/// As [`try_mh_connect`].
pub async fn try_mh_connect_with(
    url: &str,
    jwt: &str,
    keep_alive: Option<Duration>,
) -> Result<MhSession, String> {
    let conn = try_connect_wt_with(url, keep_alive).await?;
    let (send, recv) = try_mh_open_connect(&conn, jwt)
        .await
        .map_err(|e| format!("{e} (MH at {url})"))?;
    Ok(MhSession { conn, send, recv })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_join_is_an_audio_publisher_on_the_current_header_version() {
        let req = join_request("m", "t", "n");
        assert_eq!(
            req.published_kinds,
            vec![PublishedKind {
                media_kind: MediaKind::Audio as i32,
                client_muted: false,
            }]
        );
        assert!(
            req.receive_capability.is_none(),
            "absent = zero slots of each kind"
        );
        let caps = req.capabilities.expect("capabilities carried");
        assert_eq!(
            caps.supported_header_versions,
            vec![u32::from(media_protocol::frame::PROTOCOL_VERSION)]
        );
        // Story 3 is v3-only; a silent change of the derivation source shows here.
        assert_eq!(current_header_version(), 3);
    }

    #[test]
    fn receive_capability_lists_audio_then_video_in_the_given_order() {
        let cap = receive_capability(&[7, 300], &[513, 2, 9]);
        let got: Vec<(u32, i32)> = cap
            .slots
            .iter()
            .map(|s| (s.slot_id, s.media_kind))
            .collect();
        let (a, v) = (MediaKind::Audio as i32, MediaKind::VideoCamera as i32);
        assert_eq!(got, vec![(7, a), (300, a), (513, v), (2, v), (9, v)]);
        assert!(cap.slots.iter().all(|s| s.pinned_sender_id.is_none()));
    }

    #[test]
    fn the_builder_carries_raw_negative_values_unchanged() {
        let d = JoinDeclaration::default()
            .publishing_raw(&[MediaKind::Audio as i32, MediaKind::Audio as i32, 99])
            .receiving(&[], &[4])
            .header_versions(&[2]);
        let req = join_request_with("m", "t", "n", &d);
        let kinds: Vec<i32> = req.published_kinds.iter().map(|k| k.media_kind).collect();
        assert_eq!(
            kinds,
            vec![MediaKind::Audio as i32, MediaKind::Audio as i32, 99]
        );
        assert_eq!(req.capabilities.unwrap().supported_header_versions, vec![2]);
        assert_eq!(req.receive_capability.unwrap().slots.len(), 1);
        let none = join_request_with("m", "t", "n", &JoinDeclaration::default().publishing(&[]));
        assert!(
            none.published_kinds.is_empty(),
            "receive-only is an EMPTY list"
        );
    }
}
