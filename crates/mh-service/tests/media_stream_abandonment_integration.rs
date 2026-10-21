//! Per-destination abandonment and the stream-path limits (story 3 R-12,
//! R-21; ADR-0036 §1 amendment, §10 Tier 1b forward-path gates), through the
//! deterministic shim on tokio paused time — never wall-clock.
//!
//! Every abandonment path RESETS and never FINISHes (a half-written frame
//! never looks complete): subscriber backlog, publisher stall, publisher
//! reset, a truncating FIN, source ended, edge removed, meeting released, and
//! connection teardown (uncounted: the destination is gone). Plus the
//! creation-rate cap, the concurrency bound, and the live-groups gauge on
//! every exit.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

mod common;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use bytes::Bytes;
use common::media_frame::{
    parse_stream, stream_frame, stream_group, FrameShape, UPLINK_VIDEO_SEND_ID,
};
use common::stream_rig::{concat, limits, settle, StreamRig, MEETING};
use mh_service::media::edge::EdgeState;
use mh_service::media::edge_runtime::MAX_LIVE_DOWNLINKS_PER_EDGE;
use mh_service::observability::metrics::GroupResetReason;
use mh_service::transport::{GROUP_ABANDONED, INGRESS_REFUSED};
use mh_test_utils::media_policy::{
    egress_from, register_request_declared, source_stream, stream_egress,
};
use mh_test_utils::stream_shim::EgressTerminal;
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, SourceDisposition};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::time::Duration;
use tokio::time::advance;

const FWD: SourceDisposition = SourceDisposition::Forward;
const RESETS: &str = "mh_media_group_resets_total";
const DROPPED: &str = "mh_media_frames_dropped_total";
const STALL_MS: u64 = 1000;

/// Publisher 5's video (1, `STREAM_PER_GROUP`) and audio (0, DATAGRAM); each
/// subscriber holds video slot 4 (edges 11, 13, ...) and audio slot 0.
fn policy(generation: u64, subscribers: &[u32]) -> RegisterMeetingRequest {
    let mut edges = Vec::new();
    for (i, sub) in subscribers.iter().enumerate() {
        let base = 10 + 2 * u32::try_from(i).unwrap();
        edges.push(egress_from(base, *sub, 0, 5, 0));
        edges.push(stream_egress(base + 1, *sub, 4, 5, 1));
    }
    register_request_declared(
        MEETING,
        generation,
        edges,
        vec![
            source_stream(5, 0, TransportMode::Datagram, FWD),
            source_stream(5, 1, TransportMode::StreamPerGroup, FWD),
        ],
    )
}

fn resets(metrics: &MetricSnapshot, reason: GroupResetReason) -> u64 {
    metrics
        .counter(RESETS)
        .with_labels(&[("reason", reason.as_str())])
        .delta()
}

/// Exactly `n` resets for `reason` and none for any other.
fn assert_only_resets(metrics: &MetricSnapshot, reason: GroupResetReason, n: u64) {
    for other in GroupResetReason::ALL {
        let expected = if other == reason { n } else { 0 };
        assert_eq!(resets(metrics, other), expected, "{}", other.as_str());
    }
}

fn big_frame(seq: u32, gop: u32, start: bool, len: usize) -> Bytes {
    stream_frame(
        seq,
        gop,
        UPLINK_VIDEO_SEND_ID,
        start,
        &vec![0x5A; len],
        FrameShape::Minimal,
    )
}

/// One slow subscriber is reset at its cap; the fast subscriber, the
/// publisher and everyone's audio are untouched; the slow one resumes at a
/// later group start.
#[tokio::test(start_paused = true)]
async fn a_slow_subscriber_is_reset_at_its_cap_and_nobody_else_is_touched() {
    let metrics = MetricAssertion::snapshot();
    let backlog = 16_384;
    let mut rig = StreamRig::new(&policy(1, &[6, 7]), limits(backlog, STALL_MS));
    let publisher = rig.publisher(5);
    let slow = rig.subscriber(6);
    let fast = rig.subscriber(7);
    slow.set_stream_credit(0);

    let frames: Vec<Bytes> = (0..8).map(|i| big_frame(i, 0, i == 0, 4_000)).collect();
    let inbound = publisher.push_inbound_uni();
    // Both sides of the cap, derived from it: every frame whose bytes keep the
    // destination's queued total at or under `backlog` leaves it Open and
    // uncounted; the frame that crosses it resets.
    let mut queued = 0_usize;
    let mut crossed = false;
    for frame in &frames {
        queued += frame.len();
        inbound.push(frame.clone());
        settle().await;
        if queued <= backlog {
            assert_eq!(
                slow.egress_uni_streams()[0].terminal(),
                EgressTerminal::Open
            );
            assert_eq!(resets(&metrics, GroupResetReason::SubscriberBacklog), 0);
        } else if !crossed {
            crossed = true;
            assert!(
                slow.egress_uni_streams()[0].terminal().is_reset(),
                "the frame that crosses the cap resets"
            );
        }
    }
    assert!(crossed, "positive control: the frames do cross the cap");
    inbound.finish();
    settle().await;
    let slow_streams = slow.egress_uni_streams();
    assert_eq!(
        slow_streams[0].terminal(),
        EgressTerminal::Reset {
            code: GROUP_ABANDONED,
            by_drop: false
        },
        "reset past the cap, never finished"
    );
    assert_eq!(
        fast.egress_uni_streams()[0].terminal(),
        EgressTerminal::Finished
    );
    assert_eq!(
        parse_stream(&fast.egress_uni_streams()[0].bytes()).len(),
        8,
        "fast unaffected"
    );
    assert_eq!(
        inbound.stopped(),
        None,
        "the publisher is never stalled or stopped"
    );
    assert_only_resets(&metrics, GroupResetReason::SubscriberBacklog, 1);
    assert!(matches!(
        rig.cell(11).machine_state(),
        Some(EdgeState::Waiting { .. })
    ));

    // Audio for the slow subscriber still flows on the datagram path.
    let mut forwarder = mh_service::media::forwarder::ConnectionForwarder::new(
        rig.meeting.clone(),
        common::stream_rig::sender(5),
        std::sync::Arc::clone(&rig.handles),
        0.0,
        mh_service::config::NOMINAL_AUDIO_FRAME_BYTES,
    );
    let outcome = mh_service::media::forward::forward_one(
        &mut forwarder,
        &rig.routing.load(),
        &rig.subscribers.load(),
        mh_service::media::forward::IngressFrame {
            payload: common::media_frame::audio_datagram(1, 2),
            received_at: std::time::Instant::now(),
        },
    );
    assert_eq!(
        outcome.delivered, 2,
        "everyone's audio, the slow subscriber's included"
    );

    // Recovery: the next group start, once the subscriber reads again.
    slow.clear_stream_credit();
    let next = publisher.push_inbound_uni();
    next.push(concat(&stream_group(20, 1, UPLINK_VIDEO_SEND_ID, 1)));
    next.finish();
    settle().await;
    let resumed = &slow.egress_uni_streams()[1];
    assert_eq!(resumed.terminal(), EgressTerminal::Finished);
    assert!(parse_stream(&resumed.bytes())[0].group_start);
}

/// ADR §10: concurrent unfinished groups stay at or below the application
/// bound (two per edge) and below the declared QUIC ceiling, however slow the
/// subscriber.
#[tokio::test(start_paused = true)]
async fn concurrent_unfinished_groups_stay_within_the_application_bound() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let slow = rig.subscriber(6);
    slow.set_stream_credit(0);
    for gop in 0..10_u32 {
        let inbound = publisher.push_inbound_uni();
        inbound.push(concat(&stream_group(
            gop * 10,
            gop,
            UPLINK_VIDEO_SEND_ID,
            1,
        )));
        inbound.finish();
        settle().await;
    }
    let stream_edges = 1;
    let bound = MAX_LIVE_DOWNLINKS_PER_EDGE * stream_edges;
    let declared = usize::try_from(
        mh_test_utils::admission::fixture_quic_transport().max_concurrent_uni_streams,
    )
    .unwrap();
    assert!(
        slow.egress_uni_max_concurrent() <= bound,
        "application bound"
    );
    assert!(
        bound < declared,
        "the application bound trips before the QUIC ceiling"
    );
    assert!(
        resets(&metrics, GroupResetReason::SubscriberBacklog) > 0,
        "older groups abandoned"
    );
    for stream in slow.egress_uni_streams() {
        assert_ne!(
            stream.terminal(),
            EgressTerminal::Finished,
            "nothing half-written finished"
        );
    }
}

/// The mid-frame stall bound fires AT the bound, not before — on the injected
/// clock — and a slow-but-progressing publisher is never reset.
#[tokio::test(start_paused = true)]
async fn the_mid_frame_stall_reset_fires_at_the_bound() {
    // Mid-TAIL: publisher_stall, no partial_frame_discard.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let frame = big_frame(1, 0, true, 400);
        let inbound = publisher.push_inbound_uni();
        inbound.push(frame.slice(..200));
        settle().await;
        advance(Duration::from_millis(STALL_MS - 1)).await;
        settle().await;
        assert_eq!(inbound.stopped(), None, "bound - 1 ms: no reset");
        assert_eq!(
            subscriber.egress_uni_streams()[0].terminal(),
            EgressTerminal::Open
        );
        advance(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(
            inbound.stopped(),
            Some(INGRESS_REFUSED),
            "stopped at the bound"
        );
        assert!(subscriber.egress_uni_streams()[0].terminal().is_reset());
        assert_only_resets(&metrics, GroupResetReason::PublisherStall, 1);
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "partial_frame_discard")])
            .assert_delta(0);
    }
    // Mid-HEADER of the stream's FIRST frame: NO stall bound (story 3
    // amendment 4 — nothing has been forwarded; the declared QUIC
    // concurrent-stream limit bounds headerless streams).
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let _subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 40).slice(..5));
        settle().await;
        advance(Duration::from_millis(STALL_MS * 100)).await;
        settle().await;
        assert_eq!(inbound.stopped(), None, "a headerless stream never stalls");
        assert_only_resets(&metrics, GroupResetReason::PublisherStall, 0);
        // Positive control: the same stream, once its header completes and a
        // tail is pending, IS stall-bounded.
        inbound.push(big_frame(1, 0, true, 40).slice(5..60));
        settle().await;
        advance(Duration::from_millis(STALL_MS)).await;
        settle().await;
        assert_eq!(inbound.stopped(), Some(INGRESS_REFUSED));
    }
    // Mid-HEADER of a LATER frame (downstream open): still bounded —
    // publisher_stall downstream and partial_frame_discard.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400));
        inbound.push(big_frame(2, 0, false, 40).slice(..5));
        settle().await;
        advance(Duration::from_millis(STALL_MS)).await;
        settle().await;
        assert_eq!(
            inbound.stopped(),
            Some(INGRESS_REFUSED),
            "a 5-byte slowloris on frame 2 is bounded"
        );
        assert!(subscriber.egress_uni_streams()[0].terminal().is_reset());
        assert_only_resets(&metrics, GroupResetReason::PublisherStall, 1);
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "partial_frame_discard")])
            .assert_delta(1);
    }
    // Slow but progressing: a chunk every bound - 1 ms, total far past the
    // bound — never reset (the timer re-arms on progress).
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let frame = big_frame(1, 0, true, 400);
        let inbound = publisher.push_inbound_uni();
        for i in 0..frame.len() / 50 {
            inbound.push(frame.slice(i * 50..((i + 1) * 50).min(frame.len())));
            settle().await;
            advance(Duration::from_millis(STALL_MS - 1)).await;
            settle().await;
        }
        inbound.push(frame.slice((frame.len() / 50) * 50..));
        inbound.finish();
        settle().await;
        assert_eq!(inbound.stopped(), None);
        assert_eq!(
            subscriber.egress_uni_streams()[0].terminal(),
            EgressTerminal::Finished
        );
        assert_only_resets(&metrics, GroupResetReason::PublisherStall, 0);
    }
}

/// A publisher reset, and a FIN that truncates a frame, both propagate as
/// downstream RESETs (`publisher_reset`); the edge waits and resumes at the
/// next group start. A FIN at a frame boundary finishes.
#[tokio::test(start_paused = true)]
async fn publisher_ends_propagate_reset_and_a_clean_fin_finishes() {
    for (name, end) in [("reset", 0_u8), ("fin mid-tail", 1)] {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let frame = big_frame(1, 0, true, 400);
        let inbound = publisher.push_inbound_uni();
        inbound.push(frame.slice(..300));
        settle().await;
        if end == 0 {
            inbound.reset(0x5254);
        } else {
            inbound.finish();
        }
        settle().await;
        assert!(
            subscriber.egress_uni_streams()[0].terminal().is_reset(),
            "{name}"
        );
        assert_only_resets(&metrics, GroupResetReason::PublisherReset, 1);
        assert!(matches!(
            rig.cell(11).machine_state(),
            Some(EdgeState::Waiting { .. })
        ));
        let next = publisher.push_inbound_uni();
        next.push(concat(&stream_group(2, 1, UPLINK_VIDEO_SEND_ID, 0)));
        next.finish();
        settle().await;
        assert_eq!(
            subscriber.egress_uni_streams()[1].terminal(),
            EgressTerminal::Finished
        );
    }
}

/// An ended source ends every downstream stream (`source_ended`); a removed
/// edge, and a released meeting, end theirs (`edge_removed`) — all RESET.
#[tokio::test(start_paused = true)]
async fn source_ended_edge_removed_and_meeting_released_reset() {
    // Source ended: the publisher disconnects mid-group.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6, 7]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subs = [rig.subscriber(6), rig.subscriber(7)];
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400).slice(..300));
        settle().await;
        rig.disconnect_publisher(5).await;
        for s in &subs {
            assert!(s.egress_uni_streams()[0].terminal().is_reset());
        }
        assert_only_resets(&metrics, GroupResetReason::SourceEnded, 2);
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "partial_frame_discard")])
            .assert_delta(0);
    }
    // Edge removed mid-group.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6, 7]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let removed = rig.subscriber(6);
        let kept = rig.subscriber(7);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400).slice(..300));
        settle().await;
        // Re-push without subscriber 6's edges (7 keeps its egress ids 13).
        let mut request = policy(2, &[6, 7]);
        request.egress_streams.retain(|e| e.egress_stream_id >= 12);
        rig.install(&request);
        settle().await;
        assert!(removed.egress_uni_streams()[0].terminal().is_reset());
        assert_eq!(
            kept.egress_uni_streams()[0].terminal(),
            EgressTerminal::Open
        );
        assert_only_resets(&metrics, GroupResetReason::EdgeRemoved, 1);
    }
    // Meeting released (EndMeeting).
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400).slice(..300));
        settle().await;
        assert!(rig.routing.remove(&rig.meeting));
        settle().await;
        assert!(subscriber.egress_uni_streams()[0].terminal().is_reset());
        assert_only_resets(&metrics, GroupResetReason::EdgeRemoved, 1);
    }
}

/// Subscriber connection teardown aborts its writers: the seam's drop
/// contract RESETS the mid-frame stream (never FIN) — deliberately UNCOUNTED,
/// the destination being gone.
#[tokio::test(start_paused = true)]
async fn a_torn_down_subscriber_connection_resets_by_drop_uncounted() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let (subscriber, subscriber_connection) = rig.subscriber_with_cancel(6);
    subscriber.set_stream_credit(0);
    let inbound = publisher.push_inbound_uni();
    let frame = big_frame(1, 0, true, 400);
    inbound.push(frame.clone());
    settle().await;
    // Only the SUBSCRIBER's connection goes away: its writers are aborted.
    subscriber_connection.cancel();
    settle().await;
    assert_eq!(
        subscriber.egress_uni_streams()[0].terminal(),
        EgressTerminal::Reset {
            code: GROUP_ABANDONED,
            by_drop: true
        },
        "the seam's drop contract resets the mid-group stream, never FIN"
    );
    // The destination is gone: no group reset is counted for it, by any reason.
    assert_only_resets(&metrics, GroupResetReason::SubscriberBacklog, 0);
    // The next frame for that edge DISCOVERS the lost downlink: one frame,
    // one outcome (`connection_closed`), and it is never forwarded.
    let forwarded_before = metrics
        .counter("mh_media_frames_forwarded_total")
        .with_labels(&[("direction", "egress")])
        .delta();
    inbound.push(big_frame(2, 0, false, 400));
    settle().await;
    metrics
        .counter(DROPPED)
        .with_labels(&[("reason", "connection_closed")])
        .assert_delta(1);
    assert_eq!(
        metrics
            .counter("mh_media_frames_forwarded_total")
            .with_labels(&[("direction", "egress")])
            .delta(),
        forwarded_before
    );
    metrics
        .counter(DROPPED)
        .with_labels(&[("reason", "partial_frame_discard")])
        .assert_delta(0);
}

/// An aborted group (MH's own teardown) that was mid-HEADER still counts
/// `partial_frame_discard`: a publisher disconnect reaches the group as an
/// abort or as a read error depending on scheduling, and one event must not
/// have two counts.
#[tokio::test(start_paused = true)]
async fn an_aborted_mid_header_group_counts_partial_frame_discard() {
    let metrics = MetricAssertion::snapshot();
    let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let inbound = publisher.push_inbound_uni();
    inbound.push(big_frame(1, 0, true, 400).slice(..5));
    settle().await;
    rig.cancel.cancel();
    settle().await;
    metrics
        .counter(DROPPED)
        .with_labels(&[("reason", "partial_frame_discard")])
        .assert_delta(1);
}

/// The creation-rate cap: N admitted; the N+1th has ONLY its header read
/// (its counter recorded — story 3 amendment 4), then is stopped and counted
/// `stream_rate_limited` once; admitted again after a second.
#[tokio::test(start_paused = true)]
async fn the_stream_open_rate_limit_reads_the_header_then_refuses() {
    let metrics = MetricAssertion::snapshot();
    let n = 3_u32;
    let mut limits = limits(1 << 20, STALL_MS);
    limits.opens_per_connection_per_sec = n;
    let mut rig = StreamRig::new(&policy(1, &[6]), limits);
    let publisher = rig.publisher(5);
    let _subscriber = rig.subscriber(6);
    let mut admitted = Vec::new();
    for _ in 0..n {
        let s = publisher.push_inbound_uni();
        s.push(concat(&stream_group(1, 0, UPLINK_VIDEO_SEND_ID, 0)));
        admitted.push(s);
    }
    let refused = publisher.push_inbound_uni();
    let refused_group = concat(&stream_group(9, 7, UPLINK_VIDEO_SEND_ID, 3));
    refused.push(refused_group.clone());
    settle().await;
    assert!(admitted.iter().all(|s| s.stopped().is_none()));
    assert_eq!(refused.stopped(), Some(INGRESS_REFUSED));
    let read = usize::try_from(refused.bytes_read()).unwrap();
    assert!(read > 0, "the header was read");
    assert!(read < refused_group.len(), "the rest of the group was not");
    let video = mh_service::routing::StreamNumber::from_wire(1).unwrap();
    assert_eq!(
        rig.keyframes(5).unwrap().newest(video),
        Some(7),
        "the refused group's counter is recorded"
    );
    metrics
        .counter(DROPPED)
        .with_labels(&[("reason", "stream_rate_limited")])
        .assert_delta(1);
    advance(Duration::from_secs(1)).await;
    let later = publisher.push_inbound_uni();
    later.push(concat(&stream_group(1, 8, UPLINK_VIDEO_SEND_ID, 0)));
    settle().await;
    assert_eq!(later.stopped(), None, "admitted again after a second");
    assert!(later.bytes_read() > 0);
}

/// The live-groups gauge returns to zero after EVERY exit row of the stream
/// ingress terminal table, the abort included.
#[tokio::test(start_paused = true)]
async fn the_ingress_groups_gauge_returns_to_zero_on_every_exit() {
    enum Exit {
        FinAtBoundary,
        FinMidHeader,
        FinMidTail,
        Reset,
        Stall,
        Refused,
        Abort,
    }
    for exit in [
        Exit::FinAtBoundary,
        Exit::FinMidHeader,
        Exit::FinMidTail,
        Exit::Reset,
        Exit::Stall,
        Exit::Refused,
        Exit::Abort,
    ] {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let _subscriber = rig.subscriber(6);
        let frame = big_frame(1, 0, true, 400);
        let refused = matches!(exit, Exit::Refused);
        let first = match exit {
            Exit::Refused => concat(&stream_group(1, 0, 9, 0)),
            // The stall bound is armed only once the first header completed.
            Exit::Stall => frame.slice(..200),
            _ => frame.slice(..5),
        };
        let inbound = publisher.push_inbound_uni();
        inbound.push(first);
        settle().await;
        if !refused {
            // Positive control: the gauge moved while the group was live.
            metrics
                .gauge("mh_media_stream_ingress_groups")
                .assert_value(1.0);
        }
        match exit {
            Exit::FinAtBoundary => {
                inbound.push(frame.slice(5..));
                inbound.finish();
            }
            Exit::FinMidHeader => inbound.finish(),
            Exit::FinMidTail => {
                inbound.push(frame.slice(5..300));
                inbound.finish();
            }
            Exit::Reset => {
                inbound.push(frame.slice(5..300));
                inbound.reset(1);
            }
            Exit::Stall => {
                advance(Duration::from_millis(STALL_MS)).await;
            }
            Exit::Refused => {}
            Exit::Abort => rig.cancel.cancel(),
        }
        settle().await;
        metrics
            .gauge("mh_media_stream_ingress_groups")
            .assert_value(0.0);
    }
}

/// The stream-path bounds are published from the values enforcement reads
/// (ADR-0036 §11 "derive rather than guard"), in their catalog units.
#[test]
fn the_stream_path_bounds_are_published_in_catalog_units() {
    let metrics = MetricAssertion::snapshot();
    let quic = mh_test_utils::admission::fixture_quic_transport();
    let stream = mh_test_utils::admission::fixture_stream_path_limits();
    mh_service::observability::metrics::publish_stream_limits(
        quic.max_concurrent_uni_streams,
        stream.egress_stream_backlog_bytes,
        stream.mid_frame_stall,
    );
    metrics
        .gauge("mh_media_uni_streams_per_connection_limit")
        .assert_value(f64::from(quic.max_concurrent_uni_streams));
    metrics
        .gauge("mh_media_egress_stream_backlog_limit_bytes")
        .assert_value(f64::from(
            u32::try_from(stream.egress_stream_backlog_bytes).unwrap(),
        ));
    metrics
        .gauge("mh_media_stream_mid_frame_stall_seconds")
        .assert_value(stream.mid_frame_stall.as_secs_f64());
}

/// Two more exit rows with downstream evidence: a FIN mid-HEADER on a later
/// frame (downstream already open), and a codec reject mid-group.
#[tokio::test(start_paused = true)]
async fn fin_mid_header_and_a_codec_reject_mid_group_reset_downstream() {
    // FIN mid-header after a complete frame.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400));
        settle().await; // the downlink is open before the group breaks
        inbound.push(big_frame(2, 0, false, 400).slice(..5));
        inbound.finish();
        settle().await;
        assert!(subscriber.egress_uni_streams()[0].terminal().is_reset());
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "partial_frame_discard")])
            .assert_delta(1);
        assert_only_resets(&metrics, GroupResetReason::PublisherReset, 1);
    }
    // A codec reject mid-group: the frame's version byte is not 3.
    {
        let metrics = MetricAssertion::snapshot();
        let mut rig = StreamRig::new(&policy(1, &[6]), limits(1 << 20, STALL_MS));
        let publisher = rig.publisher(5);
        let subscriber = rig.subscriber(6);
        let inbound = publisher.push_inbound_uni();
        inbound.push(big_frame(1, 0, true, 400));
        settle().await;
        let mut bad = big_frame(2, 0, false, 400).to_vec();
        bad[0] = 9;
        inbound.push(Bytes::from(bad));
        settle().await;
        assert_eq!(inbound.stopped(), Some(INGRESS_REFUSED));
        assert!(subscriber.egress_uni_streams()[0].terminal().is_reset());
        metrics
            .counter(DROPPED)
            .with_labels(&[("reason", "unknown_version")])
            .assert_delta(1);
        assert_only_resets(&metrics, GroupResetReason::PublisherReset, 1);
    }
}
