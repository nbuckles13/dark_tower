// File: packages/sdk-core/src/media/pipeline/receiveVerification.ts
//
// HOT PATH (the recorder's methods run per frame). No logging, no metric names,
// no label construction, and no allocation once a key exists.
//
// THE RECEIVE-SIDE VERIFICATION COUNTERS (story 2 R-30), layers 1 and 2 plus
// drops, keyed by (slot, OBSERVED sender) so misrouting surfaces as a count under
// the wrong key rather than being hidden by attribution:
//
//   * layer 1 — `keyed`: a frame arrived on relay slot S whose key id names
//     sender X. Counted once the key id parses, BEFORE verification.
//   * layer 2 — `verified`: that frame then verified under X's roster key and
//     decrypted. Counted beside `frameAccepted`.
//   * drops — (slot, sender if the key id parsed, reject token).
//
// ---------------------------------------------------------------------------
// A TEST OBSERVATION, INJECTED — NEVER A PRODUCTION SIGNAL
// ---------------------------------------------------------------------------
//
// The ingress holds a recorder only when an embedder injects one
// (`MeetingSessionOptions.receiveVerification`). The web app constructs one
// ONLY inside `if (__E2E_HOOKS__)`, so a production bundle carries neither this
// class nor any per-(slot, sender) state, and a production ingress pays one
// untaken `undefined` check per frame. Nothing here reaches `MetricsSink`: a
// per-sender count must never become a metric label (ADR-0036 §11).
//
// `stream_id` is MH-written and signed by nobody. It is RECORDED here as the
// observed slot and is never an input to any decision — the same quarantine
// `ingress.ts` applies for the hop monitor. The key-id sender at layer 1 is
// likewise unverified; that is the point of counting it separately from layer 2.
//
// ---------------------------------------------------------------------------
// BOUNDED: A MISBEHAVING RELAY CANNOT GROW THIS
// ---------------------------------------------------------------------------
//
// Both the slot and the pre-verify sender are attacker-influenced, so state is
// bounded like the hop monitor's: an undeclared `stream_id` collapses into ONE
// `undeclared` bucket, and past {@link RECEIVE_VERIFICATION_MAX_KEYS} distinct
// keys no new key is created — the event is counted on `overflow` instead.
//
// WHAT IS NEVER RECORDED: key ids, generations, salts, wrapped keys, frame bytes,
// sizes, timestamps, roster keys. Only the sender COMPONENT of the key id.

import { ALL_REJECT_REASONS, type RejectReason } from '../frame/rejectReason.js';

/** The slot a frame was observed on, as the relay stated it. */
export type ObservedSlot =
  /** A declared relay `stream_id`. */
  | number
  /** A `stream_id` this client never declared (one bucket for all of them). */
  | 'undeclared'
  /** The frame did not parse far enough to have a `stream_id`. */
  | 'unparsed';

/** What the ingress reports. Implementations must not throw. */
export interface ReceiveVerificationRecorder {
  /** Layer 1: the key id parsed and names `senderId`. */
  keyed(slot: ObservedSlot, senderId: number): void;
  /** Layer 2: the frame verified under `senderId`'s roster key and decrypted. */
  verified(slot: ObservedSlot, senderId: number): void;
  /** A counted drop. `senderId` is `undefined` when the key id never parsed. */
  dropped(slot: ObservedSlot, senderId: number | undefined, reason: RejectReason): void;
}

/** One (slot, sender) row of layers 1 and 2. */
export interface ReceiveVerificationLayers {
  readonly slot: ObservedSlot;
  readonly senderId: number;
  readonly keyed: number;
  readonly verified: number;
}

/** One (slot, sender, reason) drop row. `senderId` is `null` when unattributed. */
export interface ReceiveVerificationDrop {
  readonly slot: ObservedSlot;
  readonly senderId: number | null;
  readonly reason: RejectReason;
  readonly count: number;
}

/** A cumulative, monotone snapshot. Plain numbers and bounded strings only. */
export interface ReceiveVerificationSnapshot {
  readonly layers: readonly ReceiveVerificationLayers[];
  readonly drops: readonly ReceiveVerificationDrop[];
  /**
   * Events not recorded: a NEW key past the key cap, or a drop whose reason is
   * outside the closed `RejectReason` vocabulary (unreachable for a well-typed
   * caller; counted here rather than misfiled).
   */
  readonly overflow: number;
}

/** Hard cap on distinct keys across layers and drops. */
export const RECEIVE_VERIFICATION_MAX_KEYS = 256;

// Numeric composite keys, so the per-frame path builds no strings.
// slotCode: 0 = unparsed, 1 = undeclared, stream_id + 2 otherwise (stream_id is
// a u16 on the wire). senderCode: 0 = unattributed, sender + 1 otherwise.
const SLOT_UNPARSED = 0;
const SLOT_UNDECLARED = 1;
const SENDER_SPACE = 0x1_0001;
// Derived from the vocabulary itself, so a growing reject set can never alias
// drop keys across (slot, sender) rows.
const REASON_SPACE = ALL_REJECT_REASONS.length;
const REASON_INDEX = new Map<RejectReason, number>(ALL_REJECT_REASONS.map((r, i) => [r, i]));

function slotCode(slot: ObservedSlot): number {
  if (slot === 'unparsed') return SLOT_UNPARSED;
  if (slot === 'undeclared') return SLOT_UNDECLARED;
  return slot + 2;
}

function slotOf(code: number): ObservedSlot {
  if (code === SLOT_UNPARSED) return 'unparsed';
  if (code === SLOT_UNDECLARED) return 'undeclared';
  return code - 2;
}

interface LayerCounts {
  keyed: number;
  verified: number;
}

/**
 * The bounded in-memory recorder. Construct one per session, inject it, and
 * {@link snapshot} it on a sampling timer — never per frame.
 */
export class BoundedReceiveVerificationRecorder implements ReceiveVerificationRecorder {
  readonly #layers = new Map<number, LayerCounts>();
  readonly #drops = new Map<number, number>();
  readonly #maxKeys: number;
  #overflow = 0;

  constructor(maxKeys: number = RECEIVE_VERIFICATION_MAX_KEYS) {
    this.#maxKeys = maxKeys;
  }

  keyed(slot: ObservedSlot, senderId: number): void {
    const counts = this.#layer(slot, senderId);
    if (counts) counts.keyed += 1;
  }

  verified(slot: ObservedSlot, senderId: number): void {
    const counts = this.#layer(slot, senderId);
    if (counts) counts.verified += 1;
  }

  dropped(slot: ObservedSlot, senderId: number | undefined, reason: RejectReason): void {
    const reasonIndex = REASON_INDEX.get(reason);
    if (reasonIndex === undefined) {
      // Unreachable for a well-typed caller (`RejectReason` is closed). Counted
      // rather than filed under some other token, which would misreport.
      this.#overflow += 1;
      return;
    }
    const senderCode = senderId === undefined ? 0 : senderId + 1;
    const key = (slotCode(slot) * SENDER_SPACE + senderCode) * REASON_SPACE + reasonIndex;
    const current = this.#drops.get(key);
    if (current !== undefined) {
      this.#drops.set(key, current + 1);
      return;
    }
    if (this.#size() >= this.#maxKeys) {
      this.#overflow += 1;
      return;
    }
    this.#drops.set(key, 1);
  }

  /** Cumulative counts. Allocates; sample it, never call it per frame. */
  snapshot(): ReceiveVerificationSnapshot {
    const layers: ReceiveVerificationLayers[] = [];
    for (const [key, counts] of this.#layers) {
      layers.push({
        slot: slotOf(Math.floor(key / SENDER_SPACE)),
        senderId: (key % SENDER_SPACE) - 1,
        keyed: counts.keyed,
        verified: counts.verified,
      });
    }
    const drops: ReceiveVerificationDrop[] = [];
    for (const [key, count] of this.#drops) {
      const reasonIndex = key % REASON_SPACE;
      const pair = Math.floor(key / REASON_SPACE);
      const senderCode = pair % SENDER_SPACE;
      drops.push({
        slot: slotOf(Math.floor(pair / SENDER_SPACE)),
        senderId: senderCode === 0 ? null : senderCode - 1,
        reason: ALL_REJECT_REASONS[reasonIndex] as RejectReason,
        count,
      });
    }
    return { layers, drops, overflow: this.#overflow };
  }

  #layer(slot: ObservedSlot, senderId: number): LayerCounts | undefined {
    const key = slotCode(slot) * SENDER_SPACE + senderId + 1;
    const existing = this.#layers.get(key);
    if (existing) return existing;
    if (this.#size() >= this.#maxKeys) {
      this.#overflow += 1;
      return undefined;
    }
    const created = { keyed: 0, verified: 0 };
    this.#layers.set(key, created);
    return created;
  }

  #size(): number {
    return this.#layers.size + this.#drops.size;
  }
}
