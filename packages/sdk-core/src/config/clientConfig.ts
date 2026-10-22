// File: packages/sdk-core/src/config/clientConfig.ts
//
// THE SINGLE CONFIGURATION POINT for the browser SDK's tunable values.
//
// CLAUDE.md: "Config over hardcoding" and "Single source of truth. When two
// places encode the same value, they will drift." Every value below is read from
// here at its use site; none is a literal where it is used.
//
// ---------------------------------------------------------------------------
// WHAT LIVES HERE AND WHAT DELIBERATELY DOES NOT
// ---------------------------------------------------------------------------
//
// HERE: values this client chooses for itself, and DEFAULTS for values a server
// may direct but has not.
//
// NOT HERE: values another component owns on the wire. In particular the audio
// bitrate — `EncodingParameters.max_bitrate_bps` on the send directive is the
// SSoT and configures the encoder whenever MC supplies it. `defaultBitrateBps`
// below is the FALLBACK for its absence, which is why it is spelled `default`
// and not `max`. There is deliberately NO client-side acceptance band: see that
// field.
//
// NOT HERE EITHER: the replay-window and transmit-key-cache bounds' DEFINITIONS.
// Those are exported constants in the frame layer, beside the classes that use
// them, and are imported below. The dependency direction is `config -> frame`,
// never the reverse: `receivePath.ts` is pure codec/crypto and must not depend on
// SDK configuration.

import {
  DEFAULT_REPLAY_CONTEXTS_PER_SENDER,
  DEFAULT_REPLAY_WINDOW_BITS,
  DEFAULT_TRANSMIT_KEYS_PER_SENDER,
} from '../media/frame/receivePath.js';
import {
  DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES,
  HOP_HALF_SPACE,
} from '../media/pipeline/hopSequenceMonitor.js';
import type { MediaKindToken } from '../media/mediaKind.js';

/** Opus `application` mode. Mirrors the WebCodecs Opus registration. */
export type OpusApplication = 'voip' | 'audio' | 'lowdelay';
/** Opus `signal` hint. Mirrors the WebCodecs Opus registration. */
export type OpusSignal = 'auto' | 'voice' | 'music';

/** Audio capture + encode parameters (ADR-0036 §3, §5). */
export interface AudioConfig {
  /** Capture and encode sample rate. */
  readonly sampleRateHz: number;
  /** Channel count. Mono: this is voice, not music. */
  readonly channels: number;
  /**
   * Encoded frame duration.
   *
   * ANCHOR (DRY): this value is encoded in three places and nothing enforces
   * agreement, because no shared home is reachable — a browser SDK cannot import
   * a Rust `const` and cannot read the ConfigMap, and
   * `proto/dark_tower/signaling/v1/signaling.proto` states that audio ignores
   * `EncodingParameters.frame_rate`, so there is no wire carrier either. The
   * siblings, by file and identifier:
   *   * `crates/mc-service`'s `MC_AUDIO_FRAME_RATE_HZ = 50` (the reciprocal unit)
   *   * `crates/mh-service`'s `AUDIO_FRAME_DURATION_MS = 20`
   * Moving to ADR-0036 §3's 40 ms signature-overhead mitigation is a COORDINATED
   * change across all three, not a unilateral one. Tracked in `docs/TODO.md`
   * under the MC-bitrate / MH-datagram-buffer entry.
   */
  readonly frameDurationMs: number;
  /**
   * Bitrate used when the send directive does not supply one.
   *
   * NAMED `default`, NOT `max`, AND THAT IS THE POINT. The send directive's
   * `EncodingParameters.max_bitrate_bps` is the wire SSoT for this value and
   * configures the encoder whenever present; this is the fallback for its
   * absence only.
   *
   * THERE IS NO CLIENT-SIDE ACCEPTANCE BAND, deliberately. `MC_AUDIO_MAX_BITRATE_BPS`
   * is an operator-settable env var that MC validates at config LOAD against a
   * code-owned band (`crates/mc-service/src/config.rs`; rejection pinned at
   * `config.rs:858` for `31999` and `48001`), and MC REFUSES TO START on an
   * out-of-band value. That control is unskippable at process startup, which is
   * strictly stronger than "MC validates". A mirrored band here would be a second
   * implementation of a rule that already has one home, and its distinctive
   * behaviour would be hard-failing every fielded SDK the first time an operator
   * legitimately raised the ceiling — fail-closed against a correctly-configured
   * control plane.
   *
   * `AudioEncoder.configure()` is NOT the control that makes this safe. It bounds
   * PLATFORM CAPABILITY, not policy sanity: WebCodecs will configure Opus at
   * 512 kbps without complaint. If MC's config-load band check is ever relaxed or
   * moved to data, the client has no bound at all — which is exactly what this
   * paragraph exists to make discoverable.
   *
   * ANCHOR (DRY): 32000 is also `crates/mh-service`'s
   * `SUPPORTED_AUDIO_BITRATE_FLOOR_BPS`, against which MH sizes
   * `NOMINAL_AUDIO_FRAME_BYTES` and therefore its operator-facing "N frames =
   * M ms" datagram-buffer budget. See `docs/TODO.md`: under `bitrateMode:
   * 'variable'` this is a TARGET, not a floor, so MH's sizing is average-case
   * rather than worst-case. That residual is recorded, not fixed here.
   */
  readonly defaultBitrateBps: number;
  /** Opus complexity. 10 = highest quality per bit, highest CPU. */
  readonly opusComplexity: number;
  /** Opus application mode. `voip` for human speech. */
  readonly opusApplication: OpusApplication;
  /** Opus signal hint. `voice`, never `music` — no music tuning (ADR-0036 §5). */
  readonly opusSignal: OpusSignal;
  /**
   * Discontinuous transmission.
   *
   * FALSE, and not merely as a default. ADR-0036 §5 makes mute an OUT-OF-BAND
   * signal precisely so that absence of frames is never itself a signal; DTX
   * reintroduces exactly that inference. It also sharpens §11's accepted
   * metadata leak: with DTX, per-frame size and timing at MH reconstruct who
   * spoke, in what order, for how long.
   */
  readonly opusUseDtx: boolean;
  /** In-band FEC. Off: forward error correction is story 8 tuning. */
  readonly opusUseInbandFec: boolean;
  /** Expected packet loss percentage fed to the encoder. 0 while FEC is off. */
  readonly opusPacketLossPerc: number;
}

/** Transmit-key generation and rotation (ADR-0036 §4). */
export interface KeyRotationConfig {
  /**
   * `T` from ADR-0036 §4's rotation table ("senders rotate transmit keys on every
   * video group and every T for audio, because rotation is free").
   *
   * MEASURED FROM PIPELINE START, so each client's rotation phase is set by when
   * it joined and the fleet is naturally desynchronised. Deliberately NOT aligned
   * to a wall-clock boundary, which would make rotation a fleet-synchronised
   * event — the same hazard §11's resume-jitter requirement exists for.
   *
   * Floored at {@link MIN_AUDIO_ROTATION_PERIOD_MS}. This is NOT the only thing
   * that triggers a rotation — resume-from-empty and unmute do too — so the
   * floor bounds the periodic half of the retirement deferral in
   * `media/lifecycle/transmitKeys.ts` and not the event-driven half.
   */
  readonly audioRotationPeriodMs: number;
  /**
   * `T` for VIDEO (story 3 R-14): the video stream rotates the session-wide
   * transmit-key generation every T, measured from video publisher start, in
   * addition to every group start and every KEK receipt. Separate from
   * {@link audioRotationPeriodMs} so the two kinds' periods are tuned
   * independently, with its own floor {@link MIN_VIDEO_ROTATION_PERIOD_MS}.
   * Neither T forces a keyframe: the new key rides the next
   * {@link videoKeyBearingFrames} frames instead.
   */
  readonly videoRotationPeriodMs: number;
  /**
   * N (story 3 R-14): how many consecutive video frames after ANY key change
   * (T, group start, KEK receipt — whatever the trigger) carry the wrapped
   * transmit key. Key-bearing frames are never discardable. Bounded
   * `1..`{@link MAX_VIDEO_KEY_BEARING_FRAMES}: 0 would make a mid-group rotation
   * undecryptable for a receiver that missed the group start, and a large N is
   * pure overhead on every frame.
   */
  readonly videoKeyBearingFrames: number;
}

/** Bounded egress queue + the transport knobs beneath it (ADR-0036 §1, §11). */
export interface EgressConfig {
  /**
   * Application egress queue bound, IN FRAMES OF AUDIO.
   *
   * ADR-0036 §1 requires this unit explicitly ("Express and document this in
   * frames of audio, not bytes") and the reason is identical on the client: N
   * frames x 20 ms is the added-latency ceiling an operator can reason about.
   * 10 frames = 200 ms.
   */
  readonly maxQueueFrames: number;
  /**
   * `WebTransportDatagramDuplexStream.outgoingHighWaterMark`, in datagrams —
   * which for audio is already frames. 2 frames = 40 ms.
   *
   * ADR-0036 §11: the SDK "keeps the transport queue shallow, owns a bounded
   * queue above it, makes the drop decision there, and counts it". A drop inside
   * the UA's queue is uncountable by us AND structurally invisible to MH, which
   * is the one drop class §11 says matters most.
   *
   * MUST be strictly less than {@link maxQueueFrames}. Asserted at setup — see
   * `validateClientConfig`. Not DERIVED from it: 200 ms of app-side buffering
   * tolerance and a 40 ms transport backpressure threshold are independently
   * motivated, so a ratio would be a false single source of truth. ADR-0036 §11
   * names startup validation as the sanctioned fallback where derivation is
   * impossible.
   */
  readonly transportOutgoingHighWaterMarkFrames: number;
  /**
   * `WebTransportDatagramDuplexStream.outgoingMaxAge`, in milliseconds.
   *
   * A BACKSTOP, NOT A COMPETITOR. UA age-discard is invisible to us and
   * structurally invisible to MH, so it must sit clear of the latency the app
   * queue plus transport queue can legitimately hold together
   * (`maxQueueFrames + transportOutgoingHighWaterMarkFrames` frames). At a value
   * close to that sum it becomes a routine uncountable drop path competing with
   * the app queue for the drop decision — reintroducing precisely the drop class
   * the high-water mark above is set to eliminate.
   *
   * Omitting it does not help: the attribute then takes a UA-chosen default, and
   * "the default being adequate is not the same as the default being chosen"
   * (ADR-0036 §1). Asserted at setup.
   *
   * This path remains UNCOUNTABLE by construction and is recorded as such for the
   * media-datagram-drop runbook scenario, alongside a reaped NAT binding and
   * stale MH policy.
   */
  readonly transportOutgoingMaxAgeMs: number;
  /**
   * Per-TARGET video backlog cap, in FRAMES (story 3 R-12). A target whose
   * unsent backlog for its current group would exceed this has that group
   * stream RESET (never finished) and receives nothing until the next group —
   * the publisher-decided abandonment. Bounds memory per target together with
   * {@link videoMaxBacklogBytes}; whichever trips first wins.
   */
  readonly videoMaxBacklogFrames: number;
  /** Per-TARGET video backlog cap, in BYTES of sealed frames. See {@link videoMaxBacklogFrames}. */
  readonly videoMaxBacklogBytes: number;
  /**
   * Pre-encode bound (story 3 C-A2, ops F3): a captured video frame is DROPPED
   * (closed, counted on `dt_client_media_encode_queue_dropped_total`) when the
   * encoder already holds more than this many frames, so a CPU-starved or
   * throttled tab sheds frames instead of growing memory and latency without
   * bound. A pending keyframe carries to the next encoded frame.
   */
  readonly videoMaxEncodeQueue: number;
}

/**
 * Video capability probe (story 3 R-8). NOT an encoding default — the SDK has
 * no client default for any video encoding value, and every encoder is
 * configured from (and re-probed at) MC's DIRECTED `EncodingParameters`. This
 * shape exists only to decide, BEFORE the join, whether `CODEC_VP8` may be
 * declared in `supported_codecs`, since the join precedes any directive. Its
 * values are the production `media-codec-profile` (VP8 640x360, 30 fps,
 * 800 kbps).
 */
export interface VideoCapabilityProbeConfig {
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  readonly bitrateBps: number;
}

/** Video-path configuration (story 3). */
export interface VideoConfig {
  readonly capabilityProbe: VideoCapabilityProbeConfig;
}

/** Receive-path bounds. */
export interface IngressConfig {
  /** `WebTransportDatagramDuplexStream.incomingHighWaterMark`, in datagrams. */
  readonly transportIncomingHighWaterMarkFrames: number;
  /**
   * Bound on cached sender identity `CryptoKey`s.
   *
   * Per-sender bounded for the same reason as the replay window and the
   * transmit-key cache: roster membership's bar is a meeting link, so anything
   * unbounded and keyed by `sender_id` is attacker-influenced memory growth.
   */
  readonly maxCachedIdentityKeys: number;
  /**
   * A downlink hop-sequence backward jump DEEPER than this many frames is a
   * publisher counter restart (a reconnecting publisher gets a fresh MH
   * forwarder whose counter starts at 0), not a reorder. See
   * `media/pipeline/hopSequenceMonitor.ts`. Must exceed any plausible datagram
   * reorder depth; must be below `HOP_HALF_SPACE` (the serial-arithmetic
   * half-space, imported from the monitor rather than restated here).
   */
  readonly hopRestartBackwardJumpFrames: number;
  /**
   * Bound on concurrent per-sender decode lanes (one `AudioDecoderSeam` each).
   *
   * Lanes exist only for senders in the MC-stated slot-assignment set, so in
   * practice the count is bounded by this client's declared slots; this is the
   * independent memory bound behind that, for the same reason every other
   * per-sender map here has one. Its OWN bound, deliberately not
   * {@link maxCachedIdentityKeys} or `maxTransmitKeysPerSender`: how many
   * decoders to run and how many keys to cache are unrelated decisions, and
   * collapsing them would be a false single source of truth.
   *
   * MUST be at least {@link ReceiveConfig.audioSlots} (N): N declared slots can
   * be filled by N distinct senders, each needing a lane. Checked by
   * {@link validateMediaConfig}; that relation is the client-side ceiling on N.
   */
  readonly maxDecodeLanes: number;
  /**
   * Minimum interval between replacements of ONE sender's faulted decoder.
   *
   * A decoder fault replaces only that sender's decoder (R-5). Without a bound
   * a roster member sending frames that fault the decoder every time would turn
   * `dt_client_media_decoder_errors_total` into per-frame telemetry and churn a
   * decoder per frame; this makes a replacement at most one per interval per
   * sender, and the lane count bounds the senders.
   */
  readonly decoderRestartBackoffMs: number;
  /**
   * Frames a decode lane holds while its decoder is being created or replaced
   * (the decoder factory is async). Overflow evicts the OLDEST and counts each
   * eviction on `dt_client_media_decode_queue_dropped_total` — a post-accept
   * loss, outside the receive-path identity by construction.
   */
  readonly decoderPendingFrames: number;
}

/** Replay-window and transmit-key-cache bounds. */
export interface ReceiverStateConfig {
  /**
   * Tracked `(stream, generation)` replay contexts per sender.
   *
   * NOT {@link maxTransmitKeysPerSender}. The two agree today by coincidence and
   * are independent tuning decisions — how many contexts to track for duplicate
   * detection versus how many unwrapped keys to hold. Collapsing them into one
   * shared constant would be a false SSoT that silently couples them.
   */
  readonly maxReplayContextsPerSender: number;
  /** Sliding duplicate-bitmap width. Unrelated to either bound above. */
  readonly replayWindowBits: number;
  /**
   * Cached unwrapped transmit keys per sender.
   *
   * NOT {@link maxReplayContextsPerSender} — see that field.
   */
  readonly maxTransmitKeysPerSender: number;
}

/**
 * What this client asks to RECEIVE (ADR-0036 §6; story 2 R-1, R-23; story 3
 * R-1, R-23).
 *
 * N audio and M video receive slots, configured INDEPENDENTLY; either may be 0
 * (story 3 R-1). Declared at join as audio slot ids `0..N-1` then video ids
 * `N..N+M-1` (`session/mediaSelection.ts:receiveDeclaration`, the one producer
 * of slot ids).
 *
 * N AND M ARE REQUESTS. MC caps each kind (`MC_MAX_AUDIO_RECEIVE_SLOTS`,
 * `MC_MAX_VIDEO_RECEIVE_SLOTS`, advertised on `JoinResponse.max_audio_receive_slots`
 * / `max_video_receive_slots`) and TRUNCATES an over-cap declaration: the excess
 * slots report `unavailable` on the wire, visibly, never a join failure. So the
 * SDK has no over-cap pre-check and never shrinks a declaration (the proto
 * forbids it). Story 2's whole-declaration rejection and its client counter
 * are retired.
 *
 * THE DEFAULTS ARE THE SDK's FALLBACK for an embedder that configures nothing,
 * NOT the demo topology: `scripts/dev-web.sh` always exports
 * `VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS`, which the web app
 * reads through {@link parseReceiveSlots}.
 *
 * NOT COVERED BY `dt-guard env-config`, which reads `infra/services/**` only:
 * the two variables are browser-side build knobs. The browser suite asserts the
 * effective values at runtime (`MeetingSession.receiveSlots`) instead.
 */
export interface ReceiveConfig {
  /**
   * N — audio receive slots. Each participant hears at most N others.
   * CLIENT CEILING: at most `ingress.maxDecodeLanes` (one decode lane per
   * assigned sender), checked by {@link validateMediaConfig}.
   */
  readonly audioSlots: number;
  /** M — video receive slots. Each participant sees at most M others. */
  readonly videoSlots: number;
}

/** The SDK's fallback N when an embedder configures none. See {@link ReceiveConfig}. */
export const DEFAULT_RECEIVE_AUDIO_SLOTS = 1;

/**
 * The SDK's fallback M when an embedder configures none: 0. This SDK has no
 * video receive path yet (story 3 task 14, C-B, adds it), so a default that
 * declared video slots would ask MC to direct publishers' video to a client
 * that cannot read it. Story 3 E6 ruling (lead, 2026-10-21).
 */
export const DEFAULT_RECEIVE_VIDEO_SLOTS = 0;

/**
 * Upper bound on N + M: slot ids are 16-bit (`ReceiveSlot.slot_id`, the same
 * value space as the relay-region `stream_id`), and ids run `0..N+M-1`.
 */
const SLOT_ID_SPACE = 0x1_0000;

/** A receive-slot kind. */
export type ReceiveSlotKind = MediaKindToken;

/**
 * THE provenance vocabulary for a client knob (R-23: absent vs defaulted is
 * visible): `configured` when the embedder SUPPLIED it, `default` when it was
 * absent and the SDK default applied — provenance, never a value comparison.
 */
export type ConfigValueSource = 'configured' | 'default';

/** Whether a slot count came from the embedder's configuration or the SDK default. */
export type ReceiveSlotsSource = ConfigValueSource;

/** A parsed slot count, with where it came from (R-23: absent vs defaulted is visible). */
export interface ParsedReceiveSlots {
  readonly count: number;
  readonly source: ReceiveSlotsSource;
}

/**
 * The RETIRED story-2 variable. Setting it is a loud failure, never an alias:
 * one N cannot say how many of each kind the operator meant.
 */
export const RETIRED_RECEIVE_SLOTS_VARIABLE = 'VITE_DT_RECEIVE_SLOTS';

/** The per-kind variables that replace {@link RETIRED_RECEIVE_SLOTS_VARIABLE}. */
export const RECEIVE_SLOTS_VARIABLES: Readonly<Record<ReceiveSlotKind, string>> = {
  audio: 'VITE_DT_AUDIO_RECEIVE_SLOTS',
  video: 'VITE_DT_VIDEO_RECEIVE_SLOTS',
};

/**
 * THE message for a set retired variable — one home, read by the web app's
 * config loader and asserted by its test. It names both replacements.
 */
export const RETIRED_RECEIVE_SLOTS_MESSAGE =
  `${RETIRED_RECEIVE_SLOTS_VARIABLE} is retired and is no longer read: set ` +
  `${RECEIVE_SLOTS_VARIABLES.audio} (audio receive slots N) and ` +
  `${RECEIVE_SLOTS_VARIABLES.video} (video receive slots M) instead; each is an integer >= 0`;

/**
 * The ONE accepted spelling of a slot count: a decimal integer >= 0 with no
 * sign, no leading zero (other than `0` itself), no whitespace and no fraction.
 * Identical to the class `scripts/dev-web.sh` accepts, so a value that passes
 * the launcher's preflight is read the same way in the browser.
 */
const RECEIVE_SLOTS_PATTERN = /^(0|[1-9][0-9]*)$/;

/**
 * Parse a configured slot count for `kind` (e.g. `VITE_DT_AUDIO_RECEIVE_SLOTS`).
 * THROWS; never falls back.
 *
 * `undefined` — the knob is ABSENT — yields that kind's SDK default with
 * `source: 'default'`. Anything present must match `^(0|[1-9][0-9]*)$`: `03`,
 * `3.0`, ` 3`, `-1`, the empty string and non-numeric values all throw a
 * {@link ClientConfigError}, because a silently substituted count is a
 * participant who hears or sees fewer people than the operator configured with
 * nothing saying so. Upper bounds are checked by {@link validateMediaConfig}.
 */
export function parseReceiveSlots(
  raw: string | undefined,
  kind: ReceiveSlotKind,
): ParsedReceiveSlots {
  if (raw === undefined) {
    return {
      count: kind === 'audio' ? DEFAULT_RECEIVE_AUDIO_SLOTS : DEFAULT_RECEIVE_VIDEO_SLOTS,
      source: 'default',
    };
  }
  if (!RECEIVE_SLOTS_PATTERN.test(raw) || !Number.isSafeInteger(Number(raw))) {
    throw new ClientConfigError(
      `media.receive.${kind}Slots`,
      `the ${kind} receive-slot count must be an integer >= 0 written without sign, leading ` +
        `zero, whitespace or fraction; got ${JSON.stringify(raw.slice(0, 32))}`,
    );
  }
  return { count: Number(raw), source: 'configured' };
}

/** Telemetry cadence. */
export interface TelemetryCadenceConfig {
  /**
   * OTel metric export interval.
   *
   * THIS IS GLOBAL, NOT MEDIA-ONLY, AND THAT IS DELIBERATE. The SDK has ONE
   * `MeterProvider` (ADR-0028 R-19/R-24), so this cadence applies to every
   * `dt_client_*` metric including the grandfathered ADR-0028 join-flow metrics.
   * Moving from the OTel JS default of 60 s to 10 s is therefore a ~6x
   * export-volume change with a blast radius wider than the media path. It was
   * checked against GC's telemetry proxy limits before landing:
   * `TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE = 60` per-`sub` per-pod GCRA, so ~1
   * export/min becomes ~6/min with comfortable headroom.
   *
   * Do NOT "scope" this by standing up a second `MeterProvider` — that breaks
   * R-19's single-provider requirement.
   *
   * 10 s is the client cadence observability documents; it is cited by config key
   * from `docs/observability/dashboard-conventions.md` rather than restated
   * there as a number.
   */
  readonly metricExportIntervalMs: number;
}

/** Media-path configuration. */
export interface MediaConfig {
  readonly audio: AudioConfig;
  readonly video: VideoConfig;
  readonly keys: KeyRotationConfig;
  readonly egress: EgressConfig;
  readonly ingress: IngressConfig;
  readonly receiverState: ReceiverStateConfig;
  readonly receive: ReceiveConfig;
}

/** The SDK's configurable surface. */
export interface ClientConfig {
  readonly media: MediaConfig;
  readonly telemetry: TelemetryCadenceConfig;
}

/**
 * Floor on {@link KeyRotationConfig.audioRotationPeriodMs}.
 *
 * NOT arbitrary, and not a round number chosen for looking sensible. It bounds
 * the periodic half of the transmit-key retirement deferral in
 * `media/lifecycle/transmitKeys.ts`: a retired key is overwritten at the NEXT
 * rotation, and the window that must not close early is one HKDF-extract — the
 * single `crypto.subtle.sign('HMAC', ...)` that reads the key material after an
 * `await importKey`, on the order of tens of microseconds.
 *
 * One second is four to five orders of magnitude above that, and is already far
 * below any plausible operational setting (the default is 60 s, and ADR-0036 §4
 * describes audio rotation as "every T" with T on the order of a minute). So the
 * floor rejects only values that were configuration errors, and it converts the
 * periodic half of that deferral's bound from an assumption into a validated
 * property.
 *
 * It does NOT bound the event-driven half — `rotate()` is also called on
 * resume-from-empty and on unmute, and that interval is caller-controlled. See
 * the comment at `TransmitKeyManager.rotate`, which states which half is
 * validated and which is not.
 */
export const MIN_AUDIO_ROTATION_PERIOD_MS = 1_000;

/**
 * Floor on {@link KeyRotationConfig.videoRotationPeriodMs}. Same derivation as
 * {@link MIN_AUDIO_ROTATION_PERIOD_MS} (the retirement deferral's periodic half
 * must exceed one HKDF-extract), kept as its own constant so the two kinds'
 * floors can move independently.
 */
export const MIN_VIDEO_ROTATION_PERIOD_MS = 1_000;

/** Upper bound on {@link KeyRotationConfig.videoKeyBearingFrames}. */
export const MAX_VIDEO_KEY_BEARING_FRAMES = 30;

/**
 * ANCHOR (DRY): the COMPILED keyframe floor, in milliseconds (story 3 R-11,
 * R-23). The publisher enforces max(directed `min_keyframe_interval_ms`, this)
 * between ANY two keyframes on a stream (requested or self-keyed), so neither
 * an absent nor a tiny directed value can raise its keyframe rate. A directed
 * value below it is clamped and counted on
 * `dt_client_media_keyframe_min_interval_clamped_total`.
 *
 * MUST EQUAL MC's `MIN_KEYFRAME_INTERVAL_MS_FLOOR` in
 * `crates/mc-service/src/media_signaling/directive.rs` (the lowest value MC will
 * direct from `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`). Pinned by
 * `scripts/guards/simple/validate-keyframe-floor-sync.sh`, which reads this
 * exact declaration line — keep it a single literal.
 */
export const KEYFRAME_MIN_INTERVAL_FLOOR_MS = 100;

/**
 * ANCHOR (DRY): the COMPILED keyframe-interval CEILING, in milliseconds. A
 * directed `min_keyframe_interval_ms` above it is REFUSED as an invalid encoding
 * field (`session/mediaWiring.ts:validateVideoEncoding`) — the wire field is a
 * `uint32`, and a value past a timer's range would otherwise turn a bounded wait
 * into a re-arm spin. MUST EQUAL MC's `MIN_KEYFRAME_INTERVAL_MS_CEILING` in
 * `crates/mc-service/src/media_signaling/directive.rs` (the top of
 * `MC_VIDEO_MIN_KEYFRAME_INTERVAL_MS`'s range); pinned by the same
 * `scripts/guards/simple/validate-keyframe-floor-sync.sh`.
 */
export const KEYFRAME_MIN_INTERVAL_CEILING_MS = 10_000;

// ---------------------------------------------------------------------------
// KEK PREVIOUS-GENERATION RETENTION (ADR-0036 §4, story 2 R-14)
// ---------------------------------------------------------------------------
//
// ANCHOR (DRY): the RULE — retention = min(W / 2, ceiling), a floor on a zero or
// absent W, floor-substituted and ceiling-clamped counted distinctly, never a
// hard failure — is stated ONCE, at the `JoinResponse.kek_rotation_debounce_seconds`
// field comment in `proto/dark_tower/signaling/v1/signaling.proto`. That field
// is the authority. What lives HERE is only what the field comment assigns to
// the client: the two numbers, and the one function that applies the rule.
// Nothing else in the tree may compute `W / 2`; call `deriveKekRetention`.
//
// These are CONSTANTS, NOT CONFIG FIELDS, and that is deliberate: the client
// never configures retention (it is derived from W, which MC carries). A knob
// would reopen exactly the "client-side value with a runtime consistency check"
// shape story 2 decided against.

/**
 * Retention used when W is zero or absent (a non-optional proto3 scalar, so the
 * two are one observable).
 *
 * The supported one-version MC rollback does not send the field, so this path is
 * EXPECTED during a rollback and must never hard-fail. Ten seconds is far above
 * any plausible frames-in-flight window and far below any sane W; it must exceed
 * the client's transmit-key re-wrap latency, which `validateMediaConfig`
 * asserts against the configured queues.
 */
export const KEK_RETENTION_FLOOR_MS = 10_000;

/**
 * Upper bound on how long a superseded KEK is held, whatever W says.
 *
 * Retention serves ONE purpose: opening frames already in flight when a
 * rotation lands — a latency-scale need of seconds, not a policy-scale one. At a
 * misconfigured W of a day, W/2 would hold a superseded KEK in client memory for
 * twelve hours to serve that; this bounds it (key minimisation against client
 * memory compromise). At MC's default W of 60 s, W/2 equals this ceiling exactly,
 * so nothing is clamped by default.
 *
 * CROSS-SERVICE COUPLING WITH NO GUARD AND NO SHARED HOME: if an operator raises
 * `MC_KEK_ROTATION_DEBOUNCE_SECONDS` so that W/2 exceeds this value,
 * `dt_client_media_kek_retention_anomalies_total{outcome="ceiling_clamped"}`
 * reads PERMANENTLY NON-ZERO on a correctly configured fleet. That is a
 * configuration state, not an incident, and must never be an alert input.
 */
export const KEK_RETENTION_CEILING_MS = 30_000;

/**
 * How a retention window was arrived at. `nominal` is the only unremarkable one;
 * the other three are counted, one increment per KEK message.
 */
export type KekRetentionOutcome =
  | 'nominal'
  /** W was zero or absent: an older MC (the supported rollback). */
  | 'floor_substituted'
  /** W/2 exceeded {@link KEK_RETENTION_CEILING_MS}: a configuration state. */
  | 'ceiling_clamped'
  /**
   * The derived retention does not exceed this client's transmit-key re-wrap
   * latency, so frames from senders that have not yet re-wrapped would drop.
   * Reachable only when W/2 is below T, since the floor is validated above T —
   * so the remedy is in MC (raise `MC_KEK_ROTATION_DEBOUNCE_SECONDS`), not here.
   */
  | 'below_rewrap_latency';

/** The derived window and how it was reached. */
export interface KekRetention {
  readonly retentionMs: number;
  readonly outcome: KekRetentionOutcome;
}

/**
 * THE ONE PLACE `min(W / 2, ceiling)` IS COMPUTED. See the section header.
 *
 * Both bounds only ever pull retention DOWN from W/2, which is what makes
 * "retention is shorter than W" structural rather than checked. A below-T result
 * is REPORTED but NOT raised: raising it could push retention to or past W,
 * breaking that structural property to paper over an MC misconfiguration.
 *
 * @param debounceSeconds W as carried on the wire (`uint32`; 0 means absent).
 * @param rewrapLatencyMs this client's T; see {@link transmitRewrapLatencyMs}.
 */
export function deriveKekRetention(debounceSeconds: number, rewrapLatencyMs: number): KekRetention {
  if (!Number.isFinite(debounceSeconds) || debounceSeconds <= 0) {
    return { retentionMs: KEK_RETENTION_FLOOR_MS, outcome: 'floor_substituted' };
  }
  const halfW = debounceSeconds * 500;
  if (halfW > KEK_RETENTION_CEILING_MS) {
    return { retentionMs: KEK_RETENTION_CEILING_MS, outcome: 'ceiling_clamped' };
  }
  if (halfW <= rewrapLatencyMs) {
    return { retentionMs: halfW, outcome: 'below_rewrap_latency' };
  }
  return { retentionMs: halfW, outcome: 'nominal' };
}

/**
 * The latency the two send-side queues can legitimately hold together, in ms.
 *
 * One home for `(maxQueueFrames + transportOutgoingHighWaterMarkFrames) *
 * frameDurationMs`: both the UA-age backstop check in `validateMediaConfig` and
 * {@link transmitRewrapLatencyMs} read it, so the two cannot drift.
 */
export function sendBufferedLatencyMs(media: MediaConfig): number {
  const { audio, egress } = media;
  return (
    (egress.maxQueueFrames + egress.transportOutgoingHighWaterMarkFrames) * audio.frameDurationMs
  );
}

/**
 * T: how long after a KEK update THIS client may still emit frames wrapped
 * under the previous KEK.
 *
 * On receipt of a new KEK the transmit-key manager rotates synchronously (R-13),
 * so the next encoded frame mints under the new KEK. What can still leave under
 * the old one is what is already queued — both send-side queues — plus the one
 * frame the encoder may be emitting. Derived from existing config; never 0.
 *
 * WHAT T DOES NOT COVER, so nobody reads "retention > T" as the whole guarantee:
 * it bounds only THIS client's queued frames. A peer that has not yet received
 * the `MeetingKekUpdate` keeps wrapping under the old KEK (cross-client push
 * delivery skew), and frames spend time on the network in flight. Those are
 * covered by the floor and the W-derived window, not by T.
 */
export function transmitRewrapLatencyMs(media: MediaConfig): number {
  return sendBufferedLatencyMs(media) + media.audio.frameDurationMs;
}

/**
 * The client cadence, exported on its own so `telemetryConfig.ts` reads ONE named
 * value rather than a literal at the reader.
 */
export const DEFAULT_METRIC_EXPORT_INTERVAL_MS = 10_000;

/** The default configuration. Every field is overridable by a caller. */
export const DEFAULT_CLIENT_CONFIG: ClientConfig = {
  media: {
    audio: {
      sampleRateHz: 48_000,
      channels: 1,
      frameDurationMs: 20,
      defaultBitrateBps: 32_000,
      opusComplexity: 10,
      opusApplication: 'voip',
      opusSignal: 'voice',
      opusUseDtx: false,
      opusUseInbandFec: false,
      opusPacketLossPerc: 0,
    },
    video: {
      capabilityProbe: { width: 640, height: 360, frameRateHz: 30, bitrateBps: 800_000 },
    },
    keys: {
      audioRotationPeriodMs: 60_000,
      videoRotationPeriodMs: 60_000,
      videoKeyBearingFrames: 3,
    },
    egress: {
      maxQueueFrames: 10,
      transportOutgoingHighWaterMarkFrames: 2,
      transportOutgoingMaxAgeMs: 500,
      // ~1 s of the production 800 kbps profile at 30 fps; a target that holds
      // more than this is not keeping up and is better reset than queued.
      videoMaxBacklogFrames: 30,
      videoMaxBacklogBytes: 131_072,
      videoMaxEncodeQueue: 2,
    },
    ingress: {
      transportIncomingHighWaterMarkFrames: 32,
      maxCachedIdentityKeys: 32,
      hopRestartBackwardJumpFrames: DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES,
      maxDecodeLanes: 32,
      decoderRestartBackoffMs: 1_000,
      decoderPendingFrames: 5,
    },
    receiverState: {
      maxReplayContextsPerSender: DEFAULT_REPLAY_CONTEXTS_PER_SENDER,
      replayWindowBits: DEFAULT_REPLAY_WINDOW_BITS,
      maxTransmitKeysPerSender: DEFAULT_TRANSMIT_KEYS_PER_SENDER,
    },
    receive: {
      audioSlots: DEFAULT_RECEIVE_AUDIO_SLOTS,
      videoSlots: DEFAULT_RECEIVE_VIDEO_SLOTS,
    },
  },
  telemetry: {
    metricExportIntervalMs: DEFAULT_METRIC_EXPORT_INTERVAL_MS,
  },
};

/** Thrown when a configured value is unusable. Never clamped, never warned. */
export class ClientConfigError extends Error {
  /** The offending key path, e.g. `media.egress.maxQueueFrames`. */
  readonly configKey: string;

  constructor(configKey: string, message: string) {
    super(message);
    this.name = 'ClientConfigError';
    this.configKey = configKey;
  }
}

function nonNegativeInteger(value: number, key: string): void {
  if (!Number.isFinite(value) || !Number.isInteger(value) || value < 0) {
    throw new ClientConfigError(key, `${key} must be an integer >= 0, got ${String(value)}`);
  }
}

function positiveInteger(value: number, key: string): void {
  if (!Number.isFinite(value) || !Number.isInteger(value) || value <= 0) {
    throw new ClientConfigError(key, `${key} must be a positive integer, got ${String(value)}`);
  }
}

/**
 * Validate a media configuration. THROWS; never clamps and never warns.
 *
 * Call this at pipeline setup, before any device is opened. Two of the checks
 * are relationships rather than ranges, and those are the load-bearing ones:
 *
 *   * `maxQueueFrames > transportOutgoingHighWaterMarkFrames` — if the pair
 *     inverts, every drop moves into the UA's queue where neither end can count
 *     it, while every counter we own reads zero. A control that applies but never
 *     fires.
 *   * `transportOutgoingMaxAgeMs` clear of the total latency both queues can hold
 *     — otherwise the UA's age-discard becomes a routine, uncountable competitor
 *     to the app queue rather than a backstop.
 */
export function validateMediaConfig(media: MediaConfig): void {
  const { audio, keys, egress, ingress, receiverState, receive } = media;

  positiveInteger(audio.sampleRateHz, 'media.audio.sampleRateHz');
  positiveInteger(audio.channels, 'media.audio.channels');
  positiveInteger(audio.frameDurationMs, 'media.audio.frameDurationMs');
  positiveInteger(audio.defaultBitrateBps, 'media.audio.defaultBitrateBps');
  positiveInteger(keys.audioRotationPeriodMs, 'media.keys.audioRotationPeriodMs');
  if (keys.audioRotationPeriodMs < MIN_AUDIO_ROTATION_PERIOD_MS) {
    throw new ClientConfigError(
      'media.keys.audioRotationPeriodMs',
      `media.keys.audioRotationPeriodMs (${keys.audioRotationPeriodMs}) must be at least ` +
        `${MIN_AUDIO_ROTATION_PERIOD_MS}ms: a transmit key superseded by a rotation is overwritten ` +
        `at the NEXT rotation, and a period below that floor could close that window while a frame ` +
        `is still reading the key as HMAC input — sealing it under zeros while its wrapped block ` +
        `announces the real key. That surfaces as decrypt_failed, whose triage points at the key ` +
        `schedule rather than at this setting`,
    );
  }
  positiveInteger(keys.videoRotationPeriodMs, 'media.keys.videoRotationPeriodMs');
  if (keys.videoRotationPeriodMs < MIN_VIDEO_ROTATION_PERIOD_MS) {
    throw new ClientConfigError(
      'media.keys.videoRotationPeriodMs',
      `media.keys.videoRotationPeriodMs (${keys.videoRotationPeriodMs}) must be at least ` +
        `${MIN_VIDEO_ROTATION_PERIOD_MS}ms, for the same retirement-window reason as the audio period`,
    );
  }
  positiveInteger(keys.videoKeyBearingFrames, 'media.keys.videoKeyBearingFrames');
  if (keys.videoKeyBearingFrames > MAX_VIDEO_KEY_BEARING_FRAMES) {
    throw new ClientConfigError(
      'media.keys.videoKeyBearingFrames',
      `media.keys.videoKeyBearingFrames must be 1..${MAX_VIDEO_KEY_BEARING_FRAMES}, ` +
        `got ${keys.videoKeyBearingFrames}`,
    );
  }
  positiveInteger(egress.videoMaxBacklogFrames, 'media.egress.videoMaxBacklogFrames');
  positiveInteger(egress.videoMaxBacklogBytes, 'media.egress.videoMaxBacklogBytes');
  positiveInteger(egress.videoMaxEncodeQueue, 'media.egress.videoMaxEncodeQueue');
  const probe = media.video.capabilityProbe;
  positiveInteger(probe.width, 'media.video.capabilityProbe.width');
  positiveInteger(probe.height, 'media.video.capabilityProbe.height');
  positiveInteger(probe.frameRateHz, 'media.video.capabilityProbe.frameRateHz');
  positiveInteger(probe.bitrateBps, 'media.video.capabilityProbe.bitrateBps');
  positiveInteger(egress.maxQueueFrames, 'media.egress.maxQueueFrames');
  positiveInteger(
    egress.transportOutgoingHighWaterMarkFrames,
    'media.egress.transportOutgoingHighWaterMarkFrames',
  );
  positiveInteger(egress.transportOutgoingMaxAgeMs, 'media.egress.transportOutgoingMaxAgeMs');
  positiveInteger(
    ingress.transportIncomingHighWaterMarkFrames,
    'media.ingress.transportIncomingHighWaterMarkFrames',
  );
  positiveInteger(ingress.maxCachedIdentityKeys, 'media.ingress.maxCachedIdentityKeys');
  positiveInteger(
    ingress.hopRestartBackwardJumpFrames,
    'media.ingress.hopRestartBackwardJumpFrames',
  );
  positiveInteger(ingress.maxDecodeLanes, 'media.ingress.maxDecodeLanes');
  positiveInteger(ingress.decoderRestartBackoffMs, 'media.ingress.decoderRestartBackoffMs');
  positiveInteger(ingress.decoderPendingFrames, 'media.ingress.decoderPendingFrames');
  nonNegativeInteger(receive.audioSlots, 'media.receive.audioSlots');
  nonNegativeInteger(receive.videoSlots, 'media.receive.videoSlots');
  if (receive.audioSlots + receive.videoSlots > SLOT_ID_SPACE) {
    throw new ClientConfigError(
      'media.receive.videoSlots',
      `media.receive.audioSlots + media.receive.videoSlots must not exceed ${SLOT_ID_SPACE}, ` +
        'the 16-bit slot id space',
    );
  }
  // The client-side ceiling on N: every declared slot can be filled by a
  // distinct sender, and each assigned sender needs its own decode lane.
  if (receive.audioSlots > ingress.maxDecodeLanes) {
    throw new ClientConfigError(
      'media.receive.audioSlots',
      `media.receive.audioSlots (${receive.audioSlots}) must not exceed ` +
        `media.ingress.maxDecodeLanes (${ingress.maxDecodeLanes}); otherwise an assigned sender ` +
        `would have no decoder and every one of its frames would be dropped`,
    );
  }
  if (ingress.hopRestartBackwardJumpFrames >= HOP_HALF_SPACE) {
    throw new ClientConfigError(
      'media.ingress.hopRestartBackwardJumpFrames',
      `media.ingress.hopRestartBackwardJumpFrames must be below ${HOP_HALF_SPACE}, ` +
        'the hop-sequence serial-arithmetic half-space',
    );
  }
  positiveInteger(
    receiverState.maxReplayContextsPerSender,
    'media.receiverState.maxReplayContextsPerSender',
  );
  positiveInteger(receiverState.replayWindowBits, 'media.receiverState.replayWindowBits');
  positiveInteger(
    receiverState.maxTransmitKeysPerSender,
    'media.receiverState.maxTransmitKeysPerSender',
  );

  if (audio.opusComplexity < 0 || audio.opusComplexity > 10) {
    throw new ClientConfigError(
      'media.audio.opusComplexity',
      `media.audio.opusComplexity must be 0..10, got ${audio.opusComplexity}`,
    );
  }
  if (audio.opusPacketLossPerc < 0 || audio.opusPacketLossPerc > 100) {
    throw new ClientConfigError(
      'media.audio.opusPacketLossPerc',
      `media.audio.opusPacketLossPerc must be 0..100, got ${audio.opusPacketLossPerc}`,
    );
  }

  // The application bound MUST trip before the transport ceiling (ADR-0036 §1,
  // §11). Inverted, every drop lands in the UA's queue: uncountable by us,
  // invisible to MH, and every counter we own reads zero.
  if (egress.maxQueueFrames <= egress.transportOutgoingHighWaterMarkFrames) {
    throw new ClientConfigError(
      'media.egress.maxQueueFrames',
      `media.egress.maxQueueFrames (${egress.maxQueueFrames}) must exceed ` +
        `media.egress.transportOutgoingHighWaterMarkFrames ` +
        `(${egress.transportOutgoingHighWaterMarkFrames}); otherwise the transport queue fills ` +
        `first and every send-side drop becomes uncountable by this client and invisible to the ` +
        `media handler`,
    );
  }

  // The UA age bound must be a BACKSTOP, not a competitor: it must exceed the
  // latency both queues can legitimately hold together.
  const bufferedMs = sendBufferedLatencyMs(media);
  if (egress.transportOutgoingMaxAgeMs <= bufferedMs) {
    throw new ClientConfigError(
      'media.egress.transportOutgoingMaxAgeMs',
      `media.egress.transportOutgoingMaxAgeMs (${egress.transportOutgoingMaxAgeMs}) must exceed ` +
        `the ${bufferedMs}ms both queues can legitimately hold ` +
        `((${egress.maxQueueFrames} + ${egress.transportOutgoingHighWaterMarkFrames}) frames x ` +
        `${audio.frameDurationMs}ms); at or below it, the user agent's silent age-discard becomes ` +
        `a routine drop path that neither this client nor the media handler can count`,
    );
  }

  // The retention FLOOR must exceed this client's own re-wrap latency, or a
  // floor-substituted client (an older MC, the supported rollback) would drop
  // its peers' in-flight frames at every rotation. The floor is a constant and T
  // is derived from the queues above, so a queue override can break it — which
  // is why this is checked here rather than asserted once in a test.
  const rewrapMs = transmitRewrapLatencyMs(media);
  if (KEK_RETENTION_FLOOR_MS <= rewrapMs) {
    throw new ClientConfigError(
      'media.egress.maxQueueFrames',
      `the send-side queues (${rewrapMs}ms of transmit-key re-wrap latency) must stay below the ` +
        `KEK retention floor (${KEK_RETENTION_FLOOR_MS}ms); otherwise a client running on the ` +
        `floor drops frames from senders that have not yet re-wrapped under a rotated KEK`,
    );
  }
}

/**
 * The video publish path's client knobs, as ONE log line (story 3 R-23), emitted
 * once at video publisher start. `source` is the PROVENANCE of the media
 * configuration (`configured` when the embedder supplied `mediaConfig`,
 * `default` when it was absent — the same rule as the slot counts,
 * `session/slotPlan.ts`); it is never inferred from a value. The keyframe floor
 * and ceiling are `compiled` (constants, not settings). Numbers and fixed
 * tokens only.
 *
 * TUNING / ROLLBACK: these are code-config — bundle constants, outside every
 * ConfigMap and the env-config guard. Changing one means rebuilding and
 * redeploying the web app (or the embedder's bundle); there is no runtime lever.
 */
export function videoConfigLogLine(media: MediaConfig, source: ConfigValueSource): string {
  const { keys, egress } = media;
  return (
    `media config (${source}): ` +
    `videoRotationPeriodMs=${keys.videoRotationPeriodMs}; ` +
    `videoKeyBearingFrames=${keys.videoKeyBearingFrames}; ` +
    `videoMaxBacklogFrames=${egress.videoMaxBacklogFrames}; ` +
    `videoMaxBacklogBytes=${egress.videoMaxBacklogBytes}; ` +
    `videoMaxEncodeQueue=${egress.videoMaxEncodeQueue}; ` +
    `keyframeMinIntervalFloorMs=${KEYFRAME_MIN_INTERVAL_FLOOR_MS} (compiled); ` +
    `keyframeMinIntervalCeilingMs=${KEYFRAME_MIN_INTERVAL_CEILING_MS} (compiled)`
  );
}
