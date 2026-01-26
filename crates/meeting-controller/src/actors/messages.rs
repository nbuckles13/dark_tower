//! Message types for actor communication (ADR-0023 Section 2).
//!
//! All inter-actor communication uses strongly-typed message passing via `tokio::sync::mpsc`.
//! Response patterns use `tokio::sync::oneshot` for request-reply semantics.

use crate::errors::McError;
use std::time::Duration;
use tokio::sync::oneshot;

/// Messages sent to `MeetingControllerActor`.
#[derive(Debug)]
pub enum ControllerMessage {
    /// Create a new meeting actor for the given meeting ID.
    CreateMeeting {
        meeting_id: String,
        /// Response channel for the meeting actor handle or error.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Get a handle to an existing meeting actor.
    GetMeeting {
        meeting_id: String,
        /// Response channel for the meeting actor handle or error.
        respond_to: oneshot::Sender<Result<MeetingInfo, McError>>,
    },

    /// Remove a meeting (called when all participants leave or meeting ends).
    RemoveMeeting {
        meeting_id: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Get current status of all meetings (for health checks).
    GetStatus {
        /// Response channel for controller status.
        respond_to: oneshot::Sender<ControllerStatus>,
    },

    /// Initiate graceful shutdown (SIGTERM received).
    Shutdown {
        /// Deadline for shutdown.
        deadline: Duration,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },
}

/// Messages sent to `MeetingActor`.
#[derive(Debug)]
pub enum MeetingMessage {
    /// A new connection wants to join this meeting.
    ConnectionJoin {
        connection_id: String,
        user_id: String,
        participant_id: String,
        /// Whether this participant has host privileges.
        is_host: bool,
        /// Response channel for join result.
        respond_to: oneshot::Sender<Result<JoinResult, McError>>,
    },

    /// A connection has disconnected (may reconnect within grace period).
    ConnectionDisconnected {
        connection_id: String,
        participant_id: String,
    },

    /// A connection is attempting to reconnect.
    ConnectionReconnect {
        connection_id: String,
        correlation_id: String,
        binding_token: String,
        /// Response channel for reconnect result.
        respond_to: oneshot::Sender<Result<ReconnectResult, McError>>,
    },

    /// A participant is leaving the meeting (explicit leave, not disconnect).
    ParticipantLeave {
        participant_id: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// Forward a signaling message from a connection.
    SignalingMessage {
        participant_id: String,
        message: SignalingPayload,
    },

    /// Get current meeting state (for debugging/health).
    GetState {
        /// Response channel for meeting state.
        respond_to: oneshot::Sender<MeetingState>,
    },

    /// Update participant mute status (self-mute, informational).
    UpdateSelfMute {
        participant_id: String,
        audio_muted: bool,
        video_muted: bool,
    },

    /// Host mutes a participant (enforced).
    HostMute {
        target_participant_id: String,
        muted_by: String,
        audio_muted: bool,
        video_muted: bool,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },

    /// End the meeting (called by host or system).
    EndMeeting {
        reason: String,
        /// Response channel for confirmation.
        respond_to: oneshot::Sender<Result<(), McError>>,
    },
}

/// Messages sent to `ConnectionActor`.
#[derive(Debug)]
pub enum ConnectionMessage {
    /// Send a signaling message to the connected client.
    Send { message: SignalingPayload },

    /// Notify connection of participant state change.
    ParticipantUpdate { update: ParticipantStateUpdate },

    /// Close the connection gracefully.
    Close { reason: String },

    /// Ping the connection to check liveness.
    Ping { respond_to: oneshot::Sender<()> },
}

// ----------------------------------------------------------------------------
// Supporting Types
// ----------------------------------------------------------------------------

/// Information about a meeting returned by GetMeeting.
#[derive(Debug, Clone)]
pub struct MeetingInfo {
    /// Meeting ID.
    pub meeting_id: String,
    /// Current participant count.
    pub participant_count: usize,
    /// Meeting creation timestamp.
    pub created_at: i64,
    /// Current fencing generation.
    pub fencing_generation: u64,
}

/// Status of the `MeetingControllerActor`.
#[derive(Debug, Clone)]
pub struct ControllerStatus {
    /// Total active meetings.
    pub meeting_count: usize,
    /// Total active connections across all meetings.
    pub connection_count: usize,
    /// Whether the controller is draining.
    pub is_draining: bool,
    /// Current mailbox depth.
    pub mailbox_depth: usize,
}

/// Result of a successful join.
#[derive(Debug, Clone)]
pub struct JoinResult {
    /// Assigned participant ID.
    pub participant_id: String,
    /// Correlation ID for reconnection.
    pub correlation_id: String,
    /// Binding token for reconnection (HMAC-SHA256).
    pub binding_token: String,
    /// List of other participants in the meeting.
    pub participants: Vec<ParticipantInfo>,
    /// Current fencing generation.
    pub fencing_generation: u64,
}

/// Result of a successful reconnection.
#[derive(Debug, Clone)]
pub struct ReconnectResult {
    /// Confirmed participant ID.
    pub participant_id: String,
    /// New correlation ID (rotated).
    pub new_correlation_id: String,
    /// New binding token (rotated).
    pub new_binding_token: String,
    /// Current participant list.
    pub participants: Vec<ParticipantInfo>,
}

/// Information about a participant.
#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    /// Participant ID.
    pub participant_id: String,
    /// User ID (from JWT).
    pub user_id: String,
    /// Display name.
    pub display_name: String,
    /// Whether audio is self-muted.
    pub audio_self_muted: bool,
    /// Whether video is self-muted.
    pub video_self_muted: bool,
    /// Whether audio is host-muted (enforced).
    pub audio_host_muted: bool,
    /// Whether video is host-muted (enforced).
    pub video_host_muted: bool,
    /// Connection status.
    pub status: ParticipantStatus,
}

/// Participant connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantStatus {
    /// Connected and active.
    Connected,
    /// Disconnected, within grace period.
    Disconnected,
    /// Reconnecting.
    Reconnecting,
}

/// State update for a participant (broadcast to other connections).
#[derive(Debug, Clone)]
pub enum ParticipantStateUpdate {
    /// A participant joined.
    Joined(ParticipantInfo),
    /// A participant left.
    Left {
        participant_id: String,
        reason: LeaveReason,
    },
    /// A participant's mute status changed.
    MuteChanged {
        participant_id: String,
        audio_self_muted: bool,
        video_self_muted: bool,
        audio_host_muted: bool,
        video_host_muted: bool,
    },
    /// A participant disconnected (still in grace period).
    Disconnected { participant_id: String },
    /// A participant reconnected.
    Reconnected { participant_id: String },
}

/// Reason for participant leaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveReason {
    /// Participant chose to leave.
    Voluntary,
    /// Disconnect grace period expired.
    Timeout,
    /// Removed by host.
    Removed,
    /// Meeting ended.
    MeetingEnded,
}

/// Current state of a meeting (for debugging/health).
#[derive(Debug, Clone)]
pub struct MeetingState {
    /// Meeting ID.
    pub meeting_id: String,
    /// Current participants.
    pub participants: Vec<ParticipantInfo>,
    /// Current fencing generation.
    pub fencing_generation: u64,
    /// Meeting creation timestamp.
    pub created_at: i64,
    /// Current mailbox depth.
    pub mailbox_depth: usize,
    /// Whether the meeting is shutting down.
    pub is_shutting_down: bool,
}

/// Signaling message payload (wraps various message types).
#[derive(Debug, Clone)]
pub enum SignalingPayload {
    /// Mute update from client.
    MuteUpdate {
        audio_muted: bool,
        video_muted: bool,
    },
    /// Layout subscription request.
    LayoutSubscribe { layout_type: String },
    /// Chat message.
    Chat { content: String },
    /// Generic signaling data (protobuf bytes).
    Raw { message_type: u32, data: Vec<u8> },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_status_equality() {
        assert_eq!(ParticipantStatus::Connected, ParticipantStatus::Connected);
        assert_ne!(
            ParticipantStatus::Connected,
            ParticipantStatus::Disconnected
        );
    }

    #[test]
    fn test_leave_reason_equality() {
        assert_eq!(LeaveReason::Voluntary, LeaveReason::Voluntary);
        assert_ne!(LeaveReason::Voluntary, LeaveReason::Timeout);
    }

    #[test]
    fn test_controller_status_default_values() {
        let status = ControllerStatus {
            meeting_count: 0,
            connection_count: 0,
            is_draining: false,
            mailbox_depth: 0,
        };
        assert_eq!(status.meeting_count, 0);
        assert!(!status.is_draining);
    }

    #[test]
    fn test_participant_info_clone() {
        let info = ParticipantInfo {
            participant_id: "p1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Test User".to_string(),
            audio_self_muted: false,
            video_self_muted: true,
            audio_host_muted: false,
            video_host_muted: false,
            status: ParticipantStatus::Connected,
        };
        let cloned = info.clone();
        assert_eq!(info.participant_id, cloned.participant_id);
        assert_eq!(info.display_name, cloned.display_name);
    }

    #[test]
    fn test_signaling_payload_variants() {
        let mute = SignalingPayload::MuteUpdate {
            audio_muted: true,
            video_muted: false,
        };
        assert!(matches!(mute, SignalingPayload::MuteUpdate { .. }));

        let layout = SignalingPayload::LayoutSubscribe {
            layout_type: "grid".to_string(),
        };
        assert!(matches!(layout, SignalingPayload::LayoutSubscribe { .. }));

        let chat = SignalingPayload::Chat {
            content: "Hello".to_string(),
        };
        assert!(matches!(chat, SignalingPayload::Chat { .. }));
    }
}
