//! What MC directs a client to **produce** (ADR-0036 §5).
//!
//! # Server mute and watch state, never client mute
//!
//! The directive depends on exactly two things about a publisher's stream:
//! whether anyone WATCHES it (its rendered out-edges) and whether its kind is
//! SERVER-muted. Either one empties the stream's targets -- a withdrawal, never
//! a removal (ADR-0036 §5, amendment 2026-10-23; story 3 R-8, R-16).
//!
//! It NEVER depends on CLIENT mute. That keeps *"MC has not asked you to send"*
//! distinct from *"you have muted yourself"*, and makes resuming from a client
//! mute a purely local decision with no round trip. The invariant is
//! structural, not a comment: [`build_send_directive`]'s only mute input is a
//! [`ServerMutedKinds`], a type DISTINCT from the roster's
//! [`crate::media_kind::ClientMutedKinds`], with no conversion between them, and
//! nothing here imports the roster's combined mute projection. A client
//! mute/unmute cycle altering a directive is therefore a **compile error**:
//!
//! ```compile_fail
//! use mc_service::media_kind::ClientMutedKinds;
//! use mc_service::media_signaling::directive::build_send_directive;
//! fn takes_server_mute<A, B, C, D, E, R>(_f: impl Fn(A, B, C, D, E, ClientMutedKinds) -> R) {}
//! takes_server_mute(build_send_directive);
//! ```
//!
//! The positive control below is byte-identical except for that one type, so
//! the `compile_fail` above fails for the type and nothing else:
//!
//! ```
//! use mc_service::media_kind::ServerMutedKinds;
//! use mc_service::media_signaling::directive::build_send_directive;
//! fn takes_server_mute<A, B, C, D, E, R>(_f: impl Fn(A, B, C, D, E, ServerMutedKinds) -> R) {}
//! takes_server_mute(build_send_directive);
//! ```
//!
//! Client mute lives in [`super::assignments`] (what a *subscriber is told
//! about a source*) and in the MH disposition (PAUSED), never in what a
//! *publisher is told to produce*. If you find yourself needing client-mute
//! state here, the change you want is in `assignments`.
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

use super::outcome::{DirectiveOutcome, MediaKindLabel};
use crate::media_admission::SenderId;
use crate::media_kind::{PublishSet, ServerMutedKinds};
use crate::media_routing::assignment::{stream_number_of, transport_of};
use crate::media_routing::{HandlerId, MeetingAssignment, MeetingHandlers};
use media_protocol::frame::PROTOCOL_VERSION;
use proto_gen::dark_tower::signaling::v1::{
    Codec, EncodingParameters, SendDirective, SendStream, SendTarget, TransportMode,
};

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
/// A configured codec NAME could not be parsed ([`parse_codec`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecParseError {
    /// The name matched nothing in the closed set.
    UnknownCodec,
    /// The name was `unspecified`, which is never valid in a directive.
    CodecUnspecified,
}

impl std::fmt::Display for CodecParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnknownCodec => "not a recognised codec",
            Self::CodecUnspecified => {
                "'unspecified' is never valid in a send directive; name a codec"
            }
        })
    }
}

impl std::error::Error for CodecParseError {}

/// Parse a configured codec name into the closed proto enum — the ONE
/// codec-name parser for both kinds; each encoding's constructor then checks
/// the codec is of its kind ([`AudioEncoding::new`], [`VideoEncoding::new`]).
///
/// Trimmed and case-folded, then matched exactly. An unrecognised value is an
/// **error**, never a fall-back to a default: silently coercing `"oppus"` to
/// Opus would leave an operator believing they had configured something they
/// had not.
///
/// # Errors
///
/// [`CodecParseError`].
pub fn parse_codec(raw: &str) -> Result<Codec, CodecParseError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "opus" => Ok(Codec::Opus),
        "vp8" => Ok(Codec::Vp8),
        "vp9" => Ok(Codec::Vp9),
        "av1" => Ok(Codec::Av1),
        "h264" => Ok(Codec::H264),
        "unspecified" => Err(CodecParseError::CodecUnspecified),
        _ => Err(CodecParseError::UnknownCodec),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEncodingError {
    /// `CODEC_UNSPECIFIED`, which is never valid in a directive.
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
            Codec::Vp8 | Codec::Vp9 | Codec::Av1 | Codec::H264 => {
                return Err(AudioEncodingError::CodecNotAudio)
            }
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
            // Video only (signaling.proto `EncodingParameters`); audio ignores
            // it, so it stays absent here.
            min_keyframe_interval_ms: None,
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

/// A configured value could not be turned into a usable video encoding.
///
/// Every variant is a BOOT REFUSAL (`config.rs` names the key, the value and
/// the bound in the returned error).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncodingError {
    /// Not VP8, the one video codec this story directs (story 3 R-8).
    CodecNotVp8,
    /// Width or height outside [`VIDEO_DIMENSION_MIN_PX`]`..=`[`VIDEO_DIMENSION_MAX_PX`].
    DimensionOutOfBand,
    /// Frame rate outside [`VIDEO_FRAME_RATE_MIN_HZ`]`..=`[`VIDEO_FRAME_RATE_MAX_HZ`].
    FrameRateOutOfBand,
    /// Bitrate outside [`VIDEO_BITRATE_MIN_BPS`]`..=`[`VIDEO_BITRATE_MAX_BPS`].
    BitrateOutOfBand,
    /// Minimum keyframe interval outside
    /// [`MIN_KEYFRAME_INTERVAL_MS_FLOOR`]`..=`[`MIN_KEYFRAME_INTERVAL_MS_CEILING`].
    MinKeyframeIntervalOutOfBand,
}

impl std::fmt::Display for VideoEncodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The bands are read from the constants below, never restated, so a
        // band change cannot leave the boot message wrong.
        match self {
            Self::CodecNotVp8 => f.write_str("not vp8, the only video codec MC directs"),
            Self::DimensionOutOfBand => write!(
                f,
                "outside the video dimension band {VIDEO_DIMENSION_MIN_PX}..={VIDEO_DIMENSION_MAX_PX} px"
            ),
            Self::FrameRateOutOfBand => write!(
                f,
                "outside the video frame-rate band {VIDEO_FRAME_RATE_MIN_HZ}..={VIDEO_FRAME_RATE_MAX_HZ} Hz"
            ),
            Self::BitrateOutOfBand => write!(
                f,
                "outside the video bitrate band {VIDEO_BITRATE_MIN_BPS}..={VIDEO_BITRATE_MAX_BPS} bps"
            ),
            Self::MinKeyframeIntervalOutOfBand => write!(
                f,
                "outside the minimum keyframe interval band \
                 {MIN_KEYFRAME_INTERVAL_MS_FLOOR}..={MIN_KEYFRAME_INTERVAL_MS_CEILING} ms"
            ),
        }
    }
}

impl std::error::Error for VideoEncodingError {}

/// Smallest video width or height MC directs, in pixels.
pub const VIDEO_DIMENSION_MIN_PX: u32 = 16;
/// Largest video width or height MC directs, in pixels.
pub const VIDEO_DIMENSION_MAX_PX: u32 = 1920;
/// Lowest video frame rate MC directs, in frames per second.
pub const VIDEO_FRAME_RATE_MIN_HZ: u32 = 1;
/// Highest video frame rate MC directs, in frames per second.
pub const VIDEO_FRAME_RATE_MAX_HZ: u32 = 60;
/// Lowest video bitrate MC directs, in bits per second.
///
/// MH derives its per-stream video cost from the SAME `media-codec-profile`
/// value (`MEDIA_VIDEO_MAX_BITRATE_BPS`), so there is no MC-side guard against
/// MH's sizing: the profile is the single source (story 3 R-24). MH's own
/// non-zero check lives in `crates/mh-service/src/config.rs`
/// (`MediaCodecProfile`); this band is the stricter boot gate.
pub const VIDEO_BITRATE_MIN_BPS: u32 = 100_000;
/// Highest video bitrate MC directs, in bits per second. See
/// [`VIDEO_BITRATE_MIN_BPS`].
pub const VIDEO_BITRATE_MAX_BPS: u32 = 4_000_000;

/// The lowest minimum keyframe interval MC will direct, in milliseconds.
///
/// DRIFT SIGNAL: the client compiles its own floor (the SDK keyframe floor in
/// `clientConfig.ts`, added by the client publish-path task, story 3 C-A2) and
/// enforces max(directed, compiled floor). That task's keyframe-floor sync
/// guard, `scripts/guards/simple/validate-keyframe-floor-sync.sh`, greps THIS
/// literal and compares it with the SDK's; a directed value the client had to
/// raise is counted client-side as `keyframe_min_interval_clamped`. Keep this
/// a single literal so the guard can read it.
pub const MIN_KEYFRAME_INTERVAL_MS_FLOOR: u32 = 100;

/// The highest minimum keyframe interval MC will direct, in milliseconds.
///
/// DRIFT SIGNAL: mirrored by the SDK's `KEYFRAME_MIN_INTERVAL_CEILING_MS`
/// (`packages/sdk-core/src/config/clientConfig.ts`), which REFUSES a directed
/// value above it; pinned by
/// `scripts/guards/simple/validate-keyframe-floor-sync.sh` (see
/// [`MIN_KEYFRAME_INTERVAL_MS_FLOOR`]). Keep this a single literal.
pub const MIN_KEYFRAME_INTERVAL_MS_CEILING: u32 = 10_000;

/// The encoding parameters MC directs for main (camera) video.
///
/// Constructible only through [`VideoEncoding::new`], which enforces every
/// band above; the fields are private, so an unchecked value is
/// unrepresentable at the emit site. Every field is ALWAYS set on the wire:
/// the client refuses a video stream missing any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoEncoding {
    codec: Codec,
    width: u32,
    height: u32,
    frame_rate_hz: u32,
    max_bitrate_bps: u32,
    min_keyframe_interval_ms: u32,
}

impl VideoEncoding {
    /// Validate configured values into an encoding MC can direct.
    ///
    /// # Errors
    ///
    /// [`VideoEncodingError`] for the first value outside its band.
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        frame_rate_hz: u32,
        max_bitrate_bps: u32,
        min_keyframe_interval_ms: u32,
    ) -> Result<Self, VideoEncodingError> {
        if codec != Codec::Vp8 {
            return Err(VideoEncodingError::CodecNotVp8);
        }
        let dims = VIDEO_DIMENSION_MIN_PX..=VIDEO_DIMENSION_MAX_PX;
        if !dims.contains(&width) || !dims.contains(&height) {
            return Err(VideoEncodingError::DimensionOutOfBand);
        }
        if !(VIDEO_FRAME_RATE_MIN_HZ..=VIDEO_FRAME_RATE_MAX_HZ).contains(&frame_rate_hz) {
            return Err(VideoEncodingError::FrameRateOutOfBand);
        }
        if !(VIDEO_BITRATE_MIN_BPS..=VIDEO_BITRATE_MAX_BPS).contains(&max_bitrate_bps) {
            return Err(VideoEncodingError::BitrateOutOfBand);
        }
        if !(MIN_KEYFRAME_INTERVAL_MS_FLOOR..=MIN_KEYFRAME_INTERVAL_MS_CEILING)
            .contains(&min_keyframe_interval_ms)
        {
            return Err(VideoEncodingError::MinKeyframeIntervalOutOfBand);
        }
        Ok(Self {
            codec,
            width,
            height,
            frame_rate_hz,
            max_bitrate_bps,
            min_keyframe_interval_ms,
        })
    }

    /// The wire form: every field set, including `min_keyframe_interval_ms`.
    #[must_use]
    pub fn to_proto(self) -> EncodingParameters {
        EncodingParameters {
            codec: self.codec as i32,
            max_bitrate_bps: self.max_bitrate_bps,
            width: self.width,
            height: self.height,
            frame_rate: self.frame_rate_hz,
            min_keyframe_interval_ms: Some(self.min_keyframe_interval_ms),
        }
    }

    /// The configured codec.
    #[must_use]
    pub fn codec(self) -> Codec {
        self.codec
    }
    /// The configured width, in pixels.
    #[must_use]
    pub fn width(self) -> u32 {
        self.width
    }
    /// The configured height, in pixels.
    #[must_use]
    pub fn height(self) -> u32 {
        self.height
    }
    /// The configured frame rate, in frames per second.
    #[must_use]
    pub fn frame_rate_hz(self) -> u32 {
        self.frame_rate_hz
    }
    /// The configured maximum bitrate, in bits per second.
    #[must_use]
    pub fn max_bitrate_bps(self) -> u32 {
        self.max_bitrate_bps
    }
    /// The directed minimum keyframe interval, in milliseconds — the ONE value
    /// both the directive and `mc_media_video_keyframe_min_interval_seconds`
    /// read.
    #[must_use]
    pub fn min_keyframe_interval_ms(self) -> u32 {
        self.min_keyframe_interval_ms
    }
}

/// What each stream number a publisher may be asked to produce actually is.
///
/// MH is type-blind by design (§7), so the forwarding assignment carries no
/// media kind — it names behaviours. MC, which is not type-blind, holds the
/// mapping here: stream `MAIN_AUDIO_STREAM_NUMBER` (0) is audio, stream
/// `MAIN_VIDEO_STREAM_NUMBER` (1) is camera video (`media_routing::assignment`).
///
/// A plan naming a stream number absent from this table is an **MC defect** and
/// fails loudly: there is no "assume audio" fallback, because guessing the kind
/// of a stream is how a client ends up encrypting video parameters into an audio
/// key stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaStreamPolicy {
    audio: AudioEncoding,
    video: VideoEncoding,
}

impl MediaStreamPolicy {
    /// Build the table from the configured encodings.
    #[must_use]
    pub fn new(audio: AudioEncoding, video: VideoEncoding) -> Self {
        Self { audio, video }
    }

    /// The configured video encoding.
    #[must_use]
    pub fn video(&self) -> VideoEncoding {
        self.video
    }

    /// The configured audio encoding.
    #[must_use]
    pub fn audio(&self) -> AudioEncoding {
        self.audio
    }

    /// The kind for a stream number, or `None` if MC has no policy for it.
    #[must_use]
    pub fn kind_of(&self, stream: StreamNumber) -> Option<MediaKindLabel> {
        MediaKindLabel::ALL
            .into_iter()
            .find(|k| stream_number_of(*k) == stream.get())
    }

    /// The wire stream (number, kind, encoding, transport) for a kind.
    fn stream_for(&self, kind: MediaKindLabel) -> (u8, EncodingParameters, TransportMode) {
        (
            stream_number_of(kind),
            kind.pick(self.audio.to_proto(), self.video.to_proto()),
            transport_of(kind),
        )
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
/// v3 is the ONLY header version (story 3 R-22): there is no allowlist and no
/// floor. The join-time refusal of a client whose `supported_header_versions`
/// lacks it lives in `webtransport::connection::validate_join_media`, against
/// the same `PROTOCOL_VERSION`.
#[must_use]
fn header_version() -> u32 {
    u32::from(PROTOCOL_VERSION)
}

/// Compose the send directive for one publisher from the forwarding assignment.
///
/// # Every published kind, always; an unpublished kind, never
///
/// The directive lists ONE stream per kind in the publisher's fixed publish
/// set (story 3 R-2, R-8), audio first, and no stream for a kind it does not
/// publish. A stream's targets are the handlers carrying at least one rendered
/// out-edge OF THAT STREAM — derived from the same [`MeetingAssignment`] the
/// MC→MH control plane is programmed from (ADR-0036 §9 multi-handler send). An
/// UNWATCHED stream (no out-edges: a solo participant, one not yet connected,
/// or a camera nobody holds in a video slot) is listed with EMPTY targets:
/// "send nothing" (§5), a specified success. Withdrawal is empty targets,
/// never removal, so the last watcher's leave withdraws and a new watcher
/// restores within the same structural change.
///
/// One target per handler, never per edge: the client encrypts each frame
/// once and transmits it to every target. Target urls come from the meeting's
/// frozen [`MeetingHandlers`], the same value `JoinResponse.media_servers` and
/// `StreamAssignment.media_handler_url` are read from. Each target's transport
/// is its stream's kind's ([`transport_of`]): datagrams for audio,
/// stream-per-group for video.
///
/// # A server-muted kind is withdrawn
///
/// A kind in `server_muted` is listed with EMPTY targets whatever its edges
/// (which stay rendered, DROP_AT_INGRESS, so MH counts a patched client's
/// frames as `server_muted` drops): capture stops, and for video the camera
/// light goes off (story 3 R-16). Lifting it restores the targets in the same
/// structural change, whatever the client mute -- there is no client-mute
/// input (module doc).
///
/// The third element is each stream's `media_kind` metric label, PARALLEL to
/// `SendDirective.streams` (same length, same order).
///
/// # Errors
///
/// A [`DirectiveOutcome`] failure variant. Every one fails closed: a plan
/// naming an unknown stream number is not assumed to be audio, and a target
/// handler with no url is never emitted as an empty url.
pub fn build_send_directive(
    publisher: SenderId,
    publish_set: PublishSet,
    assignment: &MeetingAssignment,
    meeting_handlers: &MeetingHandlers,
    policy: &MediaStreamPolicy,
    server_muted: ServerMutedKinds,
) -> Result<(SendDirective, DirectiveOutcome, Vec<MediaKindLabel>), DirectiveOutcome> {
    // kind -> owning handlers, ordered so an unchanged meeting produces an
    // unchanged directive (byte-identical output for unchanged input).
    let mut owners: [std::collections::BTreeSet<HandlerId>; 2] = Default::default();
    for (handler, handler_assignment) in &assignment.per_handler {
        for plan in &handler_assignment.egress_streams {
            if !plan.candidate_sources.contains(&publisher) {
                continue;
            }
            let Some(kind) = policy.kind_of(StreamNumber::from_u8(plan.stream_number)) else {
                return Err(DirectiveOutcome::UnknownStreamNumber);
            };
            kind.of_mut(&mut owners).insert(handler.clone());
        }
    }

    let mut streams = Vec::with_capacity(2);
    let mut stream_kinds = Vec::with_capacity(2);
    let mut any_target = false;

    for kind in publish_set.kinds() {
        let (stream_number, encoding, transport_mode) = policy.stream_for(kind);
        if transport_mode == TransportMode::Unspecified {
            return Err(DirectiveOutcome::TransportModeUnspecified);
        }
        // A server-muted kind is withdrawn: its edges stay (DROP_AT_INGRESS at
        // MH), but the publisher is told to send it nowhere.
        let withdrawn = server_muted.is_muted(kind);
        let handlers = kind.of_ref(&owners);
        let mut targets = Vec::with_capacity(handlers.len());
        for handler in handlers.iter().filter(|_| !withdrawn) {
            let Some(url) = meeting_handlers.url_of(handler) else {
                // Fail loud rather than emit a target with an empty url.
                return Err(DirectiveOutcome::HandlerUrlUnresolved);
            };
            targets.push(SendTarget {
                media_handler_url: url.to_string(),
                transport_mode: transport_mode as i32,
            });
        }
        any_target |= !targets.is_empty();
        streams.push(SendStream {
            // `u32::from`, never `as`: the narrowing direction is the fallible
            // one and lives in `StreamNumber::from_wire`.
            stream_number: u32::from(stream_number),
            media_kind: kind.to_proto() as i32,
            encoding: Some(encoding),
            targets,
        });
        stream_kinds.push(kind);
    }

    // A plan naming this publisher on a kind it does not publish is an MC
    // defect (the render only edges published streams): fail closed rather
    // than direct a stream the publish set does not contain.
    for kind in MediaKindLabel::ALL {
        if !publish_set.publishes(kind) && !kind.of_ref(&owners).is_empty() {
            return Err(DirectiveOutcome::UnknownStreamNumber);
        }
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
        stream_kinds,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::media_routing::{
        HandlerEndpoint, SlotTable, MAIN_AUDIO_STREAM_NUMBER, MAIN_VIDEO_STREAM_NUMBER,
    };
    use proto_gen::dark_tower::signaling::v1::MediaKind;
    use std::collections::BTreeMap;
    use std::num::NonZeroU16;

    const AU: MediaKindLabel = MediaKindLabel::Audio;
    const VI: MediaKindLabel = MediaKindLabel::Video;

    fn sender(n: u16) -> SenderId {
        SenderId::from_nonzero(NonZeroU16::new(n).unwrap())
    }

    fn audio() -> AudioEncoding {
        AudioEncoding::new(Codec::Opus, 48_000, 50).unwrap()
    }

    fn video() -> VideoEncoding {
        VideoEncoding::new(Codec::Vp8, 640, 360, 30, 800_000, 500).unwrap()
    }

    fn policy() -> MediaStreamPolicy {
        MediaStreamPolicy::new(audio(), video())
    }

    fn handler_set(handlers: &[&str]) -> MeetingHandlers {
        MeetingHandlers::new(handlers.iter().map(|h| HandlerEndpoint {
            id: HandlerId::new(*h),
            webtransport_url: format!("https://{h}.example:4434"),
            grpc_endpoint: format!("http://{h}.example:50053"),
        }))
        .unwrap()
    }

    fn av() -> PublishSet {
        PublishSet::of(&[AU, VI])
    }

    fn a() -> PublishSet {
        PublishSet::of(&[AU])
    }

    /// Senders 1 and 2 (audio-only) on `mh-0`, each declaring one audio slot.
    fn two_party(handlers: &[&str]) -> (MeetingAssignment, MeetingHandlers) {
        let set = handler_set(handlers);
        let mut table = SlotTable::new();
        for s in [1, 2] {
            table.admit_on(sender(s), &set, &["mh-0"]);
            table.set_audio_demand(sender(s), vec![0]);
        }
        (table.render(set.ids(), &table.quiet_mute()).unwrap(), set)
    }

    #[test]
    fn header_version_is_derived_not_a_literal() {
        assert_eq!(header_version(), u32::from(PROTOCOL_VERSION));
    }

    #[test]
    fn a_held_publisher_is_directed_to_one_audio_stream_on_one_datagram_target() {
        let (assignment, urls) = two_party(&["mh-0"]);
        let (directive, outcome, stream_kinds) = build_send_directive(
            sender(1),
            a(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::Emitted);
        assert_eq!(directive.header_version, u32::from(PROTOCOL_VERSION));
        assert_eq!(directive.streams.len(), 1);
        assert_eq!(stream_kinds, vec![AU]);
        let stream = &directive.streams[0];
        assert_eq!(stream.stream_number, u32::from(MAIN_AUDIO_STREAM_NUMBER));
        assert_eq!(stream.media_kind, MediaKind::Audio as i32);
        let params = stream.encoding.as_ref().unwrap();
        assert_eq!(params.codec, Codec::Opus as i32);
        assert_eq!(params.max_bitrate_bps, 48_000);
        assert_eq!(params.frame_rate, 50);
        assert_eq!((params.width, params.height), (0, 0));
        assert_eq!(params.min_keyframe_interval_ms, None, "absent on audio");
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

    /// R-8 / story-2 contract: an unwatched publisher is LISTED with EMPTY
    /// targets — a specified success, never an omitted stream.
    #[test]
    fn an_unwatched_publisher_lists_its_stream_with_empty_targets() {
        let (assignment, urls) = two_party(&["mh-0"]);
        let (directive, outcome, kinds) = build_send_directive(
            sender(9),
            a(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::EmittedEmptyTargets);
        assert!(outcome.is_success());
        assert_eq!(directive.streams.len(), 1);
        assert!(directive.streams[0].targets.is_empty());
        assert_eq!(kinds, vec![AU]);
    }

    /// Every published kind is ALWAYS listed (audio first), and an unpublished
    /// kind never is; a receive-only participant gets no streams.
    #[test]
    fn the_directive_lists_exactly_the_publish_set() {
        let (assignment, urls) = two_party(&["mh-0"]);
        for (set, expected) in [
            (PublishSet::NONE, vec![]),
            (a(), vec![(0u32, MediaKind::Audio as i32)]),
            (
                PublishSet::of(&[VI]),
                vec![(1, MediaKind::VideoCamera as i32)],
            ),
            (
                av(),
                vec![
                    (0, MediaKind::Audio as i32),
                    (1, MediaKind::VideoCamera as i32),
                ],
            ),
        ] {
            let (directive, _, kinds) = build_send_directive(
                sender(9),
                set,
                &assignment,
                &urls,
                &policy(),
                ServerMutedKinds::NONE,
            )
            .unwrap();
            let got: Vec<(u32, i32)> = directive
                .streams
                .iter()
                .map(|s| (s.stream_number, s.media_kind))
                .collect();
            assert_eq!(got, expected);
            assert_eq!(kinds.len(), directive.streams.len());
        }
    }

    /// The video stream: stream 1, VIDEO_CAMERA, ALL six encoding fields set,
    /// STREAM_PER_GROUP targets — exactly the handlers owning its video
    /// out-edges.
    #[test]
    fn a_watched_video_publisher_is_directed_on_stream_one_with_every_field() {
        let set = handler_set(&["mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on_publishing(sender(1), av(), &set, &["mh-0"]);
        table.admit_on_publishing(sender(2), av(), &set, &["mh-0"]);
        table.set_demand(sender(2), [vec![0], vec![1]]);
        let assignment = table.render(set.ids(), &table.quiet_mute()).unwrap();
        let (directive, outcome, kinds) = build_send_directive(
            sender(1),
            av(),
            &assignment,
            &set,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::Emitted);
        assert_eq!(kinds, vec![AU, VI]);
        let v = &directive.streams[1];
        assert_eq!(v.stream_number, u32::from(MAIN_VIDEO_STREAM_NUMBER));
        assert_eq!(v.stream_number, 1, "pinned: video send id is 1");
        assert_eq!(
            directive.streams[0].stream_number, 0,
            "pinned: audio send id is 0"
        );
        assert_eq!(v.media_kind, MediaKind::VideoCamera as i32);
        let e = v.encoding.as_ref().unwrap();
        assert_eq!(e.codec, Codec::Vp8 as i32);
        assert_eq!(
            (e.width, e.height, e.frame_rate, e.max_bitrate_bps),
            (640, 360, 30, 800_000)
        );
        assert_eq!(e.min_keyframe_interval_ms, Some(500));
        assert_eq!(v.targets.len(), 1);
        assert_eq!(
            v.targets[0].transport_mode,
            TransportMode::StreamPerGroup as i32
        );
    }

    /// Two A/V publishers on one handler, each watching the other on one
    /// audio and one video slot, rendered under `mute` for sender 1.
    fn watched_av(
        mute_of_1: crate::media_routing::SourceMute,
    ) -> (MeetingAssignment, MeetingHandlers) {
        let set = handler_set(&["mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on_publishing(sender(1), av(), &set, &["mh-0"]);
        table.admit_on_publishing(sender(2), av(), &set, &["mh-0"]);
        table.set_demand(sender(1), [vec![0], vec![1]]);
        table.set_demand(sender(2), [vec![0], vec![1]]);
        let mute =
            crate::media_routing::SourceMuteView::from_map(std::collections::HashMap::from([
                (sender(1), mute_of_1),
                (sender(2), crate::media_routing::SourceMute::default()),
            ]));
        (table.render(set.ids(), &mute).unwrap(), set)
    }

    /// R-16: a server-muted kind is listed with EMPTY targets (encoding and
    /// stream number intact) while its edges stay rendered, and the OTHER
    /// kind's stream is byte-identical to the unmuted directive -- for each
    /// kind. Lifting the mute restores the targets.
    #[test]
    fn a_server_muted_kind_is_withdrawn_and_the_other_kind_is_untouched() {
        use crate::media_routing::SourceMute;
        for kind in [AU, VI] {
            let muted_flags = kind.pick([true, false], [false, true]);
            let (assignment, set) = watched_av(SourceMute::from_flags(muted_flags, [false; 2]));
            let (unmuted, _, _) = build_send_directive(
                sender(1),
                av(),
                &assignment,
                &set,
                &policy(),
                ServerMutedKinds::NONE,
            )
            .unwrap();
            let (muted, outcome, kinds) = build_send_directive(
                sender(1),
                av(),
                &assignment,
                &set,
                &policy(),
                ServerMutedKinds::from_server_flags(muted_flags),
            )
            .unwrap();
            assert_eq!(kinds, vec![AU, VI]);
            assert_eq!(
                outcome,
                DirectiveOutcome::Emitted,
                "the other kind still has a target"
            );
            let (k, other) = (kind.index(), kind.other().index());
            assert!(muted.streams[k].targets.is_empty(), "{kind:?} withdrawn");
            assert!(!unmuted.streams[k].targets.is_empty(), "{kind:?} watched");
            assert_eq!(
                muted.streams[k].stream_number,
                unmuted.streams[k].stream_number
            );
            assert_eq!(muted.streams[k].encoding, unmuted.streams[k].encoding);
            assert_eq!(
                muted.streams[other], unmuted.streams[other],
                "the other kind is byte-identical"
            );
            // The edge is KEPT (DROP_AT_INGRESS at MH), so the withdrawal is the
            // input's doing, not an unwatched stream's.
            assert!(assignment
                .for_handler(&HandlerId::new("mh-0"))
                .unwrap()
                .egress_streams
                .iter()
                .any(|p| p.candidate_sources.contains(&sender(1))
                    && p.stream_number == stream_number_of(kind)));
        }
    }

    /// Server-muting BOTH kinds empties every target: still a success, the
    /// specified "send nothing".
    #[test]
    fn server_muting_both_kinds_emits_empty_targets_on_both_streams() {
        let (assignment, set) = watched_av(crate::media_routing::SourceMute::from_flags(
            [true; 2], [false; 2],
        ));
        let (directive, outcome, _) = build_send_directive(
            sender(1),
            av(),
            &assignment,
            &set,
            &policy(),
            ServerMutedKinds::from_server_flags([true; 2]),
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::EmittedEmptyTargets);
        assert!(directive.streams.iter().all(|s| s.targets.is_empty()));
    }

    /// ADR-0036 §5: CLIENT mute never alters the directive. The render under
    /// client mute differs (PAUSED dispositions), yet the directive built from
    /// it is byte-identical to the unmuted one -- for every client-mute
    /// combination, server mute off.
    #[test]
    fn a_client_mute_flip_leaves_the_directive_byte_identical() {
        use crate::media_routing::SourceMute;
        let (quiet, set) = watched_av(SourceMute::default());
        let (baseline, _, _) = build_send_directive(
            sender(1),
            av(),
            &quiet,
            &set,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        for client in [[true, false], [false, true], [true, true]] {
            let (paused, _) = watched_av(SourceMute::from_flags([false; 2], client));
            assert_ne!(paused, quiet, "the render did change (PAUSED)");
            let (directive, _, _) = build_send_directive(
                sender(1),
                av(),
                &paused,
                &set,
                &policy(),
                ServerMutedKinds::NONE,
            )
            .unwrap();
            assert_eq!(directive, baseline, "client mute {client:?}");
        }
    }

    /// A video publisher held only in a placeholder sense (nobody declares
    /// video) has its video stream listed with EMPTY targets: unwatched.
    #[test]
    fn an_unwatched_video_stream_is_listed_with_empty_targets() {
        let set = handler_set(&["mh-0"]);
        let mut table = SlotTable::new();
        table.admit_on_publishing(sender(1), av(), &set, &["mh-0"]);
        table.admit_on_publishing(sender(2), av(), &set, &["mh-0"]);
        table.set_demand(sender(2), [vec![0], vec![]]);
        let assignment = table.render(set.ids(), &table.quiet_mute()).unwrap();
        let (directive, _, _) = build_send_directive(
            sender(1),
            av(),
            &assignment,
            &set,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert!(!directive.streams[0].targets.is_empty(), "audio watched");
        assert!(
            directive.streams[1].targets.is_empty(),
            "video unwatched, still listed"
        );
    }

    #[test]
    fn an_unresolvable_handler_url_fails_loud_rather_than_emitting_an_empty_url() {
        let (assignment, _) = two_party(&["mh-0"]);
        let outcome = build_send_directive(
            sender(1),
            a(),
            &assignment,
            &handler_set(&["mh-9"]),
            &policy(),
            ServerMutedKinds::NONE,
        );
        assert_eq!(outcome, Err(DirectiveOutcome::HandlerUrlUnresolved));
    }

    #[test]
    fn the_directive_carries_no_priority_group() {
        let (assignment, urls) = two_party(&["mh-0"]);
        let (directive, _, _) = build_send_directive(
            sender(1),
            a(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert!(!format!("{directive:?}").contains("priority"));
    }

    #[test]
    fn stream_number_from_wire_fails_closed_and_never_wraps() {
        assert_eq!(StreamNumber::from_wire(255).unwrap().get(), 255);
        assert_eq!(
            StreamNumber::from_wire(256),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
        assert_eq!(
            StreamNumber::from_wire(u32::MAX),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
    }

    #[test]
    fn the_policy_names_exactly_streams_zero_and_one() {
        let p = policy();
        for n in 0..=u8::MAX {
            let kind = p.kind_of(StreamNumber::from_u8(n));
            match n {
                0 => assert_eq!(kind, Some(AU)),
                1 => assert_eq!(kind, Some(VI)),
                _ => assert_eq!(kind, None, "stream {n} is never assumed to be anything"),
            }
        }
    }

    /// A plan naming an unknown stream number emits nothing.
    #[test]
    fn a_plan_naming_an_unknown_stream_number_emits_nothing() {
        use crate::media_routing::HandlerAssignment;
        let mut per_handler = BTreeMap::new();
        per_handler.insert(
            HandlerId::new("mh-0"),
            HandlerAssignment {
                egress_streams: vec![crate::media_routing::EgressStreamPlan {
                    egress_stream_id: 0x200,
                    subscriber: sender(2),
                    slot_id: 0,
                    candidate_sources: vec![sender(1)],
                    stream_number: 7,
                    priority_group: 1,
                    supersede_on_independent_frame: false,
                }],
                ..HandlerAssignment::default()
            },
        );
        let assignment = MeetingAssignment { per_handler };
        assert_eq!(
            build_send_directive(
                sender(1),
                av(),
                &assignment,
                &handler_set(&["mh-0"]),
                &policy(),
                ServerMutedKinds::NONE
            ),
            Err(DirectiveOutcome::UnknownStreamNumber)
        );
    }

    #[test]
    fn codec_parsing_fails_loud_and_never_falls_back() {
        assert_eq!(parse_codec("  OPUS ").unwrap(), Codec::Opus);
        assert_eq!(parse_codec("vp8").unwrap(), Codec::Vp8);
        assert_eq!(parse_codec("oppus"), Err(CodecParseError::UnknownCodec));
        assert_eq!(parse_codec(""), Err(CodecParseError::UnknownCodec));
        assert_eq!(
            parse_codec("unspecified"),
            Err(CodecParseError::CodecUnspecified)
        );
    }

    #[test]
    fn the_video_encoding_error_names_the_bands_from_the_constants() {
        let text = VideoEncodingError::MinKeyframeIntervalOutOfBand.to_string();
        assert!(text.contains(&format!(
            "{MIN_KEYFRAME_INTERVAL_MS_FLOOR}..={MIN_KEYFRAME_INTERVAL_MS_CEILING}"
        )));
        assert!(VideoEncodingError::DimensionOutOfBand
            .to_string()
            .contains(&format!(
                "{VIDEO_DIMENSION_MIN_PX}..={VIDEO_DIMENSION_MAX_PX}"
            )));
    }

    #[test]
    fn codec_unspecified_is_rejected_at_construction_not_at_emit() {
        assert_eq!(
            AudioEncoding::new(Codec::Unspecified, 48_000, 50),
            Err(AudioEncodingError::CodecUnspecified)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Vp8, 48_000, 50),
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
            AudioEncoding::new(Codec::Opus, AUDIO_BITRATE_MAX_BPS + 1, 50),
            Err(AudioEncodingError::BitrateOutOfBand)
        );
        assert_eq!(
            AudioEncoding::new(Codec::Opus, 48_000, AUDIO_FRAME_RATE_MAX_HZ + 1),
            Err(AudioEncodingError::FrameRateOutOfBand)
        );
    }

    /// Every `VideoEncoding` bound, both ends, and the codec.
    #[test]
    fn video_encoding_bounds_refuse_every_out_of_band_value() {
        let ok = |w, h, f, b, k| VideoEncoding::new(Codec::Vp8, w, h, f, b, k);
        assert!(
            ok(16, 16, 1, 100_000, 100).is_ok(),
            "every floor is inclusive"
        );
        assert!(
            ok(1920, 1920, 60, 4_000_000, 10_000).is_ok(),
            "every ceiling is inclusive"
        );
        assert!(
            ok(320, 180, 15, 250_000, 500).is_ok(),
            "the planned Kind test profile"
        );
        use VideoEncodingError::*;
        for (w, h, f, b, k, e) in [
            (15, 360, 30, 800_000, 500, DimensionOutOfBand),
            (1921, 360, 30, 800_000, 500, DimensionOutOfBand),
            (640, 15, 30, 800_000, 500, DimensionOutOfBand),
            (640, 1921, 30, 800_000, 500, DimensionOutOfBand),
            (640, 360, 0, 800_000, 500, FrameRateOutOfBand),
            (640, 360, 61, 800_000, 500, FrameRateOutOfBand),
            (640, 360, 30, 99_999, 500, BitrateOutOfBand),
            (640, 360, 30, 4_000_001, 500, BitrateOutOfBand),
            (640, 360, 30, 800_000, 99, MinKeyframeIntervalOutOfBand),
            (640, 360, 30, 800_000, 10_001, MinKeyframeIntervalOutOfBand),
        ] {
            assert_eq!(ok(w, h, f, b, k), Err(e), "{w}x{h}@{f} {b} {k}");
        }
        for codec in [
            Codec::Unspecified,
            Codec::Opus,
            Codec::Vp9,
            Codec::Av1,
            Codec::H264,
        ] {
            assert_eq!(
                VideoEncoding::new(codec, 640, 360, 30, 800_000, 500),
                Err(CodecNotVp8)
            );
        }
        assert_eq!(MIN_KEYFRAME_INTERVAL_MS_FLOOR, 100);
        assert_eq!(MIN_KEYFRAME_INTERVAL_MS_CEILING, 10_000);
    }

    #[test]
    fn the_directive_is_byte_identical_for_unchanged_input() {
        use prost::Message;
        let (assignment, urls) = two_party(&["mh-1", "mh-0", "mh-2"]);
        let (x, _, _) = build_send_directive(
            sender(1),
            av(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        let (y, _, _) = build_send_directive(
            sender(1),
            av(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(x.encode_to_vec(), y.encode_to_vec());
    }

    /// The targets ARE the handlers carrying this publisher's edges; mh-1 holds
    /// a correct-by-design EMPTY edge set (written literals).
    #[test]
    fn the_directive_targets_exactly_the_handlers_carrying_this_publishers_edges() {
        let (assignment, urls) = two_party(&["mh-0", "mh-1"]);
        let (directive, outcome, _) = build_send_directive(
            sender(1),
            a(),
            &assignment,
            &urls,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(outcome, DirectiveOutcome::Emitted);
        let targets: Vec<&str> = directive.streams[0]
            .targets
            .iter()
            .map(|t| t.media_handler_url.as_str())
            .collect();
        assert_eq!(targets, vec!["https://mh-0.example:4434"]);
    }

    /// ADR-0036 §9 multi-handler send: one stream, both owners as targets.
    #[test]
    fn a_publisher_whose_edges_span_handlers_is_directed_to_every_owner() {
        let set = handler_set(&["mh-0", "mh-1"]);
        let mut table = SlotTable::new();
        table.admit_on(sender(1), &set, &["mh-0", "mh-1"]);
        table.admit_on(sender(2), &set, &["mh-0"]);
        table.admit_on(sender(3), &set, &["mh-1"]);
        for s in [1, 2, 3] {
            table.set_audio_demand(sender(s), vec![0, 1]);
        }
        let (directive, _, _) = build_send_directive(
            sender(1),
            a(),
            &table.render(set.ids(), &table.quiet_mute()).unwrap(),
            &set,
            &policy(),
            ServerMutedKinds::NONE,
        )
        .unwrap();
        assert_eq!(directive.streams.len(), 1);
        let mut targets: Vec<&str> = directive.streams[0]
            .targets
            .iter()
            .map(|t| t.media_handler_url.as_str())
            .collect();
        targets.sort_unstable();
        assert_eq!(
            targets,
            vec!["https://mh-0.example:4434", "https://mh-1.example:4434"]
        );
    }
}
