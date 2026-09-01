//! Wire-shape contract tests for the ADR-0036 MC→MH control plane
//! (`dark_tower.internal.v1`).
//!
//! Sibling of `signaling_roundtrip.rs`, and landed for the same reason: the
//! codegen oracle (`packages/proto-gen/scripts/verify-codegen.sh`) checks that
//! TypeScript *symbols* exist or do not exist. It does not check that a field
//! survives encode/decode, and it cannot check presence-vs-absence semantics at
//! all. Without these tests, the reshape's zero-value reasoning — the whole
//! "one hazard, four faces" analysis in the devloop output — is prose locked by
//! nothing.
//!
//! # What these tests deliberately do NOT assert
//!
//! Three normative rules in `internal.proto` are RUNTIME invariants that no test
//! in this crate can express, because the inputs that distinguish a correct
//! implementation from a broken one do not exist in this story
//! (@test: "equivalent mutants under the reachable input space"). They are
//! pinned, with their distinguishing inputs named, in `docs/TODO.md`
//! §Media Path Obligations — not here, because an inert marker in a file that
//! can never construct the input would be prose masquerading as coverage:
//!
//! - **`sender_id` is meeting-scoped** (`SubscriberSlot.sender_id`). A globally
//!   keyed `sender_id` → connection index is a cross-meeting media-crossing
//!   primitive, and it is observationally identical to a correct index until a
//!   SECOND CONCURRENT MEETING exists. Pinned at story task 5.
//! - **An `UNSPECIFIED` transport-mode echo is a mismatch, not a skip**
//!   (`RegisterMeetingResponse.transport_mode`). The fail-open form's skip
//!   branch is unreachable against a same-version peer. Pinned at story task 6.
//! - **`process_start_epoch_ms` is sampled once per process**. A per-call clock
//!   read is indistinguishable from a correct sample until a SECOND PROCESS
//!   INCARNATION SHARING A POD IDENTITY exists. Split: within-process stability
//!   at story task 5, changes-on-restart at the handler-restart story.

// Test code: a failed decode IS the failure signal here, so panicking is the
// assertion mechanism rather than a defect.
//
// `#[expect]`, not `#[allow]`, per ADR-0002 (:290 and the :334 checklist) and the
// form modelled in `clippy.toml:20`. The self-cleaning property is the point and
// it is live here: if a future edit removes the last `.expect()` from this file,
// `#[expect]` warns that the suppression is now dead, whereas `#[allow]` would rot
// silently. Only `expect_used` is listed — this file has no `.unwrap()` and no
// explicit `panic!` (assert macros do not trip `clippy::panic`), so naming the
// sibling's other two lints would itself be an unfulfilled expectation.
//
// The sibling `signaling_roundtrip.rs` uses the older `#![allow]` three-lint form;
// that is a same-owner (protocol) pre-existing deviation tracked in `docs/TODO.md`,
// NOT a licence for this file.
#![expect(
    clippy::expect_used,
    reason = "test: a failed decode IS the failure signal; panicking is the assertion mechanism"
)]

use proto_gen::dark_tower::internal::v1::{
    CandidateSource, EgressStream, RegisterMeetingRequest, RegisterMeetingResponse, SelectionRules,
    SubscriberSlot,
};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use proto_gen::Message;

fn roundtrip<T: Message + Default>(msg: &T) -> T {
    let bytes = msg.encode_to_vec();
    T::decode(&bytes[..]).expect("decodes")
}

/// The CONSTRUCTIVE half of ADR-0036 §6 make-conflicts-unrepresentable.
///
/// The destructive half — "a behaviour keyed to an egress id with no edge is
/// unrepresentable" — is carried by the shape itself and needs no test: there is
/// no second parallel list to desynchronise. What a test *can* assert is the
/// reciprocal: the edge, its candidate sources and its behaviours travel in ONE
/// message and survive together. If a future edit split them back into parallel
/// lists, this test is where the guarantee would be seen to weaken.
#[test]
fn egress_stream_carries_edge_and_behaviours_in_one_message() {
    let req = RegisterMeetingRequest {
        meeting_id: "meeting-1".to_string(),
        mc_id: "mc-1".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
        egress_streams: vec![EgressStream {
            egress_stream_id: 9,
            subscriber: Some(SubscriberSlot {
                sender_id: 42,
                slot_id: 3,
            }),
            candidate_sources: vec![
                CandidateSource {
                    sender_id: 42,
                    stream_number: 0,
                },
                CandidateSource {
                    sender_id: 77,
                    stream_number: 1,
                },
            ],
            priority_group: 5,
            supersede_on_independent_frame: true,
            transport_mode: TransportMode::Datagram as i32,
        }],
        selection_rules: Some(SelectionRules {}),
        policy_generation: 7,
    };

    let decoded = roundtrip(&req);
    let egress = &decoded.egress_streams[0];

    assert_eq!(egress.egress_stream_id, 9);
    let subscriber = egress.subscriber.expect("subscriber survives");
    assert_eq!(subscriber.sender_id, 42);
    assert_eq!(subscriber.slot_id, 3);
    assert_eq!(egress.candidate_sources.len(), 2);
    assert_eq!(egress.candidate_sources[1].sender_id, 77);
    assert_eq!(egress.candidate_sources[1].stream_number, 1);
    assert_eq!(egress.priority_group, 5);
    assert!(egress.supersede_on_independent_frame);
    assert_eq!(egress.transport_mode, TransportMode::Datagram as i32);
    assert_eq!(decoded.policy_generation, 7);
}

/// An empty `egress_streams` set is legal and means "this handler forwards
/// nothing for this meeting" — a computed output of MC's assignment, never an
/// operator lever. It must survive the wire as empty rather than being
/// indistinguishable from a decode failure.
///
/// Mirrors `signaling_roundtrip.rs`'s empty-target-set test: the same "empty is
/// a value, not an absence" rule on the other side of the contract.
#[test]
fn empty_egress_stream_set_is_legal_and_survives() {
    let req = RegisterMeetingRequest {
        meeting_id: "meeting-1".to_string(),
        mc_id: "mc-1".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
        egress_streams: Vec::new(),
        selection_rules: None,
        policy_generation: 1,
    };

    let decoded = roundtrip(&req);
    assert!(decoded.egress_streams.is_empty());
    assert_eq!(decoded.policy_generation, 1);
}

/// `selection_rules` presence must be distinguishable from its absence.
///
/// `internal.proto` claims this distinguishes "MC declared no rules" from "MC
/// did not speak about rules", and it is the stated reason the field is a NAMED
/// MESSAGE rather than `bytes` or a map. A singular message field carries
/// explicit presence in proto3; nothing tested that it survives the wire.
#[test]
fn selection_rules_presence_is_distinguishable_from_absence() {
    let base = RegisterMeetingRequest {
        meeting_id: "meeting-1".to_string(),
        mc_id: "mc-1".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
        egress_streams: Vec::new(),
        selection_rules: None,
        policy_generation: 1,
    };

    let absent = roundtrip(&base);
    assert!(
        absent.selection_rules.is_none(),
        "absence must survive as None, not as an empty-but-present message"
    );

    let present = roundtrip(&RegisterMeetingRequest {
        selection_rules: Some(SelectionRules {}),
        ..base
    });
    assert!(
        present.selection_rules.is_some(),
        "an empty-but-declared SelectionRules must survive as Some"
    );
}

/// ADR-0036 §8: `applied_generation` reports what MH's live forward path
/// reflects, NEVER an echo of what it received.
///
/// This makes "applied differs from received" representable at the type level:
/// a response asserting applied=4 while the request carried policy_generation=7
/// survives the wire with both values intact and distinct. An implementer who
/// writes `applied_generation: req.policy_generation` deletes the feature while
/// leaving the field, and the contract must at minimum be able to *express* the
/// state that catches it.
#[test]
fn applied_generation_is_independent_of_the_requested_generation() {
    let sent = roundtrip(&RegisterMeetingRequest {
        meeting_id: "meeting-1".to_string(),
        mc_id: "mc-1".to_string(),
        mc_grpc_endpoint: "http://mc:50052".to_string(),
        egress_streams: Vec::new(),
        selection_rules: None,
        policy_generation: 7,
    });

    let echoed = roundtrip(&RegisterMeetingResponse {
        accepted: true,
        applied_generation: 4,
        handler_id: "mh-1".to_string(),
        process_start_epoch_ms: 1_756_684_800_000,
        transport_mode: TransportMode::Datagram as i32,
    });

    assert_eq!(sent.policy_generation, 7);
    assert_eq!(echoed.applied_generation, 4);
    assert_ne!(
        echoed.applied_generation, sent.policy_generation,
        "the divergent state ADR-0036 §8 exists to detect must be representable"
    );
    assert_eq!(echoed.handler_id, "mh-1");
    assert_eq!(echoed.process_start_epoch_ms, 1_756_684_800_000);
}

/// The all-zero response — what a PRE-RESHAPE media handler's absent fields
/// decode to — must be a representable, decodable value.
///
/// This is the wire half of the reshape's zero-value contract: `accepted: true`
/// with `applied_generation: 0`, `handler_id: ""`, `process_start_epoch_ms: 0`
/// and `TRANSPORT_MODE_UNSPECIFIED` is exactly what MC sees from an old handler,
/// and every one of those readings is documented as "not reported" rather than
/// as a legitimate value. The rules constraining MC's *reaction* are runtime and
/// live in `docs/TODO.md` (see the module header); what is locked here is that
/// the state exists and is not silently identical to a populated response.
#[test]
fn all_zero_response_is_representable_and_distinct_from_a_populated_one() {
    let zeroed = roundtrip(&RegisterMeetingResponse {
        accepted: true,
        applied_generation: 0,
        handler_id: String::new(),
        process_start_epoch_ms: 0,
        transport_mode: TransportMode::Unspecified as i32,
    });

    assert!(zeroed.accepted, "`accepted` means received-and-parsed only");
    assert_eq!(zeroed.applied_generation, 0, "0 = nothing applied");
    assert!(zeroed.handler_id.is_empty(), "empty = not reported");
    assert_eq!(zeroed.process_start_epoch_ms, 0, "0 = not reported");
    assert_eq!(
        zeroed.transport_mode,
        TransportMode::Unspecified as i32,
        "UNSPECIFIED = no mode applied; a MISMATCH for MC, never a skip"
    );

    let populated = roundtrip(&RegisterMeetingResponse {
        accepted: true,
        applied_generation: 1,
        handler_id: "mh-1".to_string(),
        process_start_epoch_ms: 1,
        transport_mode: TransportMode::Datagram as i32,
    });
    assert_ne!(zeroed, populated);
}
