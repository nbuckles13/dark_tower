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
//! | [`assignments`] | what the client will **receive** (§6) — the join, and the only mute-aware code |
//! | [`outcome`] | the bounded telemetry vocabularies |
//!
//! # The causal chain, stated because it is contractual
//!
//! **A client that joins and never declares a receive capability is never told
//! to send.** Emission of the send directive is triggered by the capability
//! declaration, not by the join. That is intended, not incidental: composing
//! both messages from one meeting-state snapshot is what keeps the directive's
//! target set and the assignment's sources from disagreeing, and it costs one
//! actor round trip instead of two. Client-side tasks must treat the declaration
//! as the **precondition for publishing**. Recorded in `docs/TODO.md` §Media
//! Path Obligations.
//!
//! # The slot-id namespace coupling, and why a rejection enforces it
//!
//! MH stamps each frame's relay-region `stream_id` from the egress slot id in
//! the forwarding policy MC pushed at first-participant join —
//! `MAIN_AUDIO_SLOT_ID`, fixed **before** any client could declare. A subscriber
//! validates an arriving `stream_id` against its own declared slots, which is
//! precisely why `ReceiveSlot.slot_id` shares that value space. So the loopback
//! works if and only if the client declares that same id.
//!
//! In this story MC cannot renumber MH's egress: task 13 pushes once at join and
//! this task adds no re-push. MC therefore **rejects** a declaration that asks
//! for audio in a slot it has no plan for, rather than accepting it and
//! reporting a false source shortage while MH forwards frames the client will
//! drop. The wire contract permits that declaration; this is an MC limitation
//! and the rejection says so.
//!
//! Do **not** relax the equality match into a kind-only match to make it "more
//! flexible" — that reintroduces the silent-dark-media path the rejection
//! exists to prevent. The obligation and its retirement (a capability-triggered
//! re-push, or declared slots as an input to the assignment computation) are
//! recorded in `docs/TODO.md` §Media Path Obligations.
//!
//! # Bounding client-driven paths: budget versus rate limit
//!
//! Two paths here reach the shared meeting actor on client input, and they are
//! bounded by DIFFERENT mechanisms on purpose. The criterion, so the next
//! bounded path is chosen rather than pattern-matched:
//!
//! - A **cumulative budget** bounds total work and permanently denies once
//!   spent. Correct for a **rare** action whose omission costs the client
//!   nothing — re-declaring a receive capability. Applied as
//!   `MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`.
//! - A **rate limit** bounds work per unit time and never permanently denies.
//!   Required for a **repeatable steady-state user action** — client mute is
//!   one toggle per utterance, for the life of the session. Applied as the
//!   mute-work token bucket in `webtransport::connection`.
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
pub use capability::{
    DeclaredSlot, PinnedSenderId, PlannedAudioSlot, ReceiveCapabilityDeclaration, SlotId,
};
pub use directive::{
    build_send_directive, AudioEncoding, AudioEncodingError, HandlerUrls, MediaStreamPolicy,
    StreamNumber, AUDIO_BITRATE_MAX_BPS, AUDIO_BITRATE_MIN_BPS, AUDIO_FRAME_RATE_MAX_HZ,
    AUDIO_FRAME_RATE_MH_SIZING_HZ, AUDIO_FRAME_RATE_MIN_HZ,
};
pub use outcome::{CapabilityOutcome, DirectiveOutcome, MuteOutcome};

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
}
