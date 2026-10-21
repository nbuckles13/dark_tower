//! The keyless video subscriber: reads every incoming uni stream on an MH
//! connection with the pure [`StreamFrameReader`], concurrently (one task per
//! stream — a group stream stays open for the whole group), and reports frames
//! and stream ends through one ordered channel.

use std::time::Duration;
use tokio::sync::mpsc;

use super::reader::{ReadError, ReceivedFrame, ReceivedStream, StreamFrameReader};
use super::transport::{DownlinkRead, DownlinkStream};

/// One observation, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriberEvent {
    /// A complete frame on stream `stream` (a per-subscriber index, from 0 in
    /// accept order).
    Frame {
        /// The stream index.
        stream: usize,
        /// Whether this is the stream's FIRST frame (with
        /// `frame.independently_decodable`, the "never mid-group" check).
        first_in_stream: bool,
        /// The frame.
        frame: ReceivedFrame,
    },
    /// Stream `stream` ended: finished, reset, or partial.
    StreamEnded {
        /// The stream index.
        stream: usize,
        /// Its full report.
        report: ReceivedStream,
    },
    /// Stream `stream` was refused by the parser (e.g.
    /// `payload_length_exceeds_max`).
    Rejected {
        /// The stream index.
        stream: usize,
        /// Why.
        error: ReadError,
    },
    /// The connection failed while stream `stream` was open (neither a FIN nor
    /// a stream reset).
    ConnectionLost {
        /// The stream index.
        stream: usize,
    },
    /// The connection stopped accepting streams: the subscriber is DEAD, and
    /// nothing after this proves an absence.
    ConnectionClosed,
}

/// The result of [`VideoSubscriber::next`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// An observation.
    Event(SubscriberEvent),
    /// Nothing arrived within the bound, on a connection that is still LIVE —
    /// the only outcome an absence assertion may rely on.
    Quiet,
}

/// Subscribes to every uni stream on one MH connection until dropped.
///
/// Dropping it aborts the accept task, whose [`tokio::task::JoinSet`] owns
/// every per-stream reader, so no reader outlives the subscriber.
pub struct VideoSubscriber {
    events: mpsc::UnboundedReceiver<SubscriberEvent>,
    accept: tokio::task::JoinHandle<()>,
    closed: bool,
}

impl VideoSubscriber {
    /// Start accepting uni streams on `conn` (a clone; wtransport connections
    /// are shared handles).
    #[must_use]
    pub fn spawn(conn: wtransport::Connection) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let accept = tokio::spawn(async move {
            let mut readers = tokio::task::JoinSet::new();
            let mut index = 0;
            while let Ok(recv) = conn.accept_uni().await {
                readers.spawn(read_stream(recv, index, tx.clone()));
                index += 1;
            }
            let _ = tx.send(SubscriberEvent::ConnectionClosed);
            // Let the readers drain what they already hold before the set (and
            // with it any reader still running) is dropped.
            while readers.join_next().await.is_some() {}
        });
        Self {
            events,
            accept,
            closed: false,
        }
    }

    /// The next observation, or [`Next::Quiet`] if none arrives within `bound`
    /// (a liveness bound, never a latency assertion).
    ///
    /// # Panics
    ///
    /// When called again after [`SubscriberEvent::ConnectionClosed`] was
    /// returned and nothing else is queued: "quiet" on a dead connection would
    /// make every absence assertion pass vacuously.
    pub async fn next(&mut self, bound: Duration) -> Next {
        match tokio::time::timeout(bound, self.events.recv()).await {
            Err(_elapsed) if !self.closed => Next::Quiet,
            Ok(Some(event)) => {
                if event == SubscriberEvent::ConnectionClosed {
                    self.closed = true;
                }
                Next::Event(event)
            }
            Err(_) | Ok(None) => panic!(
                "video subscriber: the MH connection is closed — an absence observed now would \
                 be vacuous"
            ),
        }
    }
}

impl Drop for VideoSubscriber {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

async fn read_stream(
    mut recv: impl DownlinkStream,
    stream: usize,
    tx: mpsc::UnboundedSender<SubscriberEvent>,
) {
    let mut reader = StreamFrameReader::new();
    let mut buf = vec![0_u8; 16 * 1024];
    let reset = loop {
        match recv.read_chunk(&mut buf).await {
            DownlinkRead::Data(n) => {
                let before = reader.frames().len();
                if let Err(error) = reader.feed(&buf[..n]) {
                    let _ = tx.send(SubscriberEvent::Rejected { stream, error });
                    return;
                }
                for (i, frame) in reader.frames()[before..].iter().enumerate() {
                    let _ = tx.send(SubscriberEvent::Frame {
                        stream,
                        first_in_stream: before + i == 0,
                        frame: *frame,
                    });
                }
            }
            DownlinkRead::Finished => break false,
            DownlinkRead::Reset => break true,
            DownlinkRead::ConnectionLost => {
                let _ = tx.send(SubscriberEvent::ConnectionLost { stream });
                return;
            }
        }
    };
    let _ = tx.send(SubscriberEvent::StreamEnded {
        stream,
        report: reader.finish(reset),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::media::VIDEO_SEND_ID;
    use crate::fixtures::video::encode::{encode_video_frame, VideoFrameSpec};
    use crate::fixtures::video::reader::StreamEnd;
    use crate::fixtures::video::transport::memory::ScriptedDownlink;
    use media_protocol::codec::RejectReason;
    use media_protocol::frame::{
        MAX_PAYLOAD_BYTES, PAYLOAD_LENGTH_OFFSET, PUBLISHER_FIXED_PREFIX_SIZE,
    };
    use std::collections::VecDeque;

    fn frame(seq: u32, start: bool) -> Vec<u8> {
        encode_video_frame(&VideoFrameSpec {
            sender_id: 9,
            key_id_stream: VIDEO_SEND_ID,
            label: VIDEO_SEND_ID,
            stream_sequence: seq,
            gop_counter: 4,
            group_start: start,
            payload_len: 32,
        })
        .to_vec()
    }

    async fn events_of(
        stream: usize,
        steps: Vec<Result<Vec<u8>, DownlinkRead>>,
    ) -> Vec<SubscriberEvent> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        read_stream(ScriptedDownlink(VecDeque::from(steps)), stream, tx).await;
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    /// (stream, first_in_stream, stream_sequence) of each Frame event.
    fn frames(events: &[SubscriberEvent]) -> Vec<(usize, bool, u32)> {
        events
            .iter()
            .filter_map(|e| match e {
                SubscriberEvent::Frame {
                    stream,
                    first_in_stream,
                    frame,
                } => Some((*stream, *first_in_stream, frame.stream_sequence)),
                _ => None,
            })
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn first_in_stream_is_true_only_on_each_streams_frame_zero_across_chunk_splits() {
        let mut bytes = frame(1, true);
        bytes.extend(frame(2, false));
        bytes.extend(frame(3, false));
        // Chunk boundaries inside frame 1's header and inside frame 2's tail.
        let a = frame(1, true).len();
        let cuts = [5, a + 40, bytes.len()];
        let mut steps = Vec::new();
        let mut at = 0;
        for c in cuts {
            steps.push(Ok(bytes[at..c].to_vec()));
            at = c;
        }
        for stream in [0, 1] {
            let ev = events_of(stream, steps.clone()).await;
            assert_eq!(
                frames(&ev),
                vec![(stream, true, 1), (stream, false, 2), (stream, false, 3)]
            );
            match ev.last() {
                Some(SubscriberEvent::StreamEnded { stream: s, report }) => {
                    assert_eq!(*s, stream);
                    assert_eq!(report.end, StreamEnd::Finished);
                    assert_eq!(report.first_frame_group_start, Some(true));
                }
                _ => panic!("the last event must be StreamEnded"),
            }
            assert_eq!(ev.len(), 4);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_reset_ends_the_stream_as_reset() {
        let ev = events_of(2, vec![Ok(frame(1, true)), Err(DownlinkRead::Reset)]).await;
        assert_eq!(frames(&ev), vec![(2, true, 1)]);
        assert!(matches!(
            ev.last(),
            Some(SubscriberEvent::StreamEnded { stream: 2, report }) if report.end == StreamEnd::Reset
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn an_over_max_header_is_rejected_and_never_reported_as_an_end() {
        let mut raw = frame(1, true);
        let too_big = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_be_bytes();
        raw[PAYLOAD_LENGTH_OFFSET..PAYLOAD_LENGTH_OFFSET + 4].copy_from_slice(&too_big);
        let ev = events_of(0, vec![Ok(raw[..PUBLISHER_FIXED_PREFIX_SIZE].to_vec())]).await;
        assert_eq!(
            ev,
            vec![SubscriberEvent::Rejected {
                stream: 0,
                error: ReadError::Reject(RejectReason::PayloadLengthExceedsMax)
            }]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_connection_failure_is_its_own_event_not_a_reset() {
        let ev = events_of(
            5,
            vec![Ok(frame(1, true)), Err(DownlinkRead::ConnectionLost)],
        )
        .await;
        assert_eq!(frames(&ev), vec![(5, true, 1)]);
        assert_eq!(
            ev.last(),
            Some(&SubscriberEvent::ConnectionLost { stream: 5 })
        );
        assert_eq!(ev.len(), 2, "no StreamEnded after a lost connection");
    }

    #[tokio::test(start_paused = true)]
    async fn quiet_is_only_reported_while_the_connection_is_live() {
        let (tx, events) = mpsc::unbounded_channel();
        let mut sub = VideoSubscriber {
            events,
            accept: tokio::spawn(async {}),
            closed: false,
        };
        assert_eq!(sub.next(Duration::from_millis(1)).await, Next::Quiet);
        tx.send(SubscriberEvent::ConnectionClosed).unwrap();
        assert_eq!(
            sub.next(Duration::from_millis(1)).await,
            Next::Event(SubscriberEvent::ConnectionClosed)
        );
        drop(tx);
        let dead = tokio::spawn(async move { sub.next(Duration::from_millis(1)).await });
        assert!(
            dead.await.unwrap_err().is_panic(),
            "quiet on a dead connection panics"
        );
    }
}
