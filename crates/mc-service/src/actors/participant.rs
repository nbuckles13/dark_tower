//! `ParticipantActor` - per-participant actor within a meeting (ADR-0023).
//!
//! Each `ParticipantActor`:
//! - Represents one participant in a meeting
//! - Receives signaling messages from MeetingActor and forwards to the client
//! - Sends participant state updates (Joined/Left) to the client via stream
//!
//! # Lifecycle
//!
//! 1. Created when client's JoinRequest is accepted by MeetingActor
//! 2. Runs until participant leaves, disconnects, or meeting ends
//! 3. Cancellation via child token propagates from MeetingActor

use crate::errors::McError;

use super::meeting::MeetingActorHandle;
use super::messages::{ParticipantMessage, ParticipantStateUpdate, SignalingPayload};
use super::metrics::{ActorMetrics, ActorType, MailboxMonitor};

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

/// Default channel buffer size for the participant mailbox.
const PARTICIPANT_CHANNEL_BUFFER: usize = 200;

/// Handle to a `ParticipantActor`.
#[derive(Clone, Debug)]
pub struct ParticipantActorHandle {
    sender: mpsc::Sender<ParticipantMessage>,
    cancel_token: CancellationToken,
    connection_id: String,
    participant_id: String,
}

impl ParticipantActorHandle {
    /// Get the connection ID.
    #[must_use]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    /// Get the participant ID.
    #[must_use]
    pub fn participant_id(&self) -> &str {
        &self.participant_id
    }

    /// Send a signaling message to the client.
    pub async fn send(&self, message: SignalingPayload) -> Result<(), McError> {
        self.sender
            .send(ParticipantMessage::Send { message })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Send a participant state update to the client.
    pub async fn send_update(&self, update: ParticipantStateUpdate) -> Result<(), McError> {
        self.sender
            .send(ParticipantMessage::ParticipantUpdate { update })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Close the participant actor.
    pub async fn close(&self, reason: String) -> Result<(), McError> {
        self.sender
            .send(ParticipantMessage::Close { reason })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Ping the participant actor to check liveness.
    pub async fn ping(&self) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ParticipantMessage::Ping { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Cancel the participant actor.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

/// The `ParticipantActor` implementation.
pub struct ParticipantActor {
    /// Connection ID.
    connection_id: String,
    /// Associated participant ID.
    participant_id: String,
    /// Meeting ID.
    meeting_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<ParticipantMessage>,
    /// Cancellation token (child of meeting's token).
    cancel_token: CancellationToken,
    /// Shared metrics.
    metrics: Arc<ActorMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
    /// Whether the actor is closing.
    is_closing: bool,
    /// Optional sender for writing framed protobuf bytes to the WebTransport stream.
    /// `None` in tests or when no stream is wired up yet.
    stream_tx: Option<mpsc::Sender<bytes::Bytes>>,
    /// Handle to the parent MeetingActor for disconnect notification.
    /// `None` in tests.
    meeting_handle: Option<MeetingActorHandle>,
}

impl ParticipantActor {
    /// Spawn a new participant actor.
    ///
    /// Returns a handle and the task join handle.
    /// The actor starts without a WebTransport stream (messages are logged only).
    pub fn spawn(
        connection_id: String,
        participant_id: String,
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
    ) -> (ParticipantActorHandle, JoinHandle<()>) {
        Self::spawn_inner(
            connection_id,
            participant_id,
            meeting_id,
            cancel_token,
            metrics,
            None,
            None,
        )
    }

    /// Spawn a participant actor with a meeting handle for disconnect notification.
    ///
    /// When the actor exits (for any reason), it calls
    /// `meeting_handle.connection_disconnected()` to notify the meeting.
    pub fn spawn_with_meeting(
        connection_id: String,
        participant_id: String,
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        meeting_handle: MeetingActorHandle,
    ) -> (ParticipantActorHandle, JoinHandle<()>) {
        Self::spawn_inner(
            connection_id,
            participant_id,
            meeting_id,
            cancel_token,
            metrics,
            None,
            Some(meeting_handle),
        )
    }

    /// Spawn a participant actor with a WebTransport stream sender (test-only).
    ///
    /// Messages sent via `handle_send` and `handle_update` will be encoded as
    /// length-prefixed protobuf and forwarded through `stream_tx`.
    ///
    /// Note: spawns without a `MeetingActorHandle`, so disconnect notifications
    /// will not be sent on exit. Production code should use `spawn_inner` with
    /// both `stream_tx` and `meeting_handle`.
    #[cfg(test)]
    pub fn spawn_with_stream(
        connection_id: String,
        participant_id: String,
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        stream_tx: mpsc::Sender<bytes::Bytes>,
    ) -> (ParticipantActorHandle, JoinHandle<()>) {
        Self::spawn_inner(
            connection_id,
            participant_id,
            meeting_id,
            cancel_token,
            metrics,
            Some(stream_tx),
            None,
        )
    }

    pub(crate) fn spawn_inner(
        connection_id: String,
        participant_id: String,
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        stream_tx: Option<mpsc::Sender<bytes::Bytes>>,
        meeting_handle: Option<MeetingActorHandle>,
    ) -> (ParticipantActorHandle, JoinHandle<()>) {
        let (sender, receiver) = mpsc::channel(PARTICIPANT_CHANNEL_BUFFER);

        let actor = Self {
            connection_id: connection_id.clone(),
            participant_id: participant_id.clone(),
            meeting_id,
            receiver,
            cancel_token: cancel_token.clone(),
            metrics,
            mailbox: MailboxMonitor::new(ActorType::Participant, &connection_id),
            is_closing: false,
            stream_tx,
            meeting_handle,
        };

        let task_handle = tokio::spawn(actor.run());

        let handle = ParticipantActorHandle {
            sender,
            cancel_token,
            connection_id,
            participant_id,
        };

        (handle, task_handle)
    }

    /// Run the actor message loop.
    #[instrument(
        skip_all,
        name = "mc.actor.participant",
        fields(
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            meeting_id = %self.meeting_id
        )
    )]
    async fn run(mut self) {
        debug!(
            target: "mc.actor.participant",
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            meeting_id = %self.meeting_id,
            "ParticipantActor started"
        );

        loop {
            tokio::select! {
                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    debug!(
                        target: "mc.actor.participant",
                        connection_id = %self.connection_id,
                        "ParticipantActor received cancellation signal"
                    );
                    self.graceful_close("cancelled").await;
                    break;
                }

                // Handle messages
                msg = self.receiver.recv() => {
                    match msg {
                        Some(message) => {
                            self.mailbox.record_enqueue();
                            let should_exit = self.handle_message(message).await;
                            self.mailbox.record_dequeue();
                            self.metrics.record_message_processed();

                            if should_exit {
                                break;
                            }
                        }
                        None => {
                            debug!(
                                target: "mc.actor.participant",
                                connection_id = %self.connection_id,
                                "ParticipantActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        // Notify meeting of disconnect before stopping
        if let Some(meeting_handle) = &self.meeting_handle {
            debug!(
                target: "mc.actor.participant",
                connection_id = %self.connection_id,
                participant_id = %self.participant_id,
                "Notifying meeting of disconnect"
            );
            let _ = meeting_handle
                .connection_disconnected(self.connection_id.clone(), self.participant_id.clone())
                .await;
        }

        info!(
            target: "mc.actor.participant",
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            messages_processed = self.mailbox.messages_processed(),
            "ParticipantActor stopped"
        );
    }

    /// Handle a single message. Returns true if the actor should exit.
    async fn handle_message(&mut self, message: ParticipantMessage) -> bool {
        match message {
            ParticipantMessage::Send { message } => {
                self.handle_send(message).await;
                false
            }

            ParticipantMessage::ParticipantUpdate { update } => {
                self.handle_update(update).await;
                false
            }

            ParticipantMessage::Close { reason } => {
                self.graceful_close(&reason).await;
                true
            }

            ParticipantMessage::Ping { respond_to } => {
                let _ = respond_to.send(());
                false
            }
        }
    }

    /// Handle sending a signaling message to the client.
    ///
    /// If a WebTransport stream sender is wired up, encodes the message as
    /// a protobuf `ServerMessage` and sends it. Otherwise, logs only.
    async fn handle_send(&mut self, message: SignalingPayload) {
        if self.is_closing {
            warn!(
                target: "mc.actor.participant",
                connection_id = %self.connection_id,
                "Attempted to send message while closing"
            );
            return;
        }

        debug!(
            target: "mc.actor.participant",
            connection_id = %self.connection_id,
            message_type = ?std::mem::discriminant(&message),
            "Sending message to client"
        );

        // If no stream is wired up, log the message for debugging
        if self.stream_tx.is_none() {
            debug!(
                target: "mc.actor.participant",
                connection_id = %self.connection_id,
                "No stream wired — message logged only"
            );
            return;
        }

        // Encode as protobuf ServerMessage via the raw payload
        if let SignalingPayload::Raw { data, .. } = &message {
            if let Some(tx) = &self.stream_tx {
                if tx.try_send(bytes::Bytes::copy_from_slice(data)).is_err() {
                    warn!(
                        target: "mc.actor.participant",
                        connection_id = %self.connection_id,
                        "Stream outbound channel full or closed"
                    );
                }
            }
        } else {
            debug!(
                target: "mc.actor.participant",
                connection_id = %self.connection_id,
                message_type = ?std::mem::discriminant(&message),
                "Non-Raw payload not forwarded to stream"
            );
        }
    }

    /// Handle a participant state update.
    ///
    /// Only `ParticipantJoined` and `ParticipantLeft` are serialized to the wire.
    /// Other variants (MuteChanged, Disconnected, Reconnected) are logged only.
    async fn handle_update(&mut self, update: ParticipantStateUpdate) {
        if self.is_closing {
            return;
        }

        debug!(
            target: "mc.actor.participant",
            connection_id = %self.connection_id,
            update_type = ?std::mem::discriminant(&update),
            "Sending participant update to client"
        );

        // Encode to protobuf if it's a wire-visible update
        if let Some(server_msg) = crate::webtransport::handler::encode_participant_update(&update) {
            if let Some(tx) = &self.stream_tx {
                use prost::Message;
                let encoded = server_msg.encode_to_vec();
                if tx.try_send(bytes::Bytes::from(encoded)).is_err() {
                    warn!(
                        target: "mc.actor.participant",
                        connection_id = %self.connection_id,
                        "Stream outbound channel full or closed"
                    );
                }
            }
        }
    }

    /// Gracefully close the participant actor.
    ///
    /// Drops the stream sender to signal the WebTransport write task to close.
    async fn graceful_close(&mut self, reason: &str) {
        if self.is_closing {
            return;
        }

        self.is_closing = true;

        debug!(
            target: "mc.actor.participant",
            connection_id = %self.connection_id,
            reason = %reason,
            "Closing participant actor"
        );

        // Drop the stream sender to signal the bridge loop to close
        self.stream_tx.take();

        // Brief delay to allow final messages to be flushed
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_participant_actor_spawn() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ParticipantActor::spawn(
            "conn-123".to_string(),
            "part-456".to_string(),
            "meeting-789".to_string(),
            cancel_token.clone(),
            metrics,
        );

        assert_eq!(handle.connection_id(), "conn-123");
        assert_eq!(handle.participant_id(), "part-456");
        assert!(!handle.is_cancelled());

        handle.cancel();
        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn test_participant_actor_send() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ParticipantActor::spawn(
            "conn-send-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Send a message
        let result = handle
            .send(SignalingPayload::MuteUpdate {
                audio_muted: true,
                video_muted: false,
            })
            .await;
        assert!(result.is_ok());

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_participant_actor_send_update() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ParticipantActor::spawn(
            "conn-update-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Send an update
        let result = handle
            .send_update(ParticipantStateUpdate::Disconnected {
                participant_id: "other-part".to_string(),
            })
            .await;
        assert!(result.is_ok());

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_participant_actor_ping() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ParticipantActor::spawn(
            "conn-ping-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Ping should succeed
        let result = handle.ping().await;
        assert!(result.is_ok());

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_participant_actor_close() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = ParticipantActor::spawn(
            "conn-close-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Close the participant actor
        let result = handle.close("test close".to_string()).await;
        assert!(result.is_ok());

        // Wait for task to complete
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_participant_actor_cancellation() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = ParticipantActor::spawn(
            "conn-cancel-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Cancel
        handle.cancel();

        // Task should complete
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_participant_actor_parent_cancellation() {
        let parent_token = CancellationToken::new();
        let child_token = parent_token.child_token();
        let metrics = ActorMetrics::new();

        let (handle, task) = ParticipantActor::spawn(
            "conn-parent-cancel-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            child_token,
            metrics,
        );

        // Cancel parent
        parent_token.cancel();

        // Give time for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(handle.is_cancelled());

        // Task should complete
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok());
    }

    // =========================================================================
    // T3: spawn_with_stream tests
    // =========================================================================

    #[tokio::test]
    async fn test_spawn_with_stream_participant_joined_sends_bytes() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();
        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(16);

        let (handle, _task) = ParticipantActor::spawn_with_stream(
            "conn-stream-joined".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
            stream_tx,
        );

        // Send a ParticipantJoined update
        let update = ParticipantStateUpdate::Joined(super::super::messages::ParticipantInfo {
            participant_id: "part-new".to_string(),
            user_id: "user-new".to_string(),
            display_name: "New User".to_string(),
            audio_self_muted: false,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
            status: super::super::messages::ParticipantStatus::Connected,
        });
        handle.send_update(update).await.unwrap();

        // Give the actor time to process
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Should have received encoded protobuf bytes
        let received = stream_rx.try_recv();
        assert!(received.is_ok(), "Expected bytes on stream receiver");

        // Decode and verify it's a ParticipantJoined message
        use prost::Message;
        let server_msg =
            proto_gen::signaling::ServerMessage::decode(received.unwrap().as_ref()).unwrap();
        match server_msg.message.unwrap() {
            proto_gen::signaling::server_message::Message::ParticipantJoined(joined) => {
                let p = joined.participant.unwrap();
                assert_eq!(p.participant_id, "part-new");
                assert_eq!(p.name, "New User");
            }
            other => panic!("Expected ParticipantJoined, got {other:?}"),
        }

        handle.cancel();
    }

    #[tokio::test]
    async fn test_spawn_with_stream_mute_changed_not_forwarded() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();
        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(16);

        let (handle, _task) = ParticipantActor::spawn_with_stream(
            "conn-stream-mute".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
            stream_tx,
        );

        // Send a MuteChanged update (should NOT be forwarded to stream)
        let update = ParticipantStateUpdate::MuteChanged {
            participant_id: "part-other".to_string(),
            audio_self_muted: true,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
        };
        handle.send_update(update).await.unwrap();

        // Give the actor time to process
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Stream should be empty — MuteChanged is not serialized
        let received = stream_rx.try_recv();
        assert!(
            received.is_err(),
            "Expected no bytes for MuteChanged update"
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_spawn_with_stream_close_drops_sender() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();
        let (stream_tx, mut stream_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(16);

        let (handle, task) = ParticipantActor::spawn_with_stream(
            "conn-stream-close".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
            stream_tx,
        );

        // Close the participant actor
        handle.close("test close".to_string()).await.unwrap();

        // Wait for task to complete
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok());

        // Stream receiver should return None (sender dropped)
        let received = stream_rx.recv().await;
        assert!(received.is_none(), "Expected None after stream_tx dropped");
    }
}
