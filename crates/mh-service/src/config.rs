//! Media Handler configuration.
//!
//! Configuration is loaded from environment variables. All sensitive
//! fields are redacted in Debug output.
//!
//! ## OAuth Configuration (ADR-0003)
//!
//! MH uses OAuth 2.0 client credentials for authenticating to GC via AC.
//! Required environment variables:
//! - `AC_ENDPOINT`: Authentication Controller endpoint (e.g., `http://localhost:8082`)
//! - `MH_CLIENT_ID`: OAuth client ID for MH
//! - `MH_CLIENT_SECRET`: OAuth client secret for MH

use common::secret::SecretString;
use std::collections::HashMap;
use std::env;
use std::fmt;
use thiserror::Error;

/// Default gRPC bind address for MC→MH communication.
pub const DEFAULT_GRPC_BIND_ADDRESS: &str = "0.0.0.0:50053";

/// Default health endpoint bind address.
pub const DEFAULT_HEALTH_BIND_ADDRESS: &str = "0.0.0.0:8083";

/// Default WebTransport bind address.
pub const DEFAULT_WEBTRANSPORT_BIND_ADDRESS: &str = "0.0.0.0:4434";

// `MH_MAX_STREAMS` is RETIRED (story 2 R-19). There is no read, no default and
// no `Config` field: the value MH advertises to GC in `RegisterMhRequest
// .max_streams` is the DERIVED stream ceiling ([`EgressAdmission::stream_ceiling`]),
// the same field admission enforces and `mh_media_egress_stream_ceiling`
// publishes. The ConfigMap key and both `configMapKeyRef`s survive one deploy
// for `kubectl rollout undo` safety; their removal is scheduled in
// `docs/TODO.md` §Media Path Obligations ("Delete the retired `MH_MAX_STREAMS`").
//
// `media_handlers.max_streams DEFAULT 1000` in the GC schema
// (`migrations/20260124000001_mh_registry.sql`) is an INTENTIONALLY RETAINED,
// UNREACHABLE default: register always binds the derived ceiling, and GC's
// register upsert overwrites the column (`max_streams = EXCLUDED.max_streams`),
// so no row ever keeps 1000. It is not a second encoding of anything MH uses
// and must not be "reconciled" with the ceiling.

/// Default MH instance ID prefix.
pub const DEFAULT_MH_ID_PREFIX: &str = "mh";

/// Default `RegisterMeeting` timeout in seconds.
/// Clients connecting to unregistered meetings are provisionally accepted
/// for this window; if `RegisterMeeting` does not arrive, they are disconnected.
pub const DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS: u64 = 15;

/// Maximum allowed `RegisterMeeting` timeout in seconds (5 minutes).
/// Capped to prevent misconfiguration from effectively disabling the
/// provisional timeout security control (R-14).
pub const MAX_REGISTER_MEETING_TIMEOUT_SECONDS: u64 = 300;

// =============================================================================
// Forwarding-policy bounds (ADR-0036 §8 control plane)
// =============================================================================
//
// `RegisterMeetingRequest` carries two unbounded repeated fields. Unbounded
// repeated fields on a control-plane message are an allocation surface, so
// `internal.proto::EgressStream.candidate_sources` requires MH to reject a
// registration exceeding the configured bounds **before building any routing
// table**. `grpc::mh_service` checks them before anything that iterates,
// including duplicate detection — a `HashSet` sized by the attacker-controlled
// repeated field is the allocation the bound exists to prevent, merely moved
// earlier than the routing table.
//
// EVERY BOUND HERE IS RESOURCE EXHAUSTION, NEVER CAPACITY. None is advertised
// to GC and none participates in placement. The capacity figure is the derived
// egress STREAM ceiling ([`EgressAdmission`], below), a different control with a
// different remedy.
//
// All four are REQUIRED reads (story 2 R-22) with no code default: their
// ConfigMap entries in `infra/services/mh-service/configmap.yaml` are the single
// home for the numbers, and a required read is what gives `dt-guard env-config`
// rule-1 coverage (it keys on `ConfigError::MissingEnvVar` literals, which a
// defaulted read never emits). Each also has a hard code-level CEILING, because
// a `> 0` check alone lets a fat-fingered value silently re-open the surface the
// bound exists to close, and it would read as configured-on-purpose forever.
// The ceilings live in code, not in a manifest, so the guarantee holds in every
// environment including those with no ConfigMap.

/// Shared consequence clause for the two PRE-ALLOCATION §8 bounds, which fail
/// the same way: both size work done against a caller-controlled repeated field
/// before any routing table exists. One home, because two copies of an
/// operator-facing sentence are what drift.
const PRE_ALLOCATION_CONSEQUENCE: &str =
    "the bound sizes work done against a caller-controlled repeated field before any routing \
     table is built";

/// Hard ceiling for `MH_MAX_EGRESS_STREAMS_PER_MEETING` (per-meeting bound on
/// `RegisterMeetingRequest.egress_streams`).
pub const MAX_EGRESS_STREAMS_PER_MEETING_CEILING: usize = 8_192;

/// Hard ceiling for `MH_MAX_CANDIDATE_SOURCES_PER_EGRESS` (per-egress-stream
/// bound on `EgressStream.candidate_sources`).
///
/// The field is repeated because audio selection among many speakers must be
/// additive (ADR-0036 §7); story 2 pins exactly one candidate per stream.
pub const MAX_CANDIDATE_SOURCES_PER_EGRESS_CEILING: usize = 256;

/// Hard ceiling for `MH_MAX_TOTAL_EGRESS_EDGES`, the aggregate bound on egress
/// edges across **all** meetings.
///
/// This docstring is the surviving home of the reasoning that previously sat on
/// the deleted `DEFAULT_MAX_TOTAL_EGRESS_EDGES` (moved intact when the read
/// became required, story 2 task 8); `infra/services/mh-service/configmap.yaml`
/// and `docs/TODO.md` §Media Path Obligations cite it.
///
/// It also bounds the derived egress stream ceiling from above: MH refuses to
/// start when the ceiling exceeds the configured `MH_MAX_TOTAL_EGRESS_EDGES`
/// ([`ConfigError::EgressStreamCeilingExceedsEdgeBound`]), and this ceiling is
/// asserted below `i32::MAX` because GC stores the advertised value in an
/// `INTEGER` column.
///
/// The two per-meeting bounds above bound *one* meeting; `SessionState` caps
/// no meeting count, so without this the total is per-meeting-bound x
/// unbounded meetings. Before this story a registration entry was three short
/// strings and an `Instant` — an uncapped map of cheap things. This story hangs
/// a routing table off each entry, which is what makes the aggregate MH's
/// problem rather than an inherited one.
///
/// The configured value is sized FAR above expected peak — single-digit
/// *concurrent* meetings in this deployment — so that no plausible
/// instantaneous load approaches it.
///
/// # What this actually bounds: a floor that ratchets unless teardown arrives
///
/// **An ENDED meeting's edges are released only by `EndMeeting`** (story 2
/// R-20): the registering MC's call removes the meeting's routes through
/// `RoutingTable::remove`, which returns its edges to this total. Otherwise
/// headroom is freed only when a still-**live** meeting re-asserts a smaller
/// policy — the per-meeting subtraction makes 5 edges shrinking to 4 install
/// and drop the total by one.
///
/// So the floor still ratchets for every meeting whose MC NEVER calls
/// `EndMeeting`: every meeting before MC begins calling it (story 2 task 12),
/// and permanently for any meeting whose MC dies without calling it. That
/// portion is unreclaimable until the pod restarts and rises at the rate such
/// meetings end. "Reaching this means a bug, a leak, or a hostile MC, never
/// growth" was false as first written and is still false for those meetings:
/// uncalled teardown IS a growth path, and a future reader sizing this key
/// would trust the promise.
///
/// The pod degrades **toward** the bound rather than falling off a cliff, which
/// is harder to diagnose than a cliff would be. Already-installed meetings
/// re-assert at an equal generation and short-circuit to `Applied` *before* the
/// cap is tested, so they keep reporting healthy; what fails is every
/// registration that would ADD an edge — a new meeting, or an existing healthy
/// meeting admitting a new participant. Onset is intermittent, clearing when
/// some unrelated meeting happens to shrink or end, and worsening with uptime.
/// Meanwhile `RegisterMeeting` still answers `accepted: true` and GC keeps
/// placing meetings here, because these bounds are resource guards and are
/// deliberately not advertised. Symptom, discriminator and remedy are in
/// `docs/runbooks/mh-incident-response.md` Scenario 13; the never-called
/// residual is tracked in `docs/TODO.md` §Media Path Obligations.
///
/// **Raising the configured value is not the remedy.** It buys time proportional to the
/// rate unreleased meetings accumulate and changes nothing else, because the
/// ceiling is consumed by them at whatever rate they end — independent of
/// concurrent load. Doubling the number doubles time-to-onset and fixes
/// nothing.
///
/// Does NOT bound the number of registered meetings: an empty `egress_streams`
/// set is legal and meaningful, so near-free meetings never trip this. That
/// converse gap is closed by its own bound, `MH_MAX_REGISTERED_MEETINGS`
/// ([`MAX_REGISTERED_MEETINGS_CEILING`]).
pub const MAX_TOTAL_EGRESS_EDGES_CEILING: usize = 1_048_576;

// GC persists the advertised ceiling in `media_handlers.max_streams INTEGER`.
// The derived ceiling is bounded by the configured edge bound, which is bounded
// by this constant — so this assertion is what makes the advertised value
// representable at GC by construction rather than by luck.
const _: () = assert!(
    MAX_TOTAL_EGRESS_EDGES_CEILING as u64 <= i32::MAX as u64,
    "the edge-bound ceiling must fit GC's INTEGER max_streams column"
);

/// Hard ceiling for `MH_MAX_REGISTERED_MEETINGS`, the bound on how many meetings
/// this handler holds registered at once (story 2 R-21).
///
/// A RESOURCE-EXHAUSTION guard, never a capacity figure, and never advertised
/// to GC. It exists because the egress-edge bound is structurally blind to
/// EMPTY meetings: `internal.proto` makes an empty `egress_streams` set legal
/// and meaningful ("forward nothing"), so arbitrarily many near-free
/// registrations would never trip `MH_MAX_TOTAL_EGRESS_EDGES`. `EndMeeting`
/// (R-20) reclaims ended meetings, but it bounds nothing a caller that never
/// ends its meetings creates.
///
/// This ceiling is the fat-finger bound on the KEY, on the same footing as the
/// four §8 ceilings above: a `> 0` check alone would let a copy-pasted value
/// silently disarm the guard. 8x the deployed value; a registration is a
/// length-capped `MeetingRegistration` plus an `Arc<MeetingRoutes>`, so even at
/// the ceiling the map is tens of MB.
///
/// ITS NUMERIC EQUALITY WITH THE DEPLOYED `MH_MAX_TOTAL_EGRESS_EDGES` VALUE IS
/// ACCIDENTAL. Different units — meetings here, egress edges there — and
/// different remedies. Do not fold the two into one constant because they
/// print the same.
///
/// Zero is refused by the shared `parse_bounded` path: a configured 0 would
/// deny every registration while the handler reports healthy.
pub const MAX_REGISTERED_MEETINGS_CEILING: usize = 65_536;

/// ADVISORY lower bound for `MH_MAX_REGISTERED_MEETINGS`: below this MH logs a
/// WARN at startup (it still boots — the value is legal, just implausible).
///
/// Not a refuse-boot floor: a small deployment may genuinely want a small cap.
/// A WARN rather than the INFO the egress-ceiling nudge uses, because that
/// nudge fires on the deployed placeholder by design, whereas this one fires
/// only when someone has lowered a value deployed at 8192 — the cap is
/// cumulative across uptime until reclamation is fenced (see
/// `infra/services/mh-service/configmap.yaml`), so a low value is a
/// registration outage waiting for uptime.
pub const REGISTERED_MEETINGS_ADVISORY_MIN: usize = 100;

/// Hard ceiling for `MH_POLICY_APPLY_TIMEOUT_MS` (10 seconds), the bound on
/// awaiting the session actor's config-apply reply.
///
/// The apply is asynchronous relative to the RPC response (ADR-0036 §8), so
/// that await must be bounded: a wedged actor must produce a truthful stale
/// `applied_generation` with `outcome=apply_failed`, never a hung RPC.
///
/// Above ADR-0036 §8's <=10s re-assert cadence the await outlives the interval
/// that would have retried it, so a longer value cannot help and can only pile
/// up in-flight calls.
pub const MAX_POLICY_APPLY_TIMEOUT_MS: u64 = 10_000;

// =============================================================================
// Egress-budget admission (story 2 R-19, R-23; ADR-0036 §11 "Admission control
// is keyed on egress bandwidth, not connection count")
// =============================================================================
//
// A CAPACITY figure — unlike every bound in the section above, which is a
// resource guard. Four REQUIRED keys (`MH_EGRESS_BUDGET_BPS`,
// `MH_STREAM_COST_AUDIO_BPS`, `MH_STREAM_COST_VIDEO_BPS`,
// `MH_EGRESS_REJECTION_RATIO_THRESHOLD`) derive ONE value, the egress STREAM
// ceiling, which is simultaneously:
//   - what MH enforces at stream admission (`session::admission`, hard);
//   - what MH advertises to GC in `RegisterMhRequest.max_streams` (GC's
//     placement filter reads it unchanged and stays SOFT);
//   - what `mh_media_egress_stream_ceiling` publishes;
//   - what the startup log reports.
// All four read [`EgressAdmission::stream_ceiling`], never a re-derivation.
//
// A STREAM ceiling, never a "subscriber ceiling": since story 2 a subscriber
// holds N slots, so one subscriber is N streams.
//
// BITS ON THE KEY, BYTES EVERYWHERE ELSE — a deliberate single conversion. The
// keys are bits per second (the unit operators size links in); MH converts
// ONCE, in `from_vars`, upstream of the enforcement/gauge fork, so nothing
// downstream of load sees bits and `mh_media_egress_budget_bytes_per_second`
// is in bytes (ADR-0011's throughput rule). Both roundings fail CLOSED: the
// budget floors (`bits / 8`) and each cost ceils (`bits.div_ceil(8)`), so the
// conversion can only ever admit FEWER streams than an exact one would. The raw
// bit values are retained on [`EgressAdmission`] for the provenance log line
// ONLY.

/// Egress streams for `peers + 1` participants who all hear each other: each of
/// the `peers + 1` subscribers receives every one of its `peers` peers.
///
/// ANCHOR (DRY): the same formula, in the same shape and with the same parameter
/// meaning, is `required_edges(n)` in
/// `crates/env-tests/tests/01_mh_deployment_config.rs`, and the Kind budget
/// patch (`infra/kubernetes/overlays/kind/services/mh-service/
/// configmap-egress-budget-patch.yaml`) states it in prose. They cannot share a
/// home: env-tests links no service crate (ADR-0028 black-box layering). The
/// env-test compares the published `mh_media_egress_stream_ceiling_recommended_min`
/// against its own `required_edges(DEMO_N)`, so the agreement is CHECKED, not
/// merely anchored.
#[must_use]
pub const fn all_hear_all_streams(peers: u64) -> u64 {
    (peers + 1) * peers
}

/// REFUSE-BOOT floor on the derived egress stream ceiling.
///
/// Requirement: the smallest meeting story 2 defines — two participants, each
/// hearing the other — must be admissible. A ceiling of 0 is caught loudly
/// elsewhere (GC rejects a zero `max_streams` at register); a ceiling of exactly
/// 1 passes every other validation, boots, gets placed, and then rejects nearly
/// everything it is handed — the black-hole-reporting-healthy shape.
///
/// **A code constant, not a config knob, and never to be lowered to quiet a
/// refusing pod.** The remedy for a refusal is a real `MH_EGRESS_BUDGET_BPS`.
///
/// **Must never merge with [`EGRESS_STREAM_CEILING_RECOMMENDED_MIN`].** They are
/// two different consequences: below THIS, MH refuses to start; below THAT, MH
/// boots, serves and nudges. A deliberately small deployment below the
/// recommended minimum must boot. Asserted below and in the unit tests.
pub const MIN_EGRESS_STREAM_CEILING: u64 = all_hear_all_streams(1);

/// Peers each participant hears in the demo meeting the advisory minimum is
/// sized for (story 2: N+1 participants all hearing each other, N = 5).
const RECOMMENDED_MIN_DEMO_PEERS: u64 = 5;

/// ADVISORY recommended minimum for the derived egress stream ceiling.
///
/// Requirement: one full all-hear-all meeting at the story-2 demo size fits on
/// one handler. It is the same anchor the Kind budget is sized against, so one
/// requirement drives both (see [`all_hear_all_streams`] for the sibling
/// encodings).
///
/// **Advisory only.** Nothing enforces or alerts on it — a placeholder budget is
/// a configuration state, not an incident. Below it MH boots, serves, and emits
/// a distinct INFO "configure me" nudge naming `MH_EGRESS_BUDGET_BPS` at
/// startup; it is published as `mh_media_egress_stream_ceiling_recommended_min`
/// (suffix descriptive, deliberately not `_threshold`) for the dashboard
/// comparison and the runbook's first triage step. Never an alert input.
pub const EGRESS_STREAM_CEILING_RECOMMENDED_MIN: u64 =
    all_hear_all_streams(RECOMMENDED_MIN_DEMO_PEERS);

const _: () = assert!(
    MIN_EGRESS_STREAM_CEILING < EGRESS_STREAM_CEILING_RECOMMENDED_MIN,
    "the refuse-boot floor and the advisory recommended minimum must never merge"
);

// =============================================================================
// QUIC transport parameters (ADR-0036 §1)
// =============================================================================
//
// PROVENANCE MATTERS AND IS NOT VISIBLE FROM THE STARTUP LOG LINE. `main`'s
// "Configuration loaded successfully" event prints the effective value of every
// setting below, but it cannot say *where* a value came from. There are three
// states, not two, and an operator who wants to change something needs to know
// which one they are in:
//
//   1. ENV-DRIVEN, REQUIRED  — parsed in `from_vars`, supplied by
//      `infra/services/mh-service/configmap.yaml` (or, for the termination
//      grace, by the kustomize `replacements:` block writing each instance's
//      own pod spec into its own env). Change the manifest; no release needed.
//   2. COMPILE-TIME CONSTANT — everything in THIS section. Changing one is a
//      code change and a release. They are constants rather than keys because
//      each is either a wire/format invariant, a unit-conversion assumption, or
//      a bound whose correct value is not an operator's decision.
//   3. quinn's own defaults — everything NOT named here or in
//      `build_transport_config`. ADR-0036 §1's complaint is precisely that
//      "the default being adequate is not the same as the default being
//      chosen", so the set in state 3 should only ever shrink.
//
// The env-driven values live on `QuicTransportParams`; the compile-time ones
// live here, deliberately in one place so the distinction is legible at a
// glance rather than reconstructed by grepping `from_vars`.

/// Nominal Opus bitrate assumed when converting the datagram send buffer from
/// frames to bytes.
///
/// # This is a SIZING ASSUMPTION, not knowledge of the media
///
/// MH is opaque to encodings (ADR-0036 §7): it never inspects a payload, never
/// decrypts, and has no codec dependency. This constant exists for exactly one
/// reason — ADR-0036 §1 mandates that the datagram send buffer be *expressed
/// and documented in frames of audio*, while `quinn::TransportConfig` takes
/// bytes. It is a unit conversion and nothing else. Nothing on any forwarding
/// path may read it.
///
/// # Why the SDK's FLOOR, and not its ceiling
///
/// **MH sizes this against the LOWEST bitrate the SDK may emit, not the
/// highest.** This is counter-intuitive and it was got backwards twice during
/// this change's review, so it is stated as a rule rather than left to be
/// re-derived:
///
/// `datagram_send_buffer_size` is a **latency ceiling**, not a capacity
/// guarantee. ADR-0036 §1's complaint about quinn's 1 MiB default is that it
/// holds *too much* — "≈93 seconds of queued audio ... before the oldest is
/// silently discarded. A realtime path must prefer loss to unbounded latency."
/// Frames held equals `buffer_bytes / actual_frame_bytes`, so held latency is
/// maximised by the **smallest** real frame. Sizing at the top of the SDK's
/// range would loosen the bound on exactly the traffic §1's own worked example
/// describes:
///
/// | sized at | traffic at 32 kbps | traffic at 48 kbps |
/// |---|---|---|
/// | 32 kbps (236 B) | 32 frames, 640 ms | 27 frames, 540 ms |
/// | 48 kbps (276 B) | **37 frames, 740 ms** | 32 frames, 640 ms |
///
/// So: if the SDK's bitrate **floor** ever drops below this value, this
/// constant must drop with it. A rise in the SDK's *ceiling* does not affect
/// it. Story task 19 lands the SDK encoder config (a configured VBR range,
/// 32–48 kbps); this is deliberately **not** an `ANCHOR (DRY):` on it, because
/// MH and the SDK do not hold one value — they stand in an inequality, and an
/// anchor would claim a lock that cannot be checked. See `docs/TODO.md`.
///
/// # What the operator-facing budget actually means at each end of the range
///
/// `MH_DATAGRAM_BUFFER_AUDIO_FRAMES = 32` therefore means **32 frames (640 ms)
/// at the 32 kbps floor and ≥27 frames (~540 ms) at the 48 kbps ceiling** — a
/// budget that holds *at most* what it says at every bitrate in the range,
/// rather than a point value that is exactly true only at one end. Stated as
/// both endpoints because no single frame count is true across a range, and a
/// budget that lies at one end is the residue this whole derivation exists to
/// remove.
///
/// Sourced from ADR-0036 §3's worked figure, "an 80-byte Opus frame
/// (32 kbps, 20 ms)".
pub const NOMINAL_AUDIO_BITRATE_BPS: usize = SUPPORTED_AUDIO_BITRATE_FLOOR_BPS;

/// Bottom of the audio bitrate range MH must cover (story task 19's configured
/// VBR range is 32–48 kbps). **This is the sizing input**, per the
/// latency-ceiling argument on [`NOMINAL_AUDIO_BITRATE_BPS`].
const SUPPORTED_AUDIO_BITRATE_FLOOR_BPS: usize = 32_000;

/// Top of the audio bitrate range MH must cover. Present so the range is
/// visible and so the compile-time pin below it has something to be
/// wrong against — deliberately **not** the sizing input.
const SUPPORTED_AUDIO_BITRATE_CEILING_BPS: usize = 48_000;

/// Compile-time pin: the buffer must be sized at the bitrate range's **floor**.
///
/// This exists because sizing it at the *ceiling* is the mistake that was
/// actually made — proposed, ruled for, and reversed — during this change's
/// review, by three readers who had each read ADR-0036 §1's row. A byte-level
/// assertion cannot catch it: 32 × 236 = 7,552 and 32 × 276 = 8,832 both look
/// like plausible buffer sizes. What separates them is *held frames at the
/// smallest real frame* — 32 versus 37 — i.e. whether the latency ceiling holds.
///
/// So the assertion is written so that its failure mode reads "NOMINAL got
/// sized at the ceiling", not merely "a number changed".
const _: () = assert!(
    NOMINAL_AUDIO_BITRATE_BPS <= SUPPORTED_AUDIO_BITRATE_FLOOR_BPS
        && SUPPORTED_AUDIO_BITRATE_FLOOR_BPS <= SUPPORTED_AUDIO_BITRATE_CEILING_BPS,
    "NOMINAL_AUDIO_BITRATE_BPS must be the FLOOR of the supported bitrate range, not its ceiling: \
     the datagram send buffer is a latency ceiling, so worst-case queued frames are set by the \
     SMALLEST frame (ADR-0036 §1, prefer loss to unbounded latency)"
);

/// Audio frame duration assumed by the frames↔bytes conversion, in
/// milliseconds (ADR-0036 §1, §5: one Opus frame per datagram, 20 ms frames).
pub const AUDIO_FRAME_DURATION_MS: usize = 20;

/// Nominal Opus payload for one 20 ms frame, in bytes.
///
/// Derived, not typed: `bitrate_bps × duration_ms / (8 bits × 1000 ms)`.
pub const NOMINAL_OPUS_PAYLOAD_BYTES: usize =
    NOMINAL_AUDIO_BITRATE_BPS * AUDIO_FRAME_DURATION_MS / (8 * 1000);

/// Non-payload bytes on a nominal v2 audio frame: the whole header **except**
/// the extension region, which a nominal audio frame does not carry.
///
/// Every term is imported from `media-protocol`; the only thing enumerated here
/// is *which regions are present*, and the compile-time pin below it
/// pins that enumeration against the crate's own composition so a frame-v2
/// layout change breaks the build instead of silently resizing a live buffer.
///
/// The extension region is excluded because the TLV registry's salience entry
/// is the ADR-0036 §7 *video* selector input; an audio frame carries no
/// extension. The two-byte extension **length** field is present regardless —
/// `media_protocol::codec` requires it even when the region is empty.
const NOMINAL_HEADER_BYTES: usize = media_protocol::frame::PUBLISHER_FIXED_PREFIX_SIZE
    + media_protocol::frame::WRAPPED_TRANSMIT_KEY_SIZE
    + media_protocol::frame::EXT_LENGTH_FIELD_SIZE
    + media_protocol::frame::RELAY_REGION_SIZE;

/// Compile-time pin: [`NOMINAL_HEADER_BYTES`] must equal `media-protocol`'s own
/// maximum header composition minus the extension region.
///
/// The duplicated knowledge is **the set of regions a v2 header is made of**,
/// not their widths — the widths are imported above and track automatically. A
/// sixth region added to frame v2 would update `MAX_HEADER_BYTES` and silently
/// miss the local enumeration, which would under-size the datagram buffer *and*
/// feed a stale premise into the egress-queue-binds-first startup validation.
/// Same idiom as the `media_protocol` width pins in [`crate::routing`].
const _: () = assert!(
    NOMINAL_HEADER_BYTES
        == media_protocol::frame::MAX_HEADER_BYTES - media_protocol::frame::MAX_EXT_BYTES,
    "frame v2 gained or lost a header region: NOMINAL_HEADER_BYTES must track MAX_HEADER_BYTES"
);

/// Full on-wire size of one nominal v2 audio frame, in bytes.
///
/// Nominal Opus payload + the `SFrame` object's own overhead + the v2 header
/// (publisher prefix, wrapped transmit key, extension-length field, relay
/// region) + the trailing Ed25519 signature. Every term but the payload comes
/// from `media-protocol`.
///
/// The wrapped transmit key is unconditional here because ADR-0036 §4 carries a
/// wrapped key on **every** audio frame, not just the first of a group.
///
/// # This is NOT `media_protocol::frame::MAX_PAYLOAD_BYTES`, and the two can never converge
///
/// They differ in *kind*, not merely in value. `MAX_PAYLOAD_BYTES` is a
/// **wire-format** constant — its own docstring says "a wire-format constant,
/// not an operational knob" — enforced before any allocation, and it is the
/// ADR-0036 §2 ingress denial-of-service guard. This is an **operational
/// sizing input with no enforcement role whatsoever**. Two constants, two jobs.
/// Using `MAX_PAYLOAD_BYTES` here would give a 32 MiB per-connection send
/// buffer and defeat §1's prefer-loss-to-latency purpose by four orders of
/// magnitude; using the bare Opus payload would undersize it ~2.9×.
///
/// # Convention: NOMINAL case, in contrast to [`MIN_DATAGRAM_RECEIVE_BUFFER_BYTES`]
///
/// This constant and the datagram-receive floor derive from overlapping
/// `media-protocol` components under **opposite** conventions, which a reader
/// cannot detect from either one alone:
///
/// - **`NOMINAL_AUDIO_FRAME_BYTES` (here): nominal case**, excluding the
///   extension region. It must reflect the *typical* frame — a conservative
///   error merely wastes buffer.
/// - **`MIN_DATAGRAM_RECEIVE_BUFFER_BYTES`: maximum case**, using
///   `MAX_HEADER_BYTES` (extensions included) and Opus's packet cap. It must
///   clear the *largest frame the codec can legitimately emit* — a permissive
///   error silently breaks a real publisher.
///
/// Opposite directions of harm, hence opposite conventions. Do not unify them.
///
/// # Relationship to ADR-0036 §1's "~225-byte on-wire audio frame"
///
/// §1 quotes ~225 B and computes quinn's 1 MiB default as ≈93 s of queued
/// audio. This constant sums the tree's own size components and lands at 236 B
/// (so the same default is ≈88.9 s). The ADR's figure is written with a `~` and
/// is an approximation of this same derivation, not a competing number — the
/// chain is one chain (§3's "80-byte Opus frame (32 kbps, 20 ms)" is the
/// payload term here). Recorded so the 11-byte delta reads as rounding rather
/// than as a defect in either place.
pub const NOMINAL_AUDIO_FRAME_BYTES: usize = NOMINAL_OPUS_PAYLOAD_BYTES
    + media_protocol::frame::SFRAME_OBJECT_OVERHEAD_BYTES
    + NOMINAL_HEADER_BYTES
    + media_protocol::frame::SIGNATURE_SIZE;

/// Bound on MH's own application-level per-connection egress queue, in frames.
///
/// ADR-0036 §1: "The application-level egress queue bound must trip **before**
/// the transport ceiling, so back-pressure is observable in our code rather
/// than inside quinn." quinn's overflow behaviour on the datagram send buffer
/// is to silently evict the oldest datagram, which emits a trace and nothing
/// countable; MH's own queue refusal is countable by construction. That
/// ordering is enforced at startup — see [`ConfigError::EgressQueueDoesNotBindFirst`].
///
/// A compile-time constant rather than an env var: no manifest supplies it, and
/// a newly-required variable with no manifest is a deploy-time `CrashLoop`.
/// `infra/services/mh-service/configmap.yaml` documents this value to operators
/// as the bound `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` must sit strictly above.
///
/// The queue itself lands with the forward path (story task 16). This value is
/// live today as the left-hand side of that startup validation.
pub const EGRESS_QUEUE_FRAMES: usize = 8;

/// Bound on MH's own per-connection **ingress** datagram queue, in frames.
///
/// ADR-0036 §11's ingress denial-of-service caps: "a bounded datagram queue
/// with drop-oldest". Same ring type as the egress queue
/// ([`crate::media::queue::BoundedDropOldest`]) with the opposite direction, so
/// the two are one structure used twice rather than two hand-rolled rings.
///
/// A compile-time constant for the same reason [`EGRESS_QUEUE_FRAMES`] is: no
/// manifest supplies it, and a newly-*required* env var with no manifest key is
/// a deploy-time `CrashLoop`. It is deliberately **not** shared with
/// [`EGRESS_QUEUE_FRAMES`]: the egress bound is fixed by the ADR-0036 §1
/// ordering relationship against `MH_DATAGRAM_BUFFER_AUDIO_FRAMES` and is
/// startup-validated ([`ConfigError::EgressQueueDoesNotBindFirst`]), while this
/// one answers a different question — how much scheduling jitter between the
/// receive loop and the forward loop MH absorbs before shedding. Collapsing
/// them into one constant would make the §1 validation start constraining a
/// value it has nothing to say about.
///
/// Sized as one 20 ms audio frame per queued slot: 16 frames is ~320 ms of
/// ingest backlog, which is far beyond any healthy scheduling gap and still a
/// latency ceiling rather than a buffer.
pub const INGRESS_QUEUE_FRAMES: usize = 16;

/// Default fraction of forwarded frames whose latency is observed into
/// `mh_media_forward_latency_seconds`.
///
/// One in a hundred. ADR-0036 §11: "timestamp always, observe one in N" — the
/// clock read is a ~25 ns vDSO call and is taken on every frame; the histogram
/// observation is the expensive part.
pub const DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO: f64 = 0.01;

/// Upper bound on the random delay inserted before closing a connection whose
/// media session was declined because **MC was unreachable**, in milliseconds.
///
/// # A herd control, not a timeout — deleting it looks free and is not
///
/// `NotifyParticipantConnected` is a blocking precondition for a media session,
/// so an MC outage closes every connecting client. Every one of them reconnects,
/// and they arrive in a wave: a synchronized reconnect herd against the service
/// that is already the bottleneck. This spreads the arrival edge of that wave.
///
/// **It applies to the MC-unreachable arm ONLY.** The other decline arms are
/// MC's *answer* — MC is reachable and said no — where retrying is pointless
/// and an immediate, terminal close is correct. Jitter there would be latency
/// with no benefit.
///
/// **Jitter is not the fix, and must not be read as one.** It acts on the wave's
/// *phase*; what bounds its *rate* is (a) the MC client's existing ~48 s retry
/// budget, which functions as an accidental rate limiter and is deliberately NOT
/// shortened (`docs/runbooks/mh-deployment.md`), and (b) a client that can tell
/// a retryable close from a terminal one and back off — which does not exist yet
/// and is tracked in `docs/TODO.md`. Shortening the retry budget before that
/// client half lands converts a contained outage into a herd; the two are one
/// decision.
pub const MC_UNAVAILABLE_CLOSE_JITTER_MAX_MS: u64 = 2_000;

/// Maximum queued-audio latency the datagram send buffer may be configured to
/// hold, in milliseconds. Sole input to
/// [`MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING`].
///
/// Expressed as a latency rather than a frame count because latency is the
/// quantity ADR-0036 §1 actually bounds ("a realtime path must prefer loss to
/// unbounded latency"), so the refusal message can say what it is protecting
/// instead of quoting a number nobody can evaluate.
///
/// 3 s is ~4.7x the deployed operating point of 640 ms — generous room to tune —
/// while remaining far beyond any value a realtime audio path would choose. A
/// configuration at or above it is a typo, not a decision.
pub const MAX_DATAGRAM_BUFFER_LATENCY_MS: usize = 3_000;

/// Hard ceiling for `MH_DATAGRAM_BUFFER_AUDIO_FRAMES`, in frames.
///
/// **Derived, not typed**: `MAX_DATAGRAM_BUFFER_LATENCY_MS / AUDIO_FRAME_DURATION_MS`.
///
/// # Why `> 0` is not enough here, in this file above all
///
/// The rule is already stated for the ADR-0036 §8 bounds in this same module:
/// *"a `> 0` check alone lets a fat-fingered value silently re-open the surface
/// the bound exists to close, and it would read as configured-on-purpose
/// forever."* This variable needs it more than any of those, for three reasons
/// that compound:
///
/// 1. **It multiplies into memory twice.** `frames x NOMINAL_AUDIO_FRAME_BYTES`
///    is a per-connection allocation, then multiplied again by
///    `MH_MAX_CONNECTIONS`. A two-zero typo (`3200`) is ~377 MB against a 1 Gi
///    limit and **boots clean, reporting healthy**; a three-zero typo (`32000`)
///    is ~3.8 GB and triggers an `OOMKill` — which is `SIGKILL`, and therefore
///    voids the drain window derived in this same file. That second-order chain
///    is the one [`CONNECTION_RECEIVE_WINDOW_BYTES`] flags as easy to miss.
/// 2. **The multiply wraps silently in release.** The workspace sets no
///    `overflow-checks` in `[profile.release]`, so a value at or above
///    `usize::MAX / NOMINAL_AUDIO_FRAME_BYTES` yields an arbitrary *small*
///    buffer with no error at all.
/// 3. **The sharpest reason does not need a typo to be large.** quinn's
///    unchosen 1 MiB default is ~4,443 frames, i.e. ~89 s of queued audio. Any
///    configured value above that is **strictly worse than the default this
///    task exists to replace** — and the egress-queue validation alone would
///    accept it. A validation that permits configuring a state worse than the
///    thing it was written to eliminate is not a bound.
pub const MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING: usize =
    MAX_DATAGRAM_BUFFER_LATENCY_MS / AUDIO_FRAME_DURATION_MS;

/// quinn's own default `max_concurrent_uni_streams`
/// (`quinn-proto-0.11.17/src/config/transport.rs`, `100u32.into()`).
///
/// Named rather than inlined because it is the *reason* for the ceiling below,
/// not merely its value.
pub const QUINN_DEFAULT_MAX_CONCURRENT_UNI_STREAMS: u32 = 100;

/// Hard ceiling for `MH_MAX_CONCURRENT_UNI_STREAMS`.
///
/// **Derived from the declaration's own purpose rather than chosen.**
/// ADR-0036 §1 requires this bound be declared because "a bound has to be
/// declared to be assertable and the default being adequate is not the same as
/// the default being chosen", and
/// `infra/services/mh-service/configmap.yaml` justifies the deployed `64` as
/// "sitting below quinn's unchosen default of 100, so the ceiling is
/// demonstrably ours".
///
/// Above quinn's default the declaration stops tightening anything: MH would be
/// *loosening* a limit it went to the trouble of declaring, and the stated
/// justification for declaring it would be false. Equal to the default is still
/// a choice and is allowed; above it is not a tuning decision.
pub const MAX_CONCURRENT_UNI_STREAMS_CEILING: u32 = QUINN_DEFAULT_MAX_CONCURRENT_UNI_STREAMS;

/// QUIC `max_idle_timeout`, in seconds.
///
/// Declared rather than inherited: building a custom `TransportConfig` means MH
/// owns this whether or not it names it, and a keepalive interval is meaningless
/// without a stated idle timeout to be a fraction of. Paired with
/// [`MIN_KEEPALIVE_TO_IDLE_RATIO`] this makes `MH_KEEPALIVE_INTERVAL_MS` a
/// *chosen* ratio rather than an inherited coincidence.
pub const MAX_IDLE_TIMEOUT_SECONDS: u64 = 30;

/// Minimum ratio of [`MAX_IDLE_TIMEOUT_SECONDS`] to the configured keepalive
/// interval — i.e. how many keepalives must fit inside one idle timeout.
///
/// At 3, a single lost keepalive still leaves two more before the peer times
/// out. At 2 a single loss sits at the edge of the timeout. Enforced at startup
/// by [`ConfigError::KeepaliveTooLarge`].
///
/// # The deployed value sits EXACTLY on this boundary — there is no margin here
///
/// `MH_KEEPALIVE_INTERVAL_MS` ships at `10000`, and the bound is
/// `MAX_IDLE_TIMEOUT_SECONDS * 1000 / MIN_KEEPALIVE_TO_IDLE_RATIO`
/// = `30000 / 3` = **exactly 10000**. The check is `>`, so the shipped
/// configuration passes with **zero headroom**. That is correct — a chosen 1:3
/// ratio is what boundary-exact looks like — but it means this constant and
/// [`MAX_IDLE_TIMEOUT_SECONDS`] are a **live wall, not a margin**:
///
/// - raising this ratio, or
/// - lowering [`MAX_IDLE_TIMEOUT_SECONDS`],
///
/// **sends both pods into `CrashLoopBackOff` immediately**, with no warning band and no
/// intermediate degraded state, because the deployed keepalive is already at
/// the limit. Either edit must move `MH_KEEPALIVE_INTERVAL_MS` in
/// `infra/services/mh-service/configmap.yaml` in the same change.
///
/// Recorded because the arithmetic is invisible from either constant alone:
/// nothing here or in `MAX_IDLE_TIMEOUT_SECONDS` hints that a third value,
/// living in a manifest, is resting exactly on their quotient.
pub const MIN_KEEPALIVE_TO_IDLE_RATIO: u64 = 3;

/// Connection-level receive window, in bytes.
///
/// # This is an admission bound, and without it `MH_MAX_CONNECTIONS` is theatre
///
/// quinn-proto leaves `receive_window` at `VarInt::MAX` — unbounded — and
/// ADR-0036's Context records that MH has no `accept_uni` loop, so incoming
/// stream data is never read and the windows fill and stay filled: the worst
/// case is the *ordinary* case, not a tail. Undeclared, the per-connection
/// adversarial receive ceiling is ~206 MB against this pod's 1 Gi limit, i.e.
/// roughly five hostile connections rather than the configured 500. Lowering
/// the connection count would not have fixed it; declaring flow control does.
///
/// Second-order consequence, because it is the one that is easy to miss: an
/// `OOMKill` is `SIGKILL` and bypasses `terminationGracePeriodSeconds` entirely,
/// so an undeclared receive window voids the drain window derived in this same
/// file under exactly the adversarial condition where it would matter most.
///
/// # Corrected arithmetic — THIS SUPERSEDES `infra/services/mh-service/configmap.yaml:174`
///
/// That comment states the declared receive side drops the ceiling to
/// "~263 KB". It has **two independent defects, pointing in opposite
/// directions**, stated separately rather than netted so that whoever revisits
/// memory headroom (story 2's egress-budget work) does not inherit a figure
/// that is right by coincidence:
///
/// 1. **`stream_receive_window` was double-counted.** It is bounded *by* the
///    connection receive window, not additive to it, so the connection window
///    is the only stream-side term.
/// 2. **`crypto_buffer_size` was omitted.** quinn defaults it to 16 KiB and MH
///    does not declare it (handshake-scoped, not named by ADR-0036 §1, and
///    16 KiB × 500 ≈ 8 MB is immaterial against 1 Gi — so it stays inherited,
///    but it is part of the ceiling and belongs in the arithmetic).
///
/// Corrected per-connection ceiling with the values in this file:
/// `256 KiB (receive_window) + 2 KiB (datagram_receive_buffer_size)` = **~264 KB**,
/// or **~281 KB** including the inherited 16 KiB crypto buffer. At
/// `MH_MAX_CONNECTIONS = 500` that is ~132 MB (~13%) or ~140 MB (~13.7%) of the
/// pod's 1 Gi limit. The `ConfigMap`'s *conclusion* — that 500 is memory-derived
/// at roughly 13% of the limit — survives both figures unchanged, which is why
/// that file was left alone rather than amended for a tilde-qualified delta.
/// **Where the two disagree, this docstring is correct.**
///
/// **Third term, and the only OPERATOR-TUNABLE one: the datagram SEND buffer.**
/// `MH_DATAGRAM_BUFFER_AUDIO_FRAMES x NOMINAL_AUDIO_FRAME_BYTES` is also a
/// per-connection allocation ceiling — at the deployed 32 frames, 7,552 B, which
/// is ~3% of the ~264 KB above and does not move the ~13% conclusion. It is
/// listed anyway, and listed last, precisely because it is the term an operator
/// can change: a ledger that omits the only tunable term cannot show whoever
/// inherits it (story 2's egress-budget work) the effect of the one value they
/// are able to move. That is the same defect this docstring exists to correct in
/// `configmap.yaml` — an incomplete base someone else re-derives from — and
/// omitting it here while correcting it there would be the narrower version of
/// the same mistake. Bounded above by
/// [`MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING`], so its worst case is
/// `150 x 236 B` = ~35 KB per connection, ~17 MB across 500.
///
/// Changing `resources.limits.memory` in `mh-{0,1}-deployment.yaml` invalidates
/// this arithmetic and requires revisiting `MH_MAX_CONNECTIONS`.
pub const CONNECTION_RECEIVE_WINDOW_BYTES: u32 = 256 * 1024;

/// Per-stream receive window, in bytes.
///
/// Bounded by [`CONNECTION_RECEIVE_WINDOW_BYTES`] and therefore **not additive
/// to it** — see the corrected arithmetic there. Sized well below the
/// connection window so that no single stream can consume the whole of it.
pub const STREAM_RECEIVE_WINDOW_BYTES: u32 = 64 * 1024;

/// Maximum Opus packet size for one frame, in bytes (RFC 6716 §3.4).
///
/// Used only to compute [`MIN_DATAGRAM_RECEIVE_BUFFER_BYTES`]. This is the
/// *codec's own* cap, not a bitrate assumption, and it is why that floor is far
/// above [`NOMINAL_AUDIO_FRAME_BYTES`].
const MAX_OPUS_PACKET_BYTES: usize = 1275;

/// Bytes of encapsulation between an application datagram payload and the QUIC
/// DATAGRAM frame that `max_datagram_frame_size` actually bounds.
///
/// Two things sit in between: the QUIC DATAGRAM frame header (type byte plus a
/// length varint in the length-prefixed variant) and the HTTP/3 WebTransport
/// session-id varint that `wtransport` prepends when writing a datagram. 8 B
/// covers a 3 B frame header and a 4 B session-id varint with a byte to spare.
///
/// Without this term the floor below would assert that the *payload* fits the
/// advertised budget, which is a marginally weaker claim than the one it looks
/// like it is making — it would pass for a value that fails on the wire.
const DATAGRAM_ENCAPSULATION_OVERHEAD_BYTES: usize = 8;

/// Smallest [`DATAGRAM_RECEIVE_BUFFER_BYTES`] that can carry the largest
/// **legitimate** audio datagram, including encapsulation.
///
/// MAXIMUM case throughout — see the convention note on
/// [`NOMINAL_AUDIO_FRAME_BYTES`]. Uses `MAX_HEADER_BYTES` (extension region
/// included) and Opus's packet cap, because this floor must clear the largest
/// frame the codec can legitimately emit.
const MIN_DATAGRAM_RECEIVE_BUFFER_BYTES: usize = MAX_OPUS_PACKET_BYTES
    + media_protocol::frame::MAX_HEADER_BYTES
    + media_protocol::frame::SFRAME_OBJECT_OVERHEAD_BYTES
    + media_protocol::frame::SIGNATURE_SIZE
    + DATAGRAM_ENCAPSULATION_OVERHEAD_BYTES;

/// Ingress datagram buffer, in bytes. Explicit and non-`None` by requirement.
///
/// # Two jobs, and the second one is wire-visible
///
/// 1. It bounds the queue of received datagrams awaiting a read — the
///    ADR-0036 §11 "bounded datagram queue with drop-oldest" ingress cap.
/// 2. quinn **re-uses this same field to advertise `max_datagram_frame_size`**
///    (`min(value, u16::MAX)`), so it is also the largest datagram any peer may
///    send MH. Setting it to `None` would leave datagrams un-negotiated
///    entirely and the media path would not exist.
///
/// # Deliberately NOT derived from `MH_DATAGRAM_BUFFER_AUDIO_FRAMES`
///
/// The send and receive directions have opposing sizing pressures. The send
/// buffer is a latency ceiling and wants to be *small*; this one must be at
/// least [`MIN_DATAGRAM_RECEIVE_BUFFER_BYTES`] or it silently refuses
/// legitimate traffic. Collapsing them would couple a wire-visible negotiated
/// limit to an operator-tunable latency budget.
///
/// # Shrinking this is a client-observable protocol change whose failure MH cannot see
///
/// Below the floor, a client's `send_datagram` fails `TooLarge` **on the
/// client**. MH receives nothing, so no MH counter fires and no MH log line is
/// written: one publisher goes silent with no signal on the side that owns the
/// number. That asymmetry is why the floor is a compile-time assertion rather
/// than a comment — reason from [`MIN_DATAGRAM_RECEIVE_BUFFER_BYTES`], never
/// from a multiple of [`NOMINAL_AUDIO_FRAME_BYTES`]. A ratio argument invites
/// "half of this is still several times the nominal frame, so it is fine",
/// which is true and also below the floor.
pub const DATAGRAM_RECEIVE_BUFFER_BYTES: usize = 2 * 1024;

/// Compile-time pin: the ingress datagram buffer must clear the largest
/// legitimate audio datagram. See [`DATAGRAM_RECEIVE_BUFFER_BYTES`].
const _: () = assert!(
    DATAGRAM_RECEIVE_BUFFER_BYTES >= MIN_DATAGRAM_RECEIVE_BUFFER_BYTES,
    "DATAGRAM_RECEIVE_BUFFER_BYTES is below the largest legitimate audio datagram; peers would \
     silently fail to send, and MH could not observe it"
);

// =============================================================================
// Shutdown drain window (ADR-0036 §11)
// =============================================================================

/// Target settle time after cancellation before the process exits.
///
/// # Raising this obliges amending ADR-0036 §11 in the same change
///
/// §11 states, as a decision and not merely as current behaviour, that MH
/// "marks not-ready, cancels, and **sleeps two seconds** inside a thirty-five
/// second grace period, with no drain phase, and v1 keeps it that way".
/// Draining media sessions is explicitly out of scope for v1. That sentence is
/// true today *because this constant is 2* and there is nothing else tying the
/// two together — so a change here silently falsifies an Accepted ADR. Amend
/// §11 in the same commit, or do not change this.
pub const SHUTDOWN_SETTLE_TARGET_SECONDS: u64 = 2;

/// Seconds reserved between the end of the drain and `SIGKILL`.
///
/// Sized against what runs *after* the sleep, not against nothing: the
/// `TokenManager` task abort, and the OpenTelemetry guard's `Drop` flush at the
/// end of `main`. If an OTLP flush blocks, this margin is the only thing
/// between it and the kill.
pub const SHUTDOWN_MARGIN_SECONDS: u64 = 5;

// =============================================================================
// OpenTelemetry Configuration Defaults (R-55)
// =============================================================================

/// Default head-sampling ratio when `OTEL_SAMPLE_RATE` is unset (1.0 = sample all).
/// The `[0.0, 1.0]` range is owned + enforced by `init_otel` (common), not here —
/// config parsing is parse-only (single owner for the bound; team-lead FREEZE 2026-06-25).
pub const DEFAULT_OTEL_SAMPLE_RATE: f64 = 1.0;
/// Default deployment environment when `DEPLOYMENT_ENVIRONMENT` is unset.
pub const DEFAULT_DEPLOYMENT_ENVIRONMENT: &str = "development";

/// Forwarding-policy bounds applied to `RegisterMeeting` (ADR-0036 §8).
///
/// Grouped rather than loose on [`Config`] so the whole set threads into the
/// gRPC service as one value: a handler that received three of four bounds is
/// not a state worth making representable.
///
/// Every field is a RESOURCE-EXHAUSTION bound. None is a capacity figure, none
/// is advertised to GC, none participates in placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyLimits {
    /// Max `egress_streams` in one registration.
    pub max_egress_streams_per_meeting: usize,
    /// Max `candidate_sources` on one egress stream.
    pub max_candidate_sources_per_egress: usize,
    /// Max egress edges across **all** meetings on this handler.
    pub max_total_egress_edges: usize,
    /// Bound on awaiting the session actor's config-apply reply, in ms. Also
    /// bounds the `EndMeeting` release reply — one bound on "how long an RPC
    /// waits for the session actor", never a second literal.
    pub policy_apply_timeout_ms: u64,
    /// Max meetings held registered on this handler at once
    /// (`MH_MAX_REGISTERED_MEETINGS`, story 2 R-21). Refuses only a NEW
    /// meeting id; a re-registration of a held meeting is never refused.
    pub max_registered_meetings: usize,
}

impl PolicyLimits {
    /// Generous fixture bounds for `src/` unit tests that are not ABOUT a
    /// bound. There is no `Default`: the five keys are required, and a default
    /// would be exactly the second encoding their required-ness removed.
    /// `tests/` binaries use `mh_test_utils::admission::fixture_policy_limits`.
    ///
    /// **These numbers equal the deployed `ConfigMap` values BY HISTORY, NOT BY
    /// REQUIREMENT** — they were the code defaults story 2 task 8 deleted. Any
    /// generous value would do. DO NOT collapse this and the `mh-test-utils`
    /// twin into one shared constant "because they duplicate the ConfigMap":
    /// that resurrects, through the test surface, the very default the
    /// required read removed. (The twin exists because `src/` tests see a
    /// different `mh_service` crate instance through the dev-dependency.)
    #[cfg(test)]
    pub(crate) const fn for_tests() -> Self {
        Self {
            max_egress_streams_per_meeting: 512,
            max_candidate_sources_per_egress: 16,
            max_total_egress_edges: 65_536,
            policy_apply_timeout_ms: 1_000,
            max_registered_meetings: 8_192,
        }
    }
}

/// The egress-budget admission chain, converted and derived ONCE at load.
///
/// Grouped, and deliberately NOT on [`PolicyLimits`]: that struct's contract is
/// "resource exhaustion, never capacity", and this is the capacity figure. See
/// the "Egress-budget admission" section at the top of this module.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EgressAdmission {
    /// `MH_EGRESS_BUDGET_BPS` as configured, in BITS per second.
    /// **Log provenance only — do not compute on this field**; every consumer
    /// reads [`Self::budget_bytes_per_second`].
    pub budget_bps: u64,
    /// `MH_STREAM_COST_AUDIO_BPS` as configured, in bits. Log provenance only.
    pub stream_cost_audio_bps: u64,
    /// `MH_STREAM_COST_VIDEO_BPS` as configured, in bits. Log provenance only.
    pub stream_cost_video_bps: u64,
    /// The budget in BYTES per second (`bits / 8`, floored). What
    /// `mh_media_egress_budget_bytes_per_second` publishes.
    pub budget_bytes_per_second: u64,
    /// Audio stream cost in bytes per second (`bits.div_ceil(8)`).
    pub stream_cost_audio_bytes_per_second: u64,
    /// Video stream cost in bytes per second (`bits.div_ceil(8)`).
    pub stream_cost_video_bytes_per_second: u64,
    /// The derived egress STREAM ceiling:
    /// `budget_bytes / max(cost_audio_bytes, cost_video_bytes)`.
    ///
    /// The worst-case cost keeps MH type-blind (ADR-0036 §7): every admitted
    /// stream is costed as the heavier kind. Guaranteed at load to lie in
    /// `MIN_EGRESS_STREAM_CEILING..=MH_MAX_TOTAL_EGRESS_EDGES`. The ONE value
    /// enforced at admission, advertised to GC, published and logged.
    pub stream_ceiling: u32,
    /// `MH_EGRESS_REJECTION_RATIO_THRESHOLD`, in `0.0..=1.0`. Read by the
    /// session actor's threshold log AND published as
    /// `mh_media_stream_admission_rejection_ratio_threshold` — the same field,
    /// so the exhaustion alert is a bare gauge-to-gauge comparison.
    pub rejection_ratio_threshold: f64,
}

impl EgressAdmission {
    /// Whether the derived ceiling is below the ADVISORY recommended minimum —
    /// the condition for the startup "configure me" nudge. Never a refusal.
    #[must_use]
    pub fn below_recommended_min(&self) -> bool {
        u64::from(self.stream_ceiling) < EGRESS_STREAM_CEILING_RECOMMENDED_MIN
    }

    /// Convert, derive and validate the chain. Pure, so the derivation and every
    /// refusal is unit-testable without an environment.
    ///
    /// # Errors
    ///
    /// [`ConfigError::EgressStreamCeilingBelowFloor`] when the ceiling is below
    /// [`MIN_EGRESS_STREAM_CEILING`];
    /// [`ConfigError::EgressStreamCeilingExceedsEdgeBound`] when it exceeds
    /// `max_total_egress_edges`. The costs must already be non-zero (checked by
    /// the caller with `reject_zero`, so the divisor is non-zero by
    /// construction); a zero divisor is nonetheless mapped to an error rather
    /// than a panic.
    pub fn derive(
        budget_bps: u64,
        stream_cost_audio_bps: u64,
        stream_cost_video_bps: u64,
        rejection_ratio_threshold: f64,
        max_total_egress_edges: usize,
    ) -> Result<Self, ConfigError> {
        // Bits -> bytes, ONCE. Both roundings fail closed (fewer streams).
        let budget_bytes_per_second = budget_bps / 8;
        let stream_cost_audio_bytes_per_second = stream_cost_audio_bps.div_ceil(8);
        let stream_cost_video_bytes_per_second = stream_cost_video_bps.div_ceil(8);
        let max_cost_bytes =
            stream_cost_audio_bytes_per_second.max(stream_cost_video_bytes_per_second);
        let ceiling = budget_bytes_per_second
            .checked_div(max_cost_bytes)
            .ok_or_else(|| {
                ConfigError::InvalidValue(
                    "MH_STREAM_COST_AUDIO_BPS and MH_STREAM_COST_VIDEO_BPS must not both be 0"
                        .to_string(),
                )
            })?;

        if ceiling < MIN_EGRESS_STREAM_CEILING {
            return Err(ConfigError::EgressStreamCeilingBelowFloor {
                ceiling,
                floor: MIN_EGRESS_STREAM_CEILING,
                budget_bps,
                max_stream_cost_bps: stream_cost_audio_bps.max(stream_cost_video_bps),
            });
        }
        let edge_bound = u64::try_from(max_total_egress_edges).unwrap_or(u64::MAX);
        if ceiling > edge_bound {
            return Err(ConfigError::EgressStreamCeilingExceedsEdgeBound {
                ceiling,
                max_total_egress_edges: edge_bound,
            });
        }
        // Unreachable failure: the ceiling is <= the edge bound, which is <=
        // MAX_TOTAL_EGRESS_EDGES_CEILING (asserted to fit i32, hence u32). Kept
        // as an error, never a truncating `as`.
        let stream_ceiling = u32::try_from(ceiling).map_err(|e| {
            ConfigError::InvalidValue(format!(
                "derived egress stream ceiling {ceiling} does not fit the max_streams wire \
                 field: {e}"
            ))
        })?;

        Ok(Self {
            budget_bps,
            stream_cost_audio_bps,
            stream_cost_video_bps,
            budget_bytes_per_second,
            stream_cost_audio_bytes_per_second,
            stream_cost_video_bytes_per_second,
            stream_ceiling,
            rejection_ratio_threshold,
        })
    }
}

/// Parse one REQUIRED numeric bound whose presence the CALLER has already
/// checked, validating both ends: `1..=ceiling`.
///
/// Takes the resolved `&str`, not the map, and that is load-bearing: the
/// presence check (`ConfigError::MissingEnvVar` with a string-literal key)
/// stays written out at every call site for `dt-guard env-config` — see
/// [`parse_required_number`].
///
/// ONE message family. It is a thin wrapper over the same three helpers the
/// other required integer keys use (`parse_required_number` -> [`reject_zero`]
/// -> [`reject_above`]), so a malformed, zero or over-ceiling value is explained
/// identically whichever key it is — the drift this function's predecessor
/// recorded happening once, when it carried its own message.
///
/// Rejecting rather than clamping is deliberate: a clamped value runs under a
/// bound the operator did not choose and never learns about.
fn parse_bounded(
    key: &str,
    raw: &str,
    ceiling: u64,
    zero_consequence: &str,
    above_consequence: &str,
) -> Result<u64, ConfigError> {
    let parsed: u64 = parse_required_number(key, raw)?;
    reject_zero(key, parsed, zero_consequence)?;
    reject_above(key, parsed, ceiling, above_consequence)?;
    Ok(parsed)
}

/// Narrow a bound already validated by [`parse_bounded`] to `usize`.
///
/// Cannot fail on any supported target (every ceiling is far below
/// `u32::MAX`); an error rather than a truncating `as` regardless.
fn bound_to_usize(key: &str, value: u64) -> Result<usize, ConfigError> {
    usize::try_from(value).map_err(|e| {
        ConfigError::InvalidValue(format!("{key}={value} does not fit this platform: {e}"))
    })
}

/// Parse a ratio in `0.0..=1.0`, preserving the offending text and the parse
/// error. Presence stays at the call site (see [`parse_required_number`]).
///
/// ONE home for the ratio message, shared by `MH_MEDIA_LATENCY_SAMPLE_RATIO`
/// (optional) and `MH_EGRESS_REJECTION_RATIO_THRESHOLD` (required).
///
/// The range check is `!(0.0..=1.0).contains(..)`, NEVER `x < 0.0 || x > 1.0`:
/// `"NaN".parse::<f64>()` SUCCEEDS, and NaN compares false against everything,
/// so the two-sided form accepts it. A NaN threshold would make every
/// comparison false and the exhaustion alert silent forever; the `contains`
/// form rejects NaN and both infinities.
fn parse_unit_ratio(key: &str, raw: &str, consequence: &str) -> Result<f64, ConfigError> {
    let parsed: f64 = raw.trim().parse().map_err(|e| {
        ConfigError::InvalidValue(format!(
            "{key} must be a number in 0.0..=1.0, got '{raw}': {e}"
        ))
    })?;
    if !(0.0..=1.0).contains(&parsed) {
        return Err(ConfigError::InvalidValue(format!(
            "{key} must be in 0.0..=1.0, got {parsed} — {consequence}. Remediation: set {key} \
             in infra/services/mh-service/configmap.yaml to a value in 0.0..=1.0"
        )));
    }
    Ok(parsed)
}

/// The QUIC transport parameters MH declares explicitly (ADR-0036 §1).
///
/// Grouped rather than loose on [`Config`] so the whole set threads into the
/// WebTransport server as one value, and so the ENV-DRIVEN half of the
/// transport configuration is visually distinct from the compile-time half that
/// lives as constants at the top of this module.
///
/// Every field here is REQUIRED at load: there is no silent default anywhere in
/// this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuicTransportParams {
    /// Maximum concurrent unidirectional streams a **peer** may have open
    /// toward MH (env `MH_MAX_CONCURRENT_UNI_STREAMS`).
    ///
    /// A QUIC **transport** bound, distinct from any capacity figure. MH
    /// advertises it and quinn enforces it by withholding stream credit, so a
    /// peer at the limit *stalls* rather than MH rejecting — there is no
    /// MH-side rejection metric for it. It does not bound MH's egress; the
    /// subscriber's own advertised limit does.
    pub max_concurrent_uni_streams: u32,

    /// Datagram send buffer, in FRAMES of audio (env
    /// `MH_DATAGRAM_BUFFER_AUDIO_FRAMES`).
    ///
    /// ADR-0036 §1 requires the frame form, because "1 MiB" does not make
    /// ≈89 seconds of queued audio visible while a frame count does. Retained
    /// alongside the derived byte value so the startup log can print **both**
    /// links of the conversion — printing only bytes would hide the conversion
    /// at the moment an operator is debugging it.
    pub datagram_send_buffer_audio_frames: usize,

    /// Datagram send buffer in bytes, converted **once** at config load as
    /// `frames × NOMINAL_AUDIO_FRAME_BYTES`.
    ///
    /// Single-point conversion, upstream of every consumer, so enforcement and
    /// any future gauge cannot disagree about the unit.
    pub datagram_send_buffer_bytes: usize,

    /// QUIC connection-level keepalive, in milliseconds (env
    /// `MH_KEEPALIVE_INTERVAL_MS`).
    ///
    /// This is what refreshes the NAT binding of a **muted** participant, who
    /// by definition sends no media (ADR-0036 §1, §5) — without it an
    /// intermediary can reap the path and unmute is not instantaneous.
    pub keepalive_interval_ms: u64,
}

impl QuicTransportParams {
    /// Datagram send buffer expressed as queued audio latency, in
    /// milliseconds — the figure ADR-0036 §1 reasons with.
    #[must_use]
    pub const fn datagram_send_buffer_ms(&self) -> usize {
        self.datagram_send_buffer_audio_frames * AUDIO_FRAME_DURATION_MS
    }

    /// The `max_datagram_frame_size` MH will advertise to peers.
    ///
    /// Derived rather than configured: quinn computes it as
    /// `min(datagram_receive_buffer_size, u16::MAX)`. Exposed so the startup
    /// log can print the negotiated limit an operator would otherwise have to
    /// read quinn's source to learn.
    #[must_use]
    pub const fn max_datagram_frame_size_bytes(&self) -> usize {
        if DATAGRAM_RECEIVE_BUFFER_BYTES < u16::MAX as usize {
            DATAGRAM_RECEIVE_BUFFER_BYTES
        } else {
            u16::MAX as usize
        }
    }
}

/// Which arm of `min(SETTLE_TARGET, grace − MARGIN)` produced the drain window.
///
/// # Why a two-variant enum earns its place even though one variant is nearly unreachable
///
/// With `SHUTDOWN_SETTLE_TARGET_SECONDS = 2` and `SHUTDOWN_MARGIN_SECONDS = 5`,
/// the [`Self::GraceMinusMargin`] arm binds **only** at `grace = 6` (yielding
/// 1 s) — every larger grace resolves to the settle target. A future reader
/// will see a variant that essentially never occurs and read it as dead weight.
///
/// It is not. It is the only thing that lets an operator distinguish "2 s
/// because that is the settle target" from "1 s because someone set the
/// termination grace to 6", from `kubectl logs` alone and without reading
/// source. The near-vacuousness of the `min()` is the point: ADR-0036 §11
/// mandates deriving the drain rather than validating it, and
/// [`ConfigError::TerminationGraceTooSmall`] is what actually protects the
/// invariant. This field reports which happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainWindowSource {
    /// The drain equals the settle target — the normal case.
    SettleTarget,
    /// The drain was clamped by a small termination grace.
    GraceMinusMargin,
}

impl DrainWindowSource {
    /// Stable, bounded string form for the structured startup log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SettleTarget => "settle_target",
            Self::GraceMinusMargin => "grace_minus_margin",
        }
    }
}

/// Parse an already-resolved required value, preserving the offending text and
/// the underlying parse error.
///
/// **This helper deliberately does NOT do the presence check.** `dt-guard
/// env-config` discovers MH's required variables by scanning
/// `crates/mh-service/src/config.rs` for a `MissingEnvVar` construction whose
/// argument is a **string literal** naming the variable (regex at
/// `crates/dt-guard/src/env_config.rs:186`; the file is opened by exact path at
/// `:590`). A `require(vars, "<var>")` helper would erase every one of those
/// literals, and the guard would go blind to all of them **while still printing
/// `STATUS=OK`** — a clean verdict for surface it silently stopped checking. So
/// the `.ok_or_else(...)` presence checks stay written out at every call site,
/// and only the numeric parse — which the guard does not read — is shared.
///
/// Corollary for anyone editing the docs here, learned the direct way while
/// writing them: **the guard scans comments too.** An illustrative
/// `MissingEnvVar` construction written in prose with an upper-case string
/// literal is indistinguishable from a real one, and gets reported as a
/// required variable that no manifest declares. That is the guard being
/// appropriately literal rather than a bug in it — so write the construction in
/// real code only, and describe it in prose without instantiating it. This
/// paragraph is deliberately phrased to obey its own rule.
///
/// Rejects rather than defaults: a malformed value that quietly reverts to a
/// default runs under a bound the operator did not choose and never learns
/// about.
fn parse_required_number<T>(key: &str, raw: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: fmt::Display,
{
    raw.trim().parse::<T>().map_err(|e| {
        ConfigError::InvalidValue(format!(
            "{key} must be a positive integer, got '{raw}': {e}"
        ))
    })
}

/// Reject a required numeric bound of zero, naming what zero would silently do.
///
/// Names the `ConfigMap` as the remediation location, which is truthful for
/// every caller: each supplies a key that lives in `mh-service-config`.
///
/// CORRECTED (story 2 task 8): this used to say it was deliberately kept out of
/// the ADR-0036 §8 bounds' path because those keys were "in no manifest
/// today". That premise expired when the §8 keys landed in the `ConfigMap` and
/// their reads became required, so [`parse_bounded`] now calls this too — one
/// message family for every required integer key. It stays separate from
/// [`parse_required_number`] only because a caller may want the parse without
/// the zero check.
fn reject_zero(key: &str, value: u64, consequence: &str) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::InvalidValue(format!(
            "{key} must be greater than 0, got 0 — {consequence}. Remediation: set {key} in \
             infra/services/mh-service/configmap.yaml"
        )));
    }
    Ok(())
}

/// Reject a required numeric bound above its hard code-level ceiling, naming
/// the ceiling and what exceeding it would silently cost.
///
/// Mirror of [`reject_zero`] at the other end of the range. Ceilings live in
/// code rather than in a manifest so the guarantee holds in every environment,
/// including those with no `ConfigMap` — the same reasoning the ADR-0036 §8
/// bounds in this module already apply.
fn reject_above(key: &str, value: u64, ceiling: u64, consequence: &str) -> Result<(), ConfigError> {
    if value > ceiling {
        return Err(ConfigError::InvalidValue(format!(
            "{key} must be at most {ceiling}, got {value} — {consequence}. Remediation: lower \
             {key} in infra/services/mh-service/configmap.yaml to {ceiling} or less"
        )));
    }
    Ok(())
}

/// Where an optional configuration value came from, for the startup log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    /// Read from the environment (the `ConfigMap`, in a cluster).
    Env,
    /// The code default — the key was absent.
    Default,
}

impl ValueSource {
    /// Stable, bounded string form for the structured startup log line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Default => "default",
        }
    }
}

/// Media Handler configuration.
///
/// Loaded from environment variables with sensible defaults.
/// Sensitive fields are redacted in Debug output.
#[derive(Clone)]
pub struct Config {
    /// gRPC server bind address for MC→MH communication (default: "0.0.0.0:50053").
    pub grpc_bind_address: String,

    /// Health endpoint bind address (default: "0.0.0.0:8083").
    pub health_bind_address: String,

    /// WebTransport server bind address (default: "0.0.0.0:4434").
    pub webtransport_bind_address: String,

    /// Deployment region identifier (e.g., "us-east-1").
    pub region: String,

    /// URL to Global Controller for registration.
    pub gc_grpc_url: String,

    /// Unique identifier for this MH instance.
    pub handler_id: String,

    /// Authentication Controller endpoint for OAuth token acquisition.
    pub ac_endpoint: String,

    /// OAuth client ID for MH (used for client credentials flow to AC).
    pub client_id: String,

    /// OAuth client secret for MH (used for client credentials flow to AC).
    /// Protected by `SecretString` to prevent accidental logging.
    pub client_secret: SecretString,

    /// Path to TLS certificate file (PEM) for WebTransport server.
    pub tls_cert_path: String,

    /// Path to TLS private key file (PEM) for WebTransport server.
    pub tls_key_path: String,

    /// Advertised gRPC address for GC registration.
    /// This is the address GC uses to reach this MH pod (e.g., `grpc://10.244.0.5:50053`).
    /// Required environment variable: `MH_GRPC_ADVERTISE_ADDRESS`.
    pub grpc_advertise_address: String,

    /// Advertised WebTransport address for GC registration.
    /// This is the address GC uses to reach this MH pod (e.g., `https://10.244.0.5:4434`).
    /// Required environment variable: `MH_WEBTRANSPORT_ADVERTISE_ADDRESS`.
    pub webtransport_advertise_address: String,

    /// AC JWKS endpoint URL for JWT validation.
    /// Required environment variable: `AC_JWKS_URL`.
    pub ac_jwks_url: String,

    /// `RegisterMeeting` arrival timeout in seconds (default: 15).
    /// Clients connecting to unregistered meetings are provisionally accepted
    /// for this duration; if `RegisterMeeting` does not arrive, they are disconnected.
    pub register_meeting_timeout_seconds: u64,

    /// Accept-time resource-exhaustion guard: the maximum number of concurrent
    /// WebTransport connections MH will accept (env `MH_MAX_CONNECTIONS`,
    /// REQUIRED).
    ///
    /// **Never a capacity figure, and never advertised to GC.** That is the
    /// derived egress stream ceiling ([`Self::egress_admission`]), a different
    /// quantity enforced at stream admission and advertised as `max_streams`.
    /// This one is enforced by mh-service itself, in
    /// [`crate::webtransport::server`], before allocating handler resources —
    /// unchanged by the ADR-0036 §1 work; only its provenance and its label
    /// changed.
    ///
    /// It sits far above expected peak, so rejections attributed to it mean a
    /// connection flood or a connection leak, **not** demand that has outgrown
    /// the deployment. Scaling out is the wrong response; find the source.
    ///
    /// Required rather than defaulted because its previous 10,000 code default
    /// was in no manifest, was chosen by nobody, and — being a default — was
    /// invisible to `dt-guard env-config`, whose check 1 keys on
    /// `ConfigError::MissingEnvVar` reads. The bound only becomes memory-derived
    /// once the receive-side flow control at the top of this module is
    /// declared; see [`CONNECTION_RECEIVE_WINDOW_BYTES`] for the arithmetic.
    pub max_connections: usize,

    /// Explicit QUIC transport parameters (ADR-0036 §1). All REQUIRED.
    pub quic_transport: QuicTransportParams,

    /// Pod termination grace period in seconds (env
    /// `MH_TERMINATION_GRACE_SECONDS`, REQUIRED).
    ///
    /// **Derived at build time, never hand-typed.** The kustomize
    /// `replacements:` block in `infra/services/mh-service/kustomization.yaml`
    /// writes each instance's own
    /// `spec.template.spec.terminationGracePeriodSeconds` into that instance's
    /// own env, so the two cannot drift. The literal in the deployment file is
    /// a `"0"` sentinel; a pod reading `0` means the replacement did not run
    /// (someone used `kubectl apply -f` instead of `apply -k`) and MH refuses to
    /// start rather than draining for a window nobody chose.
    pub termination_grace_seconds: u64,

    /// Post-cancellation settle window, derived once at load as
    /// `min(SHUTDOWN_SETTLE_TARGET, grace − SHUTDOWN_MARGIN)`.
    ///
    /// Derived rather than validated, per ADR-0036 §11 ("a derived value cannot
    /// drift; a guard only catches drift after someone introduces it"). `main`
    /// sleeps for exactly this and holds no seconds literal of its own.
    pub drain_window: std::time::Duration,

    /// Which arm of the `min()` produced [`Self::drain_window`].
    pub drain_window_source: DrainWindowSource,

    /// Forwarding-policy bounds for the ADR-0036 §8 control plane.
    pub policy_limits: PolicyLimits,

    /// The egress-budget admission chain (story 2 R-19): budget, costs, the
    /// derived stream ceiling and the rejection-ratio threshold.
    pub egress_admission: EgressAdmission,

    /// Fraction of forwarded frames whose latency is observed into
    /// `mh_media_forward_latency_seconds` (env
    /// `MH_MEDIA_LATENCY_SAMPLE_RATIO`, optional, default
    /// [`DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO`], ceiling-checked to `0.0..=1.0`).
    ///
    /// **ONE field, TWO readers, and that is the point.** The sampler
    /// ([`crate::media::sampler`]) draws against it, and
    /// `mh_media_latency_sample_ratio` publishes this same field once at setup.
    /// A published ratio computed separately from the one the sampler reads is
    /// a gauge that lies exactly when it matters — ADR-0036 §11's
    /// derive-rather-than-guard rule, applied to a two-line temptation.
    ///
    /// **Not [`Self::otel_sample_rate`].** That is TRACE head-sampling and is
    /// validated by `init_otel`; this is media-path histogram sampling. The two
    /// have different owners, different consumers and different failure modes,
    /// and collapsing them would put the media path's leak posture behind a
    /// tracing knob.
    ///
    /// Optional-with-default rather than required — a REASONED carve-out, not
    /// a missed instance of story 2's "required keys" flip: both deployments
    /// declare this the one deliberately `optional:` ref, so the image may roll
    /// back alone (`infra/services/mh-service/mh-0-deployment.yaml`). Its
    /// provenance is reported by [`Self::media_latency_sample_ratio_source`].
    pub media_latency_sample_ratio: f64,

    /// Whether [`Self::media_latency_sample_ratio`] came from the environment
    /// or the code default — logged at startup so an operator can tell a
    /// chosen value from an unchosen one without reading source.
    pub media_latency_sample_ratio_source: ValueSource,

    /// Whether to initialize the OpenTelemetry SDK (R-55). Default `false`.
    /// When `true`, `main` calls `init_otel` (eager collector probe, fail-hard
    /// at init) and composes the tracing-opentelemetry layer; when `false`, no
    /// `OTel` layer is added and no collector probe occurs. Enablement is this
    /// explicit boolean, NOT presence of `otel_endpoint`.
    pub otel_enabled: bool,
    /// OTLP-gRPC collector endpoint (env `OTLP_ENDPOINT`). Only consumed when
    /// `otel_enabled` is `true`; must be a parseable `http(s)://host:port` URL.
    pub otel_endpoint: String,
    /// Head-sampling ratio in `[0.0, 1.0]` (env `OTEL_SAMPLE_RATE`). `init_otel`
    /// is the authoritative validator and re-checks this bound at startup.
    pub otel_sample_rate: f64,
    /// Deployment environment for the `deployment.environment` `OTel` resource
    /// attribute (env `DEPLOYMENT_ENVIRONMENT`, ADR-0011). Default `development`.
    pub environment: String,
}

/// Custom Debug implementation that redacts sensitive fields.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("grpc_bind_address", &self.grpc_bind_address)
            .field("health_bind_address", &self.health_bind_address)
            .field("webtransport_bind_address", &self.webtransport_bind_address)
            .field("region", &self.region)
            .field("gc_grpc_url", &self.gc_grpc_url)
            .field("handler_id", &self.handler_id)
            .field("ac_endpoint", &self.ac_endpoint)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("tls_cert_path", &self.tls_cert_path)
            .field("tls_key_path", &self.tls_key_path)
            .field("grpc_advertise_address", &self.grpc_advertise_address)
            .field(
                "webtransport_advertise_address",
                &self.webtransport_advertise_address,
            )
            .field("ac_jwks_url", &self.ac_jwks_url)
            .field(
                "register_meeting_timeout_seconds",
                &self.register_meeting_timeout_seconds,
            )
            .field("max_connections", &self.max_connections)
            .field("quic_transport", &self.quic_transport)
            .field("termination_grace_seconds", &self.termination_grace_seconds)
            .field("drain_window", &self.drain_window)
            .field("drain_window_source", &self.drain_window_source)
            .field("policy_limits", &self.policy_limits)
            .field("egress_admission", &self.egress_admission)
            .field(
                "media_latency_sample_ratio",
                &self.media_latency_sample_ratio,
            )
            .field(
                "media_latency_sample_ratio_source",
                &self.media_latency_sample_ratio_source,
            )
            .field("otel_enabled", &self.otel_enabled)
            .field("otel_endpoint", &self.otel_endpoint)
            .field("otel_sample_rate", &self.otel_sample_rate)
            .field("environment", &self.environment)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),

    #[error("Invalid OTel configuration: {0}")]
    InvalidOtelConfig(String),

    // -------------------------------------------------------------------------
    // ADR-0036 §1 startup validations.
    //
    // Each is a DISTINCT fielded variant rather than another `InvalidValue`
    // string, and that is a deliberate observability decision rather than a
    // style one. `main` loads configuration BEFORE initialising the tracing
    // subscriber (the OTel layer needs the endpoint from config, and the
    // subscriber can only be `init`ed once), so a rejected configuration
    // produces no structured event at all — only the default `Debug`-to-stderr
    // dump of this error. The variant IS the entire operator-facing surface.
    //
    // Each message therefore names the invariant, the offending value, the
    // bound it was compared against, AND the remediation location — which
    // ConfigMap key or which pod-spec field to change. The refusal line is the
    // runbook, because in a CrashLoop it is all the operator gets.
    // -------------------------------------------------------------------------
    /// MH's own egress queue bound does not trip before the transport ceiling.
    #[error(
        "MH_DATAGRAM_BUFFER_AUDIO_FRAMES={datagram_buffer_audio_frames} must be strictly greater \
         than the application egress queue bound ({egress_queue_frames} frames): ADR-0036 §1 \
         requires MH's own queue to trip FIRST so back-pressure is countable in MH's code rather \
         than silently discarded inside quinn. Remediation: raise \
         MH_DATAGRAM_BUFFER_AUDIO_FRAMES in infra/services/mh-service/configmap.yaml above \
         {egress_queue_frames}"
    )]
    EgressQueueDoesNotBindFirst {
        /// The compile-time application-level egress queue bound, in frames.
        egress_queue_frames: usize,
        /// The configured transport datagram send buffer, in frames.
        datagram_buffer_audio_frames: usize,
    },

    /// The termination grace leaves no room for the shutdown margin.
    #[error(
        "MH_TERMINATION_GRACE_SECONDS={grace_seconds} must be strictly greater than the shutdown \
         margin ({margin_seconds}s) reserved for TokenManager teardown and the OpenTelemetry \
         flush. A value of 0 means the kustomize replacement did not run — the checked-in literal \
         is a sentinel. Remediation: deploy with `kubectl apply -k`, not `apply -f`, and check \
         spec.template.spec.terminationGracePeriodSeconds in \
         infra/services/mh-service/mh-{{0,1}}-deployment.yaml"
    )]
    TerminationGraceTooSmall {
        /// The configured termination grace, in seconds.
        grace_seconds: u64,
        /// The compile-time shutdown margin, in seconds.
        margin_seconds: u64,
    },

    /// The derived egress stream ceiling is below the refuse-boot floor.
    ///
    /// Until the MH deployment runbook's config-failure section lands (story 2
    /// task 18) this message IS the runbook: it is all `kubectl logs --previous`
    /// shows on a `CrashLoopBackOff`.
    #[error(
        "Derived egress STREAM ceiling {ceiling} is below the refuse-boot floor of {floor} \
         (two participants hearing each other). ceiling = MH_EGRESS_BUDGET_BPS \
         ({budget_bps} bit/s) / max(MH_STREAM_COST_AUDIO_BPS, MH_STREAM_COST_VIDEO_BPS) \
         ({max_stream_cost_bps} bit/s), converted to bytes and floored. A handler this small \
         would boot, be placed by GC, and then reject nearly every stream. Remediation: RAISE \
         MH_EGRESS_BUDGET_BPS in infra/services/mh-service/configmap.yaml. NEVER lower the \
         floor (a code constant) to quiet this refusal"
    )]
    EgressStreamCeilingBelowFloor {
        /// The derived ceiling.
        ceiling: u64,
        /// The refuse-boot floor (`MIN_EGRESS_STREAM_CEILING`).
        floor: u64,
        /// `MH_EGRESS_BUDGET_BPS` as configured, in bits per second.
        budget_bps: u64,
        /// The larger of the two per-stream costs, in bits per second.
        max_stream_cost_bps: u64,
    },

    /// The derived egress stream ceiling exceeds the aggregate edge bound that
    /// backstops it.
    #[error(
        "Derived egress STREAM ceiling {ceiling} exceeds MH_MAX_TOTAL_EGRESS_EDGES \
         ({max_total_egress_edges}). The resource-exhaustion guard must sit at or above the \
         capacity it backstops: otherwise GC is advertised a capacity MH refuses at the \
         resource guard. Remediation: raise MH_MAX_TOTAL_EGRESS_EDGES to at least {ceiling}, or \
         lower MH_EGRESS_BUDGET_BPS, in infra/services/mh-service/configmap.yaml"
    )]
    EgressStreamCeilingExceedsEdgeBound {
        /// The derived ceiling.
        ceiling: u64,
        /// The configured `MH_MAX_TOTAL_EGRESS_EDGES`.
        max_total_egress_edges: u64,
    },

    /// The keepalive interval is too close to the idle timeout.
    ///
    /// Scope limit, stated so nobody reads this validation as a wire-level
    /// guarantee: it checks the ratio against **MH's own declared**
    /// `max_idle_timeout` only. Per RFC 9000 §10.1 the effective idle timeout is
    /// `min(local, peer)`, so a client advertising a shorter timeout can still
    /// be reaped despite a fully conforming MH keepalive. Not exploitable
    /// against MH — the client harms only itself — but a client-side reap is not
    /// excluded by this check passing.
    #[error(
        "MH_KEEPALIVE_INTERVAL_MS={keepalive_ms} is too large: keepalive must be at most \
         max_idle_timeout / {min_ratio} = {max_allowed_ms}ms (idle timeout {idle_timeout_ms}ms), \
         so a single lost keepalive still leaves margin before the peer times out. Remediation: \
         lower MH_KEEPALIVE_INTERVAL_MS in infra/services/mh-service/configmap.yaml to \
         {max_allowed_ms} or less"
    )]
    KeepaliveTooLarge {
        /// The configured keepalive interval, in milliseconds.
        keepalive_ms: u64,
        /// MH's declared `max_idle_timeout`, in milliseconds.
        idle_timeout_ms: u64,
        /// Minimum number of keepalives that must fit in one idle timeout.
        min_ratio: u64,
        /// The largest keepalive the ratio permits, in milliseconds.
        max_allowed_ms: u64,
    },
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingEnvVar` if a required variable is missing.
    /// Returns `ConfigError::InvalidValue` if a value is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(&env::vars().collect())
    }

    /// Load configuration from a `HashMap` (for testing).
    ///
    /// # Errors
    ///
    /// Returns `ConfigError::MissingEnvVar` if a required variable is missing.
    /// Returns `ConfigError::InvalidValue` if a value is invalid.
    #[expect(
        clippy::too_many_lines,
        reason = "Sequential env var parsing; splitting would obscure config loading flow"
    )]
    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let ac_endpoint = vars
            .get("AC_ENDPOINT")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_ENDPOINT".to_string()))?
            .clone();

        let client_id = vars
            .get("MH_CLIENT_ID")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_CLIENT_ID".to_string()))?
            .clone();

        let client_secret = SecretString::from(
            vars.get("MH_CLIENT_SECRET")
                .ok_or_else(|| ConfigError::MissingEnvVar("MH_CLIENT_SECRET".to_string()))?
                .clone(),
        );

        let tls_cert_path = vars
            .get("MH_TLS_CERT_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_TLS_CERT_PATH".to_string()))?
            .clone();

        let tls_key_path = vars
            .get("MH_TLS_KEY_PATH")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_TLS_KEY_PATH".to_string()))?
            .clone();

        // Validate TLS cert and key files exist at startup (fail-fast)
        if !std::path::Path::new(&tls_cert_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MH_TLS_CERT_PATH file does not exist: {tls_cert_path}"
            )));
        }
        if !std::path::Path::new(&tls_key_path).exists() {
            return Err(ConfigError::InvalidValue(format!(
                "MH_TLS_KEY_PATH file does not exist: {tls_key_path}"
            )));
        }

        let grpc_advertise_address = vars
            .get("MH_GRPC_ADVERTISE_ADDRESS")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_GRPC_ADVERTISE_ADDRESS".to_string()))?
            .clone();

        let webtransport_advertise_address = vars
            .get("MH_WEBTRANSPORT_ADVERTISE_ADDRESS")
            .ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string())
            })?
            .clone();

        let grpc_bind_address = vars
            .get("MH_GRPC_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_GRPC_BIND_ADDRESS.to_string());

        let health_bind_address = vars
            .get("MH_HEALTH_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_HEALTH_BIND_ADDRESS.to_string());

        let webtransport_bind_address = vars
            .get("MH_WEBTRANSPORT_BIND_ADDRESS")
            .cloned()
            .unwrap_or_else(|| DEFAULT_WEBTRANSPORT_BIND_ADDRESS.to_string());

        let region = vars
            .get("MH_REGION")
            .cloned()
            .unwrap_or_else(|| "us-east-1".to_string());

        let gc_grpc_url = vars
            .get("GC_GRPC_URL")
            .cloned()
            .unwrap_or_else(|| "http://localhost:50051".to_string());

        let ac_jwks_url = vars
            .get("AC_JWKS_URL")
            .ok_or_else(|| ConfigError::MissingEnvVar("AC_JWKS_URL".to_string()))?
            .clone();

        // Basic validation: JWKS URL must use http:// or https://
        if !ac_jwks_url.starts_with("http://") && !ac_jwks_url.starts_with("https://") {
            return Err(ConfigError::InvalidValue(
                "AC_JWKS_URL must start with http:// or https://".to_string(),
            ));
        }

        let register_meeting_timeout_seconds = vars
            .get("MH_REGISTER_MEETING_TIMEOUT_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS)
            .min(MAX_REGISTER_MEETING_TIMEOUT_SECONDS);

        // ---------------------------------------------------------------------
        // ADR-0036 §1 explicit QUIC transport parameters, plus the termination
        // grace that drives the shutdown drain. ALL REQUIRED — no silent
        // defaults, so `dt-guard env-config` check 1 sees every one of them and
        // a missing manifest key is a loud refusal rather than an unchosen
        // value that runs forever.
        //
        // The `ConfigError::MissingEnvVar("...")` literals below are
        // guard-load-bearing; see `parse_required_number`.
        // ---------------------------------------------------------------------
        let max_connections: usize = parse_required_number(
            "MH_MAX_CONNECTIONS",
            vars.get("MH_MAX_CONNECTIONS")
                .ok_or_else(|| ConfigError::MissingEnvVar("MH_MAX_CONNECTIONS".to_string()))?,
        )?;
        reject_zero(
            "MH_MAX_CONNECTIONS",
            max_connections as u64,
            "MH would refuse every WebTransport connection at accept",
        )?;

        let max_concurrent_uni_streams: u32 = parse_required_number(
            "MH_MAX_CONCURRENT_UNI_STREAMS",
            vars.get("MH_MAX_CONCURRENT_UNI_STREAMS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_MAX_CONCURRENT_UNI_STREAMS".to_string())
            })?,
        )?;
        reject_zero(
            "MH_MAX_CONCURRENT_UNI_STREAMS",
            u64::from(max_concurrent_uni_streams),
            "peers could never open a unidirectional stream to MH",
        )?;
        reject_above(
            "MH_MAX_CONCURRENT_UNI_STREAMS",
            u64::from(max_concurrent_uni_streams),
            u64::from(MAX_CONCURRENT_UNI_STREAMS_CEILING),
            "above quinn's own default the declaration stops tightening anything — MH would be \
             loosening a limit it declared in order to be tighter than the default (ADR-0036 §1)",
        )?;

        let datagram_send_buffer_audio_frames: usize = parse_required_number(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES",
            vars.get("MH_DATAGRAM_BUFFER_AUDIO_FRAMES").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string())
            })?,
        )?;

        let keepalive_interval_ms: u64 = parse_required_number(
            "MH_KEEPALIVE_INTERVAL_MS",
            vars.get("MH_KEEPALIVE_INTERVAL_MS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_KEEPALIVE_INTERVAL_MS".to_string())
            })?,
        )?;
        // Zero is not "no keepalive configured" — in quinn a zero/absent
        // keep_alive_interval means DISABLED, so a fat-fingered 0 silently
        // stops refreshing the NAT binding of every muted participant and
        // reads as configured-on-purpose forever.
        reject_zero(
            "MH_KEEPALIVE_INTERVAL_MS",
            keepalive_interval_ms,
            "in QUIC a zero keepalive means DISABLED, which silently stops refreshing the NAT \
             binding of every muted participant (ADR-0036 §1, §5)",
        )?;

        let termination_grace_seconds: u64 = parse_required_number(
            "MH_TERMINATION_GRACE_SECONDS",
            vars.get("MH_TERMINATION_GRACE_SECONDS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_TERMINATION_GRACE_SECONDS".to_string())
            })?,
        )?;

        // --- Startup validations. Each fails loudly with its own variant. ---

        // Upper end of the datagram-buffer range. Checked BEFORE the
        // frames -> bytes multiply below, which wraps silently in release
        // (`[profile.release]` sets no `overflow-checks`) and would otherwise
        // turn an absurd value into an arbitrary small buffer with no error.
        reject_above(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES",
            datagram_send_buffer_audio_frames as u64,
            MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING as u64,
            &format!(
                "the datagram send buffer is a LATENCY ceiling, and this would queue up to {}ms \
                 of audio per connection against a {MAX_DATAGRAM_BUFFER_LATENCY_MS}ms maximum \
                 (ADR-0036 §1: prefer loss to unbounded latency). It is also a per-connection \
                 allocation multiplied by MH_MAX_CONNECTIONS",
                datagram_send_buffer_audio_frames.saturating_mul(AUDIO_FRAME_DURATION_MS)
            ),
        )?;

        // V1 (ADR-0036 §1): MH's own queue must trip before quinn's buffer.
        if datagram_send_buffer_audio_frames <= EGRESS_QUEUE_FRAMES {
            return Err(ConfigError::EgressQueueDoesNotBindFirst {
                egress_queue_frames: EGRESS_QUEUE_FRAMES,
                datagram_buffer_audio_frames: datagram_send_buffer_audio_frames,
            });
        }

        // V2: the grace must leave room for the margin. This runs BEFORE the
        // drain is computed, so the subtraction below is already known safe;
        // the `saturating_sub` there is belt-and-braces on an unsigned type
        // where the failure mode would be an effectively infinite drain.
        if termination_grace_seconds <= SHUTDOWN_MARGIN_SECONDS {
            return Err(ConfigError::TerminationGraceTooSmall {
                grace_seconds: termination_grace_seconds,
                margin_seconds: SHUTDOWN_MARGIN_SECONDS,
            });
        }

        // V3: a keepalive is only effective if it arrives well before the peer
        // times out. See `ConfigError::KeepaliveTooLarge` for the scope limit.
        let idle_timeout_ms = MAX_IDLE_TIMEOUT_SECONDS * 1_000;
        let max_allowed_keepalive_ms = idle_timeout_ms / MIN_KEEPALIVE_TO_IDLE_RATIO;
        if keepalive_interval_ms > max_allowed_keepalive_ms {
            return Err(ConfigError::KeepaliveTooLarge {
                keepalive_ms: keepalive_interval_ms,
                idle_timeout_ms,
                min_ratio: MIN_KEEPALIVE_TO_IDLE_RATIO,
                max_allowed_ms: max_allowed_keepalive_ms,
            });
        }

        // Frames -> bytes, converted ONCE, upstream of every consumer.
        let quic_transport = QuicTransportParams {
            max_concurrent_uni_streams,
            datagram_send_buffer_audio_frames,
            datagram_send_buffer_bytes: datagram_send_buffer_audio_frames
                * NOMINAL_AUDIO_FRAME_BYTES,
            keepalive_interval_ms,
        };

        // Drain window, derived rather than validated (ADR-0036 §11).
        let grace_minus_margin = termination_grace_seconds.saturating_sub(SHUTDOWN_MARGIN_SECONDS);
        let (drain_seconds, drain_window_source) =
            if SHUTDOWN_SETTLE_TARGET_SECONDS <= grace_minus_margin {
                (
                    SHUTDOWN_SETTLE_TARGET_SECONDS,
                    DrainWindowSource::SettleTarget,
                )
            } else {
                (grace_minus_margin, DrainWindowSource::GraceMinusMargin)
            };
        let drain_window = std::time::Duration::from_secs(drain_seconds);

        // ADR-0036 §8 policy bounds — REQUIRED (story 2 R-22), no code
        // default: the ConfigMap is the single home. `parse_bounded` does the
        // rest with the one message family every required integer key shares.
        //
        // THE RULE, and it is the whole rule: `MissingEnvVar(` and its string
        // literal must stay on the SAME LINE. `dt-guard env-config` discovers
        // MH's required variables by matching that construction in this file
        // (`crates/dt-guard/src/env_config.rs`), and it cannot match across a
        // line break between the two. The surrounding `.ok_or_else(...)` chain
        // may wrap freely — and does, below.
        //
        // That is why each raw value is bound at STATEMENT level rather than
        // nested inside the `parse_bounded(...)` call: nesting pushed two of
        // these past rustfmt's width, the formatter broke the line at exactly
        // that point, and the guard went blind to those two keys WHILE STILL
        // PRINTING STATUS=OK. The construction is deliberately not written out
        // in this comment — the guard reads comments too, so an illustrative
        // one is reported as a required variable no manifest declares. A unit
        // test in this module applies the guard's requirement to this file, so
        // a future re-wrap fails there rather than silently.
        let raw_max_egress_streams =
            vars.get("MH_MAX_EGRESS_STREAMS_PER_MEETING")
                .ok_or_else(|| {
                    ConfigError::MissingEnvVar("MH_MAX_EGRESS_STREAMS_PER_MEETING".to_string())
                })?;
        let raw_max_candidate_sources = vars
            .get("MH_MAX_CANDIDATE_SOURCES_PER_EGRESS")
            .ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_MAX_CANDIDATE_SOURCES_PER_EGRESS".to_string())
            })?;
        let raw_max_total_edges = vars
            .get("MH_MAX_TOTAL_EGRESS_EDGES")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_MAX_TOTAL_EGRESS_EDGES".to_string()))?;
        let raw_policy_apply_timeout = vars
            .get("MH_POLICY_APPLY_TIMEOUT_MS")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_POLICY_APPLY_TIMEOUT_MS".to_string()))?;
        let raw_max_registered_meetings = vars
            .get("MH_MAX_REGISTERED_MEETINGS")
            .ok_or_else(|| ConfigError::MissingEnvVar("MH_MAX_REGISTERED_MEETINGS".to_string()))?;

        let policy_limits = PolicyLimits {
            max_egress_streams_per_meeting: bound_to_usize(
                "MH_MAX_EGRESS_STREAMS_PER_MEETING",
                parse_bounded(
                    "MH_MAX_EGRESS_STREAMS_PER_MEETING",
                    raw_max_egress_streams,
                    MAX_EGRESS_STREAMS_PER_MEETING_CEILING as u64,
                    "every registration carrying an egress stream would be rejected whole",
                    PRE_ALLOCATION_CONSEQUENCE,
                )?,
            )?,
            max_candidate_sources_per_egress: bound_to_usize(
                "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
                parse_bounded(
                    "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
                    raw_max_candidate_sources,
                    MAX_CANDIDATE_SOURCES_PER_EGRESS_CEILING as u64,
                    "every egress stream naming a source would be rejected",
                    PRE_ALLOCATION_CONSEQUENCE,
                )?,
            )?,
            max_total_egress_edges: bound_to_usize(
                "MH_MAX_TOTAL_EGRESS_EDGES",
                parse_bounded(
                    "MH_MAX_TOTAL_EGRESS_EDGES",
                    raw_max_total_edges,
                    MAX_TOTAL_EGRESS_EDGES_CEILING as u64,
                    "no registration carrying an egress edge could ever install",
                    "the aggregate bound would stop bounding the handler's routing memory",
                )?,
            )?,
            policy_apply_timeout_ms: parse_bounded(
                "MH_POLICY_APPLY_TIMEOUT_MS",
                raw_policy_apply_timeout,
                MAX_POLICY_APPLY_TIMEOUT_MS,
                "every policy apply would time out before the actor could answer",
                "above ADR-0036 §8's <=10 s re-assert cadence the await outlives the retry \
                 that would supersede it",
            )?,
            max_registered_meetings: bound_to_usize(
                "MH_MAX_REGISTERED_MEETINGS",
                parse_bounded(
                    "MH_MAX_REGISTERED_MEETINGS",
                    raw_max_registered_meetings,
                    MAX_REGISTERED_MEETINGS_CEILING as u64,
                    "every new meeting registration would be refused while the handler reports \
                     healthy",
                    "the guard would stop bounding the handler's registration map",
                )?,
            )?,
        };

        // Egress-budget admission chain (story 2 R-19, R-23) — four REQUIRED
        // keys; see the module-level "Egress-budget admission" section. Costs
        // refuse zero HERE, so the derivation's divisor is non-zero by
        // construction.
        let egress_budget_bps: u64 = parse_required_number(
            "MH_EGRESS_BUDGET_BPS",
            vars.get("MH_EGRESS_BUDGET_BPS")
                .ok_or_else(|| ConfigError::MissingEnvVar("MH_EGRESS_BUDGET_BPS".to_string()))?,
        )?;
        reject_zero(
            "MH_EGRESS_BUDGET_BPS",
            egress_budget_bps,
            "the derived stream ceiling would be 0 and MH would admit no stream at all",
        )?;
        let stream_cost_audio_bps: u64 = parse_required_number(
            "MH_STREAM_COST_AUDIO_BPS",
            vars.get("MH_STREAM_COST_AUDIO_BPS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_STREAM_COST_AUDIO_BPS".to_string())
            })?,
        )?;
        reject_zero(
            "MH_STREAM_COST_AUDIO_BPS",
            stream_cost_audio_bps,
            "a free stream cannot be budgeted — it is the divisor of the stream ceiling",
        )?;
        let stream_cost_video_bps: u64 = parse_required_number(
            "MH_STREAM_COST_VIDEO_BPS",
            vars.get("MH_STREAM_COST_VIDEO_BPS").ok_or_else(|| {
                ConfigError::MissingEnvVar("MH_STREAM_COST_VIDEO_BPS".to_string())
            })?,
        )?;
        reject_zero(
            "MH_STREAM_COST_VIDEO_BPS",
            stream_cost_video_bps,
            "a free stream cannot be budgeted — it is the divisor of the stream ceiling",
        )?;
        let rejection_ratio_threshold = parse_unit_ratio(
            "MH_EGRESS_REJECTION_RATIO_THRESHOLD",
            vars.get("MH_EGRESS_REJECTION_RATIO_THRESHOLD")
                .ok_or_else(|| {
                    ConfigError::MissingEnvVar("MH_EGRESS_REJECTION_RATIO_THRESHOLD".to_string())
                })?,
            "a threshold outside the ratio's own range can never be crossed (above 1.0) or is \
             always crossed (below 0.0), and a NaN compares false forever",
        )?;
        let egress_admission = EgressAdmission::derive(
            egress_budget_bps,
            stream_cost_audio_bps,
            stream_cost_video_bps,
            rejection_ratio_threshold,
            policy_limits.max_total_egress_edges,
        )?;

        // ADR-0036 §11 media latency sampling. Optional-with-default (a
        // reasoned carve-out; see the field doc) plus a hard range check: above
        // 1 silently means "observe everything" (a per-frame histogram write on
        // the hot path), below 0 silently means "observe nothing".
        let (media_latency_sample_ratio, media_latency_sample_ratio_source) =
            match vars.get("MH_MEDIA_LATENCY_SAMPLE_RATIO") {
                None => (DEFAULT_MEDIA_LATENCY_SAMPLE_RATIO, ValueSource::Default),
                Some(raw) => (
                    parse_unit_ratio(
                        "MH_MEDIA_LATENCY_SAMPLE_RATIO",
                        raw,
                        "above 1.0 observes every frame on the per-frame path and below 0.0 \
                         observes none, which reads as a healthy quiet path",
                    )?,
                    ValueSource::Env,
                ),
            };

        // R-55: OpenTelemetry SDK configuration.
        // Enablement is an explicit boolean (OTEL_ENABLED), NOT presence of the
        // endpoint — so a populated OTLP_ENDPOINT with OTEL_ENABLED unset/false
        // stays OFF (init_otel is never called and no layer is composed).
        let otel_enabled = if let Some(value_str) = vars.get("OTEL_ENABLED") {
            match value_str.trim().to_ascii_lowercase().as_str() {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(ConfigError::InvalidOtelConfig(format!(
                        "OTEL_ENABLED must be 'true' or 'false', got '{value_str}'"
                    )))
                }
            }
        } else {
            false
        };

        // Endpoint reuses the existing OTLP_ENDPOINT key (task #28 ruling).
        let otel_endpoint = vars.get("OTLP_ENDPOINT").cloned().unwrap_or_default();

        // Sample rate: PARSE-ONLY (team-lead FREEZE 2026-06-25, option X). The
        // `[0.0, 1.0]` bound has a SINGLE owner — `init_otel`
        // (common/observability/otel.rs) — which re-validates it at startup on
        // the enabled path (fail-hard at init). We do NOT duplicate the range
        // check here. Non-numeric still fails fast at load. An out-of-range
        // value is accepted at config load and only hard-fails at `init_otel`
        // when `otel_enabled=true` — intended.
        let otel_sample_rate = if let Some(value_str) = vars.get("OTEL_SAMPLE_RATE") {
            value_str.parse::<f64>().map_err(|e| {
                ConfigError::InvalidOtelConfig(format!(
                    "OTEL_SAMPLE_RATE must be a number, got '{value_str}': {e}"
                ))
            })?
        } else {
            DEFAULT_OTEL_SAMPLE_RATE
        };

        let environment = vars
            .get("DEPLOYMENT_ENVIRONMENT")
            .cloned()
            .unwrap_or_else(|| DEFAULT_DEPLOYMENT_ENVIRONMENT.to_string());

        // When OTel is enabled the endpoint is required: fail fast with a clear
        // message rather than letting init_otel reject an empty/unparseable URL.
        if otel_enabled && otel_endpoint.trim().is_empty() {
            return Err(ConfigError::InvalidOtelConfig(
                "OTEL_ENABLED=true requires a non-empty OTLP_ENDPOINT".to_string(),
            ));
        }

        // Generate MH instance ID
        let handler_id = vars.get("MH_HANDLER_ID").cloned().unwrap_or_else(|| {
            let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());
            let uuid_suffix = uuid::Uuid::new_v4().to_string();
            let short_suffix = uuid_suffix.get(..8).unwrap_or("00000000");
            format!("{DEFAULT_MH_ID_PREFIX}-{hostname}-{short_suffix}")
        });

        Ok(Config {
            grpc_bind_address,
            health_bind_address,
            webtransport_bind_address,
            region,
            gc_grpc_url,
            handler_id,
            ac_endpoint,
            client_id,
            client_secret,
            tls_cert_path,
            tls_key_path,
            grpc_advertise_address,
            webtransport_advertise_address,
            ac_jwks_url,
            register_meeting_timeout_seconds,
            max_connections,
            quic_transport,
            termination_grace_seconds,
            drain_window,
            drain_window_source,
            policy_limits,
            egress_admission,
            media_latency_sample_ratio,
            media_latency_sample_ratio_source,
            otel_enabled,
            otel_endpoint,
            otel_sample_rate,
            environment,
        })
    }

    /// Build the [`common::observability::otel::OtelConfig`] for `init_otel`,
    /// or `None` when `OTel` is disabled. Encodes the R-55 gating: `main` calls
    /// `init_otel` (and composes the layer) iff this returns `Some` — so the
    /// default-off path performs no collector probe and adds no `OTel` layer.
    #[must_use]
    pub fn otel_config(&self) -> Option<common::observability::otel::OtelConfig> {
        if self.otel_enabled {
            Some(common::observability::otel::OtelConfig {
                endpoint: self.otel_endpoint.clone(),
                sample_rate: self.otel_sample_rate,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    // OTel sample-rate assertions compare a parsed f64 against the exact
    // literal it was parsed from (or the exact DEFAULT_OTEL_SAMPLE_RATE
    // const) — not a computed/accumulated float where precision loss could
    // hide a bug, so exact equality is the correct check here.
    clippy::float_cmp
)]
mod tests {
    use super::*;
    use common::secret::ExposeSecret;

    fn base_vars() -> HashMap<String, String> {
        HashMap::from([
            (
                "AC_ENDPOINT".to_string(),
                "http://localhost:8082".to_string(),
            ),
            ("MH_CLIENT_ID".to_string(), "media-handler".to_string()),
            (
                "MH_CLIENT_SECRET".to_string(),
                "media-handler-secret-dev-003".to_string(),
            ),
            ("MH_TLS_CERT_PATH".to_string(), "/dev/null".to_string()),
            ("MH_TLS_KEY_PATH".to_string(), "/dev/null".to_string()),
            (
                "MH_GRPC_ADVERTISE_ADDRESS".to_string(),
                "grpc://localhost:50053".to_string(),
            ),
            (
                "MH_WEBTRANSPORT_ADVERTISE_ADDRESS".to_string(),
                "https://localhost:4434".to_string(),
            ),
            (
                "AC_JWKS_URL".to_string(),
                "http://localhost:8082/.well-known/jwks.json".to_string(),
            ),
            // ADR-0036 §1 transport parameters, all REQUIRED. Values are the
            // DEPLOYED ones from `infra/services/mh-service/configmap.yaml`, so
            // a manifest/code divergence shows up here rather than only in a
            // cluster: a test fixture that invented its own numbers would keep
            // passing while production CrashLooped.
            (
                "MH_MAX_CONCURRENT_UNI_STREAMS".to_string(),
                "64".to_string(),
            ),
            (
                "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
                "32".to_string(),
            ),
            ("MH_KEEPALIVE_INTERVAL_MS".to_string(), "10000".to_string()),
            ("MH_MAX_CONNECTIONS".to_string(), "500".to_string()),
            // Kustomize-derived from each pod's own
            // `terminationGracePeriodSeconds`; 35 is what both deployments set.
            ("MH_TERMINATION_GRACE_SECONDS".to_string(), "35".to_string()),
            // ADR-0036 §8 policy bounds, REQUIRED since story 2 R-22. Deployed
            // values from the base ConfigMap, for the same reason as above.
            (
                "MH_MAX_EGRESS_STREAMS_PER_MEETING".to_string(),
                "512".to_string(),
            ),
            (
                "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS".to_string(),
                "16".to_string(),
            ),
            ("MH_MAX_TOTAL_EGRESS_EDGES".to_string(), "65536".to_string()),
            ("MH_POLICY_APPLY_TIMEOUT_MS".to_string(), "1000".to_string()),
            // Registered-meeting cap (story 2 R-21), the deployed value.
            ("MH_MAX_REGISTERED_MEETINGS".to_string(), "8192".to_string()),
            // Egress-budget chain (story 2 R-19). The BASE ConfigMap's
            // deliberately unsized placeholder budget — it must boot (above the
            // refuse-boot floor) and nudge (below the recommended minimum).
            ("MH_EGRESS_BUDGET_BPS".to_string(), "10000000".to_string()),
            ("MH_STREAM_COST_AUDIO_BPS".to_string(), "90000".to_string()),
            (
                "MH_STREAM_COST_VIDEO_BPS".to_string(),
                "2500000".to_string(),
            ),
            (
                "MH_EGRESS_REJECTION_RATIO_THRESHOLD".to_string(),
                "0.05".to_string(),
            ),
        ])
    }

    /// Every key story 2 makes REQUIRED: task 8's four new egress-chain keys
    /// and four flipped §8 bounds, and task 11's registered-meeting cap. One
    /// list, iterated by both the absence and the malformed-value tests, so
    /// neither can cover a subset.
    const STORY2_REQUIRED_KEYS: [&str; 9] = [
        "MH_EGRESS_BUDGET_BPS",
        "MH_STREAM_COST_AUDIO_BPS",
        "MH_STREAM_COST_VIDEO_BPS",
        "MH_EGRESS_REJECTION_RATIO_THRESHOLD",
        "MH_MAX_EGRESS_STREAMS_PER_MEETING",
        "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
        "MH_MAX_TOTAL_EGRESS_EDGES",
        "MH_POLICY_APPLY_TIMEOUT_MS",
        "MH_MAX_REGISTERED_MEETINGS",
    ];

    /// Derive with the base costs and a budget chosen so the ceiling is
    /// exactly `ceiling` (budget = ceiling × the video cost).
    fn derive_ceiling(ceiling: u64) -> Result<EgressAdmission, ConfigError> {
        EgressAdmission::derive(ceiling * 2_500_000, 90_000, 2_500_000, 0.05, 65_536)
    }

    // =========================================================================
    // ADR-0036 §1: explicit transport parameters, required env vars, and the
    // derived shutdown drain.
    // =========================================================================

    /// Assert that removing `key` from an otherwise-valid environment produces
    /// a `MissingEnvVar` naming exactly that key.
    ///
    /// The var name is checked, not just the variant: a required var that
    /// reports the *wrong* name sends an operator to edit the wrong `ConfigMap`
    /// key, and in a `CrashLoop` this message is the only diagnostic they get
    /// (config load happens before the tracing subscriber exists).
    fn assert_required(key: &str) {
        let mut vars = base_vars();
        vars.remove(key);
        let result = Config::from_vars(&vars);
        match result {
            Err(ConfigError::MissingEnvVar(name)) => assert_eq!(name, key),
            other => panic!("{key} must be REQUIRED; got {other:?}"),
        }
    }

    #[test]
    fn missing_max_concurrent_uni_streams_is_a_loud_refusal() {
        assert_required("MH_MAX_CONCURRENT_UNI_STREAMS");
    }

    #[test]
    fn missing_datagram_buffer_audio_frames_is_a_loud_refusal() {
        assert_required("MH_DATAGRAM_BUFFER_AUDIO_FRAMES");
    }

    #[test]
    fn missing_keepalive_interval_is_a_loud_refusal() {
        assert_required("MH_KEEPALIVE_INTERVAL_MS");
    }

    #[test]
    fn missing_termination_grace_is_a_loud_refusal() {
        assert_required("MH_TERMINATION_GRACE_SECONDS");
    }

    #[test]
    fn missing_max_connections_is_a_loud_refusal() {
        // Regression pin for the R-22 defect: this key was previously absent
        // from every manifest AND silently defaulted to 10,000 in code, so
        // nothing anywhere reported that the live bound was chosen by nobody.
        assert_required("MH_MAX_CONNECTIONS");
    }

    #[test]
    fn a_malformed_value_is_rejected_never_silently_defaulted() {
        // The `unwrap_or(DEFAULT_MAX_CONNECTIONS)` idiom this change deleted
        // swallowed exactly this input. A bound that quietly reverts is a bound
        // nobody can rely on.
        for key in [
            "MH_MAX_CONCURRENT_UNI_STREAMS",
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES",
            "MH_KEEPALIVE_INTERVAL_MS",
            "MH_TERMINATION_GRACE_SECONDS",
            "MH_MAX_CONNECTIONS",
        ] {
            let mut vars = base_vars();
            vars.insert(key.to_string(), "not-a-number".to_string());
            match Config::from_vars(&vars) {
                Err(ConfigError::InvalidValue(msg)) => {
                    assert!(msg.contains(key), "message must name the var: {msg}");
                    // Error Context Preservation: the offending text survives,
                    // so an operator sees WHAT was wrong, not merely that
                    // something was.
                    assert!(
                        msg.contains("not-a-number"),
                        "message must carry the offending value: {msg}"
                    );
                }
                other => panic!("{key}='not-a-number' must be rejected; got {other:?}"),
            }
        }
    }

    // ---- frames -> bytes -----------------------------------------------------

    #[test]
    fn nominal_audio_frame_bytes_is_the_component_sum_not_a_literal() {
        // Deliberately re-derived from `media-protocol`'s exported components
        // rather than compared against 236. A frame-v2 layout change must break
        // THIS test (and the compile-time header-region pin), not silently
        // resize a live buffer.
        let expected = NOMINAL_OPUS_PAYLOAD_BYTES
            + media_protocol::frame::SFRAME_OBJECT_OVERHEAD_BYTES
            + media_protocol::frame::PUBLISHER_FIXED_PREFIX_SIZE
            + media_protocol::frame::WRAPPED_TRANSMIT_KEY_SIZE
            + media_protocol::frame::EXT_LENGTH_FIELD_SIZE
            + media_protocol::frame::RELAY_REGION_SIZE
            + media_protocol::frame::SIGNATURE_SIZE;
        assert_eq!(NOMINAL_AUDIO_FRAME_BYTES, expected);

        // ...and it is emphatically NOT the §2 ingress DoS ceiling. Two
        // constants, two jobs: conflating them would give a 32 MiB
        // per-connection send buffer.
        const {
            assert!(NOMINAL_AUDIO_FRAME_BYTES < media_protocol::frame::MAX_PAYLOAD_BYTES / 1000);
        }
    }

    #[test]
    fn frames_convert_to_bytes_once_at_load() {
        let mut vars = base_vars();
        vars.insert(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
            "32".to_string(),
        );
        let config = Config::from_vars(&vars).expect("config should load");

        assert_eq!(config.quic_transport.datagram_send_buffer_audio_frames, 32);
        assert_eq!(
            config.quic_transport.datagram_send_buffer_bytes,
            32 * NOMINAL_AUDIO_FRAME_BYTES
        );
        // Both links survive to the log line, and the ms figure is the one
        // ADR-0036 §1 reasons with.
        assert_eq!(
            config.quic_transport.datagram_send_buffer_ms(),
            32 * AUDIO_FRAME_DURATION_MS
        );
    }

    #[test]
    fn the_buffer_holds_at_most_its_budget_across_the_whole_bitrate_range() {
        // THE assertion that would have caught the mistake this change actually
        // made in review: sizing the constant at the bitrate range's CEILING
        // rather than its floor.
        //
        // Byte-level checks are blind to it — 32 x 236 = 7,552 and 32 x 276 =
        // 8,832 are both plausible buffer sizes. What separates them is HELD
        // FRAMES at the smallest real frame, because `datagram_send_buffer_size`
        // is a latency ceiling, not a capacity guarantee (ADR-0036 §1: "prefer
        // loss to unbounded latency"). Held frames = buffer / actual frame, so
        // the worst case is the SMALLEST frame, i.e. the LOWEST bitrate.
        let frames = 32usize;
        let buffer_bytes = frames * NOMINAL_AUDIO_FRAME_BYTES;

        let frame_bytes_at = |bps: usize| {
            bps * AUDIO_FRAME_DURATION_MS / (8 * 1000)
                + (NOMINAL_AUDIO_FRAME_BYTES - NOMINAL_OPUS_PAYLOAD_BYTES)
        };

        for bps in [
            SUPPORTED_AUDIO_BITRATE_FLOOR_BPS,
            SUPPORTED_AUDIO_BITRATE_CEILING_BPS,
        ] {
            let held = buffer_bytes / frame_bytes_at(bps);
            assert!(
                held <= frames,
                "at {bps} bps the buffer holds {held} frames, over the {frames}-frame budget: \
                 NOMINAL_AUDIO_FRAME_BYTES has been sized at the bitrate CEILING instead of the \
                 FLOOR"
            );
        }

        // And the budget is exactly met at the floor — so it is a real bound
        // rather than one satisfied by being pessimistic everywhere.
        assert_eq!(
            buffer_bytes / frame_bytes_at(SUPPORTED_AUDIO_BITRATE_FLOOR_BPS),
            frames
        );
    }

    // ---- hard ceilings (SEC-1) -----------------------------------------------

    #[test]
    fn datagram_buffer_frames_has_a_derived_upper_bound() {
        // A `> 0` check alone would accept every value below. The three that
        // matter, none of which the egress-queue validation alone would catch:
        //   3200  -> ~377 MB across 500 connections, BOOTS CLEAN
        //   32000 -> ~3.8 GB, OOMKill (SIGKILL, which voids the drain window)
        //   >89 s of queued audio -> strictly WORSE than the quinn default this
        //                            whole task exists to replace
        let mut vars = base_vars();
        for absurd in ["3200", "32000", "4444"] {
            vars.insert(
                "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
                absurd.to_string(),
            );
            match Config::from_vars(&vars) {
                Err(ConfigError::InvalidValue(msg)) => {
                    assert!(msg.contains("MH_DATAGRAM_BUFFER_AUDIO_FRAMES"), "{msg}");
                    // The refusal must say what LATENCY it protects, not just
                    // quote a frame count nobody can evaluate.
                    assert!(msg.contains("ms of audio"), "must name the latency: {msg}");
                }
                other => panic!("{absurd} frames must be refused; got {other:?}"),
            }
        }
    }

    #[test]
    fn the_datagram_buffer_ceiling_is_derived_from_a_latency_not_typed() {
        assert_eq!(
            MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING,
            MAX_DATAGRAM_BUFFER_LATENCY_MS / AUDIO_FRAME_DURATION_MS
        );
        // Boundary: at the ceiling accepted, one above refused.
        let mut vars = base_vars();
        vars.insert(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
            MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING.to_string(),
        );
        assert!(
            Config::from_vars(&vars).is_ok(),
            "the ceiling itself must load"
        );

        vars.insert(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
            (MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING + 1).to_string(),
        );
        assert!(matches!(
            Config::from_vars(&vars),
            Err(ConfigError::InvalidValue(_))
        ));
    }

    #[test]
    fn the_ceiling_forbids_configuring_something_worse_than_the_quinn_default() {
        // quinn's unchosen 1 MiB default is ~4,443 frames (~89 s of queued
        // audio). The ceiling must sit well below it, or MH could be configured
        // into a state strictly worse than the default this task replaces.
        let quinn_default_frames = 1_048_576 / NOMINAL_AUDIO_FRAME_BYTES;
        assert!(
            MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING < quinn_default_frames,
            "ceiling {MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING} must be below quinn's \
             {quinn_default_frames}-frame default"
        );
    }

    #[test]
    fn uni_streams_may_not_exceed_quinns_own_default() {
        // Above quinn's default the declaration stops tightening anything —
        // MH would be loosening a limit it declared in order to be tighter.
        assert_eq!(
            MAX_CONCURRENT_UNI_STREAMS_CEILING,
            QUINN_DEFAULT_MAX_CONCURRENT_UNI_STREAMS
        );
        let mut vars = base_vars();

        vars.insert(
            "MH_MAX_CONCURRENT_UNI_STREAMS".to_string(),
            MAX_CONCURRENT_UNI_STREAMS_CEILING.to_string(),
        );
        assert!(
            Config::from_vars(&vars).is_ok(),
            "equal to the default is still a CHOICE and must be allowed"
        );

        vars.insert(
            "MH_MAX_CONCURRENT_UNI_STREAMS".to_string(),
            (MAX_CONCURRENT_UNI_STREAMS_CEILING + 1).to_string(),
        );
        match Config::from_vars(&vars) {
            Err(ConfigError::InvalidValue(msg)) => {
                assert!(msg.contains("MH_MAX_CONCURRENT_UNI_STREAMS"), "{msg}");
                assert!(
                    msg.contains("configmap.yaml"),
                    "must name where to fix it: {msg}"
                );
            }
            other => panic!("above quinn's default must be refused; got {other:?}"),
        }
    }

    #[test]
    fn the_deployed_values_sit_inside_every_ceiling() {
        // Guards against a ceiling that would CrashLoop production the moment
        // it landed — the failure mode a hard bound can introduce by itself.
        let config = Config::from_vars(&base_vars()).expect("deployed values must load");
        assert!(
            config.quic_transport.datagram_send_buffer_audio_frames
                <= MAX_DATAGRAM_BUFFER_AUDIO_FRAMES_CEILING
        );
        assert!(
            config.quic_transport.max_concurrent_uni_streams <= MAX_CONCURRENT_UNI_STREAMS_CEILING
        );
    }

    #[test]
    fn all_nine_numeric_vars_explain_a_malformed_value_the_same_way() {
        // F3 regression pin: `parse_bounded` delegates to
        // `parse_required_number`, so required and optional vars cannot drift
        // into explaining the same rejection two different ways — and the §8
        // bounds no longer DISCARD the underlying parse error.
        let mut vars = base_vars();
        vars.insert(
            "MH_MAX_EGRESS_STREAMS_PER_MEETING".to_string(),
            "not-a-number".to_string(),
        );
        match Config::from_vars(&vars) {
            Err(ConfigError::InvalidValue(msg)) => {
                assert!(msg.contains("MH_MAX_EGRESS_STREAMS_PER_MEETING"), "{msg}");
                assert!(msg.contains("not-a-number"), "{msg}");
                assert!(
                    msg.contains("invalid digit"),
                    "the underlying parse error must survive, as it does for required vars: {msg}"
                );
            }
            other => panic!("malformed §8 bound must be rejected; got {other:?}"),
        }
    }

    // ---- startup validations -------------------------------------------------

    #[test]
    fn egress_queue_must_bind_before_the_transport_ceiling() {
        // Boundary on both sides: equal is a refusal (the app queue would trip
        // no earlier than quinn's), one more is accepted.
        let mut vars = base_vars();
        vars.insert(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
            EGRESS_QUEUE_FRAMES.to_string(),
        );
        match Config::from_vars(&vars) {
            Err(ConfigError::EgressQueueDoesNotBindFirst {
                egress_queue_frames,
                datagram_buffer_audio_frames,
            }) => {
                assert_eq!(egress_queue_frames, EGRESS_QUEUE_FRAMES);
                assert_eq!(datagram_buffer_audio_frames, EGRESS_QUEUE_FRAMES);
            }
            other => panic!("equal frames must be refused; got {other:?}"),
        }

        vars.insert(
            "MH_DATAGRAM_BUFFER_AUDIO_FRAMES".to_string(),
            (EGRESS_QUEUE_FRAMES + 1).to_string(),
        );
        assert!(
            Config::from_vars(&vars).is_ok(),
            "one frame above must load"
        );
    }

    #[test]
    fn the_zero_termination_grace_sentinel_refuses_to_start() {
        // `"0"` is what `mh-{0,1}-deployment.yaml` ships as a literal; the
        // kustomize `replacements:` block overwrites it at build time. A pod
        // that actually reads 0 was deployed with `apply -f` instead of
        // `apply -k`, and must refuse rather than drain for an unchosen window.
        let mut vars = base_vars();
        vars.insert("MH_TERMINATION_GRACE_SECONDS".to_string(), "0".to_string());
        match Config::from_vars(&vars) {
            Err(ConfigError::TerminationGraceTooSmall {
                grace_seconds,
                margin_seconds,
            }) => {
                assert_eq!(grace_seconds, 0);
                assert_eq!(margin_seconds, SHUTDOWN_MARGIN_SECONDS);
            }
            other => panic!("the 0 sentinel must be refused; got {other:?}"),
        }
    }

    #[test]
    fn termination_grace_must_exceed_the_shutdown_margin() {
        let mut vars = base_vars();

        // Equal is a refusal: no room at all for teardown before SIGKILL.
        vars.insert(
            "MH_TERMINATION_GRACE_SECONDS".to_string(),
            SHUTDOWN_MARGIN_SECONDS.to_string(),
        );
        assert!(matches!(
            Config::from_vars(&vars),
            Err(ConfigError::TerminationGraceTooSmall { .. })
        ));

        // One second more is accepted, and NOT the stricter
        // `grace >= MARGIN + SETTLE`: the invariant is `drain + MARGIN <= grace`,
        // which holds here (1 + 5 = 6). Refusing this would reject a safe
        // configuration.
        vars.insert(
            "MH_TERMINATION_GRACE_SECONDS".to_string(),
            (SHUTDOWN_MARGIN_SECONDS + 1).to_string(),
        );
        assert!(Config::from_vars(&vars).is_ok());
    }

    #[test]
    fn a_zero_keepalive_is_refused_because_zero_means_disabled() {
        // In QUIC a zero/absent keepalive means DISABLED, so this would
        // silently stop refreshing the NAT binding of every muted participant
        // and read as configured-on-purpose forever (ADR-0036 §1, §5).
        let mut vars = base_vars();
        vars.insert("MH_KEEPALIVE_INTERVAL_MS".to_string(), "0".to_string());
        match Config::from_vars(&vars) {
            Err(ConfigError::InvalidValue(msg)) => {
                assert!(msg.contains("MH_KEEPALIVE_INTERVAL_MS"), "{msg}");
                assert!(msg.contains("DISABLED"), "must say what zero DOES: {msg}");
            }
            other => panic!("zero keepalive must be refused; got {other:?}"),
        }
    }

    #[test]
    fn keepalive_must_leave_room_for_a_lost_packet_before_the_idle_timeout() {
        let idle_ms = MAX_IDLE_TIMEOUT_SECONDS * 1_000;
        let mut vars = base_vars();

        // 1:3 — three keepalives fit inside one idle timeout, so a single loss
        // still leaves two.
        vars.insert(
            "MH_KEEPALIVE_INTERVAL_MS".to_string(),
            (idle_ms / MIN_KEEPALIVE_TO_IDLE_RATIO).to_string(),
        );
        assert!(Config::from_vars(&vars).is_ok(), "1:3 must be accepted");

        // 1:2 — a single lost keepalive sits at the edge of the timeout.
        vars.insert(
            "MH_KEEPALIVE_INTERVAL_MS".to_string(),
            (idle_ms / 2).to_string(),
        );
        match Config::from_vars(&vars) {
            Err(ConfigError::KeepaliveTooLarge {
                keepalive_ms,
                idle_timeout_ms,
                min_ratio,
                max_allowed_ms,
            }) => {
                assert_eq!(keepalive_ms, idle_ms / 2);
                assert_eq!(idle_timeout_ms, idle_ms);
                assert_eq!(min_ratio, MIN_KEEPALIVE_TO_IDLE_RATIO);
                // The message must tell the operator the number to type, not
                // just that the one they typed was wrong.
                assert_eq!(max_allowed_ms, idle_ms / MIN_KEEPALIVE_TO_IDLE_RATIO);
            }
            other => panic!("1:2 must be refused; got {other:?}"),
        }
    }

    #[test]
    fn the_deployed_keepalive_satisfies_the_ratio_the_configmap_asserts() {
        // `configmap.yaml` states 10000 ms is a chosen 1:3 against a 30 s idle
        // timeout. Pinned here so the ConfigMap's claim is checked rather than
        // narrated.
        let config = Config::from_vars(&base_vars()).expect("deployed values must load");
        assert_eq!(config.quic_transport.keepalive_interval_ms, 10_000);
        let product = config.quic_transport.keepalive_interval_ms * MIN_KEEPALIVE_TO_IDLE_RATIO;
        let idle_ms = MAX_IDLE_TIMEOUT_SECONDS * 1_000;
        assert!(
            product <= idle_ms,
            "ZERO-HEADROOM BOUNDARY CROSSED, and this test is the thing that caught it instead of \
             a production CrashLoop. The deployed MH_KEEPALIVE_INTERVAL_MS ({}ms) sits EXACTLY at \
             max_idle_timeout / ratio, so this assertion has no slack by design: {}ms x {} = {}ms \
             against a {}ms idle timeout. A failure means a CONSTANT moved the boundary under the \
             deployed value — either MIN_KEEPALIVE_TO_IDLE_RATIO went up or MAX_IDLE_TIMEOUT_SECONDS \
             went down. Both pods would refuse to start. Fix by lowering \
             MH_KEEPALIVE_INTERVAL_MS in infra/services/mh-service/configmap.yaml in the SAME \
             change; do not relax the ratio to make this pass.",
            config.quic_transport.keepalive_interval_ms,
            config.quic_transport.keepalive_interval_ms,
            MIN_KEEPALIVE_TO_IDLE_RATIO,
            product,
            idle_ms,
        );
    }

    // ---- derived drain window ------------------------------------------------

    #[test]
    fn the_deployed_grace_yields_the_settle_target() {
        // grace 35 -> min(2, 30) = 2s. Behaviour is IDENTICAL to the hardcoded
        // sleep this replaced, which is why ADR-0036 §11's "sleeps two seconds"
        // remains true and needs no amendment: only the provenance changed.
        let config = Config::from_vars(&base_vars()).expect("config should load");
        assert_eq!(config.termination_grace_seconds, 35);
        assert_eq!(
            config.drain_window,
            std::time::Duration::from_secs(SHUTDOWN_SETTLE_TARGET_SECONDS)
        );
        assert_eq!(config.drain_window_source, DrainWindowSource::SettleTarget);
    }

    #[test]
    fn a_small_grace_clamps_the_drain_and_says_so() {
        // The ONLY grace at which the `grace - MARGIN` arm binds, given
        // SETTLE=2 and MARGIN=5. Near-vacuous in practice, which is exactly why
        // `drain_window_source` exists: it is how an operator tells "2s because
        // settle target" from "1s because someone set grace to 6".
        let mut vars = base_vars();
        vars.insert("MH_TERMINATION_GRACE_SECONDS".to_string(), "6".to_string());
        let config = Config::from_vars(&vars).expect("grace 6 is valid");
        assert_eq!(config.drain_window, std::time::Duration::from_secs(1));
        assert_eq!(
            config.drain_window_source,
            DrainWindowSource::GraceMinusMargin
        );
    }

    #[test]
    fn the_drain_always_fits_inside_the_grace_with_the_margin_intact() {
        // The invariant the validation actually protects, checked across the
        // whole valid range rather than at the two points above.
        for grace in (SHUTDOWN_MARGIN_SECONDS + 1)..=120 {
            let mut vars = base_vars();
            vars.insert(
                "MH_TERMINATION_GRACE_SECONDS".to_string(),
                grace.to_string(),
            );
            let config = Config::from_vars(&vars).expect("valid grace must load");
            assert!(
                config.drain_window.as_secs() + SHUTDOWN_MARGIN_SECONDS <= grace,
                "drain {}s + margin {SHUTDOWN_MARGIN_SECONDS}s must fit inside grace {grace}s",
                config.drain_window.as_secs()
            );
            // And it never silently becomes a drain PHASE (ADR-0036 §11).
            assert!(config.drain_window.as_secs() <= SHUTDOWN_SETTLE_TARGET_SECONDS);
        }
    }

    #[test]
    fn drain_window_source_renders_a_bounded_vocabulary() {
        assert_eq!(DrainWindowSource::SettleTarget.as_str(), "settle_target");
        assert_eq!(
            DrainWindowSource::GraceMinusMargin.as_str(),
            "grace_minus_margin"
        );
    }

    // ---- negotiated wire limit ----------------------------------------------

    #[test]
    fn the_advertised_max_datagram_frame_size_clears_a_real_audio_frame() {
        let config = Config::from_vars(&base_vars()).expect("config should load");
        // quinn advertises min(receive buffer, u16::MAX).
        assert_eq!(
            config.quic_transport.max_datagram_frame_size_bytes(),
            DATAGRAM_RECEIVE_BUFFER_BYTES
        );
        // The compile-time pin already guarantees the floor; this asserts the
        // consequence an operator cares about, in the direction they think in.
        assert!(config.quic_transport.max_datagram_frame_size_bytes() > NOMINAL_AUDIO_FRAME_BYTES);
    }

    #[test]
    fn test_from_vars_success_with_defaults() {
        let vars = base_vars();

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.grpc_bind_address, DEFAULT_GRPC_BIND_ADDRESS);
        assert_eq!(config.health_bind_address, DEFAULT_HEALTH_BIND_ADDRESS);
        assert_eq!(
            config.webtransport_bind_address,
            DEFAULT_WEBTRANSPORT_BIND_ADDRESS
        );
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.gc_grpc_url, "http://localhost:50051");
        assert!(config.handler_id.starts_with("mh-"));
        assert_eq!(config.ac_endpoint, "http://localhost:8082");
        assert_eq!(config.client_id, "media-handler");
        assert_eq!(
            config.client_secret.expose_secret(),
            "media-handler-secret-dev-003"
        );
        assert_eq!(config.grpc_advertise_address, "grpc://localhost:50053");
        assert_eq!(
            config.webtransport_advertise_address,
            "https://localhost:4434"
        );
        assert_eq!(
            config.ac_jwks_url,
            "http://localhost:8082/.well-known/jwks.json"
        );
        assert_eq!(
            config.register_meeting_timeout_seconds,
            DEFAULT_REGISTER_MEETING_TIMEOUT_SECONDS
        );
        // `max_connections` is REQUIRED and has no default to assert; its
        // required-ness is covered by `missing_max_connections_is_a_loud_refusal`.
        assert_eq!(config.max_connections, 500);
    }

    #[test]
    fn test_from_vars_success_with_custom_values() {
        let mut vars = base_vars();
        vars.insert(
            "MH_GRPC_BIND_ADDRESS".to_string(),
            "127.0.0.1:50054".to_string(),
        );
        vars.insert(
            "MH_HEALTH_BIND_ADDRESS".to_string(),
            "127.0.0.1:8084".to_string(),
        );
        vars.insert(
            "MH_WEBTRANSPORT_BIND_ADDRESS".to_string(),
            "127.0.0.1:4435".to_string(),
        );
        vars.insert("MH_REGION".to_string(), "eu-west-1".to_string());
        vars.insert("GC_GRPC_URL".to_string(), "http://gc:50051".to_string());
        // The retired key is IGNORED, never read: setting it changes nothing.
        vars.insert("MH_MAX_STREAMS".to_string(), "500".to_string());
        vars.insert("MH_HANDLER_ID".to_string(), "mh-custom-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.grpc_bind_address, "127.0.0.1:50054");
        assert_eq!(config.health_bind_address, "127.0.0.1:8084");
        assert_eq!(config.webtransport_bind_address, "127.0.0.1:4435");
        assert_eq!(config.region, "eu-west-1");
        assert_eq!(config.gc_grpc_url, "http://gc:50051");
        assert_eq!(
            config.egress_admission.stream_ceiling, 4,
            "MH_MAX_STREAMS is retired: the advertised ceiling is derived from the budget"
        );
        assert_eq!(config.handler_id, "mh-custom-001");
    }

    #[test]
    fn test_handler_id_custom_value() {
        let mut vars = base_vars();
        vars.insert("MH_HANDLER_ID".to_string(), "mh-us-east-1-001".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.handler_id, "mh-us-east-1-001");
    }

    #[test]
    fn test_from_vars_missing_ac_endpoint() {
        let mut vars = base_vars();
        vars.remove("AC_ENDPOINT");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "AC_ENDPOINT"));
    }

    #[test]
    fn test_from_vars_missing_client_id() {
        let mut vars = base_vars();
        vars.remove("MH_CLIENT_ID");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_CLIENT_ID"));
    }

    #[test]
    fn test_from_vars_missing_client_secret() {
        let mut vars = base_vars();
        vars.remove("MH_CLIENT_SECRET");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_CLIENT_SECRET"));
    }

    #[test]
    fn test_from_vars_missing_tls_cert_path() {
        let mut vars = base_vars();
        vars.remove("MH_TLS_CERT_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_TLS_CERT_PATH"));
    }

    #[test]
    fn test_from_vars_missing_tls_key_path() {
        let mut vars = base_vars();
        vars.remove("MH_TLS_KEY_PATH");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_TLS_KEY_PATH"));
    }

    #[test]
    fn test_from_vars_tls_cert_path_nonexistent() {
        let mut vars = base_vars();
        vars.insert(
            "MH_TLS_CERT_PATH".to_string(),
            "/nonexistent/cert.pem".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("does not exist"))
        );
    }

    #[test]
    fn test_from_vars_tls_key_path_nonexistent() {
        let mut vars = base_vars();
        vars.insert(
            "MH_TLS_KEY_PATH".to_string(),
            "/nonexistent/key.pem".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::InvalidValue(msg)) if msg.contains("does not exist"))
        );
    }

    #[test]
    fn test_debug_redacts_sensitive_fields() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        let debug_output = format!("{config:?}");

        // Sensitive fields should be redacted
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("media-handler-secret-dev-003"));
        // Non-sensitive fields should be visible
        assert!(debug_output.contains("media-handler"));
        assert!(debug_output.contains("http://localhost:8082"));
    }

    #[test]
    fn test_oauth_config_loaded_correctly() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.ac_endpoint, "http://localhost:8082");
        assert_eq!(config.client_id, "media-handler");
        assert_eq!(
            config.client_secret.expose_secret(),
            "media-handler-secret-dev-003"
        );
    }

    #[test]
    fn test_tls_config_loaded_correctly() {
        let vars = base_vars();
        let config = Config::from_vars(&vars).expect("Config should load successfully");

        assert_eq!(config.tls_cert_path, "/dev/null");
        assert_eq!(config.tls_key_path, "/dev/null");
    }

    #[test]
    fn test_from_vars_missing_grpc_advertise_address() {
        let mut vars = base_vars();
        vars.remove("MH_GRPC_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_GRPC_ADVERTISE_ADDRESS")
        );
    }

    #[test]
    fn test_from_vars_missing_webtransport_advertise_address() {
        let mut vars = base_vars();
        vars.remove("MH_WEBTRANSPORT_ADVERTISE_ADDRESS");

        let result = Config::from_vars(&vars);
        assert!(
            matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "MH_WEBTRANSPORT_ADVERTISE_ADDRESS")
        );
    }

    #[test]
    fn test_from_vars_missing_ac_jwks_url() {
        let mut vars = base_vars();
        vars.remove("AC_JWKS_URL");

        let result = Config::from_vars(&vars);
        assert!(matches!(result, Err(ConfigError::MissingEnvVar(v)) if v == "AC_JWKS_URL"));
    }

    #[test]
    fn test_from_vars_invalid_ac_jwks_url_scheme() {
        let mut vars = base_vars();
        vars.insert(
            "AC_JWKS_URL".to_string(),
            "ftp://localhost:8082/.well-known/jwks.json".to_string(),
        );

        let result = Config::from_vars(&vars);
        assert!(
            matches!(&result, Err(ConfigError::InvalidValue(msg)) if msg.contains("http://") && msg.contains("https://")),
            "Expected InvalidValue error for non-http scheme, got {result:?}"
        );
    }

    #[test]
    fn test_register_meeting_timeout_custom_value() {
        let mut vars = base_vars();
        vars.insert(
            "MH_REGISTER_MEETING_TIMEOUT_SECONDS".to_string(),
            "30".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.register_meeting_timeout_seconds, 30);
    }

    #[test]
    fn test_register_meeting_timeout_clamped_to_max() {
        let mut vars = base_vars();
        vars.insert(
            "MH_REGISTER_MEETING_TIMEOUT_SECONDS".to_string(),
            "999999".to_string(),
        );

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(
            config.register_meeting_timeout_seconds,
            MAX_REGISTER_MEETING_TIMEOUT_SECONDS,
        );
    }

    // -- ADR-0036 §8 policy bounds ------------------------------------------

    #[test]
    fn test_policy_limits_are_read_from_the_environment() {
        let config = Config::from_vars(&base_vars()).unwrap();
        assert_eq!(config.policy_limits.max_egress_streams_per_meeting, 512);
        assert_eq!(config.policy_limits.max_candidate_sources_per_egress, 16);
        assert_eq!(config.policy_limits.max_total_egress_edges, 65_536);
        assert_eq!(config.policy_limits.policy_apply_timeout_ms, 1_000);
    }

    // -- Story 2 task 8: every flipped/new key is REQUIRED ---------------------

    /// **The guard that watches the guard.** `dt-guard env-config` discovers
    /// MH's required variables by matching, in THIS FILE, a `MissingEnvVar`
    /// construction whose opening paren and upper-case string-literal argument
    /// are CONTIGUOUS (`crates/dt-guard/src/env_config.rs`). That construction
    /// is deliberately NOT written out here — the guard reads comments too, so
    /// an illustrative one would be reported as a required variable no manifest
    /// declares (the corollary recorded on `parse_required_number`, and yes,
    /// this test tripped it once). It therefore cannot see a call
    /// site rustfmt has wrapped between the two — and when it cannot see one it
    /// reports `STATUS=OK` regardless, a clean verdict over surface it silently
    /// stopped checking.
    ///
    /// That is not hypothetical: nesting these calls pushed two of story 2's
    /// keys past rustfmt's width, the formatter wrapped them, and both keys
    /// dropped out of the guard's view with no failure anywhere. This test
    /// replicates the guard's contiguity requirement over this file's own
    /// source, so a re-wrap fails HERE instead of going quiet.
    ///
    /// **Do not satisfy a failure by removing a key from
    /// [`STORY2_REQUIRED_KEYS`].** The fix is to keep the offending
    /// `MissingEnvVar(` and its string literal on the SAME LINE — bind the raw
    /// value at statement level rather than nesting it, as `from_vars` does.
    /// The surrounding `.ok_or_else(...)` chain may wrap freely.
    #[test]
    fn the_env_config_guard_can_see_every_required_key_in_this_file() {
        // This file's own source, read at COMPILE time — the same bytes the
        // guard reads from disk, so the test cannot drift from the artifact.
        const SOURCE: &str = include_str!("config.rs");
        // The guard's shape, replicated literally rather than by regex (no
        // regex dependency in this crate, and the property IS contiguity).
        let discovered: Vec<&str> = SOURCE
            .match_indices("MissingEnvVar(\"")
            .filter_map(|(at, m)| {
                let rest = SOURCE.get(at + m.len()..)?;
                let end = rest.find('"')?;
                let key = rest.get(..end)?;
                key.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    .then_some(key)
            })
            .collect();

        // Positive control: a wholesale-empty scan must not pass vacuously.
        assert!(
            discovered.len() >= STORY2_REQUIRED_KEYS.len(),
            "the scan found only {} MissingEnvVar literals in this file, fewer than the \
             {} story-2 keys alone — the scan itself is broken, not the call sites",
            discovered.len(),
            STORY2_REQUIRED_KEYS.len()
        );

        for key in STORY2_REQUIRED_KEYS {
            assert!(
                discovered.contains(&key),
                "`dt-guard env-config` CANNOT SEE {key}: no contiguous MissingEnvVar \
                 construction naming {key:?} appears in crates/mh-service/src/config.rs. \
                 Almost certainly rustfmt wrapped that call site between the paren and the \
                 literal because it was nested too deeply. The guard would still print \
                 STATUS=OK while checking nothing for this key — no manifest coverage, no \
                 orphan detection. FIX: bind the raw value at statement level so the \
                 construction and its literal sit on the SAME LINE (the surrounding \
                 `.ok_or_else(...)` chain may wrap freely). DO NOT fix this \
                 by deleting {key} from STORY2_REQUIRED_KEYS."
            );
        }
    }

    #[test]
    fn every_story2_key_is_a_loud_refusal_when_absent() {
        for key in STORY2_REQUIRED_KEYS {
            assert_required(key);
        }
    }

    #[test]
    fn every_story2_key_refuses_a_malformed_value_never_defaulting() {
        for key in STORY2_REQUIRED_KEYS {
            let mut vars = base_vars();
            vars.insert(key.to_string(), "not-a-number".to_string());
            match Config::from_vars(&vars) {
                Err(ConfigError::InvalidValue(msg)) => {
                    assert!(msg.contains(key), "{key}: {msg}");
                    assert!(msg.contains("not-a-number"), "{key}: {msg}");
                }
                other => panic!("{key}: malformed value must refuse to start; got {other:?}"),
            }
        }
    }

    // -- Story 2 task 8: egress-budget derivation ------------------------------

    #[test]
    fn the_ceiling_is_budget_over_the_worst_case_cost_in_bytes() {
        let a = EgressAdmission::derive(100_000_000, 90_000, 2_500_000, 0.05, 65_536).unwrap();
        // Bits converted ONCE: every downstream field is bytes.
        assert_eq!(a.budget_bytes_per_second, 12_500_000);
        assert_eq!(a.stream_cost_audio_bytes_per_second, 11_250);
        assert_eq!(a.stream_cost_video_bytes_per_second, 312_500);
        // max(cost) = video: MH stays type-blind (ADR-0036 §7).
        assert_eq!(a.stream_ceiling, 40);
        // The raw bits survive only for the provenance log.
        assert_eq!(a.budget_bps, 100_000_000);
    }

    #[test]
    fn the_ceiling_uses_the_larger_cost_whichever_kind_it_is() {
        let a = EgressAdmission::derive(80_000_000, 4_000_000, 1_000_000, 0.05, 65_536).unwrap();
        assert_eq!(a.stream_ceiling, 20, "audio is the larger cost here");
    }

    #[test]
    fn bits_convert_once_and_both_roundings_fail_closed() {
        // 7 bits of budget remainder are FLOORED away; a 1-bit cost remainder
        // is CEILED up. Both directions admit fewer streams, never more.
        let a = EgressAdmission::derive(8 * 1_000 + 7, 9, 9, 0.05, 65_536).unwrap();
        assert_eq!(a.budget_bytes_per_second, 1_000);
        assert_eq!(a.stream_cost_video_bytes_per_second, 2);
        assert_eq!(a.stream_ceiling, 500);
    }

    /// The refuse-boot floor is 2, not 1 — tested at 0, 1 AND 2. A test written
    /// as "at least 1" passes on a build that boots at ceiling 1, which is
    /// exactly the black-hole-reporting-healthy shape the floor exists for.
    #[test]
    fn a_ceiling_of_zero_or_one_refuses_to_boot_and_two_boots() {
        for ceiling in [0, 1] {
            match derive_ceiling(ceiling) {
                Err(ConfigError::EgressStreamCeilingBelowFloor {
                    ceiling: got,
                    floor,
                    ..
                }) => {
                    assert_eq!(got, ceiling);
                    assert_eq!(floor, 2);
                }
                other => panic!("ceiling {ceiling} must refuse to boot; got {other:?}"),
            }
        }
        assert_eq!(derive_ceiling(2).unwrap().stream_ceiling, 2);
    }

    #[test]
    fn the_floor_refusal_names_the_remedy_and_forbids_lowering_the_floor() {
        let msg = derive_ceiling(1).unwrap_err().to_string();
        assert!(msg.contains("MH_EGRESS_BUDGET_BPS"), "{msg}");
        assert!(msg.contains("NEVER lower the floor"), "{msg}");
        assert!(msg.contains("STREAM ceiling"), "{msg}");
        assert!(
            !msg.contains("subscriber"),
            "never 'subscriber ceiling': {msg}"
        );
    }

    #[test]
    fn a_ceiling_above_the_edge_bound_refuses_to_boot() {
        match EgressAdmission::derive(41 * 2_500_000, 90_000, 2_500_000, 0.05, 40) {
            Err(ConfigError::EgressStreamCeilingExceedsEdgeBound {
                ceiling,
                max_total_egress_edges,
            }) => {
                assert_eq!(ceiling, 41);
                assert_eq!(max_total_egress_edges, 40);
            }
            other => panic!("ceiling above the edge bound must refuse; got {other:?}"),
        }
        // Equal is fine: the resource guard sits AT the capacity.
        assert!(EgressAdmission::derive(40 * 2_500_000, 90_000, 2_500_000, 0.05, 40).is_ok());
    }

    #[test]
    fn a_zero_cost_refuses_to_boot_before_the_derivation_divides() {
        for key in [
            "MH_STREAM_COST_AUDIO_BPS",
            "MH_STREAM_COST_VIDEO_BPS",
            "MH_EGRESS_BUDGET_BPS",
        ] {
            let mut vars = base_vars();
            vars.insert(key.to_string(), "0".to_string());
            let err = Config::from_vars(&vars).unwrap_err().to_string();
            assert!(err.contains(key), "{key}: {err}");
        }
    }

    #[test]
    fn the_threshold_rejects_nan_infinities_and_out_of_range() {
        for raw in ["NaN", "inf", "-inf", "1.01", "-0.01"] {
            let mut vars = base_vars();
            vars.insert(
                "MH_EGRESS_REJECTION_RATIO_THRESHOLD".to_string(),
                raw.to_string(),
            );
            let err = Config::from_vars(&vars).unwrap_err().to_string();
            assert!(
                err.contains("MH_EGRESS_REJECTION_RATIO_THRESHOLD"),
                "{raw} must be refused: {err}"
            );
        }
        for raw in ["0", "0.05", "1"] {
            let mut vars = base_vars();
            vars.insert(
                "MH_EGRESS_REJECTION_RATIO_THRESHOLD".to_string(),
                raw.to_string(),
            );
            assert!(Config::from_vars(&vars).is_ok(), "{raw} is in range");
        }
    }

    /// The floor and the advisory minimum are two constants with two
    /// consequences and must never merge: a deployment between them boots.
    #[test]
    fn the_refuse_boot_floor_never_merges_with_the_recommended_min() {
        // Two participants hearing each other; N+1 = 6 hearing N = 5. The
        // ordering itself is a compile-time assertion next to the constants.
        assert_eq!(MIN_EGRESS_STREAM_CEILING, all_hear_all_streams(1));
        assert_eq!(MIN_EGRESS_STREAM_CEILING, 2, "the floor is 2, not 1");
        assert_eq!(
            EGRESS_STREAM_CEILING_RECOMMENDED_MIN,
            all_hear_all_streams(5)
        );
        let between = derive_ceiling(EGRESS_STREAM_CEILING_RECOMMENDED_MIN - 1).unwrap();
        assert!(between.below_recommended_min(), "boots, and is nudged");
        let at = derive_ceiling(EGRESS_STREAM_CEILING_RECOMMENDED_MIN).unwrap();
        assert!(
            !at.below_recommended_min(),
            "the nudge goes quiet at the minimum"
        );
    }

    /// The BASE `ConfigMap` placeholder boots and nudges; the KIND overlay budget
    /// boots, clears the recommended minimum, and fits the edge bound.
    #[test]
    fn the_deployed_budgets_boot_base_nudges_and_kind_does_not() {
        let base = Config::from_vars(&base_vars()).expect("base placeholder must boot");
        assert_eq!(base.egress_admission.stream_ceiling, 4);
        assert!(base.egress_admission.below_recommended_min());

        let mut vars = base_vars();
        vars.insert("MH_EGRESS_BUDGET_BPS".to_string(), "100000000".to_string());
        let kind = Config::from_vars(&vars).expect("Kind budget must boot");
        assert_eq!(kind.egress_admission.stream_ceiling, 40);
        assert!(!kind.egress_admission.below_recommended_min());
    }

    #[test]
    fn the_latency_ratio_reports_its_provenance() {
        let config = Config::from_vars(&base_vars()).unwrap();
        assert_eq!(
            config.media_latency_sample_ratio_source,
            ValueSource::Default
        );
        let mut vars = base_vars();
        vars.insert(
            "MH_MEDIA_LATENCY_SAMPLE_RATIO".to_string(),
            "0.5".to_string(),
        );
        let config = Config::from_vars(&vars).unwrap();
        assert_eq!(config.media_latency_sample_ratio_source, ValueSource::Env);
        assert_eq!(config.media_latency_sample_ratio, 0.5);
    }

    #[test]
    fn test_policy_limits_custom_values() {
        let mut vars = base_vars();
        vars.insert(
            "MH_MAX_EGRESS_STREAMS_PER_MEETING".to_string(),
            "64".to_string(),
        );
        vars.insert(
            "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS".to_string(),
            "4".to_string(),
        );
        vars.insert("MH_MAX_TOTAL_EGRESS_EDGES".to_string(), "1024".to_string());
        vars.insert("MH_POLICY_APPLY_TIMEOUT_MS".to_string(), "250".to_string());
        vars.insert("MH_MAX_REGISTERED_MEETINGS".to_string(), "300".to_string());

        let config = Config::from_vars(&vars).unwrap();
        assert_eq!(config.policy_limits.max_egress_streams_per_meeting, 64);
        assert_eq!(config.policy_limits.max_candidate_sources_per_egress, 4);
        assert_eq!(config.policy_limits.max_total_egress_edges, 1024);
        assert_eq!(config.policy_limits.policy_apply_timeout_ms, 250);
        assert_eq!(config.policy_limits.max_registered_meetings, 300);
    }

    /// Zero is rejected, not clamped.
    ///
    /// A bound of 0 would reject every registration — the same
    /// blackhole-by-configuration shape ADR-0036 §8 exists to eliminate, so it
    /// must fail loudly at startup rather than at the first `RegisterMeeting`.
    #[test]
    fn test_policy_bound_of_zero_is_rejected_loudly() {
        for key in [
            "MH_MAX_EGRESS_STREAMS_PER_MEETING",
            "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
            "MH_MAX_TOTAL_EGRESS_EDGES",
            "MH_POLICY_APPLY_TIMEOUT_MS",
            "MH_MAX_REGISTERED_MEETINGS",
        ] {
            let mut vars = base_vars();
            vars.insert(key.to_string(), "0".to_string());
            let err = Config::from_vars(&vars).unwrap_err();
            assert!(
                err.to_string().contains(key),
                "the error must name the offending variable, got: {err}"
            );
        }
    }

    /// A value above the hard ceiling is rejected, not clamped.
    ///
    /// A `> 0` check alone lets a fat-fingered or copy-pasted figure silently
    /// re-open the allocation surface the bound exists to close, and it would
    /// read as configured-on-purpose forever. An operator can raise a bound;
    /// they cannot raise it to an unbounded one. Clamping would be worse than
    /// rejecting: the process would run under a bound nobody chose and never
    /// learn about it.
    #[test]
    fn test_policy_bound_above_ceiling_is_rejected_not_clamped() {
        let cases = [
            (
                "MH_MAX_EGRESS_STREAMS_PER_MEETING",
                MAX_EGRESS_STREAMS_PER_MEETING_CEILING + 1,
            ),
            (
                "MH_MAX_CANDIDATE_SOURCES_PER_EGRESS",
                MAX_CANDIDATE_SOURCES_PER_EGRESS_CEILING + 1,
            ),
            (
                "MH_MAX_TOTAL_EGRESS_EDGES",
                MAX_TOTAL_EGRESS_EDGES_CEILING + 1,
            ),
            (
                "MH_MAX_REGISTERED_MEETINGS",
                MAX_REGISTERED_MEETINGS_CEILING + 1,
            ),
        ];
        for (key, value) in cases {
            let mut vars = base_vars();
            vars.insert(key.to_string(), value.to_string());
            let err = Config::from_vars(&vars).unwrap_err();
            assert!(err.to_string().contains(key), "got: {err}");
        }

        let mut vars = base_vars();
        vars.insert(
            "MH_POLICY_APPLY_TIMEOUT_MS".to_string(),
            (MAX_POLICY_APPLY_TIMEOUT_MS + 1).to_string(),
        );
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("MH_POLICY_APPLY_TIMEOUT_MS"));
    }

    /// A non-numeric value fails fast rather than silently reverting.
    ///
    /// Unlike the pre-existing scalars, which `parse().ok()` back to their
    /// default, a bound that quietly reverts is a bound nobody can rely on —
    /// and the whole point of these four is that exceeding them is a loud
    /// event.
    #[test]
    fn test_unparseable_policy_bound_is_rejected_rather_than_defaulted() {
        let mut vars = base_vars();
        vars.insert("MH_MAX_TOTAL_EGRESS_EDGES".to_string(), "lots".to_string());
        let err = Config::from_vars(&vars).unwrap_err();
        assert!(err.to_string().contains("MH_MAX_TOTAL_EGRESS_EDGES"));
    }

    /// The apply timeout's ceiling is tied to ADR-0036 §8's re-assert cadence.
    ///
    /// Above ~10s the await outlives the interval that would have retried it,
    /// so a longer value cannot help and can only pile up in-flight calls.
    #[test]
    fn test_policy_apply_timeout_ceiling_matches_the_reassert_cadence() {
        assert_eq!(
            MAX_POLICY_APPLY_TIMEOUT_MS, 10_000,
            "ADR-0036 §8 bounds the re-assert cadence at 10s; a longer apply await outlives the retry that would supersede it"
        );
    }

    #[test]
    fn test_max_connections_custom_value() {
        let mut vars = base_vars();
        vars.insert("MH_MAX_CONNECTIONS".to_string(), "5000".to_string());

        let config = Config::from_vars(&vars).expect("Config should load successfully");
        assert_eq!(config.max_connections, 5000);
    }

    // ============================================================================
    // OpenTelemetry Configuration Tests (R-55)
    // ============================================================================

    /// Helper: `base_vars()` plus any extra `OTel` vars supplied by the caller.
    fn otel_vars(extra: &[(&str, &str)]) -> HashMap<String, String> {
        let mut vars = base_vars();
        for (k, v) in extra {
            vars.insert((*k).to_string(), (*v).to_string());
        }
        vars
    }

    #[test]
    fn test_otel_defaults() {
        // No OTel env vars: disabled, empty endpoint, full sampling, dev environment.
        let config = Config::from_vars(&otel_vars(&[])).expect("Config should load");
        assert!(!config.otel_enabled, "OTel disabled by default");
        assert_eq!(config.otel_endpoint, "");
        assert_eq!(config.otel_sample_rate, DEFAULT_OTEL_SAMPLE_RATE);
        assert_eq!(config.otel_sample_rate, 1.0);
        assert_eq!(config.environment, "development");
        // Disabled → no OtelConfig produced.
        assert!(config.otel_config().is_none());
    }

    #[test]
    fn test_otel_enabled_true_false() {
        let enabled = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "true"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
        ]))
        .expect("Config should load");
        assert!(enabled.otel_enabled);

        let disabled = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "false")]))
            .expect("Config should load");
        assert!(!disabled.otel_enabled);
    }

    #[test]
    fn test_otel_enabled_case_insensitive() {
        let config = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "TRUE"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
        ]))
        .expect("Config should load");
        assert!(config.otel_enabled);
    }

    #[test]
    fn test_otel_enabled_invalid_value_rejected() {
        // Non-bool value is an explicit ConfigError, NOT a silent false.
        let result = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "yes")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTEL_ENABLED") && msg.contains("yes")),
            "invalid OTEL_ENABLED should be a ConfigError carrying the offending value"
        );
    }

    #[test]
    fn test_otel_endpoint_maps_from_otlp_endpoint() {
        let config = Config::from_vars(&otel_vars(&[(
            "OTLP_ENDPOINT",
            "http://otel-collector.dark-tower:4317",
        )]))
        .expect("Config should load");
        assert_eq!(
            config.otel_endpoint,
            "http://otel-collector.dark-tower:4317"
        );
    }

    #[test]
    fn test_otel_sample_rate_valid_values_stored() {
        // Parse-only (team-lead FREEZE X): valid numeric values are parsed and stored verbatim.
        for v in ["0.0", "0.5", "1.0"] {
            let config = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", v)]))
                .unwrap_or_else(|_| panic!("{v} should parse"));
            assert_eq!(
                config.otel_sample_rate,
                v.parse::<f64>().expect("test literal")
            );
        }
    }

    #[test]
    fn test_otel_sample_rate_out_of_range_accepted_at_config_load() {
        // Parse-only contract (team-lead FREEZE X): config load does NOT range-check;
        // the [0.0, 1.0] bound has a single owner, init_otel, which re-validates at
        // startup on the enabled path. Out-of-range values PARSE here and are stored
        // inert; they hard-fail only at init_otel when otel_enabled=true. This test
        // locks the intentional deferral (a re-added config-load range check would
        // break it and signal a duplicated bound).
        let low = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "-0.1")]))
            .expect("-0.1 parses at config load (range deferred to init_otel)");
        assert_eq!(low.otel_sample_rate, -0.1);

        let high = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "1.1")]))
            .expect("1.1 parses at config load (range deferred to init_otel)");
        assert_eq!(high.otel_sample_rate, 1.1);
    }

    #[test]
    fn test_otel_sample_rate_non_numeric_rejected() {
        let result = Config::from_vars(&otel_vars(&[("OTEL_SAMPLE_RATE", "half")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("OTEL_SAMPLE_RATE") && msg.contains("half")),
            "non-numeric sample rate should be a ConfigError carrying the offending value"
        );
    }

    #[test]
    fn test_deployment_environment_custom() {
        let config = Config::from_vars(&otel_vars(&[("DEPLOYMENT_ENVIRONMENT", "staging")]))
            .expect("Config should load");
        assert_eq!(config.environment, "staging");
    }

    #[test]
    fn test_otel_enabled_requires_endpoint() {
        // Enabled with no endpoint → fail fast.
        let result = Config::from_vars(&otel_vars(&[("OTEL_ENABLED", "true")]));
        assert!(
            matches!(result, Err(ConfigError::InvalidOtelConfig(msg)) if msg.contains("requires a non-empty OTLP_ENDPOINT")),
            "enabled + empty endpoint should be rejected"
        );
    }

    #[test]
    fn test_otel_config_some_when_enabled() {
        let config = Config::from_vars(&otel_vars(&[
            ("OTEL_ENABLED", "true"),
            ("OTLP_ENDPOINT", "http://collector:4317"),
            ("OTEL_SAMPLE_RATE", "0.5"),
        ]))
        .expect("Config should load");
        let otel = config.otel_config().expect("enabled → Some(OtelConfig)");
        assert_eq!(otel.endpoint, "http://collector:4317");
        assert_eq!(otel.sample_rate, 0.5);
    }

    #[test]
    fn test_otel_config_gating_precedence_endpoint_set_but_disabled() {
        // Regression guard for the Q2 semantic change: endpoint populated but
        // OTEL_ENABLED unset → otel_config() is None (NOT presence-gated).
        let config = Config::from_vars(&otel_vars(&[("OTLP_ENDPOINT", "http://collector:4317")]))
            .expect("Config should load");
        assert_eq!(config.otel_endpoint, "http://collector:4317");
        assert!(!config.otel_enabled);
        assert!(
            config.otel_config().is_none(),
            "endpoint present but disabled must NOT enable OTel (no presence-gating)"
        );
    }
}
