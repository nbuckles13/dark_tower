//! MC's client-facing media signalling (ADR-0036 §5, §6).
//!
//! Sibling to [`crate::media_routing`], and the split is deliberate:
//! `media_routing` is the **MC→MH control plane** (who forwards to whom, on
//! which handler); this module is the **MC→client signalling** built from that
//! same output. "Policy" means the forwarding assignment and belongs to the
//! sibling; nothing here is called policy except the media-stream table, which
//! is a different thing and says so.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`capability`] | **parse** a client declaration into a type that cannot hold an invalid slot |
//! | [`directive`] | what the client must **produce** (§5) — no access to mute state |
//! | [`assignments`] | what the client will **receive** (§6) — the only mute-aware code, and the one `StreamAssignments` construction site |
//! | [`outcome`] | the bounded telemetry vocabularies |
//!
//! # The causal chain, stated because it is contractual
//!
//! **A client that joins and never declares a receive capability is never sent
//! a send directive or slot assignments.** The declaration is what registers the
//! client's slot demand with the meeting actor, and the actor emits a
//! participant's view only once it has declared. Client-side tasks must treat
//! the declaration as the **precondition for publishing**. Recorded in
//! `docs/TODO.md` §Media Path Obligations.
//!
//! # Who composes: the meeting actor, on every structural change
//!
//! Since story 2 the meeting actor is the single writer of meeting state AND
//! the single composer of every participant's view. After a join, a leave, a
//! capability declaration or a mute (self or server) it re-renders the slot
//! table and re-emits, to each declared participant whose view changed, the
//! changed [`directive`] and/or [`assignments`] — directive first. Declared
//! audio slots are the assignment's INPUT, so there is no longer a slot-id
//! namespace a declaration can miss: any well-formed declaration is servable.
//!
//! # Bounding work on the shared meeting actor: three kinds, and how to choose
//!
//! Three paths put work on the shared meeting actor, and they are bounded by
//! DIFFERENT mechanisms on purpose. The criterion, so the next bounded path is
//! chosen rather than pattern-matched:
//!
//! - A **cumulative budget** bounds total work and permanently denies once
//!   spent. Correct for a **rare, client-driven** action whose omission costs
//!   the client nothing — re-declaring a receive capability. Applied as
//!   `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`.
//! - A **rate limit** bounds work per unit time and never permanently denies.
//!   Required for a **repeatable, client-driven steady-state** action — client
//!   mute is one toggle per utterance, for the life of the session. Applied as
//!   the mute-work token bucket in `webtransport::connection`.
//! - A **per-turn deferring bound** caps work per actor turn and defers — never
//!   denies, never drops — the rest to later turns, coalescing repeats.
//!   Required for **server-initiated fan-out whose omission is a correctness
//!   failure the client cannot re-request**: the slot-view re-emit after a
//!   roster change. A client holding a stale view hears the wrong person or
//!   nobody and has no message to send that would fix it, so neither denying
//!   (budget) nor shedding (rate limit) is acceptable; only latency may give.
//!   Applied as `SLOT_VIEW_FLUSH_BATCH` in `actors::meeting_media`, whose doc
//!   points back here.
//!
//! Budgeting mute would freeze `audio_self_muted` on the roster once spent, so
//! every other participant would render a live speaker as muted for the rest of
//! the meeting. A limiter that converts an amplification bound into a
//! correctness failure is the wrong limiter.
//!
//! # No key material
//!
//! Nothing here reads, derives, logs or transmits key material — no meeting KEK,
//! no transmit key, no key id, no identity key. Telemetry carries
//! `key_custody=operator` and no meeting identifier (§11).

pub mod assignments;
pub mod capability;
pub mod directive;
pub mod outcome;

pub use assignments::{
    build_stream_assignments, slot_state_label, SlotComposition, SourceMuteView,
};
pub use capability::{DeclaredSlot, PinnedSenderId, ReceiveCapabilityDeclaration, SlotId};
pub use directive::{
    build_send_directive, AudioEncoding, AudioEncodingError, MediaStreamPolicy, StreamNumber,
    AUDIO_BITRATE_MAX_BPS, AUDIO_BITRATE_MIN_BPS, AUDIO_FRAME_RATE_MAX_HZ,
    AUDIO_FRAME_RATE_MH_SIZING_HZ, AUDIO_FRAME_RATE_MIN_HZ,
};
pub use outcome::{CapabilityOutcome, DirectiveOutcome, MuteOutcome, SlotViewEmission};

/// The configured inputs to client-facing media signalling, as one value.
///
/// Bundled rather than threaded as four scalars through the WebTransport
/// server and connection handler: they are read together at exactly one place
/// (the per-connection context) and a four-scalar signature is where a caller
/// eventually transposes two `u32`s that both look like counts.
///
/// Every field is already validated — [`AudioEncoding`] cannot hold an
/// unspecified codec, and the two bounds are range-checked at config load — so
/// holding one of these is proof the configuration was accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientMediaConfig {
    /// Maximum slots one declaration may contain (`MC_MAX_RECEIVE_SLOTS`).
    pub max_receive_slots: usize,
    /// Maximum ACCEPTED declarations per connection
    /// (`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`).
    pub max_receive_capability_declarations: u32,
    /// The encoding MC directs for main audio.
    pub audio_encoding: AudioEncoding,
    /// How long a participant's handler connections may take to settle before
    /// MC routes on what it observed (`MC_MEDIA_CONNECT_SETTLE_MS`); see
    /// `media_routing/connectivity.rs` for why the window exists.
    pub connect_settle_window: std::time::Duration,
}
