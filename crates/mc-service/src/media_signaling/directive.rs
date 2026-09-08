//! What MC directs a client to **produce** (ADR-0036 §5).
//!
//! # This module has no path to mute state, and that is the point
//!
//! ADR-0036 §5: *"MC keeps the send directive active while a client reports
//! itself muted. The client suppresses locally; MC does not withdraw the
//! instruction."* That is what makes unmute instantaneous, and what keeps
//! *"MC has not asked you to send"* and *"you have muted yourself"* from
//! collapsing into one state.
//!
//! Nothing in this module takes, holds, reads, or can reach a mute flag.
//! [`build_send_directive`] has no mute parameter, and there is no import here
//! through which one could arrive. A mute/unmute cycle altering the directive is
//! therefore a **compile error**, not a test failure — the invariant is enforced
//! by the module boundary rather than by a comment asking the next author not to
//! do it. Mute lives entirely in [`super::assignments`], where it belongs: it
//! changes what a *subscriber is told about a source*, never what a *publisher
//! is told to produce*.
//!
//! If you find yourself needing mute state here, the change you want is in
//! `assignments`.
//!
//! # No priority group
//!
//! A send directive carries transport mode and **not** priority group
//! (§5 versus §7). Priority group is an MH egress property on the internal
//! registration contract: the groups *bound* how far a publisher's self-declared
//! salience signal can promote it, so a client able to name its own group could
//! promote itself past that bound — "hold the meeting's audio floor" as a
//! one-line client patch. The field does not exist on `SendDirective`,
//! `SendStream` or `SendTarget`, so this is structural rather than observed.

use super::outcome::DirectiveOutcome;
use crate::media_admission::SenderId;
use crate::media_routing::{HandlerId, MeetingAssignment, MAIN_AUDIO_STREAM_NUMBER};
use media_protocol::frame::PROTOCOL_VERSION;
use proto_gen::dark_tower::signaling::v1::{
    Codec, EncodingParameters, MediaKind, SendDirective, SendStream, SendTarget, TransportMode,
};
use std::collections::BTreeMap;

/// A publisher's stream index, carried in the signed SFrame key id.
///
/// # Why a newtype with a fallible constructor
///
/// The key id allots **8 bits** to the stream, so a wider value truncates on
/// encode and **aliases two distinct streams of one sender** — which, because
/// the key id is the AEAD associated data and the wrap nonce derives from it, is
/// a nonce-reuse hazard rather than a routing nuisance.
///
/// Typing the field `u8` alone makes an out-of-range value unconstructible and
/// therefore *undemonstrable*: a structural guarantee with no way to show it
/// holds is the "control that is alive but out of scope" shape ADR-0036 warns
/// about. [`StreamNumber::from_wire`] is the one place a wider value can enter,
/// so the guarantee has exactly one seam and that seam is testable.
///
/// Not to be confused with [`super::capability::SlotId`] (16-bit, relay-region,
/// per-subscriber, freely reusable) — see that type for the full comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamNumber(u8);

impl StreamNumber {
    /// Narrow a wire-width value, failing closed.
    ///
    /// # Errors
    ///
    /// [`DirectiveOutcome::UnknownStreamNumber`] if the value exceeds the 8 bits
    /// the key id allots. **Never truncates**: 256 is an error, not 0.
    pub fn from_wire(value: u32) -> Result<Self, DirectiveOutcome> {
        u8::try_from(value)
            .map(Self)
            .map_err(|_| DirectiveOutcome::UnknownStreamNumber)
    }

    /// Wrap a value already narrowed by its type (an assignment plan's field).
    #[must_use]
    pub fn from_u8(value: u8) -> Self {
        Self(value)
    }

    /// The numeric value.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }
}

/// A configured value could not be turned into a usable audio encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEncodingError {
    /// The codec string named nothing in the closed set.
    UnknownCodec,
    /// The codec string named `unspecified`, which is never valid in a
    /// directive.
    CodecUnspecified,
    /// The codec string named a video codec for an audio-only knob.
    CodecNotAudio,
    /// Bitrate outside the band `mh-service` sizes its datagram buffer against.
    BitrateOutOfBand,
    /// Frame rate outside the band `mh-service` sizes its datagram buffer
    /// against.
    FrameRateOutOfBand,
}

impl std::fmt::Display for AudioEncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::UnknownCodec => "not a recognised codec",
            Self::CodecUnspecified => {
                "'unspecified' is never valid in a send directive; name a codec"
            }
            Self::CodecNotAudio => "not an audio codec",
            Self::BitrateOutOfBand => {
                "outside the audio bitrate band mh-service sizes its datagram buffer against \
                 (see AUDIO_BITRATE_MIN_BPS / AUDIO_BITRATE_MAX_BPS)"
            }
            Self::FrameRateOutOfBand => {
                "outside the audio frame-rate band mh-service sizes its datagram buffer against \
                 (see AUDIO_FRAME_RATE_MIN_HZ / AUDIO_FRAME_RATE_MAX_HZ)"
            }
        };
        f.write_str(text)
    }
}

impl std::error::Error for AudioEncodingError {}

/// Lowest audio bitrate MC will direct, in bits per second.
///
/// # This IS `mh-service`'s floor, duplicated because that constant is private
///
/// Same quantity, two homes — an ANCHOR, not a boundary. `mh-service`'s
/// `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS` is a private `const`, so MC cannot read
/// it and the value is restated here. **The two must move together**: MH sizes
/// `NOMINAL_AUDIO_FRAME_BYTES` at its floor, so a floor MC will direct below
/// MH's would put real frames under the size MH budgeted for. That obligation
/// is recorded in `docs/TODO.md` §Media Path Obligations, owned by
/// media-handler and client.
///
/// Contrast [`AUDIO_BITRATE_MAX_BPS`], which is a genuinely DIFFERENT quantity
/// from MH's floor and carries a do-not-unify note for that reason. Do not
/// generalise this anchor into that boundary or vice versa — they point in
/// opposite directions.
///
/// ANCHOR (prose, unguarded):
/// `crates/mh-service/src/config.rs::SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`.
pub const AUDIO_BITRATE_MIN_BPS: u32 = 32_000;

/// Highest audio bitrate MC will direct, in bits per second.
///
/// # A DIFFERENT quantity from MH's floor, and the do-not-unify lives here
///
/// `mh-service` sizes `NOMINAL_AUDIO_FRAME_BYTES` — and therefore the
/// operator-facing datagram-buffer budget it reports — at its
/// `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`, the *smallest* frame it expects. That is
/// a **minimum MH assumes**; this is a **maximum MC directs**. They are numerically
/// unrelated and must not be unified: collapsing them would make MC's ceiling
/// track MH's sizing assumption, so raising one would silently move the other.
///
/// The hazard this bound exists to prevent: MC directing a ceiling *below* MH's
/// floor would put every real frame under MH's nominal size, loosening the
/// latency budget past what MH reports, silently and with nothing failing.
///
/// ANCHOR (prose, unguarded):
/// `crates/mh-service/src/config.rs::SUPPORTED_AUDIO_BITRATE_CEILING_BPS` — the
/// top of the band MH documents, which is the value this tracks.
pub const AUDIO_BITRATE_MAX_BPS: u32 = 48_000;

/// Lowest audio frame rate MC will direct, in frames per second.
///
/// 25 Hz is 40 ms frames — ADR-0036 §3's named mitigation for per-frame
/// signature overhead ("halves the overhead, costs 20 ms latency"), so this is a
/// live path rather than a theoretical bound.
pub const AUDIO_FRAME_RATE_MIN_HZ: u32 = 25;

/// The audio frame rate `mh-service` SIZED ITS DATAGRAM BUFFER AGAINST.
///
/// 50 Hz is 20 ms frames — the same physical quantity as
/// `crates/mh-service/src/config.rs::AUDIO_FRAME_DURATION_MS = 20`, expressed in
/// the reciprocal unit: MH multiplies its configured frame count by 20 ms to get
/// the datagram-buffer latency budget it reports to operators, so directing a
/// rate MH did not size for moves that budget without moving what MH reports.
///
/// # Why this is its own constant and not just the band ceiling
///
/// These are two DIFFERENT facts that happen to be equal today, and the startup
/// divergence WARN in `main.rs` compares against THIS one. Anchoring that
/// comparison on [`AUDIO_FRAME_RATE_MAX_HZ`] instead would make widening the
/// band silently INVERT the control: raise the ceiling to 100 Hz and the
/// predicate becomes `!= 100`, so the MH-correct value (50) starts warning and
/// the genuinely divergent value (100) goes quiet. A control that is loud on the
/// correct configuration and silent on the wrong one is worse than no control.
///
/// ANCHOR (prose, unguarded):
/// `crates/mh-service/src/config.rs::AUDIO_FRAME_DURATION_MS`.
pub const AUDIO_FRAME_RATE_MH_SIZING_HZ: u32 = 50;

/// Highest audio frame rate MC will direct, in frames per second.
///
/// **Derived, not duplicated.** Today the ceiling of the band MC will accept is
/// exactly the rate MH sized against; raising it is a coordinated change with
/// media-handler, and doing so must not disturb the divergence WARN, which
/// compares against [`AUDIO_FRAME_RATE_MH_SIZING_HZ`] directly.
pub const AUDIO_FRAME_RATE_MAX_HZ: u32 = AUDIO_FRAME_RATE_MH_SIZING_HZ;

/// The encoding parameters MC directs for main audio.
///
/// Constructible only through [`AudioEncoding::new`], which rejects
/// `CODEC_UNSPECIFIED`. That makes an unspecified codec in a send directive
/// **structurally unrepresentable** at the emit site rather than something the
/// emit site has to remember to check — `signaling.proto`'s "zero is not a
/// silent Opus" rule, enforced by the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioEncoding {
    codec: Codec,
    max_bitrate_bps: u32,
    frame_rate_hz: u32,
}

impl AudioEncoding {
    /// Validate configured values into an encoding MC can direct.
    ///
    /// # Errors
    ///
    /// [`AudioEncodingError`] if the codec is unspecified or not an audio
    /// codec, or if a value falls outside the band `mh-service` sizes against.
    pub fn new(
        codec: Codec,
        max_bitrate_bps: u32,
        frame_rate_hz: u32,
    ) -> Result<Self, AudioEncodingError> {
        match codec {
            Codec::Unspecified => return Err(AudioEncodingError::CodecUnspecified),
            Codec::Opus => {}
            Codec::Vp9 | Codec::Av1 | Codec::H264 => return Err(AudioEncodingError::CodecNotAudio),
        }
        if !(AUDIO_BITRATE_MIN_BPS..=AUDIO_BITRATE_MAX_BPS).contains(&max_bitrate_bps) {
            return Err(AudioEncodingError::BitrateOutOfBand);
        }
        if !(AUDIO_FRAME_RATE_MIN_HZ..=AUDIO_FRAME_RATE_MAX_HZ).contains(&frame_rate_hz) {
            return Err(AudioEncodingError::FrameRateOutOfBand);
        }
        Ok(Self {
            codec,
            max_bitrate_bps,
            frame_rate_hz,
        })
    }

    /// Parse a configured codec name into the closed proto enum.
    ///
    /// Trimmed and case-folded, then matched exactly. An unrecognised value is
    /// an **error**, never a fall-back to a default: silently coercing
    /// `"oppus"` to Opus would leave an operator believing they had configured
    /// something they had not.
    ///
    /// # Errors
    ///
    /// [`AudioEncodingError::UnknownCodec`] or
    /// [`AudioEncodingError::CodecUnspecified`].
    pub fn parse_codec(raw: &str) -> Result<Codec, AudioEncodingError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "opus" => Ok(Codec::Opus),
            "vp9" => Ok(Codec::Vp9),
            "av1" => Ok(Codec::Av1),
            "h264" => Ok(Codec::H264),
            "unspecified" => Err(AudioEncodingError::CodecUnspecified),
            _ => Err(AudioEncodingError::UnknownCodec),
        }
    }

    /// The wire form.
    #[must_use]
    pub fn to_proto(self) -> EncodingParameters {
        EncodingParameters {
            codec: self.codec as i32,
            max_bitrate_bps: self.max_bitrate_bps,
            // Audio carries no geometry. Zero here is the absence of a
            // constraint, not a resolution of 0x0.
            width: 0,
            height: 0,
            frame_rate: self.frame_rate_hz,
        }
    }

    /// The configured codec.
    #[must_use]
    pub fn codec(self) -> Codec {
        self.codec
    }

    /// The configured maximum bitrate, in bits per second.
    #[must_use]
    pub fn max_bitrate_bps(self) -> u32 {
        self.max_bitrate_bps
    }

    /// The configured frame rate, in frames per second.
    #[must_use]
    pub fn frame_rate_hz(self) -> u32 {
        self.frame_rate_hz
    }
}

/// What each stream number a publisher may be asked to produce actually is.
///
/// MH is type-blind by design (§7), so the forwarding assignment carries no
/// media kind — it names behaviours. MC, which is not type-blind, holds the
/// mapping here. One entry today.
///
/// A plan naming a stream number absent from this table is an **MC defect** and
/// fails loudly: there is no "assume audio" fallback, because guessing the kind
/// of a stream is how a client ends up encrypting video parameters into an audio
/// key stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaStreamPolicy {
    audio: AudioEncoding,
}

impl MediaStreamPolicy {
    /// Build the table from the configured audio encoding.
    #[must_use]
    pub fn new(audio: AudioEncoding) -> Self {
        Self { audio }
    }

    /// The kind and encoding for a stream number, or `None` if MC has no policy
    /// for it.
    #[must_use]
    pub fn lookup(&self, stream: StreamNumber) -> Option<(MediaKind, AudioEncoding)> {
        if stream.get() == MAIN_AUDIO_STREAM_NUMBER {
            Some((MediaKind::Audio, self.audio))
        } else {
            None
        }
    }
}

/// Client-facing handler urls, resolved once per connection.
///
/// # Not the other url-keyed map
///
/// Every url here comes from `MhAssignmentData` — **server-derived**, read from
/// Redis, never client-supplied. MC holds a second, similar-looking url-keyed
/// map: `ParticipantActor::mh_statuses`, keyed by the truncated `mh_url` a
/// client sends in a `MediaConnectionUpdate`. **That map must never be a source
/// for this one.** A client-controlled url reaching a `SendTarget` is a redirect
/// primitive — it is MC telling a client where to send its media — and the two
/// maps are one careless lookup apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandlerUrls {
    urls: BTreeMap<HandlerId, String>,
}

impl HandlerUrls {
    /// Build from server-derived `(handler id, client-facing url)` pairs.
    #[must_use]
    pub fn from_pairs(pairs: impl IntoIterator<Item = (HandlerId, String)>) -> Self {
        Self {
            urls: pairs.into_iter().collect(),
        }
    }

    /// This handler's client-facing url.
    #[must_use]
    pub fn get(&self, handler: &HandlerId) -> Option<&str> {
        self.urls.get(handler).map(String::as_str)
    }
}

/// The meeting-wide frame header version MC directs.
///
/// **Derived, never a literal.** The version rides in the signed publisher
/// region, so a relay cannot translate between versions; MC selects one for the
/// whole meeting and the signed field is what makes a downgrade detectable.
///
/// ANCHOR (DRY): `crates/media-protocol/src/frame.rs::PROTOCOL_VERSION`.
///
/// # What this is NOT
///
/// This is a derivation, **not** the version floor or allowlist. ADR-0036 §2 and
/// `signaling.proto` require MC to select from a server-side allowlist with a
/// configured minimum floor and to reject at join a client declaring nothing at
/// or above it. **No story task owns that yet** — `signaling.proto` says so
/// explicitly and `docs/TODO.md` §Media Path Obligations tracks it. Do not read
/// this function as that control: it is latent only because `PROTOCOL_VERSION`
/// is 2 with no v1 downgrade path, and MC adds no code path here that could
/// express a lower version.
#[must_use]
fn header_version() -> u32 {
    u32::from(PROTOCOL_VERSION)
}

/// Compose the send directive for one publisher from the forwarding assignment.
///
/// The target set is **derived from the same [`MeetingAssignment`]** the MC→MH
/// control plane is programmed from — the handlers carrying an egress stream
/// whose candidate sources include this publisher. There is deliberately no
/// parallel loopback derivation: at N=1 the general walk yields the self-edge,
/// and the same walk yields an N-participant answer unchanged.
///
/// An empty target set is a **specified success** (§5: "A target set may be
/// empty. That means send nothing."), reported as
/// [`DirectiveOutcome::EmittedEmptyTargets`] rather than as a failure.
///
/// # Errors
///
/// A [`DirectiveOutcome`] failure variant. Every one fails closed: an unknown
/// stream number is not assumed to be audio, and an unspecified transport mode
/// is not defaulted to datagram.
pub fn build_send_directive(
    publisher: SenderId,
    assignment: &MeetingAssignment,
    handler_urls: &HandlerUrls,
    policy: &MediaStreamPolicy,
) -> Result<(SendDirective, DirectiveOutcome), DirectiveOutcome> {
    // stream number -> targets, ordered so an unchanged meeting produces an
    // unchanged directive. `BTreeMap` over the handler id keeps target order
    // deterministic for the same reason the assignment sorts its streams:
    // byte-identical output for unchanged input is what lets a test assert the
    // directive did not move across a mute cycle.
    let mut by_stream: BTreeMap<StreamNumber, BTreeMap<HandlerId, TransportMode>> = BTreeMap::new();

    for (handler, handler_assignment) in &assignment.per_handler {
        for plan in &handler_assignment.egress_streams {
            if !plan.candidate_sources.contains(&publisher) {
                continue;
            }
            if plan.transport_mode == TransportMode::Unspecified {
                return Err(DirectiveOutcome::TransportModeUnspecified);
            }
            let stream = StreamNumber::from_u8(plan.stream_number);
            if policy.lookup(stream).is_none() {
                return Err(DirectiveOutcome::UnknownStreamNumber);
            }
            by_stream
                .entry(stream)
                .or_default()
                .insert(handler.clone(), plan.transport_mode);
        }
    }

    let mut streams = Vec::with_capacity(by_stream.len());
    let mut any_target = false;

    for (stream, handlers) in by_stream {
        let Some((media_kind, encoding)) = policy.lookup(stream) else {
            return Err(DirectiveOutcome::UnknownStreamNumber);
        };

        let mut targets = Vec::with_capacity(handlers.len());
        for (handler, transport_mode) in handlers {
            let Some(url) = handler_urls.get(&handler) else {
                // Fail loud rather than emit a target with an empty url: a
                // client cannot connect to "" and would report a media
                // connection failure MC could not explain.
                return Err(DirectiveOutcome::HandlerUrlUnresolved);
            };
            targets.push(SendTarget {
                media_handler_url: url.to_string(),
                transport_mode: transport_mode as i32,
            });
        }

        any_target |= !targets.is_empty();
        streams.push(SendStream {
            // 8-bit domain widened into the proto's uint32. `u32::from`, never
            // `as` — the narrowing direction is the fallible one and lives in
            // `StreamNumber::from_wire`.
            stream_number: u32::from(stream.get()),
            media_kind: media_kind as i32,
            encoding: Some(encoding.to_proto()),
            targets,
        });
    }

    let outcome = if any_target {
        DirectiveOutcome::Emitted
    } else {
        DirectiveOutcome::EmittedEmptyTargets
    };

    Ok((
        SendDirective {
            streams,
            header_version: header_version(),
        },
        outcome,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::{
        compute_assignment, MeetingRoutingInput, RoutingParticipant, MAIN_AUDIO_SLOT_ID,
    };
    use std::num::NonZeroU16;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn encoding() -> AudioEncoding {
        AudioEncoding::new(Codec::Opus, 48_000, 50).unwrap()
    }

    fn loopback(handlers: &[&str]) -> (MeetingAssignment, HandlerUrls) {
        let ids: Vec<HandlerId> = handlers.iter().map(|h| HandlerId::new(*h)).collect();
        let input = MeetingRoutingInput {
            participants: vec![RoutingParticipant {
                sender_id: sender(1),
                handlers: ids.clone(),
            }],
            handlers: ids.clone(),
        };
        let urls = HandlerUrls::from_pairs(
            ids.iter()
                .map(|h| (h.clone(), format!("https://{h}.example:4434"))),
        );
        (compute_assignment(&input).unwrap(), urls)
    }

    #[test]
    fn header_version_is_derived_not_a_literal() {
        // Compared to the anchor, never to `2`: if `PROTOCOL_VERSION` moves and
        // MC does not, this fails.
        assert_eq!(header_version(), u32::from(PROTOCOL_VERSION));
    }

    #[test]
    fn loopback_directive_names_one_audio_stream_on_one_datagram_target() {
        let (assignment, urls) = loopback(&["mh-0"]);
        let (directive, outcome) = build_send_directive(
            sender(1),
            &assignment,
            &urls,
            &MediaStreamPolicy::new(encoding()),
        )
        .unwrap();

        assert_eq!(outcome, DirectiveOutcome::Emitted);
        assert_eq!(directive.header_version, u32::from(PROTOCOL_VERSION));
        assert_eq!(directive.streams.len(), 1);

        let stream = &directive.streams[0];
        assert_eq!(stream.stream_number, u32::from(MAIN_AUDIO_STREAM_NUMBER));
        assert_eq!(stream.media_kind, MediaKind::Audio as i32);

        let params = stream.encoding.as_ref().unwrap();
        assert_eq!(params.codec, Codec::Opus as i32);
        assert_ne!(params.codec, Codec::Unspecified as i32);
        assert_eq!(params.max_bitrate_bps, 48_000);
        assert_eq!(params.frame_rate, 50);
        // Audio carries no geometry: zero is the absence of a constraint.
        assert_eq!(params.width, 0);
        assert_eq!(params.height, 0);

        assert_eq!(stream.targets.len(), 1);
        assert_eq!(
            stream.targets[0].media_handler_url,
            "https://mh-0.example:4434"
        );
        assert_eq!(
            stream.targets[0].transport_mode,
            TransportMode::Datagram as i32
        );
    }

    #[test]
    fn a_publisher_nobody_watches_gets_an_empty_target_set() {
        // ADR-0036 §5: an empty target set is a SPECIFIED SUCCESS ("that means
        // send nothing"), not a failure — so the outcome must sit in the success
        // set, or an `outcome != "emitted"` alert would page on a legal state.
        let (assignment, urls) = loopback(&["mh-0"]);
        let (directive, outcome) = build_send_directive(
            sender(9),
            &assignment,
            &urls,
            &MediaStreamPolicy::new(encoding()),
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::EmittedEmptyTargets);
        assert!(outcome.is_success());
        assert!(directive.streams.is_empty());
    }

    #[test]
    fn an_unresolvable_handler_url_fails_loud_rather_than_emitting_an_empty_url() {
        let (assignment, _) = loopback(&["mh-0"]);
        let outcome = build_send_directive(
            sender(1),
            &assignment,
            &HandlerUrls::default(),
            &MediaStreamPolicy::new(encoding()),
        );
        assert_eq!(outcome, Err(DirectiveOutcome::HandlerUrlUnresolved));
    }

    #[test]
    fn the_directive_carries_no_priority_group() {
        // Structural, not observed: no priority-group field exists on
        // SendDirective/SendStream/SendTarget. A client able to name its own
        // group could promote itself past the salience bound §7 relies on.
        // This test documents the property; the proto enforces it.
        let (assignment, urls) = loopback(&["mh-0"]);
        let (directive, _) = build_send_directive(
            sender(1),
            &assignment,
            &urls,
            &MediaStreamPolicy::new(encoding()),
        )
        .unwrap();
        let encoded = format!("{directive:?}");
        assert!(!encoded.contains("priority"));
    }

    #[test]
    fn stream_number_from_wire_fails_closed_and_never_wraps() {
        // The one seam a wider value can enter through. 256 must be an ERROR,
        // not 0 — truncation would alias two of one sender's streams inside the
        // signed key id, which is a nonce-reuse hazard, not a routing nuisance.
        assert_eq!(StreamNumber::from_wire(255).unwrap().get(), 255);
        assert_eq!(
            StreamNumber::from_wire(256),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
        assert_eq!(
            StreamNumber::from_wire(65_536),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
        assert_eq!(
            StreamNumber::from_wire(u32::MAX),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
    }

    #[test]
    fn an_unknown_stream_number_is_never_assumed_to_be_audio() {
        let policy = MediaStreamPolicy::new(encoding());
        assert!(policy
            .lookup(StreamNumber::from_u8(MAIN_AUDIO_STREAM_NUMBER))
            .is_some());
        assert!(policy.lookup(StreamNumber::from_u8(1)).is_none());
        assert!(policy.lookup(StreamNumber::from_u8(255)).is_none());
    }

    /// Build a one-plan assignment with fields the caller chooses, so the two
    /// fail-closed guards INSIDE `build_send_directive` can be driven.
    ///
    /// The neighbouring tests exercise `policy.lookup` and
    /// `StreamNumber::from_wire` directly; neither reaches the guards in the
    /// build path, which are the ones that decide whether a defaulted-datagram
    /// or assumed-audio directive can ever be EMITTED.
    fn assignment_with(stream_number: u8, transport_mode: TransportMode) -> MeetingAssignment {
        use crate::media_routing::{HandlerAssignment, MAIN_AUDIO_SLOT_ID};
        let mut per_handler = BTreeMap::new();
        per_handler.insert(
            HandlerId::new("mh-0"),
            HandlerAssignment {
                egress_streams: vec![crate::media_routing::EgressStreamPlan {
                    egress_stream_id: 1,
                    subscriber: sender(1),
                    slot_id: MAIN_AUDIO_SLOT_ID,
                    candidate_sources: vec![sender(1)],
                    stream_number,
                    priority_group: 0,
                    supersede_on_independent_frame: false,
                    transport_mode,
                }],
            },
        );
        MeetingAssignment { per_handler }
    }

    fn urls_for_mh0() -> HandlerUrls {
        HandlerUrls::from_pairs(vec![(
            HandlerId::new("mh-0"),
            "https://mh-0.example:4434".to_string(),
        )])
    }

    #[test]
    fn an_unspecified_transport_mode_is_never_defaulted_to_datagram() {
        // The proto zero is a rejection, not a convenience. Emitting a
        // datagram-defaulted directive would have MC silently choose a
        // transport the assignment never named.
        let assignment = assignment_with(MAIN_AUDIO_STREAM_NUMBER, TransportMode::Unspecified);
        assert_eq!(
            build_send_directive(
                sender(1),
                &assignment,
                &urls_for_mh0(),
                &MediaStreamPolicy::new(encoding()),
            ),
            Err(DirectiveOutcome::TransportModeUnspecified)
        );
    }

    #[test]
    fn a_plan_naming_an_unknown_stream_number_emits_nothing() {
        // Stream number 1 is absent from the policy table. There is no "assume
        // audio" fallback: guessing a stream's kind is how a client ends up
        // encrypting video parameters into an audio key stream.
        let assignment = assignment_with(1, TransportMode::Datagram);
        assert_eq!(
            build_send_directive(
                sender(1),
                &assignment,
                &urls_for_mh0(),
                &MediaStreamPolicy::new(encoding()),
            ),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
    }

    #[test]
    fn the_same_assignment_shape_succeeds_when_both_fields_are_well_formed() {
        // The control test for the two above: without it, a build_send_directive
        // that rejected EVERY hand-built assignment would pass both and prove
        // nothing about the two specific guards.
        let assignment = assignment_with(MAIN_AUDIO_STREAM_NUMBER, TransportMode::Datagram);
        let (directive, outcome) = build_send_directive(
            sender(1),
            &assignment,
            &urls_for_mh0(),
            &MediaStreamPolicy::new(encoding()),
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::Emitted);
        assert_eq!(directive.streams.len(), 1);
    }

    #[test]
    fn codec_parsing_fails_loud_and_never_falls_back() {
        assert_eq!(AudioEncoding::parse_codec("opus").unwrap(), Codec::Opus);
        assert_eq!(AudioEncoding::parse_codec("  OPUS ").unwrap(), Codec::Opus);
        assert_eq!(AudioEncoding::parse_codec("Opus").unwrap(), Codec::Opus);
        // A near-miss must NOT silently become Opus.
        assert_eq!(
            AudioEncoding::parse_codec("oppus"),
            Err(AudioEncodingError::UnknownCodec)
        );
        assert_eq!(
            AudioEncoding::parse_codec(""),
            Err(AudioEncodingError::UnknownCodec)
        );
        assert_eq!(
            AudioEncoding::parse_codec("unspecified"),
            Err(AudioEncodingError::CodecUnspecified)
        );
    }

    #[test]
    fn codec_unspecified_is_rejected_at_construction_not_at_emit() {
        // This is what makes CODEC_UNSPECIFIED structurally unrepresentable in a
        // send directive: there is no `AudioEncoding` holding it, so no emit
        // site can produce one.
        assert_eq!(
            AudioEncoding::new(Codec::Unspecified, 48_000, 50),
            Err(AudioEncodingError::CodecUnspecified)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Vp9, 48_000, 50),
            Err(AudioEncodingError::CodecNotAudio)
        );
        assert_eq!(
            AudioEncoding::new(Codec::H264, 48_000, 50),
            Err(AudioEncodingError::CodecNotAudio)
        );
    }

    #[test]
    fn encoding_values_are_bounded_against_the_band_mh_sizes_against() {
        assert!(AudioEncoding::new(Codec::Opus, AUDIO_BITRATE_MIN_BPS, 50).is_ok());
        assert!(AudioEncoding::new(Codec::Opus, AUDIO_BITRATE_MAX_BPS, 50).is_ok());
        assert_eq!(
            AudioEncoding::new(Codec::Opus, AUDIO_BITRATE_MIN_BPS - 1, 50),
            Err(AudioEncodingError::BitrateOutOfBand)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Opus, 0, 50),
            Err(AudioEncodingError::BitrateOutOfBand)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Opus, AUDIO_BITRATE_MAX_BPS + 1, 50),
            Err(AudioEncodingError::BitrateOutOfBand)
        );
        assert!(AudioEncoding::new(Codec::Opus, 48_000, AUDIO_FRAME_RATE_MIN_HZ).is_ok());
        assert!(AudioEncoding::new(Codec::Opus, 48_000, AUDIO_FRAME_RATE_MAX_HZ).is_ok());
        assert_eq!(
            AudioEncoding::new(Codec::Opus, 48_000, AUDIO_FRAME_RATE_MAX_HZ + 1),
            Err(AudioEncodingError::FrameRateOutOfBand)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Opus, 48_000, 0),
            Err(AudioEncodingError::FrameRateOutOfBand)
        );
    }

    #[test]
    fn the_directive_is_byte_identical_for_unchanged_input() {
        // What makes "the directive did not move across a mute cycle" assertable
        // as bytes rather than as a field-by-field comparison.
        use prost::Message;
        let (assignment, urls) = loopback(&["mh-1", "mh-0", "mh-2"]);
        let policy = MediaStreamPolicy::new(encoding());
        let (a, _) = build_send_directive(sender(1), &assignment, &urls, &policy).unwrap();
        let (b, _) = build_send_directive(sender(1), &assignment, &urls, &policy).unwrap();
        assert_eq!(a.encode_to_vec(), b.encode_to_vec());
    }

    #[test]
    fn the_planned_slot_constant_is_the_assignments_own_answer() {
        // The directive path and the assignment path must not develop two
        // answers to "which slot"; `compute_assignment` is the sole producer.
        let (assignment, _) = loopback(&["mh-0"]);
        let plan = assignment
            .per_handler
            .values()
            .flat_map(|h| h.egress_streams.iter())
            .find(|p| p.subscriber == sender(1))
            .unwrap();
        assert_eq!(plan.slot_id, MAIN_AUDIO_SLOT_ID);
    }

    /// The directive's target handler IS the handler carrying this publisher's
    /// edges, at N=2 handlers.
    ///
    /// # What this rejects, by name
    ///
    /// `edge_handler` places every edge on the lexicographically smallest shared
    /// handler, so with `{mh-0, mh-1}` every edge sits on **mh-0** and mh-1 holds
    /// a correct-by-design EMPTY edge set. The nameable wrong answer is therefore
    /// `https://mh-1.example:4434` — which is exactly what list-order selection
    /// produced on the live cluster, where three of four media connections landed
    /// on mh-1 while every edge sat on mh-0 (story task 24 escalation,
    /// `docs/devloop-outputs/2026-09-05-sender-id-binding-contract/main.md`
    /// §Resume). The `assert_ne!` below is that injected adverse condition, not
    /// a redundant assertion.
    ///
    /// The expected url is a WRITTEN LITERAL, deliberately not sourced from any
    /// helper the production path also calls: a test that computes its
    /// expectation the same way the code does passes through the divergence it
    /// exists to catch. Same discipline as
    /// `crates/proto-gen/tests/internal_roundtrip.rs`'s `65_535`/`65_536` —
    /// production code references the bound, boundary tests restate it. Do not
    /// hoist these to a constant.
    #[test]
    fn the_directive_targets_the_handler_carrying_this_publishers_edges_at_n_2() {
        let (assignment, urls) = loopback(&["mh-0", "mh-1"]);

        // Premise check: the assignment really does place this publisher's edges
        // on exactly one handler, and that handler is mh-0. Without this the
        // assertions below could pass over an assignment that placed nothing.
        let carrying: Vec<&HandlerId> = assignment
            .per_handler
            .iter()
            .filter(|(_, h)| {
                h.egress_streams
                    .iter()
                    .any(|p| p.candidate_sources.contains(&sender(1)))
            })
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            carrying,
            vec![&HandlerId::new("mh-0")],
            "premise: exactly one handler carries the publisher's edges, and it is mh-0"
        );
        // And the OTHER handler is present with an empty edge set — the
        // correct-by-design state a client used to be steered into.
        assert!(assignment
            .per_handler
            .get(&HandlerId::new("mh-1"))
            .expect("every input handler gets an entry")
            .egress_streams
            .is_empty());

        let (directive, outcome) = build_send_directive(
            sender(1),
            &assignment,
            &urls,
            &MediaStreamPolicy::new(encoding()),
        )
        .unwrap();

        assert_eq!(outcome, DirectiveOutcome::Emitted);
        assert_eq!(
            directive.streams.len(),
            1,
            "one publisher stream this story"
        );
        let targets = &directive.streams[0].targets;
        assert_eq!(
            targets.len(),
            1,
            "one client, one directed handler; >1 target means §9 multi-handler send landed and \
             this test must be revisited rather than relaxed"
        );
        assert_eq!(
            targets[0].media_handler_url, "https://mh-0.example:4434",
            "MC must steer the client to the handler its edges were placed on"
        );
        assert_ne!(
            targets[0].media_handler_url, "https://mh-1.example:4434",
            "mh-1 holds a correct-by-design EMPTY edge set; steering a client there is the defect \
             story task 25 fixes — three of four live-cluster media connections landed on mh-1 \
             while every edge sat on mh-0"
        );
    }

    /// Redis enumeration order cannot change where the client is steered.
    ///
    /// # This is the unit-tier SHADOW of the real proof, and says so on purpose
    ///
    /// At THIS tier the inversion is largely normalised away before the code
    /// under test runs: `MeetingAssignment::per_handler` is a `BTreeMap`,
    /// `edge_handler` does `shared.sort()`, and `build_send_directive`
    /// accumulates into a `BTreeMap<HandlerId, _>`. So a permutation test built
    /// on a hand-made [`MeetingRoutingInput`] asserts a property the TYPES
    /// guarantee and could not fail — review-protocol §Assertion Vacuity
    /// mechanism 4.
    ///
    /// The seam whose order genuinely varies is the Redis `MhAssignmentData.handlers`
    /// `Vec`, which reaches the outcome through two private paths in
    /// `webtransport/connection.rs` (`routing_input_for` into the assignment,
    /// `HandlerUrls::from_pairs` into the urls) and is only drivable through a
    /// real join. **The load-bearing proof is therefore
    /// `crates/mc-service/tests/media_client_signaling_integration.rs`'s
    /// `redis_enumeration_order_cannot_change_where_the_client_is_steered`.**
    /// This test is kept as the cheap local regression pin; do not read its green
    /// as covering the seam, and do not "add symmetry" by writing more of the
    /// inversion at this tier.
    #[test]
    fn handler_list_order_does_not_move_the_directive_at_this_tier() {
        use prost::Message;
        let policy = MediaStreamPolicy::new(encoding());
        let (ascending, urls_a) = loopback(&["mh-0", "mh-1"]);
        let (inverted, urls_b) = loopback(&["mh-1", "mh-0"]);

        let (a, _) = build_send_directive(sender(1), &ascending, &urls_a, &policy).unwrap();
        let (b, _) = build_send_directive(sender(1), &inverted, &urls_b, &policy).unwrap();

        // Byte-identical AND non-empty naming mh-0: equality alone is green when
        // both sides are empty, so the literal is what gives this teeth.
        assert_eq!(a.encode_to_vec(), b.encode_to_vec());
        for directive in [&a, &b] {
            assert_eq!(directive.streams.len(), 1);
            assert_eq!(directive.streams[0].targets.len(), 1);
            assert_eq!(
                directive.streams[0].targets[0].media_handler_url,
                "https://mh-0.example:4434"
            );
        }
    }
}
