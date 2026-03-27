//! Shared encoding utilities for WebTransport signaling messages.

use crate::actors::messages::{LeaveReason, ParticipantStateUpdate};

use proto_gen::signaling::{
    self, server_message, Participant, ParticipantJoined, ParticipantLeft, ServerMessage,
};
use tracing::debug;

/// Encode a `ParticipantStateUpdate` as a `ServerMessage`.
///
/// Only `ParticipantJoined` and `ParticipantLeft` are serialized to the wire.
/// Other variants are logged but return `None`.
pub fn encode_participant_update(update: &ParticipantStateUpdate) -> Option<ServerMessage> {
    match update {
        ParticipantStateUpdate::Joined(info) => {
            let participant = Participant {
                participant_id: info.participant_id.clone(),
                name: info.display_name.clone(),
                streams: Vec::new(),
                joined_at: 0,
            };
            Some(ServerMessage {
                message: Some(server_message::Message::ParticipantJoined(
                    ParticipantJoined {
                        participant: Some(participant),
                    },
                )),
            })
        }
        ParticipantStateUpdate::Left {
            participant_id,
            reason,
        } => {
            let proto_reason = match reason {
                LeaveReason::Voluntary => signaling::LeaveReason::Voluntary,
                LeaveReason::Timeout => signaling::LeaveReason::Timeout,
                LeaveReason::Removed => signaling::LeaveReason::Kicked,
                LeaveReason::MeetingEnded => signaling::LeaveReason::MeetingEnded,
            };
            Some(ServerMessage {
                message: Some(server_message::Message::ParticipantLeft(ParticipantLeft {
                    participant_id: participant_id.clone(),
                    reason: proto_reason as i32,
                })),
            })
        }
        ParticipantStateUpdate::MuteChanged { participant_id, .. } => {
            debug!(
                target: "mc.webtransport.handler",
                participant_id = %participant_id,
                "MuteChanged not serialized (out of scope)"
            );
            None
        }
        ParticipantStateUpdate::Disconnected { participant_id } => {
            debug!(
                target: "mc.webtransport.handler",
                participant_id = %participant_id,
                "Disconnected not serialized (out of scope)"
            );
            None
        }
        ParticipantStateUpdate::Reconnected { participant_id } => {
            debug!(
                target: "mc.webtransport.handler",
                participant_id = %participant_id,
                "Reconnected not serialized (out of scope)"
            );
            None
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::actors::messages::{
        LeaveReason, ParticipantInfo, ParticipantStateUpdate, ParticipantStatus,
    };
    use proto_gen::signaling::{self, server_message};

    fn make_participant_info(id: &str, name: &str) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: id.to_string(),
            user_id: "user-1".to_string(),
            display_name: name.to_string(),
            audio_self_muted: false,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
            status: ParticipantStatus::Connected,
        }
    }

    #[test]
    fn test_encode_participant_joined() {
        let info = make_participant_info("part-1", "Alice");
        let update = ParticipantStateUpdate::Joined(info);

        let result = encode_participant_update(&update);
        assert!(result.is_some());

        let msg = result.unwrap();
        match msg.message.unwrap() {
            server_message::Message::ParticipantJoined(joined) => {
                let p = joined.participant.unwrap();
                assert_eq!(p.participant_id, "part-1");
                assert_eq!(p.name, "Alice");
            }
            other => panic!("Expected ParticipantJoined, got {other:?}"),
        }
    }

    #[test]
    fn test_encode_participant_left_voluntary() {
        let update = ParticipantStateUpdate::Left {
            participant_id: "part-2".to_string(),
            reason: LeaveReason::Voluntary,
        };

        let result = encode_participant_update(&update);
        assert!(result.is_some());

        let msg = result.unwrap();
        match msg.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.participant_id, "part-2");
                assert_eq!(left.reason, signaling::LeaveReason::Voluntary as i32);
            }
            other => panic!("Expected ParticipantLeft, got {other:?}"),
        }
    }

    #[test]
    fn test_encode_participant_left_timeout() {
        let update = ParticipantStateUpdate::Left {
            participant_id: "part-3".to_string(),
            reason: LeaveReason::Timeout,
        };

        let msg = encode_participant_update(&update).unwrap();
        match msg.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, signaling::LeaveReason::Timeout as i32);
            }
            other => panic!("Expected ParticipantLeft, got {other:?}"),
        }
    }

    #[test]
    fn test_encode_participant_left_removed() {
        let update = ParticipantStateUpdate::Left {
            participant_id: "part-4".to_string(),
            reason: LeaveReason::Removed,
        };

        let msg = encode_participant_update(&update).unwrap();
        match msg.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, signaling::LeaveReason::Kicked as i32);
            }
            other => panic!("Expected ParticipantLeft, got {other:?}"),
        }
    }

    #[test]
    fn test_encode_participant_left_meeting_ended() {
        let update = ParticipantStateUpdate::Left {
            participant_id: "part-5".to_string(),
            reason: LeaveReason::MeetingEnded,
        };

        let msg = encode_participant_update(&update).unwrap();
        match msg.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, signaling::LeaveReason::MeetingEnded as i32);
            }
            other => panic!("Expected ParticipantLeft, got {other:?}"),
        }
    }

    #[test]
    fn test_encode_mute_changed_returns_none() {
        let update = ParticipantStateUpdate::MuteChanged {
            participant_id: "part-1".to_string(),
            audio_self_muted: true,
            video_self_muted: false,
            audio_host_muted: false,
            video_host_muted: false,
        };
        assert!(encode_participant_update(&update).is_none());
    }

    #[test]
    fn test_encode_disconnected_returns_none() {
        let update = ParticipantStateUpdate::Disconnected {
            participant_id: "part-1".to_string(),
        };
        assert!(encode_participant_update(&update).is_none());
    }

    #[test]
    fn test_encode_reconnected_returns_none() {
        let update = ParticipantStateUpdate::Reconnected {
            participant_id: "part-1".to_string(),
        };
        assert!(encode_participant_update(&update).is_none());
    }
}
