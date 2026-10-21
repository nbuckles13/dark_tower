//! The MH carrier stream's story-3 control messages: `keyframe_request` in
//! (MH → publisher), `recovery_request` out (receiver → MH). Both ride the
//! client-opened bidi stream MH accepts once, after `MhConnectRequest`, with
//! the same 4-byte big-endian length prefix and the same per-message bound
//! ([`crate::fixtures::mc_session::MAX_FRAME_BYTES`]).
//!
//! Since MH-B (task 10), MH judges every post-connect recovery request (one
//! counted outcome each) and writes its own keyframe requests here, once per
//! (source stream, counter), never retried.

#![expect(
    async_fn_in_trait,
    reason = "env-tests is single-runtime test code; callers never need a Send bound"
)]

use prost::Message;
use proto_gen::dark_tower::signaling::v1::{
    mh_client_message, mh_server_message, MhClientMessage, MhServerMessage, RecoveryRequest,
};

use crate::fixtures::mc_session::{checked_frame_len, frame, MhSession};

/// One `MhServerMessage.keyframe_request`, as scalars.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyframeRequestMsg {
    /// The publisher's uplink relay id for the stream to key (MC's send id).
    pub stream_number: u32,
    /// The newest counter MH has seen on that stream.
    pub gop_counter: u32,
}

// Hand-rolled: GoP values and stream numbers are stream identity (R-30).
impl std::fmt::Debug for KeyframeRequestMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyframeRequestMsg").finish_non_exhaustive()
    }
}

/// Where a publisher's keyframe requests come from.
///
/// `next_request` MUST be cancel-safe (it is raced against the frame tick in
/// [`super::VideoPublisher::run`]) and, once the source is closed, must stay
/// pending forever rather than returning repeatedly.
pub trait KeyframeRequestSource {
    /// The next request.
    async fn next_request(&mut self) -> KeyframeRequestMsg;
}

/// A source that never yields (publishers under `None` / `Periodic`).
pub struct NoRequests;

impl KeyframeRequestSource for NoRequests {
    async fn next_request(&mut self) -> KeyframeRequestMsg {
        std::future::pending().await
    }
}

/// Decode one carrier body as a keyframe request.
///
/// # Panics
///
/// On a malformed message or an empty oneof: MH must only send valid messages,
/// so either is a defect to surface, never a message to skip.
#[must_use]
pub fn decode_keyframe_request(body: &[u8]) -> KeyframeRequestMsg {
    let msg = MhServerMessage::decode(body)
        .unwrap_or_else(|_| panic!("carrier: MH sent a malformed MhServerMessage"));
    match msg.message {
        Some(mh_server_message::Message::KeyframeRequest(k)) => KeyframeRequestMsg {
            stream_number: k.stream_number,
            gop_counter: k.gop_counter,
        },
        None => panic!("carrier: MH sent an MhServerMessage with no message variant"),
    }
}

/// Keyframe requests read from an [`MhSession`]'s carrier receive half.
///
/// Not [`crate::fixtures::mc_session::read_framed`]: that read is not
/// cancel-safe (a partial `read_exact` lost to a `select!` desynchronises the
/// framing), and this one is raced against the frame tick. So the partial frame
/// lives in `buf` across cancellations and only cancel-safe `read` calls touch
/// the stream. Same bound ([`crate::fixtures::mc_session::MAX_FRAME_BYTES`]), checked before allocating.
pub struct CarrierRequests<'a> {
    session: &'a mut MhSession,
    buf: Vec<u8>,
    closed: bool,
}

impl<'a> CarrierRequests<'a> {
    /// Read requests from `session`'s carrier (borrowed, never moved: the
    /// session keeps the stream open).
    pub fn new(session: &'a mut MhSession) -> Self {
        Self {
            session,
            buf: Vec::new(),
            closed: false,
        }
    }
}

/// Take one complete length-prefixed body off the front of `buf`, if present.
/// The length is bounded before anything is allocated for the body.
///
/// # Panics
///
/// On a length prefix outside `1..=`[`crate::fixtures::mc_session::MAX_FRAME_BYTES`].
fn take_carrier_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let len: [u8; 4] = buf.get(..4)?.try_into().ok()?;
    let n = checked_frame_len(len, "carrier: MH frame");
    if buf.len() < 4 + n {
        return None;
    }
    let body = buf[4..4 + n].to_vec();
    buf.drain(..4 + n);
    Some(body)
}

impl KeyframeRequestSource for CarrierRequests<'_> {
    async fn next_request(&mut self) -> KeyframeRequestMsg {
        loop {
            if let Some(body) = take_carrier_frame(&mut self.buf) {
                return decode_keyframe_request(&body);
            }
            if self.closed {
                return std::future::pending().await;
            }
            let mut chunk = [0_u8; 4096];
            let (_, recv) = self.session.control();
            match recv.read(&mut chunk).await {
                Ok(Some(n)) => self.buf.extend_from_slice(&chunk[..n]),
                Ok(None) => {
                    assert!(
                        self.buf.is_empty(),
                        "carrier: MH ended its send half mid-message"
                    );
                    self.closed = true;
                }
                Err(e) => panic!("carrier: reading MH's send half failed: {e}"),
            }
        }
    }
}

/// `MhClientMessage.recovery_request { slot_id, sender_id, gop_counter }`,
/// length-framed for the carrier. Every field explicit, so MH-B's outcomes are
/// driven by value (see the recovery table in [`super`]); any `sender_id` is
/// allowed — a wrong one is how `source_mismatch` is driven.
#[must_use]
pub fn recovery_request_frame(slot_id: u32, sender_id: u32, gop_counter: u32) -> Vec<u8> {
    let msg = MhClientMessage {
        message: Some(mh_client_message::Message::RecoveryRequest(
            RecoveryRequest {
                slot_id,
                sender_id,
                gop_counter,
            },
        )),
        trace_parent: String::new(),
        trace_state: String::new(),
    };
    frame(&msg.encode_to_vec())
}

/// Write a recovery request on `session`'s carrier send half (never finished).
///
/// # Panics
///
/// If the write fails (the carrier is the session; a dead one is terminal).
pub async fn send_recovery_request(
    session: &mut MhSession,
    slot_id: u32,
    sender_id: u32,
    gop_counter: u32,
) {
    let (send, _) = session.control();
    send.write_all(&recovery_request_frame(slot_id, sender_id, gop_counter))
        .await
        .unwrap_or_else(|e| panic!("carrier: writing a recovery request failed: {e}"));
}

/// A scripted request source for the self-tests.
#[cfg(test)]
pub(crate) struct ScriptedRequests(
    pub(crate) tokio::sync::mpsc::UnboundedReceiver<KeyframeRequestMsg>,
);

#[cfg(test)]
impl KeyframeRequestSource for ScriptedRequests {
    async fn next_request(&mut self) -> KeyframeRequestMsg {
        match self.0.recv().await {
            Some(r) => r,
            None => std::future::pending().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::mc_session::MAX_FRAME_BYTES;
    use proto_gen::dark_tower::signaling::v1::KeyframeRequest;

    #[test]
    fn a_recovery_request_round_trips_with_every_field_non_default() {
        let framed = recovery_request_frame(0x0102_0304, 0xBEEF, 0xA0B0_C0D0);
        let n = usize::try_from(u32::from_be_bytes(framed[..4].try_into().unwrap())).unwrap();
        assert_eq!(n, framed.len() - 4, "length prefix covers exactly the body");
        let msg = MhClientMessage::decode(&framed[4..]).expect("decodes");
        match msg.message {
            Some(mh_client_message::Message::RecoveryRequest(r)) => {
                assert_eq!(r.slot_id, 0x0102_0304);
                assert_eq!(r.sender_id, 0xBEEF);
                assert_eq!(r.gop_counter, 0xA0B0_C0D0);
            }
            _ => panic!("not a recovery_request"),
        }
        assert!(msg.trace_parent.is_empty() && msg.trace_state.is_empty());
    }

    #[test]
    fn a_keyframe_request_decodes_to_its_scalars() {
        let body = MhServerMessage {
            message: Some(mh_server_message::Message::KeyframeRequest(
                KeyframeRequest {
                    stream_number: 1,
                    gop_counter: 0x0EAD_BEEF,
                },
            )),
            trace_parent: String::new(),
            trace_state: String::new(),
        }
        .encode_to_vec();
        assert_eq!(
            decode_keyframe_request(&body),
            KeyframeRequestMsg {
                stream_number: 1,
                gop_counter: 0x0EAD_BEEF
            }
        );
    }

    #[test]
    fn carrier_framing_reassembles_two_requests_split_at_every_offset() {
        let req = |n, g| {
            frame(
                &MhServerMessage {
                    message: Some(mh_server_message::Message::KeyframeRequest(
                        KeyframeRequest {
                            stream_number: n,
                            gop_counter: g,
                        },
                    )),
                    trace_parent: String::new(),
                    trace_state: String::new(),
                }
                .encode_to_vec(),
            )
        };
        let mut wire = req(1, 5);
        wire.extend(req(1, 6));
        for cut in 0..=wire.len() {
            let mut buf = Vec::new();
            let mut got = Vec::new();
            for part in [&wire[..cut], &wire[cut..]] {
                buf.extend_from_slice(part);
                while let Some(body) = take_carrier_frame(&mut buf) {
                    got.push(decode_keyframe_request(&body));
                }
            }
            assert_eq!(
                got,
                vec![
                    KeyframeRequestMsg {
                        stream_number: 1,
                        gop_counter: 5
                    },
                    KeyframeRequestMsg {
                        stream_number: 1,
                        gop_counter: 6
                    },
                ],
                "cut {cut}"
            );
            assert!(buf.is_empty());
        }
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn an_oversized_carrier_length_is_refused_before_allocation() {
        let mut buf = u32::try_from(MAX_FRAME_BYTES + 1)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        let _ = take_carrier_frame(&mut buf);
    }

    #[test]
    #[should_panic(expected = "no message variant")]
    fn an_empty_oneof_is_a_loud_defect() {
        let body = MhServerMessage {
            message: None,
            trace_parent: String::new(),
            trace_state: String::new(),
        }
        .encode_to_vec();
        let _ = decode_keyframe_request(&body);
    }

    #[test]
    #[should_panic(expected = "malformed")]
    fn a_malformed_message_is_a_loud_defect() {
        let _ = decode_keyframe_request(&[0xFF, 0xFF, 0xFF]);
    }
}
