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

use std::sync::Arc;
use std::time::Duration;

use mc_service::grpc::MhRegistrationClient;
use mc_service::redis::MhAssignmentStore;
use mc_test_utils::jwt_test::make_meeting_claims;
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
}

impl Session {
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

/// Join `meeting_id` (already seeded) as token subject `user`.
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
    match try_join_as(rig, stack, meeting_id, user).await {
        Ok(session) => session,
        Err(e) => panic!("expected JoinResponse, got {e}"),
    }
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
    })
}
