//! Planned transfer off a DRAINING handler (ADR-0039 §4/§7; story 4 MC-F: R-3
//! MC side; test-gates "Planned ordering").
//!
//! Tier 1b on the MC-C rig (`common/replacement_rig.rs`): the real
//! coordination service and meeting actors, paused time, a LIVE fake MH whose
//! `drain`/`drain_for` reports DRAINING with a `drain_remaining_ms` counting
//! down to ONE deadline (as MH-3 does), and a scripted GC.
//!
//! Timing is derived, never retyped: the fake's drain deadline is `t0 +
//! window`, every draining beat carries `deadline − now`, so MC's cutover is
//! exactly `t0 + window − margin` (margin from `config.env`).
//!
//! Counting (lead ruling 1): each planned transfer records EXACTLY ONE
//! terminal — `completed`, `timed_out` or `abandoned` — and a planned grant
//! reaching Active records nothing; failed Acquire attempts count per attempt.

#![cfg(feature = "test-seams")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/mod.rs"]
mod test_common;

use std::time::Duration;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use mc_service::actors::MeetingActorHandle;
use mc_service::media_routing::{HandlerId, MigrationOutcome, RemovalTrigger};
use proto_gen::dark_tower::internal::v1::ReleaseCause;
use test_common::replacement_rig::*;
use test_common::{
    advance_live, drain_frames, mh_handler, planned_transfer_timing, run_ready, MhEvent,
    MockMhRegistrationClient,
};

const PT: &str = "planned_transfer";

fn window() -> Duration {
    MockMhRegistrationClient::DEFAULT_DRAIN_WINDOW
}

fn margin() -> Duration {
    planned_transfer_timing().margin()
}

/// From the `drain()` call to MC's cutover.
fn to_cutover() -> Duration {
    window() - margin()
}

/// Enough for the next heartbeat to deliver DRAINING and the turn to settle.
fn one_beat() -> Duration {
    interval() + interval() / 2
}

fn planned(snap: &MetricSnapshot, outcome: MigrationOutcome) -> u64 {
    migrations(snap, PT, outcome)
}

/// THE per-transfer invariant for transfers that CUT OVER: each records
/// exactly one of `completed` / `timed_out` AND exactly one planned
/// declaration, so the two counts are equal (and equal `cut_over`). An
/// abandoned transfer (meeting ended, handler died first) records
/// `abandoned`, which ALSO carries MC-C's attempt-level discards (a
/// superseded or refused Acquire), so it is asserted per test, never summed in.
fn assert_one_terminal_per_transfer(snap: &MetricSnapshot, cut_over: u64) {
    let completed = planned(snap, MigrationOutcome::Completed);
    let timed_out = planned(snap, MigrationOutcome::TimedOut);
    assert_eq!(
        completed + timed_out,
        cut_over,
        "one terminal per cut-over transfer: completed={completed} timed_out={timed_out}"
    );
    assert_eq!(
        declarations(snap, PT),
        cut_over,
        "one planned declaration per cut-over transfer"
    );
}

fn declarations(snap: &MetricSnapshot, trigger: &str) -> u64 {
    snap.counter("mc_media_handler_declarations_total")
        .with_labels(&[("trigger", trigger), KC])
        .delta()
}

/// The R-33 proxy until MC-E lands the loss metric (hand-off recorded in the
/// devloop output): a planned transfer never reaches the Dead/Impaired
/// declaration site, so neither crash trigger is observed. Histograms drain on
/// read, so this reads the histogram FIRST.
fn assert_no_crash_declaration(snap: &MetricSnapshot) {
    for t in ["heartbeat_timeout", "apply_lag"] {
        snap.histogram("mc_media_handler_detection_seconds")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
        snap.counter("mc_media_handler_declarations_total")
            .with_labels(&[("trigger", t), KC])
            .assert_unobserved();
    }
}

async fn health(meeting: &MeetingActorHandle, h: &str) -> Option<&'static str> {
    meeting.probe_handler_for_test(h).await.unwrap().health
}

/// The url lists of every set on `q` since the last read.
fn url_sets(q: &mut tokio::sync::mpsc::Receiver<bytes::Bytes>) -> Vec<Vec<String>> {
    sets(&drain_frames(q)).into_iter().map(|(u, _)| u).collect()
}

fn ends_for(r: &Rig, h: &str) -> Vec<mc_service::media_routing::teardown::EndMeetingCall> {
    r.mh.end_calls()
        .into_iter()
        .filter(|c| c.handler_id == h)
        .collect()
}

// ----------------------------------------------------------------------------
// Ordering (test-gates "Planned ordering")
// ----------------------------------------------------------------------------

/// {X} with A and B on X; X drains; GC grants Y. Observed order:
/// RegisterMeeting(Y) → client set {X, Y} (with a token for Y) → A and B
/// MH-reported Connected on Y → client set {Y} → X's worker drained (no
/// RegisterMeeting to X after) → EndMeeting(X, PLANNED_TRANSFER). While
/// DRAINING and before everyone is on Y, X stays a member and keeps the
/// edges. The transfer is `completed`; no crash declaration is observed.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn make_before_break_adds_y_waits_for_connected_then_drops_x_then_ends_it() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let mut a = r.join("m", "a").await;
    let mut b = r.join("m", "b").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    assert_ne!(r.connect("m", "b", H1, "b1").await, 0);
    advance_live(settle()).await;
    drain_frames(&mut a);
    drain_frames(&mut b);
    let snap = MetricAssertion::snapshot();

    r.mh.drain(H1);
    advance_live(one_beat()).await;
    let calls = r.gc.wait_for_calls(1, n()).await;
    assert_eq!(calls[0].trigger, RemovalTrigger::PlannedTransfer);
    assert_eq!(calls[0].replaced, Some(HandlerId::new(H1)));
    run_ready().await;
    assert!(r.registers_to(H2) > 0, "RegisterMeeting(Y) issued");
    assert_eq!(health(&meeting, H1).await, Some("draining"), "X kept");
    for q in [&mut a, &mut b] {
        let s = sets(&drain_frames(q));
        let last = s.last().expect("the set adding Y");
        assert_eq!(last.0, vec![url(H1), url(H2)], "{{old, new}}");
        assert!(last.1.contains(&H2.to_string()), "a token for Y");
        assert!(
            s.iter().all(|(u, _)| u.contains(&url(H1))),
            "no set without X yet: {s:?}"
        );
    }
    let last_to_x = |r: &Rig| {
        r.mh.calls()
            .into_iter()
            .rfind(|c| c.expected_handler_id == H1)
            .map_or(0, |c| c.egress_stream_count)
    };
    assert_eq!(last_to_x(&r), 2, "X still carries A↔B while draining");

    // Only A on Y: not covered; X stays.
    r.mh.hold_ends();
    assert_ne!(r.connect("m", "a", H2, "a2").await, 0);
    advance_live(one_beat()).await;
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "B is not on Y"
    );
    assert!(url_sets(&mut a).iter().all(|u| u.contains(&url(H1))));

    // B on Y: covered → the set without X goes out FIRST. Paired-test T1:
    // step the scheduler ONE yield at a time and sample both sides after
    // each step; the first sample that shows EndMeeting(X) recorded must
    // already show the set {Y} on BOTH clients' queues (a swapped order —
    // EndMeeting before the set — fails here; verified by swapping the
    // transaction's steps once).
    assert_ne!(r.connect("m", "b", H2, "b2").await, 0);
    let mut seen = [Vec::new(), Vec::new()];
    for _ in 0..2_000 {
        tokio::task::yield_now().await;
        for (i, q) in [&mut a, &mut b].into_iter().enumerate() {
            seen[i].extend(url_sets(q));
        }
        if !ends_for(&r, H1).is_empty() {
            for (i, s) in seen.iter().enumerate() {
                assert_eq!(
                    s.last(),
                    Some(&vec![url(H2)]),
                    "client {i} had the set without X before EndMeeting(X): {s:?}"
                );
            }
            break;
        }
    }
    assert!(!ends_for(&r, H1).is_empty(), "EndMeeting(X) was issued");
    assert_eq!(health(&meeting, H1).await, None, "X removed");
    let ends = r.mh.wait_for_end_calls(1, n()).await;
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].handler_id, H1);
    assert_eq!(ends[0].cause, ReleaseCause::PlannedTransfer);
    let events = r.mh.events();
    let end_at = events
        .iter()
        .position(|e| matches!(e, MhEvent::End(c) if c.handler_id == H1))
        .unwrap();
    let x_endpoint = mh_handler(H1).grpc_endpoint;
    assert!(
        !events[end_at..].iter().any(
            |e| matches!(e, MhEvent::Register { mh_grpc_endpoint, .. } if *mh_grpc_endpoint == x_endpoint)
        ),
        "X's worker was drained before EndMeeting: no RegisterMeeting to X after it"
    );
    let y_endpoint = mh_handler(H2).grpc_endpoint;
    let first_y = events
        .iter()
        .position(|e| matches!(e, MhEvent::Register { mh_grpc_endpoint, .. } if *mh_grpc_endpoint == y_endpoint))
        .unwrap();
    assert!(first_y < end_at, "add Y before EndMeeting(X)");
    r.mh.release_ends();
    run_ready().await;

    assert_no_crash_declaration(&snap);
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "timed_out"), KC])
        .assert_unobserved();
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    assert_eq!(declarations(&snap, PT), 1);
    snap.counter("mc_media_handler_removal_releases_total")
        .with_labels(&[("trigger", PT), ("outcome", "released"), KC])
        .assert_delta(1);
    assert_eq!(r.gc.calls().len(), 1, "one Acquire");
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Cutover at drain_remaining − margin
// ----------------------------------------------------------------------------

/// {X}; Y granted; A moves to Y, B never does. At cutover − 1 ms X is still a
/// member and no set without X was sent; at the cutover X is removed (B cut
/// over) and the transfer is `timed_out` — never `completed`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_straggler_is_cut_over_at_the_deadline_and_counted_timed_out() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let mut a = r.join("m", "a").await;
    let _b = r.join("m", "b").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    assert_ne!(r.connect("m", "b", H1, "b1").await, 0);
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();

    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_ne!(r.connect("m", "a", H2, "a2").await, 0);
    advance_live(to_cutover() - one_beat() - Duration::from_millis(1)).await;
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "cutover − 1 ms"
    );
    assert!(url_sets(&mut a).iter().all(|u| u.contains(&url(H1))));

    advance_live(Duration::from_millis(1)).await;
    assert_eq!(health(&meeting, H1).await, None, "removed at the cutover");
    assert_eq!(url_sets(&mut a).last(), Some(&vec![url(H2)]));
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "completed"), KC])
        .assert_unobserved();
    assert_eq!(planned(&snap, MigrationOutcome::TimedOut), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    assert_eq!(declarations(&snap, PT), 1);
    r.c.cancel();
}

/// remaining ≤ margin (here 0: a late registration or delayed beat) means an
/// immediate cutover at the first draining beat — no underflow, no wait. The
/// participant is already on S, so the deadline cutover is `completed`: the
/// outcome is the observed coverage, never "the deadline fired".
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn zero_remaining_cuts_over_at_the_first_draining_beat() {
    let r = rig();
    let meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    assert_ne!(r.connect("m", "a", H2, "a2").await, 0);
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain_for(H1, Duration::ZERO);
    advance_live(one_beat()).await;
    assert_eq!(health(&meeting, H1).await, None, "cut over at once");
    assert_eq!(health(&meeting, H2).await, Some("healthy"), "onto S");
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "timed_out"), KC])
        .assert_unobserved();
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// No grant: survivors are the target; shed only with neither
// ----------------------------------------------------------------------------

/// Arm (a): everyone already on survivor S and GC has no capacity → X is
/// removed as soon as the "no grant" answer arrives, `completed`.
/// Positive twin of the grant rule: with a grant on its way, the same
/// meeting WAITS for Y even though S covers everyone.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn all_on_a_survivor_completes_on_no_grant_but_waits_for_a_coming_grant() {
    // (a) both no-grant answers: RESOURCE_EXHAUSTED (the rig's default),
    // then UNAVAILABLE.
    for (no_grant, outcome) in [
        (None, MigrationOutcome::NoCapacity),
        (Some(unavailable()), MigrationOutcome::GcUnavailable),
    ] {
        let r = rig();
        if let Some(answer) = no_grant {
            r.gc.set_default(answer);
        }
        let meeting = r.create("m", &[H1, H2]).await;
        let _a = r.join("m", "a").await;
        for (h, k) in [(H1, "a1"), (H2, "a2")] {
            assert_ne!(r.connect("m", "a", h, k).await, 0);
        }
        advance_live(settle()).await;
        let snap = MetricAssertion::snapshot();
        r.mh.drain(H1);
        advance_live(one_beat()).await;
        assert!(!r.gc.calls().is_empty());
        assert_eq!(health(&meeting, H1).await, None, "removed onto S");
        // Below its hand-off size, the meeting keeps acquiring (size restore).
        assert!(planned(&snap, outcome) >= 1);
        assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
        assert_one_terminal_per_transfer(&snap, 1);
        r.c.cancel();
    }

    // Twin: a grant whose registration is still in flight → X stays.
    let r = rig();
    r.gc.push(grant(H3));
    r.mh.hold_handler(H3);
    let meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    for (h, k) in [(H1, "a1"), (H2, "a2")] {
        assert_ne!(r.connect("m", "a", h, k).await, 0);
    }
    advance_live(settle()).await;
    r.mh.drain(H1);
    advance_live(one_beat() * 3).await;
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "S covers everyone, but a grant is coming: make-before-break on Y"
    );
    r.mh.release_handler(H3);
    run_ready().await;
    assert_ne!(r.connect("m", "a", H3, "a3").await, 0);
    run_ready().await;
    assert_eq!(health(&meeting, H1).await, None, "removed once on Y");
    r.c.cancel();
}

/// Code-review F3: an UNUSABLE grant (GC grants a handler this meeting
/// excludes — here the survivor S itself, a GC contract violation) is a "no
/// grant" answer like no capacity: survivors become eligible at once, so with
/// everyone on S, X is removed `completed` well before the deadline.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_unusable_grant_counts_as_no_grant_and_unlocks_the_survivor() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    for (h, k) in [(H1, "a1"), (H2, "a2")] {
        assert_ne!(r.connect("m", "a", h, k).await, 0);
    }
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_eq!(r.gc.calls()[0].replaced, Some(HandlerId::new(H1)));
    assert!(
        planned(&snap, MigrationOutcome::Abandoned) >= 1,
        "excluded_grant"
    );
    assert_eq!(health(&meeting, H1).await, None, "removed onto S at once");
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    r.c.cancel();
}

/// Arm (b): P only on X, S present, no grant (both no-grant answers) → X
/// stays until P is Connected on S, then is removed `completed`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_participant_only_on_x_is_waited_for_on_the_survivor() {
    for no_grant in [None, Some(unavailable())] {
        let r = rig();
        if let Some(answer) = no_grant.clone() {
            r.gc.set_default(answer);
        }
        let meeting = r.create("m", &[H1, H2]).await;
        let _p = r.join("m", "p").await;
        assert_ne!(r.connect("m", "p", H1, "p1").await, 0);
        advance_live(settle()).await;
        let snap = MetricAssertion::snapshot();
        r.mh.drain(H1);
        advance_live(one_beat() * 2).await;
        assert!(!r.gc.calls().is_empty());
        assert_eq!(health(&meeting, H1).await, Some("draining"), "P not on S");
        assert_ne!(r.connect("m", "p", H2, "p2").await, 0);
        run_ready().await;
        assert_eq!(health(&meeting, H1).await, None, "P on S: removed");
        assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
        assert_one_terminal_per_transfer(&snap, 1);
        r.c.cancel();
    }
}

/// Arm (c): no survivor and no grant (RESOURCE_EXHAUSTED, then UNAVAILABLE)
/// → X keeps serving until the cutover, then the meeting is shed: the empty
/// set goes to clients, EndMeeting(X, PLANNED_TRANSFER), `timed_out`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn with_neither_a_survivor_nor_a_grant_the_meeting_is_shed_at_the_cutover() {
    for no_grant in [None, Some(unavailable())] {
        let r = rig();
        if let Some(answer) = no_grant.clone() {
            r.gc.set_default(answer);
        }
        let meeting = r.create("m", &[H1]).await;
        let mut a = r.join("m", "a").await;
        assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
        advance_live(settle()).await;
        drain_frames(&mut a);
        let snap = MetricAssertion::snapshot();
        r.mh.drain(H1);
        advance_live(to_cutover() - Duration::from_millis(1)).await;
        assert_eq!(
            health(&meeting, H1).await,
            Some("draining"),
            "still serving"
        );
        advance_live(Duration::from_millis(1)).await;
        assert_eq!(health(&meeting, H1).await, None);
        assert_eq!(
            url_sets(&mut a).last(),
            Some(&vec![]),
            "shed: the empty set"
        );
        let ends = r.mh.wait_for_end_calls(1, n()).await;
        assert_eq!(ends[0].cause, ReleaseCause::PlannedTransfer);
        assert_eq!(planned(&snap, MigrationOutcome::TimedOut), 1);
        assert_one_terminal_per_transfer(&snap, 1);
        r.c.cancel();
    }
}

/// Lead ruling 2 / paired-test: the deadline arrives with the planned Acquire
/// still unanswered and survivor S present → X is removed ONTO S, never shed.
/// Everyone was on S, so the outcome is `completed` with `timed_out`
/// unobserved (observability ruling on code-review F6 / operations F1: a slow
/// GC during a rollout must not read as a drain shed). Paired with the
/// straggler and shed arms, which stay `timed_out`.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_unanswered_acquire_at_the_deadline_cuts_over_onto_the_survivor() {
    let r = rig();
    r.gc.hold();
    let meeting = r.create("m", &[H1, H2]).await;
    let mut a = r.join("m", "a").await;
    for (h, k) in [(H1, "a1"), (H2, "a2")] {
        assert_ne!(r.connect("m", "a", h, k).await, 0);
    }
    advance_live(settle()).await;
    drain_frames(&mut a);
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_eq!(r.gc.calls().len(), 1, "in flight");
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "before the deadline the target is the (pending) grant only"
    );
    advance_live(to_cutover()).await;
    assert_eq!(health(&meeting, H1).await, None);
    assert_eq!(
        url_sets(&mut a).last(),
        Some(&vec![url(H2)]),
        "onto S, not shed"
    );
    snap.gauge("mc_media_meetings_without_handlers")
        .with_labels(&[KC])
        .assert_value(0.0);
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "timed_out"), KC])
        .assert_unobserved();
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    r.gc.release();
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// Robustness: best-effort EndMeeting, fence, idempotence, crash interplay
// ----------------------------------------------------------------------------

/// EndMeeting(X) fails: the removal still completes, there is ONE EndMeeting
/// (no retry loop), X's next DRAINING heartbeat is RELEASED (the fence that
/// lets MH exit early), and a late Connected from X is refused (sender 0).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_failed_end_meeting_still_removes_and_the_next_beat_is_released() {
    let r = rig();
    r.mh.fail_ends();
    let meeting = r.create("m", &[H1, H2]).await;
    let _a = r.join("m", "a").await;
    for (h, k) in [(H1, "a1"), (H2, "a2")] {
        assert_ne!(r.connect("m", "a", h, k).await, 0);
    }
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_eq!(health(&meeting, H1).await, None);
    advance_live(interval() * 4).await;
    assert_eq!(ends_for(&r, H1).len(), 1, "best-effort: one EndMeeting");
    snap.counter("mc_media_handler_heartbeats_total")
        .with_labels(&[("outcome", "released_removed"), KC])
        .assert_delta(1);
    assert_eq!(
        r.connect("m", "a", H1, "late").await,
        0,
        "the binding gate refuses the removed handler"
    );
    // Security F-1 (S-3 negative): the removed handler's DRAINING beat starts
    // NO new transfer — past any deadline one could have had, still exactly
    // the one cut-over transfer and no spurious `abandoned(not_a_member)`.
    advance_live(to_cutover() + one_beat()).await;
    assert_one_terminal_per_transfer(&snap, 1);
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "abandoned"), KC])
        .assert_unobserved();
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    r.c.cancel();
}

/// Repeated DRAINING beats and a SERVING flip-back start no second transfer
/// and no second Acquire; the transfer runs to completion.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn repeated_draining_and_a_flip_back_neither_restart_nor_abort() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let _a = r.join("m", "a").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(interval() * 4).await;
    r.mh.undrain(H1);
    advance_live(interval() * 3).await;
    assert_eq!(r.gc.calls().len(), 1, "one transfer, one Acquire");
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "a SERVING beat (drain_remaining_ms 0) neither aborts nor pulls the cutover to now"
    );
    assert_ne!(r.connect("m", "a", H2, "a2").await, 0);
    run_ready().await;
    assert_eq!(health(&meeting, H1).await, None, "continued to completion");
    assert_eq!(planned(&snap, MigrationOutcome::Completed), 1);
    assert_one_terminal_per_transfer(&snap, 1);
    r.c.cancel();
}

/// X goes silent (Dead at N) mid-transfer, with its grant Y Active: the crash
/// path removes X (a heartbeat_timeout declaration — the positive arm of the
/// "no crash declaration" proxy), the transfer ends `abandoned` and never
/// also `completed`/`timed_out`, there is no second Acquire, and the elapsed
/// deadline does not re-fire.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_draining_handler_dying_mid_transfer_is_abandoned_once() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let _a = r.join("m", "a").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_eq!(health(&meeting, H2).await, Some("healthy"), "Y Active");
    r.mh.silence(H1);
    advance_live(past_declaration()).await;
    assert_eq!(health(&meeting, H1).await, None);
    assert_eq!(declarations(&snap, "heartbeat_timeout"), 1, "crash path");
    advance_live(to_cutover()).await;
    assert_eq!(r.gc.calls().len(), 1, "no second Acquire");
    assert_eq!(planned(&snap, MigrationOutcome::Abandoned), 1);
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "completed"), KC])
        .assert_unobserved();
    snap.counter("mc_media_handler_migrations_total")
        .with_labels(&[("trigger", PT), ("outcome", "timed_out"), KC])
        .assert_unobserved();
    assert_one_terminal_per_transfer(&snap, 0);
    r.c.cancel();
}

/// Mixed scenario: the grant Y itself starts draining mid-transfer (two MHs
/// drained back to back) while Y's own Acquire is still IN FLIGHT. Y leaves
/// X's targets at once; X is cut over at its deadline (no non-draining target:
/// timed_out), which changes the set version, so Y's held Acquire is
/// SUPERSEDED when GC answers — an attempt-level `abandoned`, NOT a transfer
/// terminal. Pins the exact invariant through that discard path:
/// completed + timed_out == planned declarations (2), abandoned == 1.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_draining_grant_is_no_target_and_a_superseded_acquire_is_no_terminal() {
    let r = rig();
    r.gc.push(grant(H2));
    let meeting = r.create("m", &[H1]).await;
    let _a = r.join("m", "a").await;
    assert_ne!(r.connect("m", "a", H1, "a1").await, 0);
    advance_live(settle()).await;
    let snap = MetricAssertion::snapshot();
    r.mh.drain(H1);
    advance_live(one_beat()).await;
    assert_eq!(health(&meeting, H2).await, Some("healthy"));
    r.gc.hold();
    r.mh.drain(H2);
    advance_live(one_beat()).await;
    assert_eq!(r.gc.calls().len(), 2, "Y's own Acquire is in flight");
    assert_eq!(r.gc.calls()[1].replaced, Some(HandlerId::new(H2)));
    assert_ne!(r.connect("m", "a", H2, "a2").await, 0);
    run_ready().await;
    assert_eq!(
        health(&meeting, H1).await,
        Some("draining"),
        "a draining grant is no target"
    );
    advance_live(to_cutover()).await;
    assert_eq!(
        health(&meeting, H1).await,
        None,
        "X cut over at its deadline"
    );
    r.gc.release();
    run_ready().await;
    assert_eq!(planned(&snap, MigrationOutcome::Abandoned), 1, "superseded");
    advance_live(one_beat() * 2).await;
    assert_eq!(
        health(&meeting, H2).await,
        None,
        "Y cut over at its deadline"
    );
    assert_eq!(planned(&snap, MigrationOutcome::TimedOut), 2);
    assert_one_terminal_per_transfer(&snap, 2);
    r.c.cancel();
}

// ----------------------------------------------------------------------------
// The published margin (config echo)
// ----------------------------------------------------------------------------

/// The margin echo is published when the server is built, from the SAME
/// `PlannedTransferTiming` every meeting's cutover reads (the shipped
/// ConfigMap value), in seconds with `key_custody`.
#[tokio::test(flavor = "current_thread")]
async fn the_planned_transfer_margin_is_published_at_boot() {
    let boot = MetricAssertion::snapshot();
    let stack = test_common::build_test_stack("mc-f-margin").await;
    let _rig = test_common::accept_loop_rig::AcceptLoopRig::start(
        std::sync::Arc::clone(&stack.controller_handle),
        std::sync::Arc::clone(&stack.jwt_validator),
        std::sync::Arc::clone(&stack.mh_reg_client)
            as std::sync::Arc<dyn mc_service::grpc::MhRegistrationClient>,
    )
    .await;
    boot.gauge("mc_media_handler_planned_transfer_margin_seconds")
        .with_labels(&[KC])
        .assert_value(planned_transfer_timing().margin().as_secs_f64());
}
