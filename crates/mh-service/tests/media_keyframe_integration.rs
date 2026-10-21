//! On-demand keyframes and receiver recovery (story 3 MH-B; R-4, R-9, R-10,
//! R-11, R-12, R-13, R-30; story Design §Wire freeze, Amendment 4), driven
//! end to end through the transport seam's deterministic shim: the real
//! stream ingress, edge runtime, per-source keyframe state, keyframe writer
//! and control-stream framer — tokio paused time (the injected clock),
//! current-thread runtime (`MetricAssertion` is thread-local).
//!
//! Every absence assertion is preceded or followed by a positive control in
//! the same test showing the observation channel delivers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use bytes::Bytes;
use common::media_frame::{stream_frame, stream_group, FrameShape, UPLINK_VIDEO_SEND_ID};
use common::stream_rig::{concat, limits, sender, settle, StreamRig, MEETING};
use mh_service::media::edge::EdgeState;
use mh_service::media::keyframe::{serve_control_message, ControlStep};
use mh_service::observability::metrics::{PendingExit, RecoveryOutcome};
use mh_service::routing::StreamNumber;
use mh_service::transport::framing::{encode_framed, FramedReader, CONTROL_PARSE_BYTES};
use mh_service::transport::INGRESS_REFUSED;
use mh_test_utils::media_policy::{register_request_declared, source_stream, stream_egress};
use mh_test_utils::transport_shim::{LossDelayTransport, ShimControlRecv};
use prost::Message;
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, SourceDisposition};
use proto_gen::dark_tower::signaling::v1::{
    mh_client_message, mh_server_message, MhClientMessage, MhConnectRequest, MhServerMessage,
    RecoveryRequest, TransportMode,
};
use std::time::Duration;
use tokio::time::advance;

const FWD: SourceDisposition = SourceDisposition::Forward;
const PAUSED: SourceDisposition = SourceDisposition::Paused;
const DROP: SourceDisposition = SourceDisposition::DropAtIngress;
const SPG: TransportMode = TransportMode::StreamPerGroup;
const VIDEO: u32 = 1;
const SLOT: u32 = 4;
const STALL_MS: u64 = 500;

const SENT: &str = "mh_media_keyframe_requests_sent_total";
const FAILED: &str = "mh_media_keyframe_request_write_failures_total";
const RECEIVED: &str = "mh_media_recovery_requests_received_total";
const EXITS: &str = "mh_media_edge_pending_exits_total";
const DURATION: &str = "mh_media_edge_pending_duration_seconds";
const PENDING: &str = "mh_media_edges_pending";
const OLDEST: &str = "mh_media_edge_pending_oldest_seconds";

fn video() -> StreamNumber {
    StreamNumber::from_wire(VIDEO).unwrap()
}

/// Publisher 5's video (`STREAM_PER_GROUP`, `disposition`) feeds slot 4 of
/// each subscriber; edge ids 10, 11, ... in subscriber order.
fn video_policy(
    generation: u64,
    disposition: SourceDisposition,
    subscribers: &[u32],
) -> RegisterMeetingRequest {
    let edges = subscribers
        .iter()
        .enumerate()
        .map(|(i, sub)| stream_egress(10 + u32::try_from(i).unwrap(), *sub, SLOT, 5, VIDEO))
        .collect();
    register_request_declared(
        MEETING,
        generation,
        edges,
        vec![source_stream(5, VIDEO, SPG, disposition)],
    )
}

/// Two video publishers (5 = A, 8 = B); subscriber 6's slot 4 (edge 10) fed
/// by `candidate`.
fn switching(generation: u64, candidate: u32) -> RegisterMeetingRequest {
    register_request_declared(
        MEETING,
        generation,
        vec![stream_egress(10, 6, SLOT, candidate, VIDEO)],
        vec![
            source_stream(5, VIDEO, SPG, FWD),
            source_stream(8, VIDEO, SPG, FWD),
        ],
    )
}

/// Every `KeyframeRequest` MH wrote to this publisher, as (stream, counter),
/// in order. Each outbound record is one framed `MhServerMessage`.
fn requests(publisher: &LossDelayTransport) -> Vec<(u32, u32)> {
    publisher
        .control_outbound()
        .iter()
        .map(|framed| {
            let message = MhServerMessage::decode(&framed[4..]).expect("framed MhServerMessage");
            assert_eq!(message.trace_parent, "", "MH sets no trace context");
            assert_eq!(message.trace_state, "");
            match message.message.expect("a non-empty MhServerMessage") {
                mh_server_message::Message::KeyframeRequest(r) => (r.stream_number, r.gop_counter),
            }
        })
        .collect()
}

fn delta(seq: u32, gop: u32) -> Bytes {
    stream_frame(
        seq,
        gop,
        UPLINK_VIDEO_SEND_ID,
        false,
        &[7; 40],
        FrameShape::Minimal,
    )
}

fn framed(message: &MhClientMessage) -> Bytes {
    Bytes::from(encode_framed(&message.encode_to_vec()).unwrap())
}

fn recovery(slot: u32, sender_id: u32, gop_counter: u32) -> Bytes {
    framed(&MhClientMessage {
        message: Some(mh_client_message::Message::RecoveryRequest(
            RecoveryRequest {
                slot_id: slot,
                sender_id,
                gop_counter,
            },
        )),
        ..MhClientMessage::default()
    })
}

/// A subscriber connection's control-stream reader, as its lifecycle loop
/// holds it.
struct Control {
    shim: LossDelayTransport,
    reader: FramedReader<ShimControlRecv>,
    buf: [u8; CONTROL_PARSE_BYTES],
    own: u32,
}

impl Control {
    fn new(shim: &LossDelayTransport, own: u32) -> Self {
        Self {
            shim: shim.clone(),
            reader: FramedReader::new(shim.control_halves().1),
            buf: [0; CONTROL_PARSE_BYTES],
            own,
        }
    }

    /// Push one raw item from the client and run one per-message step.
    async fn send(&mut self, rig: &StreamRig, bytes: Bytes) -> ControlStep {
        self.shim.push_control_inbound(bytes);
        let step = serve_control_message(
            &mut self.reader,
            &mut self.buf,
            &rig.routing,
            &rig.meeting,
            sender(self.own),
            &rig.handles,
        )
        .await;
        settle().await;
        step
    }
}

/// Cumulative expected recovery outcomes against ONE snapshot taken before
/// the rig (handles resolve against the recorder current at rig build).
#[derive(Default)]
struct Tally([u64; 7]);

impl Tally {
    /// One more `outcome`; then every outcome must read exactly its tally —
    /// the new one moved by one and every other stayed.
    fn expect(&mut self, metrics: &MetricSnapshot, outcome: RecoveryOutcome) {
        let at = RecoveryOutcome::ALL
            .iter()
            .position(|o| *o == outcome)
            .unwrap();
        self.0[at] += 1;
        self.check(metrics);
    }

    fn check(&self, metrics: &MetricSnapshot) {
        for (i, other) in RecoveryOutcome::ALL.iter().enumerate() {
            assert_eq!(
                metrics
                    .counter(RECEIVED)
                    .with_labels(&[("outcome", other.as_str())])
                    .delta(),
                self.0[i],
                "{}",
                other.as_str()
            );
        }
    }
}

fn assert_only_exit(metrics: &MetricSnapshot, exit: PendingExit, n: u64) {
    for other in PendingExit::ALL {
        let expected = if other == exit { n } else { 0 };
        assert_eq!(
            metrics
                .counter(EXITS)
                .with_labels(&[("outcome", other.as_str())])
                .delta(),
            expected,
            "{}",
            other.as_str()
        );
    }
}

// ---------------------------------------------------------------------------
// R-10: MH-originated requests
// ---------------------------------------------------------------------------

/// A late joiner requests ONLY after a frame from the source was seen, at
/// MH's newest counter, once; the clock advancing emits nothing more.
#[tokio::test(start_paused = true)]
async fn a_request_follows_a_seen_frame_once_and_never_repeats() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    settle().await;
    assert!(requests(&publisher).is_empty(), "never seen: no request");

    // A mid-group frame arrives: MH now has a counter, the edge still waits.
    let stream = publisher.push_inbound_uni();
    stream.push(delta(1, 3));
    settle().await;
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 3)],
        "exactly that stream, MH's counter"
    );
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "initial")])
        .assert_delta(1);

    // More deltas at the same counter, and an hour of clock: nothing more.
    stream.push(delta(2, 3));
    stream.push(delta(3, 3));
    settle().await;
    advance(Duration::from_secs(3600)).await;
    settle().await;
    assert_eq!(requests(&publisher).len(), 1, "never repeated, no timer");
    assert_eq!(
        rig.cell(10).awaiting_group_start().map(|s| s.sender),
        Some(sender(5)),
        "still waiting: no give-up timeout"
    );
}

/// An edge waiting on a seen source whose NEXT frame is the group start
/// requests nothing: the request is judged after the machine consumed it.
#[tokio::test(start_paused = true)]
async fn a_group_start_arriving_for_a_waiting_edge_requests_nothing() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let stream = publisher.push_inbound_uni();
    stream.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 2)));
    settle().await;
    assert!(
        requests(&publisher).is_empty(),
        "the group start needed no request"
    );
    assert!(matches!(
        rig.cell(10).machine_state(),
        Some(EdgeState::Forwarding { .. })
    ));
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "initial")])
        .assert_delta(0);

    // Positive control, same rig: a second subscriber joining mid-group asks.
    rig.install(&video_policy(2, FWD, &[6, 7]));
    let _late = rig.subscriber(7);
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 0)]);
}

/// The counter is read from forwarded, server-muted and unrouted frames
/// alike, and is the MAXIMUM read (a late frame of an older group never
/// regresses it).
#[tokio::test(start_paused = true)]
async fn the_counter_comes_from_every_declared_frame_and_never_regresses() {
    let _metrics = MetricAssertion::snapshot();
    // Unrouted: declared, no edges.
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    publisher.push_inbound_uni().push(delta(1, 4));
    settle().await;
    assert_eq!(
        rig.keyframes(5).unwrap().newest(video()),
        Some(4),
        "unrouted"
    );
    // Server-muted.
    rig.install(&video_policy(2, DROP, &[6]));
    publisher.push_inbound_uni().push(delta(2, 5));
    settle().await;
    assert_eq!(rig.keyframes(5).unwrap().newest(video()), Some(5), "muted");
    // Forwarded, then a late frame of an older group.
    rig.install(&video_policy(3, FWD, &[6]));
    let _subscriber = rig.subscriber(6);
    let newer = publisher.push_inbound_uni();
    newer.push(concat(&stream_group(3, 7, UPLINK_VIDEO_SEND_ID, 0)));
    publisher.push_inbound_uni().push(delta(4, 6));
    settle().await;
    assert_eq!(
        rig.keyframes(5).unwrap().newest(video()),
        Some(7),
        "max, not last"
    );
    // An undeclared label creates nothing (bounded by declarations).
    let other = publisher.push_inbound_uni();
    other.push(stream_frame(5, 9, 9, true, &[1; 8], FrameShape::Minimal));
    settle().await;
    assert_eq!(rig.keyframes(5).unwrap().record_count(), 1);
}

/// Abandonment — MH's backlog reset, a propagated publisher reset, a stall
/// reset — puts the edge back in Waiting, which requests; a later loss at a
/// newer counter is a new request.
#[tokio::test(start_paused = true)]
async fn every_abandonment_recovers_through_a_request_via_waiting() {
    let _metrics = MetricAssertion::snapshot();
    // Publisher reset, then a stall reset at a newer counter.
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let first = publisher.push_inbound_uni();
    first.push(concat(&stream_group(1, 2, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    assert!(requests(&publisher).is_empty());
    first.reset(1);
    settle().await;
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 2)],
        "publisher reset → request"
    );

    let second = publisher.push_inbound_uni();
    let group = stream_group(3, 3, UPLINK_VIDEO_SEND_ID, 1);
    second.push(group[0].clone());
    second.push(group[1].slice(..group[1].len() - 10));
    settle().await;
    advance(Duration::from_millis(STALL_MS)).await;
    settle().await;
    assert_eq!(second.stopped(), Some(INGRESS_REFUSED));
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 2), (VIDEO, 3)],
        "stall → a new request at the newer counter"
    );

    // Backlog reset.
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(256, STALL_MS));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    subscriber.set_stream_credit(0);
    let stream = publisher.push_inbound_uni();
    stream.push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 6)));
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 4)], "backlog → request");
}

/// A stream refused at the open-rate limit has its header read and repairs
/// its source by request — immediately if the forwarded group already
/// ended, at the old group's end if it is still live (old frames intact).
#[tokio::test(start_paused = true)]
async fn a_refused_group_is_repaired_by_request_in_both_branches() {
    for live in [false, true] {
        let metrics = MetricAssertion::snapshot();
        let mut stream_limits = limits(1 << 20, STALL_MS);
        stream_limits.opens_per_connection_per_sec = 1;
        let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), stream_limits);
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let old = publisher.push_inbound_uni();
        let old_group = stream_group(1, 1, UPLINK_VIDEO_SEND_ID, 2);
        if live {
            // Mid-frame: the second frame's tail is still to come.
            old.push(old_group[0].clone());
            old.push(old_group[1].slice(..old_group[1].len() - 5));
        } else {
            old.push(concat(&old_group));
            old.finish();
        }
        settle().await;
        let refused = publisher.push_inbound_uni();
        refused.push(concat(&stream_group(4, 2, UPLINK_VIDEO_SEND_ID, 1)));
        settle().await;
        assert_eq!(refused.stopped(), Some(INGRESS_REFUSED));
        metrics
            .counter("mh_media_frames_dropped_total")
            .with_labels(&[("reason", "stream_rate_limited")])
            .assert_delta(1);
        if live {
            assert!(
                requests(&publisher).is_empty(),
                "the old group is still live"
            );
            old.push(old_group[1].slice(old_group[1].len() - 5..));
            old.push(old_group[2].clone());
            old.finish();
            settle().await;
            let frames =
                common::media_frame::parse_stream(&subscriber.egress_uni_streams()[0].bytes());
            assert_eq!(frames.len(), 3, "the old group's frames delivered intact");
        }
        assert_eq!(
            requests(&publisher),
            vec![(VIDEO, 2)],
            "exactly one request, at the refused group's counter"
        );
        assert!(matches!(
            rig.cell(10).machine_state(),
            Some(EdgeState::Waiting { .. })
        ));
    }
}

/// An over-limit stream whose header is malformed is stopped with no
/// counter recorded and no request.
#[tokio::test(start_paused = true)]
async fn a_malformed_over_limit_header_records_nothing() {
    // Precondition: the bytes really are a malformed header (not a header
    // with an undeclared label, which would also record nothing).
    assert!(
        media_protocol::reader::HeaderReader::new()
            .feed(&[0xFF; 32])
            .is_err(),
        "precondition: the over-limit bytes fail the header reader"
    );
    let metrics = MetricAssertion::snapshot();
    let mut stream_limits = limits(1 << 20, STALL_MS);
    stream_limits.opens_per_connection_per_sec = 1;
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), stream_limits);
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let admitted = publisher.push_inbound_uni();
    admitted.push(concat(&stream_group(1, 1, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    let refused = publisher.push_inbound_uni();
    refused.push(Bytes::from_static(&[0xFF; 32]));
    settle().await;
    assert_eq!(refused.stopped(), Some(INGRESS_REFUSED));
    assert_eq!(
        rig.keyframes(5).unwrap().newest(video()),
        Some(1),
        "unchanged"
    );
    // A FIN before any header completes records nothing either.
    let headerless = publisher.push_inbound_uni();
    headerless.push(delta(9, 7).slice(..5));
    headerless.finish();
    settle().await;
    assert_eq!(headerless.stopped(), Some(INGRESS_REFUSED));
    assert_eq!(rig.keyframes(5).unwrap().newest(video()), Some(1));
    assert!(requests(&publisher).is_empty());
    metrics
        .counter("mh_media_frames_dropped_total")
        .with_labels(&[("reason", "stream_rate_limited")])
        .assert_delta(2);
    // Positive control, same rig: a well-formed over-limit header records.
    let well_formed = publisher.push_inbound_uni();
    well_formed.push(delta(10, 8));
    settle().await;
    assert_eq!(
        rig.keyframes(5).unwrap().newest(video()),
        Some(8),
        "recorded"
    );
}

/// An edge whose subscriber is not connected here neither requests nor
/// counts as pending, however many groups arrive; when it connects, it does.
#[tokio::test(start_paused = true)]
async fn an_edge_without_a_local_subscriber_never_requests() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    for gop in 0..5 {
        let s = publisher.push_inbound_uni();
        s.push(concat(&stream_group(
            gop * 3 + 1,
            gop,
            UPLINK_VIDEO_SEND_ID,
            1,
        )));
        s.finish();
        settle().await;
    }
    assert!(requests(&publisher).is_empty(), "no request loop");
    rig.routing.publish_pending(tokio::time::Instant::now());
    metrics.gauge(PENDING).assert_value(0.0);

    // Positive control: the subscriber connects mid-group.
    publisher.push_inbound_uni().push(delta(20, 5));
    settle().await;
    let _subscriber = rig.subscriber(6);
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 5)]);
    rig.routing.publish_pending(tokio::time::Instant::now());
    metrics.gauge(PENDING).assert_value(1.0);
}

// ---------------------------------------------------------------------------
// The newest-only queue
// ---------------------------------------------------------------------------

/// Behind a blocked write a newer counter replaces the queued older one and
/// only the newer is written; a publisher that never drains stalls nothing.
#[tokio::test(start_paused = true)]
async fn a_newer_request_replaces_a_queued_one_behind_a_blocked_write() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6, 7]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    publisher.block_control_writes();
    let first = publisher.push_inbound_uni();
    first.push(delta(1, 1));
    let _a = rig.subscriber(6);
    settle().await;
    assert_eq!(
        publisher.control_write_attempts(),
        1,
        "the write for 1 is blocked"
    );
    // Abandon and re-wait at counter 2 (queued), then 3 (replaces it).
    publisher.push_inbound_uni().push(delta(2, 2));
    settle().await;
    assert!(rig.keyframes(5).unwrap().has_queued(video()));
    publisher.push_inbound_uni().push(delta(3, 3));
    settle().await;
    // Media keeps flowing while the writer is blocked.
    let _b = rig.subscriber(7);
    let group = publisher.push_inbound_uni();
    group.push(concat(&stream_group(4, 4, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    assert!(matches!(
        rig.cell(10).machine_state(),
        Some(EdgeState::Forwarding { .. })
    ));
    assert!(
        !rig.keyframes(5).unwrap().has_queued(video()),
        "the group start discarded the queued request"
    );
    publisher.release_control_writes();
    settle().await;
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 1)],
        "only what was in flight"
    );
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "initial")])
        .assert_delta(1);

    // The replacement itself: blocked again, 5 queued, 6 replaces it.
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    publisher.block_control_writes();
    publisher.push_inbound_uni().push(delta(1, 4));
    let _a = rig.subscriber(6);
    settle().await; // 4 in flight
    publisher.push_inbound_uni().push(delta(2, 5));
    settle().await; // 5 queued
    publisher.push_inbound_uni().push(delta(3, 6));
    settle().await; // 6 replaces 5
    publisher.release_control_writes();
    settle().await;
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 4), (VIDEO, 6)],
        "5 never written"
    );
}

/// A queued unsent request is discarded on edge removal, PAUSED,
/// DROP_AT_INGRESS and unbind (the group-start case is above).
#[tokio::test(start_paused = true)]
async fn a_queued_request_is_discarded_when_nothing_wants_it() {
    for case in ["edge_removed", "paused", "drop_at_ingress", "unbind"] {
        let _metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&video_policy(1, FWD, &[6, 7]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        publisher.block_control_writes();
        publisher.push_inbound_uni().push(delta(1, 1));
        let _a = rig.subscriber(6);
        settle().await; // 1 in flight (blocked)
        publisher.push_inbound_uni().push(delta(2, 2));
        let _b = rig.subscriber(7);
        settle().await; // 2 queued
        let keyframes = rig.keyframes(5).unwrap();
        assert!(keyframes.has_queued(video()), "{case}: positive control");
        match case {
            "edge_removed" => rig.install(&video_policy(2, FWD, &[])),
            "paused" => rig.install(&video_policy(2, PAUSED, &[6, 7])),
            "drop_at_ingress" => rig.install(&video_policy(2, DROP, &[6, 7])),
            _ => rig.disconnect_publisher(5).await,
        }
        settle().await;
        if case == "unbind" {
            assert!(rig.keyframes(5).is_none(), "unbind: unregistered");
            assert_eq!(
                std::sync::Arc::strong_count(&keyframes),
                1,
                "unbind: the writer and ingress let go; the state is freed with them"
            );
        } else {
            assert!(!keyframes.has_queued(video()), "{case}: discarded");
        }
        publisher.release_control_writes();
        settle().await;
        // Exact: the blocked in-flight write drained (the writer resumed) and
        // nothing else; after unbind the writer was cancelled mid-write, so
        // nothing at all reached the stream.
        let expected: Vec<(u32, u32)> = if case == "unbind" {
            vec![]
        } else {
            vec![(VIDEO, 1)]
        };
        assert_eq!(requests(&publisher), expected, "{case}: 2 never written");
    }
}

/// A failed control-stream write is counted, never re-emitted, and every
/// later request to that publisher is counted, not queued.
#[tokio::test(start_paused = true)]
async fn a_failed_write_is_counted_and_never_re_emitted() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    publisher.fail_control_writes();
    publisher.push_inbound_uni().push(delta(1, 1));
    let _a = rig.subscriber(6);
    settle().await;
    metrics.counter(FAILED).assert_delta(1);
    publisher.push_inbound_uni().push(delta(2, 2));
    settle().await;
    metrics.counter(FAILED).assert_delta(2);
    assert!(!rig.keyframes(5).unwrap().has_queued(video()));
    assert!(requests(&publisher).is_empty());
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "initial")])
        .assert_delta(0);
}

// ---------------------------------------------------------------------------
// Per-source state lifetime
// ---------------------------------------------------------------------------

/// Freed on unassign and unbind; a rebind (R-16 reissue or reconnect)
/// starts clean, so its request at the same counter is not suppressed.
#[tokio::test(start_paused = true)]
async fn per_source_state_is_freed_and_a_rebind_starts_clean() {
    let _metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    publisher.push_inbound_uni().push(delta(1, 3));
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 3)]);
    let keyframes = rig.keyframes(5).unwrap();
    assert_eq!(keyframes.record_count(), 1);

    // Unassign: MC stops declaring the stream.
    rig.install(&register_request_declared(MEETING, 2, vec![], vec![]));
    assert_eq!(keyframes.record_count(), 0, "freed on unassign");

    // Unbind, then a fresh binding at the SAME counter requests again.
    rig.install(&video_policy(3, FWD, &[6]));
    rig.disconnect_publisher(5).await;
    assert!(rig.keyframes(5).is_none(), "freed on unbind");
    let again = rig.publisher(5);
    again.push_inbound_uni().push(delta(1, 3));
    settle().await;
    assert_eq!(
        requests(&again),
        vec![(VIDEO, 3)],
        "not suppressed by the old binding"
    );
}

// ---------------------------------------------------------------------------
// R-13: receiver recovery requests
// ---------------------------------------------------------------------------

/// Every outcome, each asserting its own delta and that every other outcome
/// stayed at zero.
#[tokio::test(start_paused = true)]
async fn every_recovery_outcome_is_counted_alone() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let stream = publisher.push_inbound_uni();
    stream.push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    let mut control = Control::new(&subscriber, 6);
    let cases: Vec<(Bytes, RecoveryOutcome)> = vec![
        (recovery(SLOT, 5, 3), RecoveryOutcome::Stale),
        (recovery(SLOT, 5, 5), RecoveryOutcome::CounterAhead),
        (recovery(SLOT, 8, 4), RecoveryOutcome::SourceMismatch),
        (recovery(SLOT, 0, 4), RecoveryOutcome::SourceMismatch),
        (recovery(9, 5, 4), RecoveryOutcome::UnmappedSlot),
        (
            Bytes::from_static(&[0, 0, 0, 2, 0xFF, 0xFF]),
            RecoveryOutcome::Malformed,
        ),
        (recovery(SLOT, 5, 4), RecoveryOutcome::Forwarded),
        (recovery(SLOT, 5, 4), RecoveryOutcome::Duplicate),
    ];
    for (bytes, outcome) in cases {
        assert!(matches!(
            control.send(&rig, bytes).await,
            ControlStep::Continue
        ));
        tally.expect(&metrics, outcome);
    }
    assert_eq!(
        requests(&publisher),
        vec![(VIDEO, 4)],
        "one upstream request, remapped: the source's stream, MH's counter"
    );
}

/// A second `connect_request` after connect is malformed and does nothing
/// else; an oversize well-framed body is drained and malformed (framing kept);
/// a framing break is malformed once and ends control reading.
#[tokio::test(start_paused = true)]
async fn non_recovery_messages_and_framing_breaks_are_malformed() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    publisher
        .push_inbound_uni()
        .push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    let mut control = Control::new(&subscriber, 6);

    let connect = framed(&MhClientMessage {
        message: Some(mh_client_message::Message::ConnectRequest(
            MhConnectRequest {
                join_token: "x".repeat(10),
            },
        )),
        ..MhClientMessage::default()
    });
    assert!(matches!(
        control.send(&rig, connect).await,
        ControlStep::Continue
    ));
    tally.expect(&metrics, RecoveryOutcome::Malformed);
    assert!(
        rig.keyframes(5).is_some(),
        "a second connect rebinds nothing"
    );

    let oversize = Bytes::from(encode_framed(&vec![0x0A; CONTROL_PARSE_BYTES + 1]).unwrap());
    assert!(matches!(
        control.send(&rig, oversize).await,
        ControlStep::Continue
    ));
    tally.expect(&metrics, RecoveryOutcome::Malformed);
    // Framing kept: the next message is judged normally.
    assert!(matches!(
        control.send(&rig, recovery(SLOT, 5, 4)).await,
        ControlStep::Continue
    ));
    tally.expect(&metrics, RecoveryOutcome::Forwarded);

    let step = control
        .send(&rig, Bytes::from_static(&[0xFF, 0xFF, 0xFF, 0xFF]))
        .await;
    assert!(matches!(step, ControlStep::FramingBroken));
    tally.expect(&metrics, RecoveryOutcome::Malformed);

    // A clean FIN is not a message and is not counted.
    let mut fresh = Control::new(&LossDelayTransport::new(), 6);
    fresh.shim.finish_control_inbound();
    let step = serve_control_message(
        &mut fresh.reader,
        &mut fresh.buf,
        &rig.routing,
        &rig.meeting,
        sender(6),
        &rig.handles,
    )
    .await;
    assert!(matches!(step, ControlStep::Finished));
    tally.check(&metrics);
}

/// Repeated, multi-receiver and after-MH's-own requests yield ONE upstream
/// request per (source stream, counter).
#[tokio::test(start_paused = true)]
async fn one_upstream_request_per_counter_whoever_asks() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6, 7]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let six = rig.subscriber(6);
    let seven = rig.subscriber(7);
    publisher
        .push_inbound_uni()
        .push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    let mut c6 = Control::new(&six, 6);
    let mut c7 = Control::new(&seven, 7);
    c6.send(&rig, recovery(SLOT, 5, 4)).await;
    c6.send(&rig, recovery(SLOT, 5, 4)).await;
    c7.send(&rig, recovery(SLOT, 5, 4)).await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 4)]);
    metrics
        .counter(RECEIVED)
        .with_labels(&[("outcome", "forwarded")])
        .assert_delta(1);
    metrics
        .counter(RECEIVED)
        .with_labels(&[("outcome", "duplicate")])
        .assert_delta(2);
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "receiver_recovery")])
        .assert_delta(1);

    // After MH's OWN request at a counter, a receiver's is a duplicate.
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let six = rig.subscriber(6);
    publisher.push_inbound_uni().push(delta(1, 9));
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 9)], "MH's own");
    let mut c6 = Control::new(&six, 6);
    c6.send(&rig, recovery(SLOT, 5, 9)).await;
    tally.expect(&metrics, RecoveryOutcome::Duplicate);
    assert_eq!(requests(&publisher).len(), 1);
}

/// SWITCH-RACE: slot moves A→B; a request carrying A's sender and A's
/// newest counter is `source_mismatch` and nothing goes to either publisher;
/// B's sender with A's counter value is judged in B's counter namespace.
#[tokio::test(start_paused = true)]
async fn switch_race_requests_are_judged_against_the_assigned_source() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&switching(1, 5), limits(1 << 20, STALL_MS));
    let a = rig.publisher(5);
    let b = rig.publisher(8);
    let subscriber = rig.subscriber(6);
    let a_stream = a.push_inbound_uni();
    a_stream.push(concat(&stream_group(1, 10, UPLINK_VIDEO_SEND_ID, 1)));
    let b_stream = b.push_inbound_uni();
    b_stream.push(concat(&stream_group(1, 3, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    rig.install(&switching(2, 8));
    settle().await;
    // The switch to B (seen at 3) requested once from B — MH's own.
    assert_eq!(requests(&b), vec![(VIDEO, 3)]);
    assert!(requests(&a).is_empty());
    let mut control = Control::new(&subscriber, 6);

    control.send(&rig, recovery(SLOT, 5, 10)).await;
    tally.expect(&metrics, RecoveryOutcome::SourceMismatch);
    assert!(requests(&a).is_empty(), "nothing upstream to A");
    assert_eq!(requests(&b).len(), 1, "B asked for nothing more");

    // B's sender with A's counter value: B's namespace (B is at 3).
    control.send(&rig, recovery(SLOT, 8, 10)).await;
    tally.expect(&metrics, RecoveryOutcome::CounterAhead);
    control.send(&rig, recovery(SLOT, 8, 2)).await;
    tally.expect(&metrics, RecoveryOutcome::Stale);
    // Equal to B's newest but already requested (by MH): duplicate.
    control.send(&rig, recovery(SLOT, 8, 3)).await;
    tally.expect(&metrics, RecoveryOutcome::Duplicate);
    // B advances while the edge still waits: MH's own request asks for 4
    // first (the edge wants B), so the receiver's equal request is a
    // duplicate — and nothing ever reaches A.
    b.push_inbound_uni().push(delta(9, 4));
    settle().await;
    assert_eq!(requests(&b), vec![(VIDEO, 3), (VIDEO, 4)]);
    control.send(&rig, recovery(SLOT, 8, 4)).await;
    tally.expect(&metrics, RecoveryOutcome::Duplicate);
    assert_eq!(requests(&b).len(), 2);
    assert!(requests(&a).is_empty());
}

/// Forwarded arm of the equality rule, for a slot that is NOT waiting (its
/// edge forwards): equal and not yet requested is forwarded, upstream as
/// MH's own message with the source's stream and MH's counter.
#[tokio::test(start_paused = true)]
async fn an_equal_unrequested_counter_is_forwarded_with_mh_fields() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let subscriber = rig.subscriber(6);
    let stream = publisher.push_inbound_uni();
    stream.push(concat(&stream_group(1, 6, UPLINK_VIDEO_SEND_ID, 2)));
    settle().await;
    assert!(
        requests(&publisher).is_empty(),
        "positive control: no request yet"
    );
    let mut control = Control::new(&subscriber, 6);
    control.send(&rig, recovery(SLOT, 5, 6)).await;
    tally.expect(&metrics, RecoveryOutcome::Forwarded);
    assert_eq!(requests(&publisher), vec![(VIDEO, 6)]);
    metrics
        .counter(SENT)
        .with_labels(&[("trigger", "receiver_recovery")])
        .assert_delta(1);
}

// ---------------------------------------------------------------------------
// R-4: pending
// ---------------------------------------------------------------------------

/// Each exit outcome, one test arm each: the gauge returns to zero, exactly
/// one exit of that outcome, and one duration observation.
#[tokio::test(start_paused = true)]
async fn every_pending_exit_is_counted_once_and_the_gauge_returns_to_zero() {
    // `paused_forward_nothing` covers BOTH client pause and server mute.
    let arms = PendingExit::ALL
        .into_iter()
        .map(|exit| (exit, PAUSED))
        .chain([(PendingExit::PausedForwardNothing, DROP)]);
    for (exit, nothing) in arms {
        let metrics = MetricAssertion::snapshot();
        let policy = if exit == PendingExit::Superseded {
            switching(1, 5)
        } else {
            video_policy(1, FWD, &[6])
        };
        let mut rig = StreamRig::new(&policy, limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let _b = if exit == PendingExit::Superseded {
            Some(rig.publisher(8))
        } else {
            None
        };
        let (_subscriber, cancel) = rig.subscriber_with_cancel(6);
        settle().await;
        advance(Duration::from_secs(2)).await;
        rig.routing.publish_pending(tokio::time::Instant::now());
        metrics.gauge(PENDING).assert_value(1.0);
        metrics.gauge(OLDEST).assert_value(2.0);
        match exit {
            PendingExit::GroupStart => {
                publisher.push_inbound_uni().push(concat(&stream_group(
                    1,
                    0,
                    UPLINK_VIDEO_SEND_ID,
                    0,
                )));
            }
            PendingExit::AbandonedSourceLeft => rig.disconnect_publisher(5).await,
            PendingExit::Superseded => {
                rig.install(&switching(2, 8));
                // Re-entered on B; end that episode too, by removal.
                settle().await;
                rig.install(&register_request_declared(
                    MEETING,
                    3,
                    vec![],
                    vec![
                        source_stream(5, VIDEO, SPG, FWD),
                        source_stream(8, VIDEO, SPG, FWD),
                    ],
                ));
            }
            PendingExit::PausedForwardNothing => rig.install(&video_policy(2, nothing, &[6])),
            PendingExit::EdgeRemoved => rig.disconnect_subscriber(6, &cancel),
        }
        settle().await;
        // The histogram FIRST: a snapshot read drains histogram entries.
        if exit == PendingExit::Superseded {
            metrics.histogram(DURATION).assert_observation_count(2);
            // A→B is superseded; B's episode then ends with its edge removed.
            assert_eq!(
                metrics
                    .counter(EXITS)
                    .with_labels(&[("outcome", "superseded")])
                    .delta(),
                1
            );
            assert_eq!(
                metrics
                    .counter(EXITS)
                    .with_labels(&[("outcome", "edge_removed")])
                    .delta(),
                1
            );
        } else {
            metrics.histogram(DURATION).assert_observation_count(1);
            assert_only_exit(&metrics, exit, 1);
        }
        rig.routing.publish_pending(tokio::time::Instant::now());
        metrics.gauge(PENDING).assert_value(0.0);
        metrics.gauge(OLDEST).assert_value(0.0);
    }
}

/// A never-sending source still counts as pending (it is assigned and its
/// publisher is here) and causes zero requests.
#[tokio::test(start_paused = true)]
async fn a_never_sending_source_is_pending_and_requests_nothing() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6, 7]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _six = rig.subscriber(6);
    let _seven = rig.subscriber(7);
    advance(Duration::from_secs(60)).await;
    rig.routing.publish_pending(tokio::time::Instant::now());
    metrics.gauge(PENDING).assert_value(2.0);
    metrics.gauge(OLDEST).assert_value(60.0);
    assert!(requests(&publisher).is_empty());
    // Positive control: the reader is alive — a mid-group frame asks once.
    publisher.push_inbound_uni().push(delta(1, 0));
    settle().await;
    assert_eq!(requests(&publisher), vec![(VIDEO, 0)]);
}

// ---------------------------------------------------------------------------
// Security SEC-R1: the refused-group walk is coalesced
// ---------------------------------------------------------------------------

/// After ONE refusal walk, further over-limit streams of the same source run
/// no further walk, yet still request each new counter while the edge waits;
/// the honest chain (refused, refused, ..., admitted) ends forwarding with one
/// request per refused counter.
#[tokio::test(start_paused = true)]
async fn refused_group_walks_are_coalesced_to_one_per_admitted_group() {
    let _metrics = MetricAssertion::snapshot();
    let mut stream_limits = limits(1 << 20, STALL_MS);
    stream_limits.opens_per_connection_per_sec = 1;
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), stream_limits);
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let admitted = publisher.push_inbound_uni();
    admitted.push(concat(&stream_group(1, 1, UPLINK_VIDEO_SEND_ID, 1)));
    admitted.finish();
    settle().await;
    let keyframes = rig.keyframes(5).unwrap();
    assert_eq!(keyframes.refusal_walks(), 0);

    for counter in 2..=6 {
        let refused = publisher.push_inbound_uni();
        refused.push(delta(counter * 10, counter));
        settle().await;
        assert_eq!(refused.stopped(), Some(INGRESS_REFUSED));
    }
    assert_eq!(keyframes.refusal_walks(), 1, "one walk, then O(1) refusals");
    assert_eq!(
        requests(&publisher),
        (2..=6).map(|c| (VIDEO, c)).collect::<Vec<_>>(),
        "one request per new counter while the edge waits"
    );

    // The next admitted group ends the wait ...
    advance(Duration::from_secs(1)).await;
    let next = publisher.push_inbound_uni();
    next.push(concat(&stream_group(70, 7, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    assert!(matches!(
        rig.cell(10).machine_state(),
        Some(EdgeState::Forwarding { .. })
    ));
    assert_eq!(requests(&publisher).len(), 5, "nothing more requested");
    // ... and re-arms the walk for the next refusal.
    publisher.push_inbound_uni().push(delta(80, 8));
    settle().await;
    assert_eq!(keyframes.refusal_walks(), 2);
}

/// Refusals while nothing waits for the source request nothing, and their
/// coalesced successors follow nothing up.
#[tokio::test(start_paused = true)]
async fn coalesced_refusals_request_nothing_when_nothing_waits() {
    let _metrics = MetricAssertion::snapshot();
    let mut stream_limits = limits(1 << 20, STALL_MS);
    stream_limits.opens_per_connection_per_sec = 1;
    // No subscriber connected: the edge cannot want anything.
    let mut rig = StreamRig::new(&video_policy(1, FWD, &[6]), stream_limits);
    let publisher = rig.publisher(5);
    let admitted = publisher.push_inbound_uni();
    admitted.push(concat(&stream_group(1, 1, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    for counter in 2..=4 {
        publisher
            .push_inbound_uni()
            .push(delta(counter * 10, counter));
        settle().await;
    }
    assert_eq!(rig.keyframes(5).unwrap().refusal_walks(), 1);
    assert!(requests(&publisher).is_empty());
}

// ---------------------------------------------------------------------------
// R-13: own-edge scoping and every unmapped / counter_ahead arm
// ---------------------------------------------------------------------------

const PAUSED_PUB: u32 = 8;
const MUTED_PUB: u32 = 9;
const ABSENT_PUB: u32 = 3;
const SILENT_PUB: u32 = 4;

/// Subscriber 6: slot 4 <- 5 video, slot 6 <- 5 AUDIO (datagram), slot 7 <-
/// a PAUSED video, slot 8 <- a `DROP_AT_INGRESS` video, slot 9 <- a publisher
/// never bound here, slot 10 <- a bound publisher that never sent.
/// Subscriber 7: slot 5 <- 5 video.
fn scoping_policy() -> RegisterMeetingRequest {
    register_request_declared(
        MEETING,
        1,
        vec![
            stream_egress(10, 6, 4, 5, VIDEO),
            stream_egress(11, 7, 5, 5, VIDEO),
            mh_test_utils::media_policy::egress_from(12, 6, 6, 5, 0),
            stream_egress(13, 6, 7, PAUSED_PUB, VIDEO),
            stream_egress(14, 6, 8, MUTED_PUB, VIDEO),
            stream_egress(15, 6, 9, ABSENT_PUB, VIDEO),
            stream_egress(16, 6, 10, SILENT_PUB, VIDEO),
        ],
        vec![
            source_stream(5, VIDEO, SPG, FWD),
            source_stream(5, 0, TransportMode::Datagram, FWD),
            source_stream(PAUSED_PUB, VIDEO, SPG, PAUSED),
            source_stream(MUTED_PUB, VIDEO, SPG, DROP),
            source_stream(ABSENT_PUB, VIDEO, SPG, FWD),
            source_stream(SILENT_PUB, VIDEO, SPG, FWD),
        ],
    )
}

/// A slot resolves ONLY within the requesting connection's own edges, and
/// only to a stream-per-group edge with a FORWARD source; a source MH has no
/// counter for (never seen, or its publisher not bound here) is
/// `counter_ahead`. None of these writes upstream or touches the dedupe.
#[tokio::test(start_paused = true)]
async fn recovery_resolves_only_own_forwardable_stream_slots() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let mut rig = StreamRig::new(&scoping_policy(), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let paused = rig.publisher(PAUSED_PUB);
    let muted = rig.publisher(MUTED_PUB);
    let _silent = rig.publisher(SILENT_PUB);
    let six = rig.subscriber(6);
    let seven = rig.subscriber(7);
    publisher
        .push_inbound_uni()
        .push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 1)));
    paused
        .push_inbound_uni()
        .push(concat(&stream_group(1, 2, UPLINK_VIDEO_SEND_ID, 0)));
    muted
        .push_inbound_uni()
        .push(concat(&stream_group(1, 3, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    assert!(requests(&publisher).is_empty(), "both video edges forward");
    let mut c6 = Control::new(&six, 6);
    let mut c7 = Control::new(&seven, 7);

    // 6 naming 7's slot: unmapped, nothing upstream, no dedupe change.
    c6.send(&rig, recovery(5, 5, 4)).await;
    tally.expect(&metrics, RecoveryOutcome::UnmappedSlot);
    assert!(requests(&publisher).is_empty());
    // Datagram (audio) slot, PAUSED source, DROP_AT_INGRESS source.
    c6.send(&rig, recovery(6, 5, 4)).await;
    tally.expect(&metrics, RecoveryOutcome::UnmappedSlot);
    c6.send(&rig, recovery(7, PAUSED_PUB, 2)).await;
    tally.expect(&metrics, RecoveryOutcome::UnmappedSlot);
    c6.send(&rig, recovery(8, MUTED_PUB, 3)).await;
    tally.expect(&metrics, RecoveryOutcome::UnmappedSlot);
    // No counter: the publisher not bound here, or bound and never seen.
    c6.send(&rig, recovery(9, ABSENT_PUB, 0)).await;
    tally.expect(&metrics, RecoveryOutcome::CounterAhead);
    c6.send(&rig, recovery(10, SILENT_PUB, 0)).await;
    tally.expect(&metrics, RecoveryOutcome::CounterAhead);
    assert!(requests(&publisher).is_empty());
    assert!(requests(&paused).is_empty() && requests(&muted).is_empty());

    // Positive control: 7 naming ITS slot is forwarded (the dedupe was
    // untouched by 6's unmapped request at the same counter).
    c7.send(&rig, recovery(5, 5, 4)).await;
    tally.expect(&metrics, RecoveryOutcome::Forwarded);
    assert_eq!(requests(&publisher), vec![(VIDEO, 4)]);
}

/// The SAME slot id on two subscribers fed by two sources: each subscriber's
/// request is judged against ITS slot's source only.
#[tokio::test(start_paused = true)]
async fn the_same_slot_id_resolves_per_subscriber() {
    let metrics = MetricAssertion::snapshot();
    let mut tally = Tally::default();
    let policy = register_request_declared(
        MEETING,
        1,
        vec![
            stream_egress(10, 6, SLOT, 5, VIDEO),
            stream_egress(11, 7, SLOT, 8, VIDEO),
        ],
        vec![
            source_stream(5, VIDEO, SPG, FWD),
            source_stream(8, VIDEO, SPG, FWD),
        ],
    );
    let mut rig = StreamRig::new(&policy, limits(1 << 20, STALL_MS));
    let five = rig.publisher(5);
    let eight = rig.publisher(8);
    let six = rig.subscriber(6);
    let _seven = rig.subscriber(7);
    five.push_inbound_uni()
        .push(concat(&stream_group(1, 4, UPLINK_VIDEO_SEND_ID, 1)));
    eight
        .push_inbound_uni()
        .push(concat(&stream_group(1, 6, UPLINK_VIDEO_SEND_ID, 1)));
    settle().await;
    let mut c6 = Control::new(&six, 6);
    c6.send(&rig, recovery(SLOT, 5, 4)).await;
    tally.expect(&metrics, RecoveryOutcome::Forwarded);
    assert_eq!(requests(&five), vec![(VIDEO, 4)]);
    assert!(requests(&eight).is_empty(), "8 receives nothing");
    c6.send(&rig, recovery(SLOT, 8, 6)).await;
    tally.expect(&metrics, RecoveryOutcome::SourceMismatch);
    assert!(requests(&eight).is_empty());
}
