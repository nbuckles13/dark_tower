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

import { transmitRewrapLatencyMs, type MediaConfig } from '../config/clientConfig.js';
import { SignalingMediaState } from './signalingMediaState.js';
import { SlotPlan } from './slotPlan.js';
import type { SlotPair } from '../signaling/pairing.js';
import type { PublishKind } from '../signaling/events.js';
import { AuthApiClient } from '../http/AuthApiClient.js';
import { bytesToHex } from '../media/frame/hex.js';
import { MeetingApiClient } from '../http/MeetingApiClient.js';
import { SignalingClient } from '../signaling/SignalingClient.js';
import { SignalingCodec } from '../signaling/codecMap.js';
import type { SignalingClientOptions } from '../signaling/SignalingClient.js';
import type { JoinedEvent } from '../signaling/events.js';
import { MediaTransport } from '../media/MediaTransport.js';
import type { MediaTransportOptions } from '../media/events.js';
import { MeetingIdentity } from '../media/setup/identity.js';
import type { AudioPipeline } from '../media/lifecycle/AudioPipeline.js';
import { MeetingKekHolder } from '../media/setup/kekSource.js';
import {
  kekObserver,
  mhControlSink,
  rosterInvalidationListener,
  zeroInitDirectiveSeries,
  bestEffortMuteReport,
} from './mediaWiring.js';
import { readIsHost } from './meetingRole.js';
import { MediaMetrics } from '../media/setup/mediaMetrics.js';
import type { MuteSnapshot } from '../media/lifecycle/muteState.js';
import { RosterIdentityKeys } from '../media/setup/rosterKeys.js';
import type { ReceiveVerificationRecorder } from '../media/pipeline/receiveVerification.js';
import type { VideoPublisher } from '../media/lifecycle/VideoPublisher.js';
import type { VideoReceiveView } from '../media/lifecycle/VideoReceiver.js';
import { probeVp8Support, type Vp8Profile } from '../media/setup/vp8.js';
import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import {
  buildSessionMedia,
  probeVideoCodec,
  sessionMediaSeams,
  vp8StartFault,
  type SessionMedia,
  type SessionMediaSeams,
  type Vp8ProbeResult,
} from './mediaStart.js';
import { validateUserToken } from '../validation/limits.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import { MeetingUnauthorizedError } from '../errors/MeetingError.js';
import { CloseReason, normalizeCloseReason } from '../telemetry/closeReason.js';
import { configureTelemetry, flushMetrics, getMetricsSink } from '../telemetry/telemetryConfig.js';
import type { TelemetryConfig } from '../telemetry/telemetryConfig.js';
import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';

import {
  MeetingSessionState,
  type CaptureSourceInfo,
  type VideoCaptureSourceInfo,
  type JoinCredentials,
  type JoinOptions,
  type MeetingSessionEventMap,
  type MeetingSessionOptions,
  type ReceiveSlotsDiagnostics,
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
   * The KEK holder: current generation plus at most one retained previous. THE
   * ONLY PLACE THE MEETING KEK LIVES. Never on an event, never on an error,
   * never persisted, zeroed at disconnect.
   */
  readonly #kekSource: MeetingKekHolder;
  /**
   * The media metric handles. Built at JOIN rather than at `startMedia()`, so a
   * KEK arriving on the join response — before media starts — is counted on
   * arrival rather than inferred later from whether one happens to be held.
   * Media-path labels are an allow-list of two strings (plus a closed media kind on
   * per-kind handles); see `mediaMetrics.ts`.
   */
  #mediaMetrics: MediaMetrics | undefined;
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
  readonly #receiveVerification: ReceiveVerificationRecorder | undefined;
  /**
   * What this session publishes and declares, and the caps its join reported —
   * resolved and frozen at construction (story 3 R-1, R-2). See `slotPlan.ts`.
   */
  readonly #slots: SlotPlan;
  /**
   * Signalling-derived media state: the E3 directive/assignments cache, server
   * mute per participant, the R-18 tracker and client mute. See
   * `signalingMediaState.ts`.
   */
  readonly #mediaState: SignalingMediaState;
  #captureSource: CaptureSourceInfo | undefined;
  /** What `startMedia()` built (keys, audio, video) and its one release. */
  #sessionMedia: SessionMedia | undefined;
  /** The join-time VP8 probe (story 3 R-8); `undefined` when video is not in use. */
  #vp8: Vp8ProbeResult | undefined;
  readonly #seams: SessionMediaSeams;
  readonly #vp8Probe: (profile: Vp8Profile) => Promise<void>;
  /**
   * Whether the meeting token names this participant host — a UI HINT for the
   * server-mute affordance, never an authority (MC authorises every request).
   * Only this boolean survives `join()`; see `meetingRole.ts`.
   */
  #isHost = false;

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
    // At the boundary, BEFORE N and M are used: every refusal throws typed, here.
    this.#slots = new SlotPlan(options);
    this.#mediaConfig = this.#slots.mediaConfig;
    this.#mediaState = new SignalingMediaState((kind) =>
      this.emit('captureResumedAfterServerMute', { kind }),
    );
    this.#receiveVerification = options.receiveVerification;
    this.#kekSource = new MeetingKekHolder({
      rewrapLatencyMs: transmitRewrapLatencyMs(this.#mediaConfig),
      clock: this.#clock,
    });
    this.#seams = sessionMediaSeams(options);
    this.#vp8Probe = options.vp8Probe ?? ((profile) => probeVp8Support(profile));
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
    const mediaMetrics = this.#buildMediaMetrics();
    this.#mediaMetrics = mediaMetrics;
    this.#kekSource.setObserver(kekObserver(mediaMetrics.session));
    zeroInitDirectiveSeries(mediaMetrics, this.#slots.publish);
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
      // The host hint (story 2 R-11). The decoded payload never leaves
      // `readIsHost`; only the boolean and a bounded failure token come back.
      const role = readIsHost(joinResp.token);
      this.#isHost = role.isHost;
      if ('failure' in role) {
        console.warn(
          '[dt-meeting-role]',
          `could not read the meeting role from the meeting token (${role.failure}); ` +
            'host controls stay hidden. MC still authorises every request.',
        );
      }

      // ADR-0036 §4 step 1: generate the identity signing keypair BEFORE the
      // join request, so its public half can travel on it. Per meeting, never
      // persisted, private half non-extractable.
      this.#identity = await MeetingIdentity.create();

      // Story 3 R-8: when video is published or received, prove VP8 encode AND
      // decode BEFORE the join, so `CODEC_VP8` is declared only when both pass.
      // A failure does not fail the join: it is held and fails `startMedia()`
      // loudly (never an audio-only degradation).
      this.#vp8 = await probeVideoCodec(
        this.#slots.publish.includes('video') || this.#mediaConfig.receive.videoSlots > 0,
        this.#mediaConfig.video.capabilityProbe,
        this.#vp8Probe,
      );

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
      const media = this.#createMedia(signaling, mediaMetrics);
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
      this.#slots.joined(joined.maxAudioReceiveSlots, joined.maxVideoReceiveSlots);
      // OUR OWN key goes into the roster resolver, and this is NOT a self-trust
      // branch.
      //
      // MC's roster carries `existing_participants` — everyone EXCEPT us — so
      // `#feedRosterKeys` never learns our own key. In a one-participant meeting
      // that is fatal to the whole objective: every frame MH forwards back to us
      // carries `key_id.sender_id == our own id`, `identityKeyFor` returns
      // `undefined`, and the receive path fails closed on `no_roster_entry`. Not
      // some frames — ALL of them, silently, with audio visibly arriving at the
      // wire. (Found by the Layer 7 loopback env-test, which is the only thing
      // in the suite that could have found it: every unit-tier loopback seeds
      // its own roster by hand.)
      //
      // What this does NOT do is skip verification. The frame still has to carry
      // a signature that verifies against this key, and only the holder of the
      // matching private key can produce one — so a forged frame claiming our
      // sender id is rejected exactly as before. The ingress path keeps its
      // single, uniform "resolve the key for `key_id.sender_id`, then verify"
      // rule with no `if (senderId === mine)` anywhere; we are simply supplying
      // the key for a sender the roster structurally cannot tell us about.
      await this.#seedOwnRosterKey();
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
   * THE DECLARATION RODE THE JOIN
   * ---------------------------------------------------------------------------
   *
   * The join carried the publish set and the receive declaration (story 3 R-2),
   * so MC directs from the join; the directive and assignments already received
   * are replayed into the pipeline as it attaches. No post-join declaration is
   * sent here (a redeclaration stays available).
   *
   * VIDEO (story 3 C-A2): when video is published or received and the join-time
   * VP8 probe failed, this THROWS `VideoCodecUnsupportedError` and emits a fatal
   * `mediaFault` — never an audio-only degradation (R-8).
   *
   * @throws {SignalingError} if called before a settled join.
   * @throws {VideoCodecUnsupportedError} if VP8 is unsupported and video is in use.
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
    const metrics = this.#mediaMetrics ?? this.#buildMediaMetrics();

    // R-8: video in use and VP8 unproven -> fail media start LOUDLY, typed.
    if (this.#vp8?.ok === false) {
      this.emit('mediaFault', vp8StartFault(this.#vp8.error));
      throw this.#vp8.error;
    }

    // The session's ONE transmit-key manager + KEK subscription, the audio
    // pipeline and (when published) the video publisher — `mediaStart.ts`.
    const built = buildSessionMedia({
      config: this.#mediaConfig,
      configSource: this.#slots.mediaConfigSource,
      metrics,
      kekSource: this.#kekSource,
      roster:
        this.#rosterKeys ?? new RosterIdentityKeys(this.#mediaConfig.ingress.maxCachedIdentityKeys),
      senderId,
      identity,
      publish: this.#slots.publish,
      declaration: this.#slots.declaration,
      media: this.#media,
      mute: this.#mediaState.mute,
      reportMute: bestEffortMuteReport(signaling),
      seams: this.#seams,
      verification: this.#receiveVerification,
      clock: this.#clock,
      emit: {
        muteChanged: (snapshot) => this.emit('muteChanged', snapshot),
        firstMediaFrame: (elapsedMs) => this.emit('firstMediaFrame', elapsedMs),
        mediaFault: (fault) => this.emit('mediaFault', fault),
        videoSlotRenderChanged: (e) => this.emit('videoSlotRenderChanged', e),
        cameraHealthChanged: (e) => this.emit('cameraHealthChanged', e),
      },
      mediaState: this.#mediaState,
    });
    this.#sessionMedia = built;
    const { pipeline } = built;
    this.#pipeline = pipeline;
    this.#captureSource = built.captureInfo;

    // Audio then video; a failure releases everything (keys cleared) first.
    await built.start(options.deviceId !== undefined ? { deviceId: options.deviceId } : {});
    return pipeline;
  }

  /**
   * Publish this client's own identity public key into the roster resolver.
   *
   * Best-effort by the same reasoning `#feedRosterKeys` uses: a key that has not
   * landed means the next frame from that sender is dropped and counted as
   * `no_roster_entry`, which is the correct transient. It is awaited at join
   * rather than fired-and-forgotten because, unlike a peer's key, this one is
   * needed by the FIRST frame that returns — and the join has already settled by
   * the time we get here, so the wait costs nothing on the media path.
   *
   * THE ENTRY IS NOT PINNED, and that is deliberate. `#feedRosterKeys` routes
   * every later `ParticipantJoined` through the same `upsert`, and
   * `RosterIdentityKeys` replaces on write — so a roster message naming our own
   * sender id would overwrite this key, after which frames forged against our
   * sender id would verify.
   *
   * That is bounded by something larger rather than by anything special here:
   * MC distributes every participant's identity key with no out-of-band
   * verification, so MC can already impersonate any peer to us. Hardening only
   * the self entry would buy little and would imply a protection that does not
   * exist for peers. What IS worth recording is the asymmetry — this is the one
   * key we know independently, because we generated it. Pinning it is a change
   * to the crypto resolver in `media/setup/rosterKeys.ts` with its own tests,
   * not a change here.
   */
  async #seedOwnRosterKey(): Promise<void> {
    const senderId = this.#senderId;
    const publicKey = this.#identity?.publicKey;
    // No sender id means MC assigned none; `startMedia()` already fails closed on
    // that, so there is nothing to seed and nothing to report here.
    if (senderId === undefined || !publicKey) return;
    await this.#rosterKeys?.upsert({ senderId, identityPublicKey: publicKey });
  }

  /**
   * The effective receive-slot configuration and the server cap (story 2 R-23),
   * readable at runtime. Numbers and a bounded source token only.
   */
  get receiveSlots(): ReceiveSlotsDiagnostics {
    return this.#slots.diagnostics();
  }

  /**
   * The participant pairing view (story 3 R-3): sender id -> the audio and video
   * slots naming that sender, from MC's LATEST assignments. Pairing is by
   * publisher, never by slot index. A live video slot pairs by the VERIFIED
   * sender of its last rendered frame (story 3 C-B).
   */
  get slotPairing(): ReadonlyMap<number, SlotPair> {
    return this.#mediaState.slotPairing(this.#sessionMedia?.videoReceiver?.liveSenders());
  }

  /** Video receive (story 3 C-B): attach slot sinks, read evidence. Narrow by design. */
  get videoReceive(): VideoReceiveView | undefined {
    return this.#sessionMedia?.videoView;
  }

  /**
   * AUDIO client mute (ADR-0036 §5); never touches video. Through the running
   * pipeline when there is one (capture-side enforcement).
   */
  setAudioMuted(muted: boolean): void {
    if (this.#pipeline) this.#pipeline.setAudioMuted(muted);
    else this.#clientMuteChanged('audio', muted, this.#mediaState.setClientMute('audio', muted));
  }

  /**
   * VIDEO client mute (story 3 R-15, R-17); never touches audio. Via the video
   * publisher when running: the camera STOPS; unmute re-acquires it (also the
   * R-19 retry edge).
   */
  setVideoMuted(muted: boolean): void {
    const video = this.video;
    if (video) video.setMuted(muted);
    else this.#clientMuteChanged('video', muted, this.#mediaState.setClientMute('video', muted));
  }

  /** The video publisher, once media started with video published. */
  get video(): VideoPublisher | undefined {
    return this.#sessionMedia?.video;
  }

  /** Camera light: directed with targets AND not client-muted AND healthy. Sampled. */
  get cameraTrackLive(): boolean {
    return this.video?.cameraTrackLive ?? false;
  }

  /** What feeds the VIDEO send path, once media started with video published. */
  get videoCaptureSource(): VideoCaptureSourceInfo | undefined {
    return this.#sessionMedia?.videoCaptureInfo;
  }

  /** The current client-mute state, both kinds. */
  get muteState(): MuteSnapshot {
    return this.#mediaState.mute.snapshot();
  }

  #clientMuteChanged(kind: PublishKind, muted: boolean, snapshot: MuteSnapshot | undefined): void {
    if (!snapshot) return;
    this.emit('muteChanged', snapshot);
    // Best-effort, ONE kind per request: the other kind's field stays absent,
    // so MC leaves it exactly as it is (`signaling/muteRequest.ts`).
    if (this.#state === MeetingSessionState.Joined) {
      void this.#signaling?.sendMuteRequest(kind, muted).catch(() => undefined);
    }
  }

  /**
   * What feeds the send path — `undefined` before `startMedia()`. `toneHz` only
   * in a test-tone build. For the E2E bus and diagnostics; never a metric label
   * beyond the bounded `mode`.
   */
  get captureSource(): CaptureSourceInfo | undefined {
    return this.#captureSource;
  }

  /**
   * Whether this participant is the meeting's host, from the meeting token's
   * `role` claim. A UI HINT for showing the server-mute affordance: MC decides
   * authority from its own validated token and the connection on every request,
   * so this being wrong shows or hides a button and grants nothing. `false`
   * before `join()`.
   */
  get isHost(): boolean {
    return this.#isHost;
  }

  /**
   * The CURRENT meeting-KEK generation this client holds, or `undefined` before
   * one arrives. The generation is already public — it is in every frame's key
   * id — so this is a diagnostic (the test bus's per-participant rotation
   * evidence), never key material.
   */
  get currentKekGeneration(): number | undefined {
    return this.#kekSource.currentForWrapping()?.generation;
  }

  /**
   * Ask MC to server-mute (`muted = true`) or lift a server mute on another
   * participant's `kind` (ADR-0036 §5, story 2 R-8, story 3 R-15). Carries
   * target and action only; MC identifies the requester from the connection and
   * refuses a non-host with a post-join `FORBIDDEN` session `error` (the
   * connection stays up).
   *
   * ONE KIND PER REQUEST: the other kind's field is left absent, which MC
   * reads as "unchanged", so a request about one kind never changes the other
   * (`signaling/muteRequest.ts`).
   *
   * @throws {SignalingError} `Transport` if called before a settled join.
   */
  async setServerMute(participantId: string, kind: PublishKind, muted: boolean): Promise<void> {
    const signaling = this.#requireJoinedSignaling('setServerMute');
    await signaling.sendServerMuteRequest(participantId, kind, muted);
  }

  /**
   * Ask the host to lift THIS participant's server mute (R-10). Only notifies the
   * host; the server-muted state changes when MC says so, never before.
   *
   * @throws {SignalingError} `Transport` if called before a settled join.
   */
  async requestUnmute(): Promise<void> {
    await this.#requireJoinedSignaling('requestUnmute').sendUnmuteRequest();
  }

  #requireJoinedSignaling(what: string): SignalingClient {
    const signaling = this.#signaling;
    if (signaling === undefined || this.#state !== MeetingSessionState.Joined) {
      throw new SignalingError(SignalingErrorCode.Transport, `${what} requires a settled join`);
    }
    return signaling;
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
        void this.#sessionMedia?.release();
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
    // R-18: a roster change to a sender's key purges ITS cached transmit keys
    // (never its replay state). See `mediaWiring.ts`.
    this.#rosterKeys.setTransmitKeyInvalidationListener(
      rosterInvalidationListener(
        () => this.#mediaMetrics?.session,
        () => this.#sessionMedia, // every kind's key cache (`mediaStart.ts`)
      ),
    );
    const opts: SignalingClientOptions = {
      ...(this.#connectFn ? { connect: this.#connectFn } : {}),
      ...(this.#joinTimeoutMs !== undefined ? { joinTimeoutMs: this.#joinTimeoutMs } : {}),
      // The KEK goes from the decode boundary straight into the seam and the
      // decoded field is scrubbed — it never rides an event payload.
      kekSink: this.#kekSource,
      rosterKeys: this.#rosterKeys,
    };
    const signaling = new SignalingClient(opts);
    // Signalling-derived media state first, so the pipeline sees an update no
    // later than an embedder (`signalingMediaState.ts`).
    this.#mediaState.observe(signaling);
    signaling.on('participantJoined', (e) => this.emit('participantJoined', e));
    signaling.on('participantLeft', (e) => this.emit('participantLeft', e));
    signaling.on('error', (e) => this.emit('error', e));
    // ADR-0036 §6 slot state, forwarded verbatim. Bridged HERE rather than in
    // `startMedia()` because it is signalling state, not pipeline state: MC can
    // reassign slots for a client that has not started media, and a UI that
    // renders "waiting for the controller" must see that happen.
    signaling.on('streamAssignments', (e) => this.emit('streamAssignments', e));
    // Server-mute state and the host's unmute-request relay (story 2 R-10/R-11):
    // signalling state, bridged at creation for the same reason.
    signaling.on('participantMuteChanged', (e) => this.emit('participantMuteChanged', e));
    signaling.on('unmuteRequested', (e) => this.emit('unmuteRequested', e));
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
        // The frozen fields, never the options object (story 3 R-2).
        publishKinds: this.#slots.publish,
        receiveSlots: this.#slots.declaration,
        capabilities: { supportedCodecs: this.#vp8?.ok === true ? [SignalingCodec.Vp8] : [] },
      });
      this.#emitSignalingConnection('success', CloseReason.Normal);
      this.#histogram('dt_client_time_to_signaling_ready_ms');
      return joined;
    } catch (err) {
      this.#emitSignalingConnection('failure', closeReasonForError(err));
      throw err;
    }
  }

  #createMedia(signaling: SignalingClient, metrics: MediaMetrics): MediaTransport {
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
      // The MH control channel's input (story 3): a keyframe request for the
      // directed video stream reaches the video publisher's gate; everything
      // else is refused and counted before any gate state is touched.
      controlSink: mhControlSink({
        metrics,
        publish: this.#slots.publish,
        latestDirective: () => this.#mediaState.latestDirective,
        video: () => this.video,
      }),
      ...(this.#connectFn ? { connect: this.#connectFn } : {}),
      ...(this.#connectTimeoutMs !== undefined ? { connectTimeoutMs: this.#connectTimeoutMs } : {}),
      ...(this.#metricsSink ? { metricsSink: this.#metricsSink } : {}),
    };
    return new MediaTransport(opts);
  }

  /** The ONE construction of the media handles (join and the startMedia fallback). */
  #buildMediaMetrics(): MediaMetrics {
    return new MediaMetrics(
      { clientVersion: __SDK_VERSION__, orgId: this.#orgId },
      this.#metricsSink,
    );
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
