//! Wire-shape tests for the ADR-0036 signalling contract.
//!
//! These assert the *contract*, not behaviour: every message the media path
//! adds must survive an encode/decode round trip with its fields intact, the
//! named slot states must be distinguishable on the wire, and the redacting
//! `Debug` impls must not print secrets. Pure encode/decode — fixed inputs, no
//! timing, no clock, deterministic.
//!
//! What these tests deliberately do NOT assert: the 1..=65535 `sender_id`
//! bound and the 8-bit `stream_number` bound are MC runtime invariants (story
//! tasks 10 and 14), not proto-layer checks. Asserting rejection here would
//! encode a check the contract does not make.

// Test code: a failed decode or an unexpected oneof variant IS the failure
// signal here, so panicking is the assertion mechanism rather than a defect.
// Same posture as `crates/media-protocol/tests/reject_reasons.rs` and
// `crates/mc-service/tests/join_tests.rs`.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

use proto_gen::dark_tower::signaling::v1::*;
use proto_gen::Message;

mod common;
use common::{roundtrip, top_level_field_numbers, UNKNOWN_ENUM_VALUE};

/// The number `SLOT_STATE_ZERO_REQUESTED` occupied before it was reserved
/// (`signaling.proto`, `reserved 5; reserved "SLOT_STATE_ZERO_REQUESTED";` in
/// `SlotState`). A raw literal on purpose: the name no longer exists in
/// generated code, which is what the reservation tests below prove.
const RESERVED_SLOT_STATE_ZERO_REQUESTED: i32 = 5;

/// The number `SLOT_STATE_SOURCE_MUTED` occupied before MC-B deleted it and
/// reserved number and name (`reserved 2; reserved "SLOT_STATE_SOURCE_MUTED";`).
/// A raw literal for the same reason as the one above.
const RESERVED_SLOT_STATE_SOURCE_MUTED: i32 = 2;

// ---------------------------------------------------------------------------
// Receive capability (§6)
// ---------------------------------------------------------------------------

#[test]
fn receive_capability_roundtrips_with_pin_absent_and_present() {
    let cap = ReceiveCapability {
        slots: vec![
            ReceiveSlot {
                slot_id: 0,
                media_kind: MediaKind::Audio as i32,
                pinned_sender_id: None,
            },
            ReceiveSlot {
                slot_id: 65535,
                media_kind: MediaKind::VideoCamera as i32,
                pinned_sender_id: Some(4242),
            },
        ],
    };

    let out = roundtrip(&cap);
    assert_eq!(out.slots.len(), 2);
    // Absence must survive as absence: the pin is an optional per-slot
    // parameter, so "no pin" and "pinned to sender 0" are different states.
    assert_eq!(out.slots[0].pinned_sender_id, None);
    assert_eq!(out.slots[1].pinned_sender_id, Some(4242));
    assert_eq!(out.slots[1].slot_id, 65535);
    assert_eq!(out.slots[1].media_kind, MediaKind::VideoCamera as i32);
}

// ---------------------------------------------------------------------------
// Send directive (§5)
// ---------------------------------------------------------------------------

#[test]
fn send_directive_roundtrips_encoding_targets_and_header_version() {
    let directive = SendDirective {
        header_version: 2,
        streams: vec![SendStream {
            stream_number: 3,
            media_kind: MediaKind::Audio as i32,
            encoding: Some(EncodingParameters {
                codec: Codec::Opus as i32,
                max_bitrate_bps: 32_000,
                width: 0,
                height: 0,
                frame_rate: 0,
                min_keyframe_interval_ms: None,
            }),
            targets: vec![
                SendTarget {
                    media_handler_url: "https://mh-1.example/mh".to_string(),
                    transport_mode: TransportMode::Datagram as i32,
                },
                SendTarget {
                    media_handler_url: "https://mh-2.example/mh".to_string(),
                    transport_mode: TransportMode::StreamPerGroup as i32,
                },
            ],
        }],
    };

    let out = roundtrip(&directive);
    assert_eq!(out.header_version, 2);
    let stream = &out.streams[0];
    assert_eq!(stream.stream_number, 3);
    assert_eq!(stream.encoding.as_ref().unwrap().codec, Codec::Opus as i32);
    assert_eq!(stream.encoding.as_ref().unwrap().max_bitrate_bps, 32_000);
    assert_eq!(stream.targets.len(), 2);
    assert_eq!(
        stream.targets[1].transport_mode,
        TransportMode::StreamPerGroup as i32
    );
}

#[test]
fn send_directive_empty_target_set_means_send_nothing() {
    // ADR-0036 §5: an empty target set is a meaningful instruction, not a
    // malformed message. It must survive the round trip as empty.
    let directive = SendDirective {
        header_version: 2,
        streams: vec![SendStream {
            stream_number: 0,
            media_kind: MediaKind::VideoCamera as i32,
            encoding: None,
            targets: Vec::new(),
        }],
    };

    let out = roundtrip(&directive);
    assert!(out.streams[0].targets.is_empty());
}

// ---------------------------------------------------------------------------
// Stream assignment and slot state (§6, §7)
// ---------------------------------------------------------------------------

#[test]
fn stream_assignment_roundtrips_full_attribution_shape() {
    let assignments = StreamAssignments {
        assignments: vec![StreamAssignment {
            slot_id: 7,
            sender_id: Some(4242),
            media_kind: MediaKind::VideoCamera as i32,
            media_handler_url: "https://mh-1.example/mh".to_string(),
            slot_state: SlotState::Active as i32,
            switch_command_id: None,
        }],
        unreachable_sender_ids: Vec::new(),
    };

    let out = roundtrip(&assignments);
    let a = &out.assignments[0];
    assert_eq!(a.slot_id, 7);
    assert_eq!(a.sender_id, Some(4242));
    assert_eq!(a.media_handler_url, "https://mh-1.example/mh");
    assert_eq!(a.slot_state, SlotState::Active as i32);
}

#[test]
fn switch_pending_carries_the_requesting_command_identifier() {
    // ADR-0036 §7: completion is reported by command id, never by state
    // description, because "A to B" is ambiguous across a revert-and-reswitch.
    let assignment = StreamAssignment {
        slot_id: 1,
        sender_id: Some(9),
        media_kind: MediaKind::VideoCamera as i32,
        media_handler_url: "https://mh-1.example/mh".to_string(),
        slot_state: SlotState::SwitchPending as i32,
        switch_command_id: Some(u64::MAX),
    };

    let out = roundtrip(&assignment);
    assert_eq!(out.slot_state, SlotState::SwitchPending as i32);
    assert_eq!(out.switch_command_id, Some(u64::MAX));
}

#[test]
fn assignment_without_a_source_keeps_sender_absent() {
    // The states that mean "no source" must be expressible without inventing a
    // sender id. Absent stays absent — never coerced to 0.
    for state in [
        SlotState::FewerSourcesThanSlots,
        SlotState::Unavailable,
        SlotState::SourceUnreachable,
    ] {
        let out = roundtrip(&StreamAssignment {
            slot_id: 2,
            sender_id: None,
            media_kind: MediaKind::Audio as i32,
            media_handler_url: String::new(),
            slot_state: state as i32,
            switch_command_id: None,
        });
        assert_eq!(
            out.sender_id, None,
            "state {state:?} must not fabricate a sender_id"
        );
    }
}

#[test]
fn every_named_slot_state_is_distinct_and_the_story_3_values_are_pinned() {
    // ADR-0036 §6 states are on the wire because absence of frames is not a
    // signal. The flat enum (story 3 wire freeze) must keep every NAMED value
    // distinct, never collide with the fail-closed zero, and carry the four
    // frozen story-3 numbers exactly -- a renumber would silently re-point a
    // published value.
    let states = [
        SlotState::Active,
        SlotState::WithheldByCongestion,
        SlotState::FewerSourcesThanSlots,
        SlotState::SourceUnreachable,
        SlotState::SwitchPending,
        SlotState::AvatarNoVideo,
        SlotState::Unavailable,
        SlotState::AvatarClientMuted,
        SlotState::AvatarServerMuted,
    ];

    let mut seen: Vec<i32> = states.iter().map(|s| *s as i32).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        states.len(),
        "every named slot state must be distinct on the wire"
    );
    assert!(
        !seen.contains(&(SlotState::Unspecified as i32)),
        "no real state may collide with the fail-closed zero value"
    );
    assert_eq!(SlotState::Unspecified as i32, 0);
    assert_eq!(SlotState::AvatarNoVideo as i32, 8);
    assert_eq!(SlotState::Unavailable as i32, 9);
    assert_eq!(SlotState::AvatarClientMuted as i32, 10);
    assert_eq!(SlotState::AvatarServerMuted as i32, 11);
    assert!(
        !seen.contains(&RESERVED_SLOT_STATE_ZERO_REQUESTED),
        "no named state may occupy the reserved number"
    );
    assert!(
        !seen.contains(&RESERVED_SLOT_STATE_SOURCE_MUTED),
        "no named state may occupy the number MC-B reserved for SOURCE_MUTED"
    );
}

// ---------------------------------------------------------------------------
// Retyped publish/media streams (§5): the Rule-R fresh-tag repoint
// ---------------------------------------------------------------------------

#[test]
fn media_stream_roundtrips_on_the_retyped_fresh_tags() {
    // `stream_type`/`metadata` were reserved and `media_kind` (tag 4) +
    // `encoding` (tag 5) took fresh tags — the exact zero-meaning hazard Rule R
    // guards. Prove the new shape survives the wire, so a regression to the old
    // tags fails here rather than silently in a consumer.
    let out = roundtrip(&MediaStream {
        stream_id: "s-1".to_string(),
        media_kind: MediaKind::VideoCamera as i32,
        encoding: Some(EncodingParameters {
            codec: Codec::Vp9 as i32,
            max_bitrate_bps: 1_500_000,
            width: 1280,
            height: 720,
            frame_rate: 30,
            min_keyframe_interval_ms: None,
        }),
    });
    assert_eq!(out.stream_id, "s-1");
    assert_eq!(out.media_kind, MediaKind::VideoCamera as i32);
    assert_eq!(out.encoding.as_ref().unwrap().codec, Codec::Vp9 as i32);
    assert_eq!(out.encoding.as_ref().unwrap().width, 1280);
}

#[test]
fn publish_stream_roundtrips_on_the_retyped_fresh_tags() {
    let out = roundtrip(&PublishStream {
        stream_id: "s-2".to_string(),
        media_kind: MediaKind::Audio as i32,
        encoding: Some(EncodingParameters {
            codec: Codec::Opus as i32,
            max_bitrate_bps: 32_000,
            width: 0,
            height: 0,
            frame_rate: 0,
            min_keyframe_interval_ms: None,
        }),
    });
    assert_eq!(out.stream_id, "s-2");
    assert_eq!(out.media_kind, MediaKind::Audio as i32);
    assert_eq!(out.encoding.as_ref().unwrap().codec, Codec::Opus as i32);
}

// ---------------------------------------------------------------------------
// Join: identity key, KEK, sender id (§2, §3, §4)
// ---------------------------------------------------------------------------

#[test]
fn join_request_roundtrips_raw_identity_public_key() {
    let key = vec![0xABu8; 32];
    let out = roundtrip(&JoinRequest {
        meeting_id: "m-1".to_string(),
        join_token: "jwt".to_string(),
        participant_name: "A".to_string(),
        capabilities: Some(ParticipantCapabilities {
            supported_codecs: vec![Codec::Opus as i32, Codec::Vp9 as i32],
            supported_header_versions: vec![2],
        }),
        correlation_id: String::new(),
        binding_token: String::new(),
        identity_public_key: key.clone(),
        receive_capability: None,
        published_kinds: Vec::new(),
    });

    assert_eq!(out.identity_public_key, key);
    let caps = out.capabilities.unwrap();
    assert_eq!(
        caps.supported_codecs,
        vec![Codec::Opus as i32, Codec::Vp9 as i32]
    );
    assert_eq!(caps.supported_header_versions, vec![2]);
}

#[test]
fn join_response_roundtrips_kek_generation_and_sender_id() {
    let kek = vec![0x5Au8; 32];
    let out = roundtrip(&JoinResponse {
        participant_id: "p-1".to_string(),
        existing_participants: vec![Participant {
            participant_id: "p-2".to_string(),
            name: "B".to_string(),
            streams: Vec::new(),
            joined_at: 0,
            sender_id: Some(2),
            identity_public_key: vec![0x11; 32],
            media_states: vec![kind_state(MediaKind::Audio, ClientMediaState::Live, "")],
        }],
        media_servers: Vec::new(),
        correlation_id: "c-1".to_string(),
        binding_token: "b-1".to_string(),
        sender_id: Some(65535),
        meeting_kek: kek.clone(),
        kek_generation: 65535,
        kek_rotation_debounce_seconds: 0,
        receive_slot_caps: Vec::new(),
    });

    assert_eq!(out.sender_id, Some(65535));
    assert_eq!(out.meeting_kek, kek);
    assert_eq!(out.kek_generation, 65535);
    // The roster carries both halves of the attribution chain and no other key
    // material (ADR-0036 §4).
    assert_eq!(out.existing_participants[0].sender_id, Some(2));
    assert_eq!(out.existing_participants[0].identity_public_key.len(), 32);
}

#[test]
fn sender_id_absent_decodes_as_absent_not_zero() {
    // Regression guard for the eliminated zero sentinel. This fails if anyone
    // drops `optional` (a bare uint32 makes absent == 0) or coerces absence to
    // 0 on the way through.
    //
    // Invariant it protects: absent means "not yet assigned"; 0 must never
    // decode to a live sender, because identical sender ids collide on the key
    // id and therefore on the derived AES-GCM wrap nonce under one KEK.
    let out = roundtrip(&JoinResponse {
        participant_id: "p-1".to_string(),
        sender_id: None,
        ..Default::default()
    });
    assert_eq!(out.sender_id, None);
    assert_ne!(out.sender_id, Some(0));
}

#[test]
fn sender_id_zero_survives_as_a_present_zero_not_absence() {
    // The load-bearing half of `optional` presence: `Some(0)` must round-trip
    // as `Some(0)`, distinct from `None`. If the wire collapsed `Some(0)` to
    // `None`, MC's task-10/14 rejection of an attacker-supplied `Some(0)`
    // (@security: "a peer can send Some(0), and the wrap-nonce collision
    // follows from the value, not the presence bit") would never see the value
    // it exists to reject. Proved on both surfaces that receive an
    // attacker-supplied id: the join response and the per-slot pin.
    let out = roundtrip(&JoinResponse {
        sender_id: Some(0),
        ..Default::default()
    });
    assert_eq!(out.sender_id, Some(0));
    assert_ne!(out.sender_id, None);

    let cap = roundtrip(&ReceiveCapability {
        slots: vec![ReceiveSlot {
            slot_id: 0,
            media_kind: MediaKind::Audio as i32,
            pinned_sender_id: Some(0),
        }],
    });
    assert_eq!(cap.slots[0].pinned_sender_id, Some(0));
    assert_ne!(cap.slots[0].pinned_sender_id, None);
}

/// One `MediaKindState` entry.
fn kind_state(kind: MediaKind, client: ClientMediaState, by: &str) -> MediaKindState {
    MediaKindState {
        media_kind: kind as i32,
        client_state: client as i32,
        server_muted: !by.is_empty(),
        server_muted_by: by.to_string(),
    }
}

/// ADR-0036 §5 amendment 2026-10-25: the per-kind state is a kind-keyed
/// `repeated MediaKindState` on BOTH carriers (`Participant`,
/// `ParticipantMuteUpdate`, the same field name and tag 9). Order, every
/// client state, a present `false` and the imposer survive; the wire does not
/// dedupe (a receiver must SEE a duplicate to treat it as a protocol error).
#[test]
fn media_states_roundtrip_on_both_carriers() {
    let states = vec![
        kind_state(MediaKind::Audio, ClientMediaState::ClientMuted, "host"),
        kind_state(MediaKind::VideoCamera, ClientMediaState::Live, ""),
        kind_state(MediaKind::VideoScreen, ClientMediaState::NotPublished, ""),
        kind_state(MediaKind::Audio, ClientMediaState::Live, ""),
    ];
    let p = roundtrip(&Participant {
        participant_id: "p-1".to_string(),
        media_states: states.clone(),
        ..Default::default()
    });
    assert_eq!(p.media_states, states);
    let u = roundtrip(&ParticipantMuteUpdate {
        participant_id: "p-1".to_string(),
        media_states: states.clone(),
    });
    assert_eq!(u.media_states, states);
    assert_eq!(
        top_level_field_numbers(&u.encode_to_vec()),
        vec![1, 9, 9, 9, 9]
    );
}

/// `ClientMediaState`'s numbers are pinned, and its zero value is the
/// malformed `UNSPECIFIED`: an unset entry decodes as 0, never as a state.
#[test]
fn client_media_state_zero_is_unspecified_and_the_values_are_pinned() {
    assert_eq!(ClientMediaState::Unspecified as i32, 0);
    assert_eq!(ClientMediaState::NotPublished as i32, 1);
    assert_eq!(ClientMediaState::Live as i32, 2);
    assert_eq!(ClientMediaState::ClientMuted as i32, 3);
    let out = roundtrip(&MediaKindState::default());
    assert_eq!(out.client_state, ClientMediaState::Unspecified as i32);
}

/// The retired tags are never emitted: a fully populated update encodes 1 and
/// 9 only, so an old reader can never decode a stale per-kind field from a new
/// sender (Participant 7/8, ParticipantMuteUpdate 2-8 reserved).
#[test]
fn participant_mute_update_never_encodes_the_retired_tags() {
    let full = ParticipantMuteUpdate {
        participant_id: "p-1".to_string(),
        media_states: vec![kind_state(
            MediaKind::Audio,
            ClientMediaState::ClientMuted,
            "h",
        )],
    };
    assert_eq!(top_level_field_numbers(&full.encode_to_vec()), vec![1, 9]);
    let roster = Participant {
        participant_id: "p-1".to_string(),
        media_states: full.media_states.clone(),
        ..Default::default()
    };
    let fields = top_level_field_numbers(&roster.encode_to_vec());
    assert!(!fields.contains(&7) && !fields.contains(&8), "{fields:?}");
}

#[test]
fn sender_id_is_genuinely_uint32_on_the_wire() {
    // A 65535-only test cannot distinguish `uint32` from `uint16`. Encoding a
    // value in the 65536..u32::MAX gap must round-trip FAITHFULLY: the 16-bit
    // bound is an MC runtime invariant (task 10), not a wire truncation.
    let out = roundtrip(&JoinResponse {
        sender_id: Some(u32::MAX),
        ..Default::default()
    });
    assert_eq!(out.sender_id, Some(u32::MAX));
}

#[test]
fn meeting_kek_update_roundtrips() {
    let kek = vec![0x77u8; 32];
    let out = roundtrip(&MeetingKekUpdate {
        meeting_kek: kek.clone(),
        kek_generation: 7,
        kek_rotation_debounce_seconds: 0,
    });
    assert_eq!(out.meeting_kek, kek);
    assert_eq!(out.kek_generation, 7);
}

// ---------------------------------------------------------------------------
// Story 2: W on the wire (R-14) and the per-subscriber unreachable set (R-33)
// ---------------------------------------------------------------------------

/// W survives by VALUE on both carriers. Distinct values per carrier, so a
/// codegen slip that wired one message's field to the other's tag reds here.
#[test]
fn kek_rotation_debounce_seconds_roundtrips_on_both_carriers() {
    let join = roundtrip(&JoinResponse {
        participant_id: "p-1".to_string(),
        kek_rotation_debounce_seconds: 60,
        ..Default::default()
    });
    assert_eq!(join.kek_rotation_debounce_seconds, 60);

    let push = roundtrip(&MeetingKekUpdate {
        meeting_kek: vec![0x77u8; 32],
        kek_generation: 3,
        kek_rotation_debounce_seconds: 45,
    });
    assert_eq!(push.kek_rotation_debounce_seconds, 45);
    // The additive field does not disturb its neighbours.
    assert_eq!(push.kek_generation, 3);
}

/// Zero and absent are ONE observable (a non-optional scalar), which is exactly
/// why the contract makes the client floor-substitute on either: an older MC
/// that never sends the field is indistinguishable from one sending 0. If this
/// field ever becomes `optional`, this test's premise changes and the client's
/// "zero-or-absent" rule must be re-read.
#[test]
fn kek_rotation_debounce_seconds_absent_and_zero_are_one_observable() {
    let absent = roundtrip(&MeetingKekUpdate {
        meeting_kek: vec![0x77u8; 32],
        kek_generation: 3,
        ..Default::default()
    });
    let zero = roundtrip(&MeetingKekUpdate {
        meeting_kek: vec![0x77u8; 32],
        kek_generation: 3,
        kek_rotation_debounce_seconds: 0,
    });
    assert_eq!(absent.kek_rotation_debounce_seconds, 0);
    assert_eq!(absent.encode_to_vec(), zero.encode_to_vec());
}

/// More than one entry, values AND order checked: a single-element case cannot
/// catch repeated-field handling defects.
#[test]
fn unreachable_sender_ids_roundtrip_by_value_and_order() {
    let out = roundtrip(&StreamAssignments {
        assignments: vec![StreamAssignment {
            slot_id: 1,
            sender_id: Some(3),
            media_kind: MediaKind::Audio as i32,
            media_handler_url: "https://mh-0.example/mh".to_string(),
            slot_state: SlotState::Active as i32,
            switch_command_id: None,
        }],
        unreachable_sender_ids: vec![9, 2, 65535],
    });
    assert_eq!(out.unreachable_sender_ids, vec![9, 2, 65535]);
    assert_eq!(out.assignments.len(), 1);
}

/// The contract says a message MAY carry zero assignments and a non-empty
/// unreachable set ("everyone else is on another handler"). That shape must be
/// well-formed on the wire, not collapse into an empty message.
#[test]
fn unreachable_set_survives_with_zero_assignments() {
    let msg = StreamAssignments {
        assignments: Vec::new(),
        unreachable_sender_ids: vec![4, 5],
    };
    assert!(!msg.encode_to_vec().is_empty());
    let out = roundtrip(&msg);
    assert!(out.assignments.is_empty());
    assert_eq!(out.unreachable_sender_ids, vec![4, 5]);
}

// ---------------------------------------------------------------------------
// Envelopes and terminology
// ---------------------------------------------------------------------------

#[test]
fn server_mute_request_roundtrips_through_the_client_envelope() {
    let envelope = ClientMessage {
        message: Some(client_message::Message::ServerMuteRequest(
            ServerMuteRequest {
                participant_id: "p-2".to_string(),
                changes: vec![
                    mute_change(MediaKind::Audio, true),
                    mute_change(MediaKind::VideoCamera, false),
                ],
                reason: "policy".to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    match roundtrip(&envelope).message {
        Some(client_message::Message::ServerMuteRequest(m)) => {
            assert_eq!(m.participant_id, "p-2");
            assert_eq!(
                m.changes,
                vec![
                    mute_change(MediaKind::Audio, true),
                    mute_change(MediaKind::VideoCamera, false),
                ]
            );
        }
        other => panic!("expected ServerMuteRequest, got {other:?}"),
    }
}

#[test]
fn receive_capability_roundtrips_through_the_client_envelope() {
    // The new ClientMessage variant (tag 12) must be proven through its
    // envelope, not just as a bare message — the variant wiring at tag 12 is
    // what the server dispatches on. Symmetric with the ServerMessage variants
    // exercised below.
    let envelope = ClientMessage {
        message: Some(client_message::Message::ReceiveCapability(
            ReceiveCapability {
                slots: vec![ReceiveSlot {
                    slot_id: 3,
                    media_kind: MediaKind::VideoCamera as i32,
                    pinned_sender_id: Some(9),
                }],
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    match roundtrip(&envelope).message {
        Some(client_message::Message::ReceiveCapability(cap)) => {
            assert_eq!(cap.slots.len(), 1);
            assert_eq!(cap.slots[0].slot_id, 3);
            assert_eq!(cap.slots[0].pinned_sender_id, Some(9));
        }
        other => panic!("expected ReceiveCapability, got {other:?}"),
    }
}

#[test]
fn new_server_variants_roundtrip_through_the_server_envelope() {
    for message in [
        server_message::Message::StreamAssignments(StreamAssignments::default()),
        server_message::Message::SendDirective(SendDirective::default()),
        server_message::Message::MeetingKekUpdate(MeetingKekUpdate::default()),
    ] {
        let out = roundtrip(&ServerMessage {
            message: Some(message),
            trace_parent: "00-trace".to_string(),
            trace_state: String::new(),
        });
        assert!(out.message.is_some());
        // Envelope-level trace context is unaffected by the variant set.
        assert_eq!(out.trace_parent, "00-trace");
    }
}

#[test]
fn leave_reason_zero_decodes_as_unspecified() {
    // `LeaveReason` renumbered to free 0 for UNSPECIFIED. Value 0 is the one
    // change proto3 forces to be a re-point rather than a reservation, so its
    // new meaning is asserted rather than assumed.
    assert_eq!(LeaveReason::Unspecified as i32, 0);
    let out = roundtrip(&ParticipantLeft {
        participant_id: "p-1".to_string(),
        reason: 0,
    });
    assert_eq!(out.reason, LeaveReason::Unspecified as i32);
    // The real reasons live above the reserved 1..=4 band.
    assert_eq!(LeaveReason::Voluntary as i32, 5);
    assert_eq!(LeaveReason::Timeout as i32, 9);
}

#[test]
fn error_code_zero_keeps_its_number_and_meaning() {
    // Unlike LeaveReason, ErrorCode is not renumbered: UNKNOWN -> UNSPECIFIED
    // is the same number with the same meaning.
    assert_eq!(ErrorCode::Unspecified as i32, 0);
    assert_eq!(ErrorCode::InvalidRequest as i32, 1);
    assert_eq!(ErrorCode::StreamError as i32, 8);
}

// ---------------------------------------------------------------------------
// Redaction (the control stood up in build.rs + lib.rs)
// ---------------------------------------------------------------------------

/// W is added to both hand-written redacting `Debug` impls: it must render in
/// the clear (it is a duration, and the operator triaging a floor/ceiling
/// substitution needs it) while the KEK beside it stays a length placeholder.
/// Pins both halves, because the impls are hand-maintained field lists with no
/// compiler-enforced exhaustiveness.
#[test]
fn debug_shows_w_in_the_clear_and_still_redacts_the_kek() {
    let join = JoinResponse {
        meeting_kek: vec![0xC3u8; 32],
        kek_rotation_debounce_seconds: 60,
        ..Default::default()
    };
    let push = MeetingKekUpdate {
        meeting_kek: vec![0xC3u8; 32],
        kek_generation: 2,
        kek_rotation_debounce_seconds: 45,
    };
    for (rendered, w) in [(format!("{join:?}"), 60), (format!("{push:?}"), 45)] {
        assert!(
            rendered.contains(&format!("kek_rotation_debounce_seconds: {w}")),
            "W must be visible: {rendered}"
        );
        assert!(rendered.contains("<redacted 32 B>"));
        assert!(
            !rendered.contains("195, 195"),
            "KEK bytes must not be printed"
        );
    }
}

#[test]
fn debug_never_prints_the_meeting_kek() {
    let kek = vec![0xC3u8; 32];
    let response = JoinResponse {
        participant_id: "p-1".to_string(),
        correlation_id: "c-1".to_string(),
        binding_token: "super-secret-binding-token".to_string(),
        meeting_kek: kek,
        kek_generation: 3,
        ..Default::default()
    };

    let rendered = format!("{response:?}");
    assert!(!rendered.contains("c3c3"), "KEK bytes must not be printed");
    assert!(
        !rendered.contains("195, 195"),
        "KEK bytes must not be printed"
    );
    assert!(
        !rendered.contains("super-secret-binding-token"),
        "the session-binding credential must not be printed"
    );
    assert!(
        rendered.contains("<redacted 32 B>"),
        "KEK must render as a length placeholder"
    );
    // Metadata stays legible: the generation, and the reconnect correlation key
    // an operator triages with.
    assert!(rendered.contains("kek_generation: 3"));
    assert!(
        rendered.contains("c-1"),
        "correlation_id is an identifier, not a credential"
    );
}

#[test]
fn debug_never_prints_join_credentials() {
    let request = JoinRequest {
        meeting_id: "m-1".to_string(),
        join_token: "eyJhbGciOiJFZERTQSJ9.super-secret-jwt".to_string(),
        binding_token: "super-secret-binding-token".to_string(),
        identity_public_key: vec![0x11; 32],
        participant_name: "Ada Lovelace".to_string(),
        ..Default::default()
    };

    let rendered = format!("{request:?}");
    assert!(!rendered.contains("super-secret-jwt"));
    assert!(!rendered.contains("super-secret-binding-token"));
    // participant_name is PII and must not print in the clear.
    assert!(!rendered.contains("Ada Lovelace"));
    assert!(rendered.contains("[REDACTED]"));
    assert!(rendered.contains("m-1"), "non-secret fields stay legible");
}

#[test]
fn debug_never_prints_the_roster_names_or_keys() {
    // `Participant` derives `Debug`, so a naive `existing_participants` field
    // would dump every member's name (PII, ADR-0011 membership disclosure) and
    // raw identity key. The impl renders a count instead.
    let response = JoinResponse {
        participant_id: "p-1".to_string(),
        existing_participants: vec![
            Participant {
                participant_id: "p-2".to_string(),
                name: "Grace Hopper".to_string(),
                identity_public_key: vec![0xEE; 32],
                ..Default::default()
            },
            Participant {
                participant_id: "p-3".to_string(),
                name: "Katherine Johnson".to_string(),
                identity_public_key: vec![0xDD; 32],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains("Grace Hopper"),
        "roster names must not print"
    );
    assert!(!rendered.contains("Katherine Johnson"));
    assert!(
        !rendered.contains("238, 238"),
        "raw key bytes must not print"
    );
    // The count stays legible — roster size is triage-useful, membership is not.
    assert!(rendered.contains("existing_participants_count: 2"));
}

#[test]
fn debug_never_prints_participant_pii_through_the_join_broadcast() {
    // `ParticipantJoined` wraps the same `Participant` and rides inside
    // `ServerMessage`'s transitive Debug on the join fan-out — the path that
    // broadcasts a new member's identity key to everyone. Redacting only
    // `JoinResponse` would leave this printable one participant at a time.
    let envelope = ServerMessage {
        message: Some(server_message::Message::ParticipantJoined(
            ParticipantJoined {
                participant: Some(Participant {
                    participant_id: "p-9".to_string(),
                    name: "Margaret Hamilton".to_string(),
                    identity_public_key: vec![0xEE; 32],
                    sender_id: Some(7),
                    ..Default::default()
                }),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    let rendered = format!("{envelope:?}");
    assert!(
        !rendered.contains("Margaret Hamilton"),
        "roster name must not print"
    );
    assert!(
        !rendered.contains("238, 238"),
        "raw key bytes must not print"
    );
    assert!(rendered.contains("[REDACTED]"));
    // Non-sensitive fields stay legible.
    assert!(rendered.contains("p-9"));
    assert!(rendered.contains("identity_public_key_len: 32"));
}

#[test]
fn debug_never_prints_the_mh_connect_join_token() {
    // `MhConnectRequest.join_token` is a bearer meeting JWT and rides inside
    // `MhClientMessage`, which derives `Debug` transitively. Same redaction as
    // the join path, asserted through the envelope that would leak it.
    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: "eyJhbGciOiJFZERTQSJ9.super-secret-mh-jwt".to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    let rendered = format!("{envelope:?}");
    assert!(!rendered.contains("super-secret-mh-jwt"));
    assert!(rendered.contains("<redacted"));
}

#[test]
fn debug_redaction_survives_the_enclosing_envelope() {
    // The leak path that matters is `{:?}` on a whole ServerMessage, not on
    // the inner message someone remembered to be careful with.
    let envelope = ServerMessage {
        message: Some(server_message::Message::MeetingKekUpdate(
            MeetingKekUpdate {
                meeting_kek: vec![0xC3u8; 32],
                kek_generation: 1,
                kek_rotation_debounce_seconds: 60,
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    let rendered = format!("{envelope:?}");
    assert!(!rendered.contains("195, 195"));
    assert!(rendered.contains("<redacted 32 B>"));
}

// ---------------------------------------------------------------------------
// Story 3 (see-each-other): per-kind mute presence (R-15)
// ---------------------------------------------------------------------------

/// One `MuteChange` entry.
fn mute_change(kind: MediaKind, muted: bool) -> MuteChange {
    MuteChange {
        media_kind: kind as i32,
        muted,
    }
}

/// A kind-keyed request list survives by value AND order, duplicates and an
/// explicit `muted = false` included, on BOTH request messages (an absent kind
/// is "unchanged"; a present false is an unmute, never collapsed into it), and
/// `UnmuteRequest.media_kinds` likewise.
#[test]
fn kind_keyed_request_lists_survive_on_every_request() {
    let lists = [
        vec![],
        vec![mute_change(MediaKind::Audio, false)],
        vec![
            mute_change(MediaKind::VideoCamera, true),
            mute_change(MediaKind::Audio, false),
            mute_change(MediaKind::Audio, true),
        ],
    ];
    for changes in lists {
        let client = roundtrip(&MuteRequest {
            changes: changes.clone(),
        });
        assert_eq!(client.changes, changes);
        let server = roundtrip(&ServerMuteRequest {
            participant_id: "p-2".to_string(),
            changes: changes.clone(),
            reason: String::new(),
        });
        assert_eq!(server.changes, changes);
    }
    let unmute = roundtrip(&UnmuteRequest {
        participant_id: "p".to_string(),
        media_kinds: vec![MediaKind::VideoCamera as i32, MediaKind::Audio as i32],
    });
    assert_eq!(
        unmute.media_kinds,
        vec![MediaKind::VideoCamera as i32, MediaKind::Audio as i32]
    );
}

/// The retired per-kind request tags never reach the wire: `MuteRequest`
/// emits only 3, `ServerMuteRequest` 1/4/5, `UnmuteRequest` 3/4 -- and the
/// retired `MuteRequest` tag 1 from an old sender decodes as an EMPTY list,
/// which MC refuses (`no_kind_named`), never as an implicit unmute.
#[test]
fn kind_keyed_requests_use_only_their_fresh_tags() {
    assert_eq!(
        top_level_field_numbers(
            &MuteRequest {
                changes: vec![mute_change(MediaKind::Audio, false)],
            }
            .encode_to_vec()
        ),
        vec![3]
    );
    assert_eq!(
        top_level_field_numbers(
            &ServerMuteRequest {
                participant_id: "p".to_string(),
                changes: vec![mute_change(MediaKind::Audio, true)],
                reason: "r".to_string(),
            }
            .encode_to_vec()
        ),
        vec![1, 4, 5]
    );
    assert_eq!(
        top_level_field_numbers(
            &UnmuteRequest {
                participant_id: "p".to_string(),
                media_kinds: vec![MediaKind::Audio as i32],
            }
            .encode_to_vec()
        ),
        vec![3, 4]
    );
    let old_true_audio = [0x08u8, 0x01];
    let out = MuteRequest::decode(&old_true_audio[..]).expect("old bytes decode");
    assert!(
        out.changes.is_empty(),
        "a pre-2026-10-25 request names no kind"
    );
}

// ---------------------------------------------------------------------------
// Story 3: slot-state vocabulary (R-4, R-5) and open enums
// ---------------------------------------------------------------------------

/// `SLOT_STATE_SOURCE_MUTED` (deleted by MC-B) is reserved by number AND name,
/// exactly like `SLOT_STATE_ZERO_REQUESTED` below: a raw 2 from an older MC
/// survives as the integer for a consumer to fail closed on, never as a value
/// it could render.
#[test]
fn reserved_source_muted_is_not_a_named_value() {
    assert!(SlotState::try_from(RESERVED_SLOT_STATE_SOURCE_MUTED).is_err());
    assert_eq!(SlotState::from_str_name("SLOT_STATE_SOURCE_MUTED"), None);
    // Positive control: the same lookups DO resolve a live value.
    assert!(SlotState::try_from(SlotState::AvatarServerMuted as i32).is_ok());
    assert_eq!(
        SlotState::from_str_name("SLOT_STATE_AVATAR_SERVER_MUTED"),
        Some(SlotState::AvatarServerMuted)
    );

    let out = roundtrip(&StreamAssignment {
        slot_id: 1,
        slot_state: RESERVED_SLOT_STATE_SOURCE_MUTED,
        ..Default::default()
    });
    assert_eq!(out.slot_state, RESERVED_SLOT_STATE_SOURCE_MUTED);
}

/// `SLOT_STATE_ZERO_REQUESTED` is reserved by number AND name: its number no
/// longer converts to a named value, its name no longer parses, and a raw 5 on
/// the wire survives as the integer for a consumer to reject (fail closed).
#[test]
fn reserved_zero_requested_is_not_a_named_value() {
    assert!(SlotState::try_from(RESERVED_SLOT_STATE_ZERO_REQUESTED).is_err());
    assert_eq!(SlotState::from_str_name("SLOT_STATE_ZERO_REQUESTED"), None);

    let out = roundtrip(&StreamAssignment {
        slot_id: 1,
        slot_state: RESERVED_SLOT_STATE_ZERO_REQUESTED,
        ..Default::default()
    });
    assert_eq!(out.slot_state, RESERVED_SLOT_STATE_ZERO_REQUESTED);
    assert!(SlotState::try_from(out.slot_state).is_err());
}

/// Every enum is OPEN in proto3: an unknown number decodes as its integer, never
/// as a decode error and never as a named value. Consumers are told to reject
/// (and count) it; this pins that there is something to reject. One test per
/// story-3 surface: a slot state, a packed repeated `MediaKind`, a `Codec`.
#[test]
fn unknown_enum_values_decode_as_open_integers() {
    let assignment = roundtrip(&StreamAssignment {
        slot_state: UNKNOWN_ENUM_VALUE,
        ..Default::default()
    });
    assert_eq!(assignment.slot_state, UNKNOWN_ENUM_VALUE);
    assert!(SlotState::try_from(assignment.slot_state).is_err());

    let join = roundtrip(&JoinRequest {
        published_kinds: vec![PublishedKind {
            media_kind: UNKNOWN_ENUM_VALUE,
            client_muted: false,
        }],
        ..Default::default()
    });
    let kind = join.published_kinds[0].media_kind;
    assert_eq!(kind, UNKNOWN_ENUM_VALUE);
    assert!(MediaKind::try_from(kind).is_err());

    let state = roundtrip(&MediaKindState {
        media_kind: UNKNOWN_ENUM_VALUE,
        client_state: UNKNOWN_ENUM_VALUE,
        ..Default::default()
    });
    assert_eq!(state.media_kind, UNKNOWN_ENUM_VALUE);
    assert!(ClientMediaState::try_from(state.client_state).is_err());

    let encoding = roundtrip(&EncodingParameters {
        codec: UNKNOWN_ENUM_VALUE,
        ..Default::default()
    });
    assert_eq!(encoding.codec, UNKNOWN_ENUM_VALUE);
    assert!(Codec::try_from(encoding.codec).is_err());
}

/// `StreamAssignment.sender_id` presence per state (R-5): absent on a video
/// ACTIVE slot and on the sourceless states, present on audio ACTIVE and every
/// avatar. Each survives the wire as written, never coerced to 0.
#[test]
fn stream_assignment_sender_presence_survives_per_state() {
    let cases = [
        (MediaKind::VideoCamera, SlotState::Active, None),
        (MediaKind::Audio, SlotState::Active, Some(7)),
        (MediaKind::VideoCamera, SlotState::AvatarNoVideo, Some(7)),
        (
            MediaKind::VideoCamera,
            SlotState::AvatarClientMuted,
            Some(7),
        ),
        (MediaKind::Audio, SlotState::AvatarServerMuted, Some(7)),
        (MediaKind::VideoCamera, SlotState::Unavailable, None),
        (MediaKind::Audio, SlotState::FewerSourcesThanSlots, None),
    ];
    for (kind, state, sender) in cases {
        let out = roundtrip(&StreamAssignment {
            slot_id: 4,
            sender_id: sender,
            media_kind: kind as i32,
            media_handler_url: String::new(),
            slot_state: state as i32,
            switch_command_id: None,
        });
        assert_eq!(out.sender_id, sender, "{kind:?} {state:?}");
        assert_eq!(out.slot_state, state as i32);
        assert_eq!(
            out.slot_id, 4,
            "UNAVAILABLE echoes the declared slot id too"
        );
    }
}

// ---------------------------------------------------------------------------
// Story 3: join declarations and caps (R-1, R-2, R-23)
// ---------------------------------------------------------------------------

/// The published kinds survive by value AND order, conflicting duplicates
/// included (MC must SEE a duplicate to reject it); empty = receive-only.
#[test]
fn published_kinds_keep_order_and_duplicates() {
    let empty = roundtrip(&JoinRequest::default());
    assert!(empty.published_kinds.is_empty(), "empty = receive-only");

    let kinds = vec![
        PublishedKind {
            media_kind: MediaKind::VideoCamera as i32,
            client_muted: true,
        },
        PublishedKind {
            media_kind: MediaKind::Audio as i32,
            client_muted: false,
        },
        PublishedKind {
            media_kind: MediaKind::Audio as i32,
            client_muted: true,
        },
    ];
    let out = roundtrip(&JoinRequest {
        published_kinds: kinds.clone(),
        ..Default::default()
    });
    assert_eq!(out.published_kinds, kinds);
}

/// `JoinRequest.receive_capability` ABSENT (zero slots of each kind) is
/// distinguishable from PRESENT-BUT-EMPTY, and a populated declaration
/// survives.
#[test]
fn join_receive_capability_absent_empty_and_populated_are_distinct() {
    let absent = roundtrip(&JoinRequest::default());
    assert_eq!(absent.receive_capability, None);

    let empty = roundtrip(&JoinRequest {
        receive_capability: Some(ReceiveCapability::default()),
        ..Default::default()
    });
    assert_eq!(empty.receive_capability, Some(ReceiveCapability::default()));

    let populated = roundtrip(&JoinRequest {
        receive_capability: Some(ReceiveCapability {
            slots: vec![
                ReceiveSlot {
                    slot_id: 0,
                    media_kind: MediaKind::Audio as i32,
                    pinned_sender_id: None,
                },
                ReceiveSlot {
                    slot_id: 1,
                    media_kind: MediaKind::VideoCamera as i32,
                    pinned_sender_id: None,
                },
            ],
        }),
        ..Default::default()
    });
    let slots = populated.receive_capability.expect("present").slots;
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[1].media_kind, MediaKind::VideoCamera as i32);
}

/// The kind-keyed caps survive by value and order, a present 0 (the video-off
/// lever) included -- a 0 entry still emits its element, and the retired tags
/// 13/14 decode as nothing. (Tag numbers
/// are pinned from the bytes in `story_3_frozen_tags_are_on_the_wire`.)
#[test]
fn receive_slot_caps_roundtrip_with_a_present_zero() {
    let caps = vec![
        ReceiveSlotCap {
            media_kind: MediaKind::Audio as i32,
            max_slots: 8,
        },
        ReceiveSlotCap {
            media_kind: MediaKind::VideoCamera as i32,
            max_slots: 0,
        },
        ReceiveSlotCap {
            media_kind: MediaKind::VideoScreen as i32,
            max_slots: 0,
        },
    ];
    let out = roundtrip(&JoinResponse {
        participant_id: "p-1".to_string(),
        receive_slot_caps: caps.clone(),
        ..Default::default()
    });
    assert_eq!(out.receive_slot_caps, caps);
    // The retired tags 13/14 are reserved: their bytes populate nothing.
    let retired = [0x68u8, 0x08, 0x70, 0x04]; // 13: 8, 14: 4
    let decoded = JoinResponse::decode(&retired[..]).expect("unknown tags are skipped");
    assert!(decoded.receive_slot_caps.is_empty());
}

/// The redacting `Debug` impls are hand-maintained field lists: the new
/// non-secret fields must render in the clear, and the secrets beside them
/// must stay redacted.
#[test]
fn debug_shows_story_3_join_fields_in_the_clear() {
    let response = JoinResponse {
        meeting_kek: vec![0xC3u8; 32],
        receive_slot_caps: vec![ReceiveSlotCap {
            media_kind: MediaKind::Audio as i32,
            max_slots: 8,
        }],
        ..Default::default()
    };
    let rendered = format!("{response:?}");
    assert!(
        rendered
            .contains("receive_slot_caps: [ReceiveSlotCap { media_kind: Audio, max_slots: 8 }]"),
        "{rendered}"
    );
    assert!(
        !rendered.contains("195, 195"),
        "KEK bytes must not be printed"
    );

    let request = JoinRequest {
        join_token: "eyJ.super-secret-jwt".to_string(),
        published_kinds: vec![PublishedKind {
            media_kind: MediaKind::Audio as i32,
            client_muted: true,
        }],
        ..Default::default()
    };
    let rendered = format!("{request:?}");
    assert!(
        rendered
            .contains("published_kinds: [PublishedKind { media_kind: Audio, client_muted: true }]"),
        "{rendered}"
    );
    assert!(rendered.contains("receive_capability: None"));
    assert!(!rendered.contains("super-secret-jwt"));
}

// ---------------------------------------------------------------------------
// Story 3: video encoding (R-8, R-11, R-23)
// ---------------------------------------------------------------------------

#[test]
fn vp8_is_codec_5() {
    assert_eq!(Codec::Vp8 as i32, 5);
    assert_eq!(Codec::from_str_name("CODEC_VP8"), Some(Codec::Vp8));
}

/// `min_keyframe_interval_ms` absent / present-0 / present-500 are three
/// distinct observables: absent is what a client must REFUSE on a video
/// stream, so it must never decode as a present value.
#[test]
fn min_keyframe_interval_absent_zero_and_value_are_distinct() {
    for value in [None, Some(0), Some(500)] {
        let out = roundtrip(&EncodingParameters {
            codec: Codec::Vp8 as i32,
            max_bitrate_bps: 800_000,
            width: 640,
            height: 360,
            frame_rate: 30,
            min_keyframe_interval_ms: value,
        });
        assert_eq!(out.min_keyframe_interval_ms, value);
        assert_eq!(out.codec, Codec::Vp8 as i32);
        assert_eq!(out.frame_rate, 30, "the additive field leaves neighbours");
    }
}

// ---------------------------------------------------------------------------
// Story 3: keyframe and recovery requests on the MH bidi stream (R-10, R-13)
// ---------------------------------------------------------------------------

/// `MhServerMessage` (new) carries `KeyframeRequest` at tag 1 and the
/// envelope trace fields at 20/21; the GoP counter's full u32 range survives,
/// 0 included.
#[test]
fn keyframe_request_roundtrips_through_the_mh_server_envelope() {
    for gop_counter in [0, u32::MAX] {
        let envelope = MhServerMessage {
            message: Some(mh_server_message::Message::KeyframeRequest(
                KeyframeRequest {
                    stream_number: 1,
                    gop_counter,
                },
            )),
            trace_parent: "00-trace".to_string(),
            trace_state: "k=v".to_string(),
        };
        let bytes = envelope.encode_to_vec();
        assert_eq!(top_level_field_numbers(&bytes), vec![1, 20, 21]);
        match roundtrip(&envelope).message {
            Some(mh_server_message::Message::KeyframeRequest(k)) => {
                assert_eq!(k.stream_number, 1);
                assert_eq!(k.gop_counter, gop_counter);
            }
            other => panic!("expected KeyframeRequest, got {other:?}"),
        }
    }
}

/// `MhClientMessage.recovery_request` (tag 2) carries all three fields; the
/// GoP counter's full range survives, 0 included (a legal first group).
#[test]
fn recovery_request_roundtrips_through_the_mh_client_envelope() {
    for gop_counter in [0, u32::MAX] {
        let envelope = MhClientMessage {
            message: Some(mh_client_message::Message::RecoveryRequest(
                RecoveryRequest {
                    slot_id: 65535,
                    sender_id: 9,
                    gop_counter,
                },
            )),
            trace_parent: "00-trace".to_string(),
            trace_state: "k=v".to_string(),
        };
        assert_eq!(
            top_level_field_numbers(&envelope.encode_to_vec()),
            vec![2, 20, 21]
        );
        let out = roundtrip(&envelope);
        assert_eq!(out.trace_parent, "00-trace");
        match out.message {
            Some(mh_client_message::Message::RecoveryRequest(r)) => {
                assert_eq!(r.slot_id, 65535);
                assert_eq!(r.sender_id, 9);
                assert_eq!(r.gop_counter, gop_counter);
            }
            other => panic!("expected RecoveryRequest, got {other:?}"),
        }
    }
}

/// The connect variant is unchanged by the new sibling: still tag 1, still the
/// first framed message a handler reads.
#[test]
fn mh_connect_request_keeps_tag_1() {
    let envelope = MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: "jwt".to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    assert_eq!(top_level_field_numbers(&envelope.encode_to_vec()), vec![1]);
    assert!(matches!(
        roundtrip(&envelope).message,
        Some(mh_client_message::Message::ConnectRequest(_))
    ));
}

/// The story's FROZEN tag numbers, read from the encoded BYTES. A round trip
/// through one prost codegen cannot catch a wrong tag in the `.proto` (encode
/// and decode agree with each other), and `buf breaking` only guards a tag
/// after it has landed -- so on the landing commit this is the only check.
/// Present-0 values are used where possible, which also proves explicit
/// presence emits the tag.
#[test]
fn story_3_frozen_tags_are_on_the_wire() {
    let join = top_level_field_numbers(
        &JoinRequest {
            receive_capability: Some(ReceiveCapability::default()),
            published_kinds: vec![PublishedKind::default()],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    assert_eq!(join, vec![9, 10]);

    let caps = top_level_field_numbers(
        &JoinResponse {
            receive_slot_caps: vec![ReceiveSlotCap::default(), ReceiveSlotCap::default()],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    assert_eq!(caps, vec![15, 15]);

    let encoding = top_level_field_numbers(
        &EncodingParameters {
            min_keyframe_interval_ms: Some(0),
            ..Default::default()
        }
        .encode_to_vec(),
    );
    assert_eq!(encoding, vec![6]);

    let keyframe = top_level_field_numbers(
        &KeyframeRequest {
            stream_number: 1,
            gop_counter: 1,
        }
        .encode_to_vec(),
    );
    assert_eq!(keyframe, vec![1, 2]);

    let recovery = top_level_field_numbers(
        &RecoveryRequest {
            slot_id: 1,
            sender_id: 1,
            gop_counter: 1,
        }
        .encode_to_vec(),
    );
    assert_eq!(recovery, vec![1, 2, 3]);
}
