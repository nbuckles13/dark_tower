//! The unidirectional-stream half of the deterministic transport double
//! ([`crate::transport_shim::LossDelayTransport`]), for story 3's
//! stream-per-group video path (ADR-0036 §1, §10; story task MH-A2).
//!
//! Everything a cut-through forward-path test needs to MANUFACTURE, on tokio
//! paused time and with zero syscalls:
//!
//! * **Inbound streams in caller-chosen chunks.** [`InboundUni`] feeds `Bytes`
//!   chunks split anywhere — inside the header included — then a FIN, a
//!   RESET_STREAM, or nothing at all (a publisher stalled mid-frame). It
//!   records whether MH `stop()`ped the stream, with which code, and how many
//!   bytes MH actually read (zero for a refused stream).
//! * **Send streams with a recorded terminal state.** Every stream MH opens is
//!   an [`EgressUni`] record: `Open`, `Finished`, or `Reset { code, by_drop }`.
//!   A stream DROPPED without `finish`/`reset` records
//!   `Reset { code: GROUP_ABANDONED, by_drop: true }` — the seam's abandonment
//!   contract (`mh_service::transport`), which the production `Drop` honours
//!   and `tests/transport_real_impl.rs` proves on a real QUIC pair. Modelling
//!   quinn's FINISH-on-drop here instead would make every reset-never-finish
//!   gate vacuous in the other direction.
//! * **Controllable credit** (the slow subscriber). Connection-wide stream
//!   CREDIT in bytes, and a stream-OPEN credit (QUIC `MAX_STREAMS`): when
//!   exhausted, `write_all` / `open_uni` pend until a test grants more.
//! * **Counts** of streams offered, accepted and stopped (inbound) and opened,
//!   current and maximum concurrent (outbound).
//! * **The MH↔client control envelope, both ways** (story 3 MH-B: keyframe
//!   and recovery requests), as seam stream halves MH's own framer reads and
//!   writes (`LossDelayTransport::control_halves`). Inbound items are RAW
//!   bytes (a test pushes framed messages, or deliberately broken framing);
//!   each outbound record is one `write_all`, i.e. one framed message. Writes
//!   can be BLOCKED (a publisher that never reads) or made to FAIL.
//!
//! The codes come from `mh_service::transport` and are never restated.

use bytes::Bytes;
use mh_service::transport::GROUP_ABANDONED;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::Notify;

/// One item a test fed into an inbound stream.
#[derive(Debug)]
pub(crate) enum InboundItem {
    Chunk(Bytes),
    Fin,
    Reset(u64),
}

/// Shared state of one inbound (publisher → MH) stream.
#[derive(Debug, Default)]
pub(crate) struct InboundShared {
    pub(crate) items: Mutex<VecDeque<InboundItem>>,
    pub(crate) notify: Notify,
    pub(crate) stopped: Mutex<Option<u32>>,
    pub(crate) bytes_read: AtomicU64,
}

/// The test's handle on one inbound stream it injected with
/// [`crate::transport_shim::LossDelayTransport::push_inbound_uni`].
#[derive(Debug, Clone)]
pub struct InboundUni {
    pub(crate) shared: Arc<InboundShared>,
}

impl InboundUni {
    fn push_item(&self, item: InboundItem) {
        self.shared
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(item);
        self.shared.notify.notify_one();
    }

    /// Deliver one chunk, exactly as split by the caller.
    pub fn push(&self, chunk: Bytes) {
        self.push_item(InboundItem::Chunk(chunk));
    }

    /// Deliver every chunk of `chunks`, in order.
    pub fn push_all(&self, chunks: impl IntoIterator<Item = Bytes>) {
        for chunk in chunks {
            self.push(chunk);
        }
    }

    /// The publisher's FIN.
    pub fn finish(&self) {
        self.push_item(InboundItem::Fin);
    }

    /// The publisher's RESET_STREAM with `code`.
    pub fn reset(&self, code: u64) {
        self.push_item(InboundItem::Reset(code));
    }

    /// The STOP_SENDING code MH sent, if it stopped this stream.
    #[must_use]
    pub fn stopped(&self) -> Option<u32> {
        *self
            .shared
            .stopped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Bytes MH has read from this stream so far.
    #[must_use]
    pub fn bytes_read(&self) -> u64 {
        self.shared.bytes_read.load(Ordering::Acquire)
    }
}

/// How an MH-opened stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressTerminal {
    /// Neither finished nor reset (yet).
    Open,
    /// `finish()` ran: a clean FIN.
    Finished,
    /// RESET_STREAM with `code`. `by_drop` is true when the stream was dropped
    /// without `finish`/`reset` and the seam's drop contract reset it.
    Reset {
        /// The application code.
        code: u32,
        /// Reset by the drop contract rather than an explicit `reset`.
        by_drop: bool,
    },
}

impl EgressTerminal {
    /// Whether the stream was reset, explicitly or by drop.
    #[must_use]
    pub const fn is_reset(self) -> bool {
        matches!(self, Self::Reset { .. })
    }
}

/// The record of one stream MH opened.
#[derive(Debug)]
pub struct EgressUni {
    /// Open-ordinal on this shim, from 0. Shim-local; carries no protocol
    /// meaning (see `FinishedUniStream::open_ordinal`).
    pub open_ordinal: u64,
    pub(crate) terminal: Mutex<EgressTerminal>,
    pub(crate) chunks: Mutex<Vec<Bytes>>,
    pub(crate) priority: AtomicI32,
}

impl EgressUni {
    pub(crate) fn new(open_ordinal: u64) -> Self {
        Self {
            open_ordinal,
            terminal: Mutex::new(EgressTerminal::Open),
            chunks: Mutex::new(Vec::new()),
            priority: AtomicI32::new(0),
        }
    }

    /// The terminal state.
    #[must_use]
    pub fn terminal(&self) -> EgressTerminal {
        *self.terminal.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every byte written, concatenated. A COPY: `write_all` takes `&[u8]`.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.chunks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .flat_map(|chunk| chunk.iter().copied())
            .collect()
    }

    /// The priority last set on the stream (quinn convention: higher first).
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.priority.load(Ordering::Acquire)
    }
}

/// Sentinel for "credit is unlimited".
pub(crate) const UNLIMITED: u64 = u64::MAX;

/// Connection-wide stream state on the shim.
#[derive(Debug)]
pub(crate) struct StreamState {
    // inbound
    pub(crate) inbound_offered: AtomicU64,
    pub(crate) inbound_accepted: AtomicU64,
    pub(crate) inbound_queue: Mutex<VecDeque<Arc<InboundShared>>>,
    pub(crate) inbound_notify: Notify,
    pub(crate) inbound_all: Mutex<Vec<Arc<InboundShared>>>,
    // outbound
    pub(crate) egress: Mutex<Vec<Arc<EgressUni>>>,
    pub(crate) write_credit: AtomicU64,
    pub(crate) open_credit: AtomicU64,
    pub(crate) credit_notify: Notify,
    pub(crate) current_open: AtomicUsize,
    pub(crate) max_open: AtomicUsize,
    // control envelope
    pub(crate) control_inbound: Mutex<VecDeque<Bytes>>,
    pub(crate) control_inbound_notify: Notify,
    pub(crate) control_outbound: Mutex<Vec<Bytes>>,
    /// MH's control-stream writes pend while set (a publisher that is not
    /// reading, or never will).
    pub(crate) control_write_blocked: AtomicBool,
    /// MH's control-stream writes fail while set.
    pub(crate) control_write_failing: AtomicBool,
    pub(crate) control_write_notify: Notify,
    /// Writes MH has STARTED on the control stream (blocked or not).
    pub(crate) control_write_attempts: AtomicU64,
    pub(crate) closed: AtomicBool,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            inbound_offered: AtomicU64::new(0),
            inbound_accepted: AtomicU64::new(0),
            inbound_queue: Mutex::new(VecDeque::new()),
            inbound_notify: Notify::new(),
            inbound_all: Mutex::new(Vec::new()),
            egress: Mutex::new(Vec::new()),
            write_credit: AtomicU64::new(UNLIMITED),
            open_credit: AtomicU64::new(UNLIMITED),
            credit_notify: Notify::new(),
            current_open: AtomicUsize::new(0),
            max_open: AtomicUsize::new(0),
            control_inbound: Mutex::new(VecDeque::new()),
            control_inbound_notify: Notify::new(),
            control_outbound: Mutex::new(Vec::new()),
            control_write_blocked: AtomicBool::new(false),
            control_write_failing: AtomicBool::new(false),
            control_write_notify: Notify::new(),
            control_write_attempts: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        }
    }
}

impl StreamState {
    /// Take up to `wanted` units of `credit`; returns how many were taken.
    pub(crate) fn take_credit(credit: &AtomicU64, wanted: u64) -> u64 {
        let mut taken = 0;
        let _ = credit.try_update(Ordering::AcqRel, Ordering::Acquire, |available| {
            if available == UNLIMITED {
                taken = wanted;
                return Some(UNLIMITED);
            }
            taken = available.min(wanted);
            Some(available - taken)
        });
        taken
    }

    /// Add `n` units to `credit` (saturating below the unlimited sentinel) and
    /// wake every waiter.
    pub(crate) fn grant(&self, credit: &AtomicU64, n: u64) {
        let _ = credit.try_update(Ordering::AcqRel, Ordering::Acquire, |available| {
            if available == UNLIMITED {
                return Some(UNLIMITED);
            }
            Some(available.saturating_add(n).min(UNLIMITED - 1))
        });
        self.credit_notify.notify_waiters();
    }

    pub(crate) fn opened(&self) -> usize {
        let open = self.current_open.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_open.fetch_max(open, Ordering::AcqRel);
        open
    }

    pub(crate) fn closed_one(&self) {
        self.current_open.fetch_sub(1, Ordering::AcqRel);
    }

    pub(crate) fn wake_all(&self) {
        self.inbound_notify.notify_waiters();
        self.credit_notify.notify_waiters();
        self.control_inbound_notify.notify_waiters();
        self.control_write_notify.notify_waiters();
        for inbound in self
            .inbound_all
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            inbound.notify.notify_waiters();
        }
    }
}

/// The drop-contract code, re-exported for assertions.
pub const DROP_RESET_CODE: u32 = GROUP_ABANDONED;
