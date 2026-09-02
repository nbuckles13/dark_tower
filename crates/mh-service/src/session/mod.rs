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

use crate::routing::{MeetingPolicy, RoutingSnapshot, RoutingTable};

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
    /// Register a meeting. Returns any pending connections that were promoted.
    RegisterMeeting {
        meeting_id: String,
        registration: MeetingRegistration,
        respond_to: oneshot::Sender<Vec<PendingConnection>>,
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
}

impl SessionManagerActor {
    fn new(
        receiver: mpsc::Receiver<SessionMessage>,
        config_receiver: mpsc::Receiver<ConfigApplyMessage>,
        routing: Arc<RoutingTable>,
    ) -> Self {
        Self {
            receiver,
            config_receiver,
            state: SessionState::default(),
            routing,
        }
    }

    /// Main run loop. Selects over both mailboxes until both close.
    ///
    /// A closed mailbox yields `None` forever, so the `is_some()` guards stop
    /// that arm busy-looping while the other still has senders. The loop ends
    /// only when both channels are closed, which happens at shutdown.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.receiver.recv() => self.handle_message(msg),
                Some(msg) = self.config_receiver.recv() => self.handle_config_apply(msg),
                else => break,
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
        } else if projected_total_edges > max_total_egress_edges {
            // Aggregate resource-exhaustion guard, NOT a capacity figure and
            // never advertised to GC. All-or-none: no swap, prior generation
            // stays live, so the forward path reflects N-1 rather than neither.
            //
            // `installed_meeting_count` and `installed_total_edges` are here
            // because this bound RATCHETS: MH receives no meeting-teardown
            // signal, so a meeting's edges are released only by that same
            // meeting re-asserting a smaller policy, never by it ending. The
            // one question an operator has on seeing this line is "is this one
            // fat policy, or accumulated dead meetings?", and only these two
            // counts answer it. Both are bounded aggregates carrying no
            // identity.
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
            self.routing.install(&policy);
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

    fn handle_message(&mut self, msg: SessionMessage) {
        match msg {
            SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                respond_to,
            } => {
                let result = self.handle_register_meeting(meeting_id, registration);
                let _ = respond_to.send(result);
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
    /// that existed with `Arc<RwLock<SessionState>>`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "meeting_id is used as owned key in insert/entry calls; &str would require .to_string() at each site"
    )]
    fn handle_register_meeting(
        &mut self,
        meeting_id: String,
        registration: MeetingRegistration,
    ) -> Vec<PendingConnection> {
        // An ordinary re-assert and a change of OWNING MC are the same insert,
        // and before this they were also the same INFO line. They are not the
        // same event, and the generation ratchet this story adds is why:
        // `handle_config_apply` refuses anything below the installed
        // generation and has no downward path within the process, so a
        // registration from a different MC carrying a high `policy_generation`
        // wedges every subsequent re-assert from the owning MC onto
        // `rejected_stale` — permanently, while `accepted: true` and
        // `mh_grpc_requests_total{status="success"}` both stay green.
        //
        // MH cannot decide here whether that is an attack or a legitimate MC
        // failover: `MhAuthLayer` authorizes on scope and `service_type` only
        // and binds no caller to a `meeting_id`, so both look identical on the
        // wire. Enforcement is a contract question for media-handler +
        // meeting-controller and is tracked in `docs/TODO.md` §Media Path
        // Obligations. What lands here is the DETECTION half: the two events
        // stop sharing a log line and a level.
        //
        // `mc_id` is safe as a field — a bounded service identity the handler
        // already length-caps and already logs — unlike the per-stream
        // identities ADR-0036 §11 bars.
        match self.state.registered_meetings.get(&meeting_id) {
            Some(existing) if existing.mc_id != registration.mc_id => {
                tracing::warn!(
                    target: "mh.session",
                    key_custody = common::observability::labels::KEY_CUSTODY_OPERATOR,
                    meeting_id = %meeting_id,
                    previous_mc_id = %existing.mc_id,
                    new_mc_id = %registration.mc_id,
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
            None => {}
        }

        self.state
            .registered_meetings
            .insert(meeting_id.clone(), registration);

        // Drain pending connections for this meeting
        let pending = self
            .state
            .pending_connections
            .remove(&meeting_id)
            .unwrap_or_default();

        // Promote pending connections to active
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
            });
        }

        // Notify any waiters that the meeting is now registered
        if let Some(notifier) = self.state.meeting_notifiers.get(&meeting_id) {
            notifier.notify_waiters();
        }

        pending
    }

    fn handle_add_connection(&mut self, meeting_id: String, entry: ConnectionEntry) {
        let meeting_conns = self.state.active_connections.entry(meeting_id).or_default();
        let participant_conns = meeting_conns
            .entry(entry.participant_id.clone())
            .or_default();
        participant_conns.push(entry);
    }

    fn handle_remove_connection(&mut self, meeting_id: &str, connection_id: &str) -> bool {
        if let Some(meeting_conns) = self.state.active_connections.get_mut(meeting_id) {
            for participant_conns in meeting_conns.values_mut() {
                if let Some(pos) = participant_conns
                    .iter()
                    .position(|c| c.connection_id == connection_id)
                {
                    participant_conns.remove(pos);
                    return true;
                }
            }
        }
        false
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
                // Clean up empty entries
                if pending_list.is_empty() {
                    self.state.pending_connections.remove(meeting_id);
                }
                return true;
            }
        }
        false
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
}

impl SessionManagerHandle {
    /// Create a new `SessionManagerActor` and return a handle to it.
    ///
    /// Spawns the actor task immediately. The actor runs until all handles
    /// are dropped (channels close).
    #[must_use]
    pub fn new() -> Self {
        let (handle, actor) = Self::new_with_parts();
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
    pub fn new_with_parts() -> (Self, SessionManagerActor) {
        let (sender, receiver) = mpsc::channel(SESSION_CHANNEL_BUFFER);
        let (config_sender, config_receiver) = mpsc::channel(CONFIG_APPLY_CHANNEL_BUFFER);
        let routing = Arc::new(RoutingTable::new());
        let actor = SessionManagerActor::new(receiver, config_receiver, Arc::clone(&routing));
        (
            Self {
                sender,
                config_sender,
                routing,
            },
            actor,
        )
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
    /// connections that were waiting for this meeting's registration,
    /// so the caller can dispatch notifications.
    pub async fn register_meeting(
        &self,
        meeting_id: String,
        registration: MeetingRegistration,
    ) -> Vec<PendingConnection> {
        let (tx, rx) = oneshot::channel();
        if self
            .sender
            .send(SessionMessage::RegisterMeeting {
                meeting_id,
                registration,
                respond_to: tx,
            })
            .await
            .is_err()
        {
            tracing::warn!(target: "mh.session", "SessionManagerActor channel closed on register_meeting");
            return Vec::new();
        }
        rx.await.unwrap_or_default()
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

impl Default for SessionManagerHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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
        }
    }

    fn make_pending(conn_id: &str, meeting_id: &str, participant_id: &str) -> PendingConnection {
        PendingConnection {
            connection_id: conn_id.to_string(),
            meeting_id: meeting_id.to_string(),
            participant_id: participant_id.to_string(),
            connected_at: Instant::now(),
        }
    }

    #[tokio::test]
    async fn test_register_meeting() {
        let handle = SessionManagerHandle::new();
        assert!(!handle.is_meeting_registered("meeting-1").await);

        let pending = handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(handle.is_meeting_registered("meeting-1").await);
        assert!(pending.is_empty());
        assert_eq!(
            handle.get_mc_endpoint("meeting-1").await.unwrap(),
            "http://mc:50052"
        );
    }

    #[tokio::test]
    async fn test_add_and_remove_connection() {
        let handle = SessionManagerHandle::new();
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

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
        let handle = SessionManagerHandle::new();

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
            )
            .await;

        assert_eq!(promoted.len(), 2);
        assert_eq!(promoted[0].connection_id, "conn-1");
        assert_eq!(promoted[1].connection_id, "conn-2");

        // Promoted connections should now be active
        assert_eq!(handle.active_connection_count().await, 2);
    }

    #[tokio::test]
    async fn test_pending_connection_for_different_meeting_not_promoted() {
        let handle = SessionManagerHandle::new();

        // Add pending for meeting-1
        let _notify = handle
            .add_pending_connection(make_pending("conn-1", "meeting-1", "user-1"))
            .await;

        // Register meeting-2 — should NOT promote meeting-1's pending
        let promoted = handle
            .register_meeting(
                "meeting-2".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

        assert!(promoted.is_empty());
        assert_eq!(handle.active_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_pending_connection() {
        let handle = SessionManagerHandle::new();

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
        let handle = SessionManagerHandle::new();

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
            )
            .await;

        let result = join_handle.await.unwrap();
        assert!(result, "Meeting should be registered after notify");
    }

    #[tokio::test]
    async fn test_registered_connection_not_affected_by_timeout() {
        let handle = SessionManagerHandle::new();

        // Register meeting first
        handle
            .register_meeting(
                "meeting-1".to_string(),
                make_registration("mc-1", "http://mc:50052"),
            )
            .await;

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
        let handle = SessionManagerHandle::new();
        assert!(handle.get_mc_endpoint("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_default_impl() {
        let handle = SessionManagerHandle::default();
        assert!(!handle.is_meeting_registered("any").await);
        assert_eq!(handle.active_connection_count().await, 0);
    }
}
