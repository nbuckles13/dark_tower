// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// and that pinning is LOAD-BEARING — do not "simplify" it away. `MetricAssertion`
// binds a per-thread recorder; `MhClient::register_meeting` makes a real gRPC
// call whose post-RPC `record_register_meeting()` emission lives on the
// caller's task. On `current_thread` that task IS the test thread; on
// multi-thread it can land on a worker and the assertion silently observes
// zero. See `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `MhClient::register_meeting()` driving real
//! `mc_register_meeting_total` and `mc_register_meeting_duration_seconds`
//! emissions per ADR-0032 Step 3 §Cluster E.
//!
//! Both paths run against `mc_test_utils::MediaHandlerStub`, the shared
//! configurable MH. The success path needs
//! `AppliedGenerationBehaviour::EchoSent` **selected deliberately**: under
//! ADR-0036 §8, MC treats only `applied_generation == policy_generation` as
//! success, so a stub reporting the honest default (0, "nothing installed")
//! makes the push diverge and `mc_register_meeting_total{status="success"}`
//! stay flat. The stub's default is that honest 0 precisely so a success test
//! cannot inherit the echo-a-received-value lie by accident.
//!
//! The `outcome`-labelled media-path metrics have their own home in
//! `media_policy_push_integration.rs`; this file stays on the two
//! `mc_register_meeting_*` series it was written for.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ::common::observability::testing::MetricAssertion;
use mc_service::grpc::{MeetingProgramming, MhClient};
use mc_test_utils::media::{loopback_assignment, TEST_HANDLER_ID};
use mc_test_utils::mock_mh::{AppliedGenerationBehaviour, MediaHandlerStub};
use mc_test_utils::test_token_receiver;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn register_meeting_success_emits_status_success_and_duration_observation() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully(TEST_HANDLER_ID)
        .spawn()
        .await;
    let endpoint = stub.endpoint();
    let assignment = loopback_assignment();
    let client = MhClient::new(test_token_receiver());

    let snap = MetricAssertion::snapshot();
    let result = client
        .register_meeting(&MeetingProgramming {
            mh_grpc_endpoint: &endpoint,
            expected_handler_id: TEST_HANDLER_ID,
            meeting_id: "meeting-success",
            mc_id: "mc-test",
            mc_grpc_endpoint: "http://mc-test:50052",
            assignment: &assignment,
            policy_generation: std::num::NonZeroU64::MIN,
        })
        .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Histogram first (drain-on-read).
    snap.histogram("mc_register_meeting_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
}

/// A handler that parses the registration but installs nothing. `accepted` is
/// deliberately left `true`: ADR-0036 §8 says `accepted` is not evidence of
/// application, so this asserts MC routes on the applied echo rather than on
/// the acknowledgement.
#[tokio::test(flavor = "current_thread")]
async fn register_meeting_not_applied_emits_status_error() {
    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::ReportZero)
        .handler_id(TEST_HANDLER_ID)
        .spawn()
        .await;
    let endpoint = stub.endpoint();
    let assignment = loopback_assignment();
    let client = MhClient::new(test_token_receiver());

    let snap = MetricAssertion::snapshot();
    let result = client
        .register_meeting(&MeetingProgramming {
            mh_grpc_endpoint: &endpoint,
            expected_handler_id: TEST_HANDLER_ID,
            meeting_id: "meeting-not-applied",
            mc_id: "mc-test",
            mc_grpc_endpoint: "http://mc-test:50052",
            assignment: &assignment,
            policy_generation: std::num::NonZeroU64::MIN,
        })
        .await;
    assert!(
        result.is_err(),
        "accepted==true must NOT be treated as success, got {result:?}"
    );

    snap.histogram("mc_register_meeting_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "error")])
        .assert_delta(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "success")])
        .assert_delta(0);
}

// NOTE on the connect-failure branch:
//
// `MhClient::register_meeting()` returns `McError::Grpc("Failed to connect:
// ...")` on connect failure BEFORE the metric-recording block. So an
// unreachable-endpoint test would observe `mc_register_meeting_total` delta=0.
// This is a small fidelity gap in the recorder placement (tracked as
// informational, not a Step-3 fix target). The two tests above cover the two
// emission branches that DO fire: (1) the applied echo matches → success, and
// (2) it does not → error.

/// O-11 regression: a non-fatal `handler_id_mismatch` must record
/// `status="success"`, because the meeting **is** programmed.
///
/// Neither test above distinguishes the two mappings — `Match` and
/// `NoAppliedGeneration` agree under both `outcome == Match` and
/// `disposition() == Programmed` — which is exactly why the contradiction
/// between `mh_client.rs` and `confirm.rs::disposition()` survived review. This
/// is the only input that tells them apart.
///
/// Why it matters beyond consistency: `handler_id` is a per-incarnation token,
/// so this outcome fires on **every ordinary MH pod restart**. Keyed on
/// `== Match`, that recorded a failure on `mc_register_meeting_total`, which is
/// a plain success/error split with no yellow, no description caveat and no
/// `outcome!~` escape — reintroducing on a pre-existing series the false
/// positive OPS-17/OPS-18 disarmed on `mc_media_policy_pushes_total`. Story
/// task 21 writes MC alert rules against this surface, so leaving it would have
/// laid a trap for that task.
#[tokio::test(flavor = "current_thread")]
async fn non_fatal_handler_id_mismatch_records_status_success() {
    // Programmed correctly, but the handler asserts a different id — an MH pod
    // that restarted since MC captured its assignment snapshot.
    let stub = MediaHandlerStub::builder()
        .accept(true)
        .applied_generation(AppliedGenerationBehaviour::EchoSent)
        .handler_id("mh-test-0-restarted-a1b2c3d4")
        .transport_mode(proto_gen::dark_tower::signaling::v1::TransportMode::Datagram)
        .spawn()
        .await;
    let endpoint = stub.endpoint();
    let assignment = loopback_assignment();
    let client = MhClient::new(test_token_receiver());

    let snap = MetricAssertion::snapshot();
    let result = client
        .register_meeting(&MeetingProgramming {
            mh_grpc_endpoint: &endpoint,
            expected_handler_id: TEST_HANDLER_ID,
            meeting_id: "meeting-handler-restarted",
            mc_id: "mc-test",
            mc_grpc_endpoint: "http://mc-test:50052",
            assignment: &assignment,
            policy_generation: std::num::NonZeroU64::MIN,
        })
        .await;

    assert!(
        result.is_ok(),
        "handler_id_mismatch is non-fatal for the interim; got {result:?}"
    );
    snap.histogram("mc_register_meeting_duration_seconds")
        .assert_observation_count_at_least(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "success")])
        .assert_delta(1);
    snap.counter("mc_register_meeting_total")
        .with_labels(&[("status", "error")])
        .assert_delta(0);
}
