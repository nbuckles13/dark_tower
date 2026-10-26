//! A live post-join WebTransport session for media-signalling tests (ADR-0036
//! §5, §6), shared by every test file that drives more than one participant.
//!
//! Since story 2 a participant's `SendDirective` and `StreamAssignments` are
//! pushed by the meeting actor whenever meeting state changes what they would
//! say — including on ANOTHER participant's join, leave, declaration or mute —
//! and roster broadcasts (`ParticipantJoined`/`Left`) interleave with them on
//! the same stream. So every read here skips roster traffic and returns only
//! media-signalling messages, and "nothing arrived" is bounded by a read
//! timeout rather than a sleep.
//!
//! # Connectivity is reported, exactly as MH reports it
//!
//! Since story 2 task 20 a participant is routable only once the HANDLERS
//! report its connections (`NotifyParticipantConnected`, ADR-0036 §9). A real
//! client connects to every handler it is offered, so [`join_as`] joins and then
//! reports a connection to EVERY handler of the meeting's registered set,
//! through the real `McMediaCoordinationService` (validation, resolution,
//! metrics and all) — never by reaching into the actor. [`join_as_on`] reports
//! a chosen subset, which is how partial connectivity is produced: the same way
//! an env-test client produces it, by which handlers it actually reaches. Once
//! a participant has reported every handler it settles at once (no window).

use std::sync::Arc;
use std::time::Duration;

use mc_service::grpc::{McMediaCoordinationService, MhRegistrationClient};
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantDisconnectedRequest,
};
use proto_gen::dark_tower::signaling::v1::{
    client_message, server_message, ClientMessage, JoinRequest, MediaKind, MuteRequest,
    ReceiveCapability, ReceiveSlot, SendDirective, ServerMessage, StreamAssignments,
};

use super::accept_loop_rig::AcceptLoopRig;
use super::{
    build_test_stack, client_media_config, connect, encode_framed, read_server_message,
    sample_identity_public_key, TestStackHandles,
};

/// How long a read waits before concluding "nothing more is coming".
///
/// A bound on a negative, not a performance assertion: nothing in the suite
/// asserts a wall-clock threshold.
const QUIET: Duration = Duration::from_secs(2);

/// Start the component stack with the suite's client media configuration.
pub async fn start_stack(label: &str) -> (TestStackHandles, AcceptLoopRig) {
    start_stack_with(label, client_media_config()).await
}

/// Start the component stack with an explicit client media configuration.
pub async fn start_stack_with(
    label: &str,
    media_config: mc_service::media_signaling::ClientMediaConfig,
) -> (TestStackHandles, AcceptLoopRig) {
    let stack = build_test_stack(label).await;
    let rig = AcceptLoopRig::start_with_media_config(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::clone(&stack.mh_reg_client) as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        32,
        media_config,
    )
    .await;
    (stack, rig)
}

/// One receive slot.
pub fn slot(slot_id: u32, kind: MediaKind) -> ReceiveSlot {
    ReceiveSlot {
        slot_id,
        media_kind: kind as i32,
        pinned_sender_id: None,
    }
}

/// `n` audio slots numbered `0..n`.
pub fn audio_slots(n: u32) -> Vec<ReceiveSlot> {
    (0..n).map(|i| slot(i, MediaKind::Audio)).collect()
}

/// A framed `ReceiveCapability`.
pub fn capability_frame(slots: Vec<ReceiveSlot>) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::ReceiveCapability(
            ReceiveCapability { slots },
        )),
    })
}

/// A framed `MuteRequest` naming BOTH kinds explicitly.
pub fn mute_frame(audio_muted: bool, video_muted: bool) -> bytes::BytesMut {
    mute_kinds_frame(Some(audio_muted), Some(video_muted))
}

/// A framed `MuteRequest` with each kind's presence as given (`None` = no
/// entry, i.e. "that kind unchanged"; both `None` is the rejected empty list).
pub fn mute_kinds_frame(audio_muted: Option<bool>, video_muted: Option<bool>) -> bytes::BytesMut {
    mute_changes_frame(&kind_changes(audio_muted, video_muted))
}

/// The kind-keyed `changes` list for per-kind asks (`None` = no entry).
pub fn kind_changes(audio: Option<bool>, video: Option<bool>) -> Vec<(i32, bool)> {
    mc_service::media_kind::MediaKindLabel::ALL
        .into_iter()
        .zip([audio, video])
        .filter_map(|(kind, muted)| muted.map(|m| (kind.to_proto() as i32, m)))
        .collect()
}

fn mute_changes(changes: &[(i32, bool)]) -> Vec<proto_gen::dark_tower::signaling::v1::MuteChange> {
    changes
        .iter()
        .map(
            |&(media_kind, muted)| proto_gen::dark_tower::signaling::v1::MuteChange {
                media_kind,
                muted,
            },
        )
        .collect()
}

/// A framed `MuteRequest` carrying exactly `changes` as raw `(media_kind,
/// muted)` entries -- malformed lists included (UNSPECIFIED, unknown numbers,
/// duplicates, screen).
pub fn mute_changes_frame(changes: &[(i32, bool)]) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            changes: mute_changes(changes),
        })),
    })
}

/// A media-signalling message, with roster traffic filtered out.
#[derive(Debug)]
pub enum Media {
    /// A `SendDirective`.
    Directive(SendDirective),
    /// A `StreamAssignments`.
    Assignments(StreamAssignments),
    /// An `ErrorMessage`'s text.
    Error(String),
}

/// A live post-join session: the streams stay open so the bridge loop runs.
pub struct Session {
    conn: wtransport::Connection,
    send: wtransport::stream::SendStream,
    recv: wtransport::stream::RecvStream,
    /// This participant's own sender id, read off the `JoinResponse`.
    pub sender_id: u32,
    /// `JoinResponse.media_servers`, as MC sent it.
    pub media_servers: Vec<String>,
    /// `JoinResponse.receive_slot_caps`, as MC sent it.
    pub receive_slot_caps: Vec<proto_gen::dark_tower::signaling::v1::ReceiveSlotCap>,
    /// The `SendDirective` MC emitted in the join turn.
    pub join_directive: Option<SendDirective>,
    /// The `StreamAssignments` MC emitted in the join turn.
    pub join_assignments: Option<StreamAssignments>,
    /// Non-media messages read while taking the join view, replayed first.
    pending: std::collections::VecDeque<ServerMessage>,
    /// The token `sub` this session joined as (what MH reports).
    pub user: String,
    /// MC's participant id for this session, read off the `JoinResponse`.
    pub participant_id: String,
    /// `JoinResponse.existing_participants`, as MC sent it (the roster
    /// snapshot, client-mute facts included).
    pub existing_participants: Vec<proto_gen::dark_tower::signaling::v1::Participant>,
    /// The meeting joined.
    pub meeting_id: String,
}

impl Session {
    /// `kind`'s advertised receive-slot cap: `receive_slot_caps` must carry
    /// EXACTLY one entry for it (CONVENTIONS §6).
    #[must_use]
    pub fn slot_cap(&self, kind: MediaKind) -> u32 {
        let mut entries = self
            .receive_slot_caps
            .iter()
            .filter(|c| c.media_kind == kind as i32);
        let cap = entries
            .next()
            .expect("an entry for every known kind")
            .max_slots;
        assert!(entries.next().is_none(), "exactly one entry per kind");
        cap
    }
}

impl Session {
    /// Report, as handler `handler_id` would, that this participant opened
    /// media connection `connection_id` to it. Returns the `sender_id` MC
    /// answered (0 = declined).
    pub async fn connect_to(
        &self,
        stack: &TestStackHandles,
        handler_id: &str,
        connection_id: &str,
    ) -> u32 {
        notify_connected(
            stack,
            &self.meeting_id,
            &self.user,
            handler_id,
            connection_id,
        )
        .await
    }

    /// Report, as handler `handler_id` would, that media connection
    /// `connection_id` closed.
    pub async fn disconnect_from(
        &self,
        stack: &TestStackHandles,
        handler_id: &str,
        connection_id: &str,
    ) {
        notify_disconnected(
            stack,
            &self.meeting_id,
            &self.user,
            handler_id,
            connection_id,
        )
        .await;
    }

    /// Close the WebTransport session cleanly (a tab close): MC classifies it
    /// `ClientClosed` and removes the participant immediately, skipping grace.
    pub fn close(self) {
        self.conn.close(wtransport::VarInt::from_u32(0), b"leave");
    }

    /// Write a framed client message.
    pub async fn write(&mut self, frame: bytes::BytesMut) {
        self.send.write_all(&frame).await.expect("write frame");
    }

    async fn next_raw(&mut self) -> Option<ServerMessage> {
        if let Some(m) = self.pending.pop_front() {
            return Some(m);
        }
        tokio::time::timeout(QUIET, read_server_message(&mut self.recv))
            .await
            .ok()
    }

    /// Consume the joiner's JOIN-TURN view (story 3: every join carries its
    /// declaration, so MC emits the joiner's `SendDirective` then
    /// `StreamAssignments` in the join turn). Non-media messages read on the
    /// way (the server-mute replay, roster broadcasts) are kept, in order, for
    /// the test to read next — so a session starts from the same clean slate
    /// the story-2 suite assumed, with the join view recorded.
    async fn take_join_view(&mut self) {
        let mut kept = std::collections::VecDeque::new();
        loop {
            let Ok(m) = tokio::time::timeout(QUIET, read_server_message(&mut self.recv)).await
            else {
                panic!("{}: the join-turn view never arrived", self.user);
            };
            match &m.message {
                Some(server_message::Message::SendDirective(d)) => {
                    self.join_directive = Some(d.clone());
                }
                Some(server_message::Message::StreamAssignments(a)) => {
                    self.join_assignments = Some(a.clone());
                    break;
                }
                _ => kept.push_back(m),
            }
        }
        self.pending = kept;
    }

    /// The next server message of ANY kind (roster traffic included), or
    /// `None` if none arrives within the quiet bound.
    pub async fn next_server_message(&mut self) -> Option<ServerMessage> {
        self.next_raw().await
    }

    /// Read until a `ParticipantMuteUpdate` about `participant_id` arrives.
    ///
    /// # Panics
    ///
    /// Panics if none arrives within the quiet bound.
    pub async fn expect_mute_update_for(
        &mut self,
        participant_id: &str,
    ) -> proto_gen::dark_tower::signaling::v1::ParticipantMuteUpdate {
        while let Some(msg) = self.next_raw().await {
            if let Some(server_message::Message::ParticipantMuteUpdate(u)) = msg.message {
                if u.participant_id == participant_id {
                    return u;
                }
            }
        }
        panic!("no ParticipantMuteUpdate for {participant_id} arrived");
    }

    /// The next media-signalling message, skipping roster traffic, or `None`
    /// if none arrives within the quiet bound.
    pub async fn next_media(&mut self) -> Option<Media> {
        loop {
            match self.next_raw().await?.message {
                Some(server_message::Message::SendDirective(d)) => {
                    return Some(Media::Directive(d))
                }
                Some(server_message::Message::StreamAssignments(a)) => {
                    return Some(Media::Assignments(a))
                }
                Some(server_message::Message::Error(e)) => return Some(Media::Error(e.message)),
                // Roster broadcasts and anything else: not what these tests read.
                _ => {}
            }
        }
    }

    /// The next media message must be a `SendDirective`.
    pub async fn expect_directive(&mut self) -> SendDirective {
        match self.next_media().await {
            Some(Media::Directive(d)) => d,
            other => panic!("expected SendDirective, got {other:?}"),
        }
    }

    /// The next media message must be a `StreamAssignments`.
    pub async fn expect_assignments(&mut self) -> StreamAssignments {
        match self.next_media().await {
            Some(Media::Assignments(a)) => a,
            other => panic!("expected StreamAssignments, got {other:?}"),
        }
    }

    /// The next media message must be an error.
    pub async fn expect_error(&mut self) -> String {
        match self.next_media().await {
            Some(Media::Error(e)) => e,
            other => panic!("expected ErrorMessage, got {other:?}"),
        }
    }

    /// No media-signalling message arrives within the quiet bound.
    pub async fn expect_no_media(&mut self) {
        if let Some(m) = self.next_media().await {
            panic!("expected no media signalling, got {m:?}");
        }
    }

    /// Drain media messages until quiet; return the LAST directive and the LAST
    /// assignments seen (full-replace semantics: the latest is the view).
    pub async fn settle(&mut self) -> (Option<SendDirective>, Option<StreamAssignments>) {
        let mut directive = None;
        let mut assignments = None;
        while let Some(m) = self.next_media().await {
            match m {
                Media::Directive(d) => directive = Some(d),
                Media::Assignments(a) => assignments = Some(a),
                Media::Error(e) => panic!("unexpected error while settling: {e}"),
            }
        }
        (directive, assignments)
    }

    /// Read media messages until `predicate` holds for a `StreamAssignments`,
    /// failing distinctly on "none arrived" versus "wrong content".
    pub async fn assignments_until(
        &mut self,
        what: &str,
        predicate: impl Fn(&StreamAssignments) -> bool,
    ) -> StreamAssignments {
        let pick = |m: Media| match m {
            Media::Assignments(a) => Some(a),
            _ => None,
        };
        self.until(what, "StreamAssignments", pick, predicate).await
    }

    /// Read media messages until `predicate` holds for a `SendDirective`.
    pub async fn directive_until(
        &mut self,
        what: &str,
        predicate: impl Fn(&SendDirective) -> bool,
    ) -> SendDirective {
        let pick = |m: Media| match m {
            Media::Directive(d) => Some(d),
            _ => None,
        };
        self.until(what, "SendDirective", pick, predicate).await
    }

    /// The one wait loop behind [`Self::assignments_until`] and
    /// [`Self::directive_until`]: read media messages, keep those `pick`
    /// extracts, return the first that matches `predicate`.
    async fn until<T: std::fmt::Debug>(
        &mut self,
        what: &str,
        kind: &str,
        pick: impl Fn(Media) -> Option<T>,
        predicate: impl Fn(&T) -> bool,
    ) -> T {
        let mut last = None;
        while let Some(m) = self.next_media().await {
            if let Some(x) = pick(m) {
                if predicate(&x) {
                    return x;
                }
                last = Some(x);
            }
        }
        match last {
            None => panic!("{what}: no {kind} arrived at all"),
            Some(x) => panic!("{what}: {kind} arrived but never matched; last: {x:?}"),
        }
    }
}

/// The media connection id this harness reports for `user` on `handler_id`.
#[must_use]
pub fn connection_id_for(user: &str, handler_id: &str) -> String {
    format!("{user}@{handler_id}")
}

/// A `McMediaCoordinationService` over the stack's controller — the real MH→MC
/// ingestion path.
fn coordination(stack: &TestStackHandles) -> McMediaCoordinationService {
    McMediaCoordinationService::new(Arc::clone(&stack.controller_handle))
}

/// Send one `NotifyParticipantConnected` exactly as MH would. Returns the
/// answered `sender_id`.
pub async fn notify_connected(
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    handler_id: &str,
    connection_id: &str,
) -> u32 {
    coordination(stack)
        .notify_participant_connected(tonic::Request::new(NotifyParticipantConnectedRequest {
            meeting_id: meeting_id.to_string(),
            participant_id: user.to_string(),
            handler_id: handler_id.to_string(),
            connection_id: connection_id.to_string(),
        }))
        .await
        .expect("notify connected")
        .into_inner()
        .sender_id
}

/// Send one `NotifyParticipantDisconnected` exactly as MH would.
pub async fn notify_disconnected(
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    handler_id: &str,
    connection_id: &str,
) {
    coordination(stack)
        .notify_participant_disconnected(tonic::Request::new(
            NotifyParticipantDisconnectedRequest {
                meeting_id: meeting_id.to_string(),
                participant_id: user.to_string(),
                handler_id: handler_id.to_string(),
                reason: 1,
                connection_id: connection_id.to_string(),
            },
        ))
        .await
        .expect("notify disconnected");
}

/// Every handler id the meeting is registered on, as seeded.
pub async fn registered_handlers(stack: &TestStackHandles, meeting_id: &str) -> Vec<String> {
    stack
        .mh_store
        .get_mh_assignment(meeting_id)
        .await
        .expect("store read")
        .expect("meeting seeded")
        .handlers
        .into_iter()
        .map(|h| h.mh_id)
        .collect()
}

/// Join `meeting_id` (already seeded) as token subject `user`, then report a
/// media connection to EVERY registered handler — a real client's
/// active/active behaviour.
///
/// # Panics
///
/// Panics if MC answers anything but a `JoinResponse`.
pub async fn join_as(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
) -> Session {
    let all = registered_handlers(stack, meeting_id).await;
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    join_as_on(rig, stack, meeting_id, user, &all).await
}

/// Join, then report media connections ONLY to `handlers` (possibly none):
/// partial connectivity, produced the way a real client produces it.
///
/// # Panics
///
/// Panics if MC answers anything but a `JoinResponse`, or declines a binding
/// for a handler of the set.
pub async fn join_as_on(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    handlers: &[&str],
) -> Session {
    let session = match try_join_as(rig, stack, meeting_id, user).await {
        Ok(session) => session,
        Err(e) => panic!("expected JoinResponse, got {e}"),
    };
    for handler in handlers {
        let answered = session
            .connect_to(stack, handler, &connection_id_for(user, handler))
            .await;
        assert_eq!(
            answered, session.sender_id,
            "{user} on {handler}: MC must bind the connection to the joiner's own sender id"
        );
    }
    session
}

/// [`join_as`], but as the meeting's HOST (the token carries
/// `MeetingRole::Host`, as GC stamps it for the meeting's creator).
///
/// # Panics
///
/// As [`join_as`].
pub async fn join_as_host(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
) -> Session {
    let session = match try_join_with_role(rig, stack, meeting_id, user, true).await {
        Ok(session) => session,
        Err(e) => panic!("expected JoinResponse, got {e}"),
    };
    for handler in registered_handlers(stack, meeting_id).await {
        let answered = session
            .connect_to(stack, &handler, &connection_id_for(user, &handler))
            .await;
        assert_eq!(answered, session.sender_id);
    }
    session
}

/// [`join_as`], returning MC's error text instead of panicking on a refusal.
pub async fn try_join_as(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
) -> Result<Session, String> {
    try_join_with_role(rig, stack, meeting_id, user, false).await
}

/// What a join presents for media (story 3 R-2, R-22). The suite default is
/// story 2's shape: publish audio, header v3, no join-carried declaration.
#[derive(Debug, Clone)]
pub struct JoinShape {
    /// `JoinRequest.published_kinds` as raw `(media_kind, client_muted)`
    /// entries (empty = receive-only).
    pub publish: Vec<(i32, bool)>,
    /// `ParticipantCapabilities.supported_header_versions`.
    pub header_versions: Vec<u32>,
    /// `JoinRequest.receive_capability` (`None` = absent).
    pub receive: Option<Vec<ReceiveSlot>>,
}

impl Default for JoinShape {
    fn default() -> Self {
        Self {
            publish: vec![(MediaKind::Audio as i32, false)],
            header_versions: vec![u32::from(media_protocol::frame::PROTOCOL_VERSION)],
            receive: None,
        }
    }
}

impl JoinShape {
    /// Publish exactly `kinds`, each live (not client-muted).
    #[must_use]
    pub fn publishing(kinds: &[MediaKind]) -> Self {
        Self {
            publish: kinds.iter().map(|k| (*k as i32, false)).collect(),
            ..Self::default()
        }
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

    /// Carry `slots` as the join's receive declaration.
    #[must_use]
    pub fn receiving(mut self, slots: Vec<ReceiveSlot>) -> Self {
        self.receive = Some(slots);
        self
    }
}

/// Join with an explicit [`JoinShape`]; a refusal returns MC's error text.
/// On success the join-turn view is taken as for [`join_as`]. No media
/// connection is reported.
pub async fn try_join_shaped(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    shape: JoinShape,
) -> Result<Session, String> {
    try_join_full(rig, stack, meeting_id, user, false, shape).await
}

/// Join with an explicit [`JoinShape`] as a host or not, then report a media
/// connection to EVERY registered handler (as [`join_as`]).
///
/// # Panics
///
/// Panics if MC answers anything but a `JoinResponse`.
pub async fn join_shaped(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    host: bool,
    shape: JoinShape,
) -> Session {
    let session = match try_join_full(rig, stack, meeting_id, user, host, shape).await {
        Ok(session) => session,
        Err(e) => panic!("{user}: expected JoinResponse, got {e}"),
    };
    for handler in registered_handlers(stack, meeting_id).await {
        let answered = session
            .connect_to(stack, &handler, &connection_id_for(user, &handler))
            .await;
        assert_eq!(answered, session.sender_id);
    }
    session
}

/// The one join path behind [`try_join_as`] and [`join_as_host`].
async fn try_join_with_role(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    host: bool,
) -> Result<Session, String> {
    try_join_full(rig, stack, meeting_id, user, host, JoinShape::default()).await
}

async fn try_join_full(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
    host: bool,
    shape: JoinShape,
) -> Result<Session, String> {
    let mut claims = make_meeting_claims(meeting_id);
    claims.sub = user.to_string();
    if host {
        claims.role = ::common::jwt::MeetingRole::Host;
    }
    let token = stack.keypair.sign_token(&claims);

    let conn = connect(&rig.url).await;
    let (mut send, mut recv) = conn.open_bi().await.expect("open bi").await.expect("bi");

    send.write_all(&encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::JoinRequest(JoinRequest {
            meeting_id: meeting_id.to_string(),
            join_token: token,
            participant_name: user.to_string(),
            capabilities: Some(
                proto_gen::dark_tower::signaling::v1::ParticipantCapabilities {
                    supported_codecs: Vec::new(),
                    supported_header_versions: shape.header_versions,
                },
            ),
            correlation_id: String::new(),
            binding_token: String::new(),
            identity_public_key: sample_identity_public_key(),
            published_kinds: shape
                .publish
                .into_iter()
                .map(|(media_kind, client_muted)| {
                    proto_gen::dark_tower::signaling::v1::PublishedKind {
                        media_kind,
                        client_muted,
                    }
                })
                .collect(),
            receive_capability: shape
                .receive
                .map(|slots| proto_gen::dark_tower::signaling::v1::ReceiveCapability { slots }),
        })),
    }))
    .await
    .expect("write join");

    let resp = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("join response timeout");
    let (participant_id, sender_id, media_servers, receive_slot_caps, existing_participants) =
        match resp.message {
            Some(server_message::Message::JoinResponse(r)) => (
                r.participant_id.clone(),
                r.sender_id.expect("MC allocates a sender id at join"),
                r.media_servers
                    .into_iter()
                    .map(|m| m.media_handler_url)
                    .collect::<Vec<String>>(),
                r.receive_slot_caps,
                r.existing_participants,
            ),
            other => return Err(format!("{other:?}")),
        };

    let mut session = Session {
        conn,
        send,
        recv,
        sender_id,
        media_servers,
        receive_slot_caps,
        join_directive: None,
        join_assignments: None,
        pending: std::collections::VecDeque::new(),
        user: user.to_string(),
        participant_id,
        existing_participants,
        meeting_id: meeting_id.to_string(),
    };
    session.take_join_view().await;
    Ok(session)
}

/// A framed `ServerMuteRequest` naming `target`, about AUDIO only (video
/// absent: unchanged).
pub fn server_mute_frame(target: &str, audio_muted: bool) -> bytes::BytesMut {
    server_mute_kinds_frame(target, Some(audio_muted), None)
}

/// A framed `ServerMuteRequest` naming `target`, with each kind's presence as
/// given (`None` = absent, i.e. "that kind unchanged").
pub fn server_mute_kinds_frame(
    target: &str,
    audio_muted: Option<bool>,
    video_muted: Option<bool>,
) -> bytes::BytesMut {
    server_mute_changes_frame(target, &kind_changes(audio_muted, video_muted))
}

/// A framed `ServerMuteRequest` naming `target` with exactly `changes` as raw
/// `(media_kind, muted)` entries (malformed lists included).
pub fn server_mute_changes_frame(target: &str, changes: &[(i32, bool)]) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::ServerMuteRequest(
            proto_gen::dark_tower::signaling::v1::ServerMuteRequest {
                participant_id: target.to_string(),
                changes: mute_changes(changes),
                reason: String::new(),
            },
        )),
    })
}

/// A framed `UnmuteRequest` for audio. `claimed_participant_id` is what the
/// CLIENT writes into the field MC overwrites on relay — pass another
/// participant's id to exercise the forgery path.
pub fn unmute_request_frame(claimed_participant_id: &str) -> bytes::BytesMut {
    unmute_kinds_frame(claimed_participant_id, &[MediaKind::Audio as i32])
}

/// A framed `UnmuteRequest` naming exactly `media_kinds` (raw; malformed lists
/// included).
pub fn unmute_kinds_frame(claimed_participant_id: &str, media_kinds: &[i32]) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::UnmuteRequest(
            proto_gen::dark_tower::signaling::v1::UnmuteRequest {
                participant_id: claimed_participant_id.to_string(),
                media_kinds: media_kinds.to_vec(),
            },
        )),
    })
}
