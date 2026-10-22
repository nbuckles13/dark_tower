// File: packages/test-utils/src/MockWebTransport.ts
//
// Test double for `IWebTransport`. R-15 control points
// (`simulateReady`/`simulateClose`/`simulateError`/`simulateIncomingDatagram`/
// `simulateBidiStream`/`simulateServerMessage`) plus inspector helpers for
// asserting on outbound writes (R-19/R-58 trace-context tests in task #13
// will assert that ClientMessage envelopes carry populated
// trace_parent/trace_state).

import type {
  IWebTransport,
  WebTransportBidirectionalStream,
  WebTransportCloseInfo,
  WebTransportDatagrams,
} from './contracts/IWebTransport.js';
import {
  MockUniStreams,
  streamError,
  type InboundUniStreamHandle,
  type InboundUniStreamInfo,
  type OutboundUniStreamInfo,
} from './mockUniStreams.js';

/**
 * Width of the length prefix on MC/MH bidi control streams (4-byte big-endian).
 *
 * A DELIBERATE, INDEPENDENT COPY of the framing contract whose canonical home is
 * `packages/sdk-core/src/framing/length-prefix.ts`: test-utils takes no sdk-core
 * dependency (and must not gain one), and a splitter written separately is a
 * better oracle than reusing the decoder under test. Drift is caught by
 * `packages/sdk-core/src/media/__tests__/mhControlChannel.test.ts`, which frames
 * with sdk-core's `encodeFrame`/`sendFramedMessage` and splits with
 * {@link MockWebTransport.getOutboundBidiFrames}.
 */
const LENGTH_PREFIX_BYTES = 4;

interface CapturedBidiStream {
  readonly index: number;
  readonly openedAt: number;
  readonly outboundChunks: Uint8Array[];
  readonly stream: WebTransportBidirectionalStream;
  readonly readableController: ReadableStreamDefaultController<Uint8Array>;
}

/**
 * Test double for `IWebTransport`. Tests drive the transport via
 * `simulate*` control points and assert on outbound traffic via the
 * `get*` inspector helpers.
 *
 * @example
 * const wt = new MockWebTransport();
 * wt.simulateReady();
 * const stream = await wt.createBidirectionalStream();
 * // ...drive SDK code...
 * wt.simulateServerMessage(0, encodedJoinResponse);
 * expect(wt.getOutboundBidiWrites(0)).toHaveLength(1);
 */
export class MockWebTransport implements IWebTransport {
  readonly #readyResolvers: { resolve: () => void; reject: (e: Error) => void };
  readonly #closedResolvers: {
    resolve: (info: WebTransportCloseInfo) => void;
    reject: (e: Error) => void;
  };

  readonly ready: Promise<void>;
  readonly closed: Promise<WebTransportCloseInfo>;

  readonly #datagramReadable: ReadableStream<Uint8Array>;
  readonly #datagramReadableController: ReadableStreamDefaultController<Uint8Array>;
  readonly #datagramWritable: WritableStream<Uint8Array>;
  readonly #outboundDatagrams: Uint8Array[] = [];
  /** Outbound datagrams the injected drop policy discarded. Never in `getOutboundDatagrams()`. */
  readonly #droppedOutboundDatagrams: Uint8Array[] = [];
  /** Inbound datagrams `simulateDatagramLoss` recorded without delivering. */
  readonly #lostInboundDatagrams: Uint8Array[] = [];

  /** Injected send-side drop policy. `true` => discard silently, as a lossy path would. */
  #dropOutbound: ((bytes: Uint8Array, index: number) => boolean) | undefined;
  /** Injected send-side failure. When set, the writable's `write` rejects with it. */
  #writeFailure: Error | undefined;
  /** Backpressure gate: while set, `write` blocks until `resumeDatagramWrites()`. */
  #writeGate: { promise: Promise<void>; release: () => void } | undefined;
  /** How many datagrams the SDK has offered, dropped or not. Drives index-based policies. */
  #offeredDatagramCount = 0;

  /**
   * The duplex object handed out by `datagrams`, built once.
   *
   * The three queue knobs are PLAIN MUTABLE FIELDS on it so a test can assert the
   * value the SDK CHOSE, rather than inferring that it chose one — ADR-0036 §1's
   * "the default being adequate is not the same as the default being chosen",
   * made checkable.
   *
   * `maxDatagramSize` is `undefined` by default, matching a platform that does
   * not report one, so the sender's oversize check is EXERCISED only when a test
   * opts in via `setMaxDatagramSize` rather than silently skipped everywhere.
   */
  readonly #datagrams: WebTransportDatagrams;

  readonly #bidiStreams: CapturedBidiStream[] = [];

  /** Uni streams, both directions. See `mockUniStreams.ts` for the model. */
  readonly #uni = new MockUniStreams();

  #closed: boolean = false;

  constructor() {
    let readyResolve!: () => void;
    let readyReject!: (e: Error) => void;
    this.ready = new Promise<void>((resolve, reject) => {
      readyResolve = resolve;
      readyReject = reject;
    });
    this.#readyResolvers = { resolve: readyResolve, reject: readyReject };

    let closedResolve!: (info: WebTransportCloseInfo) => void;
    let closedReject!: (e: Error) => void;
    this.closed = new Promise<WebTransportCloseInfo>((resolve, reject) => {
      closedResolve = resolve;
      closedReject = reject;
    });
    this.#closedResolvers = { resolve: closedResolve, reject: closedReject };

    let datagramReadableController!: ReadableStreamDefaultController<Uint8Array>;
    this.#datagramReadable = new ReadableStream<Uint8Array>({
      start(controller) {
        datagramReadableController = controller;
      },
    });
    this.#datagramReadableController = datagramReadableController;

    this.#datagramWritable = new WritableStream<Uint8Array>({
      write: async (chunk) => {
        // Backpressure FIRST: a gated write must not be recorded as sent, or the
        // egress-queue test would see the frame leave while the transport is
        // stalled — which is the state the bounded queue exists to survive.
        if (this.#writeGate) await this.#writeGate.promise;
        if (this.#writeFailure) throw this.#writeFailure;
        const index = this.#offeredDatagramCount++;
        const copy = copyChunk(chunk);
        if (this.#dropOutbound?.(copy, index) === true) {
          // Recorded SEPARATELY from `getOutboundDatagrams()`. A silently-lossy
          // path that also appeared in the sent list would make "sent" and
          // "arrived" indistinguishable, which is the whole property these tests
          // exist to separate.
          this.#droppedOutboundDatagrams.push(copy);
          return;
        }
        this.#outboundDatagrams.push(copy);
      },
    });

    this.#datagrams = {
      readable: this.#datagramReadable,
      writable: this.#datagramWritable,
      outgoingHighWaterMark: undefined,
      outgoingMaxAge: undefined,
      incomingHighWaterMark: undefined,
      maxDatagramSize: undefined,
    };
  }

  // ---------------- IWebTransport ----------------

  get datagrams(): WebTransportDatagrams {
    // The SAME object every time, so a setter the SDK calls reaches the state
    // `getDatagramQueueSettings()` reads back. Returning a fresh literal would
    // make every knob the SDK sets vanish, and the assertion that it CHOSE a
    // value would silently become an assertion about a throwaway object.
    return this.#datagrams;
  }

  /**
   * Open a new bidirectional stream. Call after `simulateReady()` —
   * mirrors browser-WebTransport semantics that streams open after the
   * handshake.
   */
  async createBidirectionalStream(): Promise<WebTransportBidirectionalStream> {
    if (this.#closed) {
      throw new Error('MockWebTransport: cannot open bidi stream on closed transport');
    }
    const index = this.#bidiStreams.length;
    const outboundChunks: Uint8Array[] = [];
    let readableController!: ReadableStreamDefaultController<Uint8Array>;
    const readable = new ReadableStream<Uint8Array>({
      start(c) {
        readableController = c;
      },
    });
    const writable = new WritableStream<Uint8Array>({
      write(chunk) {
        outboundChunks.push(copyChunk(chunk));
      },
    });
    const stream: WebTransportBidirectionalStream = { readable, writable };
    this.#bidiStreams.push({
      index,
      openedAt: Date.now(),
      outboundChunks,
      stream,
      readableController,
    });
    return stream;
  }

  /**
   * Open an outgoing uni stream. Indexed in its OWN ordinal space (0 = first
   * SDK-opened uni stream), separate from bidi and from inbound uni.
   */
  async createUnidirectionalStream(): Promise<WritableStream<Uint8Array>> {
    if (this.#closed) {
      throw new Error('MockWebTransport: cannot open uni stream on closed transport');
    }
    return this.#uni.create();
  }

  /** Peer-opened uni streams; buffered until the SDK reads them. */
  get incomingUnidirectionalStreams(): ReadableStream<ReadableStream<Uint8Array>> {
    return this.#uni.incoming;
  }

  /** Close the transport, resolving the `closed` promise. */
  close(info?: WebTransportCloseInfo): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#uni.end();
    this.#closedResolvers.resolve(info ?? {});
  }

  // ---------------- R-15 control points ----------------

  /** Resolve the `ready` promise. Subsequent calls are no-ops. */
  simulateReady(): void {
    this.#readyResolvers.resolve();
  }

  /** Resolve the `closed` promise with the given close info. */
  simulateClose(code?: number, reason?: string): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#uni.end();
    const info: WebTransportCloseInfo = {
      ...(code !== undefined ? { closeCode: code } : {}),
      ...(reason !== undefined ? { reason } : {}),
    };
    this.#closedResolvers.resolve(info);
  }

  /** Reject both `ready` (if pending) and `closed` with the given error. */
  simulateError(err: Error): void {
    this.#readyResolvers.reject(err);
    if (!this.#closed) {
      this.#closed = true;
      this.#uni.end(err);
      this.#closedResolvers.reject(err);
    }
  }

  /** Push an inbound datagram to `datagrams.readable`. */
  simulateIncomingDatagram(bytes: Uint8Array): void {
    this.#datagramReadableController.enqueue(copyChunk(bytes));
  }

  /**
   * Record an inbound datagram as LOST — the network dropped it, so the SDK
   * never sees it.
   *
   * Deliberately a method rather than "just don't call `simulateIncomingDatagram`":
   * a test asserting `received = accepted + sum(drops)` needs to state which
   * frames were withheld, and an omission states nothing. `getLostInboundDatagrams()`
   * is what lets the assertion say "these three never arrived" rather than leaving
   * the reader to infer it from a count.
   */
  simulateDatagramLoss(bytes: Uint8Array): void {
    this.#lostInboundDatagrams.push(copyChunk(bytes));
  }

  /**
   * Close `datagrams.readable`, as a transport whose datagram stream ends.
   *
   * Distinct from `simulateClose()`: the reader's loop terminates while the
   * connection stays up, which is a degradation the pipeline must surface rather
   * than absorb silently.
   */
  simulateDatagramReadableEnd(): void {
    this.#datagramReadableController.close();
  }

  /**
   * Inject a send-side drop policy. Return `true` to discard the datagram.
   *
   * Models a lossy path BELOW our transport seam — the class of loss neither
   * end can count. Pass `undefined` to clear.
   */
  setOutboundDatagramDropPolicy(
    policy: ((bytes: Uint8Array, index: number) => boolean) | undefined,
  ): void {
    this.#dropOutbound = policy;
  }

  /**
   * Report a maximum datagram size, as a real transport does.
   *
   * Left `undefined` by default so the SDK's oversize check is exercised only
   * where a test asks for it — a mock that always reported a limit would make
   * that branch look covered everywhere and tested nowhere.
   */
  setMaxDatagramSize(bytes: number | undefined): void {
    (this.#datagrams as { maxDatagramSize: number | undefined }).maxDatagramSize = bytes;
  }

  /** Make every subsequent datagram `write` reject with `err`. Pass `undefined` to clear. */
  setDatagramWriteFailure(err: Error | undefined): void {
    this.#writeFailure = err;
  }

  /**
   * Stall the datagram writable so the SDK's bounded egress queue fills.
   *
   * This is how the drop-oldest path is reached deterministically: with the
   * writer gated, the queue is the only place frames can accumulate, and the
   * eviction is the SDK's own decision rather than the user agent's.
   */
  pauseDatagramWrites(): void {
    if (this.#writeGate) return;
    let release!: () => void;
    const promise = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.#writeGate = { promise, release };
  }

  /** Release a `pauseDatagramWrites()` stall. Idempotent. */
  resumeDatagramWrites(): void {
    const gate = this.#writeGate;
    this.#writeGate = undefined;
    gate?.release();
  }

  /**
   * Push raw inbound bytes to a previously-opened bidi stream's
   * `readable`. `streamIndex` selects the stream (0 = first opened).
   */
  simulateBidiStream(streamIndex: number, send: Uint8Array): void {
    const stream = this.#bidiStreams[streamIndex];
    if (stream === undefined) {
      throw new Error(`MockWebTransport: no bidi stream at index ${streamIndex}`);
    }
    stream.readableController.enqueue(copyChunk(send));
  }

  /**
   * The PEER finishes its send half of bidi stream `streamIndex` (FIN): the
   * SDK's reader then reads `done`.
   */
  simulateBidiStreamEnd(streamIndex: number): void {
    this.#requireBidi(streamIndex).readableController.close();
  }

  /**
   * The PEER resets its send half of bidi stream `streamIndex`: the SDK's next
   * read rejects with the platform stream-error shape carrying `code`.
   */
  simulateBidiStreamReset(streamIndex: number, code: number | null): void {
    this.#requireBidi(streamIndex).readableController.error(streamError(code));
  }

  #requireBidi(streamIndex: number): CapturedBidiStream {
    const stream = this.#bidiStreams[streamIndex];
    if (stream === undefined) {
      throw new Error(`MockWebTransport: no bidi stream at index ${streamIndex}`);
    }
    return stream;
  }

  /**
   * Convenience alias for `simulateBidiStream` framed for tests that think
   * in terms of "server message" rather than "raw bytes". Tests responsible
   * for producing the framed envelope (4-byte BE length prefix + protobuf)
   * via their own helpers.
   */
  simulateServerMessage(streamIndex: number, framedBytes: Uint8Array): void {
    this.simulateBidiStream(streamIndex, framedBytes);
  }

  // ---------------- Uni-stream control points ----------------

  /**
   * Open a peer-initiated (inbound) uni stream and return its handle. Inbound
   * streams have their OWN ordinal space (0 = first simulated). Buffered on
   * `incomingUnidirectionalStreams` until the SDK reads it, never dropped.
   */
  simulateIncomingUniStream(): InboundUniStreamHandle {
    return this.#uni.simulateIncoming();
  }

  /**
   * Peer STOP_SENDING on outbound uni stream `index`. The SDK's next write
   * (including one stalled on a pause gate) rejects with the platform
   * stream-error shape carrying `code`. Recorded as `stopSendingCode`, never as
   * the SDK's own `ending`.
   */
  simulateOutboundUniStopSending(index: number, code: number | null): void {
    this.#uni.stopSending(index, code);
  }

  /**
   * Slow-reader knob for outbound uni stream `index` (PER STREAM): writes stall
   * until `resumeUniStreamWrites(index)`, so the SDK sees backpressure
   * (`writer.ready` pending). A stalled write is NOT in the inspector until it
   * is released. No timers.
   */
  pauseUniStreamWrites(index: number): void {
    this.#uni.pause(index);
  }

  /** Release `pauseUniStreamWrites(index)`. Idempotent. */
  resumeUniStreamWrites(index: number): void {
    this.#uni.resume(index);
  }

  // ---------------- Inspectors ----------------

  /** Every SDK-opened uni stream: accepted chunks, the SDK's ending, any injected stop. */
  getOutboundUniStreams(): readonly OutboundUniStreamInfo[] {
    return this.#uni.outboundInfo();
  }

  /** Every simulated inbound uni stream: the injected peer ending and whether the SDK cancelled. */
  getInboundUniStreams(): readonly InboundUniStreamInfo[] {
    return this.#uni.inboundInfo();
  }

  /**
   * The length-prefixed messages the SDK wrote on bidi stream `streamIndex`,
   * as payloads (prefix stripped), split from the concatenated writes.
   *
   * THROWS on a truncated trailing frame or a leftover partial prefix, rather
   * than dropping it: a framing bug must read as a framing bug, never as "N-1
   * messages were sent". Raw bytes only — test-utils has no protobuf; tests
   * decode the payloads with sdk-core's generated schemas.
   */
  getOutboundBidiFrames(streamIndex: number = 0): readonly Uint8Array[] {
    const chunks = this.getOutboundBidiWrites(streamIndex);
    const total = chunks.reduce((n, c) => n + c.byteLength, 0);
    const buf = new Uint8Array(total);
    let offset = 0;
    for (const c of chunks) {
      buf.set(c, offset);
      offset += c.byteLength;
    }
    const frames: Uint8Array[] = [];
    let pos = 0;
    while (pos < buf.byteLength) {
      if (buf.byteLength - pos < LENGTH_PREFIX_BYTES) {
        throw new Error(
          `MockWebTransport: bidi stream ${streamIndex} ends with a partial length prefix ` +
            `(${buf.byteLength - pos} byte(s))`,
        );
      }
      const len = new DataView(buf.buffer, pos, LENGTH_PREFIX_BYTES).getUint32(0, false);
      const end = pos + LENGTH_PREFIX_BYTES + len;
      if (end > buf.byteLength) {
        throw new Error(
          `MockWebTransport: bidi stream ${streamIndex} ends with a truncated frame ` +
            `(declared ${len} byte(s), ${buf.byteLength - pos - LENGTH_PREFIX_BYTES} present)`,
        );
      }
      frames.push(buf.slice(pos + LENGTH_PREFIX_BYTES, end));
      pos = end;
    }
    return frames;
  }

  /** All datagrams written via `datagrams.writable` AND NOT DROPPED, in order. */
  getOutboundDatagrams(): readonly Uint8Array[] {
    return this.#outboundDatagrams;
  }

  /** Datagrams the injected drop policy discarded, in order. Disjoint from the above. */
  getDroppedOutboundDatagrams(): readonly Uint8Array[] {
    return this.#droppedOutboundDatagrams;
  }

  /** Inbound datagrams recorded by `simulateDatagramLoss` and never delivered. */
  getLostInboundDatagrams(): readonly Uint8Array[] {
    return this.#lostInboundDatagrams;
  }

  /**
   * Datagrams the SDK OFFERED to the transport, dropped or not.
   *
   * The denominator for a drop-policy assertion, and distinct from
   * `getOutboundDatagrams().length` exactly when a policy is active.
   */
  getOfferedDatagramCount(): number {
    return this.#offeredDatagramCount;
  }

  /**
   * The three queue knobs as the SDK left them.
   *
   * `undefined` means the SDK never set one — which is a FAILURE for a value
   * ADR-0036 §1 requires be chosen rather than inherited, so a test asserts the
   * number and not merely that a write happened.
   */
  getDatagramQueueSettings(): {
    readonly outgoingHighWaterMark: number | undefined;
    readonly outgoingMaxAge: number | null | undefined;
    readonly incomingHighWaterMark: number | undefined;
  } {
    return {
      outgoingHighWaterMark: this.#datagrams.outgoingHighWaterMark,
      outgoingMaxAge: this.#datagrams.outgoingMaxAge,
      incomingHighWaterMark: this.#datagrams.incomingHighWaterMark,
    };
  }

  /**
   * Raw byte chunks written to a bidi stream's `writable`. `streamIndex`
   * defaults to 0 (first opened).
   *
   * Returns chunks AS WRITTEN by the SDK. Consumers must reconstruct frames
   * by reading the 4-byte big-endian length prefix off a concatenated
   * buffer (per ADR-0028 §3 / R-16).
   *
   * NOTE(task #13): once protobuf-es types exist in sdk-core, the
   * recommended decode pattern will be documented in the package README.
   * For now, callers concatenate chunks then strip framing manually.
   */
  getOutboundBidiWrites(streamIndex: number = 0): readonly Uint8Array[] {
    const stream = this.#bidiStreams[streamIndex];
    if (stream === undefined) return [];
    return stream.outboundChunks;
  }

  /** Metadata about every bidi stream the SDK has opened. */
  getOpenedBidiStreams(): readonly { index: number; openedAt: number }[] {
    return this.#bidiStreams.map((s) => ({ index: s.index, openedAt: s.openedAt }));
  }

  /** Reset captured outbound traffic; preserves connection state. */
  clearInspector(): void {
    this.#outboundDatagrams.length = 0;
    this.#droppedOutboundDatagrams.length = 0;
    this.#lostInboundDatagrams.length = 0;
    this.#offeredDatagramCount = 0;
    for (const s of this.#bidiStreams) {
      s.outboundChunks.length = 0;
    }
    this.#uni.clearCaptures();
  }
}

function copyChunk(chunk: Uint8Array): Uint8Array {
  // Defensive copy — callers may reuse buffers; tests must see the bytes
  // as they were at write/read time.
  const out = new Uint8Array(chunk.byteLength);
  out.set(chunk);
  return out;
}
