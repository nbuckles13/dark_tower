//! Per-(meeting, handler) heartbeat intake through the real gRPC service and
//! meeting actor (ADR-0039 §1, §2, §6; story 4 MC-B1: R-7, R-9, R-10, R-11).
//!
//! Every test runs `current_thread` + `start_paused`: `MetricAssertion` only
//! sees same-thread emissions, and time moves only by `advance` (no sleeps).
//! Every negative arm has its positive arm in the same test.
//!
//! What is asserted is what MH and the operator can observe — the wire verdict,
//! the gRPC status, the counters — plus MC's own state through the test-seams
//! probe (`probe_handler_for_test`): the binding ledger, the connectivity keys
//! and the liveness record MC-B2's `assess()` will read.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;
use std::time::Duration;

use ::common::observability::testing::MetricAssertion;
use mc_service::actors::{JoinMedia, MeetingActorHandle, MeetingControllerActorHandle};
use mc_service::grpc::McMediaCoordinationService;
use mc_service::media_routing::{HeartbeatTiming, RemovalTrigger};
use mc_service::media_signaling::ReceiveCapabilityDeclaration;
use proto_gen::dark_tower::internal::v1::{
    ApplyRefusal, HeartbeatVerdict, MeetingHeartbeatRequest, ReleaseCause,
};
use proto_gen::dark_tower::signaling::v1::{MediaKind, ReceiveCapability};
use test_common::coordination::{beat, connect, disconnect, heartbeat_request};
use test_common::media_session::slot;
use test_common::run_ready;
use test_common::{
    client_media_config, handler_set, heartbeat_timing, join_media_on, join_media_with_heartbeat,
    mh_handler, MockMhRegistrationClient,
};
use tonic::Code;

const MC: &str = "mc-heartbeat-test";
const KC: (&str, &str) = ("key_custody", "operator");
const H_A: &str = "mh-a";
const H_B: &str = "mh-b";

// ----------------------------------------------------------------------------
// Fixtures
// ----------------------------------------------------------------------------

fn controller() -> Arc<MeetingControllerActorHandle> {
    Arc::new(test_common::make_controller(MC))
}

fn service(c: &Arc<MeetingControllerActorHandle>) -> McMediaCoordinationService {
    McMediaCoordinationService::new(Arc::clone(c), test_common::heartbeat_intake(MC))
}

/// A service whose `connection_ids` bound is `max_participants` × 4.
fn service_with_cap(
    c: &Arc<MeetingControllerActorHandle>,
    max_participants: u32,
) -> McMediaCoordinationService {
    McMediaCoordinationService::new(
        Arc::clone(c),
        test_common::heartbeat_intake_with_cap(MC, max_participants),
    )
}

async fn create(c: &Arc<MeetingControllerActorHandle>, meeting: &str, handlers: &[&str]) {
    let infos: Vec<_> = handlers.iter().map(|h| mh_handler(h)).collect();
    c.create_meeting(meeting.to_string(), handler_set(&infos))
        .await
        .unwrap();
}

async fn handle(c: &Arc<MeetingControllerActorHandle>, meeting: &str) -> MeetingActorHandle {
    c.get_meeting_handle(meeting.to_string()).await.unwrap()
}

/// Join `p` (token sub `user-p`), awaiting the (possibly parked) reply.
async fn join(c: &Arc<MeetingControllerActorHandle>, meeting: &str, p: &str, media: JoinMedia) {
    let rx = start_join(c, meeting, p, media).await;
    rx.await.unwrap().unwrap();
}

async fn start_join(
    c: &Arc<MeetingControllerActorHandle>,
    meeting: &str,
    p: &str,
    media: JoinMedia,
) -> tokio::sync::oneshot::Receiver<
    Result<mc_service::actors::JoinResult, mc_service::errors::McError>,
> {
    let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(256);
    // The outbound queue is drained by nobody; keep the receiver alive.
    std::mem::forget(rx);
    c.join_connection(
        meeting.to_string(),
        format!("conn-{p}"),
        format!("user-{p}"),
        p.to_string(),
        String::new(),
        false,
        test_common::test_identity_key(),
        tx,
        media,
    )
    .await
    .unwrap()
}

fn receiving_one_audio_slot(mut media: JoinMedia) -> JoinMedia {
    media.demand = ReceiveCapabilityDeclaration::parse(
        &ReceiveCapability {
            slots: vec![slot(0, MediaKind::Audio)],
        },
        true,
    )
    .expect("valid declaration")
    .truncate(client_media_config().receive_slot_caps);
    media
}

fn heartbeat(meeting: &str, handler: &str, applied: u64, ids: &[&str]) -> MeetingHeartbeatRequest {
    heartbeat_request(
        MC,
        meeting,
        handler,
        applied,
        ApplyRefusal::Unspecified,
        ids,
    )
}

fn owned() -> (HeartbeatVerdict, ReleaseCause) {
    (HeartbeatVerdict::Owned, ReleaseCause::Unspecified)
}

/// Every heartbeat outcome's delta, so an arm also proves adjacency.
fn assert_outcomes(
    snap: &::common::observability::testing::MetricSnapshot,
    expect: &[(&str, u64)],
) {
    for outcome in [
        "owned",
        "released_removed",
        "released_ended",
        "released_unassigned",
        "owner_mismatch",
    ] {
        let want = expect
            .iter()
            .find(|(o, _)| *o == outcome)
            .map_or(0, |(_, n)| *n);
        snap.counter("mc_media_handler_heartbeats_total")
            .with_labels(&[("outcome", outcome), KC])
            .assert_delta(want);
    }
}

// ----------------------------------------------------------------------------
// Verdicts (L4-6, L4-7): every arm in one test
// ----------------------------------------------------------------------------

/// OWNED for an Active and a Registering member; RELEASED with EACH removed
/// handler's own trigger (two removals, two triggers); RELEASED(MEETING_ENDED)
/// with no actor; RELEASED(UNSPECIFIED) for a handler the live meeting never
/// held; FAILED_PRECONDITION for a foreign `mc_id` — with an in-set handler
/// (else OWNED) and with a meeting that has no actor (else MEETING_ENDED), so
/// the owner check provably runs first. Each arm moves exactly its outcome.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn verdicts_all_arms_in_one_test() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.fail_handler(H_B); // mh-b never acks: stays Registering
    create(&c, "m", &[H_A, H_B, "mh-c", "mh-d"]).await;
    join(&c, "m", "p1", join_media_on(Arc::clone(&mh))).await;
    let meeting = handle(&c, "m").await;
    assert!(meeting
        .remove_handler_for_test("mh-c", RemovalTrigger::HeartbeatTimeout)
        .await
        .unwrap());
    assert!(meeting
        .remove_handler_for_test("mh-d", RemovalTrigger::PlannedTransfer)
        .await
        .unwrap());

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &[])).await,
        Ok(owned()),
        "Active"
    );
    assert_outcomes(&snap, &[("owned", 1)]);

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_B, 0, &[])).await,
        Ok(owned()),
        "Registering"
    );
    assert_outcomes(&snap, &[("owned", 1)]);

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", "mh-c", 0, &[])).await,
        Ok((HeartbeatVerdict::Released, ReleaseCause::HeartbeatTimeout))
    );
    assert_eq!(
        beat(&svc, heartbeat("m", "mh-d", 0, &[])).await,
        Ok((HeartbeatVerdict::Released, ReleaseCause::PlannedTransfer))
    );
    assert_outcomes(&snap, &[("released_removed", 2)]);

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("no-such-meeting", H_A, 0, &[])).await,
        Ok((HeartbeatVerdict::Released, ReleaseCause::MeetingEnded))
    );
    assert_outcomes(&snap, &[("released_ended", 1)]);

    let snap = MetricAssertion::snapshot();
    assert!(
        !meeting
            .probe_handler_for_test("mh-never")
            .await
            .unwrap()
            .release_noticed
    );
    for _ in 0..2 {
        assert_eq!(
            beat(&svc, heartbeat("m", "mh-never", 0, &[])).await,
            Ok((HeartbeatVerdict::Released, ReleaseCause::Unspecified))
        );
    }
    assert_outcomes(&snap, &[("released_unassigned", 2)]);
    assert!(
        meeting
            .probe_handler_for_test("mh-never")
            .await
            .unwrap()
            .release_noticed,
        "WARNed once and remembered: the second RELEASED is counted, not logged"
    );

    let snap = MetricAssertion::snapshot();
    for meeting_id in ["m", "no-such-meeting"] {
        let mut req = heartbeat(meeting_id, H_A, 0, &[]);
        req.mc_id = "mc-other".to_string();
        assert_eq!(beat(&svc, req).await, Err(Code::FailedPrecondition));
    }
    assert_outcomes(&snap, &[("owner_mismatch", 2)]);

    // The removed handlers' liveness entries are gone with them, and a later
    // heartbeat does not recreate one (DRY 1: one removal path).
    assert_eq!(
        meeting
            .probe_handler_for_test("mh-c")
            .await
            .unwrap()
            .liveness,
        None
    );
    assert!(meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .is_some());
    c.cancel();
}

/// Race gate: the handler's first heartbeat, arriving before MC has processed
/// its RegisterMeeting ack, is OWNED (it entered the set as Registering before
/// the push was sent). Positive arm: still OWNED once the ack lands.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn first_heartbeat_before_register_ack_is_owned() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    mh.hold_handler(H_A);
    create(&c, "m", &[H_A]).await;
    let reply = start_join(&c, "m", "p1", join_media_on(Arc::clone(&mh))).await;
    run_ready().await;
    assert_eq!(
        mh.calls().len(),
        1,
        "the RegisterMeeting is in flight, unacked"
    );

    assert_eq!(beat(&svc, heartbeat("m", H_A, 0, &[])).await, Ok(owned()));

    mh.release_handler(H_A);
    reply.await.unwrap().unwrap();
    assert_eq!(beat(&svc, heartbeat("m", H_A, 1, &[])).await, Ok(owned()));
    c.cancel();
}

// ----------------------------------------------------------------------------
// Remove-only reconcile (previous-heartbeat rule)
// ----------------------------------------------------------------------------

/// K1 bound at t0; hb1 at t1 lists nothing (first heartbeat: retires nothing);
/// K_eq bound AT t1 (equal instant); K2 bound after t1; hb2 at t2 lists none
/// of them → only K1 retired (bound strictly before t1). hb3 at t3 → K_eq and
/// K2 retired, which proves the keep came from the rule, not from never
/// retiring. Retirements release the ledger, drive the routing resync (the
/// pushed snapshot loses the edge) and count exactly. An unknown listed id is
/// never added.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn reconcile_retires_only_keys_bound_before_the_previous_heartbeat() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A]).await;
    join(&c, "m", "pub", join_media_on(Arc::clone(&mh))).await;
    join(
        &c,
        "m",
        "sub",
        receiving_one_audio_slot(join_media_on(Arc::clone(&mh))),
    )
    .await;
    let meeting = handle(&c, "m").await;

    // t0: the publisher binds K1; the subscriber K3 (listed in every heartbeat).
    assert_ne!(connect(&svc, "m", "pub", H_A, "K1").await, 0);
    assert_ne!(connect(&svc, "m", "sub", H_A, "K3").await, 0);
    tokio::time::advance(client_media_config().connect_settle_window).await;
    run_ready().await;
    meeting.get_state().await.unwrap();
    run_ready().await;
    assert_eq!(
        mh.calls().last().unwrap().egress_stream_count,
        1,
        "pub -> sub edge pushed"
    );

    // t1: first heartbeat for the pair retires nothing, even though K1 is absent.
    tokio::time::advance(Duration::from_secs(1)).await;
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &["K3"])).await,
        Ok(owned())
    );
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(0);
    // Bound at exactly t1 (no advance): equal to the previous receipt.
    assert_ne!(connect(&svc, "m", "pub", H_A, "K_eq").await, 0);

    tokio::time::advance(Duration::from_secs(1)).await;
    assert_ne!(connect(&svc, "m", "pub", H_A, "K2").await, 0);

    // t2: K1 (t0 < t1) retired; K_eq (== t1) kept; K2 (> t1) kept; "ghost" not added.
    tokio::time::advance(Duration::from_secs(1)).await;
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &["K3", "ghost"])).await,
        Ok(owned())
    );
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(1);
    let probe = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert_eq!(
        probe.bindings,
        vec!["K2", "K3", "K_eq"],
        "K1's binding released"
    );
    let conns: Vec<&str> = probe.connectivity.iter().map(|(_, k)| k.as_str()).collect();
    assert_eq!(
        conns.iter().filter(|k| **k == "ghost").count(),
        0,
        "never added"
    );
    assert!(!conns.contains(&"K1") && conns.contains(&"K_eq") && conns.contains(&"K2"));
    assert_eq!(
        mh.calls().last().unwrap().egress_stream_count,
        1,
        "pub still routed"
    );

    // t3: K_eq and K2 now precede the previous receipt (t2): both retired, the
    // publisher loses its last key, and the routing resync drops the edge.
    tokio::time::advance(Duration::from_secs(1)).await;
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &["K3"])).await,
        Ok(owned())
    );
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(2);
    run_ready().await;
    let probe = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert_eq!(probe.bindings, vec!["K3"]);
    assert_eq!(
        probe.connectivity,
        vec![(probe.connectivity[0].0.clone(), "K3".to_string())]
    );
    assert_eq!(
        mh.calls().last().unwrap().egress_stream_count,
        0,
        "the routing resync pushed the snapshot without the edge"
    );
    c.cancel();
}

/// paired-test A: a heartbeat walks only ITS handler's keys. mh-a's heartbeat
/// never retires a key or binding on mh-b, however old. Positive arm: mh-b's
/// own heartbeats retire it.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn reconcile_is_scoped_to_the_heartbeating_handler() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A, H_B]).await;
    join(&c, "m", "p", join_media_on(Arc::clone(&mh))).await;
    let meeting = handle(&c, "m").await;
    connect(&svc, "m", "p", H_A, "Ka").await;
    connect(&svc, "m", "p", H_B, "Kb").await;

    let snap = MetricAssertion::snapshot();
    for _ in 0..3 {
        tokio::time::advance(Duration::from_secs(1)).await;
        beat(&svc, heartbeat("m", H_A, 0, &[])).await.unwrap();
    }
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(1);
    let b = meeting.probe_handler_for_test(H_B).await.unwrap();
    assert_eq!(b.bindings, vec!["Kb"], "mh-b's binding untouched");
    assert_eq!(b.connectivity.len(), 1, "mh-b's key untouched");
    assert!(meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .bindings
        .is_empty());

    for _ in 0..2 {
        tokio::time::advance(Duration::from_secs(1)).await;
        beat(&svc, heartbeat("m", H_B, 0, &[])).await.unwrap();
    }
    assert!(meeting
        .probe_handler_for_test(H_B)
        .await
        .unwrap()
        .bindings
        .is_empty());
    c.cancel();
}

/// Security P-1: an over-cap or malformed `connection_ids` list still gets its
/// verdict and counts as liveness, but SKIPS the reconcile (counted, no
/// correction). Positive arm: the next well-formed heartbeat retires the key.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_over_cap_or_malformed_list_skips_the_reconcile_only() {
    let c = controller();
    let svc = service_with_cap(&c, 1); // bound: 1 x 4 = 4 ids
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A]).await;
    join(&c, "m", "p", join_media_on(Arc::clone(&mh))).await;
    let meeting = handle(&c, "m").await;
    connect(&svc, "m", "p", H_A, "K1").await;
    tokio::time::advance(Duration::from_secs(1)).await;
    beat(&svc, heartbeat("m", H_A, 0, &[])).await.unwrap();

    for (ids, reason) in [
        (vec!["a", "b", "c", "d", "e"], "connection_ids_over_cap"),
        (vec!["a", ""], "connection_id_malformed"),
    ] {
        tokio::time::advance(Duration::from_secs(1)).await;
        let snap = MetricAssertion::snapshot();
        assert_eq!(beat(&svc, heartbeat("m", H_A, 0, &ids)).await, Ok(owned()));
        assert_outcomes(&snap, &[("owned", 1)]);
        snap.counter("mc_media_handler_heartbeat_anomalies_total")
            .with_labels(&[("reason", reason), KC])
            .assert_delta(1);
        snap.counter("mc_media_handler_connected_set_corrections_total")
            .with_labels(&[KC])
            .assert_delta(0);
        assert_eq!(
            meeting.probe_handler_for_test(H_A).await.unwrap().bindings,
            vec!["K1"]
        );
    }
    let last = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();
    assert_eq!(
        last.last_accepted,
        tokio::time::Instant::now(),
        "still liveness"
    );

    tokio::time::advance(Duration::from_secs(1)).await;
    let snap = MetricAssertion::snapshot();
    beat(&svc, heartbeat("m", H_A, 0, &[])).await.unwrap();
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(1);
    c.cancel();
}

// ----------------------------------------------------------------------------
// Stale heartbeats, lag and the interval histogram
// ----------------------------------------------------------------------------

/// Applied G (accepted) → 0 (stale: no liveness, no instant shift, no interval
/// observation, but one lag observation) → G (equal: accepted) → G+1 (above
/// what MC sent: accepted as liveness, `applied_ahead_of_sent` counted, NO lag
/// observation). Before any push there is no lag reading at all.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn stale_heartbeat_is_not_liveness_and_lag_is_read_against_what_mc_sent() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A]).await;
    let meeting = handle(&c, "m").await;

    // No push yet: no lag reading, never a fake 0.
    let snap = MetricAssertion::snapshot();
    beat(&svc, heartbeat("m", H_A, 0, &[])).await.unwrap();
    snap.histogram("mc_media_handler_applied_sequence_lag")
        .with_labels(&[KC])
        .assert_unobserved();

    join(&c, "m", "p", join_media_on(Arc::clone(&mh))).await;
    let sent = mh.calls().last().unwrap().policy_generation;
    // K is bound after the first heartbeat and before the accepted one below,
    // and NO later heartbeat lists it: only an accepted heartbeat whose
    // previous receipt is that accepted one may retire it.
    assert_ne!(connect(&svc, "m", "p", H_A, "K").await, 0);

    tokio::time::advance(Duration::from_secs(2)).await;
    let snap = MetricAssertion::snapshot();
    beat(&svc, heartbeat("m", H_A, sent, &[])).await.unwrap();
    snap.histogram("mc_media_handler_heartbeat_interval_seconds")
        .with_labels(&[KC])
        .assert_observation_count(1);
    let accepted = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();

    tokio::time::advance(Duration::from_secs(2)).await;
    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &[])).await,
        Ok(owned()),
        "still OWNED"
    );
    snap.histogram("mc_media_handler_heartbeat_interval_seconds")
        .with_labels(&[KC])
        .assert_unobserved();
    let after_stale = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();
    assert_eq!(after_stale, accepted, "a stale heartbeat changes nothing");
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(0);
    assert_eq!(
        meeting.probe_handler_for_test(H_A).await.unwrap().bindings,
        vec!["K"],
        "a stale heartbeat never drives the reconcile"
    );
    // The stale one WAS a lag reading (sent - 0).
    let snap = MetricAssertion::snapshot();
    tokio::time::advance(Duration::from_secs(2)).await;
    beat(&svc, heartbeat("m", H_A, 0, &[])).await.unwrap();
    snap.histogram("mc_media_handler_applied_sequence_lag")
        .with_labels(&[KC])
        .assert_observation_count(1);

    tokio::time::advance(Duration::from_secs(2)).await;
    let snap = MetricAssertion::snapshot();
    beat(&svc, heartbeat("m", H_A, sent, &[])).await.unwrap();
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(1);
    assert!(
        meeting
            .probe_handler_for_test(H_A)
            .await
            .unwrap()
            .bindings
            .is_empty(),
        "positive arm: the equal-generation (accepted) heartbeat retires K"
    );
    let equal = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();
    assert_eq!(
        equal.previous_accepted,
        Some(accepted.last_accepted),
        "equal is accepted"
    );
    assert_eq!(equal.last_accepted, tokio::time::Instant::now());

    tokio::time::advance(Duration::from_secs(2)).await;
    let snap = MetricAssertion::snapshot();
    beat(&svc, heartbeat("m", H_A, sent + 1, &[]))
        .await
        .unwrap();
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "applied_ahead_of_sent"), KC])
        .assert_delta(1);
    snap.histogram("mc_media_handler_applied_sequence_lag")
        .with_labels(&[KC])
        .assert_unobserved();
    let ahead = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();
    assert_eq!(
        ahead.last_accepted,
        tokio::time::Instant::now(),
        "ahead of sent: liveness (a live MH)"
    );
    assert_eq!(
        ahead.applied_generation, sent,
        "but the watermark is clamped to what MC sent (security S-2): it never \
         takes the ahead value"
    );
    // The poisoning twin: the honest heartbeat after it, at a generation
    // BELOW the old raw ahead value, is still liveness (not stale).
    tokio::time::advance(Duration::from_secs(2)).await;
    beat(&svc, heartbeat("m", H_A, sent, &[])).await.unwrap();
    let honest = meeting
        .probe_handler_for_test(H_A)
        .await
        .unwrap()
        .liveness
        .unwrap();
    assert_eq!(honest.last_accepted, tokio::time::Instant::now());
    assert_eq!(honest.previous_accepted, Some(ahead.last_accepted));
    c.cancel();
}

// ----------------------------------------------------------------------------
// The binding gate and RELEASED mutating nothing
// ----------------------------------------------------------------------------

/// A Connected from a REMOVED handler gets sender 0 and changes nothing (ledger,
/// connectivity), counted `handler_not_in_set` + `refused`. The removal itself
/// purged the handler's connectivity and bulk-released its bindings (MC-B2), so
/// a zombie's Disconnected after removal is a counted no-op that finds nothing
/// and leaves the replacement's connectivity untouched. A RELEASED heartbeat with an empty
/// list mutates nothing and corrects nothing. Positive arm: the in-set handler
/// binds.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_binding_gate_refuses_a_removed_handler_and_released_mutates_nothing() {
    let c = controller();
    let svc = service(&c);
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A, H_B]).await;
    join(&c, "m", "p", join_media_on(Arc::clone(&mh))).await;
    let meeting = handle(&c, "m").await;
    let sender = connect(&svc, "m", "p", H_A, "Ka").await;
    assert_ne!(sender, 0);
    assert_eq!(
        connect(&svc, "m", "p", H_B, "Kb").await,
        sender,
        "positive arm"
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    beat(&svc, heartbeat("m", H_A, 0, &["Ka"])).await.unwrap();
    assert!(meeting
        .remove_handler_for_test(H_A, RemovalTrigger::ApplyLag)
        .await
        .unwrap());
    let before_a = meeting.probe_handler_for_test(H_A).await.unwrap();
    let before_b = meeting.probe_handler_for_test(H_B).await.unwrap();

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        connect(&svc, "m", "p", H_A, "Ka2").await,
        0,
        "removed: refused"
    );
    disconnect(&svc, "m", "p", H_A, "Ka").await;
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &[])).await,
        Ok((HeartbeatVerdict::Released, ReleaseCause::ApplyLag))
    );
    snap.counter("mc_mh_notifications_unapplied_total")
        .with_labels(&[("reason", "handler_not_in_set"), KC])
        .assert_delta(2);
    snap.counter("mc_media_sender_binding_responses_total")
        .with_labels(&[("outcome", "refused"), KC])
        .assert_delta(1);
    snap.counter("mc_media_handler_connected_set_corrections_total")
        .with_labels(&[KC])
        .assert_delta(0);
    let after_a = meeting.probe_handler_for_test(H_A).await.unwrap();
    assert_eq!(
        (after_a.bindings, after_a.connectivity),
        (before_a.bindings.clone(), before_a.connectivity.clone()),
        "ledger and connectivity unchanged (only the WARN dedupe moved)"
    );
    assert_eq!(
        meeting.probe_handler_for_test(H_B).await.unwrap(),
        before_b,
        "replacement untouched"
    );
    assert!(
        before_a.bindings.is_empty() && before_a.connectivity.is_empty(),
        "the removal purged the handler's keys and bulk-released its binding"
    );
    assert_eq!(
        before_b.bindings,
        vec!["Kb"],
        "the survivor's binding is kept"
    );
    c.cancel();
}

// ----------------------------------------------------------------------------
// The actor-send bound: a drop is counted, never silent, never RELEASED
// ----------------------------------------------------------------------------

/// With the meeting actor parked and its mailbox full, the heartbeat's send
/// waits at most the budget (< one interval), is counted on
/// `mc_messages_dropped_total{actor_type="meeting"}` exactly once and answered
/// UNAVAILABLE — never RELEASED. Positive arm: once the actor drains, OWNED.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_full_meeting_mailbox_answers_unavailable_and_counts_the_drop() {
    let c = controller();
    let svc = service(&c);
    create(&c, "m", &[H_A]).await;
    let meeting = handle(&c, "m").await;
    let release = meeting.park_for_test().await.unwrap();
    assert!(meeting.fill_mailbox_for_test() > 0);

    let snap = MetricAssertion::snapshot();
    let started = tokio::time::Instant::now();
    assert_eq!(
        beat(&svc, heartbeat("m", H_A, 0, &[])).await,
        Err(Code::Unavailable)
    );
    assert!(
        started.elapsed() < heartbeat_timing().interval(),
        "bounded below one interval"
    );
    snap.counter("mc_messages_dropped_total")
        .with_labels(&[("actor_type", "meeting")])
        .assert_delta(1);
    assert_outcomes(&snap, &[]);

    release.send(()).unwrap();
    meeting.get_state().await.unwrap();
    assert_eq!(beat(&svc, heartbeat("m", H_A, 0, &[])).await, Ok(owned()));
    c.cancel();
}

// ----------------------------------------------------------------------------
// Config: the interval MC sends MH
// ----------------------------------------------------------------------------

/// N = 7000 (not divisible by 3): every RegisterMeeting carries
/// floor(7000/3) = 2333 ms.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn register_meeting_carries_the_derived_heartbeat_interval() {
    let c = controller();
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A]).await;
    join(
        &c,
        "m",
        "p",
        join_media_with_heartbeat(
            Arc::clone(&mh) as Arc<dyn mc_service::grpc::MhRegistrationClient>,
            HeartbeatTiming::from_timeout_ms(7_000),
        ),
    )
    .await;
    let calls = mh.calls();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|call| call.heartbeat_interval_ms == 2_333));
    c.cancel();
}

/// The deadline config gauge echoes N from the SAME `HeartbeatTiming` the
/// intake and the RegisterMeeting interval derive from (ADR-0032).
#[tokio::test(flavor = "current_thread")]
async fn the_heartbeat_deadline_gauge_echoes_n() {
    let snap = MetricAssertion::snapshot();
    mc_service::observability::metrics::set_heartbeat_deadline(HeartbeatTiming::from_timeout_ms(
        7_000,
    ));
    snap.gauge("mc_media_handler_heartbeat_deadline_seconds")
        .with_labels(&[KC])
        .assert_value(7.0);
}

/// Observability F2: a heartbeat that is BOTH over the list bound AND ahead of
/// what MC sent counts BOTH reasons — neither masks the other.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn every_anomaly_reason_of_one_heartbeat_is_counted() {
    let c = controller();
    let svc = service_with_cap(&c, 1); // bound: 1 x 4 = 4 ids
    let mh = Arc::new(MockMhRegistrationClient::new());
    create(&c, "m", &[H_A]).await;
    join(&c, "m", "p", join_media_on(Arc::clone(&mh))).await;
    let sent = mh.calls().last().unwrap().policy_generation;

    let snap = MetricAssertion::snapshot();
    assert_eq!(
        beat(
            &svc,
            heartbeat("m", H_A, sent + 1, &["a", "b", "c", "d", "e"])
        )
        .await,
        Ok(owned())
    );
    for reason in ["connection_ids_over_cap", "applied_ahead_of_sent"] {
        snap.counter("mc_media_handler_heartbeat_anomalies_total")
            .with_labels(&[("reason", reason), KC])
            .assert_delta(1);
    }
    snap.counter("mc_media_handler_heartbeat_anomalies_total")
        .with_labels(&[("reason", "connection_id_malformed"), KC])
        .assert_delta(0);
    c.cancel();
}
