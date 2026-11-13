// File: packages/sdk-core/src/media/lifecycle/transportRecency.ts
//
// The receiver's MEMBERSHIP BOUND over arrival transports (story 4 C2), for
// the replay window's duplicate classification (`frame/receivePath.ts`
// `ReplayArrival`): a duplicate whose first copy arrived via a transport that
// is a current set member — or left the set within the bounded reconnect
// window (`media.handlers.reconnectAbandonMs`) — is a make-before-break
// overlap (`duplicate_cross_transport`); one whose first copy came via a
// long-gone transport stays `replay_detected`.
//
// Ids are handler refs as plain numbers: process-local, never emitted, never a
// label. Refs are never reused within a meeting (ADR-0039 §4), so a removed id
// can only age out. Bounded: removal timestamps are pruned once older than the
// window, and the per-ref arrival objects are dropped at removal.

import type { HandlerRef } from '../handlerSet.js';
import type { ReplayArrival } from '../frame/receivePath.js';

export class TransportRecency {
  readonly #windowMs: number;
  readonly #clock: () => number;
  /**
   * POSITIVE membership: refs a frame has arrived via and that have not left
   * the set. Never re-entered after `forget` (a straggler frame from a removed
   * ref does not make it live again).
   */
  readonly #live = new Set<number>();
  /**
   * Every ref that ever LEFT the set — never pruned, so a straggler frame from
   * a removed ref (even long after) can never re-mark it live. Bounded by the
   * handler incarnations of one meeting (one number per removal).
   */
  readonly #retired = new Set<number>();
  /** ref -> when it left the set. Pruned past the window; pruning only ever turns an id NOT recent. */
  readonly #removedAt = new Map<number, number>();
  /** One arrival object per ref (no per-frame allocation). A cache, not membership. */
  readonly #arrivals = new Map<number, ReplayArrival>();
  /**
   * FAILS CLOSED: recent iff a live member, or removed within the window. An
   * unknown id — never seen, or removed and since pruned — is NOT recent, so a
   * long-gone transport can never launder a replay into the overlap token.
   */
  readonly #isRecent = (transport: number): boolean => {
    if (this.#live.has(transport)) return true;
    const at = this.#removedAt.get(transport);
    return at !== undefined && this.#clock() - at <= this.#windowMs;
  };

  constructor(windowMs: number, clock: () => number) {
    this.#windowMs = windowMs;
    this.#clock = clock;
  }

  /** The arrival of a frame received via `ref`. Marks `ref` live unless it already left the set. */
  arrival(ref: HandlerRef): ReplayArrival {
    if (!this.#retired.has(ref)) this.#live.add(ref);
    let arrival = this.#arrivals.get(ref);
    if (!arrival) {
      arrival = { transport: ref, isRecent: this.#isRecent };
      this.#arrivals.set(ref, arrival);
    }
    return arrival;
  }

  /** `ref` left the set (removal cleanup). */
  forget(ref: HandlerRef): void {
    this.#live.delete(ref);
    this.#retired.add(ref);
    this.#arrivals.delete(ref);
    const now = this.#clock();
    this.#removedAt.set(ref, now);
    for (const [id, at] of this.#removedAt) {
      if (now - at > this.#windowMs) this.#removedAt.delete(id);
    }
  }

  /** Drop everything (teardown). */
  clear(): void {
    this.#live.clear();
    this.#retired.clear();
    this.#arrivals.clear();
    this.#removedAt.clear();
  }
}
