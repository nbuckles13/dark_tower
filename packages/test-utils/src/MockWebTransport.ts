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
} from './contracts/IWebTransport.js';

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
  readonly #closedResolvers: { resolve: (info: WebTransportCloseInfo) => void; reject: (e: Error) => void };

  readonly ready: Promise<void>;
  readonly closed: Promise<WebTransportCloseInfo>;

  readonly #datagramReadable: ReadableStream<Uint8Array>;
  readonly #datagramReadableController: ReadableStreamDefaultController<Uint8Array>;
  readonly #datagramWritable: WritableStream<Uint8Array>;
  readonly #outboundDatagrams: Uint8Array[] = [];

  readonly #bidiStreams: CapturedBidiStream[] = [];

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

    const captured = this.#outboundDatagrams;
    this.#datagramWritable = new WritableStream<Uint8Array>({
      write(chunk) {
        captured.push(copyChunk(chunk));
      },
    });
  }

  // ---------------- IWebTransport ----------------

  get datagrams(): {
    readonly readable: ReadableStream<Uint8Array>;
    readonly writable: WritableStream<Uint8Array>;
  } {
    return { readable: this.#datagramReadable, writable: this.#datagramWritable };
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

  /** Close the transport, resolving the `closed` promise. */
  close(info?: WebTransportCloseInfo): void {
    if (this.#closed) return;
    this.#closed = true;
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
      this.#closedResolvers.reject(err);
    }
  }

  /** Push an inbound datagram to `datagrams.readable`. */
  simulateIncomingDatagram(bytes: Uint8Array): void {
    this.#datagramReadableController.enqueue(copyChunk(bytes));
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
   * Convenience alias for `simulateBidiStream` framed for tests that think
   * in terms of "server message" rather than "raw bytes". Tests responsible
   * for producing the framed envelope (4-byte BE length prefix + protobuf)
   * via their own helpers.
   */
  simulateServerMessage(streamIndex: number, framedBytes: Uint8Array): void {
    this.simulateBidiStream(streamIndex, framedBytes);
  }

  // ---------------- Inspectors ----------------

  /** All datagrams written via `datagrams.writable`, in order. */
  getOutboundDatagrams(): readonly Uint8Array[] {
    return this.#outboundDatagrams;
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
    for (const s of this.#bidiStreams) {
      s.outboundChunks.length = 0;
    }
  }
}

function copyChunk(chunk: Uint8Array): Uint8Array {
  // Defensive copy — callers may reuse buffers; tests must see the bytes
  // as they were at write/read time.
  const out = new Uint8Array(chunk.byteLength);
  out.set(chunk);
  return out;
}
