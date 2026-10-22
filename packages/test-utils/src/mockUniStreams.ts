// File: packages/test-utils/src/mockUniStreams.ts
//
// The unidirectional-stream half of `MockWebTransport` (story 3: one uni stream
// per video group per handler). Kept in its own module so the mock's datagram
// and bidi machinery stays readable; `MockWebTransport` owns one instance and
// forwards to it.
//
// ---------------------------------------------------------------------------
// DETERMINISM
// ---------------------------------------------------------------------------
//
// No wall clock and no timers. Streams are identified by ORDINAL INDEX, in
// three independent spaces: outbound uni (SDK-opened), inbound uni
// (peer-opened), and the mock's bidi streams (a separate list in
// `MockWebTransport`). Every stall is an explicit promise gate the test
// releases; every fault is injected synchronously by the test.
//
// ---------------------------------------------------------------------------
// WHO ENDED THE STREAM IS TWO FIELDS, NEVER ONE
// ---------------------------------------------------------------------------
//
// On an outbound stream the SDK chooses between FINISH (`writable.close()`)
// and RESET (`writable.abort()`), and story 3 R-12 makes that choice a
// contract ("reset, never finish"). The PEER can only ask us to stop
// (STOP_SENDING). So `ending` records the SDK's choice and `stopSendingCode`
// records the peer's request, separately: a test asserting the SDK's choice
// must never pass because of something the test itself injected.
//
// Inbound, the peer resets (`handle.reset(code)`) and the SDK may cancel
// (`readable.cancel()`), likewise recorded separately.
//
// Errors use the PLATFORM shape (`WebTransportError`, `source: 'stream'`,
// `streamErrorCode`), so SDK classification code runs unchanged against the
// mock and the browser.

import type { WebTransportStreamError } from './contracts/IWebTransport.js';

/** How the SDK ended an outbound uni stream. */
export type OutboundUniEnding = 'open' | 'closed' | 'aborted';

/** Inspector view of one outbound (SDK-opened) uni stream. */
export interface OutboundUniStreamInfo {
  readonly index: number;
  /** Chunks the SDK wrote and the mock ACCEPTED, in order. Gated writes appear only once released. */
  readonly chunks: readonly Uint8Array[];
  /** The SDK's own ending: finished (`closed`), reset (`aborted`), or neither. */
  readonly ending: OutboundUniEnding;
  /**
   * The code of a peer STOP_SENDING the TEST injected, `null` for one with no
   * code, `undefined` if none. Never derived from anything the SDK did.
   */
  readonly stopSendingCode: number | null | undefined;
  /** Whether writes are currently gated (`pauseUniStreamWrites`). */
  readonly paused: boolean;
}

/** Inspector view of one inbound (peer-opened) uni stream. */
export interface InboundUniStreamInfo {
  readonly index: number;
  /** Peer ending the TEST injected. */
  readonly peerEnding: 'open' | 'closed' | 'reset';
  /** Whether the SDK cancelled its reader (`readable.cancel()`). */
  readonly cancelled: boolean;
}

/** Test handle for driving one inbound uni stream. */
export interface InboundUniStreamHandle {
  readonly index: number;
  /** Deliver a chunk to the SDK's reader. */
  enqueue(bytes: Uint8Array): void;
  /** Finish the stream cleanly (peer FIN). */
  close(): void;
  /** Reset the stream (peer RESET_STREAM): the SDK's next read rejects with the code. */
  reset(code: number | null): void;
}

/** Build an error with the platform's stream-error shape. */
export function streamError(code: number | null): WebTransportStreamError {
  const err = new Error(
    `WebTransport stream error${code === null ? '' : ` (code ${code})`}`,
  ) as Error & { source: string; streamErrorCode: number | null };
  err.name = 'WebTransportError';
  err.source = 'stream';
  err.streamErrorCode = code;
  return err as unknown as WebTransportStreamError;
}

/** The error a session-level close puts on every stream still open. */
function sessionClosedError(): Error {
  const err = new Error('WebTransport session closed') as Error & {
    source: string;
    streamErrorCode: number | null;
  };
  err.name = 'WebTransportError';
  err.source = 'session';
  err.streamErrorCode = null;
  return err;
}

interface Gate {
  readonly promise: Promise<void>;
  readonly release: () => void;
}

function newGate(): Gate {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
}

interface OutboundState {
  readonly index: number;
  readonly chunks: Uint8Array[];
  ending: OutboundUniEnding;
  stopSendingCode: number | null | undefined;
  gate: Gate | undefined;
  /** Resolved when the stream is interrupted (peer stop or session end) so a gated write wakes. */
  readonly interrupt: Gate;
  interruptError: Error | undefined;
  controller: WritableStreamDefaultController | undefined;
}

interface InboundState {
  readonly index: number;
  peerEnding: 'open' | 'closed' | 'reset';
  cancelled: boolean;
  readonly controller: ReadableStreamDefaultController<Uint8Array>;
}

function copyChunk(chunk: Uint8Array): Uint8Array {
  const out = new Uint8Array(chunk.byteLength);
  out.set(chunk);
  return out;
}

/** Uni-stream state for one `MockWebTransport`. */
export class MockUniStreams {
  readonly #outbound: OutboundState[] = [];
  readonly #inbound: InboundState[] = [];
  readonly #incoming: ReadableStream<ReadableStream<Uint8Array>>;
  readonly #incomingController: ReadableStreamDefaultController<ReadableStream<Uint8Array>>;
  #ended = false;

  constructor() {
    let controller!: ReadableStreamDefaultController<ReadableStream<Uint8Array>>;
    this.#incoming = new ReadableStream<ReadableStream<Uint8Array>>({
      start(c) {
        controller = c;
      },
    });
    this.#incomingController = controller;
  }

  /** `IWebTransport.incomingUnidirectionalStreams`. Buffers streams opened before the SDK reads. */
  get incoming(): ReadableStream<ReadableStream<Uint8Array>> {
    return this.#incoming;
  }

  /** `IWebTransport.createUnidirectionalStream`. Rejects once the transport has ended. */
  async create(): Promise<WritableStream<Uint8Array>> {
    if (this.#ended) {
      throw new Error('MockWebTransport: cannot open uni stream on closed transport');
    }
    const state: OutboundState = {
      index: this.#outbound.length,
      chunks: [],
      ending: 'open',
      stopSendingCode: undefined,
      gate: undefined,
      interrupt: newGate(),
      interruptError: undefined,
      controller: undefined,
    };
    this.#outbound.push(state);
    return new WritableStream<Uint8Array>({
      start(controller) {
        state.controller = controller;
      },
      async write(chunk, controller) {
        if (state.interruptError) throw state.interruptError;
        if (state.gate) {
          // A gated write waits for release, a peer stop, a session end, or the
          // SDK's own abort — whichever comes first. `controller.signal` fires on
          // `abort()` immediately, as a real reset does, rather than after the
          // stalled write.
          const aborted = new Promise<void>((resolve) => {
            controller.signal.addEventListener('abort', () => resolve(), { once: true });
          });
          await Promise.race([state.gate.promise, state.interrupt.promise, aborted]);
          if (controller.signal.aborted) throw controller.signal.reason;
          if (state.interruptError) throw state.interruptError;
        }
        state.chunks.push(copyChunk(chunk));
      },
      close() {
        state.ending = 'closed';
      },
      abort() {
        state.ending = 'aborted';
      },
    });
  }

  /**
   * Make a test-driven inbound stream. Delivered on `incoming` at once and
   * buffered there until the SDK reads it.
   */
  simulateIncoming(): InboundUniStreamHandle {
    if (this.#ended) {
      throw new Error('MockWebTransport: cannot deliver a uni stream on a closed transport');
    }
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const index = this.#inbound.length;
    const readable = new ReadableStream<Uint8Array>({
      start(c) {
        controller = c;
      },
      cancel: () => {
        state.cancelled = true;
      },
    });
    const state: InboundState = { index, peerEnding: 'open', cancelled: false, controller };
    this.#inbound.push(state);
    this.#incomingController.enqueue(readable);
    return {
      index,
      enqueue: (bytes) => {
        if (state.peerEnding !== 'open') {
          throw new Error(`MockWebTransport: inbound uni stream ${index} already ended`);
        }
        controller.enqueue(copyChunk(bytes));
      },
      close: () => {
        if (state.peerEnding !== 'open') return;
        state.peerEnding = 'closed';
        controller.close();
      },
      reset: (code) => {
        if (state.peerEnding !== 'open') return;
        state.peerEnding = 'reset';
        controller.error(streamError(code));
      },
    };
  }

  /** Peer STOP_SENDING on outbound stream `index`: the SDK's next (or gated) write rejects. */
  stopSending(index: number, code: number | null): void {
    const state = this.#requireOutbound(index);
    if (state.stopSendingCode !== undefined) return;
    state.stopSendingCode = code;
    const err = streamError(code);
    state.interruptError = err;
    state.interrupt.release();
    try {
      state.controller?.error(err);
    } catch {
      // Already errored or closed — the recorded code still stands.
    }
  }

  /** Gate writes on outbound stream `index` until `resume(index)`. Idempotent. */
  pause(index: number): void {
    const state = this.#requireOutbound(index);
    if (state.gate) return;
    state.gate = newGate();
  }

  /** Release a `pause(index)` gate. Idempotent. */
  resume(index: number): void {
    const state = this.#requireOutbound(index);
    const gate = state.gate;
    state.gate = undefined;
    gate?.release();
  }

  /**
   * The session ended (`close`/`simulateClose`/`simulateError`). Every stream
   * still open errors, as a real session's do, and `incoming` ends: cleanly on
   * a close, errored on an error.
   */
  end(err?: Error): void {
    if (this.#ended) return;
    this.#ended = true;
    const sessionErr = err ?? sessionClosedError();
    for (const state of this.#outbound) {
      if (state.ending !== 'open' || state.interruptError) continue;
      state.interruptError = sessionErr;
      state.interrupt.release();
      try {
        state.controller?.error(sessionErr);
      } catch {
        // Already ended.
      }
    }
    for (const state of this.#inbound) {
      if (state.peerEnding !== 'open' || state.cancelled) continue;
      try {
        state.controller.error(sessionErr);
      } catch {
        // Already ended.
      }
    }
    try {
      if (err) this.#incomingController.error(err);
      else this.#incomingController.close();
    } catch {
      // Already ended.
    }
  }

  outboundInfo(): readonly OutboundUniStreamInfo[] {
    return this.#outbound.map((s) => ({
      index: s.index,
      chunks: s.chunks,
      ending: s.ending,
      stopSendingCode: s.stopSendingCode,
      paused: s.gate !== undefined,
    }));
  }

  inboundInfo(): readonly InboundUniStreamInfo[] {
    return this.#inbound.map((s) => ({
      index: s.index,
      peerEnding: s.peerEnding,
      cancelled: s.cancelled,
    }));
  }

  /** Forget captured chunks; stream states are kept. */
  clearCaptures(): void {
    for (const s of this.#outbound) s.chunks.length = 0;
  }

  #requireOutbound(index: number): OutboundState {
    const state = this.#outbound[index];
    if (state === undefined) {
      throw new Error(`MockWebTransport: no outbound uni stream at index ${index}`);
    }
    return state;
  }
}
