// File: packages/test-utils/src/FakeScheduler.ts
//
// A hand-driven timer double for `setTimeout`/`clearTimeout` seams (the
// `TransportScheduler` / `MeetingKekHolderOptions` shape). Nothing fires on its
// own: a test lists the pending timers, reads their delays, and fires exactly
// the one it means to — so "the scheduled delay was X" is an assertion, not a
// wait, and no wall time passes. Also carries a manual clock that `fire`/
// `advance` move to each timer's due time.

/** One pending timer. */
export interface PendingTimer {
  readonly id: number;
  readonly delayMs: number;
  /** Clock time at which it is due. */
  readonly dueMs: number;
}

interface Entry extends PendingTimer {
  readonly callback: () => void;
}

/** See the module header. */
export class FakeScheduler {
  #now = 0;
  #nextId = 1;
  readonly #pending = new Map<number, Entry>();

  /** The manual clock (ms). */
  readonly clock = (): number => this.#now;

  readonly setTimeout = (callback: () => void, delayMs: number): unknown => {
    const id = this.#nextId++;
    this.#pending.set(id, { id, delayMs, dueMs: this.#now + delayMs, callback });
    return id;
  };

  readonly clearTimeout = (handle: unknown): void => {
    if (typeof handle === 'number') this.#pending.delete(handle);
  };

  /** Pending timers, earliest due first (ties: creation order). */
  pending(): PendingTimer[] {
    return [...this.#pending.values()]
      .sort((a, b) => a.dueMs - b.dueMs || a.id - b.id)
      .map(({ id, delayMs, dueMs }) => ({ id, delayMs, dueMs }));
  }

  /** Number of pending timers. */
  get size(): number {
    return this.#pending.size;
  }

  /** Fire one pending timer by id (moving the clock to its due time). Throws if absent. */
  fire(id: number): void {
    const entry = this.#pending.get(id);
    if (!entry) throw new Error(`FakeScheduler: no pending timer ${id}`);
    this.#pending.delete(id);
    this.#now = Math.max(this.#now, entry.dueMs);
    entry.callback();
  }

  /** Fire the earliest pending timer. Returns it, or `undefined` if none is pending. */
  fireNext(): PendingTimer | undefined {
    const next = this.pending()[0];
    if (next) this.fire(next.id);
    return next;
  }

  /**
   * Advance the clock by `ms`, firing every timer that falls due on the way (in
   * due order, including timers scheduled by the callbacks themselves).
   */
  advance(ms: number): void {
    const target = this.#now + ms;
    for (;;) {
      const next = this.pending()[0];
      if (!next || next.dueMs > target) break;
      this.fire(next.id);
    }
    this.#now = target;
  }
}
