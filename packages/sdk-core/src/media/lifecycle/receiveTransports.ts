// File: packages/sdk-core/src/media/lifecycle/receiveTransports.ts
//
// The inbound read loops: ONE loop and ONE hop-sequence monitor per media-handler
// transport. A sibling of the hot path (see `AudioPipeline.ts`), split out of the
// pipeline because it is a separable responsibility — which transports are being
// read — rather than orchestration.
//
// ---------------------------------------------------------------------------
// EACH LOOP STARTS AT MOST ONCE, AND IS NEVER TORN DOWN ON AN ASSIGNMENT CHANGE
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
// ---------------------------------------------------------------------------
// A HOP MONITOR PER TRANSPORT, NEVER SHARED
// ---------------------------------------------------------------------------
//
// Each media handler writes its own `hop_sequence` numbering (per (connection,
// media stream) in the frame format), so one monitor across two transports
// would read two independent sequences as one and post false gaps. Bounded by
// the meeting's registered handler set. `IngressPipeline.accept` takes the
// monitor as a parameter for exactly this reason.

import { HopSequenceMonitor } from '../pipeline/hopSequenceMonitor.js';

/** What the read loops need from their owner. */
export interface ReceiveTransportsOptions {
  /**
   * Inbound datagrams for an ALREADY-CONNECTED handler, or `undefined`. A
   * selector over transports `connectAll` opened, never a dialer.
   */
  readonly readableFor: (mediaHandlerUrl: string) => ReadableStream<Uint8Array> | undefined;
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

/** The read loops and their per-transport hop monitors. */
export class ReceiveTransports {
  readonly #options: ReceiveTransportsOptions;
  /** The handlers a read loop is running on. One entry per transport. */
  readonly #readLoops = new Set<string>();
  readonly #hopMonitors = new Map<string, HopSequenceMonitor>();

  constructor(options: ReceiveTransportsOptions) {
    this.#options = options;
  }

  /**
   * Start a read loop on every handler in `urls` not already looping.
   *
   * Loops already running are left exactly as they are, including when an edge
   * moves off their handler.
   */
  open(urls: readonly string[]): void {
    if (this.#options.isStopped()) return;
    for (const url of urls) {
      if (this.#readLoops.has(url)) continue;
      const readable = this.#options.readableFor(url);
      if (!readable) {
        // A slot assigned on a transport we never opened. Structurally a server
        // condition — MC's connectivity view names a transport we do not hold —
        // and the last thing this client can do about it is say so loudly.
        this.#options.transportFault(
          'slot assignments name a media handler this client is not connected to',
        );
        continue;
      }
      if (readable.locked) {
        // Two DISTINCT urls resolved to one datagram stream. A `ReadableStream`
        // takes only one reader, so `getReader()` would THROW — from inside an
        // assignment handler, where it would surface as "media stopped when
        // someone joined" rather than as what it is. Checked rather than caught
        // so the condition is named, and so the other handlers in this snapshot
        // still get their loops.
        this.#options.transportFault('two media handlers resolved to the same media transport');
        continue;
      }
      this.#readLoops.add(url);
      const monitor = new HopSequenceMonitor(
        this.#options.declaredSlotIds,
        this.#options.hopRestartBackwardJumpFrames,
      );
      this.#hopMonitors.set(url, monitor);
      this.#start(readable, monitor);
    }
  }

  /** Drop every hop monitor's state. Called from the owner's teardown. */
  clear(): void {
    for (const monitor of this.#hopMonitors.values()) monitor.clear();
    this.#hopMonitors.clear();
  }

  #start(readable: ReadableStream<Uint8Array>, monitor: HopSequenceMonitor): void {
    const reader = readable.getReader();
    // Registered per loop. The name is a static literal plus the LOOP COUNT, not
    // the URL: a teardown-failure fault interpolates this name, and a handler URL
    // in a fault message would be an unbounded value on a bounded path.
    this.#options.register(`datagram-reader-${this.#readLoops.size}`, () => {
      void reader.cancel().catch(() => {
        // Already cancelled or errored; the transport close is what matters.
      });
    });
    void this.#loop(reader, monitor);
  }

  async #loop(
    reader: ReadableStreamDefaultReader<Uint8Array>,
    monitor: HopSequenceMonitor,
  ): Promise<void> {
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) {
          if (!this.#options.isStopped()) {
            // Absence of frames is not a signal (ADR-0036 §6): a datagram stream
            // that ends while the session continues is a real condition and is
            // surfaced rather than read as silence.
            this.#options.transportFault(
              'the inbound media datagram stream ended while the session was still active',
            );
          }
          return;
        }
        if (value === undefined) continue;
        try {
          // The monitor for THIS transport travels with the datagram: each
          // handler writes its own hop numbering (see `IngressPipeline.accept`).
          await this.#options.accept(value, monitor);
        } catch {
          // `accept` returns normally for every WIRE condition; reaching here
          // means a defect. Surfaced ONCE — a repeating defect must not become
          // per-frame telemetry — and the loop continues, because one bad frame
          // must not end media for the session.
          this.#options.frameFault('a media frame could not be processed');
        }
      }
    } catch {
      if (this.#options.isStopped()) return;
      this.#options.transportFault('the media datagram reader failed');
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Already released during teardown.
      }
    }
  }
}
