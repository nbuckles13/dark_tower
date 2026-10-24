//! On-demand keyframes and receiver recovery at MH (story 3 MH-B; R-4, R-9,
//! R-10, R-11, R-12, R-13, R-30, R-33; story Design §Wire freeze, Amendment 4).
//!
//! One meeting of four real participants (A and E publish video, B and C
//! receive), each joined through GC -> MC and connected to BOTH Kind MH pods.
//! The forwarding policy is INJECTED on both pods through
//! `env_tests::fixtures::injection` acting as MC, at a generation above MC's.
//! Video comes from T-A's keyless `VideoPublisher` with
//! `GroupStartSource::OnKeyframeRequest`, reading MH's `KeyframeRequest`s off
//! its own carrier stream and recording every one, in order with its group
//! starts; it is read by T-A's `VideoSubscriber`. Every phase runs on BOTH
//! pods (each publisher publishes on each of its two connections).
//!
//! Phases, run SEQUENTIALLY inside ONE test fn:
//!
//! (a) NEVER-SENDING: B's video edge on A is installed while A's publisher is
//!     connected but sends nothing. MH counts the edge pending
//!     (`mh_media_edges_pending` >= 1 on each pod: an absolute lower bound —
//!     other suites can only add to it) and A's carrier receives NO keyframe
//!     request (MH has no counter for a source it has never seen). The
//!     absence window ends on that pending EVENT, never a sleep.
//! (b) LATE JOINER (also (a)'s positive control: A's carrier reader was alive
//!     throughout): A starts (its first frame is a stream-start group start,
//!     needing no request); C's edge goes in while A is mid-group. C's first
//!     frame is a group start, and A's log shows a `KeyframeRequest` for
//!     exactly A's video stream at the in-flight counter BEFORE that group
//!     start.
//! (c) RECOVERY: B writes six `RecoveryRequest`s back to back on each pod —
//!     forwarded, duplicate, stale, counter_ahead, source_mismatch,
//!     unmapped_slot — and A's log records exactly ONE request for that
//!     counter, remapped to A's stream (MH's own fields). Each outcome rises
//!     on each pod (secondary, lower bounds).
//! (d) BACKLOG: B's readers are HELD; MH resets B's group past its backlog cap
//!     (`subscriber_backlog`), and B's edge recovers through MH's request: A's
//!     log shows a request after the reset, and a released B reads a group
//!     start.
//! (e) PUBLISHER ABANDONMENT: E resets its own group mid-group and keeps
//!     running (`Fault::AbandonMidGroup`); C sees the propagated reset, then a
//!     group start at the next counter, which E's log records as
//!     `TargetAbandoned` (inside E's minimum interval, through its single
//!     deferral).
//!
//! Across the run, every publisher log records at most ONE request per
//! (stream, counter) — per publisher CONNECTION, i.e. per pod.
//!
//! # Evidence (crates/env-tests/README.md evidence rule)
//!
//! PRIMARY is per entity: each publisher's own ordered log (requests and
//! group starts) and what B's and C's own connections receive. SECONDARY is
//! the pinned pod counter past its settled baseline (a lower bound); the
//! pending gauge is asserted only as an absolute lower bound, never a value
//! or a delta. No wall-clock pass condition anywhere: every bound below is a
//! FAILURE bound, a drain bound or a send cadence; "within min_interval" is
//! proven by the fixture's unit tests, and asserted here by ORDER and CAUSE.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! One test fn; P = 4 participants, each with N = 3 audio slots and at most
//! M = 2 video slots: 4 x (3 + 2) = 20 (planned at most 30).
//!
//! Edge-sum (concurrent Σ P×(N+M)): 20

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::injection::{
    self, assert_mc_programmed_a_carrying_pod, injectable_handlers, install_all, Injection,
};
use env_tests::fixtures::kube::MH_INSTANCES;
use env_tests::fixtures::mc_session::MhSession;
use env_tests::fixtures::media::VIDEO_SEND_ID;
use env_tests::fixtures::metrics::{
    gauge_by_live_pod, group_reset_promql, poll_until_pinned_instance,
};
use env_tests::fixtures::mh_grpc::{self, ReleaseOnDrop};
use env_tests::fixtures::participant::{self, open_sessions, AUDIO};
use env_tests::fixtures::video::backlog::backlog_payload_len;
use env_tests::fixtures::video::{
    recovery_request_frame, CarrierRequests, Fault, GroupStartCause, GroupStartSource,
    KeyframeRequestSource, Next, PublisherEvent, RequestOutcome, StreamEnd, SubscriberEvent,
    VideoPublisher, VideoPublisherConfig, VideoSubscriber,
};
use env_tests::fixtures::{AuthClient, GcClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::SourceDisposition;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The publishers' frame cadence: a real camera's, on tokio time.
const FRAME_INTERVAL: Duration = Duration::from_millis(20);
/// The publishers' minimum keyframe interval (the client's bound). Long
/// enough that a burst of recovery requests is judged before any answer can
/// land; never a pass condition.
const MIN_INTERVAL: Duration = Duration::from_secs(2);
/// Ticks per publisher run slice (the log is published between slices).
const SLICE_TICKS: u32 = 10;
/// Failure bounds only — never pass conditions.
const PHASE_DEADLINE: Duration = Duration::from_secs(90);
/// A subscriber drain bound, never a pass condition.
const QUIET: Duration = Duration::from_millis(500);
const DRAIN_SLICE: Duration = Duration::from_secs(2);
/// Frames per sizing "group" for the backlog row (see `backlog_payload_len`).
const BACKLOG_FRAMES_PER_GROUP: u64 = 100;
/// E abandons after this many frames (well inside `MIN_INTERVAL`, so its
/// self-key goes through the single deferral).
const ABANDON_AFTER_FRAMES: u32 = 25;

/// The injected VIDEO slot ids.
const B_SLOT_A: u32 = 700;
const C_SLOT_A: u32 = 701;
const C_SLOT_E: u32 = 703;
/// A slot id B does not hold: `unmapped_slot`.
const UNMAPPED_SLOT: u32 = 999;

const RECEIVED: &str = "mh_media_recovery_requests_received_total";

fn outcome_promql(outcome: &str) -> String {
    format!(r#"sum by (instance) ({RECEIVED}{{outcome="{outcome}"}})"#)
}

/// One publisher connection, running on its own task: its log is published
/// after every slice so the test reads it while it runs.
struct RunningPublisher {
    log: Arc<Mutex<Vec<PublisherEvent>>>,
    stop: Arc<AtomicBool>,
    /// Hands the session back, so it stays open until CLEANUP.
    task: tokio::task::JoinHandle<MhSession>,
}

impl RunningPublisher {
    fn spawn(mut session: MhSession, cfg: VideoPublisherConfig) -> Self {
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log_w, stop_r) = (Arc::clone(&log), Arc::clone(&stop));
        let task = tokio::spawn(async move {
            let conn = session.connection().clone();
            let mut publisher = VideoPublisher::new(conn, cfg);
            {
                let mut carrier = CarrierRequests::new(&mut session);
                while !stop_r.load(Ordering::Acquire) && !publisher.is_stopped() {
                    publisher.run(&mut carrier, SLICE_TICKS).await;
                    *log_w.lock().expect("log lock") = publisher.events().to_vec();
                }
            }
            publisher.finish().await;
            *log_w.lock().expect("log lock") = publisher.events().to_vec();
            session
        });
        Self { log, stop, task }
    }

    fn log(&self) -> Vec<PublisherEvent> {
        self.log.lock().expect("log lock").clone()
    }

    async fn stop(self) -> (Vec<PublisherEvent>, MhSession) {
        self.stop.store(true, Ordering::Release);
        let session = self.task.await.expect("a publisher task panicked");
        let log = self.log.lock().expect("log lock").clone();
        (log, session)
    }
}

/// Wait until `who`'s log satisfies `pred`; the deadline is a FAILURE.
async fn until_log(
    phase: &str,
    who: &str,
    p: &RunningPublisher,
    pred: impl Fn(&[PublisherEvent]) -> bool,
) {
    let deadline = Instant::now() + PHASE_DEADLINE;
    while !pred(&p.log()) {
        assert!(
            Instant::now() < deadline,
            "KEYFRAME-NOT-OBSERVED: {phase}: {who}'s publisher log never reached the expected \
             state within {PHASE_DEADLINE:?}: {:?}. Check mh_media_keyframe_requests_sent_total \
             and mh_media_edges_pending on both pods.",
            p.log()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn requests(log: &[PublisherEvent]) -> Vec<(usize, u32, u32, RequestOutcome)> {
    log.iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            PublisherEvent::KeyframeRequest {
                stream_number,
                gop_counter,
                outcome,
            } => Some((i, *stream_number, *gop_counter, *outcome)),
            _ => None,
        })
        .collect()
}

fn group_start_at(log: &[PublisherEvent], counter: u32) -> Option<(usize, GroupStartCause)> {
    log.iter().enumerate().find_map(|(i, e)| match e {
        PublisherEvent::GroupStart { gop_counter, cause } if *gop_counter == counter => {
            Some((i, *cause))
        }
        _ => None,
    })
}

/// MH sends at most ONE request per (stream, counter) to one publisher
/// connection, ever.
fn assert_one_request_per_counter(phase: &str, who: &str, log: &[PublisherEvent]) {
    let mut seen = BTreeSet::new();
    for (_, stream, counter, _) in requests(log) {
        assert!(
            seen.insert((stream, counter)),
            "{phase}: {who} received a SECOND KeyframeRequest for one (stream, counter): MH \
             must never repeat a counter (story 3 amendment 4)"
        );
    }
}

async fn drain(sub: &mut VideoSubscriber) -> Vec<SubscriberEvent> {
    sub.drain(QUIET, DRAIN_SLICE).await
}

/// The first frame `sub` reports on slot `slot`: (first in its stream, group
/// start, counter). The deadline is a FAILURE.
async fn first_frame_on(
    phase: &str,
    who: &str,
    sub: &mut VideoSubscriber,
    slot: u32,
) -> (bool, bool, u32) {
    let deadline = Instant::now() + PHASE_DEADLINE;
    loop {
        assert!(
            Instant::now() < deadline,
            "VIDEO-NOT-OBSERVED: {phase}: {who} received no frame on slot {slot} within \
             {PHASE_DEADLINE:?}"
        );
        if let Next::Event(SubscriberEvent::Frame {
            first_in_stream,
            frame,
            ..
        }) = sub.next(QUIET).await
        {
            if u32::from(frame.slot_id) == slot {
                return (
                    first_in_stream,
                    frame.independently_decodable,
                    frame.gop_counter,
                );
            }
        }
    }
}

/// The group the publisher is inside right now, on `slot` (the shared
/// fixture helper at this file's bounds).
async fn mid_group_counter(phase: &str, who: &str, sub: &mut VideoSubscriber, slot: u32) -> u32 {
    sub.mid_group_counter(
        &format!("{phase}: {who}"),
        Some(slot),
        QUIET,
        PHASE_DEADLINE,
    )
    .await
}

#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_on_demand_keyframes_and_recovery() {
    let cluster = ClusterConnection::new()
        .await
        .expect("PRECONDITION: cluster connection (ENV_TEST_* URLs)");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps =
        participant::meeting_of(&auth, &gc, "Video Keyframes 39", &["A", "B", "C", "E"]).await;
    let (x, y) = participant::two_offered_handlers("KEYFRAMES", &ps);
    let (a, b, c, e) = (
        ps[0].sender_id(),
        ps[1].sender_id(),
        ps[2].sender_id(),
        ps[3].sender_id(),
    );
    let mut sessions = open_sessions("KEYFRAMES", &ps, &[&x, &y]).await;

    ps[0].declare(&[11, 12, 13], &[]).await;
    ps[1].declare(&[21, 22, 23], &[]).await;
    ps[2].declare(&[31, 32, 33], &[]).await;
    ps[3].declare(&[41, 42, 43], &[]).await;
    let all = [a, b, c, e];
    let bound = window + participant::VIEW_MARGIN;
    for (i, me) in all.iter().enumerate() {
        let want: BTreeSet<u32> = all.iter().copied().filter(|s| s != me).collect();
        ps[i]
            .converge("KEYFRAMES", bound, |v, d| {
                v.is_some_and(|v| {
                    participant::filled_senders(v, AUDIO)
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        == want
                }) && d.is_some()
            })
            .await;
    }

    let handlers =
        injectable_handlers(&prom, env_tests::fixtures::metrics::BASELINE_SETTLE_BOUND).await;
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let mut injections = Vec::new();
    for handler in &handlers {
        injections.push(Injection::open(handler, &token, &cleanup, &ps[0]).await);
    }
    assert_mc_programmed_a_carrying_pod(&injections);

    // The video edges, each enabled per phase.
    let edges = |c_on_a: bool, c_on_e: bool| {
        let mut v = vec![injection::declared_edge(
            (b << 8) | 1,
            b,
            B_SLOT_A,
            a,
            VIDEO_SEND_ID,
        )];
        if c_on_a {
            v.push(injection::declared_edge(
                (c << 8) | 1,
                c,
                C_SLOT_A,
                a,
                VIDEO_SEND_ID,
            ));
        }
        if c_on_e {
            v.push(injection::declared_edge(
                (c << 8) | 3,
                c,
                C_SLOT_E,
                e,
                VIDEO_SEND_ID,
            ));
        }
        v
    };
    let sources = || {
        [a, e]
            .into_iter()
            .map(|s| {
                injection::declared_source(
                    s,
                    VIDEO_SEND_ID,
                    TransportMode::StreamPerGroup,
                    SourceDisposition::Forward,
                )
            })
            .collect::<Vec<_>>()
    };

    // A's payload is sized so that a HELD subscriber trips the backlog cap
    // (phase d); the same publisher runs every A phase, so its counter never
    // restarts within its binding.
    // Read from EACH live MH pod, never a terminated predecessor's series
    // (which after a roll still carries the OLD cap), and the two must agree:
    // the cap comes from one ConfigMap, so a disagreement is a pod that
    // predates the config — never resolved by taking the larger.
    let backlog_gauge = gauge_by_live_pod(
        &prom,
        "mh_media_egress_stream_backlog_limit_bytes",
        MH_INSTANCES,
        env_tests::fixtures::metrics::PINNED_METRIC_BOUND,
    )
    .await;
    let mut caps: Vec<f64> = backlog_gauge.values().copied().collect();
    caps.sort_by(f64::total_cmp);
    caps.dedup();
    assert!(
        caps.len() == 1,
        "PRECONDITION: the live MH pods publish different backlog caps {backlog_gauge:?}: one \
         predates the config; {}",
        env_tests::fixtures::kube::REDEPLOY_HINT
    );
    let backlog = caps[0];
    assert!(
        backlog > 0.0,
        "PRECONDITION: the deployed backlog cap must be published"
    );
    let backlog = format!("{backlog:.0}")
        .parse::<u64>()
        .expect("PRECONDITION: the published backlog cap is a whole, non-negative byte count");
    let payload_len = backlog_payload_len(backlog, BACKLOG_FRAMES_PER_GROUP);
    let a_cfg = VideoPublisherConfig {
        group_start: GroupStartSource::OnKeyframeRequest {
            min_interval: MIN_INTERVAL,
        },
        frame_interval: FRAME_INTERVAL,
        payload_len,
        ..VideoPublisherConfig::new(a)
    };

    let urls = [&x, &y];
    let conns = |i: usize| -> Vec<wtransport::Connection> {
        urls.iter()
            .map(|u| sessions[i][*u].connection().clone())
            .collect()
    };
    let (b_conns, c_conns) = (conns(1), conns(2));
    let mut b_subs: Vec<VideoSubscriber> = b_conns
        .iter()
        .cloned()
        .map(VideoSubscriber::spawn)
        .collect();
    let mut c_subs: Vec<VideoSubscriber> = c_conns
        .iter()
        .cloned()
        .map(VideoSubscriber::spawn)
        .collect();

    eprintln!("[39] phase (a) NEVER-SENDING: start");
    install_all(&mut injections, 0, edges(false, false), sources()).await;
    // The absence window ends on an EVENT: each pod counts B's edge pending.
    for h in &handlers {
        poll_until_pinned_instance(
            &prom,
            "sum by (instance) (mh_media_edges_pending)",
            &h.pod_ip,
            env_tests::fixtures::metrics::PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            "KEYFRAMES (a) PENDING",
            ">= 1 (B's edge on the never-sending A; an absolute lower bound)",
            |v| v >= 1.0,
        )
        .await;
    }
    for (pod, url) in urls.iter().enumerate() {
        let session = sessions[0].get_mut(*url).expect("A's session");
        let mut carrier = CarrierRequests::new(session);
        assert!(
            tokio::time::timeout(QUIET, carrier.next_request())
                .await
                .is_err(),
            "KEYFRAMES (a): MH sent a keyframe request to A (pod {pod}) for a source it has \
             never seen"
        );
    }

    eprintln!("[39] phase (b) LATE JOINER: start");
    let a_pubs: Vec<RunningPublisher> = urls
        .iter()
        .map(|u| RunningPublisher::spawn(sessions[0].remove(*u).expect("A's session"), a_cfg))
        .collect();
    let mut in_flight = Vec::new();
    for (pod, sub) in b_subs.iter_mut().enumerate() {
        let (first, start, _) = first_frame_on("KEYFRAMES (b)", "B", sub, B_SLOT_A).await;
        assert!(
            first && start,
            "KEYFRAMES (b): B's (pod {pod}) first frame is A's stream-start group start"
        );
        in_flight.push(mid_group_counter("KEYFRAMES (b)", "B", sub, B_SLOT_A).await);
    }
    install_all(&mut injections, 1, edges(true, false), sources()).await;
    for (pod, sub) in c_subs.iter_mut().enumerate() {
        let (first, start, gop) = first_frame_on("KEYFRAMES (b)", "C", sub, C_SLOT_A).await;
        assert!(
            first && start,
            "KEYFRAMES (b): C's (pod {pod}) FIRST frame is a group start — never mid-group"
        );
        assert!(
            gop > in_flight[pod],
            "KEYFRAMES (b): C (pod {pod}) starts at a LATER group than the one in flight"
        );
    }
    for (pod, p) in a_pubs.iter().enumerate() {
        let want = in_flight[pod];
        until_log("KEYFRAMES (b)", "A", p, |log| {
            group_start_at(log, want + 1).is_some()
        })
        .await;
        let log = p.log();
        let (req_at, stream, _, _) = requests(&log)
            .into_iter()
            .find(|(_, _, counter, _)| *counter == want)
            .unwrap_or_else(|| {
                panic!("KEYFRAMES (b): A (pod {pod}) got no request at the in-flight counter")
            });
        assert_eq!(
            stream,
            u32::from(VIDEO_SEND_ID),
            "KEYFRAMES (b): the request names exactly A's video stream"
        );
        let (start_at, cause) = group_start_at(&log, want + 1).expect("checked above");
        assert!(
            req_at < start_at,
            "KEYFRAMES (b): the request (pod {pod}) preceded the group start"
        );
        assert!(
            matches!(
                cause,
                GroupStartCause::Request | GroupStartCause::DeferredRequest
            ),
            "KEYFRAMES (b): the late joiner's group was started BY the request (pod {pod})"
        );
    }

    eprintln!("[39] phase (c) RECOVERY: start");
    let outcomes = [
        "forwarded",
        "duplicate",
        "stale",
        "counter_ahead",
        "source_mismatch",
        "unmapped_slot",
    ];
    // Settled baselines CONCURRENTLY: each settle waits one scrape interval,
    // so six sequential settles were pure wall-clock (the gate is unchanged —
    // each is still a settled per-pod snapshot taken before the burst).
    let q = outcomes.map(outcome_promql);
    let (b0, b1, b2, b3, b4, b5) = tokio::join!(
        injection::baselines(&prom, &q[0]),
        injection::baselines(&prom, &q[1]),
        injection::baselines(&prom, &q[2]),
        injection::baselines(&prom, &q[3]),
        injection::baselines(&prom, &q[4]),
        injection::baselines(&prom, &q[5]),
    );
    let bases = [b0, b1, b2, b3, b4, b5];
    let mut recovered_at = Vec::new();
    for (pod, url) in urls.iter().enumerate() {
        // MH's newest for A on this pod, from the per-entity authority: with
        // OnKeyframeRequest no group starts without a request, so A's LAST
        // group start is the group in flight. (B's own queue may still hold
        // frames of an older group.)
        let log = a_pubs[pod].log();
        let g = log
            .iter()
            .rev()
            .find_map(|e| match e {
                PublisherEvent::GroupStart { gop_counter, .. } => Some(*gop_counter),
                _ => None,
            })
            .expect("KEYFRAMES (c): A has started a group");
        assert!(
            g > 0,
            "KEYFRAMES (c): A is past its first group (stale needs g - 1)"
        );
        assert!(
            !requests(&log)
                .iter()
                .any(|(_, _, counter, _)| *counter == g),
            "KEYFRAMES (c): precondition — A (pod {pod}) holds no request at the counter in \
             flight, so the one after the burst is the burst's"
        );
        // ONE write: MH judges all six back to back, long before any answer
        // could land (the publisher's minimum interval is a further margin).
        let mut burst = Vec::new();
        burst.extend(recovery_request_frame(B_SLOT_A, a, g)); // forwarded
        burst.extend(recovery_request_frame(B_SLOT_A, a, g)); // duplicate
        burst.extend(recovery_request_frame(B_SLOT_A, a, g - 1)); // stale
        burst.extend(recovery_request_frame(B_SLOT_A, a, g + 5)); // counter_ahead
        burst.extend(recovery_request_frame(B_SLOT_A, c, g)); // source_mismatch
        burst.extend(recovery_request_frame(UNMAPPED_SLOT, a, g)); // unmapped_slot
        let session = sessions[1].get_mut(*url).expect("B's session");
        let (send, _) = session.control();
        send.write_all(&burst)
            .await
            .unwrap_or_else(|err| panic!("KEYFRAMES (c): writing recovery requests failed: {err}"));
        recovered_at.push(g);
    }
    for (pod, p) in a_pubs.iter().enumerate() {
        let g = recovered_at[pod];
        until_log("KEYFRAMES (c)", "A", p, |log| {
            requests(log).iter().any(|(_, _, counter, _)| *counter == g)
        })
        .await;
        let at_g: Vec<_> = requests(&p.log())
            .into_iter()
            .filter(|(_, _, counter, _)| *counter == g)
            .collect();
        assert_eq!(
            at_g.len(),
            1,
            "KEYFRAMES (c): A (pod {pod}) records exactly ONE request for the recovered counter"
        );
        assert_eq!(
            at_g[0].1,
            u32::from(VIDEO_SEND_ID),
            "KEYFRAMES (c): remapped to A's stream number, never B's slot"
        );
    }
    // The six secondary rises, polled CONCURRENTLY (each is a failure-bounded
    // poll; sequential polling only added wall-clock).
    let phase = outcomes.map(|o| format!("KEYFRAMES (c) RECOVERY {o} (secondary)"));
    tokio::join!(
        injection::assert_rise(&prom, &handlers, &q[0], &bases[0], 1, &phase[0]),
        injection::assert_rise(&prom, &handlers, &q[1], &bases[1], 1, &phase[1]),
        injection::assert_rise(&prom, &handlers, &q[2], &bases[2], 1, &phase[2]),
        injection::assert_rise(&prom, &handlers, &q[3], &bases[3], 1, &phase[3]),
        injection::assert_rise(&prom, &handlers, &q[4], &bases[4], 1, &phase[4]),
        injection::assert_rise(&prom, &handlers, &q[5], &bases[5], 1, &phase[5]),
    );

    eprintln!("[39] phase (d) BACKLOG: start");
    let reset_base = injection::baselines(&prom, &group_reset_promql("subscriber_backlog")).await;
    // Log positions at the phase start: a request after them is phase (d)'s.
    let log_before: Vec<usize> = a_pubs.iter().map(|p| p.log().len()).collect();
    for sub in &mut b_subs {
        let _ = drain(sub).await;
    }
    drop(b_subs);
    let mut b_held: Vec<VideoSubscriber> = b_conns
        .iter()
        .cloned()
        .map(VideoSubscriber::spawn_held)
        .collect();
    injection::assert_rise(
        &prom,
        &handlers,
        &group_reset_promql("subscriber_backlog"),
        &reset_base,
        1,
        "KEYFRAMES (d) SUBSCRIBER_BACKLOG (secondary)",
    )
    .await;
    // B's edge recovers through MH's request: A receives a NEW request.
    for (pod, p) in a_pubs.iter().enumerate() {
        let before = log_before[pod];
        until_log("KEYFRAMES (d)", "A", p, |log| {
            requests(log).iter().any(|(i, ..)| *i >= before)
        })
        .await;
    }
    for sub in &b_held {
        sub.release();
    }
    // A group B receives after the reset was started BY a phase-(d) request:
    // GroupStart{K} (cause Request / DeferredRequest) in A's log, preceded by
    // a KeyframeRequest logged after the phase began.
    let started_by_phase_request = |log: &[PublisherEvent], k: u32, before: usize| {
        group_start_at(log, k).is_some_and(|(start_at, cause)| {
            matches!(
                cause,
                GroupStartCause::Request | GroupStartCause::DeferredRequest
            ) && requests(log)
                .iter()
                .any(|(i, ..)| *i >= before && *i < start_at)
        })
    };
    for (pod, sub) in b_held.iter_mut().enumerate() {
        let deadline = Instant::now() + PHASE_DEADLINE;
        let mut reset = false;
        // Every first-in-stream group start on B's slot of A, collected as
        // the drain yields it and judged only against a log re-read AFTER the
        // drain (the publisher publishes its log between run slices, so a
        // snapshot taken before a frame may not hold its GroupStart yet).
        let mut candidates = BTreeSet::new();
        loop {
            for ev in drain(sub).await {
                match ev {
                    SubscriberEvent::StreamEnded { report, .. }
                        if report.end == StreamEnd::Reset =>
                    {
                        reset = true;
                    }
                    SubscriberEvent::Frame {
                        first_in_stream: true,
                        frame,
                        ..
                    } if frame.independently_decodable && u32::from(frame.slot_id) == B_SLOT_A => {
                        candidates.insert(frame.gop_counter);
                    }
                    _ => {}
                }
            }
            let log = a_pubs[pod].log();
            let recovered = candidates
                .iter()
                .any(|k| started_by_phase_request(&log, *k, log_before[pod]));
            if reset && recovered {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "VIDEO-NOT-OBSERVED: KEYFRAMES (d): released B (pod {pod}) saw reset={reset}, \
                 a group started by MH's request={recovered}"
            );
        }
    }

    // Sessions stay open until CLEANUP (released before any session closes).
    let mut parked = Vec::new();
    let mut a_logs = Vec::new();
    for p in a_pubs {
        let (log, session) = p.stop().await;
        a_logs.push(log);
        parked.push(session);
    }
    for (pod, log) in a_logs.iter().enumerate() {
        assert_one_request_per_counter("KEYFRAMES (a..d)", &format!("A (pod {pod})"), log);
    }

    eprintln!("[39] phase (e) ABANDONMENT: start");
    for sub in &mut c_subs {
        let _ = drain(sub).await;
    }
    install_all(&mut injections, 2, edges(false, true), sources()).await;
    let e_cfg = VideoPublisherConfig {
        group_start: GroupStartSource::OnKeyframeRequest {
            min_interval: MIN_INTERVAL,
        },
        frame_interval: FRAME_INTERVAL,
        fault: Fault::AbandonMidGroup {
            after_frames: ABANDON_AFTER_FRAMES,
        },
        ..VideoPublisherConfig::new(e)
    };
    let e_pubs: Vec<RunningPublisher> = urls
        .iter()
        .map(|u| RunningPublisher::spawn(sessions[3].remove(*u).expect("E's session"), e_cfg))
        .collect();
    for (pod, sub) in c_subs.iter_mut().enumerate() {
        let deadline = Instant::now() + PHASE_DEADLINE;
        let mut reset_gop = None;
        let mut next_start = None;
        while next_start.is_none() {
            assert!(
                Instant::now() < deadline,
                "VIDEO-NOT-OBSERVED: KEYFRAMES (e): C (pod {pod}) saw the propagated reset={} \
                 and no later group start",
                reset_gop.is_some()
            );
            for ev in drain(sub).await {
                match ev {
                    SubscriberEvent::StreamEnded { report, .. }
                        if report.end == StreamEnd::Reset
                            && report.frames.first().map(|f| u32::from(f.slot_id))
                                == Some(C_SLOT_E) =>
                    {
                        reset_gop = report.frames.first().map(|f| f.gop_counter);
                    }
                    SubscriberEvent::Frame {
                        first_in_stream: true,
                        frame,
                        ..
                    } if reset_gop.is_some() && u32::from(frame.slot_id) == C_SLOT_E => {
                        assert!(
                            frame.independently_decodable,
                            "KEYFRAMES (e): C's (pod {pod}) next E stream opens on a group start"
                        );
                        next_start = Some(frame.gop_counter);
                    }
                    _ => {}
                }
            }
        }
        let reset_gop = reset_gop.expect("the reset came first");
        assert_eq!(
            next_start,
            Some(reset_gop + 1),
            "KEYFRAMES (e): after the propagated reset, C's (pod {pod}) next group is the next \
             counter"
        );
        let p = &e_pubs[pod];
        until_log("KEYFRAMES (e)", "E", p, |log| {
            group_start_at(log, reset_gop + 1).is_some()
        })
        .await;
        assert_eq!(
            group_start_at(&p.log(), reset_gop + 1).map(|(_, cause)| cause),
            Some(GroupStartCause::TargetAbandoned),
            "KEYFRAMES (e): E (pod {pod}) self-keyed the group after its own abandonment"
        );
    }
    for (pod, p) in e_pubs.into_iter().enumerate() {
        let (log, session) = p.stop().await;
        parked.push(session);
        assert_one_request_per_counter("KEYFRAMES (e)", &format!("E (pod {pod})"), &log);
    }

    eprintln!("[39] phase CLEANUP: start");
    // CLEANUP before any session closes, with the release PROVED.
    for injection in injections {
        injection.release_and_prove(&cleanup).await;
    }
    drop(b_held);
    drop(c_subs);
    drop(parked);
    drop(sessions);
}
