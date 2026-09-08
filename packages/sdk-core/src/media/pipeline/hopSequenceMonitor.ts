// File: packages/sdk-core/src/media/pipeline/hopSequenceMonitor.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction.
//
// Downlink hop-sequence gap detection — the far-end compensating control for a
// blind spot the media handler structurally cannot close.
//
// ---------------------------------------------------------------------------
// WHY ONLY THE RECEIVER CAN SEE THIS
// ---------------------------------------------------------------------------
//
// MH writes the downlink hop sequence, so it can never see a gap in a number it
// generates itself. And beneath MH's own bounded queue, quinn evicts datagrams
// from its send buffer SILENTLY: it pops the oldest, emits a trace-level log,
// increments no `ConnectionStats` field, and — because an evicted datagram was
// never transmitted — it never enters quinn's lost-packet statistics either. So
// under congestion severe enough to saturate quinn's buffer but not MH's,
// MH's egress-overflow counter READS FLAT AT EXACTLY THE MOMENT LOSS IS WORST:
// "no loss" and "loss we cannot see" are indistinguishable on that signal.
//
// This monitor is the honest compensating control named in `docs/TODO.md`, and
// it lives here because the eviction is on MH's DOWNLINK — only the far end can
// observe it.
//
// ---------------------------------------------------------------------------
// GAPS ARE COUNTED IN FRAMES, AND REORDERING IS COUNTED SEPARATELY
// ---------------------------------------------------------------------------
//
// The gap counter increments by the SIZE of each gap, not by one per gap event:
// a counter of missing NUMBERS is comparable against `frames_received_total` as
// a loss rate, while a counter of events is not.
//
// QUIC datagrams are unordered, so a reordered frame first opens a gap and then
// arrives late. The gap counter therefore OVER-COUNTS true loss by exactly the
// reorder count, and the honest estimate is `gap_frames - reorder`. Keeping the
// two separate is what makes that subtraction possible; folding them would make
// normal reordering read as loss in the first congestion incident.
//
// ---------------------------------------------------------------------------
// THE RELAY REGION IS UNAUTHENTICATED, SO `stream_id` BOUNDS STATE CREATION
// ---------------------------------------------------------------------------
//
// `stream_id` and `hop_sequence` live in the relay region: a media handler
// writes both freely and NOBODY SIGNS THEM. An unbounded 16-bit field used as a
// map key is 65 536 attacker-chosen state entries per connection, created by the
// relay at datagram rate.
//
// So `observe()` takes the DECLARED SLOT SET and creates no state for a
// `stream_id` outside it. The frame itself is unaffected — it is still verified
// and attributed from its own key id — and `hop_sequence` never gates a
// drop-or-play decision anywhere. A relay-supplied counter must not be able to
// suppress a frame.

/** What `observe()` concluded about one frame's hop sequence. */
export interface HopObservation {
  /** Frames missing between the previous highest and this one. Never negative. */
  readonly missing: number;
  /** True when this frame arrived at or below the running high-water mark. */
  readonly reordered: boolean;
  /** True when `stream_id` was not one this client declared; no state was created. */
  readonly undeclaredStreamId: boolean;
}

const NONE: HopObservation = { missing: 0, reordered: false, undeclaredStreamId: false };
const UNDECLARED: HopObservation = { missing: 0, reordered: false, undeclaredStreamId: true };

/**
 * Per-declared-slot hop-sequence high-water marks.
 *
 * Bounded BY CONSTRUCTION rather than by an eviction policy: the map is keyed by
 * the slot ids this client itself declared, which it chose and which are
 * therefore not attacker-influenced. That is a stronger bound than an LRU and is
 * why there is no eviction here.
 */
export class HopSequenceMonitor {
  readonly #declaredStreamIds: ReadonlySet<number>;
  readonly #highest = new Map<number, number>();

  constructor(declaredStreamIds: Iterable<number>) {
    this.#declaredStreamIds = new Set(declaredStreamIds);
  }

  /**
   * Record one frame's relay-region hop sequence.
   *
   * Called AFTER the wire-level receive count and BEFORE nothing in particular:
   * its result never influences whether the frame is processed.
   */
  observe(streamId: number, hopSequence: number): HopObservation {
    if (!this.#declaredStreamIds.has(streamId)) {
      // No map entry, no allocation, no growth. The bound is on STATE CREATION,
      // not on rendering.
      return UNDECLARED;
    }
    const highest = this.#highest.get(streamId);
    if (highest === undefined) {
      // First frame for this slot. There is no previous number, so there is no
      // gap — a first observation cannot be evidence of loss.
      this.#highest.set(streamId, hopSequence);
      return NONE;
    }
    if (hopSequence <= highest) {
      return { missing: 0, reordered: true, undeclaredStreamId: false };
    }
    this.#highest.set(streamId, hopSequence);
    return { missing: hopSequence - highest - 1, reordered: false, undeclaredStreamId: false };
  }

  /** Drop all marks. No key material; a plain state reset for teardown symmetry. */
  clear(): void {
    this.#highest.clear();
  }
}
