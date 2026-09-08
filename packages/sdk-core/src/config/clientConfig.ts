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
  readonly keys: KeyRotationConfig;
  readonly egress: EgressConfig;
  readonly ingress: IngressConfig;
  readonly receiverState: ReceiverStateConfig;
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
    keys: {
      audioRotationPeriodMs: 60_000,
    },
    egress: {
      maxQueueFrames: 10,
      transportOutgoingHighWaterMarkFrames: 2,
      transportOutgoingMaxAgeMs: 500,
    },
    ingress: {
      transportIncomingHighWaterMarkFrames: 32,
      maxCachedIdentityKeys: 32,
    },
    receiverState: {
      maxReplayContextsPerSender: DEFAULT_REPLAY_CONTEXTS_PER_SENDER,
      replayWindowBits: DEFAULT_REPLAY_WINDOW_BITS,
      maxTransmitKeysPerSender: DEFAULT_TRANSMIT_KEYS_PER_SENDER,
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
  const { audio, keys, egress, ingress, receiverState } = media;

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
  const bufferedMs =
    (egress.maxQueueFrames + egress.transportOutgoingHighWaterMarkFrames) * audio.frameDurationMs;
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
}
