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

import { AuthApiClient } from '../http/AuthApiClient.js';
import { MeetingApiClient } from '../http/MeetingApiClient.js';
import { SignalingClient } from '../signaling/SignalingClient.js';
import type { SignalingClientOptions } from '../signaling/SignalingClient.js';
import type { JoinedEvent } from '../signaling/events.js';
import { MediaTransport } from '../media/MediaTransport.js';
import type { MediaTransportOptions } from '../media/events.js';
import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import { validateUserToken } from '../validation/limits.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import { MeetingUnauthorizedError } from '../errors/MeetingError.js';
import { CloseReason, normalizeCloseReason } from '../telemetry/closeReason.js';
import { configureTelemetry, getMetricsSink } from '../telemetry/telemetryConfig.js';
import type { TelemetryConfig } from '../telemetry/telemetryConfig.js';
import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';

import {
  MeetingSessionState,
  type JoinCredentials,
  type JoinOptions,
  type MeetingSessionEventMap,
  type MeetingSessionOptions,
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
  let hex = '';
  for (const byte of digest) {
    hex += byte.toString(16).padStart(2, '0');
  }
  return hex.slice(0, 16);
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
    const opts: SignalingClientOptions = {
      ...(this.#connectFn ? { connect: this.#connectFn } : {}),
      ...(this.#joinTimeoutMs !== undefined ? { joinTimeoutMs: this.#joinTimeoutMs } : {}),
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
    const opts: MediaTransportOptions = {
      clock: this.#clock,
      metricLabels: this.#metricLabels,
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
