//! MC's media-routing control plane (ADR-0036 §7, §8, §9).
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`slots`] | per-meeting **join-order slot state and shared-handler edges** (held by the meeting actor): an edge exists iff the pair shares a connected handler, and its pure render into a per-handler forwarding snapshot |
//! | [`connectivity`] | one participant's OBSERVED connectivity keyed by MH connection, the tombstone, and the settle rule |
//! | [`edges`] | which shared handler carries one edge — policy only (co-locate), correct for ANY choice |
//! | [`placement`] | the meeting's MC-owned, versioned, ref-addressed handler set (the one home for client-facing handler urls and refs) and [`ConnectedHandlers`], the observed connectivity that can only hold resolved handlers |
//! | [`assignment`] | the snapshot's **output types** and the id-packing rules every producer shares |
//! | [`generation`] | a **change-detector** that numbers those snapshots, plus floor adoption after an MC restart |
//! | [`confirm`] | a **total classifier** of the handler's reply into one bounded outcome |
//! | [`pusher`] | one **push worker per (meeting, handler)**: serialized, latest-wins, meeting-scoped |
//! | [`liveness`] | per-(meeting, handler) heartbeat timing (N and everything derived from it), the liveness record, the ONE remove-only retire predicate, and the heartbeat metric vocabularies (ADR-0039 §1/§2/§6) |
//! | [`handler_health`] | THE pure health decision per (meeting, handler) — `assess` and its `next_deadline` (ADR-0039 §2) |
//! | [`replacement`] | replacement after removal (ADR-0039 §3/§4): THE backoff ladder and the published retry budget, the per-meeting attempt bound, the migration metric vocabularies and the per-meeting bookkeeping (pure; the meeting actor drives it) |
//! | [`planned_transfer`] | planned transfer off a DRAINING handler (ADR-0039 §4/§7): THE cutover instant, the per-handler transfer bookkeeping and THE target-eligibility rule (pure; the meeting actor drives it) |
//! | [`teardown`] | releasing an ended meeting on EVERY handler of its handler set, and ONE removed handler (`release_removed`), only after the push workers concerned have DRAINED (the `Quiesced` witness) |
//! | (`grpc::mh_client`) | the programming call that carries a snapshot and confirms the echo |
//!
//! Loopback is gone (story 2 R-3): visibility is non-reflexive, so a solo
//! participant hears nothing — the same general computation, not a special case.
//!
//! # Structural re-push; liveness comes from the heartbeat
//!
//! Every structural change — a join, a leave, a capability declaration, a mute,
//! a settled connectivity change reported by a handler —
//! re-renders the meeting and re-pushes the FULL snapshot to each assigned
//! handler under a generation that advances only when that handler's snapshot
//! changed, and confirms MH's applied-generation echo. There is no periodic
//! re-assert and no restart detection (ADR-0039 supersedes ADR-0036 §8's
//! recovery model): each handler's liveness and applied generation arrive on
//! its per-meeting heartbeat ([`liveness`]), and a lost handler is replaced,
//! never re-programmed in place.
//!
//! # Key material never appears here
//!
//! Nothing in this module reads, derives, logs or transmits key material. The
//! MC→MH contract carries meeting/handler identity, edges, behaviours and a
//! generation — no KEK, no identity key, no wrap nonce. Telemetry carries
//! `key_custody=operator` (ADR-0036 §4, §11) and **no meeting identifier**.

pub mod assignment;
pub mod confirm;
pub mod connectivity;
pub mod edges;
pub mod generation;
pub mod handler_health;
pub mod liveness;
pub mod placement;
pub mod planned_transfer;
pub mod pusher;
pub mod replacement;
pub mod slots;
pub mod teardown;

pub use assignment::{
    AssignmentError, EgressStreamPlan, HandlerAssignment, HandlerId, MeetingAssignment,
    SourceDispositionPlan, SourceStreamPlan, AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
    MAIN_VIDEO_STREAM_NUMBER, VIDEO_PRIORITY_GROUP,
};
pub use confirm::{
    evaluate, evaluate_with_echo, first_echo_mismatch, handler_replaced, EchoArm,
    PolicyPushOutcome, PushDisposition, PushExpectation,
};
pub use connectivity::{ConnectionKey, ParticipantConnectivity, Phase, SettleOutcome, Unapplied};
pub use generation::{GenerationSpaceExhausted, PolicyGenerations};
pub use handler_health::{
    assess, next_deadline, HandlerHealth, HealthConfig, Observations, RefusalClass,
    SuspicionOutcome,
};
pub use liveness::{
    applied_lag, retirable, Acceptance, HandlerHeartbeat, HandlerLiveness, HeartbeatAnomaly,
    HeartbeatOutcome, HeartbeatTiming, HeartbeatVerdict, LagReading, Observed,
    APPLIED_SEQUENCE_LAG_BUCKETS, APPLY_LAG_FACTOR,
};
pub use placement::{
    AssignmentRejected, ClientHandlerEntry, ClientHandlerSet, ConnectedHandlers, HandlerEndpoint,
    HandlerSetError, MeetingHandlers, ReleaseCauseExt, RemovalTrigger, SetChange,
};
pub use planned_transfer::PlannedTransferTiming;
pub use pusher::{FloorAdoption, HandlerPusher, PushJob, PushTarget, WorkerDrain};
pub use replacement::{MigrationOutcome, MigrationPhase, ReplacementTiming};
pub use slots::{
    Edge, JoinRank, SlotTable, SlotTableError, SourceMute, SourceMuteView, KIND_FILL_ORDER,
};
