// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — do not "simplify" it away. `MetricAssertion`
// binds a per-thread recorder; the media-path metrics are emitted by
// `MhClient::register_meeting`'s post-RPC confirm, on the caller's task. On
// `current_thread` that task IS the test thread; on multi-thread it can land on
// a worker and every assertion below silently observes zero.
// See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for the ADR-0036 §8 policy push and its confirm.
//!
//! The subject is the media-path metric MC emits on **every** classified push
//! outcome, `mc_media_policy_pushes_total{outcome, key_custody}` (the
//! `mc_media_generation_divergence` gauge was retired in story 4 MC-B1; live
//! lag is `mc_media_handler_applied_sequence_lag`, observed per heartbeat),
//! and the fail-loud behaviour they accompany. A real `MhClient` dials a real
//! `mc_test_utils::MediaHandlerStub` over a real gRPC channel, so what is under
//! test is the wire round trip and the classification of the reply — not a
//! hand-constructed `RegisterMeetingResponse`. The classification's own
//! exhaustive per-outcome coverage is a unit concern and lives in
//! `src/media_routing/confirm.rs`.
//!
//! **`key_custody=operator` is asserted on the metric, not just in logs**
//! (ADR-0036 §4, §11), and **no meeting identifier appears on the series** —
//! `MetricAssertion` matches on the full label set, so a stray label would fail
//! these assertions rather than pass unnoticed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU64;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::errors::McError;
use mc_service::grpc::{MeetingProgramming, MhClient};
use mc_service::media_routing::PolicyPushOutcome;
use mc_test_utils::media::{two_party_assignment, TEST_HANDLER_ID};
use mc_test_utils::mock_mh::{
    AppliedGenerationBehaviour, MediaHandlerStub, MediaHandlerStubHandle,
};
use mc_test_utils::test_token_receiver;
use proto_gen::dark_tower::signaling::v1::TransportMode;

const KEY_CUSTODY: (&str, &str) = ("key_custody", "operator");

/// Push the two-party policy at `generation` and return the call result.
async fn push(
    stub: &MediaHandlerStubHandle,
    meeting_id: &str,
    generation: u64,
) -> Result<(), McError> {
    push_with(stub, meeting_id, generation, false).await
}

/// [`push`], with the pusher's first-confirm discriminator set as given.
async fn push_with(
    stub: &MediaHandlerStubHandle,
    meeting_id: &str,
    generation: u64,
    restart_floor_adoptable: bool,
) -> Result<(), McError> {
    let endpoint = stub.endpoint();
    let assignment = two_party_assignment();
    let client = MhClient::new(test_token_receiver());
    client
        .register_meeting(&MeetingProgramming {
            mh_grpc_endpoint: &endpoint,
            expected_handler_id: TEST_HANDLER_ID,
            meeting_id,
            mc_id: "mc-test",
            mc_grpc_endpoint: "http://mc-test:50052",
            assignment: &assignment,
            policy_generation: NonZeroU64::new(generation).unwrap(),
            heartbeat_interval_ms: 2_000,
            restart_floor_adoptable,
        })
        .await
}

/// Assert exactly one outcome series moved, and that the other four did not.
///
/// A bare "the expected series incremented" assertion passes even if the code
/// also increments a second one, which is how a classification bug hides.
fn assert_only_outcome(snap: &MetricSnapshot, expected: PolicyPushOutcome) {
    for outcome in PolicyPushOutcome::ALL {
        let delta = u64::from(outcome == expected);
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", outcome.label()), KEY_CUSTODY])
            .assert_delta(delta);
    }
}

// ---------------------------------------------------------------------------
// (a) Injected apply failure
// ---------------------------------------------------------------------------

/// The task's named integration case: **inject an apply failure at the mocked
/// handler and assert the echoed applied generation does not advance and the
/// push is counted as a mismatch.**
///
/// The stub stalls at generation 3 — it received 7 and could not install it, so
/// it correctly keeps reporting the generation its live forward path still
/// reflects. That is ADR-0036 §8's distinction between "applied" and "received"
/// made observable: a handler echoing the *received* value would report 7 and
/// this test would go green on a blackhole.
#[tokio::test(flavor = "current_thread")]
async fn injected_apply_failure_does_not_advance_the_echo_and_is_counted() {
    const STALLED_AT: u64 = 3;
    const SENT: u64 = 7;

    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::Stall(STALLED_AT))
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-apply-failure", SENT).await;

    // The echo did not advance: MH was sent 7 and still reflects 3.
    assert_eq!(
        stub.received_generations(),
        vec![SENT],
        "MC must send the generation it derived"
    );
    assert!(
        matches!(
            &result,
            Err(McError::MediaPolicyDivergence { outcome, .. })
                if *outcome == PolicyPushOutcome::GenerationMismatch
        ),
        "a stalled applied generation must fail loud, got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::GenerationMismatch);
}

/// The arm a suite written only against the too-low case never reaches: MH
/// holds a HIGHER generation than MC sent (an MC restart re-deriving from 1, or
/// the `u64::MAX` ratchet wedge). It is a mismatch, never a match.
#[tokio::test(flavor = "current_thread")]
async fn applied_above_sent_is_a_mismatch() {
    const STALLED_AT: u64 = 9;
    const SENT: u64 = 1;

    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::Stall(STALLED_AT))
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-applied-above-sent", SENT).await;
    assert!(result.is_err(), "got {result:?}");

    assert_only_outcome(&snap, PolicyPushOutcome::GenerationMismatch);
}

/// The MC-restart floor: on the FIRST confirm of a (meeting, handler), a higher
/// echo is the pre-restart MC's number, not a divergence. `confirm` records
/// NOTHING on the outcome counter (the pusher records exactly one of the
/// adoption counter or, if adoption fails, `generation_mismatch`), so
/// `MCMediaGenerationDivergence` does not page on every MC rollout — and the
/// error still carries the classified outcome for the pusher to adopt from.
#[tokio::test(flavor = "current_thread")]
async fn a_restart_floor_on_first_confirm_is_not_recorded_as_a_mismatch() {
    const HELD: u64 = 9;
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::Stall(HELD))
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push_with(&stub, "meeting-restart-floor", 1, true).await;
    assert!(
        matches!(
            result,
            Err(McError::MediaPolicyDivergence {
                outcome: PolicyPushOutcome::GenerationMismatch,
                applied_generation: HELD,
            })
        ),
        "got {result:?}"
    );
    for outcome in PolicyPushOutcome::ALL {
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", outcome.label()), KEY_CUSTODY])
            .assert_delta(0);
    }
    // Recorded by the pusher, never here, under either disposition.
    for outcome in ["adopted", "superseded"] {
        snap.counter("mc_media_policy_generation_adoptions_total")
            .with_labels(&[("outcome", outcome), KEY_CUSTODY])
            .assert_delta(0);
    }
}

/// The discriminator is fail-closed: with it set, a LOWER echo (the ordinary
/// transient mismatch) is still recorded and still pages.
#[tokio::test(flavor = "current_thread")]
async fn a_lower_echo_on_first_confirm_is_still_a_recorded_mismatch() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::Stall(2))
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push_with(&stub, "meeting-lower-echo-first", 5, true).await;
    assert!(result.is_err(), "got {result:?}");
    assert_only_outcome(&snap, PolicyPushOutcome::GenerationMismatch);
}

// ---------------------------------------------------------------------------
// (b) Success
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn confirmed_push_reports_match() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully(TEST_HANDLER_ID)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-confirmed", 4).await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_only_outcome(&snap, PolicyPushOutcome::Match);
}

/// `accepted == true` is not success (ADR-0036 §8). A handler that parsed the
/// registration and installed nothing reports `applied_generation: 0`, which is
/// its own outcome — never folded into `match` and never into
/// `generation_mismatch`.
#[tokio::test(flavor = "current_thread")]
async fn accepted_but_nothing_installed_is_no_applied_generation() {
    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::ReportZero)
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-nothing-installed", 2).await;
    assert!(
        matches!(
            &result,
            Err(McError::MediaPolicyDivergence { outcome, .. })
                if *outcome == PolicyPushOutcome::NoAppliedGeneration
        ),
        "got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::NoAppliedGeneration);
}

// ---------------------------------------------------------------------------
// The two fail-open shapes, driven over a real wire
// ---------------------------------------------------------------------------

/// A pre-reshape handler decodes `transport_mode` to its proto3 default and
/// echoes `TRANSPORT_MODE_UNSPECIFIED`. `internal.proto` requires MC to compare
/// `sent != echoed` unconditionally, so this is a MISMATCH — the
/// `if echoed != UNSPECIFIED { compare }` fail-open would wave through exactly
/// the peer population the check exists to police.
#[tokio::test(flavor = "current_thread")]
async fn unspecified_transport_echo_is_a_mismatch_over_the_wire() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Unspecified)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-unspecified-mode", 5).await;
    assert!(
        matches!(
            &result,
            Err(McError::MediaPolicyDivergence { outcome, .. })
                if *outcome == PolicyPushOutcome::TransportModeMismatch
        ),
        "got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::TransportModeMismatch);
}

/// The `handler_id` twin of the case above: proto3 decodes an absent string to
/// `""`, so the comparison is unconditional there too. Since story 4 MC-B1
/// (ADR-0039 §7) a reply from any other incarnation — empty id included — is
/// `HandlerReplaced`: the registration FAILS for the dialed handler and the
/// reply is never classified (no outcome sample, no credit), even though its
/// generation and echo are perfect.
#[tokio::test(flavor = "current_thread")]
async fn empty_handler_id_echo_is_handler_replaced_and_unclassified() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id("")
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-empty-handler-id", 6).await;
    assert!(
        matches!(result, Err(McError::HandlerReplaced)),
        "got {result:?}"
    );
    for outcome in PolicyPushOutcome::ALL {
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", outcome.label()), KEY_CUSTODY])
            .assert_delta(0);
    }
}

/// A restarted handler (a later incarnation at the reused address) asserting a
/// different per-incarnation id, with the policy genuinely applied: the input
/// that actually occurs in production. Same posture as the empty-string case.
#[tokio::test(flavor = "current_thread")]
async fn restarted_handler_id_is_handler_replaced_and_unclassified() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id("mh-test-0-a1b2c3d4")
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-restarted-handler", 8).await;
    assert!(
        matches!(result, Err(McError::HandlerReplaced)),
        "got {result:?}"
    );
    for outcome in PolicyPushOutcome::ALL {
        snap.counter("mc_media_policy_pushes_total")
            .with_labels(&[("outcome", outcome.label()), KEY_CUSTODY])
            .assert_delta(0);
    }
}

// ---------------------------------------------------------------------------
// What MC actually puts on the wire
// ---------------------------------------------------------------------------

/// The two-party policy as MH receives it. Pins the §7 per-egress behaviours at
/// the wire boundary rather than only in the pure computation, so a bug in the
/// request builder cannot pass the unit tests and ship the wrong flags.
#[tokio::test(flavor = "current_thread")]
async fn pushed_request_carries_the_two_party_policy_and_a_non_zero_generation() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully(TEST_HANDLER_ID)
        .spawn()
        .await;

    push(&stub, "meeting-wire-shape", 1).await.unwrap();

    // `MhClient` must not retry internally — the retry budget belongs to the
    // per-handler push worker (`media_routing::pusher`), which owns the
    // disposition split. If a
    // second retry layer ever appeared here, a terminal outcome would be
    // retried despite `PushDisposition::Terminal`, and the loop-level test
    // could not see it.
    assert_eq!(
        stub.call_count(),
        1,
        "one push per register_meeting call; MhClient owns no retry loop"
    );

    let request = stub.last_request().expect("stub recorded the request");
    assert_eq!(request.meeting_id, "meeting-wire-shape");
    assert_eq!(
        request.policy_generation, 1,
        "MC must never send policy_generation 0"
    );
    assert!(
        request.selection_rules.is_none(),
        "this story declares no selection rules"
    );

    assert_eq!(
        request.egress_streams.len(),
        2,
        "one pinned audio egress stream per subscriber"
    );
    for stream in &request.egress_streams {
        let subscriber = stream.subscriber.as_ref().expect("subscriber present");
        assert_eq!(
            stream.candidate_sources.len(),
            1,
            "exactly one pinned candidate under static fill"
        );
        assert_ne!(
            stream.candidate_sources[0].sender_id, subscriber.sender_id,
            "no self-edge: loopback is removed (story 2 R-3)"
        );
        assert!(
            !stream.supersede_on_independent_frame,
            "every audio frame forwards; MH is told the behaviour, never the media type"
        );
        // An edge's transport is derived at MH from the candidate's
        // `SourceStream`, which carries DATAGRAM for stream 0.
        let declared = request
            .source_streams
            .iter()
            .find(|s| {
                s.sender_id == stream.candidate_sources[0].sender_id
                    && s.stream_number == stream.candidate_sources[0].stream_number
            })
            .expect("every candidate is declared in source_streams");
        assert_eq!(declared.transport_mode, TransportMode::Datagram as i32);
        assert_ne!(
            stream.priority_group, 0,
            "MC assigns the priority group; 0 is the never-set default"
        );
    }
}

/// A gen>0 snapshot with ZERO egress streams — a solo participant, or a handler
/// nobody is placed on — confirms `match`. Unreachable before story 2 (the
/// self-edge meant the first push always carried one stream).
#[tokio::test(flavor = "current_thread")]
async fn an_empty_snapshot_at_a_non_zero_generation_confirms_match() {
    // What a real MH answers for an empty set: it installs zero edges, echoes
    // the generation, and — reading transport mode off the live snapshot —
    // echoes UNSPECIFIED, which is exactly what MC declares for an empty set.
    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id(TEST_HANDLER_ID)
        .transport_mode(TransportMode::Unspecified)
        .spawn()
        .await;
    let empty = mc_service::media_routing::HandlerAssignment::default();
    let snap = MetricAssertion::snapshot();
    let result = MhClient::new(test_token_receiver())
        .register_meeting(&MeetingProgramming {
            mh_grpc_endpoint: &stub.endpoint(),
            expected_handler_id: TEST_HANDLER_ID,
            meeting_id: "meeting-empty",
            mc_id: "mc-test",
            mc_grpc_endpoint: "http://mc:50052",
            assignment: &empty,
            policy_generation: NonZeroU64::new(3).unwrap(),
            heartbeat_interval_ms: 2_000,
            restart_floor_adoptable: false,
        })
        .await;
    assert!(
        result.is_ok(),
        "an empty snapshot must confirm, got {result:?}"
    );
    assert_eq!(stub.last_request().unwrap().egress_streams.len(), 0);
    assert_only_outcome(&snap, PolicyPushOutcome::Match);
}
