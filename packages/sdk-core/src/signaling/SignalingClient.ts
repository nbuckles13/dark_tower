// File: packages/sdk-core/src/signaling/SignalingClient.ts
//
// R-16/R-17/R-18/R-19: the WebTransport signaling layer connecting the browser
// SDK to the Meeting Controller (MC). FIRST consumer of the generated protobuf-es
// types in sdk-core.
//
// Mechanism (one place): `join()` → connect (injected factory, default prod
// `connect`) → await `ready` → open the FIRST bidi stream → build a
// `ClientMessage{joinRequest}` (empty correlation/binding for first join) → inject
// W3C trace (R-19) → `toBinary` → `encodeFrame` (reused 4-byte BE framing) → write.
// A long-lived read loop then pulls chunks through the reused `FrameDecoder`
// (partial-read + oversize bounded), decodes each frame as a `ServerMessage`, and
// dispatches on `sm.message.case`.
//
// R-19 tracing: a SINGLE root span `dt_client.join` per `join()` call (via
// `getTracer()`); the JoinRequest is built + injected inside
// `context.with(trace.setSpan(active, span))` so the registered StackContextManager
// (installed by `configureTelemetry`) makes the span active at inject time. When
// telemetry is unconfigured `getTracer()` is `undefined` ⇒ no span and injection is
// a safe no-op. The span is ended exactly once on settle (success or terminal
// error); error settles `recordException` ONLY the SignalingError (never the
// token-bearing JoinRequest).
//
// DRY: the send path + read loop are kept as two cohesive, liftable private
// methods (`#sendClientMessage` / `#runReadLoop`) — task #14 (MhClientMessage /
// MediaTransport) lifts them on second use (extract-on-second-use, per dry review).
//
// R-23: the join token is held only as a local `join()` parameter and placed only
// into the `JoinRequest` proto on the send path. It is NEVER stored on the
// instance, logged, or placed on any error. Unknown ServerMessage variants log the
// `message.case` discriminant only — never the decoded payload.

import { create, fromBinary } from '@bufbuild/protobuf';
import { timestampFromDate } from '@bufbuild/protobuf/wkt';
import { context, SpanStatusCode, trace } from '@opentelemetry/api';
import type { Span } from '@opentelemetry/api';

import { FrameDecoder, FramingError } from '../framing/length-prefix.js';
import { sendFramedMessage } from '../framing/sendFramed.js';
import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import { getTracer } from '../telemetry/telemetryConfig.js';
import { capMhUrl } from '../validation/limits.js';
import { connect as defaultConnect } from '../transport/BrowserWebTransport.js';
import type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
} from '../transport/IWebTransport.js';
import type { WebTransportConnectOptions } from '../transport/types.js';
import { SignalingError, SignalingErrorCode } from '../errors/SignalingError.js';
import {
  ClientMessageSchema,
  ConnectionState,
  ErrorCode,
  JoinRequestSchema,
  MediaConnectionUpdateSchema,
  MhConnectionStatusSchema,
  ParticipantCapabilitiesSchema,
  ServerMessageSchema,
} from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import type {
  ClientMessage,
  ErrorMessage,
  ServerMessage,
} from '../proto/dark_tower/signaling/v1/signaling_pb.js';

import { isAuthClass, mapErrorCode, staticMessageFor } from './errorCodeMap.js';
import { mapLeaveReason } from './events.js';
import type {
  JoinedEvent,
  MhConnectionStatusReport,
  ParticipantJoinedEvent,
  ParticipantLeftEvent,
  SignalingJoinParams,
} from './events.js';

/**
 * Application close code used when the connection is closed for an auth-class
 * signaling error (R-18 "typed reason"). `normalizeCloseReason(3401)` maps it to
 * `CloseReason.AuthFailed` (see `telemetry/closeReason.ts`), keeping the bounded
 * label consistent for R-22.
 */
const AUTH_CLOSE_CODE = 3401;
const AUTH_CLOSE_REASON = 'auth';

/**
 * Default join deadline (ms). If MC accepts the bidi stream + JoinRequest but never
 * sends a `JoinResponse` and never closes (stalled/wedged MC, half-open path), the
 * read loop would block forever in `reader.read()` and `join()` would hang silently.
 * The deadline guarantees `join()` settles with a typed `SignalingErrorCode.Timeout`
 * and the transport/reader are torn down. NOT reconnection logic (out of scope).
 */
const DEFAULT_JOIN_TIMEOUT_MS = 15_000;

/** Factory that initiates a transport handshake (matches the prod `connect`). */
export type WebTransportConnectFn = (
  url: string,
  options?: WebTransportConnectOptions,
) => IWebTransport;

/** Minimal debug sink for unhandled-variant logging (defaults to `console`). */
export interface SignalingLogger {
  debug(message: string): void;
}

/** Construction options for {@link SignalingClient}. */
export interface SignalingClientOptions {
  /**
   * Transport factory. Defaults to the production `connect`. Tests inject a
   * factory returning a `MockWebTransport`.
   */
  readonly connect?: WebTransportConnectFn;
  /** Debug sink for unhandled-variant logs. Defaults to `console`. */
  readonly logger?: SignalingLogger;
  /**
   * Join deadline in ms (default {@link DEFAULT_JOIN_TIMEOUT_MS} = 15000). If a
   * `JoinResponse` has not arrived within this window, `join()` rejects with
   * `SignalingErrorCode.Timeout` and the transport is torn down. A non-finite or
   * `<= 0` value disables the timeout (opt-out).
   */
  readonly joinTimeoutMs?: number;
}

/** The typed events SignalingClient emits. */
export interface SignalingEventMap {
  /** Emitted once when `JoinResponse` is received (also resolves `join()`). */
  joined: JoinedEvent;
  /** Emitted on each `ParticipantJoined`. */
  participantJoined: ParticipantJoinedEvent;
  /** Emitted on each `ParticipantLeft`. */
  participantLeft: ParticipantLeftEvent;
  /** Emitted on a signaling error that occurs AFTER join has settled. */
  error: SignalingError;
}

interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve(value: T): void;
  reject(reason: unknown): void;
}

function defer<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const defaultLogger: SignalingLogger = {
  debug(message: string): void {
    // Bounded debug sink (case-name only; never the payload) — see R-23/R-26.
    console.debug(message);
  },
};

/**
 * WebTransport signaling client to the Meeting Controller. Single-use: one
 * `join()` per instance (mirrors a single signaling session); construct a new
 * instance to reconnect.
 *
 * @example
 * const client = new SignalingClient();
 * // Never log participant display names (PII, R-23/R-26) — branch on the opaque id.
 * client.on('participantJoined', (e) => console.log('participant joined', e.participant.participantId));
 * const joined = await client.join({
 *   webtransportEndpoint, meetingId, joinToken, participantName,
 * });
 */
export class SignalingClient extends TypedEventEmitter<SignalingEventMap> {
  readonly #connectFn: WebTransportConnectFn;
  readonly #logger: SignalingLogger;
  readonly #joinTimeoutMs: number;

  #transport: IWebTransport | undefined;
  #reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  #stream: WebTransportBidirectionalStream | undefined;
  #span: Span | undefined;
  #join!: Deferred<JoinedEvent>;
  #joinTimer: ReturnType<typeof setTimeout> | undefined;

  #started = false;
  #joinSettled = false;
  #terminated = false;

  #correlationId: string | undefined;
  #bindingToken: string | undefined;

  constructor(options: SignalingClientOptions = {}) {
    super();
    this.#connectFn = options.connect ?? defaultConnect;
    this.#logger = options.logger ?? defaultLogger;
    this.#joinTimeoutMs = options.joinTimeoutMs ?? DEFAULT_JOIN_TIMEOUT_MS;
  }

  /**
   * The `correlation_id` from `JoinResponse`, stored for future reconnection
   * (storage only this story). `undefined` until joined.
   */
  get correlationId(): string | undefined {
    return this.#correlationId;
  }

  /**
   * The `binding_token` from `JoinResponse`, stored for future reconnection
   * (storage only this story). `undefined` until joined.
   */
  get bindingToken(): string | undefined {
    return this.#bindingToken;
  }

  /**
   * Connect to MC, send the `JoinRequest`, and resolve with the typed
   * {@link JoinedEvent} when `JoinResponse` arrives. Rejects with a
   * {@link SignalingError} on auth/framing/transport failure before join settles.
   * May only be called once per instance.
   */
  join(params: SignalingJoinParams): Promise<JoinedEvent> {
    if (this.#started) {
      return Promise.reject(
        new SignalingError(
          SignalingErrorCode.Transport,
          'SignalingClient.join() may only be called once',
        ),
      );
    }
    this.#started = true;
    this.#join = defer();
    this.#span = getTracer()?.startSpan('dt_client.join');
    // Bounded join deadline: if no JoinResponse (and no close) arrives, settle with a
    // typed Timeout instead of hanging forever in `reader.read()` (per @operations).
    // The timer is cleared on settle so it never outlives or post-fires the join.
    if (Number.isFinite(this.#joinTimeoutMs) && this.#joinTimeoutMs > 0) {
      this.#joinTimer = setTimeout(() => {
        if (!this.#joinSettled) {
          this.#terminate(
            new SignalingError(
              SignalingErrorCode.Timeout,
              staticMessageFor(SignalingErrorCode.Timeout),
            ),
          );
        }
      }, this.#joinTimeoutMs);
    }
    // The connect/send pipeline runs in the background; any failure routes to
    // #terminate so `join()` (and only `join()`) surfaces it. The `.catch` here
    // guarantees no unhandled rejection regardless of timing.
    void this.#connectAndJoin(params).catch((err: unknown) => {
      this.#terminate(this.#toSignalingError(err));
    });
    return this.#join.promise;
  }

  /** Tear down the connection and read loop. Idempotent. */
  close(): void {
    this.#terminate();
  }

  // ----------------------------------------------------------------------------
  // Internal: connect + send (cohesive, liftable for task #14)
  // ----------------------------------------------------------------------------

  async #connectAndJoin(params: SignalingJoinParams): Promise<void> {
    const transport = this.#connectFn(params.webtransportEndpoint);
    this.#transport = transport;

    // Subscribe to the terminal close hook BEFORE awaiting ready so an early
    // close/error still routes to teardown.
    transport.closed.then(
      (info) => {
        this.#onTransportClosed(info, undefined);
      },
      (err: unknown) => {
        this.#onTransportClosed(undefined, err);
      },
    );

    await transport.ready;
    if (this.#terminated) return;

    // R-16: the FIRST bidirectional stream carries the JoinRequest. Stored so
    // post-join sends (task #14 `sendMediaConnectionUpdate`) reuse the SAME stream.
    const stream = await transport.createBidirectionalStream();
    this.#stream = stream;
    if (this.#terminated) return;

    const caps = params.capabilities;
    const capabilities = create(ParticipantCapabilitiesSchema, {
      videoCodecs: caps?.videoCodecs ? [...caps.videoCodecs] : [],
      audioCodecs: caps?.audioCodecs ? [...caps.audioCodecs] : [],
      supportsSimulcast: caps?.supportsSimulcast ?? false,
      maxVideoStreams: caps?.maxVideoStreams ?? 0,
    });
    const joinRequest = create(JoinRequestSchema, {
      meetingId: params.meetingId,
      joinToken: params.joinToken,
      participantName: params.participantName,
      capabilities,
      // First-time join: correlation/binding are empty (ADR-0023).
      correlationId: '',
      bindingToken: '',
    });
    const clientMessage = create(ClientMessageSchema, {
      message: { case: 'joinRequest', value: joinRequest },
    });

    // R-19: build+inject under the active span context so the W3C propagator
    // writes trace_parent/trace_state onto the ClientMessage. Injection happens
    // synchronously inside `context.with`; the write proceeds after.
    const span = this.#span;
    const ctx = span ? trace.setSpan(context.active(), span) : context.active();
    await context.with(ctx, () => this.#sendClientMessage(stream, clientMessage));

    // Long-lived read loop. Launched detached; it owns its own error routing and
    // can never produce an unhandled rejection.
    void this.#runReadLoop(stream);
  }

  /**
   * Thin private wrapper over the shared {@link sendFramedMessage} pipeline (the
   * extract-on-second-use: the SAME inject/frame/write core also serves the MH
   * `MhClientMessage` send in `MediaTransport`). Kept private so `join()` and
   * `sendMediaConnectionUpdate()` route their `ClientMessage` sends through
   * SignalingClient's own method; the single `injectIntoClientMessage` call lives
   * inside the shared leaf. Callers wrap this in `context.with(...)` for trace.
   */
  async #sendClientMessage(
    stream: WebTransportBidirectionalStream,
    clientMessage: ClientMessage,
  ): Promise<void> {
    await sendFramedMessage(stream, ClientMessageSchema, clientMessage);
  }

  /**
   * Run `fn` SYNCHRONOUSLY inside the active `dt_client.join` span context (R-58).
   * Lets a caller (e.g. `MediaTransport.connectAll`) inject W3C trace context onto
   * an outbound MH envelope from the SAME join trace this client owns — WITHOUT
   * relocating span ownership. The wrap MUST be synchronous around the inject call
   * (the global StackContextManager does not survive an `await`). No-op (`fn()`
   * under the current context) when telemetry is unconfigured (`#span` undefined).
   */
  runInJoinContext<T>(fn: () => T): T {
    const span = this.#span;
    if (span === undefined) return fn();
    return context.with(trace.setSpan(context.active(), span), fn);
  }

  /**
   * Send ONE post-join `MediaConnectionUpdate` over the existing MC stream (R-60/R-21),
   * reporting per-MH terminal state. Maps the plain {@link MhConnectionStatusReport}s
   * to proto `MhConnectionStatus` internally (no generated type crosses the public
   * boundary). `mhUrl`/`failureReason`/`failureCode` are capped ≤256 UTF-8 bytes;
   * `observedAtMs` → proto `Timestamp`. Trace context is injected on the SAME single
   * path `join()` uses (inside `context.with(#span)`).
   *
   * @throws {SignalingError} `Transport` if called before join settled or after teardown.
   */
  async sendMediaConnectionUpdate(reports: readonly MhConnectionStatusReport[]): Promise<void> {
    const stream = this.#stream;
    if (stream === undefined || this.#terminated || !this.#joinSettled) {
      throw new SignalingError(
        SignalingErrorCode.Transport,
        'sendMediaConnectionUpdate requires a settled, open join',
      );
    }
    const statuses = reports.map((r) =>
      create(MhConnectionStatusSchema, {
        mhUrl: capMhUrl(r.mhUrl),
        state: r.state === 'connected' ? ConnectionState.CONNECTED : ConnectionState.FAILED,
        ...(r.failureReason !== undefined ? { failureReason: capMhUrl(r.failureReason) } : {}),
        ...(r.failureCode !== undefined ? { failureCode: capMhUrl(r.failureCode) } : {}),
        observedAt: timestampFromDate(new Date(r.observedAtMs)),
      }),
    );
    const update = create(MediaConnectionUpdateSchema, { statuses });
    const clientMessage = create(ClientMessageSchema, {
      message: { case: 'mediaConnectionUpdate', value: update },
    });
    await this.runInJoinContext(() => this.#sendClientMessage(stream, clientMessage));
  }

  // ----------------------------------------------------------------------------
  // Internal: read loop + dispatch (cohesive, liftable for task #14)
  // ----------------------------------------------------------------------------

  async #runReadLoop(stream: WebTransportBidirectionalStream): Promise<void> {
    const reader = stream.readable.getReader();
    this.#reader = reader;
    const decoder = new FrameDecoder();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value === undefined) continue;
        let frames: Uint8Array[];
        try {
          frames = decoder.push(value);
        } catch (err) {
          // FramingError (oversize/empty/decode) — surface and stop.
          this.#terminate(this.#toSignalingError(err));
          return;
        }
        for (const frame of frames) {
          if (this.#terminated) return;
          const serverMessage = fromBinary(ServerMessageSchema, frame);
          this.#dispatch(serverMessage);
        }
      }
      // Peer closed the stream cleanly.
      this.#onStreamEnd();
    } catch (err) {
      this.#terminate(this.#toSignalingError(err));
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Reader already released/cancelled during teardown — ignore.
      }
    }
  }

  #dispatch(serverMessage: ServerMessage): void {
    const message = serverMessage.message;
    switch (message.case) {
      case 'joinResponse': {
        const jr = message.value;
        // R-17: store for future reconnection (storage only).
        this.#correlationId = jr.correlationId;
        this.#bindingToken = jr.bindingToken;
        const event: JoinedEvent = {
          participantId: jr.participantId,
          userId: jr.userId,
          existingParticipants: jr.existingParticipants.map((p) => ({
            participantId: p.participantId,
            name: p.name,
          })),
          mediaServers: jr.mediaServers.map((m) => m.mediaHandlerUrl),
          correlationId: jr.correlationId,
          bindingToken: jr.bindingToken,
        };
        this.emit('joined', event);
        this.#settleJoinSuccess(event);
        return;
      }
      case 'participantJoined': {
        const participant = message.value.participant;
        if (participant !== undefined) {
          this.emit('participantJoined', {
            participant: { participantId: participant.participantId, name: participant.name },
          });
        }
        return;
      }
      case 'participantLeft': {
        const pl = message.value;
        this.emit('participantLeft', {
          participantId: pl.participantId,
          reason: mapLeaveReason(pl.reason),
        });
        return;
      }
      case 'error': {
        this.#handleErrorMessage(message.value);
        return;
      }
      default: {
        // R-17: decode succeeded; unhandled variant is debug-logged by CASE NAME
        // ONLY (never the server-controlled payload — R-23). Does not throw.
        this.#logger.debug(
          `SignalingClient: ignoring unhandled ServerMessage variant '${message.case ?? 'unset'}'`,
        );
        return;
      }
    }
  }

  #handleErrorMessage(errorMessage: ErrorMessage): void {
    const signalingCode = mapErrorCode(errorMessage.code);
    // Proto enum NAME (e.g. "UNAUTHORIZED") for the non-secret `serverCode`.
    const serverCode: string | undefined = ErrorCode[errorMessage.code];
    // Server `message` is allowed on `SdkError.message` (transitive-PII contract);
    // fall back to a static message when empty. The `details` map is dropped.
    const text =
      errorMessage.message !== '' ? errorMessage.message : staticMessageFor(signalingCode);
    const error = new SignalingError(signalingCode, text, {
      ...(serverCode !== undefined ? { serverCode } : {}),
    });
    this.#raise(error, isAuthClass(signalingCode));
  }

  // ----------------------------------------------------------------------------
  // Internal: settle / terminate / teardown
  // ----------------------------------------------------------------------------

  /** Surface a SignalingError; auth-class additionally closes the connection. */
  #raise(error: SignalingError, closeConnection: boolean): void {
    if (!this.#joinSettled) {
      this.#settleJoinFailure(error);
    } else {
      this.emit('error', error);
    }
    if (closeConnection) {
      // R-18: typed close reason. Triggers `transport.closed` → #onTransportClosed
      // (guarded), which tears the read loop down.
      this.#transport?.close({ closeCode: AUTH_CLOSE_CODE, reason: AUTH_CLOSE_REASON });
    }
  }

  #onTransportClosed(info: WebTransportCloseInfo | undefined, err: unknown): void {
    if (this.#terminated) return;
    const closeCode = info?.closeCode;
    if (!this.#joinSettled) {
      this.#terminate(this.#transportError(closeCode, err));
    } else if (err !== undefined) {
      this.#terminate(this.#transportError(closeCode, err));
    } else {
      // Graceful post-join close — tear down without surfacing an error.
      this.#terminate();
    }
  }

  #onStreamEnd(): void {
    if (this.#terminated) return;
    if (!this.#joinSettled) {
      this.#terminate(this.#transportError(undefined, undefined));
    } else {
      this.#terminate();
    }
  }

  /**
   * Single terminal path. Idempotent. When `error` is present: reject `join()` if
   * still pending, otherwise emit `'error'`. Always tears down the transport +
   * reader.
   */
  #terminate(error?: SignalingError): void {
    if (this.#terminated) return;
    this.#terminated = true;
    if (error !== undefined) {
      if (this.#started && !this.#joinSettled) {
        this.#settleJoinFailure(error);
      } else if (this.#joinSettled) {
        this.emit('error', error);
      }
    } else if (this.#started && !this.#joinSettled) {
      // Graceful teardown before join completed → reject the pending join.
      this.#settleJoinFailure(this.#transportError(undefined, undefined));
    }
    this.#teardown();
  }

  #teardown(): void {
    const reader = this.#reader;
    if (reader !== undefined) {
      try {
        const cancelled = reader.cancel();
        // Swallow cancel rejection (e.g. stream already errored).
        void cancelled.catch(() => {});
      } catch {
        // Reader already released — ignore.
      }
    }
    // Idempotent: a no-op if already closed (e.g. an auth-class `#raise` close).
    this.#transport?.close();
  }

  #clearJoinTimer(): void {
    if (this.#joinTimer !== undefined) {
      clearTimeout(this.#joinTimer);
      this.#joinTimer = undefined;
    }
  }

  #settleJoinSuccess(event: JoinedEvent): void {
    if (this.#joinSettled) return;
    this.#joinSettled = true;
    this.#clearJoinTimer();
    const span = this.#span;
    if (span !== undefined) {
      span.setStatus({ code: SpanStatusCode.OK });
      span.end();
    }
    this.#join.resolve(event);
  }

  #settleJoinFailure(error: SignalingError): void {
    if (this.#joinSettled) return;
    this.#joinSettled = true;
    this.#clearJoinTimer();
    const span = this.#span;
    if (span !== undefined) {
      // Token + PII safety (R-23/R-26): record a BOUNDED exception carrying only the
      // stable `signalingCode` — NOT `error` itself. For an `ErrorMessage`-derived
      // failure `error.message` is the server-authored free-form string (and
      // `error.stack`'s first line repeats it), which `recordException(error)` would
      // ship to the telemetry backend via `exception.message`. The verbose server
      // message stays ONLY on the caller-facing SignalingError (transitive-PII
      // contract). No token is ever on the error or the JoinRequest is recorded here.
      span.recordException({ name: error.name, message: error.signalingCode });
      span.setStatus({ code: SpanStatusCode.ERROR, message: error.signalingCode });
      span.end();
    }
    this.#join.reject(error);
  }

  #toSignalingError(err: unknown): SignalingError {
    if (err instanceof SignalingError) {
      return err;
    }
    if (err instanceof FramingError) {
      return new SignalingError(
        SignalingErrorCode.Framing,
        staticMessageFor(SignalingErrorCode.Framing),
        {
          cause: err,
        },
      );
    }
    return new SignalingError(
      SignalingErrorCode.Transport,
      staticMessageFor(SignalingErrorCode.Transport),
      {
        cause: err,
      },
    );
  }

  #transportError(closeCode: number | undefined, cause: unknown): SignalingError {
    return new SignalingError(
      SignalingErrorCode.Transport,
      staticMessageFor(SignalingErrorCode.Transport),
      {
        ...(closeCode !== undefined ? { closeCode } : {}),
        ...(cause !== undefined ? { cause } : {}),
      },
    );
  }
}
