// File: packages/sdk-core/src/media/lifecycle/receiveTransports.ts
//
// The inbound read loops: ONE loop per media-handler transport (and, for audio
// datagrams, ONE hop-sequence monitor per transport). A sibling of the hot path
// (see `AudioPipeline.ts`), split out of the pipeline because it is a separable
// responsibility — which transports are being read — rather than orchestration.
//
// ---------------------------------------------------------------------------
// ONE LOOP PER TRANSPORT, NEVER TORN DOWN ON AN ASSIGNMENT CHANGE
// ---------------------------------------------------------------------------
//
// MC re-emits `StreamAssignments` on every structural change in the meeting
// (every join, leave, declaration, mute and connectivity change), so `open` runs
// many times per session. A `ReadableStream` can be locked by only ONE reader,
// so a second `getReader()` throws — and it would throw from inside an
// assignment handler, where the failure would present as "media stopped after
// someone joined" rather than as what it is. A re-emit naming handlers already
// looping is therefore a no-op: nothing on this path touches the transport or
// any receiver state, so the replay window survives every unrelated roster
// event.
//
// A loop is NEVER torn down when an edge leaves its handler. The handler may
// carry another edge later, and the frames still arriving on it belong to
// senders whose edges have not moved. Tearing down on an assignment change is
// how "someone joined and I went deaf" happens.
//
// Loops are keyed by `handler_ref` (ADR-0039 §4). Exactly two things end one
// besides session teardown: the handler LEAVING THE SET (`stop(ref)`, quietly —
// a directed removal is not a fault), and its transport ending (the loop frees
// its entry, so the ref's next transport after a redial is read on the next
// `open`).
//
// ---------------------------------------------------------------------------
// A HOP MONITOR PER TRANSPORT, NEVER SHARED
// ---------------------------------------------------------------------------
//
// Each media handler writes its own `hop_sequence` numbering (per (connection,
// media stream) in the frame format), so one monitor across two transports
// would read two independent sequences as one and post false gaps. Bounded by
// the current handler set (`stop(ref)` clears a removed ref's monitor). `IngressPipeline.accept` takes the
// monitor as a parameter for exactly this reason.

import { HopSequenceMonitor } from '../pipeline/hopSequenceMonitor.js';
import type { HandlerRef } from '../handlerSet.js';

/**
 * The bounded, literal texts one kind of loop reports with. Static strings
 * only: they are part of the owner's event-once fault dedupe key.
 */
export interface ReadLoopTexts {
  /** Registration name prefix for a loop's disposer (a static literal). */
  readonly disposerName: string;
  /** The readable ended while the session was still active. */
  readonly ended: string;
  /** The reader failed. */
  readonly failed: string;
  /** One item could not be processed for a non-wire reason: a defect. */
  readonly itemDefect: string;
}

/** What one kind's per-handler read loops need from their owner. */
export interface PerHandlerReadLoopsOptions<T, C> {
  /**
   * The readable for an ALREADY-CONNECTED handler, or `undefined`. A selector
   * over connected transports, never a dialer.
   */
  readonly readableFor: (handlerRef: HandlerRef) => ReadableStream<T> | undefined;
  /** Per-handler context, created once when that handler's loop starts. */
  readonly contextFor: (handlerRef: HandlerRef) => C;
  /**
   * Process one item. Returns normally for every WIRE condition; a throw is a
   * defect, surfaced once through `frameFault`, and the loop continues. The loop
   * AWAITS it, so an item that must not block the loop returns at once.
   */
  readonly onItem: (item: T, context: C) => Promise<void> | void;
  readonly isStopped: () => boolean;
  /** Register a disposer with the owner's teardown. */
  readonly register: (name: string, dispose: () => void) => void;
  /** A transport-level condition. Non-fatal; deduplicated by the owner. */
  readonly transportFault: (message: string) => void;
  /** An item that could not be processed for a non-wire reason: a defect. */
  readonly frameFault: (message: string) => void;
  readonly texts: ReadLoopTexts;
}

/**
 * ONE read loop per media-handler transport, each started at most once and
 * never torn down on an assignment change — the rules in this module's header,
 * written once for every kind (story 3 C-B, DRY P4): datagrams for audio, the
 * incoming group-stream acceptor for video.
 */
export class PerHandlerReadLoops<T, C> {
  readonly #options: PerHandlerReadLoopsOptions<T, C>;
  /**
   * The handlers a read loop is running on. One entry per transport; an entry
   * is deleted when its loop ends, so a ref that reconnects (a redial after a
   * loss) gets a fresh loop on its new transport at the next `open`.
   */
  readonly #loops = new Map<HandlerRef, Loop<T, C>>();
  /** Monotonic loop id for disposer names (never a ref or url). */
  #started = 0;

  constructor(options: PerHandlerReadLoopsOptions<T, C>) {
    this.#options = options;
  }

  /**
   * Start a read loop on every handler in `refs` not already looping. Loops
   * already running are left exactly as they are, including when an edge moves
   * off their handler.
   *
   * A ref with no CONNECTED transport yet is skipped silently: every ref here
   * is a member of the current set (non-members are dropped and surfaced at
   * decode, ADR-0039 §4), so "not connected" is a dial or redial in flight. The
   * owner calls `open` again when that ref connects.
   */
  open(refs: readonly HandlerRef[]): void {
    if (this.#options.isStopped()) return;
    for (const ref of refs) {
      if (this.#loops.has(ref)) continue;
      const readable = this.#options.readableFor(ref);
      if (!readable) continue;
      if (readable.locked) {
        // Two DISTINCT refs resolved to one readable. A `ReadableStream` takes
        // only one reader, so `getReader()` would THROW — from inside an
        // assignment handler. Checked rather than caught so the condition is
        // named, and so the other handlers in this snapshot still get loops.
        this.#options.transportFault('two media handlers resolved to the same media transport');
        continue;
      }
      const loop: Loop<T, C> = {
        context: this.#options.contextFor(ref),
        reader: readable.getReader(),
        quiet: false,
      };
      this.#loops.set(ref, loop);
      this.#start(ref, loop);
    }
  }

  /**
   * End `ref`'s loop QUIETLY: its handler left the set (ADR-0039 §4 removal
   * cleanup). No `ended`/`failed` transportFault — a directed removal is not a
   * fault. Returns the loop's context (so the owner can clear its per-transport
   * state, e.g. the hop monitor), or `undefined` if none was running.
   */
  stop(ref: HandlerRef): C | undefined {
    const loop = this.#loops.get(ref);
    if (!loop) return undefined;
    this.#loops.delete(ref);
    loop.quiet = true;
    void loop.reader.cancel().catch(() => {
      // Already cancelled or errored; the transport close is what matters.
    });
    return loop.context;
  }

  /** Every running loop's context. For the owner's teardown. */
  contexts(): IterableIterator<C> {
    return [...this.#loops.values()].map((l) => l.context)[Symbol.iterator]();
  }

  #start(ref: HandlerRef, loop: Loop<T, C>): void {
    const { reader } = loop;
    // Registered per loop. The name is a static literal plus a LOOP COUNTER,
    // never a ref or url: a teardown-failure fault interpolates this name.
    this.#started += 1;
    this.#options.register(`${this.#options.texts.disposerName}-${this.#started}`, () => {
      void reader.cancel().catch(() => {
        // Already cancelled or errored; the transport close is what matters.
      });
    });
    void this.#loop(ref, loop);
  }

  async #loop(ref: HandlerRef, loop: Loop<T, C>): Promise<void> {
    const { texts } = this.#options;
    const { reader, context } = loop;
    const silent = (): boolean => loop.quiet || this.#options.isStopped();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) {
          if (!silent()) {
            // Absence of media is not a signal (ADR-0036 §6): a readable that
            // ends while the session continues is a real condition and is
            // surfaced rather than read as silence.
            this.#options.transportFault(texts.ended);
          }
          return;
        }
        if (value === undefined) continue;
        if (loop.quiet) return;
        try {
          await this.#options.onItem(value, context);
        } catch {
          // `onItem` returns normally for every WIRE condition; reaching here
          // means a defect. Surfaced ONCE — a repeating defect must not become
          // per-frame telemetry — and the loop continues, because one bad item
          // must not end media for the session.
          this.#options.frameFault(texts.itemDefect);
        }
      }
    } catch {
      if (silent()) return;
      this.#options.transportFault(texts.failed);
    } finally {
      // A loop that ended frees its slot, so the ref's NEXT transport (after a
      // redial) can be read. Only if the entry is still THIS loop.
      if (this.#loops.get(ref) === loop) this.#loops.delete(ref);
      try {
        reader.releaseLock();
      } catch {
        // Already released during teardown.
      }
    }
  }
}

/** One running loop. */
interface Loop<T, C> {
  readonly context: C;
  readonly reader: ReadableStreamDefaultReader<T>;
  /** Set by `stop`: the loop ends without a fault. */
  quiet: boolean;
}

/** What the DATAGRAM read loops need from their owner. */
export interface ReceiveTransportsOptions {
  /**
   * Inbound datagrams for an ALREADY-CONNECTED handler, or `undefined`. A
   * selector over connected transports, never a dialer.
   */
  readonly readableFor: (handlerRef: HandlerRef) => ReadableStream<Uint8Array> | undefined;
  readonly declaredSlotIds: readonly number[];
  readonly hopRestartBackwardJumpFrames: number;
  /** Process one datagram. Returns normally for every WIRE condition. */
  readonly accept: (datagram: Uint8Array, hopMonitor: HopSequenceMonitor) => Promise<void>;
  readonly isStopped: () => boolean;
  /** Register a disposer with the owner's teardown. */
  readonly register: (name: string, dispose: () => void) => void;
  /** A transport-level condition. Non-fatal; deduplicated by the owner. */
  readonly transportFault: (message: string) => void;
  /** A frame that could not be processed for a non-wire reason: a defect. */
  readonly frameFault: (message: string) => void;
}

/** The DATAGRAM (audio) read loops and their per-transport hop monitors. */
export class ReceiveTransports {
  readonly #loops: PerHandlerReadLoops<Uint8Array, HopSequenceMonitor>;

  constructor(options: ReceiveTransportsOptions) {
    this.#loops = new PerHandlerReadLoops({
      readableFor: options.readableFor,
      // A hop monitor PER TRANSPORT, never shared (module header).
      contextFor: () =>
        new HopSequenceMonitor(options.declaredSlotIds, options.hopRestartBackwardJumpFrames),
      // The monitor for THIS transport travels with the datagram: each handler
      // writes its own hop numbering (see `IngressPipeline.accept`).
      onItem: (datagram, monitor) => options.accept(datagram, monitor),
      isStopped: options.isStopped,
      register: options.register,
      transportFault: options.transportFault,
      frameFault: options.frameFault,
      texts: {
        disposerName: 'datagram-reader',
        ended: 'the inbound media datagram stream ended while the session was still active',
        failed: 'the media datagram reader failed',
        itemDefect: 'a media frame could not be processed',
      },
    });
  }

  /** Start a read loop on every handler in `refs` not already looping. */
  open(refs: readonly HandlerRef[]): void {
    this.#loops.open(refs);
  }

  /**
   * The handler `ref` left the set: end its read loop QUIETLY and clear its
   * per-transport hop monitor (ADR-0039 §4 removal cleanup).
   */
  stop(ref: HandlerRef): void {
    this.#loops.stop(ref)?.clear();
  }

  /** Drop every hop monitor's state. Called from the owner's teardown. */
  clear(): void {
    for (const monitor of this.#loops.contexts()) monitor.clear();
  }
}
