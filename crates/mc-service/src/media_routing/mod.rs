//! MC's media-routing control plane (ADR-0036 §7, §8, §9).
//!
//! MC owns four things here, and they are deliberately four modules rather than
//! one:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`assignment`] | a **pure function** from meeting state to a per-handler forwarding snapshot |
//! | [`generation`] | a **change-detector** that numbers those snapshots |
//! | [`confirm`] | a **total classifier** of the handler's reply into one bounded outcome |
//! | (`grpc::mh_client`) | the **one-shot programming call** that carries a snapshot |
//!
//! The loopback ("hear yourself") is the N=1 evaluation of the first of those,
//! not a special case inside it.
//!
//! # Scope: the wire contract and one push, nothing else
//!
//! ADR-0036 §8's handler-restart recovery — periodic re-assert cadence,
//! connectivity-loss trigger, dispatch jitter, cadence-versus-provisional-timeout
//! startup validation, re-assert-failure paging, and restart detection off
//! `process_start_epoch_ms` — is deliberately **not** here. What lands is the
//! policy fields, the derived generation, the applied-generation echo, and
//! single-push-plus-confirm.
//!
//! That later story is additive rather than a rewrite because of one shape
//! choice: [`generation::PolicyGenerations`] holds
//! `(meeting, handler) -> (assignment, generation)`, which is exactly the state
//! a cadence task walks, and the push path takes a *snapshot* rather than a join
//! event.
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

pub use assignment::{
    compute_assignment, AssignmentError, EgressStreamPlan, HandlerAssignment, HandlerId,
    MeetingAssignment, MeetingRoutingInput, RoutingParticipant, AUDIO_PRIORITY_GROUP,
    MAIN_AUDIO_SLOT_ID, MAIN_AUDIO_STREAM_NUMBER,
};
pub use confirm::{
    divergence_magnitude, evaluate, PolicyPushOutcome, PushDisposition, PushExpectation,
};
pub use generation::{GenerationSpaceExhausted, PolicyGenerations};
