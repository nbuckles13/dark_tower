//! MC's media-routing control plane (ADR-0036 §7, §8, §9).
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`slots`] | per-meeting **join-order slot state** (held by the meeting actor) and its pure render into a per-handler forwarding snapshot |
//! | [`placement`] | which ONE handler of the meeting's set each participant is placed on — the one home for placement and for client-facing handler urls |
//! | [`assignment`] | the snapshot's **output types** and the id-packing rules every producer shares |
//! | [`generation`] | a **change-detector** that numbers those snapshots, plus floor adoption after an MC restart |
//! | [`confirm`] | a **total classifier** of the handler's reply into one bounded outcome |
//! | [`pusher`] | one **push worker per (meeting, handler)**: serialized, latest-wins, meeting-scoped |
//! | (`grpc::mh_client`) | the programming call that carries a snapshot and confirms the echo |
//!
//! Loopback is gone (story 2 R-3): visibility is non-reflexive, so a solo
//! participant hears nothing — the same general computation, not a special case.
//!
//! # Scope: structural re-push, not the §8 cadence
//!
//! Every structural change — a join, a leave, a capability declaration, a mute —
//! re-renders the meeting and re-pushes the FULL snapshot to each assigned
//! handler under a generation that advances only when that handler's snapshot
//! changed, and confirms MH's applied-generation echo. ADR-0036 §8's
//! handler-restart recovery — periodic re-assert cadence, connectivity-loss
//! trigger, dispatch jitter, cadence-versus-provisional-timeout validation, and
//! restart detection off `process_start_epoch_ms` — is story 4. It lands
//! additively: [`generation::PolicyGenerations`] holds
//! `(meeting, handler) -> (assignment, generation)`, exactly the state a
//! cadence walks, and the push path takes a *snapshot* rather than an event.
//!
//! # Key material never appears here
//!
//! Nothing in this module reads, derives, logs or transmits key material. The
//! MC→MH contract carries meeting/handler identity, edges, behaviours and a
//! generation — no KEK, no identity key, no wrap nonce. Telemetry carries
//! `key_custody=operator` (ADR-0036 §4, §11) and **no meeting identifier**.

pub mod assignment;
pub mod confirm;
pub mod generation;
pub mod placement;
pub mod pusher;
pub mod slots;

pub use assignment::{
    AssignmentError, EgressStreamPlan, HandlerAssignment, HandlerId, MeetingAssignment,
    AUDIO_PRIORITY_GROUP, MAIN_AUDIO_STREAM_NUMBER,
};
pub use confirm::{
    divergence_magnitude, evaluate, PolicyPushOutcome, PushDisposition, PushExpectation,
};
pub use generation::{GenerationSpaceExhausted, PolicyGenerations};
pub use placement::{HandlerEndpoint, HandlerSetError, MeetingHandlers};
pub use pusher::{FloorAdoption, HandlerPusher, PushJob, PushTarget};
pub use slots::{JoinRank, SlotTable, SlotTableError};
