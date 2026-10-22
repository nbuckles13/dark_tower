// File: packages/sdk-core/src/media/lifecycle/unfulfilledSlots.ts
//
// UNFULFILLED video slots (story 3 R-30, amendment 4): a slot MC reports ACTIVE
// that shows neither decodable video nor an avatar. PURE bookkeeping on an
// injected clock; the slot pipelines drive it.
//
//   * ENTER: the slot turns ACTIVE with no eligible decoded frame from its
//     current source to draw, or loses decodability (a loss event) while ACTIVE.
//     Idempotent: a second loss while already unfulfilled is the same entry.
//   * EXIT, every path OBSERVED once on the histogram: the next drawn frame, MC
//     moving the slot out of ACTIVE, the slot leaving the snapshot, and teardown
//     (leave, disconnect, SDK disposal all reach {@link stop}).
//   * The gauge is set to the ABSOLUTE current count on every transition,
//     re-set on the export tick ({@link reassert}; the SDK exports DELTA and a
//     last-value gauge is exported only in an interval it was recorded in), and
//     set to 0 at {@link stop} — so neither series leaks.
//
// NO IDENTITY AND NO SLOT IDS in either series. The per-slot detail
// ({@link snapshot}) is LOCAL ONLY — the R-35 overlay's input — and never
// exported or put on the test bus (security ruling: no durations on the bus).

import type { MediaKindMetrics } from '../setup/mediaMetrics.js';

/** One slot's local unfulfilled state. Never exported. */
export interface UnfulfilledSlotState {
  readonly slotId: number;
  readonly unfulfilled: boolean;
  /** Time in the unfulfilled state so far; 0 when fulfilled. */
  readonly sinceMs: number;
}

/** See the module header. */
export class UnfulfilledSlots {
  readonly #metrics: MediaKindMetrics;
  readonly #clock: () => number;
  /** slot id -> when it entered the unfulfilled state. */
  readonly #entered = new Map<number, number>();
  #stopped = false;

  constructor(metrics: MediaKindMetrics, clock: () => number) {
    this.#metrics = metrics;
    this.#clock = clock;
  }

  enter(slotId: number): void {
    if (this.#stopped || this.#entered.has(slotId)) return;
    this.#entered.set(slotId, this.#clock());
    this.#metrics.slotsUnfulfilled(this.#entered.size);
  }

  exit(slotId: number): void {
    const enteredAt = this.#entered.get(slotId);
    if (enteredAt === undefined) return;
    this.#entered.delete(slotId);
    this.#metrics.slotUnfulfilledDurationMs(this.#clock() - enteredAt);
    this.#metrics.slotsUnfulfilled(this.#entered.size);
  }

  isUnfulfilled(slotId: number): boolean {
    return this.#entered.has(slotId);
  }

  /** Re-set the gauge to the current count (the export-tick cadence). */
  reassert(): void {
    if (!this.#stopped) this.#metrics.slotsUnfulfilled(this.#entered.size);
  }

  /** Per-slot local state for `slotIds`. Allocates; sample it, never per frame. */
  snapshot(slotIds: readonly number[]): readonly UnfulfilledSlotState[] {
    const now = this.#clock();
    return slotIds.map((slotId) => {
      const enteredAt = this.#entered.get(slotId);
      return {
        slotId,
        unfulfilled: enteredAt !== undefined,
        sinceMs: enteredAt === undefined ? 0 : now - enteredAt,
      };
    });
  }

  /** Teardown: every open entry exits (observed), and the gauge reads 0. Idempotent. */
  stop(): void {
    if (this.#stopped) return;
    for (const slotId of [...this.#entered.keys()]) this.exit(slotId);
    this.#stopped = true;
    this.#metrics.slotsUnfulfilled(0);
  }
}
