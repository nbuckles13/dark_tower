//! Stream-per-group VIDEO forwarding at MH (story 3 MH-A2; R-10, R-12, R-16,
//! R-17, R-20, R-21, R-33).
//!
//! One meeting of three real participants (A publishes, B and C receive),
//! each joined through GC -> MC and connected to BOTH Kind MH pods. The
//! forwarding policy is INJECTED on both pods through
//! `env_tests::fixtures::injection` acting as MC, at a generation above MC's:
//! A declares audio (send id 0, DATAGRAM) and video (send id 1,
//! STREAM_PER_GROUP); B and C declare audio. B and C hold MC's own audio
//! slots plus one injected VIDEO slot of A. Video comes from T-A's keyless
//! `VideoPublisher` (one uni stream per group, group starts from the
//! fixture-only `GroupStartSource::Periodic`, never a production keyframe
//! interval) and is read by T-A's `VideoSubscriber`; audio from
//! `fixtures/media.rs`. Every phase runs on BOTH pods (A publishes on each of
//! its two connections), so the pinned counters rise on each pod.
//!
//! Phases, run SEQUENTIALLY inside ONE test fn:
//!
//! (a) MIXED: audio and video forward each to its own kind's slots only;
//! (b) LATE SUBSCRIBER: C's video edge is installed while A is mid-group — C's
//!     first frame on every stream is a group start (never mid-group);
//! (c) SERVER VIDEO MUTE: A's video DROP_AT_INGRESS, the patched publisher
//!     keeps sending — no video reaches any receiver while A's audio forwards;
//! (d) TRANSPORT MISMATCH: A labels its uni streams with the AUDIO (DATAGRAM)
//!     send id — every stream is refused (`transport_mismatch`), nothing lands;
//! (e) PAUSED: A's video PAUSED (client mute) still forwards, counted as
//!     paused-source arrivals;
//! (f) BACKLOG: B's readers are HELD; MH resets B's group past its backlog cap
//!     (`subscriber_backlog`) and B resumes at a LATER group once released,
//!     while C receives every group, contiguous, finished, never reset — and
//!     B's audio keeps arriving while its video is held.
//!
//! # Evidence (crates/env-tests/README.md evidence rule)
//!
//! PRIMARY is per entity: what B's and C's OWN connections receive — each
//! downlink stream's frames (slot, key-id sender, group start, GoP) and how it
//! ended (FIN / RESET). A video absence (c, d) is concluded only AFTER the
//! pinned drop counter on each pod has risen by at least the frames/streams A
//! sent there, which proves every one was ingested and dropped — never on a
//! timeout. SECONDARY is the pinned pod counter past its settled baseline (a
//! lower bound). The story-2 audio suites are untouched by this file.
//!
//! No wall-clock pass condition anywhere: every bound below is a FAILURE
//! bound or a send cadence.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! One test fn; P = 3 participants, each with N = 2 audio slots and at most
//! M = 1 video slot: 3 x (2 + 1) = 9 (planned at most 30).
//!
//! Edge-sum (concurrent Σ P×(N+M)): 9

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::injection::{
    self, assert_mc_programmed_a_carrying_pod, injectable_handlers, install_all, Injection,
};
use env_tests::fixtures::media::{audio_datagram, AUDIO_SEND_ID, VIDEO_SEND_ID};
use env_tests::fixtures::metrics::{
    gauge_by_instance_present, group_reset_promql, paused_promql, reason_promql,
};
use env_tests::fixtures::mh_grpc::{self, ReleaseOnDrop};
use env_tests::fixtures::participant::{self, open_sessions};
use env_tests::fixtures::video::{
    GroupStartSource, NoRequests, PublisherEvent, PublisherMode, ReceivedStream, StreamEnd,
    SubscriberEvent, UplinkLabel, VideoPublisher, VideoPublisherConfig, VideoSubscriber,
    VideoTransport,
};
use env_tests::fixtures::{AuthClient, GcClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::SourceDisposition;
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MARK_A: u8 = 0xA8;

/// The publisher's frame cadence: a real camera's, on tokio time.
const FRAME_INTERVAL: Duration = Duration::from_millis(20);
/// The fixture-only group cadence (NOT a production keyframe interval).
const GROUP_INTERVAL: Duration = Duration::from_millis(400);
/// Ticks per publishing phase: five groups.
const PHASE_TICKS: u32 = 100;
/// Failure bounds only — never pass conditions.
const PHASE_DEADLINE: Duration = Duration::from_secs(60);
/// How long a subscriber's event channel must stay empty for a drain to end.
/// A drain BOUND, never a pass condition: every absence below is ordered by a
/// pinned counter first.
const QUIET: Duration = Duration::from_millis(500);
/// The most one [`drain`] call reads before returning to its caller's
/// deadline check (a drain bound, never a pass condition).
const DRAIN_SLICE: Duration = Duration::from_secs(2);

/// The injected VIDEO slot ids B and C hold for A.
const B_VIDEO_SLOT: u32 = 700;
const C_VIDEO_SLOT: u32 = 701;
const SEQ_A: u32 = 1_400_000;
const _: () = assert!(SEQ_A >= env_tests::fixtures::media::AUDIO_SEQUENCE_CEILING);

/// Group cadence for the backlog row and the late-subscriber row: long groups,
/// so "mid-group" is wide and each backlog group carries the sized byte count
/// at a modest rate.
const BACKLOG_GROUP_INTERVAL: Duration = Duration::from_secs(2);
const BACKLOG_FRAMES_PER_GROUP: u64 = 100;

/// Every subscriber drain in this file: the shared fixture drain at this
/// file's bounds.
async fn drain(sub: &mut VideoSubscriber) -> Vec<SubscriberEvent> {
    sub.drain(QUIET, DRAIN_SLICE).await
}

fn ended(events: &[SubscriberEvent]) -> Vec<ReceivedStream> {
    events
        .iter()
        .filter_map(|e| match e {
            SubscriberEvent::StreamEnded { report, .. } => Some(report.clone()),
            _ => None,
        })
        .collect()
}

/// Wait until `sub` has reported at least `n` ended streams, accumulating
/// every event; the deadline is a FAILURE.
async fn until_streams(
    phase: &str,
    who: &str,
    sub: &mut VideoSubscriber,
    seen: &mut Vec<SubscriberEvent>,
    n: usize,
) {
    let deadline = Instant::now() + PHASE_DEADLINE;
    while ended(seen).len() < n {
        assert!(
            Instant::now() < deadline,
            "VIDEO-NOT-OBSERVED: {phase}: {who} reported {} ended video streams within \
             {PHASE_DEADLINE:?}, expected at least {n}. Check mh_media_group_starts_total and \
             mh_media_frames_dropped_total on both pods.",
            ended(seen).len()
        );
        seen.extend(drain(sub).await);
    }
}

/// Ticks per backlog-row group.
fn group_ticks() -> u32 {
    u32::try_from(BACKLOG_FRAMES_PER_GROUP).expect("a small frame count")
}

/// The group the publisher is inside right now, read off `sub` (the shared
/// fixture helper at this file's bounds).
async fn until_mid_group_frame(phase: &str, who: &str, sub: &mut VideoSubscriber) -> u32 {
    sub.mid_group_counter(&format!("{phase}: {who}"), None, QUIET, PHASE_DEADLINE)
        .await
}

/// Every stream a receiver got for A carries A's frames on `slot` only, opens
/// on a group start, and carries one slot.
fn assert_clean_streams(phase: &str, who: &str, streams: &[ReceivedStream], a: u32, slot: u32) {
    for s in streams {
        assert_eq!(
            s.first_frame_group_start,
            s.frames.first().map(|_| true),
            "{phase}: {who} got a stream that did not open on a group start (never mid-group)"
        );
        assert!(
            !s.slot_changed_within_stream,
            "{phase}: one downlink stream, one slot"
        );
        for f in &s.frames {
            assert_eq!(
                u32::from(f.slot_id),
                slot,
                "{phase}: {who}: video only on its video slot"
            );
            assert_eq!(u32::from(f.key_id_sender), a, "{phase}: {who}: A's video");
        }
    }
}

/// Run one A publisher per connection concurrently for `ticks`, then FIN.
async fn publish(
    a: u32,
    conns: &[wtransport::Connection],
    cfg: impl Fn(VideoPublisherConfig) -> VideoPublisherConfig,
    ticks: u32,
) -> Vec<VideoPublisher<wtransport::Connection>> {
    let mut tasks = Vec::new();
    for conn in conns {
        let config = cfg(VideoPublisherConfig {
            group_start: GroupStartSource::Periodic(GROUP_INTERVAL),
            frame_interval: FRAME_INTERVAL,
            ..VideoPublisherConfig::new(a)
        });
        let conn = conn.clone();
        tasks.push(tokio::spawn(async move {
            let mut publisher = VideoPublisher::new(conn, config);
            publisher.run(&mut NoRequests, ticks).await;
            publisher.finish().await;
            publisher
        }));
    }
    let mut out = Vec::new();
    for t in tasks {
        out.push(t.await.expect("a publisher task panicked"));
    }
    out
}

/// A's audio, paced, on every A connection; returns the last sequence.
async fn send_a_audio(conns: &[wtransport::Connection], floor: u32, n: u32) -> u32 {
    let mut tick = tokio::time::interval(FRAME_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    for i in 1..=n {
        tick.tick().await;
        for conn in conns {
            conn.send_datagram(audio_datagram(floor + i, MARK_A))
                .unwrap_or_else(|e| panic!("A could not send audio: {e}"));
        }
    }
    floor + n
}

/// A's audio datagrams `conn` received above `floor`, on which slots.
async fn a_audio_slots(conn: &wtransport::Connection, floor: u32) -> (usize, BTreeSet<u32>) {
    let mut n = 0;
    let mut slots = BTreeSet::new();
    while let Ok(Ok(d)) =
        tokio::time::timeout(Duration::from_millis(50), conn.receive_datagram()).await
    {
        let payload = d.payload();
        let view = media_protocol::codec::decode_datagram(&payload)
            .unwrap_or_else(|_| panic!("a receiver got a malformed datagram"));
        if view.payload().first() == Some(&MARK_A) && view.stream_sequence() > floor {
            n += 1;
            slots.insert(u32::from(view.stream_id()));
        }
    }
    (n, slots)
}

#[tokio::test]
#[serial_test::serial(mh_notifications)]
#[allow(clippy::too_many_lines)]
async fn test_stream_per_group_video_forwarding() {
    let cluster = ClusterConnection::new()
        .await
        .expect("PRECONDITION: cluster connection (ENV_TEST_* URLs)");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps = participant::meeting_of(&auth, &gc, "Video Forwarding 38", &["A", "B", "C"]).await;
    let (x, y) = participant::two_offered_handlers("VIDEO", &ps);
    let (a, b, c) = (ps[0].sender_id(), ps[1].sender_id(), ps[2].sender_id());
    let sessions = open_sessions("VIDEO", &ps, &[&x, &y]).await;

    ps[0].declare(&[11, 12]).await;
    ps[1].declare(&[21, 22]).await;
    ps[2].declare(&[31, 32]).await;
    let bound = window + participant::VIEW_MARGIN;
    for (i, others) in [(0, [b, c]), (1, [a, c]), (2, [a, b])] {
        let want: BTreeSet<u32> = others.into_iter().collect();
        ps[i]
            .converge("VIDEO", bound, |v, d| {
                v.is_some_and(|v| {
                    participant::filled_senders(v)
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        == want
                }) && d.is_some()
            })
            .await;
    }
    let slot = |i: usize, from: u32| ps[i].slot_for(from);

    let handlers =
        injectable_handlers(&prom, env_tests::fixtures::metrics::BASELINE_SETTLE_BOUND).await;
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let mut injections = Vec::new();
    for handler in &handlers {
        injections.push(Injection::open(handler, &token, &cleanup, &ps[0]).await);
    }
    assert_mc_programmed_a_carrying_pod(&injections);

    let edges = |with_c_video: bool| {
        let mut e = Vec::new();
        for (i, subscriber) in [(0_usize, a), (1, b), (2, c)] {
            for (ordinal, from) in [a, b, c]
                .into_iter()
                .filter(|s| *s != subscriber)
                .enumerate()
            {
                let ordinal = u32::try_from(ordinal).expect("two sources");
                e.push(injection::declared_edge(
                    (subscriber << 8) | ordinal,
                    subscriber,
                    slot(i, from),
                    from,
                    AUDIO_SEND_ID,
                ));
            }
        }
        e.push(injection::declared_edge(
            (b << 8) | 9,
            b,
            B_VIDEO_SLOT,
            a,
            VIDEO_SEND_ID,
        ));
        if with_c_video {
            e.push(injection::declared_edge(
                (c << 8) | 9,
                c,
                C_VIDEO_SLOT,
                a,
                VIDEO_SEND_ID,
            ));
        }
        e
    };
    let sources = |video: SourceDisposition| {
        vec![
            injection::declared_source(
                a,
                AUDIO_SEND_ID,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ),
            injection::declared_source(a, VIDEO_SEND_ID, TransportMode::StreamPerGroup, video),
            injection::declared_source(
                b,
                AUDIO_SEND_ID,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ),
            injection::declared_source(
                c,
                AUDIO_SEND_ID,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ),
        ]
    };

    let conns = |i: usize| -> Vec<wtransport::Connection> {
        [&x, &y]
            .iter()
            .map(|u| sessions[i][*u].connection().clone())
            .collect()
    };
    let (a_conns, b_conns, c_conns) = (conns(0), conns(1), conns(2));
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

    eprintln!("[38] phase (a) MIXED: start");
    // (a) MIXED: audio and video each to their own slots only.
    install_all(
        &mut injections,
        0,
        edges(false),
        sources(SourceDisposition::Forward),
    )
    .await;
    let (_, last_audio) = tokio::join!(
        publish(a, &a_conns, |c| c, PHASE_TICKS),
        send_a_audio(&a_conns, SEQ_A, 50)
    );
    let mut b_seen: Vec<Vec<SubscriberEvent>> = vec![Vec::new(), Vec::new()];
    for (sub, seen) in b_subs.iter_mut().zip(b_seen.iter_mut()) {
        until_streams("VIDEO (a) MIXED", "B", sub, seen, 2).await;
        let streams = ended(seen);
        assert_clean_streams("VIDEO (a) MIXED", "B", &streams, a, B_VIDEO_SLOT);
        assert!(
            streams.iter().all(|s| s.end == StreamEnd::Finished),
            "VIDEO (a): groups end cleanly at a frame boundary"
        );
    }
    for (i, conn) in b_conns.iter().enumerate() {
        let (n, slots) = a_audio_slots(conn, SEQ_A).await;
        assert!(n > 0, "VIDEO (a): B (pod {i}) receives A's audio");
        assert_eq!(
            slots,
            BTreeSet::from([slot(1, a)]),
            "VIDEO (a): A's audio lands on B's audio slot only, never the video slot"
        );
    }
    let _ = last_audio;

    eprintln!("[38] phase (b) LATE: start");
    // (b) LATE SUBSCRIBER: C's video edge goes in while A is mid-group.
    // Long groups (`BACKLOG_GROUP_INTERVAL`), so "mid-group" is wide.
    for sub in &mut b_subs {
        let _ = drain(sub).await;
    }
    let late = {
        let a_conns = a_conns.clone();
        tokio::spawn(async move {
            publish(
                a,
                &a_conns,
                |c| VideoPublisherConfig {
                    group_start: GroupStartSource::Periodic(BACKLOG_GROUP_INTERVAL),
                    ..c
                },
                3 * group_ticks(),
            )
            .await
        })
    };
    // Event-driven, never a sleep: per pod, B's first NON-start frame of this
    // run names the group A is inside; C's edge is installed right after.
    let mut in_flight = Vec::new();
    for sub in &mut b_subs {
        in_flight.push(until_mid_group_frame("VIDEO (b) LATE", "B", sub).await);
    }
    install_all(
        &mut injections,
        1,
        edges(true),
        sources(SourceDisposition::Forward),
    )
    .await;
    late.await.expect("publisher task");
    for (pod, sub) in c_subs.iter_mut().enumerate() {
        let mut seen = Vec::new();
        until_streams("VIDEO (b) LATE", "C", sub, &mut seen, 1).await;
        let first = seen.iter().find_map(|e| match e {
            SubscriberEvent::Frame {
                first_in_stream,
                frame,
                ..
            } => Some((
                *first_in_stream,
                frame.independently_decodable,
                frame.gop_counter,
            )),
            _ => None,
        });
        let (first_in_stream, group_start, gop) = first.expect("VIDEO (b): C received A's video");
        assert!(
            first_in_stream && group_start,
            "VIDEO (b): C's (pod {pod}) FIRST video frame is a group start — never mid-group"
        );
        assert!(
            gop > in_flight[pod],
            "VIDEO (b): C (pod {pod}) received nothing of the group A was inside when C's edge \
             went in — it starts at a LATER group"
        );
        assert_clean_streams("VIDEO (b) LATE", "C", &ended(&seen), a, C_VIDEO_SLOT);
    }
    for (sub, seen) in b_subs.iter_mut().zip(b_seen.iter_mut()) {
        seen.extend(drain(sub).await);
    }

    eprintln!("[38] phase (c) MUTE: start");
    // (c) SERVER VIDEO MUTE: the patched publisher keeps sending; nothing
    // reaches a video slot while A's audio forwards.
    let muted_base = injection::baselines(&prom, &reason_promql("server_muted")).await;
    install_all(
        &mut injections,
        2,
        edges(true),
        sources(SourceDisposition::DropAtIngress),
    )
    .await;
    let (_, last) = tokio::join!(
        publish(
            a,
            &a_conns,
            |c| VideoPublisherConfig {
                mode: PublisherMode::KeepSendingWhileMuted,
                ..c
            },
            PHASE_TICKS
        ),
        send_a_audio(&a_conns, SEQ_A + 10_000, 50)
    );
    // Ordering evidence FIRST: every video frame A sent was ingested and
    // dropped on each pod.
    injection::assert_rise(
        &prom,
        &handlers,
        &reason_promql("server_muted"),
        &muted_base,
        PHASE_TICKS,
        "VIDEO (c) SERVER_MUTED",
    )
    .await;
    for sub in b_subs.iter_mut().chain(c_subs.iter_mut()) {
        let events = drain(sub).await;
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, SubscriberEvent::Frame { .. })),
            "VIDEO (c): a video frame reached a receiver while A's video is DROP_AT_INGRESS"
        );
    }
    for conn in b_conns.iter().chain(c_conns.iter()) {
        let (n, _) = a_audio_slots(conn, SEQ_A + 10_000).await;
        assert!(
            n > 0,
            "VIDEO (c): A's audio still forwards while its video is muted"
        );
    }
    let _ = last;

    eprintln!("[38] phase (d) MISMATCH: start");
    // (d) TRANSPORT MISMATCH: uni streams labelled with the DATAGRAM audio id.
    let mismatch_base = injection::baselines(&prom, &reason_promql("transport_mismatch")).await;
    install_all(
        &mut injections,
        3,
        edges(true),
        sources(SourceDisposition::Forward),
    )
    .await;
    let publishers = publish(
        a,
        &a_conns,
        |c| VideoPublisherConfig {
            label: UplinkLabel::Override(AUDIO_SEND_ID),
            transport: VideoTransport::StreamPerGroup,
            ..c
        },
        PHASE_TICKS,
    )
    .await;
    // Per entity: each publisher saw MH stop its wrong-transport streams.
    for p in &publishers {
        assert!(
            p.events()
                .iter()
                .any(|e| matches!(e, PublisherEvent::StreamStoppedByPeer)),
            "VIDEO (d): MH must STOP_SENDING a uni stream labelled with the DATAGRAM audio id"
        );
    }
    // One refusal per stream: at least one per group start the publisher made.
    let groups = publishers
        .iter()
        .map(|p| {
            p.events()
                .iter()
                .filter(|e| matches!(e, PublisherEvent::GroupStart { .. }))
                .count()
        })
        .min()
        .unwrap_or(0);
    let groups = u32::try_from(groups).expect("a handful of groups").max(1);
    injection::assert_rise(
        &prom,
        &handlers,
        &reason_promql("transport_mismatch"),
        &mismatch_base,
        groups,
        "VIDEO (d) TRANSPORT_MISMATCH",
    )
    .await;
    for sub in b_subs.iter_mut().chain(c_subs.iter_mut()) {
        assert!(
            !drain(sub)
                .await
                .iter()
                .any(|e| matches!(e, SubscriberEvent::Frame { .. })),
            "VIDEO (d): a wrong-transport stream was delivered"
        );
    }

    eprintln!("[38] phase (e) PAUSED: start");
    // (e) PAUSED: client-muted video still forwards, counted as arrivals.
    let paused_base = injection::baselines(&prom, &paused_promql()).await;
    install_all(
        &mut injections,
        4,
        edges(true),
        sources(SourceDisposition::Paused),
    )
    .await;
    publish(a, &a_conns, |c| c, PHASE_TICKS).await;
    for (who, subs, slot_id) in [
        ("B", &mut b_subs, B_VIDEO_SLOT),
        ("C", &mut c_subs, C_VIDEO_SLOT),
    ] {
        for sub in subs.iter_mut() {
            let mut seen = Vec::new();
            until_streams("VIDEO (e) PAUSED", who, sub, &mut seen, 1).await;
            assert_clean_streams("VIDEO (e) PAUSED", who, &ended(&seen), a, slot_id);
        }
    }
    injection::assert_rise(
        &prom,
        &handlers,
        &paused_promql(),
        &paused_base,
        PHASE_TICKS,
        "VIDEO (e) PAUSED ARRIVALS",
    )
    .await;

    eprintln!("[38] phase (f) BACKLOG: start");
    // (f) BACKLOG: B HELD; C normal.
    install_all(
        &mut injections,
        5,
        edges(true),
        sources(SourceDisposition::Forward),
    )
    .await;
    let backlog_gauge = gauge_by_instance_present(
        &prom,
        "mh_media_egress_stream_backlog_limit_bytes",
        handlers.len(),
        env_tests::fixtures::metrics::PINNED_METRIC_BOUND,
    )
    .await;
    let backlog = backlog_gauge.values().copied().fold(0.0_f64, f64::max);
    assert!(
        backlog > 0.0,
        "PRECONDITION: the deployed backlog cap must be published"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let backlog = backlog as u64;
    let payload_len =
        env_tests::fixtures::video::backlog::backlog_payload_len(backlog, BACKLOG_FRAMES_PER_GROUP);
    let reset_base = injection::baselines(&prom, &group_reset_promql("subscriber_backlog")).await;
    for sub in &mut b_subs {
        let _ = drain(sub).await;
    }
    for sub in &mut c_subs {
        let _ = drain(sub).await;
    }
    drop(b_subs);
    let mut b_held: Vec<VideoSubscriber> = b_conns
        .iter()
        .cloned()
        .map(VideoSubscriber::spawn_held)
        .collect();
    let backlog_cfg = move |c: VideoPublisherConfig| VideoPublisherConfig {
        group_start: GroupStartSource::Periodic(BACKLOG_GROUP_INTERVAL),
        payload_len,
        ..c
    };
    // ONE publisher run spanning held -> released (its GoP counter never
    // restarts), group by group until told to stop.
    let stop = Arc::new(AtomicBool::new(false));
    let mut runs = Vec::new();
    for conn in &a_conns {
        let (conn, stop) = (conn.clone(), Arc::clone(&stop));
        let cfg = backlog_cfg(VideoPublisherConfig {
            frame_interval: FRAME_INTERVAL,
            ..VideoPublisherConfig::new(a)
        });
        runs.push(tokio::spawn(async move {
            let mut publisher = VideoPublisher::new(conn, cfg);
            while !stop.load(Ordering::Acquire) {
                publisher.run(&mut NoRequests, group_ticks()).await;
            }
            publisher.finish().await;
            publisher
        }));
    }
    eprintln!("[38] phase (f): publishers running, B held");
    send_a_audio(&a_conns, SEQ_A + 20_000, 50).await;
    for (i, conn) in b_conns.iter().enumerate() {
        let (n, _) = a_audio_slots(conn, SEQ_A + 20_000).await;
        assert!(
            n > 0,
            "VIDEO (f): B (pod {i})'s AUDIO keeps arriving while its video is held"
        );
    }
    injection::assert_rise(
        &prom,
        &handlers,
        &group_reset_promql("subscriber_backlog"),
        &reset_base,
        1,
        "VIDEO (f) SUBSCRIBER_BACKLOG (secondary)",
    )
    .await;
    eprintln!("[38] phase (f): subscriber_backlog rose; releasing B");
    for sub in &b_held {
        sub.release();
    }
    // Event-driven stop: once each released B has finished two groups.
    let mut b_seen_f: Vec<Vec<SubscriberEvent>> = vec![Vec::new(), Vec::new()];
    for (sub, seen) in b_held.iter_mut().zip(b_seen_f.iter_mut()) {
        let deadline = Instant::now() + PHASE_DEADLINE;
        while ended(seen)
            .iter()
            .filter(|s| s.end == StreamEnd::Finished)
            .count()
            < 2
        {
            assert!(
                Instant::now() < deadline,
                "VIDEO-NOT-OBSERVED: VIDEO (f): a released B finished fewer than two groups"
            );
            seen.extend(drain(sub).await);
        }
    }
    eprintln!("[38] phase (f): released B finished two groups; stopping publishers");
    stop.store(true, Ordering::Release);
    let mut publishers = Vec::new();
    for run in runs {
        publishers.push(run.await.expect("a publisher task panicked"));
    }

    eprintln!("[38] phase (f): publishers stopped; checking evidence");
    for pod in 0..publishers.len() {
        let sent: BTreeSet<u32> = publishers[pod]
            .events()
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::GroupStart { gop_counter, .. } => Some(*gop_counter),
                _ => None,
            })
            .collect();
        // C: every group the publisher sent, contiguous, all finished.
        let mut c_seen = Vec::new();
        let deadline = Instant::now() + PHASE_DEADLINE;
        while ended(&c_seen).len() < sent.len() {
            assert!(
                Instant::now() < deadline,
                "VIDEO-NOT-OBSERVED: VIDEO (f): C (pod {pod}) did not receive every group"
            );
            c_seen.extend(drain(&mut c_subs[pod]).await);
        }
        let c_streams = ended(&c_seen);
        assert!(
            c_streams.iter().all(|s| s.end == StreamEnd::Finished),
            "VIDEO (f): C (pod {pod}) is unaffected by B — never reset"
        );
        let c_gops: BTreeSet<u32> = c_streams
            .iter()
            .filter_map(|s| s.frames.first().map(|f| f.gop_counter))
            .collect();
        assert_eq!(
            c_gops, sent,
            "VIDEO (f): C (pod {pod}) receives every group A sent"
        );
        assert_clean_streams("VIDEO (f) BACKLOG", "C", &c_streams, a, C_VIDEO_SLOT);

        // B: reset while held; resumed at a LATER group, never mid-group.
        b_seen_f[pod].extend(drain(&mut b_held[pod]).await);
        let b_streams = ended(&b_seen_f[pod]);
        assert!(
            b_streams.iter().any(|s| s.end == StreamEnd::Reset),
            "VIDEO (f): B (pod {pod}) was reset past its backlog cap"
        );
        let b_finished: Vec<&ReceivedStream> = b_streams
            .iter()
            .filter(|s| s.end == StreamEnd::Finished)
            .collect();
        let b_gops: BTreeSet<u32> = b_finished
            .iter()
            .filter_map(|s| s.frames.first().map(|f| f.gop_counter))
            .collect();
        assert!(
            b_gops.is_subset(&c_gops),
            "VIDEO (f): B (pod {pod}) only got groups A sent"
        );
        let missing: BTreeSet<u32> = c_gops.difference(&b_gops).copied().collect();
        let (Some(&last_missing), Some(&first_resumed)) = (missing.last(), b_gops.first()) else {
            panic!("VIDEO (f): B (pod {pod}) must miss the abandoned group and resume after it");
        };
        assert!(
            first_resumed > last_missing,
            "VIDEO (f): B (pod {pod}) resumes at a group LATER than every one it missed"
        );
        assert!(
            b_finished
                .iter()
                .all(|s| s.first_frame_group_start == Some(true)),
            "VIDEO (f): B (pod {pod}) resumes on a group start"
        );
    }

    eprintln!("[38] phase CLEANUP: start");
    // CLEANUP before any session closes, with the release PROVED.
    for injection in injections {
        injection.release_and_prove(&cleanup).await;
    }
    drop(b_held);
    drop(c_subs);
    drop(sessions);
}
