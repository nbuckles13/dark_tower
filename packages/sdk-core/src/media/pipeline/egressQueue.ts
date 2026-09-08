// File: packages/sdk-core/src/media/pipeline/egressQueue.ts
//
// The bounded, drop-oldest egress queue (ADR-0036 §1, §11).
//
// ---------------------------------------------------------------------------
// THE QUEUE HOLDS NO METRIC HANDLE. THE CALLER COUNTS.
// ---------------------------------------------------------------------------
//
// `push` RETURNS the evicted item; deciding what that means and counting it is
// the caller's job. That keeps this a data structure rather than a
// telemetry-coupled one, and it is the shape
// `crates/mh-service/src/media/queue.rs::BoundedDropOldest` already uses on the
// Rust side — deliberately, so the two ends of one hop behave the same way and
// read the same way.
//
// CROSS-LANGUAGE SIBLING, NOT A SHARED MECHANISM. There is no TypeScript
// equivalent in tree and no shared home is reachable, so this is a genuine
// second implementation of a named mechanism. Nothing enforces that they agree.
//
// WHAT CAN ACTUALLY DRIFT IS THE **POLICY**, NOT THE CODE — and the sharper
// statement of that risk lives in `docs/TODO.md` §Cross-Service Duplication.
// Read it before changing the eviction order here: MH pins its drop-direction in
// `crates/mh-service/tests/media_backpressure_integration.rs`, while the
// eviction-order test beside this file asserts DATA-STRUCTURE behaviour rather
// than the ADR-0036 §1 rationale for it. So a reversal at this end would go
// green here and red nowhere, which is the gap that entry exists to name.
//
// ---------------------------------------------------------------------------
// DROP OLDEST, NEVER NEWEST
// ---------------------------------------------------------------------------
//
// A realtime path prefers loss to unbounded latency: the freshest audio is the
// audio worth sending, and the oldest frame in a stalled queue is already past
// its usefulness by the time the stall clears. Note the deliberate NON-collapse
// against `crates/mc-service/src/actors/participant.rs`'s mailbox, which drops
// NEWEST — a control-plane mailbox wants the earliest instruction, a media queue
// wants the latest sample. Same data structure, opposite policy, for reasons
// that do not transfer.
//
// ---------------------------------------------------------------------------
// WHY THIS EXISTS AT ALL
// ---------------------------------------------------------------------------
//
// WebTransport exposes NO send-side drop event, and the user agent's own
// datagram queue discards silently. A drop that happens down there is
// uncountable by us AND structurally invisible to the media handler — §11 calls
// that the drop that matters most. So the SDK keeps the transport queue shallow
// (`outgoingHighWaterMark`), owns this queue above it, makes the drop decision
// HERE, and counts it — making the drop observable by construction. The
// application bound must trip BEFORE the transport ceiling; that ordering is
// asserted at setup by `validateMediaConfig`.

/** A bounded FIFO that evicts its oldest entry when full. */
export class BoundedDropOldestQueue<T> {
  readonly #capacity: number;
  readonly #items: T[] = [];

  constructor(capacity: number) {
    if (!Number.isInteger(capacity) || capacity <= 0) {
      throw new RangeError(`egress queue capacity must be a positive integer, got ${capacity}`);
    }
    this.#capacity = capacity;
  }

  /**
   * Append `item`.
   *
   * @returns the evicted oldest entry when the queue was full, otherwise
   * `undefined`. The CALLER counts the eviction — this class holds no metric
   * handle and no sink reference.
   */
  push(item: T): T | undefined {
    let evicted: T | undefined;
    if (this.#items.length >= this.#capacity) {
      evicted = this.#items.shift();
    }
    this.#items.push(item);
    return evicted;
  }

  /** Remove and return the oldest entry, or `undefined` when empty. */
  shift(): T | undefined {
    return this.#items.shift();
  }

  /** Current depth, in items. For the queue-depth gauge. */
  get depth(): number {
    return this.#items.length;
  }

  /** The configured bound. */
  get capacity(): number {
    return this.#capacity;
  }

  /** Discard everything. Used at teardown; returns what was discarded. */
  drain(): T[] {
    return this.#items.splice(0, this.#items.length);
  }
}
