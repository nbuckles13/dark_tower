//! Per-connection transport seam for the MH media path (ADR-0036 §10).
//!
//! This module declares the boundary that hot-path per-connection I/O crosses:
//! send a datagram, receive a datagram, open a unidirectional stream, write to
//! it, finish it. Nothing else. The QUIC endpoint and the accept loop stay
//! concrete types in [`crate::webtransport`] — only per-connection I/O is
//! abstracted, which is what ADR-0036 §10 specifies.
//!
//! # Why the seam exists
//!
//! §10 justifies it four ways: benchmark representativeness, deterministic
//! reachability of drop paths for metric coverage, the back-pressure gate, and
//! control-plane reconnect testing. The Consequences section makes it an
//! ordering constraint — "the transport seam precedes the ingress and egress
//! loops — cheap against nothing, expensive against concrete types".
//!
//! The second justification is the load-bearing one for telemetry. Every drop
//! counter in ADR-0036 claims to observe a failure that only real network
//! conditions produce. This seam is where those conditions can be manufactured,
//! so "we believe the counter can fire" becomes "here is the line that fires
//! it".
//!
//! # Placement (ADR-0036 §11 layout constraint)
//!
//! A sibling module, deliberately:
//!
//! - **Not** in `media/`. §11 requires that directory hold only the hot path,
//!   so the directory boundary and the hot-path boundary are the same boundary
//!   and the directory-scoped macro-deny guard scopes exactly. A narrowed deny
//!   is the failure mode §11 names by hand.
//! - **Not** in `webtransport/`. That module is the accept loop and connection
//!   lifecycle. The real implementation of these traits lives there and imports
//!   them from here; the declaration does not belong beside its consumer.
//! - **Not** in `common`. One consumer, one crate. Hoisting it would
//!   materialise a Guarded Shared Area glob that currently matches nothing and
//!   convert a single-consumer trait into an owner-co-sign surface for no gain.
//!
//! # Scope: per-connection I/O, no lifecycle
//!
//! Datagrams (send, receive) and unidirectional streams in both directions:
//! open a send stream, write, finish, reset, prioritise; accept a receive
//! stream, read chunks, stop. Nothing else. There is deliberately no `ready`,
//! no `closed` and no `close(info)`. Those are connection lifecycle, which §11
//! keeps as a *sibling* of the hot path rather than inside it; they stay in
//! [`crate::webtransport::connection`]. Loop termination comes from
//! [`MediaTransport::recv_datagram`] / [`MediaTransport::accept_uni`] returning
//! [`TransportError::ConnectionClosed`], plus the existing `CancellationToken`.
//!
//! The shape mirrors the client SDK's `IWebTransport`
//! (`packages/sdk-core/src/transport/IWebTransport.ts`) **conceptually, not
//! member for member**. That interface is `ready` / `closed` / `datagrams
//! {readable, writable}` / `createBidirectionalStream()` / `close()` — Web
//! Streams and *bidirectional* streams. This one is datagrams plus *uni*
//! streams. Same idea — per-connection I/O behind an interface with a
//! deterministic double — in two idioms. Stated explicitly so nobody builds a
//! cross-language parity test against a parity that was never claimed.
//!
//! # Abandonment is a RESET, and dropping a send stream is an abandonment
//!
//! quinn's own `Drop for SendStream` FINISHES a stream that was neither
//! finished nor reset (`quinn-0.11.12/src/send_stream.rs`), and wtransport adds
//! no override. A stream dropped mid-frame — an early return, a cancelled task,
//! an aborted `JoinSet`, a panic unwind — would therefore deliver a clean FIN
//! on a half-written frame, which is exactly what story 3 R-12 forbids ("a
//! half-written frame is never left looking complete"). So this seam's
//! contract is the opposite of quinn's default, and it is a contract, not a
//! convention: **a [`MediaSendStream`] dropped without [`MediaSendStream::finish`]
//! or [`MediaSendStream::reset`] having run RESETS with [`GROUP_ABANDONED`].**
//! The production implementation enforces it in its own `Drop`; the test double
//! models the same contract, and `tests/transport_real_impl.rs` proves the
//! production `Drop` on a real QUIC pair so the double never certifies a
//! property production lacks.
//!
//! [`MediaSendStream::finish`] means "the group ended at a frame boundary" and
//! completes immediately; it never waits for the peer's acknowledgement, so a
//! writer that is cancelled after finishing never turns a finished stream into
//! a reset.
//!
//! # No emission originates here
//!
//! No `tracing` / `metrics` / `log` / `println!` / span / `#[instrument]` macro
//! is reachable from these traits or any implementation of them, and **no
//! method takes a metric handle as a parameter**. Task 16's contract is zero
//! metric-registry lookup per frame, with handles resolved once at setup in
//! [`crate::observability::metrics`] and injected into the per-stream
//! forwarder. If this seam accepted handles, that decision would have been
//! relocated into the transport module and `media/`'s directory-scoped deny
//! would stop meaning what it says.
//!
//! # Payload types, and why they differ by path
//!
//! Datagrams use [`Bytes`] by value in both directions; stream writes take
//! `&[u8]`. That asymmetry is in the upstream API and in the ownership
//! requirements, not a style preference:
//!
//! - `send_datagram` needs ownership. [`DatagramSendError::WouldBlock`] hands
//!   the payload back so the caller's bounded queue can requeue it without
//!   copying, and fan-out shares one ingress buffer across N egress payloads
//!   before any transport call happens.
//! - `recv_datagram` returns [`Bytes`] because the real implementation is
//!   `connection.receive_datagram().await?.payload()`, and wtransport's
//!   `Datagram::payload()` is already a zero-copy slice of the ingress buffer.
//!   A `&[u8]` return could not cross a `tokio::spawn` boundary; a `Vec<u8>`
//!   return would force a copy.
//! - Stream writes have no hand-back and no fan-out gate, and the underlying
//!   `wtransport::SendStream::write_all` takes `&[u8]`. Requiring [`Bytes`]
//!   there would make callers materialise a `Bytes` for every length prefix,
//!   which is precisely the pressure that would push framing *down* into this
//!   module.
//!
//! **This seam does no framing.** The 4-byte length prefix already has two
//! homes in the workspace; a third here would be real duplication. Callers
//! frame above the seam and hand bytes down.
//!
//! # The payload is opaque, and stays opaque
//!
//! MH is keyless (§4). No associated type, method, or parameter here names key
//! material, a KEK, a generation, or a parsed publisher region, and no
//! `media-protocol` type appears in any signature. The publisher region,
//! payload and signature travel as one uninspected blob.
//!
//! # Zero per-frame allocation is a property held ABOVE this seam only
//!
//! This is a signature property, not an implementation detail, and it is stated
//! that way on purpose — "the vendor allocates today" invites someone to
//! re-check later and conclude it was fixed; the following cannot be fixed
//! without an upstream API change.
//!
//! wtransport's production entry point is
//! `wtransport-0.7.2/src/driver/mod.rs:206-211`,
//! `pub fn send_datagram(&self, session_id: SessionId, payload: &[u8])`. It
//! takes `&[u8]`. **The refcount is discarded by the signature.** Its body
//! confirms the cost: `Datagram::write` allocates `vec![0; write_size]` and
//! copies the payload in, because the HTTP/3 session-id varint has to be
//! prepended (`src/datagram.rs:34-48`).
//!
//! Two consequences that must not be lost:
//!
//! 1. **No refcount assertion may be written *through* a `send_datagram`
//!    call.** A test double does not copy; production does. A gate that hands
//!    a [`Bytes`] through the seam and asserts sharing afterwards would pass
//!    against the double while certifying a property production does not have.
//!    Assert fan-out sharing strictly *upstream* of the trait call. This
//!    extends §10's existing scoping ("scoped to the fan-out, not across the
//!    stream-read boundary where a copy is inherent") to the send boundary.
//! 2. **§10 Tier-2 benchmark deltas measured through a test double are a
//!    narrower claim than they appear.** They measure the forward path above
//!    this seam and deliberately exclude a known per-datagram allocation and
//!    copy below it. Since §10 justifies the seam partly on benchmark
//!    representativeness, the exclusion belongs recorded at the seam rather
//!    than rediscovered during an egress-exhaustion incident.
//!
//! # Drop-path reachability, and why loss direction is not a detail
//!
//! §10's second justification for this seam is that drop paths must be
//! *deterministically reachable*, so a later metric-coverage test can trip them
//! on demand. The deterministic double injects datagram loss in **both**
//! directions, and the two directions are not interchangeable — they are the
//! firing paths for different counters.
//!
//! ADR-0036 §2 is the governing text: "The **hop sequence** is set by whoever
//! transmits — **it applies to the client's uplink as well as MH's downlink**,
//! so each side can detect loss on the hop it receives."
//!
//! - **Egress loss** (accepted by `send_datagram`, never delivered) fires no MH
//!   counter at all. MH writes its own downlink hop sequence, so MH cannot
//!   observe a gap in a number it generates itself. It is asserted on the
//!   double's delivered sink, and it proves the *client-side* detection
//!   precondition — which in a loopback story is the receiver that matters.
//! - **Ingress loss** (accepted by the injection API, never queued) is loss
//!   upstream of MH. MH's own gap detector reads the **client-set uplink** hop
//!   sequence, so this is the *only* firing path for that counter.
//! - **Ingress burst** is a third thing, not a substitute for the second.
//!   Overrunning MH's bounded inbound queue is MH's own refusal — drop-oldest,
//!   observable by construction — rather than an inferred gap. Exercising
//!   burst alone would leave the gap counter with zero firing paths.
//!
//! This is recorded rather than merely applied, because the design review got
//! it wrong first. The author of this seam initially proposed detecting ingress
//! gaps on `stream_sequence`. That is the natural error if you have filed the
//! hop sequence as MH-generated-only, which is the natural reading arriving
//! from the egress side — and it is wrong twice over. `stream_sequence` is the
//! AEAD nonce input, and selection is intentional gapping, so §2's warning
//! applies verbatim: "Get that backwards and the metric measures selection
//! policy instead of loss." A doc that states only the right answer leaves the
//! next reader to re-derive why the obvious field is the wrong one, and the
//! re-derivation fails the same way.
//!
//! # Receive streams expose chunks, never frames
//!
//! [`MediaRecvStream::read_chunk`] returns whatever the transport delivered, as
//! [`Bytes`] — in production a zero-copy slice of quinn's receive buffer. It
//! never returns a frame: a `read_frame() -> Vec<u8>` shape would allocate from
//! an attacker-controlled length before any cap could apply, silently
//! foreclosing §11's ingress denial-of-service cap ("maximum frame payload
//! before any allocation"). Framing happens above the seam, in
//! `crate::media::stream_ingress`, through `media_protocol::reader::HeaderReader`,
//! which rejects a declared length over `MAX_PAYLOAD_BYTES` from the bounded
//! fixed prefix before anything is sized from it.
//!
//! A chunk is a slice of a larger receive allocation, so RETAINING it pins that
//! parent. The forward path therefore copies any chunk it must hold in a
//! destination's backlog (`crate::media::downlink`); see there for the bound.
//!
//! On the datagram path the ordering is already safe: `recv_datagram` yields a
//! [`Bytes`] that is a slice of a buffer the transport already owns, so the
//! caller reads `.len()` and rejects oversize frames with no copy having
//! occurred and nothing having been allocated on our behalf.
//!
//! # Stream reset and stop codes
//!
//! Two application codes, one home (this module), wire-visible but carrying no
//! reason: [`GROUP_ABANDONED`] on every downlink `RESET_STREAM` MH sends,
//! whatever the cause, and [`INGRESS_REFUSED`] on every `STOP_SENDING` MH
//! sends to a publisher stream. The receiver semantics are anchored once in
//! ADR-0036 §2, *Stream reset and stop codes carry no reason*. A peer MUST NOT
//! branch on the value: wtransport 0.7.2 hands the varint straight to quinn
//! without the WebTransport-over-HTTP/3 application-code mapping (the
//! `0x52e4a40fa8db` range), so a browser may surface these resets with a null
//! or unmapped `streamErrorCode`. Neither code is 0. The reason a stream was
//! abandoned is MH-local telemetry (`mh_media_group_resets_total{reason}`) and
//! never crosses the wire, so a
//! subscriber gets no oracle into MH policy.

use bytes::Bytes;
use std::future::Future;
use thiserror::Error;

pub mod framing;

/// The `RESET_STREAM` application code on every downlink group MH abandons.
/// See the module docs: one reasonless code; peers must not branch on it.
pub const GROUP_ABANDONED: u32 = 0x4741;

/// The `STOP_SENDING` application code on every publisher stream MH refuses
/// (rate limit, attribution failure, codec reject, stall). See the module docs.
pub const INGRESS_REFUSED: u32 = 0x4952;

const _: () = assert!(
    GROUP_ABANDONED != 0 && INGRESS_REFUSED != 0 && GROUP_ABANDONED != INGRESS_REFUSED,
    "the two MH stream codes are non-zero and distinct"
);

/// The underlying cause of a transport failure, kept for diagnosis.
///
/// Cheap to clone (one `Arc`), and constructed only on an error path — never
/// per frame — so carrying it costs the hot path nothing. Exposed as the
/// `#[source]` of [`TransportError`] and [`StreamReadError`] so a sibling that
/// logs the error at teardown keeps the quinn/wtransport reason (application
/// close, idle timeout, transport error) instead of a bare "closed".
#[derive(Clone)]
pub struct TransportCause(std::sync::Arc<dyn std::error::Error + Send + Sync + 'static>);

impl TransportCause {
    /// Wrap the transport's own error.
    #[must_use]
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(error))
    }

    /// A cause with no underlying error object — for test doubles and for
    /// conditions the seam detects itself (e.g. a write after `finish`).
    #[must_use]
    pub fn local(reason: &'static str) -> Self {
        Self(std::sync::Arc::new(LocalCause(reason)))
    }
}

impl std::fmt::Debug for TransportCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&*self.0, f)
    }
}

impl std::fmt::Display for TransportCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&*self.0, f)
    }
}

impl std::error::Error for TransportCause {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}

#[derive(Debug)]
struct LocalCause(&'static str);

impl std::fmt::Display for LocalCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LocalCause {}

/// Failure modes of the asynchronous per-connection operations.
///
/// Each variant carries its [`TransportCause`]; callers match on the variant
/// for control flow and never need the cause to decide anything. Equality is
/// **variant-only**: two `ConnectionClosed` errors are equal whatever their
/// causes, which is what a control-flow assertion means.
///
/// These variants carry no metric label vocabulary. `docs/observability/metrics/mh-service.md`
/// is the single source of truth for drop reasons and directions; the forward
/// path maps an error to a label at the emission site, so that vocabulary has
/// exactly one home.
#[derive(Debug, Clone, Error)]
pub enum TransportError {
    /// The connection is gone. Tear the per-connection tasks down.
    #[error("connection closed")]
    ConnectionClosed(#[source] TransportCause),

    /// The stream is gone (finished, reset, or stopped by the peer), but the
    /// connection may still be usable.
    #[error("stream closed")]
    StreamClosed(#[source] TransportCause),
}

impl PartialEq for TransportError {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::ConnectionClosed(_), Self::ConnectionClosed(_))
                | (Self::StreamClosed(_), Self::StreamClosed(_))
        )
    }
}

impl Eq for TransportError {}

/// Failure modes of [`MediaRecvStream::read_chunk`].
///
/// A clean end of stream is `Ok(None)`, never an error. Equality is
/// variant-only (and code-sensitive for `Reset`).
#[derive(Debug, Clone, Error)]
pub enum StreamReadError {
    /// The peer reset the stream (`RESET_STREAM`) with this application code.
    /// The code is the peer's and carries no meaning MH acts on.
    #[error("stream reset by peer")]
    Reset(u64),

    /// The connection is gone.
    #[error("connection closed")]
    ConnectionClosed(#[source] TransportCause),

    /// The stream is unusable for another reason (already stopped locally,
    /// an illegal read). Should not occur on MH's read path.
    #[error("stream read failed")]
    Failed(#[source] TransportCause),
}

impl PartialEq for StreamReadError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Reset(a), Self::Reset(b)) => a == b,
            (Self::ConnectionClosed(_), Self::ConnectionClosed(_))
            | (Self::Failed(_), Self::Failed(_)) => true,
            _ => false,
        }
    }
}

impl Eq for StreamReadError {}

/// Failure modes of [`MediaTransport::send_datagram`].
///
/// Flat rather than wrapping [`TransportError`]: a nested `Fatal(StreamClosed)`
/// would be representable in the type and impossible in reality, and the
/// forward path would carry a match arm that can never be exercised — the same
/// dead-arm problem the seam's reachability tests exist to prevent. These four
/// variants map one-to-one onto what can actually happen.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DatagramSendError {
    /// The transport refused the datagram for lack of send capacity and handed
    /// the payload back, uncopied, so the caller can requeue or drop it.
    ///
    /// # No production implementation returns this
    ///
    /// Stated as five independently checkable clauses rather than one summary
    /// judgement. A reader who checks a single clause and finds it false would
    /// discard a summary wholesale — and the compressed form of this fact
    /// ("no blocked variant exists below us") was written and corrected three
    /// times during this seam's design. The tempting compression of "X is
    /// unreachable through the API we can use" is "X does not exist", and that
    /// compression forecloses a live question.
    ///
    /// 1. **The shape exists.** `quinn_proto::SendDatagramError::Blocked(Bytes)`
    ///    — `quinn-proto-0.11.17/src/connection/datagrams.rs:243`. It hands the
    ///    payload back, which is exactly this variant's shape.
    /// 2. **It is unreachable as an error return through the API wtransport
    ///    uses.** `quinn::Connection::send_datagram` hardcodes `drop: true`
    ///    (`quinn-0.11.11/src/connection.rs:442`) and treats the blocked arm as
    ///    `unreachable!()` (`:448`). quinn instead **silently evicts the oldest
    ///    queued datagram** — which is the §1/§11 gap MH's own bounded egress
    ///    queue exists to close.
    /// 3. **quinn's congestion-aware path reaches it but never returns it.**
    ///    `send_datagram_wait` (`:464`) calls `send(.., false)` (`:849`), so it
    ///    genuinely hits the blocked path, then retains the payload and yields
    ///    `Poll::Pending` (`:857-860`). Back-pressure is expressed by awaiting,
    ///    never by returning a value.
    /// 4. **wtransport does not use that path.** Its escape hatch to the raw
    ///    QUIC connection, `quic_connection()`
    ///    (`wtransport-0.7.2/src/connection.rs:415`), is `#[cfg(feature = "quinn")]`.
    ///
    ///    > **Corrected 2026-09-02** (story task 12, MH transport config). This
    ///    > clause previously read "a feature absent from wtransport's defaults
    ///    > and from this workspace's dependency, so it does not compile here
    ///    > today". **That is no longer true**: `crates/mh-service/Cargo.toml`
    ///    > now enables `wtransport/quinn`, because
    ///    > `ServerConfigBuilder::with_custom_transport` — the only way to hand
    ///    > `wtransport` an explicit `quinn::TransportConfig`, which ADR-0036 §1
    ///    > requires — is gated behind the same feature. So `quic_connection()`
    ///    > **does** compile here now.
    ///    >
    ///    > The conclusion is unchanged; only this clause's reason is. Clause 5
    ///    > is now the load-bearing one, and it was always the stronger of the
    ///    > two. Corrected rather than deleted because a reader who checked this
    ///    > clause, found it false, and discarded the whole five-clause argument
    ///    > would conclude `WouldBlock` is reachable in production — which it is
    ///    > not.
    ///    >
    ///    > The invariant is in fact safer than this correction alone suggests:
    ///    > **clauses 2 and 3 hold independently of the feature gate.** quinn's
    ///    > public API `unreachable!()`s the blocked arm and expresses
    ///    > back-pressure by *awaiting*, never by returning `Blocked` as a
    ///    > value — so removing the compile barrier opens no value-return path
    ///    > at all. Clause 5 closes only the attributability angle.
    /// 5. **Enabling that feature would not help.** Datagrams sent through the
    ///    raw QUIC connection skip the HTTP/3 session-id varint that
    ///    `Datagram::write` prepends (`src/datagram.rs:34-48`), so they are
    ///    unattributable to a WebTransport session and discarded by a
    ///    conforming peer. Reimplementing that framing here would duplicate
    ///    protocol-owned wire logic.
    ///
    /// So this variant's sole producer is the test double, **by design**: it is
    /// the deterministic trip-wire that lets a test stall the egress drain and
    /// fire the bounded queue's overflow counter on demand. It is a deliberate
    /// test affordance, not dead code awaiting deletion.
    ///
    /// The counter that fires in production is MH's own queue-bound overflow,
    /// never a transport refusal. MH's egress back-pressure is a property of
    /// MH's own queue depth. Nothing may describe that counter as observing
    /// transport back-pressure: genuine loss occurs beneath it, in quinn's
    /// silent eviction, which it structurally cannot see.
    #[error("datagram send would block")]
    WouldBlock(Bytes),

    /// The datagram exceeds what the connection can currently carry.
    ///
    /// The payload is deliberately **not** handed back: it is not retryable at
    /// that size, and returning the bytes would invite a retry loop.
    #[error("datagram too large")]
    TooLarge,

    /// The connection is gone.
    #[error("connection closed")]
    ConnectionClosed,

    /// The peer does not support datagrams. Fatal for the media path.
    #[error("datagrams unsupported by peer")]
    DatagramsUnsupported,
}

impl DatagramSendError {
    /// Recovers the payload from [`DatagramSendError::WouldBlock`], returning
    /// the **same allocation** that was passed to
    /// [`MediaTransport::send_datagram`] — no copy, so a caller can requeue it
    /// or make its own drop decision without paying for the refusal.
    ///
    /// Returns `None` for every other variant, none of which carries a payload.
    #[must_use]
    pub fn into_payload(self) -> Option<Bytes> {
        match self {
            Self::WouldBlock(payload) => Some(payload),
            Self::TooLarge | Self::ConnectionClosed | Self::DatagramsUnsupported => None,
        }
    }
}

/// The send half of a unidirectional stream.
///
/// Writes borrow (`&[u8]`) rather than take ownership: there is no hand-back
/// and no fan-out refcount gate on the stream path, and requiring [`Bytes`]
/// would force callers to allocate for every length prefix.
pub trait MediaSendStream: Send {
    /// Writes the whole buffer to the stream.
    ///
    /// # Errors
    ///
    /// [`TransportError::StreamClosed`] if the stream is gone,
    /// [`TransportError::ConnectionClosed`] if the connection is.
    fn write_all(&mut self, buf: &[u8]) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Finishes the stream, signalling a clean end of data to the peer.
    ///
    /// # Contract: writes after `finish` fail
    ///
    /// Once `finish` has returned `Ok`, a subsequent [`MediaSendStream::write_all`]
    /// on the same stream MUST return [`TransportError::StreamClosed`]. This is
    /// stated as a contract rather than left to each implementation because a
    /// test double and the real transport diverging on it would make the double
    /// unfaithful exactly where a forward-path test would trust it.
    ///
    /// It costs nothing to honour: quinn already behaves this way, returning
    /// `WriteError::ClosedStream` from the write path once the stream is closed
    /// (`quinn-0.11.11/src/send_stream.rs:160-162`).
    ///
    /// # Errors
    ///
    /// [`TransportError::StreamClosed`] if the stream is gone,
    /// [`TransportError::ConnectionClosed`] if the connection is.
    fn finish(&mut self) -> impl Future<Output = Result<(), TransportError>> + Send;

    /// Resets the stream immediately with application code `code`, discarding
    /// anything buffered. The abandonment primitive (story 3 R-12): the peer
    /// sees `RESET_STREAM`, never a FIN, so a half-written frame cannot look
    /// complete. Synchronous, as in quinn.
    ///
    /// # Errors
    ///
    /// [`TransportError::StreamClosed`] if the stream was already finished or
    /// reset. A caller abandoning a stream treats that as done.
    fn reset(&mut self, code: u32) -> Result<(), TransportError>;

    /// Sets the stream's send priority. quinn's convention: a HIGHER value is
    /// transmitted first. Callers map MC's `priority_group` (lower is higher)
    /// through `crate::media::downlink::quic_priority`.
    fn set_priority(&mut self, priority: i32);
}

/// The receive half of a unidirectional stream a peer opened.
pub trait MediaRecvStream: Send {
    /// Reads the next chunk the transport delivered, at most `max_len` bytes.
    /// `Ok(None)` is the peer's FIN.
    ///
    /// Returns a chunk, never a frame (see the module docs): nothing on this
    /// path is sized from wire content.
    ///
    /// # Errors
    ///
    /// [`StreamReadError::Reset`] when the peer reset the stream,
    /// [`StreamReadError::ConnectionClosed`] when the connection is gone.
    fn read_chunk(
        &mut self,
        max_len: usize,
    ) -> impl Future<Output = Result<Option<Bytes>, StreamReadError>> + Send;

    /// Stops reading: the peer receives `STOP_SENDING` with `code` and unread
    /// data is discarded. Idempotent.
    fn stop(&mut self, code: u32);
}

/// Per-connection hot-path I/O.
///
/// # Consumed by generics, never by `dyn`
///
/// The asynchronous methods use return-position `impl Future` with an explicit
/// `+ Send` bound rather than `async fn`. Two reasons, both load-bearing:
///
/// - **`Send` must be part of the contract.** A bare `async fn` in a trait does
///   not guarantee the returned future is `Send`, so a generic consumer spawned
///   with `tokio::spawn` fails to compile at the *call site* — meaning the
///   defect would not surface in this module, or in a test double, but only in
///   the forward path built against it later, after code depends on the
///   signature. That is exactly the "cheap against nothing, expensive against
///   concrete types" ordering ADR-0036 invokes, applied to the seam's own
///   signature.
/// - **It makes `dyn` unrepresentable.** This trait is not dyn-compatible
///   (E0038), so the no-`dyn`-dispatch requirement needs no guard and no
///   reviewer vigilance. **This is deliberate.** An author who hits E0038 and
///   "fixes" it by boxing the futures reintroduces the per-frame allocation the
///   seam exists to avoid; the correct response is to add a type parameter.
pub trait MediaTransport: Send + Sync + 'static {
    /// The unidirectional send stream this transport produces.
    type SendStream: MediaSendStream;

    /// The unidirectional receive stream a peer opens.
    type RecvStream: MediaRecvStream;

    /// Sends one datagram, without blocking.
    ///
    /// **Synchronous by design.** `wtransport::Connection::send_datagram` is a
    /// plain `fn`, not an `async fn` (`wtransport-0.7.2/src/connection.rs:297`).
    /// An `async` signature here would force the real implementation to return
    /// a future that never yields, putting a state machine and a misleading
    /// await point on the per-frame path. It also makes the back-pressure
    /// contract natural: a non-blocking try-send that hands the payload back is
    /// what [`DatagramSendError::WouldBlock`] expresses.
    ///
    /// Takes [`Bytes`] by value so fan-out can share one ingress buffer across
    /// subscribers and so a refusal can return the payload uncopied.
    ///
    /// # Errors
    ///
    /// [`DatagramSendError::WouldBlock`] carries the payload back (test doubles
    /// only — see its documentation), [`DatagramSendError::TooLarge`] if the
    /// datagram exceeds the connection's current limit,
    /// [`DatagramSendError::ConnectionClosed`] if the connection is gone, and
    /// [`DatagramSendError::DatagramsUnsupported`] if the peer never supported
    /// datagrams.
    fn send_datagram(&self, payload: Bytes) -> Result<(), DatagramSendError>;

    /// Receives one datagram payload.
    ///
    /// Returns [`Bytes`] rather than a borrowed slice so the value can cross a
    /// spawn boundary, and rather than a `Vec<u8>` so no copy is forced: the
    /// real implementation returns a zero-copy slice of a buffer the transport
    /// already owns. Callers check `.len()` against `media-protocol`'s
    /// `MAX_PAYLOAD_BYTES` before any per-frame work.
    ///
    /// # Errors
    ///
    /// [`TransportError::ConnectionClosed`] when the connection is gone. This
    /// is the forward path's loop-termination signal.
    fn recv_datagram(&self) -> impl Future<Output = Result<Bytes, TransportError>> + Send;

    /// Opens a unidirectional stream to the peer.
    ///
    /// # Errors
    ///
    /// [`TransportError::ConnectionClosed`] when the connection is gone.
    fn open_uni(&self) -> impl Future<Output = Result<Self::SendStream, TransportError>> + Send;

    /// Accepts the next unidirectional stream the peer opened.
    ///
    /// Concurrency is bounded below this call by the declared QUIC transport
    /// parameter `MH_MAX_CONCURRENT_UNI_STREAMS`; the per-connection creation
    /// RATE is enforced by the caller: an over-limit stream has only its
    /// header prefix read (its `GoP` counter recorded, story 3 amendment 4)
    /// before it is stopped.
    ///
    /// # Errors
    ///
    /// [`TransportError::ConnectionClosed`] when the connection is gone — the
    /// stream accept loop's termination signal.
    fn accept_uni(&self) -> impl Future<Output = Result<Self::RecvStream, TransportError>> + Send;
}

#[cfg(test)]
mod tests {
    //! Unit coverage for the seam's own value types.
    //!
    //! The traits themselves are exercised by
    //! `tests/transport_seam_reachability.rs` against the deterministic double
    //! in `mh-test-utils`; these tests cover only what lives in this file.

    use super::{DatagramSendError, StreamReadError, TransportCause, TransportError};
    use bytes::Bytes;

    #[test]
    fn would_block_returns_the_same_allocation_uncopied() {
        // Pointer identity, not byte equality: byte equality would also pass
        // against an implementation that copied, which is the property this
        // variant exists to rule out.
        let payload = Bytes::from(vec![7_u8, 8, 9, 10]);
        let original_ptr = payload.as_ptr();
        let original_len = payload.len();

        let recovered = DatagramSendError::WouldBlock(payload)
            .into_payload()
            .expect("WouldBlock carries a payload");

        assert_eq!(recovered.as_ptr(), original_ptr, "payload was copied");
        assert_eq!(recovered.len(), original_len);
    }

    #[test]
    fn payload_free_variants_recover_nothing() {
        assert!(DatagramSendError::TooLarge.into_payload().is_none());
        assert!(DatagramSendError::ConnectionClosed.into_payload().is_none());
        assert!(DatagramSendError::DatagramsUnsupported
            .into_payload()
            .is_none());
    }

    #[test]
    fn datagram_send_error_display_is_stable() {
        assert_eq!(
            DatagramSendError::WouldBlock(Bytes::new()).to_string(),
            "datagram send would block"
        );
        assert_eq!(
            DatagramSendError::TooLarge.to_string(),
            "datagram too large"
        );
        assert_eq!(
            DatagramSendError::ConnectionClosed.to_string(),
            "connection closed"
        );
        assert_eq!(
            DatagramSendError::DatagramsUnsupported.to_string(),
            "datagrams unsupported by peer"
        );
    }

    #[test]
    fn transport_error_display_is_stable() {
        assert_eq!(
            TransportError::ConnectionClosed(TransportCause::local("x")).to_string(),
            "connection closed"
        );
        assert_eq!(
            TransportError::StreamClosed(TransportCause::local("x")).to_string(),
            "stream closed"
        );
    }

    #[test]
    fn the_cause_survives_as_the_error_source() {
        use std::error::Error as _;
        let error = TransportError::ConnectionClosed(TransportCause::local("idle timeout"));
        let source = error.source().expect("the cause is the source");
        assert_eq!(source.to_string(), "idle timeout");
    }

    #[test]
    fn equality_is_variant_only_and_reset_is_code_sensitive() {
        assert_eq!(
            TransportError::ConnectionClosed(TransportCause::local("a")),
            TransportError::ConnectionClosed(TransportCause::local("b"))
        );
        assert_ne!(
            TransportError::ConnectionClosed(TransportCause::local("a")),
            TransportError::StreamClosed(TransportCause::local("a"))
        );
        assert_eq!(StreamReadError::Reset(7), StreamReadError::Reset(7));
        assert_ne!(StreamReadError::Reset(7), StreamReadError::Reset(8));
    }
}
