//! `ConnectionActor` - per-WebTransport connection actor (ADR-0023).
//!
//! Each `ConnectionActor`:
//! - Handles exactly one WebTransport connection
//! - Is 1:1 with meeting participation (one connection = one meeting)
//! - Receives signaling messages from client and forwards to MeetingActor
//! - Sends signaling messages from MeetingActor to client
//!
//! # Lifecycle
//!
//! 1. Created when client's JoinRequest is accepted by MeetingActor
//! 2. Runs until connection closes, participant leaves, or meeting ends
//! 3. Cancellation via child token propagates from MeetingActor

use crate::errors::McError;

use super::messages::{ConnectionMessage, ParticipantStateUpdate, SignalingPayload};
use super::metrics::{ActorMetrics, ActorType, MailboxMonitor};

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

/// Default channel buffer size for the connection mailbox.
const CONNECTION_CHANNEL_BUFFER: usize = 200;

/// Handle to a `ConnectionActor`.
#[derive(Clone, Debug)]
pub struct ConnectionActorHandle {
    sender: mpsc::Sender<ConnectionMessage>,
    cancel_token: CancellationToken,
    connection_id: String,
    participant_id: String,
}

impl ConnectionActorHandle {
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
            .send(ConnectionMessage::Send { message })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Send a participant state update to the client.
    pub async fn send_update(&self, update: ParticipantStateUpdate) -> Result<(), McError> {
        self.sender
            .send(ConnectionMessage::ParticipantUpdate { update })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Close the connection.
    pub async fn close(&self, reason: String) -> Result<(), McError> {
        self.sender
            .send(ConnectionMessage::Close { reason })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Ping the connection to check liveness.
    pub async fn ping(&self) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ConnectionMessage::Ping { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Cancel the connection actor.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

/// The `ConnectionActor` implementation.
pub struct ConnectionActor {
    /// Connection ID.
    connection_id: String,
    /// Associated participant ID.
    participant_id: String,
    /// Meeting ID.
    meeting_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<ConnectionMessage>,
    /// Cancellation token (child of meeting's token).
    cancel_token: CancellationToken,
    /// Shared metrics.
    metrics: Arc<ActorMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
    /// Whether the connection is closing.
    is_closing: bool,
}

impl ConnectionActor {
    /// Spawn a new connection actor.
    ///
    /// Returns a handle and the task join handle.
    pub fn spawn(
        connection_id: String,
        participant_id: String,
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
    ) -> (ConnectionActorHandle, JoinHandle<()>) {
        let (sender, receiver) = mpsc::channel(CONNECTION_CHANNEL_BUFFER);

        let actor = Self {
            connection_id: connection_id.clone(),
            participant_id: participant_id.clone(),
            meeting_id,
            receiver,
            cancel_token: cancel_token.clone(),
            metrics,
            mailbox: MailboxMonitor::new(ActorType::Connection, &connection_id),
            is_closing: false,
        };

        let task_handle = tokio::spawn(actor.run());

        let handle = ConnectionActorHandle {
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
        name = "mc.actor.connection",
        fields(
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            meeting_id = %self.meeting_id
        )
    )]
    async fn run(mut self) {
        debug!(
            target: "mc.actor.connection",
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            meeting_id = %self.meeting_id,
            "ConnectionActor started"
        );

        loop {
            tokio::select! {
                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    debug!(
                        target: "mc.actor.connection",
                        connection_id = %self.connection_id,
                        "ConnectionActor received cancellation signal"
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
                                target: "mc.actor.connection",
                                connection_id = %self.connection_id,
                                "ConnectionActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        info!(
            target: "mc.actor.connection",
            connection_id = %self.connection_id,
            participant_id = %self.participant_id,
            messages_processed = self.mailbox.messages_processed(),
            "ConnectionActor stopped"
        );
    }

    /// Handle a single message. Returns true if the actor should exit.
    async fn handle_message(&mut self, message: ConnectionMessage) -> bool {
        match message {
            ConnectionMessage::Send { message } => {
                self.handle_send(message).await;
                false
            }

            ConnectionMessage::ParticipantUpdate { update } => {
                self.handle_update(update).await;
                false
            }

            ConnectionMessage::Close { reason } => {
                self.graceful_close(&reason).await;
                true
            }

            ConnectionMessage::Ping { respond_to } => {
                let _ = respond_to.send(());
                false
            }
        }
    }

    /// Handle sending a signaling message to the client.
    async fn handle_send(&mut self, message: SignalingPayload) {
        if self.is_closing {
            warn!(
                target: "mc.actor.connection",
                connection_id = %self.connection_id,
                "Attempted to send message while closing"
            );
            return;
        }

        debug!(
            target: "mc.actor.connection",
            connection_id = %self.connection_id,
            message_type = ?std::mem::discriminant(&message),
            "Sending message to client"
        );

        // TODO (Phase 6g): Actually send via WebTransport connection
        // For now, just log the message
        match &message {
            SignalingPayload::MuteUpdate {
                audio_muted,
                video_muted,
            } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    audio_muted = audio_muted,
                    video_muted = video_muted,
                    "Would send mute update"
                );
            }
            SignalingPayload::LayoutSubscribe { layout_type } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    layout_type = %layout_type,
                    "Would send layout subscription"
                );
            }
            SignalingPayload::Chat { content } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    content_len = content.len(),
                    "Would send chat message"
                );
            }
            SignalingPayload::Raw { message_type, data } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    message_type = message_type,
                    data_len = data.len(),
                    "Would send raw message"
                );
            }
        }
    }

    /// Handle a participant state update.
    async fn handle_update(&mut self, update: ParticipantStateUpdate) {
        if self.is_closing {
            return;
        }

        debug!(
            target: "mc.actor.connection",
            connection_id = %self.connection_id,
            update_type = ?std::mem::discriminant(&update),
            "Sending participant update to client"
        );

        // TODO (Phase 6g): Convert to protobuf and send via WebTransport
        match &update {
            ParticipantStateUpdate::Joined(info) => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    participant_id = %info.participant_id,
                    "Would notify: participant joined"
                );
            }
            ParticipantStateUpdate::Left {
                participant_id,
                reason,
            } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    participant_id = %participant_id,
                    reason = ?reason,
                    "Would notify: participant left"
                );
            }
            ParticipantStateUpdate::MuteChanged {
                participant_id,
                audio_self_muted,
                video_self_muted,
                audio_host_muted,
                video_host_muted,
            } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    participant_id = %participant_id,
                    audio_self_muted = audio_self_muted,
                    video_self_muted = video_self_muted,
                    audio_host_muted = audio_host_muted,
                    video_host_muted = video_host_muted,
                    "Would notify: mute changed"
                );
            }
            ParticipantStateUpdate::Disconnected { participant_id } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    participant_id = %participant_id,
                    "Would notify: participant disconnected"
                );
            }
            ParticipantStateUpdate::Reconnected { participant_id } => {
                debug!(
                    target: "mc.actor.connection",
                    connection_id = %self.connection_id,
                    participant_id = %participant_id,
                    "Would notify: participant reconnected"
                );
            }
        }
    }

    /// Gracefully close the connection.
    async fn graceful_close(&mut self, reason: &str) {
        if self.is_closing {
            return;
        }

        self.is_closing = true;

        debug!(
            target: "mc.actor.connection",
            connection_id = %self.connection_id,
            reason = %reason,
            "Closing connection"
        );

        // TODO (Phase 6g): Send close frame to WebTransport connection
        // and wait for acknowledgment

        // Brief delay to allow final messages to be sent
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_actor_spawn() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ConnectionActor::spawn(
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
    async fn test_connection_actor_send() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ConnectionActor::spawn(
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
    async fn test_connection_actor_send_update() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ConnectionActor::spawn(
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
    async fn test_connection_actor_ping() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = ConnectionActor::spawn(
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
    async fn test_connection_actor_close() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = ConnectionActor::spawn(
            "conn-close-test".to_string(),
            "part-1".to_string(),
            "meeting-1".to_string(),
            cancel_token.clone(),
            metrics,
        );

        // Close the connection
        let result = handle.close("test close".to_string()).await;
        assert!(result.is_ok());

        // Wait for task to complete
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connection_actor_cancellation() {
        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, task) = ConnectionActor::spawn(
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
    async fn test_connection_actor_parent_cancellation() {
        let parent_token = CancellationToken::new();
        let child_token = parent_token.child_token();
        let metrics = ActorMetrics::new();

        let (handle, task) = ConnectionActor::spawn(
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
}
