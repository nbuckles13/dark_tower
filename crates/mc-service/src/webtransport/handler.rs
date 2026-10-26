//! Shared encoding utilities for WebTransport signaling messages.

use crate::actors::messages::{LeaveReason, ParticipantInfo, ParticipantStateUpdate};
use crate::media_kind::{served_kind, KindRefusal, MediaKindLabel, MediaStates, KNOWN_WIRE_KINDS};
use crate::webtransport::trace::inject_current_context;

use proto_gen::dark_tower::signaling::v1::{
    self, server_message, ClientMediaState, MediaKindState, Participant, ParticipantJoined,
    ParticipantLeft, ParticipantMuteUpdate, ReceiveSlotCap, ServerMessage,
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

/// `payload_kind` for a dropped `ParticipantMuteUpdate` (story 2 task 12).
///
/// A separate value by DEMONSTRATED CONSUMER NEED, not message-type taxonomy
/// (the same rule that keeps `signaling_raw` one value): a dropped mute update
/// has no re-sync path. Join/leave state is rebuilt from the roster, but mute
/// state after join is delivered ONLY by these updates -- server mute (the live
/// broadcast and the late-joiner replay) and, since the story 3 R-15 fix,
/// client mute (the per-meeting fan-out; the join roster carries only the
/// snapshot at join). So a drop leaves that client rendering a stale indicator
/// until the next mute change on that participant, or the observer's rejoin --
/// which may never come. Client- and server-mute updates share this value: the
/// same message, the same consequence, the same remedy. Past participle,
/// matching `_joined` / `_left`, so `participant_update.*` still recovers the
/// merged series.
pub const PAYLOAD_KIND_PARTICIPANT_UPDATE_MUTED: &str = "participant_update_muted";

/// THE one builder of a roster entry (`Participant`), for both carriers: the
/// join response's `existing_participants` (`connection.rs`) and the
/// `ParticipantJoined` fan-out (below). One builder, so a field cannot be set
/// on one carrier and forgotten on the other.
pub fn roster_entry(info: &ParticipantInfo) -> Participant {
    Participant {
        participant_id: info.participant_id.clone(),
        name: info.display_name.clone(),
        streams: Vec::new(),
        joined_at: 0,
        // ADR-0036 §4: the roster carries the sender id and the identity
        // signing public key, and NO other key material — no meeting KEK, no
        // wrapped transmit key, no key id, no thumbprint.
        //
        // Populated on the fan-out as well as on the join response, so a
        // participant already in the meeting can resolve a later joiner's
        // frames: key id -> sender_id -> this entry -> identity_public_key.
        //
        // `Some` of a `NonZeroU16`, so never `Some(0)`: 0 is reserved-invalid,
        // and N participants sharing it would collide on one roster identity
        // and — via key id, derived wrap nonce — on one AES-GCM nonce. Trust on
        // first use: same-keyholder consistency, never a verified identity.
        sender_id: Some(u32::from(info.sender_id.get().get())),
        // `identity_public_key` is either exactly 32 bytes or EMPTY (MC refuses
        // the join otherwise). `None` MUST become EMPTY BYTES, never a
        // zero-filled 32-byte array: an all-zero key reads as present, sends a
        // consumer down the verify path, and silently destroys the "no key
        // published" signal. Explicit `match`, never `unwrap_or_default()`. Do
        // not "simplify" this.
        identity_public_key: match &info.identity_public_key {
            Some(key) => key.as_bytes().to_vec(),
            None => Vec::new(),
        },
        // The full per-kind state, both halves (the one carrier; there is no
        // server-mute replay).
        media_states: media_kind_states(&info.media),
    }
}

/// THE ONE per-kind mute encoder (`Participant.media_states` and
/// `ParticipantMuteUpdate.media_states`): exactly one entry per KNOWN wire
/// kind (CONVENTIONS §6), never `UNSPECIFIED`.
///
/// A served kind carries the roster's state; `server_muted_by` is set IFF that
/// kind is server-muted (it is the attribution store's own value, so the two
/// cannot disagree). An unserved known kind (`VIDEO_SCREEN` today) is the MC
/// invariant constant: NOT_PUBLISHED, not server-muted, no imposer.
#[must_use]
pub fn media_kind_states(media: &MediaStates) -> Vec<MediaKindState> {
    KNOWN_WIRE_KINDS
        .iter()
        .map(|&wire| match served_kind(wire as i32) {
            Ok(kind) => {
                let imposer = media.server.imposer(kind);
                MediaKindState {
                    media_kind: wire as i32,
                    client_state: media.client.get(kind).to_proto() as i32,
                    server_muted: imposer.is_some(),
                    server_muted_by: imposer.unwrap_or_default().to_string(),
                }
            }
            Err(KindRefusal::Unsupported | KindRefusal::Unspecified) => MediaKindState {
                media_kind: wire as i32,
                client_state: ClientMediaState::NotPublished as i32,
                server_muted: false,
                server_muted_by: String::new(),
            },
        })
        .collect()
}

/// `JoinResponse.receive_slot_caps`: exactly one entry per KNOWN wire kind
/// (CONVENTIONS §6), from `cap_of` for a served kind; an unserved known kind
/// (`VIDEO_SCREEN` today) is 0 -- this story serves no slots of it.
#[must_use]
pub fn receive_slot_caps(cap_of: impl Fn(MediaKindLabel) -> u32) -> Vec<ReceiveSlotCap> {
    KNOWN_WIRE_KINDS
        .iter()
        .map(|&wire| ReceiveSlotCap {
            media_kind: wire as i32,
            max_slots: served_kind(wire as i32).map_or(0, &cap_of),
        })
        .collect()
}

/// Encode a `ParticipantStateUpdate` for the wire.
///
/// `ParticipantJoined`, `ParticipantLeft` and `ParticipantMuteUpdate` are
/// serialized. Other variants are logged but return `None`.
///
/// THE ONE SERIALIZER for `MuteChanged`: the server-mute broadcast and the
/// client-mute fan-out both reach the wire through this function, and the
/// roster entry through [`media_kind_states`] too, so a mute applied before
/// you join and one applied after cannot be encoded two ways.
pub fn encode_participant_update(update: &ParticipantStateUpdate) -> Option<EncodedUpdate> {
    match update {
        ParticipantStateUpdate::Joined(info) => {
            let participant = roster_entry(info);
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
        ParticipantStateUpdate::MuteChanged {
            participant_id,
            media,
        } => {
            // Each `server_muted_by` is MC's OWN value — the requester's participant
            // id from the authenticated connection, never parsed off the wire —
            // so there is no emit-side truncation here. The proto's "truncate
            // before logging" obligation is the RECEIVER's.
            let (trace_parent, trace_state) = inject_current_context();
            Some(EncodedUpdate {
                payload_kind: PAYLOAD_KIND_PARTICIPANT_UPDATE_MUTED,
                server_message: ServerMessage {
                    message: Some(server_message::Message::ParticipantMuteUpdate(
                        ParticipantMuteUpdate {
                            participant_id: participant_id.clone(),
                            // A FULL snapshot on EVERY mute update, server-mute
                            // and client-mute ones alike.
                            media_states: media_kind_states(media),
                        },
                    )),
                    trace_parent,
                    trace_state,
                },
            })
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
    use crate::media_kind::{ClientMediaStates, MuteChange};
    use proto_gen::dark_tower::signaling::v1::{self, server_message, MediaKind};

    /// A state with `kind` server-muted by `who`.
    fn server_muted(mut media: MediaStates, kind: MediaKindLabel, who: &str) -> MediaStates {
        let _ = media.server.apply(MuteChange::one(kind, true), who);
        media
    }

    fn entry(kind: MediaKind, client: ClientMediaState, by: &str) -> MediaKindState {
        MediaKindState {
            media_kind: kind as i32,
            client_state: client as i32,
            server_muted: !by.is_empty(),
            server_muted_by: by.to_string(),
        }
    }

    /// The top-level field numbers present in an encoded message (a minimal
    /// tag walker: varint and length-delimited fields only, which is all
    /// these messages use).
    fn field_numbers(bytes: &[u8]) -> Vec<u64> {
        fn varint(b: &[u8], i: &mut usize) -> u64 {
            let mut v = 0u64;
            let mut shift = 0;
            loop {
                let byte = b[*i];
                *i += 1;
                v |= u64::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 {
                    return v;
                }
                shift += 7;
            }
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let key = varint(bytes, &mut i);
            out.push(key >> 3);
            match key & 7 {
                0 => {
                    varint(bytes, &mut i);
                }
                2 => {
                    let len = usize::try_from(varint(bytes, &mut i)).unwrap();
                    i += len;
                }
                other => panic!("unexpected wire type {other}"),
            }
        }
        out
    }

    fn make_participant_info(id: &str, name: &str) -> ParticipantInfo {
        ParticipantInfo {
            participant_id: id.to_string(),
            user_id: "user-1".to_string(),
            display_name: name.to_string(),
            media: MediaStates::default(),
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

    /// A `MuteChanged` encodes as a full kind-keyed snapshot: one entry per
    /// known kind, each kind's own client state and imposer, screen as the
    /// NOT_PUBLISHED constant.
    #[test]
    fn test_encode_mute_changed_is_a_participant_mute_update() {
        let media = server_muted(
            MediaStates {
                client: ClientMediaStates::from_declared([Some(true), None]),
                ..MediaStates::default()
            },
            MediaKindLabel::Audio,
            "host-1",
        );
        let update = ParticipantStateUpdate::MuteChanged {
            participant_id: "part-1".to_string(),
            media,
        };
        let encoded = encode_participant_update(&update).expect("MuteChanged is wire-visible");
        assert_eq!(encoded.payload_kind, PAYLOAD_KIND_PARTICIPANT_UPDATE_MUTED);
        let Some(server_message::Message::ParticipantMuteUpdate(m)) =
            encoded.server_message.message
        else {
            panic!("expected a ParticipantMuteUpdate");
        };
        assert_eq!(
            m,
            ParticipantMuteUpdate {
                participant_id: "part-1".to_string(),
                media_states: vec![
                    entry(MediaKind::Audio, ClientMediaState::ClientMuted, "host-1"),
                    entry(MediaKind::VideoCamera, ClientMediaState::NotPublished, ""),
                    entry(MediaKind::VideoScreen, ClientMediaState::NotPublished, ""),
                ],
            }
        );
    }

    /// The encoder's invariants, over every state combination: exactly one
    /// entry per known kind, never UNSPECIFIED, `server_muted_by` set iff
    /// `server_muted`, and VIDEO_SCREEN always the MC constant.
    #[test]
    fn media_kind_states_is_complete_and_server_muted_by_tracks_server_muted() {
        for declared in [[None, None], [Some(false), None], [Some(true), Some(false)]] {
            for muted in [
                None,
                Some(MediaKindLabel::Audio),
                Some(MediaKindLabel::Video),
            ] {
                let mut media = MediaStates {
                    client: ClientMediaStates::from_declared(declared),
                    ..MediaStates::default()
                };
                if let Some(kind) = muted {
                    media = server_muted(media, kind, "h");
                }
                let states = media_kind_states(&media);
                let kinds: Vec<i32> = states.iter().map(|s| s.media_kind).collect();
                assert_eq!(
                    kinds,
                    KNOWN_WIRE_KINDS
                        .iter()
                        .map(|k| *k as i32)
                        .collect::<Vec<_>>()
                );
                for s in &states {
                    assert_ne!(s.client_state, ClientMediaState::Unspecified as i32);
                    assert_eq!(s.server_muted, !s.server_muted_by.is_empty(), "{s:?}");
                }
                let screen = states
                    .iter()
                    .find(|s| s.media_kind == MediaKind::VideoScreen as i32)
                    .unwrap();
                assert_eq!(
                    screen,
                    &entry(MediaKind::VideoScreen, ClientMediaState::NotPublished, "")
                );
            }
        }
    }

    /// `receive_slot_caps`: one entry per known kind, the served kinds' caps
    /// as configured (0 included), screen 0.
    #[test]
    fn receive_slot_caps_carries_every_known_kind() {
        let caps = receive_slot_caps(|k| k.of([8, 0]));
        assert_eq!(
            caps,
            vec![
                ReceiveSlotCap {
                    media_kind: MediaKind::Audio as i32,
                    max_slots: 8
                },
                ReceiveSlotCap {
                    media_kind: MediaKind::VideoCamera as i32,
                    max_slots: 0
                },
                ReceiveSlotCap {
                    media_kind: MediaKind::VideoScreen as i32,
                    max_slots: 0
                },
            ]
        );
    }

    /// The retired tags 2-8 never reach the wire from MC's encoder: a
    /// `MuteChanged` encodes fields 1 and 9 only, whatever the state.
    #[test]
    fn a_mute_update_never_emits_the_retired_tags_2_to_8() {
        use prost::Message;
        let media = server_muted(
            MediaStates {
                client: ClientMediaStates::from_declared([Some(true), Some(true)]),
                ..MediaStates::default()
            },
            MediaKindLabel::Video,
            "h",
        );
        let update = ParticipantStateUpdate::MuteChanged {
            participant_id: "p".to_string(),
            media,
        };
        let encoded = encode_participant_update(&update).unwrap();
        let Some(server_message::Message::ParticipantMuteUpdate(m)) =
            encoded.server_message.message
        else {
            panic!("expected a ParticipantMuteUpdate");
        };
        let fields = field_numbers(&m.encode_to_vec());
        assert!(!fields.is_empty());
        assert!(fields.iter().all(|f| *f == 1 || *f == 9), "{fields:?}");
        let entry = roster_entry(&make_participant_info("p", "P")).encode_to_vec();
        assert!(
            !field_numbers(&entry).iter().any(|f| *f == 7 || *f == 8),
            "the roster's retired client-mute tags are never written"
        );
    }

    /// ONE roster builder for both carriers: the full per-kind state rides it
    /// (definite from admission: NOT_PUBLISHED explicit), and
    /// `ParticipantJoined` carries exactly that entry.
    #[test]
    fn the_roster_entry_carries_one_definite_state_per_known_kind() {
        let mut info = make_participant_info("part-9", "Nine");
        let receive_only = roster_entry(&info);
        assert!(receive_only
            .media_states
            .iter()
            .all(|s| s.client_state == ClientMediaState::NotPublished as i32 && !s.server_muted));
        info.media = server_muted(
            MediaStates {
                client: ClientMediaStates::from_declared([Some(false), Some(true)]),
                ..MediaStates::default()
            },
            MediaKindLabel::Audio,
            "host",
        );
        let entry_ = roster_entry(&info);
        assert_eq!(
            entry_.media_states,
            vec![
                entry(MediaKind::Audio, ClientMediaState::Live, "host"),
                entry(MediaKind::VideoCamera, ClientMediaState::ClientMuted, ""),
                entry(MediaKind::VideoScreen, ClientMediaState::NotPublished, ""),
            ]
        );
        let joined = encode_participant_update(&ParticipantStateUpdate::Joined(info)).unwrap();
        let Some(server_message::Message::ParticipantJoined(j)) = joined.server_message.message
        else {
            panic!("expected ParticipantJoined");
        };
        assert_eq!(j.participant, Some(entry_));
    }

    #[test]
    fn payload_kinds_are_distinct_and_share_the_update_prefix() {
        let kinds = [
            PAYLOAD_KIND_PARTICIPANT_UPDATE_JOINED,
            PAYLOAD_KIND_PARTICIPANT_UPDATE_LEFT,
            PAYLOAD_KIND_PARTICIPANT_UPDATE_MUTED,
        ];
        let unique: std::collections::HashSet<_> = kinds.iter().collect();
        assert_eq!(unique.len(), kinds.len());
        assert!(kinds.iter().all(|k| k.starts_with("participant_update_")));
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
