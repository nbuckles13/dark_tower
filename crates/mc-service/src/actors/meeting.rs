//! `MeetingActor` - per-meeting actor that owns meeting state (ADR-0023).
//!
//! Each `MeetingActor`:
//! - Owns all state for one meeting (participants, subscriptions, mute status)
//! - Supervises N `ConnectionActor` instances
//! - Handles session binding tokens for reconnection
//! - Coordinates with Redis for persistent state
//!
//! # Participant Disconnect Handling (ADR-0023 Section 1a)
//!
//! When a connection drops:
//! 1. Participant marked as "disconnected" (still visible to others)
//! 2. 30-second grace period for reconnection
//! 3. If not reconnected: participant removed, slots released

use crate::errors::McError;

use super::connection::{ConnectionActor, ConnectionActorHandle};
use super::messages::{
    JoinResult, LeaveReason, MeetingMessage, MeetingState, ParticipantInfo, ParticipantStateUpdate,
    ParticipantStatus, ReconnectResult, SignalingPayload,
};
use super::metrics::{ActorMetrics, ActorType, ControllerMetrics, MailboxMonitor};
use super::session::{SessionBindingManager, StoredBinding};

use common::secret::SecretBox;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Default channel buffer size for the meeting mailbox.
const MEETING_CHANNEL_BUFFER: usize = 500;

/// Grace period for participant reconnection (ADR-0023: 30 seconds).
const DISCONNECT_GRACE_PERIOD: Duration = Duration::from_secs(30);

/// Handle to a `MeetingActor`.
#[derive(Clone)]
pub struct MeetingActorHandle {
    sender: mpsc::Sender<MeetingMessage>,
    cancel_token: CancellationToken,
    meeting_id: String,
}

impl MeetingActorHandle {
    /// Get the meeting ID.
    #[must_use]
    pub fn meeting_id(&self) -> &str {
        &self.meeting_id
    }

    /// Request a new connection to join this meeting.
    ///
    /// # Arguments
    ///
    /// * `connection_id` - Unique connection identifier
    /// * `user_id` - User ID from JWT
    /// * `participant_id` - Participant ID for this meeting
    /// * `is_host` - Whether this participant has host privileges
    pub async fn connection_join(
        &self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        is_host: bool,
    ) -> Result<JoinResult, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ConnectionJoin {
                connection_id,
                user_id,
                participant_id,
                is_host,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Notify of a connection disconnect.
    pub async fn connection_disconnected(
        &self,
        connection_id: String,
        participant_id: String,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Attempt to reconnect with binding token.
    pub async fn connection_reconnect(
        &self,
        connection_id: String,
        correlation_id: String,
        binding_token: String,
    ) -> Result<ReconnectResult, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ConnectionReconnect {
                connection_id,
                correlation_id,
                binding_token,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Participant leaves the meeting (explicit leave).
    pub async fn participant_leave(&self, participant_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ParticipantLeave {
                participant_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Forward a signaling message.
    pub async fn signaling_message(
        &self,
        participant_id: String,
        message: SignalingPayload,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::SignalingMessage {
                participant_id,
                message,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Get current meeting state.
    pub async fn get_state(&self) -> Result<MeetingState, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::GetState { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Update self-mute status (informational).
    pub async fn update_self_mute(
        &self,
        participant_id: String,
        audio_muted: bool,
        video_muted: bool,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::UpdateSelfMute {
                participant_id,
                audio_muted,
                video_muted,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))
    }

    /// Host mutes a participant (enforced).
    pub async fn host_mute(
        &self,
        target_participant_id: String,
        muted_by: String,
        audio_muted: bool,
        video_muted: bool,
    ) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::HostMute {
                target_participant_id,
                muted_by,
                audio_muted,
                video_muted,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// End the meeting.
    pub async fn end_meeting(&self, reason: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::EndMeeting {
                reason,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Cancel the meeting actor.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Get a child token for connection actors.
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.cancel_token.child_token()
    }
}

/// Participant state within a meeting.
#[derive(Debug)]
struct Participant {
    /// Participant ID.
    participant_id: String,
    /// User ID (from JWT).
    user_id: String,
    /// Display name.
    display_name: String,
    /// Correlation ID for reconnection.
    correlation_id: String,
    /// Current connection actor handle (if connected).
    connection: Option<ConnectionActorHandle>,
    /// Connection status.
    status: ParticipantStatus,
    /// Timestamp when disconnected (for grace period).
    disconnected_at: Option<Instant>,
    /// Audio self-mute (informational).
    audio_self_muted: bool,
    /// Video self-mute (informational).
    video_self_muted: bool,
    /// Audio host-mute (enforced).
    audio_host_muted: bool,
    /// Video host-mute (enforced).
    video_host_muted: bool,
    /// Whether this participant has host privileges.
    is_host: bool,
}

impl Participant {
    fn to_info(&self) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: self.participant_id.clone(),
            user_id: self.user_id.clone(),
            display_name: self.display_name.clone(),
            audio_self_muted: self.audio_self_muted,
            video_self_muted: self.video_self_muted,
            audio_host_muted: self.audio_host_muted,
            video_host_muted: self.video_host_muted,
            status: self.status,
        }
    }
}

/// Managed connection state.
struct ManagedConnection {
    /// Handle to the connection actor.
    #[allow(dead_code)] // Will be used in Phase 6g for signaling
    handle: ConnectionActorHandle,
    /// Join handle for monitoring.
    task_handle: JoinHandle<()>,
    /// Associated participant ID.
    participant_id: String,
}

/// The `MeetingActor` implementation.
pub struct MeetingActor {
    /// Meeting ID.
    meeting_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<MeetingMessage>,
    /// Cancellation token (child of controller's token).
    cancel_token: CancellationToken,
    /// Participants by ID.
    participants: HashMap<String, Participant>,
    /// Connections by ID.
    connections: HashMap<String, ManagedConnection>,
    /// Correlation ID to participant ID mapping.
    correlation_to_participant: HashMap<String, String>,
    /// Session binding manager for token generation/validation (ADR-0023).
    binding_manager: SessionBindingManager,
    /// Stored bindings by correlation ID.
    stored_bindings: HashMap<String, StoredBinding>,
    /// Current fencing generation.
    fencing_generation: u64,
    /// Meeting creation timestamp.
    created_at: i64,
    /// Whether the meeting is shutting down.
    is_shutting_down: bool,
    /// Shared actor metrics.
    metrics: Arc<ActorMetrics>,
    /// Controller metrics for GC heartbeat reporting (participant count).
    controller_metrics: Arc<ControllerMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
}

impl MeetingActor {
    /// Spawn a new meeting actor.
    ///
    /// Returns a handle and the task join handle.
    ///
    /// # Arguments
    ///
    /// * `meeting_id` - Unique meeting identifier
    /// * `cancel_token` - Cancellation token (child of controller's token)
    /// * `metrics` - Shared actor metrics
    /// * `controller_metrics` - Controller metrics for GC heartbeat reporting (participant count)
    /// * `master_secret` - Master secret for HKDF key derivation (ADR-0023). Wrapped in
    ///   SecretBox to ensure secure memory handling (zeroization on drop, redacted Debug).
    pub fn spawn(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> (MeetingActorHandle, JoinHandle<()>) {
        let (sender, receiver) = mpsc::channel(MEETING_CHANNEL_BUFFER);

        let actor = Self {
            meeting_id: meeting_id.clone(),
            receiver,
            cancel_token: cancel_token.clone(),
            participants: HashMap::new(),
            connections: HashMap::new(),
            correlation_to_participant: HashMap::new(),
            binding_manager: SessionBindingManager::new(master_secret),
            stored_bindings: HashMap::new(),
            fencing_generation: 1,
            created_at: chrono::Utc::now().timestamp(),
            is_shutting_down: false,
            metrics,
            controller_metrics,
            mailbox: MailboxMonitor::new(ActorType::Meeting, &meeting_id),
        };

        let task_handle = tokio::spawn(actor.run());

        let handle = MeetingActorHandle {
            sender,
            cancel_token,
            meeting_id,
        };

        (handle, task_handle)
    }

    /// Run the actor message loop.
    #[instrument(skip_all, name = "mc.actor.meeting", fields(meeting_id = %self.meeting_id))]
    async fn run(mut self) {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            "MeetingActor started"
        );

        // Create interval for checking disconnect grace periods
        let mut grace_check = tokio::time::interval(Duration::from_secs(5));

        loop {
            // Check for terminated connection actors
            self.check_connection_health().await;

            tokio::select! {
                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    info!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        "MeetingActor received cancellation signal"
                    );
                    self.graceful_shutdown().await;
                    break;
                }

                // Check disconnect grace periods
                _ = grace_check.tick() => {
                    self.check_disconnect_timeouts().await;
                }

                // Handle messages
                msg = self.receiver.recv() => {
                    match msg {
                        Some(message) => {
                            self.mailbox.record_enqueue();
                            self.handle_message(message).await;
                            self.mailbox.record_dequeue();
                            self.metrics.record_message_processed();
                        }
                        None => {
                            info!(
                                target: "mc.actor.meeting",
                                meeting_id = %self.meeting_id,
                                "MeetingActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participants = self.participants.len(),
            messages_processed = self.mailbox.messages_processed(),
            "MeetingActor stopped"
        );
    }

    /// Handle a single message.
    async fn handle_message(&mut self, message: MeetingMessage) {
        match message {
            MeetingMessage::ConnectionJoin {
                connection_id,
                user_id,
                participant_id,
                is_host,
                respond_to,
            } => {
                let result = self
                    .handle_join(connection_id, user_id, participant_id, is_host)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
            } => {
                self.handle_disconnect(&connection_id, &participant_id)
                    .await;
            }

            MeetingMessage::ConnectionReconnect {
                connection_id,
                correlation_id,
                binding_token,
                respond_to,
            } => {
                let result = self
                    .handle_reconnect(connection_id, correlation_id, binding_token)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::ParticipantLeave {
                participant_id,
                respond_to,
            } => {
                let result = self.handle_leave(&participant_id).await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::SignalingMessage {
                participant_id,
                message,
            } => {
                self.handle_signaling(&participant_id, message).await;
            }

            MeetingMessage::GetState { respond_to } => {
                let state = self.get_state();
                let _ = respond_to.send(state);
            }

            MeetingMessage::UpdateSelfMute {
                participant_id,
                audio_muted,
                video_muted,
            } => {
                self.handle_self_mute(&participant_id, audio_muted, video_muted)
                    .await;
            }

            MeetingMessage::HostMute {
                target_participant_id,
                muted_by,
                audio_muted,
                video_muted,
                respond_to,
            } => {
                let result = self
                    .handle_host_mute(&target_participant_id, &muted_by, audio_muted, video_muted)
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::EndMeeting { reason, respond_to } => {
                let result = self.handle_end_meeting(&reason).await;
                let _ = respond_to.send(result);
            }
        }
    }

    /// Handle a new connection joining.
    ///
    /// Generates secure binding tokens per ADR-0023 Section 1:
    /// - Correlation ID (UUIDv7)
    /// - Binding token via HMAC-SHA256(meeting_key, correlation_id || participant_id || nonce)
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_join(
        &mut self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        is_host: bool,
    ) -> Result<JoinResult, McError> {
        if self.is_shutting_down {
            return Err(McError::Draining);
        }

        // Check if participant already exists (don't include participant_id in error - MINOR-002)
        if self.participants.contains_key(&participant_id) {
            return Err(McError::Conflict(
                "Participant already in meeting".to_string(),
            ));
        }

        debug!(
            target: "mc.actor.meeting",
            "Participant joining"
        );

        // Generate correlation ID and binding token (ADR-0023 Section 1)
        let correlation_id = StoredBinding::generate_correlation_id();
        let (binding_token, nonce) =
            self.binding_manager
                .generate_token(&self.meeting_id, &correlation_id, &participant_id);

        // Store the binding for reconnection validation
        let stored_binding = StoredBinding::new(
            correlation_id.clone(),
            participant_id.clone(),
            user_id.clone(),
            nonce,
            binding_token.clone(),
        );
        self.stored_bindings
            .insert(correlation_id.clone(), stored_binding);

        // Create connection actor
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ConnectionActor::spawn(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
        );

        // Store connection
        self.connections.insert(
            connection_id.clone(),
            ManagedConnection {
                handle: conn_handle.clone(),
                task_handle: conn_task,
                participant_id: participant_id.clone(),
            },
        );

        // Create participant (MINOR-003: use generic display name, not derived from user_id)
        let display_name = format!("Participant {}", self.participants.len() + 1);
        let participant = Participant {
            participant_id: participant_id.clone(),
            user_id: user_id.clone(),
            display_name,
            correlation_id: correlation_id.clone(),
            connection: Some(conn_handle),
            status: ParticipantStatus::Connected,
            disconnected_at: None,
            audio_self_muted: false,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
            is_host,
        };

        let participant_info = participant.to_info();

        self.participants
            .insert(participant_id.clone(), participant);
        self.correlation_to_participant
            .insert(correlation_id.clone(), participant_id.clone());

        self.metrics.connection_created();
        self.controller_metrics.increment_participants();

        // Get list of other participants
        let participants: Vec<ParticipantInfo> = self
            .participants
            .values()
            .filter(|p| p.participant_id != participant_id)
            .map(Participant::to_info)
            .collect();

        // Broadcast join to other participants
        self.broadcast_update(
            &participant_id,
            ParticipantStateUpdate::Joined(participant_info),
        )
        .await;

        info!(
            target: "mc.actor.meeting",
            total_participants = self.participants.len(),
            "Participant joined"
        );

        Ok(JoinResult {
            participant_id,
            correlation_id,
            binding_token,
            participants,
            fencing_generation: self.fencing_generation,
        })
    }

    /// Handle connection disconnect.
    async fn handle_disconnect(&mut self, connection_id: &str, participant_id: &str) {
        debug!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            connection_id = %connection_id,
            "Connection disconnected"
        );

        // Remove connection
        if let Some(conn) = self.connections.remove(connection_id) {
            // Wait briefly for task to complete
            let _ = tokio::time::timeout(Duration::from_millis(100), conn.task_handle).await;
            self.metrics.connection_closed();
        }

        // Mark participant as disconnected (start grace period)
        if let Some(participant) = self.participants.get_mut(participant_id) {
            participant.status = ParticipantStatus::Disconnected;
            participant.disconnected_at = Some(Instant::now());
            participant.connection = None;

            // Broadcast disconnect to other participants
            self.broadcast_update(
                participant_id,
                ParticipantStateUpdate::Disconnected {
                    participant_id: participant_id.to_string(),
                },
            )
            .await;

            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Participant disconnected, grace period started"
            );
        }
    }

    /// Handle reconnection attempt.
    ///
    /// Validates binding token per ADR-0023 Section 1:
    /// 1. Correlation ID exists
    /// 2. Binding token HMAC verification (constant-time)
    /// 3. Token not expired (30s TTL)
    ///
    /// On success, rotates correlation ID and binding token.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_reconnect(
        &mut self,
        connection_id: String,
        correlation_id: String,
        binding_token: String,
    ) -> Result<ReconnectResult, McError> {
        // Find stored binding by correlation ID
        let stored_binding =
            self.stored_bindings
                .get(&correlation_id)
                .ok_or(McError::SessionBinding(
                    crate::errors::SessionBindingError::SessionNotFound,
                ))?;

        // Check if binding has expired (ADR-0023: 30s TTL)
        if stored_binding.is_expired() {
            // Remove expired binding
            self.stored_bindings.remove(&correlation_id);
            return Err(McError::SessionBinding(
                crate::errors::SessionBindingError::TokenExpired,
            ));
        }

        // Validate binding token via HMAC-SHA256 (MAJOR-003 fix)
        let is_valid = self.binding_manager.validate_token(
            &self.meeting_id,
            &correlation_id,
            &stored_binding.participant_id,
            &stored_binding.nonce,
            &binding_token,
        );

        if !is_valid {
            warn!(
                target: "mc.actor.meeting",
                "Invalid binding token on reconnect attempt"
            );
            return Err(McError::SessionBinding(
                crate::errors::SessionBindingError::InvalidToken,
            ));
        }

        // Find participant by correlation ID
        let participant_id = self
            .correlation_to_participant
            .get(&correlation_id)
            .ok_or(McError::SessionBinding(
                crate::errors::SessionBindingError::SessionNotFound,
            ))?
            .clone();

        let participant =
            self.participants
                .get_mut(&participant_id)
                .ok_or(McError::SessionBinding(
                    crate::errors::SessionBindingError::SessionNotFound,
                ))?;

        debug!(
            target: "mc.actor.meeting",
            "Participant reconnecting"
        );

        // Create new connection actor
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ConnectionActor::spawn(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
        );

        // Store new connection
        self.connections.insert(
            connection_id.clone(),
            ManagedConnection {
                handle: conn_handle.clone(),
                task_handle: conn_task,
                participant_id: participant_id.clone(),
            },
        );

        // Update participant state
        participant.status = ParticipantStatus::Connected;
        participant.disconnected_at = None;
        participant.connection = Some(conn_handle);

        // Remove old binding and correlation mapping
        self.stored_bindings.remove(&correlation_id);
        self.correlation_to_participant.remove(&correlation_id);

        // Generate new correlation ID and binding token (rotation per ADR-0023)
        let new_correlation_id = StoredBinding::generate_correlation_id();
        let (new_binding_token, new_nonce) = self.binding_manager.generate_token(
            &self.meeting_id,
            &new_correlation_id,
            &participant_id,
        );

        // Store new binding
        let new_binding = StoredBinding::new(
            new_correlation_id.clone(),
            participant_id.clone(),
            participant.user_id.clone(),
            new_nonce,
            new_binding_token.clone(),
        );
        self.stored_bindings
            .insert(new_correlation_id.clone(), new_binding);

        // Update mapping
        self.correlation_to_participant
            .insert(new_correlation_id.clone(), participant_id.clone());
        participant.correlation_id = new_correlation_id.clone();

        self.metrics.connection_created();

        // Broadcast reconnection
        self.broadcast_update(
            &participant_id,
            ParticipantStateUpdate::Reconnected {
                participant_id: participant_id.clone(),
            },
        )
        .await;

        info!(
            target: "mc.actor.meeting",
            "Participant reconnected"
        );

        // Get current participant list
        let participants: Vec<ParticipantInfo> = self
            .participants
            .values()
            .filter(|p| p.participant_id != participant_id)
            .map(Participant::to_info)
            .collect();

        Ok(ReconnectResult {
            participant_id,
            new_correlation_id,
            new_binding_token,
            participants,
        })
    }

    /// Handle participant leaving.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_leave(&mut self, participant_id: &str) -> Result<(), McError> {
        if let Some(participant) = self.participants.remove(participant_id) {
            debug!(
                target: "mc.actor.meeting",
                "Participant leaving"
            );

            // Remove correlation and binding mappings
            self.correlation_to_participant
                .remove(&participant.correlation_id);
            self.stored_bindings.remove(&participant.correlation_id);

            // Close connection if still active
            if let Some(conn_handle) = &participant.connection {
                conn_handle.cancel();
            }

            // Decrement participant count for GC heartbeat reporting
            self.controller_metrics.decrement_participants();

            // Broadcast leave
            self.broadcast_update(
                participant_id,
                ParticipantStateUpdate::Left {
                    participant_id: participant_id.to_string(),
                    reason: LeaveReason::Voluntary,
                },
            )
            .await;

            info!(
                target: "mc.actor.meeting",
                remaining_participants = self.participants.len(),
                "Participant left"
            );

            Ok(())
        } else {
            // MINOR-002 fix: Don't include participant ID in error message
            Err(McError::ParticipantNotFound(
                "Participant not found".to_string(),
            ))
        }
    }

    /// Handle signaling message.
    async fn handle_signaling(&mut self, participant_id: &str, message: SignalingPayload) {
        // Verify participant exists
        if !self.participants.contains_key(participant_id) {
            warn!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Signaling message from unknown participant"
            );
            return;
        }

        debug!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            message_type = ?std::mem::discriminant(&message),
            "Received signaling message"
        );

        // TODO (Phase 6g): Route signaling messages appropriately
        match message {
            SignalingPayload::MuteUpdate {
                audio_muted,
                video_muted,
            } => {
                self.handle_self_mute(participant_id, audio_muted, video_muted)
                    .await;
            }
            SignalingPayload::LayoutSubscribe { .. } => {
                // TODO: Handle layout subscription
            }
            SignalingPayload::Chat { .. } => {
                // TODO: Handle chat message
            }
            SignalingPayload::Raw { .. } => {
                // TODO: Handle raw protobuf message
            }
        }
    }

    /// Get current meeting state.
    fn get_state(&self) -> MeetingState {
        MeetingState {
            meeting_id: self.meeting_id.clone(),
            participants: self
                .participants
                .values()
                .map(Participant::to_info)
                .collect(),
            fencing_generation: self.fencing_generation,
            created_at: self.created_at,
            mailbox_depth: self.mailbox.current_depth(),
            is_shutting_down: self.is_shutting_down,
        }
    }

    /// Handle self-mute update.
    async fn handle_self_mute(
        &mut self,
        participant_id: &str,
        audio_muted: bool,
        video_muted: bool,
    ) {
        // Update mute state and extract values for broadcast
        let update = if let Some(participant) = self.participants.get_mut(participant_id) {
            participant.audio_self_muted = audio_muted;
            participant.video_self_muted = video_muted;

            Some(ParticipantStateUpdate::MuteChanged {
                participant_id: participant_id.to_string(),
                audio_self_muted: participant.audio_self_muted,
                video_self_muted: participant.video_self_muted,
                audio_host_muted: participant.audio_host_muted,
                video_host_muted: participant.video_host_muted,
            })
        } else {
            None
        };

        // Broadcast mute change after releasing the mutable borrow
        if let Some(update) = update {
            self.broadcast_update(participant_id, update).await;
        }
    }

    /// Handle host mute (enforced).
    ///
    /// Only participants with host privileges can mute other participants.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_host_mute(
        &mut self,
        target_participant_id: &str,
        muted_by: &str,
        audio_muted: bool,
        video_muted: bool,
    ) -> Result<(), McError> {
        // MAJOR-002 fix: Verify muted_by has host privileges
        let is_host = self
            .participants
            .get(muted_by)
            .map(|p| p.is_host)
            .unwrap_or(false);

        if !is_host {
            warn!(
                target: "mc.actor.meeting",
                "Non-host attempted host mute operation"
            );
            return Err(McError::PermissionDenied(
                "Only hosts can mute other participants".to_string(),
            ));
        }

        // Update mute state and extract values for broadcast
        let update = if let Some(participant) = self.participants.get_mut(target_participant_id) {
            participant.audio_host_muted = audio_muted;
            participant.video_host_muted = video_muted;

            info!(
                target: "mc.actor.meeting",
                audio_muted = audio_muted,
                video_muted = video_muted,
                "Host mute applied"
            );

            Some(ParticipantStateUpdate::MuteChanged {
                participant_id: target_participant_id.to_string(),
                audio_self_muted: participant.audio_self_muted,
                video_self_muted: participant.video_self_muted,
                audio_host_muted: participant.audio_host_muted,
                video_host_muted: participant.video_host_muted,
            })
        } else {
            None
        };

        // Broadcast mute change after releasing the mutable borrow
        if let Some(update) = update {
            self.broadcast_update(target_participant_id, update).await;
            // TODO (Phase 6d): Notify MH to enforce mute
            Ok(())
        } else {
            // MINOR-002 fix: Don't include participant ID in error message
            Err(McError::ParticipantNotFound(
                "Target participant not found".to_string(),
            ))
        }
    }

    /// Handle meeting end.
    async fn handle_end_meeting(&mut self, reason: &str) -> Result<(), McError> {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            reason = %reason,
            participants = self.participants.len(),
            "Ending meeting"
        );

        self.is_shutting_down = true;

        // Notify all participants
        for participant_id in self.participants.keys().cloned().collect::<Vec<_>>() {
            self.broadcast_update(
                &participant_id,
                ParticipantStateUpdate::Left {
                    participant_id: participant_id.clone(),
                    reason: LeaveReason::MeetingEnded,
                },
            )
            .await;
        }

        // Cancel all connections
        for managed in self.connections.values() {
            managed.handle.cancel();
        }

        // Cancel self (will trigger graceful shutdown)
        self.cancel_token.cancel();

        Ok(())
    }

    /// Check for disconnect timeouts.
    async fn check_disconnect_timeouts(&mut self) {
        let now = Instant::now();
        let mut timed_out = Vec::new();

        for (participant_id, participant) in &self.participants {
            if participant.status == ParticipantStatus::Disconnected {
                if let Some(disconnected_at) = participant.disconnected_at {
                    if now.duration_since(disconnected_at) >= DISCONNECT_GRACE_PERIOD {
                        timed_out.push(participant_id.clone());
                    }
                }
            }
        }

        for participant_id in timed_out {
            info!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Disconnect grace period expired, removing participant"
            );

            if let Some(participant) = self.participants.remove(&participant_id) {
                self.correlation_to_participant
                    .remove(&participant.correlation_id);

                // Decrement participant count for GC heartbeat reporting
                self.controller_metrics.decrement_participants();

                self.broadcast_update(
                    &participant_id,
                    ParticipantStateUpdate::Left {
                        participant_id: participant_id.clone(),
                        reason: LeaveReason::Timeout,
                    },
                )
                .await;
            }
        }
    }

    /// Check health of connection actors.
    async fn check_connection_health(&mut self) {
        let mut finished = Vec::new();

        for (conn_id, managed) in &self.connections {
            if managed.task_handle.is_finished() {
                finished.push(conn_id.clone());
            }
        }

        for conn_id in finished {
            if let Some(managed) = self.connections.remove(&conn_id) {
                match managed.task_handle.await {
                    Ok(()) => {
                        debug!(
                            target: "mc.actor.meeting",
                            meeting_id = %self.meeting_id,
                            connection_id = %conn_id,
                            "Connection actor exited cleanly"
                        );
                    }
                    Err(join_error) => {
                        if join_error.is_panic() {
                            error!(
                                target: "mc.actor.meeting",
                                meeting_id = %self.meeting_id,
                                connection_id = %conn_id,
                                error = ?join_error,
                                "Connection actor panicked"
                            );
                            self.metrics.record_panic(ActorType::Connection);
                        }
                    }
                }

                // Mark participant as disconnected
                self.handle_disconnect(&conn_id, &managed.participant_id)
                    .await;
            }
        }
    }

    /// Broadcast an update to all participants except the source.
    async fn broadcast_update(&self, except_participant_id: &str, update: ParticipantStateUpdate) {
        for participant in self.participants.values() {
            if participant.participant_id != except_participant_id {
                if let Some(conn) = &participant.connection {
                    let _ = conn.send_update(update.clone()).await;
                }
            }
        }
    }

    /// Perform graceful shutdown.
    async fn graceful_shutdown(&mut self) {
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participants = self.participants.len(),
            connections = self.connections.len(),
            "Performing graceful shutdown"
        );

        self.is_shutting_down = true;

        // Cancel all connection actors
        for managed in self.connections.values() {
            managed.handle.cancel();
        }

        // Wait for connections to complete
        for (conn_id, managed) in self.connections.drain() {
            match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
                Ok(Ok(())) => {
                    debug!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        "Connection completed cleanly"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        error = ?e,
                        "Connection task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        target: "mc.actor.meeting",
                        meeting_id = %self.meeting_id,
                        connection_id = %conn_id,
                        "Connection shutdown timed out"
                    );
                }
            }
        }

        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            "Graceful shutdown complete"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Test secret for session binding (32 bytes as required by ADR-0023).
    fn test_secret() -> SecretBox<Vec<u8>> {
        SecretBox::new(Box::new(vec![0u8; 32]))
    }

    #[tokio::test]
    async fn test_meeting_actor_spawn() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-123".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        assert_eq!(handle.meeting_id(), "meeting-123");
        assert!(!handle.is_cancelled());

        handle.cancel();
        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn test_meeting_actor_join() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-join-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false, // not host
            )
            .await;

        assert!(result.is_ok());
        let join_result = result.unwrap();
        assert_eq!(join_result.participant_id, "part-1");
        assert!(!join_result.correlation_id.is_empty());
        // Binding token should be 64 hex chars (HMAC-SHA256)
        assert_eq!(join_result.binding_token.len(), 64);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_duplicate_join() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-dup-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await;
        assert!(result.is_ok());

        let result = handle
            .connection_join(
                "conn-2".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await;
        assert!(matches!(result, Err(McError::Conflict(_))));

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_get_state() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-state-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await;

        let state = handle.get_state().await;
        assert!(state.is_ok());
        let state = state.unwrap();
        assert_eq!(state.meeting_id, "meeting-state-test");
        assert_eq!(state.participants.len(), 1);
        assert!(!state.is_shutting_down);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_leave() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-leave-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await;

        // Leave
        let result = handle.participant_leave("part-1".to_string()).await;
        assert!(result.is_ok());

        // Verify empty
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 0);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_reconnect() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-reconnect-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected("conn-1".to_string(), "part-1".to_string())
            .await;

        // Reconnect with valid binding token
        let result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
            )
            .await;

        assert!(result.is_ok());
        let reconnect_result = result.unwrap();
        assert_eq!(reconnect_result.participant_id, "part-1");
        // New correlation ID should be different (rotation per ADR-0023)
        assert_ne!(
            reconnect_result.new_correlation_id,
            join_result.correlation_id
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_reconnect_invalid_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-reconnect-invalid".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected("conn-1".to_string(), "part-1".to_string())
            .await;

        // Reconnect with invalid binding token
        let result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                "invalid-token".to_string(),
            )
            .await;

        // Should fail with InvalidToken error
        assert!(matches!(
            result,
            Err(McError::SessionBinding(
                crate::errors::SessionBindingError::InvalidToken
            ))
        ));

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_self_mute() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-mute-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await;

        // Update mute
        let result = handle
            .update_self_mute("part-1".to_string(), true, false)
            .await;
        assert!(result.is_ok());

        // Check state
        let state = handle.get_state().await.unwrap();
        let participant = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .unwrap();
        assert!(participant.audio_self_muted);
        assert!(!participant.video_self_muted);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_host_mute() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-host-mute-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join host (part-1) and non-host (part-2)
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                true, // host
            )
            .await;
        let _ = handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                false, // not host
            )
            .await;

        // Host mutes part-2
        let result = handle
            .host_mute("part-2".to_string(), "part-1".to_string(), true, false)
            .await;
        assert!(result.is_ok());

        // Check state
        let state = handle.get_state().await.unwrap();
        let participant = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-2")
            .unwrap();
        assert!(participant.audio_host_muted);
        assert!(!participant.video_host_muted);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_host_mute_denied_for_non_host() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-host-mute-denied".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join two non-host participants
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false, // not host
            )
            .await;
        let _ = handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                false, // not host
            )
            .await;

        // Non-host tries to mute part-2 - should fail
        let result = handle
            .host_mute("part-2".to_string(), "part-1".to_string(), true, false)
            .await;
        assert!(matches!(result, Err(McError::PermissionDenied(_))));

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_child_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-token-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let child = handle.child_token();
        assert!(!child.is_cancelled());

        handle.cancel();

        // Give time for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(child.is_cancelled());
    }

    /// Test that the 30-second disconnect grace period expires correctly.
    /// Uses `tokio::time::pause()` to control time advancement.
    #[tokio::test(start_paused = true)]
    async fn test_disconnect_grace_period_expires() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-grace-period-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await
            .unwrap();

        assert_eq!(join_result.participant_id, "part-1");

        // Verify participant is in the meeting
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        // Disconnect the participant
        let _ = handle
            .connection_disconnected("conn-1".to_string(), "part-1".to_string())
            .await;

        // Give actor time to process the disconnect message
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify participant is disconnected but still in the meeting
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].status,
            ParticipantStatus::Disconnected
        );

        // Advance time by 29 seconds - participant should still be present
        tokio::time::advance(Duration::from_secs(29)).await;

        // Give actor time for the grace check interval to tick
        tokio::time::sleep(Duration::from_millis(10)).await;

        let state = handle.get_state().await.unwrap();
        assert_eq!(
            state.participants.len(),
            1,
            "Participant should still be present before grace period expires"
        );

        // Advance time past the 30-second grace period (total now > 30s)
        tokio::time::advance(Duration::from_secs(6)).await;

        // Wait for the grace check interval (every 5 seconds) to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify participant has been removed
        let state = handle.get_state().await.unwrap();
        assert_eq!(
            state.participants.len(),
            0,
            "Participant should be removed after grace period expires"
        );

        handle.cancel();
    }

    /// Test that reconnection within grace period preserves participant.
    #[tokio::test(start_paused = true)]
    async fn test_reconnect_within_grace_period() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = MeetingActor::spawn(
            "meeting-reconnect-grace-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant
        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                false,
            )
            .await
            .unwrap();

        // Disconnect the participant
        let _ = handle
            .connection_disconnected("conn-1".to_string(), "part-1".to_string())
            .await;

        // Give actor time to process
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Advance time by 20 seconds (within grace period)
        tokio::time::advance(Duration::from_secs(20)).await;

        // Reconnect with valid token
        let reconnect_result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
            )
            .await;

        assert!(reconnect_result.is_ok());
        let reconnect_result = reconnect_result.unwrap();
        assert_eq!(reconnect_result.participant_id, "part-1");

        // Verify participant is connected again
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        handle.cancel();
    }
}
