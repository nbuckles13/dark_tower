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
//! The subject is the pair of media-path metrics MC emits on **every** push
//! outcome:
//!
//! * `mc_media_policy_pushes_total{outcome, key_custody}`
//! * `mc_media_generation_divergence{key_custody}`
//!
//! and the fail-loud behaviour they accompany. A real `MhClient` dials a real
//! `mc_test_utils::MediaHandlerStub` over a real gRPC channel, so what is under
//! test is the wire round trip and the classification of the reply — not a
//! hand-constructed `RegisterMeetingResponse`. The classification's own
//! exhaustive per-outcome coverage is a unit concern and lives in
//! `src/media_routing/confirm.rs`.
//!
//! **`key_custody=operator` is asserted on the metrics, not just in logs**
//! (ADR-0036 §4, §11), and **no meeting identifier appears on either series** —
//! `MetricAssertion` matches on the full label set, so a stray label would fail
//! these assertions rather than pass unnoticed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::num::NonZeroU64;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::errors::McError;
use mc_service::grpc::{MeetingProgramming, MhClient};
use mc_service::media_routing::PolicyPushOutcome;
use mc_test_utils::media::{loopback_assignment, TEST_HANDLER_ID};
use mc_test_utils::mock_mh::{
    AppliedGenerationBehaviour, MediaHandlerStub, MediaHandlerStubHandle,
};
use mc_test_utils::test_token_receiver;
use proto_gen::dark_tower::signaling::v1::TransportMode;

const KEY_CUSTODY: (&str, &str) = ("key_custody", "operator");

/// Push the loopback policy at `generation` and return the call result.
async fn push(
    stub: &MediaHandlerStubHandle,
    meeting_id: &str,
    generation: u64,
) -> Result<(), McError> {
    let endpoint = stub.endpoint();
    let assignment = loopback_assignment();
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
/// divergence gauge goes non-zero.**
///
/// The stub stalls at generation 3 — it received 7 and could not install it, so
/// it correctly keeps reporting the generation its live forward path still
/// reflects. That is ADR-0036 §8's distinction between "applied" and "received"
/// made observable: a handler echoing the *received* value would report 7 and
/// this test would go green on a blackhole.
#[tokio::test(flavor = "current_thread")]
async fn injected_apply_failure_does_not_advance_the_echo_and_drives_the_gauge() {
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
            Err(McError::MediaPolicyDivergence { outcome })
                if *outcome == PolicyPushOutcome::GenerationMismatch
        ),
        "a stalled applied generation must fail loud, got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::GenerationMismatch);

    // Non-zero, and specifically the MAGNITUDE |sent - applied| = 4.
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value((SENT - STALLED_AT) as f64);
}

/// The arm a suite written only against the too-low case never reaches, and the
/// one a naive `sent - applied` underflows on: MH holds a HIGHER generation than
/// MC sent (an MC restart re-deriving from 1, or the `u64::MAX` ratchet wedge).
///
/// `saturating_sub` would report **0** here — a healthy-looking gauge on the one
/// response shape that proves MC and MH disagree about which policy is live.
#[tokio::test(flavor = "current_thread")]
async fn applied_above_sent_reports_a_positive_magnitude_not_zero_and_not_a_wrap() {
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
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value((STALLED_AT - SENT) as f64);
}

// ---------------------------------------------------------------------------
// (b) Success
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn confirmed_push_reports_match_and_a_zero_gauge() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully(TEST_HANDLER_ID)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-confirmed", 4).await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_only_outcome(&snap, PolicyPushOutcome::Match);
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
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
            Err(McError::MediaPolicyDivergence { outcome })
                if *outcome == PolicyPushOutcome::NoAppliedGeneration
        ),
        "got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::NoAppliedGeneration);
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(2.0);
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
            Err(McError::MediaPolicyDivergence { outcome })
                if *outcome == PolicyPushOutcome::TransportModeMismatch
        ),
        "got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::TransportModeMismatch);
    // The generation matched, so the magnitude is 0 even though the push
    // failed — which is exactly why the COUNTER is the detection signal and
    // the gauge is only the magnitude a responder reads next.
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
}

/// The `handler_id` twin of the case above: proto3 decodes an absent string to
/// `""`, so the comparison must be unconditional there too.
///
/// It is also the OPS-18 decision end to end: the outcome is observed on the
/// counter, and the call still returns **`Ok`** — an ordinary MH pod restart
/// changes the per-incarnation `handler_id`, and failing the push would kick
/// every meeting whose assignment predates the restart. Reaching this outcome
/// at all proves the meeting IS programmed, because the demoted precedence puts
/// it below both generation and transport.
#[tokio::test(flavor = "current_thread")]
async fn empty_handler_id_echo_is_observed_but_non_fatal() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id("")
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-empty-handler-id", 6).await;
    assert!(
        result.is_ok(),
        "handler_id_mismatch is non-fatal for the interim (2026-09-02-mh-stable-handler-id); \
         got {result:?}"
    );

    assert_only_outcome(&snap, PolicyPushOutcome::HandlerIdMismatch);
}

/// A restarted handler asserting a different per-incarnation id, with the policy
/// genuinely applied. Same posture as the empty-string case, driven by the input
/// that actually occurs in production.
#[tokio::test(flavor = "current_thread")]
async fn restarted_handler_id_is_observed_but_non_fatal() {
    let stub = MediaHandlerStub::builder()
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id("mh-test-0-a1b2c3d4")
        .transport_mode(TransportMode::Datagram)
        .spawn()
        .await;

    let snap = MetricAssertion::snapshot();
    let result = push(&stub, "meeting-restarted-handler", 8).await;
    assert!(result.is_ok(), "got {result:?}");

    assert_only_outcome(&snap, PolicyPushOutcome::HandlerIdMismatch);
    snap.gauge("mc_media_generation_divergence")
        .with_labels(&[KEY_CUSTODY])
        .assert_value(0.0);
}

// ---------------------------------------------------------------------------
// What MC actually puts on the wire
// ---------------------------------------------------------------------------

/// The loopback policy as MH receives it. Pins the §7 per-egress behaviours at
/// the wire boundary rather than only in the pure computation, so a bug in the
/// request builder cannot pass the unit tests and ship the wrong flags.
#[tokio::test(flavor = "current_thread")]
async fn pushed_request_carries_the_loopback_policy_and_a_non_zero_generation() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully(TEST_HANDLER_ID)
        .spawn()
        .await;

    push(&stub, "meeting-wire-shape", 1).await.unwrap();

    // `MhClient` must not retry internally — the retry budget belongs to
    // `register_meeting_with_handlers`, which owns the disposition split. If a
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

    assert_eq!(request.egress_streams.len(), 1, "one audio egress stream");
    let stream = &request.egress_streams[0];
    let subscriber = stream.subscriber.as_ref().expect("subscriber present");
    assert_eq!(
        stream.candidate_sources.len(),
        1,
        "one candidate: the subscriber themselves"
    );
    assert_eq!(
        stream.candidate_sources[0].sender_id, subscriber.sender_id,
        "the loopback edge: publisher and subscriber are the same participant"
    );
    assert!(
        !stream.supersede_on_independent_frame,
        "every audio frame forwards; MH is told the behaviour, never the media type"
    );
    assert_eq!(stream.transport_mode, TransportMode::Datagram as i32);
    assert_ne!(
        stream.priority_group, 0,
        "MC assigns the priority group; 0 is the never-set default"
    );
}
