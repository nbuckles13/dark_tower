//! `MeetingControllerActor` - singleton supervisor for meeting actors (ADR-0023).
//!
//! The `MeetingControllerActor` is the top-level actor in the MC hierarchy:
//!
//! - Singleton per MC instance
//! - Supervises N `MeetingActor` instances
//! - Handles meeting creation/removal
//! - Owns the root `CancellationToken` for graceful shutdown
//! - Monitors child actor health (panic detection via `JoinHandle`)
//!
//! # Graceful Shutdown
//!
//! On SIGTERM, the controller:
//! 1. Sets `accepting_new = false`
//! 2. Cancels the root `CancellationToken` (propagates to all children)
//! 3. Waits for meetings to drain or migrate
//! 4. Reports completion to GC

use crate::errors::McError;

use super::meeting::{MeetingActor, MeetingActorHandle};
use super::messages::{ControllerMessage, ControllerStatus, MeetingInfo};
use super::metrics::{ActorMetrics, ActorType, MailboxMonitor};

use common::secret::{ExposeSecret, SecretBox};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Default channel buffer size for the controller mailbox.
const CONTROLLER_CHANNEL_BUFFER: usize = 1000;

/// Handle to the `MeetingControllerActor`.
///
/// This is the public interface for interacting with the controller.
/// All methods are async and return results via oneshot channels.
#[derive(Clone)]
pub struct MeetingControllerActorHandle {
    sender: mpsc::Sender<ControllerMessage>,
    cancel_token: CancellationToken,
}

impl MeetingControllerActorHandle {
    /// Create a new `MeetingControllerActor` and return a handle to it.
    ///
    /// This spawns the actor task and returns immediately.
    ///
    /// # Arguments
    ///
    /// * `mc_id` - MC instance ID
    /// * `metrics` - Shared actor metrics
    /// * `master_secret` - Master secret for session binding tokens (must be >= 32 bytes).
    ///   Wrapped in SecretBox to ensure secure memory handling (zeroization on drop,
    ///   redacted Debug output).
    #[must_use]
    pub fn new(
        mc_id: String,
        metrics: Arc<ActorMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(CONTROLLER_CHANNEL_BUFFER);
        let cancel_token = CancellationToken::new();

        let actor = MeetingControllerActor::new(
            mc_id,
            receiver,
            cancel_token.clone(),
            Arc::clone(&metrics),
            master_secret,
        );

        tokio::spawn(actor.run());

        Self {
            sender,
            cancel_token,
        }
    }

    /// Create a new meeting.
    ///
    /// Returns `Ok(())` if the meeting was created, or an error if creation failed.
    pub async fn create_meeting(&self, meeting_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::CreateMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Get information about an existing meeting.
    pub async fn get_meeting(&self, meeting_id: String) -> Result<MeetingInfo, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::GetMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Remove a meeting (called when all participants leave).
    pub async fn remove_meeting(&self, meeting_id: String) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::RemoveMeeting {
                meeting_id,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Get the current controller status.
    pub async fn get_status(&self) -> Result<ControllerStatus, McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::GetStatus { respond_to: tx })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))
    }

    /// Initiate graceful shutdown.
    pub async fn shutdown(&self, deadline: Duration) -> Result<(), McError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(ControllerMessage::Shutdown {
                deadline,
                respond_to: tx,
            })
            .await
            .map_err(|e| McError::Internal(format!("channel send failed: {e}")))?;

        rx.await
            .map_err(|e| McError::Internal(format!("response receive failed: {e}")))?
    }

    /// Cancel the actor (for immediate shutdown).
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the actor is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Get a child token for spawning child actors.
    #[must_use]
    pub fn child_token(&self) -> CancellationToken {
        self.cancel_token.child_token()
    }
}

/// Internal state for a managed meeting.
struct ManagedMeeting {
    /// Handle to the meeting actor.
    handle: MeetingActorHandle,
    /// Join handle for monitoring the actor task.
    task_handle: JoinHandle<()>,
    /// Meeting creation timestamp.
    created_at: i64,
}

/// The `MeetingControllerActor` implementation.
///
/// This struct owns the actor state and runs the message loop.
pub struct MeetingControllerActor {
    /// MC instance ID.
    mc_id: String,
    /// Message receiver.
    receiver: mpsc::Receiver<ControllerMessage>,
    /// Cancellation token (root).
    cancel_token: CancellationToken,
    /// Managed meetings by ID.
    meetings: HashMap<String, ManagedMeeting>,
    /// Whether the controller is accepting new meetings.
    accepting_new: bool,
    /// Shared metrics.
    metrics: Arc<ActorMetrics>,
    /// Mailbox monitor.
    mailbox: MailboxMonitor,
    /// Master secret for session binding tokens (ADR-0023).
    /// Wrapped in SecretBox to ensure secure memory handling.
    master_secret: SecretBox<Vec<u8>>,
}

impl MeetingControllerActor {
    /// Create a new controller actor (not started).
    ///
    /// # Arguments
    ///
    /// * `mc_id` - MC instance ID
    /// * `receiver` - Message receiver channel
    /// * `cancel_token` - Root cancellation token
    /// * `metrics` - Shared actor metrics
    /// * `master_secret` - Master secret for session binding tokens (must be >= 32 bytes).
    ///   Wrapped in SecretBox to ensure secure memory handling.
    fn new(
        mc_id: String,
        receiver: mpsc::Receiver<ControllerMessage>,
        cancel_token: CancellationToken,
        metrics: Arc<ActorMetrics>,
        master_secret: SecretBox<Vec<u8>>,
    ) -> Self {
        let mailbox = MailboxMonitor::new(ActorType::Controller, &mc_id);

        Self {
            mc_id,
            receiver,
            cancel_token,
            meetings: HashMap::new(),
            accepting_new: true,
            metrics,
            mailbox,
            master_secret,
        }
    }

    /// Run the actor message loop.
    #[instrument(skip_all, name = "mc.actor.controller", fields(mc_id = %self.mc_id))]
    async fn run(mut self) {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            "MeetingControllerActor started"
        );

        loop {
            // Check for terminated meeting actors
            self.check_meeting_health().await;

            tokio::select! {
                // Handle cancellation
                () = self.cancel_token.cancelled() => {
                    info!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        "MeetingControllerActor received cancellation signal"
                    );
                    self.graceful_shutdown().await;
                    break;
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
                            // Channel closed, exit
                            info!(
                                target: "mc.actor.controller",
                                mc_id = %self.mc_id,
                                "MeetingControllerActor channel closed, exiting"
                            );
                            break;
                        }
                    }
                }
            }
        }

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meetings_remaining = self.meetings.len(),
            messages_processed = self.mailbox.messages_processed(),
            "MeetingControllerActor stopped"
        );
    }

    /// Handle a single message.
    async fn handle_message(&mut self, message: ControllerMessage) {
        match message {
            ControllerMessage::CreateMeeting {
                meeting_id,
                respond_to,
            } => {
                let result = self.create_meeting(meeting_id).await;
                let _ = respond_to.send(result);
            }

            ControllerMessage::GetMeeting {
                meeting_id,
                respond_to,
            } => {
                let result = self.get_meeting(&meeting_id).await;
                let _ = respond_to.send(result);
            }

            ControllerMessage::RemoveMeeting {
                meeting_id,
                respond_to,
            } => {
                let result = self.remove_meeting(&meeting_id).await;
                let _ = respond_to.send(result);
            }

            ControllerMessage::GetStatus { respond_to } => {
                let status = self.get_status();
                let _ = respond_to.send(status);
            }

            ControllerMessage::Shutdown {
                deadline,
                respond_to,
            } => {
                let result = self.initiate_shutdown(deadline).await;
                let _ = respond_to.send(result);
            }
        }
    }

    /// Create a new meeting actor.
    async fn create_meeting(&mut self, meeting_id: String) -> Result<(), McError> {
        // Check if we're accepting new meetings
        if !self.accepting_new {
            return Err(McError::Draining);
        }

        // Check if meeting already exists (MINOR-001: don't include meeting_id in error)
        if self.meetings.contains_key(&meeting_id) {
            return Err(McError::Conflict("Meeting already exists".to_string()));
        }

        debug!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            "Creating new meeting actor"
        );

        // Create child token for the meeting
        let meeting_token = self.cancel_token.child_token();

        // Create the meeting actor (with master_secret for session binding tokens)
        // Create a new SecretBox from the exposed secret bytes for each meeting
        let meeting_secret = SecretBox::new(Box::new(self.master_secret.expose_secret().clone()));
        let (handle, task_handle) = MeetingActor::spawn(
            meeting_id.clone(),
            meeting_token,
            Arc::clone(&self.metrics),
            meeting_secret,
        );

        let created_at = chrono::Utc::now().timestamp();

        self.meetings.insert(
            meeting_id.clone(),
            ManagedMeeting {
                handle,
                task_handle,
                created_at,
            },
        );

        self.metrics.meeting_created();

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_id = %meeting_id,
            total_meetings = self.meetings.len(),
            "Meeting actor created"
        );

        Ok(())
    }

    /// Get information about a meeting.
    ///
    /// Queries the `MeetingActor` to get the actual participant count and fencing generation.
    async fn get_meeting(&self, meeting_id: &str) -> Result<MeetingInfo, McError> {
        match self.meetings.get(meeting_id) {
            Some(managed) => {
                // Query the meeting actor to get actual participant count and state
                match managed.handle.get_state().await {
                    Ok(state) => Ok(MeetingInfo {
                        meeting_id: meeting_id.to_string(),
                        participant_count: state.participants.len(),
                        created_at: managed.created_at,
                        fencing_generation: state.fencing_generation,
                    }),
                    Err(_) => {
                        // Meeting actor may have shut down - return cached info
                        warn!(
                            target: "mc.actor.controller",
                            mc_id = %self.mc_id,
                            meeting_id = %meeting_id,
                            "Failed to query meeting actor state, returning cached info"
                        );
                        Ok(MeetingInfo {
                            meeting_id: meeting_id.to_string(),
                            participant_count: 0,
                            created_at: managed.created_at,
                            fencing_generation: 0,
                        })
                    }
                }
            }
            None => Err(McError::MeetingNotFound(meeting_id.to_string())),
        }
    }

    /// Remove a meeting.
    ///
    /// This method initiates meeting removal but does not block waiting for
    /// the meeting actor task to complete. The cleanup is spawned as a
    /// background task to avoid blocking the message loop (ADR-0023).
    async fn remove_meeting(&mut self, meeting_id: &str) -> Result<(), McError> {
        match self.meetings.remove(meeting_id) {
            Some(managed) => {
                debug!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    "Removing meeting actor"
                );

                // Cancel the meeting actor
                managed.handle.cancel();

                // Spawn background task to wait for cleanup - don't block the message loop
                let meeting_id_owned = meeting_id.to_string();
                let mc_id = self.mc_id.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(Duration::from_secs(5), managed.task_handle).await {
                        Ok(Ok(())) => {
                            debug!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                "Meeting actor task completed cleanly"
                            );
                        }
                        Ok(Err(e)) => {
                            warn!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                error = ?e,
                                "Meeting actor task panicked during removal"
                            );
                        }
                        Err(_) => {
                            warn!(
                                target: "mc.actor.controller",
                                mc_id = %mc_id,
                                meeting_id = %meeting_id_owned,
                                "Meeting actor task cleanup timed out"
                            );
                        }
                    }
                });

                self.metrics.meeting_removed();

                info!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    total_meetings = self.meetings.len(),
                    "Meeting actor removed"
                );

                Ok(())
            }
            None => Err(McError::MeetingNotFound(meeting_id.to_string())),
        }
    }

    /// Get current controller status.
    fn get_status(&self) -> ControllerStatus {
        ControllerStatus {
            meeting_count: self.meetings.len(),
            connection_count: self.metrics.connection_count(),
            is_draining: !self.accepting_new,
            mailbox_depth: self.mailbox.current_depth(),
        }
    }

    /// Initiate graceful shutdown.
    async fn initiate_shutdown(&mut self, _deadline: Duration) -> Result<(), McError> {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_count = self.meetings.len(),
            "Initiating graceful shutdown"
        );

        // Stop accepting new meetings
        self.accepting_new = false;

        // Cancel the root token (propagates to all children)
        self.cancel_token.cancel();

        Ok(())
    }

    /// Perform graceful shutdown.
    async fn graceful_shutdown(&mut self) {
        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            meeting_count = self.meetings.len(),
            "Performing graceful shutdown"
        );

        // Stop accepting new meetings
        self.accepting_new = false;

        // Cancel all meeting actors (already done via parent token, but be explicit)
        for (meeting_id, managed) in &self.meetings {
            debug!(
                target: "mc.actor.controller",
                mc_id = %self.mc_id,
                meeting_id = %meeting_id,
                "Cancelling meeting actor"
            );
            managed.handle.cancel();
        }

        // Wait for all meeting tasks to complete
        for (meeting_id, managed) in self.meetings.drain() {
            match tokio::time::timeout(Duration::from_secs(30), managed.task_handle).await {
                Ok(Ok(())) => {
                    debug!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        "Meeting actor completed cleanly"
                    );
                }
                Ok(Err(e)) => {
                    warn!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        error = ?e,
                        "Meeting actor task panicked during shutdown"
                    );
                }
                Err(_) => {
                    warn!(
                        target: "mc.actor.controller",
                        mc_id = %self.mc_id,
                        meeting_id = %meeting_id,
                        "Meeting actor shutdown timed out"
                    );
                }
            }
        }

        info!(
            target: "mc.actor.controller",
            mc_id = %self.mc_id,
            "Graceful shutdown complete"
        );
    }

    /// Check health of managed meeting actors.
    async fn check_meeting_health(&mut self) {
        let mut failed_meetings = Vec::new();

        for (meeting_id, managed) in &self.meetings {
            if managed.task_handle.is_finished() {
                warn!(
                    target: "mc.actor.controller",
                    mc_id = %self.mc_id,
                    meeting_id = %meeting_id,
                    "Meeting actor task finished unexpectedly"
                );
                failed_meetings.push(meeting_id.clone());
            }
        }

        // Handle failed meetings
        for meeting_id in failed_meetings {
            if let Some(managed) = self.meetings.remove(&meeting_id) {
                // Check if it was a panic
                match managed.task_handle.await {
                    Ok(()) => {
                        // Clean exit, meeting ended naturally
                        info!(
                            target: "mc.actor.controller",
                            mc_id = %self.mc_id,
                            meeting_id = %meeting_id,
                            "Meeting actor exited cleanly"
                        );
                    }
                    Err(join_error) => {
                        // Panic or cancellation
                        if join_error.is_panic() {
                            error!(
                                target: "mc.actor.controller",
                                mc_id = %self.mc_id,
                                meeting_id = %meeting_id,
                                error = ?join_error,
                                "Meeting actor panicked - triggering investigation"
                            );
                            self.metrics.record_panic(ActorType::Meeting);

                            // TODO (Phase 6e): Trigger meeting migration to another MC
                        }
                    }
                }

                self.metrics.meeting_removed();
            }
        }
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
    async fn test_controller_handle_create_meeting() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-001".to_string(), metrics, test_secret());

        // Create a meeting
        let result = handle.create_meeting("meeting-123".to_string()).await;
        assert!(result.is_ok());

        // Get the meeting
        let info = handle.get_meeting("meeting-123".to_string()).await;
        assert!(info.is_ok());
        let info = info.unwrap();
        assert_eq!(info.meeting_id, "meeting-123");

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_duplicate_meeting() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-002".to_string(), metrics, test_secret());

        // Create first meeting
        let result = handle.create_meeting("meeting-456".to_string()).await;
        assert!(result.is_ok());

        // Try to create duplicate
        let result = handle.create_meeting("meeting-456".to_string()).await;
        assert!(matches!(result, Err(McError::Conflict(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_get_nonexistent_meeting() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-003".to_string(), metrics, test_secret());

        let result = handle.get_meeting("nonexistent".to_string()).await;
        assert!(matches!(result, Err(McError::MeetingNotFound(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_remove_meeting() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-004".to_string(), metrics, test_secret());

        // Create a meeting
        let result = handle.create_meeting("meeting-789".to_string()).await;
        assert!(result.is_ok());

        // Remove it
        let result = handle.remove_meeting("meeting-789".to_string()).await;
        assert!(result.is_ok());

        // Verify it's gone
        let result = handle.get_meeting("meeting-789".to_string()).await;
        assert!(matches!(result, Err(McError::MeetingNotFound(_))));

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_status() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-005".to_string(), metrics, test_secret());

        // Get initial status
        let status = handle.get_status().await;
        assert!(status.is_ok());
        let status = status.unwrap();
        assert_eq!(status.meeting_count, 0);
        assert!(!status.is_draining);

        // Create some meetings
        let _ = handle.create_meeting("m1".to_string()).await;
        let _ = handle.create_meeting("m2".to_string()).await;

        let status = handle.get_status().await.unwrap();
        assert_eq!(status.meeting_count, 2);

        // Cleanup
        handle.cancel();
    }

    #[tokio::test]
    async fn test_controller_handle_shutdown() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-006".to_string(), metrics, test_secret());

        // Create a meeting
        let _ = handle.create_meeting("meeting-shutdown".to_string()).await;

        // Initiate shutdown - this triggers cancellation
        let result = handle.shutdown(Duration::from_secs(30)).await;
        assert!(result.is_ok());

        // Give time for cancellation to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // After shutdown, the controller is cancelled
        assert!(handle.is_cancelled());

        // Operations after shutdown may fail since actor is shutting down
        // This is expected behavior - the actor cancels after shutdown
    }

    #[tokio::test]
    async fn test_controller_cancellation_token() {
        let metrics = ActorMetrics::new();
        let handle =
            MeetingControllerActorHandle::new("mc-test-007".to_string(), metrics, test_secret());

        assert!(!handle.is_cancelled());

        let child = handle.child_token();
        assert!(!child.is_cancelled());

        handle.cancel();

        // Give time for cancellation to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(handle.is_cancelled());
        assert!(child.is_cancelled());
    }
}
