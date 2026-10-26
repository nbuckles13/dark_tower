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
//! | [`capability`] | **parse** a client declaration into a type that cannot hold an invalid slot, then **truncate** it per kind |
//! | [`directive`] | what the client must **produce** (§5) — server mute and watch state, never client mute |
//! | [`assignments`] | what the client will **receive** (§6) — both mutes (client and server) as slot states, and the one `StreamAssignments` construction site |
//! | [`outcome`] | the bounded telemetry vocabularies |
//!
//! # The causal chain, stated because it is contractual
//!
//! **Since story 3 the JOIN carries the initial receive declaration** (absent =
//! zero slots of each kind) and the fixed publish set, so every admitted
//! participant is declared from its join turn: its first `SendDirective` and
//! `StreamAssignments` are queued in that turn, behind the `JoinResponse` and
//! the server-mute replay. A post-join `ReceiveCapability` is a REDECLARATION;
//! one identical to the join-carried declaration is `accepted_unchanged`.
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
//! Budgeting mute would freeze a client-mute fact on the roster once spent, so
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

pub use crate::media_kind::PublishSet;
pub use assignments::{
    build_stream_assignments, slot_state_label, SlotComposition, SourceMuteView,
};
pub use capability::{
    publishes_and_receives_nothing, DeclaredSlot, PinnedSenderId, ReceiveCapabilityDeclaration,
    SlotDemand, SlotId,
};
pub use directive::{
    build_send_directive, parse_codec, AudioEncoding, AudioEncodingError, CodecParseError,
    MediaStreamPolicy, StreamNumber, VideoEncoding, VideoEncodingError, AUDIO_BITRATE_MAX_BPS,
    AUDIO_BITRATE_MIN_BPS, AUDIO_FRAME_RATE_MAX_HZ, AUDIO_FRAME_RATE_MH_SIZING_HZ,
    AUDIO_FRAME_RATE_MIN_HZ, MIN_KEYFRAME_INTERVAL_MS_CEILING, MIN_KEYFRAME_INTERVAL_MS_FLOOR,
};
pub use outcome::{
    CapabilityOutcome, ClientMuteFanoutOutcome, DirectiveOutcome, MediaKindLabel, MuteOutcome,
    MuteRejection, MuteSurface, RefusalReplySurface, ServerMuteAction, ServerMuteOutcome,
    SlotViewEmission, UnmuteRequestOutcome, WithdrawalReason,
};

/// The configured inputs to client-facing media signalling, as one value.
///
/// Bundled rather than threaded as scalars through the WebTransport server and
/// connection handler: read together at exactly one place (the per-connection
/// context), and a many-scalar signature is where a caller eventually
/// transposes two `u8`s that both look like counts.
///
/// Every field is already validated — the encodings cannot hold an
/// unspecified codec or an out-of-band value, and the bounds are range-checked
/// at config load — so holding one of these is proof the configuration was
/// accepted.
///
/// # Read once, at startup
///
/// The per-kind caps are read when MC starts and never re-read: a live
/// participant's slot vectors are NEVER re-truncated in place. A cap change —
/// including the R-26 video-off lever (`MC_MAX_VIDEO_RECEIVE_SLOTS=0`) —
/// applies to declarations made after the MC restart that picks it up, and
/// that restart reconnects everyone (`docs/runbooks/mc-deployment.md`
/// §Video-off lever).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientMediaConfig {
    /// Per-kind receive-slot caps, laid out by [`MediaKindLabel::index`]:
    /// `MC_MAX_AUDIO_RECEIVE_SLOTS` (1..=64) and `MC_MAX_VIDEO_RECEIVE_SLOTS`
    /// (0..=64; 0 disables video).
    ///
    /// THE values MC truncates every declaration to, publishes
    /// (`mc_media_receive_slot_cap{media_kind}`) and advertises
    /// (`JoinResponse.receive_slot_caps`, one entry per kind).
    pub receive_slot_caps: [u8; 2],
    /// Maximum ACCEPTED declarations per connection
    /// (`MC_MAX_RECEIVE_CAPABILITY_DECLARATIONS`).
    pub max_receive_capability_declarations: u32,
    /// The encoding MC directs for main audio.
    pub audio_encoding: AudioEncoding,
    /// The encoding MC directs for main video.
    pub video_encoding: VideoEncoding,
    /// How long a participant's handler connections may take to settle before
    /// MC routes on what it observed (`MC_MEDIA_CONNECT_SETTLE_MS`).
    pub connect_settle_window: std::time::Duration,
}
