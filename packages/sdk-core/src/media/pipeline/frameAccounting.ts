// File: packages/sdk-core/src/media/pipeline/frameAccounting.ts
//
// HOT PATH. No logging, no metric names, no label construction.
//
// THE RECEIVE-PATH ACCOUNTING IDENTITY, ONCE, FOR EVERY KIND:
//
//     received = accepted + sum(drops by reason)
//
// counted at the PARSE boundary (story 3 C-B; the video story's NOTE in
// `ingress.ts`): audio receives one frame per datagram, video one per frame at
// its first byte on a group stream. Every received frame ends on the right
// exactly once — as accepted, or as one reject reason — and the video identity
// excludes `VIDEO_POST_DECODE_REJECT_REASONS` (`../frame/rejectReason.ts`),
// whose frames were accepted first.
//
// ONE INCREMENT, TWO READERS. Each method bumps the production counter on the
// per-kind handle AND the plain field a sampling embedder reads, adjacently, so
// the getter and `dt_client_media_frames_{received,accepted,dropped}_total`
// cannot drift apart. Do not split them.
//
// PER-SLOT TALLIES (story 3 R-35, the LOCAL statistics overlay). Beside the
// kind totals, each DECLARED slot gets one tally — packets and bytes that
// arrived naming it, and drops by reason — PRE-ALLOCATED at construction and
// looked up by NUMERIC slot id, so the forward path does integer `++` only: no
// insertion, no allocation, no string key. A slot id outside the declared set
// (`'undeclared'`, `'unparsed'`, or a number never declared) is simply not
// tallied. The tallies are read by {@link FrameAccounting.slotTally}, a sampled
// snapshot; they never reach a metric handle, a label or the e2e bus — per-slot
// byte counts are per-stream size series, which ADR-0036 §11 forbids exporting.

import type { WrapOutcome } from '../frame/receivePath.js';
import { ALL_REJECT_REASONS, type RejectReason } from '../frame/rejectReason.js';
import type { MediaKindMetrics } from '../setup/mediaMetrics.js';
import type { ObservedSlot, ReceiveVerificationRecorder } from './receiveVerification.js';

/** The monotone counters one ingress keeps. Plain reads; never per-frame emissions. */
export interface FrameCounts {
  readonly framesReceived: number;
  readonly framesAccepted: number;
  readonly framesDropped: number;
  /** The most recent reject token; bounded by TYPE (the vectors file's closed set). */
  readonly lastDropReason: RejectReason | undefined;
}

/** One declared slot's local tally (R-35). Cumulative; sampled, never exported. */
export interface SlotReceiveTally {
  /** Frames that arrived naming this slot (its relay `stream_id`). */
  readonly packets: number;
  /** Their wire bytes. A cumulative SUM; no per-frame size is retained. */
  readonly bytes: number;
  /** Drops attributed to this slot, by reason (every reason present, zero-initialised). */
  readonly drops: Readonly<Record<RejectReason, number>>;
}

interface MutableTally {
  packets: number;
  bytes: number;
  readonly drops: Record<RejectReason, number>;
}

function newTally(): MutableTally {
  const drops = {} as Record<RejectReason, number>;
  for (const reason of ALL_REJECT_REASONS) drops[reason] = 0;
  return { packets: 0, bytes: 0, drops };
}

/**
 * One kind's accounting — the ONE accept tail and the ONE drop tail both
 * ingresses use, so the counters, the wrap-outcome report and the test-injected
 * verification evidence (story 2 R-30) cannot drift between kinds.
 */
export class FrameAccounting implements FrameCounts {
  readonly #metrics: MediaKindMetrics;
  readonly #verify: ReceiveVerificationRecorder | undefined;
  #received = 0;
  #accepted = 0;
  #dropped = 0;
  /**
   * Typed as the closed union rather than `string` deliberately: a bounded
   * telemetry token is always one refactor away from becoming a label or a log
   * field, and `string` is the annotation that would let a platform message
   * land here and compile (ADR-0036 §11, `label-taxonomy.md` R2).
   */
  #lastDropReason: RejectReason | undefined;
  /** Declared slot id -> its tally. Built once; never inserted into afterwards. */
  readonly #slots: ReadonlyMap<number, MutableTally>;

  constructor(
    metrics: MediaKindMetrics,
    verification?: ReceiveVerificationRecorder,
    declaredSlotIds: readonly number[] = [],
  ) {
    this.#metrics = metrics;
    this.#verify = verification;
    this.#slots = new Map(declaredSlotIds.map((id) => [id, newTally()]));
  }

  /**
   * A frame naming `slot` arrived with `bytes` wire bytes (R-35 local tally).
   * Called once per frame where the slot is first known. Not a metric.
   */
  arrived(slot: ObservedSlot, bytes: number): void {
    if (typeof slot !== 'number') return;
    const tally = this.#slots.get(slot);
    if (tally === undefined) return;
    tally.packets += 1;
    tally.bytes += bytes;
  }

  /**
   * `slotId`'s local tally, COPIED (allocates — sample it, never per frame), or
   * `undefined` for a slot this accounting was not built with.
   */
  slotTally(slotId: number): SlotReceiveTally | undefined {
    const tally = this.#slots.get(slotId);
    if (tally === undefined) return undefined;
    return { packets: tally.packets, bytes: tally.bytes, drops: { ...tally.drops } };
  }

  /** A frame arrived at the parse boundary. The identity's left side. */
  received(): void {
    this.#metrics.frameReceived();
    this.#received += 1;
  }

  /**
   * A frame verified, decrypted and reached its decoder (lane or slot
   * pipeline): counted, its wrap outcome reported, and layer 2 of the
   * verification evidence recorded under the VERIFIED sender.
   */
  accepted(wrapOutcome: WrapOutcome, slot: ObservedSlot, senderId: number): void {
    // -----------------------------------------------------------------------
    // REPORTED BY EXCLUSION. DO NOT "SIMPLIFY" THIS BACK TO NAMING THE TWO
    // VALUES — it reads like the same thing and is not.
    // -----------------------------------------------------------------------
    //
    // What is spelled here are the three UNINTERESTING outcomes; the two that
    // are counted are whatever remains of the `WrapOutcome` union, and
    // TypeScript narrows `outcome` to exactly those — so `wrap_key_id_mismatch`'s
    // pending rename will not have to find ANY call site, and this is the one
    // site for both kinds. Both reported outcomes are NON-DROPPING: the frame
    // was accepted, so counting either as a drop would break the identity.
    if (wrapOutcome !== 'absent' && wrapOutcome !== 'cached' && wrapOutcome !== 'already_held') {
      this.#metrics.wrapOutcome(wrapOutcome);
    }
    this.#metrics.frameAccepted();
    this.#accepted += 1;
    this.#verify?.verified(slot, senderId);
  }

  /**
   * A frame was dropped for a wire or policy reason, verbatim — no mapping
   * table, no bucket, no `other`. `senderId` is the key-id sender when one was
   * parsed from the decoded frame, else `undefined`.
   */
  dropped(reason: RejectReason, slot: ObservedSlot, senderId: number | undefined): void {
    this.#metrics.frameDropped(reason);
    this.#dropped += 1;
    this.#lastDropReason = reason;
    if (typeof slot === 'number') {
      const tally = this.#slots.get(slot);
      if (tally !== undefined) tally.drops[reason] += 1;
    }
    this.#verify?.dropped(slot, senderId, reason);
  }

  get framesReceived(): number {
    return this.#received;
  }

  get framesAccepted(): number {
    return this.#accepted;
  }

  get framesDropped(): number {
    return this.#dropped;
  }

  get lastDropReason(): RejectReason | undefined {
    return this.#lastDropReason;
  }
}
