//! `MeetingActor` - per-meeting actor that owns meeting state (ADR-0023).
//!
//! Each `MeetingActor`:
//! - Owns all state for one meeting (participants, subscriptions, mute status)
//! - Supervises N `ParticipantActor` instances
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
use crate::media_admission::{IdentityPublicKey, MeetingKeyState, SenderId, SenderIdAllocator};

use super::messages::{
    DisconnectCause, JoinResult, LeaveReason, MeetingMessage, MeetingState, ParticipantInfo,
    ParticipantStateUpdate, ParticipantStatus, ReconnectResult, SignalingPayload,
};
use super::metrics::{ActorMetrics, ActorType, ControllerMetrics, MailboxMonitor};
use super::participant::{ParticipantActor, ParticipantActorHandle};
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
#[derive(Clone, Debug)]
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
    /// * `display_name` - Registered display name from the validated meeting-token
    ///   claim (already length-bounded at the connection boundary); empty means the
    ///   claim carried no name and a generic label is applied at the join sink
    /// * `is_host` - Whether this participant has host privileges
    /// * `identity_public_key` - The joiner's Ed25519 identity signing public
    ///   key, parsed at the WebTransport trust boundary; `None` means no key
    ///   published
    #[expect(
        clippy::too_many_arguments,
        reason = "actor join signature threads the full join tuple (ids + display_name + host flag + identity key + stream); bundling into a JoinConnectionParams struct is a larger cross-message refactor tracked in docs/TODO.md"
    )]
    pub async fn connection_join(
        &self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        display_name: String,
        is_host: bool,
        identity_public_key: Option<IdentityPublicKey>,
        stream_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>>,
    ) -> Result<JoinResult, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ConnectionJoin {
                connection_id,
                user_id,
                participant_id,
                display_name,
                is_host,
                identity_public_key,
                stream_tx,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Notify of a connection disconnect.
    ///
    /// `cause` is the transport-authenticated close classification (see
    /// [`DisconnectCause`]). A [`DisconnectCause::ClientClosed`] removes the
    /// participant from the roster immediately; the abrupt/ambiguous causes keep
    /// the ADR-0023 grace period for reconnection.
    pub async fn connection_disconnected(
        &self,
        connection_id: String,
        participant_id: String,
        cause: DisconnectCause,
    ) -> Result<(), McError> {
        self.sender
            .send(MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
                cause,
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

    /// Server-mutes a participant by meeting policy (enforced at MH ingress,
    /// ADR-0036 §5). "Host mute" is avoided as a term: it presumes a role
    /// model this system has not defined.
    pub async fn server_mute(
        &self,
        target_participant_id: String,
        muted_by: String,
        audio_muted: bool,
        video_muted: bool,
    ) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(MeetingMessage::ServerMute {
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
    /// Current participant actor handle (if connected).
    connection: Option<ParticipantActorHandle>,
    /// Connection status.
    status: ParticipantStatus,
    /// Timestamp when disconnected (for grace period).
    disconnected_at: Option<Instant>,
    /// Audio self-mute (informational).
    audio_self_muted: bool,
    /// Video self-mute (informational).
    video_self_muted: bool,
    /// Audio server-mute (enforced, ADR-0036 §5).
    audio_server_muted: bool,
    /// Video server-mute (enforced, ADR-0036 §5).
    video_server_muted: bool,
    /// Whether this participant has host privileges.
    is_host: bool,
    /// Per-meeting sender id, allocated once at admission (ADR-0036 §2, §4).
    ///
    /// Held on the participant rather than recomputed, which is what makes
    /// reconnect continuity fall out for free: a reconnecting participant is
    /// still in the roster and still holds this, so the allocator is never
    /// consulted and no id is recycled.
    sender_id: SenderId,
    /// Ed25519 identity signing public key, as presented at join. `None` means
    /// **no key published** — a defined state, not a failure.
    ///
    /// Trust on first use — no `cnf` binding. Same-keyholder consistency only,
    /// never a verified identity (ADR-0036 §4).
    identity_public_key: Option<IdentityPublicKey>,
}

impl Participant {
    fn to_info(&self) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: self.participant_id.clone(),
            user_id: self.user_id.clone(),
            display_name: self.display_name.clone(),
            audio_self_muted: self.audio_self_muted,
            video_self_muted: self.video_self_muted,
            audio_server_muted: self.audio_server_muted,
            video_server_muted: self.video_server_muted,
            status: self.status,
            sender_id: self.sender_id,
            identity_public_key: self.identity_public_key,
        }
    }
}

/// Managed connection state.
struct ManagedConnection {
    /// Handle to the participant actor.
    #[allow(dead_code)] // Used for signaling
    handle: ParticipantActorHandle,
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
    /// Handle to self, for passing to child ParticipantActors.
    self_handle: MeetingActorHandle,
    /// The meeting KEK and its generation (ADR-0036 §4).
    ///
    /// Generated at meeting-actor creation, held only here, never persisted and
    /// never logged. Dies with the actor.
    media_keys: MeetingKeyState,
    /// Monotonic, non-recycling `sender_id` allocator for this meeting.
    sender_ids: SenderIdAllocator,
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
    /// # Errors
    ///
    /// [`McError::Internal`] if the system CSPRNG cannot produce the meeting
    /// KEK. **Fail closed**: the meeting is not created. There is deliberately
    /// no fallback RNG and no default key — a predictable KEK would silently
    /// defeat every confidentiality property ADR-0036 §4 claims.
    pub fn spawn(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        Self::spawn_inner(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            SenderIdAllocator::new(),
        )
    }

    /// Shared construction for [`Self::spawn`] and the `test-seams` variant.
    ///
    /// The allocator is a parameter rather than always `SenderIdAllocator::new()`
    /// so the seam threads a pre-seeded cursor without duplicating this body —
    /// a forked copy would drift from the real construction path and the
    /// exhaustion test would stop testing production behaviour.
    fn spawn_inner(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        sender_ids: SenderIdAllocator,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        let (sender, receiver) = mpsc::channel(MEETING_CHANNEL_BUFFER);

        // Build the handle first so we can give the actor a clone of it
        let handle = MeetingActorHandle {
            sender,
            cancel_token: cancel_token.clone(),
            meeting_id: meeting_id.clone(),
        };

        // ADR-0036 §4: the meeting KEK is generated at meeting-actor creation,
        // random, held only in memory, never persisted and never logged.
        let media_keys =
            MeetingKeyState::generate(&ring::rand::SystemRandom::new()).map_err(|_| {
                // No key material in the error, and none in any log this
                // produces — the failure is that there IS no key.
                McError::Internal("Failed to initialise meeting key material".to_string())
            })?;

        crate::observability::metrics::record_meeting_kek_generated();

        // `key_custody=operator` (ADR-0036 §4, §11): media is encrypted between
        // clients; MH, transport and storage cannot read it; MC can. Never
        // described as end-to-end or zero-trust, and never a boolean.
        info!(
            target: "mc.actor.meeting",
            meeting_id = %meeting_id,
            key_custody = crate::observability::metrics::KEY_CUSTODY_OPERATOR,
            "Meeting key material provisioned"
        );

        let actor = Self {
            meeting_id: meeting_id.clone(),
            receiver,
            cancel_token,
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
            self_handle: handle.clone(),
            media_keys,
            sender_ids,
        };

        let task_handle = tokio::spawn(actor.run());

        Ok((handle, task_handle))
    }

    /// Spawn a meeting actor whose `sender_id` allocator is pre-seeded.
    /// **Test builds only.**
    ///
    /// Exists so the fail-closed exhaustion reject can be exercised through the
    /// real join path — `handle_join`, the `JoinResponse` write, and
    /// `record_session_join`'s `error_type` label — without performing 65535
    /// joins. Compiled only under the non-default `test-seams` feature, and a
    /// release build with that feature on fails to compile (see `lib.rs`).
    ///
    /// This IS the exhaustion guard's bypass. It must never be reachable in
    /// production.
    ///
    /// # Errors
    ///
    /// As [`MeetingActor::spawn`].
    #[cfg(feature = "test-seams")]
    pub fn spawn_with_sender_id_cursor(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
        next_sender_id: Option<std::num::NonZeroU16>,
    ) -> Result<(MeetingActorHandle, JoinHandle<()>), McError> {
        Self::spawn_inner(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
            SenderIdAllocator::resuming_from(next_sender_id),
        )
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
                display_name,
                is_host,
                identity_public_key,
                stream_tx,
                respond_to,
            } => {
                let result = self
                    .handle_join(
                        connection_id,
                        user_id,
                        participant_id,
                        display_name,
                        is_host,
                        identity_public_key,
                        stream_tx,
                    )
                    .await;
                let _ = respond_to.send(result);
            }

            MeetingMessage::ConnectionDisconnected {
                connection_id,
                participant_id,
                cause,
            } => {
                self.handle_disconnect(&connection_id, &participant_id, cause)
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
                self.handle_self_mute(&participant_id, audio_muted, video_muted);
            }

            MeetingMessage::ServerMute {
                target_participant_id,
                muted_by,
                audio_muted,
                video_muted,
                respond_to,
            } => {
                let result = self
                    .handle_server_mute(&target_participant_id, &muted_by, audio_muted, video_muted)
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
    #[expect(
        clippy::too_many_arguments,
        reason = "actor join signature threads the full join tuple (ids + display_name + host flag + identity key + stream); bundling into a JoinConnectionParams struct is a larger cross-message refactor tracked in docs/TODO.md"
    )]
    async fn handle_join(
        &mut self,
        connection_id: String,
        user_id: String,
        participant_id: String,
        display_name: String,
        is_host: bool,
        identity_public_key: Option<IdentityPublicKey>,
        stream_tx: Option<tokio::sync::mpsc::Sender<bytes::Bytes>>,
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

        // ADR-0036 §2/§4, invariant R-35: allocate the sender id BEFORE any
        // other admission state is built, so an exhaustion reject costs nothing
        // and leaves nothing to unwind.
        //
        // Fail closed. At the wall we REJECT the admission rather than
        // allocating: a warn-and-continue that still wrapped would silently
        // reissue a live sender_id and produce exactly the key-id collision
        // R-35 exists to prevent. The KEK-epoch reset that would reclaim the
        // namespace is deferred with all KEK rotation, so there is no in-place
        // operator remedy — the meeting must end and restart.
        let allocation = self.sender_ids.allocate().map_err(|_| {
            // OPS-8: logged HERE, not at the connection layer. This function's
            // span carries `meeting_id`; the WebTransport connection span
            // carries `connection_id` only, and the metric correctly carries no
            // meeting identifier — so without this line an operator at the wall
            // could not tell WHICH meeting exhausted.
            warn!(
                target: "mc.actor.meeting",
                admissions_total = self.sender_ids.issued(),
                meeting_age_seconds = chrono::Utc::now().timestamp() - self.created_at,
                "sender_id space exhausted; refusing admission. The namespace is consumed by \
                 cumulative lifetime admissions, not concurrent participants, so the participant \
                 cap does not bound it. No in-place remedy: the meeting must end and restart."
            );
            McError::SenderIdSpaceExhausted
        })?;
        let sender_id = allocation.sender_id;

        // `high_watermark_crossed` is the allocator's own one-shot latch (true on
        // exactly the crossing allocation and never again), so there is no
        // second guard here — one latch, unit-tested in `sender_id.rs`.
        if allocation.high_watermark_crossed {
            // Forensic, not actionable: the remedy at 90% and at 100% is
            // identical, so this exists so that after an exhaustion incident an
            // operator can reconstruct whether consumption was sudden or
            // gradual. Answerable from this one line without joining back to
            // the meeting-create record. Carries no sender_id value.
            warn!(
                target: "mc.actor.meeting",
                admissions_total = self.sender_ids.issued(),
                namespace_remaining = self.sender_ids.remaining(),
                meeting_age_seconds = chrono::Utc::now().timestamp() - self.created_at,
                "sender_id namespace past high watermark"
            );
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

        // Create participant actor with stream + meeting handle for disconnect notification
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ParticipantActor::spawn_inner(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
            stream_tx,
            Some(self.self_handle.clone()),
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

        // Prefer the token's display_name; fall back to a generic label only when
        // the claim is genuinely absent (empty). The claim was already length-bounded
        // at the connection trust boundary, so it is used verbatim here. Record the
        // resolution outcome (bounded label, never the name value) so the empty-claim
        // degradation is observable on the MC consumer side ("fail loudly").
        let display_name = if display_name.is_empty() {
            crate::observability::metrics::record_display_name_resolution("fallback");
            format!("Participant {}", self.participants.len() + 1)
        } else {
            crate::observability::metrics::record_display_name_resolution("present");
            display_name
        };
        let conn_handle_for_result = conn_handle.clone();
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
            audio_server_muted: false,
            video_server_muted: false,
            is_host,
            sender_id,
            identity_public_key,
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
            sender_id,
            // Arc clone: a handle, never a copy of the key bytes.
            meeting_kek: Arc::clone(self.media_keys.kek()),
            kek_generation: self.media_keys.generation(),
            participant_handle: conn_handle_for_result,
        })
    }

    /// Handle connection disconnect.
    ///
    /// `cause` (transport-authenticated; see [`DisconnectCause`]) selects the
    /// path:
    /// - [`DisconnectCause::ClientClosed`] — a clean tab-close: remove the
    ///   participant from the roster IMMEDIATELY and broadcast
    ///   `ParticipantLeft{Voluntary}` (the ADR-0023 grace period is skipped — a
    ///   deliberately closed session is not reconnecting).
    /// - [`DisconnectCause::ConnectionLost`] / [`DisconnectCause::ServerInitiated`]
    ///   — abrupt/ambiguous: mark the participant `Disconnected` and start the
    ///   grace period so a genuine transient disconnect can reconnect.
    ///
    /// Idempotent under the double-notify path (the `ParticipantActor` self-notify
    /// plus [`check_connection_health`](Self::check_connection_health)): once the
    /// participant is removed (`ClientClosed`) or already in grace
    /// (`ConnectionLost`/`ServerInitiated`), a second call is a no-op. Both arms
    /// gate `record_participant_disconnect` on a live→dead transition
    /// (`status != Disconnected`), so the physical drop is counted exactly once
    /// even when a racing inline `ConnectionLost` marks the participant
    /// `Disconnected` before the queued `ClientClosed` is processed — the disconnect
    /// counter is never double-incremented, and the leave counter fires once via the
    /// single `remove_and_broadcast_left` choke-point.
    async fn handle_disconnect(
        &mut self,
        connection_id: &str,
        participant_id: &str,
        cause: DisconnectCause,
    ) {
        debug!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            connection_id = %connection_id,
            cause = %cause.label(),
            "Connection disconnected"
        );

        // Remove connection (once).
        if let Some(conn) = self.connections.remove(connection_id) {
            // Wait briefly for task to complete
            let _ = tokio::time::timeout(Duration::from_millis(100), conn.task_handle).await;
            self.metrics.connection_closed();
        }

        // Look up current participant status; if already gone (removed/left),
        // there is nothing to do — keeps the double-notify path idempotent.
        let Some(status) = self.participants.get(participant_id).map(|p| p.status) else {
            return;
        };

        match cause {
            DisconnectCause::ClientClosed => {
                // Clean close — the user closed the tab. Remove immediately and
                // broadcast ParticipantLeft{Voluntary}; skip the grace period.
                //
                // Count the physical drop ONLY on a live→dead transition, matching
                // the ConnectionLost arm below. If the participant is already
                // Disconnected, a racing inline ConnectionLost (from
                // check_connection_health observing the finished conn task before
                // this queued ClientClosed is processed) already counted the drop —
                // re-counting here would double-increment
                // mc_participant_disconnects_total for one departure and skew the
                // reconnect-rate identity. We still remove + broadcast Left below
                // (skip grace); only the counter is gated.
                if status != ParticipantStatus::Disconnected {
                    crate::observability::metrics::record_participant_disconnect(cause.label());
                }
                info!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    "Clean transport close — removing participant immediately (grace skipped)"
                );
                self.remove_and_broadcast_left(participant_id, LeaveReason::Voluntary)
                    .await;
            }
            DisconnectCause::ConnectionLost | DisconnectCause::ServerInitiated => {
                // Abrupt / ambiguous loss. Start the ADR-0023 grace period so a
                // genuine transient disconnect can reconnect. Idempotent: if the
                // participant is already in grace, do nothing (no double count).
                if status == ParticipantStatus::Disconnected {
                    return;
                }
                crate::observability::metrics::record_participant_disconnect(cause.label());
                if let Some(participant) = self.participants.get_mut(participant_id) {
                    participant.status = ParticipantStatus::Disconnected;
                    participant.disconnected_at = Some(Instant::now());
                    participant.connection = None;
                }

                // Broadcast disconnect to other participants (informational; not
                // serialized to the wire — the roster is only removed on grace
                // expiry via check_disconnect_timeouts).
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
    }

    /// Remove a participant from the roster and broadcast `ParticipantLeft`.
    ///
    /// The SINGLE roster-removal choke-point (ADR-0032 metric-path completeness):
    /// every removal — clean-close, grace-timeout, explicit leave — flows through
    /// here, so `mc_participant_leaves_total{reason}` is emitted exactly once per
    /// removal and can never be broadcast-without-metric. Cleans up the
    /// correlation/binding maps, cancels any live connection, decrements the GC
    /// participant count, records the metric, then broadcasts.
    async fn remove_and_broadcast_left(&mut self, participant_id: &str, reason: LeaveReason) {
        let Some(participant) = self.participants.remove(participant_id) else {
            return;
        };

        // Remove correlation and binding mappings (session recovery cleanup).
        self.correlation_to_participant
            .remove(&participant.correlation_id);
        self.stored_bindings.remove(&participant.correlation_id);

        // Cancel the connection if still active.
        if let Some(conn_handle) = &participant.connection {
            conn_handle.cancel();
        }

        // Decrement participant count for GC heartbeat reporting.
        self.controller_metrics.decrement_participants();

        // Record the leave (bounded reason label) at the single choke-point.
        crate::observability::metrics::record_participant_leave(reason.label());

        // Broadcast leave to the remaining participants.
        self.broadcast_update(
            participant_id,
            ParticipantStateUpdate::Left {
                participant_id: participant_id.to_string(),
                reason,
            },
        )
        .await;
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

        // Create new participant actor with meeting handle for disconnect notification
        let connection_token = self.cancel_token.child_token();
        let (conn_handle, conn_task) = ParticipantActor::spawn_with_meeting(
            connection_id.clone(),
            participant_id.clone(),
            self.meeting_id.clone(),
            connection_token,
            Arc::clone(&self.metrics),
            self.self_handle.clone(),
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
        if self.participants.contains_key(participant_id) {
            debug!(
                target: "mc.actor.meeting",
                "Participant leaving"
            );

            // Explicit voluntary leave — remove via the single choke-point
            // (emits mc_participant_leaves_total{reason="voluntary"}).
            self.remove_and_broadcast_left(participant_id, LeaveReason::Voluntary)
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
                self.handle_self_mute(participant_id, audio_muted, video_muted);
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

    /// Handle self-mute update — records the reported state and does NOT fan out.
    ///
    /// # Why there is no `broadcast_update` here
    ///
    /// `MuteChanged` has no consumer.
    /// `webtransport::handler::encode_participant_update` returns `None` for it
    /// and the roster `Participant` proto carries no mute field, so a broadcast
    /// from this path delivers **zero bytes to zero clients**: it would clone the
    /// update N times and await N `send_update`s *on this actor's own task*,
    /// head-of-line-blocking joins, leaves and every other connection's
    /// `get_state()`, so that N participant actors could each wake, call the
    /// encoder, get `None` and emit a DEBUG line.
    ///
    /// That cost was latent while nothing client-driven reached here. The
    /// client-facing `MuteRequest` dispatch arm (ADR-0036 §5) makes this path
    /// client-drivable, and a per-connection rate limit cannot bound a
    /// **per-meeting** resource: N connections each within their own limit still
    /// aggregate to O(N) awaited sends per report on one shared task, and a
    /// buggy client *release* drives all N simultaneously. Removing the producer
    /// is structural; rate-limiting it would only change the constant.
    ///
    /// The subscriber-visible mute signal is `SLOT_STATE_SOURCE_MUTED`, which
    /// each subscriber composes from its **own** `get_state()` read on its own
    /// task (`webtransport::connection::compose_and_emit`). Nothing is lost.
    ///
    /// # Retirement condition
    ///
    /// If `MuteChanged` ever becomes wire-serialized, the broadcast must come
    /// back — and it needs a **per-meeting** bound at that point, not the
    /// per-connection `ClientWorkLimiter` on the mute path, for the reason above.
    ///
    /// `handle_server_mute` deliberately still broadcasts: it is not
    /// client-drivable in this story (enforcement is story 2), so it is not this
    /// diff's amplifier to remove.
    fn handle_self_mute(&mut self, participant_id: &str, audio_muted: bool, video_muted: bool) {
        let Some(participant) = self.participants.get_mut(participant_id) else {
            return;
        };

        // IDEMPOTENT: a report that changes neither flag is a no-op. Retained
        // now that the fan-out is gone because it still states the contract —
        // an unchanged pair is not a transition — and keeps the write off the
        // hot path for a client repeating one state.
        if participant.audio_self_muted == audio_muted
            && participant.video_self_muted == video_muted
        {
            return;
        }

        participant.audio_self_muted = audio_muted;
        participant.video_self_muted = video_muted;
    }

    /// Handle a server-mute request (enforced, ADR-0036 §5).
    ///
    /// Only participants with host privileges can mute other participants.
    #[instrument(skip_all, fields(meeting_id = %self.meeting_id))]
    async fn handle_server_mute(
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
                "Non-host attempted server-mute operation"
            );
            return Err(McError::PermissionDenied(
                "Only hosts can mute other participants".to_string(),
            ));
        }

        // Update mute state and extract values for broadcast
        let update = if let Some(participant) = self.participants.get_mut(target_participant_id) {
            participant.audio_server_muted = audio_muted;
            participant.video_server_muted = video_muted;

            info!(
                target: "mc.actor.meeting",
                audio_muted = audio_muted,
                video_muted = video_muted,
                "Server mute applied"
            );

            Some(ParticipantStateUpdate::MuteChanged {
                participant_id: target_participant_id.to_string(),
                audio_self_muted: participant.audio_self_muted,
                video_self_muted: participant.video_self_muted,
                audio_server_muted: participant.audio_server_muted,
                video_server_muted: participant.video_server_muted,
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

        // Notify all participants. The meeting is being torn down, so the roster
        // maps are dropped wholesale rather than removed one-by-one; emit the
        // leave metric per participant so meeting-end departures are counted like
        // every other removal (ADR-0032 metric-path completeness).
        for participant_id in self.participants.keys().cloned().collect::<Vec<_>>() {
            crate::observability::metrics::record_participant_leave(
                LeaveReason::MeetingEnded.label(),
            );
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

            // Grace expired — remove via the single choke-point
            // (emits mc_participant_leaves_total{reason="timeout"}).
            self.remove_and_broadcast_left(&participant_id, LeaveReason::Timeout)
                .await;
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
                            self.metrics.record_panic(ActorType::Participant);
                        }
                    }
                }

                // Mark participant as disconnected. A task that finished/panicked
                // without a transport-classified clean close is an abrupt loss →
                // ConnectionLost (grace-preserving). If the ParticipantActor's own
                // exit notification (which carries the authoritative cause) already
                // ran, this call is an idempotent no-op.
                self.handle_disconnect(
                    &conn_id,
                    &managed.participant_id,
                    DisconnectCause::ConnectionLost,
                )
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

    /// Spawn a meeting actor, panicking if KEK generation fails.
    ///
    /// `MeetingActor::spawn` is fallible because it generates the meeting KEK
    /// from the system CSPRNG and fails closed if that fails (ADR-0036 §4).
    /// Tests treat that as unreachable rather than threading a `Result`.
    fn must_spawn(
        meeting_id: String,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        controller_metrics: Arc<ControllerMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> (MeetingActorHandle, JoinHandle<()>) {
        MeetingActor::spawn(
            meeting_id,
            cancel_token,
            metrics,
            controller_metrics,
            master_secret,
        )
        .expect("system CSPRNG must be available in tests")
    }

    /// A syntactically valid identity key for in-crate actor tests.
    ///
    /// Exact-length only — MC does no curve validation, so a fixed pattern is
    /// sufficient. Accepting it asserts nothing about identity: with no `cnf`
    /// binding this is trust-on-first-use, giving same-keyholder consistency
    /// and never a verified identity.
    fn test_identity_key() -> Option<IdentityPublicKey> {
        crate::media_admission::fixtures::sample_identity_key()
    }

    #[tokio::test]
    async fn test_meeting_actor_spawn() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false, // not host
                test_identity_key(),
                None,
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await;
        assert!(result.is_ok());

        let result = handle
            .connection_join(
                "conn-2".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
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

    /// The registered display_name from the (already-validated, already-bounded)
    /// token claim must flow through the join path onto the participant's own
    /// roster entry — keyed by participant, NOT derived from join order/count.
    /// Distinct names make this discriminating: a position/count-derived label
    /// (the MINOR-003 stopgap) would fail here.
    #[tokio::test]
    async fn test_handle_join_uses_claim_display_name_per_participant() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-name-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                "Alice".to_string(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .expect("join 1");
        handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                "Bob".to_string(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .expect("join 2");

        let state = handle.get_state().await.unwrap();
        let alice = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .expect("part-1 present");
        let bob = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-2")
            .expect("part-2 present");
        assert_eq!(
            alice.display_name, "Alice",
            "each participant must render its OWN registered name"
        );
        assert_eq!(bob.display_name, "Bob");

        handle.cancel();
    }

    /// Genuine-absence fallback: an EMPTY claim display_name (the only case the
    /// generic label is allowed) yields the `Participant N` stopgap. Exact match,
    /// not `contains`, so the fallback shape is pinned.
    #[tokio::test]
    async fn test_handle_join_empty_claim_falls_back_to_generic_label() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-fallback-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(), // genuine absence → generic fallback
                false,
                test_identity_key(),
                None,
            )
            .await
            .expect("join");

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].display_name, "Participant 1",
            "empty claim must fall back to the generic 'Participant N' label"
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_leave() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
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

    /// `sender_id` continuity across an ADR-0023 reconnect.
    ///
    /// # What this exercises, and what it does NOT prove about production
    ///
    /// This drives the **actor-level** `handle_reconnect`. That path is not yet
    /// wired to the WebTransport accept path — `connection_reconnect` has no
    /// caller outside `actors/` — so it is **not production-reachable today**;
    /// a browser "reconnect" is currently a fresh join with a fresh
    /// `participant_id` and therefore a NEW `sender_id`. That is correct
    /// (non-recycling), just not continuity. Continuity is proven here at the
    /// actor seam, and the test is named for that rather than for a shipped
    /// guarantee.
    ///
    /// # Continuity is not recycling
    ///
    /// Keeping an id across a reconnect does not violate R-35: the participant
    /// never left the roster, so the id was never released and the allocator is
    /// never consulted. The continuity key is the ADR-0023 `correlation_id`
    /// plus its HMAC `binding_token` — never a client-supplied participant id,
    /// which a caller could forge to capture someone else's `sender_id`.
    #[tokio::test]
    async fn sender_id_continuity_across_actor_level_reconnect() {
        let (handle, _task) = must_spawn(
            "meeting-sender-id-continuity".to_string(),
            CancellationToken::new(),
            ActorMetrics::new(),
            ControllerMetrics::new(),
            test_secret(),
        );

        let join = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .expect("join succeeds");
        let original_sender_id = join.sender_id;

        // Mark the connection lost so the participant enters the ADR-0023 grace
        // period rather than being removed.
        handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await
            .expect("disconnect notification delivered");

        handle
            .connection_reconnect(
                "conn-2".to_string(),
                join.correlation_id.clone(),
                join.binding_token.clone(),
            )
            .await
            .expect("reconnect within grace succeeds");

        let state = handle.get_state().await.expect("get_state");
        let entry = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-1")
            .expect("participant survived the reconnect");

        assert_eq!(
            entry.sender_id, original_sender_id,
            "a reconnecting participant keeps its sender_id: it never left the roster, so the id \
             was never released. Issuing a new one here would burn namespace on every transient \
             network blip."
        );
    }

    #[tokio::test]
    async fn test_meeting_actor_reconnect() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .unwrap();

        // Disconnect
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
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

    /// NO self-mute report fans out to the other participants — not a no-op,
    /// and not a real transition either.
    ///
    /// # Why this is asserted through the actor message counter
    ///
    /// `MuteChanged` is not wire-serialized
    /// (`webtransport::handler::encode_participant_update` returns `None` for
    /// it), so there is no outbound frame to watch for. What is being pinned is
    /// the absence of the O(N) `broadcast_update` — one message DELIVERED to
    /// every other participant's actor — and that is exactly what
    /// `ActorMetrics::total_messages_processed` counts. A dedicated
    /// `ActorMetrics` per test means the meeting and its two participant actors
    /// are the only contributors.
    ///
    /// Expected delta is **1 per report** — the meeting actor's own
    /// `UpdateSelfMute`, which must stop there — for the no-op AND for the real
    /// transition. Restoring `broadcast_update` in `handle_self_mute` makes the
    /// transition cost 2 and fails this test.
    ///
    /// # The positive control is the state write, not a second broadcast
    ///
    /// Asserting only "the counter did not move" would pass just as well if the
    /// actor had ignored the message entirely, or if the harness were broken —
    /// the vacuous-control shape this devloop hit three times. So the real
    /// transition is confirmed to have LANDED, via `get_state()` observing
    /// `audio_self_muted`, on the same read path
    /// `webtransport::connection::compose_and_emit` uses to derive
    /// `SLOT_STATE_SOURCE_MUTED`. That is the mechanism that replaced the
    /// broadcast, so the control exercises the actual delivery route.
    ///
    /// The connection-side `last_reported_mute` cache short-circuits BEFORE the
    /// actor hop, so no integration test can reach the no-op branch; a redundant
    /// first report after join can, and does here.
    #[tokio::test]
    async fn no_self_mute_report_is_broadcast_to_other_participants() {
        use std::sync::atomic::Ordering;

        let metrics = ActorMetrics::new();
        let cancel_token = CancellationToken::new();
        let (handle, _task) = must_spawn(
            "meeting-mute-idempotence".to_string(),
            cancel_token.clone(),
            Arc::clone(&metrics),
            ControllerMetrics::new(),
            test_secret(),
        );

        let (tx_a, _rx_a) = tokio::sync::mpsc::channel(16);
        let (tx_b, _rx_b) = tokio::sync::mpsc::channel(16);
        for (conn, part, tx) in [("conn-a", "part-a", tx_a), ("conn-b", "part-b", tx_b)] {
            handle
                .connection_join(
                    conn.to_string(),
                    format!("user-{part}"),
                    part.to_string(),
                    String::new(),
                    false,
                    test_identity_key(),
                    Some(tx),
                )
                .await
                .expect("join succeeds");
        }
        // Let the join fan-out drain before taking the baseline.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let base = metrics.total_messages_processed.load(Ordering::Relaxed);

        // Both flags already false at join, so this changes nothing. It must
        // reach the meeting actor and stop there.
        handle
            .update_self_mute("part-a".to_string(), false, false)
            .await
            .expect("send succeeds");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert_eq!(
            metrics.total_messages_processed.load(Ordering::Relaxed) - base,
            1,
            "a no-op self-mute must not be broadcast: broadcasting a MuteChanged for a \
             transition that did not occur states a change on the wire that never happened, \
             and it is the cheapest client-driven O(N) fan-out in MC"
        );

        // A REAL transition must not fan out either: `MuteChanged` has no
        // consumer, so the broadcast would deliver zero bytes to zero clients
        // while awaiting N sends on the shared meeting-actor task.
        handle
            .update_self_mute("part-a".to_string(), true, false)
            .await
            .expect("send succeeds");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert_eq!(
            metrics.total_messages_processed.load(Ordering::Relaxed) - base,
            2,
            "a real mute transition must reach the meeting actor and STOP there: \
             `MuteChanged` is not wire-serialized, so a fan-out here delivers nothing \
             to anyone while awaiting one send per participant on the shared actor task"
        );

        // POSITIVE CONTROL — the transition actually landed. Without this the
        // assertions above would pass just as well if the actor had ignored
        // both messages. This is also the path that REPLACED the broadcast:
        // each subscriber derives `SLOT_STATE_SOURCE_MUTED` from its own
        // `get_state()` read, so the control exercises the real delivery route.
        let state = handle.get_state().await.expect("state read succeeds");
        let part_a = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-a")
            .expect("part-a is on the roster");
        assert!(
            part_a.audio_self_muted,
            "the mute transition must be observable via get_state, which is how \
             subscribers learn about it now that there is no broadcast"
        );
        assert!(
            !part_a.video_self_muted,
            "only the audio flag was reported; video must not have moved"
        );

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_self_mute() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
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
    async fn test_meeting_actor_server_mute() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-server-mute-test".to_string(),
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
                String::new(),
                true, // host
                test_identity_key(),
                None,
            )
            .await;
        let _ = handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                String::new(),
                false, // not host
                test_identity_key(),
                None,
            )
            .await;

        // Host-privileged participant server-mutes part-2
        let result = handle
            .server_mute("part-2".to_string(), "part-1".to_string(), true, false)
            .await;
        assert!(result.is_ok());

        // Check state
        let state = handle.get_state().await.unwrap();
        let participant = state
            .participants
            .iter()
            .find(|p| p.participant_id == "part-2")
            .unwrap();
        assert!(participant.audio_server_muted);
        assert!(!participant.video_server_muted);

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_server_mute_denied_for_non_host() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-server-mute-denied".to_string(),
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
                String::new(),
                false, // not host
                test_identity_key(),
                None,
            )
            .await;
        let _ = handle
            .connection_join(
                "conn-2".to_string(),
                "user-2".to_string(),
                "part-2".to_string(),
                String::new(),
                false, // not host
                test_identity_key(),
                None,
            )
            .await;

        // Non-host tries to mute part-2 - should fail
        let result = handle
            .server_mute("part-2".to_string(), "part-1".to_string(), true, false)
            .await;
        assert!(matches!(result, Err(McError::PermissionDenied(_))));

        handle.cancel();
    }

    #[tokio::test]
    async fn test_meeting_actor_child_token() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
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
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
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

        let (handle, _task) = must_spawn(
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
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .unwrap();

        // Disconnect the participant
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
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

    /// Task #64: a CLEAN transport close (`DisconnectCause::ClientClosed`) removes
    /// the participant from the roster IMMEDIATELY — the 30s grace period is
    /// SKIPPED. Uses `start_paused`: the participant is gone without any time
    /// being advanced past the grace window, which pins "grace skipped".
    #[tokio::test(start_paused = true)]
    async fn test_clean_close_skips_grace_removes_immediately() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-clean-close-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        // Join a participant.
        let _ = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .unwrap();

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);

        // Clean transport close — user closed the tab.
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ClientClosed,
            )
            .await;

        // Give the actor time to process the disconnect message. NOTE: we do NOT
        // advance past the 30s grace window — this small settle is only for
        // mailbox processing under start_paused.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Participant is GONE already (grace skipped) — not merely Disconnected.
        let state = handle.get_state().await.unwrap();
        assert_eq!(
            state.participants.len(),
            0,
            "clean close must remove the participant immediately, skipping grace"
        );

        handle.cancel();
    }

    /// Task #64: an ABRUPT loss (`DisconnectCause::ConnectionLost`) must PRESERVE
    /// the ADR-0023 grace period — the participant stays visible (Disconnected)
    /// and a reconnect within grace still succeeds. Guards against the skip-grace
    /// change regressing session recovery.
    #[tokio::test(start_paused = true)]
    async fn test_connection_lost_preserves_grace_and_allows_reconnect() {
        let metrics = ActorMetrics::new();
        let controller_metrics = ControllerMetrics::new();
        let cancel_token = CancellationToken::new();

        let (handle, _task) = must_spawn(
            "meeting-conn-lost-grace-test".to_string(),
            cancel_token.clone(),
            metrics,
            controller_metrics,
            test_secret(),
        );

        let join_result = handle
            .connection_join(
                "conn-1".to_string(),
                "user-1".to_string(),
                "part-1".to_string(),
                String::new(),
                false,
                test_identity_key(),
                None,
            )
            .await
            .unwrap();

        // Abrupt loss — keep grace.
        let _ = handle
            .connection_disconnected(
                "conn-1".to_string(),
                "part-1".to_string(),
                DisconnectCause::ConnectionLost,
            )
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Still present, but Disconnected (grace running) — NOT removed.
        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(
            state.participants[0].status,
            ParticipantStatus::Disconnected
        );

        // Reconnect within grace still works (ADR-0023 preserved).
        tokio::time::advance(Duration::from_secs(10)).await;
        let reconnect_result = handle
            .connection_reconnect(
                "conn-2".to_string(),
                join_result.correlation_id.clone(),
                join_result.binding_token.clone(),
            )
            .await;
        assert!(reconnect_result.is_ok());

        let state = handle.get_state().await.unwrap();
        assert_eq!(state.participants.len(), 1);
        assert_eq!(state.participants[0].status, ParticipantStatus::Connected);

        handle.cancel();
    }
}
