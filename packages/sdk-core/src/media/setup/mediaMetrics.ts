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
// So this is structural rather than disciplined: {@link MediaMetrics}'s
// constructor takes TWO STRINGS, not a `MetricLabels` bag. There is no spread to
// inherit from and nothing to prune, so `meeting_id_hash` is not merely absent —
// it is UNREPRESENTABLE at this boundary. A deletion-based helper
// (`{...labels, meeting_id_hash: undefined}`) would be one refactor away from
// re-inheriting a newly-added join label; an allow-list cannot be.
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
import type { RejectReason } from '../frame/rejectReason.js';
import type { WrapOutcome } from '../frame/receivePath.js';

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
 * (`proto/test-vectors/frame-v2.vectors.json`) and a both-directions
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
 * One value this story. KEK-push rotation adds `kek_update` when story 2 lands;
 * the vocabulary is declared as a map rather than a bare literal so that
 * addition is additive.
 */
export const MEDIA_KEK_SOURCES = {
  JoinResponse: 'join_response',
} as const;

/** One of {@link MEDIA_KEK_SOURCES}. */
export type MediaKekSource = (typeof MEDIA_KEK_SOURCES)[keyof typeof MEDIA_KEK_SOURCES];

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

/** The identity dimensions a media metric may carry. Two strings; nothing else. */
export interface MediaMetricIdentity {
  /** SDK build version. */
  readonly clientVersion: string;
  /** Organisation subdomain. Bounded cardinality; not a participant dimension. */
  readonly orgId: string;
}

/**
 * Build the media-path label set BY ALLOW-LIST.
 *
 * The complete set, and there is no path by which it grows: `client_version`,
 * `org_id`, `key_custody`. Callers pass two named strings, so no ambient label
 * bag can reach a media metric.
 */
export function mediaMetricLabels(identity: MediaMetricIdentity): MetricLabels {
  return {
    client_version: identity.clientVersion,
    org_id: identity.orgId,
    key_custody: KEY_CUSTODY,
  };
}

/**
 * Cached metric handles for the media path.
 *
 * Constructed ONCE at pipeline setup. The hot path calls the methods below and
 * never names a metric or builds a label object.
 */
export class MediaMetrics {
  readonly #sink: MetricsSink | undefined;
  /** Resolved once. Every emission below spreads THIS, never any other set. */
  readonly #base: MetricLabels;

  constructor(identity: MediaMetricIdentity, sink: MetricsSink | undefined) {
    this.#base = mediaMetricLabels(identity);
    this.#sink = sink;
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
   * A datagram arrived. Counted AT THE WIRE, before any parse or verification,
   * so `received = accepted + sum(drops by reason)` holds.
   *
   * NOTE FOR THE VIDEO STORY: this counting point must move from the transport
   * boundary to the PARSE boundary once several frames share one stream, and the
   * identity must be re-established there.
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
   * `proto/test-vectors/frame-v2.vectors.json` — so the spellings are derived,
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
   * The relay region is UNAUTHENTICATED — a media handler writes `stream_id`
   * freely and nobody signs it — so it is validated against the declared slot
   * set before ANY receiver state is created for it. The frame itself is still
   * verified, attributed from its own key id, and played; only the hop-sequence
   * bookkeeping is withheld. No frame reject token is minted for this: the
   * sixteen are frozen, and the condition is not a decode failure.
   */
  undeclaredStreamId(): void {
    this.#sink?.counter('dt_client_media_undeclared_stream_id_total', this.#base);
  }

  /**
   * The audio decoder raised its error callback.
   *
   * BOUNDED / EVENT-DRIVEN, never per frame: a decoder erroring on every frame
   * must not become per-frame telemetry. No `reason` label — `AudioDecoder`
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

  /** A meeting KEK arrived through the KEK-source seam. Never the key itself. */
  kekUpdate(source: MediaKekSource): void {
    this.#sink?.counter('dt_client_media_kek_updates_total', { ...this.#base, source });
  }

  /**
   * Time from media start to the first media frame arriving at the wire.
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
