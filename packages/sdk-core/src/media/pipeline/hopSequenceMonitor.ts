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
const REORDERED: HopObservation = { missing: 0, reordered: true, undeclaredStreamId: false };
const UNDECLARED: HopObservation = { missing: 0, reordered: false, undeclaredStreamId: true };

/**
 * Default for `media.ingress.hopRestartBackwardJumpFrames`.
 *
 * 64 frames is 1.28 s of 20 ms audio: far deeper than any plausible QUIC
 * datagram reorder, so a genuine late frame is never mistaken for a restart. The
 * cost of the bound in the other direction is small and one-off: a publisher
 * whose counter restarts while the mark is at most this high reads as at most
 * this many reorders before its counter passes the old mark. Defined here, beside
 * the class, for the same `config -> media` dependency direction the
 * receive-path bounds use.
 */
export const DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES = 64;

/** The hop sequence is a u32 on the wire; serial arithmetic is modulo 2^32. */
const HOP_SPACE = 2 ** 32;
/**
 * RFC 1982 half-space: a difference at or above this is "behind", not "ahead".
 *
 * Exported as the single source for the upper bound on
 * `media.ingress.hopRestartBackwardJumpFrames`: a bound at or above it cannot
 * tell a backward jump from a forward one. `clientConfig.ts` imports it rather
 * than restating the number.
 */
export const HOP_HALF_SPACE = 2 ** 31;

/**
 * Per-declared-slot hop-sequence high-water marks, reset on a change of sender.
 *
 * ---------------------------------------------------------------------------
 * WHY THE MARK IS PER (SLOT, SENDER), NOT PER SLOT
 * ---------------------------------------------------------------------------
 *
 * MH's hop counter is NOT per slot. It is per (publishing connection,
 * egress_stream_id): it lives in the PUBLISHER's `ConnectionForwarder`
 * (`crates/mh-service/src/media/forwarder.rs`), starts at 0 the first time that
 * connection forwards onto an id, and has no reset API. Two events therefore
 * change the counter under an unchanged slot id:
 *
 *   * SLOT REFILL (static fill, story 2). When a slot's sender leaves, MC
 *     refills the slot with a different sender under the SAME egress stream, and
 *     frames now come from a different forwarder whose counter starts at 0.
 *     Detected by the frame's key-id sender differing from the slot's recorded
 *     sender.
 *   * PUBLISHER RECONNECT. A reconnecting publisher keeps its sender id and slot
 *     but gets a fresh MH connection, so a fresh forwarder, so a counter back at
 *     0 on an UNCHANGED (slot, sender). Detected as a backward jump deeper than
 *     `restartBackwardJumpFrames`.
 *
 * Without these resets, every frame from the new counter reads as a REORDER until
 * it passes the old mark (about 100 s at audio rate from a mark of 5000). That
 * makes the documented loss estimate `gap_frames - reorder` go NEGATIVE, which
 * HIDES real loss during roster churn — this control failing exactly when it is
 * needed. A reset is a new baseline: never counted as loss, reorder or duplicate.
 *
 * KNOWN BLIND WINDOW AT A REFILL. Frames the OLD sender had in flight when MH
 * switched the edge still arrive after the new sender's first frame. Each one
 * flips the slot's recorded sender back, and the next new-sender frame flips it
 * again — every flip a silent reset. So for the in-flight window around a
 * refill (bounded by what was queued at the switch, a handful of frames), loss
 * on this slot is neither counted as a gap nor as a reorder. Accepted rather
 * than suppressed: ignoring the previous sender outright would blind the
 * monitor to it for good if it legitimately returns to the slot (a subscriber's
 * shrink-then-grow redeclaration can do that), which is worse than a few
 * uncounted frames at a boundary.
 *
 * ---------------------------------------------------------------------------
 * STILL BOUNDED BY CONSTRUCTION
 * ---------------------------------------------------------------------------
 *
 * The map is keyed by SLOT alone, and each entry holds the slot's current
 * `{ sender, highest }`. A sender change OVERWRITES the slot's entry; there is no
 * (slot × sender) key space and so nothing to evict. The bound is still the slot
 * ids this client itself declared, which it chose and which are therefore not
 * attacker-influenced. That is a stronger bound than an LRU.
 *
 * The sender comes from the frame's key id BEFORE verification, so it is
 * attacker-influenced — but it only selects whether to reset a counter a relay
 * already writes freely, it creates no state, and it never gates a drop-or-play
 * decision. The replay window (`frame/receivePath.ts`) is a different mark over
 * a different field (`stream_sequence`) and is NEVER reset by anything here.
 *
 * It CAN suppress this monitor's own output: a writer alternating the key-id
 * sender on every frame takes the refill branch each time, so neither `missing`
 * nor `reordered` ever increments. That grants nothing new, and the reason is
 * the trust boundary, stated so a later change cannot lean on this comment
 * without it: this monitor's OUTPUT is only as trustworthy as the relay that
 * writes its INPUT. MH writes the hop sequence itself, so an adversarial or
 * buggy relay can already blind it completely — consecutive hop numbers while
 * dropping frames — with no key-id manipulation at all. Nobody else can reach
 * the field: it rides inside the QUIC/TLS connection to that relay. And a forged
 * key id fails signature verification downstream, so the frame is rejected
 * anyway; the writer spends a rejected frame to hide a gap it could hide for
 * free. If the reset path ever gains a consequence beyond these counters, this
 * argument no longer holds and must be redone.
 */
export class HopSequenceMonitor {
  readonly #declaredStreamIds: ReadonlySet<number>;
  readonly #restartBackwardJumpFrames: number;
  readonly #bySlot = new Map<number, { sender: number | undefined; highest: number }>();

  /**
   * @param restartBackwardJumpFrames a backward jump DEEPER than this is a
   * counter restart, not a reorder. From `media.ingress.hopRestartBackwardJumpFrames`.
   */
  constructor(
    declaredStreamIds: Iterable<number>,
    restartBackwardJumpFrames: number = DEFAULT_HOP_RESTART_BACKWARD_JUMP_FRAMES,
  ) {
    if (
      !Number.isInteger(restartBackwardJumpFrames) ||
      restartBackwardJumpFrames <= 0 ||
      restartBackwardJumpFrames >= HOP_HALF_SPACE
    ) {
      throw new RangeError(
        `restartBackwardJumpFrames must be an integer in [1, ${HOP_HALF_SPACE}), got ${String(restartBackwardJumpFrames)}`,
      );
    }
    this.#declaredStreamIds = new Set(declaredStreamIds);
    this.#restartBackwardJumpFrames = restartBackwardJumpFrames;
  }

  /**
   * Record one frame's relay-region hop sequence.
   *
   * Called AFTER the wire-level receive count and BEFORE nothing in particular:
   * its result never influences whether the frame is processed.
   *
   * @param senderId the sender unpacked from the frame's own key id, or
   * `undefined` when the key id could not be read. `undefined` never triggers a
   * sender-change reset: an unreadable frame is not evidence of a new sender.
   */
  observe(streamId: number, hopSequence: number, senderId: number | undefined): HopObservation {
    if (!this.#declaredStreamIds.has(streamId)) {
      // No map entry, no allocation, no growth. The bound is on STATE CREATION,
      // not on rendering.
      return UNDECLARED;
    }
    const mark = this.#bySlot.get(streamId);
    if (mark === undefined) {
      // First frame for this slot. There is no previous number, so there is no
      // gap — a first observation cannot be evidence of loss.
      this.#bySlot.set(streamId, { sender: senderId, highest: hopSequence });
      return NONE;
    }
    if (senderId !== undefined && mark.sender !== undefined && senderId !== mark.sender) {
      // SLOT REFILL: a different publisher's counter. New baseline.
      mark.sender = senderId;
      mark.highest = hopSequence;
      return NONE;
    }
    if (mark.sender === undefined) mark.sender = senderId;

    // RFC 1982 serial comparison over the u32 hop space, so a wrap from near
    // u32::MAX to 0 reads as one step forward rather than as a restart.
    const ahead = (hopSequence - mark.highest + HOP_SPACE) % HOP_SPACE;
    if (ahead === 0) return REORDERED; // a duplicate number: at the mark
    if (ahead < HOP_HALF_SPACE) {
      mark.highest = hopSequence;
      return { missing: ahead - 1, reordered: false, undeclaredStreamId: false };
    }
    const behind = HOP_SPACE - ahead;
    if (behind > this.#restartBackwardJumpFrames) {
      // PUBLISHER RECONNECT: the counter restarted. New baseline. A large
      // FORWARD jump is deliberately NOT treated this way — that is real loss.
      mark.highest = hopSequence;
      return NONE;
    }
    return REORDERED;
  }

  /** Drop all marks. No key material; a plain state reset for teardown symmetry. */
  clear(): void {
    this.#bySlot.clear();
  }
}
