// File: packages/sdk-core/src/media/MediaTransport.ts
//
// R-20/R-21/R-58/R-60 (SDK side): the active/active MH media-transport layer.
// `connectAll(urls, jwt)` opens a WebTransport connection to EVERY MH URL from
// `JoinResponse.media_servers` IN PARALLEL and performs ONLY the auth handshake on
// each: open the first bidi stream, write a single length-prefixed
// `MhClientMessage{connectRequest{joinToken}}` envelope (trace-injected via the SAME
// shared `sendFramedMessage` path as the MC send), and treat the MH as CONNECTED iff
// `ready` resolved AND that envelope wrote without error. No frame I/O beyond the
// connect envelope, no SFrame, no WebCodecs, no datagrams.
//
// R-21 resolver: `connectAll` resolves (with the per-MH reports) when ≥1 MH connects
// and rejects with `MediaConnectionError(ALL_FAILED)` only when EVERY MH fails. Each
// `#connectOne` NEVER rejects — it captures its outcome into per-URL state — so one
// slow/hung MH can't delay the others (`Promise.allSettled`).
//
// @operations: a bounded per-MH connect deadline (mirrors SignalingClient's join
// timer) guarantees `connectAll` settles even against a half-open QUIC handshake
// whose `ready` never resolves; the timed-out MH is reported FAILED. Transports are
// registered for teardown BEFORE awaiting `ready`, so `disconnect()` always reaches
// an in-flight connection. Teardown is a single idempotent path.
//
// R-23: the meeting JWT is threaded only as the `connectAll` parameter into each
// `#connectOne`; it is never stored on the instance. `failureReason`/`failureCode`
// are SDK-authored bounded classifications (never a raw transport-error message).

import { create } from '@bufbuild/protobuf';

import { TypedEventEmitter } from '../events/TypedEventEmitter.js';
import { sendFramedMessage } from '../framing/sendFramed.js';
import { MediaConnectionError, MediaConnectionErrorCode } from '../errors/MediaConnectionError.js';
import { getMetricsSink } from '../telemetry/telemetryConfig.js';
import type { MetricLabels, MetricsSink } from '../telemetry/MetricsSink.js';
import { connect as defaultConnect } from '../transport/BrowserWebTransport.js';
import type { IWebTransport } from '../transport/IWebTransport.js';
import { capMhUrl } from '../validation/limits.js';
import {
  MhClientMessageSchema,
  MhConnectRequestSchema,
} from '../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { WebTransportConnectFn } from '../signaling/SignalingClient.js';
import type { MhConnectionStatusReport } from '../signaling/events.js';

import {
  DEFAULT_MH_CONNECT_TIMEOUT_MS,
  type MediaConnectionState,
  type MediaTransportEventMap,
  type MediaTransportOptions,
  type RunInContext,
} from './events.js';

/** Static, bounded failure reasons (never a raw cause string — R-23/security). */
const REASON_TRANSPORT = 'media handler connection failed';
const REASON_TIMEOUT = 'media handler connect deadline exceeded';

/** Bucket a raw MH index into the bounded `mh_index_bucket` label set (`0|1|2+`). */
function mhIndexBucket(index: number): string {
  return index === 0 ? '0' : index === 1 ? '1' : '2+';
}

const identityRunInContext: RunInContext = (fn) => fn();

interface FailureInfo {
  readonly failureCode: string;
  readonly failureReason: string;
}

/**
 * Active/active MH media-transport. Single-use: one `connectAll()` per instance
 * (mirrors `SignalingClient`'s single-join); `MeetingSession` constructs a fresh
 * instance per join.
 */
export class MediaTransport extends TypedEventEmitter<MediaTransportEventMap> {
  readonly #connectFn: WebTransportConnectFn;
  readonly #clock: () => number;
  readonly #connectTimeoutMs: number;
  readonly #metricsSink: MetricsSink | undefined;
  readonly #metricLabels: MetricLabels;
  readonly #runInContext: RunInContext;

  readonly #order: string[] = [];
  readonly #states = new Map<string, MediaConnectionState>();
  readonly #observedAt = new Map<string, number>();
  readonly #failures = new Map<string, FailureInfo>();
  readonly #transports: IWebTransport[] = [];
  readonly #timers = new Set<ReturnType<typeof setTimeout>>();

  #terminated = false;

  constructor(options: MediaTransportOptions = {}) {
    super();
    this.#connectFn = options.connect ?? defaultConnect;
    this.#clock = options.clock ?? Date.now;
    this.#connectTimeoutMs = options.connectTimeoutMs ?? DEFAULT_MH_CONNECT_TIMEOUT_MS;
    this.#metricsSink = options.metricsSink ?? getMetricsSink();
    this.#metricLabels = options.metricLabels ?? {};
    this.#runInContext = options.runInContext ?? identityRunInContext;
  }

  /**
   * Connect to every URL in `urls` in parallel using `jwt` as the MH connect token.
   * Resolves with one {@link MhConnectionStatusReport} per URL (index-ordered) when
   * ≥1 connects; rejects with {@link MediaConnectionError} (`ALL_FAILED`) when every
   * URL fails. An empty `urls` resolves with `[]`.
   */
  async connectAll(urls: readonly string[], jwt: string): Promise<MhConnectionStatusReport[]> {
    // Seed per-URL state in INPUT ORDER before any await so report order is stable
    // regardless of which MH settles first.
    for (const url of urls) {
      this.#order.push(url);
      this.#states.set(url, 'connecting');
    }
    await Promise.allSettled(urls.map((url, index) => this.#connectOne(url, index, jwt)));
    const reports = this.getStatusReports();
    const anyConnected = this.#order.some((url) => this.#states.get(url) === 'connected');
    if (this.#order.length > 0 && !anyConnected) {
      throw new MediaConnectionError(
        MediaConnectionErrorCode.AllFailed,
        'all media servers failed to connect',
      );
    }
    return reports;
  }

  /** The current observable state of `url`, or `undefined` if not a known MH. */
  getState(url: string): MediaConnectionState | undefined {
    return this.#states.get(url);
  }

  /**
   * Per-URL terminal reports in INPUT ORDER — the input to
   * `SignalingClient.sendMediaConnectionUpdate`. Available even on the all-fail
   * rejection so the caller can still report every MH's state.
   */
  getStatusReports(): MhConnectionStatusReport[] {
    return this.#order.map((url) => {
      const observedAtMs = this.#observedAt.get(url) ?? this.#clock();
      const connected = this.#states.get(url) === 'connected';
      if (connected) {
        return { mhUrl: url, state: 'connected', observedAtMs };
      }
      const failure = this.#failures.get(url);
      if (failure !== undefined) {
        return {
          mhUrl: url,
          state: 'failed',
          failureReason: failure.failureReason,
          failureCode: failure.failureCode,
          observedAtMs,
        };
      }
      return { mhUrl: url, state: 'failed', observedAtMs };
    });
  }

  /** Tear down every transport + timer. Idempotent (R-23 / @operations). */
  disconnect(): void {
    if (this.#terminated) return;
    this.#terminated = true;
    for (const timer of this.#timers) {
      clearTimeout(timer);
    }
    this.#timers.clear();
    for (const transport of this.#transports) {
      this.#safeClose(transport);
    }
  }

  // ----------------------------------------------------------------------------
  // Internal
  // ----------------------------------------------------------------------------

  async #connectOne(url: string, index: number, jwt: string): Promise<void> {
    const transport = this.#connectFn(url);
    // Register for teardown IMMEDIATELY — BEFORE awaiting ready — so disconnect()
    // can always reach an in-flight / never-ready transport (no leak).
    this.#transports.push(transport);
    // This connect-only handshake never observes `closed` (no media-plane I/O, no
    // reconnect this story). Attach a no-op catch so a transport that rejects
    // `closed` (e.g. a connect failure) doesn't surface as an unhandled rejection;
    // the connect outcome is driven entirely off `ready` + the write below.
    void transport.closed.catch(() => {});

    let timer: ReturnType<typeof setTimeout> | undefined;
    const deadline = new Promise<'timeout'>((resolve) => {
      timer = setTimeout(() => resolve('timeout'), this.#connectTimeoutMs);
      this.#timers.add(timer);
    });
    const clearDeadline = (): void => {
      if (timer !== undefined) {
        clearTimeout(timer);
        this.#timers.delete(timer);
      }
    };

    try {
      // Map ready's reject → 'error' so the loser of the race never produces an
      // unhandled rejection if the deadline wins. Capture the rejection reason so it
      // can be preserved on the failure's `Error.cause` (symmetric with the
      // write-path catch below). This cause is leak-free: the `ready` rejection
      // fires BEFORE the joinToken envelope is ever written, so it cannot echo token
      // bytes — and it lives only on the non-enumerable `Error.cause` regardless.
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
      if (this.#terminated) return;
      if (outcome === 'timeout') {
        this.#failOne(
          url,
          index,
          MediaConnectionErrorCode.ConnectTimeout,
          REASON_TIMEOUT,
          undefined,
        );
        this.#safeClose(transport);
        return;
      }
      if (outcome === 'error') {
        this.#failOne(url, index, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, readyError);
        this.#safeClose(transport);
        return;
      }
      const stream = await transport.createBidirectionalStream();
      const envelope = create(MhClientMessageSchema, {
        message: {
          case: 'connectRequest',
          value: create(MhConnectRequestSchema, { joinToken: jwt }),
        },
      });
      // R-58: inject under the active dt_client.join span. The wrap is SYNCHRONOUS
      // around the inject (the first step of `sendFramedMessage`); the async write
      // tail runs after. (A coarse wrap around `connectAll` would lose context at
      // the first await under the StackContextManager.)
      await this.#runInContext(() => sendFramedMessage(stream, MhClientMessageSchema, envelope));
      // A disconnect mid-handshake closes the transport, so createBidi/write would
      // throw into the catch (whose #terminated guard handles it); no intermediate
      // checks needed here.
      this.#succeedOne(url, index);
    } catch (err) {
      clearDeadline();
      if (this.#terminated) return;
      this.#failOne(url, index, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, err);
      this.#safeClose(transport);
    }
  }

  #succeedOne(url: string, index: number): void {
    this.#states.set(url, 'connected');
    this.#observedAt.set(url, this.#clock());
    this.#emitMetric('success', index);
    this.emit('connected', url);
  }

  #failOne(
    url: string,
    index: number,
    code: MediaConnectionErrorCode,
    reason: string,
    cause: unknown,
  ): void {
    this.#states.set(url, 'failed');
    this.#observedAt.set(url, this.#clock());
    this.#failures.set(url, { failureCode: code, failureReason: reason });
    this.#emitMetric('failure', index);
    this.emit(
      'failed',
      // Use the COMPUTED `reason` (not a hardcoded one) so the error's `.message`
      // always matches its `.mediaCode` — e.g. a CONNECT_TIMEOUT carries the timeout
      // reason, a TRANSPORT carries the transport reason (@code-reviewer F1).
      new MediaConnectionError(code, reason, {
        mhUrl: capMhUrl(url),
        ...(cause !== undefined ? { cause } : {}),
      }),
    );
  }

  #emitMetric(status: 'success' | 'failure', index: number): void {
    this.#metricsSink?.counter('dt_client_mh_connection_total', {
      ...this.#metricLabels,
      status,
      mh_index_bucket: mhIndexBucket(index),
    });
  }

  #safeClose(transport: IWebTransport): void {
    try {
      transport.close();
    } catch {
      // Already closed / errored — ignore (one bad close can't abort the rest).
    }
  }
}
