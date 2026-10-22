//! Live-video evidence for MC-programmed meetings (story 3 MC-A; R-3, R-5).
//!
//! MC never names the source of a LIVE video slot (`ACTIVE` carries no
//! `sender_id`, R-5), so "this video slot holds publisher P" can only be
//! proven by what arrives on it: a frame relayed onto the receiver's slot id
//! whose key-id sender is P. That is also the per-entity proof that MC's
//! registration DECLARED P's video stream (stream 1 exists on no other
//! reading), so a frame arriving at all is evidence the declared path ran.
//!
//! Group starts come from the fixture-only `GroupStartSource::Periodic` (never
//! a production keyframe interval; see [`super`]), so a receiver whose edge
//! appears mid-group gets the next group start without depending on MH's
//! keyframe-request path, which `39_mh_video_keyframes.rs` owns.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    GroupStartSource, Next, NoRequests, ReceivedFrame, SubscriberEvent, VideoPublisher,
    VideoPublisherConfig, VideoSubscriber,
};

/// The fixture's group cadence for probes (fixture-only; short so a late edge
/// waits at most one group).
pub const PROBE_GROUP_INTERVAL: Duration = Duration::from_millis(250);
/// The probe publisher's frame cadence.
pub const PROBE_FRAME_INTERVAL: Duration = Duration::from_millis(33);
/// Ticks per run slice between stop-flag checks.
const SLICE_TICKS: u32 = 5;
/// Per-read bound while waiting; a quiet read is not a failure until the
/// caller's deadline.
const READ_SLICE: Duration = Duration::from_millis(200);

/// One compliant video publisher running in the background on one MH
/// connection until [`BackgroundPublisher::stop`].
pub struct BackgroundPublisher {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl BackgroundPublisher {
    /// Start publishing `sender_id`'s video (directed send id 1, stream per
    /// group) on `conn`.
    #[must_use]
    pub fn start(conn: wtransport::Connection, sender_id: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let config = VideoPublisherConfig {
            group_start: GroupStartSource::Periodic(PROBE_GROUP_INTERVAL),
            frame_interval: PROBE_FRAME_INTERVAL,
            ..VideoPublisherConfig::new(sender_id)
        };
        let task = tokio::spawn(async move {
            let mut publisher = VideoPublisher::new(conn, config);
            while !flag.load(Ordering::SeqCst) && !publisher.is_stopped() {
                publisher.run(&mut NoRequests, SLICE_TICKS).await;
            }
            publisher.finish().await;
        });
        Self { stop, task }
    }

    /// Stop and finish the current group (FIN).
    ///
    /// # Panics
    ///
    /// If the publisher task panicked (its own failure, surfaced here).
    pub async fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.task
            .await
            .expect("a background video publisher panicked");
    }
}

/// Read `sub` until a frame relayed onto `slot` arrives, and require it (and
/// every earlier frame on `slot`) to carry key-id sender `sender`. Frames on
/// other slots are ignored (other publishers may be live). Returns the frame.
///
/// # Panics
///
/// On a frame on `slot` from another sender (misattribution), on the
/// subscriber's connection closing, or when nothing qualifying arrives within
/// `deadline` (a FAILURE bound). Text names `phase`/`who` and the slot only —
/// never a key id or counter (R-30).
pub async fn until_video_on_slot(
    phase: &str,
    who: &str,
    sub: &mut VideoSubscriber,
    slot: u32,
    sender: u32,
    deadline: Duration,
) -> ReceivedFrame {
    let until = Instant::now() + deadline;
    loop {
        assert!(
            Instant::now() < until,
            "{phase}: {who} received no video frame on its slot {slot} within {deadline:?}. The \
             expected publisher was directed and sending; check the publisher's SendDirective \
             targets, mh_media_frames_dropped_total{{reason}} and mh_media_egress_edges on the \
             handler carrying this edge."
        );
        match sub.next(READ_SLICE).await {
            Next::Quiet => {}
            Next::Event(SubscriberEvent::Frame { frame, .. })
                if u32::from(frame.slot_id) == slot =>
            {
                assert_eq!(
                    u32::from(frame.key_id_sender),
                    sender,
                    "{phase}: {who}'s video slot {slot} carried ANOTHER publisher's frame — the \
                     slot's occupant is not the one the fill rule names"
                );
                return frame;
            }
            Next::Event(SubscriberEvent::ConnectionClosed) => {
                panic!("{phase}: {who}'s MH connection closed while waiting for video")
            }
            Next::Event(_) => {}
        }
    }
}
