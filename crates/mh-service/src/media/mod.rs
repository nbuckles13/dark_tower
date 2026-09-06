//! The MH audio datagram forward path (ADR-0036 §2 / §7 / §11).
//!
//! # THIS DIRECTORY IS THE HOT PATH, AND THE DIRECTORY BOUNDARY IS THE POINT
//!
//! ADR-0036 §11 requires the media-path log and metric deny to be scoped to a
//! **directory**, not a file list — "a file list narrows silently on refactor
//! while the guard keeps passing" — and that requires one layout constraint:
//!
//! > lifecycle, setup, and teardown are **siblings** of the media directory,
//! > not children, so the directory boundary and the hot-path boundary are the
//! > same boundary.
//!
//! So: **no denied macro FORM appears anywhere under `media/`** — no `tracing`
//! or `log` import, no `counter!` / `histogram!` / `gauge!` / `describe_*!`, no
//! `println!` / `eprintln!` / `dbg!`, no `event!` or span macro, no
//! `#[instrument]`. Observation goes through handles resolved once at setup
//! (`crate::observability::metrics::resolve_media_handles`) and called as plain
//! methods — `.increment(1)`, `.record(..)`, `.set(..)`. The deny is about macro
//! *forms*; cached-handle calls are the pattern it exists to enforce and must
//! never be caught by it.
//!
//! **"No macro form here" is not the same as "no macro reachable from here",
//! and the difference is one function.** `crate::observability::per_frame_trace::record`
//! is called from [`forward`]; under `--features per-frame-trace` it *is* a
//! `tracing::trace!`. That is deliberate and is why the facility lives in a
//! sibling: `media/` calls a plain function, so the `#[cfg]` decision never
//! enters this directory and the directory-scoped deny still scopes exactly.
//! Outside that feature the call compiles to an `#[inline(always)]` no-op, and
//! enabling the feature in a release build is a `compile_error!`. Stated
//! precisely because "none is reachable from anything here" is the sentence a
//! future reader would cite, and it would be false.
//!
//! The siblings, at their unchanged homes: handle resolution and bucket
//! registration in `crate::observability::metrics`; the `RegisterMeeting` apply
//! in `crate::grpc::mh_service` and `crate::session`; connection lifecycle,
//! task spawn and teardown in `crate::webtransport::connection`; process wiring
//! in `main`. **If setup or teardown drifts in here at review, pull it out —
//! do not add an exemption.**
//!
//! Until `dt-guard` grows the directory-scoped deny (owner: infrastructure; see
//! `docs/TODO.md`), the in-crate enforcement is
//! `crates/mh-service/tests/media_metrics_integration.rs`, which walks these
//! files and the transport-seam adapter and fails on any denied macro form —
//! and fails if its configured directory is missing or empty, because a deny
//! over nothing is the vacuous pass this whole arrangement exists to avoid.
//!
//! # This is a forward path, not an AUDIO forward path
//!
//! ADR-0036 §7 makes MH type-blind: it is told what a stream *does*, never what
//! it *is*. Nothing under `media/` branches on media kind, and the only
//! datagram-specific code is the transport call itself. The structure is built
//! around `EgressEdge`, so the video/uni-stream path is an addition rather than
//! a redesign — it shares the routing read, the relay rewrite, the queues, the
//! caps and every metric.
//!
//! # There is no client-SDK parity here to mirror
//!
//! Recorded because the phrase "mirroring the client SDK's bounded egress
//! queue" has no referent: there is no bounded drop-oldest egress send queue in
//! the client tree and the SDK does not send a datagram at all today. The
//! egress queue exists on ADR-0036 §1's own justification — the
//! application-level bound must trip before the transport ceiling so
//! back-pressure is observable in our code rather than inside quinn — which is
//! already the live premise of `config::EGRESS_QUEUE_FRAMES` and its startup
//! validation. **No cross-language parity test follows from this**, per the
//! standing warning at `crate::transport`.

use crate::observability::metrics::MediaMetricHandles;
use crate::routing::RoutingTable;
use crate::session::LocalSubscribers;
use std::sync::Arc;

pub mod caps;
pub mod forward;
pub mod forwarder;
pub mod ingress;
pub mod queue;
pub mod sampler;

/// What a connection needs to start its media tasks, assembled once at
/// process setup.
///
/// A setup value, deliberately constructed by `main` and threaded through the
/// accept loop rather than resolved per connection: resolving handles per
/// connection would put a registry lookup on the connection path and, worse,
/// would make it plausible to resolve one per *frame*.
#[derive(Clone)]
pub struct MediaSetup {
    /// Metric handles resolved once, at process start.
    pub handles: Arc<MediaMetricHandles>,
    /// The fraction of forwarded frames whose latency is observed. One field,
    /// two readers: the sampler draws against it and the gauge publishes it.
    pub latency_sample_ratio: f64,
}

/// The per-process state every media task reads.
///
/// Assembled by the caller at spawn time (a sibling's job) and read-only from
/// here on. Held behind one `Arc` so a connection's three tasks share one
/// allocation rather than three.
pub struct MediaTaskContext {
    /// The lock-free forwarding policy the data plane reads per frame.
    pub routing: Arc<RoutingTable>,
    /// Which subscribers are connected to this handler.
    pub subscribers: Arc<LocalSubscribers>,
    /// Metric handles resolved once, before any loop started.
    pub handles: Arc<MediaMetricHandles>,
}
