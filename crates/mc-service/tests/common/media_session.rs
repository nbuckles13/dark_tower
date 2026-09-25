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

/// A framed `MuteRequest`.
pub fn mute_frame(audio_muted: bool, video_muted: bool) -> bytes::BytesMut {
    encode_framed(&ClientMessage {
        trace_parent: String::new(),
        trace_state: String::new(),
        message: Some(client_message::Message::MuteRequest(MuteRequest {
            audio_muted,
            video_muted,
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
    /// The token `sub` this session joined as (what MH reports).
    pub user: String,
    /// The meeting joined.
    pub meeting_id: String,
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
        tokio::time::timeout(QUIET, read_server_message(&mut self.recv))
            .await
            .ok()
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

/// [`join_as`], returning MC's error text instead of panicking on a refusal.
pub async fn try_join_as(
    rig: &AcceptLoopRig,
    stack: &TestStackHandles,
    meeting_id: &str,
    user: &str,
) -> Result<Session, String> {
    let mut claims = make_meeting_claims(meeting_id);
    claims.sub = user.to_string();
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
            capabilities: None,
            correlation_id: String::new(),
            binding_token: String::new(),
            identity_public_key: sample_identity_public_key(),
        })),
    }))
    .await
    .expect("write join");

    let resp = tokio::time::timeout(Duration::from_secs(5), read_server_message(&mut recv))
        .await
        .expect("join response timeout");
    let (sender_id, media_servers) = match resp.message {
        Some(server_message::Message::JoinResponse(r)) => (
            r.sender_id.expect("MC allocates a sender id at join"),
            r.media_servers
                .into_iter()
                .map(|m| m.media_handler_url)
                .collect::<Vec<String>>(),
        ),
        other => return Err(format!("{other:?}")),
    };

    Ok(Session {
        conn,
        send,
        recv,
        sender_id,
        media_servers,
        user: user.to_string(),
        meeting_id: meeting_id.to_string(),
    })
}
