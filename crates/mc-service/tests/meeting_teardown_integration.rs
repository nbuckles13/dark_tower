//! Meeting teardown on the media handlers, over REAL gRPC (story 2 task 12;
//! R-20; `EndMeetingRequest` in `internal.proto`).
//!
//! MC's real `MhClient` talks to `mc-test-utils` MH stubs, which record every
//! call to either RPC in ONE interleaved log (`StubEvent`). That log is what
//! makes the contract's ordering assertable directly: every `RegisterMeeting`
//! has RETURNED before the `EndMeeting` arrives, and nothing is registered
//! after it — never inferred from two counts.
//!
//! The controller is built with a recording `MeetingEndedNotifier`, so the GC
//! notification is asserted too — both that it fires for a meeting that
//! ENDED, and that it does not for one that was torn down for another reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use common::observability::testing::MetricAssertion;
use mc_service::grpc::{MhClient, MhRegistrationClient};
use mc_service::redis::{MhAssignmentStore, MhEndpointInfo};
use mc_test_utils::mock_mh::{MediaHandlerStub, MediaHandlerStubHandle, StubEvent};
use test_common::accept_loop_rig::AcceptLoopRig;
use test_common::media_session::{audio_slots, capability_frame, join_as};
use test_common::{
    build_test_stack_with_notifier, client_media_config, seed_meeting_with_handlers,
    RecordingMeetingEnded, TestStackHandles,
};

/// The rig and stack, with a real `MhClient` and a recording GC notifier.
async fn stack(label: &str) -> (TestStackHandles, AcceptLoopRig, Arc<RecordingMeetingEnded>) {
    let notifier = Arc::new(RecordingMeetingEnded::default());
    let stack = build_test_stack_with_notifier(label, Arc::clone(&notifier) as _).await;
    let rig = AcceptLoopRig::start_with_media_config(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_store) as Arc<dyn MhAssignmentStore>,
        Arc::new(MhClient::new(mc_test_utils::test_token_receiver()))
            as Arc<dyn MhRegistrationClient>,
        "mc-test".to_string(),
        "http://mc-test:50052".to_string(),
        32,
        client_media_config(),
    )
    .await;
    (stack, rig, notifier)
}

fn endpoint(id: &str, stub: &MediaHandlerStubHandle) -> MhEndpointInfo {
    MhEndpointInfo {
        mh_id: id.to_string(),
        webtransport_endpoint: format!("wt://{id}:4433"),
        grpc_endpoint: stub.endpoint(),
    }
}

async fn wait_for_end(stub: &MediaHandlerStubHandle, expected: usize) -> Vec<StubEvent> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let events = stub.events();
        let ends = events
            .iter()
            .filter(|e| matches!(e, StubEvent::EndMeeting { .. }))
            .count();
        if ends >= expected {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "expected {expected} EndMeeting on the stub; events: {events:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The contract, per handler: exactly one `EndMeeting`, naming this meeting and
/// carrying this MC's `mc_id`; every `RegisterMeeting` RETURNED before it; and
/// none arrived after it.
fn assert_released_after_pushes_stopped(events: &[StubEvent], meeting_id: &str, handler: &str) {
    let end_at = events
        .iter()
        .position(|e| matches!(e, StubEvent::EndMeeting { .. }))
        .unwrap_or_else(|| panic!("{handler}: no EndMeeting in {events:?}"));
    let ends: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            StubEvent::EndMeeting { meeting_id, mc_id } => {
                Some((meeting_id.clone(), mc_id.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        vec![(meeting_id.to_string(), "mc-test".to_string())],
        "{handler}: EXACTLY one EndMeeting — none extra, none missing — for this meeting, with \
         this MC's own mc_id"
    );
    let (before_end, after_end) = events.split_at(end_at);
    let arrived = before_end
        .iter()
        .filter(|e| matches!(e, StubEvent::RegisterArrived { .. }))
        .count();
    let returned = before_end
        .iter()
        .filter(|e| matches!(e, StubEvent::RegisterReturned { .. }))
        .count();
    assert!(
        arrived > 0,
        "{handler}: positive control — the meeting was programmed"
    );
    assert_eq!(
        arrived, returned,
        "{handler}: every RegisterMeeting had RETURNED before the release arrived (none \
         abandoned in flight)"
    );
    assert!(
        !after_end
            .iter()
            .any(|e| matches!(e, StubEvent::RegisterArrived { .. })),
        "{handler}: nothing was registered after the release"
    );
}

/// Two handlers, two participants: the meeting ends when both leave, and each
/// handler is released exactly once — after every push to it has returned —
/// and only THEN is GC told the meeting ended.
#[tokio::test]
async fn the_last_leave_releases_every_handler_once_after_its_pushes_stopped() {
    // One stub holds each RegisterMeeting in flight, so a push is genuinely
    // outstanding when the meeting ends: the drain must WAIT for it.
    let slow = MediaHandlerStub::builder()
        .programs_successfully("mh-slow")
        .register_delay(Duration::from_millis(300))
        .spawn()
        .await;
    let fast = MediaHandlerStub::builder()
        .programs_successfully("mh-fast")
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-both").await;
    seed_meeting_with_handlers(
        &stack,
        "td-both",
        vec![endpoint("mh-slow", &slow), endpoint("mh-fast", &fast)],
    )
    .await;
    let snap = MetricAssertion::snapshot();

    let mut a = join_as(&rig, &stack, "td-both", "user-a").await;
    let mut b = join_as(&rig, &stack, "td-both", "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    a.close();
    b.close();

    let slow_events = wait_for_end(&slow, 1).await;
    let fast_events = wait_for_end(&fast, 1).await;
    assert_released_after_pushes_stopped(&slow_events, "td-both", "mh-slow");
    assert_released_after_pushes_stopped(&fast_events, "td-both", "mh-fast");

    // GC is told the meeting ended — exactly once, for this meeting — and only
    // after the release (both ends were already recorded above).
    assert_eq!(
        notifier.wait_for(1, Duration::from_secs(10)).await,
        vec!["td-both".to_string()]
    );
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[("outcome", "released"), ("key_custody", "operator")])
        .assert_delta(2);
    snap.counter("mc_media_push_quiesce_total")
        .with_labels(&[("outcome", "quiesced"), ("key_custody", "operator")])
        .assert_delta(1);
    // The fence was lifted by the teardown's own completion, never by the
    // backstop deadline (which would have skipped the GC notification above).
    snap.counter("mc_media_teardown_fence_backstop_total")
        .with_labels(&[("key_custody", "operator")])
        .assert_delta(0);
}

/// `FAILED_PRECONDITION` (another MC holds the meeting) is terminal: counted
/// `rejected_ownership`, never retried.
#[tokio::test]
async fn an_ownership_rejection_is_counted_and_not_retried() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-other")
        .end_meeting_status(tonic::Code::FailedPrecondition)
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-mismatch").await;
    seed_meeting_with_handlers(&stack, "td-mismatch", vec![endpoint("mh-other", &stub)]).await;
    let snap = MetricAssertion::snapshot();
    join_as(&rig, &stack, "td-mismatch", "user-a").await.close();

    // The teardown's COMPLETION (GC being told) is the signal that no further
    // attempt will follow — no timed wait for an absence.
    notifier.wait_for(1, Duration::from_secs(10)).await;
    assert_eq!(
        stub.end_meeting_requests().len(),
        1,
        "a mismatch is never retried"
    );
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[
            ("outcome", "rejected_ownership"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

/// The same `FAILED_PRECONDITION` on a teardown MC runs because the meeting is
/// being REMOVED rather than ended (the graceful-shutdown path, where a
/// successor MC may already hold the meeting) is counted on its own value,
/// `superseded_by_successor` — never `rejected_ownership`, whose alert must not
/// fire on a rolling deploy.
#[tokio::test]
async fn an_ownership_refusal_on_the_shutdown_path_is_superseded_not_rejected() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-successor")
        .end_meeting_status(tonic::Code::FailedPrecondition)
        .spawn()
        .await;
    let (stack, rig, _notifier) = stack("teardown-superseded").await;
    seed_meeting_with_handlers(
        &stack,
        "td-superseded",
        vec![endpoint("mh-successor", &stub)],
    )
    .await;
    let _held = join_as(&rig, &stack, "td-superseded", "user-a").await;
    let snap = MetricAssertion::snapshot();
    stack
        .controller_handle
        .remove_meeting("td-superseded".to_string())
        .await
        .unwrap();
    wait_for_end(&stub, 1).await;

    // The outcome is recorded right after the reply; poll the counter itself.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while snap
        .counter("mc_media_end_meeting_total")
        .with_labels(&[
            ("outcome", "superseded_by_successor"),
            ("key_custody", "operator"),
        ])
        .delta()
        == 0
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the shutdown-path refusal was never counted"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[
            ("outcome", "superseded_by_successor"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[
            ("outcome", "rejected_ownership"),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);
    assert_eq!(stub.end_meeting_requests().len(), 1, "never retried");
}

/// A graceful shutdown RELEASES its live meetings before the process would
/// exit, which is `main`'s sequence: `shutdown` (returns once every meeting
/// actor has exited and handed its teardown off), then settle the teardown
/// tracker. No polling of the stub: when settle reports nothing cut off, the
/// release has already reached MH. Were `shutdown` to return before the
/// actors drained, settle would see zero in flight at once and exit with the
/// release never sent — the runtime drop that used to cut every shutdown
/// release off.
#[tokio::test]
async fn a_graceful_shutdown_releases_live_meetings_before_its_settle_returns() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-shutdown")
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-shutdown").await;
    seed_meeting_with_handlers(&stack, "td-shutdown", vec![endpoint("mh-shutdown", &stub)]).await;
    let _held = join_as(&rig, &stack, "td-shutdown", "user-a").await;
    // Positive control: the meeting is programmed on the handler.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !stub
        .events()
        .iter()
        .any(|e| matches!(e, StubEvent::RegisterReturned { .. }))
    {
        assert!(tokio::time::Instant::now() < deadline, "never programmed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let teardowns = stack.controller_handle.teardowns();
    let budget = mc_service::media_routing::teardown::SHUTDOWN_RELEASE_BUDGET;
    let release_deadline = tokio::time::Instant::now() + budget;
    stack.controller_handle.shutdown(budget).await.unwrap();
    let cut_off = teardowns.settle_until(release_deadline).await;

    assert_eq!(cut_off, 0, "a healthy release finishes inside the budget");
    assert_released_after_pushes_stopped(&stub.events(), "td-shutdown", "mh-shutdown");
    assert!(
        notifier.ended().is_empty(),
        "a shutdown is not a meeting end: GC is not told"
    );
}

/// `UNAVAILABLE` is retried (a release is idempotent), bounded, then counted
/// `unavailable_exhausted`.
#[tokio::test]
async fn unavailable_is_retried_to_the_bound_then_counted() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-down")
        .end_meeting_status(tonic::Code::Unavailable)
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-unavailable").await;
    seed_meeting_with_handlers(&stack, "td-unavail", vec![endpoint("mh-down", &stub)]).await;
    let snap = MetricAssertion::snapshot();
    join_as(&rig, &stack, "td-unavail", "user-a").await.close();

    // Completion (GC told) follows the last attempt and its recorded outcome.
    notifier.wait_for(1, Duration::from_secs(20)).await;
    let max = mc_service::media_routing::teardown::MAX_END_MEETING_ATTEMPTS as usize;
    assert_eq!(
        stub.end_meeting_requests().len(),
        max,
        "retried exactly to the bound"
    );
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[
            ("outcome", "unavailable_exhausted"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

/// `UNIMPLEMENTED` (an MH that predates the RPC) is expected during a
/// wrong-order rollout: counted, never retried, and teardown proceeds.
#[tokio::test]
async fn unimplemented_is_counted_once_and_teardown_proceeds() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-old")
        .end_meeting_status(tonic::Code::Unimplemented)
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-unimpl").await;
    seed_meeting_with_handlers(&stack, "td-unimpl", vec![endpoint("mh-old", &stub)]).await;
    let snap = MetricAssertion::snapshot();
    join_as(&rig, &stack, "td-unimpl", "user-a").await.close();

    // Teardown proceeds to completion: GC is still told.
    notifier.wait_for(1, Duration::from_secs(10)).await;
    assert_eq!(stub.end_meeting_requests().len(), 1, "never retried");
    snap.counter("mc_media_end_meeting_total")
        .with_labels(&[("outcome", "unimplemented"), ("key_custody", "operator")])
        .assert_delta(1);
}

/// The cause filter, exercised on a REAL non-ending teardown with the
/// controller still running: a meeting REMOVED (not ended) releases its
/// handlers but does NOT tell GC. Positive control in the same test: a meeting
/// that empties does.
#[tokio::test]
async fn a_removed_meeting_is_released_without_telling_gc_and_an_ended_one_tells_gc() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-x")
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-cause").await;
    seed_meeting_with_handlers(&stack, "td-removed", vec![endpoint("mh-x", &stub)]).await;
    seed_meeting_with_handlers(&stack, "td-ended", vec![endpoint("mh-x", &stub)]).await;

    let _held = join_as(&rig, &stack, "td-removed", "user-a").await;
    stack
        .controller_handle
        .remove_meeting("td-removed".to_string())
        .await
        .unwrap();
    wait_for_end(&stub, 1).await;

    join_as(&rig, &stack, "td-ended", "user-b").await.close();
    let ended = notifier.wait_for(1, Duration::from_secs(10)).await;
    wait_for_end(&stub, 2).await;
    assert_eq!(
        ended,
        vec!["td-ended".to_string()],
        "only the meeting that ENDED is reported to GC; the removed one never is"
    );
}

/// A meeting re-created under the same id after its teardown is a fresh
/// install: it registers from generation 1 (MH forgot the old one, and MC
/// evicted its own).
#[tokio::test]
async fn a_meeting_recreated_after_teardown_registers_from_generation_one() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-re")
        .spawn()
        .await;
    let (stack, rig, notifier) = stack("teardown-recreate").await;
    seed_meeting_with_handlers(&stack, "td-again", vec![endpoint("mh-re", &stub)]).await;
    let mut a = join_as(&rig, &stack, "td-again", "user-a").await;
    let mut b = join_as(&rig, &stack, "td-again", "user-b").await;
    a.write(capability_frame(audio_slots(1))).await;
    b.write(capability_frame(audio_slots(1))).await;
    let _ = a.settle().await;
    a.close();
    b.close();
    notifier.wait_for(1, Duration::from_secs(10)).await;
    let before = stub.received_generations();
    assert!(
        before.iter().any(|g| *g >= 2),
        "positive control: gen advanced past 1"
    );

    seed_meeting_with_handlers(&stack, "td-again", vec![endpoint("mh-re", &stub)]).await;
    let _c = join_as(&rig, &stack, "td-again", "user-c").await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while stub.received_generations().len() == before.len() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no push for the new incarnation"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        stub.received_generations()[before.len()],
        1,
        "the new incarnation starts at generation 1"
    );
}

/// The teardown FENCE: a create for a meeting id whose previous incarnation is
/// still being released is PARKED, not failed, and answered only once the
/// release has gone out — so it can never race the release onto MH.
#[tokio::test]
async fn a_create_during_teardown_waits_for_the_release_then_succeeds() {
    let stub = MediaHandlerStub::builder()
        .programs_successfully("mh-fence")
        .register_delay(Duration::from_millis(500))
        .spawn()
        .await;
    let (stack, rig, _n) = stack("teardown-fence").await;
    seed_meeting_with_handlers(&stack, "td-fence", vec![endpoint("mh-fence", &stub)]).await;
    join_as(&rig, &stack, "td-fence", "user-a").await.close();

    // Wait until the meeting has ended (MC answers "teardown in progress"),
    // then ask to create it again while the slow push is still draining.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match stack
            .controller_handle
            .get_meeting_handle("td-fence".to_string())
            .await
        {
            Err(mc_service::errors::McError::MeetingTeardownInProgress) => break,
            _ => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the meeting never ended"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
    stack
        .controller_handle
        .create_meeting("td-fence".to_string())
        .await
        .expect("the parked create succeeds once the release is out");
    assert!(
        stub.events()
            .iter()
            .any(|e| matches!(e, StubEvent::EndMeeting { .. })),
        "the create was answered only AFTER the previous incarnation's release"
    );
}
