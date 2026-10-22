// File: packages/sdk-core/src/media/lifecycle/recoveryRequestQueue.ts
//
// The receiver's recovery-request queue (story 3 R-13, amendment 4), placed IN
// FRONT of the per-handler control send (`MediaTransport.sendRecoveryRequest`).
//
//   * AT MOST ONE queued, unsent request per (handler, slot). A newer request
//     for the same slot REPLACES the queued one, so an obsolete request never
//     waits behind a newer one.
//   * AT MOST ONE send in flight per handler. The next request is handed to the
//     transport only after the previous write settled, so nothing BELOW this
//     queue (the transport's serialised send chain) ever holds an obsolete
//     request either.
//   * NO TIMER AND NO RETRY. Requests ride a reliable stream, so repeating one
//     achieves nothing; a failed write is surfaced (event-once fault) and not
//     re-sent; a later loss is a new event with a newer counter.
//   * Each request carries ONLY `slot_id`, `sender_id` and `gop_counter`
//     (security R4). The queue holds no transport, stream or credential.
//
// COUNTING: `dt_client_media_recovery_requests_sent_total{source}` on a
// SUCCESSFUL write only. A replaced request and a failed write are not counted.

import type { MhRecoveryRequest } from '../events.js';
import type { MediaRecoverySource } from '../setup/mediaMetrics.js';

/** What the queue needs from its owner. */
export interface RecoveryRequestQueueOptions {
  /** The per-handler control send. Rejects on any failure. */
  readonly send: (mediaHandlerUrl: string, request: MhRecoveryRequest) => Promise<void>;
  /** A request was written. Counted by cause (`source`). */
  readonly sent: (reason: MediaRecoverySource) => void;
  /** A write failed. Surfaced event-once by the owner; never re-sent. */
  readonly failed: () => void;
}

interface Pending {
  readonly request: MhRecoveryRequest;
  readonly reason: MediaRecoverySource;
}

interface HandlerQueue {
  /** slot id -> the newest unsent request for it. Insertion order = send order. */
  readonly pending: Map<number, Pending>;
  inFlight: boolean;
}

/** See the module header. */
export class RecoveryRequestQueue {
  readonly #options: RecoveryRequestQueueOptions;
  readonly #handlers = new Map<string, HandlerQueue>();
  #stopped = false;

  constructor(options: RecoveryRequestQueueOptions) {
    this.#options = options;
  }

  /** Queue one request for `slotId` on `mediaHandlerUrl`, replacing any unsent one. */
  request(mediaHandlerUrl: string, request: MhRecoveryRequest, reason: MediaRecoverySource): void {
    if (this.#stopped) return;
    let queue = this.#handlers.get(mediaHandlerUrl);
    if (!queue) {
      queue = { pending: new Map(), inFlight: false };
      this.#handlers.set(mediaHandlerUrl, queue);
    }
    // Replace: delete first so the slot moves to the back of the send order.
    queue.pending.delete(request.slotId);
    queue.pending.set(request.slotId, { request, reason });
    this.#pump(mediaHandlerUrl, queue);
  }

  /** Requests queued and not yet handed to the transport. For assertion. */
  pendingFor(mediaHandlerUrl: string): number {
    return this.#handlers.get(mediaHandlerUrl)?.pending.size ?? 0;
  }

  /** Drop everything queued; nothing more is sent. Idempotent. */
  stop(): void {
    this.#stopped = true;
    for (const queue of this.#handlers.values()) queue.pending.clear();
  }

  #pump(url: string, queue: HandlerQueue): void {
    if (queue.inFlight || this.#stopped) return;
    const next = queue.pending.entries().next();
    if (next.done) return;
    const [slotId, pending] = next.value;
    queue.pending.delete(slotId);
    queue.inFlight = true;
    this.#options.send(url, pending.request).then(
      () => {
        queue.inFlight = false;
        if (!this.#stopped) this.#options.sent(pending.reason);
        this.#pump(url, queue);
      },
      () => {
        queue.inFlight = false;
        if (!this.#stopped) this.#options.failed();
        this.#pump(url, queue);
      },
    );
  }
}
