//! Per-stream ingress attribution at MH on the DATAGRAM path (story 3 MH-A1;
//! R-16, R-17, R-20, R-33).
//!
//! One meeting of three real participants (A publishes, B and C receive),
//! each joined through GC -> MC and connected to BOTH Kind MH pods. The
//! forwarding policy is INJECTED on both pods with explicit `source_streams`
//! (MC sends none until MC-A), through the shared
//! `env_tests::fixtures::injection::Injection` helper, at a generation above
//! MC's: A declares audio (send id 0, DATAGRAM) and video (send id 1,
//! STREAM_PER_GROUP); B and C declare audio. B and C each hold one audio slot
//! of every other participant (MC's own placement, read off their
//! `StreamAssignments`) plus one injected VIDEO slot of A. Datagrams come from
//! `fixtures/media.rs` (audio) and T-A's keyless `VideoPublisher` over
//! datagrams with the label knob (video bytes, relabelled or not).
//!
//! Phases, run SEQUENTIALLY inside ONE test fn so no two concurrent fns ever
//! produce the same reason token:
//!
//! (a) the mixed DATAGRAM + STREAM_PER_GROUP registration applies on both pods
//!     and its per-stream echo lists every declared source stream;
//! (b) per-kind server mute: A's AUDIO at DROP_AT_INGRESS drops A's audio at
//!     every receiver while the stream stays declared (echoed);
//! (c) the patched client: A's VIDEO at DROP_AT_INGRESS, A keeps sending video
//!     bytes over datagrams — labelled as video they count `transport_mismatch`;
//!     RELABELLED as A's audio they reach B's and C's AUDIO slots only and
//!     nothing ever lands on a video slot; A's real audio is unaffected;
//! (d) an undeclared label counts `unmatched_stream`;
//! (e) A's audio PAUSED (client mute) still forwards, counted as paused-source
//!     arrivals.
//!
//! # Evidence (crates/env-tests/README.md evidence rule)
//!
//! PRIMARY is per entity: what B's and C's OWN connections receive, by the
//! downlink slot id each frame was rewritten to. Absence is concluded by a
//! STOP RULE, never a sleep, ORDERED on the publisher's own connection: MH
//! ingests each connection through one FIFO forwarder, so in phases (c), (d)
//! and (e) A's own forwarded audio, sent AFTER the test frames on the SAME
//! connections, is the control — absence holds once each receiver has seen
//! [`K_CONTROL`] of A's audio frames above the last test frame's sequence (plus
//! [`K_CONTROL`] of the other receiver's audio, proving the receiver itself is
//! live). In phase (b) every datagram from A drops, so there is no
//! same-connection control: the pinned `server_muted` rise (>= every test
//! frame, per pod) is asserted FIRST, which proves each test frame was
//! ingested and dropped, and the drain after it is then ordered. The deadline
//! is a FAILURE (`CONTROL-NOT-OBSERVED`), never the pass condition.
//!
//! SECONDARY is the pinned pod counter, past its own settled baseline. For
//! `unmatched_stream` and `transport_mismatch` the counter is the only positive
//! evidence (absence at a subscriber also looks like loss), which is acceptable
//! here because THIS FILE IS THE SOLE PRODUCER of those two tokens and env-test
//! files run one after another; the rise is asserted at least the number of
//! test frames the publisher sent to that pod (a lower bound, so one stray drop
//! cannot satisfy it). `server_muted` and the paused counter are read the same
//! way and are secondary to the per-receiver proof.
//!
//! No wall-clock pass condition anywhere.
//!
//! # Edge sum (README per-file edge-sum header rule)
//!
//! One test fn; P = 3 participants, each with N = 2 audio slots and at most
//! M = 1 video slot: 3 x (2 + 1) = 9.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 9

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::injection::{
    self, assert_mc_programmed_a_carrying_pod, injectable_handlers, Injection,
};
use env_tests::fixtures::media::{audio_datagram, AUDIO_SEND_ID, VIDEO_SEND_ID};
use env_tests::fixtures::metrics::{
    instance_for_pod_ip, poll_until_pinned_instance, settled_baseline, InstanceCounters,
};
use env_tests::fixtures::mh_grpc::{self, Handler, ReleaseOnDrop};
use env_tests::fixtures::participant::{self, open_sessions};
use env_tests::fixtures::video::{
    key_id_sender, NoRequests, PublisherMode, UplinkLabel, VideoPublisher, VideoPublisherConfig,
    VideoTransport,
};
use env_tests::fixtures::{AuthClient, GcClient, PrometheusClient};
use proto_gen::dark_tower::internal::v1::{EgressStream, SourceDisposition, SourceStream};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

const MARK_A: u8 = 0xA7;
const MARK_B: u8 = 0xB7;
const MARK_C: u8 = 0xC7;

/// Control frames (with a send sequence above the last test frame's) each
/// receiver must observe before an absence is concluded.
const K_CONTROL: usize = 5;
/// Test frames per phase, per pod.
const TEST_FRAMES: u32 = 20;
/// The cadence every test frame is sent at: one frame per 20 ms per
/// connection, a real publisher's rate (audio's 20 ms frames; the video
/// fixture's default). NOT a pass condition and not a wait for an outcome —
/// it is the send rate. It exists because MH's per-connection datagram
/// receive buffer is deliberately small (`DATAGRAM_RECEIVE_BUFFER_BYTES`,
/// ADR-0036 §11's bounded drop-oldest ingress): an unpaced burst of 20 frames
/// overflows it before the ingress loop reads, quinn evicts the oldest, and
/// the excess lands on `transport_receive_dropped` instead of the reason under
/// test (observed at Gate 2: 6 of 20 counted, 14 on
/// `transport_receive_dropped`, on both pods). Paced, every test frame is read
/// and attributed, which is what makes the `>= TEST_FRAMES` counter gates
/// sound on the lossless Kind link. Every pacing interval uses
/// `MissedTickBehavior::Delay`. A shortfall's failure message names
/// TRANSPORT-OVERFLOW as the first thing to rule out: MH counts that loss
/// (`transport_receive_dropped`) only at connection teardown, so it cannot be
/// checked live inside a phase.
const FRAME_INTERVAL: Duration = Duration::from_millis(20);
/// Failure bounds only — never pass conditions.
const PHASE_DEADLINE: Duration = Duration::from_secs(30);
const PINNED_METRIC_BOUND: Duration = Duration::from_secs(90);
const BASELINE_SETTLE_BOUND: Duration = Duration::from_secs(90);
const VIEW_MARGIN: Duration = Duration::from_secs(10);

/// The injected VIDEO slot ids B and C hold for A — disjoint from every audio
/// slot id below and from every sender id.
const B_VIDEO_SLOT: u32 = 600;
const C_VIDEO_SLOT: u32 = 601;

/// Sequence floors per phase, above the shared binding sequence.
const SEQ_B: u32 = 1_100_000;
const SEQ_C: u32 = 1_200_000;
const SEQ_D: u32 = 1_250_000;
const SEQ_E: u32 = 1_300_000;
const _: () = assert!(SEQ_B >= env_tests::fixtures::media::AUDIO_SEQUENCE_CEILING);

fn reason_promql(reason: &str) -> String {
    format!(r#"sum by (instance) (mh_media_frames_dropped_total{{reason="{reason}"}})"#)
}

fn paused_promql() -> String {
    "sum by (instance) (mh_media_paused_source_frames_total)".to_string()
}

/// One egress stream through the shared fixture. The superseded per-egress
/// transport is UNSPECIFIED, as MC-A writes it: with `source_streams` declared,
/// the edge's transport derives from them.
fn edge(id: u32, subscriber: u32, slot: u32, source: u32, stream: u16) -> EgressStream {
    injection::edge(
        id,
        subscriber,
        slot,
        source,
        u32::from(stream),
        TransportMode::Unspecified,
    )
}

fn source(sender_id: u32, stream: u16, mode: TransportMode, d: SourceDisposition) -> SourceStream {
    injection::source_stream(sender_id, u32::from(stream), mode, d)
}

/// What one received datagram is, for this file's classification.
enum Rx {
    /// An audio-marker frame: `(marker, seq, slot)`.
    Audio(u8, u32, u32),
    /// A video-fixture frame whose key id names A: its downlink slot.
    AVideo(u32),
    Other,
}

fn classify(raw: &[u8], a: u32) -> Rx {
    let view = media_protocol::codec::decode_datagram(raw)
        .unwrap_or_else(|_| panic!("a receiver got a malformed datagram"));
    let slot = u32::from(view.stream_id());
    let payload = view.payload();
    match payload.first().copied() {
        Some(m @ (MARK_A | MARK_B | MARK_C)) => Rx::Audio(m, view.stream_sequence(), slot),
        _ if key_id_sender(payload).map(u32::from) == Some(a) => Rx::AVideo(slot),
        _ => Rx::Other,
    }
}

/// Drain everything queued on `conn` into `each`.
async fn drain_raw(conn: &wtransport::Connection, mut each: impl FnMut(&[u8])) {
    while let Ok(Ok(d)) =
        tokio::time::timeout(Duration::from_millis(50), conn.receive_datagram()).await
    {
        each(&d.payload());
    }
}

/// The per-receiver view of one phase.
#[derive(Default)]
struct Observed {
    /// `(slot, seq)` of A's audio-marker frames above the phase floor.
    a_audio: Vec<(u32, u32)>,
    /// Slots A's video-fixture frames arrived on.
    a_video_slots: BTreeSet<u32>,
    /// Control frames above the last test frame's sequence.
    controls_after: usize,
    /// A's OWN audio frames above the last test frame's sequence — the
    /// same-connection ordering control.
    a_tail: usize,
}

/// One receiver: its two MH connections, its slot for A, the control marker
/// the OTHER receiver sends it, and that sender's connections.
struct Receiver<'a> {
    name: &'static str,
    conns: Vec<&'a wtransport::Connection>,
    a_audio_slot: u32,
    video_slot: u32,
    control_marker: u8,
}

/// Observe a phase after the test frames are on the wire: the other
/// receivers' audio controls are sent, and — when `a_tail` is given — A's OWN
/// audio on A's connections (the same connections as the test frames, so MH's
/// per-connection ingress orders them after every test frame), until every
/// receiver has seen [`K_CONTROL`] of each above `last_test_seq`. Returns what
/// each receiver saw.
///
/// A frame on a VIDEO slot fails at once, in every phase: on the datagram path
/// nothing may ever reach a stream-per-group slot.
async fn observe_phase(
    phase: &str,
    a: u32,
    floor: u32,
    receivers: &[Receiver<'_>],
    controls: &[(&wtransport::Connection, u8)],
    a_tail: Option<&[&wtransport::Connection]>,
    last_test_seq: u32,
) -> Vec<Observed> {
    let mut observed: Vec<Observed> = receivers.iter().map(|_| Observed::default()).collect();
    let deadline = Instant::now() + PHASE_DEADLINE;
    let mut seq = last_test_seq;
    while observed
        .iter()
        .any(|o| o.controls_after < K_CONTROL || (a_tail.is_some() && o.a_tail < K_CONTROL))
    {
        assert!(
            Instant::now() < deadline,
            "CONTROL-NOT-OBSERVED: {phase}: within {PHASE_DEADLINE:?} not every receiver saw \
             {K_CONTROL} control frames after the last test frame (counts {:?}). This bounds the \
             failure only; the absence assertions need the control as their stop rule. Check \
             mh_media_frames_dropped_total{{reason}} for the control senders' audio.",
            observed
                .iter()
                .map(|o| (o.controls_after, o.a_tail))
                .collect::<Vec<_>>()
        );
        seq += 1;
        for conn in a_tail.into_iter().flatten() {
            conn.send_datagram(audio_datagram(seq, MARK_A))
                .unwrap_or_else(|e| panic!("{phase}: A's tail audio could not send: {e}"));
        }
        for (conn, marker) in controls {
            conn.send_datagram(audio_datagram(seq, *marker))
                .unwrap_or_else(|e| panic!("{phase}: control {marker:#04x} could not send: {e}"));
        }
        for (r, o) in receivers.iter().zip(observed.iter_mut()) {
            for conn in &r.conns {
                drain_raw(conn, |raw| match classify(raw, a) {
                    Rx::Audio(m, s, slot) => {
                        assert!(
                            slot != r.video_slot,
                            "{phase}: {} received an audio-marker frame on its VIDEO slot",
                            r.name
                        );
                        if m == MARK_A && s > floor {
                            o.a_audio.push((slot, s));
                            if s > last_test_seq {
                                o.a_tail += 1;
                            }
                        } else if m == r.control_marker && s > last_test_seq {
                            o.controls_after += 1;
                        }
                    }
                    Rx::AVideo(slot) => {
                        assert!(
                            slot != r.video_slot,
                            "{phase}: {} received A's video bytes on its VIDEO slot over a \
                             datagram — nothing on the datagram path may reach a \
                             stream-per-group slot",
                            r.name
                        );
                        o.a_video_slots.insert(slot);
                    }
                    Rx::Other => {}
                })
                .await;
            }
        }
    }
    observed
}

/// Baselines for `promql` on each pinned pod.
async fn baselines(prom: &PrometheusClient, promql: &str) -> InstanceCounters {
    settled_baseline(
        prom,
        promql,
        BASELINE_SETTLE_BOUND,
        "the assertion under test",
    )
    .await
}

/// Each pinned pod's counter rises by at least `at_least` past its baseline.
async fn assert_rise(
    prom: &PrometheusClient,
    handlers: &[Handler],
    promql: &str,
    base: &InstanceCounters,
    at_least: u32,
    phase: &str,
) {
    for h in handlers {
        let b = instance_for_pod_ip(base, &h.pod_ip).map_or(0.0, |(_, v)| v);
        let target = b + f64::from(at_least);
        poll_until_pinned_instance(
            prom,
            promql,
            &h.pod_ip,
            PINNED_METRIC_BOUND,
            Duration::from_secs(2),
            phase,
            &format!(
                ">= {target} (baseline {b} + the {at_least} test frames sent to this pod). \
                 If short, rule out TRANSPORT-OVERFLOW first: receive-buffer loss before \
                 attribution is counted as transport_receive_dropped only at connection \
                 TEARDOWN (it is forensic, not live), so read that series on this pod after \
                 the run; a rise there is input-side loss, not an attribution fault, and is \
                 never fixed by relaxing this gate"
            ),
            |v| v >= target,
        )
        .await;
    }
}

/// Install `edges` + `declared` at `step` on every pod; require the apply and
/// a per-stream echo of exactly the declared set (PAUSED and DROP_AT_INGRESS
/// included — the echo carries no disposition).
async fn install_all(
    injections: &mut [Injection<'_>],
    step: u64,
    edges: Vec<EgressStream>,
    declared: Vec<SourceStream>,
) {
    let want: BTreeSet<(u32, u32, i32)> = declared
        .iter()
        .map(|s| (s.sender_id, s.stream_number, s.transport_mode))
        .collect();
    for injection in injections.iter_mut() {
        let g = injection.generation(step);
        let resp = injection
            .register_full(g, edges.clone(), &[], declared.clone())
            .await;
        assert_eq!(
            resp.applied_generation, g,
            "ATTR APPLY on {}: the mixed-transport registration must apply",
            injection.handler.name
        );
        let echoed: BTreeSet<(u32, u32, i32)> = resp
            .applied_source_streams
            .iter()
            .map(|e| (e.sender_id, e.stream_number, e.transport_mode))
            .collect();
        assert_eq!(
            echoed, want,
            "ATTR ECHO on {}: the per-stream echo must list every declared source stream with \
             its transport",
            injection.handler.name
        );
    }
}

#[tokio::test]
#[serial_test::serial(mh_notifications)]
async fn test_per_stream_attribution_on_the_datagram_path() {
    let cluster = ClusterConnection::new()
        .await
        .expect("PRECONDITION: cluster connection (ENV_TEST_* URLs)");
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let window = participant::settle_window(&cluster).await;
    let auth = AuthClient::new(&cluster.ac_base_url);
    let gc = GcClient::new(&cluster.gc_base_url);
    let mut ps =
        participant::meeting_of(&auth, &gc, "Stream Attribution 37", &["A", "B", "C"]).await;
    let (x, y) = participant::two_offered_handlers("ATTR", &ps);
    let (a, b, c) = (ps[0].sender_id(), ps[1].sender_id(), ps[2].sender_id());
    let sessions = open_sessions("ATTR", &ps, &[&x, &y]).await;

    // N = 2 audio slots each, ids disjoint from sender ids and video slots.
    ps[0].declare(&[11, 12]).await;
    ps[1].declare(&[21, 22]).await;
    ps[2].declare(&[31, 32]).await;
    let bound = window + VIEW_MARGIN;
    for (i, others) in [(0, [b, c]), (1, [a, c]), (2, [a, b])] {
        let want: BTreeSet<u32> = others.into_iter().collect();
        ps[i]
            .converge("ATTR", bound, |v, d| {
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

    // (a) INJECT the mixed registration on both pods.
    let handlers = injectable_handlers(&prom, BASELINE_SETTLE_BOUND).await;
    let token = mh_grpc::mc_service_token(&cluster.ac_base_url).await;
    let cleanup = ReleaseOnDrop::new(token.clone());
    let mut injections = Vec::new();
    for handler in &handlers {
        injections.push(Injection::open(handler, &token, &cleanup, &ps[0]).await);
    }
    assert_mc_programmed_a_carrying_pod(&injections);

    let edges = || {
        let mut e = Vec::new();
        for (i, subscriber) in [(0_usize, a), (1, b), (2, c)] {
            for (ordinal, from) in [a, b, c]
                .into_iter()
                .filter(|s| *s != subscriber)
                .enumerate()
            {
                let ordinal = u32::try_from(ordinal).expect("two sources");
                e.push(edge(
                    (subscriber << 8) | ordinal,
                    subscriber,
                    slot(i, from),
                    from,
                    AUDIO_SEND_ID,
                ));
            }
        }
        e.push(edge((b << 8) | 9, b, B_VIDEO_SLOT, a, VIDEO_SEND_ID));
        e.push(edge((c << 8) | 9, c, C_VIDEO_SLOT, a, VIDEO_SEND_ID));
        e
    };
    let sources = |audio: SourceDisposition, video: SourceDisposition| {
        vec![
            source(a, AUDIO_SEND_ID, TransportMode::Datagram, audio),
            source(a, VIDEO_SEND_ID, TransportMode::StreamPerGroup, video),
            source(
                b,
                AUDIO_SEND_ID,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ),
            source(
                c,
                AUDIO_SEND_ID,
                TransportMode::Datagram,
                SourceDisposition::Forward,
            ),
        ]
    };
    install_all(
        &mut injections,
        0,
        edges(),
        sources(SourceDisposition::Forward, SourceDisposition::Forward),
    )
    .await;

    let a_conns: Vec<&wtransport::Connection> = [&x, &y]
        .iter()
        .map(|u| sessions[0][*u].connection())
        .collect();
    let b_conns: Vec<&wtransport::Connection> = [&x, &y]
        .iter()
        .map(|u| sessions[1][*u].connection())
        .collect();
    let c_conns: Vec<&wtransport::Connection> = [&x, &y]
        .iter()
        .map(|u| sessions[2][*u].connection())
        .collect();
    let receivers = [
        Receiver {
            name: "B",
            conns: b_conns.clone(),
            a_audio_slot: slot(1, a),
            video_slot: B_VIDEO_SLOT,
            control_marker: MARK_C,
        },
        Receiver {
            name: "C",
            conns: c_conns.clone(),
            a_audio_slot: slot(2, a),
            video_slot: C_VIDEO_SLOT,
            control_marker: MARK_B,
        },
    ];
    let controls: Vec<(&wtransport::Connection, u8)> = c_conns
        .iter()
        .map(|c| (*c, MARK_C))
        .chain(b_conns.iter().map(|b| (*b, MARK_B)))
        .collect();
    // Test frames are PACED at a real publisher's cadence (see
    // `FRAME_INTERVAL`): an unpaced burst overflows MH's deliberately small
    // per-connection datagram receive buffer before its ingress loop reads,
    // and the excess is quinn's drop-oldest (`transport_receive_dropped`), not
    // the attribution under test.
    let send_a_audio = |floor: u32| {
        let a_conns = a_conns.clone();
        async move {
            let mut tick = tokio::time::interval(FRAME_INTERVAL);
            // Delay, never Burst: a stalled task must not fire its missed
            // ticks back to back and rebuild the burst this pacing removes
            // (same choice as `VideoPublisher::run`).
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            for n in 1..=TEST_FRAMES {
                tick.tick().await;
                for conn in &a_conns {
                    conn.send_datagram(audio_datagram(floor + n, MARK_A))
                        .unwrap_or_else(|e| panic!("A could not send audio: {e}"));
                }
            }
            floor + TEST_FRAMES
        }
    };
    let send_a_video = |label: UplinkLabel| {
        let a_conns = a_conns.clone();
        async move {
            for conn in &a_conns {
                let mut publisher = VideoPublisher::new(
                    (*conn).clone(),
                    VideoPublisherConfig {
                        label,
                        transport: VideoTransport::Datagram,
                        mode: PublisherMode::KeepSendingWhileMuted,
                        frame_interval: FRAME_INTERVAL,
                        ..VideoPublisherConfig::new(a)
                    },
                );
                // `run` paces at `frame_interval` (see `send_a_audio`).
                publisher.run(&mut NoRequests, TEST_FRAMES).await;
            }
        }
    };

    // (b) PER-KIND SERVER MUTE: A's audio DROP_AT_INGRESS, still declared.
    let muted_base = baselines(&prom, &reason_promql("server_muted")).await;
    install_all(
        &mut injections,
        1,
        edges(),
        sources(SourceDisposition::DropAtIngress, SourceDisposition::Forward),
    )
    .await;
    let last = send_a_audio(SEQ_B).await;
    // Every datagram from A drops here, so there is no same-connection
    // control: the ORDERING evidence is the pinned counter, FIRST — once each
    // pod has counted every test frame as `server_muted`, every test frame was
    // ingested and dropped, so the drain after it is ordered.
    assert_rise(
        &prom,
        &handlers,
        &reason_promql("server_muted"),
        &muted_base,
        TEST_FRAMES,
        "ATTR (b) SERVER_MUTED",
    )
    .await;
    let seen = observe_phase(
        "ATTR (b) AUDIO MUTED",
        a,
        SEQ_B,
        &receivers,
        &controls,
        None,
        last,
    )
    .await;
    for (r, o) in receivers.iter().zip(&seen) {
        assert!(
            o.a_audio.is_empty(),
            "ATTR (b): {} received A's audio while A's audio stream is DROP_AT_INGRESS",
            r.name
        );
    }

    // (c) THE PATCHED CLIENT: A's video DROP_AT_INGRESS, A keeps sending video
    // bytes over datagrams.
    install_all(
        &mut injections,
        2,
        edges(),
        sources(SourceDisposition::Forward, SourceDisposition::DropAtIngress),
    )
    .await;
    // Labelled as video, on the wrong transport: transport_mismatch.
    let mismatch_base = baselines(&prom, &reason_promql("transport_mismatch")).await;
    send_a_video(UplinkLabel::Directed).await;
    assert_rise(
        &prom,
        &handlers,
        &reason_promql("transport_mismatch"),
        &mismatch_base,
        TEST_FRAMES,
        "ATTR (c) TRANSPORT_MISMATCH",
    )
    .await;
    // Relabelled as A's audio: reaches AUDIO slots only (positive), never a
    // video slot (asserted on every frame by `observe_phase`); A's real audio
    // flows too.
    send_a_video(UplinkLabel::Override(AUDIO_SEND_ID)).await;
    let last = send_a_audio(SEQ_C).await;
    let seen = observe_phase(
        "ATTR (c) PATCHED CLIENT",
        a,
        SEQ_C,
        &receivers,
        &controls,
        Some(&a_conns),
        last,
    )
    .await;
    for (r, o) in receivers.iter().zip(&seen) {
        assert_eq!(
            o.a_video_slots,
            BTreeSet::from([r.a_audio_slot]),
            "ATTR (c): {}'s relabelled-video frames must arrive on its AUDIO slot for A and \
             nowhere else",
            r.name
        );
        assert!(
            !o.a_audio.is_empty() && o.a_audio.iter().all(|(s, _)| *s == r.a_audio_slot),
            "ATTR (c): A's audio is unaffected by its video mute — {} must keep receiving it on \
             its audio slot for A",
            r.name
        );
    }

    // (d) UNMATCHED: an undeclared label. Per entity: the frames are delivered
    // NOWHERE — A's own audio, sent after them on the same connections, is
    // the ordering control, and no A video frame lands on any slot.
    let unmatched_base = baselines(&prom, &reason_promql("unmatched_stream")).await;
    send_a_video(UplinkLabel::Override(7)).await;
    let seen = observe_phase(
        "ATTR (d) UNMATCHED",
        a,
        SEQ_D,
        &receivers,
        &controls,
        Some(&a_conns),
        SEQ_D,
    )
    .await;
    for (r, o) in receivers.iter().zip(&seen) {
        assert!(
            o.a_video_slots.is_empty(),
            "ATTR (d): {} received A's undeclared-label frames on slots {:?}; an unmatched \
             stream is delivered nowhere",
            r.name,
            o.a_video_slots
        );
    }
    assert_rise(
        &prom,
        &handlers,
        &reason_promql("unmatched_stream"),
        &unmatched_base,
        TEST_FRAMES,
        "ATTR (d) UNMATCHED_STREAM",
    )
    .await;

    // (e) CLIENT MUTE: A's audio PAUSED still forwards, counted as arrivals.
    let paused_base = baselines(&prom, &paused_promql()).await;
    install_all(
        &mut injections,
        3,
        edges(),
        sources(SourceDisposition::Paused, SourceDisposition::Forward),
    )
    .await;
    let last = send_a_audio(SEQ_E).await;
    let seen = observe_phase(
        "ATTR (e) PAUSED",
        a,
        SEQ_E,
        &receivers,
        &controls,
        Some(&a_conns),
        last,
    )
    .await;
    for (r, o) in receivers.iter().zip(&seen) {
        assert!(
            !o.a_audio.is_empty() && o.a_audio.iter().all(|(s, _)| *s == r.a_audio_slot),
            "ATTR (e): a PAUSED stream is never dropped at ingress — {} must receive A's audio \
             on its audio slot for A",
            r.name
        );
    }
    assert_rise(
        &prom,
        &handlers,
        &paused_promql(),
        &paused_base,
        TEST_FRAMES,
        "ATTR (e) PAUSED ARRIVALS",
    )
    .await;

    // CLEANUP before any session closes, with the release PROVED.
    for injection in injections {
        injection.release_and_prove(&cleanup).await;
    }
    drop(sessions);
}
