//! The keyless video publisher: one uni stream per group (or one datagram per
//! frame), driven by a [`GroupScheduler`], over any [`UplinkTransport`].

use std::time::Duration;
use tokio::time::Instant;

use media_protocol::frame::{KEY_ID_BYTES, PUBLISHER_FIXED_PREFIX_SIZE};

use super::control::KeyframeRequestSource;
use super::encode::{encode_video_frame, VideoFrameSpec};
use super::schedule::{Decision, GroupScheduler, GroupStartSource, PublisherEvent};
use super::transport::{UplinkError, UplinkStream, UplinkTransport};
use crate::fixtures::media::VIDEO_SEND_ID;

/// The code [`Fault::ResetMidGroup`] and [`Fault::AbandonMidGroup`] reset
/// with. ADR-0036 does not specify one and MH does not interpret it; any reset
/// is abandonment.
pub const RESET_MID_GROUP_CODE: u32 = 0x5254;

/// The uplink relay `stream_id` knob. `Debug` omits the overriding id.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum UplinkLabel {
    /// The send id MC directed for this stream.
    #[default]
    Directed,
    /// Any other id: `AUDIO_SEND_ID` relabels, anything undeclared is unknown.
    Override(u16),
}

impl std::fmt::Debug for UplinkLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Directed => f.write_str("Directed"),
            Self::Override(_) => f.write_str("Override(..)"),
        }
    }
}

/// The transport knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoTransport {
    /// One unidirectional QUIC stream per group (video's declared transport).
    #[default]
    StreamPerGroup,
    /// One ≤MTU datagram per frame.
    Datagram,
}

/// Whether the publisher honours its directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublisherMode {
    /// Sends only while directed ([`VideoPublisher::set_directed`]).
    #[default]
    Compliant,
    /// Ignores withdrawal and mute: the patched client for R-16.
    KeepSendingWhileMuted,
}

/// Injected faults (stream transport only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    /// None.
    #[default]
    None,
    /// After `after_frames` complete frames, write one frame's header and part
    /// of its payload, then PARK the stream (held open, never finished) until
    /// [`VideoPublisher::release_stall`] — R-21's stall bound.
    StallMidFrame {
        /// Complete frames sent before the stall.
        after_frames: u32,
    },
    /// After `after_frames` complete frames, reset the group's stream with
    /// [`RESET_MID_GROUP_CODE`], and the publisher then ENDS (sends nothing
    /// more). The pre-amendment-4 behaviour, kept for tests that need a
    /// publisher that resets and goes away.
    ResetMidGroup {
        /// Complete frames sent before the reset.
        after_frames: u32,
    },
    /// After `after_frames` complete frames, ONCE: reset the group's stream
    /// with [`RESET_MID_GROUP_CODE`] (the publisher's own per-target
    /// abandonment, R-12) and KEEP RUNNING — then self-key the next group
    /// (story 3 amendment 4: a publisher keys whenever a target has no group
    /// in progress), at once or, inside `min_interval`, through the single
    /// deferral, logged as
    /// [`GroupStartCause::TargetAbandoned`](super::schedule::GroupStartCause::TargetAbandoned).
    /// Frames are withheld until that group starts.
    AbandonMidGroup {
        /// Complete frames sent before the abandonment.
        after_frames: u32,
    },
}

/// A publisher's configuration. Build with [`Self::new`] and struct update.
/// `Debug` omits the sender id, send id and label (stream identity, R-30).
#[derive(Clone, Copy)]
pub struct VideoPublisherConfig {
    /// Meeting-scoped sender id (`JoinResponse.sender_id`), for the key id.
    pub sender_id: u16,
    /// The send id MC directed for this stream (default [`VIDEO_SEND_ID`]).
    pub directed_send_id: u16,
    /// What starts groups after the first.
    pub group_start: GroupStartSource,
    /// The uplink relay label.
    pub label: UplinkLabel,
    /// Stream-per-group or datagram.
    pub transport: VideoTransport,
    /// Compliant or patched.
    pub mode: PublisherMode,
    /// Injected fault.
    pub fault: Fault,
    /// Frame cadence on tokio time.
    pub frame_interval: Duration,
    /// Payload bytes per frame (at least `KEY_ID_BYTES`).
    pub payload_len: usize,
}

impl std::fmt::Debug for VideoPublisherConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoPublisherConfig")
            .field("group_start", &self.group_start)
            .field("transport", &self.transport)
            .field("mode", &self.mode)
            .field("fault", &self.fault)
            .field("frame_interval", &self.frame_interval)
            .finish_non_exhaustive()
    }
}

impl VideoPublisherConfig {
    /// Default knobs for `sender_id` (a `JoinResponse.sender_id`).
    ///
    /// # Panics
    ///
    /// If `sender_id` does not fit the key id's 16-bit field (never masked).
    #[must_use]
    pub fn new(sender_id: u32) -> Self {
        Self {
            sender_id: u16::try_from(sender_id)
                .expect("sender_id must fit the key id's 16-bit sender field"),
            directed_send_id: VIDEO_SEND_ID,
            group_start: GroupStartSource::None,
            label: UplinkLabel::Directed,
            transport: VideoTransport::StreamPerGroup,
            mode: PublisherMode::Compliant,
            fault: Fault::None,
            frame_interval: Duration::from_millis(20),
            payload_len: 200,
        }
    }

    fn validate(&self) {
        assert!(self.sender_id != 0, "sender id 0 never names a sender");
        assert!(
            self.payload_len >= KEY_ID_BYTES,
            "video publisher payload_len must hold at least the key id"
        );
        assert!(
            self.transport == VideoTransport::StreamPerGroup || self.fault == Fault::None,
            "stall/reset faults apply to the stream transport only"
        );
    }
}

/// What one frame tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    /// A frame went out.
    Sent {
        /// Whether it started a group.
        group_start: bool,
    },
    /// Compliant and not directed: nothing sent.
    NotDirected,
    /// The stall fault fired on this tick; the stream is parked.
    Stalled,
    /// The reset fault fired on this tick (the publisher then ends).
    Reset,
    /// The abandonment fault fired on this tick: the group was reset and the
    /// publisher keeps running, self-keying the next group.
    Abandoned,
    /// The publisher has stopped (a fault fired earlier, or the counter is
    /// exhausted).
    Stopped,
    /// MH stopped the group stream (STOP_SENDING) on this write: the frame was
    /// not delivered, the stream is dropped, and frames are withheld until the
    /// next decided group start (a request, the periodic cadence or a resume).
    StoppedByPeer,
    /// Withheld: the last group stream was stopped by MH and no new group has
    /// been decided yet (never a mid-group frame on a fresh stream).
    AwaitingGroupStart,
}

/// The keyless video publisher.
pub struct VideoPublisher<T: UplinkTransport> {
    cfg: VideoPublisherConfig,
    transport: T,
    sched: GroupScheduler,
    stream: Option<T::Stream>,
    stalled: Option<T::Stream>,
    /// A decided group start whose first frame has not gone out yet.
    pending_group: bool,
    seq: u32,
    frames_sent: u32,
    directed: bool,
    stopped: bool,
    /// The group stream was stopped by MH; withhold until a group start.
    awaiting_group: bool,
    /// A compliant publisher was withdrawn after starting: its next frame
    /// starts a new group (a resume).
    resume_pending: bool,
    /// [`Fault::AbandonMidGroup`] is one-shot.
    abandon_fired: bool,
}

impl<T: UplinkTransport> VideoPublisher<T> {
    /// A publisher over `transport`, directed from the start.
    ///
    /// # Panics
    ///
    /// On an invalid configuration.
    pub fn new(transport: T, cfg: VideoPublisherConfig) -> Self {
        cfg.validate();
        Self {
            sched: GroupScheduler::new(cfg.group_start, cfg.directed_send_id),
            cfg,
            transport,
            stream: None,
            stalled: None,
            pending_group: false,
            seq: 0,
            frames_sent: 0,
            directed: true,
            stopped: false,
            awaiting_group: false,
            resume_pending: false,
            abandon_fired: false,
        }
    }

    /// What the publisher's directive says now (withdrawal or mute stops the
    /// camera). A COMPLIANT publisher:
    /// * on withdrawal finishes the current group stream at the frame boundary
    ///   and answers no keyframe request while withdrawn (`NotSending`: a
    ///   request never makes an undirected stream send);
    /// * on re-direction starts a NEW group with the next counter
    ///   ([`GroupStartCause::Resume`](super::schedule::GroupStartCause::Resume)),
    ///   never resuming mid-group — MH forwards a paused or waiting source from
    ///   the next group start.
    ///
    /// The patched `KeepSendingWhileMuted` publisher ignores all of this.
    pub async fn set_directed(&mut self, directed: bool) {
        let was = self.directed;
        self.directed = directed;
        if self.cfg.mode != PublisherMode::Compliant {
            return;
        }
        self.sched.set_sending(directed);
        if was && !directed {
            self.finish_stream().await;
            self.pending_group = false;
            self.resume_pending = self.sched.current().is_some();
        }
    }

    /// Finish the current group stream; a STOP_SENDING here is recorded, not
    /// fatal (the stream is gone either way).
    async fn finish_stream(&mut self) {
        if let Some(mut s) = self.stream.take() {
            match s.finish().await {
                Ok(()) => {}
                Err(UplinkError::Stopped(_)) => self.sched.record_stopped_by_peer(),
                Err(UplinkError::Other(e)) => {
                    panic!("video publisher: finishing a group failed: {e}")
                }
            }
        }
    }

    /// MH stopped the stream under a write: record it, drop the stream, and
    /// withhold frames until the next group start.
    fn stopped_by_peer(&mut self) -> FrameOutcome {
        self.sched.record_stopped_by_peer();
        self.stream = None;
        self.awaiting_group = true;
        FrameOutcome::StoppedByPeer
    }

    /// The ordered request / group-start log.
    #[must_use]
    pub fn events(&self) -> &[PublisherEvent] {
        self.sched.events()
    }

    /// Whether the publisher has stopped.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// The transport, e.g. to read a recorder.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn label(&self) -> u16 {
        match self.cfg.label {
            UplinkLabel::Directed => self.cfg.directed_send_id,
            UplinkLabel::Override(id) => id,
        }
    }

    fn apply(&mut self, decision: Option<Decision>) {
        match decision {
            Some(Decision::GroupStart { .. }) => self.pending_group = true,
            Some(Decision::Exhausted) => self.stopped = true,
            None => {}
        }
    }

    /// Apply one keyframe request at the current tokio instant.
    pub fn on_request(&mut self, stream_number: u32, gop_counter: u32) {
        let d = self
            .sched
            .on_request(stream_number, gop_counter, Instant::now());
        self.apply(d);
    }

    /// Fire whatever the scheduler has due now (periodic, deferred).
    pub fn poll(&mut self) {
        let d = self.sched.poll(Instant::now());
        self.apply(d);
    }

    /// Send one frame (one tick), starting the first group if needed and
    /// rolling the stream over on a decided group start.
    ///
    /// # Panics
    ///
    /// On a transport failure, or a datagram larger than the peer accepts.
    pub async fn send_frame(&mut self) -> FrameOutcome {
        if self.stopped {
            return FrameOutcome::Stopped;
        }
        if self.cfg.mode == PublisherMode::Compliant && !self.directed {
            return FrameOutcome::NotDirected;
        }
        if self.sched.current().is_none() {
            let d = self.sched.start(Instant::now());
            self.apply(d);
        }
        if std::mem::take(&mut self.resume_pending) {
            let d = self.sched.resume(Instant::now());
            self.apply(d);
        }
        if self.stopped {
            return FrameOutcome::Stopped;
        }
        if self.awaiting_group {
            if !self.pending_group {
                return FrameOutcome::AwaitingGroupStart;
            }
            self.awaiting_group = false;
        }
        let group_start = std::mem::take(&mut self.pending_group);
        let gop_counter = self.sched.current().expect("a group has started");
        self.seq = self.seq.wrapping_add(1);
        let bytes = encode_video_frame(&VideoFrameSpec {
            sender_id: self.cfg.sender_id,
            key_id_stream: self.cfg.directed_send_id,
            label: self.label(),
            stream_sequence: self.seq,
            gop_counter,
            group_start,
            payload_len: self.cfg.payload_len,
        });

        if self.cfg.transport == VideoTransport::Datagram {
            let max = self
                .transport
                .max_datagram_size()
                .expect("video publisher: the peer accepts no datagrams");
            assert!(
                bytes.len() <= max,
                "video publisher: a frame does not fit one datagram (never truncated or split); \
                 lower payload_len"
            );
            self.transport
                .send_datagram(bytes)
                .unwrap_or_else(|e| panic!("video publisher: datagram send failed: {e}"));
            self.frames_sent += 1;
            return FrameOutcome::Sent { group_start };
        }

        if group_start || self.stream.is_none() {
            self.finish_stream().await;
            self.stream =
                Some(self.transport.open_uni().await.unwrap_or_else(|e| {
                    panic!("video publisher: opening a group stream failed: {e}")
                }));
        }
        let stream = self.stream.as_mut().expect("a group stream is open");

        match self.cfg.fault {
            Fault::StallMidFrame { after_frames } if self.frames_sent == after_frames => {
                // The whole fixed prefix and then some — a header and part of a
                // payload — but never the whole frame.
                let cut = (PUBLISHER_FIXED_PREFIX_SIZE + bytes.len()) / 2;
                match stream.write_all(&bytes[..cut]).await {
                    Ok(()) => {}
                    Err(UplinkError::Stopped(_)) => return self.stopped_by_peer(),
                    Err(UplinkError::Other(e)) => {
                        panic!("video publisher: stall write failed: {e}")
                    }
                }
                self.stalled = self.stream.take();
                self.stopped = true;
                return FrameOutcome::Stalled;
            }
            Fault::ResetMidGroup { after_frames } if self.frames_sent == after_frames => {
                stream
                    .reset(RESET_MID_GROUP_CODE)
                    .unwrap_or_else(|e| panic!("video publisher: reset failed: {e}"));
                self.stream = None;
                self.stopped = true;
                return FrameOutcome::Reset;
            }
            Fault::AbandonMidGroup { after_frames }
                if !self.abandon_fired && self.frames_sent == after_frames =>
            {
                stream
                    .reset(RESET_MID_GROUP_CODE)
                    .unwrap_or_else(|e| panic!("video publisher: reset failed: {e}"));
                self.stream = None;
                self.abandon_fired = true;
                // The publisher DECIDED this abandonment, so it self-keys the
                // target (MH-decided STOP_SENDING never reaches here). Frames
                // are withheld until the decided group start.
                self.awaiting_group = true;
                let d = self.sched.on_target_abandoned(Instant::now());
                self.apply(d);
                return FrameOutcome::Abandoned;
            }
            _ => {}
        }
        match stream.write_all(&bytes).await {
            Ok(()) => {}
            Err(UplinkError::Stopped(_)) => return self.stopped_by_peer(),
            Err(UplinkError::Other(e)) => panic!("video publisher: frame write failed: {e}"),
        }
        self.frames_sent += 1;
        FrameOutcome::Sent { group_start }
    }

    /// Run `ticks` frame ticks at `frame_interval` on tokio time, answering
    /// requests from `source` and firing scheduler deadlines between ticks.
    /// Returns early if the publisher stops. The current group's stream stays
    /// OPEN afterwards (call [`Self::finish`] to end it).
    pub async fn run(&mut self, source: &mut impl KeyframeRequestSource, ticks: u32) {
        let mut tick = tokio::time::interval(self.cfg.frame_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut done = 0;
        while done < ticks && !self.stopped {
            let deadline = self.sched.next_deadline();
            tokio::select! {
                // Requests first, then deadlines, then the tick: deterministic
                // under paused time when they coincide.
                biased;
                req = source.next_request() => self.on_request(req.stream_number, req.gop_counter),
                () = sleep_until_opt(deadline) => self.poll(),
                _ = tick.tick() => {
                    self.send_frame().await;
                    done += 1;
                }
            }
        }
    }

    /// Finish the current group's stream (FIN). A STOP_SENDING is recorded.
    ///
    /// # Panics
    ///
    /// On any other transport failure.
    pub async fn finish(&mut self) {
        self.finish_stream().await;
    }

    /// Release a stalled stream: it is dropped, which a real `SendStream`
    /// turns into a FIN (so MH then sees a partial frame end).
    pub fn release_stall(&mut self) {
        self.stalled = None;
    }
}

async fn sleep_until_opt(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::media::AUDIO_SEND_ID;
    use crate::fixtures::video::control::{KeyframeRequestMsg, NoRequests, ScriptedRequests};
    use crate::fixtures::video::reader::{StreamEnd, StreamFrameReader};
    use crate::fixtures::video::schedule::{GroupStartCause, RequestOutcome};
    use crate::fixtures::video::transport::memory::{MemoryTransport, Wire};
    use media_protocol::codec::decode_datagram;

    fn publisher(cfg: VideoPublisherConfig) -> VideoPublisher<MemoryTransport> {
        VideoPublisher::new(MemoryTransport::new(Some(1200)), cfg)
    }

    fn opened(t: &MemoryTransport) -> Vec<usize> {
        t.wire()
            .into_iter()
            .filter_map(|w| match w {
                Wire::Opened(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    fn parse(t: &MemoryTransport, id: usize) -> crate::fixtures::video::ReceivedStream {
        let mut r = StreamFrameReader::new();
        r.feed(&t.stream_bytes(id)).expect("publisher bytes decode");
        r.finish(false)
    }

    #[tokio::test(start_paused = true)]
    async fn default_publisher_sends_one_group_on_one_open_stream() {
        let mut p = publisher(VideoPublisherConfig::new(0x8001));
        p.run(&mut NoRequests, 10).await;
        let t = p.transport().clone();
        assert_eq!(opened(&t), vec![1], "None: one group, one stream");
        let s = parse(&t, 1);
        assert_eq!(s.frames.len(), 10);
        assert_eq!(s.first_frame_group_start, Some(true));
        assert!(s.frames[1..].iter().all(|f| !f.independently_decodable));
        assert!(s
            .frames
            .iter()
            .all(|f| f.gop_counter == 0 && f.slot_id == VIDEO_SEND_ID));
        assert!(s.frames.iter().all(|f| f.key_id_sender == 0x8001));
        assert!(
            !t.wire().contains(&Wire::Finished(1)),
            "the group stays open"
        );
        p.finish().await;
        assert!(p.transport().wire().contains(&Wire::Finished(1)));
    }

    #[tokio::test(start_paused = true)]
    async fn an_answered_request_finishes_the_stream_and_opens_a_keyed_one() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut src = ScriptedRequests(rx);
        let mut p = publisher(VideoPublisherConfig {
            group_start: GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::ZERO,
            },
            ..VideoPublisherConfig::new(7)
        });
        p.run(&mut src, 3).await;
        // One batch per run, so each answered request's group gets frames.
        for batch in [&[(1, 0)][..], &[(1, 0), (0, 5)][..], &[(1, 1)][..]] {
            for &(n, g) in batch {
                tx.send(KeyframeRequestMsg {
                    stream_number: n,
                    gop_counter: g,
                })
                .unwrap();
            }
            p.run(&mut src, 2).await;
        }
        let t = p.transport().clone();
        assert_eq!(
            opened(&t),
            vec![1, 2, 3],
            "two answered requests, two new groups"
        );
        let wire = t.wire();
        let pos = |w: &Wire| wire.iter().position(|x| x == w).unwrap();
        assert!(
            pos(&Wire::Finished(1)) < pos(&Wire::Opened(2)),
            "explicit finish before rollover"
        );
        assert!(pos(&Wire::Finished(2)) < pos(&Wire::Opened(3)));
        let g2 = parse(&t, 2);
        let g3 = parse(&t, 3);
        assert_eq!(g2.first_frame_group_start, Some(true));
        assert_eq!(g3.first_frame_group_start, Some(true));
        assert!(g2.frames.iter().all(|f| f.gop_counter == 1));
        assert!(g3.frames.iter().all(|f| f.gop_counter == 2));
        let outcomes: Vec<_> = p
            .events()
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::KeyframeRequest { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes,
            vec![
                RequestOutcome::Answered,
                RequestOutcome::IgnoredLower,
                RequestOutcome::IgnoredOtherStream,
                RequestOutcome::Answered
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn periodic_rolls_a_group_every_interval_on_paused_time() {
        let mut p = publisher(VideoPublisherConfig {
            group_start: GroupStartSource::Periodic(Duration::from_millis(100)),
            ..VideoPublisherConfig::new(3)
        });
        // 20 ms frames for 300 ms of ticks: group starts at 0, 100, 200 ms.
        p.run(&mut NoRequests, 15).await;
        let causes: Vec<_> = p
            .events()
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::GroupStart { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect();
        assert_eq!(
            causes,
            vec![
                GroupStartCause::StreamStart,
                GroupStartCause::Periodic,
                GroupStartCause::Periodic
            ]
        );
        let t = p.transport().clone();
        assert_eq!(opened(&t), vec![1, 2, 3]);
        for id in [1, 2, 3] {
            let s = parse(&t, id);
            assert_eq!(
                s.first_frame_group_start,
                Some(true),
                "stream {id} starts at a group"
            );
            assert_eq!(
                s.frames
                    .iter()
                    .filter(|f| f.independently_decodable)
                    .count(),
                1
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_label_knob_relabels_without_touching_the_key_id_stream() {
        let mut p = publisher(VideoPublisherConfig {
            label: UplinkLabel::Override(AUDIO_SEND_ID),
            transport: VideoTransport::Datagram,
            ..VideoPublisherConfig::new(0x0102)
        });
        p.run(&mut NoRequests, 3).await;
        let grams: Vec<_> = p
            .transport()
            .wire()
            .into_iter()
            .filter_map(|w| match w {
                Wire::Datagram(b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(grams.len(), 3);
        assert!(
            opened(p.transport()).is_empty(),
            "datagram transport opens no stream"
        );
        for (i, g) in grams.iter().enumerate() {
            let v = decode_datagram(g).expect("each frame is one datagram");
            assert_eq!(v.stream_id(), AUDIO_SEND_ID, "relabelled to the audio id");
            assert_eq!(
                v.payload()[2],
                1,
                "key-id stream stays the directed video id"
            );
            assert_eq!(v.flags().independently_decodable, i == 0);
        }
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "does not fit one datagram")]
    async fn an_oversized_datagram_fails_loudly() {
        let mut p = VideoPublisher::new(
            MemoryTransport::new(Some(100)),
            VideoPublisherConfig {
                transport: VideoTransport::Datagram,
                ..VideoPublisherConfig::new(1)
            },
        );
        p.send_frame().await;
    }

    #[tokio::test(start_paused = true)]
    async fn compliant_obeys_withdrawal_and_the_patched_client_does_not() {
        let mut c = publisher(VideoPublisherConfig::new(1));
        c.set_directed(false).await;
        assert_eq!(c.send_frame().await, FrameOutcome::NotDirected);
        assert!(
            c.transport().wire().is_empty(),
            "nothing on the wire while withdrawn"
        );
        c.set_directed(true).await;
        assert_eq!(
            c.send_frame().await,
            FrameOutcome::Sent { group_start: true }
        );

        let mut k = publisher(VideoPublisherConfig {
            mode: PublisherMode::KeepSendingWhileMuted,
            ..VideoPublisherConfig::new(1)
        });
        k.set_directed(false).await;
        assert_eq!(
            k.send_frame().await,
            FrameOutcome::Sent { group_start: true }
        );
        assert_eq!(
            k.send_frame().await,
            FrameOutcome::Sent { group_start: false }
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stall_parks_a_partial_frame_reset_resets_and_rollover_finishes() {
        // Stall: two whole frames, then header + part of a payload, held open.
        let mut s = publisher(VideoPublisherConfig {
            fault: Fault::StallMidFrame { after_frames: 2 },
            ..VideoPublisherConfig::new(1)
        });
        s.run(&mut NoRequests, 5).await;
        assert!(s.is_stopped());
        let t = s.transport().clone();
        let mut r = StreamFrameReader::new();
        r.feed(&t.stream_bytes(1)).unwrap();
        assert_eq!(r.frames().len(), 2);
        assert!(r.mid_frame(), "a header and part of a payload are owed");
        let w = t.wire();
        assert!(!w.contains(&Wire::Finished(1)) && !w.contains(&Wire::Dropped(1)));
        assert!(
            !w.iter().any(|x| matches!(x, Wire::Reset(..))),
            "held, neither finished nor reset"
        );
        s.release_stall();
        assert!(
            t.wire().contains(&Wire::Dropped(1)),
            "released only on request"
        );
        assert_eq!(r.finish(false).end, StreamEnd::Partial);

        // Reset: explicit reset with the documented code, never a finish.
        let mut x = publisher(VideoPublisherConfig {
            fault: Fault::ResetMidGroup { after_frames: 3 },
            ..VideoPublisherConfig::new(1)
        });
        x.run(&mut NoRequests, 10).await;
        let w = x.transport().wire();
        assert!(w.contains(&Wire::Reset(1, RESET_MID_GROUP_CODE)));
        assert!(!w.contains(&Wire::Finished(1)) && !w.contains(&Wire::Dropped(1)));
        assert_eq!(parse(x.transport(), 1).frames.len(), 3);
        assert_eq!(x.send_frame().await, FrameOutcome::Stopped);
    }

    fn group_starts(p: &VideoPublisher<MemoryTransport>) -> Vec<GroupStartCause> {
        p.events()
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::GroupStart { cause, .. } => Some(*cause),
                _ => None,
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn abandon_mid_group_resets_keeps_running_and_self_keys() {
        let mut p = publisher(VideoPublisherConfig {
            fault: Fault::AbandonMidGroup { after_frames: 3 },
            ..VideoPublisherConfig::new(1)
        });
        p.run(&mut NoRequests, 8).await;
        assert!(!p.is_stopped(), "the publisher keeps running");
        let t = p.transport().clone();
        let w = t.wire();
        assert!(w.contains(&Wire::Reset(1, RESET_MID_GROUP_CODE)));
        assert!(!w.contains(&Wire::Finished(1)) && !w.contains(&Wire::Dropped(1)));
        assert_eq!(parse(&t, 1).frames.len(), 3, "the abandoned group's frames");
        assert_eq!(
            opened(&t),
            vec![1, 2],
            "a fresh stream for the self-keyed group"
        );
        let next = parse(&t, 2);
        assert_eq!(next.first_frame_group_start, Some(true));
        assert!(
            next.frames.iter().all(|f| f.gop_counter == 1),
            "counter + 1"
        );
        assert_eq!(
            group_starts(&p),
            vec![
                GroupStartCause::StreamStart,
                GroupStartCause::TargetAbandoned
            ]
        );
        assert_eq!(
            next.frames.len(),
            4,
            "one-shot: the second group runs on (8 ticks - 3 sent - 1 abandon tick)"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_in_interval_abandonment_withholds_frames_until_the_deferred_self_key() {
        let mut p = publisher(VideoPublisherConfig {
            group_start: GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::from_millis(500),
            },
            fault: Fault::AbandonMidGroup { after_frames: 2 },
            frame_interval: Duration::from_millis(20),
            ..VideoPublisherConfig::new(1)
        });
        // 2 frames, the abandon tick, then ticks inside the interval withheld.
        let mut withheld = 0;
        for _ in 0..3 {
            p.send_frame().await;
        }
        for _ in 0..5 {
            if p.send_frame().await == FrameOutcome::AwaitingGroupStart {
                withheld += 1;
            }
        }
        assert_eq!(withheld, 5, "withheld while the self-key is deferred");
        assert_eq!(opened(p.transport()), vec![1]);
        tokio::time::advance(Duration::from_millis(500)).await;
        p.poll();
        assert_eq!(
            p.send_frame().await,
            FrameOutcome::Sent { group_start: true }
        );
        assert_eq!(
            group_starts(&p),
            vec![
                GroupStartCause::StreamStart,
                GroupStartCause::TargetAbandoned
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_with_the_old_counter_after_a_self_key_is_ignored() {
        let mut p = publisher(VideoPublisherConfig {
            group_start: GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::ZERO,
            },
            fault: Fault::AbandonMidGroup { after_frames: 1 },
            ..VideoPublisherConfig::new(1)
        });
        p.send_frame().await;
        assert_eq!(p.send_frame().await, FrameOutcome::Abandoned);
        // MH's Waiting-path request carries the abandoned group's counter (0).
        p.on_request(1, 0);
        assert_eq!(
            group_starts(&p),
            vec![
                GroupStartCause::StreamStart,
                GroupStartCause::TargetAbandoned
            ],
            "no extra group"
        );
        assert!(p.events().contains(&PublisherEvent::KeyframeRequest {
            stream_number: 1,
            gop_counter: 0,
            outcome: RequestOutcome::IgnoredLower,
        }));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_by_mh_never_self_keys() {
        // Positive control first: an abandonment the publisher decided does.
        let mut keyed = publisher(VideoPublisherConfig {
            fault: Fault::AbandonMidGroup { after_frames: 1 },
            ..VideoPublisherConfig::new(1)
        });
        keyed.send_frame().await;
        keyed.send_frame().await;
        assert!(group_starts(&keyed).contains(&GroupStartCause::TargetAbandoned));

        let mut t = MemoryTransport::new(None);
        t.stop.on_write = Some((1, 2));
        let mut p = VideoPublisher::new(t, VideoPublisherConfig::new(4));
        p.send_frame().await;
        assert_eq!(p.send_frame().await, FrameOutcome::StoppedByPeer);
        for _ in 0..5 {
            assert_eq!(p.send_frame().await, FrameOutcome::AwaitingGroupStart);
        }
        assert_eq!(
            group_starts(&p),
            vec![GroupStartCause::StreamStart],
            "MH's stop is MH's to repair by request"
        );
    }

    #[test]
    #[should_panic(expected = "16-bit sender field")]
    fn a_sender_id_wider_than_the_key_id_field_is_never_masked() {
        let _ = VideoPublisherConfig::new(0x1_0000);
    }

    #[test]
    #[should_panic(expected = "at least the key id")]
    fn a_payload_shorter_than_the_key_id_is_refused_at_config_time() {
        let _ = VideoPublisher::new(
            MemoryTransport::new(None),
            VideoPublisherConfig {
                payload_len: KEY_ID_BYTES - 1,
                ..VideoPublisherConfig::new(1)
            },
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_sending_on_write_is_an_outcome_and_frames_wait_for_the_next_group() {
        let mut t = MemoryTransport::new(None);
        t.stop.on_write = Some((1, 3));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut src = ScriptedRequests(rx);
        let mut p = VideoPublisher::new(
            t,
            VideoPublisherConfig {
                group_start: GroupStartSource::OnKeyframeRequest {
                    min_interval: Duration::ZERO,
                },
                ..VideoPublisherConfig::new(4)
            },
        );
        assert_eq!(
            p.send_frame().await,
            FrameOutcome::Sent { group_start: true }
        );
        assert_eq!(
            p.send_frame().await,
            FrameOutcome::Sent { group_start: false }
        );
        assert_eq!(
            p.send_frame().await,
            FrameOutcome::StoppedByPeer,
            "no panic"
        );
        assert_eq!(p.send_frame().await, FrameOutcome::AwaitingGroupStart);
        assert!(p.events().contains(&PublisherEvent::StreamStoppedByPeer));
        tx.send(KeyframeRequestMsg {
            stream_number: 1,
            gop_counter: 0,
        })
        .unwrap();
        p.run(&mut src, 2).await;
        let t = p.transport().clone();
        assert_eq!(
            opened(&t),
            vec![1, 2],
            "a fresh stream only at the next group start"
        );
        let g = parse(&t, 2);
        assert_eq!(g.first_frame_group_start, Some(true));
        assert!(g.frames.iter().all(|f| f.gop_counter == 1));
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_sending_on_finish_is_recorded_not_fatal() {
        let mut t = MemoryTransport::new(None);
        t.stop.on_finish = Some(1);
        let mut p = VideoPublisher::new(
            t,
            VideoPublisherConfig {
                group_start: GroupStartSource::Periodic(Duration::from_millis(40)),
                ..VideoPublisherConfig::new(4)
            },
        );
        p.run(&mut NoRequests, 4).await;
        assert!(p.events().contains(&PublisherEvent::StreamStoppedByPeer));
        assert_eq!(
            opened(p.transport()),
            vec![1, 2],
            "the next group still opens"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_compliant_withdrawal_finishes_the_group_and_a_resume_starts_a_new_one() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut src = ScriptedRequests(rx);
        let mut p = publisher(VideoPublisherConfig {
            group_start: GroupStartSource::OnKeyframeRequest {
                min_interval: Duration::ZERO,
            },
            ..VideoPublisherConfig::new(2)
        });
        p.run(&mut src, 3).await;
        p.set_directed(false).await;
        assert!(
            p.transport().wire().contains(&Wire::Finished(1)),
            "the group stream ends at the frame boundary on withdrawal"
        );
        tx.send(KeyframeRequestMsg {
            stream_number: 1,
            gop_counter: 0,
        })
        .unwrap();
        p.run(&mut src, 2).await;
        assert_eq!(
            opened(p.transport()),
            vec![1],
            "withdrawn: no frame, no group"
        );
        p.set_directed(true).await;
        p.run(&mut src, 2).await;
        let t = p.transport().clone();
        assert_eq!(opened(&t), vec![1, 2]);
        let resumed = parse(&t, 2);
        assert_eq!(
            resumed.first_frame_group_start,
            Some(true),
            "never resumed mid-group"
        );
        assert!(
            resumed.frames.iter().all(|f| f.gop_counter == 1),
            "a resume is the next counter"
        );
        let outcomes: Vec<_> = p
            .events()
            .iter()
            .filter_map(|e| match e {
                PublisherEvent::KeyframeRequest { outcome, .. } => Some(*outcome),
                _ => None,
            })
            .collect();
        assert_eq!(outcomes, vec![RequestOutcome::NotSending]);
        assert!(p.events().iter().any(|e| matches!(
            e,
            PublisherEvent::GroupStart {
                cause: GroupStartCause::Resume,
                ..
            }
        )));
    }

    #[test]
    fn config_debug_omits_sender_and_stream_ids() {
        let cfg = VideoPublisherConfig {
            label: UplinkLabel::Override(0x7A7A),
            ..VideoPublisherConfig::new(0x5B5B)
        };
        let text = format!("{cfg:?}");
        for secret in [0x7A7A_u32, 0x5B5B] {
            assert!(!text.contains(&secret.to_string()), "{text}");
        }
        assert_eq!(
            format!("{:?}", UplinkLabel::Override(0x7A7A)),
            "Override(..)"
        );
    }
}
