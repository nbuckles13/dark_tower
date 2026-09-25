//! Meeting and participant session tracking for Media Handler.
//!
//! `SessionManagerHandle` is the public API for session state. It communicates
//! with a `SessionManagerActor` running in a dedicated `tokio::spawn` task
//! via typed message passing (ADR-0001 actor pattern).
//!
//! # Actor Pattern (ADR-0001)
//!
//! - `SessionManagerActor` (task): Owns `SessionState` exclusively. Processes
//!   messages sequentially via `mpsc::Receiver<SessionMessage>`. No locks.
//! - `SessionManagerHandle` (handle): Cloneable. Sends messages via
//!   `mpsc::Sender<SessionMessage>`. Uses `oneshot` for request-reply.
//! - `SessionMessage` (enum): One variant per operation.
//!
//! # Two mailboxes, one actor (ADR-0036 §8)
//!
//! The actor serves **two** independent `mpsc` channels and selects over both:
//!
//! - the **connection-lifecycle** mailbox (`SessionMessage`), sized for
//!   one-shot-per-meeting registration and per-connection add/remove;
//! - the **config-apply** mailbox (`ConfigApplyMessage`), carrying full
//!   forwarding-policy snapshots.
//!
//! §8 requires this: *"Config-apply does not route through the
//! connection-lifecycle mailbox. That mailbox is bounded and was sized when
//! registration was one-shot per meeting; a ten-second tick carrying full
//! policy is a different workload and must not starve connection handling."*
//!
//! Two channels rather than two actors, so the actor remains the **sole owner**
//! of all session state — there is no lock, and no ordering hazard between the
//! two workloads beyond the sequencing the single `select!` loop already
//! provides.
//!
//! # Notification
//!
//! Uses `tokio::sync::Notify` per meeting to wake pending connections
//! when `RegisterMeeting` arrives. The actor owns the Notify; callers
//! receive an `Arc<Notify>` clone for awaiting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Notify};
use tokio_util::sync::CancellationToken;

pub mod admission;

use crate::media::forward::EgressQueue;
use crate::observability::metrics::{
    resolve_session_handles, MeetingTeardownOutcome, SessionMetricHandles, StreamAdmissionOutcome,
};
use crate::routing::{MeetingKey, MeetingPolicy, RoutingSnapshot, RoutingTable, SenderId};
pub use admission::StreamAdmission;
use admission::{AdmissionWindow, ThresholdCrossing, ThresholdLatch, ADMISSION_WINDOW_BUCKET};
use arc_swap::ArcSwap;

/// Channel buffer size for the session manager actor mailbox.
///
/// Operations are fast `HashMap` lookups/inserts, so backpressure is unlikely.
/// Lower than MC's `MEETING_CHANNEL_BUFFER` (500) due to lower throughput.
pub(crate) const SESSION_CHANNEL_BUFFER: usize = 256;

/// Channel buffer size for the config-apply mailbox (ADR-0036 §8).
///
/// Deliberately NOT shared with [`SESSION_CHANNEL_BUFFER`], which was
/// calibrated for one-shot-per-meeting registration. This mailbox carries a
/// different workload: one full-policy snapshot per meeting per re-assert
/// cadence tick, where §8 bounds the cadence at <=10 s. At that rate the
/// steady-state depth is one message per meeting per tick against a drain that
/// is a `HashMap` rebuild, so this is sized to absorb a synchronised burst —
/// every meeting re-asserting on one tick, the shape §8's dispatch jitter
/// exists to spread — rather than to buffer sustained overload.
///
/// Bounded on purpose. Unbounded would trade a visible `apply_failed` for
/// invisible memory growth, and the whole point of the §8 acknowledgement is
/// that a policy which did not take effect is *observable*.
pub(crate) const CONFIG_APPLY_CHANNEL_BUFFER: usize = 64;

// ---------------------------------------------------------------------------
// Public data types (unchanged)
// ---------------------------------------------------------------------------

/// Registration data for a meeting on this MH instance.
#[derive(Debug, Clone)]
pub struct MeetingRegistration {
    /// MC instance ID that registered this meeting.
    pub mc_id: String,
    /// MC gRPC endpoint for callbacks (`NotifyParticipant*`).
    pub mc_grpc_endpoint: String,
    /// When the meeting was registered.
    pub registered_at: Instant,
}

/// An active participant connection.
#[derive(Debug, Clone)]
pub struct ConnectionEntry {
    /// Unique connection identifier.
    pub connection_id: String,
    /// Participant ID (from JWT `sub` claim).
    pub participant_id: String,
    /// When the connection was established.
    pub connected_at: Instant,
    /// Cancelling this closes the connection. `EndMeeting` release fires it for
    /// every ACTIVE connection of the released meeting (story 2 R-20); the
    /// connection task owns the other end and tears itself down. A child of the
    /// server shutdown token, so shutdown still reaches it.
    pub close: CancellationToken,
}

/// A pending connection awaiting `RegisterMeeting`.
#[derive(Debug, Clone)]
pub struct PendingConnection {
    /// Unique connection identifier.
    pub connection_id: String,
    /// Meeting ID from the JWT.
    pub meeting_id: String,
    /// Participant ID (from JWT `sub` claim).
    pub participant_id: String,
    /// When the connection was established.
    pub connected_at: Instant,
    /// Carried into the [`ConnectionEntry`] this becomes on promotion, so a
    /// promoted connection is closable by release like any other. Release
    /// itself never fires it while the connection is still PENDING: pending
    /// connections are left to the registration timeout (`internal.proto`,
    /// `EndMeetingRequest`) because one may be a re-join racing the teardown,
    /// and the next registration rescues it.
    pub close: CancellationToken,
}

/// Whether the MC asking about a meeting is the one that registered it.
///
/// The ONE comparison behind both the register-path takeover WARN (detection)
/// and the `EndMeeting` reject (enforcement), so the two cannot drift on
/// trimming, case or empty-string handling.
///
/// BYTE-EXACT `==`, deliberately: no trimming, no case folding, no Unicode
/// normalisation — any of those widens the accepted set. And NOT a
/// constant-time compare: `mc_id` is self-asserted and not a secret
/// (`internal.proto`: "`mc_id` IS SELF-ASSERTED AND IS NOT AN AUTHORIZATION
/// CONTROL"), so a constant-time compare would misrepresent its trust level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// The caller's `mc_id` is the one that registered the meeting.
    SameMc,
    /// A different `mc_id` than the registering one.
    DifferentMc,
}

impl Ownership {
    /// Compare the registered `mc_id` with the caller's.
    #[must_use]
    pub fn of(registered_mc_id: &str, caller_mc_id: &str) -> Self {
        if registered_mc_id == caller_mc_id {
            Self::SameMc
        } else {
            Self::DifferentMc
        }
    }
}

/// Why a registration was refused before any state changed.
///
/// A typed refusal the gRPC boundary maps to a status, never a string threaded
/// out of the actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterRefusal {
    /// A NEW meeting id while this handler already holds
    /// `MH_MAX_REGISTERED_MEETINGS` registrations (story 2 R-21).
    MeetingCapReached {
        /// Registrations held when the refusal was decided.
        held: usize,
        /// The configured cap.
        cap: usize,
    },
}

/// Why an `EndMeeting` could not be decided by the session actor. Neither
/// failure records a teardown outcome: the actor never decided anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMeetingFailure {
    /// The actor is gone (channel closed) — shutdown, or a panicked task.
    ActorUnavailable,
    /// The actor did not reply within the bound.
    Timeout,
}

impl EndMeetingFailure {
    /// A bounded operator-facing reason for the log line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ActorUnavailable => "session_actor_unavailable",
            Self::Timeout => "end_meeting_timeout",
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

/// Messages sent to `SessionManagerActor`.
///
/// Each variant corresponds to one public method on `SessionManagerHandle`.
/// Request-reply variants carry a `oneshot::Sender` for the response.
#[derive(Debug)]
enum SessionMessage {
    /// Register a meeting. Returns any pending connections that were promoted,
    /// or the refusal if a NEW meeting would exceed the registration cap.
    RegisterMeeting {
        meeting_id: String,
        registration: MeetingRegistration,
        max_registered_meetings: usize,
        respond_to: oneshot::Sender<Result<Vec<PendingConnection>, RegisterRefusal>>,
    },
    /// Release a meeting (story 2 R-20). On the LIFECYCLE mailbox, not the
    /// config-apply one: teardown is lifecycle, and sharing a FIFO with
    /// `RegisterMeeting` orders the two.
    EndMeeting {
        meeting_id: String,
        mc_id: String,
        respond_to: oneshot::Sender<MeetingTeardownOutcome>,
    },
    /// Check if a meeting is registered.
    IsMeetingRegistered {
        meeting_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Get the MC gRPC endpoint for a registered meeting.
    GetMcEndpoint {
        meeting_id: String,
        respond_to: oneshot::Sender<Option<String>>,
    },
    /// Add an active connection (fire-and-forget).
    AddConnection {
        meeting_id: String,
        entry: ConnectionEntry,
    },
    /// Remove an active connection by ID. Returns true if found.
    RemoveConnection {
        meeting_id: String,
        connection_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Add a pending connection. Returns an `Arc<Notify>` for awaiting registration.
    AddPendingConnection {
        pending: PendingConnection,
        respond_to: oneshot::Sender<Arc<Notify>>,
    },
    /// Remove a pending connection by ID. Returns true if found.
    RemovePendingConnection {
        meeting_id: String,
        connection_id: String,
        respond_to: oneshot::Sender<bool>,
    },
    /// Get the total count of active connections across all meetings.
    ActiveConnectionCount { respond_to: oneshot::Sender<usize> },
}

/// Messages sent to the actor's **config-apply** mailbox (ADR-0036 §8).
///
/// Separate from [`SessionMessage`] by contract, not by convenience: policy
/// application must not contend with connection handling for mailbox capacity.
#[derive(Debug)]
enum ConfigApplyMessage {
    /// Install a validated forwarding policy, if its generation advances.
    ApplyPolicy {
        policy: Box<MeetingPolicy>,
        /// Aggregate egress-edge bound across all meetings on this handler.
        max_total_egress_edges: usize,
        respond_to: oneshot::Sender<ApplyOutcome>,
    },
}

/// Why a policy apply did not install.
///
/// Bounded `&'static str` reasons, never a metric label: `apply_failed` has
/// more than one cause with materially different remedies, and an operator
/// needs to tell "the actor is wedged or overloaded" from "policy volume hit
/// its bound" — but a generation value or a meeting id must never become a
/// series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyFailure {
    /// The config-apply mailbox was full. The handler never blocks on it.
    MailboxFull,
    /// The actor did not reply within the configured bound.
    Timeout,
    /// The actor is gone (channel closed) — shutdown, or a panicked task.
    ActorUnavailable,
    /// Installing would exceed the handler's aggregate egress-edge bound.
    EdgeCapExceeded,
    /// Installing would take the handler's installed egress streams above its
    /// derived egress STREAM ceiling — a capacity refusal (story 2 R-19).
    StreamCeilingExceeded,
}

impl ApplyFailure {
    /// A bounded operator-facing reason for the log line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::MailboxFull => "config_apply_mailbox_full",
            Self::Timeout => "config_apply_timeout",
            Self::ActorUnavailable => "session_actor_unavailable",
            Self::EdgeCapExceeded => "total_egress_edge_cap_exceeded",
            Self::StreamCeilingExceeded => "egress_stream_ceiling_exceeded",
        }
    }
}

/// What the actor did with a policy.
///
/// Deliberately does **not** carry the applied generation. That value has a
/// single source — a read of the live routing snapshot — so that echoing the
/// *received* generation is not a line anyone can write. See
/// [`SessionManagerHandle::routing_snapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// The live forward path reflects this generation. Covers both a fresh
    /// install and an idempotent re-assert of an already-installed generation
    /// (which performs no swap).
    Applied,
    /// The generation was lower than the one installed, so it was ignored.
    /// Reordered or retried delivery must not roll policy back.
    RejectedStale,
    /// Nothing was installed; the prior generation stays live.
    Failed(ApplyFailure),
}

// ---------------------------------------------------------------------------
// Actor (task)
// ---------------------------------------------------------------------------

/// Internal state owned exclusively by the actor. No `Arc`, no `RwLock`.
#[derive(Debug, Default)]
struct SessionState {
    /// Registered meetings: `meeting_id` -> registration data.
    registered_meetings: HashMap<String, MeetingRegistration>,
    /// Active connections: `meeting_id` -> (`participant_id` -> connections).
    active_connections: HashMap<String, HashMap<String, Vec<ConnectionEntry>>>,
    /// Pending connections awaiting `RegisterMeeting`: `meeting_id` -> pending list.
    pending_connections: HashMap<String, Vec<PendingConnection>>,
    /// Notify handles for pending connections: `meeting_id` -> `Notify`.
    meeting_notifiers: HashMap<String, Arc<Notify>>,
}

/// Actor that owns session state and processes messages sequentially.
///
/// Spawned by [`SessionManagerHandle::new`]. Runs until all senders are dropped
/// (channel closed), which happens naturally during shutdown.
pub struct SessionManagerActor {
    receiver: mpsc::Receiver<SessionMessage>,
    config_receiver: mpsc::Receiver<ConfigApplyMessage>,
    state: SessionState,
    routing: Arc<RoutingTable>,
    /// The derived stream ceiling and the ratio threshold (story 2 R-19).
    admission: StreamAdmission,
    /// Sliding window the rejection ratio is computed over.
    admission_window: AdmissionWindow,
    /// Edge-triggered reader of the rejection-ratio threshold (log only).
    threshold_latch: ThresholdLatch,
    /// Admission, occupancy and teardown handles, resolved once at
    /// construction.
    metrics: SessionMetricHandles,
}

impl SessionManagerActor {
    fn new(
        receiver: mpsc::Receiver<SessionMessage>,
        config_receiver: mpsc::Receiver<ConfigApplyMessage>,
        routing: Arc<RoutingTable>,
        admission: StreamAdmission,
    ) -> Self {
        let metrics = resolve_session_handles();
        // Both gauges exist from construction — never lazily on first
        // decision/install. A gauge-to-gauge alert whose left-hand series is
        // absent cannot fire, and `mh-media.json`'s "an empty panel means
        // healthy, because MH registers eagerly" rests on this.
        metrics.publish_rejection_ratio(0.0);
        metrics.publish_egress_edges(routing.load().total_edges());
        // The state is empty at construction; published anyway so the series
        // is present-at-0 from boot rather than appearing on first register.
        metrics.publish_registered_meetings(0);
        Self {
            receiver,
            config_receiver,
            state: SessionState::default(),
            routing,
            admission,
            admission_window: AdmissionWindow::new(),
            threshold_latch: ThresholdLatch::default(),
            metrics,
        }
    }

    /// Main run loop. Selects over both mailboxes, plus the admission-window
    /// rotation tick, until both mailboxes close.
    ///
    /// The tick arm is always ready eventually, so the old `else => break`
    /// (which fires only when EVERY arm is disabled) would never run and the
    /// actor would outlive shutdown. Explicit open-flags instead: a mailbox arm
    /// is disabled once its channel returns `None`, and the loop condition —
    /// not the tick — decides termination, so the loop still ends exactly when
    /// both channels are closed.
    pub async fn run(mut self) {
        let mut lifecycle_open = true;
        let mut config_open = true;
        // First tick one bucket from now (an `interval` fires immediately).
        let mut rotation = tokio::time::interval_at(
            tokio::time::Instant::now() + ADMISSION_WINDOW_BUCKET,
            ADMISSION_WINDOW_BUCKET,
        );
        // DEFAULT `Burst` missed-tick behaviour, deliberately — do not set
        // `Skip`. Under `Skip`, ticks missed while the actor is busy are
        // discarded, so a 60 s stall rotates ONCE instead of six times and data
        // that should have aged out stays in the window: the effective window
        // stretches past its documented five minutes exactly when the actor is
        // under load, which is when the ratio matters. `Burst` catches up, and
        // catching up is cheap (`rotate` clears one small struct; the latch is
        // edge-triggered, so a burst logs at most one line).
        while lifecycle_open || config_open {
            tokio::select! {
                msg = self.receiver.recv(), if lifecycle_open => match msg {
                    Some(msg) => self.handle_message(msg),
                    None => lifecycle_open = false,
                },
                msg = self.config_receiver.recv(), if config_open => match msg {
                    Some(msg) => self.handle_config_apply(msg),
                    None => config_open = false,
                },
                _ = rotation.tick() => {
                    self.admission_window.rotate();
                    self.publish_admission_ratio();
                }
            }
        }
        tracing::debug!(
            target: "mh.session",
            "SessionManagerActor stopped (both mailboxes closed)"
        );
    }

    /// Apply a validated forwarding policy (ADR-0036 §8).
    ///
    /// # Generation semantics
    ///
    /// ```text
    /// generation <  installed -> RejectedStale ; NO swap
    /// generation == installed -> Applied       ; NO swap (idempotent re-assert)
    /// generation >  installed -> Applied       ; exactly one swap
    ///                            (or Failed, prior generation stays live)
    /// ```
    ///
    /// Generation `0` never reaches here: it is short-circuited at the handler,
    /// which owns the ordering constraint that keeps its rejection out of this
    /// story.
    ///
    /// **The equal case performs no swap.** §8 derives the generation from the
    /// assignment computation's *output change*, so unchanged policy carries the
    /// same number and MH no-ops. Without that, an identical re-assert every
    /// cadence tick would churn the snapshot — periodic media glitches wearing a
    /// configuration disguise.
    fn handle_config_apply(&mut self, msg: ConfigApplyMessage) {
        let ConfigApplyMessage::ApplyPolicy {
            policy,
            max_total_egress_edges,
            respond_to,
        } = msg;

        let snapshot = self.routing.load();
        let installed = snapshot.generation_for(&policy.meeting);
        let projected_total_edges = snapshot.projected_total_edges(&policy);

        let outcome = if policy.generation < installed {
            tracing::warn!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                installed_generation = installed,
                received_generation = policy.generation,
                "Ignoring stale policy generation; live forward path is unchanged"
            );
            ApplyOutcome::RejectedStale
        } else if policy.generation == installed {
            // Contract-violation detector. §8 guarantees an unchanged
            // generation means unchanged policy, but nothing on this side can
            // enforce that on MC, so a divergence is worth a loud line. The
            // no-op stands either way: honouring the generation is what makes
            // the steady state silent, and applying here would churn the data
            // plane at exactly the cadence interval.
            //
            // Counts only. Logging WHICH edges differ would put sender ids,
            // slot ids and egress stream ids in the log — the per-stream
            // identity ADR-0036 §11 bars, and the back door that opened when
            // StreamTelemetry was deleted.
            if let Some(current) = snapshot.routes_for(&policy.meeting) {
                if current.edges() != policy.edges.as_slice()
                    || current.transport_mode() != policy.transport_mode
                {
                    tracing::warn!(
                        target: "mh.session.policy",
                        key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                        generation = policy.generation,
                        installed_edge_count = current.edge_count(),
                        received_edge_count = policy.edges.len(),
                        "Re-assert at an unchanged policy_generation carries DIFFERENT policy; \
                         honouring the generation and not swapping (ADR-0036 §8). MC's \
                         generation must derive from assignment-output change"
                    );
                }
            }
            ApplyOutcome::Applied
        } else if projected_total_edges > self.admission.stream_ceiling {
            // EGRESS STREAM ADMISSION (story 2 R-19; ADR-0036 §11) — the HARD
            // backstop behind GC's soft placement filter. A capacity check,
            // separate from the resource guard below and first in order.
            //
            // Unit: `projected_total_edges` counts EGRESS STREAMS — one
            // `EgressEdge` per `egress_stream_id`, its `candidates` being the
            // sources eligible to fill that one stream — summed over every
            // meeting on this handler, via the same `projected_total_edges`
            // the edge bound and `with_policy` use (no second accounting). The
            // same unit as `max_streams` advertised to GC and as
            // `mh_media_egress_edges`.
            //
            // All-or-none: no swap, prior generation stays live. Reached only
            // by a generation-ADVANCING apply: stale and equal-generation
            // re-asserts short-circuit above, so an idempotent re-assert on a
            // full handler can never trip a capacity refusal.
            //
            // The WARN names the CONDITION and points at the recovery; it does not
            // carry the restart command. Deployment names and the namespace are
            // infra topology (CLAUDE.md: keep it out of application code), and a
            // restated command goes stale silently — it would name two handlers
            // the day a third exists, so the recovery half-works and the ratchet
            // persists on the unnamed pod. `docs/TODO.md` records the same
            // "point, don't restate" ruling from a prior cert-rotation incident.
            //
            // RATCHET: MH releases an ended meeting's streams when its MC calls
            // `EndMeeting` (story 2 R-20). MC begins calling it in story 2 task
            // 12; before that, and ALWAYS for a meeting whose MC dies without
            // calling it, the streams stay held until the pod restarts.
            // Ordinary departures re-push shrinking policies and do release.
            // `installed_meeting_count` / `installed_total_streams` answer "one
            // fat policy, or accumulated dead meetings?" from this line alone;
            // both are identity-free aggregates.
            self.record_admission(StreamAdmissionOutcome::RejectedStreamCeiling);
            tracing::warn!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                reason = ApplyFailure::StreamCeilingExceeded.reason(),
                installed_generation = installed,
                received_generation = policy.generation,
                projected_total_streams = projected_total_edges,
                installed_total_streams = snapshot.total_edges(),
                installed_meeting_count = snapshot.meeting_count(),
                stream_ceiling = self.admission.stream_ceiling,
                "Egress stream admission refused: projected streams exceed this handler's \
                 stream ceiling; prior generation stays live. If installed streams sit at the \
                 ceiling with no live meeting behind them, that is the ratchet (streams held by \
                 meetings that ended without an EndMeeting): see 'The ratchet' in \
                 docs/observability/metrics/mh-service.md"
            );
            ApplyOutcome::Failed(ApplyFailure::StreamCeilingExceeded)
        } else if projected_total_edges > max_total_egress_edges {
            // DEFENSE-IN-DEPTH BACKSTOP, unreachable while config load's
            // invariant holds: `from_vars` refuses a stream ceiling above
            // `MH_MAX_TOTAL_EGRESS_EDGES`
            // (`ConfigError::EgressStreamCeilingExceedsEdgeBound`), so any
            // projection over this bound is already over the ceiling and was
            // refused above. Kept honest by that boot test AND by the
            // deliberately-labelled backstop tests that build an actor whose
            // ceiling sits above the edge bound (`mh_service` tests,
            // `apply_failure_does_not_advance_the_acknowledged_generation` and
            // `reassert_at_the_aggregate_bound_does_not_double_count_itself`).
            // The admission decision was "admitted" — the resource guard, not
            // capacity, refuses. That counting choice is itself held up by the
            // boot invariant: were `EgressStreamCeilingExceedsEdgeBound` ever
            // relaxed, this arm would become reachable and every refusal here
            // would inflate the ratio's DENOMINATOR, suppressing the rejection
            // signal exactly while the handler refuses. Revisit this line if
            // that invariant is ever loosened.
            self.record_admission(StreamAdmissionOutcome::Admitted);
            // Aggregate resource-exhaustion guard, NOT a capacity figure and
            // never advertised to GC. All-or-none: no swap, prior generation
            // stays live, so the forward path reflects N-1 rather than neither.
            //
            // `installed_meeting_count` and `installed_total_edges` are here
            // because this bound can RATCHET: a meeting's edges are released by
            // it re-asserting a smaller policy or by its MC calling `EndMeeting`
            // (story 2 R-20), and a meeting whose MC never calls it (MC crash,
            // or any MC before story 2 task 12) keeps them until the pod
            // restarts. The one question an operator has on seeing this line is
            // "is this one fat policy, or accumulated dead meetings?", and only
            // these two counts answer it. Both are bounded aggregates carrying
            // no identity.
            tracing::warn!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                reason = ApplyFailure::EdgeCapExceeded.reason(),
                installed_generation = installed,
                received_generation = policy.generation,
                projected_total_edges,
                installed_total_edges = snapshot.total_edges(),
                installed_meeting_count = snapshot.meeting_count(),
                max_total_egress_edges,
                "Policy apply refused; prior generation stays live"
            );
            ApplyOutcome::Failed(ApplyFailure::EdgeCapExceeded)
        } else {
            self.record_admission(StreamAdmissionOutcome::Admitted);
            self.routing.install(&policy);
            self.metrics
                .publish_egress_edges(self.routing.load().total_edges());
            tracing::info!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                previous_generation = installed,
                applied_generation = policy.generation,
                edge_count = policy.edges.len(),
                "Forwarding policy applied"
            );
            ApplyOutcome::Applied
        };

        let _ = respond_to.send(outcome);
    }

    /// Count one admission decision, update the window, republish the ratio.
    fn record_admission(&mut self, outcome: StreamAdmissionOutcome) {
        self.metrics.record(outcome);
        self.admission_window.record(outcome);
        self.publish_admission_ratio();
    }

    /// Publish the windowed rejection ratio and feed the threshold latch.
    ///
    /// The log line is a debugging BREADCRUMB; the Prometheus alert on the
    /// ratio and threshold gauges is authoritative for paging, and the two may
    /// disagree at the edges (the alert has its own `for:` and denominator
    /// guard).
    fn publish_admission_ratio(&mut self) {
        let ratio = self.admission_window.ratio();
        let decisions = self.admission_window.decisions();
        self.metrics.publish_rejection_ratio(ratio);
        let threshold = self.admission.rejection_ratio_threshold;
        let installed_total_streams = self.routing.load().total_edges();
        match self.threshold_latch.observe(ratio, decisions, threshold) {
            Some(ThresholdCrossing::Exceeded) => tracing::warn!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                rejection_ratio = ratio,
                rejection_ratio_threshold = threshold,
                window_decisions = decisions,
                installed_total_streams,
                stream_ceiling = self.admission.stream_ceiling,
                "Egress stream admission rejection ratio above threshold (debugging \
                 breadcrumb; the alert is authoritative)"
            ),
            // `installed_total_streams` and `stream_ceiling` are on this line so
            // it cannot be misread as an all-clear. The latch also re-arms when
            // the window EMPTIES — MC stopped offering, not the handler
            // recovering — so "recovered, window_decisions=0,
            // installed_total_streams == stream_ceiling" is a still-full (idle
            // or ratcheted) handler, and says so from this one line.
            Some(ThresholdCrossing::Recovered) => tracing::info!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                rejection_ratio = ratio,
                rejection_ratio_threshold = threshold,
                window_decisions = decisions,
                installed_total_streams,
                stream_ceiling = self.admission.stream_ceiling,
                "Egress stream admission rejection ratio back below threshold (if \
                 window_decisions is 0 this only means nothing was offered; compare \
                 installed_total_streams with stream_ceiling)"
            ),
            None => {}
        }
    }

    fn handle_message(&mut self, msg: SessionMessage) {
        match msg {
            SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                max_registered_meetings,
                respond_to,
            } => {
                // Every gauge publish happens INSIDE the handler, i.e. before
                // this send. A caller reading the gauge after the reply must see
                // the moved value; do not move a publish after the send.
                let result =
                    self.handle_register_meeting(meeting_id, registration, max_registered_meetings);
                let _ = respond_to.send(result);
            }
            SessionMessage::EndMeeting {
                meeting_id,
                mc_id,
                respond_to,
            } => {
                // Same ordering rule: the release republishes the gauges before
                // this reply is sent.
                let outcome = self.handle_end_meeting(&meeting_id, &mc_id);
                let _ = respond_to.send(outcome);
            }
            SessionMessage::IsMeetingRegistered {
                meeting_id,
                respond_to,
            } => {
                let result = self.state.registered_meetings.contains_key(&meeting_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::GetMcEndpoint {
                meeting_id,
                respond_to,
            } => {
                let result = self
                    .state
                    .registered_meetings
                    .get(&meeting_id)
                    .map(|r| r.mc_grpc_endpoint.clone());
                let _ = respond_to.send(result);
            }
            SessionMessage::AddConnection { meeting_id, entry } => {
                self.handle_add_connection(meeting_id, entry);
            }
            SessionMessage::RemoveConnection {
                meeting_id,
                connection_id,
                respond_to,
            } => {
                let result = self.handle_remove_connection(&meeting_id, &connection_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::AddPendingConnection {
                pending,
                respond_to,
            } => {
                let result = self.handle_add_pending_connection(pending);
                let _ = respond_to.send(result);
            }
            SessionMessage::RemovePendingConnection {
                meeting_id,
                connection_id,
                respond_to,
            } => {
                let result = self.handle_remove_pending_connection(&meeting_id, &connection_id);
                let _ = respond_to.send(result);
            }
            SessionMessage::ActiveConnectionCount { respond_to } => {
                let count = self
                    .state
                    .active_connections
                    .values()
                    .flat_map(|m| m.values())
                    .map(Vec::len)
                    .sum();
                let _ = respond_to.send(count);
            }
        }
    }

    /// Register a meeting and promote any pending connections.
    ///
    /// This runs sequentially inside the actor, eliminating the TOCTOU race
    /// that existed with `Arc<RwLock<SessionState>>` — and it is why the
    /// registration cap is checked HERE, atomically with the insert, rather
    /// than by a caller that read the count first.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "meeting_id is used as owned key in insert/entry calls; &str would require .to_string() at each site"
    )]
    fn handle_register_meeting(
        &mut self,
        meeting_id: String,
        registration: MeetingRegistration,
        max_registered_meetings: usize,
    ) -> Result<Vec<PendingConnection>, RegisterRefusal> {
        // An ordinary re-assert and a change of OWNING MC are the same insert,
        // and before this they were also the same INFO line. They are not the
        // same event, and the generation ratchet is why: `handle_config_apply`
        // refuses anything below the installed generation, so a registration
        // from a different MC carrying a high `policy_generation` wedges every
        // subsequent re-assert from the owning MC onto `rejected_stale` —
        // while `accepted: true` and `mh_grpc_requests_total{method="register_meeting",status="success"}`
        // both stay green. (`EndMeeting` is now the in-process recovery; see the
        // runbook's ownership-reject arm.)
        //
        // MH cannot decide here whether that is an attack or a legitimate MC
        // failover: `MhAuthLayer` authorizes on scope and `service_type` only
        // and binds no caller to a `meeting_id`, so both look identical on the
        // wire. So registration stays an OVERWRITE — failover semantics are
        // untouched: a legitimate new owner re-registers first, taking over
        // `mc_id`, and may then release. What lands here is the DETECTION half,
        // through the same [`Ownership`] comparison `EndMeeting` enforces on.
        //
        // `mc_id` is safe as a field — a bounded service identity the handler
        // already length-caps and already logs — unlike the per-stream
        // identities ADR-0036 §11 bars. The field names match the
        // `EndMeeting` reject's so one search covers both events.
        match self.state.registered_meetings.get(&meeting_id) {
            Some(existing)
                if Ownership::of(&existing.mc_id, &registration.mc_id)
                    == Ownership::DifferentMc =>
            {
                tracing::warn!(
                    target: "mh.session",
                    key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                    meeting_id = %meeting_id,
                    registered_mc_id = %existing.mc_id,
                    caller_mc_id = %registration.mc_id,
                    "Meeting registration taken over by a DIFFERENT MC; MH does not \
                     verify meeting ownership, and the installed policy generation \
                     never moves back down"
                );
            }
            Some(_) => {
                tracing::info!(
                    target: "mh.session",
                    meeting_id = %meeting_id,
                    "Overwriting existing meeting registration"
                );
            }
            // A NEW meeting id: the only case the registration cap applies to.
            // A re-registration of a held meeting is never refused — refusing
            // it would stop a full handler accepting policy for meetings it is
            // already serving, a blackhole caused by the resource guard itself.
            None if self.state.registered_meetings.len() >= max_registered_meetings => {
                let held = self.state.registered_meetings.len();
                tracing::warn!(
                    target: "mh.session",
                    key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                    registered_meetings = held,
                    max_registered_meetings,
                    "Refusing a NEW meeting registration: this handler holds \
                     MH_MAX_REGISTERED_MEETINGS registrations (a resource guard, not \
                     capacity). Held registrations are released only by EndMeeting; \
                     see the meeting-cap arm in docs/runbooks/mh-incident-response.md"
                );
                return Err(RegisterRefusal::MeetingCapReached {
                    held,
                    cap: max_registered_meetings,
                });
            }
            None => {}
        }

        self.state
            .registered_meetings
            .insert(meeting_id.clone(), registration);
        self.metrics
            .publish_registered_meetings(self.state.registered_meetings.len());

        // Drain pending connections for this meeting
        let pending = self
            .state
            .pending_connections
            .remove(&meeting_id)
            .unwrap_or_default();

        // Promote pending connections to active, carrying each one's close
        // token so a later release can close it.
        for conn in &pending {
            let meeting_conns = self
                .state
                .active_connections
                .entry(meeting_id.clone())
                .or_default();
            let participant_conns = meeting_conns
                .entry(conn.participant_id.clone())
                .or_default();
            participant_conns.push(ConnectionEntry {
                connection_id: conn.connection_id.clone(),
                participant_id: conn.participant_id.clone(),
                connected_at: conn.connected_at,
                close: conn.close.clone(),
            });
        }

        // Notify any waiters that the meeting is now registered, then drop the
        // notifier: its only purpose is waking PENDING connections and this
        // meeting now has none. Waiters hold their own `Arc` clone, so dropping
        // the map's copy strands nobody, and a later pending connection creates
        // a fresh one. Without this the map is insert-only — one entry per
        // meeting that ever had a pending connection, forever.
        if let Some(notifier) = self.state.meeting_notifiers.remove(&meeting_id) {
            notifier.notify_waiters();
        }

        Ok(pending)
    }

    /// Release a meeting (story 2 R-20; semantics normative on
    /// `EndMeetingRequest` in `proto/dark_tower/internal/v1/internal.proto`).
    ///
    /// Unconditional BY MEETING ID, with no generation, by contract.
    ///
    /// STORY-2 ASSUMPTION this rests on: MC stops every `RegisterMeeting` push
    /// for the meeting before calling `EndMeeting` (the quiesce rule stated on
    /// `EndMeetingRequest`, which is the source of truth — not restated here).
    /// MH does not enforce it. The accepted residual: an attempt whose
    /// client-side deadline expired can still be applied here AFTER this
    /// release and re-create the meeting, bounded by the RPC deadline.
    ///
    /// STORY-4 NOTE: once the periodic §8 re-assert exists, a re-assert racing
    /// this release RESURRECTS the ended meeting's routes and edge budget on a
    /// timer. That needs a fence (a teardown tombstone here, or MC-side
    /// cancellation that happens-before the call) — `docs/TODO.md`
    /// "`EndMeeting` has no fence against a late `RegisterMeeting`".
    fn handle_end_meeting(&mut self, meeting_id: &str, mc_id: &str) -> MeetingTeardownOutcome {
        let Some(registered) = self.state.registered_meetings.get(meeting_id) else {
            // Nothing held. Routes cannot exist without a registration — every
            // apply follows its registration's upsert, and release removes both.
            debug_assert!(
                self.routing
                    .load()
                    .routes_for(&MeetingKey::new(meeting_id))
                    .is_none(),
                "routes installed for a meeting with no registration"
            );
            tracing::info!(
                target: "mh.session",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                meeting_id = %meeting_id,
                caller_mc_id = %mc_id,
                "EndMeeting for a meeting this handler does not hold; acknowledged as a no-op"
            );
            self.metrics
                .record_teardown(MeetingTeardownOutcome::UnknownMeeting);
            return MeetingTeardownOutcome::UnknownMeeting;
        };

        if Ownership::of(&registered.mc_id, mc_id) == Ownership::DifferentMc {
            // Both ids in the SERVER-SIDE log only; the caller's status carries
            // neither (a scoped-but-wrong caller must not learn the owner's id,
            // which is exactly what a matching forged release would need).
            tracing::warn!(
                target: "mh.session",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                meeting_id = %meeting_id,
                registered_mc_id = %registered.mc_id,
                caller_mc_id = %mc_id,
                "Refusing EndMeeting from an MC that did not register this meeting; \
                 nothing released. A legitimate new owner re-registers first"
            );
            self.metrics
                .record_teardown(MeetingTeardownOutcome::RejectedOwnership);
            return MeetingTeardownOutcome::RejectedOwnership;
        }

        self.state.registered_meetings.remove(meeting_id);
        let meeting = MeetingKey::new(meeting_id);
        // Removing the routes entry is also what FORGETS the applied
        // generation, so a same-id re-create installs fresh from generation 1.
        let routes_removed = self.routing.remove(&meeting);

        // Close every ACTIVE connection. Pending ones are left alone (see
        // `PendingConnection::close`), and so is this meeting's notifier while
        // any pending connection still waits on it.
        let mut closed_connections = 0_usize;
        if let Some(meeting_conns) = self.state.active_connections.remove(meeting_id) {
            for entry in meeting_conns.values().flatten() {
                entry.close.cancel();
                closed_connections += 1;
            }
        }
        if !self.state.pending_connections.contains_key(meeting_id) {
            self.state.meeting_notifiers.remove(meeting_id);
        }

        let snapshot = self.routing.load();
        self.metrics.publish_egress_edges(snapshot.total_edges());
        self.metrics
            .publish_registered_meetings(self.state.registered_meetings.len());
        self.metrics
            .record_teardown(MeetingTeardownOutcome::Released);

        tracing::info!(
            target: "mh.session",
            key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
            meeting_id = %meeting_id,
            mc_id = %mc_id,
            routes_removed,
            closed_connections,
            installed_total_streams = snapshot.total_edges(),
            registered_meetings = self.state.registered_meetings.len(),
            "Meeting released by EndMeeting"
        );
        MeetingTeardownOutcome::Released
    }

    fn handle_add_connection(&mut self, meeting_id: String, entry: ConnectionEntry) {
        // The connection task checked registration, THEN sent this — two
        // messages, so an `EndMeeting` can land between them. Inserting anyway
        // would leave an entry for a released meeting that nothing will ever
        // close. Close it instead; the connection tears itself down.
        if !self.state.registered_meetings.contains_key(&meeting_id) {
            tracing::info!(
                target: "mh.session",
                meeting_id = %meeting_id,
                connection_id = %entry.connection_id,
                "Connection arrived for a meeting released since it checked; closing it"
            );
            entry.close.cancel();
            return;
        }
        let meeting_conns = self.state.active_connections.entry(meeting_id).or_default();
        let participant_conns = meeting_conns
            .entry(entry.participant_id.clone())
            .or_default();
        participant_conns.push(entry);
    }

    fn handle_remove_connection(&mut self, meeting_id: &str, connection_id: &str) -> bool {
        let Some(meeting_conns) = self.state.active_connections.get_mut(meeting_id) else {
            return false;
        };
        let mut emptied_participant = None;
        let mut found = false;
        for (participant_id, participant_conns) in meeting_conns.iter_mut() {
            if let Some(pos) = participant_conns
                .iter()
                .position(|c| c.connection_id == connection_id)
            {
                participant_conns.remove(pos);
                if participant_conns.is_empty() {
                    emptied_participant = Some(participant_id.clone());
                }
                found = true;
                break;
            }
        }
        // Drop entries this removal emptied, so the map holds only live
        // connections rather than one husk per participant and meeting ever
        // seen.
        if let Some(participant_id) = emptied_participant {
            meeting_conns.remove(&participant_id);
        }
        if meeting_conns.is_empty() {
            self.state.active_connections.remove(meeting_id);
        }
        found
    }

    fn handle_add_pending_connection(&mut self, pending: PendingConnection) -> Arc<Notify> {
        let meeting_id = pending.meeting_id.clone();

        self.state
            .pending_connections
            .entry(meeting_id.clone())
            .or_default()
            .push(pending);

        // Get or create notifier for this meeting
        self.state
            .meeting_notifiers
            .entry(meeting_id)
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    fn handle_remove_pending_connection(&mut self, meeting_id: &str, connection_id: &str) -> bool {
        if let Some(pending_list) = self.state.pending_connections.get_mut(meeting_id) {
            if let Some(pos) = pending_list
                .iter()
                .position(|c| c.connection_id == connection_id)
            {
                pending_list.remove(pos);
                // Clean up empty entries — and the notifier with the last
                // pending connection, since waking pending connections is its
                // only purpose. The waiter holds its own `Arc`; a later pending
                // connection creates a fresh notifier.
                if pending_list.is_empty() {
                    self.state.pending_connections.remove(meeting_id);
                    self.state.meeting_notifiers.remove(meeting_id);
                }
                return true;
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Local subscriber registry (ADR-0036 §2/§7 forward path)
// ---------------------------------------------------------------------------

/// A subscriber, addressable only within one meeting.
///
/// **Unconstructible without a [`MeetingKey`]**, for the same reason
/// [`crate::routing::RoutingSnapshot::for_each_source`] takes one: a
/// `sender_id` is a 16-bit **per-meeting ordinal**, so `sender_id` 5 exists
/// concurrently in every meeting on this handler and a flat
/// `HashMap<SenderId, _>` would be a cross-meeting media-crossing primitive.
/// The wrong index is a missing type here, not a remembered rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubscriberKey {
    meeting: MeetingKey,
    sender: SenderId,
}

impl SubscriberKey {
    /// The only constructor: naming a meeting is not optional.
    #[must_use]
    pub fn new(meeting: MeetingKey, sender: SenderId) -> Self {
        Self { meeting, sender }
    }
}

/// The set of subscriber connections live on this handler, as one snapshot.
///
/// Read by the forward path on every frame; replaced wholesale on connect and
/// disconnect. Same shape and same reasoning as
/// [`crate::routing::RoutingSnapshot`]: connect/disconnect is rare, per-frame
/// reads are not, and a lock on this path would let one connection's teardown
/// stall every other connection's audio.
#[derive(Debug, Default)]
pub struct LocalSubscriberSnapshot {
    connections: HashMap<SubscriberKey, SubscriberEntry>,
}

/// One connected subscriber's egress queue, tagged with the connection that owns
/// it.
///
/// The `connection_id` is what makes teardown a **compare-and-remove**
/// ([`LocalSubscribers::unregister`]), symmetric to [`SenderBindings`]. A
/// connection superseded by a same-participant reconnect holds the same
/// `(meeting, sender)` ordinal as its successor; without the tag, the superseded
/// connection's teardown would clear the LIVE successor's egress queue. The
/// successor then stays connected but vanishes from the snapshot, so every frame
/// addressed to it counts `no_local_subscriber` — a permanent silent-audio
/// failure under a label the catalog documents as ordinary. This is the same
/// failure `SenderBindings`' compare-and-remove prevents one registry over
/// (@security SEC-2 / the media-handler reviewer's independent finding).
#[derive(Debug, Clone)]
struct SubscriberEntry {
    connection_id: String,
    queue: Arc<EgressQueue>,
}

impl LocalSubscriberSnapshot {
    /// The egress queue for one meeting-scoped subscriber, if it is connected
    /// **to this handler**.
    ///
    /// `None` is a legitimate, counted outcome (`no_local_subscriber`): under
    /// ADR-0036 §9 a meeting's participants can be spread across handlers, so
    /// an edge naming a subscriber this handler does not hold is ordinary, not
    /// an error.
    #[must_use]
    pub fn egress_queue(
        &self,
        meeting: &MeetingKey,
        sender: SenderId,
    ) -> Option<&Arc<EgressQueue>> {
        // Cloning the key to probe costs one `Arc<str>` refcount bump and no
        // allocation; `HashMap::get` needs an owned-shaped key here because
        // `SubscriberKey` is a composite and `Borrow` cannot decompose it.
        self.connections
            .get(&SubscriberKey::new(meeting.clone(), sender))
            .map(|entry| &entry.queue)
    }

    /// How many subscriber connections this handler holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Whether this handler holds no subscriber connections.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

/// Lock-free publication of [`LocalSubscriberSnapshot`] to the forward path.
#[derive(Debug)]
pub struct LocalSubscribers {
    snapshot: ArcSwap<LocalSubscriberSnapshot>,
}

impl Default for LocalSubscribers {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalSubscribers {
    /// A registry with no connections.
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(LocalSubscriberSnapshot::default()),
        }
    }

    /// The current snapshot. Lock-free; safe to call per frame.
    #[must_use]
    pub fn load(&self) -> Arc<LocalSubscriberSnapshot> {
        self.snapshot.load_full()
    }

    /// Register one connection's egress queue. Setup, not hot path.
    ///
    /// # `rcu`, not load-clone-store — the writers here RACE
    ///
    /// [`crate::routing::RoutingTable::install`] has the same outward shape and
    /// is safe with a plain load-modify-store, because its only writer is the
    /// session-manager actor and its writes are serialized by the actor loop.
    /// **These writers are different**: `register` and `unregister` are called
    /// from `handle_connection`, one task per connection, all racing on the same
    /// runtime. A plain read-modify-write loses an update when two participants
    /// connect concurrently.
    ///
    /// The lost update is a **silent availability failure wearing a
    /// normal-looking label**: a dropped `register` leaves a participant
    /// connected but absent from the snapshot, so every frame addressed to them
    /// is counted `no_local_subscriber` — which the catalog documents as
    /// ordinary, because a subscriber held by another handler produces it
    /// legitimately. The participant hears nothing, permanently, and the only
    /// signal says "this is normal". A dropped `unregister` leaves a departed
    /// participant's queue being pushed into and never drained.
    ///
    /// `ArcSwap::rcu` is the compare-exchange retry loop for exactly this; the
    /// closure body is the same work.
    ///
    /// `connection_id` tags the entry so [`Self::unregister`] can tell a live
    /// holder from a superseded one; see [`SubscriberEntry`].
    pub fn register(
        &self,
        meeting: MeetingKey,
        sender: SenderId,
        connection_id: &str,
        queue: &Arc<EgressQueue>,
    ) {
        let key = SubscriberKey::new(meeting, sender);
        let entry = SubscriberEntry {
            connection_id: connection_id.to_string(),
            queue: Arc::clone(queue),
        };
        self.snapshot.rcu(|current| {
            let mut connections = current.connections.clone();
            connections.insert(key.clone(), entry.clone());
            LocalSubscriberSnapshot { connections }
        });
    }

    /// Remove one connection's egress queue, but only if `connection_id` still
    /// holds the `(meeting, sender)` slot. Teardown, not hot path.
    ///
    /// **Compare-and-remove**, for the same reason [`SenderBindings::unbind`] is:
    /// a connection superseded by a same-participant reconnect no longer owns the
    /// slot, and its teardown must not clear the live successor's queue. See
    /// [`SubscriberEntry`] for the silent-audio failure this prevents.
    ///
    /// `rcu` for the same concurrency reason as [`Self::register`].
    pub fn unregister(&self, meeting: &MeetingKey, sender: SenderId, connection_id: &str) {
        let key = SubscriberKey::new(meeting.clone(), sender);
        self.snapshot.rcu(|current| {
            let holder_is_caller = current
                .connections
                .get(&key)
                .is_some_and(|entry| entry.connection_id == connection_id);
            let mut connections = current.connections.clone();
            if holder_is_caller {
                connections.remove(&key);
            }
            LocalSubscriberSnapshot { connections }
        });
    }
}

/// Meeting-scoped sender-ordinal bindings: **which connection holds which
/// `sender_id` in which meeting**.
///
/// # What this type is FOR: uniqueness enforcement at the last hop
///
/// The binding itself arrives over the control plane — MC answers
/// [`NotifyParticipantConnected`] with the `sender_id` it allocated, and
/// `crate::webtransport::connection` validates it through
/// [`crate::routing::SenderId::from_wire`] and hands the resulting value
/// *directly* to the media tasks. **Nothing in the forward path reads this
/// registry**, and that is deliberate: `start_media_session` takes a validated
/// [`SenderId`] by value, so "a media session started without a binding" is
/// unrepresentable rather than being a property of the call graph that a reader
/// has to re-derive.
///
/// What this registry does is enforce the one invariant that no single
/// connection can check for itself: **within one meeting, one `sender_id` has
/// one holder**. Two different participants bound to the same ordinal is the
/// precise cross-participant collision the ordinal exists to prevent — MH would
/// forward one participant's frames onto the other's edges — and MH is the
/// component that would *act* on such a collision, so the last hop is where it
/// has to be detected. [`Self::bind`] refuses rather than overwrites, and the
/// refusal is counted (`mh_media_session_starts_total{outcome="declined_sender_binding_conflict"}`).
///
/// # Why the key is `(meeting, sender)` and not `(meeting, participant)`
///
/// The invariant is about the ORDINAL's uniqueness, so the ordinal is the key.
/// Keyed the other way, two different participants holding one ordinal are two
/// different map keys and a plain insert accepts both silently — the collision
/// would be undetectable by construction, and a counter named for it would name
/// a condition the code cannot see.
///
/// **Sender ids are meeting-scoped** (`internal.proto`: ordinal 5 exists
/// concurrently in every meeting), so the meeting is part of the key and a
/// collision in meeting A says nothing about meeting B. A global index would be
/// a cross-meeting media-crossing primitive.
///
/// # The value carries the connection, not just the participant
///
/// Teardown is a **compare-and-remove**: a connection only clears the binding it
/// actually holds. Without that, a superseded connection for the same
/// participant tearing down would clear a live connection's entry — which does
/// not break the running session (the session holds its own [`SenderId`]) but
/// *silently frees the ordinal for the uniqueness check*, i.e. a control about a
/// control failing open, which is the worse of the two failures.
///
/// Residual, bounded rather than chased: a participant reconnecting with a
/// **different** MC-allocated ordinal leaves its old entry held until the old
/// connection tears down. Reaching a stale entry that outlives its connection
/// requires MC to reallocate an ordinal for a live participant, which
/// contradicts the non-recycling allocation bound `internal.proto` states on
/// `sender_id`.
#[derive(Debug)]
pub struct SenderBindings {
    bindings: ArcSwap<HashMap<(MeetingKey, SenderId), BoundConnection>>,
}

/// Who holds one `(meeting, sender_id)` ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundConnection {
    /// The participant the ordinal was allocated to, from the validated meeting
    /// token's `sub`. Compared against on [`SenderBindings::bind`] to tell a
    /// cross-participant collision from a same-participant re-bind.
    participant_id: String,
    /// The connection that currently holds it. Compared against on
    /// [`SenderBindings::unbind`] so a superseded connection cannot clear a live
    /// one's binding.
    connection_id: String,
}

/// Why a [`SenderBindings::bind`] was refused.
///
/// One variant, because there is one refusable condition. It is an enum rather
/// than a `bool` so a second condition arrives as a compile error at every match
/// site instead of as a silently widened meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindConflict {
    /// The ordinal is already held by a **different** participant in this
    /// meeting. The incumbent is untouched; the newcomer is refused.
    HeldByAnotherParticipant,
}

impl Default for SenderBindings {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderBindings {
    /// A registry with no bindings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: ArcSwap::from_pointee(HashMap::new()),
        }
    }

    /// Claim `sender` for `participant_id` on `connection_id`, within `meeting`.
    ///
    /// **Incumbent wins.** If the ordinal is already held by a different
    /// participant, this refuses and mutates nothing; the incumbent connection
    /// keeps its binding and its edges. Overwriting instead would blackhole the
    /// incumbent and hand its edges to the newcomer, which is exactly the
    /// primitive this check exists to prevent.
    ///
    /// A re-bind by the **same** participant (a reconnect) takes over the entry
    /// rather than conflicting: it crosses no media between participants, so it
    /// is not an anomaly and has no operator remedy.
    ///
    /// # Errors
    ///
    /// [`BindConflict::HeldByAnotherParticipant`] — the caller must decline the
    /// media session and count it; see
    /// `crate::observability::metrics::MediaSessionStartOutcome::DeclinedSenderBindingConflict`.
    pub fn bind(
        &self,
        meeting: MeetingKey,
        participant_id: &str,
        connection_id: &str,
        sender: SenderId,
    ) -> Result<(), BindConflict> {
        let key = (meeting, sender);
        let claimant = BoundConnection {
            participant_id: participant_id.to_string(),
            connection_id: connection_id.to_string(),
        };

        // `rcu` for the same reason as `LocalSubscribers::register` — the
        // writers are per-connection tasks racing on one runtime, and a plain
        // read-modify-write loses an update when two participants connect
        // concurrently.
        //
        // The conflict check lives INSIDE the closure, not before it. A check
        // outside the compare-and-swap loop is a TOCTOU on precisely the
        // concurrent-connect race the `rcu` exists for: two connections could
        // both observe the ordinal free and both claim it. `rcu` may run the
        // closure more than once, so `conflict` is reset at the top of every
        // attempt rather than accumulated across them.
        let mut conflict = false;
        self.bindings.rcu(|current| {
            conflict = false;
            if let Some(incumbent) = current.get(&key) {
                if incumbent.participant_id != claimant.participant_id {
                    conflict = true;
                    // Store the map unchanged: refusing must not mutate.
                    return current.as_ref().clone();
                }
            }
            let mut bindings = current.as_ref().clone();
            bindings.insert(key.clone(), claimant.clone());
            bindings
        });

        if conflict {
            return Err(BindConflict::HeldByAnotherParticipant);
        }
        Ok(())
    }

    /// Release `sender` in `meeting`, but only if `connection_id` still holds it.
    ///
    /// Compare-and-remove: a connection that was superseded by a reconnect for
    /// the same participant no longer holds the entry, and its teardown must not
    /// clear the live connection's binding. See the type docs for why a
    /// clear-anyway `unbind` fails the uniqueness check open rather than
    /// breaking the running session.
    pub fn unbind(&self, meeting: &MeetingKey, sender: SenderId, connection_id: &str) {
        let key = (meeting.clone(), sender);
        self.bindings.rcu(|current| {
            let holder_is_caller = current
                .get(&key)
                .is_some_and(|held| held.connection_id == connection_id);
            if !holder_is_caller {
                return current.as_ref().clone();
            }
            let mut bindings = current.as_ref().clone();
            bindings.remove(&key);
            bindings
        });
    }

    /// The participant currently holding `sender` in `meeting`, if any.
    ///
    /// The registry's read API. Its production consumer is [`Self::bind`]'s own
    /// conflict check; this exposes the same question to tests and to future
    /// diagnostics without handing out the map.
    #[must_use]
    pub fn holder_of(&self, meeting: &MeetingKey, sender: SenderId) -> Option<String> {
        self.bindings
            .load()
            .get(&(meeting.clone(), sender))
            .map(|held| held.participant_id.clone())
    }
}

// ---------------------------------------------------------------------------
// Handle (public API)
// ---------------------------------------------------------------------------

/// Handle to the `SessionManagerActor`.
///
/// This is the public interface for session management. Cloneable via the
/// inner `mpsc::Sender`. All methods are async and use message passing
/// to communicate with the actor task.
#[derive(Debug, Clone)]
pub struct SessionManagerHandle {
    sender: mpsc::Sender<SessionMessage>,
    config_sender: mpsc::Sender<ConfigApplyMessage>,
    /// Shared with the actor. The actor is the only **writer**; handles read it
    /// directly, without a mailbox round trip, which is what lets the RPC
    /// response report the live applied generation even when the actor is
    /// unreachable.
    routing: Arc<RoutingTable>,
    /// Which subscriber connections live on this handler, for the forward
    /// path's egress lookup. Written at connect/disconnect, read per frame.
    /// Not behind the mailbox for the same reason `routing` is not: a per-frame
    /// read must not take a mailbox round trip.
    subscribers: Arc<LocalSubscribers>,
    /// Which connection holds which `sender_id` in which meeting. Written at
    /// connect and cleared at teardown; see [`SenderBindings`] for why the
    /// forward path does not read it.
    sender_bindings: Arc<SenderBindings>,
}

impl SessionManagerHandle {
    /// Create a new `SessionManagerActor` and return a handle to it.
    ///
    /// Spawns the actor task immediately. The actor runs until all handles
    /// are dropped (channels close).
    #[must_use]
    pub fn new(admission: StreamAdmission) -> Self {
        let (handle, actor) = Self::new_with_parts(admission);
        tokio::spawn(actor.run());
        handle
    }

    /// Build a handle and its actor **without spawning**, leaving the caller to
    /// decide when — or whether — the actor runs.
    ///
    /// This is the ordinary two-part actor constructor, not a test seam:
    /// [`Self::new`] is written in terms of it. It matters because the two
    /// config-apply failure paths (mailbox full, apply timeout) are otherwise
    /// only reachable by racing a live actor, and a gate that depends on
    /// winning a race is a flake rather than a gate. With the actor built and
    /// deliberately not spawned, the consumer is *structurally absent*: a full
    /// mailbox stays full because nothing exists that could drain it, and a
    /// reply never arrives because no task holds the sending half.
    #[must_use]
    pub fn new_with_parts(admission: StreamAdmission) -> (Self, SessionManagerActor) {
        let (sender, receiver) = mpsc::channel(SESSION_CHANNEL_BUFFER);
        let (config_sender, config_receiver) = mpsc::channel(CONFIG_APPLY_CHANNEL_BUFFER);
        let routing = Arc::new(RoutingTable::new());
        let actor =
            SessionManagerActor::new(receiver, config_receiver, Arc::clone(&routing), admission);
        (
            Self {
                sender,
                config_sender,
                routing,
                subscribers: Arc::new(LocalSubscribers::new()),
                sender_bindings: Arc::new(SenderBindings::new()),
            },
            actor,
        )
    }

    /// The lock-free routing table itself, for consumers that must re-read it
    /// per frame rather than hold one snapshot.
    ///
    /// The forward path takes this, not [`Self::routing_snapshot`]: a snapshot
    /// captured once and held would be a stale edge list, and ADR-0036 §7
    /// enforces server mute and participant removal through this lookup.
    #[must_use]
    pub fn routing_table(&self) -> Arc<RoutingTable> {
        Arc::clone(&self.routing)
    }

    /// The subscriber registry the forward path reads on every frame.
    #[must_use]
    pub fn subscribers(&self) -> &Arc<LocalSubscribers> {
        &self.subscribers
    }

    /// The meeting-scoped sender-ordinal bindings.
    #[must_use]
    pub fn sender_bindings(&self) -> &Arc<SenderBindings> {
        &self.sender_bindings
    }

    /// Remaining headroom in the **connection-lifecycle** mailbox.
    ///
    /// Exposed alongside [`Self::config_apply_capacity`] because ADR-0036 §8
    /// requires the two workloads not to contend, and a requirement about
    /// mailbox contention needs the depths to be *observable* — "declared in
    /// one place, assumed in another, verified nowhere" is the precedent §8
    /// itself warns about.
    #[must_use]
    pub fn lifecycle_capacity(&self) -> usize {
        self.sender.capacity()
    }

    /// Remaining headroom in the **config-apply** mailbox.
    #[must_use]
    pub fn config_apply_capacity(&self) -> usize {
        self.config_sender.capacity()
    }

    /// The live routing snapshot the data plane forwards against.
    ///
    /// Lock-free and mailbox-free, so it is safe on the per-frame path and
    /// still answerable when the actor is wedged.
    ///
    /// **This is the single source for `RegisterMeetingResponse`'s
    /// `applied_generation` and `transport_mode`.** ADR-0036 §8: an implementer
    /// who writes `applied_generation: req.policy_generation` "has deleted the
    /// feature while leaving the field". Sourcing both from here means the
    /// received generation is not merely forbidden as an answer — it is not in
    /// scope at the point the response is built.
    #[must_use]
    pub fn routing_snapshot(&self) -> Arc<RoutingSnapshot> {
        self.routing.load()
    }

    /// Hand a validated policy to the actor's config-apply mailbox.
    ///
    /// Never blocks on a full mailbox (`try_send`) and never awaits the reply
    /// unbounded: a wedged actor must yield a truthful stale
    /// `applied_generation` with a failure outcome, not a hung RPC that
    /// consumes MC's deadline.
    ///
    /// # Instrumentation
    ///
    /// Spanned here and **only** here on this path. This runs on the caller's
    /// task, so the span is a genuine child of `register_meeting`'s and
    /// isolates the two steps the handler cannot otherwise tell apart: the
    /// `try_send`, and the bounded await. That distinction is the difference
    /// between "apply failed, timeout" and "apply failed, and here is the
    /// 1000 ms" — there is no histogram on this path, so the span is the only
    /// evidence of where the time went.
    ///
    /// `skip_all` is mandatory, not stylistic: [`MeetingPolicy`] carries the
    /// entire egress set, so a default `#[instrument]` would render every
    /// `sender_id`, `slot_id` and `egress_stream_id` into span attributes —
    /// `label-taxonomy.md` §R2 on the span surface.
    ///
    /// **The actor-side handler is deliberately NOT instrumented.** It runs on
    /// a different task reached over an mpsc, so an `#[instrument]` there
    /// produces a disconnected ROOT span rather than a child: it would look
    /// like instrumentation while splitting the trace into two unrelated
    /// pieces, which is worse than none. Per-actor timing, if ever wanted,
    /// needs explicit context propagation across the message.
    #[tracing::instrument(skip_all)]
    pub async fn apply_policy(
        &self,
        policy: MeetingPolicy,
        max_total_egress_edges: usize,
        timeout: Duration,
    ) -> ApplyOutcome {
        let (tx, rx) = oneshot::channel();
        if self
            .config_sender
            .try_send(ConfigApplyMessage::ApplyPolicy {
                policy: Box::new(policy),
                max_total_egress_edges,
                respond_to: tx,
            })
            .is_err()
        {
            // Covers both a full mailbox and a closed one. Distinguished for
            // the operator because the remedies differ: back-pressure on the
            // control plane versus an actor that is gone.
            let failure = if self.config_sender.is_closed() {
                ApplyFailure::ActorUnavailable
            } else {
                ApplyFailure::MailboxFull
            };
            tracing::warn!(
                target: "mh.session.policy",
                key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                reason = failure.reason(),
                config_apply_capacity = CONFIG_APPLY_CHANNEL_BUFFER,
                "Policy apply not attempted; prior generation stays live"
            );
            return ApplyOutcome::Failed(failure);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => {
                tracing::warn!(
                    target: "mh.session.policy",
                    key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                    reason = ApplyFailure::ActorUnavailable.reason(),
                    "Policy apply reply dropped; prior generation stays live"
                );
                ApplyOutcome::Failed(ApplyFailure::ActorUnavailable)
            }
            Err(_) => {
                tracing::warn!(
                    target: "mh.session.policy",
                    key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                    reason = ApplyFailure::Timeout.reason(),
                    timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
                    "Policy apply timed out; prior generation stays live"
                );
                ApplyOutcome::Failed(ApplyFailure::Timeout)
            }
        }
    }

    /// Register a meeting on this MH instance.
    ///
    /// Called when MC sends `RegisterMeeting` RPC. Returns any pending
    /// connections that were waiting for this meeting's registration, so the
    /// caller can dispatch notifications.
    ///
    /// # Errors
    ///
    /// [`RegisterRefusal::MeetingCapReached`] when this is a NEW meeting id and
    /// the handler already holds `max_registered_meetings` registrations.
    /// Nothing was upserted or promoted.
    pub async fn register_meeting(
        &self,
        meeting_id: String,
        registration: MeetingRegistration,
        max_registered_meetings: usize,
    ) -> Result<Vec<PendingConnection>, RegisterRefusal> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                max_registered_meetings,
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on register_meeting");
            return Ok(Vec::new());
        }
        rx.await.unwrap_or_else(|_| Ok(Vec::new()))
    }

    /// Release a meeting (story 2 R-20): the actor decides released / unknown
    /// / rejected-ownership and counts it.
    ///
    /// The await is BOUNDED by `timeout` (the caller passes
    /// `MH_POLICY_APPLY_TIMEOUT_MS`, the one "how long an RPC waits for the
    /// session actor" bound): a wedged actor must yield an error, not a hung
    /// RPC that consumes MC's deadline and stacks its retries. Uses the
    /// lifecycle mailbox's `send().await`, bounded by the same timeout.
    ///
    /// # Errors
    ///
    /// [`EndMeetingFailure`] when the actor is gone or did not answer in time.
    /// The actor may still process a timed-out request afterwards; that is
    /// safe, because a release is idempotent and a later retry of the same
    /// request answers `unknown_meeting`.
    pub async fn end_meeting(
        &self,
        meeting_id: String,
        mc_id: String,
        timeout: Duration,
    ) -> Result<MeetingTeardownOutcome, EndMeetingFailure> {
        let (tx, rx) = oneshot::channel();
        let exchange = async {
            self.sender
                .send(SessionMessage::EndMeeting {
                    meeting_id,
                    mc_id,
                    respond_to: tx,
                })
                .await
                .map_err(|_| EndMeetingFailure::ActorUnavailable)?;
            rx.await.map_err(|_| EndMeetingFailure::ActorUnavailable)
        };
        match tokio::time::timeout(timeout, exchange).await {
            Ok(result) => result,
            Err(_) => Err(EndMeetingFailure::Timeout),
        }
    }

    /// Check if a meeting is registered on this MH instance.
    pub async fn is_meeting_registered(&self, meeting_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::IsMeetingRegistered {
                meeting_id: meeting_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on is_meeting_registered");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Get the MC gRPC endpoint for a registered meeting.
    pub async fn get_mc_endpoint(&self, meeting_id: &str) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::GetMcEndpoint {
                meeting_id: meeting_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on get_mc_endpoint");
            return None;
        }
        rx.await.unwrap_or(None)
    }

    /// Add an active connection for a registered meeting.
    ///
    /// Fire-and-forget: the caller does not need confirmation.
    pub async fn add_connection(&self, meeting_id: &str, entry: ConnectionEntry) {
        if self
            .sender
            .send(SessionMessage::AddConnection {
                meeting_id: meeting_id.to_string(),
                entry,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on add_connection");
        }
    }

    /// Remove a connection by `connection_id`. Returns true if found and removed.
    pub async fn remove_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RemoveConnection {
                meeting_id: meeting_id.to_string(),
                connection_id: connection_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on remove_connection");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Add a pending connection for an unregistered meeting.
    ///
    /// Returns a `Notify` handle that will be triggered when
    /// `RegisterMeeting` arrives for this `meeting_id`.
    pub async fn add_pending_connection(&self, pending: PendingConnection) -> Arc<Notify> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::AddPendingConnection {
                pending,
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on add_pending_connection");
            return Arc::new(Notify::new());
        }
        rx.await.unwrap_or_else(|_| Arc::new(Notify::new()))
    }

    /// Remove a pending connection by `connection_id`.
    /// Called when the provisional timeout expires.
    pub async fn remove_pending_connection(&self, meeting_id: &str, connection_id: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RemovePendingConnection {
                meeting_id: meeting_id.to_string(),
                connection_id: connection_id.to_string(),
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on remove_pending_connection");
            return false;
        }
        rx.await.unwrap_or(false)
    }

    /// Get count of active connections across all meetings.
    pub async fn active_connection_count(&self) -> usize {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::ActiveConnectionCount { respond_to: tx })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on active_connection_count");
            return 0;
        }
        rx.await.unwrap_or(0)
    }
}

// There is deliberately no `Default` impl: a session manager with no stream
// ceiling is not a representable state (story 2 R-19), and a default would
// have to invent one.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::admission::test_admission;
    use super::*;

    /// The cap tests run with when they are not ABOUT the cap — the `src/`
    /// fixture's value, so there is one source.
    const TEST_CAP: usize = crate::config::PolicyLimits::for_tests().max_registered_meetings;

    fn make_registration(mc_id: &str, endpoint: &str) -> MeetingRegistration {
        MeetingRegistration {
            mc_id: mc_id.to_string(),
            mc_grpc_endpoint: endpoint.to_string(),
            registered_at: Instant::now(),
        }
    }

    fn make_connection(conn_id: &str, participant_id: &str) -> ConnectionEntry {
        ConnectionEntry {
            connection_id: conn_id.to_string(),
            participant_id: participant_id.to_string(),
            connected_at: Instant::now(),
            close: CancellationToken::new(),
        }
    }

    fn make_pending(conn_id: &str, meeting_id: &str, participant_id: &str) -> PendingConnection {
        PendingConnection {
            connection_id: conn_id.to_string(),
            meeting_id: meeting_id.to_string(),
            participant_id: participant_id.to_string(),
            connected_at: Instant::now(),
            close: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn test_register_meeting() {
        let handle = SessionManagerHandle::new(test_admission());
        assert!(!handle.is_meeting_registered("meeting-1").await);

        let pending = handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        assert!(handle.is_meeting_registered("meeting-1").await);
        assert!(pending.is_empty());
        assert_eq!(
            handle.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_add_and_remove_connection() {
        let handle = SessionManagerHandle::new(test_admission());
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        handle
            .add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;
        handle
            .add_connection("meeting-1", make_connection("conn-2", "user-2"))
            .await;

        assert_eq!(handle.active_connection_count().await, 2);

        assert!(handle.remove_connection("meeting-1", "conn-1").await);
        assert_eq!(handle.active_connection_count().await, 1);

        // Removing non-existent connection returns false
        assert!(!handle.remove_connection("meeting-1", "conn-999").await);
    }

    #[tokio::test]
    async fn test_pending_connection_promoted_on_register() {
        let handle = SessionManagerHandle::new(test_admission());

        // Add pending connection before RegisterMeeting
        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;
        let _notify2 = handle
            .add_pending_connection(make_pending("conn-2", "meeting-1", "user-2"))
            .await;

        // RegisterMeeting arrives — returns pending connections
        let promoted = handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        assert_eq!(promoted.len(), 2);
        assert_eq!(promoted[0].connection_id, "conn-1");
        assert_eq!(promoted[1].connection_id, "conn-2");

        // Promoted connections should now be active
        assert_eq!(handle.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_pending_connection_for_different_meeting_not_promoted() {
        let handle = SessionManagerHandle::new(test_admission());

        // Add pending for meeting-1
        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Register meeting-2 — should NOT promote meeting-1's pending
        let promoted = handle
            .register_meeting(
                "meeting-2".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        assert!(promoted.is_empty());
        assert_eq!(handle.active_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_pending_connection() {
        let handle = SessionManagerHandle::new(test_admission());

        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        assert!(
            handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );
        // Should not find it again
        assert!(
            !handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );
    }

    #[tokio::test]
    async fn test_notify_wakes_pending_on_register() {
        let handle = SessionManagerHandle::new(test_admission());

        let notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Spawn a task that waits on the notify
        let handle_clone = handle.clone();
        let join_handle = tokio::spawn(async move {
            notify.notified().await;
            handle_clone.is_meeting_registered("meeting-1").await
        });

        // Small yield to let the spawned task start waiting
        tokio::task::yield_now().await;

        // Register meeting — should trigger notify
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        let result = join_handle.await.unwrap();
        assert!(result, "Meeting should be registered after notify");
    }

    #[tokio::test]
    async fn test_registered_connection_not_affected_by_timeout() {
        let handle = SessionManagerHandle::new(test_admission());

        // Register meeting first
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
                TEST_CAP,
            )
            .await
            .unwrap();

        // Add active connection (not pending — meeting is already registered)
        handle
            .add_connection("meeting-1", make_connection("conn-1", "user-1"))
            .await;

        // Trying to remove as pending should return false
        assert!(
            !handle
                .remove_pending_connection("meeting-1", "conn-1")
                .await
        );

        // Active connection should still be there
        assert_eq!(handle.active_connection_count().await, 1);
    }

    #[tokio::test]
    async fn test_get_mc_endpoint_unregistered() {
        let handle = SessionManagerHandle::new(test_admission());
        assert!(handle.get_mc_endpoint("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn a_fresh_handle_is_empty() {
        let handle = SessionManagerHandle::new(test_admission());
        assert!(!handle.is_meeting_registered("any").await);
        assert_eq!(handle.active_connection_count().await, 0);
    }

    // -----------------------------------------------------------------------
    // Subscriber registry and sender bindings
    // -----------------------------------------------------------------------

    fn key(value: u32) -> SenderId {
        SenderId::from_wire(value).unwrap()
    }

    fn queue() -> Arc<crate::media::forward::EgressQueue> {
        Arc::new(crate::media::queue::SharedQueue::new(4))
    }

    #[test]
    fn a_registered_subscriber_resolves_and_an_unregistered_one_does_not() {
        // Both arms: the negative alone also passes for a registry that
        // resolves nothing at all.
        let subscribers = LocalSubscribers::new();
        let meeting = MeetingKey::new("m-1");
        subscribers.register(meeting.clone(), key(5), "conn-a", &queue());

        assert!(subscribers.load().egress_queue(&meeting, key(5)).is_some());
        assert!(
            subscribers.load().egress_queue(&meeting, key(6)).is_none(),
            "an unregistered ordinal must not resolve"
        );
        assert_eq!(subscribers.load().len(), 1);
    }

    #[test]
    fn unregistering_removes_only_that_subscriber() {
        let subscribers = LocalSubscribers::new();
        let meeting = MeetingKey::new("m-1");
        subscribers.register(meeting.clone(), key(5), "conn-5", &queue());
        subscribers.register(meeting.clone(), key(6), "conn-6", &queue());

        subscribers.unregister(&meeting, key(5), "conn-5");

        assert!(subscribers.load().egress_queue(&meeting, key(5)).is_none());
        assert!(
            subscribers.load().egress_queue(&meeting, key(6)).is_some(),
            "unregistering one subscriber must not disturb another"
        );
    }

    #[test]
    fn a_superseded_connection_teardown_does_not_clear_the_live_subscriber() {
        // SEC-2, the subscriber-registry twin of
        // `a_superseded_connection_teardown_does_not_clear_the_live_binding`.
        // `conn-1` and `conn-2` are the SAME participant reconnecting, so both
        // hold ordinal 5. `conn-2` registers second and wins the slot. When the
        // superseded `conn-1` tears down it must NOT clear `conn-2`'s egress
        // queue — an unconditional remove here leaves `conn-2` connected but
        // absent from the snapshot, so every frame addressed to it counts
        // `no_local_subscriber` and the participant goes permanently silent.
        let subscribers = LocalSubscribers::new();
        let meeting = MeetingKey::new("m-1");
        let live = queue();
        subscribers.register(meeting.clone(), key(5), "conn-1", &queue());
        subscribers.register(meeting.clone(), key(5), "conn-2", &live);

        subscribers.unregister(&meeting, key(5), "conn-1");

        let held = subscribers
            .load()
            .egress_queue(&meeting, key(5))
            .cloned()
            .expect("the live successor's queue must survive the superseded teardown");
        assert!(
            Arc::ptr_eq(&held, &live),
            "the slot must still hold conn-2's queue, not be cleared by conn-1's teardown"
        );

        // And a real teardown by the current holder still releases it.
        subscribers.unregister(&meeting, key(5), "conn-2");
        assert!(
            subscribers.load().egress_queue(&meeting, key(5)).is_none(),
            "the holder's own unregister must release the slot"
        );
    }

    #[test]
    fn a_subscriber_registered_in_one_meeting_does_not_resolve_in_another() {
        // The load-bearing arm. A `sender_id` is a 16-bit PER-MEETING ordinal,
        // so ordinal 5 exists concurrently in every meeting on this handler; a
        // registry keyed on the ordinal alone would resolve meeting B's frames
        // onto meeting A's subscriber. Both arms, because the negative alone
        // also passes for a registry that resolves nothing anywhere.
        let subscribers = LocalSubscribers::new();
        let meeting_a = MeetingKey::new("meeting-a");
        let meeting_b = MeetingKey::new("meeting-b");
        subscribers.register(meeting_a.clone(), key(5), "conn-a", &queue());

        assert!(
            subscribers
                .load()
                .egress_queue(&meeting_a, key(5))
                .is_some(),
            "ordinal 5 must resolve within its own meeting"
        );
        assert!(
            subscribers
                .load()
                .egress_queue(&meeting_b, key(5))
                .is_none(),
            "ordinal 5 is meeting A's; resolving it in meeting B is cross-tenant leakage"
        );
    }

    #[test]
    fn an_ordinal_is_held_by_the_participant_that_bound_it() {
        let bindings = SenderBindings::new();
        let meeting = MeetingKey::new("m-1");

        bindings
            .bind(meeting.clone(), "participant-a", "conn-a", key(5))
            .expect("a free ordinal must bind");

        assert_eq!(
            bindings.holder_of(&meeting, key(5)),
            Some("participant-a".to_string())
        );
        assert_eq!(
            bindings.holder_of(&meeting, key(6)),
            None,
            "an unclaimed ordinal has no holder"
        );
    }

    #[test]
    fn a_second_participant_cannot_take_an_ordinal_the_first_holds() {
        // THE reason this type exists. Overwriting here would blackhole the
        // incumbent and hand its edges to the newcomer — one participant's
        // frames delivered to another participant's subscribers.
        let bindings = SenderBindings::new();
        let meeting = MeetingKey::new("m-1");
        bindings
            .bind(meeting.clone(), "participant-a", "conn-a", key(5))
            .expect("a free ordinal must bind");

        let refused = bindings.bind(meeting.clone(), "participant-b", "conn-b", key(5));

        assert_eq!(
            refused,
            Err(BindConflict::HeldByAnotherParticipant),
            "a second participant claiming a held ordinal must be refused, not accepted"
        );
        assert_eq!(
            bindings.holder_of(&meeting, key(5)),
            Some("participant-a".to_string()),
            "INCUMBENT WINS: the refusal must leave the incumbent's binding exactly as it was. \
             A refusal that also clears or overwrites the entry would deny both participants"
        );
    }

    #[test]
    fn the_same_participant_reconnecting_takes_over_its_own_ordinal() {
        // Self-collision crosses no media between participants, so it is not an
        // anomaly and must not be refused — a reconnect would otherwise be
        // permanently unable to start media until the old connection tore down.
        let bindings = SenderBindings::new();
        let meeting = MeetingKey::new("m-1");
        bindings
            .bind(meeting.clone(), "participant-a", "conn-1", key(5))
            .expect("a free ordinal must bind");

        bindings
            .bind(meeting.clone(), "participant-a", "conn-2", key(5))
            .expect("the same participant reconnecting must not be treated as a collision");

        assert_eq!(
            bindings.holder_of(&meeting, key(5)),
            Some("participant-a".to_string())
        );
    }

    #[test]
    fn unbinding_releases_the_ordinal_for_another_participant() {
        let bindings = SenderBindings::new();
        let meeting = MeetingKey::new("m-1");
        bindings
            .bind(meeting.clone(), "participant-a", "conn-a", key(5))
            .expect("a free ordinal must bind");

        bindings.unbind(&meeting, key(5), "conn-a");

        assert_eq!(bindings.holder_of(&meeting, key(5)), None);
        bindings
            .bind(meeting.clone(), "participant-b", "conn-b", key(5))
            .expect("a released ordinal must be claimable again");
    }

    #[test]
    fn a_superseded_connection_teardown_does_not_clear_the_live_binding() {
        // The control-about-a-control case. `conn-1` is superseded by `conn-2`
        // for the same participant; when `conn-1` finally tears down it must NOT
        // release the ordinal, because `conn-2` is still using it. A
        // clear-anyway unbind does not break `conn-2`'s running session — the
        // session holds its own `SenderId` — it silently frees the ordinal for
        // the uniqueness check, so the collision guard goes quiet rather than
        // anything visibly failing. That is why this is a test and not a comment.
        let bindings = SenderBindings::new();
        let meeting = MeetingKey::new("m-1");
        bindings
            .bind(meeting.clone(), "participant-a", "conn-1", key(5))
            .expect("a free ordinal must bind");
        bindings
            .bind(meeting.clone(), "participant-a", "conn-2", key(5))
            .expect("the same participant reconnecting must not be treated as a collision");

        bindings.unbind(&meeting, key(5), "conn-1");

        assert_eq!(
            bindings.holder_of(&meeting, key(5)),
            Some("participant-a".to_string()),
            "a superseded connection must not release an ordinal it no longer holds"
        );
        assert_eq!(
            bindings.bind(meeting.clone(), "participant-b", "conn-b", key(5)),
            Err(BindConflict::HeldByAnotherParticipant),
            "the uniqueness guard must still be armed after the superseded teardown — this is \
             the assertion that fails if `unbind` clears unconditionally"
        );
    }

    #[test]
    fn one_ordinal_is_held_independently_in_two_meetings() {
        // Sender ids are per-meeting ordinals (`internal.proto`): ordinal 5
        // exists concurrently in every meeting on this handler. A collision in
        // meeting A must say nothing about meeting B — a registry keyed on the
        // ordinal alone would refuse legitimate bindings AND, keyed the other
        // way, would let meeting B's forward path resolve an ordinal minted in
        // meeting A.
        let bindings = SenderBindings::new();
        let meeting_a = MeetingKey::new("meeting-a");
        let meeting_b = MeetingKey::new("meeting-b");

        bindings
            .bind(meeting_a.clone(), "participant-a", "conn-a", key(5))
            .expect("a free ordinal must bind");
        bindings
            .bind(meeting_b.clone(), "participant-b", "conn-b", key(5))
            .expect("the same ordinal in a DIFFERENT meeting is not a collision");

        assert_eq!(
            bindings.holder_of(&meeting_a, key(5)),
            Some("participant-a".to_string())
        );
        assert_eq!(
            bindings.holder_of(&meeting_b, key(5)),
            Some("participant-b".to_string()),
            "each meeting holds its own ordinal 5; conflating them is cross-tenant leakage"
        );
    }

    // =======================================================================
    // Story 2 R-20 / R-21: ownership, the registration cap, and release.
    // Driven on an UNSPAWNED actor so the handlers run synchronously and the
    // maps the RPC never exposes (notifiers, emptied connection entries) can
    // be inspected directly.
    // =======================================================================

    fn unspawned_actor() -> SessionManagerActor {
        SessionManagerHandle::new_with_parts(test_admission()).1
    }

    #[test]
    fn ownership_is_a_byte_exact_comparison_with_no_normalisation() {
        assert_eq!(Ownership::of("mc-1", "mc-1"), Ownership::SameMc);
        for different in ["mc-2", "MC-1", " mc-1", "mc-1 ", "", "mc-1\u{0}"] {
            assert_eq!(
                Ownership::of("mc-1", different),
                Ownership::DifferentMc,
                "{different:?} must not match mc-1: any normalisation widens the accept set"
            );
        }
    }

    #[test]
    fn the_cap_refuses_only_a_new_meeting_id() {
        let mut actor = unspawned_actor();
        actor
            .handle_register_meeting("m-1".into(), make_registration("mc-1", "http://mc"), 1)
            .unwrap();

        // The held meeting re-registers at the cap: admitted.
        assert!(actor
            .handle_register_meeting("m-1".into(), make_registration("mc-1", "http://mc"), 1)
            .is_ok());
        // A new one: refused, with the counts, and nothing inserted.
        assert_eq!(
            actor
                .handle_register_meeting("m-2".into(), make_registration("mc-1", "http://mc"), 1)
                .unwrap_err(),
            RegisterRefusal::MeetingCapReached { held: 1, cap: 1 }
        );
        assert!(!actor.state.registered_meetings.contains_key("m-2"));
    }

    /// Release closes every ACTIVE connection, leaves PENDING ones (and the
    /// notifier they wait on) for the registration timeout, and reclaims the
    /// registration and the active-connection entry.
    #[test]
    fn release_closes_active_connections_and_leaves_pending_ones() {
        let mut actor = unspawned_actor();
        let promoted_close = CancellationToken::new();
        let mut early = make_pending("conn-early", "m-1", "user-1");
        early.close = promoted_close.clone();
        actor.handle_add_pending_connection(early);
        actor
            .handle_register_meeting(
                "m-1".into(),
                make_registration("mc-1", "http://mc"),
                TEST_CAP,
            )
            .unwrap();
        let direct = make_connection("conn-direct", "user-2");
        let direct_close = direct.close.clone();
        actor.handle_add_connection("m-1".into(), direct);

        // A re-join racing the teardown: pending, NOT to be closed.
        let late = make_pending("conn-late", "m-1", "user-3");
        let late_close = late.close.clone();
        actor.handle_add_pending_connection(late);

        assert_eq!(
            actor.handle_end_meeting("m-1", "mc-1"),
            MeetingTeardownOutcome::Released
        );
        assert!(
            promoted_close.is_cancelled(),
            "a promoted connection is closed"
        );
        assert!(
            direct_close.is_cancelled(),
            "a directly-added connection is closed"
        );
        assert!(
            !late_close.is_cancelled(),
            "a pending connection is left alone"
        );
        assert!(!actor.state.registered_meetings.contains_key("m-1"));
        assert!(!actor.state.active_connections.contains_key("m-1"));
        assert!(
            actor.state.meeting_notifiers.contains_key("m-1"),
            "the pending connection still waits on this meeting's notifier"
        );
    }

    #[test]
    fn a_mismatched_release_mutates_nothing_and_an_unknown_one_is_a_no_op() {
        let mut actor = unspawned_actor();
        actor
            .handle_register_meeting(
                "m-1".into(),
                make_registration("mc-1", "http://mc"),
                TEST_CAP,
            )
            .unwrap();
        let conn = make_connection("conn-1", "user-1");
        let close = conn.close.clone();
        actor.handle_add_connection("m-1".into(), conn);

        assert_eq!(
            actor.handle_end_meeting("m-1", "mc-2"),
            MeetingTeardownOutcome::RejectedOwnership
        );
        assert!(actor.state.registered_meetings.contains_key("m-1"));
        assert!(!close.is_cancelled());

        assert_eq!(
            actor.handle_end_meeting("nope", "mc-1"),
            MeetingTeardownOutcome::UnknownMeeting
        );
    }

    /// An `AddConnection` that lands after the meeting was released (the
    /// connection checked registration, then `EndMeeting` ran) is closed, not
    /// inserted into a map nothing will ever release again.
    #[test]
    fn a_connection_arriving_for_a_released_meeting_is_closed_not_inserted() {
        let mut actor = unspawned_actor();
        let conn = make_connection("conn-1", "user-1");
        let close = conn.close.clone();
        actor.handle_add_connection("released".into(), conn);
        assert!(close.is_cancelled());
        assert!(!actor.state.active_connections.contains_key("released"));
    }

    /// Every meeting-keyed map reclaims its entry: emptied connection entries
    /// are dropped, and the notifier goes with the last pending connection and
    /// on promotion. Before this, both maps only grew.
    #[test]
    fn meeting_keyed_maps_do_not_keep_husks() {
        let mut actor = unspawned_actor();
        actor
            .handle_register_meeting(
                "m-1".into(),
                make_registration("mc-1", "http://mc"),
                TEST_CAP,
            )
            .unwrap();
        actor.handle_add_connection("m-1".into(), make_connection("conn-1", "user-1"));
        assert!(actor.handle_remove_connection("m-1", "conn-1"));
        assert!(
            !actor.state.active_connections.contains_key("m-1"),
            "an emptied meeting entry is dropped"
        );

        actor.handle_add_pending_connection(make_pending("conn-2", "m-2", "user-2"));
        assert!(actor.state.meeting_notifiers.contains_key("m-2"));
        assert!(actor.handle_remove_pending_connection("m-2", "conn-2"));
        assert!(!actor.state.meeting_notifiers.contains_key("m-2"));

        actor.handle_add_pending_connection(make_pending("conn-3", "m-3", "user-3"));
        actor
            .handle_register_meeting(
                "m-3".into(),
                make_registration("mc-1", "http://mc"),
                TEST_CAP,
            )
            .unwrap();
        assert!(
            !actor.state.meeting_notifiers.contains_key("m-3"),
            "promotion empties the pending list, so the notifier goes with it"
        );
    }
}
