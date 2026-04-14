//! MH Connection Registry for tracking participant-to-MH connection state.
//!
//! Maintains a per-meeting, per-participant registry of which Media Handlers
//! each participant is connected to. Updated by MH notifications via
//! `MediaCoordinationService` (R-15). Read by future media routing logic (R-18).
//!
//! # Data Structure
//!
//! ```text
//! HashMap<meeting_id, HashMap<participant_id, Vec<MhConnectionInfo>>>
//! ```
//!
//! Each participant may be connected to multiple MHs (active/active topology).
//!
//! # Thread Safety
//!
//! Uses `tokio::sync::RwLock` for concurrent read access (many readers from
//! future media routing, few writers from MH notifications).

use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Maximum connections tracked per meeting to prevent unbounded memory growth.
const MAX_CONNECTIONS_PER_MEETING: usize = 1000;

/// Maximum ID field length (bytes). IDs are typically UUIDs (~36 chars).
pub const MAX_ID_LENGTH: usize = 256;

/// Information about a single participant-to-MH connection.
#[derive(Debug, Clone)]
pub struct MhConnectionInfo {
    /// MH instance identifier.
    pub handler_id: String,
    /// When the connection was registered.
    pub connected_at: Instant,
}

/// Registry tracking participant-to-MH connection state per meeting.
///
/// Thread-safe via `RwLock`. Shared as `Arc<MhConnectionRegistry>` between
/// the `MediaCoordinationService` gRPC handler and future media routing.
pub struct MhConnectionRegistry {
    /// meeting_id -> (participant_id -> connections)
    connections: RwLock<HashMap<String, HashMap<String, Vec<MhConnectionInfo>>>>,
}

impl MhConnectionRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Record a participant connecting to an MH.
    ///
    /// Returns `true` if the connection was added, `false` if the meeting
    /// has reached the maximum connection limit.
    pub async fn add_connection(
        &self,
        meeting_id: &str,
        participant_id: &str,
        handler_id: &str,
    ) -> bool {
        let mut connections = self.connections.write().await;
        let meeting = connections.entry(meeting_id.to_string()).or_default();

        // Check per-meeting connection limit
        let total: usize = meeting.values().map(|v| v.len()).sum();
        if total >= MAX_CONNECTIONS_PER_MEETING {
            warn!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                limit = MAX_CONNECTIONS_PER_MEETING,
                "Meeting connection limit reached, rejecting new connection"
            );
            return false;
        }

        let participant_connections = meeting.entry(participant_id.to_string()).or_default();

        // Avoid duplicate connections for same handler
        if participant_connections
            .iter()
            .any(|c| c.handler_id == handler_id)
        {
            debug!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                participant_id = %participant_id,
                handler_id = %handler_id,
                "Duplicate connection notification, ignoring"
            );
            return true;
        }

        participant_connections.push(MhConnectionInfo {
            handler_id: handler_id.to_string(),
            connected_at: Instant::now(),
        });

        debug!(
            target: "mc.mh_registry",
            meeting_id = %meeting_id,
            participant_id = %participant_id,
            handler_id = %handler_id,
            total_connections = participant_connections.len(),
            "Participant connection added"
        );

        true
    }

    /// Remove a participant's connection to a specific MH.
    ///
    /// Returns `true` if the connection was found and removed.
    pub async fn remove_connection(
        &self,
        meeting_id: &str,
        participant_id: &str,
        handler_id: &str,
    ) -> bool {
        let mut connections = self.connections.write().await;
        let Some(meeting) = connections.get_mut(meeting_id) else {
            debug!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                "Remove connection: meeting not found in registry"
            );
            return false;
        };

        let Some(participant_connections) = meeting.get_mut(participant_id) else {
            debug!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                participant_id = %participant_id,
                "Remove connection: participant not found in registry"
            );
            return false;
        };

        let before = participant_connections.len();
        participant_connections.retain(|c| c.handler_id != handler_id);
        let removed = participant_connections.len() < before;

        if removed {
            debug!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                participant_id = %participant_id,
                handler_id = %handler_id,
                remaining = participant_connections.len(),
                "Participant connection removed"
            );
        }

        // Clean up empty entries
        if participant_connections.is_empty() {
            meeting.remove(participant_id);
        }
        if meeting.is_empty() {
            connections.remove(meeting_id);
        }

        removed
    }

    /// Remove all connection state for a meeting.
    ///
    /// Called when a meeting ends to prevent stale registry entries.
    pub async fn remove_meeting(&self, meeting_id: &str) {
        let mut connections = self.connections.write().await;
        if connections.remove(meeting_id).is_some() {
            debug!(
                target: "mc.mh_registry",
                meeting_id = %meeting_id,
                "Meeting removed from connection registry"
            );
        }
    }

    /// Get connection info for a participant in a meeting.
    ///
    /// Returns an empty vec if the participant or meeting is not found.
    pub async fn get_connections(
        &self,
        meeting_id: &str,
        participant_id: &str,
    ) -> Vec<MhConnectionInfo> {
        let connections = self.connections.read().await;
        connections
            .get(meeting_id)
            .and_then(|m| m.get(participant_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Get total number of tracked meetings.
    pub async fn meeting_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

impl Default for MhConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_connection() {
        let registry = MhConnectionRegistry::new();

        let added = registry.add_connection("meeting-1", "part-1", "mh-1").await;
        assert!(added);

        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].handler_id, "mh-1");
    }

    #[tokio::test]
    async fn test_add_multiple_mh_connections() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;
        registry.add_connection("meeting-1", "part-1", "mh-2").await;

        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns.len(), 2);

        let handler_ids: Vec<&str> = conns.iter().map(|c| c.handler_id.as_str()).collect();
        assert!(handler_ids.contains(&"mh-1"));
        assert!(handler_ids.contains(&"mh-2"));
    }

    #[tokio::test]
    async fn test_duplicate_connection_ignored() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;
        registry.add_connection("meeting-1", "part-1", "mh-1").await;

        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_connection() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;
        registry.add_connection("meeting-1", "part-1", "mh-2").await;

        let removed = registry
            .remove_connection("meeting-1", "part-1", "mh-1")
            .await;
        assert!(removed);

        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].handler_id, "mh-2");
    }

    #[tokio::test]
    async fn test_remove_nonexistent_connection() {
        let registry = MhConnectionRegistry::new();

        let removed = registry
            .remove_connection("meeting-1", "part-1", "mh-1")
            .await;
        assert!(!removed);
    }

    #[tokio::test]
    async fn test_remove_cleans_up_empty_entries() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;

        registry
            .remove_connection("meeting-1", "part-1", "mh-1")
            .await;

        // Meeting should be cleaned up since it has no connections
        assert_eq!(registry.meeting_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_meeting() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;
        registry.add_connection("meeting-1", "part-2", "mh-2").await;

        registry.remove_meeting("meeting-1").await;

        assert_eq!(registry.meeting_count().await, 0);
        let conns = registry.get_connections("meeting-1", "part-1").await;
        assert!(conns.is_empty());
    }

    #[tokio::test]
    async fn test_get_connections_unknown_meeting() {
        let registry = MhConnectionRegistry::new();

        let conns = registry.get_connections("unknown", "part-1").await;
        assert!(conns.is_empty());
    }

    #[tokio::test]
    async fn test_get_connections_unknown_participant() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;

        let conns = registry.get_connections("meeting-1", "unknown").await;
        assert!(conns.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_meetings() {
        let registry = MhConnectionRegistry::new();

        registry.add_connection("meeting-1", "part-1", "mh-1").await;
        registry.add_connection("meeting-2", "part-2", "mh-2").await;

        assert_eq!(registry.meeting_count().await, 2);

        let conns1 = registry.get_connections("meeting-1", "part-1").await;
        assert_eq!(conns1.len(), 1);
        assert_eq!(conns1[0].handler_id, "mh-1");

        let conns2 = registry.get_connections("meeting-2", "part-2").await;
        assert_eq!(conns2.len(), 1);
        assert_eq!(conns2[0].handler_id, "mh-2");
    }

    #[tokio::test]
    async fn test_connection_limit_per_meeting() {
        let registry = MhConnectionRegistry::new();

        // Fill up to the limit
        for i in 0..MAX_CONNECTIONS_PER_MEETING {
            let added = registry
                .add_connection("meeting-1", &format!("part-{i}"), "mh-1")
                .await;
            assert!(added, "Connection {i} should be accepted");
        }

        // Next connection should be rejected
        let added = registry
            .add_connection("meeting-1", "part-overflow", "mh-1")
            .await;
        assert!(!added, "Connection beyond limit should be rejected");
    }

    #[tokio::test]
    async fn test_default_creates_empty_registry() {
        let registry = MhConnectionRegistry::default();
        assert_eq!(registry.meeting_count().await, 0);
    }
}
