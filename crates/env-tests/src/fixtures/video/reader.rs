//! The chunk-fed stream parser behind [`super::VideoSubscriber`]: pure, no I/O.
//!
//! Each frame's header goes through `media_protocol::reader::HeaderReader` (the
//! production cut-through reader, which enforces `MAX_PAYLOAD_BYTES` against the
//! declared length before a single tail byte is wanted); then exactly
//! `tail_len()` tail bytes are COUNTED, not buffered. Only the first
//! `KEY_ID_BYTES` of the payload are kept, for the key-id sender. Nothing is
//! ever sized from `payload_length`.

use media_protocol::codec::RejectReason;
use media_protocol::frame::KEY_ID_BYTES;
use media_protocol::reader::{Feed, HeaderReader};

use super::encode::key_id_sender;

/// One frame as a receiver saw it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ReceivedFrame {
    /// The key id's `sender_id` (the payload's first two bytes).
    pub key_id_sender: u16,
    /// The relay `stream_id` (on a downlink: the receiver's slot id).
    pub slot_id: u16,
    /// The relay hop sequence ([`crate::fixtures::media::HOP_SENTINEL`] = never rewritten).
    pub hop_sequence: u32,
    /// The group counter.
    pub gop_counter: u32,
    /// The per-stream frame sequence.
    pub stream_sequence: u32,
    /// Whether the frame is independently decodable (a group start).
    pub independently_decodable: bool,
}

// Hand-rolled: a frame names a sender and carries a GoP value, which are stream
// identity (R-30) and must not reach a log through a `{:?}`.
impl std::fmt::Debug for ReceivedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceivedFrame")
            .field("independently_decodable", &self.independently_decodable)
            .finish_non_exhaustive()
    }
}

/// How a stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEnd {
    /// FIN at a frame boundary.
    Finished,
    /// The peer reset the stream (whatever was in flight is discarded).
    Reset,
    /// FIN mid-header or mid-tail: the last frame is NOT reported as a frame.
    Partial,
}

/// A stream the parser refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// The production reader rejected the header (e.g.
    /// `payload_length_exceeds_max`). The reason token only.
    Reject(RejectReason),
    /// A payload shorter than the key id: a harness error in this fixture
    /// (every frame it sends leads with a key id), never a sender of 0.
    ShortPayload,
}

/// A whole stream's report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedStream {
    /// Every complete frame, in order.
    pub frames: Vec<ReceivedFrame>,
    /// Whether the first frame was a group start (`None`: no complete frame) —
    /// the "never mid-group" check.
    pub first_frame_group_start: Option<bool>,
    /// Whether the relay slot id changed within the stream (one downlink uni
    /// stream must carry exactly one slot).
    pub slot_changed_within_stream: bool,
    /// How it ended.
    pub end: StreamEnd,
}

struct Tail {
    frame: ReceivedFrame,
    remaining: usize,
    kid: [u8; KEY_ID_BYTES],
    kid_filled: usize,
}

/// Incremental parser for ONE uni stream.
pub struct StreamFrameReader {
    header: HeaderReader,
    tail: Option<Tail>,
    frames: Vec<ReceivedFrame>,
    first_slot: Option<u16>,
    slot_changed: bool,
}

impl Default for StreamFrameReader {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamFrameReader {
    /// A parser at the start of a stream.
    #[must_use]
    pub fn new() -> Self {
        Self {
            header: HeaderReader::new(),
            tail: None,
            frames: Vec::new(),
            first_slot: None,
            slot_changed: false,
        }
    }

    /// Feed the next chunk, returning how many frames it COMPLETED (they are
    /// appended to [`Self::frames`]).
    ///
    /// # Errors
    ///
    /// A header reject (sticky in the production reader) or a short payload.
    pub fn feed(&mut self, mut chunk: &[u8]) -> Result<usize, ReadError> {
        let before = self.frames.len();
        while !chunk.is_empty() {
            if let Some(tail) = self.tail.as_mut() {
                let take = tail.remaining.min(chunk.len());
                let want_kid = KEY_ID_BYTES - tail.kid_filled;
                let kid_take = want_kid.min(take);
                tail.kid[tail.kid_filled..tail.kid_filled + kid_take]
                    .copy_from_slice(&chunk[..kid_take]);
                tail.kid_filled += kid_take;
                tail.remaining -= take;
                chunk = &chunk[take..];
                if tail.remaining == 0 {
                    let mut frame = tail.frame;
                    frame.key_id_sender =
                        key_id_sender(&tail.kid).ok_or(ReadError::ShortPayload)?;
                    self.tail = None;
                    self.header.reset();
                    self.push(frame);
                }
                continue;
            }
            match self
                .header
                .feed(chunk)
                .map_err(|e| ReadError::Reject(e.reason()))?
            {
                Feed::NeedMore => break,
                Feed::Complete { header, consumed } => {
                    // The tail is payload then signature, so the payload's
                    // first KEY_ID_BYTES are the tail's first bytes — when the
                    // payload is at least that long.
                    if header.payload_length() < KEY_ID_BYTES {
                        return Err(ReadError::ShortPayload);
                    }
                    let flags = header.flags();
                    self.tail = Some(Tail {
                        frame: ReceivedFrame {
                            key_id_sender: 0,
                            slot_id: header.stream_id(),
                            hop_sequence: header.hop_sequence(),
                            gop_counter: header.gop_counter(),
                            stream_sequence: header.stream_sequence(),
                            independently_decodable: flags.independently_decodable,
                        },
                        remaining: header.tail_len(),
                        kid: [0; KEY_ID_BYTES],
                        kid_filled: 0,
                    });
                    chunk = &chunk[consumed..];
                }
            }
        }
        Ok(self.frames.len() - before)
    }

    fn push(&mut self, frame: ReceivedFrame) {
        match self.first_slot {
            None => self.first_slot = Some(frame.slot_id),
            Some(s) if s != frame.slot_id => self.slot_changed = true,
            Some(_) => {}
        }
        self.frames.push(frame);
    }

    /// Every complete frame so far.
    #[must_use]
    pub fn frames(&self) -> &[ReceivedFrame] {
        &self.frames
    }

    /// Whether a frame is partly read (a header byte or a tail byte is owed).
    #[must_use]
    pub fn mid_frame(&self) -> bool {
        self.tail.is_some() || self.header.is_mid_header()
    }

    /// Close the stream: `reset` says the peer reset it; otherwise it was a FIN,
    /// which is [`StreamEnd::Partial`] if it landed mid-frame.
    #[must_use]
    pub fn finish(self, reset: bool) -> ReceivedStream {
        let end = if reset {
            StreamEnd::Reset
        } else if self.mid_frame() {
            StreamEnd::Partial
        } else {
            StreamEnd::Finished
        };
        ReceivedStream {
            first_frame_group_start: self.frames.first().map(|f| f.independently_decodable),
            slot_changed_within_stream: self.slot_changed,
            frames: self.frames,
            end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::media::HOP_SENTINEL;
    use crate::fixtures::media::VIDEO_SEND_ID;
    use crate::fixtures::video::encode::{encode_video_frame, VideoFrameSpec};
    use media_protocol::frame::{
        MAX_PAYLOAD_BYTES, PAYLOAD_LENGTH_OFFSET, PUBLISHER_FIXED_PREFIX_SIZE,
    };

    fn frame(seq: u32, gop: u32, start: bool, sender: u16, len: usize) -> Vec<u8> {
        encode_video_frame(&VideoFrameSpec {
            sender_id: sender,
            key_id_stream: VIDEO_SEND_ID,
            label: VIDEO_SEND_ID,
            stream_sequence: seq,
            gop_counter: gop,
            group_start: start,
            payload_len: len,
        })
        .to_vec()
    }

    fn expected() -> Vec<ReceivedFrame> {
        vec![
            ReceivedFrame {
                key_id_sender: 0x1234,
                slot_id: VIDEO_SEND_ID,
                hop_sequence: HOP_SENTINEL,
                gop_counter: 5,
                stream_sequence: 10,
                independently_decodable: true,
            },
            ReceivedFrame {
                key_id_sender: 0x1234,
                slot_id: VIDEO_SEND_ID,
                hop_sequence: HOP_SENTINEL,
                gop_counter: 5,
                stream_sequence: 11,
                independently_decodable: false,
            },
        ]
    }

    #[test]
    fn a_two_frame_stream_round_trips_split_at_every_byte_offset() {
        let mut stream = frame(10, 5, true, 0x1234, 40);
        stream.extend(frame(11, 5, false, 0x1234, 9));
        for cut in 0..=stream.len() {
            let mut r = StreamFrameReader::new();
            let (a, b) = stream.split_at(cut);
            r.feed(a)
                .unwrap_or_else(|_| panic!("first half rejected at cut {cut}"));
            r.feed(b)
                .unwrap_or_else(|_| panic!("second half rejected at cut {cut}"));
            let got = r.finish(false);
            assert_eq!(got.frames, expected(), "fields differ at cut {cut}");
            assert_eq!(got.end, StreamEnd::Finished, "cut {cut}");
            assert_eq!(got.first_frame_group_start, Some(true));
            assert!(!got.slot_changed_within_stream);
        }
    }

    #[test]
    fn byte_at_a_time_feeding_matches_one_shot() {
        let mut stream = frame(10, 5, true, 0x1234, 40);
        stream.extend(frame(11, 5, false, 0x1234, 9));
        let mut r = StreamFrameReader::new();
        let mut completed = 0;
        for b in &stream {
            completed += r.feed(std::slice::from_ref(b)).unwrap();
        }
        assert_eq!(completed, 2);
        assert_eq!(r.finish(false).frames, expected());
    }

    #[test]
    fn an_over_max_declared_length_rejects_before_any_tail_byte() {
        // A real header with its declared length raised past the maximum, cut
        // to the fixed prefix: the reader must refuse WITHOUT being fed (or
        // allocating for) a single payload byte.
        let mut raw = frame(1, 0, true, 1, 16);
        let too_big = u32::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_be_bytes();
        raw[PAYLOAD_LENGTH_OFFSET..PAYLOAD_LENGTH_OFFSET + 4].copy_from_slice(&too_big);
        let mut r = StreamFrameReader::new();
        let err = r.feed(&raw[..PUBLISHER_FIXED_PREFIX_SIZE]).unwrap_err();
        assert_eq!(
            err,
            ReadError::Reject(RejectReason::PayloadLengthExceedsMax)
        );
        assert_eq!(err_token(err), "payload_length_exceeds_max");
    }

    fn err_token(e: ReadError) -> &'static str {
        match e {
            ReadError::Reject(r) => r.as_str(),
            ReadError::ShortPayload => "short_payload",
        }
    }

    #[test]
    fn ending_mid_header_or_mid_tail_is_partial_never_a_frame() {
        let one = frame(10, 5, true, 0x1234, 40);
        for cut in [1, PUBLISHER_FIXED_PREFIX_SIZE, one.len() - 1] {
            let mut r = StreamFrameReader::new();
            r.feed(&one[..cut]).unwrap();
            let got = r.finish(false);
            assert_eq!(got.end, StreamEnd::Partial, "cut {cut}");
            assert!(got.frames.is_empty(), "no frame reported at cut {cut}");
            assert_eq!(got.first_frame_group_start, None);
        }
        let mut r = StreamFrameReader::new();
        r.feed(&one[..5]).unwrap();
        assert_eq!(r.finish(true).end, StreamEnd::Reset);
    }

    #[test]
    fn a_mid_group_first_frame_and_a_slot_change_are_reported() {
        let mut s = frame(3, 2, false, 9, 16);
        let mut other = frame(4, 2, false, 9, 16);
        // Re-stamp the second frame's relay id through the production rewrite.
        let view_len = other.len();
        media_protocol::codec::rewrite_relay_region(&mut other[..view_len], 7, 1).unwrap();
        s.extend(other);
        let mut r = StreamFrameReader::new();
        r.feed(&s).unwrap();
        let got = r.finish(false);
        assert_eq!(
            got.first_frame_group_start,
            Some(false),
            "mid-group start is visible"
        );
        assert!(got.slot_changed_within_stream);
    }

    #[test]
    fn a_payload_shorter_than_a_key_id_is_a_harness_error() {
        // Encode through the production codec directly (the fixture encoder
        // refuses short payloads).
        let raw = media_protocol::codec::encode_frame(&media_protocol::codec::MediaFrameParts {
            flags: media_protocol::frame::FrameFlags::default(),
            stream_sequence: 0,
            gop_counter: 0,
            wrapped_transmit_key: None,
            extensions: &[],
            stream_id: 1,
            hop_sequence: 0,
            payload: &[1, 2, 3],
            signature: &[0; media_protocol::frame::SIGNATURE_SIZE],
        })
        .unwrap();
        assert_eq!(
            StreamFrameReader::new().feed(&raw),
            Err(ReadError::ShortPayload)
        );
    }
}
