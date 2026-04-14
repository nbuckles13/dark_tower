//! Meeting and participant session tracking for Media Handler.
//!
//! `SessionManagerHandle` is the public API for session state. It communicates
//! with a `SessionManagerActor` running in a dedicated `tokio::spawn` task
//! via typed message passing (ADR-0001 actor pattern).
//!
//! # Actor Pattern (ADR-0001)
//!
//! - `SessionManagerActor` (task): Owns `SessionState` exclusively. Processes
//!   messages sequentially via `mpsc::Receiver<SessionMessage>`. No locks.
//! - `SessionManagerHandle` (handle): Cloneable. Sends messages via
//!   `mpsc::Sender<SessionMessage>`. Uses `oneshot` for request-reply.
//! - `SessionMessage` (enum): One variant per operation.
//!
//! # Notification
//!
//! Uses `tokio::sync::Notify` per meeting to wake pending connections
//! when `RegisterMeeting` arrives. The actor owns the Notify; callers
//! receive an `Arc<Notify>` clone for awaiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Notify};

/// Channel buffer size for the session manager actor mailbox.
///
/// Operations are fast `HashMap` lookups/inserts, so backpressure is unlikely.
/// Lower than MC's `MEETING_CHANNEL_BUFFER` (500) due to lower throughput.
const SESSION_CHANNEL_BUFFER: usize = 256;

// ---------------------------------------------------------------------------
// Public data types (unchanged)
// ---------------------------------------------------------------------------

/// Registration data for a meeting on this MH instance.
#[derive(Debug, Clone)]
pub struct MeetingRegistration {
    /// MC instance ID that registered this meeting.
    pub mc_id: String,
    /// MC gRPC endpoint for callbacks (`NotifyParticipant*`).
    pub mc_grpc_endpoint: String,
    /// When the meeting was registered.
    pub registered_at: Instant,
}

/// An active participant connection.
#[derive(Debug, Clone)]
pub struct ConnectionEntry {
    /// Unique connection identifier.
    pub connection_id: String,
    /// Participant ID (from JWT `sub` claim).
    pub participant_id: String,
    /// When the connection was established.
    pub connected_at: Instant,
}

/// A pending connection awaiting `RegisterMeeting`.
#[derive(Debug, Clone)]
pub struct PendingConnection {
    /// Unique connection identifier.
    pub connection_id: String,
    /// Meeting ID from the JWT.
    pub meeting_id: String,
    /// Participant ID (from JWT `sub` claim).
    pub participant_id: String,
    /// When the connection was established.
    pub connected_at: Instant,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Messages sent to `SessionManagerActor`.
///
/// Each variant corresponds to one public method on `SessionManagerHandle`.
/// Request-reply variants carry a `oneshot::Sender` for the response.
#[derive(Debug)]
enum SessionMessage {
    /// Register a meeting. Returns any pending connections that were promoted.
    RegisterMeeting {
        meeting_id: String,
        registration: MeetingRegistration,
        respond_to: oneshot::Sender<Vec<PendingConnection>>,
    },
    /// Check if a meeting is registered.
    IsMeetingRegistered {
        meeting_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Get the MC gRPC endpoint for a registered meeting.
    GetMcEndpoint {
        meeting_id: String,
        respond_to: oneshot::Sender<Option<String>>,
    },
    /// Add an active connection (fire-and-forget).
    AddConnection {
        meeting_id: String,
        entry: ConnectionEntry,
    },
    /// Remove an active connection by ID. Returns true if found.
    RemoveConnection {
        meeting_id: String,
        connection_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Add a pending connection. Returns an `Arc<Notify>` for awaiting registration.
    AddPendingConnection {
        pending: PendingConnection,
        respond_to: oneshot::Sender<Arc<Notify>>,
    },
    /// Remove a pending connection by ID. Returns true if found.
    RemovePendingConnection {
        meeting_id: String,
        connection_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Get the total count of active connections across all meetings.
    ActiveConnectionCount { respond_to: oneshot::Sender<usize> },
}

// ---------------------------------------------------------------------------
// Actor (task)
// ---------------------------------------------------------------------------

/// Internal state owned exclusively by the actor. No `Arc`, no `RwLock`.
#[derive(Debug, Default)]
struct SessionState {
    /// Registered meetings: `meeting_id` -> registration data.
    registered_meetings: HashMap<String, MeetingRegistration>,
    /// Active connections: `meeting_id` -> (`participant_id` -> connections).
    active_connections: HashMap<String, HashMap<String, Vec<ConnectionEntry>>>,
    /// Pending connections awaiting `RegisterMeeting`: `meeting_id` -> pending list.
    pending_connections: HashMap<String, Vec<PendingConnection>>,
    /// Notify handles for pending connections: `meeting_id` -> `Notify`.
    meeting_notifiers: HashMap<String, Arc<Notify>>,
}

/// Actor that owns session state and processes messages sequentially.
///
/// Spawned by [`SessionManagerHandle::new`]. Runs until all senders are dropped
/// (channel closed), which happens naturally during shutdown.
struct SessionManagerActor {
    receiver: mpsc::Receiver<SessionMessage>,
    state: SessionState,
}

impl SessionManagerActor {
    fn new(receiver: mpsc::Receiver<SessionMessage>) -> Self {
        Self {
            receiver,
            state: SessionState::default(),
        }
    }

    /// Main run loop. Processes messages until the channel closes.
    async fn run(mut self) {
        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg);
        }
        tracing::debug!(
            target: "mh.session",
            "SessionManagerActor stopped (channel closed)"
        );
    }

    fn handle_message(&mut self, msg: SessionMessage) {
        match msg {
            SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                respond_to,
            } => {
                let result = self.handle_register_meeting(meeting_id, registration);
                let _ = respond_to.send(result);
            }
            SessionMessage::IsMeetingRegistered {
                meeting_id,
                respond_to,
            } => {
                let result = self.state.registered_meetings.contains_key(&meeting_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::GetMcEndpoint {
                meeting_id,
                respond_to,
            } => {
                let result = self
                    .state
                    .registered_meetings
                    .get(&meeting_id)
                    .map(|r| r.mc_grpc_endpoint.clone());
                let _ = respond_to.send(result);
            }
            SessionMessage::AddConnection { meeting_id, entry } => {
                self.handle_add_connection(meeting_id, entry);
            }
            SessionMessage::RemoveConnection {
                meeting_id,
                connection_id,
                respond_to,
            } => {
                let result = self.handle_remove_connection(&meeting_id, &connection_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::AddPendingConnection {
                pending,
                respond_to,
            } => {
                let result = self.handle_add_pending_connection(pending);
                let _ = respond_to.send(result);
            }
            SessionMessage::RemovePendingConnection {
                meeting_id,
                connection_id,
                respond_to,
            } => {
                let result = self.handle_remove_pending_connection(&meeting_id, &connection_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::ActiveConnectionCount { respond_to } => {
                let count = self
                    .state
                    .active_connections
                    .values()
                    .flat_map(|m| m.values())
                    .map(Vec::len)
                    .sum();
                let _ = respond_to.send(count);
            }
        }
    }

    /// Register a meeting and promote any pending connections.
    ///
    /// This runs sequentially inside the actor, eliminating the TOCTOU race
    /// that existed with `Arc<RwLock<SessionState>>`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "meeting_id is used as owned key in insert/entry calls; &str would require .to_string() at each site"
    )]
    fn handle_register_meeting(
        &mut self,
        meeting_id: String,
        registration: MeetingRegistration,
    ) -> Vec<PendingConnection> {
        if self.state.registered_meetings.contains_key(&meeting_id) {
            tracing::info!(
                target: "mh.session",
                meeting_id = %meeting_id,
                "Overwriting existing meeting registration"
            );
        }

        self.state
            .registered_meetings
            .insert(meeting_id.clone(), registration);

        // Drain pending connections for this meeting
        let pending = self
            .state
            .pending_connections
            .remove(&meeting_id)
            .unwrap_or_default();

        // Promote pending connections to active
        for conn in &pending {
            let meeting_conns = self
                .state
                .active_connections
                .entry(meeting_id.clone())
                .or_default();
            let participant_conns = meeting_conns
                .entry(conn.participant_id.clone())
                .or_default();
            participant_conns.push(ConnectionEntry {
                connection_id: conn.connection_id.clone(),
                participant_id: conn.participant_id.clone(),
                connected_at: conn.connected_at,
            });
        }

        // Notify any waiters that the meeting is now registered
        if let Some(notifier) = self.state.meeting_notifiers.get(&meeting_id) {
            notifier.notify_waiters();
        }

        pending
    }

    fn handle_add_connection(&mut self, meeting_id: String, entry: ConnectionEntry) {
        let meeting_conns = self.state.active_connections.entry(meeting_id).or_default();
        let participant_conns = meeting_conns
            .entry(entry.participant_id.clone())
            .or_default();
        participant_conns.push(entry);
    }

    fn handle_remove_connection(&mut self, meeting_id: &str, connection_id: &str) -> bool {
        if let Some(meeting_conns) = self.state.active_connections.get_mut(meeting_id) {
            for participant_conns in meeting_conns.values_mut() {
                if let Some(pos) = participant_conns
                    .iter()
                    .position(|c| c.connection_id == connection_id)
                {
                    participant_conns.remove(pos);
                    return true;
                }
            }
        }
        false
    }

    fn handle_add_pending_connection(&mut self, pending: PendingConnection) -> Arc<Notify> {
        let meeting_id = pending.meeting_id.clone();

        self.state
            .pending_connections
            .entry(meeting_id.clone())
            .or_default()
            .push(pending);

        // Get or create notifier for this meeting
        self.state
            .meeting_notifiers
            .entry(meeting_id)
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    fn handle_remove_pending_connection(&mut self, meeting_id: &str, connection_id: &str) -> bool {
        if let Some(pending_list) = self.state.pending_connections.get_mut(meeting_id) {
            if let Some(pos) = pending_list
                .iter()
                .position(|c| c.connection_id == connection_id)
            {
                pending_list.remove(pos);
                // Clean up empty entries
                if pending_list.is_empty() {
                    self.state.pending_connections.remove(meeting_id);
                }
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Handle (public API)
// ---------------------------------------------------------------------------

/// Handle to the `SessionManagerActor`.
///
/// This is the public interface for session management. Cloneable via the
/// inner `mpsc::Sender`. All methods are async and use message passing
/// to communicate with the actor task.
#[derive(Debug, Clone)]
pub struct SessionManagerHandle {
    sender: mpsc::Sender<SessionMessage>,
}

impl SessionManagerHandle {
    /// Create a new `SessionManagerActor` and return a handle to it.
    ///
    /// Spawns the actor task immediately. The actor runs until all handles
    /// are dropped (channel closes).
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(SESSION_CHANNEL_BUFFER);
        let actor = SessionManagerActor::new(receiver);
        tokio::spawn(actor.run());
        Self { sender }
    }

    /// Register a meeting on this MH instance.
    ///
    /// Called when MC sends `RegisterMeeting` RPC. Returns any pending
    /// connections that were waiting for this meeting's registration,
    /// so the caller can dispatch notifications.
    pub async fn register_meeting(
        &self,
        meeting_id: String,
        registration: MeetingRegistration,
    ) -> Vec<PendingConnection> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on register_meeting");
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    /// Check if a meeting is registered on this MH instance.
    pub async fn is_meeting_registered(&self, meeting_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::IsMeetingRegistered {
                meeting_id: meeting_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on is_meeting_registered");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Get the MC gRPC endpoint for a registered meeting.
    pub async fn get_mc_endpoint(&self, meeting_id: &str) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::GetMcEndpoint {
                meeting_id: meeting_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on get_mc_endpoint");
            return None;
        }
        rx.await.unwrap_or(None)
    }

    /// Add an active connection for a registered meeting.
    ///
    /// Fire-and-forget: the caller does not need confirmation.
    pub async fn add_connection(&self, meeting_id: &str, entry: ConnectionEntry) {
        if self
            .sender
            .send(SessionMessage::AddConnection {
                meeting_id: meeting_id.to_string(),
                entry,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on add_connection");
        }
    }

    /// Remove a connection by `connection_id`. Returns true if found and removed.
    pub async fn remove_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RemoveConnection {
                meeting_id: meeting_id.to_string(),
                connection_id: connection_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on remove_connection");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Add a pending connection for an unregistered meeting.
    ///
    /// Returns a `Notify` handle that will be triggered when
    /// `RegisterMeeting` arrives for this `meeting_id`.
    pub async fn add_pending_connection(&self, pending: PendingConnection) -> Arc<Notify> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::AddPendingConnection {
                pending,
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on add_pending_connection");
            return Arc::new(Notify::new());
        }
        rx.await.unwrap_or_else(|_| Arc::new(Notify::new()))
    }

    /// Remove a pending connection by `connection_id`.
    /// Called when the provisional timeout expires.
    pub async fn remove_pending_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RemovePendingConnection {
                meeting_id: meeting_id.to_string(),
                connection_id: connection_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on remove_pending_connection");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Get count of active connections across all meetings.
    pub async fn active_connection_count(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::ActiveConnectionCount { respond_to: tx })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on active_connection_count");
            return 0;
        }
        rx.await.unwrap_or(0)
    }
}

impl Default for SessionManagerHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_registration(mc_id: &str, endpoint: &str) -> MeetingRegistration {
        MeetingRegistration {
            mc_id: mc_id.to_string(),
            mc_grpc_endpoint: endpoint.to_string(),
            registered_at: Instant::now(),
        }
    }

    fn make_connection(conn_id: &str, participant_id: &str) -> ConnectionEntry {
        ConnectionEntry {
            connection_id: conn_id.to_string(),
            participant_id: participant_id.to_string(),
            connected_at: Instant::now(),
        }
    }

    fn make_pending(conn_id: &str, meeting_id: &str, participant_id: &str) -> PendingConnection {
        PendingConnection {
            connection_id: conn_id.to_string(),
            meeting_id: meeting_id.to_string(),
            participant_id: participant_id.to_string(),
            connected_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_register_meeting() {
        let handle = SessionManagerHandle::new();
        assert!(!handle.is_meeting_registered("meeting-1").await);

        let pending = handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(handle.is_meeting_registered("meeting-1").await);
        assert!(pending.is_empty());
        assert_eq!(
            handle.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_add_and_remove_connection() {
        let handle = SessionManagerHandle::new();
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        handle
            .add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;
        handle
            .add_connection("meeting-1", make_connection("conn-2", "user-2"))
            .await;

        assert_eq!(handle.active_connection_count().await, 2);

        assert!(handle.remove_connection("meeting-1", "conn-1").await);
        assert_eq!(handle.active_connection_count().await, 1);

        // Removing non-existent connection returns false
        assert!(!handle.remove_connection("meeting-1", "conn-999").await);
    }

    #[tokio::test]
    async fn test_pending_connection_promoted_on_register() {
        let handle = SessionManagerHandle::new();

        // Add pending connection before RegisterMeeting
        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;
        let _notify2 = handle
            .add_pending_connection(make_pending("conn-2", "meeting-1", "user-2"))
            .await;

        // RegisterMeeting arrives — returns pending connections
        let promoted = handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert_eq!(promoted.len(), 2);
        assert_eq!(promoted[0].connection_id, "conn-1");
        assert_eq!(promoted[1].connection_id, "conn-2");

        // Promoted connections should now be active
        assert_eq!(handle.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_pending_connection_for_different_meeting_not_promoted() {
        let handle = SessionManagerHandle::new();

        // Add pending for meeting-1
        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Register meeting-2 — should NOT promote meeting-1's pending
        let promoted = handle
            .register_meeting(
                "meeting-2".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(promoted.is_empty());
        assert_eq!(handle.active_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_pending_connection() {
        let handle = SessionManagerHandle::new();

        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        assert!(
            handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );
        // Should not find it again
        assert!(
            !handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );
    }

    #[tokio::test]
    async fn test_notify_wakes_pending_on_register() {
        let handle = SessionManagerHandle::new();

        let notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Spawn a task that waits on the notify
        let handle_clone = handle.clone();
        let join_handle = tokio::spawn(async move {
            notify.notified().await;
            handle_clone.is_meeting_registered("meeting-1").await
        });

        // Small yield to let the spawned task start waiting
        tokio::task::yield_now().await;

        // Register meeting — should trigger notify
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        let result = join_handle.await.unwrap();
        assert!(result, "Meeting should be registered after notify");
    }

    #[tokio::test]
    async fn test_registered_connection_not_affected_by_timeout() {
        let handle = SessionManagerHandle::new();

        // Register meeting first
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        // Add active connection (not pending — meeting is already registered)
        handle
            .add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;

        // Trying to remove as pending should return false
        assert!(
            !handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );

        // Active connection should still be there
        assert_eq!(handle.active_connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_get_mc_endpoint_unregistered() {
        let handle = SessionManagerHandle::new();
        assert!(handle.get_mc_endpoint("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_default_impl() {
        let handle = SessionManagerHandle::default();
        assert!(!handle.is_meeting_registered("any").await);
        assert_eq!(handle.active_connection_count().await, 0);
    }
}
