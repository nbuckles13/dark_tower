//! The ONE length-prefix framing for the client-opened MH control stream
//! (`MhClientMessage` in, `MhServerMessage` out; `signaling.proto`): a 4-byte
//! big-endian length, then that many bytes, bounded by [`MAX_MESSAGE_SIZE`].
//!
//! Both directions live here, over the transport seam's stream halves, so the
//! connect read, the post-connect recovery-request loop and the keyframe
//! writer share one framer and the deterministic shim reaches the same code as
//! the real transport. No telemetry here: callers count outcomes.

use crate::transport::{MediaRecvStream, StreamReadError};
use bytes::{Buf, Bytes};

/// Maximum size of one framed message body (64 KiB).
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// The fixed parse buffer for a POST-CONNECT message (story 3 R-13): a
/// `RecoveryRequest` is parsed from this stack buffer, with no per-message
/// allocation. A larger body (≤ [`MAX_MESSAGE_SIZE`]) is drained in chunks
/// and counted `malformed`, which keeps the framing in sync.
///
/// The bound is DERIVED, not assumed. A conforming client's largest
/// post-connect envelope (`packages/sdk-core/src/framing/sendFramed.ts`
/// always injects trace context when a span is active):
///
/// | field | bytes |
/// |---|---|
/// | `RecoveryRequest`: 3 x (tag + ≤5-byte varint), + oneof tag + length | 18 + 2 |
/// | `trace_parent` (tag 20): 55 chars + 2-byte tag + length | 55 + 3 |
/// | `trace_state` (tag 21): ≤512 chars (`OTel` JS `MAX_TRACE_STATE_LEN`) + 3 | 512 + 3 |
///
/// ≤ 593 bytes. `framing_tests::a_maximal_legitimate_envelope_fits` encodes
/// that envelope, so a future field pushing a legitimate message past this
/// bound fails a test instead of silently counting `malformed`.
pub const CONTROL_PARSE_BYTES: usize = 1024;

const _: () = assert!(
    CONTROL_PARSE_BYTES <= MAX_MESSAGE_SIZE,
    "the post-connect parse buffer must fit inside the framing bound"
);

/// One frame's body length, read from its prefix.
const PREFIX_BYTES: usize = 4;

/// Frame `body` for the wire: prefix + body in ONE buffer, so one write puts
/// one whole message on the stream. `None` if the body exceeds
/// [`MAX_MESSAGE_SIZE`] or is empty (never truncated: `u32::try_from`).
#[must_use]
pub fn encode_framed(body: &[u8]) -> Option<Vec<u8>> {
    if body.is_empty() || body.len() > MAX_MESSAGE_SIZE {
        return None;
    }
    let len = u32::try_from(body.len()).ok()?;
    let mut framed = Vec::with_capacity(PREFIX_BYTES + body.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(body);
    Some(framed)
}

/// Why a framed read ended without a message.
#[derive(Debug)]
pub enum FrameEnd {
    /// The peer finished the stream (FIN) at a frame boundary or mid-frame.
    Finished,
    /// The stream or connection failed.
    Failed(StreamReadError),
    /// A length prefix of 0 or above [`MAX_MESSAGE_SIZE`]: the framing cannot
    /// be resynchronised.
    FramingBreak,
}

/// What [`FramedReader::next_into`] produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framed {
    /// A message of this many bytes, at the front of the caller's buffer.
    Message(usize),
    /// A well-framed message larger than the caller's buffer, drained.
    Oversize,
}

/// A length-prefixed message reader over one seam receive half. Holds only
/// the transport's leftover chunk between messages.
pub struct FramedReader<R> {
    stream: R,
    leftover: Bytes,
}

impl<R: MediaRecvStream> FramedReader<R> {
    /// Wrap a receive half.
    pub const fn new(stream: R) -> Self {
        Self {
            stream,
            leftover: Bytes::new(),
        }
    }

    /// Fill `out` completely from the stream.
    async fn fill(&mut self, out: &mut [u8]) -> Result<(), FrameEnd> {
        let mut filled = 0;
        while filled < out.len() {
            if self.leftover.is_empty() {
                self.leftover = match self.stream.read_chunk(MAX_MESSAGE_SIZE).await {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => return Err(FrameEnd::Finished),
                    Err(error) => return Err(FrameEnd::Failed(error)),
                };
            }
            let take = (out.len() - filled).min(self.leftover.len());
            if let (Some(dst), Some(src)) = (
                out.get_mut(filled..filled + take),
                self.leftover.get(..take),
            ) {
                dst.copy_from_slice(src);
            }
            self.leftover.advance(take);
            filled += take;
        }
        Ok(())
    }

    /// Discard `len` bytes from the stream, in chunks.
    async fn drain(&mut self, mut len: usize) -> Result<(), FrameEnd> {
        while len > 0 {
            if self.leftover.is_empty() {
                self.leftover = match self.stream.read_chunk(len).await {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => return Err(FrameEnd::Finished),
                    Err(error) => return Err(FrameEnd::Failed(error)),
                };
            }
            let take = len.min(self.leftover.len());
            self.leftover.advance(take);
            len -= take;
        }
        Ok(())
    }

    async fn prefix(&mut self) -> Result<usize, FrameEnd> {
        let mut prefix = [0u8; PREFIX_BYTES];
        self.fill(&mut prefix).await?;
        let len = usize::try_from(u32::from_be_bytes(prefix)).unwrap_or(usize::MAX);
        if len == 0 || len > MAX_MESSAGE_SIZE {
            return Err(FrameEnd::FramingBreak);
        }
        Ok(len)
    }

    /// Read the next message into the caller's fixed buffer (the post-connect
    /// path: no allocation). A body larger than `out` is drained and reported
    /// [`Framed::Oversize`].
    ///
    /// # Errors
    ///
    /// [`FrameEnd`] when the stream ends, fails, or breaks framing.
    pub async fn next_into(&mut self, out: &mut [u8]) -> Result<Framed, FrameEnd> {
        let len = self.prefix().await?;
        if let Some(body) = out.get_mut(..len) {
            self.fill(body).await?;
            Ok(Framed::Message(len))
        } else {
            self.drain(len).await?;
            Ok(Framed::Oversize)
        }
    }

    /// Read the next message into a buffer sized from its prefix (the connect
    /// path, whose meeting token may approach [`MAX_MESSAGE_SIZE`]).
    ///
    /// # Errors
    ///
    /// [`FrameEnd`] when the stream ends, fails, or breaks framing.
    pub async fn next_owned(&mut self) -> Result<Bytes, FrameEnd> {
        let len = self.prefix().await?;
        let mut body = vec![0u8; len];
        self.fill(&mut body).await?;
        Ok(Bytes::from(body))
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use prost::Message;
    use proto_gen::dark_tower::signaling::v1::{
        mh_client_message, MhClientMessage, RecoveryRequest,
    };

    #[test]
    fn encode_prefixes_the_length_and_refuses_out_of_bounds_bodies() {
        let framed = encode_framed(&[1, 2, 3]).unwrap();
        assert_eq!(framed, vec![0, 0, 0, 3, 1, 2, 3]);
        assert!(encode_framed(&[]).is_none());
        assert!(encode_framed(&vec![0; MAX_MESSAGE_SIZE + 1]).is_none());
        assert!(encode_framed(&vec![0; MAX_MESSAGE_SIZE]).is_some());
    }

    #[test]
    fn a_maximal_legitimate_envelope_fits() {
        let envelope = MhClientMessage {
            message: Some(mh_client_message::Message::RecoveryRequest(
                RecoveryRequest {
                    slot_id: u32::MAX,
                    sender_id: u32::MAX,
                    gop_counter: u32::MAX,
                },
            )),
            trace_parent: "0".repeat(55),
            trace_state: "s".repeat(512),
        };
        let encoded = envelope.encode_to_vec();
        assert!(
            encoded.len() <= CONTROL_PARSE_BYTES,
            "a conforming client's largest post-connect envelope ({} bytes) must fit",
            encoded.len()
        );
        let decoded = MhClientMessage::decode(encoded.as_slice()).unwrap();
        assert!(matches!(
            decoded.message,
            Some(mh_client_message::Message::RecoveryRequest(_))
        ));
    }
}
