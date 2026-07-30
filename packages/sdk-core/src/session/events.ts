// File: packages/sdk-core/src/session/events.ts
//
// R-22: the PUBLIC, plain-typed surface of the `MeetingSession` facade — options,
// the join input (credentials/codes), the explicit state-machine states, and the
// high-level event map. No generated `*_pb` type crosses this boundary.

import type { SdkError } from '../errors/SdkError.js';
import type { FetchLike } from '../http/types.js';
import type { MetricsSink } from '../telemetry/MetricsSink.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type {
  JoinedEvent,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
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
}

/** Construction options for {@link MeetingSession}. */
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
}
