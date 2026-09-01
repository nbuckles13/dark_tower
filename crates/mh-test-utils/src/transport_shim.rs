//! Deterministic test double for the MH per-connection transport seam
//! (ADR-0036 §10).
//!
//! [`LossDelayTransport`] implements `mh_service::transport::MediaTransport`
//! entirely in memory. It exists so that every drop path ADR-0036 asks a
//! counter to observe can be *made to fire on demand*, rather than being
//! believed to be reachable.
//!
//! # Zero syscalls
//!
//! Nothing here opens a socket, a file or a timer fd. Ingress is a bounded
//! `tokio::sync::mpsc` channel; delay is `tokio::time::sleep_until`. A caller
//! can largely prove the property rather than assert it, by driving the shim
//! on a runtime built with `enable_time()` and **no I/O driver**: any *tokio*
//! network or file I/O on such a runtime panics with "there is no reactor
//! running". `crates/mh-service/tests/transport_seam_reachability.rs` does
//! exactly that.
//!
//! **Stated at its true strength**: that control traps tokio-driven I/O, which
//! is the real risk here because the production implementation of this seam
//! goes through wtransport and quinn, both of which are tokio-based — so a
//! forward loop that accidentally reached the network WOULD trip it. It would
//! NOT trap a blocking `std::net` call or a raw syscall issued outside tokio.
//! Nothing in this shim does either, but "no syscalls are possible" would be a
//! stronger claim than the mechanism supports, and the difference matters to
//! whoever extends the driven path later.
//!
//! # Deterministic time — no `Clock` trait
//!
//! Delay is `tokio::time::sleep_until`, released by `tokio::time::advance`
//! under a `start_paused` runtime. The shim takes no clock argument and there
//! is no `Clock` trait anywhere in this workspace; the established idiom is
//! tokio's paused timer (see `crates/mc-service/src/actors/meeting.rs` and
//! `crates/mh-service/src/webtransport/connection.rs`). ADR-0036 §10 Tier 1a's
//! "All deadline logic takes an injected clock" names that *property* — a
//! deadline must be drivable from a test — and a test runtime substituting
//! virtual time at the runtime boundary satisfies it. A drift entry proposing
//! the ADR reword is in `docs/TODO.md` §Documentation Hygiene.
//!
//! Added delay is **path latency, not per-call cost**: [`LossDelayTransport::push_inbound`]
//! stamps `ready_at = now + delay` and [`LossDelayTransport::recv_datagram`]
//! sleeps until it, so N datagrams injected at one virtual instant all become
//! readable at one virtual instant. On the unidirectional-stream path it is a
//! per-call sleep in `write_all`/`finish`, which is the slow-subscriber
//! mechanism ADR-0036 §10's forward-path gate needs.
//!
//! # THE FIDELITY BOUNDARY — read this before writing a zero-copy gate
//!
//! **Production copies the datagram payload at the transport boundary, and it
//! does so structurally.** The signatures say it, not just the bodies:
//!
//! * `wtransport-0.7.2/src/connection.rs:297-302` —
//!   `send_datagram<D: AsRef<[u8]>>(&self, payload: D)` **borrows**. Handing it
//!   a `Bytes` by value transfers ownership to nothing.
//! * `wtransport-0.7.2/src/driver/mod.rs:206-213` —
//!   `send_datagram(&self, session_id, payload: &[u8])`.
//! * `wtransport-0.7.2/src/datagram.rs:35-44` allocates
//!   `vec![0; h3dgram.write_size()]`, and
//!   `wtransport-proto-0.7.2/src/datagram.rs:50-66` then does
//!   `put_varint(qstream_id)` followed by `put_bytes(payload)`. **The payload
//!   is copied to prepend the WebTransport session-id varint.** H3 datagram
//!   framing requires that prefix, so the copy is permanent, not incidental.
//!
//! Two consequences:
//!
//! 1. `Bytes` by value on `send_datagram` is the right type, but **not**
//!    because the write to the wire is zero-copy — it is not. The two real
//!    justifications are both *above* the transport boundary: the
//!    `WouldBlock(Bytes)` hand-back needs ownership to requeue without
//!    copying, and MH's fan-out shares one ingress buffer across N egress
//!    payloads before any transport call is made.
//! 2. **Scope the refcount gate to the fan-out, upstream of the trait call.**
//!    A gate written as "hand a `Bytes` to `send_datagram`, then assert
//!    sharing" passes against this shim while asserting a property production
//!    does not have — a test double certifying fidelity it does not possess,
//!    which reads as coverage. ADR-0036 §10 already states the principle in
//!    the other direction ("scoped to the fan-out, not across the stream-read
//!    boundary where a copy is inherent"); the send boundary is the same case.
//!    Assert one ingress `Bytes` and N egress payloads sharing an allocation
//!    **before** the trait call; assert nothing about refcounts surviving it.
//!
//! Related, and deliberately reachable from here rather than only from a
//! document someone must know to search: `docs/TODO.md`
//! §Media Path Obligations records that quinn silently evicts the OLDEST
//! queued datagram beneath this seam, so MH's own egress-drop counter is
//! structurally blind to that loss and the far-end hop-sequence gap counter is
//! the compensating control. That entry and this block are the two halves of
//! one fact about what this shim can and cannot faithfully represent.
//!
//! The one refcount-shaped assertion that IS legitimate is on the
//! `WouldBlock(Bytes)` hand-back, because on that path the payload never
//! reached the wire. That is a property of *this shim's* contract, which
//! requeue tests are built on — it does not describe the real implementation,
//! which never produces `WouldBlock` at all (see [`Self::set_send_capacity`]).
//!
//! **The stream path makes no zero-copy claim whatsoever.** `write_all` takes
//! `&[u8]` (framing stays above the seam), so [`FinishedUniStream`] capture
//! necessarily copies.
//!
//! # Benchmark representativeness
//!
//! ADR-0036 §10 justifies this seam partly on benchmark representativeness and
//! drives its Tier-2 regression benchmark through it. Since the shim is
//! allocation-free on the send path while production pays an allocation and a
//! copy per datagram, a Tier-2 delta measures the forward path *above* the
//! seam and deliberately excludes a known per-datagram allocation below it. A
//! green delta is a narrower claim than it first appears.
//!
//! # What is deliberately NOT here
//!
//! No bounded egress queue, no drop-oldest policy, no ingress or egress loop.
//! Those belong to the forward-path task in `crates/mh-service/src/media/`;
//! building a queue here would be the second abstraction the transport-config
//! task is required not to introduce. None of the reachability assertions need
//! one.

// FILE-SCOPED no-panic bar. The crate opts out of workspace lints so its
// assertion helpers can panic (that is how a test double fails a test), but
// THIS file is the code that drives the ADR-0036 §10 Tier-2 benchmark path,
// and the rest of this design prefers structural enforcement to convention
// everywhere else (dyn-incompatibility via RPITIT, zero syscalls via a runtime
// with no I/O driver, drop-path coverage via exhaustive match). Leaving the
// shim's no-panic posture on convention was the one lapse. A genuine future
// need surfaces as `#[expect(..., reason = "...")]`, never a bare `#[allow]`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bytes::Bytes;
use mh_service::transport::{DatagramSendError, MediaSendStream, MediaTransport, TransportError};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};
use tokio::time::Instant;

/// Sentinel for "no maximum datagram size configured".
const NO_MAX_DATAGRAM_SIZE: usize = usize::MAX;
/// Sentinel for "send capacity is unlimited".
const UNLIMITED_CAPACITY: u64 = u64::MAX;

/// Default depth of the injected-ingress queue.
pub const DEFAULT_INBOUND_CAPACITY: usize = 1024;
/// Default number of delivered datagrams retained for inspection.
pub const DEFAULT_CAPTURE_LIMIT: usize = 4096;
/// Default number of finished unidirectional streams retained for inspection.
///
/// Lower than [`DEFAULT_CAPTURE_LIMIT`] because each retained entry holds every
/// byte written to that stream, not one datagram. Under ADR-0036 §1 a video
/// stream carries a whole group of pictures, so an unbounded history is a
/// multi-megabyte-per-iteration leak on the Tier-2 benchmark path.
pub const DEFAULT_FINISHED_STREAM_LIMIT: usize = 256;

/// A deterministic, counted loss schedule. No randomness, seeded or otherwise:
/// a probabilistic double makes a reachability test a coin flip, and
/// ADR-0028's zero-retry policy leaves no room for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatagramLoss {
    /// Deliver everything.
    #[default]
    None,
    /// Drop the next `n` datagrams offered, then deliver normally. Use this to
    /// manufacture one gap at a known position.
    Next(u32),
    /// Drop every `n`-th datagram, counting offers from 1.
    EveryNth(u32),
    /// Drop everything.
    All,
}

/// Lock-free encoding of a [`DatagramLoss`] schedule.
#[derive(Debug, Default)]
struct LossSchedule {
    /// 0 = None, 1 = Next, 2 = EveryNth, 3 = All.
    mode: AtomicU8,
    /// Remaining count for `Next`; the divisor for `EveryNth`.
    param: AtomicU32,
}

impl LossSchedule {
    fn set(&self, loss: DatagramLoss) {
        let (mode, param) = match loss {
            DatagramLoss::None => (0_u8, 0_u32),
            DatagramLoss::Next(n) => (1, n),
            // A zero divisor would mean "drop nothing" by accident. Clamp to 1
            // ("drop every one") so a caller who writes `EveryNth(0)` gets a
            // loud, total loss rather than a silent no-op.
            DatagramLoss::EveryNth(n) => (2, n.max(1)),
            DatagramLoss::All => (3, 0),
        };
        // param first: a reader that observes the new mode must not be able to
        // observe the old param alongside it.
        self.param.store(param, Ordering::Release);
        self.mode.store(mode, Ordering::Release);
    }

    /// Consult the schedule for the `offer_index`-th datagram (1-based).
    fn should_drop(&self, offer_index: u64) -> bool {
        match self.mode.load(Ordering::Acquire) {
            1 => self
                .param
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok(),
            2 => {
                let divisor = u64::from(self.param.load(Ordering::Acquire).max(1));
                offer_index.is_multiple_of(divisor)
            }
            3 => true,
            _ => false,
        }
    }
}

/// Construction-time sizing for [`LossDelayTransport`].
///
/// **All three retention sites** are pre-sized at construction, so none
/// allocates on the measured path and none grows without bound during a
/// benchmark: the ingress queue, the delivered-datagram capture, and the
/// finished-stream history. Each bound is enforced where it is retained, and
/// each overflow is made loud rather than silent — a truncated capture that
/// looked complete would make an assertion wrong for reasons unrelated to its
/// subject.
#[derive(Debug, Clone, Copy)]
pub struct ShimConfig {
    /// Depth of the injected-ingress queue. `push_inbound` hands the payload
    /// back when this is full rather than dropping it silently.
    pub inbound_capacity: usize,
    /// How many delivered egress datagrams are retained for inspection.
    pub capture_limit: usize,
    /// How many finished unidirectional streams are retained for inspection.
    ///
    /// Separate from [`Self::capture_limit`] because the cost per entry is
    /// different in kind: a captured datagram is one `Bytes` handle, whereas a
    /// finished stream owns every chunk written to it.
    pub finished_stream_limit: usize,
}

impl Default for ShimConfig {
    fn default() -> Self {
        Self {
            inbound_capacity: DEFAULT_INBOUND_CAPACITY,
            capture_limit: DEFAULT_CAPTURE_LIMIT,
            finished_stream_limit: DEFAULT_FINISHED_STREAM_LIMIT,
        }
    }
}

/// One injected ingress datagram plus the virtual instant it becomes readable.
#[derive(Debug)]
struct Inbound {
    payload: Bytes,
    ready_at: Instant,
}

/// A unidirectional stream that reached `finish()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedUniStream {
    /// Monotonic open-ordinal assigned at `open_uni()`, starting at 0.
    ///
    /// Deliberately NOT called `stream_id`: ADR-0036 §2 gives that name to the
    /// media-stream identity in the relay region — the subscriber slot MH
    /// rewrites per frame — and §11 bars stream-identity as a dimension
    /// anywhere on the media path. This is a shim-local count of how many
    /// streams this double has opened. It carries no protocol, participant or
    /// media meaning, and nothing should ever map it onto one.
    pub open_ordinal: u64,
    /// Chunks in write order. These are COPIES: `write_all` takes `&[u8]`, so
    /// there is no zero-copy claim on this path.
    pub chunks: Vec<Vec<u8>>,
}

#[derive(Debug)]
struct ShimState {
    // ---- knobs -------------------------------------------------------
    egress_loss: LossSchedule,
    ingress_loss: LossSchedule,
    send_capacity: AtomicU64,
    added_delay_nanos: AtomicU64,
    max_datagram_size: AtomicUsize,
    closed: AtomicBool,
    datagrams_unsupported: AtomicBool,

    // ---- counters, one per distinct outcome --------------------------
    egress_accepted: AtomicU64,
    egress_delivered: AtomicU64,
    egress_dropped_in_flight: AtomicU64,
    refused_for_capacity: AtomicU64,
    ingress_offered: AtomicU64,
    ingress_dropped: AtomicU64,

    // ---- capture -----------------------------------------------------
    capture_limit: usize,
    finished_stream_limit: usize,
    egress_capture: Mutex<Vec<Bytes>>,
    capture_truncated: AtomicBool,

    // ---- unidirectional streams --------------------------------------
    next_open_ordinal: AtomicU64,
    open_uni_streams: AtomicUsize,
    max_concurrent_open_uni: AtomicUsize,
    finished_uni_streams: Mutex<Vec<FinishedUniStream>>,
    finished_streams_truncated: AtomicBool,

    // ---- ingress -----------------------------------------------------
    inbound_tx: mpsc::Sender<Inbound>,
    inbound_rx: AsyncMutex<mpsc::Receiver<Inbound>>,
    closed_notify: Notify,
}

impl ShimState {
    fn added_delay(&self) -> Duration {
        Duration::from_nanos(self.added_delay_nanos.load(Ordering::Acquire))
    }

    /// Sleep the configured added delay, if any. A zero delay does not touch
    /// the timer at all, so a no-delay benchmark run has no timer registration
    /// in its measured path.
    async fn sleep_added_delay(&self) {
        let delay = self.added_delay();
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }
}

/// Deterministic in-memory `MediaTransport` with loss, delay and
/// back-pressure knobs.
///
/// Cheap to clone: every clone shares one state, so knobs remain settable
/// through `&self` after the shim has been handed to a forwarder that owns it.
#[derive(Debug, Clone)]
pub struct LossDelayTransport {
    state: Arc<ShimState>,
}

impl Default for LossDelayTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl LossDelayTransport {
    /// Build a shim with [`ShimConfig::default`] sizing.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ShimConfig::default())
    }

    /// Build a shim with explicit sizing.
    #[must_use]
    pub fn with_config(config: ShimConfig) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(config.inbound_capacity.max(1));
        Self {
            state: Arc::new(ShimState {
                egress_loss: LossSchedule::default(),
                ingress_loss: LossSchedule::default(),
                send_capacity: AtomicU64::new(UNLIMITED_CAPACITY),
                added_delay_nanos: AtomicU64::new(0),
                max_datagram_size: AtomicUsize::new(NO_MAX_DATAGRAM_SIZE),
                closed: AtomicBool::new(false),
                datagrams_unsupported: AtomicBool::new(false),
                egress_accepted: AtomicU64::new(0),
                egress_delivered: AtomicU64::new(0),
                egress_dropped_in_flight: AtomicU64::new(0),
                refused_for_capacity: AtomicU64::new(0),
                ingress_offered: AtomicU64::new(0),
                ingress_dropped: AtomicU64::new(0),
                capture_limit: config.capture_limit,
                finished_stream_limit: config.finished_stream_limit,
                egress_capture: Mutex::new(Vec::with_capacity(config.capture_limit)),
                capture_truncated: AtomicBool::new(false),
                next_open_ordinal: AtomicU64::new(0),
                open_uni_streams: AtomicUsize::new(0),
                max_concurrent_open_uni: AtomicUsize::new(0),
                finished_uni_streams: Mutex::new(Vec::with_capacity(config.finished_stream_limit)),
                finished_streams_truncated: AtomicBool::new(false),
                inbound_tx,
                inbound_rx: AsyncMutex::new(inbound_rx),
                closed_notify: Notify::new(),
            }),
        }
    }

    // ================================================================
    // Knobs
    // ================================================================

    /// Drop datagrams MH SENDS: accepted by [`MediaTransport::send_datagram`]
    /// (which returns `Ok`) but never delivered to the capture sink.
    ///
    /// # This direction fires no MH counter, and that is the point
    ///
    /// ADR-0036 §2 makes the hop sequence **bidirectional** — "it applies to
    /// the client's uplink as well as MH's downlink, so each side can detect
    /// loss on the hop it receives". MH writes its own downlink hop sequence,
    /// so it structurally cannot observe a gap in its own output: egress loss
    /// is visible only to the far-end client. Assert it on
    /// [`Self::delivered_datagrams`], never on an MH counter.
    ///
    /// Had this knob been the only direction — the inversion considered and
    /// rejected — MH's own uplink gap detector would have had **no firing path
    /// at all**, and a metric-coverage test for it would have had to assert
    /// against the one signal that cannot reach it. See
    /// [`Self::set_ingress_datagram_loss`] for the direction that does fire it.
    pub fn set_egress_datagram_loss(&self, loss: DatagramLoss) {
        self.state.egress_loss.set(loss);
    }

    /// Drop datagrams MH RECEIVES: accepted by [`Self::push_inbound`] (which
    /// returns `Ok`) but never queued, so a `recv_datagram` loop observes a
    /// gap rather than a swallowed item.
    ///
    /// This is upstream of MH, is what MH's own uplink hop-sequence gap
    /// detector reads, and is the **only** firing path for that future
    /// counter.
    ///
    /// It is **not interchangeable with burst injection.** Pushing faster than
    /// the consumer drains trips MH's own bounded inbound queue — MH's own
    /// refusal, a different counter with a different meaning. Loss produces a
    /// gap; saturation produces a refusal. A test that reaches for one to
    /// exercise the other is asserting against the wrong mechanism.
    pub fn set_ingress_datagram_loss(&self, loss: DatagramLoss) {
        self.state.ingress_loss.set(loss);
    }

    /// Accept `n` more datagrams, then refuse with
    /// `DatagramSendError::WouldBlock(payload)`, handing the payload back
    /// uncopied.
    ///
    /// # No production producer, by design
    ///
    /// Verified against the locked versions (`Cargo.lock`: wtransport 0.7.2,
    /// quinn 0.11.11, quinn-proto 0.11.17):
    ///
    /// * wtransport surfaces three variants — `NotConnected`,
    ///   `UnsupportedByPeer`, `TooLarge` (`src/error.rs:210-221`, mapped at
    ///   `src/driver/mod.rs:214-227`). None is a refusal.
    /// * `quinn_proto::SendDatagramError::Blocked(Bytes)` DOES exist
    ///   (`quinn-proto-0.11.17/src/connection/datagrams.rs:243`) and is
    ///   structurally this exact shape — but it is unreachable *by
    ///   construction*: `quinn-0.11.11/src/connection.rs:442` calls
    ///   `datagrams().send(data, true)` with `drop` hardcoded true, and `:448`
    ///   matches the blocked arm as `unreachable!()`. With `drop == true`,
    ///   quinn silently evicts the OLDEST queued datagram instead of refusing.
    ///   That silent eviction is the ADR-0036 §1/§11 gap MH's own bounded
    ///   egress queue exists to close.
    /// * quinn does offer a congestion-aware `send_datagram_wait`
    ///   (`quinn-0.11.11/src/connection.rs:464`), which wtransport does not
    ///   use — but it *awaits* rather than returning a blocked error, so there
    ///   is no `WouldBlock`-shaped **return** anywhere in the stack on any
    ///   path.
    ///
    /// So this knob is the deterministic trip-wire for the future
    /// egress-overflow counter, and the shim is its only producer.
    ///
    /// **In production the refusal is produced by MH's own bounded egress
    /// queue ABOVE the seam; the transport below the seam never refuses.** No
    /// doc, metric description, dashboard panel, alert or runbook sentence may
    /// therefore describe the egress-overflow counter as observing *transport*
    /// back-pressure — it is wholly a property of MH's own queue. Getting that
    /// wrong reproduces exactly the shape ADR-0036's "A control's coverage
    /// must be demonstrated, not asserted" warns about.
    ///
    /// **The escape hatch, named because someone will look for it.** wtransport
    /// exposes the raw connection at `src/connection.rs:415`,
    /// `quic_connection() -> &quinn::Connection`. It sits behind
    /// `#[cfg(feature = "quinn")]`, which this workspace does not enable
    /// (`Cargo.toml` declares `wtransport = "0.7"` with no `features` key; the
    /// only feature either service names is dev-scoped
    /// `dangerous-configuration`), so it does not compile today. And even after
    /// a manifest edit it sends a **QUIC** datagram *below* WebTransport,
    /// skipping the H3 session-id varint that `Datagram::write` prepends
    /// (`src/datagram.rs:35-48`) — producing frames a conforming peer drops.
    /// Taking the hatch means reimplementing H3 datagram framing inside MH.
    /// It is self-defeating rather than merely gated, which is a better control
    /// than a gate because it depends on nobody maintaining it.
    pub fn set_send_capacity(&self, n: u64) {
        self.state.send_capacity.store(n, Ordering::Release);
    }

    /// Restore unlimited send capacity.
    pub fn clear_send_capacity(&self) {
        self.state
            .send_capacity
            .store(UNLIMITED_CAPACITY, Ordering::Release);
    }

    /// Set the added path delay. See the module docs: this is path latency on
    /// the ingress path (`ready_at` stamped at injection) and a per-call cost
    /// on the unidirectional-stream path (the slow-subscriber mechanism).
    pub fn set_added_delay(&self, delay: Duration) {
        let nanos = u64::try_from(delay.as_nanos()).unwrap_or(u64::MAX);
        self.state.added_delay_nanos.store(nanos, Ordering::Release);
    }

    /// Configure a maximum datagram size, so `DatagramSendError::TooLarge` is
    /// reachable. `None` removes the limit.
    pub fn set_max_datagram_size(&self, max: Option<usize>) {
        self.state
            .max_datagram_size
            .store(max.unwrap_or(NO_MAX_DATAGRAM_SIZE), Ordering::Release);
    }

    /// Mark the connection closed. Wakes a blocked
    /// [`MediaTransport::recv_datagram`] rather than hanging it.
    pub fn set_closed(&self) {
        self.state.closed.store(true, Ordering::Release);
        self.state.closed_notify.notify_waiters();
    }

    /// Make the peer refuse datagram support, so
    /// `DatagramSendError::DatagramsUnsupported` is reachable.
    pub fn set_datagrams_unsupported(&self, unsupported: bool) {
        self.state
            .datagrams_unsupported
            .store(unsupported, Ordering::Release);
    }

    // ================================================================
    // Ingress injection
    // ================================================================

    /// Inject a datagram for MH to receive.
    ///
    /// Returns `Err(payload)` when the bounded ingress queue is full or the
    /// receiver is gone — handing the payload back and failing loudly rather
    /// than dropping it silently, which would be indistinguishable from the
    /// loss this shim injects deliberately.
    ///
    /// # Errors
    ///
    /// Returns the payload unchanged if the queue cannot accept it.
    pub fn push_inbound(&self, payload: Bytes) -> Result<(), Bytes> {
        let index = self.state.ingress_offered.fetch_add(1, Ordering::AcqRel) + 1;
        if self.state.ingress_loss.should_drop(index) {
            self.state.ingress_dropped.fetch_add(1, Ordering::AcqRel);
            // Accepted, deliberately never delivered.
            return Ok(());
        }
        let ready_at = Instant::now() + self.state.added_delay();
        self.state
            .inbound_tx
            .try_send(Inbound { payload, ready_at })
            .map_err(|e| match e {
                TrySendError::Full(item) | TrySendError::Closed(item) => item.payload,
            })
    }

    // ================================================================
    // Observation — one accessor per distinct outcome, never one shared
    // `drops`, so two counters downstream cannot be conflated into one.
    // ================================================================

    /// Datagrams MH sent that this shim accepted and deliberately did not
    /// deliver. Observable only by the far end in production.
    #[must_use]
    pub fn egress_dropped_in_flight(&self) -> u64 {
        self.state.egress_dropped_in_flight.load(Ordering::Acquire)
    }

    /// Injected datagrams that this shim accepted and deliberately did not
    /// queue. The firing path for MH's own uplink gap detection.
    #[must_use]
    pub fn ingress_dropped(&self) -> u64 {
        self.state.ingress_dropped.load(Ordering::Acquire)
    }

    /// Datagram sends refused with `WouldBlock`. Distinct from both loss
    /// counters: the payload came back and MH still owns it.
    #[must_use]
    pub fn refused_for_capacity(&self) -> u64 {
        self.state.refused_for_capacity.load(Ordering::Acquire)
    }

    /// Datagrams that reached the capture sink.
    #[must_use]
    pub fn delivered_count(&self) -> u64 {
        self.state.egress_delivered.load(Ordering::Acquire)
    }

    /// Datagrams `send_datagram` ACCEPTED — those that passed the closed,
    /// unsupported, size and capacity checks and went on to be either
    /// delivered or dropped in flight. It is therefore exactly
    /// `delivered_count() + egress_dropped_in_flight()`.
    ///
    /// Deliberately NOT a count of everything offered: a refusal consumes no
    /// capacity and is not an acceptance, so it does not appear here. It is
    /// counted by [`Self::refused_for_capacity`] instead. Naming this
    /// "offered" would make it read as a denominator it cannot serve.
    #[must_use]
    pub fn egress_accepted(&self) -> u64 {
        self.state.egress_accepted.load(Ordering::Acquire)
    }

    /// Delivered payloads, in send order. Cloning `Bytes` bumps a refcount and
    /// copies nothing, so a caller can compare allocations.
    #[must_use]
    pub fn delivered_datagrams(&self) -> Vec<Bytes> {
        self.state
            .egress_capture
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// True once delivery exceeded the configured capture limit.
    ///
    /// Check this before asserting on [`Self::delivered_datagrams`]: past the
    /// limit the shim keeps counting but stops capturing, and an assertion
    /// against a truncated capture would otherwise pass or fail for reasons
    /// unrelated to what it means to test.
    #[must_use]
    pub fn capture_truncated(&self) -> bool {
        self.state.capture_truncated.load(Ordering::Acquire)
    }

    /// True once finished streams exceeded the configured retention limit.
    ///
    /// Check this before asserting on [`Self::finished_uni_streams`], for the
    /// same reason as [`Self::capture_truncated`]: past the limit the shim
    /// stops retaining, and an assertion against a silently truncated history
    /// would pass or fail for reasons unrelated to what it means to test.
    #[must_use]
    pub fn finished_streams_truncated(&self) -> bool {
        self.state
            .finished_streams_truncated
            .load(Ordering::Acquire)
    }

    /// Unidirectional streams currently open (opened, not yet finished or
    /// dropped).
    #[must_use]
    pub fn open_uni_streams(&self) -> usize {
        self.state.open_uni_streams.load(Ordering::Acquire)
    }

    /// High-water mark of concurrently open unidirectional streams — the
    /// observation ADR-0036 §10's "concurrent unfinished groups stay at or
    /// below the application cap" gate reads.
    #[must_use]
    pub fn max_concurrent_open_uni(&self) -> usize {
        self.state.max_concurrent_open_uni.load(Ordering::Acquire)
    }

    /// Streams that reached `finish()`, in finish order.
    #[must_use]
    pub fn finished_uni_streams(&self) -> Vec<FinishedUniStream> {
        self.state
            .finished_uni_streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    // ================================================================
    // Internals
    // ================================================================

    fn capture_delivered(&self, payload: Bytes) {
        let mut sink = self
            .state
            .egress_capture
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if sink.len() >= self.state.capture_limit {
            // Keep counting, stop retaining, and make the truncation loud.
            self.state.capture_truncated.store(true, Ordering::Release);
            return;
        }
        sink.push(payload);
    }
}

impl MediaTransport for LossDelayTransport {
    type SendStream = ShimSendStream;

    /// Order of checks is the contract, because it is what keeps the two
    /// back-pressure and loss knobs mapped onto two structurally distinct
    /// outcomes:
    ///
    /// 1. closed → `ConnectionClosed`
    /// 2. datagrams-unsupported knob → `DatagramsUnsupported`
    /// 3. oversize → `TooLarge` (payload NOT handed back: not retryable at
    ///    this size, and returning it would invite a retry loop)
    /// 4. capacity exhausted → `WouldBlock(payload)`, payload MOVED back
    /// 5. loss schedule → `Ok(())`, payload never reaching the sink
    /// 6. otherwise deliver
    ///
    /// A refusal consumes no capacity and is not an offer for loss purposes:
    /// the datagram was never accepted.
    fn send_datagram(&self, payload: Bytes) -> Result<(), DatagramSendError> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(DatagramSendError::ConnectionClosed);
        }
        if self.state.datagrams_unsupported.load(Ordering::Acquire) {
            return Err(DatagramSendError::DatagramsUnsupported);
        }
        let max = self.state.max_datagram_size.load(Ordering::Acquire);
        if max != NO_MAX_DATAGRAM_SIZE && payload.len() > max {
            return Err(DatagramSendError::TooLarge);
        }

        // Consume one unit of capacity, or refuse. `fetch_update` keeps the
        // check-and-decrement atomic so a cloned handle cannot oversend.
        let consumed = self.state.send_capacity.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |remaining| {
                if remaining == UNLIMITED_CAPACITY {
                    Some(UNLIMITED_CAPACITY)
                } else {
                    remaining.checked_sub(1)
                }
            },
        );
        if consumed.is_err() {
            self.state
                .refused_for_capacity
                .fetch_add(1, Ordering::AcqRel);
            // The SAME allocation goes back. Nothing is cloned or copied.
            return Err(DatagramSendError::WouldBlock(payload));
        }

        let index = self.state.egress_accepted.fetch_add(1, Ordering::AcqRel) + 1;
        if self.state.egress_loss.should_drop(index) {
            self.state
                .egress_dropped_in_flight
                .fetch_add(1, Ordering::AcqRel);
            // Accepted, deliberately never delivered: `Ok`, no capture.
            return Ok(());
        }

        self.state.egress_delivered.fetch_add(1, Ordering::AcqRel);
        self.capture_delivered(payload);
        Ok(())
    }

    async fn recv_datagram(&self) -> Result<Bytes, TransportError> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(TransportError::ConnectionClosed);
        }
        let closed = self.state.closed_notify.notified();
        tokio::pin!(closed);

        let item = tokio::select! {
            () = &mut closed => return Err(TransportError::ConnectionClosed),
            item = async {
                let mut rx = self.state.inbound_rx.lock().await;
                rx.recv().await
            } => item,
        };

        match item {
            Some(inbound) => {
                // Released by `tokio::time::advance` under a paused runtime,
                // or by the runtime's own auto-advance when it goes idle.
                // Either way the VIRTUAL elapsed time is exactly the
                // configured delay.
                tokio::time::sleep_until(inbound.ready_at).await;
                Ok(inbound.payload)
            }
            None => Err(TransportError::ConnectionClosed),
        }
    }

    async fn open_uni(&self) -> Result<Self::SendStream, TransportError> {
        if self.state.closed.load(Ordering::Acquire) {
            return Err(TransportError::ConnectionClosed);
        }
        let open_ordinal = self.state.next_open_ordinal.fetch_add(1, Ordering::AcqRel);
        let open = self.state.open_uni_streams.fetch_add(1, Ordering::AcqRel) + 1;
        self.state
            .max_concurrent_open_uni
            .fetch_max(open, Ordering::AcqRel);
        Ok(ShimSendStream {
            state: Arc::clone(&self.state),
            open_ordinal,
            chunks: Vec::new(),
            finished: false,
        })
    }
}

/// A unidirectional send stream produced by [`LossDelayTransport::open_uni`].
///
/// Chunks are COPIED into the capture (`write_all` takes `&[u8]`), so this
/// path carries no zero-copy claim — see the module's fidelity-boundary
/// section.
#[derive(Debug)]
pub struct ShimSendStream {
    state: Arc<ShimState>,
    open_ordinal: u64,
    chunks: Vec<Vec<u8>>,
    finished: bool,
}

impl ShimSendStream {
    /// This stream's monotonic open-ordinal, assigned at open. See
    /// [`FinishedUniStream::open_ordinal`] for why this is not `stream_id`.
    #[must_use]
    pub fn open_ordinal(&self) -> u64 {
        self.open_ordinal
    }
}

impl MediaSendStream for ShimSendStream {
    fn write_all(
        &mut self,
        buf: &[u8],
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send {
        // Copy before the async block so the returned future borrows only
        // `self`, not `buf` — which is what lets a caller write from a
        // temporary slice.
        let chunk = buf.to_vec();
        async move {
            if self.finished {
                return Err(TransportError::StreamClosed);
            }
            if self.state.closed.load(Ordering::Acquire) {
                return Err(TransportError::ConnectionClosed);
            }
            // Per-call delay: this is the slow-subscriber mechanism.
            self.state.sleep_added_delay().await;
            self.chunks.push(chunk);
            Ok(())
        }
    }

    async fn finish(&mut self) -> Result<(), TransportError> {
        if self.finished {
            return Err(TransportError::StreamClosed);
        }
        if self.state.closed.load(Ordering::Acquire) {
            return Err(TransportError::ConnectionClosed);
        }
        self.state.sleep_added_delay().await;
        self.finished = true;
        self.state.open_uni_streams.fetch_sub(1, Ordering::AcqRel);
        let finished = FinishedUniStream {
            open_ordinal: self.open_ordinal,
            chunks: std::mem::take(&mut self.chunks),
        };
        // Same shape as `capture_delivered`: keep counting, stop retaining,
        // make the truncation loud. Bounded because each entry owns every byte
        // written to its stream — under ADR-0036 §1 a video stream carries a
        // whole group of pictures, so an unbounded history would leak
        // monotonically across a Tier-2 benchmark run and pollute the very
        // deltas that benchmark exists to compare.
        let mut history = self
            .state
            .finished_uni_streams
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if history.len() >= self.state.finished_stream_limit {
            self.state
                .finished_streams_truncated
                .store(true, Ordering::Release);
        } else {
            history.push(finished);
        }
        drop(history);
        Ok(())
    }
}

impl Drop for ShimSendStream {
    /// A stream dropped without `finish()` models an abandoned group of
    /// pictures (ADR-0036 §1's reset case). It leaves the open count correct
    /// so the concurrency high-water mark stays meaningful, and it is NOT
    /// recorded as finished — an abandoned group must not read as a delivered
    /// one.
    fn drop(&mut self) {
        if !self.finished {
            self.state.open_uni_streams.fetch_sub(1, Ordering::AcqRel);
        }
    }
}
