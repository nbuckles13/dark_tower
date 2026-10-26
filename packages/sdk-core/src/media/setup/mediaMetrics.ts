// File: packages/sdk-core/src/media/setup/mediaMetrics.ts
//
// THE ONLY PLACE IN `media/**` THAT MAY NAME A `dt_client_*` METRIC.
//
// ---------------------------------------------------------------------------
// THE LABEL SET IS AN ALLOW-LIST PROJECTION, NOT A PRUNED SPREAD
// ---------------------------------------------------------------------------
//
// ADR-0036 §11: media-path metrics carry `client_version`, `org_id` and
// `key_custody=operator`, and NEVER a participant, meeting, or stream dimension.
// The SDK's join label set (`MeetingSession`'s `#metricLabels`) carries
// `meeting_id_hash` and is threaded into every join emission site — so the
// violation here arrives BY INERTIA, from a default that reads as the local
// convention, and nothing mechanical catches it in TypeScript (`dt-guard`'s
// media-path deny is Rust-only, its metric-label scanner reads `crates/`, and
// `ts_pii.rs` scans only `console.*` / `logger.*` call sites).
//
// So this is structural rather than disciplined: {@link MediaKindMetrics}'s and
// {@link MediaSessionMetrics}' constructors take TWO NAMED STRINGS (plus, for the
// per-kind handle, a closed `MediaKindLabel` union value), never a
// `MetricLabels` bag. There is no spread to inherit from and nothing to prune,
// so `meeting_id_hash` is not merely absent — it is UNREPRESENTABLE at this
// boundary. A deletion-based helper (`{...labels, meeting_id_hash: undefined}`)
// would be one refactor away from re-inheriting a newly-added join label; an
// allow-list cannot be.
//
// ---------------------------------------------------------------------------
// PER-KIND HANDLES AND SESSION-LEVEL HANDLES ARE DIFFERENT TYPES
// ---------------------------------------------------------------------------
//
// Every media series whose subject has a media kind carries `media_kind`
// (`audio` | `video`); the session-level series (KEK lifecycle, roster key
// changes, capture source) do NOT, because one session event labelled per kind
// would be counted twice. The split is by TYPE: {@link MediaKindMetrics} has no
// session method and {@link MediaSessionMetrics} takes no kind, so emitting a
// session series per kind (or a per-kind series kind-free) is a compile error,
// not a review catch. Catalog: `docs/observability/metrics/client.md`.
//
// `MediaTransport`'s `#emitMetric` DOES spread the join set. That is legal for
// exactly one grandfathered metric and is annotated at that site; see
// `../MediaTransport.ts`. Nothing here shares code with it, deliberately — what
// is reused is `MetricsSink` (the interface), never the label set.
//
// ---------------------------------------------------------------------------
// HANDLES ARE RESOLVED ONCE, HERE, AT SETUP
// ---------------------------------------------------------------------------
//
// ADR-0036 §11's per-frame invariant is zero allocation and zero registry
// lookup, with "no metric macro reachable from the forward function". The
// TypeScript analogue: the hot path in `../pipeline/**` calls METHODS on an
// instance built here, and contains no metric name and no label construction at
// all. `__tests__/hotPathLayout.test.ts` asserts that — the only file under
// `media/**` permitted to contain a `dt_client_` literal is this one.
//
// ---------------------------------------------------------------------------
// KEY CUSTODY IS A LABEL, NOT A BOOLEAN
// ---------------------------------------------------------------------------
//
// ADR-0036 §4/§11: media is encrypted between clients; MH, transport and storage
// cannot read it; MC can. Every service reports `key_custody=operator`, and no
// metric, log, span attribute or document may carry an end-to-end or zero-trust
// boolean, because the default deployment is neither.

import type { MetricLabels, MetricsSink } from '../../telemetry/MetricsSink.js';
import type { KekRetentionOutcome } from '../../config/clientConfig.js';
import type { RejectReason } from '../frame/rejectReason.js';
import type { WrapOutcome } from '../frame/receivePath.js';
import type { CaptureFailure } from './captureFailure.js';

/**
 * Key custody, fixed at `operator` (ADR-0036 §4). A LABEL, never a boolean.
 *
 * There is deliberately no other value: a second value would imply the
 * end-to-end claim §11 bars, and adding one is a design decision, not a config
 * change.
 */
const KEY_CUSTODY = 'operator';

/**
 * Send-side drop reasons — a NEW bounded vocabulary, distinct from the frame
 * reject taxonomy in `../frame/rejectReason.ts`.
 *
 * ---------------------------------------------------------------------------
 * THIS MIRRORS MH'S `MediaDropReason` AND HAS NO DRIFT GUARD, WHICH IS THE
 * WHOLE REASON THIS PARAGRAPH EXISTS
 * ---------------------------------------------------------------------------
 *
 * Four of the five spellings deliberately match
 * `crates/mh-service/src/observability/metrics.rs`'s `MediaDropReason`, so that
 * `sum by(reason)` compares across the two ends of one hop. ANCHOR (DRY): that
 * file.
 *
 * **The reject-reason mirror three files away is protected and this one is
 * not.** `rejectReason.ts` earns its hand-written token list with a JSON SSoT
 * (`proto/test-vectors/frame-v3.vectors.json`) and a both-directions
 * set-equality test. THIS list has a comment and nothing else: no shared home is
 * reachable (the natural candidate, the vectors file, is a Guarded Shared Area
 * carrying frame REJECT reasons, not send-drop reasons, so widening it is a
 * protocol-owned decision), and no guard compares these five strings to
 * anything.
 *
 * That asymmetry is stated because PROXIMITY TO A GUARDED MIRROR IMPLIES
 * COVERAGE. A reader who has just seen `rejectReason.ts`'s three-assertion
 * header will reasonably assume this list is protected the same way. It is not,
 * and that inference is what this paragraph exists to block.
 *
 * **But the spellings are the well-protected half, not the exposed one** — each
 * side is type- or compile-checked against its own vocabulary, so a typo fails
 * locally. `docs/TODO.md` §Cross-Service Duplication carries the sharper
 * statement of what is actually at risk and the argument against widening a
 * Guarded Shared Area to hold a telemetry vocabulary. Read it before adding,
 * renaming, or removing a token here.
 *
 * MUTE IS NOT A SEND DROP. While client-muted nothing is encoded, so nothing
 * enters the egress queue, nothing is dropped, and nothing is counted here or
 * against `dt_client_media_frames_sent_total`. "Count the frames we did not
 * send" is the tempting wrong turn; it would spike the send-drop rate on every
 * mute. Mute is `dt_client_media_mute_transitions_total{action}` and nothing
 * else.
 *
 * RESERVED SENTINEL: `invalid` must never become a real token in this or any
 * neighbouring media vocabulary. The OTel collector rewrites any value failing
 * its charset check to exactly that string
 * (`infra/services/otel-collector/collector.yaml`), so a real token spelled
 * `invalid` would be indistinguishable from a rewritten junk value from a
 * patched client. `dt-guard client-metrics-export` (G6) fails the build if it
 * ever appears here or in MH's `MediaDropReason`.
 */
export const MEDIA_SEND_DROP_REASONS = {
  /**
   * The bounded egress queue was full and the OLDEST frame was evicted.
   * SHARED spelling with MH's `MediaDropReason::EgressQueueOverflow`.
   */
  EgressQueueOverflow: 'egress_queue_overflow',
  /**
   * The transport refused a datagram it should have accepted.
   * SHARED spelling with MH's `MediaDropReason::TransportSendRefused`.
   *
   * Fleet contract: this should read ZERO FOREVER and is alertable at `> 0`.
   * That is why {@link MEDIA_SEND_DROP_REASONS.OversizeDatagram} exists
   * separately — see it.
   */
  TransportSendRefused: 'transport_send_refused',
  /**
   * The frame exceeded the transport's maximum datagram size and was never
   * offered. SHARED spelling with MH's `MediaDropReason::OversizeDatagram`.
   *
   * NOT theoretical on this story: every audio frame is key-bearing, so each
   * carries the publisher region plus a 50-byte wrapped-key block plus a 64-byte
   * signature on top of the Opus payload, and the bitrate is directed by MC from
   * an operator-settable ceiling. Without this token a single configuration
   * change would produce 100% send loss reported as a transport fault, poisoning
   * a counter whose contract is "reads zero forever" and making it unalertable.
   */
  OversizeDatagram: 'oversize_datagram',
  /**
   * The connection closed underneath a send. SHARED spelling with MH's
   * `MediaDropReason::ConnectionClosed`.
   *
   * ROUTINE — a participant leaves every meeting, many times — and deliberately
   * NOT folded into `transport_send_refused`, which would make that counter an
   * unalertable mixture of "we have a bug" and "someone hung up". MH made the
   * identical split for the identical reason.
   */
  ConnectionClosed: 'connection_closed',
  /**
   * A send was attempted before any transport was up. CLIENT-ONLY: MH never
   * initiates, so there is no counterpart token and `sum by(reason)` across the
   * two ends is meaningful only for the four shared values above.
   *
   * A LIFECYCLE ORDERING BUG. Should read zero forever; alertable at `> 0`.
   */
  NotConnected: 'not_connected',
  /**
   * The stream's `gop_counter` is exhausted: `u32::MAX` was its last legal
   * value, so ADR-0036 §2 stops the stream rather than wrap. Every later frame
   * is refused BEFORE it is built, and counted once per ACTIVE lane — the send
   * attempts it would have made — so `frames_sent + send_dropped` stays a
   * per-lane attempt total; an exhausted stream reads 100% send loss on every
   * lane. CLIENT-ONLY (MH never originates a stream). Unreachable in practice
   * (audio at 50 fps needs ~2.7 years on one stream): zero forever, and
   * deliberately not alerted (`docs/observability/alerts.md`).
   */
  GopCounterExhausted: 'gop_counter_exhausted',
} as const;

/** One of {@link MEDIA_SEND_DROP_REASONS}. Derived, never retyped at a call site. */
export type MediaSendDropReason =
  (typeof MEDIA_SEND_DROP_REASONS)[keyof typeof MEDIA_SEND_DROP_REASONS];

/** Bounded `action` vocabulary for `dt_client_media_mute_transitions_total`. */
export const MEDIA_MUTE_ACTIONS = {
  Mute: 'mute',
  Unmute: 'unmute',
} as const;

/** One of {@link MEDIA_MUTE_ACTIONS}. */
export type MediaMuteAction = (typeof MEDIA_MUTE_ACTIONS)[keyof typeof MEDIA_MUTE_ACTIONS];

/**
 * Bounded `source` vocabulary for `dt_client_media_kek_updates_total`.
 *
 * TWO values, deliberately: `join_response` (a first join, AND a reconnect
 * re-issue, whose response is JoinResponse-shaped) and `kek_update` (a
 * rotation push; `MeetingKekUpdate` is sent only by MC's rotation path). The
 * client cannot see a rotation's cause; that split lives on MC's `trigger`
 * label on `mc_meeting_kek_generated_total`, never as a third value here.
 *
 * The ONE home of this vocabulary. `kekSource.ts`'s arrival type is DERIVED from
 * it, never retyped, so an addition cannot land in one copy only.
 */
export const MEDIA_KEK_SOURCES = {
  JoinResponse: 'join_response',
  KekUpdate: 'kek_update',
} as const;

/** One of {@link MEDIA_KEK_SOURCES}. */
export type MediaKekSource = (typeof MEDIA_KEK_SOURCES)[keyof typeof MEDIA_KEK_SOURCES];

/**
 * Bounded `outcome` vocabulary for `dt_client_media_kek_retention_anomalies_total`.
 *
 * Every retention outcome EXCEPT `nominal`, spelled here as literals so the
 * collector-sentinel check (`dt-guard client-metrics-export` G6, which reads this
 * file) can see them, and tied to the config's `KekRetentionOutcome` by the
 * `satisfies` below so the two cannot drift: a new outcome added there without
 * one here is a type error.
 *
 * A PER-METRIC vocabulary. `outcome` carries no cross-metric aggregation
 * contract (`docs/observability/label-taxonomy.md`), so these values never mean
 * anything summed against `key_wrap_outcomes_total`'s or the refusals counter's.
 */
export const MEDIA_KEK_RETENTION_ANOMALIES = {
  FloorSubstituted: 'floor_substituted',
  CeilingClamped: 'ceiling_clamped',
  BelowRewrapLatency: 'below_rewrap_latency',
} as const satisfies Record<string, Exclude<KekRetentionOutcome, 'nominal'>>;

/** One of {@link MEDIA_KEK_RETENTION_ANOMALIES}. */
export type MediaKekRetentionAnomaly =
  (typeof MEDIA_KEK_RETENTION_ANOMALIES)[keyof typeof MEDIA_KEK_RETENTION_ANOMALIES];

/**
 * Bounded `outcome` vocabulary for `dt_client_media_kek_install_refusals_total`.
 *
 * `outcome`, NOT `reason`: `reason` is scoped to per-FRAME media-path drops with
 * exactly two named homes (`label-taxonomy.md`), and a KEK install refusal is
 * not a frame drop. A per-metric vocabulary, like the one above.
 */
export const MEDIA_KEK_INSTALL_REFUSALS = {
  /** Same generation already held, DIFFERENT bytes. The held key is kept. */
  ConflictingKey: 'conflicting_key',
  /** A generation below the current. Never rolls the current back. */
  OlderGeneration: 'older_generation',
  /** Wrong width, all-zero, or a generation outside the wire's u16 range. */
  Malformed: 'malformed',
} as const;

/**
 * Bounded `outcome` vocabulary for `dt_client_media_roster_key_rebinds_total`.
 *
 * Split because the two point at DIFFERENT fixes, and because only one of them
 * is near-zero by construction: MC never legitimately rebinds a live sender to
 * different bytes, but it DOES publish empty keys (the `no_roster_entry` catalog
 * entry documents it), so a merged counter would make any alert on it fire on a
 * known, expected condition. A per-metric vocabulary.
 *
 * `rebind` IS NOT READS-ZERO-FOREVER (story 2 R-16). MC reissues a sender id to a
 * new identity after a KEK-epoch reset. Normally the old holder's
 * `ParticipantLeft` arrives first, the roster forgets the id, and the reissue is
 * a FIRST binding — not counted. But MC sends roster updates with `try_send`,
 * and a `ParticipantLeft` dropped under outbound backpressure means this client
 * later sees the id bound to a new key with no prior removal: a `rebind`. So a
 * non-zero reading has THREE causes: an MC defect, a cache-poisoning attempt, or
 * a lost `ParticipantLeft`. MC's
 * `mc_participant_outbound_messages_dropped_total{payload_kind="participant_update_left"}`
 * non-zero over the same window makes the third SUPPORTED; zero rules it out.
 * It is fleet-wide, so it never confirms that any individual increment was a
 * lost leave. Key hygiene holds on every cause: the `rebind` arm purges the same
 * transmit keys a removal would have. The underlying delivery gap is filed in
 * `docs/TODO.md` (roster removals are droppable).
 */
export const MEDIA_ROSTER_KEY_CHANGES = {
  /**
   * A live sender bound to DIFFERENT, well-formed key bytes. Near-zero, not
   * zero: see the vocabulary's doc for the three causes, one of them benign.
   */
  Rebind: 'rebind',
  /** A live sender's key replaced by an empty, wrong-width or unusable one. */
  Downgrade: 'downgrade',
} as const;

/** One of {@link MEDIA_ROSTER_KEY_CHANGES}. */
export type MediaRosterKeyChange =
  (typeof MEDIA_ROSTER_KEY_CHANGES)[keyof typeof MEDIA_ROSTER_KEY_CHANGES];

/** One of {@link MEDIA_KEK_INSTALL_REFUSALS}. */
export type MediaKekInstallRefusal =
  (typeof MEDIA_KEK_INSTALL_REFUSALS)[keyof typeof MEDIA_KEK_INSTALL_REFUSALS];

/**
 * Bounded `outcome` vocabulary for `dt_client_media_capture_failures_total`
 * (story 3 R-19, R-30): the capture failure vocabulary
 * (`captureFailure.ts:CaptureFailure`) plus `ended` (a track that stopped on
 * its own mid-call: unplugged, revoked).
 *
 * `outcome`, NOT `reason` (observability ruling, story 3 E4): `reason` is
 * reserved for the two per-FRAME drop families (`label-taxonomy.md`), and a
 * capture failure is not a frame drop. A per-metric vocabulary.
 *
 * Spelled as literals here so G6 (which reads this file) sees them, and tied to
 * `CaptureFailure` in BOTH directions so the two cannot drift: the `satisfies`
 * rejects a token that is not a capture failure (or `ended`), and
 * {@link CaptureOutcomesCoverCaptureFailure} rejects a capture failure missing
 * here.
 */
export const MEDIA_CAPTURE_FAILURE_OUTCOMES = {
  PermissionDenied: 'permission_denied',
  NoDevice: 'no_device',
  DeviceUnavailable: 'device_unavailable',
  Unsupported: 'unsupported',
  PlatformUnsupported: 'platform_unsupported',
  Unknown: 'unknown',
  Ended: 'ended',
} as const satisfies Record<string, CaptureFailure | 'ended'>;

/** One of {@link MEDIA_CAPTURE_FAILURE_OUTCOMES}. */
export type MediaCaptureFailureOutcome =
  (typeof MEDIA_CAPTURE_FAILURE_OUTCOMES)[keyof typeof MEDIA_CAPTURE_FAILURE_OUTCOMES];

/** Compile-time: every `CaptureFailure` is an outcome. `never` here is a type error below. */
type CaptureOutcomesCoverCaptureFailure =
  Exclude<CaptureFailure, MediaCaptureFailureOutcome> extends never ? true : never;
const CAPTURE_OUTCOMES_COVER_CAPTURE_FAILURE: CaptureOutcomesCoverCaptureFailure = true;
void CAPTURE_OUTCOMES_COVER_CAPTURE_FAILURE;

/**
 * Bounded `outcome` vocabulary for
 * `dt_client_media_keyframe_requests_received_total` (story 3 R-11, R-30): what
 * the publisher did with an MH `KeyframeRequest` for one of its streams.
 * Declared IN FULL now (story 3 C-A1); C-A2 (task 13) emits the honour paths.
 *
 *   * `honoured` — a keyframe was produced for this request.
 *   * `honoured_counter_ahead` — produced, the request naming a counter ahead
 *     of the publisher's own.
 *   * `stale_counter` — refused: the counter is behind (a newer group exists).
 *   * `min_interval` — deferred: within the minimum keyframe interval.
 *   * `not_sending` — refused: not a stream this client is currently sending
 *     to that handler. Until C-A2 every request lands here.
 *   * `malformed` — not decodable. Counted KIND-FREE on
 *     `dt_client_media_keyframe_requests_unattributed_total`, since a malformed
 *     request names no kind; declared here so the vocabulary is one list.
 *
 * NOT ZERO-INITIALISED per kind (story 3 E4 ruling): a `media_kind="video"`
 * series before video flows would let the video read-back pass vacuously, and
 * audio has no keyframes. A per-metric vocabulary: never summed by `outcome`
 * against any other series (`label-taxonomy.md`, per-metric `outcome`).
 */
export const MEDIA_KEYFRAME_REQUEST_OUTCOMES = {
  Honoured: 'honoured',
  HonouredCounterAhead: 'honoured_counter_ahead',
  StaleCounter: 'stale_counter',
  MinInterval: 'min_interval',
  NotSending: 'not_sending',
  Malformed: 'malformed',
} as const;

/** One of {@link MEDIA_KEYFRAME_REQUEST_OUTCOMES}. */
export type MediaKeyframeRequestOutcome =
  (typeof MEDIA_KEYFRAME_REQUEST_OUTCOMES)[keyof typeof MEDIA_KEYFRAME_REQUEST_OUTCOMES];

/**
 * The outcomes a request with NO attributable kind can have — the `outcome`
 * values of `dt_client_media_keyframe_requests_unattributed_total`. A subset of
 * {@link MEDIA_KEYFRAME_REQUEST_OUTCOMES}, derived from it.
 */
export const MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES = [
  MEDIA_KEYFRAME_REQUEST_OUTCOMES.NotSending,
  MEDIA_KEYFRAME_REQUEST_OUTCOMES.Malformed,
] as const;

/** One of {@link MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES}. */
export type MediaKeyframeUnattributedOutcome =
  (typeof MEDIA_KEYFRAME_UNATTRIBUTED_OUTCOMES)[number];

/**
 * The outcomes a request ATTRIBUTED to a published kind can have: every
 * keyframe outcome except `malformed` (a malformed request names no kind and is
 * counted kind-free).
 */
export type MediaKeyframePerKindOutcome = Exclude<
  MediaKeyframeRequestOutcome,
  typeof MEDIA_KEYFRAME_REQUEST_OUTCOMES.Malformed
>;

/**
 * Bounded `source` vocabulary for `dt_client_media_keyframes_produced_total`
 * (story 3 R-30, amendment 4): WHY the publisher keyed a video stream.
 *
 *   * `directive_start` — a directed start or resume, or a target added to an
 *     active stream (a target with no group in progress).
 *   * `request` — an accepted MH `KeyframeRequest`.
 *   * `abandonment` — the publisher re-keying after an abandonment IT decided
 *     (its per-target backlog cap). Never folded into `directive_start`: the
 *     split keeps a lossy-uplink keying loop distinguishable from a request
 *     storm. An MH STOP_SENDING / refusal is MH's decision and never lands here.
 *
 * A keyframe is counted ONCE; a merged deferral counts under the cause that
 * created it. A per-metric vocabulary: `source` also means `join_response |
 * kek_update` on `dt_client_media_kek_updates_total`, and the two are never
 * summed by `source` across series.
 */
export const MEDIA_KEYFRAME_SOURCES = {
  DirectiveStart: 'directive_start',
  Request: 'request',
  Abandonment: 'abandonment',
} as const satisfies Record<string, string>;

/**
 * Bounded `outcome` vocabulary for `dt_client_media_send_groups_stopped_total`
 * (story 3 C-A2): why a target went dark for the PEER's reasons — the lane went
 * `mh_stopped` and gets no publisher re-key (whoever decides an abandonment
 * repairs it).
 *
 *   * `peer_stopped` — MH sent STOP_SENDING, or a write failed in any way.
 *   * `open_failed` — the group stream open was rejected.
 *   * `open_stalled` — the backlog cap tripped while the open was still pending
 *     (stream credit never arrived: MAX_STREAMS starvation). The ONLY signal for
 *     it anywhere: quinn enforces the limit and MH has no event to count.
 *
 * A per-metric vocabulary: never summed by `outcome` across series.
 */
export const MEDIA_SEND_GROUP_STOP_OUTCOMES = {
  PeerStopped: 'peer_stopped',
  OpenFailed: 'open_failed',
  OpenStalled: 'open_stalled',
} as const satisfies Record<string, string>;

/** One of {@link MEDIA_SEND_GROUP_STOP_OUTCOMES}. */
export type MediaSendGroupStopOutcome =
  (typeof MEDIA_SEND_GROUP_STOP_OUTCOMES)[keyof typeof MEDIA_SEND_GROUP_STOP_OUTCOMES];

/** One of {@link MEDIA_KEYFRAME_SOURCES}. */
export type MediaKeyframeSource =
  (typeof MEDIA_KEYFRAME_SOURCES)[keyof typeof MEDIA_KEYFRAME_SOURCES];

/**
 * Bounded `source` vocabulary for `dt_client_media_recovery_requests_sent_total`
 * (story 3 R-13, C-B): the CAUSE — why a receiver's video slot lost
 * decodability and sent its handler ONE recovery request for that loss event.
 *
 * `source`, NOT `reason` (label-taxonomy owner ruling at C-B, superseding the
 * task text's `reason=`): `reason` is the frame-drop label space whose contract
 * is `sum by(reason)` across hops, and `decrypt_failed` here is a SUPERSET of
 * the codec token of that name (it also covers unwrap, missing transmit key and
 * missing KEK). `source` is the client's "what caused this event" key, as on
 * `dt_client_media_keyframes_produced_total`.
 *
 *   * `verify_failed` — a frame on the slot could not be verified: bad
 *     signature, no roster key for its key-id sender (roster lag on a new
 *     source), or a whole-frame codec reject with a readable key id.
 *   * `decrypt_failed` — verified but could not be opened: decrypt or unwrap
 *     failure, no transmit key (a missed key-bearing frame), no KEK for its
 *     generation (KEK lag).
 *   * `decoder_error` — the slot's decoder raised its error callback.
 *   * `local_drop` — the slot's jitter buffer overflowed or held a frame past
 *     its maximum hold time (local resource pressure).
 *   * `group_abandoned` — MH reset the group's stream after a frame header was
 *     parsed on it (R-12).
 *   * `server_mute_lifted` — a frame dropped `sender_server_muted` at arrival
 *     (typically the publisher's resume keyframe beating the
 *     `ParticipantMuteUpdate`) is re-requested once the unmute arrives; a
 *     sustained rate means unmute ordering races are common, not a fault. An
 *     observability extension of the task's five (C-B Gate 1, ruling O2).
 *
 * Zero-initialised when the video receiver starts. Never a sender, slot or GoP
 * value. A per-metric vocabulary: never summed by `source` with
 * `keyframes_produced_total` or `kek_updates_total`.
 */
export const MEDIA_RECOVERY_SOURCES = {
  VerifyFailed: 'verify_failed',
  DecryptFailed: 'decrypt_failed',
  DecoderError: 'decoder_error',
  LocalDrop: 'local_drop',
  GroupAbandoned: 'group_abandoned',
  ServerMuteLifted: 'server_mute_lifted',
} as const satisfies Record<string, string>;

/** One of {@link MEDIA_RECOVERY_SOURCES}. */
export type MediaRecoverySource =
  (typeof MEDIA_RECOVERY_SOURCES)[keyof typeof MEDIA_RECOVERY_SOURCES];

/**
 * Bucket upper bounds (ms) for `dt_client_media_slot_unfulfilled_duration_ms`
 * — THE SSoT; the catalog row cites this name. Extends past 100 s (10x the
 * 10 000 ms maximum keyframe interval) so a slot stuck for a whole interval
 * cycle is not folded into `+Inf`. Changing a boundary splits the `le` series
 * across client versions.
 */
export const SLOT_UNFULFILLED_BUCKETS_MS: readonly number[] = [
  100, 250, 500, 1_000, 2_500, 5_000, 10_000, 20_000, 30_000, 60_000, 120_000, 300_000,
];

/**
 * Bucket upper bounds (ms) for the two first-render latency histograms
 * (`dt_client_media_time_to_first_render_ms`,
 * `dt_client_media_slot_active_to_first_render_ms`) — THE SSoT. Extends past
 * the 10 000 ms maximum keyframe interval to 30 s. Changing a boundary splits
 * the `le` series across client versions.
 */
export const FIRST_RENDER_BUCKETS_MS: readonly number[] = [
  50, 100, 250, 500, 1_000, 2_000, 3_000, 5_000, 7_500, 10_000, 15_000, 20_000, 30_000,
];

/**
 * Bounded `outcome` vocabulary for
 * `dt_client_media_mh_control_channel_ended_total`: how a handler's control
 * stream ended while this client was not tearing down. A per-metric vocabulary.
 */
export const MEDIA_MH_CONTROL_CHANNEL_ENDINGS = {
  /** MH reset the stream, or the connection was lost. */
  Reset: 'reset',
  /** MH finished its send half. */
  Finished: 'finished',
} as const;

/** One of {@link MEDIA_MH_CONTROL_CHANNEL_ENDINGS}. */
export type MediaMhControlChannelEnding =
  (typeof MEDIA_MH_CONTROL_CHANNEL_ENDINGS)[keyof typeof MEDIA_MH_CONTROL_CHANNEL_ENDINGS];

/**
 * The wrap outcomes that are REPORTED.
 *
 * Derived from `WrapOutcome` by exclusion rather than retyped, so the two
 * spellings cannot drift from the union that produces them.
 *
 * `absent`, `cached` and `already_held` are deliberately NOT counted:
 * `already_held` is the steady state for audio (the wrap rides every frame), so
 * counting them is per-frame counter work on buckets whose semantics are
 * "nothing to do", and it would bury the two real signals in a noise floor.
 *
 * Neither reported value reaches `dt_client_media_frames_dropped_total`: both
 * describe frames that were ACCEPTED, and counting an accepted frame as a drop
 * breaks `received = accepted + sum(drops by reason)` in aggregate, silently,
 * long after the label set is frozen.
 */
export type ReportableWrapOutcome = Exclude<WrapOutcome, 'absent' | 'cached' | 'already_held'>;

/**
 * Bounded `mode` vocabulary for `dt_client_media_capture_source` — what feeds
 * this client's send path.
 *
 * ---------------------------------------------------------------------------
 * A TYPE, NOT A CONST OBJECT — DELIBERATELY UNLIKE {@link MEDIA_MUTE_ACTIONS}
 * ---------------------------------------------------------------------------
 *
 * A const object would put the string `test_tone` into every production bundle.
 * The token must exist ONLY inside the `__DT_TEST_TONE__` build-time gate (the
 * capture-source selection, `session/mediaSelection.ts:selectCaptureSource`), so a production
 * build cannot even spell the mode, and
 * `packages/web-app/tests/bundle-content.test.ts` asserts its absence. Types
 * vanish at compile time; the union still bounds every call site.
 *
 * Bounded and identity-free: a BUILD-TIME property of the bundle, never per
 * user. Admitted as a datapoint label key by GC's `MEDIA_DATAPOINT_EXTRA` and
 * the collector's `keep_keys` (`docs/observability/label-taxonomy.md`).
 * `invalid` is the collector's reserved rewrite sentinel and must never become a
 * value here.
 */
export type MediaCaptureSourceMode = 'microphone' | 'test_tone';

/** The identity dimensions a media metric may carry. Two named strings; nothing else. */
export interface MediaMetricIdentity {
  /** SDK build version. */
  readonly clientVersion: string;
  /**
   * Organisation subdomain. Not a participant dimension.
   *
   * ADVISORY ON THE WIRE: GC replaces this value with the authenticated
   * `UserClaims.org_id` (an org UUID) before anything is forwarded, so the
   * stored label is the authenticated tenant and not whatever a browser sent —
   * see `crates/gc-service/src/services/telemetry_filter.rs`. The previous
   * "bounded cardinality" claim here was never enforceable from the client (a
   * patched browser sends what it likes); it is now true for a different and
   * better reason: the value space is the set of authenticated orgs.
   */
  readonly orgId: string;
}

/**
 * The `media_kind` label vocabulary — closed, two values.
 *
 * ANCHOR (DRY): the `media_kind` row of the label taxonomy
 * (`docs/observability/label-taxonomy.md`) is the single source of truth; this
 * is its client code home, and `__tests__/mediaMetrics.test.ts` fails if the two
 * disagree. MC's Rust home is `MediaKindLabel` in
 * `crates/mc-service/src/media_kind.rs`; the collector bounds the
 * value with an EXACT `^(audio|video)$` rewrite to `invalid`
 * (`infra/services/otel-collector/collector.yaml`). Deliberately NOT the
 * generated proto `MediaKind` (four wire values incl. unspecified and screen),
 * and NOT the public `media/mediaKind.ts` tokens (`unknown` included).
 *
 * `invalid` is the collector's reserved rewrite sentinel and must never become a
 * value here.
 */
export const MEDIA_KIND_LABELS = {
  Audio: 'audio',
  Video: 'video',
} as const;

/** One of {@link MEDIA_KIND_LABELS}. Derived, never retyped at a call site. */
export type MediaKindLabel = (typeof MEDIA_KIND_LABELS)[keyof typeof MEDIA_KIND_LABELS];

/**
 * Build a PER-KIND media-path label set BY ALLOW-LIST.
 *
 * The complete set, and there is no path by which it grows: `client_version`,
 * `org_id`, `key_custody`, `media_kind`. Callers pass two named strings and a
 * closed-union kind, so no ambient label bag can reach a media metric.
 *
 * The returned object literal must stay SPREAD-FREE: `dt-guard
 * client-metrics-export` resolves `this.#base` to exactly these keys and fails
 * closed on anything it cannot read.
 */
export function mediaMetricLabels(
  identity: MediaMetricIdentity,
  kind: MediaKindLabel,
): MetricLabels {
  return {
    client_version: identity.clientVersion,
    org_id: identity.orgId,
    key_custody: KEY_CUSTODY,
    media_kind: kind,
  };
}

/**
 * Cached PER-KIND metric handles for the media path.
 *
 * Constructed ONCE per kind at pipeline setup. The hot path calls the methods
 * below and never names a metric or builds a label object.
 */
export class MediaKindMetrics {
  readonly #sink: MetricsSink | undefined;
  readonly #kind: MediaKindLabel;
  /** Resolved once. Every emission below spreads THIS, never any other set. */
  readonly #base: MetricLabels;

  constructor(identity: MediaMetricIdentity, kind: MediaKindLabel, sink: MetricsSink | undefined) {
    this.#kind = kind;
    this.#base = mediaMetricLabels(identity, kind);
    this.#sink = sink;
  }

  /** The kind every series of this handle carries. */
  get kind(): MediaKindLabel {
    return this.#kind;
  }

  /** The base label set, exposed for assertion in tests. Never mutated. */
  get labels(): MetricLabels {
    return this.#base;
  }

  // ---------------------------------------------------------------- egress ---

  /** A frame left the device. The denominator for the send-drop ratio. */
  frameSent(): void {
    this.#sink?.counter('dt_client_media_frames_sent_total', this.#base);
  }

  /**
   * A frame was dropped on the send side.
   *
   * ADR-0036 §11 calls this the drop that matters most, because it happens in
   * the sender and MH structurally cannot observe it. WebTransport exposes no
   * send-side drop event, so the SDK keeps the transport queue shallow, owns a
   * bounded queue above it, makes the decision there, and counts it here —
   * making the drop observable BY CONSTRUCTION.
   */
  sendDropped(reason: MediaSendDropReason): void {
    this.#sink?.counter('dt_client_media_send_dropped_total', { ...this.#base, reason });
  }

  /** Current depth of the bounded egress queue, in frames. */
  sendQueueDepth(depth: number): void {
    this.#sink?.gauge('dt_client_media_send_queue_depth', this.#base, depth);
  }

  // --------------------------------------------------------------- ingress ---

  /**
   * A frame arrived, counted at the PARSE boundary before any verification, so
   * `received = accepted + sum(drops by reason)` holds per kind. Audio: one per
   * datagram (a datagram is one frame). Video (story 3 C-B): one per frame at
   * its FIRST BYTE on a group stream, so a partial frame cut by a reset or FIN
   * is received and then dropped `stream_partial_reset`. The video identity
   * excludes `VIDEO_POST_DECODE_REJECT_REASONS` (`frame/rejectReason.ts`).
   */
  frameReceived(): void {
    this.#sink?.counter('dt_client_media_frames_received_total', this.#base);
  }

  /**
   * A frame was dropped on the receive path.
   *
   * `reason` is `FrameRejectedError.rejectReason` PASSED THROUGH VERBATIM. There
   * is no mapping table, no `decode_reject` bucket, no layer-based collapse and
   * no `other`: the eight structural codec tokens are emitted INDIVIDUALLY,
   * because collapsing them destroys the only lever that detects a
   * version-skewed rollback (`unknown_version` staying individually visible) and
   * breaks `sum by(reason)` comparability with MH.
   *
   * The parameter's type is `RejectReason`, whose token list is set-equality
   * pinned in both directions against
   * `proto/test-vectors/frame-v3.vectors.json` — so the spellings are derived,
   * never retyped here.
   */
  frameDropped(reason: RejectReason): void {
    this.#sink?.counter('dt_client_media_frames_dropped_total', { ...this.#base, reason });
  }

  /**
   * A frame was opened and handed to the audio decoder.
   *
   * NAMED `accepted`, NOT `played`, and the distinction is load-bearing. A frame
   * handed to a decoder is not played: the decoder can error, the output can be
   * discarded, the audio context can be suspended. `received = accepted + sum(
   * drops by reason)` is a RECEIVE-PATH ACCOUNTING IDENTITY — its job is to
   * prove that no drop path fails to count itself — and it says nothing about
   * audibility. All fifteen dropping reject reasons fire at or before the
   * crypto/parse boundary, so the identity is exact THERE by construction; at
   * the playback boundary it would be false, because a frame lost between
   * handoff and audible decrements nothing on the right-hand side.
   *
   * ADR-0036 / R-25 prose spells this term "played"; the metrics catalog
   * deliberately supersedes that spelling. The accepted-to-audible segment is
   * covered only partially, by {@link decoderError}.
   */
  frameAccepted(): void {
    this.#sink?.counter('dt_client_media_frames_accepted_total', this.#base);
  }

  /**
   * A non-dropping wrapped-key outcome. The frame was ACCEPTED; this is not a
   * drop and must never be counted as one.
   */
  wrapOutcome(outcome: ReportableWrapOutcome): void {
    this.#sink?.counter('dt_client_media_key_wrap_outcomes_total', {
      ...this.#base,
      outcome,
    });
  }

  /**
   * Frames MISSING between MH's egress and our ingress, counted by the SIZE of
   * each gap in the relay hop sequence — a counter of missing numbers, not of
   * gap events, so it is comparable against `frames_received_total` as a loss
   * rate.
   *
   * This is the far-end compensating control for a blind spot MH cannot close:
   * quinn evicts datagrams from its own send buffer silently, emitting only a
   * trace-level log and incrementing no statistic, so under congestion severe
   * enough to saturate quinn's buffer but not MH's application queue, MH's
   * egress-overflow counter READS FLAT AT EXACTLY THE MOMENT LOSS IS WORST.
   * Only the receiving end can see it.
   *
   * OVER-COUNTS TRUE LOSS BY EXACTLY THE REORDER COUNT: QUIC datagrams are
   * unordered, so a reordered frame first opens a gap and then arrives late. The
   * honest estimate is `gap_frames - reorder`. See {@link downlinkReorder}.
   */
  downlinkGapFrames(missing: number): void {
    this.#sink?.counter('dt_client_media_downlink_gap_frames_total', this.#base, missing);
  }

  /** A datagram arrived below the running hop-sequence high-water mark. */
  downlinkReorder(): void {
    this.#sink?.counter('dt_client_media_downlink_reorder_total', this.#base);
  }

  /**
   * A frame arrived on a relay `stream_id` this client never declared.
   *
   * VIDEO (story 3 C-B, lead ruling E2) differs: there the relay id SELECTS the
   * slot pipeline, so a group stream naming an undeclared or non-video slot is
   * aborted and its frame counted `stream_slot_mismatch`; this diagnostic is
   * bumped beside it so the cause stays distinguishable. The audio text below
   * describes the datagram path.
   *
   * The relay region is UNAUTHENTICATED — a media handler writes `stream_id`
   * freely and nobody signs it — so it is validated against the declared slot
   * set before ANY receiver state is created for it. The frame itself is still
   * verified, attributed from its own key id, and played; only the hop-sequence
   * bookkeeping is withheld. No frame reject token is minted for this, because
   * the frame is NOT DROPPED — it plays — so it has no place in a drop
   * vocabulary; and the condition is not a decode failure either.
   */
  undeclaredStreamId(): void {
    this.#sink?.counter('dt_client_media_undeclared_stream_id_total', this.#base);
  }

  /**
   * An audio decoder raised its error callback.
   *
   * COUNTING POINT: one decoder per ACTIVE SENDER (story 2), event-once per
   * decoder INSTANCE — so one fault class can advance this up to N times, and
   * the magnitude is not comparable with the one-decoder-per-client history.
   *
   * BOUNDED / EVENT-DRIVEN, never per frame: a decoder erroring on every frame
   * must not become per-frame telemetry, which is why a faulted sender's decoder
   * is replaced at most once per `ingress.decoderRestartBackoffMs`. No `reason` label — `AudioDecoder`
   * gives no bounded one, and an unbounded label here would be the cardinality
   * hazard §11 exists to prevent.
   *
   * This is the only counter covering the accepted-to-audible segment, which the
   * receive-path identity deliberately does not reach.
   */
  decoderError(): void {
    this.#sink?.counter('dt_client_media_decoder_errors_total', this.#base);
  }

  // ------------------------------------------------------------- lifecycle ---

  /** A client-mute transition. Not a drop; see {@link MEDIA_SEND_DROP_REASONS}. */
  muteTransition(action: MediaMuteAction): void {
    this.#sink?.counter('dt_client_media_mute_transitions_total', { ...this.#base, action });
  }

  /**
   * A frame was evicted from a decode lane's bounded pending queue.
   *
   * ---------------------------------------------------------------------------
   * THIS IS A POST-ACCEPT LOSS AND MUST NEVER BE A `frames_dropped_total` REASON
   * ---------------------------------------------------------------------------
   *
   * It measures the ACCEPTED -> AUDIBLE segment, downstream of the accounting
   * boundary: the frame it counts was already counted on
   * `dt_client_media_frames_accepted_total`. A counter named `..._dropped_total`
   * one method from {@link frameDropped} reads like a reason someone forgot to
   * add — and "fixing" that would put one frame on BOTH sides of
   * `received = accepted + sum(drops by reason)`, breaking it silently and only
   * in aggregate. That is exactly the failure `wrap_key_id_mismatch`'s
   * `drops_frame: false` exists to prevent. One increment per evicted frame.
   *
   * VIDEO (story 3 C-B) counts every POST-ACCEPT loss before render here, one
   * per frame: jitter-buffer overflow or max-hold discards, frames discarded
   * while a slot awaits a keyframe, superseded-source trailing frames after a
   * switch, unmatched decoder output, and held frames the never-render guard
   * rejects. `decode_over_bound` is the one post-accept loss that is a frame
   * drop reason instead (see `VIDEO_POST_DECODE_REJECT_REASONS`).
   */
  decodeQueueDropped(): void {
    this.#sink?.counter('dt_client_media_decode_queue_dropped_total', this.#base);
  }

  /**
   * `count` ACTIVE receive assignments decoded nothing in the last export
   * interval (unit: assignment-intervals). `0` is a legal call — the
   * zero-initialisation at pipeline start, so `rate()` has a series before the
   * first deficit. No sender, slot or mode dimension, ever.
   */
  receiveSourceDeficit(count: number): void {
    this.#sink?.counter('dt_client_media_receive_source_deficit_total', this.#base, count);
  }

  /**
   * Capture for THIS kind failed (story 3 R-19, R-30): a start or device swap
   * that could not acquire the device (the classified `CaptureFailure`), or a
   * track that ended on its own mid-call (`ended`). Lazily created — no
   * zero-init: an absent series means no failures yet. No device, sender or
   * slot dimension.
   */
  captureFailure(outcome: MediaCaptureFailureOutcome): void {
    this.#sink?.counter('dt_client_media_capture_failures_total', { ...this.#base, outcome });
  }

  /**
   * An MH `KeyframeRequest` for a stream of THIS kind, by what the publisher did
   * with it. Never the stream number or GoP counter. Not zero-initialised (see
   * {@link MEDIA_KEYFRAME_REQUEST_OUTCOMES}).
   */
  keyframeRequestReceived(outcome: MediaKeyframePerKindOutcome): void {
    this.#sink?.counter('dt_client_media_keyframe_requests_received_total', {
      ...this.#base,
      outcome,
    });
  }

  // ---------------------------------------------------------- video send ---

  /**
   * The publisher produced a keyframe on a stream of THIS kind, by cause
   * ({@link MEDIA_KEYFRAME_SOURCES}). Counted ONCE per keyframe at the gate's
   * decision; a merged deferral counts under its creating cause. `count` 0 is
   * the zero-initialisation when the stream is first directed. Never the GoP
   * value, stream number or timing.
   */
  keyframeProduced(source: MediaKeyframeSource, count: 0 | 1 = 1): void {
    this.#sink?.counter(
      'dt_client_media_keyframes_produced_total',
      { ...this.#base, source },
      count,
    );
  }

  /**
   * The publisher RESET one target's current group stream because that target's
   * backlog crossed its cap (story 3 R-12) — a PUBLISHER-DECIDED abandonment.
   * NOT counted here: a peer stop / refused open / stalled open — those are on
   * {@link sendGroupStopped}. `count` 0 is the
   * zero-initialisation when the stream is first directed. No depth or size.
   */
  sendGroupAbandoned(count: 0 | 1 = 1): void {
    this.#sink?.counter('dt_client_media_send_groups_abandoned_total', this.#base, count);
  }

  /**
   * A target went dark for the PEER's reasons (`mh_stopped`), once per
   * transition into that state, by {@link MEDIA_SEND_GROUP_STOP_OUTCOMES}. No
   * target or handler dimension. `count` 0 is the zero-initialisation at first
   * direction.
   */
  sendGroupStopped(outcome: MediaSendGroupStopOutcome, count: 0 | 1 = 1): void {
    this.#sink?.counter(
      'dt_client_media_send_groups_stopped_total',
      { ...this.#base, outcome },
      count,
    );
  }

  /**
   * A CAPTURED frame was dropped BEFORE encode because the encoder's queue was
   * over `media.egress.videoMaxEncodeQueue` (pre-encode, per captured frame, not
   * per lane; never part of the send-drop ratio). Audio has no pre-encode bound
   * and never emits it. `count` 0 is the zero-initialisation at first direction.
   */
  encodeQueueDropped(count: 0 | 1 = 1): void {
    this.#sink?.counter('dt_client_media_encode_queue_dropped_total', this.#base, count);
  }

  /**
   * A directive's `min_keyframe_interval_ms` was below the compiled floor and
   * was clamped up to it — once per directive RECEIPT that clamps. A correct MC
   * never directs below its own floor, so a non-zero value is a drift or
   * hostile-controller signal. Lazily created; panel only, no alert.
   */
  keyframeMinIntervalClamped(): void {
    this.#sink?.counter('dt_client_media_keyframe_min_interval_clamped_total', this.#base);
  }

  /**
   * An encoder for THIS kind raised its error callback, or could not be built
   * for the directed configuration. Event-once per encoder instance; no
   * `reason` label (WebCodecs gives no bounded one). Lazily created.
   */
  encoderError(): void {
    this.#sink?.counter('dt_client_media_encoder_errors_total', this.#base);
  }

  /**
   * MC directed a stream of THIS kind with a required encoding field missing or
   * invalid (video: codec, width, height, frame rate, bitrate,
   * `min_keyframe_interval_ms`). The stream is refused and fails closed. No
   * field label: the WARN names the field. Lazily created.
   */
  sendDirectiveEncodingRefused(): void {
    this.#sink?.counter('dt_client_media_send_directive_encoding_refused_total', this.#base);
  }

  /**
   * MC's send directive omitted THIS published kind from a non-empty `streams`
   * list — a protocol fault (story 3 wire freeze: every published kind is
   * always listed; withdrawal is an empty target set, never removal). The kind
   * fails closed (sends nothing). `0` is the zero-initialisation at join for
   * each published kind; reads zero forever against a conforming MC.
   */
  sendDirectiveKindMissing(count: 0 | 1 = 1): void {
    this.#sink?.counter('dt_client_media_send_directive_kind_missing_total', this.#base, count);
  }

  // -------------------------------------------------------- video receive ---

  /**
   * A video slot sent its handler ONE recovery request for a loss event
   * (story 3 R-13), by cause ({@link MEDIA_RECOVERY_SOURCES}, label `source`). Counted on a SUCCESSFUL
   * control-stream write: a request replaced while queued, or a failed write,
   * is not counted (a failed write raises an event-once transport `mediaFault`).
   * `count` 0 is the zero-initialisation at receiver start.
   */
  recoveryRequestSent(source: MediaRecoverySource, count: 0 | 1 = 1): void {
    this.#sink?.counter(
      'dt_client_media_recovery_requests_sent_total',
      { ...this.#base, source },
      count,
    );
  }

  /**
   * An incoming group stream was refused at the concurrency cap
   * (`media.videoReceive.maxIncomingStreams`) — cancelled unread, never queued.
   * Unit: streams; OUTSIDE the frame accounting identity (no frame was parsed).
   * `count` 0 is the zero-initialisation at receiver start.
   */
  incomingStreamRefused(count: 0 | 1 = 1): void {
    this.#sink?.counter('dt_client_media_incoming_streams_refused_total', this.#base, count);
  }

  /**
   * One EXIT from the unfulfilled state (story 3 R-30): how long a slot MC
   * reports ACTIVE showed neither decodable video nor an avatar. Observed on
   * EVERY exit, teardown included. Buckets: {@link SLOT_UNFULFILLED_BUCKETS_MS}.
   */
  slotUnfulfilledDurationMs(ms: number): void {
    this.#sink?.histogram(
      'dt_client_media_slot_unfulfilled_duration_ms',
      this.#base,
      ms,
      SLOT_UNFULFILLED_BUCKETS_MS,
    );
  }

  /**
   * How many of this client's slots are CURRENTLY unfulfilled — the absolute
   * count, set on every transition, re-set on the export tick, 0 at teardown.
   * Last-writer-wins across browsers: panel only, never a fleet count.
   */
  slotsUnfulfilled(count: number): void {
    this.#sink?.gauge('dt_client_media_slots_unfulfilled', this.#base, count);
  }

  /**
   * Distinct verified senders with at least one DECODED output in the last
   * export interval (unit: source-intervals). Fed from decode activity, never
   * slot state. `0` is a legal call — the zero-initialisation at receiver start.
   */
  activeSourceIntervals(count: number): void {
    this.#sink?.counter('dt_client_media_active_source_intervals_total', this.#base, count);
  }

  /**
   * Media start to the first RENDERED frame of this kind, once per session.
   * Open objective, observed never gated. Buckets: {@link FIRST_RENDER_BUCKETS_MS}.
   */
  timeToFirstRenderMs(ms: number): void {
    this.#sink?.histogram(
      'dt_client_media_time_to_first_render_ms',
      this.#base,
      ms,
      FIRST_RENDER_BUCKETS_MS,
    );
  }

  /**
   * A slot's ACTIVE entry to its first rendered frame. An entry that exits
   * unrendered is NOT observed (survivorship; read with the unfulfilled
   * histogram). Open objective, observed never gated.
   */
  slotActiveToFirstRenderMs(ms: number): void {
    this.#sink?.histogram(
      'dt_client_media_slot_active_to_first_render_ms',
      this.#base,
      ms,
      FIRST_RENDER_BUCKETS_MS,
    );
  }

  /**
   * Time from media start to the first media frame OF THIS KIND arriving at the
   * wire. One sample per kind per session: a join that receives both audio and
   * video records one sample on each handle, never two on one.
   *
   * OBSERVED, NEVER GATED (ADR-0036 §10). No test asserts a wall-clock threshold
   * against it: asserting an end-to-end latency target on a local cluster
   * produces a permanent flake, and ADR-0028 forbids quarantining quality gates,
   * so the test would be deleted and the headline objective would end with zero
   * coverage.
   */
  timeToFirstMediaFrameMs(ms: number): void {
    this.#sink?.histogram('dt_client_time_to_first_media_frame_ms', this.#base, ms);
  }
}

/**
 * Cached SESSION-LEVEL (kind-free) metric handles.
 *
 * KEK lifecycle, roster key changes and the capture source are properties of the
 * session, not of a media kind: one KEK install serves every kind, so labelling
 * these per kind would count one event twice. This type takes no kind, so a
 * session series cannot be built with one.
 */
export class MediaSessionMetrics {
  readonly #sink: MetricsSink | undefined;
  /**
   * The kind-free label set, built BY ALLOW-LIST from two named strings.
   *
   * RESTATES three of {@link mediaMetricLabels}' four keys. A literal rather than
   * a derivation because `dt-guard client-metrics-export` resolves this bag only
   * from an object-literal assignment (and only through the two declared
   * wrappers below); Rule 1 does NOT compare the two bags. The drift guard is
   * `__tests__/mediaMetrics.test.ts`'s exact four-key (per-kind) and three-key
   * (session) set assertions.
   */
  readonly #sessionLabels: MetricLabels;

  constructor(identity: MediaMetricIdentity, sink: MetricsSink | undefined) {
    this.#sessionLabels = {
      client_version: identity.clientVersion,
      org_id: identity.orgId,
      key_custody: KEY_CUSTODY,
    };
    this.#sink = sink;
  }

  /** The kind-free label set, exposed for assertion in tests. Never mutated. */
  get labels(): MetricLabels {
    return this.#sessionLabels;
  }

  /** The ONE counter path for session series (a `dt-guard` declared wrapper). */
  #sessionCounter(name: string, labels: MetricLabels = {}, value?: number): void {
    this.#sink?.counter(name, { ...this.#sessionLabels, ...labels }, value);
  }

  /** The ONE gauge path for session series (a `dt-guard` declared wrapper). */
  #sessionGauge(name: string, labels: MetricLabels, value: number): void {
    this.#sink?.gauge(name, { ...this.#sessionLabels, ...labels }, value);
  }

  /** A meeting KEK arrived through the KEK-source seam. Never the key itself. */
  kekUpdate(source: MediaKekSource): void {
    this.#sessionCounter('dt_client_media_kek_updates_total', { source });
  }

  /**
   * The KEK holder would have retained more than one previous generation.
   *
   * A TRIPWIRE for a state no production path can reach: reads zero forever, and
   * alertable at `> 0`. It watches for the refactor most likely to remove its own
   * call site, which is why a test pins that the call site is on the live
   * install path. A counter rather than a gauge: with N browsers at one stream
   * identity a gauge is last-writer-wins and hides the rare value that matters.
   */
  kekRetentionViolation(): void {
    this.#sessionCounter('dt_client_media_kek_retention_violations_total');
  }

  /**
   * A KEK message's W produced a non-nominal retention window. One per message.
   *
   * `floor_substituted` is the one-version MC rollback detector — the only
   * cluster-visible evidence of it, since the WARN log reaches a browser console
   * no operator has.
   */
  kekRetentionAnomaly(outcome: MediaKekRetentionAnomaly): void {
    this.#sessionCounter('dt_client_media_kek_retention_anomalies_total', { outcome });
  }

  /** A delivered KEK was refused. Never the key, never its generation. */
  kekInstallRefused(outcome: MediaKekInstallRefusal): void {
    this.#sessionCounter('dt_client_media_kek_install_refusals_total', { outcome });
  }

  /**
   * An install demoted the previous current generation and RETAINED it.
   *
   * The opposite tripwire to {@link kekRetentionViolation}: that one watches for
   * retaining too MANY, a state no path reaches; this one is what goes flat when a
   * refactor retains NONE, whose only other symptom is an audio gap at every
   * rotation. Read against `kek_updates_total{source="kek_update"}`. Not
   * incremented by a first install or by an idempotent or refused one.
   */
  kekGenerationRetained(): void {
    this.#sessionCounter('dt_client_media_kek_generations_retained_total');
  }

  /**
   * A roster update changed a live sender's key: a `rebind` to different bytes,
   * or a `downgrade` to none. ONE increment per EVENT, not per purged key. No
   * sender dimension.
   */
  rosterKeyRebind(outcome: MediaRosterKeyChange): void {
    this.#sessionCounter('dt_client_media_roster_key_rebinds_total', { outcome });
  }

  /**
   * MC directed a frame header version this client cannot send, so the WHOLE
   * directive was refused (fail-closed: nothing sent). Kind-free: one refusal
   * per directive, never one per kind. `0` is the zero-initialisation at join;
   * reads zero forever against a conforming MC.
   */
  sendDirectiveHeaderVersionRefused(count: 0 | 1 = 1): void {
    this.#sessionCounter('dt_client_media_send_directive_header_version_refused_total', {}, count);
  }

  /**
   * An MH `KeyframeRequest` with no attributable kind: undecodable
   * (`malformed`), or naming a stream number that is no published kind's
   * directed stream (`not_sending`). Never the stream number or counter.
   * Zero-initialised at join (`0`) for both outcomes. Never summed by `outcome`
   * with `dt_client_media_keyframe_requests_received_total`.
   */
  keyframeRequestUnattributed(outcome: MediaKeyframeUnattributedOutcome, count: 0 | 1 = 1): void {
    this.#sessionCounter(
      'dt_client_media_keyframe_requests_unattributed_total',
      { outcome },
      count,
    );
  }

  /**
   * A handler's MH control stream ended while this client was not tearing down
   * (`reset`: MH reset it or the connection was lost; `finished`: MH finished
   * its send half). From then on that handler's keyframe requests are never
   * received. `0` is the zero-initialisation at join for both outcomes.
   */
  mhControlChannelEnded(outcome: MediaMhControlChannelEnding, count: 0 | 1 = 1): void {
    this.#sessionCounter('dt_client_media_mh_control_channel_ended_total', { outcome }, count);
  }

  /**
   * The active capture source, as a presence gauge: value `1` for the ACTIVE
   * mode only. Never a `0`-valued series for the other mode — the signal is the
   * series' presence, and a zero series would make presence lie.
   *
   * Re-set on every export-interval tick by the pipeline (the SDK exports DELTA,
   * and a last-value gauge is exported only for an interval it was recorded in);
   * the pipeline stops calling it at teardown so the series is not pinned.
   */
  captureSource(mode: MediaCaptureSourceMode): void {
    this.#sessionGauge('dt_client_media_capture_source', { mode }, 1);
  }
}

/**
 * The handles one media session uses: the kind-free session set and one
 * per-kind set per kind the session runs.
 *
 * Constructing a handle emits NOTHING (zero-initialisation is an explicit call
 * at join or at first direction), so the video handle exists from join while no
 * `media_kind="video"` series appears before video is directed.
 */
export class MediaMetrics {
  readonly session: MediaSessionMetrics;
  readonly audio: MediaKindMetrics;
  /**
   * The VIDEO handle (story 3 C-A2). Constructing it emits nothing: every video
   * series is either lazy or zero-initialised only when a video stream is first
   * DIRECTED (`lifecycle/VideoPublisher.ts`), so no `media_kind="video"` series
   * exists before video flows and C-B's read-back cannot pass vacuously.
   */
  readonly video: MediaKindMetrics;

  constructor(identity: MediaMetricIdentity, sink: MetricsSink | undefined) {
    this.session = new MediaSessionMetrics(identity, sink);
    this.audio = new MediaKindMetrics(identity, MEDIA_KIND_LABELS.Audio, sink);
    this.video = new MediaKindMetrics(identity, MEDIA_KIND_LABELS.Video, sink);
  }
}
