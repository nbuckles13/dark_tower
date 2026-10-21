//! The real [`MediaTransport`] implementation, wrapping `wtransport`
//! per-connection I/O (ADR-0036 §10).
//!
//! This is the production half of the seam declared in [`crate::transport`].
//! The seam's own module docs place it here deliberately — not in `media/`
//! (which §11 reserves for the hot path), not in `common` (one consumer, one
//! crate) — and this module imports the traits from there rather than
//! redeclaring anything. **There is no second abstraction.**
//!
//! # Scope: plumbing, not a forward loop
//!
//! Per-connection I/O and nothing else: send and receive datagrams; open a
//! unidirectional stream, write, finish, reset and prioritise it; accept a
//! peer's unidirectional stream, read chunks from it and stop it. No ingress
//! loop, no egress loop, no queue, no routing, no framing. The forward paths
//! (story task 16, story 3 MH-A2) run *against* the trait, with this type in
//! production and `mh_test_utils::transport_shim::LossDelayTransport` in
//! tests.
//!
//! # No emission originates here
//!
//! No `tracing` / `metrics` / `log` macro is reachable from any function in this
//! module, and no constructor or method takes a metric handle. That is the
//! seam's contract, and it is what keeps ADR-0036 §11's "zero registry lookup
//! per frame" a property of the type system rather than of reviewer vigilance.
//! Errors map to seam variants here; the *label* is chosen at the emission site
//! in the forward path, so the drop-reason vocabulary keeps one home.
//!
//! # Ordering obligation for whoever wires this up
//!
//! **Nothing in this module may run before the JWT gate.**
//! [`crate::webtransport::connection::handle_connection`] validates the meeting
//! token and resolves the connection's registration state; per-connection media
//! I/O starts only after that returns. Constructing a [`WtMediaTransport`]
//! earlier would start unauthenticated I/O on an accepted-but-unvalidated
//! session.
//!
//! Related, and easy to get wrong in the same edit: the accept loop in
//! [`crate::webtransport::server`] does capacity-check → `fetch_add` → spawn,
//! and decrements on **every** exit path of the spawned task. A wrapper that
//! took ownership of the connection and returned early on a construction error
//! without that decrement would leak capacity monotonically toward
//! `max_connections` — a self-inflicted denial of service that looks exactly
//! like organic growth. This type's constructor is infallible precisely so that
//! there is no such early-return path to forget.

use bytes::Bytes;
use std::sync::Arc;

use crate::transport::{
    DatagramSendError, MediaRecvStream, MediaSendStream, MediaTransport, StreamReadError,
    TransportCause, TransportError, GROUP_ABANDONED,
};
use wtransport::VarInt;

/// Per-connection media I/O over a live `wtransport` WebTransport session.
///
/// Cheap to clone in the sense that matters: it holds an [`Arc`], so fan-out
/// tasks share one connection without cloning transport state.
#[derive(Clone)]
pub struct WtMediaTransport {
    connection: Arc<wtransport::Connection>,
}

impl WtMediaTransport {
    /// Wrap an established WebTransport connection.
    ///
    /// Infallible by design — see the ordering obligation in the module docs.
    #[must_use]
    pub const fn new(connection: Arc<wtransport::Connection>) -> Self {
        Self { connection }
    }
}

/// The send half of a unidirectional stream over `wtransport`.
pub struct WtSendStream {
    stream: wtransport::SendStream,
    /// Whether [`MediaSendStream::finish`] has returned `Ok` on this stream.
    ///
    /// # Why local state is required rather than merely convenient
    ///
    /// The seam states as a **contract** that a `write_all` after a successful
    /// `finish` must return [`TransportError::StreamClosed`], and it justifies
    /// that by citing quinn, which returns the stream-scoped
    /// `WriteError::ClosedStream` in exactly that case. quinn does. **But
    /// `wtransport` sits in between and erases the distinction**:
    ///
    /// ```text
    /// wtransport-0.7.2/src/driver/streams/mod.rs:564-568
    ///     quinn::WriteError::ConnectionLost(_) | quinn::WriteError::ClosedStream
    ///         => StreamWriteError::NotConnected
    /// ```
    ///
    /// So by the time an error reaches this module, "this stream was already
    /// finished" and "the whole connection is gone" are the **same value** —
    /// and `StreamWriteError::NotConnected`'s own docstring reads "Connection
    /// has been dropped". `wtransport` does define a `Closed` variant meaning
    /// "already finished or reset locally", but its `From<quinn::WriteError>`
    /// never produces it on this path.
    ///
    /// No mapping function can recover information that has already been
    /// discarded. Mapping `NotConnected` to `StreamClosed` instead would invert
    /// the damage: a genuinely lost connection would be reported as a dead
    /// stream, and the forward path would keep a dead connection alive
    /// indefinitely rather than tearing it down — strictly worse, because the
    /// stream case is recoverable and the connection case is not.
    ///
    /// This flag is the disambiguation, and MH is entitled to it: MH is the
    /// side that called `finish`, so it holds the fact `wtransport` dropped.
    /// Cost is one `bool` test per write, and on the post-finish path it also
    /// avoids a doomed call into the transport.
    ///
    /// **This was found by test, not by reading.**
    /// `tests/transport_real_impl.rs` asserts the contract against this
    /// implementation and the deterministic double through one shared
    /// assertion body; the real side failed with `ConnectionClosed` on the
    /// first run. Before that test existed, nothing in the tree opened a
    /// unidirectional stream over the real transport, so this divergence would
    /// have shipped underneath a forward path that trusts the double.
    finished: bool,
    /// Whether [`MediaSendStream::reset`] has run. With `finished`, decides
    /// what `Drop` does: a stream that is neither is RESET, never left to
    /// quinn's finishing `Drop` (see the seam's "Abandonment is a RESET"
    /// contract).
    reset: bool,
}

impl WtSendStream {
    /// Wrap the send half of the client-opened MH control stream (story 3
    /// MH-B: `MhServerMessage` is written here), with the same write and
    /// drop contract as a media stream.
    #[must_use]
    pub const fn from_stream(stream: wtransport::SendStream) -> Self {
        Self {
            stream,
            finished: false,
            reset: false,
        }
    }
}

/// RESET, never FIN, on drop: the seam's abandonment contract.
///
/// quinn's `Drop for SendStream` finishes a stream that was neither finished
/// nor reset, and wtransport adds no override, so without this impl every
/// early return, cancelled writer task or aborted `JoinSet` would deliver a
/// clean FIN on a possibly half-written frame. `tests/transport_real_impl.rs`
/// proves on a real QUIC pair that the peer observes `RESET_STREAM`.
impl Drop for WtSendStream {
    fn drop(&mut self) {
        if !self.finished && !self.reset {
            // Already-closed is the only error, and it means there is nothing
            // left to abandon.
            let _ = self.stream.reset(VarInt::from_u32(GROUP_ABANDONED));
        }
    }
}

/// The receive half of a peer-opened unidirectional stream over `wtransport`.
pub struct WtRecvStream {
    stream: wtransport::RecvStream,
}

impl WtRecvStream {
    /// Wrap the receive half of the client-opened MH control stream, so the
    /// connect read and the post-connect loop share the seam's framer
    /// (`crate::transport::framing`).
    #[must_use]
    pub const fn from_stream(stream: wtransport::RecvStream) -> Self {
        Self { stream }
    }
}

/// Map quinn's read failure onto the seam's vocabulary, keeping the cause.
fn map_read_error(error: quinn::ReadError) -> StreamReadError {
    match error {
        quinn::ReadError::Reset(code) => StreamReadError::Reset(code.into_inner()),
        quinn::ReadError::ConnectionLost(lost) => {
            StreamReadError::ConnectionClosed(TransportCause::new(lost))
        }
        other => StreamReadError::Failed(TransportCause::new(other)),
    }
}

/// Map a `wtransport` datagram-send failure onto the seam's vocabulary.
///
/// # `DatagramSendError::WouldBlock` is deliberately unreachable here
///
/// It has no arm below and that is correct, not an omission.
/// [`DatagramSendError::WouldBlock`]'s own documentation carries the
/// five-clause argument for why no production implementation can return it —
/// quinn's `send_datagram` hardcodes drop-oldest eviction and treats the
/// blocked arm as `unreachable!()`. That argument is **pointed at, never
/// restated**: two copies of a five-clause proof drift, and the seam's copy is
/// the single source of truth.
///
/// The practical consequence for the forward path: MH's observable
/// back-pressure signal is its own bounded egress queue overflowing, never a
/// transport refusal. Genuine loss happens *beneath* this function, inside
/// quinn's silent eviction, which MH structurally cannot see — which is exactly
/// why `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` is validated at startup to sit above
/// the application queue bound.
fn map_send_datagram_error(error: &wtransport::error::SendDatagramError) -> DatagramSendError {
    match error {
        wtransport::error::SendDatagramError::NotConnected => DatagramSendError::ConnectionClosed,
        wtransport::error::SendDatagramError::UnsupportedByPeer => {
            DatagramSendError::DatagramsUnsupported
        }
        wtransport::error::SendDatagramError::TooLarge => DatagramSendError::TooLarge,
    }
}

/// Map a `wtransport` stream-write failure onto the seam's vocabulary.
///
/// `Closed` and `Stopped` are both stream-scoped: the connection may still be
/// usable and other streams on it may still make progress, so collapsing them
/// into `ConnectionClosed` would make the forward path tear down a healthy
/// connection because one subscriber's stream went away. `NotConnected` and a
/// QUIC protocol error are connection-scoped and terminal.
///
/// # `NotConnected` is AMBIGUOUS at this boundary, and that is not fixable here
///
/// `wtransport` folds quinn's stream-scoped `WriteError::ClosedStream` into
/// `StreamWriteError::NotConnected` alongside a genuinely lost connection
/// (`driver/streams/mod.rs:564-568`). This function maps it to
/// `ConnectionClosed` — the safe reading, since treating a dead connection as a
/// merely-dead stream would leave the forward path holding a connection that
/// will never recover.
///
/// The one case where that reading is wrong — a write after a successful
/// `finish` — is disambiguated **before** reaching here, by
/// [`WtSendStream::finished`], because MH holds the fact `wtransport` discarded.
fn map_stream_write_error(error: wtransport::error::StreamWriteError) -> TransportError {
    match error {
        wtransport::error::StreamWriteError::Closed
        | wtransport::error::StreamWriteError::Stopped(_) => {
            TransportError::StreamClosed(TransportCause::new(error))
        }
        wtransport::error::StreamWriteError::NotConnected
        | wtransport::error::StreamWriteError::QuicProto => {
            TransportError::ConnectionClosed(TransportCause::new(error))
        }
    }
}

impl MediaTransport for WtMediaTransport {
    type SendStream = WtSendStream;
    type RecvStream = WtRecvStream;

    fn send_datagram(&self, payload: Bytes) -> Result<(), DatagramSendError> {
        // `wtransport::Connection::send_datagram` is a plain `fn`, not `async` —
        // which is why the seam declares this synchronous. It takes `AsRef<[u8]>`
        // and copies (it must prepend the HTTP/3 session-id varint), so the
        // `Bytes` refcount is discarded by the vendor signature. That cost is
        // documented at the seam; it is not something this wrapper introduces
        // and it is not something it can avoid.
        self.connection
            .send_datagram(&payload)
            .map_err(|e| map_send_datagram_error(&e))
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        // Every `ConnectionError` variant is terminal for this connection —
        // peer-aborted, peer-closed, locally closed, H3 violation, timed out.
        // The forward path's loop-termination signal is exactly this.
        let datagram = self
            .connection
            .receive_datagram()
            .await
            .map_err(|e| TransportError::ConnectionClosed(TransportCause::new(e)))?;

        // Zero-copy: `Datagram::payload()` returns a `Bytes` slice of a buffer
        // the transport already owns. Callers check `.len()` against
        // `media_protocol`'s `MAX_PAYLOAD_BYTES` before any per-frame work,
        // with nothing allocated on their behalf first.
        Ok(datagram.payload())
    }

    async fn open_uni(&self) -> Result<Self::SendStream, TransportError> {
        // Two failure points, both terminal for the stream and reported as a
        // dead connection: the request to open, and the peer's acceptance.
        // `StreamOpeningError::Refused` is the peer stopping the stream during
        // initialisation, which for a media egress stream we cannot proceed past.
        let opening = self
            .connection
            .open_uni()
            .await
            .map_err(|e| TransportError::ConnectionClosed(TransportCause::new(e)))?;

        let stream = opening
            .await
            .map_err(|e| TransportError::ConnectionClosed(TransportCause::new(e)))?;

        Ok(WtSendStream {
            stream,
            finished: false,
            reset: false,
        })
    }

    async fn accept_uni(&self) -> Result<Self::RecvStream, TransportError> {
        // Every `ConnectionError` is terminal for this connection; it is the
        // stream accept loop's termination signal.
        let stream = self
            .connection
            .accept_uni()
            .await
            .map_err(|e| TransportError::ConnectionClosed(TransportCause::new(e)))?;
        Ok(WtRecvStream { stream })
    }
}

impl MediaRecvStream for WtRecvStream {
    async fn read_chunk(&mut self, max_len: usize) -> Result<Option<Bytes>, StreamReadError> {
        // quinn's `read_chunk` hands out a `Bytes` slice of its own receive
        // buffer: zero copy. That slice PINS its parent allocation for as long
        // as it is held; `crate::media::downlink` copies any chunk it must
        // retain in a backlog for exactly that reason.
        self.stream
            .quic_stream_mut()
            .read_chunk(max_len, true)
            .await
            .map(|chunk| chunk.map(|chunk| chunk.bytes))
            .map_err(map_read_error)
    }

    fn stop(&mut self, code: u32) {
        // Already-stopped is the only error and means there is nothing to do.
        let _ = self
            .stream
            .quic_stream_mut()
            .stop(quinn::VarInt::from_u32(code));
    }
}

impl MediaSendStream for WtSendStream {
    async fn write_all(&mut self, buf: &[u8]) -> Result<(), TransportError> {
        // Checked BEFORE touching the transport: past this point `wtransport`
        // can no longer tell us whether the stream or the connection died. See
        // the field docs on `WtSendStream::finished` for why the information is
        // unrecoverable below this line.
        if self.finished || self.reset {
            return Err(TransportError::StreamClosed(TransportCause::local(
                "write after finish or reset",
            )));
        }

        self.stream
            .write_all(buf)
            .await
            .map_err(map_stream_write_error)
    }

    fn finish(&mut self) -> impl std::future::Future<Output = Result<(), TransportError>> + Send {
        // quinn's SYNCHRONOUS finish, not wtransport's `finish().await`, which
        // additionally waits for the peer to acknowledge every byte. The seam
        // defines `finish` as "the group ended at a frame boundary": the data
        // is committed the moment quinn accepts the FIN, and a writer that was
        // cancelled while awaiting an acknowledgement would otherwise reach
        // `Drop` with `finished == false` and RESET a stream that had in fact
        // been finished — discarding the tail of a complete group. Hence a
        // ready future rather than an `async fn` with nothing to await.
        //
        // The seam's contract that a `write_all` after a successful `finish`
        // fails is honoured by the `finished` latch above (asserted against
        // this implementation in `tests/transport_real_impl.rs`).
        let result = if self.finished || self.reset {
            Err(TransportError::StreamClosed(TransportCause::local(
                "finish after finish or reset",
            )))
        } else {
            self.stream
                .quic_stream_mut()
                .finish()
                .map_err(|e| TransportError::StreamClosed(TransportCause::new(e)))
        };
        if result.is_ok() {
            self.finished = true;
        }
        std::future::ready(result)
    }

    fn reset(&mut self, code: u32) -> Result<(), TransportError> {
        // Latched whatever the outcome: an already-closed stream has nothing
        // left to abandon, and `Drop` must not try again.
        self.reset = true;
        self.stream
            .reset(VarInt::from_u32(code))
            .map_err(|e| TransportError::StreamClosed(TransportCause::new(e)))
    }

    fn set_priority(&mut self, priority: i32) {
        self.stream.set_priority(priority);
    }
}

#[cfg(test)]
mod tests {
    //! Error-mapping coverage.
    //!
    //! These are the arms that a live-connection test cannot reach on demand —
    //! a peer that does not support datagrams, an oversize datagram, a stopped
    //! stream. The behavioural contracts that *can* be driven deterministically
    //! (finish-then-write, and receive-after-close) are pinned against a real
    //! connection in `tests/transport_real_impl.rs`, and the finish-then-write
    //! assertion is shared with the test double so the two implementations are
    //! checked against one body rather than two.

    use super::{map_send_datagram_error, map_stream_write_error};
    use crate::transport::{DatagramSendError, TransportCause, TransportError};
    use wtransport::error::{SendDatagramError, StreamWriteError};
    use wtransport::VarInt;

    #[test]
    fn datagram_send_errors_map_to_their_seam_variants() {
        assert_eq!(
            map_send_datagram_error(&SendDatagramError::NotConnected),
            DatagramSendError::ConnectionClosed
        );
        assert_eq!(
            map_send_datagram_error(&SendDatagramError::UnsupportedByPeer),
            DatagramSendError::DatagramsUnsupported
        );
        assert_eq!(
            map_send_datagram_error(&SendDatagramError::TooLarge),
            DatagramSendError::TooLarge
        );
    }

    #[test]
    fn too_large_does_not_hand_the_payload_back() {
        // Not retryable at that size, so returning the bytes would invite a
        // retry loop. Asserted rather than assumed because the seam's
        // `into_payload` contract depends on it.
        assert!(map_send_datagram_error(&SendDatagramError::TooLarge)
            .into_payload()
            .is_none());
    }

    #[test]
    fn stream_scoped_write_errors_do_not_report_a_dead_connection() {
        // The distinction is load-bearing: collapsing these into
        // `ConnectionClosed` would make the forward path tear down a healthy
        // connection because one subscriber's stream went away.
        assert_eq!(
            map_stream_write_error(StreamWriteError::Closed),
            TransportError::StreamClosed(TransportCause::local("x"))
        );
        assert_eq!(
            map_stream_write_error(StreamWriteError::Stopped(VarInt::from_u32(7))),
            TransportError::StreamClosed(TransportCause::local("x"))
        );
    }

    #[test]
    fn connection_scoped_write_errors_report_a_dead_connection() {
        assert_eq!(
            map_stream_write_error(StreamWriteError::NotConnected),
            TransportError::ConnectionClosed(TransportCause::local("x"))
        );
        assert_eq!(
            map_stream_write_error(StreamWriteError::QuicProto),
            TransportError::ConnectionClosed(TransportCause::local("x"))
        );
    }

    #[test]
    fn a_write_error_keeps_its_cause_as_the_source() {
        use std::error::Error as _;
        let error = map_stream_write_error(StreamWriteError::Stopped(VarInt::from_u32(7)));
        assert!(
            error.source().is_some(),
            "the wtransport error must survive"
        );
    }
}
