//! Crash replacement via GC (ADR-0039 §3/§4; story 4 MC-C: R-1, R-2, R-6,
//! R-23 MC side).
//!
//! Tier 1b: the real gRPC coordination service and meeting actors under a
//! controller, `current_thread` + `start_paused` (time moves only by
//! `advance`), a LIVE fake MH (`MockMhRegistrationClient::heartbeat_via`: it
//! heartbeats every meeting it acked, so `silence` is a crash) and a scripted
//! GC (`mc_test_utils::ScriptedGc`) answering `AcquireMediaHandler`.
//!
//! Counting unit (observability ruling): `mc_media_handler_migrations_total`
//! records ONE outcome per acquisition attempt — one Acquire call, or one
//! owed replacement refused at the bound. Each arm asserts its exact deltas,
//! and [`assert_one_outcome_per_attempt`] pins "sum of outcome deltas ==
//! Acquire calls + bound refusals".

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;

use ::common::observability::testing::MetricAssertion;
use mc_service::grpc::gc_client::{AcquireRejection, AcquireResult};
use mc_service::media_routing::{HandlerId, MigrationOutcome, RemovalTrigger};
use proto_gen::dark_tower::signaling::v1::server_message;
use test_common::replacement_rig::*;
use test_common::{advance_live, replacement_timing, run_ready};

// ----------------------------------------------------------------------------
// R-2: the stranded pair recovers on the replacement (break-before-make)
// ----------------------------------------------------------------------------

/// Set {mh-1, mh-2}; A and B are connected ONLY to mh-1. mh-1 crashes; GC
/// grants mh-3. mh-3's RegisterMeeting is held: while it is in flight MC has
/// already sent it (RegisterMeeting(mh-3) is recorded) and NO client set names
/// mh-3 — register before token. Released, the set names mh-3 with a token for
/// it. After A and B report Connected on mh-3, mh-3's snapshot carries the
/// A↔B edges (one stream each way) and neither is reported unreachable to the
/// other. The attempt is `completed`, with all three phases observed.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_stranded_pair_hears_each_other_on_the_granted_replacement() {
    let r = rig();
    r.gc.push(grant(H3));
    r.mh.hold_handler(H3);
    let meeting = r.create("m", &[H1, H2]).await;
    let mut a = r.join("m", "a").await;
    let mut b = r.join("m", "b").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    assert_ne!(r.connect("m", "b", H1, "b1").await, 0);
    advance_live(settle()).await;
    test_common::drain_frames(&mut a);
    test_common::drain_frames(&mut b);
    let snap = MetricAssertion::snapshot();

    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    let calls = r.gc.wait_for_calls(1, n()).await;
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.trigger, RemovalTrigger::HeartbeatTimeout);
    assert_eq!(call.replaced, Some(HandlerId::new(H1)));
    assert_eq!(
        call.exclude
            .iter()
            .map(HandlerId::as_str)
            .collect::<Vec<_>>(),
        vec![H1, H2],
        "current ∪ removed, by id"
    );

    // While mh-3's RegisterMeeting is in flight: sent, and named by no set.
    run_ready().await;
    assert!(r.registers_to(H3) > 0, "RegisterMeeting(mh-3) was issued");
    let before = sets(&test_common::drain_frames(&mut a));
    assert_eq!(
        before.iter().map(|(u, _)| u.clone()).collect::<Vec<_>>(),
        vec![vec![url(H2)]],
        "break-before-make: the set without mh-1, and nothing naming mh-3 yet"
    );
    let probe = meeting.probe_handler_for_test(H3).await.unwrap();
    assert_eq!(probe.health, Some("healthy"), "a Registering member");
    // A histogram query drains its own name+labels, so each phase is read at
    // its own boundary, when only it has been observed since the previous read.
    snap.histogram("mc_media_handler_migration_duration_seconds")
        .with_labels(&[
            ("trigger", "heartbeat_timeout"),
            ("phase", "replacement"),
            KC,
        ])
        .assert_observation_count(1);

    r.mh.release_handler(H3);
    run_ready().await;
    snap.histogram("mc_media_handler_migration_duration_seconds")
        .with_labels(&[
            ("trigger", "heartbeat_timeout"),
            ("phase", "registration"),
            KC,
        ])
        .assert_observation_count(1);
    for q in [&mut a, &mut b] {
        let after = sets(&test_common::drain_frames(q));
        let last = after.last().expect("a set naming the replacement");
        assert!(last.0.contains(&url(H3)), "{after:?}");
        assert!(
            last.1.contains(&H3.to_string()),
            "a media_dial token for mh-3"
        );
        assert!(!last.0.contains(&url(H1)));
    }

    // Recovery is the join path: both connect to mh-3; nothing special.
    assert_ne!(r.connect("m", "a", H3, "a3").await, 0);
    assert_ne!(r.connect("m", "b", H3, "b3").await, 0);
    advance_live(settle()).await;
    snap.histogram("mc_media_handler_migration_duration_seconds")
        .with_labels(&[("trigger", "heartbeat_timeout"), ("phase", "reconnect"), KC])
        .assert_observation_count(1);
    let last_h3 =
        r.mh.calls()
            .into_iter()
            .rfind(|c| c.expected_handler_id == H3)
            .unwrap();
    assert_eq!(
        last_h3.egress_stream_count, 2,
        "mh-3 carries A->B and B->A (one audio slot each)"
    );
    for q in [&mut a, &mut b] {
        let frames = test_common::drain_frames(q);
        let assignments = frames
            .iter()
            .rev()
            .find_map(|f| match f {
                server_message::Message::StreamAssignments(s) => Some(s.clone()),
                _ => None,
            })
            .expect("a fresh StreamAssignments after the reconnect");
        assert!(
            assignments.unreachable_sender_ids.is_empty(),
            "neither is reported unreachable to the other: {assignments:?}"
        );
    }

    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::Completed),
        1
    );
    assert_one_outcome_per_attempt(&snap, r.gc.calls().len(), 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// R-6: removal never waits on GC
// ----------------------------------------------------------------------------

/// Two handlers, both participants on both; GC is UNAVAILABLE. The survivor
/// re-places the edges without GC; MC keeps retrying with backoff (every
/// attempt counted `gc_unavailable`), each wait within the config ladder.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn with_gc_unavailable_survivors_re_place_and_attempts_back_off() {
    let r = rig();
    r.gc.set_default(unavailable());
    let _meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    let _b = r.join("m", "b").await;
    for (p, h, k) in [
        ("a", H1, "a1"),
        ("b", H1, "b1"),
        ("a", H2, "a2"),
        ("b", H2, "b2"),
    ] {
        assert_ne!(r.connect("m", p, h, k).await, 0);
    }
    advance_live(settle()).await;
    // NON-VACUITY: crash the handler that CARRIES the pair's edges (the
    // co-locate policy puts both on one handler), so the survivor's two
    // streams afterwards are a re-placement, not the initial placement.
    let last_to = |h: &str| {
        r.mh.calls()
            .into_iter()
            .rfind(|c| c.expected_handler_id == h)
            .map_or(0, |c| c.egress_stream_count)
    };
    let (carrier, survivor) = if last_to(H1) > 0 { (H1, H2) } else { (H2, H1) };
    assert_eq!(
        last_to(carrier),
        2,
        "the carrier holds both streams before the crash"
    );
    assert_eq!(
        last_to(survivor),
        0,
        "the survivor holds none before the crash"
    );
    let snap = MetricAssertion::snapshot();
    r.mh.silence(carrier);
    advance_live(past_declaration()).await;
    assert_eq!(
        last_to(survivor),
        2,
        "re-placed onto the survivor, without GC"
    );

    let timing = replacement_timing();
    advance_live(timing.acquisition_budget()).await;
    let calls = r.gc.calls().len();
    assert!(calls > 1, "retried: {calls}");
    // Every gap between attempts is at least the minimum interval and at most
    // one capped ladder step (plus the advance granularity).
    let times = r.gc.call_times();
    for w in times.windows(2) {
        let gap = w[1] - w[0];
        assert!(
            gap >= timing.min_interval() && gap <= timing.backoff_max() + interval(),
            "attempt gap {gap:?} outside [min interval, backoff max]"
        );
    }
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::GcUnavailable),
        calls as u64
    );
    assert_one_outcome_per_attempt(&snap, calls, 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Awaiting handler: no survivor, no capacity, then a later grant
// ----------------------------------------------------------------------------

/// One handler crashes and GC has no capacity: the meeting is awaiting a
/// handler (gauge 1), MC keeps acquiring with backoff (`no_capacity` per
/// call). When GC later grants mh-2, the meeting recovers by itself: the set
/// names mh-2 and the gauge returns to 0. `no_capacity` == calls − 1 and
/// `completed` +1 on the last call.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn awaiting_handler_retries_until_a_later_grant_recovers_the_meeting() {
    let r = rig();
    let _meeting = r.create("m", &[H1]).await;
    let mut a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    snap.gauge("mc_media_meetings_without_handlers")
        .with_labels(&[KC])
        .assert_value(1.0);
    advance_live(replacement_timing().ladder_step(3)).await;
    let failed = r.gc.calls().len();
    assert!(failed > 1, "kept acquiring: {failed}");

    r.gc.set_default(grant(H2));
    advance_live(replacement_timing().backoff_max() + interval()).await;
    let calls = r.gc.calls().len();
    assert!(calls > failed);
    let frames = test_common::drain_frames(&mut a);
    let last = sets(&frames).last().cloned().unwrap();
    assert_eq!(last.0, vec![url(H2)], "recovered onto the grant");
    snap.gauge("mc_media_meetings_without_handlers")
        .with_labels(&[KC])
        .assert_value(0.0);
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::NoCapacity),
        calls as u64 - 1
    );
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::Completed),
        1
    );
    assert_one_outcome_per_attempt(&snap, calls, 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Refusals are never retried (positive arm: no capacity IS retried)
// ----------------------------------------------------------------------------

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn not_owner_and_invalid_request_are_not_retried_but_no_capacity_is() {
    for (name, result, retried) in [
        (
            "not_owner",
            AcquireResult::Rejected {
                reason: AcquireRejection::NotOwner,
                code: tonic::Code::FailedPrecondition,
                message: "not this meeting's MC (fixture)".to_string(),
            },
            false,
        ),
        (
            "invalid",
            AcquireResult::Rejected {
                reason: AcquireRejection::InvalidRequest,
                code: tonic::Code::InvalidArgument,
                message: "over the cap (fixture)".to_string(),
            },
            false,
        ),
        (
            "no_capacity",
            AcquireResult::NoCapacity {
                message: "fixture".to_string(),
            },
            true,
        ),
    ] {
        let r = rig();
        r.gc.set_default(result);
        let _meeting = r.create(name, &[H1, H2]).await;
        let _a = r.join(name, "a").await;
        let snap = MetricAssertion::snapshot();
        r.mh.silence(H1);
        advance_live(past_declaration() + replacement_timing().acquisition_budget()).await;
        let calls = r.gc.calls().len();
        if retried {
            assert!(calls > 1, "{name}: retried ({calls})");
            assert_eq!(
                migrations(&snap, "heartbeat_timeout", MigrationOutcome::NoCapacity),
                calls as u64
            );
        } else {
            assert_eq!(calls, 1, "{name}: exactly one call, never retried");
            assert_eq!(
                migrations(&snap, "heartbeat_timeout", MigrationOutcome::Abandoned),
                1,
                "{name}"
            );
        }
        assert_one_outcome_per_attempt(&snap, calls, 0);
        r.c.cancel();
    }
}

// ----------------------------------------------------------------------------
// The per-meeting attempt bound
// ----------------------------------------------------------------------------

/// cap 4, set {mh-1, mh-2}: every grant is refused at admission (its
/// RegisterMeeting fails), so each is removed and excluded. Exactly
/// cap − set = 2 Acquire calls are made, then the meeting stops (rate_limited)
/// and no further call is made however long it waits. A second loss on the
/// stopped meeting is refused too (+1), so "once per meeting" cannot pass.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_attempt_bound_stops_acquisition_when_every_replacement_is_bad() {
    let r = rig_with_cap(4);
    r.gc.push(grant(H3));
    r.gc.push(grant(H4));
    r.mh.fail_handler(H3);
    r.mh.fail_handler(H4);
    let _meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H1);
    advance_live(past_declaration() + replacement_timing().retry_budget() * 2).await;
    let calls = r.gc.calls();
    assert_eq!(calls.len(), 2, "cap − set size = 4 − 2");
    assert!(
        calls[1].exclude.contains(&HandlerId::new(H3)),
        "refused-at-admission excluded"
    );
    assert!(
        calls.iter().all(|c| c.exclude.len() < 4),
        "never an over-cap list"
    );
    assert_eq!(
        migrations(
            &snap,
            "heartbeat_timeout",
            MigrationOutcome::AdmissionRefused
        ),
        2
    );
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::RateLimited),
        1
    );

    // A second loss on the stopped meeting: refused, counted again.
    r.mh.silence(H2);
    advance_live(past_declaration() + replacement_timing().retry_budget()).await;
    assert_eq!(r.gc.calls().len(), 2, "no further call");
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::RateLimited),
        2
    );
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_unobserved();
    assert_one_outcome_per_attempt(&snap, 2, 2);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// A grant removed while still Registering (declared, not a failed register)
// ----------------------------------------------------------------------------

/// A grant whose RegisterMeeting is still in flight is REMOVED by a
/// declaration (here the test seam's removal, StartReason `declared`): that
/// attempt ends `admission_refused` (+1, under the replaced handler's
/// trigger), and acquisition continues with the removed grant excluded. The
/// terminal log's `reason="declared"` is pinned in its own binary
/// (`handler_replacement_log_reasons.rs`: a TRACE capture must not share a
/// process).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_grant_declared_while_registering_ends_admission_refused_and_is_replaced() {
    let r = rig();
    r.gc.push(grant(H3));
    r.mh.hold_handler(H3);
    let meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    r.gc.wait_for_calls(1, n()).await;
    run_ready().await;
    assert!(
        r.registers_to(H3) > 0,
        "the grant is Registering (push held)"
    );
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[
            ("trigger", "heartbeat_timeout"),
            ("outcome", "admission_refused"),
            KC,
        ])
        .assert_unobserved();

    assert!(meeting
        .remove_handler_for_test(H3, RemovalTrigger::ApplyLag)
        .await
        .unwrap());
    let calls =
        r.gc.wait_for_calls(2, replacement_timing().retry_budget())
            .await;
    run_ready().await;
    assert!(
        calls[1].exclude.contains(&HandlerId::new(H3)),
        "the removed grant is excluded"
    );
    assert_eq!(
        calls[1].trigger,
        RemovalTrigger::HeartbeatTimeout,
        "the original migration"
    );
    assert_eq!(
        migrations(
            &snap,
            "heartbeat_timeout",
            MigrationOutcome::AdmissionRefused
        ),
        1
    );
    assert_one_outcome_per_attempt(&snap, r.gc.calls().len(), 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// A late grant for a superseded version
// ----------------------------------------------------------------------------

/// The Acquire for mh-1's loss is HELD at GC while mh-2 is also lost (the set
/// version moves). Released, its grant (mh-4) is discarded: never registered,
/// never named by a set, no token minted for it, no worker. The retry for the
/// new state grants mh-5 — the positive control — which IS registered and
/// named.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_late_grant_for_a_superseded_version_is_discarded() {
    let r = rig();
    r.gc.push(grant(H4));
    r.gc.push(grant(H5));
    r.gc.push(grant("mh-6"));
    r.gc.hold();
    let meeting = r.create("m", &[H1, H2, H3]).await;
    let mut a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    r.gc.wait_for_calls(1, n()).await;
    r.mh.silence(H2);
    advance_live(past_declaration()).await;
    r.gc.release();
    advance_live(replacement_timing().min_interval() * 4 + interval()).await;

    assert_eq!(
        r.registers_to(H4),
        0,
        "the superseded grant is never registered"
    );
    assert!(meeting
        .probe_handler_for_test(H4)
        .await
        .unwrap()
        .health
        .is_none());
    let all = sets(&test_common::drain_frames(&mut a));
    assert!(
        all.iter()
            .all(|(u, aud)| !u.contains(&url(H4)) && !aud.contains(&H4.to_string())),
        "no set and no token names the superseded grant: {all:?}"
    );
    assert!(
        r.registers_to(H5) > 0,
        "positive control: the retry's grant registers"
    );
    assert!(
        all.iter().any(|(u, _)| u.contains(&url(H5))),
        "positive control: and is named: {all:?}"
    );
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::Abandoned),
        1
    );
    assert_one_outcome_per_attempt(&snap, r.gc.calls().len(), 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Minimum interval: a deferral is not an attempt
// ----------------------------------------------------------------------------

/// Two handlers of three die in the same instant. The first replacement is
/// requested at once; the second waits the per-meeting minimum interval
/// (deferred, NOT counted), then is requested and granted: completed +2,
/// rate_limited unobserved.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_minimum_interval_deferral_is_not_counted() {
    let r = rig();
    r.gc.push(grant(H4));
    r.gc.push(grant(H5));
    let _meeting = r.create("m", &[H1, H2, H3]).await;
    let _a = r.join("m", "a").await;
    let snap = MetricAssertion::snapshot();
    r.mh.silence(H1);
    r.mh.silence(H2);
    advance_live(past_declaration()).await;
    advance_live(replacement_timing().min_interval() * 2 + interval()).await;
    assert_eq!(r.gc.calls().len(), 2);
    let times = r.gc.call_times();
    assert!(
        times[1] - times[0] >= replacement_timing().min_interval(),
        "the second replacement waited the minimum interval: {:?}",
        times[1] - times[0]
    );
    assert_eq!(
        migrations(&snap, "heartbeat_timeout", MigrationOutcome::Completed),
        2
    );
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[
            ("trigger", "heartbeat_timeout"),
            ("outcome", "rate_limited"),
            KC,
        ])
        .assert_unobserved();
    assert_one_outcome_per_attempt(&snap, 2, 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// ADR-0039 §3: an initial handler that never registers is replaced
// ----------------------------------------------------------------------------

/// The hand-off's only handler never acks its RegisterMeeting (retries
/// exhausted): the join still succeeds with an empty set; the handler is then
/// removed (apply_lag — NOT a health declaration) and replaced through GC. The
/// set names ONLY the replacement, never the failed handler.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_handoff_handler_that_never_registers_is_removed_and_replaced() {
    let r = rig();
    r.mh.fail_handler(H1);
    r.gc.push(grant(H2));
    let _meeting = r.create("m", &[H1]).await;
    let snap = MetricAssertion::snapshot();
    let mut a = r.join("m", "a").await;
    advance_live(replacement_timing().min_interval() + interval()).await;
    let calls = r.gc.wait_for_calls(1, n()).await;
    assert_eq!(calls[0].trigger, RemovalTrigger::ApplyLag);
    assert_eq!(calls[0].replaced, Some(HandlerId::new(H1)));
    run_ready().await;
    let all = sets(&test_common::drain_frames(&mut a));
    assert_eq!(all.last().map(|s| s.0.clone()), Some(vec![url(H2)]));
    assert!(all.iter().all(|(u, _)| !u.contains(&url(H1))), "{all:?}");
    assert_eq!(
        migrations(&snap, "apply_lag", MigrationOutcome::Completed),
        1
    );
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", "apply_lag"), KC])
        .assert_unobserved();
    assert_one_outcome_per_attempt(&snap, 1, 0);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// The published pacing (the awaiting-handler page bound's source)
// ----------------------------------------------------------------------------

/// The three replacement config echoes are published when the server is
/// built, from the SAME `ReplacementTiming` every meeting's scheduler reads
/// (the shipped ConfigMap values): the minimum interval, the ladder cap and
/// the DERIVED retry budget (ladder + one cap + the registration bound).
#[tokio::test(flavor = "current_thread")]
async fn the_replacement_pacing_and_budget_are_published_at_boot() {
    let boot = MetricAssertion::snapshot();
    let stack = test_common::build_test_stack("mc-c-pacing").await;
    let _rig = test_common::accept_loop_rig::AcceptLoopRig::start(
        Arc::clone(&stack.controller_handle),
        Arc::clone(&stack.jwt_validator),
        Arc::clone(&stack.mh_reg_client) as Arc<dyn mc_service::grpc::MhRegistrationClient>,
    )
    .await;
    let timing = replacement_timing();
    boot.gauge("mc_media_handler_replacement_min_interval_seconds")
        .with_labels(&[KC])
        .assert_value(timing.min_interval().as_secs_f64());
    boot.gauge("mc_media_handler_replacement_backoff_max_seconds")
        .with_labels(&[KC])
        .assert_value(timing.backoff_max().as_secs_f64());
    boot.gauge("mc_media_handler_replacement_retry_budget_seconds")
        .with_labels(&[KC])
        .assert_value(timing.retry_budget().as_secs_f64());
    assert!(
        timing.retry_budget() > timing.acquisition_budget(),
        "the budget includes the registration bound"
    );
}
