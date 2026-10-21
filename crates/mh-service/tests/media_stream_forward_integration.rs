//! Stream-per-group cut-through forwarding (story 3 MH-A2; ADR-0036 §1, §2,
//! §7, §10 Tier 1a/1b), driven end to end through the transport seam's
//! deterministic shim: the real stream ingress accept loop, the real edge
//! runtime and the real per-subscriber stream egress task — zero syscalls,
//! tokio paused time, current-thread runtime (MetricAssertion is
//! thread-local).
//!
//! Gates here: frames in = frames out across every chunk split; zero-copy
//! payload fan-out and copy-on-backlog; first forwarded frame is a group
//! start; keep A until B's group start, abandon when B leaves, switch to
//! PAUSED; hop continuity (and the audio refill regression); re-push
//! idempotency; the stream-path attribution rows. Abandonment gates live in
//! `media_stream_abandonment_integration.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use ::common::observability::testing::MetricAssertion;
use bytes::Bytes;
use common::media_frame::{
    parse_stream, stream_frame, stream_group, FrameShape, UPLINK_VIDEO_SEND_ID,
};
use common::stream_rig::{concat, limits, sender, settle, StreamRig, MEETING};
use mh_service::media::edge::EdgeState;
use mh_service::transport::INGRESS_REFUSED;
use mh_test_utils::media_policy::{
    egress_from, register_request_declared, source_stream, stream_egress,
};
use mh_test_utils::stream_shim::EgressTerminal;
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, SourceDisposition};
use proto_gen::dark_tower::signaling::v1::TransportMode;

const FWD: SourceDisposition = SourceDisposition::Forward;
const PAUSED: SourceDisposition = SourceDisposition::Paused;
const DROP: SourceDisposition = SourceDisposition::DropAtIngress;
const DGRAM: TransportMode = TransportMode::Datagram;
const SPG: TransportMode = TransportMode::StreamPerGroup;
const AUDIO: u32 = 0;
const VIDEO: u32 = 1;
const VIDEO_SLOT: u16 = 4;

const FORWARDED: &str = "mh_media_frames_forwarded_total";
const DROPPED: &str = "mh_media_frames_dropped_total";

/// Publisher 5 declares audio (0, DATAGRAM) and video (1, `STREAM_PER_GROUP`)
/// with `video` disposition; each subscriber holds an audio slot 0 and a video
/// slot 4 fed by publisher 5.
fn mixed(generation: u64, video: SourceDisposition, subscribers: &[u32]) -> RegisterMeetingRequest {
    let mut edges = Vec::new();
    for (i, sub) in subscribers.iter().enumerate() {
        let base = 10 + 2 * u32::try_from(i).unwrap();
        edges.push(egress_from(base, *sub, 0, 5, AUDIO));
        edges.push(stream_egress(
            base + 1,
            *sub,
            u32::from(VIDEO_SLOT),
            5,
            VIDEO,
        ));
    }
    register_request_declared(
        MEETING,
        generation,
        edges,
        vec![
            source_stream(5, AUDIO, DGRAM, FWD),
            source_stream(5, VIDEO, SPG, video),
        ],
    )
}

/// Two video publishers (5 = A, 8 = B); subscriber 6's video slot (edge 11)
/// fed by `candidate`, with B's disposition `b`.
fn switching(generation: u64, candidate: u32, b: SourceDisposition) -> RegisterMeetingRequest {
    register_request_declared(
        MEETING,
        generation,
        vec![stream_egress(
            11,
            6,
            u32::from(VIDEO_SLOT),
            candidate,
            VIDEO,
        )],
        vec![
            source_stream(5, VIDEO, SPG, FWD),
            source_stream(8, VIDEO, SPG, b),
        ],
    )
}

/// The frames as they must read on the downlink: each input frame with its
/// relay region rewritten to (slot, hop) through the codec.
fn expected_downlink(frames: &[Bytes], slot: u16, first_hop: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, frame) in frames.iter().enumerate() {
        let mut copy = frame.to_vec();
        media_protocol::codec::rewrite_relay_region(
            &mut copy,
            slot,
            first_hop + u32::try_from(i).unwrap(),
        )
        .unwrap();
        out.extend_from_slice(&copy);
    }
    out
}

/// A tiny deterministic generator for the seeded random splitter.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from((self.0 >> 33) % u64::try_from(bound).unwrap()).unwrap()
    }
}

fn split_at_points(all: &Bytes, points: &[usize]) -> Vec<Bytes> {
    let mut chunks = Vec::new();
    let mut from = 0;
    for &p in points {
        if p > from && p < all.len() {
            chunks.push(all.slice(from..p));
            from = p;
        }
    }
    chunks.push(all.slice(from..));
    chunks
}

/// Drive one group through one split and assert frames in = frames out.
async fn one_split(frames: &[Bytes], chunks: Vec<Bytes>, label: &str) {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 1000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let inbound = publisher.push_inbound_uni();
    for chunk in chunks {
        inbound.push(chunk);
        settle().await;
    }
    inbound.finish();
    settle().await;

    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 1, "{label}: one group, one downlink stream");
    assert_eq!(streams[0].terminal(), EgressTerminal::Finished, "{label}");
    assert_eq!(
        streams[0].bytes(),
        expected_downlink(frames, VIDEO_SLOT, 0),
        "{label}: byte-identical except the rewritten relay region"
    );
    let n = u64::try_from(frames.len()).unwrap();
    assert!(n > 0, "positive control");
    metrics
        .counter(FORWARDED)
        .with_labels(&[("direction", "ingress")])
        .assert_delta(n);
    metrics
        .counter(FORWARDED)
        .with_labels(&[("direction", "egress")])
        .assert_delta(n);
}

#[tokio::test(start_paused = true)]
async fn frames_in_equal_frames_out_across_every_chunk_split() {
    let frames = stream_group(100, 7, UPLINK_VIDEO_SEND_ID, 2);
    let all = concat(&frames);
    let first_end = frames[0].len();

    // (i) a single split at every byte offset, header bytes included.
    for k in 1..all.len() {
        one_split(&frames, split_at_points(&all, &[k]), &format!("split@{k}")).await;
    }
    // (ii) one byte at a time.
    let bytes: Vec<Bytes> = (0..all.len()).map(|i| all.slice(i..=i)).collect();
    one_split(&frames, bytes, "1-byte").await;
    // (iii) seeded random multi-splits.
    for seed in [1_u64, 7, 42, 1_000_003] {
        let mut rng = Lcg(seed);
        let mut points: Vec<usize> = (0..12).map(|_| rng.next(all.len())).collect();
        points.sort_unstable();
        points.dedup();
        one_split(
            &frames,
            split_at_points(&all, &points),
            &format!("seed {seed}"),
        )
        .await;
    }
    // (iv) one chunk spanning the end of frame 0 and the next header.
    one_split(
        &frames,
        split_at_points(&all, &[first_end - 5, first_end + 30]),
        "frame-end + next header",
    )
    .await;
    // (v) the whole group in ONE chunk: end of N, all of N+1, start of N+2.
    one_split(&frames, vec![all.clone()], "one chunk").await;
}

/// ADR-0036 §10 Tier 1a: one ingress buffer, N egress payloads sharing it —
/// asserted UPSTREAM of the transport seam (on the downlinks' queues), never
/// through a write.
#[tokio::test(start_paused = true)]
async fn payload_fan_out_is_zero_copy_on_buffer_identity() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6, 7]), limits(1 << 20, 1000));
    let publisher = rig.publisher(5);
    let subs = [rig.subscriber(6), rig.subscriber(7)];
    // Writers park in open_uni: the queues hold what fan-out produced.
    for s in &subs {
        s.set_open_credit(0);
    }
    let frame = stream_frame(
        1,
        0,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[9; 400],
        FrameShape::Minimal,
    );
    let parent = frame.clone();
    let inbound = publisher.push_inbound_uni();
    inbound.push(frame);
    settle().await;

    let range = parent.as_ptr() as usize..parent.as_ptr() as usize + parent.len();
    let mut shared_ptrs = Vec::new();
    for edge in [11, 13] {
        let downlink = rig
            .cell(edge)
            .current_downlink()
            .expect("a downlink was opened");
        let items = downlink.queued_items();
        assert!(
            !items[0].is_shared(),
            "the header is a per-subscriber rewritten copy"
        );
        let shared: Vec<_> = items.iter().filter(|i| i.is_shared()).collect();
        assert_eq!(shared.len(), 1, "the tail went out uncopied");
        let ptr = shared[0].bytes().as_ptr() as usize;
        assert!(
            range.contains(&ptr),
            "the payload is a slice of the ingress buffer"
        );
        shared_ptrs.push(ptr);
    }
    assert_eq!(
        shared_ptrs[0], shared_ptrs[1],
        "N egress payloads, one allocation"
    );
    assert!(
        !parent.is_unique(),
        "positive control: the fan-out holds the parent"
    );

    for s in &subs {
        s.grant_open_credit(1);
    }
    inbound.finish();
    settle().await;
    assert!(
        parent.is_unique(),
        "nothing retains the ingress buffer once written"
    );
}

/// Copy-on-backlog (security): while an uncopied slice is outstanding, the
/// next chunk is COPIED, so a slow destination never pins more than one
/// ingress allocation — and the coalesced copy still arrives byte-identical
/// and in order.
#[tokio::test(start_paused = true)]
async fn a_backlogged_destination_copies_rather_than_pinning_and_stays_byte_identical() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    subscriber.set_stream_credit(0); // the writer opens, then parks in write_all
                                     // `parents` is the ONLY test-side handle on each frame's allocation.
    let parents = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 2);
    let expected = expected_downlink(&parents, VIDEO_SLOT, 0);
    let inbound = publisher.push_inbound_uni();
    for frame in &parents {
        inbound.push(frame.clone());
        settle().await;
    }
    assert!(
        !parents[0].is_unique(),
        "one uncopied slice: the first frame's tail"
    );
    assert!(
        parents[1].is_unique(),
        "while it is outstanding the next tail was copied"
    );
    assert!(parents[2].is_unique());

    subscriber.clear_stream_credit();
    inbound.finish();
    settle().await;
    assert!(parents[0].is_unique(), "released once written");
    let streams = subscriber.egress_uni_streams();
    assert_eq!(
        streams[0].bytes(),
        expected,
        "the coalesced copy is byte-identical, in order"
    );
    assert_eq!(streams[0].terminal(), EgressTerminal::Finished);
}

#[tokio::test(start_paused = true)]
async fn a_subscribers_first_forwarded_frame_is_a_group_start() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 1000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);

    // A stream whose first frame is NOT a group start: a publisher fault,
    // gated like any non-start — nothing forwarded.
    let headless = publisher.push_inbound_uni();
    headless.push(stream_frame(
        1,
        0,
        UPLINK_VIDEO_SEND_ID,
        false,
        &[1; 40],
        FrameShape::Minimal,
    ));
    headless.push(stream_frame(
        2,
        0,
        UPLINK_VIDEO_SEND_ID,
        false,
        &[2; 40],
        FrameShape::Minimal,
    ));
    headless.finish();
    settle().await;
    assert!(
        subscriber.egress_uni_streams().is_empty(),
        "never mid-group"
    );
    assert!(
        rig.cell(11).waiting_with_source_seen().is_some(),
        "seen, still waiting"
    );

    let group = stream_group(3, 1, UPLINK_VIDEO_SEND_ID, 2);
    let inbound = publisher.push_inbound_uni();
    inbound.push(concat(&group));
    inbound.finish();
    settle().await;
    let frames = parse_stream(&subscriber.egress_uni_streams()[0].bytes());
    assert!(
        frames[0].group_start,
        "the first forwarded frame starts a group"
    );
    assert_eq!(frames.len(), 3);
    // The gated frames were ingested (counted) but consumed no hop number.
    assert_eq!(frames[0].hop, 0);
    metrics
        .counter(FORWARDED)
        .with_labels(&[("direction", "ingress")])
        .assert_delta(5);
    metrics
        .counter(FORWARDED)
        .with_labels(&[("direction", "egress")])
        .assert_delta(3);
}

/// A late subscriber (edge installed mid-group) receives nothing until the
/// publisher's NEXT group start.
#[tokio::test(start_paused = true)]
async fn a_late_subscriber_waits_for_the_next_group_start() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let early = rig.subscriber(6);
    let late = rig.subscriber(7);
    let group1 = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 3);
    let s1 = publisher.push_inbound_uni();
    s1.push(group1[0].clone());
    s1.push(group1[1].clone());
    settle().await;
    rig.install(&mixed(2, FWD, &[6, 7]));
    s1.push(group1[2].clone());
    s1.push(group1[3].clone());
    s1.finish();
    settle().await;
    assert!(
        late.egress_uni_streams().is_empty(),
        "mid-group: nothing for the late subscriber"
    );
    let group2 = stream_group(5, 1, UPLINK_VIDEO_SEND_ID, 1);
    let s2 = publisher.push_inbound_uni();
    s2.push(concat(&group2));
    s2.finish();
    settle().await;
    let late_frames = parse_stream(&late.egress_uni_streams()[0].bytes());
    assert!(late_frames[0].group_start);
    assert_eq!(
        late_frames[0].gop, 1,
        "the next group, not the one in flight"
    );
    assert_eq!(early.egress_uni_streams().len(), 2);
}

/// ADR-0036 §7 pending switch: A keeps flowing until B's group start, B's
/// non-starts are gated (consuming no hop), and the hop continues across both
/// group streams and the source change.
#[tokio::test(start_paused = true)]
async fn a_is_kept_until_bs_group_start_and_the_hop_continues_across_the_switch() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&switching(1, 5, FWD), limits(1 << 20, 10_000));
    let a = rig.publisher(5);
    let b = rig.publisher(8);
    let subscriber = rig.subscriber(6);

    let a_group = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 4);
    let a_stream = a.push_inbound_uni();
    a_stream.push(a_group[0].clone());
    a_stream.push(a_group[1].clone());
    settle().await;

    rig.install(&switching(2, 8, FWD));
    assert!(matches!(
        rig.cell(11).machine_state(),
        Some(EdgeState::SwitchPending { .. })
    ));
    a_stream.push(a_group[2].clone()); // still A's
    settle().await;
    let b_stream = b.push_inbound_uni();
    b_stream.push(stream_frame(
        50,
        9,
        UPLINK_VIDEO_SEND_ID,
        false,
        &[7; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    assert_eq!(
        subscriber.egress_uni_streams().len(),
        1,
        "B's delta is gated"
    );
    let b_start = b.push_inbound_uni();
    b_start.push(stream_frame(
        51,
        10,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[8; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    a_stream.push(a_group[3].clone()); // A is done for this edge
    settle().await;

    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams[0].terminal(),
        EgressTerminal::Finished,
        "A's group finished at its frame boundary, never reset"
    );
    let a_frames = parse_stream(&streams[0].bytes());
    let b_frames = parse_stream(&streams[1].bytes());
    assert_eq!(a_frames.len(), 3, "A kept until B's group start");
    assert_eq!(b_frames.len(), 1);
    let hops: Vec<u32> = a_frames.iter().chain(&b_frames).map(|f| f.hop).collect();
    assert_eq!(
        hops,
        vec![0, 1, 2, 3],
        "per-edge hop: contiguous across streams and sources"
    );
    assert!(b_frames.iter().all(|f| f.stream_id == VIDEO_SLOT));
}

#[tokio::test(start_paused = true)]
async fn the_switch_is_abandoned_and_a_kept_when_b_leaves_first() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&switching(1, 5, FWD), limits(1 << 20, 10_000));
    let a = rig.publisher(5);
    let _b = rig.publisher(8);
    let subscriber = rig.subscriber(6);
    let a_group = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 3);
    let a_stream = a.push_inbound_uni();
    a_stream.push(a_group[0].clone());
    settle().await;
    rig.install(&switching(2, 8, FWD));
    rig.disconnect_publisher(8).await;
    assert!(
        matches!(
            rig.cell(11).machine_state(),
            Some(EdgeState::SwitchPending { .. })
        ),
        "A kept, the switch still pending on B (MC still assigns it)"
    );
    a_stream.push(a_group[1].clone());
    a_stream.push(a_group[2].clone());
    a_stream.finish();
    settle().await;
    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 1);
    assert_eq!(
        parse_stream(&streams[0].bytes()).len(),
        3,
        "the slot did not go dark"
    );
}

/// R-17: a switch to a PAUSED source completes at once as "forward nothing",
/// cancelling the wait and stopping A — and a resume that beats MC's update
/// forwards from B's next group start.
#[tokio::test(start_paused = true)]
async fn a_switch_to_a_paused_source_completes_immediately() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&switching(1, 5, PAUSED), limits(1 << 20, 10_000));
    let a = rig.publisher(5);
    let b = rig.publisher(8);
    let subscriber = rig.subscriber(6);
    let a_group = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 3);
    let a_stream = a.push_inbound_uni();
    a_stream.push(a_group[0].clone());
    a_stream.push(a_group[1].clone());
    settle().await;
    rig.install(&switching(2, 8, PAUSED));
    settle().await;
    assert!(matches!(
        rig.cell(11).machine_state(),
        Some(EdgeState::Paused { .. })
    ));
    assert_eq!(
        rig.cell(11).waiting_with_source_seen(),
        None,
        "no keyframe wait"
    );
    assert_eq!(
        subscriber.egress_uni_streams()[0].terminal(),
        EgressTerminal::Finished,
        "A stopped at once, at its frame boundary"
    );
    a_stream.push(a_group[2].clone());
    settle().await;
    assert_eq!(
        parse_stream(&subscriber.egress_uni_streams()[0].bytes()).len(),
        2
    );
    // A non-start from the paused source first: nothing new opens.
    let b_delta = b.push_inbound_uni();
    b_delta.push(stream_frame(
        9,
        2,
        UPLINK_VIDEO_SEND_ID,
        false,
        &[0xB0; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    assert_eq!(
        subscriber.egress_uni_streams().len(),
        1,
        "paused: never mid-group"
    );
    // Then B's group start: the resume that beat MC's update forwards.
    let b_stream = b.push_inbound_uni();
    b_stream.push(stream_frame(
        10,
        3,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[0xB1; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 2, "the resume forwards");
    let resumed = parse_stream(&streams[1].bytes());
    assert!(resumed[0].group_start, "from B's group start");
    assert_eq!(resumed[0].stream_id, VIDEO_SLOT);
    assert_eq!(resumed[0].payload, vec![0xB1; 40], "B's frame");
}

/// ADR-0036 §10 re-assert idempotency: an identical or unrelated re-push hands
/// the SAME runtime cell over, with its state and next hop untouched, and the
/// group in flight continues on the same downlink.
#[tokio::test(start_paused = true)]
async fn a_re_push_preserves_edge_state_and_hop() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let group = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 3);
    let inbound = publisher.push_inbound_uni();
    inbound.push(group[0].clone());
    inbound.push(group[1].clone());
    settle().await;
    let cell = rig.cell(11);
    let (state, hop) = (cell.machine_state(), cell.peek_hop());

    rig.install(&mixed(2, FWD, &[6])); // identical content
    assert!(std::sync::Arc::ptr_eq(&cell, &rig.cell(11)));
    assert_eq!((cell.machine_state(), cell.peek_hop()), (state, hop));
    rig.install(&mixed(3, FWD, &[6, 7])); // unrelated edges added
    assert!(std::sync::Arc::ptr_eq(&cell, &rig.cell(11)));
    assert_eq!((cell.machine_state(), cell.peek_hop()), (state, hop));

    inbound.push(group[2].clone());
    inbound.push(group[3].clone());
    inbound.finish();
    settle().await;
    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 1, "the group continued on the same downlink");
    let hops: Vec<u32> = parse_stream(&streams[0].bytes())
        .iter()
        .map(|f| f.hop)
        .collect();
    assert_eq!(hops, vec![0, 1, 2, 3]);
}

/// The hop continues across group streams: ≥2 groups, one stream each.
#[tokio::test(start_paused = true)]
async fn the_downlink_hop_continues_across_group_streams() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    for (first, gop) in [(1_u32, 0_u32), (10, 1), (20, 2)] {
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(first, gop, UPLINK_VIDEO_SEND_ID, 2)));
        inbound.finish();
        settle().await;
    }
    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 3, "one downlink stream per group");
    let hops: Vec<u32> = streams
        .iter()
        .flat_map(|s| parse_stream(&s.bytes()))
        .map(|f| f.hop)
        .collect();
    assert_eq!(hops, (0..9).collect::<Vec<_>>(), "never per QUIC stream");
    assert!(streams
        .iter()
        .all(|s| s.priority() == mh_service::media::downlink::quic_priority(2)));
}

/// The stream-path attribution rows (extending task 8's table).
#[tokio::test(start_paused = true)]
async fn stream_path_attribution_rows() {
    // label 1 over a uni stream -> video slots only, cut-through.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 1)));
        inbound.finish();
        settle().await;
        let frames = parse_stream(&subscriber.egress_uni_streams()[0].bytes());
        assert!(
            frames.iter().all(|f| f.stream_id == VIDEO_SLOT),
            "never the audio slot"
        );
        metrics
            .counter("mh_media_group_starts_total")
            .with_labels(&[("direction", "ingress")])
            .assert_delta(1);
        metrics
            .counter("mh_media_group_starts_total")
            .with_labels(&[("direction", "egress")])
            .assert_delta(1);
    }
    // label 0 (the DATAGRAM audio stream) over a uni stream -> transport_mismatch.
    // undeclared label -> unmatched_stream. Both: counted once, stream stopped.
    for (label, reason) in [(0_u16, "transport_mismatch"), (9, "unmatched_stream")] {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(1, 0, label, 2)));
        inbound.finish();
        settle().await;
        assert_eq!(inbound.stopped(), Some(INGRESS_REFUSED), "{reason}");
        assert!(subscriber.egress_uni_streams().is_empty(), "{reason}");
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", reason)])
            .assert_delta(1);
        metrics
            .counter("mh_media_group_starts_total")
            .with_labels(&[("direction", "ingress")])
            .assert_delta(0);
        for reason in mh_service::observability::metrics::GroupResetReason::ALL {
            metrics
                .counter("mh_media_group_resets_total")
                .with_labels(&[("reason", reason.as_str())])
                .assert_delta(0);
        }
    }
    // label 1 while video is DROP_AT_INGRESS -> server_muted per frame, no video
    // slot receives anything, the stream KEPT reading; the sender's audio flows.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&mixed(1, DROP, &[6]), limits(1 << 20, 10_000));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 2)));
        settle().await;
        assert_eq!(
            inbound.stopped(),
            None,
            "kept reading so a lifted mute resumes"
        );
        assert!(subscriber.egress_uni_streams().is_empty());
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "server_muted")])
            .assert_delta(3);
        // Audio from the same sender, through the datagram forward path.
        let mut forwarder = mh_service::media::forwarder::ConnectionForwarder::new(
            rig.meeting.clone(),
            sender(5),
            std::sync::Arc::clone(&rig.handles),
            0.0,
            mh_service::config::NOMINAL_AUDIO_FRAME_BYTES,
        );
        let outcome = mh_service::media::forward::forward_one(
            &mut forwarder,
            &rig.routing.load(),
            &rig.subscribers.load(),
            mh_service::media::forward::IngressFrame {
                payload: common::media_frame::audio_datagram(1, 3),
                received_at: std::time::Instant::now(),
            },
        );
        assert_eq!(
            outcome.delivered, 1,
            "audio flows while video is server-muted"
        );
        // Lift the mute: the same stream resumes at its next group start.
        rig.install(&mixed(2, FWD, &[6]));
        inbound.push(concat(&stream_group(10, 1, UPLINK_VIDEO_SEND_ID, 0)));
        inbound.finish();
        settle().await;
        assert_eq!(subscriber.egress_uni_streams().len(), 1);
    }
    // label 1 while video is PAUSED -> forwarded AND counted as paused arrivals.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&mixed(1, PAUSED, &[6]), limits(1 << 20, 10_000));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 2)));
        inbound.finish();
        settle().await;
        assert_eq!(
            parse_stream(&subscriber.egress_uni_streams()[0].bytes()).len(),
            3
        );
        metrics
            .counter("mh_media_paused_source_frames_total")
            .assert_delta(3);
        metrics
            .counter(FORWARDED)
            .with_labels(&[("direction", "ingress")])
            .assert_delta(3);
    }
}

/// A group is one source: a later frame relabelled mid-stream is refused
/// (`unmatched_stream`, stopped) and what it fed is reset, never finished.
#[tokio::test(start_paused = true)]
async fn a_label_change_mid_stream_is_refused_and_resets_downstream() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let inbound = publisher.push_inbound_uni();
    inbound.push(stream_frame(
        1,
        0,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[1; 40],
        FrameShape::Minimal,
    ));
    settle().await; // the downlink is open before the relabelled frame
    inbound.push(stream_frame(2, 0, 0, false, &[2; 40], FrameShape::Minimal));
    settle().await;
    assert_eq!(inbound.stopped(), Some(INGRESS_REFUSED));
    let streams = subscriber.egress_uni_streams();
    assert!(streams[0].terminal().is_reset(), "reset, never finished");
    metrics
        .counter(DROPPED)
        .with_labels(&[("reason", "unmatched_stream")])
        .assert_delta(1);
    metrics
        .counter("mh_media_group_resets_total")
        .with_labels(&[("reason", "publisher_reset")])
        .assert_delta(1);
}

/// Audio slot refill regression (story 3 MH-A2 moved the hop to the edge):
/// MC keeps `egress_stream_id` stable across a refill A->C, and the hop must
/// continue — before this task it restarted from C's own counter.
#[tokio::test(start_paused = true)]
async fn an_audio_slot_refill_continues_the_hop() {
    let _metrics = MetricAssertion::snapshot();
    let declared = |candidate: u32, generation: u64| {
        register_request_declared(
            MEETING,
            generation,
            vec![egress_from(10, 6, 3, candidate, AUDIO)],
            vec![
                source_stream(5, AUDIO, DGRAM, FWD),
                source_stream(8, AUDIO, DGRAM, FWD),
            ],
        )
    };
    let rig = StreamRig::new(&declared(5, 1), limits(1 << 20, 10_000));
    let _subscriber = rig.subscriber(6);
    let queue_owner = rig.subscribers.load();
    let queue = queue_owner
        .egress_queue(&rig.meeting, sender(6))
        .expect("registered")
        .clone();
    let send = |from: u32, seq: u32| {
        let mut forwarder = mh_service::media::forwarder::ConnectionForwarder::new(
            rig.meeting.clone(),
            sender(from),
            std::sync::Arc::clone(&rig.handles),
            0.0,
            mh_service::config::NOMINAL_AUDIO_FRAME_BYTES,
        );
        mh_service::media::forward::forward_one(
            &mut forwarder,
            &rig.routing.load(),
            &rig.subscribers.load(),
            mh_service::media::forward::IngressFrame {
                payload: common::media_frame::audio_datagram(seq, 1),
                received_at: std::time::Instant::now(),
            },
        )
    };
    assert_eq!(send(5, 1).delivered, 1);
    assert_eq!(send(5, 2).delivered, 1);
    rig.install(&declared(8, 2));
    assert_eq!(send(8, 1).delivered, 1);
    let mut hops = Vec::new();
    while let Some(frame) = queue.try_pop() {
        hops.push(
            media_protocol::codec::decode_datagram(&frame.payload)
                .unwrap()
                .hop_sequence(),
        );
    }
    assert_eq!(hops, vec![0, 1, 2], "the refill continued the slot's hop");
}

/// Zero syscalls, proved rather than asserted (ADR-0036 §10): the whole
/// stream path — accept loop, group task, edge runtime, downlink writer — on
/// a runtime with a timer and NO I/O driver, where any tokio network or file
/// I/O would panic.
#[test]
fn the_stream_path_needs_no_io_driver() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap();
    runtime.block_on(async {
        let _metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 1000));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 2)));
        inbound.finish();
        settle().await;
        assert_eq!(
            parse_stream(&subscriber.egress_uni_streams()[0].bytes()).len(),
            3
        );
    });
}

/// Protocol: the hop continues across a switch while A's OLD downlink and B's
/// NEW one are BOTH live — A is mid-frame when B's group start lands.
#[tokio::test(start_paused = true)]
async fn the_hop_is_unique_while_both_downlinks_are_live_across_a_switch() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&switching(1, 5, FWD), limits(1 << 20, 10_000));
    let a = rig.publisher(5);
    let b = rig.publisher(8);
    let subscriber = rig.subscriber(6);
    let a_frames = stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 1);
    let a_stream = a.push_inbound_uni();
    a_stream.push(a_frames[0].clone());
    settle().await;
    rig.install(&switching(2, 8, FWD));
    // A's second frame: header plus part of its tail.
    let cut = a_frames[1].len() - 30;
    a_stream.push(a_frames[1].slice(..cut));
    settle().await;
    // B's group start lands while A is mid-frame.
    let b_stream = b.push_inbound_uni();
    b_stream.push(stream_frame(
        50,
        4,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[8; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    // The rest of A's tail.
    a_stream.push(a_frames[1].slice(cut..));
    settle().await;

    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 2);
    assert_eq!(
        streams[0].terminal(),
        EgressTerminal::Finished,
        "A's in-flight frame completes, then A's stream FINISHES at the boundary"
    );
    let a_out = parse_stream(&streams[0].bytes());
    let b_out = parse_stream(&streams[1].bytes());
    assert_eq!(a_out.len(), 2, "A's frame arrived whole");
    assert!(b_out[0].group_start, "B opens on its group start");
    let mut hops: Vec<u32> = a_out.iter().chain(&b_out).map(|f| f.hop).collect();
    hops.sort_unstable();
    assert_eq!(
        hops,
        vec![0, 1, 2],
        "unique and contiguous as a set, never repeated"
    );
}

/// Security/protocol: an independently-decodable frame inside ONE uplink
/// stream never opens another downlink — egress opens cannot exceed the
/// rate-limited ingress opens.
#[tokio::test(start_paused = true)]
async fn an_all_group_start_stream_opens_exactly_one_downlink() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let frames: Vec<Bytes> = (0..6)
        .map(|i| {
            stream_frame(
                i,
                i,
                UPLINK_VIDEO_SEND_ID,
                true,
                &[3; 40],
                FrameShape::Minimal,
            )
        })
        .collect();
    let inbound = publisher.push_inbound_uni();
    inbound.push(concat(&frames));
    inbound.finish();
    settle().await;
    let streams = subscriber.egress_uni_streams();
    assert_eq!(streams.len(), 1, "one uplink group, one downlink");
    assert_eq!(
        parse_stream(&streams[0].bytes()).len(),
        6,
        "every frame on it"
    );
}

/// ...and alternating group starts across two concurrent uplink streams open
/// at most ONE new downlink (when the newer stream starts); the older
/// stream's frames are skipped from then on.
#[tokio::test(start_paused = true)]
async fn alternating_group_starts_across_two_streams_open_at_most_one_more() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&mixed(1, FWD, &[6]), limits(1 << 20, 10_000));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let older = publisher.push_inbound_uni();
    older.push(stream_frame(
        1,
        1,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[1; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    let newer = publisher.push_inbound_uni();
    newer.push(stream_frame(
        2,
        2,
        UPLINK_VIDEO_SEND_ID,
        true,
        &[2; 40],
        FrameShape::Minimal,
    ));
    settle().await;
    for i in 0..6_u32 {
        let (stream, marker) = if i % 2 == 0 {
            (&older, 1_u8)
        } else {
            (&newer, 2_u8)
        };
        stream.push(stream_frame(
            10 + i,
            3 + i,
            UPLINK_VIDEO_SEND_ID,
            true,
            &[marker; 40],
            FrameShape::Minimal,
        ));
        settle().await;
    }
    older.finish();
    newer.finish();
    settle().await;
    let streams = subscriber.egress_uni_streams();
    assert_eq!(
        streams.len(),
        2,
        "the newer stream opened exactly one more downlink"
    );
    let newer_out = parse_stream(&streams[1].bytes());
    assert!(
        newer_out.iter().all(|f| f.payload[0] == 2),
        "the older stream's frames are skipped once the newer group started"
    );
    assert_eq!(newer_out.len(), 4);
}
