// File: packages/sdk-core/src/session/events.ts
//
// R-22: the PUBLIC, plain-typed surface of the `MeetingSession` facade — options,
// the join input (credentials/codes), the explicit state-machine states, and the
// high-level event map. No generated `*_pb` type crosses this boundary.

import type { SdkError } from '../errors/SdkError.js';
import type { FetchLike } from '../http/types.js';
import type { MetricsSink } from '../telemetry/MetricsSink.js';
import type {
  MediaConfig,
  ParsedReceiveSlots,
  ReceiveSlotsSource,
} from '../config/clientConfig.js';
import type {
  MediaCaptureFailureOutcome,
  MediaCaptureSourceMode,
} from '../media/setup/mediaMetrics.js';
import type { ReceiveVerificationRecorder } from '../media/pipeline/receiveVerification.js';
import type {
  AudioDecoderFactory,
  AudioEncoderFactory,
  CaptureSourceFactory,
  PlaybackSinkFactory,
  VideoCaptureSourceFactory,
  VideoDecoderFactory,
  VideoEncoderFactory,
} from '../media/setup/seams.js';
import type { VideoRenderState } from '../media/pipeline/renderDecision.js';
import type { Vp8Profile } from '../media/setup/vp8.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type { MediaFault } from '../media/lifecycle/AudioPipeline.js';
import type { MuteSnapshot } from '../media/lifecycle/muteState.js';
import type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  ParticipantMuteEvent,
  PublishKind,
  StreamAssignmentsEvent,
  UnmuteRequestedEvent,
} from '../signaling/events.js';

/**
 * Explicit `MeetingSession` lifecycle states (R-22). Const-object union (mirroring
 * the codebase `SignalingErrorCode` idiom) so consumers branch on stable values and
 * observability/UI map them without coupling to internals.
 */
export const MeetingSessionState = {
  Idle: 'idle',
  FetchingToken: 'fetching-token',
  ConnectingMc: 'connecting-mc',
  Joining: 'joining',
  Joined: 'joined',
  Disconnecting: 'disconnecting',
} as const;

export type MeetingSessionState = (typeof MeetingSessionState)[keyof typeof MeetingSessionState];

/**
 * Join with a user access token the caller already holds — **the path apps should
 * use**. No AC round-trip: `join()` presents this token to GC directly.
 *
 * Prefer this whenever the app has already authenticated. Re-authenticating at join
 * time means retaining the password past the token exchange that made it
 * unnecessary, which is a credential-minimisation defect (and was a real one — it
 * also made the register-vs-login mismatch possible).
 *
 * `displayName` is the roster label; it is optional here because a token-holding app
 * may not have one (AC's token response carries no identity fields).
 */
export interface TokenCredentials {
  readonly mode: 'token';
  readonly userToken: string;
  readonly displayName?: string;
}

/**
 * Sign in with an existing account, then join. `displayName` is the optional roster label.
 *
 * **Standalone-join only.** Legal solely where the credential is a call-scoped
 * argument that no caller retains — i.e. reached through `join(options)` and never
 * stored on a field, in component state, or in a store. Prefer {@link TokenCredentials}
 * if you have already authenticated. That criterion is enforced mechanically, not by
 * convention: `dt-guard ts-no-retained-credentials` flags any *retained* type carrying
 * a credential field, so this interface needs no exemption — it passes on the merits
 * because a function-parameter annotation is not a retention site.
 */
export interface LoginCredentials {
  readonly mode: 'login';
  readonly email: string;
  readonly password: string;
  readonly displayName?: string;
}

/**
 * Create a new account, then join. `displayName` is required (becomes the roster label).
 *
 * **Standalone-join only** — same criterion as {@link LoginCredentials}.
 */
export interface RegisterCredentials {
  readonly mode: 'register';
  readonly email: string;
  readonly password: string;
  readonly displayName: string;
}

/**
 * Discriminated credential carrier for {@link MeetingSession.join}. Held in memory
 * only (R-23), and — for the password-bearing variants — only for the duration of the
 * `join()` call.
 */
export type JoinCredentials = TokenCredentials | LoginCredentials | RegisterCredentials;

/** Parameters for {@link MeetingSession.join}. */
export interface JoinOptions {
  /** Org subdomain selecting the AC origin (validated before URL interpolation). */
  readonly orgSubdomain: string;
  /** Meeting code (12 base62) — validated client-side before the GC request. */
  readonly meetingCode: string;
  /** Login or register credentials. */
  readonly credentials: JoinCredentials;
}

/** The typed high-level events {@link MeetingSession} emits (R-22). */
export interface MeetingSessionEventMap {
  /** Emitted once when the MC join completes (`JoinResponse`). */
  joined: JoinedEvent;
  /** Emitted on each `ParticipantJoined` (bridged from SignalingClient). */
  participantJoined: ParticipantJoinedEvent;
  /** Emitted on each `ParticipantLeft` (bridged from SignalingClient). */
  participantLeft: ParticipantLeftEvent;
  /** Emitted per MH whose handshake succeeded (payload: the MH URL). */
  mediaConnected: string;
  /** Emitted on any terminal/post-join error (signaling error or all-MH-failed). */
  error: SdkError;
  /** Emitted on every state-machine transition (the new state). */
  stateChange: MeetingSessionState;

  // --------------------------------------------------------------------------
  // THE FOUR ABSENCE-SIGNALS (ADR-0036 §5/§6/§10)
  // --------------------------------------------------------------------------
  //
  // In the media path, "nothing is arriving" is ambiguous. §6 is explicit that
  // absence of frames is not a signal, and §5 requires the mute signal to travel
  // out of band rather than be inferred from absence — *muted*, *silent* and
  // *the network died* are indistinguishable to a relay.
  //
  // So every condition an embedder might be tempted to infer from silence is
  // published here as an explicit event with ONE authoritative holder inside the
  // SDK. A UI that derives any of these from frame absence inverts the dangerous
  // way: it shows "muted" during a transport stall.
  //
  // These are BRIDGES, not new state. `muteChanged` / `firstMediaFrame` /
  // `mediaFault` are forwarded verbatim from the running `AudioPipeline` (also
  // reachable as `MeetingSession.media`), and `streamAssignments` verbatim from
  // `SignalingClient`. Nothing is recomputed on the way through, so an embedder
  // and the SDK cannot disagree about any of them.

  /**
   * Client-mute state changed (§5). **The only source a UI may render a mute
   * indicator from.** Fires on real transitions only, so a UI that sets the same
   * value twice does not see two events.
   */
  muteChanged: MuteSnapshot;
  /**
   * The first media frame arrived, in ms since media start (§10).
   *
   * The same value as `dt_client_time_to_first_media_frame_ms` — one measurement,
   * two consumers. **Observed, never gated**: §10 forbids asserting a wall-clock
   * target on it, and this event carries no threshold, target or comparison.
   */
  firstMediaFrame: number;
  /**
   * MC's current slot assignments (§6), with each slot's state explicit on the
   * wire. This is where "the far end is muted", "withheld by congestion",
   * "fewer sources than slots" and "source unreachable" come from — all four
   * present as no media and render completely differently.
   */
  streamAssignments: StreamAssignmentsEvent;
  /**
   * The media pipeline hit a bounded, SDK-authored fault. At most once per
   * stage, never per frame.
   *
   * Separate from `error`, which is terminal for the SESSION: a media fault
   * leaves signaling joined and healthy, because degraded audio beats a dropped
   * meeting. Surfaced rather than absorbed so a broken pipeline is visible
   * instead of presenting as silence.
   */
  mediaFault: MediaFault;
  /**
   * A participant's SERVER mute changed (or was replayed at join) — ADR-0036 §5,
   * story 2 R-11. Includes this participant's own. The ONLY source for rendering
   * server-muted state: nothing flips it locally, not even this client's own
   * unmute request.
   */
  participantMuteChanged: ParticipantMuteEvent;
  /** A server-muted participant asked the host to unmute them (host only; R-10). */
  unmuteRequested: UnmuteRequestedEvent;
  /**
   * A host's server UNMUTE resumed this participant's capture or send for a
   * kind (story 3 R-18). Fires when the kind's EFFECTIVE send targets go from
   * empty to non-empty while a server mute of that kind was recorded — in
   * either arrival order relative to the unmute update, and even if that update
   * is lost — and never when the participant is client-muted for that kind
   * (nothing resumes then). A UI MUST show a non-suppressible notice for it
   * (story 3 C-C1). Carries the kind ONLY.
   */
  captureResumedAfterServerMute: CaptureResumedAfterServerMuteEvent;
  /**
   * What one VIDEO receive slot renders changed (story 3 R-4, R-5): live from a
   * verified sender, an avatar for MC's named participant with MC's reason,
   * fewer sources, unavailable, or none (fail closed). TRANSITIONS ONLY, never
   * per frame. Frames themselves never travel on an event: they are drawn
   * through the slot's `VideoSink` (`MeetingSession.videoReceive.setVideoSink`).
   */
  videoSlotRenderChanged: VideoSlotRenderChangedEvent;
  /**
   * This participant's CAMERA health changed (story 3 R-19): `failure` is the
   * camera gate's latched failure (capture refused, no device, the track
   * ended, ...) or `undefined` once healthy again (a user mute/unmute or a
   * successful restore). While failed the participant is presented to others
   * as video-muted (the `muteChanged` that accompanies it); this event is what
   * lets a UI tell the participant WHY. A closed token, never a platform
   * string. Fires on transitions only.
   */
  cameraHealthChanged: CameraHealthChangedEvent;
}

/** Payload of `cameraHealthChanged`. */
export interface CameraHealthChangedEvent {
  readonly failure: MediaCaptureFailureOutcome | undefined;
}

/** Payload of `videoSlotRenderChanged`. */
export interface VideoSlotRenderChangedEvent {
  readonly slotId: number;
  readonly state: VideoRenderState;
}

/** Payload of `captureResumedAfterServerMute`: the kind only, never an identity. */
export interface CaptureResumedAfterServerMuteEvent {
  readonly kind: PublishKind;
}

/** Construction options for {@link MeetingSession}. */

/** Options for `MeetingSession.startMedia`. */
export interface StartMediaOptions {
  /** `MediaDeviceInfo.deviceId` of the chosen microphone; the default when absent. */
  readonly deviceId?: string;
}

/**
 * MC's per-kind receive-slot cap as this session learned it from `JoinResponse`
 * (`max_audio_receive_slots` / `max_video_receive_slots`; the proto comment is
 * the canonical statement).
 *
 *   * `known` — MC advertised a cap. 0 is LEGAL (a video cap of 0 is the
 *     video-off lever, story 3 R-26), never folded into `unknown`.
 *   * `unknown` — absent: an MC that does not send them, or no join yet.
 *     Never coerced to 0, which would read as "video disabled".
 *
 * Informational: MC TRUNCATES an over-cap declaration (excess slots report
 * `unavailable`), so a client never shrinks its declaration from this.
 */
export type ReceiveSlotCap =
  { readonly state: 'known'; readonly value: number } | { readonly state: 'unknown' };

/** One kind's effective receive-slot configuration. */
export interface KindSlotsDiagnostics {
  /** The slots of this kind this client declares. */
  readonly declared: number;
  /**
   * This kind's declared slot ids, in declaration order — from the ONE producer
   * of slot ids (`mediaSelection.ts:receiveDeclaration`), so an embedder never
   * restates the id layout.
   */
  readonly slotIds: readonly number[];
  /** Whether the count was configured by the embedder or is the SDK default. */
  readonly source: ReceiveSlotsSource;
  /** This kind's server cap, as the latest join advertised it. */
  readonly serverCap: ReceiveSlotCap;
}

/**
 * The effective publish set and receive-slot configuration, readable at
 * runtime (story 2 R-23; story 3 R-23): "why can't I hear/see the sixth
 * person" is answered by these numbers. Numbers and bounded tokens only.
 */
export interface ReceiveSlotsDiagnostics {
  /** What this session publishes — frozen, fixed for the membership. */
  readonly publish: readonly PublishKind[];
  /** N audio slots (ids `0..N-1`). */
  readonly audio: KindSlotsDiagnostics;
  /** M video slots (ids `N..N+M-1`). */
  readonly video: KindSlotsDiagnostics;
}

/**
 * What feeds this session's send path, once `startMedia()` has chosen it.
 * `toneHz` is present only in a test-tone build (`__DT_TEST_TONE__`): the
 * frequency derived from this participant's meeting-scoped sender id, so a test
 * harness reads expected tones rather than restating the derivation.
 */
export interface CaptureSourceInfo {
  readonly mode: MediaCaptureSourceMode;
  readonly toneHz?: number;
}

/**
 * What feeds the VIDEO send path (story 3 R-27). `patternId` only in a
 * test-pattern build: the meeting-scoped sender id the pattern encodes (the
 * test bus's `videoCaptureSource.patternId`). Never a metric label.
 */
export interface VideoCaptureSourceInfo {
  readonly mode: 'camera' | 'test_pattern';
  readonly patternId?: number;
}

export interface MeetingSessionOptions {
  /** AC origin template containing `{subdomain}` (e.g. `https://{subdomain}.localhost:8443`). */
  readonly acOriginTemplate: string;
  /** GC base URL (e.g. `https://localhost:8444`). */
  readonly gcBaseUrl: string;
  /** `fetch` implementation; defaults to `globalThis.fetch`. Injected for tests. */
  readonly fetchImpl?: FetchLike;
  /** WebTransport factory for MC + MH; defaults to the production `connect`. Injected for tests. */
  readonly connect?: WebTransportConnectFn;
  /** Wall-clock source (default `Date.now`). Injected for deterministic tests. */
  readonly clock?: () => number;
  /** Per-MH connect deadline in ms. */
  readonly connectTimeoutMs?: number;
  /** MC join deadline in ms (forwarded to SignalingClient). */
  readonly joinTimeoutMs?: number;
  /** Metrics sink (default `getMetricsSink()`; no-op when telemetry unconfigured). */
  readonly metricsSink?: MetricsSink;
  /**
   * Media-path configuration. Defaults to `DEFAULT_CLIENT_CONFIG.media`.
   *
   * Validated at pipeline construction — including the two RELATIONSHIPS that
   * matter (the application egress bound must exceed the transport high-water
   * mark, and the user agent's age bound must sit clear of what both queues can
   * hold), which throw rather than clamp.
   */
  readonly mediaConfig?: MediaConfig;
  /** Microphone capture factory. Defaults to `getUserMedia`; injected for tests. */
  readonly captureFactory?: CaptureSourceFactory;
  /** Opus encoder factory. Defaults to WebCodecs; injected for tests. */
  readonly encoderFactory?: AudioEncoderFactory;
  /** Opus decoder factory. Defaults to WebCodecs; injected for tests. */
  readonly decoderFactory?: AudioDecoderFactory;
  /** Playback sink factory. Defaults to `AudioContext`; injected for tests. */
  readonly playbackFactory?: PlaybackSinkFactory;
  /**
   * What this participant publishes (story 3 R-2). Default
   * `config/clientConfig.ts:DEFAULT_PUBLISH`. FIXED
   * FOR THE MEMBERSHIP: copied and frozen at construction. `audio`, `video`,
   * both, or neither; a duplicate throws `PublishKindNotSupported`, and
   * publishing nothing with 0 + 0 slots throws `NothingToPublishOrReceive`.
   */
  readonly publish?: readonly PublishKind[];
  /** Camera capture factory. Defaults to `getUserMedia({video})`; injected for tests. */
  readonly videoCaptureFactory?: VideoCaptureSourceFactory;
  /** VP8 encoder factory. Defaults to WebCodecs; injected for tests. */
  readonly videoEncoderFactory?: VideoEncoderFactory;
  /**
   * VP8 DECODER factory, one call per video receive slot (story 3 C-B).
   * Defaults to WebCodecs; injected for tests — its call count is the R-6 test
   * seam (exactly M; a source switch constructs none).
   */
  readonly videoDecoderFactory?: VideoDecoderFactory;
  /**
   * The VP8 capability probe run at join when video is published or received
   * (story 3 R-8). Defaults to `media/setup/vp8.ts:probeVp8Support`; injected
   * for tests. Rejecting means "not supported".
   */
  readonly vp8Probe?: (profile: Vp8Profile) => Promise<void>;
  /** Timer seams for the video keyframe gate and rotation (tests). */
  readonly setTimeout?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  readonly clearTimeout?: (handle: ReturnType<typeof setTimeout>) => void;
  readonly setInterval?: (fn: () => void, ms: number) => ReturnType<typeof setInterval>;
  readonly clearInterval?: (handle: ReturnType<typeof setInterval>) => void;
  /**
   * N, as parsed by `parseReceiveSlots(…, 'audio')` (e.g. from
   * `VITE_DT_AUDIO_RECEIVE_SLOTS`), with its source. When present it sets
   * `mediaConfig.receive.audioSlots`; when absent N is whatever `mediaConfig`
   * says. A REQUEST: MC truncates above its per-kind cap (see `ReceiveConfig`).
   */
  readonly audioReceiveSlots?: ParsedReceiveSlots;
  /** M, as parsed by `parseReceiveSlots(…, 'video')` (`VITE_DT_VIDEO_RECEIVE_SLOTS`). */
  readonly videoReceiveSlots?: ParsedReceiveSlots;
  /**
   * TEST-ONLY receive verification (story 2 R-30). Absent in production: the web
   * app injects one only inside `if (__E2E_HOOKS__)`. See
   * `media/pipeline/receiveVerification.ts`.
   */
  readonly receiveVerification?: ReceiveVerificationRecorder;
}
