// File: packages/sdk-core/src/session/MeetingSession.ts
//
// R-22: the PUBLIC SDK entry point. `MeetingSession` composes `AuthApiClient` +
// `MeetingApiClient` + `SignalingClient` + `MediaTransport` and drives the full
// browser join happy path through an explicit state machine:
//   idle → fetching-token → connecting-mc → joining → joined  (+ disconnecting)
//
// `join()` orchestrates: auth (login/register) → GC meeting token → MC signaling
// join → active/active MH handshake (`MediaTransport.connectAll`) → send ONE
// `MediaConnectionUpdate` (R-21: ALWAYS, once connectAll settles). It emits the
// high-level events `onJoined`/`onParticipantJoined`/`onParticipantLeft`/
// `onMediaConnected`/`onError` (+ `stateChange`) and resolves with the join result.
//
// R-24/R-25: all five `dt_client_*` join metrics are emitted here (the lifecycle
// observer) + from `MediaTransport`, via the injectable metrics sink. R-26 bounded
// logs are DEFERRED (see docs/TODO.md §Observability Debt) — no `logJoinEvent` here.
//
// R-23 token hygiene: the user + meeting JWTs live ONLY as in-memory fields and are
// dropped on `disconnect()` (JS strings are immutable, so we null the references;
// `Authorization: Bearer` strings are per-call transients inside `MeetingApiClient`,
// never cached here). No token is ever logged, placed on an error, or emitted as a
// metric/log label. `meeting_id_hash` is a SHA-256 digest of the meeting id (R-32:
// `crypto.subtle.digest`, never `Math.random`).

import { DEFAULT_CLIENT_CONFIG, type MediaConfig } from '../config/clientConfig.js';
import { AuthApiClient } from '../http/AuthApiClient.js';
import { bytesToHex } from '../media/frame/hex.js';
import { MeetingApiClient } from '../http/MeetingApiClient.js';
import { SignalingClient } from '../signaling/SignalingClient.js';
import type { SignalingClientOptions } from '../signaling/SignalingClient.js';
import type { JoinedEvent } from '../signaling/events.js';
import { MediaTransport } from '../media/MediaTransport.js';
import type { MediaTransportOptions } from '../media/events.js';
import { MeetingIdentity } from '../media/setup/identity.js';
import { AudioPipeline, type AudioPipelineOptions } from '../media/lifecycle/AudioPipeline.js';
import { JoinResponseKekSource } from '../media/setup/kekSource.js';
import { MEDIA_KEK_SOURCES, MediaMetrics } from '../media/setup/mediaMetrics.js';
import { RosterIdentityKeys } from '../media/setup/rosterKeys.js';
import { createMicrophoneCapture } from '../media/setup/capture.js';
import { createAudioDecoder, createAudioEncoder } from '../media/setup/opus.js';
import { createAudioContextPlaybackSink } from '../media/setup/audioPlayback.js';
import type {
  AudioDecoderFactory,
  AudioEncoderFactory,
  CaptureSourceFactory,
  PlaybackSinkFactory,
} from '../media/setup/seams.js';
import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import { validateUserToken } from '../validation/limits.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import { MeetingUnauthorizedError } from '../errors/MeetingError.js';
import { CloseReason, normalizeCloseReason } from '../telemetry/closeReason.js';
import { configureTelemetry, flushMetrics, getMetricsSink } from '../telemetry/telemetryConfig.js';
import type { TelemetryConfig } from '../telemetry/telemetryConfig.js';
import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';

import {
  DEFAULT_AUDIO_SLOT_ID,
  MeetingSessionState,
  type JoinCredentials,
  type JoinOptions,
  type MeetingSessionEventMap,
  type MeetingSessionOptions,
  type StartMediaOptions,
} from './events.js';

/**
 * Bounded `failure_stage` label values (R-25 catalog; full set documented in
 * `docs/observability/metrics/client.md`). `gc_create_token` is RESERVED for catalog
 * parity with the server-side join pipeline — the browser client issues a single
 * `joinMeeting` POST (token-mint + join happen server-side behind it), so it cannot
 * observe that sub-stage and never emits it (catalog honesty, @observability F1).
 */
const FailureStage = {
  None: 'none',
  Signup: 'signup',
  /**
   * The caller's credential was rejected — a malformed token caught locally by
   * `validateUserToken`, or a **401** from GC's `joinMeeting`. NOT 403 — that is an
   * authorization denial on a valid token, which belongs to `gc_join`.
   *
   * Distinct from `signup` (which means an AC register/login call failed — a stage
   * the token path does not execute) and deliberately NOT `internal` (the SDK-fault
   * bucket): this is a caller-input error, deterministic and caller-fixable. Routing
   * it to `internal` would make that bucket non-actionable for oncall
   * (@observability, task #58).
   */
  CredentialInvalid: 'credential_invalid',
  /** Reserved — server-internal stage, not separately observable from this client. */
  GcCreateToken: 'gc_create_token',
  GcJoin: 'gc_join',
  McSignalingConnect: 'mc_signaling_connect',
  McJoinResponse: 'mc_join_response',
  MhConnect: 'mh_connect',
  Internal: 'internal',
} as const;
type FailureStage = (typeof FailureStage)[keyof typeof FailureStage];

/**
 * Which stage owns a failure raised while resolving the user token.
 *
 * `signup` means an AC register/login call failed — a stage the token path never
 * executes. An UNRECOGNIZED mode is neither: it is an SDK-contract violation, and
 * `internal` is the one case where that bucket is genuinely right (@observability,
 * task #58 Gate 3).
 */
function authFailureStage(mode: JoinCredentials['mode']): FailureStage {
  switch (mode) {
    case 'token':
      return FailureStage.CredentialInvalid;
    case 'login':
    case 'register':
      return FailureStage.Signup;
    default: {
      // Typed as `JoinCredentials['mode']`, not `string`, so this is unreachable and a
      // FOURTH variant is a compile error HERE as well as in `#authenticate`
      // (@paired-client C3). With a widened `string` it would have been one compile
      // error and one SILENT mislabel — and the mislabel lands in `internal`, the
      // SDK-fault bucket @observability requires stay actionable. A caller-input
      // failure quietly reported as an SDK fault is exactly what that bucket's
      // documentation exists to prevent.
      // Compile-time exhaustiveness AND an honest runtime fallback — both are needed,
      // and returning `exhaustive` gives only the first. `never` is erased at runtime,
      // so a JS caller passing `{mode:'sso'}` would have made the metric label the
      // literal string `sso` rather than `internal`. @observability's test caught that
      // the moment the `never` assert landed.
      const exhaustive: never = mode;
      void exhaustive;
      return FailureStage.Internal;
    }
  }
}

/**
 * Refine a GC-join failure into its `failure_stage`: a credential rejection (**401**)
 * is the caller's token being bad, not the meeting being unavailable.
 *
 * Why this exists (@observability F3): before the token path, the user token handed to
 * `joinMeeting` was seconds old and SDK-minted, so a credential rejection there was
 * rare. A caller-supplied token of arbitrary age makes it a ROUTINE `gc_join` outcome
 * — so `gc_join` would otherwise mix two populations with different oncall responses
 * ("meeting join denied" vs "your token is dead, re-authenticate"). Mirrors the
 * `signalingFailureStage` refinement below.
 */
function gcJoinFailureStage(err: unknown): FailureStage {
  // 401 ONLY. Two things deliberately absent (both Gate-3 findings):
  //
  // * NOT `ValidationError` — `joinMeeting` runs `validateMeetingCode` as its first
  //   statement, so the only ValidationError reachable here is a malformed MEETING
  //   CODE. Labelling a user's typo `credential_invalid` would page oncall toward auth
  //   for a mistyped code. (The local token shape-reject is already attributed by
  //   `authFailureStage` — it throws in `#authenticate`, outside this try.)
  //
  // * NOT 403 — GC returns 403 for authorization decisions on a VALID, live token:
  //   external participants not allowed, insufficient permissions, org meeting limit
  //   exceeded. GC's own contract is explicit (401 = invalid/missing token, 403 = user
  //   not allowed to join). Routing 403 here would put the primary "meeting join
  //   denied" status into the credential bucket — re-creating the exact population
  //   mixing this refinement exists to prevent, one label over.
  if (err instanceof MeetingUnauthorizedError) {
    return FailureStage.CredentialInvalid;
  }
  return FailureStage.GcJoin;
}

/**
 * Refine a signaling-phase failure into its `failure_stage`: a server-side REJECTION
 * of the join (the channel opened, but auth/forbidden/server-error came back in the
 * JoinResponse/ErrorMessage) is `mc_join_response`; a connect-level failure (timeout,
 * transport close, never reached the server) is `mc_signaling_connect` (@code-reviewer
 * F3 / @observability F1).
 */
function signalingFailureStage(err: unknown): FailureStage {
  if (err instanceof SignalingError) {
    switch (err.signalingCode) {
      // LOCAL causes — the signaling channel never opened or no server response
      // arrived (transport failure / framing / join deadline) → connect stage.
      case SignalingErrorCode.Transport:
      case SignalingErrorCode.Framing:
      case SignalingErrorCode.Timeout:
        return FailureStage.McSignalingConnect;
      // Everything else is a server-authored `ErrorMessage` code (Unauthorized,
      // Forbidden, NotFound, InternalError, …): the channel opened and the server
      // REJECTED the join → join-response stage.
      default:
        return FailureStage.McJoinResponse;
    }
  }
  // A non-SignalingError on this path means we never reached a server response.
  return FailureStage.McSignalingConnect;
}

/**
 * SHA-256 over `input`, hex-encoded, truncated to 16 hex chars (64 bits) — the
 * client-originated `meeting_id_hash` convention (@observability Q2; documented in
 * `docs/observability/metrics/client.md`). R-32: a digest, not randomness.
 */
async function meetingIdHash(input: string): Promise<string> {
  const data = new TextEncoder().encode(input);
  const digest = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', data));
  // Uses the shared `bytesToHex` rather than an inline loop. This function
  // previously carried a byte-for-byte copy of that loop, which
  // `docs/TODO.md` §Cross-Service Duplication tracked as blocked on the story
  // task-15 promotion of `media/frame/hex.ts` out of the test tree. That
  // promotion landed with this change, so the duplication is closed here in the
  // same pass rather than left for a future reader to rediscover.
  return bytesToHex(digest).slice(0, 16);
}

/** Map a signaling failure to the bounded `close_reason` label (never the raw string). */
function closeReasonForError(err: unknown): CloseReason {
  if (err instanceof SignalingError) {
    switch (err.signalingCode) {
      case SignalingErrorCode.Unauthorized:
      case SignalingErrorCode.Forbidden:
        return CloseReason.AuthFailed;
      case SignalingErrorCode.Timeout:
        return CloseReason.Timeout;
      case SignalingErrorCode.InternalError:
        return CloseReason.ServerError;
      default:
        // Transport-close-derived errors carry a numeric closeCode; map it.
        return normalizeCloseReason(err.closeCode);
    }
  }
  return CloseReason.Unknown;
}

/**
 * The public browser SDK entry point. Single-use: one `join()` per instance;
 * construct a new instance to re-join.
 *
 * @example
 * const session = new MeetingSession({ acOriginTemplate, gcBaseUrl });
 * session.on('mediaConnected', (url) => console.log('mh connected', url));
 * // Reuse the token the app already holds — no re-authentication, no retained password.
 * await session.join({
 *   orgSubdomain: 'demo',
 *   meetingCode,
 *   credentials: { mode: 'token', userToken },
 * });
 */
export class MeetingSession extends TypedEventEmitter<MeetingSessionEventMap> {
  readonly #authClient: AuthApiClient;
  readonly #meetingClient: MeetingApiClient;
  readonly #connectFn: WebTransportConnectFn | undefined;
  readonly #clock: () => number;
  readonly #connectTimeoutMs: number | undefined;
  readonly #joinTimeoutMs: number | undefined;
  readonly #metricsSink: MetricsSink | undefined;

  #state: MeetingSessionState = MeetingSessionState.Idle;
  #signaling: SignalingClient | undefined;
  #media: MediaTransport | undefined;
  #joinStartMs = 0;
  #metricLabels: MetricLabels = {};
  #torn = false;
  readonly #mediaConfig: MediaConfig;
  /**
   * The KEK-source seam. THE ONLY PLACE THE MEETING KEK LIVES. Never on an
   * event, never on an error, never persisted, zeroed at disconnect.
   */
  readonly #kekSource = new JoinResponseKekSource();
  #rosterKeys: RosterIdentityKeys | undefined;
  /**
   * The meeting identity — one non-extractable Ed25519 signing capability plus
   * its public half, generated per meeting and never persisted (ADR-0036 §4: a
   * key reused across meetings makes a participant linkable by public key
   * regardless of display name).
   *
   * Held behind {@link MeetingIdentity} rather than as a bare keypair record.
   * See that module's header for what is retained, why ADR-0036 requires it, and
   * the `ts-no-retained-credentials` interaction that is disclosed there rather
   * than routed around.
   */
  #identity: MeetingIdentity | undefined;
  #pipeline: AudioPipeline | undefined;
  #senderId: number | undefined;
  #orgId = '';
  readonly #captureFactory: CaptureSourceFactory;
  readonly #encoderFactory: AudioEncoderFactory;
  readonly #decoderFactory: AudioDecoderFactory;
  readonly #playbackFactory: PlaybackSinkFactory;

  constructor(options: MeetingSessionOptions) {
    super();
    this.#authClient = new AuthApiClient({
      acOriginTemplate: options.acOriginTemplate,
      ...(options.fetchImpl ? { fetchImpl: options.fetchImpl } : {}),
    });
    this.#meetingClient = new MeetingApiClient({
      gcBaseUrl: options.gcBaseUrl,
      ...(options.fetchImpl ? { fetchImpl: options.fetchImpl } : {}),
    });
    this.#connectFn = options.connect;
    this.#clock = options.clock ?? Date.now;
    this.#connectTimeoutMs = options.connectTimeoutMs;
    this.#joinTimeoutMs = options.joinTimeoutMs;
    this.#metricsSink = options.metricsSink ?? getMetricsSink();
    this.#mediaConfig = options.mediaConfig ?? DEFAULT_CLIENT_CONFIG.media;
    this.#captureFactory = options.captureFactory ?? createMicrophoneCapture;
    this.#encoderFactory = options.encoderFactory ?? createAudioEncoder;
    this.#decoderFactory = options.decoderFactory ?? createAudioDecoder;
    this.#playbackFactory = options.playbackFactory ?? createAudioContextPlaybackSink;
  }

  /**
   * Configure the SDK's single global telemetry providers (delegates to
   * `configureTelemetry` — the contract telemetryConfig.ts documents).
   */
  static configure(config: TelemetryConfig): MetricsSink {
    return configureTelemetry(config);
  }

  /** The current state-machine state. */
  get state(): MeetingSessionState {
    return this.#state;
  }

  /**
   * Drive the full join happy path. Resolves with the {@link JoinedEvent} when ≥1
   * MH connects; rejects (and tears down) on any phase failure, including all-MH-fail.
   */
  async join(options: JoinOptions): Promise<JoinedEvent> {
    this.#joinStartMs = this.#clock();
    this.#metricLabels = {
      client_version: __SDK_VERSION__,
      org_id: options.orgSubdomain,
      meeting_id_hash: 'none',
    };
    this.#orgId = options.orgSubdomain;
    let stage: FailureStage = FailureStage.Internal;
    try {
      // --- fetching-token: auth + GC meeting token ---
      this.#setState(MeetingSessionState.FetchingToken);
      // `signup` covers only the AC register/login call. On the token path
      // `#authenticate` performs no network call, and a malformed token is refined to
      // `credential_invalid` below — so `signup` is unreachable for token joins by
      // construction rather than by accident.
      stage = authFailureStage(options.credentials.mode);
      // R-23: tokens are held ONLY as `join()`-scoped locals — never on the
      // instance — so no token-bearing reference outlives this call (GC-eligible
      // the moment `join()` settles). `disconnect()` therefore has no token state
      // to zero; the per-call `Authorization: Bearer` strings are transients inside
      // `MeetingApiClient`. The values never reach a log, error, or metric label.
      const userToken = await this.#authenticate(options);

      stage = FailureStage.GcJoin;
      let joinResp: Awaited<ReturnType<MeetingApiClient['joinMeeting']>>;
      try {
        joinResp = await this.#meetingClient.joinMeeting(options.meetingCode, { userToken });
      } catch (err) {
        // Split credential rejection out of `gc_join` (see `gcJoinFailureStage`).
        stage = gcJoinFailureStage(err);
        throw err;
      }
      this.#metricLabels = {
        ...this.#metricLabels,
        meeting_id_hash: await meetingIdHash(joinResp.meetingId),
      };

      // ADR-0036 §4 step 1: generate the identity signing keypair BEFORE the
      // join request, so its public half can travel on it. Per meeting, never
      // persisted, private half non-extractable.
      this.#identity = await MeetingIdentity.create();

      // --- connecting-mc: MC signaling join ---
      this.#setState(MeetingSessionState.ConnectingMc);
      stage = FailureStage.McSignalingConnect;
      const signaling = this.#createSignaling();
      this.#signaling = signaling;
      let joined: JoinedEvent;
      try {
        joined = await this.#joinSignaling(signaling, joinResp, options);
      } catch (err) {
        // Distinguish a server join-REJECTION (mc_join_response) from a connect-level
        // failure (mc_signaling_connect) for the `failure_stage` label (F3/F1).
        stage = signalingFailureStage(err);
        throw err;
      }

      // --- joining: active/active MH handshake + MediaConnectionUpdate ---
      this.#setState(MeetingSessionState.Joining);
      stage = FailureStage.MhConnect;
      const media = this.#createMedia(signaling);
      this.#media = media;
      let firstMh = true;
      media.on('connected', (url) => {
        if (firstMh) {
          firstMh = false;
          this.#histogram('dt_client_time_to_first_mh_connected_ms');
        }
        this.emit('mediaConnected', url);
      });
      let connectErr: unknown;
      try {
        await media.connectAll(joined.mediaServers, joinResp.token);
      } catch (err) {
        connectErr = err;
      }
      // R-21: ALWAYS send ONE MediaConnectionUpdate once connectAll settles.
      await signaling.sendMediaConnectionUpdate(media.getStatusReports());
      if (connectErr !== undefined) {
        throw connectErr;
      }

      // --- joined ---
      this.#senderId = joined.senderId;
      this.#setState(MeetingSessionState.Joined);
      this.#emitJoinAttempt('success', FailureStage.None);
      this.emit('joined', joined);
      return joined;
    } catch (err) {
      this.#emitJoinAttempt('failure', stage);
      this.disconnect();
      throw err;
    }
  }

  /**
   * Start the audio media pipeline (ADR-0036 §5/§6).
   *
   * ---------------------------------------------------------------------------
   * EXPLICIT, AND A MEDIA FAULT DOES NOT TAKE THE MEETING WITH IT
   * ---------------------------------------------------------------------------
   *
   * Deliberately NOT automatic on `join()`. Media needs a microphone permission
   * prompt and — because of the browser autoplay policy — a user gesture for
   * playback, so it belongs to an application action rather than to the join. It
   * is also the only in-story lever resembling a kill switch: "no way to disable
   * the media path short of a redeploy" is recorded as a story-2 operations item,
   * and an explicit start is what keeps that from being true today.
   *
   * A failure here rejects THIS call and leaves the signaling session joined and
   * healthy. Degraded audio beats a dropped meeting.
   *
   * ---------------------------------------------------------------------------
   * THE ORDER IS FIXED: DECLARE CAPABILITY, THEN EXPECT A DIRECTIVE
   * ---------------------------------------------------------------------------
   *
   * MC emits the send directive when the client declares its receive capability,
   * NOT at join. A client that joins and never declares is never told to send,
   * and the connection stays healthy in every other respect — which is exactly
   * what makes the omission hard to notice from the client side.
   *
   * @throws {SignalingError} if called before a settled join.
   */
  async startMedia(options: StartMediaOptions = {}): Promise<AudioPipeline> {
    const signaling = this.#signaling;
    const identity = this.#identity;
    const senderId = this.#senderId;
    if (!signaling || this.#state !== MeetingSessionState.Joined) {
      throw new SignalingError(SignalingErrorCode.Transport, 'startMedia requires a settled join');
    }
    const signer = identity?.signer;
    if (!identity || !signer || senderId === undefined) {
      // Fail loudly rather than degrading. Without a sender id there is no key
      // id, and without an identity key there is no signature — and this SDK
      // does not send or accept unsigned frames under any degradation.
      throw new SignalingError(
        SignalingErrorCode.Transport,
        'the meeting controller did not assign a sender id, so media cannot be published',
      );
    }
    if (this.#pipeline) return this.#pipeline;

    // The media label set is built BY ALLOW-LIST from two named strings. It
    // shares nothing with `this.#metricLabels`, which carries `meeting_id_hash`
    // — the dimension ADR-0036 §11 bars from every media emission.
    const metrics = new MediaMetrics(
      { clientVersion: __SDK_VERSION__, orgId: this.#orgId },
      this.#metricsSink,
    );
    if (this.#kekSource.isProvisioned) metrics.kekUpdate(MEDIA_KEK_SOURCES.JoinResponse);

    const slotId = options.slotId ?? DEFAULT_AUDIO_SLOT_ID;
    const media = this.#media;
    const pipelineOptions: AudioPipelineOptions = {
      config: this.#mediaConfig,
      metrics,
      kekSource: this.#kekSource,
      roster:
        this.#rosterKeys ?? new RosterIdentityKeys(this.#mediaConfig.ingress.maxCachedIdentityKeys),
      senderId,
      kekGeneration: signaling.kekGeneration,
      identity,
      declaredSlotIds: [slotId],
      senderFor: (url) => media?.getDatagramChannel(url),
      readableFor: (url) => media?.getDatagramChannel(url)?.readable,
      reportMute: (audioMuted) => {
        // Informational and deliberately best-effort: ADR-0036 §5 requires
        // client mute to hold WITHOUT the server honouring it, so a failed
        // report must not undo the local suppression.
        void signaling.sendMuteRequest(audioMuted).catch(() => {
          // Nothing actionable, and nothing safe to log about a mute report.
        });
      },
      captureFactory: this.#captureFactory,
      encoderFactory: this.#encoderFactory,
      decoderFactory: this.#decoderFactory,
      playbackFactory: this.#playbackFactory,
      clock: this.#clock,
    };
    const pipeline = new AudioPipeline(pipelineOptions);
    this.#pipeline = pipeline;

    signaling.on('sendDirective', (directive) => {
      const audio = directive.streams.find((s) => s.mediaKind === 'audio');
      if (!audio) return;
      pipeline.setSendDirective({
        streamNumber: audio.streamNumber,
        // MC's directed value when present; the configured DEFAULT otherwise.
        // Never a local ceiling applied on top — see `clientConfig.ts`.
        bitrateBps: audio.maxBitrateBps ?? this.#mediaConfig.audio.defaultBitrateBps,
        targets: audio.targets,
      });
    });

    await signaling.sendReceiveCapability([{ slotId, mediaKind: 'audio' }]);
    await pipeline.start(options.deviceId !== undefined ? { deviceId: options.deviceId } : {});
    return pipeline;
  }

  /** The running media pipeline, or `undefined` before `startMedia()`. */
  get media(): AudioPipeline | undefined {
    return this.#pipeline;
  }

  /**
   * Tear down ALL transports and drop token references (R-23). Idempotent: safe to
   * call multiple times and from the `join()` failure path.
   */
  disconnect(): void {
    if (this.#torn) return;
    this.#torn = true;
    // The `Disconnecting` stateChange notification fires FIRST but must NOT be able
    // to skip transport teardown: a throwing `stateChange` listener would otherwise
    // leak the MC/MH connections. Run teardown in a `finally` so it always executes
    // (then re-raise any listener error after the resources are released) — @operations.
    try {
      this.#setState(MeetingSessionState.Disconnecting);
    } finally {
      // Guard each teardown independently so a slow/throwing close can't abort the rest.
      try {
        this.#media?.disconnect();
      } catch {
        // ignore — best-effort teardown
      }
      try {
        this.#signaling?.close();
      } catch {
        // ignore
      }
      try {
        // Stops capture (the microphone indicator), the codecs, the reader and
        // the rotation timer, and clears the replay window, the transmit-key
        // cache and the send-side keys — ADR-0028 §5's explicit cleanup, whose
        // wiring the codec task left to this task. Detached because
        // `disconnect()` is synchronous by contract; the registry is idempotent
        // and each disposer is isolated.
        // `.catch` and not just `void`: `stop()` rejects if an embedder's
        // `fault` listener throws (`TypedEventEmitter.emit` does not isolate
        // listeners), and the surrounding try/catch catches synchronous throws
        // only. An unhandled rejection here would surface in the embedder's
        // error reporting attributed to the SDK, on disconnect, exactly when
        // something else has already gone wrong.
        void this.#pipeline?.stop().catch(() => {
          // Teardown is already complete by the time a fault can be raised —
          // `dispose()` has run — so resources are released either way.
        });
      } catch {
        // ignore — best-effort teardown
      }
      // The KEK is zeroed and dropped, and the roster keys released. The
      // identity signing capability is non-extractable, so there are no bytes to
      // overwrite; dropping the reference is the whole cleanup.
      this.#kekSource.clear();
      this.#rosterKeys?.clear();
      this.#identity?.clear();
      this.#identity = undefined;
      // At a 10 s export cadence up to 10 s of media counters would otherwise die
      // with the tab — and the window that dies is the one containing the
      // incident. `keepalive` rescues requests already in flight; it does nothing
      // for deltas not yet exported.
      void flushMetrics().catch(() => {
        // `forceFlush()` rejects when the exporter fails, and this flush is an
        // EXTRA export beyond the periodic ones aimed at a proxy with a per-`sub`
        // rate limit — so a 429 or an unreachable proxy rejects it routinely.
        // Losing the last window of counters is the cost; an unhandled rejection
        // in the embedder's page during an incident is not an acceptable
        // addition to it.
      });
    }
    // R-23: no token references to drop — tokens are never stored on the instance
    // (held only as `join()`-scoped locals, GC-eligible once `join()` settles), and
    // `SignalingClient` likewise keeps the join token off its instance. So teardown
    // is transport-only; there is no persistent token holder to outlive disconnect.
  }

  // ----------------------------------------------------------------------------
  // Internal
  // ----------------------------------------------------------------------------

  /**
   * Resolve the user token `join()` presents to GC.
   *
   * `switch` with an exhaustiveness assert rather than a chain of `if`s: with a
   * trailing `return login(...)` the login branch is a *default*, and a future
   * variant that happened to carry `email` + `password` would compile and silently
   * route there — the same silent-misroute class as the register-vs-login mismatch
   * this task exists to remove. `never` makes a new variant a compile error instead.
   */
  async #authenticate(options: JoinOptions): Promise<string> {
    const c = options.credentials;
    switch (c.mode) {
      case 'token':
        // No AC round-trip. The token is caller-supplied, so it reaches an
        // `Authorization: Bearer` header the SDK did not mint — shape-validate before
        // it goes anywhere (header-injection guard, NOT verification; see
        // `validateUserToken`).
        validateUserToken(c.userToken);
        return c.userToken;
      case 'register': {
        const resp = await this.#authClient.register({
          subdomain: options.orgSubdomain,
          email: c.email,
          password: c.password,
          displayName: c.displayName,
        });
        return resp.accessToken;
      }
      case 'login': {
        const resp = await this.#authClient.login({
          subdomain: options.orgSubdomain,
          email: c.email,
          password: c.password,
        });
        return resp.accessToken;
      }
      default: {
        // Interpolate the DISCRIMINANT ONLY — never the object.
        //
        // This branch exists precisely for when the type contract is violated (a JS
        // consumer, a cast, a deserialized value, a future variant), so it must assume
        // `c` is a real credentials object. `JSON.stringify(c)` would serialize
        // `password` straight onto `Error.message`, which propagates out of `join()` to
        // the caller — and `sdk-core` is a published SDK, so an embedder's
        // `console.error(err)` or error-reporting hook would receive it. Found at Gate 3
        // by four reviewers independently; it also tripped §Credential Leak items 8 and 9
        // of the very lens this task adds.
        //
        // `mode` is a string-literal discriminant, never a secret, and it carries the
        // entire diagnostic value: you learn which variant was unhandled.
        const exhaustive: never = c;
        const mode = String((exhaustive as { mode?: unknown }).mode);
        throw new Error(`unhandled credential mode: ${mode}`);
      }
    }
  }

  #createSignaling(): SignalingClient {
    this.#rosterKeys = new RosterIdentityKeys(this.#mediaConfig.ingress.maxCachedIdentityKeys);
    const opts: SignalingClientOptions = {
      ...(this.#connectFn ? { connect: this.#connectFn } : {}),
      ...(this.#joinTimeoutMs !== undefined ? { joinTimeoutMs: this.#joinTimeoutMs } : {}),
      // The KEK goes from the decode boundary straight into the seam and the
      // decoded field is scrubbed — it never rides an event payload.
      kekSink: this.#kekSource,
      rosterKeys: this.#rosterKeys,
    };
    const signaling = new SignalingClient(opts);
    signaling.on('participantJoined', (e) => this.emit('participantJoined', e));
    signaling.on('participantLeft', (e) => this.emit('participantLeft', e));
    signaling.on('error', (e) => this.emit('error', e));
    return signaling;
  }

  async #joinSignaling(
    signaling: SignalingClient,
    joinResp: Awaited<ReturnType<MeetingApiClient['joinMeeting']>>,
    options: JoinOptions,
  ): Promise<JoinedEvent> {
    const participantName = options.credentials.displayName ?? '';
    try {
      const joined = await signaling.join({
        webtransportEndpoint: joinResp.mcAssignment.webtransportEndpoint ?? '',
        meetingId: joinResp.meetingId,
        joinToken: joinResp.token,
        participantName,
        ...(this.#identity?.publicKey ? { identityPublicKey: this.#identity.publicKey } : {}),
      });
      this.#emitSignalingConnection('success', CloseReason.Normal);
      this.#histogram('dt_client_time_to_signaling_ready_ms');
      return joined;
    } catch (err) {
      this.#emitSignalingConnection('failure', closeReasonForError(err));
      throw err;
    }
  }

  #createMedia(signaling: SignalingClient): MediaTransport {
    const egress = this.#mediaConfig.egress;
    const opts: MediaTransportOptions = {
      clock: this.#clock,
      metricLabels: this.#metricLabels,
      // CHOSEN, not inherited (ADR-0036 §1). The transport queue is kept shallow
      // so the bounded application queue above it makes — and counts — the drop
      // decision; a drop inside the user agent's queue is uncountable by us and
      // structurally invisible to MH.
      datagramQueue: {
        outgoingHighWaterMark: egress.transportOutgoingHighWaterMarkFrames,
        outgoingMaxAgeMs: egress.transportOutgoingMaxAgeMs,
        incomingHighWaterMark: this.#mediaConfig.ingress.transportIncomingHighWaterMarkFrames,
      },
      runInContext: (fn) => signaling.runInJoinContext(fn),
      ...(this.#connectFn ? { connect: this.#connectFn } : {}),
      ...(this.#connectTimeoutMs !== undefined ? { connectTimeoutMs: this.#connectTimeoutMs } : {}),
      ...(this.#metricsSink ? { metricsSink: this.#metricsSink } : {}),
    };
    return new MediaTransport(opts);
  }

  #setState(state: MeetingSessionState): void {
    this.#state = state;
    this.emit('stateChange', state);
  }

  #counter(name: string, labels: MetricLabels): void {
    this.#metricsSink?.counter(name, { ...this.#metricLabels, ...labels });
  }

  #histogram(name: string): void {
    this.#metricsSink?.histogram(
      name,
      { ...this.#metricLabels },
      this.#clock() - this.#joinStartMs,
    );
  }

  #emitJoinAttempt(status: 'success' | 'failure', failureStage: FailureStage): void {
    this.#counter('dt_client_join_attempts_total', { status, failure_stage: failureStage });
  }

  #emitSignalingConnection(status: 'success' | 'failure', closeReason: CloseReason): void {
    this.#counter('dt_client_signaling_connection_total', { status, close_reason: closeReason });
  }
}
