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
import type { MediaCaptureSourceMode } from '../media/setup/mediaMetrics.js';
import type { ReceiveVerificationRecorder } from '../media/pipeline/receiveVerification.js';
import type {
  AudioDecoderFactory,
  AudioEncoderFactory,
  CaptureSourceFactory,
  PlaybackSinkFactory,
} from '../media/setup/seams.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type { MediaFault } from '../media/lifecycle/AudioPipeline.js';
import type { MuteSnapshot } from '../media/lifecycle/muteState.js';
import type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  StreamAssignmentsEvent,
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
}

/** Construction options for {@link MeetingSession}. */

/** Options for `MeetingSession.startMedia`. */
export interface StartMediaOptions {
  /** `MediaDeviceInfo.deviceId` of the chosen microphone; the default when absent. */
  readonly deviceId?: string;
}

/**
 * MC's receive-slot cap as this session learned it from `JoinResponse`
 * (`max_receive_slots`; see its proto comment, the canonical statement).
 *
 *   * `known` — MC advertised a cap. The declaration is checked against it.
 *   * `unknown` — the field was absent: an older MC (the supported rollback), or
 *     no join yet. The SDK declares and MC's whole-declaration rejection is the
 *     enforcer; nothing is inferred.
 *   * `invalid` — MC sent a PRESENT 0, a contract violation. Surfaced (here and
 *     as a console WARN) and then treated like `unknown`, never as "refuse
 *     everything".
 */
export type ReceiveSlotCap =
  | { readonly state: 'known'; readonly value: number }
  | { readonly state: 'unknown' }
  | { readonly state: 'invalid'; readonly value: number };

/**
 * The effective receive-slot configuration, readable at runtime (story 2 R-23):
 * "why can't I hear the sixth person" is answered by these two numbers.
 * Numbers and a bounded source token only.
 */
export interface ReceiveSlotsDiagnostics {
  /** N: the audio receive slots this client declares (slot ids `0..N-1`). */
  readonly declared: number;
  /** Whether N was configured by the embedder or is the SDK default. */
  readonly source: ReceiveSlotsSource;
  /** The server cap, as the latest join advertised it. */
  readonly serverCap: ReceiveSlotCap;
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
   * N, as parsed by `parseReceiveSlots` (e.g. from `VITE_DT_RECEIVE_SLOTS`), with
   * its source. When present it sets `mediaConfig.receive.audioSlots`; when
   * absent N is whatever `mediaConfig` says (the SDK default unless the embedder
   * supplied a media config). See `ReceiveConfig.audioSlots`: N is a REQUEST
   * bounded by MC's `MC_MAX_RECEIVE_SLOTS`, rejected whole above it.
   */
  readonly receiveSlots?: ParsedReceiveSlots;
  /**
   * TEST-ONLY receive verification (story 2 R-30). Absent in production: the web
   * app injects one only inside `if (__E2E_HOOKS__)`. See
   * `media/pipeline/receiveVerification.ts`.
   */
  readonly receiveVerification?: ReceiveVerificationRecorder;
}
