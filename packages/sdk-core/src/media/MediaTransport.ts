// File: packages/sdk-core/src/media/MediaTransport.ts
//
// The active/active MH media-transport layer, RECONCILED TO THE HANDLER SET
// (ADR-0039 §4; story 4 C1). Every `MediaHandlerSet` the client receives — the
// join-time one included — goes through ONE path, `applyHandlerSet`, which
// diffs on `handler_ref` ONLY (`handlerSet.ts`):
//   * a ref not yet held is DIALED (join and migration share one dial path,
//     `#dial`, with no join special case in dialling);
//   * a ref that left the set goes through THE ONE removal function,
//     `#removeRef` — a directed removal: no fault, no loss report;
//   * identical refs with new tokens are a token-only refresh: no dial, no close;
//   * a new ref at a previously used URL is a removal plus an add, i.e. a real
//     close and a fresh WebTransport session (`allowPooling` stays false),
//     never deduplicated by URL.
// It never dials anything outside the latest set.
//
// Each dial opens a WebTransport session, writes ONE length-prefixed
// `MhClientMessage{connectRequest{mediaDialToken}}` on the first bidi
// stream (trace-injected through the same `sendFramedMessage` as the MC send),
// and is CONNECTED iff `ready` resolved and that envelope wrote. The bidi stream
// then carries the MH control channel in both directions (story 3): MH ->
// client `MhServerMessage`s through the shared `FrameDecoder` (malformed input
// counted through the injected `controlSink`, never thrown), client -> MH
// `recovery_request`s via `sendRecoveryRequest`, serialised per ref.
//
// LIFECYCLE per ref: `connecting` -> `connected` -> (`lost` | `parked`) ->
// `connecting` ...; removal deletes the record. A session end is classified by
// `closeCode.ts` (ADR-0039 §7) and drives the redial / park / report decisions;
// redials use full-jitter backoff (`redialBackoff.ts`) with the first retry no
// earlier than one MH keepalive and stop the moment the ref leaves the set.
//
// SELECTORS STAY SELECTORS: `getDatagramChannel`, `openUniStream`,
// `incomingUniStreamsFor` and `sendRecoveryRequest` look up the CONNECTED
// transport of a ref and can never create one. The only dial site is `#dial`,
// reached only from the reconcile, a redial timer of an in-set ref, or a parked
// ref's release by a newer token.
//
// @operations: every dial has a bounded connect deadline, so a half-open QUIC
// handshake whose `ready` never resolves still settles. Every timer runs on the
// injected scheduler and is cancelled on removal and at `disconnect()`.
//
// R-23 / ADR-0039 §5 credential rules: see `#dialTokens` (the scoped dial-token
// retention) and `arm` (the meeting JWT never reaches this class).
// `failureReason`/`failureCode` are SDK-authored bounded strings.

import { create, fromBinary } from '@bufbuild/protobuf';

import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import { encodeFramedMessage, sendFramedMessage, writeFrame } from '../framing/sendFramed.js';
import { FrameDecoder } from '../framing/length-prefix.js';
import { MediaConnectionError, MediaConnectionErrorCode } from '../errors/MediaConnectionError.js';
import { getMetricsSink } from '../telemetry/telemetryConfig.js';
import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import { connect as defaultConnect } from '../transport/BrowserWebTransport.js';
import type { IWebTransport, WebTransportBidirectionalStream } from '../transport/IWebTransport.js';
import { capMhUrl } from '../validation/limits.js';
import {
  MhClientMessageSchema,
  MhConnectRequestSchema,
  MhServerMessageSchema,
  RecoveryRequestSchema,
} from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type { MhConnectionStatusReport } from '../signaling/events.js';
import {
  DEFAULT_CLIENT_CONFIG,
  MH_KEEPALIVE_FLOOR_MS,
  type HandlerSetConfig,
} from '../config/clientConfig.js';
import { FaultOnce } from './lifecycle/faultOnce.js';

import {
  DEFAULT_MH_CONNECT_TIMEOUT_MS,
  type DatagramQueueSettings,
  type MediaConnectionState,
  type MediaDatagramChannel,
  type MediaTransportEventMap,
  type MediaTransportOptions,
  type MhControlSink,
  type MhRecoveryRequest,
  type RunInContext,
  type TransportScheduler,
} from './events.js';
import {
  diffHandlerSet,
  type HandlerRef,
  type HandlerSetIntake,
  type HandlerSetView,
} from './handlerSet.js';
import { classifyClose } from './closeCode.js';
import { redialDelayMs } from './redialBackoff.js';

/** Static, bounded failure reasons (never a raw cause string — R-23/security). */
const REASON_TRANSPORT = 'media handler connection failed';
const REASON_TIMEOUT = 'media handler connect deadline exceeded';
const REASON_NO_CONTROL_CHANNEL = 'no control channel is open to that media handler';
const REASON_DECLINED = 'media handler declined the dial credential';

/** The bounded event-once contract WARN for a 0/unknown MH close code. */
const CLOSE_CODE_FAULT = 'a media handler closed with an unknown or zero close code';

/** Bucket a raw MH index into the bounded `mh_index_bucket` label set (`0|1|2+`). */
function mhIndexBucket(index: number): string {
  return index === 0 ? '0' : index === 1 ? '1' : '2+';
}

const identityRunInContext: RunInContext = (fn) => fn();

const defaultScheduler: TransportScheduler = {
  setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** The last state reported to MC for a ref (report on CHANGE only). */
type ReportedState = MhConnectionStatusReport['state'];

/**
 * ONE record per in-set ref (never parallel maps): everything removal must
 * release lives here, so `#removeRef` cannot miss a piece. A removed ref has NO
 * record; stale async work detects removal by identity (`#refs.get(ref) !== rec`)
 * and a superseded transport by `rec.transport !== transport`.
 *
 * Holds NO credential (tokens live only in `#dialTokens`).
 */
interface RefRecord {
  readonly ref: HandlerRef;
  readonly url: string;
  /** Position in the latest set — the `mh_index_bucket` of the next dial. */
  index: number;
  state: MediaConnectionState;
  /** The current (dialing or connected) transport. */
  transport: IWebTransport | undefined;
  /** Consecutive failed attempts since the last connect (backoff exponent). */
  attempt: number;
  /**
   * `attempt` as it was when the current session's handshake completed. A
   * session MH then closes with a RETRY-class or contract-fault code was never a
   * healthy connect, so the exponent is restored from here rather than reset —
   * otherwise a persistent MC_UNAVAILABLE / BINDING_CONFLICT redials at the
   * floor forever, fleet-synchronised, and the cap never comes into play.
   */
  attemptBeforeConnect: number;
  /**
   * TOKEN GENERATION: bumped whenever a set brings a DIFFERENT token string for
   * this ref (opaque inequality — the token itself is never copied here).
   */
  tokenGen: number;
  /** The `tokenGen` the current/last dial read its token at. */
  dialTokenGen: number;
  redialTimer: unknown;
  graceTimer: unknown;
  /** Ends an in-flight dial's handshake wait at once (removal / teardown). */
  abortDial: (() => void) | undefined;
  /** The control stream (connect bidi stream after its envelope) + send chain. */
  control: { readonly stream: WebTransportBidirectionalStream; tail: Promise<void> } | undefined;
  controlReader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  datagram:
    | {
        readonly channel: MediaDatagramChannel;
        readonly writer: WritableStreamDefaultWriter<Uint8Array>;
      }
    | undefined;
  lastReported: ReportedState | undefined;
}

/** Counts of how sessions ended, by disposition (numbers only). */
export interface MediaTransportDiagnostics {
  readonly retryCloses: number;
  readonly releasedCloses: number;
  readonly declinedCloses: number;
  readonly lossCloses: number;
  readonly contractFaultCloses: number;
}

/**
 * Active/active MH media transport, reconciled to the handler set. One instance
 * per session (`MeetingSession` builds it before the signalling join so the
 * join-time set has a home).
 */
export class MediaTransport extends TypedEventEmitter<MediaTransportEventMap> {
  readonly #connectFn: WebTransportConnectFn;
  readonly #clock: () => number;
  readonly #connectTimeoutMs: number;
  readonly #metricsSink: MetricsSink | undefined;
  readonly #metricLabels: MetricLabels;
  readonly #runInContext: RunInContext;
  readonly #datagramQueue: DatagramQueueSettings | undefined;
  readonly #controlSink: MhControlSink | undefined;
  readonly #handlers: HandlerSetConfig;
  readonly #scheduler: TransportScheduler;
  readonly #random: () => number;
  readonly #faults = new FaultOnce();

  /** The latest set (token-free). Empty until the first set arrives. */
  #set: HandlerSetView = { version: '0', handlers: [] };
  /** The latest set's wire `handler_set_version`, echoed on every report. */
  #version = 0n;
  /** One record per in-set ref that has been dialed. */
  readonly #refs = new Map<HandlerRef, RefRecord>();
  /**
   * CREDENTIAL — SCOPED RETENTION (ADR-0039 §5 "Client retention"; the scoped
   * R-23 amendment; `scripts/guards/semantic/checks.md` §Client Credential
   * Lifetime, "Scoped exception: the media_dial dial credential").
   *
   * The latest opaque `media_dial` token per IN-SET ref, and NOWHERE ELSE in
   * the SDK. It is:
   *   - REPLACED WHOLESALE by every set (a new Map is built and assigned —
   *     never merged, never gated on `handler_set_version`; a token-only
   *     refresh replaces it too);
   *   - DELETED for a ref in `#removeRef`, and CLEARED in `disconnect()`
   *     (reached on every session end path, join failure included);
   *   - read only at the moment a dial builds its `MhConnectRequest`: timers and
   *     dial closures capture the REF and read this map when they fire (an
   *     absent ref means no dial);
   *   - never parsed (opaque: no split, no base64 decode, no `exp` read), never
   *     logged, stringified, put on an error or `cause`, a metric, a span, an
   *     event, a return value or the DOM. No getter exists.
   * A token is reusable across dials within its lifetime (MH keeps no jti
   * cache), so every dial — join, add, redial, retry, post-park — sends it.
   */
  #dialTokens = new Map<HandlerRef, string>();
  /** `arm()` was called: the join completed and dialling may start. */
  #armed = false;
  #emptySetTimer: unknown;
  #overdueEmitted = false;
  #terminated = false;
  readonly #diagnostics = {
    retryCloses: 0,
    releasedCloses: 0,
    declinedCloses: 0,
    lossCloses: 0,
    contractFaultCloses: 0,
  };

  constructor(options: MediaTransportOptions = {}) {
    super();
    this.#connectFn = options.connect ?? defaultConnect;
    this.#clock = options.clock ?? Date.now;
    this.#connectTimeoutMs = options.connectTimeoutMs ?? DEFAULT_MH_CONNECT_TIMEOUT_MS;
    this.#metricsSink = options.metricsSink ?? getMetricsSink();
    this.#metricLabels = options.metricLabels ?? {};
    this.#runInContext = options.runInContext ?? identityRunInContext;
    this.#datagramQueue = options.datagramQueue;
    this.#controlSink = options.controlSink;
    this.#handlers = options.handlers ?? DEFAULT_CLIENT_CONFIG.media.handlers;
    this.#scheduler = options.scheduler ?? defaultScheduler;
    this.#random = options.random ?? Math.random;
  }

  // ----------------------------------------------------------------------------
  // The set
  // ----------------------------------------------------------------------------

  /**
   * Apply a validated set (`signaling/handlerSetIntake.ts` -> the signalling
   * sink). The ONE entry point for every set, join-time included. Before
   * {@link arm} the set is only held; after it, added refs are dialed at once.
   */
  applyHandlerSet(intake: HandlerSetIntake): void {
    if (this.#terminated) return;
    const previousTokens = this.#dialTokens;
    const nextRefs = intake.view.handlers.map((h) => h.ref);
    const diff = diffHandlerSet(
      this.#set.handlers.map((h) => h.ref),
      nextRefs,
    );
    this.#set = intake.view;
    this.#version = intake.version;
    for (const ref of diff.removed) this.#removeRef(ref);
    // Wholesale replacement: a NEW map, never a merge (see the field).
    this.#dialTokens = new Map(intake.dialTokens);
    intake.view.handlers.forEach((h, index) => {
      const rec = this.#refs.get(h.ref);
      if (rec) rec.index = index;
    });
    if (intake.view.handlers.length > 0) this.#cancelEmptySetNotice();
    for (const ref of diff.retained) {
      const rec = this.#refs.get(ref);
      if (!rec) continue;
      // Opaque string inequality only; no copy of any token is kept.
      if (previousTokens.get(ref) !== this.#dialTokens.get(ref)) rec.tokenGen++;
      if (rec.state === 'parked') {
        // Released ONLY by a token newer than the one the declined dial used; a
        // set repeating it keeps the park. Then exactly one dial.
        if (rec.tokenGen > rec.dialTokenGen) {
          rec.attempt = 0;
          void this.#dial(rec);
        }
      } else if (rec.graceTimer !== undefined) {
        // A set update inside the HANDLER_RELEASED grace that keeps the ref:
        // no loss report; back to the normal redial.
        this.#clearTimer(rec, 'graceTimer');
        this.#scheduleRedial(rec);
      }
    }
    if (this.#armed) this.#reconcile();
  }

  /**
   * Start dialling: called ONCE by `MeetingSession` after the signalling join
   * resolved. Dials every member of the latest set and resolves when those
   * dials settle.
   *
   * THE MEETING JWT NEVER REACHES THIS CLASS (R-23; ADR-0039 §5: the join JWT
   * is "deliberately not retained by the SDK"). Every dial — join, added ref,
   * redial, retry, post-park — carries exactly one credential, the ref's
   * latest `media_dial_token`, read from `#dialTokens` when the request is
   * built. Do NOT thread the JWT in here: it is MC's join credential, never an
   * MH one.
   *
   * @throws {MediaConnectionError} `ALL_FAILED` when the join-time set is
   *   NON-EMPTY and none of its dials connected (the join gate, unchanged). An
   *   empty set resolves at once ("media connecting").
   */
  async arm(): Promise<void> {
    if (this.#armed || this.#terminated) return;
    this.#armed = true;
    if (this.#set.handlers.length === 0) this.#startEmptySetNotice();
    await Promise.allSettled(this.#reconcile());
    if (this.#terminated) return;
    const anyConnected = [...this.#refs.values()].some((r) => r.state === 'connected');
    if (this.#set.handlers.length > 0 && !anyConnected) {
      throw new MediaConnectionError(
        MediaConnectionErrorCode.AllFailed,
        'all media servers failed to connect',
      );
    }
  }

  /** The current lifecycle state of `ref`, or `undefined` if it has no record. */
  getState(ref: HandlerRef): MediaConnectionState | undefined {
    return this.#refs.get(ref)?.state;
  }

  /**
   * How sessions ended, by disposition — numbers only. The C2 hook for the
   * R-42 series; NOT exported from the package.
   */
  diagnostics(): MediaTransportDiagnostics {
    return { ...this.#diagnostics };
  }

  /**
   * Tear down every ref (through `#removeRef`), every timer and every token.
   * Idempotent (R-23 / @operations).
   */
  disconnect(): void {
    if (this.#terminated) return;
    this.#terminated = true;
    this.#cancelEmptySetNotice();
    for (const ref of [...this.#refs.keys()]) this.#removeRef(ref);
    this.#dialTokens.clear();
  }

  // ----------------------------------------------------------------------------
  // Selectors over CONNECTED transports — never dialers
  // ----------------------------------------------------------------------------

  /**
   * A datagram send/receive channel over the CONNECTED handler `ref`, or
   * `undefined` (the caller counts that as `not_connected`). One cached writer:
   * `send` awaits `writer.ready` before every write, so queue depth accumulates
   * in the application queue where it is bounded and counted.
   */
  getDatagramChannel(ref: HandlerRef): MediaDatagramChannel | undefined {
    const rec = this.#connected(ref);
    if (!rec || !rec.transport) return undefined;
    if (rec.datagram) return rec.datagram.channel;
    const transport = rec.transport;
    const writer = transport.datagrams.writable.getWriter();
    let open = true;
    // `closed` resolving or rejecting both mean: no transport left.
    void transport.closed.then(
      () => {
        open = false;
      },
      () => {
        open = false;
      },
    );
    const channel: MediaDatagramChannel = {
      readable: transport.datagrams.readable,
      get isOpen(): boolean {
        return open;
      },
      // Absent under a test double or a UA that does not expose it.
      maxDatagramSize: transport.datagrams.maxDatagramSize,
      async send(bytes: Uint8Array): Promise<void> {
        await writer.ready;
        await writer.write(bytes);
      },
    };
    rec.datagram = { channel, writer };
    return channel;
  }

  /**
   * Open one OUTGOING unidirectional stream to the CONNECTED handler `ref`
   * (one per video group per handler), or `undefined`. A selector, never a dialer.
   */
  openUniStream(ref: HandlerRef): Promise<WritableStream<Uint8Array>> | undefined {
    return this.#connected(ref)?.transport?.createUnidirectionalStream();
  }

  /**
   * The INCOMING unidirectional streams of the CONNECTED handler `ref`, or
   * `undefined`. A selector, never a dialer.
   */
  incomingUniStreamsFor(ref: HandlerRef): ReadableStream<ReadableStream<Uint8Array>> | undefined {
    return this.#connected(ref)?.transport?.incomingUnidirectionalStreams;
  }

  /**
   * Send a receiver's recovery request to the handler `ref` on its control
   * stream (story 3 R-13), serialised per ref.
   *
   * @throws {MediaConnectionError} `Transport` when no control channel is open
   * to `ref`, or the write fails. Bounded messages only.
   */
  async sendRecoveryRequest(ref: HandlerRef, request: MhRecoveryRequest): Promise<void> {
    const rec = this.#refs.get(ref);
    const channel = this.#terminated ? undefined : rec?.control;
    if (!rec || !channel) {
      throw new MediaConnectionError(
        MediaConnectionErrorCode.Transport,
        REASON_NO_CONTROL_CHANNEL,
        rec ? { mhUrl: capMhUrl(rec.url) } : {},
      );
    }
    const message = create(MhClientMessageSchema, {
      message: {
        case: 'recoveryRequest',
        value: create(RecoveryRequestSchema, {
          slotId: request.slotId,
          senderId: request.senderId,
          gopCounter: request.gopCounter,
        }),
      },
    });
    // Encoded (and trace-injected) NOW, under the caller's context; only the
    // write is queued behind any earlier one.
    const frame = this.#runInContext(() => encodeFramedMessage(MhClientMessageSchema, message));
    const send = channel.tail.then(() => writeFrame(channel.stream, frame));
    // The chain continues past a failed send; the failure is the caller's.
    channel.tail = send.catch(() => undefined);
    try {
      await send;
    } catch {
      // Never the platform error: it is not ours to forward (R-23).
      throw new MediaConnectionError(MediaConnectionErrorCode.Transport, REASON_TRANSPORT, {
        mhUrl: capMhUrl(rec.url),
      });
    }
  }

  // ----------------------------------------------------------------------------
  // Reconcile, dial, end, remove
  // ----------------------------------------------------------------------------

  /**
   * Dial every member of the latest set that has no record. A ref with a record
   * is connecting, connected, awaiting a redial, or parked — none is redialed
   * here. Returns the dials started (so `arm` can await the join-time ones).
   */
  #reconcile(): Promise<void>[] {
    const dials: Promise<void>[] = [];
    this.#set.handlers.forEach((h, index) => {
      if (this.#refs.has(h.ref)) return;
      const rec: RefRecord = {
        ref: h.ref,
        url: h.url,
        index,
        state: 'connecting',
        transport: undefined,
        attempt: 0,
        attemptBeforeConnect: 0,
        tokenGen: 0,
        dialTokenGen: 0,
        redialTimer: undefined,
        graceTimer: undefined,
        abortDial: undefined,
        control: undefined,
        controlReader: undefined,
        datagram: undefined,
        lastReported: undefined,
      };
      this.#refs.set(h.ref, rec);
      dials.push(this.#dial(rec));
    });
    return dials;
  }

  /** Whether `rec` is still the live record of its ref (not removed). */
  #isCurrent(rec: RefRecord): boolean {
    return !this.#terminated && this.#refs.get(rec.ref) === rec;
  }

  #connected(ref: HandlerRef): RefRecord | undefined {
    if (this.#terminated) return undefined;
    const rec = this.#refs.get(ref);
    return rec?.state === 'connected' ? rec : undefined;
  }

  /**
   * THE ONE DIAL PATH (join, add, redial, retry, post-park). Never rejects: the
   * outcome lands on the record. The dial token is read from `#dialTokens`
   * when the request is built. A dial whose ref was removed meanwhile is closed
   * when it resolves and emits NOTHING (no metric, no report, no event).
   */
  async #dial(rec: RefRecord): Promise<void> {
    rec.state = 'connecting';
    let transport: IWebTransport;
    try {
      transport = this.#connectFn(rec.url);
    } catch (err) {
      // A SYNCHRONOUS connect failure is a failed dial like any other.
      this.#dialFailed(rec, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, err);
      return;
    }
    rec.transport = transport;
    // The end of THIS session, whenever it comes. A rejected `closed` is a
    // session lost without a clean close (no code).
    void transport.closed.then(
      (info) => this.#ended(rec, transport, info.closeCode),
      () => this.#ended(rec, transport, undefined),
    );
    const live = (): boolean => this.#isCurrent(rec) && rec.transport === transport;

    let timer: unknown;
    let timerArmed = true;
    const clearDeadline = (): void => {
      if (!timerArmed) return;
      timerArmed = false;
      this.#scheduler.clearTimeout(timer);
    };
    const deadline = new Promise<'timeout' | 'aborted'>((resolve) => {
      timer = this.#scheduler.setTimeout(() => {
        timerArmed = false;
        resolve('timeout');
      }, this.#connectTimeoutMs);
      // Removal mid-handshake ends the wait at once: no orphaned deadline timer.
      rec.abortDial = () => {
        clearDeadline();
        resolve('aborted');
      };
    });

    try {
      // `ready`'s rejection reason is preserved on the error's non-enumerable
      // `cause`: it fires before any envelope is written, so it cannot echo a
      // credential.
      let readyError: unknown;
      const readyOutcome = transport.ready.then(
        () => 'ready' as const,
        (err: unknown) => {
          readyError = err;
          return 'error' as const;
        },
      );
      const outcome = await Promise.race([readyOutcome, deadline]);
      clearDeadline();
      if (rec.transport === transport) rec.abortDial = undefined;
      if (!live() || outcome === 'aborted') {
        this.#safeClose(transport);
        return;
      }
      if (outcome === 'timeout') {
        this.#safeClose(transport);
        this.#dialFailed(rec, MediaConnectionErrorCode.ConnectTimeout, REASON_TIMEOUT, undefined);
        return;
      }
      if (outcome === 'error') {
        this.#safeClose(transport);
        this.#dialFailed(rec, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, readyError);
        return;
      }
      // CHOOSE the datagram queue depths rather than inheriting the UA's
      // (ADR-0036 §1/§11; `validateMediaConfig` asserts the relationships).
      this.#applyDatagramQueueSettings(transport);

      const stream = await transport.createBidirectionalStream();
      if (!live()) {
        this.#safeClose(transport);
        return;
      }
      // Read the credential NOW, at request build — never captured earlier.
      rec.dialTokenGen = rec.tokenGen;
      const envelope = create(MhClientMessageSchema, {
        message: {
          case: 'connectRequest',
          value: create(MhConnectRequestSchema, {
            mediaDialToken: this.#dialTokens.get(rec.ref) ?? '',
          }),
        },
      });
      // R-58: inject under the active dt_client.join span; the wrap is
      // synchronous around the inject (the first step of `sendFramedMessage`).
      await this.#runInContext(() => sendFramedMessage(stream, MhClientMessageSchema, envelope));
      if (!live()) {
        this.#safeClose(transport);
        return;
      }
      this.#dialSucceeded(rec, transport, stream);
    } catch (err) {
      clearDeadline();
      this.#safeClose(transport);
      if (!live()) return;
      this.#dialFailed(rec, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, err);
    }
  }

  #dialSucceeded(
    rec: RefRecord,
    transport: IWebTransport,
    stream: WebTransportBidirectionalStream,
  ): void {
    rec.state = 'connected';
    rec.attemptBeforeConnect = rec.attempt;
    rec.attempt = 0;
    this.#emitMetric('success', rec.index);
    this.#startControlChannel(rec, transport, stream);
    this.#report(rec, 'connected');
    this.emit('connected', { ref: rec.ref, url: rec.url });
  }

  #dialFailed(
    rec: RefRecord,
    code: MediaConnectionErrorCode,
    reason: string,
    cause: unknown,
  ): void {
    rec.transport = undefined;
    rec.state = 'lost';
    this.#emitMetric('failure', rec.index);
    this.#report(rec, 'failed', { failureCode: code, failureReason: reason });
    this.emit(
      'failed',
      new MediaConnectionError(code, reason, {
        mhUrl: capMhUrl(rec.url),
        ...(cause !== undefined ? { cause } : {}),
      }),
    );
    this.#scheduleRedial(rec);
  }

  /**
   * A CONNECTED session ended. Classified by `closeCode.ts`; a session that
   * ends while still dialing is the dial path's failure, and anything stale
   * (removed ref, superseded transport, teardown) is ignored — so a server
   * close that follows a directed removal produces nothing.
   */
  #ended(rec: RefRecord, transport: IWebTransport, closeCode: number | undefined): void {
    if (!this.#isCurrent(rec) || rec.transport !== transport || rec.state !== 'connected') return;
    this.#releaseConnection(rec);
    rec.transport = undefined;
    rec.state = 'lost';
    const disposition = classifyClose(closeCode);
    switch (disposition) {
      case 'retry':
        this.#diagnostics.retryCloses++;
        rec.attempt = rec.attemptBeforeConnect;
        this.#scheduleRedial(rec);
        return;
      case 'released':
        this.#diagnostics.releasedCloses++;
        // Still in the set (a removal would have deleted the record): wait a
        // short grace for a set update, then treat as loss.
        rec.graceTimer = this.#scheduler.setTimeout(() => {
          rec.graceTimer = undefined;
          if (!this.#isCurrent(rec)) return;
          this.#report(rec, 'disconnected');
          this.#scheduleRedial(rec);
        }, this.#handlers.releasedGraceMs);
        return;
      case 'declined':
        this.#diagnostics.declinedCloses++;
        rec.state = 'parked';
        // FAILED EVERY TIME (not change-gated): it is MC's re-push trigger.
        this.#report(
          rec,
          'failed',
          { failureCode: MediaConnectionErrorCode.Declined, failureReason: REASON_DECLINED },
          true,
        );
        // A refresh that landed while the declined dial was in flight already
        // brought a NEWER token: dial once with it now instead of parking under
        // it (which would stall until MC's next TTL/3 push).
        if (rec.tokenGen > rec.dialTokenGen) {
          rec.attempt = 0;
          void this.#dial(rec);
        }
        return;
      case 'contract_fault':
        this.#diagnostics.contractFaultCloses++;
        rec.attempt = rec.attemptBeforeConnect;
        if (this.#faults.report('mh-close', CLOSE_CODE_FAULT)) {
          console.warn('[dt-signaling-contract]', CLOSE_CODE_FAULT);
        }
        this.#report(rec, 'disconnected');
        this.#scheduleRedial(rec);
        return;
      case 'loss':
        this.#diagnostics.lossCloses++;
        this.#report(rec, 'disconnected');
        this.#scheduleRedial(rec);
        return;
      default: {
        const unhandled: never = disposition;
        return unhandled;
      }
    }
  }

  /** Arm a full-jitter redial for an in-set ref. Captures the RECORD, never a token. */
  #scheduleRedial(rec: RefRecord): void {
    if (!this.#isCurrent(rec)) return;
    rec.state = 'lost';
    const delay = redialDelayMs(rec.attempt, {
      baseMs: this.#handlers.redialBaseMs,
      capMs: this.#handlers.redialCapMs,
      floorMs: MH_KEEPALIVE_FLOOR_MS,
      random: this.#random,
    });
    rec.attempt++;
    this.#clearTimer(rec, 'redialTimer');
    rec.redialTimer = this.#scheduler.setTimeout(() => {
      rec.redialTimer = undefined;
      // Never dial outside the latest set: a removed ref has no live record.
      if (!this.#isCurrent(rec)) return;
      void this.#dial(rec);
    }, delay);
  }

  /**
   * THE ONE REMOVAL CLEANUP for a ref that left the set — and, from
   * `disconnect()`, for every ref at session end. A DIRECTED removal: no fault,
   * no loss report, no metric.
   *   - drop its token and its record (stale async work then sees it gone);
   *   - cancel its redial timer, HANDLER_RELEASED grace and park;
   *   - tell per-ref consumers FIRST (`handlerRemoved`, synchronous): egress
   *     lanes, read loops (quietly), hop monitors, recovery queues;
   *   - end its control loop quietly and release its datagram writer;
   *   - close its transport — including one the server left OPEN (the
   *     data-plane fence) and an in-flight dial (aborted; the dial path sees the
   *     record gone and closes whatever resolves).
   */
  #removeRef(ref: HandlerRef): void {
    this.#dialTokens.delete(ref);
    const rec = this.#refs.get(ref);
    if (!rec) return;
    this.#refs.delete(ref);
    this.#clearTimer(rec, 'redialTimer');
    this.#clearTimer(rec, 'graceTimer');
    const abort = rec.abortDial;
    rec.abortDial = undefined;
    abort?.();
    this.emit('handlerRemoved', ref);
    this.#releaseConnection(rec);
    const transport = rec.transport;
    rec.transport = undefined;
    if (transport) this.#safeClose(transport);
  }

  /** Release a connection's control loop and datagram writer (not the transport). */
  #releaseConnection(rec: RefRecord): void {
    rec.control = undefined;
    const reader = rec.controlReader;
    rec.controlReader = undefined;
    if (reader) {
      void reader.cancel().catch(() => {
        // Already released or errored — the transport close ends it.
      });
    }
    const datagram = rec.datagram;
    rec.datagram = undefined;
    if (datagram) {
      try {
        void datagram.writer.close().catch(() => {
          // A writer on an errored stream rejects on close; the transport
          // close is what releases the connection.
        });
      } catch {
        // Already released.
      }
    }
  }

  #clearTimer(rec: RefRecord, which: 'redialTimer' | 'graceTimer'): void {
    if (rec[which] !== undefined) {
      this.#scheduler.clearTimeout(rec[which]);
      rec[which] = undefined;
    }
  }

  // ----------------------------------------------------------------------------
  // Reports, empty-set notice, metrics
  // ----------------------------------------------------------------------------

  /**
   * Emit a per-ref transition for `MediaConnectionUpdate`: ONLY when the
   * reported state changes (a retry loop must not skew MC's failed/connected
   * ratio), unless `always` (DECLINED).
   */
  #report(
    rec: RefRecord,
    state: ReportedState,
    failure?: { readonly failureCode: string; readonly failureReason: string },
    always = false,
  ): void {
    if (!always && rec.lastReported === state) return;
    rec.lastReported = state;
    this.emit('status', {
      handlerRef: rec.ref,
      handlerSetVersion: this.#version,
      state,
      ...(failure ?? {}),
      observedAtMs: this.#clock(),
    });
  }

  #startEmptySetNotice(): void {
    if (this.#overdueEmitted || this.#emptySetTimer !== undefined) return;
    this.#emptySetTimer = this.#scheduler.setTimeout(() => {
      this.#emptySetTimer = undefined;
      if (this.#terminated || this.#overdueEmitted) return;
      this.#overdueEmitted = true;
      this.emit('handlersOverdue', undefined);
    }, this.#handlers.emptySetNoticeMs);
  }

  #cancelEmptySetNotice(): void {
    if (this.#emptySetTimer === undefined) return;
    this.#scheduler.clearTimeout(this.#emptySetTimer);
    this.#emptySetTimer = undefined;
  }

  /**
   * ---------------------------------------------------------------------------
   * THE `...this.#metricLabels` SPREAD BELOW IS LEGAL FOR THIS ONE METRIC ONLY.
   * DO NOT COPY IT INTO A MEDIA EMISSION.
   * ---------------------------------------------------------------------------
   *
   * `dt_client_mh_connection_total` is an ADR-0028 join-flow metric that carried
   * the join label set — including `meeting_id_hash` — from birth. ADR-0036 §11
   * grandfathers that set AS A SET: it is a closed, enumerated exception, it is
   * not extended, and nothing joins it. The frozen roster is named in
   * `docs/observability/metrics/client.md`.
   *
   * **The test is what a metric OBSERVES, not which directory it lives in.**
   * Connect-lifecycle — once per connection, at handshake, before any frame
   * exists — is grandfathered. Media-carrying — per frame, per stream, or on the
   * media data path — is under the bar. "It is in `MediaTransport.ts`" is the
   * wrong test in BOTH directions: it would wrongly bar this metric, and it
   * would wrongly permit a media counter someone places outside the media tree.
   * The second is the one that will actually happen.
   *
   * Substantively neither §11 argument reaches this counter. The trace argument
   * is about the time-ordered sequence of sizes for a single stream, and a
   * connect-outcome counter reconstructs nothing. The oracle argument is about
   * `reason` faithfully mirroring which crypto layer rejected a frame; `status`
   * and `mh_index_bucket` are not crypto-layer discriminators and reveal no
   * receiver key state.
   *
   * EVERY NEW MEDIA EMISSION goes through `setup/mediaMetrics.ts`, which builds
   * `{client_version, org_id, key_custody, media_kind}` (per-kind series) or
   * `{client_version, org_id, key_custody}` (session series) by allow-list from
   * two named strings and a closed media kind — so there is no set to inherit and
   * nothing to prune. Nothing mechanical enforces this in TypeScript.
   */
  #emitMetric(status: 'success' | 'failure', index: number): void {
    this.#metricsSink?.counter('dt_client_mh_connection_total', {
      ...this.#metricLabels,
      status,
      mh_index_bucket: mhIndexBucket(index),
    });
  }

  // ----------------------------------------------------------------------------
  // The MH control channel (story 3)
  // ----------------------------------------------------------------------------

  /**
   * Open the MH control channel on `stream` — the connect bidi stream AFTER its
   * envelope. Holds the STREAM only: nothing that carried a credential is
   * captured by the loop or the control send.
   */
  #startControlChannel(
    rec: RefRecord,
    transport: IWebTransport,
    stream: WebTransportBidirectionalStream,
  ): void {
    rec.control = { stream, tail: Promise.resolve() };
    // The transport ending ends the channel: a later send then reports "no
    // control channel", not a write failure.
    const drop = (): void => {
      if (rec.control?.stream === stream) rec.control = undefined;
    };
    void transport.closed.then(drop, drop);
    void this.#runControlReadLoop(rec, stream);
  }

  /**
   * Read `MhServerMessage`s off a ref's control stream until it ends. NEVER
   * throws. A loop on a removed ref or a torn-down session ends QUIETLY.
   */
  async #runControlReadLoop(
    rec: RefRecord,
    stream: WebTransportBidirectionalStream,
  ): Promise<void> {
    let reader: ReadableStreamDefaultReader<Uint8Array>;
    try {
      reader = stream.readable.getReader();
    } catch {
      return;
    }
    rec.controlReader = reader;
    const live = (): boolean => this.#isCurrent(rec) && rec.controlReader === reader;
    // The SAME decoder as MC signalling: the per-message bound is checked on the
    // length prefix, before the body is buffered.
    const decoder = new FrameDecoder();
    let desynchronised = false;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (!live()) return;
        if (done) {
          // MH finished its send half: no keyframe request can arrive again.
          this.#controlSink?.ended(rec.ref, 'finished');
          return;
        }
        let frames: Uint8Array[];
        try {
          frames = decoder.push(value);
        } catch {
          // Oversized or empty length: the INBOUND half is desynchronised. Count
          // it and stop reading. The channel entry is KEPT: the OUTBOUND half
          // is independent, so recovery requests can still be sent on it.
          this.#controlSink?.malformed(rec.ref);
          desynchronised = true;
          return;
        }
        for (const frame of frames) this.#dispatchControl(rec.ref, frame);
      }
    } catch {
      // Reset, or the transport ended. Removal and teardown are ours and are
      // not counted; anything else silently ends keyframe requests, so it is
      // made visible.
      if (live()) this.#controlSink?.ended(rec.ref, 'reset');
    } finally {
      if (rec.controlReader === reader) rec.controlReader = undefined;
      if (!desynchronised && rec.control?.stream === stream) rec.control = undefined;
      try {
        reader.releaseLock();
      } catch {
        // Already released during teardown.
      }
    }
  }

  /** Decode one control message and hand it to the sink. Never throws. */
  #dispatchControl(ref: HandlerRef, frame: Uint8Array): void {
    let message;
    try {
      message = fromBinary(MhServerMessageSchema, frame);
    } catch {
      this.#controlSink?.malformed(ref);
      return;
    }
    switch (message.message.case) {
      case 'keyframeRequest':
        this.#controlSink?.keyframeRequest(ref, {
          streamNumber: message.message.value.streamNumber,
          gopCounter: message.message.value.gopCounter,
        });
        return;
      default:
        // An unset or unknown variant carries nothing this client can act on.
        this.#controlSink?.malformed(ref);
        return;
    }
  }

  #applyDatagramQueueSettings(transport: IWebTransport): void {
    const settings = this.#datagramQueue;
    if (!settings) return;
    const datagrams = transport.datagrams;
    datagrams.outgoingHighWaterMark = settings.outgoingHighWaterMark;
    datagrams.outgoingMaxAge = settings.outgoingMaxAgeMs;
    datagrams.incomingHighWaterMark = settings.incomingHighWaterMark;
  }

  #safeClose(transport: IWebTransport): void {
    try {
      transport.close();
    } catch {
      // Already closed / errored — ignore (one bad close can't abort the rest).
    }
  }
}
