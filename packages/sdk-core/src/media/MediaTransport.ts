// File: packages/sdk-core/src/media/MediaTransport.ts
//
// R-20/R-21/R-58/R-60 (SDK side): the active/active MH media-transport layer.
// `connectAll(urls, jwt)` opens a WebTransport connection to EVERY MH URL from
// `JoinResponse.media_servers` IN PARALLEL and performs ONLY the auth handshake on
// each: open the first bidi stream, write a single length-prefixed
// `MhClientMessage{connectRequest{joinToken}}` envelope (trace-injected via the SAME
// shared `sendFramedMessage` path as the MC send), and treat the MH as CONNECTED iff
// `ready` resolved AND that envelope wrote without error. This layer does no
// SFrame and no WebCodecs; datagrams and the control channel (below) are
// exposed to the layers above.
//
// STORY 3 ADDS THE MH CONTROL CHANNEL (C-A1). The connect bidi stream is no
// longer write-once: after the connect envelope it carries, in BOTH directions,
// the same 4-byte big-endian length prefix and size bound as `MhClientMessage`:
//   * MH -> client: `MhServerMessage` (a `keyframe_request`), read by a per-handler
//     loop (`#runControlReadLoop`) through the shared `FrameDecoder`, which
//     refuses an oversized length from the PREFIX ALONE, before the body is
//     buffered. Malformed input is COUNTED (via the injected `controlSink`) and
//     never throws: a decode failure skips the message, a framing failure stops
//     that handler's loop (the stream is desynchronised; media is unaffected).
//   * client -> MH: `MhClientMessage.recovery_request`, via `sendRecoveryRequest`
//     (story 3 task 14, C-B, is its first caller), framed by the same
//     `sendFramedMessage` as the connect envelope and serialised per handler.
// The loop and the control send hold the STREAM only — never the meeting JWT or
// the connect envelope that carried it.
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
// TASK 19 ADDS DATAGRAM I/O. `getDatagramChannel(url)` exposes a send/receive
// channel over an already-connected MH, and the three
// `WebTransportDatagramDuplexStream` queue knobs are SET from configuration at
// connect time (ADR-0036 §1: "the default being adequate is not the same as the
// default being chosen"). The application egress queue lives one layer up, in
// `pipeline/egressQueue.ts`, and must trip BEFORE the transport ceiling so the
// drop decision — and the count — happen in our code rather than silently inside
// the user agent, where neither this client nor the media handler could see it.
//
// R-23: the meeting JWT is threaded only as the `connectAll` parameter into each
// `#connectOne`; it is never stored on the instance. `failureReason`/`failureCode`
// are SDK-authored bounded classifications (never a raw transport-error message).

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
  DEFAULT_MH_CONNECT_TIMEOUT_MS,
  type DatagramQueueSettings,
  type MediaConnectionState,
  type MediaDatagramChannel,
  type MediaTransportEventMap,
  type MediaTransportOptions,
  type MhControlSink,
  type MhRecoveryRequest,
  type RunInContext,
} from './events.js';

/** Static, bounded failure reasons (never a raw cause string — R-23/security). */
const REASON_TRANSPORT = 'media handler connection failed';
const REASON_TIMEOUT = 'media handler connect deadline exceeded';
const REASON_NO_CONTROL_CHANNEL = 'no control channel is open to that media handler';

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
  /** Per-URL datagram channels, created lazily on first use after connect. */
  readonly #datagramChannels = new Map<string, MediaDatagramChannel>();
  readonly #datagramWriters = new Map<string, WritableStreamDefaultWriter<Uint8Array>>();
  /** Connected transports by URL, so a datagram channel can find its transport. */
  readonly #connectedTransports = new Map<string, IWebTransport>();
  readonly #datagramQueue: DatagramQueueSettings | undefined;
  readonly #controlSink: MhControlSink | undefined;
  /**
   * Per-URL control stream (the connect bidi stream after its envelope) and the
   * tail of its serialised send chain. Holds the STREAM only — never the JWT.
   */
  readonly #controlChannels = new Map<
    string,
    { readonly stream: WebTransportBidirectionalStream; tail: Promise<void> }
  >();
  readonly #controlReaders = new Set<ReadableStreamDefaultReader<Uint8Array>>();

  #terminated = false;

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
    for (const writer of this.#datagramWriters.values()) {
      try {
        void writer.close().catch(() => {
          // A writer on an already-errored stream rejects on close. The
          // transport close below is what actually releases the connection.
        });
      } catch {
        // Already released — ignore.
      }
    }
    this.#datagramWriters.clear();
    this.#datagramChannels.clear();
    for (const reader of this.#controlReaders) {
      void reader.cancel().catch(() => {
        // Already released or errored — the transport close below ends it.
      });
    }
    this.#controlReaders.clear();
    this.#controlChannels.clear();
    this.#connectedTransports.clear();
    for (const transport of this.#transports) {
      this.#safeClose(transport);
    }
  }

  /**
   * A datagram send/receive channel over a CONNECTED media handler.
   *
   * Returns `undefined` for a URL that is not connected — the caller counts that
   * as `not_connected` rather than being handed a channel that cannot send.
   *
   * The channel holds ONE cached writer: `send` awaits `writer.ready` before
   * every write, so at most one write is in flight in the transport and queue
   * depth accumulates in the application queue where it can be bounded and
   * counted.
   */
  getDatagramChannel(url: string): MediaDatagramChannel | undefined {
    const existing = this.#datagramChannels.get(url);
    if (existing) return existing;
    const transport = this.#connectedTransports.get(url);
    if (!transport || this.#terminated) return undefined;

    const writer = transport.datagrams.writable.getWriter();
    this.#datagramWriters.set(url, writer);
    let open = true;
    // `closed` resolving or rejecting both mean the same thing to a sender:
    // there is no transport left. Distinguishing them would produce two drop
    // reasons for one condition.
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
      // The platform reports this on the duplex stream; it is absent under a
      // test double and under a UA that does not expose it, in which case the
      // sender skips the oversize check rather than inventing a bound.
      maxDatagramSize: transport.datagrams.maxDatagramSize,
      async send(bytes: Uint8Array): Promise<void> {
        await writer.ready;
        await writer.write(bytes);
      },
    };
    this.#datagramChannels.set(url, channel);
    return channel;
  }

  /**
   * Open one OUTGOING unidirectional stream to the CONNECTED handler at `url`
   * (story 3: one stream per video group per handler), or `undefined` when no
   * transport to `url` is connected.
   *
   * A SELECTOR, NEVER A DIALER — the same rule as `getDatagramChannel`: it
   * looks up transports `connectAll` already opened and can never create one,
   * so a directed target can pick among those transports only.
   */
  openUniStream(url: string): Promise<WritableStream<Uint8Array>> | undefined {
    const transport = this.#connectedTransports.get(url);
    if (!transport || this.#terminated) return undefined;
    return transport.createUnidirectionalStream();
  }

  /**
   * The INCOMING unidirectional streams of the CONNECTED handler at `url`
   * (story 3 C-B: one stream per forwarded video group), or `undefined` when no
   * transport to `url` is connected.
   *
   * A SELECTOR, NEVER A DIALER — the same rule as `getDatagramChannel`.
   */
  incomingUniStreamsFor(url: string): ReadableStream<ReadableStream<Uint8Array>> | undefined {
    const transport = this.#connectedTransports.get(url);
    if (!transport || this.#terminated) return undefined;
    return transport.incomingUnidirectionalStreams;
  }

  /**
   * Send a receiver's recovery request for one of its slots to the handler at
   * `url` (story 3 R-13), as `MhClientMessage.recovery_request` on that
   * handler's control stream. Framed by the same pipeline as the connect
   * envelope; sends to one handler are serialised (one writer at a time).
   *
   * @throws {MediaConnectionError} `Transport` when no control channel is open
   * to `url`, or the write fails. Bounded messages only.
   */
  async sendRecoveryRequest(url: string, request: MhRecoveryRequest): Promise<void> {
    const channel = this.#controlChannels.get(url);
    if (!channel || this.#terminated) {
      throw new MediaConnectionError(
        MediaConnectionErrorCode.Transport,
        REASON_NO_CONTROL_CHANNEL,
        {
          mhUrl: capMhUrl(url),
        },
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
        mhUrl: capMhUrl(url),
      });
    }
  }

  /**
   * Open the MH control channel on `stream` — the connect bidi stream AFTER its
   * envelope. Takes the URL, the transport and the stream ONLY: nothing that
   * carried the meeting JWT is captured by the loop or the control send.
   */
  #startControlChannel(
    url: string,
    transport: IWebTransport,
    stream: WebTransportBidirectionalStream,
  ): void {
    this.#controlChannels.set(url, { stream, tail: Promise.resolve() });
    // The transport ending ends the channel, whatever the read loop is doing:
    // a later send then reports "no control channel", not a write failure.
    const drop = (): void => this.#dropControlChannel(url, stream);
    void transport.closed.then(drop, drop);
    void this.#runControlReadLoop(url, stream);
  }

  /** Forget `url`'s control channel if it is still this stream. */
  #dropControlChannel(url: string, stream: WebTransportBidirectionalStream): void {
    if (this.#controlChannels.get(url)?.stream === stream) this.#controlChannels.delete(url);
  }

  /**
   * Read `MhServerMessage`s off a handler's control stream until it ends.
   * NEVER throws: every failure is bounded, counted through the control sink,
   * and ends or continues the loop.
   */
  async #runControlReadLoop(url: string, stream: WebTransportBidirectionalStream): Promise<void> {
    let reader: ReadableStreamDefaultReader<Uint8Array>;
    try {
      reader = stream.readable.getReader();
    } catch {
      return;
    }
    this.#controlReaders.add(reader);
    // The SAME decoder as MC signalling: the 64 KiB per-message bound is checked
    // on the length prefix, before the body is buffered.
    const decoder = new FrameDecoder();
    let desynchronised = false;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (this.#terminated) return;
        if (done) {
          // MH finished its send half: no keyframe request can arrive again.
          this.#controlSink?.ended(url, 'finished');
          return;
        }
        let frames: Uint8Array[];
        try {
          frames = decoder.push(value);
        } catch {
          // Oversized or empty length: the INBOUND half is desynchronised and no
          // later boundary can be trusted. Count it and stop reading; its
          // datagrams are unaffected. The channel entry is KEPT deliberately: the
          // OUTBOUND half is independent of an inbound desync, so recovery
          // requests can still be sent on it.
          this.#controlSink?.malformed(url);
          desynchronised = true;
          return;
        }
        for (const frame of frames) this.#dispatchControl(url, frame);
      }
    } catch {
      // The stream was reset or the transport ended. Teardown is ours and is not
      // counted; anything else silently ends this handler's keyframe requests,
      // so it is made visible.
      if (!this.#terminated) this.#controlSink?.ended(url, 'reset');
    } finally {
      this.#controlReaders.delete(reader);
      // The stream ENDED (MH finished or reset it, or the transport went): the
      // channel is gone, so a later send reports "no control channel" — not a
      // generic write failure an operator cannot tell from a transient.
      if (!desynchronised) this.#dropControlChannel(url, stream);
      try {
        reader.releaseLock();
      } catch {
        // Already released during teardown.
      }
    }
  }

  /** Decode one control message and hand it to the sink. Never throws. */
  #dispatchControl(url: string, frame: Uint8Array): void {
    let message;
    try {
      message = fromBinary(MhServerMessageSchema, frame);
    } catch {
      this.#controlSink?.malformed(url);
      return;
    }
    switch (message.message.case) {
      case 'keyframeRequest':
        this.#controlSink?.keyframeRequest(url, {
          streamNumber: message.message.value.streamNumber,
          gopCounter: message.message.value.gopCounter,
        });
        return;
      default:
        // An unset or unknown variant carries nothing this client can act on.
        this.#controlSink?.malformed(url);
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

  // ----------------------------------------------------------------------------
  // Internal
  // ----------------------------------------------------------------------------

  async #connectOne(url: string, index: number, jwt: string): Promise<void> {
    let transport: IWebTransport;
    try {
      transport = this.#connectFn(url);
    } catch (err) {
      // A SYNCHRONOUS connect failure (an unavailable WebTransport constructor, a
      // URL the constructor rejects, a refused dial) is a failed MH like any
      // other: recorded with the bounded transport reason — never the raw
      // message (R-23) — rather than escaping this async function, where
      // `allSettled` would swallow it and leave the URL `connecting` forever.
      this.#failOne(url, index, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, err);
      return;
    }
    // Register for teardown IMMEDIATELY — BEFORE awaiting ready — so disconnect()
    // can always reach an in-flight / never-ready transport (no leak).
    this.#transports.push(transport);
    // The handshake never observes `closed` (the datagram channel and the
    // control channel each observe their own end; no reconnect this story).
    // Attach a no-op catch so a transport that rejects `closed` (e.g. a connect
    // failure) doesn't surface as an unhandled rejection; the connect outcome is
    // driven entirely off `ready` + the write below.
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
      // CHOOSE the datagram queue depths rather than inheriting the user
      // agent's. A drop inside the UA's outgoing queue is uncountable by us AND
      // structurally invisible to MH — the one drop class ADR-0036 §11 says
      // matters most — so the transport queue is kept shallow and the bounded
      // application queue above it makes the decision. `outgoingMaxAge` is a
      // BACKSTOP set clear of what both queues can legitimately hold; at a value
      // close to that sum it would become a routine uncountable competitor
      // instead. `validateMediaConfig` asserts both relationships at setup.
      this.#applyDatagramQueueSettings(transport);

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
      this.#succeedOne(url, index, transport);
      this.#startControlChannel(url, transport, stream);
    } catch (err) {
      clearDeadline();
      if (this.#terminated) return;
      this.#failOne(url, index, MediaConnectionErrorCode.Transport, REASON_TRANSPORT, err);
      this.#safeClose(transport);
    }
  }

  #succeedOne(url: string, index: number, transport: IWebTransport): void {
    this.#connectedTransports.set(url, transport);
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

  #safeClose(transport: IWebTransport): void {
    try {
      transport.close();
    } catch {
      // Already closed / errored — ignore (one bad close can't abort the rest).
    }
  }
}
