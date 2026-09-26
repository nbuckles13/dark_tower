// File: packages/sdk-core/src/media/pipeline/receiveLanes.ts
//
// HOT PATH (`laneFor` and `DecodeLane.decode` run per frame). No logging, no
// metric names, no label construction. Metric handles arrive pre-bound from
// `../setup/mediaMetrics.ts`.
//
// ---------------------------------------------------------------------------
// THE ONE SLOT -> SENDER VIEW, AND IT COMES FROM MC
// ---------------------------------------------------------------------------
//
// This object is the receive side's single answer to "which senders may this
// client decode?" (ADR-0036 §9, story 2 slot-edge gate). It is fed ONLY from
// MC's `StreamAssignments`, through `AudioPipeline.setReceiveAssignments`.
//
// It must never be fed from observed traffic. The hop-sequence monitor also
// tracks (slot, sender), but as a DIAGNOSTIC of what arrived; a gate that
// authorised whatever arrived would authorise any sender a misrouting or
// compromised handler chose to forward, which is precisely what it exists to
// refuse. Two maps answering "who is on which slot" — one stated, one observed —
// is a redirect primitive; the stated one is the authority.
//
// The gate is on the key-id SENDER, after that sender's signature verified. The
// relay `stream_id` is not consulted: it is MH-written and signed by nobody, and
// `ingress.ts` keeps it out of every drop-or-play decision. Membership is
// therefore per sender regardless of slot or transport — a sender assigned to
// any of this client's slots is decodable on whichever handler its frame
// arrives on.
//
// ---------------------------------------------------------------------------
// DECODERS ARE CHILDREN OF THE ASSIGNMENT SET, KEYED BY SENDER
// ---------------------------------------------------------------------------
//
// One decode lane — its own `AudioDecoderSeam` and its own playback lane — per
// ASSIGNED sender, so:
//
//   * the decoder count is bounded by this client's DECLARED slots, never by the
//     roster or by attacker-chosen sender ids (assignments for a slot this
//     client never declared are ignored, so a misbehaving MC cannot inflate it);
//   * a decoder fault replaces only THAT sender's decoder (R-5); the others keep
//     decoding and playing;
//   * a slot re-map closes only the lanes of senders that LEFT the set. A sender
//     that moved slot, or whose slot is unchanged, keeps its lane; and nothing
//     here ever touches the replay window, the transmit-key cache or the roster,
//     which are per sender and owned elsewhere.

import type { MediaMetrics } from '../setup/mediaMetrics.js';
import type {
  AudioDecoderFactory,
  AudioDecoderSeam,
  EncodedAudioFrame,
  PlaybackLane,
  PlaybackSink,
} from '../setup/seams.js';

/** One slot's source, as MC stated it. `senderId` absent means no source. */
export interface SlotAssignment {
  readonly slotId: number;
  readonly senderId: number | undefined;
}

/** Where an accepted frame goes. Obtainable only for an assigned sender. */
export interface DecodeLane {
  /** Hand one accepted frame to this sender's decoder. Never throws. */
  decode(frame: EncodedAudioFrame): void;
}

/** Construction options for {@link ReceiveLanes}. */
export interface ReceiveLanesOptions {
  readonly metrics: MediaMetrics;
  readonly decoderFactory: AudioDecoderFactory;
  readonly playback: PlaybackSink;
  readonly sampleRateHz: number;
  readonly channels: number;
  /** The slot ids this client declared. Assignments for any other slot are ignored. */
  readonly declaredSlotIds: readonly number[];
  /** Independent memory bound behind the declared-slot bound. */
  readonly maxLanes: number;
  /** A faulted sender's decoder is replaced at most once per this interval. */
  readonly restartBackoffMs: number;
  /** Frames a lane holds while its decoder is being created or replaced. */
  readonly pendingFrames: number;
  readonly clock: () => number;
  /**
   * A decoder faulted. Raised by the owner as a NON-fatal fault, deduplicated
   * there — one bad sender is never allowed to stop the pipeline.
   */
  readonly onDecoderFault: () => void;
}

/**
 * The slot-assignment view plus one decode lane per assigned sender.
 */
export class ReceiveLanes {
  readonly #options: ReceiveLanesOptions;
  readonly #declared: ReadonlySet<number>;
  /**
   * sender id -> its decode lane. THE AUTHORITY FOR THE GATE: a sender has a
   * lane exactly when MC's latest snapshot assigns it to a declared slot. The
   * slot -> sender pairs themselves are not retained — each snapshot replaces
   * the set wholesale, so only the set of assigned senders is ever consulted.
   */
  readonly #lanes = new Map<number, Lane>();
  #closed = false;

  constructor(options: ReceiveLanesOptions) {
    this.#options = options;
    this.#declared = new Set(options.declaredSlotIds);
  }

  /**
   * Apply MC's latest assignment snapshot. REPLACES the view; never merges.
   *
   * Opens a lane for each sender newly in the set and closes the lane of each
   * sender that left it. A sender present before and after is untouched —
   * whether or not it changed slot.
   */
  setAssignments(assignments: readonly SlotAssignment[]): void {
    if (this.#closed) return;
    const assigned = new Set<number>();
    for (const { slotId, senderId } of assignments) {
      // An undeclared slot is not ours to fill, whatever MC says; and absent is
      // "no source", never sender 0.
      if (!this.#declared.has(slotId) || senderId === undefined) continue;
      assigned.add(senderId);
    }
    for (const [senderId, lane] of this.#lanes) {
      if (!assigned.has(senderId)) {
        lane.close();
        this.#lanes.delete(senderId);
      }
    }
    for (const senderId of assigned) {
      if (this.#lanes.has(senderId) || this.#lanes.size >= this.#options.maxLanes) continue;
      this.#lanes.set(senderId, new Lane(this.#options));
    }
  }

  /**
   * The decode lane for `senderId`, or `undefined` if MC has not assigned that
   * sender to any of this client's declared slots — in which case the frame is
   * dropped as `sender_not_assigned` by the caller, before it is opened.
   *
   * The ONLY input is the verified key-id sender.
   */
  laneFor(senderId: number): DecodeLane | undefined {
    return this.#lanes.get(senderId);
  }

  /** Senders currently assigned and decodable. For assertion and sampling only. */
  get assignedSenders(): readonly number[] {
    return [...this.#lanes.keys()];
  }

  /** Frames waiting on a lane's decoder. For the per-lane accounting identity. */
  pendingFor(senderId: number): number {
    return this.#lanes.get(senderId)?.pendingCount ?? 0;
  }

  /** Close every lane. Idempotent. */
  close(): void {
    this.#closed = true;
    for (const lane of this.#lanes.values()) lane.close();
    this.#lanes.clear();
  }
}

/** One sender's decoder, pending queue and playback lane. */
class Lane implements DecodeLane {
  readonly #options: ReceiveLanesOptions;
  readonly #playback: PlaybackLane;
  #decoder: AudioDecoderSeam | undefined;
  #creating = false;
  /**
   * Incremented per decoder instance, so a late creation or a late error from
   * a decoder this lane has already discarded cannot act on the current one.
   */
  #instance = 0;
  #lastFaultAtMs: number | undefined;
  readonly #pending: EncodedAudioFrame[] = [];
  #closed = false;

  constructor(options: ReceiveLanesOptions) {
    this.#options = options;
    this.#playback = options.playback.openLane();
    // Created EAGERLY at assignment, so it is usually ready before MH forwards
    // the sender's first frame.
    this.#ensureDecoder();
  }

  get pendingCount(): number {
    return this.#pending.length;
  }

  decode(frame: EncodedAudioFrame): void {
    if (this.#closed) return;
    if (this.#decoder) {
      this.#decoder.decode(frame);
      return;
    }
    if (this.#pending.length >= this.#options.pendingFrames) {
      // Bounded: evict the OLDEST and count it. A post-accept loss — see
      // `MediaMetrics.decodeQueueDropped` for why it is not a frame drop reason.
      this.#pending.shift();
      this.#options.metrics.decodeQueueDropped();
    }
    this.#pending.push(frame);
    this.#ensureDecoder();
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#instance += 1;
    this.#pending.length = 0;
    this.#decoder?.close();
    this.#decoder = undefined;
    this.#playback.close();
  }

  /**
   * Create a decoder unless one exists, one is being created, or this sender's
   * last fault is inside the backoff. Called at construction and on every frame
   * that finds no decoder, so a replacement needs no timer: the first frame
   * after the backoff triggers it.
   */
  #ensureDecoder(): void {
    if (this.#closed || this.#decoder || this.#creating) return;
    const lastFault = this.#lastFaultAtMs;
    if (
      lastFault !== undefined &&
      this.#options.clock() - lastFault < this.#options.restartBackoffMs
    ) {
      return;
    }
    this.#creating = true;
    const instance = (this.#instance += 1);
    this.#options
      .decoderFactory({
        sampleRateHz: this.#options.sampleRateHz,
        channels: this.#options.channels,
        onOutput: (data) => {
          if (instance === this.#instance) this.#playback.enqueue(data);
          else data.close();
        },
        onError: () => this.#onFault(instance),
      })
      .then(
        (decoder) => {
          this.#creating = false;
          if (instance !== this.#instance || this.#closed) {
            decoder.close();
            return;
          }
          this.#decoder = decoder;
          // Flush in arrival order, synchronously, so no newer frame can be
          // decoded ahead of an older queued one.
          for (const frame of this.#pending.splice(0)) decoder.decode(frame);
        },
        () => {
          this.#creating = false;
          this.#onFault(instance);
        },
      );
  }

  /**
   * THIS sender's decoder failed. Counted and reported once per decoder
   * instance, then discarded; no other lane is touched.
   */
  #onFault(instance: number): void {
    if (instance !== this.#instance || this.#closed) return;
    this.#instance += 1;
    this.#options.metrics.decoderError();
    this.#decoder?.close();
    this.#decoder = undefined;
    this.#lastFaultAtMs = this.#options.clock();
    this.#options.onDecoderFault();
  }
}
