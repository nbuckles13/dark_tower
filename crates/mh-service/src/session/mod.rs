//! Meeting and participant session tracking for Media Handler.
//!
//! `SessionManager` tracks:
//! - Registered meetings (from MC's `RegisterMeeting` RPC)
//! - Active participant connections (authenticated via meeting JWT)
//! - Pending connections awaiting `RegisterMeeting` arrival
//!
//! # Concurrency
//!
//! Uses `tokio::sync::RwLock` for thread-safe access. This is acceptable
//! because operations are simple lookups/inserts with no nested locking
//! and writes are infrequent (see ADR-0001 justification in plan).
//!
//! # Notification
//!
//! Uses `tokio::sync::Notify` per meeting to wake pending connections
//! when `RegisterMeeting` arrives.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Notify, RwLock};

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

/// Internal state protected by `RwLock`.
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

/// Manages meeting registrations and participant connections.
///
/// Thread-safe via internal `RwLock`. All methods are async.
#[derive(Debug, Clone)]
pub struct SessionManager {
    state: Arc<RwLock<SessionState>>,
}

impl SessionManager {
    /// Create a new empty `SessionManager`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SessionState::default())),
        }
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
        let mut state = self.state.write().await;
        state
            .registered_meetings
            .insert(meeting_id.clone(), registration);

        // Drain pending connections for this meeting
        let pending = state
            .pending_connections
            .remove(&meeting_id)
            .unwrap_or_default();

        // Promote pending connections to active
        for conn in &pending {
            let meeting_conns = state
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
        if let Some(notifier) = state.meeting_notifiers.get(&meeting_id) {
            notifier.notify_waiters();
        }

        pending
    }

    /// Check if a meeting is registered on this MH instance.
    pub async fn is_meeting_registered(&self, meeting_id: &str) -> bool {
        let state = self.state.read().await;
        state.registered_meetings.contains_key(meeting_id)
    }

    /// Get the MC gRPC endpoint for a registered meeting.
    pub async fn get_mc_endpoint(&self, meeting_id: &str) -> Option<String> {
        let state = self.state.read().await;
        state
            .registered_meetings
            .get(meeting_id)
            .map(|r| r.mc_grpc_endpoint.clone())
    }

    /// Add an active connection for a registered meeting.
    pub async fn add_connection(&self, meeting_id: &str, entry: ConnectionEntry) {
        let mut state = self.state.write().await;
        let meeting_conns = state
            .active_connections
            .entry(meeting_id.to_string())
            .or_default();
        let participant_conns = meeting_conns
            .entry(entry.participant_id.clone())
            .or_default();
        participant_conns.push(entry);
    }

    /// Remove a connection by `connection_id`. Returns true if found and removed.
    pub async fn remove_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let mut state = self.state.write().await;
        if let Some(meeting_conns) = state.active_connections.get_mut(meeting_id) {
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

    /// Add a pending connection for an unregistered meeting.
    ///
    /// Returns a `Notify` handle that will be triggered when
    /// `RegisterMeeting` arrives for this `meeting_id`.
    pub async fn add_pending_connection(&self, pending: PendingConnection) -> Arc<Notify> {
        let mut state = self.state.write().await;
        let meeting_id = pending.meeting_id.clone();

        state
            .pending_connections
            .entry(meeting_id.clone())
            .or_default()
            .push(pending);

        // Get or create notifier for this meeting
        state
            .meeting_notifiers
            .entry(meeting_id)
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Remove a pending connection by `connection_id`.
    /// Called when the provisional timeout expires.
    pub async fn remove_pending_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let mut state = self.state.write().await;
        if let Some(pending_list) = state.pending_connections.get_mut(meeting_id) {
            if let Some(pos) = pending_list
                .iter()
                .position(|c| c.connection_id == connection_id)
            {
                pending_list.remove(pos);
                // Clean up empty entries
                if pending_list.is_empty() {
                    state.pending_connections.remove(meeting_id);
                }
                return true;
            }
        }
        false
    }

    /// Get count of active connections across all meetings.
    pub async fn active_connection_count(&self) -> usize {
        let state = self.state.read().await;
        state
            .active_connections
            .values()
            .flat_map(|m| m.values())
            .map(Vec::len)
            .sum()
    }
}

impl Default for SessionManager {
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
        let mgr = SessionManager::new();
        assert!(!mgr.is_meeting_registered("meeting-1").await);

        let pending = mgr
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(mgr.is_meeting_registered("meeting-1").await);
        assert!(pending.is_empty());
        assert_eq!(
            mgr.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_add_and_remove_connection() {
        let mgr = SessionManager::new();
        mgr.register_meeting(
            "meeting-1".to_string(),
            make_registration("mc-1", "http://mc:50052"),
        )
        .await;

        mgr.add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;
        mgr.add_connection("meeting-1", make_connection("conn-2", "user-2"))
            .await;

        assert_eq!(mgr.active_connection_count().await, 2);

        assert!(mgr.remove_connection("meeting-1", "conn-1").await);
        assert_eq!(mgr.active_connection_count().await, 1);

        // Removing non-existent connection returns false
        assert!(!mgr.remove_connection("meeting-1", "conn-999").await);
    }

    #[tokio::test]
    async fn test_pending_connection_promoted_on_register() {
        let mgr = SessionManager::new();

        // Add pending connection before RegisterMeeting
        let _notify = mgr
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;
        let _notify2 = mgr
            .add_pending_connection(make_pending("conn-2", "meeting-1", "user-2"))
            .await;

        // RegisterMeeting arrives — returns pending connections
        let promoted = mgr
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert_eq!(promoted.len(), 2);
        assert_eq!(promoted[0].connection_id, "conn-1");
        assert_eq!(promoted[1].connection_id, "conn-2");

        // Promoted connections should now be active
        assert_eq!(mgr.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_pending_connection_for_different_meeting_not_promoted() {
        let mgr = SessionManager::new();

        // Add pending for meeting-1
        let _notify = mgr
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Register meeting-2 — should NOT promote meeting-1's pending
        let promoted = mgr
            .register_meeting(
                "meeting-2".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(promoted.is_empty());
        assert_eq!(mgr.active_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_pending_connection() {
        let mgr = SessionManager::new();

        let _notify = mgr
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        assert!(mgr.remove_pending_connection("meeting-1", "conn-1").await);
        // Should not find it again
        assert!(!mgr.remove_pending_connection("meeting-1", "conn-1").await);
    }

    #[tokio::test]
    async fn test_notify_wakes_pending_on_register() {
        let mgr = SessionManager::new();

        let notify = mgr
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Spawn a task that waits on the notify
        let mgr_clone = mgr.clone();
        let handle = tokio::spawn(async move {
            notify.notified().await;
            mgr_clone.is_meeting_registered("meeting-1").await
        });

        // Small yield to let the spawned task start waiting
        tokio::task::yield_now().await;

        // Register meeting — should trigger notify
        mgr.register_meeting(
            "meeting-1".to_string(),
            make_registration("mc-1", "http://mc:50052"),
        )
        .await;

        let result = handle.await.unwrap();
        assert!(result, "Meeting should be registered after notify");
    }

    #[tokio::test]
    async fn test_registered_connection_not_affected_by_timeout() {
        let mgr = SessionManager::new();

        // Register meeting first
        mgr.register_meeting(
            "meeting-1".to_string(),
            make_registration("mc-1", "http://mc:50052"),
        )
        .await;

        // Add active connection (not pending — meeting is already registered)
        mgr.add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;

        // Trying to remove as pending should return false
        assert!(!mgr.remove_pending_connection("meeting-1", "conn-1").await);

        // Active connection should still be there
        assert_eq!(mgr.active_connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_get_mc_endpoint_unregistered() {
        let mgr = SessionManager::new();
        assert!(mgr.get_mc_endpoint("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_default_impl() {
        let mgr = SessionManager::default();
        assert!(!mgr.is_meeting_registered("any").await);
        assert_eq!(mgr.active_connection_count().await, 0);
    }
}
