//! Wire-shape tests for the ADR-0036 signalling contract.
//!
//! These assert the *contract*, not behaviour: every message the media path
//! adds must survive an encode/decode round trip with its fields intact, the
//! seven slot states must be distinguishable on the wire, and the redacting
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

fn roundtrip<T: Message + Default>(msg: &T) -> T {
    let bytes = msg.encode_to_vec();
    T::decode(bytes.as_slice()).expect("decode must succeed for a message we just encoded")
}

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
        SlotState::ZeroRequested,
        SlotState::FewerSourcesThanSlots,
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
fn slot_state_has_seven_distinct_states_plus_unspecified() {
    // ADR-0036 §6 names seven states, and they are on the wire because absence
    // of frames is not a signal. buf STANDARD requires the zero value to be
    // UNSPECIFIED, so the enum carries eight values.
    let states = [
        SlotState::Active,
        SlotState::SourceMuted,
        SlotState::WithheldByCongestion,
        SlotState::FewerSourcesThanSlots,
        SlotState::ZeroRequested,
        SlotState::SourceUnreachable,
        SlotState::SwitchPending,
    ];

    let mut seen: Vec<i32> = states.iter().map(|s| *s as i32).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        7,
        "the seven §6 states must be distinct on the wire"
    );
    assert!(
        !seen.contains(&(SlotState::Unspecified as i32)),
        "no real state may collide with the fail-closed zero value"
    );
    assert_eq!(SlotState::Unspecified as i32, 0);
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
        }],
        media_servers: Vec::new(),
        correlation_id: "c-1".to_string(),
        binding_token: "b-1".to_string(),
        sender_id: Some(65535),
        meeting_kek: kek.clone(),
        kek_generation: 65535,
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
    });
    assert_eq!(out.meeting_kek, kek);
    assert_eq!(out.kek_generation, 7);
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
                audio_muted: true,
                video_muted: false,
                reason: "policy".to_string(),
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    match roundtrip(&envelope).message {
        Some(client_message::Message::ServerMuteRequest(m)) => {
            assert_eq!(m.participant_id, "p-2");
            assert!(m.audio_muted);
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
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };

    let rendered = format!("{envelope:?}");
    assert!(!rendered.contains("195, 195"));
    assert!(rendered.contains("<redacted 32 B>"));
}
