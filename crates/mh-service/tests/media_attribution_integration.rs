//! Per-stream ingress attribution through `forward_one` (story 3 R-16, R-17,
//! R-20; ADR-0036 §10 Tier-1a).
//!
//! Every test drives `media::forward::forward_one` on hand-built frames against
//! a REAL routing snapshot and a REAL subscriber registry. The publisher
//! declares audio = send id 0 on DATAGRAM and video = send id 1 on
//! STREAM_PER_GROUP; "label" is the uplink relay-region `stream_id`.
//!
//! Each outcome row asserts THREE things: where the frame went (which slot, by
//! the downlink slot id it was rewritten to), the counter it moved (a
//! `MetricAssertion` delta on a snapshot bound before the handles were
//! resolved), and the ingress accounting identity — `forwarded{ingress}` plus
//! every ingress-direction drop moved by exactly ONE for the one frame.
//! Absence rows ("never a video slot") carry a positive control in the same
//! row, so the absence is not vacuous.
//!
//! The uplink label is publisher-set and unauthenticated at MH; these tests
//! pin what the lookup STRUCTURALLY guarantees, not that MH identifies a
//! frame's true stream.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

#[path = "common/mod.rs"]
mod common;

use common::media_frame::labelled_datagram;
use common::media_rig::sender;

use ::common::observability::testing::{MetricAssertion, MetricSnapshot};
use media_protocol::codec::decode_datagram;
use media_protocol::frame::KEY_ID_BYTES;
use mh_service::config::{EGRESS_QUEUE_FRAMES, NOMINAL_AUDIO_FRAME_BYTES};
use mh_service::media::forward::{forward_one, EgressQueue, ForwardOutcome, IngressFrame};
use mh_service::media::forwarder::ConnectionForwarder;
use mh_service::media::queue::SharedQueue;
use mh_service::observability::metrics::{resolve_media_handles, MediaDirection, MediaDropReason};
use mh_service::routing::{MeetingKey, MeetingPolicy, RoutingTable};
use mh_service::session::LocalSubscribers;
use mh_test_utils::admission::fixture_policy_limits;
use mh_test_utils::media_policy::{egress_from, register_request_declared, source_stream};
use proto_gen::dark_tower::internal::v1::{RegisterMeetingRequest, SourceDisposition};
use proto_gen::dark_tower::signaling::v1::TransportMode;
use std::sync::Arc;
use std::time::Instant;

const MEETING: &str = "meeting-attribution";

/// The publisher, and the subscriber holding one audio and one video slot of
/// it. Ids and slots from disjoint ranges.
const PUB: u32 = 41;
const SUB: u32 = 52;
const AUDIO_SLOT: u16 = 6;
const VIDEO_SLOT: u16 = 9;
const AUDIO: u32 = 0;
const VIDEO: u32 = 1;
const AUDIO_EDGE: u32 = 100;
const VIDEO_EDGE: u32 = 101;

use SourceDisposition::{DropAtIngress as DROP, Forward as FWD, Paused as PAUSED};

fn declared(
    audio: SourceDisposition,
    video: SourceDisposition,
    generation: u64,
) -> RegisterMeetingRequest {
    register_request_declared(
        MEETING,
        generation,
        vec![
            egress_from(AUDIO_EDGE, SUB, u32::from(AUDIO_SLOT), PUB, AUDIO),
            egress_from(VIDEO_EDGE, SUB, u32::from(VIDEO_SLOT), PUB, VIDEO),
        ],
        vec![
            source_stream(PUB, AUDIO, TransportMode::Datagram, audio),
            source_stream(PUB, VIDEO, TransportMode::StreamPerGroup, video),
        ],
    )
}

fn install(table: &RoutingTable, request: &RegisterMeetingRequest) {
    table.install(&MeetingPolicy::from_request(request, &fixture_policy_limits()).unwrap());
}

/// A live forward path: policy installed, the subscriber connected, the
/// publisher's forwarder bound. Built AFTER the caller's `MetricAssertion`
/// snapshot, so the handles bind to it.
struct Rig {
    table: RoutingTable,
    registry: LocalSubscribers,
    queue: Arc<EgressQueue>,
    forwarder: ConnectionForwarder,
}

impl Rig {
    fn new(request: &RegisterMeetingRequest) -> Self {
        let table = RoutingTable::unbounded_for_tests();
        install(&table, request);
        let registry = LocalSubscribers::new();
        let queue: Arc<EgressQueue> = Arc::new(SharedQueue::new(EGRESS_QUEUE_FRAMES));
        registry.register(MeetingKey::new(MEETING), sender(SUB), "conn-sub", &queue);
        let forwarder = ConnectionForwarder::new(
            MeetingKey::new(MEETING),
            sender(PUB),
            Arc::new(resolve_media_handles()),
            1.0,
            NOMINAL_AUDIO_FRAME_BYTES,
        );
        Self {
            table,
            registry,
            queue,
            forwarder,
        }
    }

    fn send(&mut self, seq: u32, label: u16) -> ForwardOutcome {
        self.send_payload(seq, label, &[0xC3; 32])
    }

    fn send_payload(&mut self, seq: u32, label: u16, payload: &[u8]) -> ForwardOutcome {
        forward_one(
            &mut self.forwarder,
            &self.table.load(),
            &self.registry.load(),
            IngressFrame {
                payload: labelled_datagram(seq, label, payload),
                received_at: Instant::now(),
            },
        )
    }

    /// Every downlink `(slot, hop)` delivered since the last drain.
    fn drain(&self) -> Vec<(u16, u32)> {
        let mut out = Vec::new();
        while let Some(frame) = self.queue.try_pop() {
            let view = decode_datagram(&frame.payload).expect("relayed frame decodes");
            out.push((view.stream_id(), view.hop_sequence()));
        }
        out
    }
}

/// The ingress accounting identity for ONE frame: `forwarded{ingress}` plus
/// every ingress-direction drop moved by exactly one, and the named counter
/// is the one that moved.
fn assert_one_ingress_outcome(snap: &MetricSnapshot, moved: Option<MediaDropReason>) {
    let forwarded = counter(
        snap,
        "mh_media_frames_forwarded_total",
        &[("direction", "ingress")],
    );
    let mut drops = 0;
    for reason in MediaDropReason::ALL {
        if reason.direction() != MediaDirection::Ingress {
            continue;
        }
        let value = counter(
            snap,
            "mh_media_frames_dropped_total",
            &[("reason", reason.as_str()), ("direction", "ingress")],
        );
        assert_eq!(
            value,
            u64::from(moved == Some(reason)),
            "ingress drop `{}`",
            reason.as_str()
        );
        drops += value;
    }
    assert_eq!(
        forwarded,
        u64::from(moved.is_none()),
        "forwarded{{ingress}} once per frame, never per edge"
    );
    assert_eq!(forwarded + drops, 1, "every frame is counted exactly once");
}

fn counter(snap: &MetricSnapshot, name: &'static str, labels: &[(&str, &str)]) -> u64 {
    let mut all: Vec<(&str, &str)> = labels.to_vec();
    all.push(("key_custody", "operator"));
    snap.counter(name).with_labels(&all).delta()
}

fn paused_delta(snap: &MetricSnapshot) -> u64 {
    counter(snap, "mh_media_paused_source_frames_total", &[])
}

// -- the outcome table ---------------------------------------------------------

#[test]
fn label_0_datagram_reaches_the_audio_slot_only() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::new(&declared(FWD, FWD, 1));
    let outcome = rig.send(1, 0);
    assert_eq!(outcome.delivered, 1);
    assert_eq!(
        rig.drain(),
        vec![(AUDIO_SLOT, 0)],
        "audio slot, never the video slot"
    );
    assert_one_ingress_outcome(&snap, None);
    assert_eq!(paused_delta(&snap), 0);
}

#[test]
fn label_1_datagram_is_a_transport_mismatch() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::new(&declared(FWD, FWD, 1));
    assert_eq!(
        rig.send(1, 1).rejected,
        Some(MediaDropReason::TransportMismatch)
    );
    assert!(rig.drain().is_empty());
    assert_one_ingress_outcome(&snap, Some(MediaDropReason::TransportMismatch));
}

#[test]
fn undeclared_labels_are_unmatched_streams() {
    // 7: undeclared. 256 = 0x0100: low byte = the declared AUDIO id, so an
    // `as u8` truncation would ROUTE it; it must be unmatched.
    for label in [7_u16, 256] {
        let snap = MetricAssertion::snapshot();
        let mut rig = Rig::new(&declared(FWD, FWD, 1));
        assert_eq!(
            rig.send(1, label).rejected,
            Some(MediaDropReason::UnmatchedStream),
            "label {label}"
        );
        assert!(rig.drain().is_empty(), "label {label}");
        assert_one_ingress_outcome(&snap, Some(MediaDropReason::UnmatchedStream));
    }
}

/// The patched client: video DROP_AT_INGRESS, video bytes relabelled as audio
/// over datagrams — they reach the AUDIO slot only, never the video slot.
#[test]
fn a_relabelled_frame_while_video_is_server_muted_reaches_audio_slots_only() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::new(&declared(FWD, DROP, 1));
    assert_eq!(rig.send(1, 0).delivered, 1);
    let delivered = rig.drain();
    assert_eq!(
        delivered,
        vec![(AUDIO_SLOT, 0)],
        "positive control: the audio slot"
    );
    assert!(delivered.iter().all(|(slot, _)| *slot != VIDEO_SLOT));
    assert_one_ingress_outcome(&snap, None);
}

/// Per-kind server mute of AUDIO: the audio stream drops `server_muted` while
/// the stream stays declared.
#[test]
fn an_audio_drop_at_ingress_stream_drops_server_muted() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::new(&declared(DROP, FWD, 1));
    assert_eq!(rig.send(1, 0).rejected, Some(MediaDropReason::ServerMuted));
    assert!(rig.drain().is_empty());
    assert_one_ingress_outcome(&snap, Some(MediaDropReason::ServerMuted));
}

/// The sender-level shortcut runs FIRST: with every declared stream
/// DROP_AT_INGRESS, an UNDECLARED label counts `server_muted`, not
/// `unmatched_stream` (this row is what distinguishes the ordering).
#[test]
fn a_fully_muted_sender_counts_server_muted_even_for_an_unattributable_label() {
    for label in [0_u16, 7, 1] {
        let snap = MetricAssertion::snapshot();
        let mut rig = Rig::new(&declared(DROP, DROP, 1));
        assert_eq!(
            rig.send(1, label).rejected,
            Some(MediaDropReason::ServerMuted),
            "label {label}"
        );
        assert_one_ingress_outcome(&snap, Some(MediaDropReason::ServerMuted));
    }
}

/// PAUSED (client mute) is NEVER dropped at ingress: forwarded normally and
/// counted on the paused overlay, which is not a drop.
#[test]
fn a_paused_stream_forwards_and_is_counted_as_a_paused_arrival() {
    let snap = MetricAssertion::snapshot();
    let mut rig = Rig::new(&declared(PAUSED, FWD, 1));
    assert_eq!(rig.send(1, 0).delivered, 1);
    assert_eq!(rig.drain(), vec![(AUDIO_SLOT, 0)]);
    assert_eq!(paused_delta(&snap), 1);
    assert_one_ingress_outcome(&snap, None);
}

/// MH never reads the `SFrame` key id: a frame whose payload key id names the
/// sender's VIDEO stream but whose relay label is 0 routes by the LABEL.
#[test]
fn a_key_id_that_disagrees_with_the_relay_label_routes_by_the_label() {
    // key id = sender_id(16) | stream(8) | generation(40), big-endian.
    let pub_id = u16::try_from(PUB).unwrap().to_be_bytes();
    let agreeing: [u8; KEY_ID_BYTES] = [pub_id[0], pub_id[1], 0, 0, 0, 0, 0, 0];
    let disagreeing: [u8; KEY_ID_BYTES] = [pub_id[0], pub_id[1], 1, 0, 0, 0, 0, 0];
    let mut agree_payload = agreeing.to_vec();
    agree_payload.extend_from_slice(&[0xAA; 24]);
    let mut disagree_payload = disagreeing.to_vec();
    disagree_payload.extend_from_slice(&[0xAA; 24]);
    // Non-vacuity: the two frames really differ in the key-id stream byte.
    assert_ne!(
        labelled_datagram(1, 0, &agree_payload),
        labelled_datagram(1, 0, &disagree_payload)
    );

    let mut rig = Rig::new(&declared(FWD, FWD, 1));
    assert_eq!(rig.send_payload(1, 0, &disagree_payload).delivered, 1);
    assert_eq!(
        rig.drain(),
        vec![(AUDIO_SLOT, 0)],
        "the relay label (audio) routes it, whatever the key id says"
    );
}

/// A declared but unwatched stream is `no_subscriber`: the frame was ingested
/// (`forwarded{ingress}`, once) and delivered nowhere — the documented
/// identity, with the drop attributed to its egress-side cause.
#[test]
fn an_unwatched_declared_stream_is_no_subscriber() {
    let snap = MetricAssertion::snapshot();
    let mut request = declared(FWD, FWD, 1);
    request.egress_streams.clear();
    let mut rig = Rig::new(&request);
    assert_eq!(rig.send(1, 0).rejected, Some(MediaDropReason::NoSubscriber));
    // `no_subscriber` is egress-direction and follows a forwarded{ingress}.
    assert_one_ingress_outcome(&snap, None);
    snap.counter("mh_media_frames_dropped_total")
        .with_labels(&[
            ("reason", "no_subscriber"),
            ("direction", "egress"),
            ("key_custody", "operator"),
        ])
        .assert_delta(1);
}

// -- re-push idempotency ---------------------------------------------------------

/// An identical re-push, an unrelated disposition toggle, and a
/// FORWARD -> PAUSED -> FORWARD toggle on the CARRYING stream (MC re-pushes
/// on every client-mute toggle) never reset the edge's downlink hop sequence.
#[test]
fn re_pushes_preserve_the_downlink_hop_sequence() {
    let mut rig = Rig::new(&declared(FWD, FWD, 1));
    let mut expected_hop = 0;
    let mut send_and_check = |rig: &mut Rig, seq: u32| {
        assert_eq!(rig.send(seq, 0).delivered, 1);
        assert_eq!(rig.drain(), vec![(AUDIO_SLOT, expected_hop)], "frame {seq}");
        expected_hop += 1;
    };

    send_and_check(&mut rig, 1);
    send_and_check(&mut rig, 2);
    // Identical re-push at a new generation.
    install(&rig.table, &declared(FWD, FWD, 2));
    send_and_check(&mut rig, 3);
    // Unrelated: the video stream toggles.
    install(&rig.table, &declared(FWD, PAUSED, 3));
    send_and_check(&mut rig, 4);
    // The carrying stream itself: FORWARD -> PAUSED -> FORWARD.
    install(&rig.table, &declared(PAUSED, PAUSED, 4));
    send_and_check(&mut rig, 5);
    install(&rig.table, &declared(FWD, PAUSED, 5));
    send_and_check(&mut rig, 6);
}

// -- an audio-only (story-2-shaped) meeting, declared -----------------------------

/// A story-2-shaped audio-only meeting, now always DECLARED (MC-B deleted the
/// transitional reading): the publisher's declared label 0 forwards, any other
/// label is unmatched, and a sender whose every declared stream is
/// DROP_AT_INGRESS drops `server_muted` whatever label it stamps.
#[test]
fn an_audio_only_declared_meeting_forwards_label_0_and_rejects_other_labels() {
    use mh_test_utils::media_policy::{egress, register_request, register_request_muted};
    let audio_only = register_request(
        MEETING,
        1,
        vec![egress(AUDIO_EDGE, SUB, u32::from(AUDIO_SLOT), PUB)],
    );
    for (label, expected) in [
        (0_u16, None),
        (1, Some(MediaDropReason::UnmatchedStream)),
        (7, Some(MediaDropReason::UnmatchedStream)),
    ] {
        let snap = MetricAssertion::snapshot();
        let mut rig = Rig::new(&audio_only);
        assert_eq!(rig.send(1, label).rejected, expected, "label {label}");
        assert_one_ingress_outcome(&snap, expected);
    }

    let muted = register_request_muted(
        MEETING,
        1,
        vec![egress(AUDIO_EDGE, SUB, u32::from(AUDIO_SLOT), PUB)],
        &[PUB],
    );
    for label in [0_u16, 7] {
        let snap = MetricAssertion::snapshot();
        let mut rig = Rig::new(&muted);
        assert_eq!(
            rig.send(1, label).rejected,
            Some(MediaDropReason::ServerMuted)
        );
        assert_one_ingress_outcome(&snap, Some(MediaDropReason::ServerMuted));
    }
}
