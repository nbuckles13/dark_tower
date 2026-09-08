// Every `#[tokio::test]` in this file is pinned to `flavor = "current_thread"`
// — the `notify_participant_*` handlers `record_mh_notification` synchronously
// on the caller's task. On `current_thread` that's the test thread and
// `MetricAssertion` captures the emission. See
// `crates/common/src/observability/testing.rs:60-72`.
//
//! Component tests for `McMediaCoordinationService` driving real
//! `mc_mh_notifications_received_total` emissions per ADR-0032 Step 3
//! §Cluster D.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ::common::observability::testing::MetricAssertion;
use ::common::secret::SecretBox;
use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::grpc::McMediaCoordinationService;
use mc_service::media_admission::SenderBindingOutcome;
use mc_service::media_routing::PolicyGenerations;
use mc_service::mh_connection_registry::MhConnectionRegistry;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationService;
use proto_gen::dark_tower::internal::v1::{
    NotifyParticipantConnectedRequest, NotifyParticipantDisconnectedRequest,
};
use tonic::Request;

fn make_controller() -> Arc<MeetingControllerActorHandle> {
    Arc::new(MeetingControllerActorHandle::new(
        "mc-media-coord-test".to_string(),
        ActorMetrics::new(),
        ControllerMetrics::new(),
        SecretBox::new(Box::new(vec![0u8; 32])),
        Arc::new(MhConnectionRegistry::new()),
        Arc::new(PolicyGenerations::new()),
    ))
}

fn make_service() -> McMediaCoordinationService {
    McMediaCoordinationService::new(Arc::new(MhConnectionRegistry::new()), make_controller())
}

/// A service sharing one controller with the caller, so the test can seed
/// meetings and participants the handler will then resolve against.
fn make_service_with(controller: &Arc<MeetingControllerActorHandle>) -> McMediaCoordinationService {
    McMediaCoordinationService::new(
        Arc::new(MhConnectionRegistry::new()),
        Arc::clone(controller),
    )
}

// ---------------------------------------------------------------------------
// `mc_mh_notifications_received_total` — direct-call coverage
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn notify_participant_connected_records_event_connected() {
    let svc = make_service();
    let req = Request::new(NotifyParticipantConnectedRequest {
        meeting_id: "meeting-1".to_string(),
        participant_id: "part-1".to_string(),
        handler_id: "mh-1".to_string(),
    });

    let snap = MetricAssertion::snapshot();
    svc.notify_participant_connected(req).await.unwrap();

    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "connected")])
        .assert_delta(1);
    // Adjacency catches a future event-label swap.
    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "disconnected")])
        .assert_delta(0);
}

#[tokio::test(flavor = "current_thread")]
async fn notify_participant_disconnected_records_event_disconnected() {
    let svc = make_service();
    let req = Request::new(NotifyParticipantDisconnectedRequest {
        meeting_id: "meeting-1".to_string(),
        participant_id: "part-1".to_string(),
        handler_id: "mh-1".to_string(),
        reason: 0,
    });

    let snap = MetricAssertion::snapshot();
    svc.notify_participant_disconnected(req).await.unwrap();

    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "disconnected")])
        .assert_delta(1);
    snap.counter("mc_mh_notifications_received_total")
        .with_labels(&[("event_type", "connected")])
        .assert_delta(0);
}

// ---------------------------------------------------------------------------
// `sender_id` binding contract (R-15) — the send side
//
// MC answers `NotifyParticipantConnected` with the participant's allocated
// ordinal, or with `0` meaning "I do not know this participant". These arms
// assert the WIRE VALUE on every path, not merely the outcome label: the
// no-production-constructor argument in `media_admission/sender_id.rs` is a
// structural guarantee about what CANNOT happen, and is not a substitute for
// observing what DOES.
// ---------------------------------------------------------------------------

/// Seed a real joined participant and return its allocated `sender_id`.
///
/// The expectation is DERIVED from the allocator's actual output rather than
/// written as a literal: a test asserting `1` would pass on the exact input it
/// was written for and keep passing if the handler returned the allocator's
/// counter instead of this participant's id.
async fn seed_participant(
    controller: &Arc<MeetingControllerActorHandle>,
    meeting_id: &str,
    participant_id: &str,
) -> u32 {
    controller
        .create_meeting(meeting_id.to_string())
        .await
        .expect("create_meeting should succeed");

    join_participant(controller, meeting_id, participant_id).await
}

/// Join a participant into an ALREADY-created meeting, returning its allocated
/// ordinal as it appears on the wire.
async fn join_participant(
    controller: &Arc<MeetingControllerActorHandle>,
    meeting_id: &str,
    participant_id: &str,
) -> u32 {
    let (outbound_tx, _outbound_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);
    let join_rx = controller
        .join_connection(
            meeting_id.to_string(),
            format!("conn-{participant_id}"),
            format!("user-{participant_id}"),
            participant_id.to_string(),
            String::new(),
            false,
            None,
            outbound_tx,
        )
        .await
        .expect("join_connection should succeed");

    let result = tokio::time::timeout(std::time::Duration::from_secs(3), join_rx)
        .await
        .expect("timeout waiting for join")
        .expect("join channel dropped")
        .expect("join should succeed");

    u32::from(result.sender_id.get().get())
}

/// The token `sub` MC stores as `user_id` for a participant seeded by
/// [`join_participant`].
///
/// Derived from that helper's own construction rather than re-spelled, so the
/// two cannot drift.
fn token_sub_for(participant_id: &str) -> String {
    format!("user-{participant_id}")
}

/// Build the request MH would send.
///
/// `token_sub` goes in the `participant_id` field because that is what the
/// contract says the field carries: **the validated meeting token's `sub`**, not
/// MC's per-join UUID. Naming the parameter `token_sub` rather than
/// `participant_id` is deliberate — the field's name is exactly what misled the
/// first implementation into keying MC's lookup on the wrong namespace, and a
/// test that repeats the misleading name invites the same mistake back.
fn connected_request(
    meeting_id: &str,
    token_sub: &str,
) -> Request<NotifyParticipantConnectedRequest> {
    Request::new(NotifyParticipantConnectedRequest {
        meeting_id: meeting_id.to_string(),
        participant_id: token_sub.to_string(),
        handler_id: "mh-1".to_string(),
    })
}

#[tokio::test(flavor = "current_thread")]
async fn resolvable_participant_gets_its_own_allocated_sender_id() {
    let controller = make_controller();
    // A throwaway joins FIRST so the participant under test is never allocated
    // ordinal 1. Without this the arm passes against a hardcoded `1` — verified
    // by mutation, not assumed: the allocator issues 1 to the first joiner, so
    // an expectation derived from the allocator still cannot discriminate when
    // the participant under test IS the first joiner.
    let first = seed_participant(&controller, "meeting-bind", "part-decoy").await;
    let allocated = join_participant(&controller, "meeting-bind", "part-bind").await;
    assert_ne!(
        allocated, first,
        "allocator must not recycle; the decoy exists to move the id under test off 1"
    );
    assert!(
        allocated > 1,
        "the participant under test must not hold ordinal 1, or this arm cannot \
         distinguish a correct lookup from a hardcoded 1"
    );
    let svc = make_service_with(&controller);

    let snap = MetricAssertion::snapshot();
    let response = svc
        .notify_participant_connected(connected_request(
            "meeting-bind",
            &token_sub_for("part-bind"),
        ))
        .await
        .expect("notify should succeed")
        .into_inner();

    assert_eq!(
        response.sender_id, allocated,
        "MC must answer with the ordinal the allocator issued to THIS participant"
    );
    assert!(
        response.sender_id >= 1,
        "a resolved binding is never the reserved-invalid 0"
    );
    assert!(response.acknowledged);

    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::Resolved.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // Adjacency: a future swap that resolved correctly but counted the wrong
    // outcome would otherwise pass the assertion above.
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::ParticipantUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_meeting_answers_zero_and_counts_meeting_unknown() {
    let controller = make_controller();
    let svc = make_service_with(&controller);

    let snap = MetricAssertion::snapshot();
    let response = svc
        .notify_participant_connected(connected_request(
            "no-such-meeting",
            &token_sub_for("part-1"),
        ))
        .await
        .expect("notify should succeed")
        .into_inner();

    assert_eq!(
        response.sender_id, 0,
        "MC must answer 0 for a meeting it does not hold — never an invented or borrowed id"
    );
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::MeetingUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // An unheld meeting must NOT be triaged as a join race.
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::ParticipantUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_participant_in_known_meeting_answers_zero() {
    let controller = make_controller();
    // A real meeting with a real participant, so the ONLY thing missing is the
    // participant being asked about. Without the seeded joiner this would pass
    // for the wrong reason (empty meeting), which is the vacuity this arm
    // exists to avoid.
    let seeded = seed_participant(&controller, "meeting-race", "part-present").await;
    let svc = make_service_with(&controller);

    let snap = MetricAssertion::snapshot();
    let response = svc
        .notify_participant_connected(connected_request(
            "meeting-race",
            &token_sub_for("part-absent"),
        ))
        .await
        .expect("notify should succeed")
        .into_inner();

    assert_eq!(
        response.sender_id, 0,
        "a participant not on the roster gets 0 — MC must not hand back the id of the \
         participant that IS present"
    );
    assert_ne!(
        response.sender_id, seeded,
        "answering with a roster neighbour's ordinal is the cross-participant injection \
         primitive this contract exists to prevent"
    );
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::ParticipantUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // A held meeting must NOT be reported as a routing fault.
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::MeetingUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn distinct_participants_resolve_to_their_own_distinct_ids() {
    let controller = make_controller();
    let first = seed_participant(&controller, "meeting-two", "part-a").await;

    let second = join_participant(&controller, "meeting-two", "part-b").await;

    let svc = make_service_with(&controller);
    let a = svc
        .notify_participant_connected(connected_request("meeting-two", &token_sub_for("part-a")))
        .await
        .expect("notify a")
        .into_inner()
        .sender_id;
    let b = svc
        .notify_participant_connected(connected_request("meeting-two", &token_sub_for("part-b")))
        .await
        .expect("notify b")
        .into_inner()
        .sender_id;

    // PER-PARTICIPANT CORRESPONDENCE, not mere distinctness: a swapped
    // implementation satisfies `a != b` while binding each participant to the
    // other's ordinal, which is precisely the defect this contract prevents.
    assert_eq!(a, first, "part-a must resolve to part-a's allocated id");
    assert_eq!(b, second, "part-b must resolve to part-b's allocated id");
    assert_ne!(a, b, "two live participants never share an ordinal");

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn same_participant_id_in_two_meetings_resolves_per_meeting() {
    // `sender_id` is a PER-MEETING ordinal: id 5 exists concurrently in every
    // meeting on a handler. A resolution path that dropped the meeting key
    // would answer meeting A's ordinal for a meeting-B question — a
    // cross-tenant media-crossing primitive that the two-participants-in-one-
    // meeting arm above structurally cannot catch.
    let controller = make_controller();
    let in_a = seed_participant(&controller, "meeting-alpha", "shared-id").await;
    let in_b = seed_participant(&controller, "meeting-beta", "shared-id").await;
    let svc = make_service_with(&controller);

    let from_a = svc
        .notify_participant_connected(connected_request(
            "meeting-alpha",
            &token_sub_for("shared-id"),
        ))
        .await
        .expect("notify alpha")
        .into_inner()
        .sender_id;
    let from_b = svc
        .notify_participant_connected(connected_request(
            "meeting-beta",
            &token_sub_for("shared-id"),
        ))
        .await
        .expect("notify beta")
        .into_inner()
        .sender_id;

    assert_eq!(from_a, in_a, "meeting-alpha's question gets alpha's answer");
    assert_eq!(from_b, in_b, "meeting-beta's question gets beta's answer");

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn every_outcome_label_is_emittable() {
    // POSITIVE CONTROL over the label vocabulary: `ALL` is iterated rather than
    // re-enumerated, so a new variant added without a recorder path fails
    // here instead of shipping as a series nobody emits.
    for outcome in SenderBindingOutcome::ALL {
        let snap = MetricAssertion::snapshot();
        mc_service::observability::metrics::record_sender_binding_response(outcome);
        snap.counter("mc_media_sender_binding_responses_total")
            .with_labels(&[("outcome", outcome.label()), ("key_custody", "operator")])
            .assert_delta(1);
    }
}

// ---------------------------------------------------------------------------
// Identity-translation regression (Gate 2 attempt 1)
//
// THE DEFECT THIS FILE FAILED TO CATCH THE FIRST TIME. Every arm above passed
// while MC keyed its lookup on `participant_id` — because the fixture happened
// to pass MC's `participant_id` in the request too, so both sides of the
// comparison used one namespace and the mismatch was invisible. In production
// MH sends the token `sub`, MC's roster is keyed by a per-join UUID MH has never
// seen, and every media connection returned `participant_unknown`.
//
// A component test whose fixture supplies both sides of an identity comparison
// cannot detect that the two sides are different namespaces. These two arms fix
// that by asserting the namespaces are NOT interchangeable.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn mcs_own_participant_id_does_not_resolve_only_the_token_sub_does() {
    let controller = make_controller();
    let allocated = seed_participant(&controller, "meeting-ns", "part-ns").await;
    let svc = make_service_with(&controller);

    // The token `sub` — what MH actually sends — resolves.
    let by_sub = svc
        .notify_participant_connected(connected_request("meeting-ns", &token_sub_for("part-ns")))
        .await
        .expect("notify by sub")
        .into_inner()
        .sender_id;
    assert_eq!(
        by_sub, allocated,
        "the token sub is the identifier MH holds and MUST resolve"
    );

    // MC's own per-join participant id is a DIFFERENT namespace. MH never sees
    // it and never sends it; if this ever started resolving, MC would be
    // accepting an identifier whose provenance is not the validated token —
    // which is the @security S1 hazard, not a convenience.
    let by_participant_id = svc
        .notify_participant_connected(connected_request("meeting-ns", "part-ns"))
        .await
        .expect("notify by participant id")
        .into_inner()
        .sender_id;
    assert_eq!(
        by_participant_id, 0,
        "MC's internal participant_id is not the contract's identifier and must NOT resolve; \
         if this returns non-zero the lookup is keyed on the wrong namespace and production \
         media will silently fail closed on every connection"
    );

    controller.cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn one_user_with_two_participants_is_ambiguous_and_fails_closed() {
    // MC mints a fresh participant_id per join and does not bar the same user
    // joining twice, so the token `sub` can match two roster entries. Answering
    // with either would bind MH's connection to an ordinal that may belong to
    // the user's OTHER participant — right half the time, undetectable when
    // wrong.
    let controller = make_controller();
    controller
        .create_meeting("meeting-dual".to_string())
        .await
        .expect("create_meeting");

    let shared_sub = "user-dual".to_string();
    let mut ordinals = Vec::new();
    for conn in ["device-a", "device-b"] {
        let (tx, _rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(100);
        let join_rx = controller
            .join_connection(
                "meeting-dual".to_string(),
                format!("conn-{conn}"),
                shared_sub.clone(),
                format!("participant-{conn}"),
                String::new(),
                false,
                None,
                tx,
            )
            .await
            .expect("join_connection");
        ordinals.push(u32::from(
            tokio::time::timeout(std::time::Duration::from_secs(3), join_rx)
                .await
                .expect("timeout")
                .expect("channel dropped")
                .expect("join should succeed")
                .sender_id
                .get()
                .get(),
        ));
    }
    assert_ne!(
        ordinals[0], ordinals[1],
        "two participants always hold distinct ordinals, even for one user"
    );

    let svc = make_service_with(&controller);
    let snap = MetricAssertion::snapshot();
    let response = svc
        .notify_participant_connected(connected_request("meeting-dual", &shared_sub))
        .await
        .expect("notify should succeed")
        .into_inner();

    assert_eq!(
        response.sender_id, 0,
        "an ambiguous sub must fail closed, never pick a candidate"
    );
    assert!(
        !ordinals.contains(&response.sender_id),
        "MC must not answer with EITHER participant's ordinal — a coin flip is wrong half the \
         time and there is no signal saying which half"
    );

    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::UserAmbiguous.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
    // Ambiguity is NOT the join race: it does not clear by waiting, and its
    // remedy is a contract change rather than a retry.
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[
            ("outcome", SenderBindingOutcome::ParticipantUnknown.label()),
            ("key_custody", "operator"),
        ])
        .assert_delta(0);

    controller.cancel();
}
