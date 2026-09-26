//! Shared encoding utilities for WebTransport signaling messages.

use crate::actors::messages::{LeaveReason, ParticipantStateUpdate};
use crate::webtransport::trace::inject_current_context;

use proto_gen::dark_tower::signaling::v1::{
    self, server_message, Participant, ParticipantJoined, ParticipantLeft, ServerMessage,
};
use tracing::debug;

/// A roster update encoded for the wire, with the `payload_kind` label it is
/// counted under if the outbound channel drops it.
///
/// The label travels WITH the message deliberately. This function is the single
/// place that decides which updates are wire-visible, so returning the label
/// from here means a newly wire-visible variant cannot compile without choosing
/// one — the difference between a bounded label domain and a convention. The
/// alternative, a literal at the `try_send` call site, would silently hand every
/// future variant whatever label happened to be there.
#[derive(Debug)]
pub struct EncodedUpdate {
    /// The encoded message.
    pub server_message: ServerMessage,
    /// `mc_participant_outbound_messages_dropped_total{payload_kind}` value.
    pub payload_kind: &'static str,
}

/// `payload_kind` for a dropped `ParticipantJoined`.
pub const PAYLOAD_KIND_PARTICIPANT_UPDATE_JOINED: &str = "participant_update_joined";

/// `payload_kind` for a dropped `ParticipantLeft`. Split from joins (story 2
/// task 9) because a dropped leave has a consequence a dropped join does not:
/// the client keeps a stale roster entry and later counts a legitimate
/// `sender_id` reissue as an R-18 rebind. Non-zero here over a window makes
/// that explanation SUPPORTED — not confirmed for any single rebind increment,
/// since this counter is fleet-wide.
pub const PAYLOAD_KIND_PARTICIPANT_UPDATE_LEFT: &str = "participant_update_left";

/// Encode a `ParticipantStateUpdate` for the wire.
///
/// Only `ParticipantJoined` and `ParticipantLeft` are serialized to the wire.
/// Other variants are logged but return `None`.
pub fn encode_participant_update(update: &ParticipantStateUpdate) -> Option<EncodedUpdate> {
    match update {
        ParticipantStateUpdate::Joined(info) => {
            let participant = Participant {
                participant_id: info.participant_id.clone(),
                name: info.display_name.clone(),
                streams: Vec::new(),
                joined_at: 0,
                // ADR-0036 §4: sender id and identity signing key are the only
                // roster additions, and no other key material rides here — no
                // meeting KEK, no wrapped transmit key, no thumbprint.
                //
                // Populated on the fan-out as well as on the join response, so
                // a participant already in the meeting can resolve a later
                // joiner's frames: key id -> sender_id -> this entry ->
                // identity_public_key. Publishing only on the join response
                // would leave existing members unable to attribute new frames.
                //
                // `Some` of a `NonZeroU16`, so never `Some(0)` — 0 is
                // reserved-invalid. Trust on first use: same-keyholder
                // consistency, never a verified identity.
                sender_id: Some(u32::from(info.sender_id.get().get())),
                // `None` MUST become EMPTY BYTES, never a zero-filled 32-byte
                // array — see the same choke-point in `connection.rs`. An
                // all-zero key reads as present, sends a consumer down the
                // verify path, and silently destroys the "no key published"
                // signal. Explicit `match`, never `unwrap_or_default()`.
                identity_public_key: match &info.identity_public_key {
                    Some(key) => key.as_bytes().to_vec(),
                    None => Vec::new(),
                },
            };
            // R-57: carry the current server-side trace context to the client on
            // the fan-out broadcast (bounded W3C IDs only).
            let (trace_parent, trace_state) = inject_current_context();
            Some(EncodedUpdate {
                payload_kind: PAYLOAD_KIND_PARTICIPANT_UPDATE_JOINED,
                server_message: ServerMessage {
                    message: Some(server_message::Message::ParticipantJoined(
                        ParticipantJoined {
                            participant: Some(participant),
                        },
                    )),
                    trace_parent,
                    trace_state,
                },
            })
        }
        ParticipantStateUpdate::Left {
            participant_id,
            reason,
        } => {
            let proto_reason = match reason {
                LeaveReason::Voluntary => v1::LeaveReason::Voluntary,
                LeaveReason::Timeout => v1::LeaveReason::Timeout,
                LeaveReason::Removed => v1::LeaveReason::Kicked,
                LeaveReason::MeetingEnded => v1::LeaveReason::MeetingEnded,
            };
            let (trace_parent, trace_state) = inject_current_context();
            Some(EncodedUpdate {
                payload_kind: PAYLOAD_KIND_PARTICIPANT_UPDATE_LEFT,
                server_message: ServerMessage {
                    message: Some(server_message::Message::ParticipantLeft(ParticipantLeft {
                        participant_id: participant_id.clone(),
                        reason: proto_reason as i32,
                    })),
                    trace_parent,
                    trace_state,
                },
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
    use proto_gen::dark_tower::signaling::v1::{self, server_message};

    fn make_participant_info(id: &str, name: &str) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: id.to_string(),
            user_id: "user-1".to_string(),
            display_name: name.to_string(),
            audio_self_muted: false,
            video_self_muted: false,
            audio_server_muted: false,
            video_server_muted: false,
            status: ParticipantStatus::Connected,
            sender_id: crate::media_admission::fixtures::sample_sender_id(),
            identity_public_key: crate::media_admission::fixtures::sample_identity_key(),
        }
    }

    #[test]
    fn test_encode_participant_joined() {
        let info = make_participant_info("part-1", "Alice");
        let update = ParticipantStateUpdate::Joined(info);

        let result = encode_participant_update(&update);
        assert!(result.is_some());

        let msg = result.unwrap();
        match msg.server_message.message.unwrap() {
            server_message::Message::ParticipantJoined(joined) => {
                let p = joined.participant.unwrap();
                assert_eq!(p.participant_id, "part-1");
                assert_eq!(p.name, "Alice");
            }
            other => panic!("Expected ParticipantJoined, got {other:?}"),
        }
    }

    /// Joins and leaves carry DIFFERENT drop labels. They were one bucket until
    /// story 2 task 9, which made a dropped leave indistinguishable from a
    /// dropped join — and only the leave explains a client counting a
    /// legitimate `sender_id` reissue as an R-18 rebind.
    #[test]
    fn joins_and_leaves_are_labelled_apart_for_the_drop_counter() {
        let joined = encode_participant_update(&ParticipantStateUpdate::Joined(
            make_participant_info("p", "P"),
        ))
        .unwrap();
        let left = encode_participant_update(&ParticipantStateUpdate::Left {
            participant_id: "p".to_string(),
            reason: LeaveReason::Voluntary,
        })
        .unwrap();
        assert_eq!(joined.payload_kind, "participant_update_joined");
        assert_eq!(left.payload_kind, "participant_update_left");
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
        match msg.server_message.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.participant_id, "part-2");
                assert_eq!(left.reason, v1::LeaveReason::Voluntary as i32);
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
        match msg.server_message.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, v1::LeaveReason::Timeout as i32);
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
        match msg.server_message.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, v1::LeaveReason::Kicked as i32);
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
        match msg.server_message.message.unwrap() {
            server_message::Message::ParticipantLeft(left) => {
                assert_eq!(left.reason, v1::LeaveReason::MeetingEnded as i32);
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
            audio_server_muted: false,
            video_server_muted: false,
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
