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

/** Sign in with an existing account. `displayName` is the optional roster label. */
export interface LoginCredentials {
  readonly mode: 'login';
  readonly email: string;
  readonly password: string;
  readonly displayName?: string;
}

/** Create a new account, then join. `displayName` is required (becomes the roster label). */
export interface RegisterCredentials {
  readonly mode: 'register';
  readonly email: string;
  readonly password: string;
  readonly displayName: string;
}

/** Discriminated credential carrier for {@link MeetingSession.join}. Held in memory only (R-23). */
export type JoinCredentials = LoginCredentials | RegisterCredentials;

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
