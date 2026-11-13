//! The meeting actor's media state: join-order slots and shared-handler edges,
//! each participant's OBSERVED handler connectivity, the per-handler push
//! workers, and every declared participant's last-emitted view (ADR-0036 §5,
//! §6, §8, §9; story 2 R-1..R-4, R-33).
//!
//! # Connectivity has one home: here
//!
//! Which handlers a participant is connected to is recorded ONLY by this actor,
//! from MH's `NotifyParticipantConnected` / `NotifyParticipantDisconnected`
//! (`grpc/media_coordination.rs`), in the same actor turn that resolves the
//! connection's `sender_id` — so the sender-binding answer and the routing input
//! cannot drift apart. There is no second store (the former
//! `MhConnectionRegistry` is gone), and client-reported `MediaConnectionUpdate`
//! statuses are never routing input. The per-participant state
//! machine, its keying by MH connection, the tombstone and the settle rule live
//! in `media_routing/connectivity.rs`. A settled connectivity change is a
//! structural change like a join or leave: same recompute, generation advance,
//! re-push and applied-echo machinery.
//!
//! **Lost Disconnecteds (ADR-0039 §6).** A Disconnected a LIVE handler never
//! delivered is healed by that handler's heartbeat: the remove-only reconcile
//! ([`MeetingMedia::retire_absent`]) retires keys absent from its
//! `connection_ids` that were bound before its previous heartbeat, through
//! the same routing sync. A CRASHED handler sends no heartbeat either: its
//! keys stay until handler-loss detection removes it from the set (MC-B2).
//!
//! # The actor is the single writer AND the single composer
//!
//! Membership, join order, slot demand and mute state all live on the meeting
//! actor, so the actor is the only place that can know when a participant's
//! view changed — including when the cause is SOMEONE ELSE's join, leave,
//! declaration or mute, for which that participant's own connection never runs
//! any code. After every structural change the actor:
//!
//! 1. renders the slot table into a per-handler forwarding snapshot;
//! 2. takes each handler's `policy_generation` (advances only when that
//!    handler's snapshot changed) and publishes it to that handler's push
//!    worker, which pushes and confirms MH's applied-generation echo; and
//! 3. marks the affected declared participants dirty and flushes their views.
//!
//! # The flush bound: per turn, deferring, never shedding
//!
//! A roster change in an N-party meeting can change N views, so filling a
//! meeting is O(N²) emissions and none of it is client-rate-limited. The actor
//! therefore takes at most [`SLOT_VIEW_FLUSH_BATCH`] dirty participants per
//! actor turn and DEFERS the rest to later turns, interleaved with mailbox
//! traffic. Nothing is dropped: a stale view means hearing the wrong person or
//! nobody, and the client has no message it could send to fix it. The dirty
//! structure is a SET keyed by participant with idempotent marking, so the
//! carried backlog is bounded by declared participants, never by event count,
//! and a burst of changes coalesces into one emission per participant carrying
//! the latest state. The retained view state is O(participants × declared
//! slots), bounded by the roster. See `media_signaling`'s module doc for when
//! this kind of bound is the right one.
//!
//! One accepted DISTINCT re-declaration costs a render, one `next_generation`
//! per handler, one publish per handler (coalesced by the worker's latest-wins
//! channel, so a re-declaration loop cannot drive a per-declaration MH RPC
//! rate) and a dirty-flush over declared participants — bounded upstream by the
//! per-connection declaration budget.
//!
//! # Only what changed, and only to declared participants
//!
//! Each declared participant's last emitted `SendDirective` and
//! `StreamAssignments` are retained; a flush emits a message only if it differs
//! (directive first). A participant that has not declared is sent nothing: the
//! declaration is the client's precondition for publishing, and an empty list
//! would read as "declared zero slots".
//!
//! # The meeting's handler set is MC's own (ADR-0039 §3)
//!
//! The handler set — ids, urls, refs and states, as ONE [`MeetingHandlers`]
//! value — is installed when the actor is CREATED, from GC's hand-off, and
//! never re-read: no join carries a handler set, so no join and no changed or
//! tampered Redis entry can add a handler to a live meeting or move an
//! already-placed participant. It changes only through its own mutators.
//!
//! # Register before token
//!
//! A handler is client-visible only once its push worker reports the first
//! acked `RegisterMeeting` ([`HandlerRegistration`], drained by the actor in
//! its own `select!` arm — the actor never awaits MH). Then
//! [`MeetingMedia::on_registration`] marks it Active, the actor sends the new
//! client snapshot to every connected participant, and only THEN re-routes,
//! so on each participant's ordered queue the set precedes any directive or
//! assignment that names the new ref.
//!
//! # No key material
//!
//! Sender ids, slot ids, handler ids and urls only. Nothing here reads the
//! meeting KEK; it stays in `MeetingKeyState`.

use super::messages::SignalingPayload;
use super::participant::ParticipantActorHandle;
use crate::errors::McError;
use crate::grpc::MhRegistrationClient;
use crate::media_admission::SenderId;
use crate::media_kind::PublishSet;
use crate::media_routing::pusher::{
    HandlerRegistration, RegistrationOutcome, RegistrationReporter,
};
use crate::media_routing::{
    assess, next_deadline, Acceptance, ClientHandlerSet, ConnectionKey, HandlerEndpoint,
    HandlerHealth, HandlerHeartbeat, HandlerId, HandlerLiveness, HandlerPusher, HealthConfig,
    HeartbeatAnomaly, MeetingAssignment, MeetingHandlers, Observations, Observed,
    ParticipantConnectivity, PolicyGenerations, PushJob, PushTarget, RefusalClass, RemovalTrigger,
    SetChange, SlotTable, SourceMute, Unapplied, WorkerDrain,
};
use crate::media_signaling::{
    build_send_directive, build_stream_assignments, DirectiveOutcome, MediaKindLabel,
    MediaStreamPolicy, SlotDemand, SlotViewEmission, SourceMuteView, WithdrawalReason,
};
use crate::observability::metrics;
use proto_gen::dark_tower::signaling::v1::{
    server_message, SendDirective, ServerMessage, StreamAssignments,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Dirty participants the actor flushes per actor turn.
///
/// A server work bound, not an operator lever: it moves latency only, never
/// correctness (deferred views are flushed on later turns, coalesced), so it is
/// a Rust constant in the register of `webtransport::connection`'s
/// `MUTE_WORK_BURST` rather than a ConfigMap key. Sized so a meeting at the
/// organisation participant cap flushes in two turns. Why this bound defers
/// where the other two deny or shed: `media_signaling` §"Bounding work on the
/// shared meeting actor".
pub const SLOT_VIEW_FLUSH_BATCH: usize = 64;

/// Non-member handler ids per meeting whose RELEASED verdict is WARNed once.
/// A log-volume bound, not a correctness one: past it a new id is counted on
/// `mc_media_handler_heartbeats_total` but not logged.
pub const RELEASE_NOTICE_DEDUPE_CAP: usize = 64;

/// The server-level dependencies media routing needs, built once by the
/// WebTransport server and shared by every meeting.
pub struct MediaRoutingDeps {
    /// MC→MH registration client.
    pub mh_client: Arc<dyn MhRegistrationClient>,
    /// Process-wide `policy_generation` registry (evicted on meeting teardown).
    pub policy_generations: Arc<PolicyGenerations>,
    /// This MC's id.
    pub mc_id: String,
    /// This MC's advertised gRPC endpoint, for MH→MC callbacks.
    pub mc_grpc_endpoint: String,
    /// The stream-number to media-kind/encoding table MC directs from.
    pub stream_policy: MediaStreamPolicy,
    /// The connect settle window (`MC_MEDIA_CONNECT_SETTLE_MS`), see
    /// `media_routing/connectivity.rs` for why it exists.
    pub connect_settle_window: Duration,
    /// N and its derivations (`MC_MH_HEARTBEAT_TIMEOUT_MS`, ADR-0039 §1): the
    /// interval every push worker sends MH.
    pub heartbeat: crate::media_routing::HeartbeatTiming,
    /// The `media_dial` credential TTL and its TTL/3 refresh
    /// (`MC_MEDIA_DIAL_TOKEN_TTL_SECONDS`, ADR-0039 §5).
    pub media_dial: crate::media_dial::MediaDialTiming,
    /// MC→GC replacement grants (`AcquireMediaHandler`, ADR-0039 §3).
    pub acquirer: Arc<dyn crate::grpc::gc_client::HandlerAcquirer>,
    /// The replacement pacing (`MC_MH_REPLACEMENT_MIN_INTERVAL_MS`,
    /// `MC_MH_REPLACEMENT_BACKOFF_MAX_MS`; story 4 MC-C).
    pub replacement: crate::media_routing::ReplacementTiming,
    /// The planned-transfer cutover margin (`MC_PLANNED_TRANSFER_MARGIN_MS`;
    /// story 4 MC-F).
    pub planned_transfer: crate::media_routing::PlannedTransferTiming,
}

impl std::fmt::Debug for MediaRoutingDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaRoutingDeps")
            .field("mc_id", &self.mc_id)
            .field("mc_grpc_endpoint", &self.mc_grpc_endpoint)
            .field("connect_settle_window", &self.connect_settle_window)
            .finish_non_exhaustive()
    }
}

/// A pod census keyed by meeting id: each meeting actor writes its ABSOLUTE
/// value, and the pod total is recomputed from the census and SET, never
/// moved by deltas (story 2 task 12's design, generalised for story 4 MC-B2).
///
/// # Recompute-and-set, never delta arithmetic
///
/// Nothing ever adds or subtracts, so there is no increment that can be
/// doubled and no exit-path decrement that an aborted actor can skip.
///
/// Two callers publish the pod total, and they are NOT redundant:
/// - the controller's `check_meeting_health` walk is the LOAD-BEARING one: it
///   prunes entries for meetings that no longer exist (a panicked actor never
///   reaches its own exit path) and re-sets the gauge, so any drift is erased on
///   the next pass;
/// - each change in the meeting also publishes, but that is FRESHNESS ONLY.
///
/// Delete the second and the gauge is stale but correct. Delete the first and
/// the drift class this design exists to prevent comes back — silently.
///
/// The map key is the meeting id ONLY — never a participant, sender or
/// handler id. A value equal to `V::default()` FORGETS the meeting (so a
/// meeting at zero holds no entry).
#[derive(Debug, Default)]
pub struct MeetingCensus<V> {
    entries: std::sync::Mutex<HashMap<String, V>>,
}

impl<V: Copy + Default + PartialEq> MeetingCensus<V> {
    /// Record `meeting_id`'s absolute value (the default forgets it).
    pub fn set(&self, meeting_id: &str, value: V) {
        let mut entries = self.lock();
        if value == V::default() {
            entries.remove(meeting_id);
        } else {
            entries.insert(meeting_id.to_string(), value);
        }
    }

    /// Forget every meeting `live` rejects.
    pub fn retain(&self, live: impl Fn(&str) -> bool) {
        self.lock().retain(|id, _| live(id));
    }

    /// Fold every recorded value (the pod total a `publish` sets).
    fn fold<T>(&self, init: T, f: impl Fn(T, &V) -> T) -> T {
        self.lock().values().fold(init, f)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, V>> {
        // A poisoned census still holds valid values (every write is a single
        // insert or remove), so recover rather than propagate a panic into
        // every meeting that writes it.
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The pod's census of server-muted participants, per meeting — the source
/// `mc_media_server_muted_sources` is recomputed from (story 2 task 12; closes
/// the "server mute is never observable as STATE" gap). Freshness writes come
/// from every mute change; the gauge is the one read against MH's
/// `server_muted` drops.
///
/// # Per (meeting, kind) — story 3 OBS-A
///
/// Each meeting holds one count PER KIND, a `[u64; 2]` indexed by
/// [`MediaKindLabel::index`] (built with [`MediaKindLabel::per_kind`]).
/// [`MeetingCensus::publish`] sets EVERY kind in [`MediaKindLabel::ALL`] on
/// every call, zero included: setting only the kinds present in the map would
/// leave the video gauge stuck at its last non-zero value after the last
/// video-muted meeting ends — exactly the drift this recompute design exists
/// to prevent.
pub type MutedSourceCensus = MeetingCensus<[u64; 2]>;

impl MeetingCensus<[u64; 2]> {
    /// Recompute the pod total PER KIND from the census and publish every kind.
    pub fn publish(&self) {
        let totals = self.totals();
        for kind in MediaKindLabel::ALL {
            metrics::set_server_muted_sources(kind.labels(), kind.of(totals));
        }
    }

    /// The pod totals per kind, indexed by [`MediaKindLabel::index`].
    fn totals(&self) -> [u64; 2] {
        MediaKindLabel::per_kind(|k| self.fold(0u64, |t, c| t.saturating_add(k.of(*c))))
    }
}

/// The pod's census of meetings whose handler set has NO member — "awaiting
/// handler" (ADR-0039 §3) — the source `mc_media_meetings_without_handlers` is
/// recomputed from. `true` = the meeting's set is empty. A meeting whose only
/// members are still Registering does NOT count (a handler is on its way).
/// Freshness writes come from every handler-set membership change.
pub type HandlerlessCensus = MeetingCensus<bool>;

impl MeetingCensus<bool> {
    /// Recompute the pod count from the census and publish it.
    pub fn publish(&self) {
        metrics::set_meetings_without_handlers(self.fold(0u64, |n, empty| {
            if *empty {
                n.saturating_add(1)
            } else {
                n
            }
        }));
    }
}

/// What a join carries for media: the server-level dependencies and the
/// joiner's validated media declaration. NOT a handler set: the meeting's set
/// is installed at actor creation (ADR-0039 §3) and no join can alter it.
#[derive(Debug, Clone)]
pub struct JoinMedia {
    /// Server-level dependencies.
    pub deps: Arc<MediaRoutingDeps>,
    /// What the joiner publishes (fixed for its membership, story 3 R-2) and
    /// each published kind's initial client mute: the parsed join
    /// `published_kinds`.
    pub declared: crate::media_kind::ClientMediaStates,
    /// The joiner's initial receive declaration, validated and truncated at
    /// the connection (`JoinRequest.receive_capability`; absent = zero slots).
    pub demand: SlotDemand,
}

/// Test-only construction overrides for a meeting actor. **Test builds only.**
///
/// Compiled only under the non-default `test-seams` feature, which fails to
/// compile without `debug_assertions` (see `lib.rs`). The TYPE is gated, not
/// its fields, so a default build has nothing here to extend.
#[cfg(feature = "test-seams")]
#[derive(Debug, Clone, Default)]
pub struct MeetingSeams {
    /// Pre-seed the `sender_id` allocator cursor (`Some`) — the exhaustion
    /// guard's bypass. `None` means a fresh allocator.
    pub sender_id_cursor: Option<Option<std::num::NonZeroU16>>,
    /// Override [`SLOT_VIEW_FLUSH_BATCH`] (e.g. 1, to observe deferral).
    pub slot_view_flush_batch: Option<usize>,
    /// Where a post-reset allocator's cursor starts, instead of 1. Lets a test
    /// place a KEK-epoch reset's exclusion snapshot in the cursor's path — the
    /// only way to exercise "an excluded id is never reissued in its epoch"
    /// through the real join path without 65,535 joins.
    pub epoch_reset_cursor: Option<std::num::NonZeroU16>,
    /// Never auto-fire the leave-debounced rotation; tests drive it with
    /// `MeetingMessage::ForceKekRotation` once the removals are recorded, so
    /// coalescing holds structurally instead of racing a real wall-clock W.
    pub kek_debounce_manual: bool,
    /// Seed the `media_dial` refresh jitter (`None`: OS-seeded, as production).
    pub dial_refresh_seed: Option<u64>,
    /// Stamp `media_dial` `iat` from a clock anchored to tokio time at this
    /// UNIX second, so paused-time tests move `iat` with the refresh arm
    /// (`None`: the wall clock, as production).
    pub dial_clock_base_unix: Option<i64>,
    /// Seed the replacement backoff jitter (`None`: OS-seeded, as production).
    pub replacement_seed: Option<u64>,
}

/// The handler set in-crate actor tests create a meeting with: one handler,
/// as GC's hand-off would describe it (Registering until its push is acked).
#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) fn test_handlers() -> MeetingHandlers {
    MeetingHandlers::from_handoff([crate::media_routing::HandlerEndpoint {
        id: crate::media_routing::HandlerId::new("mh-0"),
        webtransport_url: "https://mh-0.test:4434".to_string(),
        grpc_endpoint: "http://mh-0.test:50053".to_string(),
    }])
    .unwrap()
}

/// Media-routing inputs for in-crate actor tests: an MH client that confirms
/// every push. Routing behaviour itself is covered by
/// `media_routing` unit tests and the integration suite.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) fn test_join_media() -> JoinMedia {
    use crate::grpc::{MeetingProgramming, MhRegistrationClient};
    use crate::media_routing::PolicyGenerations;
    use crate::media_signaling::{AudioEncoding, MediaStreamPolicy, VideoEncoding};
    use proto_gen::dark_tower::signaling::v1::Codec;

    struct ConfirmingMh;
    impl MhRegistrationClient for ConfirmingMh {
        fn register_meeting<'a>(
            &'a self,
            _programming: &'a MeetingProgramming<'a>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), McError>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }

        fn end_meeting<'a>(
            &'a self,
            _call: &'a crate::media_routing::teardown::EndMeetingCall,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<(), crate::media_routing::teardown::EndMeetingFailure>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    /// A GC with no capacity: nothing is ever granted.
    struct NoCapacityGc;
    impl crate::grpc::gc_client::HandlerAcquirer for NoCapacityGc {
        fn acquire<'a>(
            &'a self,
            _call: &'a crate::grpc::gc_client::AcquireCall,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = crate::grpc::gc_client::AcquireResult> + Send + 'a,
            >,
        > {
            Box::pin(async {
                crate::grpc::gc_client::AcquireResult::NoCapacity {
                    message: "fixture".to_string(),
                }
            })
        }

        fn max_exclude_handler_ids(&self) -> Option<std::num::NonZeroU32> {
            std::num::NonZeroU32::new(16)
        }
    }

    JoinMedia {
        deps: Arc::new(MediaRoutingDeps {
            mh_client: Arc::new(ConfirmingMh),
            policy_generations: Arc::new(PolicyGenerations::new()),
            mc_id: "mc-test".to_string(),
            mc_grpc_endpoint: "http://mc-test:50052".to_string(),
            stream_policy: MediaStreamPolicy::new(
                AudioEncoding::new(Codec::Opus, 48_000, 50).unwrap(),
                VideoEncoding::new(Codec::Vp8, 640, 360, 30, 800_000, 500).unwrap(),
            ),
            connect_settle_window: std::time::Duration::from_millis(1500),
            heartbeat: crate::media_routing::HeartbeatTiming::from_timeout_ms(6_000),
            media_dial: crate::media_dial::MediaDialTiming::from_ttl_seconds(300),
            acquirer: Arc::new(NoCapacityGc),
            replacement: crate::media_routing::ReplacementTiming::from_config_ms(1_000, 30_000),
            planned_transfer: crate::media_routing::PlannedTransferTiming::from_margin_ms(5_000),
        }),
        declared: crate::media_kind::ClientMediaStates::live(PublishSet::of(&[
            MediaKindLabel::Audio,
        ])),
        demand: crate::media_signaling::ReceiveCapabilityDeclaration::empty().truncate([8, 4]),
    }
}

/// Which declared participants a structural change may have affected.
pub(super) enum Affected {
    /// Every declared participant (join, leave, declaration, reconnect).
    All,
    /// The subscribers holding this source in a slot of EITHER kind (its
    /// CLIENT mute changed: the publisher's own directive never depends on
    /// client mute, so the publisher is not dirtied).
    HoldersOf(SenderId),
    /// The publisher itself AND the subscribers holding it (its SERVER mute
    /// changed: the publisher's directive is withdrawn or restored for the
    /// moved kind, and its holders' slot states change).
    PublisherAndHoldersOf(SenderId),
}

/// One roster participant, as a flush needs it.
pub(super) struct RosterEntry<'a> {
    /// The participant's sender id.
    pub sender: SenderId,
    /// Its live connection, if connected.
    pub connection: Option<&'a ParticipantActorHandle>,
    /// Its fixed publish set (the directive lists exactly these kinds).
    pub publishes: PublishSet,
    /// Its per-kind mute flags, client and server kept apart.
    pub mute: SourceMute,
}

/// One participant's declaration, observed connectivity and last-emitted view.
struct View {
    sender: SenderId,
    connectivity: ParticipantConnectivity,
    /// The truncated declaration in force. Set at join (a join always carries
    /// one; absent on the wire = zero slots).
    declaration: Option<SlotDemand>,
    last_directive: Option<SendDirective>,
    last_assignments: Option<StreamAssignments>,
}

/// One assessment of one member: the actor acts only when `previous` and
/// `next` differ.
pub(super) struct HealthStep {
    /// The member.
    pub handler_id: HandlerId,
    /// Its health before.
    pub previous: HandlerHealth,
    /// Its health now.
    pub next: HandlerHealth,
    /// What it was judged on (bounded scalars for the transition log).
    pub observations: Observations,
}

/// How many routable participants a planned transfer's target covers
/// ([`MeetingMedia::coverage`]; story 4 MC-F).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Coverage {
    /// Participants holding a live connection key on any member.
    pub routable: usize,
    /// Routable participants on none of the targets.
    pub stragglers: usize,
}

impl Coverage {
    /// Every routable participant is on a target.
    pub(crate) fn is_complete(self) -> bool {
        self.stragglers == 0
    }
}

/// What [`MeetingMedia::remove_handler`] took out of the meeting.
pub(super) struct RemovedHandler {
    /// The removed handler (for its `EndMeeting`).
    pub endpoint: HandlerEndpoint,
    /// The set change (new version; whether the client-visible set changed).
    pub set_change: SetChange,
    /// Its push worker, moved OUT so the release can drain exactly this one.
    /// Load-bearing for the ordering: no render can publish to it any more.
    pub pusher: Option<HandlerPusher>,
    /// Connectivity keys purged across all participants.
    pub forgotten_keys: usize,
}

/// The meeting actor's media state.
pub(super) struct MeetingMedia {
    meeting_id: String,
    /// The push workers' OWN cancellation token — deliberately NOT a child of
    /// the actor's cancel token.
    ///
    /// Push workers live as long as the MEETING, never a connection. But a
    /// meeting teardown must DRAIN them (every issued `RegisterMeeting` has
    /// RETURNED before `EndMeeting` is called — `EndMeetingRequest`'s quiesce
    /// MUST), and a worker hanging off the actor token would be cancelled by the
    /// very `cancel()` that ends the actor, abandoning an in-flight attempt that
    /// can then land after the release and re-create the ended meeting. So the
    /// actor's exit path moves this token out, together with the workers, into
    /// the teardown ([`Self::take_teardown`]), which cancels it and then awaits
    /// each worker.
    ///
    /// An actor that dies WITHOUT reaching its exit path (a panic) still stops
    /// the workers: dropping this struct drops each `HandlerPusher`, whose
    /// `Drop` aborts — correct there, because nothing will call `EndMeeting`.
    pushers_cancel: CancellationToken,
    deps: Option<Arc<MediaRoutingDeps>>,
    /// The meeting's MC-owned set, installed at actor creation.
    handlers: MeetingHandlers,
    /// Push workers report registration verdicts here (each holds a clone of
    /// the sender in its `RegistrationReporter`).
    registrations_tx: mpsc::UnboundedSender<HandlerRegistration>,
    /// Drained by the actor's own `select!` arm.
    registrations_rx: mpsc::UnboundedReceiver<HandlerRegistration>,
    slots: SlotTable,
    pushers: BTreeMap<HandlerId, HandlerPusher>,
    views: HashMap<String, View>,
    dirty: BTreeSet<String>,
    flush_batch: usize,
    /// The latest render, shared by the push and the flush of one change.
    assignment: Option<MeetingAssignment>,
    /// Per-handler heartbeat liveness (ADR-0039 §2), beside — never inside —
    /// the handler set so a liveness write never moves `handler_set_version`.
    /// An entry exists only for a member, and [`Self::remove_handler`] (the
    /// ONE removal path) drops it with the member.
    liveness: BTreeMap<HandlerId, HandlerLiveness>,
    /// Per-handler health (ADR-0039 §2), beside the liveness record and
    /// dropped with it by [`Self::remove_handler`]. Absent = Live/healthy.
    health: BTreeMap<HandlerId, HandlerHealth>,
    /// In-flight handler-removal releases (`teardown::release_removed`),
    /// handed to the meeting's teardown so its fence covers them. Bounded by
    /// the meeting's removed set (removal is one-way), so never pruned.
    removals: Vec<tokio::task::JoinHandle<()>>,
    /// Non-member handler ids whose RELEASED verdict was already WARNed
    /// (bounded; dropped with the meeting).
    release_noticed: BTreeSet<String>,
    /// The PUBLIC half of the meeting's `media_dial` key (ADR-0039 §5), copied
    /// into every push worker's `RegisterMeeting`. The private half is the
    /// meeting actor's own field and never reaches this struct.
    media_dial_public_key: [u8; 32],
}

impl MeetingMedia {
    pub(super) fn new(
        meeting_id: String,
        handlers: MeetingHandlers,
        media_dial_public_key: [u8; 32],
    ) -> Self {
        let (registrations_tx, registrations_rx) = mpsc::unbounded_channel();
        Self {
            slots: SlotTable::for_meeting(&meeting_id),
            meeting_id,
            pushers_cancel: CancellationToken::new(),
            deps: None,
            handlers,
            registrations_tx,
            registrations_rx,
            pushers: BTreeMap::new(),
            views: HashMap::new(),
            dirty: BTreeSet::new(),
            flush_batch: SLOT_VIEW_FLUSH_BATCH,
            assignment: None,
            liveness: BTreeMap::new(),
            health: BTreeMap::new(),
            removals: Vec::new(),
            release_noticed: BTreeSet::new(),
            media_dial_public_key,
        }
    }

    /// The `media_dial` timing, once a join has installed the routing deps.
    pub(super) fn media_dial_timing(&self) -> Option<crate::media_dial::MediaDialTiming> {
        self.deps.as_ref().map(|d| d.media_dial)
    }

    /// The per-turn flush bound (shared by the slot-view and handler-set
    /// flushes).
    pub(super) fn flush_batch(&self) -> usize {
        self.flush_batch
    }

    #[cfg(feature = "test-seams")]
    pub(super) fn apply_seams(&mut self, seams: &MeetingSeams) {
        if let Some(batch) = seams.slot_view_flush_batch {
            self.flush_batch = batch.max(1);
        }
    }

    /// Install the join's server-level routing dependencies (the first join's
    /// are kept; every join carries the same server-wide value).
    pub(super) fn install(&mut self, media: &JoinMedia) {
        if self.deps.is_none() {
            self.deps = Some(Arc::clone(&media.deps));
        }
    }

    /// Admit a participant into slot state. It is not routable until MH
    /// reports its connections and they settle, so nothing moves yet.
    ///
    /// # Errors
    ///
    /// [`McError::Internal`] if the slot table refuses the admission.
    pub(super) fn admit(
        &mut self,
        participant_id: &str,
        sender: SenderId,
        publishes: PublishSet,
    ) -> Result<(), McError> {
        self.slots
            .admit(sender, publishes)
            .map_err(|e| McError::Internal(format!("slot admission failed: {e}")))?;
        self.views.insert(
            participant_id.to_string(),
            View {
                sender,
                connectivity: ParticipantConnectivity::default(),
                declaration: None,
                last_directive: None,
                last_assignments: None,
            },
        );
        Ok(())
    }

    /// The routing deps, once a join installed them.
    pub(super) fn deps(&self) -> Option<&Arc<MediaRoutingDeps>> {
        self.deps.as_ref()
    }

    /// Add a GC-granted replacement as Registering (ADR-0039 §3): from here
    /// its heartbeats are OWNED and the binding gate admits it; it is NOT
    /// client-visible until its first `RegisterMeeting` is acked. The caller
    /// re-renders, which spawns its push worker through the ONE
    /// `PushTarget` builder (`render_and_publish`).
    ///
    /// # Errors
    ///
    /// Any [`MeetingHandlers::add`] refusal; nothing changes.
    pub(super) fn add_granted(
        &mut self,
        endpoint: HandlerEndpoint,
    ) -> Result<SetChange, crate::media_routing::HandlerSetError> {
        self.handlers.add(endpoint).map(|(_, change)| change)
    }

    /// The participants admitted right now (a reconnect phase waits on these).
    pub(super) fn admitted(&self) -> std::collections::BTreeSet<String> {
        self.views.keys().cloned().collect()
    }

    /// Whether `participant_id` has left, or holds a live connection key on
    /// `handler` (its reconnect wait is over either way).
    pub(super) fn reconnected_or_gone(&self, handler: &HandlerId, participant_id: &str) -> bool {
        self.on_any_or_gone(participant_id, std::slice::from_ref(handler))
    }

    /// THE "is this participant on these handlers" read over the ONE
    /// connectivity home: whether `participant_id` has left, or holds a live
    /// MH-reported connection key on at least one of `handlers`. The reconnect
    /// phase (one handler) and the planned-transfer coverage (a target set)
    /// both read it.
    pub(super) fn on_any_or_gone(&self, participant_id: &str, handlers: &[HandlerId]) -> bool {
        self.views.get(participant_id).is_none_or(|v| {
            handlers
                .iter()
                .any(|h| v.connectivity.keys_on(h).next().is_some())
        })
    }

    /// Whether member `id` has reported DRAINING — read ONLY through
    /// [`crate::media_routing::HandlerHealth::is_draining`] (one reader).
    fn is_draining(&self, id: &HandlerId) -> bool {
        self.health.get(id).is_some_and(HandlerHealth::is_draining)
    }

    /// THE non-draining filter's Active half: the members a planned transfer
    /// may target (Active, not DRAINING; removed members are not members).
    /// Registering members are deliberately NOT here — no client holds a token
    /// for them, so nobody can be on them (register before token).
    pub(super) fn non_draining_active(&self) -> Vec<HandlerId> {
        self.handlers
            .ids()
            .filter(|id| self.handlers.active_endpoint(id).is_some() && !self.is_draining(id))
            .cloned()
            .collect()
    }

    /// THE non-draining filter's count for the replacement target accounting
    /// (`ReplacementState::on_removal`): every member that is not DRAINING,
    /// Registering grants INCLUDED (a grant on its way counts toward the
    /// target, unlike for eligibility above — deliberate). Every removal path
    /// passes this.
    pub(super) fn non_draining_member_count(&self) -> usize {
        self.handlers
            .ids()
            .filter(|id| !self.is_draining(id))
            .count()
    }

    /// Planned-transfer coverage against `targets`. A routable participant
    /// holds at least one live connection key on any member (one that has not
    /// connected media anywhere is not waited on); a straggler is routable and
    /// on none of `targets`.
    pub(super) fn coverage(&self, targets: &[HandlerId]) -> Coverage {
        let mut coverage = Coverage::default();
        for (participant_id, view) in &self.views {
            if view.connectivity.observed().next().is_none() {
                continue;
            }
            coverage.routable += 1;
            if !self.on_any_or_gone(participant_id, targets) {
                coverage.stragglers += 1;
            }
        }
        coverage
    }

    /// The meeting's handler set (internal view).
    pub(super) fn handlers(&self) -> &MeetingHandlers {
        &self.handlers
    }

    /// The client-facing snapshot (Active only) — the ONLY source of every
    /// `MediaHandlerSet`.
    pub(super) fn client_snapshot(&self) -> ClientHandlerSet {
        self.handlers.client_snapshot()
    }

    /// The registration-report receiver, for the actor's `select!` arm.
    pub(super) fn registrations(&mut self) -> &mut mpsc::UnboundedReceiver<HandlerRegistration> {
        &mut self.registrations_rx
    }

    /// Apply one push worker's registration report. Returns whether the
    /// client-visible (Active) set changed — the emission rule: the caller
    /// then sends the new snapshot to every connected participant BEFORE it
    /// re-routes ([`Self::reroute_all`]).
    ///
    /// A late or stale report (a removed incarnation, an already-Active
    /// handler) changes nothing.
    pub(super) fn on_registration(&mut self, report: &HandlerRegistration) -> bool {
        if report.outcome != RegistrationOutcome::Registered {
            return false;
        }
        // The first confirmation: the origin of the health function's silence
        // clock (stamped at the actor — later than the worker's ack, so it
        // errs toward a later Dead, never an earlier one).
        match self
            .handlers
            .mark_active(&report.handler_id, report.handler_ref, Instant::now())
        {
            Ok(Some(change)) => {
                info!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    handler_id = %report.handler_id,
                    handler_ref = report.handler_ref.get(),
                    handler_set_version = change.version,
                    "Media handler registered; handler set membership changed"
                );
                change.active_changed
            }
            Ok(None) => false,
            Err(e) => {
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    handler_id = %report.handler_id,
                    error = %e,
                    "Could not mark a registered media handler Active; it stays \
                     client-invisible. MC-internal invariant violation: capture and escalate."
                );
                false
            }
        }
    }

    /// Re-evaluate every participant's routable set after a membership change
    /// (routing is over Active members only). Returns whether any changed.
    pub(super) fn reroute_all(&mut self, now: Instant) -> bool {
        let ids: Vec<String> = self.views.keys().cloned().collect();
        let mut changed = false;
        for participant_id in ids {
            changed |= self.sync_routing(&participant_id, now);
        }
        changed
    }

    /// Record an MH-reported connection for a resolved roster participant.
    /// Returns whether routing changed (the caller then reconciles).
    ///
    /// # Errors
    ///
    /// The [`Unapplied`] reason; nothing is recorded on error.
    pub(super) fn media_connected(
        &mut self,
        participant_id: &str,
        wire_handler_id: &str,
        key: ConnectionKey,
        now: Instant,
    ) -> Result<bool, Unapplied> {
        let Some(deps) = &self.deps else {
            return Err(self.invariant_violation("connect", "no routing deps"));
        };
        // S1a: exact-match membership in the meeting's set, or nothing.
        let Some(endpoint) = self.handlers.resolve(wire_handler_id).cloned() else {
            return Err(Unapplied::HandlerNotInSet);
        };
        let window = deps.connect_settle_window;
        let Some(view) = self.views.get_mut(participant_id) else {
            return Err(self.invariant_violation("connect", "roster participant has no view"));
        };
        view.connectivity.connect(&endpoint, key, now, window)?;
        Ok(self.sync_routing(participant_id, now))
    }

    /// Record an MH-reported disconnection. Returns whether routing changed.
    ///
    /// Scoped to THIS participant's keys (security S12): there is no lookup
    /// across participants, so a key another participant holds is simply
    /// unknown here.
    ///
    /// # Errors
    ///
    /// The [`Unapplied`] reason; nothing is changed on error.
    pub(super) fn media_disconnected(
        &mut self,
        participant_id: &str,
        wire_handler_id: &str,
        key: &ConnectionKey,
        now: Instant,
    ) -> Result<bool, Unapplied> {
        let Some(handler) = self.handlers.resolve(wire_handler_id).map(|h| h.id.clone()) else {
            return Err(Unapplied::HandlerNotInSet);
        };
        let Some(view) = self.views.get_mut(participant_id) else {
            return Err(self.invariant_violation("disconnect", "roster participant has no view"));
        };
        view.connectivity.disconnect(&handler, key)?;
        Ok(self.sync_routing(participant_id, now))
    }

    /// The ONE home for a handler notification that reached this actor for a
    /// roster-resolved participant but found state MC cannot legitimately be
    /// in: a participant is admitted only by a join that installed the routing
    /// deps, and a view exists exactly
    /// while the participant is on the roster (both change in the same actor
    /// turn). The genuine join race is caught upstream (`resolve_user` →
    /// `NotFound`), so these are NOT the routine `participant_unknown` race —
    /// the counter reason stays `participant_unknown` (fail closed; no
    /// vocabulary for a can't-happen branch), and this loud line is what
    /// distinguishes it (security S-2).
    fn invariant_violation(&self, event: &'static str, state: &'static str) -> Unapplied {
        error!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            event,
            state,
            "Handler notification for a roster participant found impossible media state; \
             nothing recorded (counted as participant_unknown). This is an MC-internal \
             invariant violation: capture and escalate."
        );
        Unapplied::ParticipantUnknown
    }

    /// Settle every establishing participant that is due. Returns whether any
    /// participant's routing changed.
    pub(super) fn settle_due(&mut self, now: Instant) -> bool {
        let due: Vec<String> = self
            .views
            .iter()
            .filter(|(_, v)| {
                v.connectivity
                    .next_wake(self.handler_count())
                    .is_some_and(|w| w <= now)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut changed = false;
        for participant_id in due {
            changed |= self.sync_routing(&participant_id, now);
        }
        changed
    }

    /// The earliest instant an establishing participant could settle.
    pub(super) fn next_settle_wake(&self) -> Option<Instant> {
        let count = self.handler_count();
        self.views
            .values()
            .filter_map(|v| v.connectivity.next_wake(count))
            .min()
    }

    /// The handlers a participant can be routed over: the Active members
    /// (settle completes early once a participant reaches all of them).
    fn handler_count(&self) -> usize {
        self.handlers.active_len()
    }

    /// Settle this participant if due, and push its routable set into the slot
    /// table if it differs from what the table holds. Returns whether it did.
    fn sync_routing(&mut self, participant_id: &str, now: Instant) -> bool {
        let count = self.handler_count();
        let handlers = &self.handlers;
        let Some(view) = self.views.get_mut(participant_id) else {
            return false;
        };
        if let Some(outcome) = view.connectivity.poll(now, count) {
            metrics::record_connect_settle(outcome);
        }
        let routable = view.connectivity.routable(handlers);
        let sender = view.sender;
        let phase = view.connectivity.phase();
        let observed: Vec<String> = view
            .connectivity
            .observed()
            .map(ToString::to_string)
            .collect();
        if self.slots.connected_of(sender) == routable.as_ref() {
            return false;
        }
        self.slots.set_connectivity(sender, routable);
        // Current state for the operator, keyed by participant (never by sender
        // id, ADR-0036 §11): its observed handlers and its phase. The latest
        // line per participant is its connectivity now.
        info!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            connected_mh_ids = %observed.join(","),
            phase = phase.label(),
            "Participant media connectivity changed"
        );
        true
    }

    /// Record a heartbeat from a MEMBER as liveness (ADR-0039 §2): the first
    /// one creates the pair's record; later ones are accepted iff not stale.
    /// The watermark is clamped to `sent` (an ahead heartbeat is liveness,
    /// never a poisoned watermark).
    /// `member` was resolved by `MeetingHandlers::heartbeat_member`, so this
    /// cannot miss. Anomalies are noted separately ([`Self::note_anomalies`]),
    /// after the record exists.
    pub(super) fn observe_heartbeat(
        &mut self,
        member: &HandlerId,
        heartbeat: &HandlerHeartbeat,
        receipt: Instant,
        sent: Option<NonZeroU64>,
    ) -> Observed {
        match self.liveness.entry(member.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                entry.into_mut().observe(heartbeat, receipt, sent)
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(HandlerLiveness::first(heartbeat, receipt, sent));
                Observed {
                    acceptance: Acceptance::Accepted { previous: None },
                    transition: true,
                }
            }
        }
    }

    /// The contract fault a member's RECORDED evidence carries right now,
    /// through the ONE refusal gate (`Observations::effective_refusal`): a
    /// refusal with MC's policy installed concerns a policy MC never sent and
    /// is no contract fault.
    pub(super) fn contract_fault_now(
        &self,
        member: &HandlerId,
    ) -> Option<crate::media_routing::handler_health::ContractFault> {
        self.observations(member)
            .effective_refusal()
            .contract_fault()
    }

    /// Note this heartbeat's anomalies on the member's record; returns those
    /// NEW for the pair (the only ones logged).
    pub(super) fn note_anomalies(
        &mut self,
        member: &HandlerId,
        anomalies: &[HeartbeatAnomaly],
    ) -> Vec<HeartbeatAnomaly> {
        self.liveness
            .get_mut(member)
            .map_or_else(Vec::new, |record| record.note_anomalies(anomalies))
    }

    /// The spans `assess` judges against, once a join installed the deps.
    fn health_config(&self) -> Option<HealthConfig> {
        self.deps
            .as_ref()
            .map(|d| HealthConfig::from_timing(d.heartbeat))
    }

    /// What the actor knows about one member, for `assess`.
    fn observations(&self, id: &HandlerId) -> Observations {
        let record = self.liveness.get(id);
        Observations {
            confirmed_at: self.handlers.confirmed_at(id),
            last_accepted: record.map(|l| l.last_accepted),
            applied: record.map(|l| l.applied_generation),
            owed: record.and_then(|l| l.owed_generation),
            refusal: record.map_or(RefusalClass::None, |l| {
                RefusalClass::from_wire(l.apply_refusal)
            }),
            draining: record.is_some_and(|l| {
                crate::media_routing::handler_health::is_draining(l.serving_state)
            }),
            // Weight 0 in this story: MC-E wires the client loss census here.
            client_loss_reports: 0,
        }
    }

    /// Assess ONE member at `now` and store the result. `None` for a
    /// non-member, or before a join installed the deps (nothing confirmed).
    pub(super) fn assess_handler(&mut self, id: &HandlerId, now: Instant) -> Option<HealthStep> {
        let cfg = self.health_config()?;
        self.handlers.get(id)?;
        let observations = self.observations(id);
        let previous = self.health.get(id).copied().unwrap_or_default();
        let next = assess(now, &observations, &previous, &cfg);
        self.health.insert(id.clone(), next);
        Some(HealthStep {
            handler_id: id.clone(),
            previous,
            next,
            observations,
        })
    }

    /// Assess every member at `now` (the deadline arm).
    pub(super) fn assess_all(&mut self, now: Instant) -> Vec<HealthStep> {
        let ids: Vec<HandlerId> = self.handlers.ids().cloned().collect();
        ids.iter()
            .filter_map(|id| self.assess_handler(id, now))
            .collect()
    }

    /// The earliest instant any member's health could change without a
    /// heartbeat — the deadline arm's wake. Every value is strictly after the
    /// assessment that produced it (`handler_health::expiry`), so a fired
    /// wake re-arms in the future, never at the instant it fired.
    pub(super) fn next_health_wake(&self) -> Option<Instant> {
        let cfg = self.health_config()?;
        self.handlers
            .ids()
            .filter_map(|id| {
                let health = self.health.get(id).copied().unwrap_or_default();
                next_deadline(&health, &self.observations(id), &cfg)
            })
            .min()
    }

    /// Re-send a member's CURRENT snapshot to its push worker (the Lagging
    /// re-publish). A removed handler has no worker here, so this can never
    /// reach one.
    pub(super) fn republish(&self, id: &HandlerId) {
        if let Some(pusher) = self.pushers.get(id) {
            pusher.republish();
        }
    }

    /// Whether this is the FIRST non-OWNED verdict for `wire_handler_id` in
    /// this meeting (the once-per-(meeting, handler) WARN dedupe). Bounded by
    /// [`RELEASE_NOTICE_DEDUPE_CAP`]: past it, new ids are not remembered and
    /// not logged (the counter still carries the rate).
    pub(super) fn first_release_notice(&mut self, wire_handler_id: &str) -> bool {
        if self.release_noticed.contains(wire_handler_id)
            || self.release_noticed.len() >= RELEASE_NOTICE_DEDUPE_CAP
        {
            return false;
        }
        self.release_noticed.insert(wire_handler_id.to_string())
    }

    /// Whether `wire_handler_id`'s RELEASED WARN was already emitted.
    #[cfg(feature = "test-seams")]
    pub(super) fn release_noticed(&self, wire_handler_id: &str) -> bool {
        self.release_noticed.contains(wire_handler_id)
    }

    /// A member's health state label (test probe). `None` for a non-member.
    #[cfg(feature = "test-seams")]
    pub(super) fn health_label(&self, wire_handler_id: &str) -> Option<&'static str> {
        let id = &self.handlers.resolve(wire_handler_id)?.id;
        Some(
            self.health
                .get(id)
                .copied()
                .unwrap_or_default()
                .state_label(),
        )
    }

    /// The liveness record of a member (test probe).
    #[cfg_attr(
        not(feature = "test-seams"),
        expect(
            dead_code,
            reason = "test-seams probe only; assess reads the map directly"
        )
    )]
    pub(super) fn liveness(&self, wire_handler_id: &str) -> Option<&HandlerLiveness> {
        let id = &self.handlers.resolve(wire_handler_id)?.id;
        self.liveness.get(id)
    }

    /// The generation MC last SENT this handler (the lag's minuend), or
    /// `None` if MC has never pushed to it.
    pub(super) async fn sent_generation(&self, member: &HandlerId) -> Option<NonZeroU64> {
        let deps = self.deps.as_ref()?;
        deps.policy_generations
            .current(&self.meeting_id, member)
            .await
    }

    /// The remove-only reconcile over every participant's connectivity keys on
    /// ONE handler (ADR-0039 §6; predicate `liveness::retirable`). Returns the
    /// retired connection ids and whether any participant's routing changed
    /// (the caller then recomputes through the normal path).
    pub(super) fn retire_absent(
        &mut self,
        wire_handler_id: &str,
        present: &HashSet<&str>,
        before: Instant,
        now: Instant,
    ) -> (Vec<String>, bool) {
        let Some(handler) = self.handlers.resolve(wire_handler_id).map(|h| h.id.clone()) else {
            return (Vec::new(), false);
        };
        let mut retired = Vec::new();
        let mut touched = Vec::new();
        for (participant_id, view) in &mut self.views {
            let keys = view.connectivity.retire_absent(&handler, present, before);
            if !keys.is_empty() {
                touched.push(participant_id.clone());
                retired.extend(keys.into_iter().map(|k| k.as_str().to_string()));
            }
        }
        let mut changed = false;
        for participant_id in touched {
            changed |= self.sync_routing(&participant_id, now);
        }
        (retired, changed)
    }

    /// THE one removal path for a handler (ADR-0039 §6, step 1 of the
    /// removal sequence the meeting actor drives): drops it from the set —
    /// recorded as removed with its trigger, so the verdict answers RELEASED
    /// and the binding gate refuses it from this instant — AND in the same
    /// call drops its liveness and health records, moves its push worker OUT
    /// (for the caller to drain; never cancelled through the shared token),
    /// and purges its keys from every participant's connectivity, re-routing
    /// everyone. `None` if it was not a member (or the set refused).
    pub(super) fn remove_handler(
        &mut self,
        wire_handler_id: &str,
        trigger: RemovalTrigger,
        now: Instant,
    ) -> Option<RemovedHandler> {
        let id = HandlerId::new(wire_handler_id);
        let endpoint = self.handlers.get(&id).cloned()?;
        let set_change = match self.handlers.remove(&id, trigger) {
            Ok(Some(change)) => change,
            Ok(None) => return None,
            Err(e) => {
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    handler_id = %wire_handler_id,
                    error = %e,
                    "Handler removal refused; the set is unchanged. MC-internal invariant \
                     violation: capture and escalate."
                );
                return None;
            }
        };
        self.liveness.remove(&id);
        self.health.remove(&id);
        let pusher = self.pushers.remove(&id);
        let mut forgotten = 0usize;
        for view in self.views.values_mut() {
            forgotten += view.connectivity.forget_handler(&id).len();
        }
        // Re-route everyone; the caller re-renders unconditionally (the set
        // changed even where no participant's routing did).
        self.reroute_all(now);
        Some(RemovedHandler {
            endpoint,
            set_change,
            pusher,
            forgotten_keys: forgotten,
        })
    }

    /// Hand a removed handler's release to a task the meeting's teardown will
    /// await (the fence covers it). `None` before a join installed the deps
    /// (nothing was ever programmed, so there is nothing to release).
    pub(super) fn spawn_removal_release(
        &mut self,
        endpoint: HandlerEndpoint,
        trigger: RemovalTrigger,
        pusher: Option<HandlerPusher>,
    ) {
        let Some(deps) = &self.deps else {
            return;
        };
        let pusher = pusher.map_or_else(WorkerDrain::none, HandlerPusher::into_drain);
        let release = crate::media_routing::teardown::RemovedHandlerRelease {
            meeting_id: self.meeting_id.clone(),
            mc_id: deps.mc_id.clone(),
            removed: endpoint,
            trigger,
            pusher,
            mh_client: Arc::clone(&deps.mh_client),
            policy_generations: Arc::clone(&deps.policy_generations),
        };
        self.removals.push(tokio::spawn(
            crate::media_routing::teardown::release_removed(release),
        ));
    }

    /// `(participant_id, connection_id)` live keys on one handler (sorted).
    #[cfg(feature = "test-seams")]
    pub(super) fn connectivity_on(&self, wire_handler_id: &str) -> Vec<(String, String)> {
        let handler = HandlerId::new(wire_handler_id);
        let mut keys: Vec<(String, String)> = self
            .views
            .iter()
            .flat_map(|(p, v)| {
                v.connectivity
                    .keys_on(&handler)
                    .map(move |(k, _)| (p.clone(), k.as_str().to_string()))
            })
            .collect();
        keys.sort();
        keys
    }

    /// Remove a participant from slot state and view tracking.
    pub(super) fn remove(&mut self, participant_id: &str) {
        if let Some(view) = self.views.remove(participant_id) {
            self.slots.remove(view.sender);
        }
        self.dirty.remove(participant_id);
    }

    /// Register a validated, TRUNCATED declaration: both kinds' kept slot ids
    /// land in the slot table before one refill.
    ///
    /// # Errors
    ///
    /// [`McError::ParticipantNotFound`] if the participant is not tracked.
    pub(super) fn declare(
        &mut self,
        participant_id: &str,
        demand: SlotDemand,
    ) -> Result<(), McError> {
        let Some(view) = self.views.get_mut(participant_id) else {
            return Err(McError::ParticipantNotFound(
                "Participant not found".to_string(),
            ));
        };
        self.slots.set_demand(
            view.sender,
            MediaKindLabel::per_kind(|k| demand.kept_slot_ids(k)),
        );
        view.declaration = Some(demand);
        Ok(())
    }

    /// The truncated declaration in force for a participant, if any.
    pub(super) fn declaration_of(&self, participant_id: &str) -> Option<&SlotDemand> {
        self.views
            .get(participant_id)
            .and_then(|v| v.declaration.as_ref())
    }

    /// A reconnect: the new connection has received nothing, so forget what
    /// was emitted and re-dirty. Slots and rank are untouched.
    pub(super) fn reset_view(&mut self, participant_id: &str) {
        if let Some(view) = self.views.get_mut(participant_id) {
            view.last_directive = None;
            view.last_assignments = None;
            if view.declaration.is_some() {
                self.dirty.insert(participant_id.to_string());
            }
        }
    }

    /// Hand the meeting's MH-side teardown to the caller, as the actor exits.
    ///
    /// Moves the push workers OUT — with their own token — so that nothing left
    /// in this struct can abort them, and returns everything the teardown needs
    /// to drain them and then release every handler of the meeting's set
    /// (Registering included: its `RegisterMeeting` may have landed).
    /// `None` if no join ever installed the routing deps: no handler was ever
    /// programmed, so there is nothing to release.
    pub(super) fn take_teardown(
        &mut self,
        reason: crate::media_routing::teardown::TeardownReason,
    ) -> Option<crate::media_routing::teardown::MediaTeardown> {
        let (deps, handlers) = (self.deps.as_ref()?, &self.handlers);
        let pusher_tasks = std::mem::take(&mut self.pushers)
            .into_values()
            .filter_map(HandlerPusher::into_task)
            .collect();
        let removals = std::mem::take(&mut self.removals);
        Some(crate::media_routing::teardown::MediaTeardown {
            removals,
            meeting_id: self.meeting_id.clone(),
            mc_id: deps.mc_id.clone(),
            handlers: handlers.clone(),
            pushers_cancel: self.pushers_cancel.clone(),
            pusher_tasks,
            mh_client: Arc::clone(&deps.mh_client),
            policy_generations: Arc::clone(&deps.policy_generations),
            reason,
        })
    }

    /// Is any participant waiting to be flushed?
    pub(super) fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Re-render, re-publish to every handler, and mark `affected` dirty.
    ///
    /// `mute` is REQUIRED, never defaulted: the meeting's roster is the one
    /// home for mute state, so every reconcile must be handed the current
    /// per-kind projection. A call site that could omit it would declare every
    /// stream FORWARD — an unmute nobody asked for, failing open. Making it a
    /// parameter lets the compiler enumerate every call site.
    pub(super) async fn reconcile(&mut self, affected: Affected, mute: &SourceMuteView) {
        self.render_and_publish(mute).await;
        match affected {
            Affected::All => {
                for (participant_id, view) in &self.views {
                    if view.declaration.is_some() {
                        self.dirty.insert(participant_id.clone());
                    }
                }
            }
            Affected::HoldersOf(source) | Affected::PublisherAndHoldersOf(source) => {
                let with_publisher = matches!(affected, Affected::PublisherAndHoldersOf(_));
                let holders = self.slots.holders_of(source);
                for (participant_id, view) in &self.views {
                    let hit =
                        holders.contains(&view.sender) || (with_publisher && view.sender == source);
                    if view.declaration.is_some() && hit {
                        self.dirty.insert(participant_id.clone());
                    }
                }
            }
        }
    }

    /// Render the slot table and publish each handler's snapshot.
    ///
    /// Each snapshot carries the meeting's per-stream `source_streams`, whose
    /// dispositions (from `mute`) are the only mute carrier to MH — see
    /// [`SlotTable::render`]. Synchronous in the actor; the push itself is
    /// the handler's push worker's.
    async fn render_and_publish(&mut self, mute: &SourceMuteView) {
        let Some(deps) = &self.deps else {
            return;
        };
        let handlers = &self.handlers;
        let assignment = match self.slots.render(handlers.ids(), mute) {
            Ok(assignment) => assignment,
            Err(e) => {
                // Fail loud: no push is better than pushing a policy MH will
                // reject whole. Every flush of this state then counts a
                // composition failure.
                error!(
                    target: "mc.register_meeting.trigger",
                    meeting_id = %self.meeting_id,
                    reason = e.label(),
                    error = %e,
                    "Forwarding assignment could not be rendered; MH not programmed"
                );
                self.assignment = None;
                return;
            }
        };

        for (endpoint, handler_ref) in handlers.iter_with_ref() {
            let Some(policy) = assignment.for_handler(&endpoint.id) else {
                continue;
            };
            let generation = match deps
                .policy_generations
                .next_generation(&self.meeting_id, &endpoint.id, policy)
                .await
            {
                Ok(generation) => generation,
                Err(e) => {
                    error!(
                        target: "mc.register_meeting.trigger",
                        meeting_id = %self.meeting_id,
                        mh_grpc_endpoint = %endpoint.grpc_endpoint,
                        error = %e,
                        "Could not derive a policy generation; MH not programmed"
                    );
                    continue;
                }
            };
            let registrations = &self.registrations_tx;
            let pusher = self.pushers.entry(endpoint.id.clone()).or_insert_with(|| {
                HandlerPusher::spawn(
                    PushTarget {
                        mh_client: Arc::clone(&deps.mh_client),
                        policy_generations: Arc::clone(&deps.policy_generations),
                        meeting_id: self.meeting_id.clone(),
                        mc_id: deps.mc_id.clone(),
                        mc_grpc_endpoint: deps.mc_grpc_endpoint.clone(),
                        handler: endpoint.clone(),
                        heartbeat_interval_ms: deps.heartbeat.interval_ms(),
                        media_dial_public_key: self.media_dial_public_key,
                        registration: RegistrationReporter::new(
                            registrations.clone(),
                            endpoint.id.clone(),
                            handler_ref,
                        ),
                    },
                    self.pushers_cancel.child_token(),
                )
            });
            // `send_replace` on EVERY reconcile, never behind an equality
            // guard: that is what re-sends a push that failed and is therefore
            // unconfirmed, even when this handler's snapshot did not change.
            pusher.publish(PushJob {
                generation,
                assignment: policy.clone(),
            });
        }
        self.record_edge_moves(&assignment);
        self.log_in_edge_changes(&assignment);
        self.assignment = Some(assignment);
    }

    /// Classify every edge that changed handler between the previous render and
    /// this one (O-7). An edge may move only because its handler left one
    /// party's connected set; one that moved while BOTH parties are still on
    /// the old handler is an invariant violation (`unexpected`, expected-empty).
    ///
    /// Deliberately an independent diff of two renders, not a counter inside
    /// the slot table's mutation code: a regression in that code cannot also
    /// hide its own evidence.
    fn record_edge_moves(&self, next: &MeetingAssignment) {
        let Some(previous) = &self.assignment else {
            return;
        };
        let index = |a: &MeetingAssignment| -> HashMap<u32, (HandlerId, SenderId, SenderId)> {
            a.per_handler
                .iter()
                .flat_map(|(handler, policy)| {
                    policy.egress_streams.iter().filter_map(move |plan| {
                        plan.candidate_sources.first().map(|source| {
                            (
                                plan.egress_stream_id,
                                (handler.clone(), plan.subscriber, *source),
                            )
                        })
                    })
                })
                .collect()
        };
        let before = index(previous);
        for (stream, (handler, subscriber, source)) in index(next) {
            let Some((old_handler, old_subscriber, old_source)) = before.get(&stream) else {
                continue;
            };
            if *old_subscriber != subscriber || *old_source != source || *old_handler == handler {
                continue;
            }
            let reason = if self
                .slots
                .both_connected_to(subscriber, source, old_handler)
            {
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    from_mh_id = %old_handler,
                    to_mh_id = %handler,
                    "An edge moved handler while both parties are still connected to its old \
                     handler (edge-stability invariant violation). Capture and escalate."
                );
                metrics::EdgeMove::Unexpected
            } else {
                metrics::EdgeMove::ConnectivityChange
            };
            metrics::record_edge_move(reason);
        }
    }

    /// One INFO line per participant whose in-edge handler summary changed:
    /// how many of its slots each handler carries (e.g. `mh-0:2,mh-1:1`). No
    /// per-edge sender identity (ADR-0036 §11).
    fn log_in_edge_changes(&self, next: &MeetingAssignment) {
        let summary = |a: Option<&MeetingAssignment>, subscriber: SenderId| -> String {
            let mut counts: BTreeMap<&HandlerId, usize> = BTreeMap::new();
            if let Some(a) = a {
                for (handler, policy) in &a.per_handler {
                    let n = policy
                        .egress_streams
                        .iter()
                        .filter(|p| p.subscriber == subscriber)
                        .count();
                    if n > 0 {
                        counts.insert(handler, n);
                    }
                }
            }
            counts
                .iter()
                .map(|(h, n)| format!("{h}:{n}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        for (participant_id, view) in &self.views {
            let before = summary(self.assignment.as_ref(), view.sender);
            let after = summary(Some(next), view.sender);
            if before != after {
                info!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    in_edges = %after,
                    "Participant in-edge handlers changed"
                );
            }
        }
    }

    /// Flush up to the per-turn bound of dirty participants; defer the rest.
    pub(super) async fn flush(&mut self, roster: &HashMap<String, RosterEntry<'_>>) {
        let mute = SourceMuteView::from_map(roster.values().map(|e| (e.sender, e.mute)).collect());
        let mut taken = 0usize;
        while taken < self.flush_batch {
            let Some(participant_id) = self.dirty.pop_first() else {
                break;
            };
            taken += 1;
            self.flush_one(&participant_id, roster, &mute).await;
        }
        for _ in 0..self.dirty.len() {
            metrics::record_slot_view_emission(SlotViewEmission::Deferred);
        }
    }

    /// A dirty participant that cannot be flushed because MC's own state
    /// sets disagree. Logged, not counted: an MC-internal invariant violation
    /// whose WARN line carries the meeting and participant ids, and a series
    /// per can't-happen branch would be noise.
    fn warn_unflushable(&self, participant_id: &str, reason: &'static str) {
        warn!(
            target: "mc.actor.meeting",
            meeting_id = %self.meeting_id,
            participant_id = %participant_id,
            reason = reason,
            "Dirty participant has no state to flush (MC-internal invariant violation); \
             its media view was not sent"
        );
    }

    async fn flush_one(
        &mut self,
        participant_id: &str,
        roster: &HashMap<String, RosterEntry<'_>>,
        mute: &SourceMuteView,
    ) {
        // Three of the five early returns below are INVARIANT VIOLATIONS (a
        // dirty participant with no roster entry, no routing deps, or no view):
        // if those sets ever drift, the participant silently stops receiving
        // view updates for the rest of its session, so each is logged loudly.
        // The other two are routine and produce no disposition.
        let Some(entry) = roster.get(participant_id) else {
            self.warn_unflushable(participant_id, "not_in_roster");
            return;
        };
        // Routine: disconnected (in grace), nothing to send to. A reconnect
        // re-dirties.
        let Some(connection) = entry.connection else {
            return;
        };
        let Some(deps) = &self.deps else {
            self.warn_unflushable(participant_id, "no_routing_deps");
            return;
        };
        let handlers = &self.handlers;
        let Some(view) = self.views.get(participant_id) else {
            self.warn_unflushable(participant_id, "no_view");
            return;
        };
        // Routine: not yet declared a receive capability; its declaration
        // dirties it.
        let Some(declaration) = &view.declaration else {
            return;
        };

        let composed = match &self.assignment {
            None => Err(DirectiveOutcome::AssignmentFailed),
            // The SERVER half of this publisher's mute only: the directive is
            // withdrawn for a server-muted kind and never depends on client
            // mute (ADR-0036 §5) -- `entry.mute.client` is a different type
            // and cannot be passed here.
            Some(assignment) => build_send_directive(
                view.sender,
                entry.publishes,
                assignment,
                handlers,
                &deps.stream_policy,
                entry.mute.server,
            )
            .and_then(|(directive, outcome, stream_kinds)| {
                build_stream_assignments(
                    view.sender,
                    declaration,
                    &self.slots,
                    handlers,
                    mute,
                    &self.slots.unreachable_for(view.sender),
                )
                .map(|composition| (directive, outcome, stream_kinds, composition))
            }),
        };

        let (directive, directive_outcome, stream_kinds, composition) = match composed {
            Ok(composed) => composed,
            Err(outcome) => {
                // Fails CLOSED: nothing is emitted for this participant — never
                // an ACTIVE slot or a send target with a non-member ref.
                metrics::record_send_directive(outcome);
                metrics::record_slot_view_emission(SlotViewEmission::CompositionFailed);
                error!(
                    target: "mc.actor.meeting",
                    meeting_id = %self.meeting_id,
                    participant_id = %participant_id,
                    outcome = outcome.label(),
                    "Media signalling could not be composed; this participant's view was not sent"
                );
                return;
            }
        };

        // A view the signalling frame cannot carry is refused HERE, loudly and
        // counted, rather than written as a frame the client's reader must
        // reject (the client enforces the same 64 KiB bound). Reachable only
        // through a client's own near-frame-sized declaration of slots MC
        // truncates (`capability::SlotDemand`): it affects that client alone.
        // Measured on the EXACT `ServerMessage` that would be written (oneof
        // tag, length varint and trace context included), and that same value
        // is the one sent below: no envelope margin to drift.
        let assignments_message = envelope(server_message::Message::StreamAssignments(
            composition.assignments.clone(),
        ));
        if !fits_signalling_frame(&assignments_message) {
            metrics::record_slot_view_emission(SlotViewEmission::CompositionFailed);
            error!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                reason = "view_exceeds_message_limit",
                declared_slots = composition.assignments.assignments.len(),
                "Slot view exceeds the signalling frame limit; this participant's view was not \
                 sent. Only a client declaring a near-frame-sized slot list reaches this."
            );
            return;
        }

        let directive_changed = view.last_directive.as_ref() != Some(&directive);
        let assignments_changed = view.last_assignments.as_ref() != Some(&composition.assignments);
        if !directive_changed && !assignments_changed {
            return;
        }
        // Streams that go from non-empty to EMPTY targets in this change,
        // counted only once the directive is DELIVERED (below).
        let withdrawn: Vec<MediaKindLabel> = match &view.last_directive {
            Some(previous) if directive_changed => {
                withdrawn_kinds(previous, &directive, &stream_kinds)
            }
            _ => Vec::new(),
        };

        let mut delivered = true;
        if directive_changed {
            metrics::record_send_directive(directive_outcome);
            // PER STREAM, with that stream's kind — never summed across the
            // directive's streams (which would put video targets on an audio
            // series). The per-stream counts sum to the old total.
            // `stream_kinds` is parallel to `directive.streams`, projected once
            // at composition, so there is no fallible lookup here.
            for (kind, stream) in stream_kinds.iter().zip(&directive.streams) {
                metrics::record_send_targets(kind.labels(), stream.targets.len());
            }
            delivered &= send(
                connection,
                server_message::Message::SendDirective(directive.clone()),
            )
            .await;
            if delivered {
                for kind in &withdrawn {
                    // Attributed to the CURRENT cause: a stream emptied while
                    // its kind is server-muted is a `server_muted` withdrawal
                    // (a server mute of an already-unwatched, already-empty
                    // stream is no transition and counts nothing); otherwise
                    // its last watcher went.
                    let reason = if entry.mute.server.is_muted(*kind) {
                        WithdrawalReason::ServerMuted
                    } else {
                        WithdrawalReason::Unwatched
                    };
                    metrics::record_send_directive_withdrawal(kind.labels(), reason);
                }
            }
        }
        if assignments_changed && delivered {
            for (kind, state) in &composition.slot_states {
                metrics::record_slot_state(kind.labels(), *state);
            }
            delivered &= send_message(connection, &assignments_message).await;
        }

        if !delivered {
            metrics::record_slot_view_emission(SlotViewEmission::DeliveryFailed);
            warn!(
                target: "mc.actor.meeting",
                meeting_id = %self.meeting_id,
                participant_id = %participant_id,
                "Failed to deliver media signalling to the participant actor (mailbox closed); \
                 the client was NOT told what mc_media_send_directives_total says was composed"
            );
            return;
        }

        metrics::record_slot_view_emission(SlotViewEmission::Sent);
        // Counted only on a DELIVERED `StreamAssignments`, matching the
        // `outcome="sent"` denominator of the catalog's ratio.
        if assignments_changed {
            metrics::record_unreachable_senders(
                composition.assignments.unreachable_sender_ids.len(),
            );
            // Pairs with the unreachable count: silence because the sets are
            // disjoint vs silence because someone (possibly this subscriber
            // itself) is not connected yet.
            metrics::record_not_yet_connected_senders(self.slots.not_routable_count());
        }
        if let Some(view) = self.views.get_mut(participant_id) {
            view.last_directive = Some(directive);
            view.last_assignments = Some(composition.assignments);
        }
    }
}

/// The kinds whose stream had targets in `previous` and has NONE in `next`
/// (a withdrawal: empty targets, never removal). `next_kinds` is parallel to
/// `next.streams`.
fn withdrawn_kinds(
    previous: &SendDirective,
    next: &SendDirective,
    next_kinds: &[MediaKindLabel],
) -> Vec<MediaKindLabel> {
    next.streams
        .iter()
        .zip(next_kinds)
        .filter(|(stream, _)| stream.targets.is_empty())
        .filter(|(stream, _)| {
            previous
                .streams
                .iter()
                .any(|p| p.stream_number == stream.stream_number && !p.targets.is_empty())
        })
        .map(|(_, kind)| *kind)
        .collect()
}

/// Hand one `ServerMessage` to the participant actor. `false` means the
/// participant actor is gone (mailbox closed).
async fn send(connection: &ParticipantActorHandle, message: server_message::Message) -> bool {
    send_message(connection, &envelope(message)).await
}

/// Wrap a message in the `ServerMessage` envelope as it goes on the wire.
/// R-57: carries the current server-side trace context (bounded W3C ids only).
fn envelope(message: server_message::Message) -> ServerMessage {
    let (trace_parent, trace_state) = crate::webtransport::trace::inject_current_context();
    ServerMessage {
        message: Some(message),
        trace_parent,
        trace_state,
    }
}

/// Does this `ServerMessage` fit the 64 KiB signalling frame the client's
/// reader enforces (`webtransport::connection::MAX_MESSAGE_SIZE`)? Measured
/// on the whole envelope, never on the payload alone.
fn fits_signalling_frame(message: &ServerMessage) -> bool {
    prost::Message::encoded_len(message) <= crate::webtransport::connection::MAX_MESSAGE_SIZE
}

/// Hand an already-built `ServerMessage` to the participant actor.
async fn send_message(connection: &ParticipantActorHandle, message: &ServerMessage) -> bool {
    connection
        .send(SignalingPayload::server_message(message))
        .await
        .is_ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod frame_bound_tests {
    use super::*;
    use proto_gen::dark_tower::signaling::v1::{StreamAssignment, StreamAssignments};

    fn assignments(n: usize) -> StreamAssignments {
        StreamAssignments {
            assignments: (0..n)
                .map(|i| StreamAssignment {
                    slot_id: u32::try_from(i).unwrap(),
                    sender_id: None,
                    media_kind: 2,
                    slot_state: 9,
                    switch_command_id: None,
                    handler_ref: 0,
                })
                .collect(),
            unreachable_sender_ids: Vec::new(),
        }
    }

    /// The boundary: a payload that fits the frame ON ITS OWN but not inside
    /// the `ServerMessage` envelope is refused — the bound is the envelope.
    #[test]
    fn the_frame_bound_is_measured_on_the_envelope_not_the_payload() {
        let limit = crate::webtransport::connection::MAX_MESSAGE_SIZE;
        let mut n = 0;
        while prost::Message::encoded_len(&assignments(n + 1)) <= limit {
            n += 1;
        }
        let payload = assignments(n);
        assert!(
            prost::Message::encoded_len(&payload) <= limit,
            "the payload alone fits"
        );
        let wrapped = ServerMessage {
            message: Some(server_message::Message::StreamAssignments(payload)),
            trace_parent: "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01".to_string(),
            trace_state: String::new(),
        };
        assert!(!fits_signalling_frame(&wrapped), "the envelope does not");
        let small = ServerMessage {
            message: Some(server_message::Message::StreamAssignments(assignments(4))),
            trace_parent: String::new(),
            trace_state: String::new(),
        };
        assert!(fits_signalling_frame(&small));
    }
}

#[cfg(test)]
mod census_tests {
    use super::*;
    use crate::observability::metrics::MEDIA_KIND_LABEL;
    use common::observability::testing::MetricAssertion;

    /// Per (meeting, kind), and `publish` sets EVERY kind every time: after the
    /// last video-muted meeting is forgotten the video gauge reads 0, not its
    /// last non-zero value.
    #[test]
    fn publish_sets_every_kind_including_zero_after_the_last_video_mute_ends() {
        let census = MutedSourceCensus::default();
        let snap = MetricAssertion::snapshot();
        census.set("m-1", [2, 1]);
        census.set("m-2", [1, 0]);
        census.publish();
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[(MEDIA_KIND_LABEL, "audio")])
            .assert_value(3.0);
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[(MEDIA_KIND_LABEL, "video")])
            .assert_value(1.0);

        census.set("m-1", [0; 2]);
        census.publish();
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[(MEDIA_KIND_LABEL, "audio")])
            .assert_value(1.0);
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[(MEDIA_KIND_LABEL, "video")])
            .assert_value(0.0);

        census.retain(|_| false);
        census.publish();
        snap.gauge("mc_media_server_muted_sources")
            .with_labels(&[(MEDIA_KIND_LABEL, "audio")])
            .assert_value(0.0);
    }

    /// The awaiting-handler census: recompute-and-set, `false` forgets, and
    /// the health walk's prune re-sets it to 0 when the meeting is gone.
    #[test]
    fn handlerless_census_counts_meetings_with_an_empty_set_and_prunes() {
        let census = HandlerlessCensus::default();
        let snap = MetricAssertion::snapshot();
        let kc = (
            common::observability::labels::KEY_CUSTODY_LABEL,
            common::observability::labels::KEY_CUSTODY_OPERATOR,
        );
        census.set("m-1", true);
        census.set("m-2", true);
        census.set("m-3", false);
        census.publish();
        snap.gauge("mc_media_meetings_without_handlers")
            .with_labels(&[kc])
            .assert_value(2.0);
        census.set("m-2", false);
        census.publish();
        snap.gauge("mc_media_meetings_without_handlers")
            .with_labels(&[kc])
            .assert_value(1.0);
        census.retain(|id| id != "m-1"); // the actor died without clearing
        census.publish();
        snap.gauge("mc_media_meetings_without_handlers")
            .with_labels(&[kc])
            .assert_value(0.0);
    }
}
