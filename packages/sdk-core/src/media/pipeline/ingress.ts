// File: packages/sdk-core/src/media/pipeline/ingress.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. Metric handles arrive pre-bound from `../setup/mediaMetrics.ts`.
//
// One QUIC datagram -> decode -> resolve key -> verify -> SLOT-EDGE GATE ->
// replay -> unwrap -> decrypt -> that sender's Opus decoder -> its playback lane.
//
// ---------------------------------------------------------------------------
// COUNTED AT THE WIRE, BEFORE ANY PARSE
// ---------------------------------------------------------------------------
//
// `frameReceived()` is the FIRST statement for every datagram — before decode,
// before verification, before anything that can fail. That is what makes
//
//     received = accepted + sum(drops by reason)
//
// hold: every datagram is counted once on the left, and exactly once on the
// right. It is a RECEIVE-PATH ACCOUNTING IDENTITY whose job is to prove that no
// drop path fails to count itself; it says nothing about audibility.
//
// THE COUNTING POINT IS THE PARSE BOUNDARY, PER KIND (story 3 C-B resolved the
// video story's note here). For audio one datagram IS one frame, so the parse
// boundary and the wire coincide and nothing about this path changed. Video
// frames share a group stream, so `videoIngress.ts` counts each frame at its
// first byte, and the identity is re-established there with one stated
// exception: `received = accepted + sum(drops{reason not in
// VIDEO_POST_DECODE_REJECT_REASONS})`. Both kinds keep the counters in ONE
// helper, `frameAccounting.ts`, and run ONE policy, `receivePolicy.ts`.
//
// ---------------------------------------------------------------------------
// THERE IS NO SELF-TRUST BRANCH
// ---------------------------------------------------------------------------
//
// Frames from N senders arrive here, possibly including this client's own if MC
// ever assigns it. The pipeline does not distinguish them and must not: every
// frame runs the identical resolve-then-verify path, and there is no branch
// anywhere that accepts a frame because this client signed it.
//
// The identity key is resolved from `key_id.sender_id` AND NOTHING ELSE — never
// from the slot assignment, never from the relay `stream_id`, never from a
// "current speaker" cache. `RosterIdentityKeys` has no method that takes any of
// those, so the wrong lookup is not expressible. The resolve/verify/gate/open
// sequence itself is `receivePolicy.ts:receiveByKind`, shared with video.
//
// ---------------------------------------------------------------------------
// THE RELAY REGION IS UNAUTHENTICATED AND IS QUARANTINED TO ONE CONSUMER
// ---------------------------------------------------------------------------
//
// `stream_id` and `hop_sequence` are written by MH and signed by nobody. On
// this DATAGRAM path they reach exactly one place — the hop-sequence monitor,
// which creates no state for an undeclared `stream_id` — and they never reach
// key selection, roster lookup, attribution, the replay window, or any
// drop-or-play decision.
//
// VIDEO DIFFERS, BY DESIGN (story 3 C-B; security accepted at Gate 1): on a
// group stream the relay `stream_id` SELECTS the slot pipeline, because MH owns
// the per-subscriber slot mapping and the media kind comes from transport and
// slot. It still never reaches attribution: the render tag, the pairing view
// and every evidence hook use only the VERIFIED key-id sender. A stream naming
// an undeclared or non-video slot, or changing slot mid-stream, is aborted and
// counted `stream_slot_mismatch` (lead ruling E2). See `videoIngress.ts`.
//
// ---------------------------------------------------------------------------
// THE SLOT-EDGE GATE: VERIFY, THEN GATE, THEN OPEN
// ---------------------------------------------------------------------------
//
// ADR-0036 §9 puts every participant on the roster, including those this client
// shares no handler with, so "has a roster key" is not the same as "may be
// decoded here". A frame from a sender MC has not assigned to one of this
// client's slots is dropped as `sender_not_assigned` — defence in depth against
// a misrouting or compromised handler; in the honest data plane MH never
// forwards it.
//
// ORDER IS LOAD-BEARING. The gate runs AFTER verify, so the drop is of a frame
// whose attribution is established and the reason carries no unverified claim.
// It runs BEFORE `openVerifiedFrame`, so a gated frame neither decrypts, caches a
// transmit key from its wrap, nor advances the replay window — which is what
// lets the SAME frame be accepted later if that sender is then assigned. The
// gate reads the verified key-id sender only; see `receiveLanes.ts` for why its
// authority is MC's stated assignment and never observed traffic.

import { decodeFrame } from '../frame/frameCodec.js';
import { keyIdSenderOf } from '../frame/sframe.js';
import type { ReceiverKeys, ReplayWindow, TransmitKeyCache } from '../frame/receivePath.js';
import { FrameRejectedError } from '../frame/rejectReason.js';
import type { RejectReason } from '../frame/rejectReason.js';
import type { MediaKindMetrics } from '../setup/mediaMetrics.js';
import type { DecodeLane } from './receiveLanes.js';
import type { FirstMediaObserver } from '../setup/measurement.js';
import type { HopSequenceMonitor } from './hopSequenceMonitor.js';
import type { ObservedSlot, ReceiveVerificationRecorder } from './receiveVerification.js';
import { FrameAccounting } from './frameAccounting.js';
import { receiveByKind, type IdentityKeyResolver, type ReceiveGate } from './receivePolicy.js';

export type { IdentityKeyResolver } from './receivePolicy.js';

/** A frame that completed the receive path. Attribution comes from the key id. */
export interface AcceptedFrame {
  /** The sender, derived from `key_id.sender_id` and nothing else. */
  readonly senderId: number;
  /** Decrypted Opus payload. */
  readonly plaintext: Uint8Array;
}

/** The slot-edge gate, as ingress consumes it. See `receiveLanes.ts`. */
export interface SenderGate {
  /** The lane for an ASSIGNED sender, or `undefined` for any other. */
  laneFor(senderId: number): DecodeLane | undefined;
}

/** Construction options for {@link IngressPipeline}. */
export interface IngressPipelineOptions {
  readonly metrics: MediaKindMetrics;
  readonly roster: IdentityKeyResolver;
  readonly lanes: SenderGate;
  readonly keys: ReceiverKeys;
  readonly cache: TransmitKeyCache;
  readonly replay: ReplayWindow;
  readonly firstMedia: FirstMediaObserver;
  /** Called for every accepted frame, before the decoder is fed. */
  readonly onAccepted?: (frame: AcceptedFrame) => void;
  /**
   * Test-only receive verification (story 2 R-30). ABSENT in production: the
   * web app injects one only inside `if (__E2E_HOOKS__)`. See
   * `receiveVerification.ts`.
   */
  readonly verification?: ReceiveVerificationRecorder;
}

/**
 * Drives one datagram through the receive path.
 *
 * Decoders live in the per-sender lanes, not here, so a decoder fault replaces
 * one sender's decoder without touching the pipeline's receiver state — the
 * replay window and the transmit-key cache must survive a decoder restart, or a
 * restart would reopen the replay surface.
 */
export class IngressPipeline {
  readonly #metrics: MediaKindMetrics;
  readonly #roster: IdentityKeyResolver;
  readonly #gate: Extract<ReceiveGate, { kind: 'audio' }>;
  readonly #keys: ReceiverKeys;
  readonly #cache: TransmitKeyCache;
  readonly #replay: ReplayWindow;
  readonly #firstMedia: FirstMediaObserver;
  readonly #onAccepted: ((frame: AcceptedFrame) => void) | undefined;
  readonly #verify: ReceiveVerificationRecorder | undefined;

  /**
   * `received = accepted + sum(drops by reason)`, kept by the ONE accounting
   * helper both kinds use (`frameAccounting.ts`). Without the received side a
   * reader cannot tell "nothing is arriving" from "arriving and being rejected"
   * — two states with opposite remediations that look identical when only
   * `accepted` is visible; that ambiguity cost a live misdiagnosis in story 2.
   */
  readonly #accounting: FrameAccounting;

  constructor(options: IngressPipelineOptions) {
    this.#metrics = options.metrics;
    this.#roster = options.roster;
    const lanes = options.lanes;
    this.#gate = { kind: 'audio', laneFor: (senderId) => lanes.laneFor(senderId) };
    this.#accounting = new FrameAccounting(options.metrics, options.verification);
    this.#keys = options.keys;
    this.#cache = options.cache;
    this.#replay = options.replay;
    this.#firstMedia = options.firstMedia;
    this.#onAccepted = options.onAccepted;
    this.#verify = options.verification;
  }

  /**
   * Frames accepted since construction (monotone).
   *
   * Named `Accepted`, not `Received`, and the distinction is the one
   * `MediaKindMetrics.frameAccepted` already draws: a received frame may be dropped
   * for a wire reason, so `received = accepted + sum(drops by reason)`. This
   * counts the left-hand side's *accepted* term, so it moves only for a frame
   * that verified, decrypted and reached the decoder.
   */
  get framesAccepted(): number {
    return this.#accounting.framesAccepted;
  }

  /** Datagrams that arrived at the wire (monotone). The identity's left side. */
  get framesReceived(): number {
    return this.#accounting.framesReceived;
  }

  /** Frames rejected for a wire reason (monotone). The `sum(drops)` term. */
  get framesDropped(): number {
    return this.#accounting.framesDropped;
  }

  /** The most recent reject token; `undefined` before the first drop. Bounded by type. */
  get lastDropReason(): RejectReason | undefined {
    return this.#accounting.lastDropReason;
  }

  /**
   * Process one datagram that arrived on the transport `hopMonitor` tracks.
   *
   * ---------------------------------------------------------------------------
   * THE HOP MONITOR IS A PARAMETER, NOT PIPELINE STATE
   * ---------------------------------------------------------------------------
   *
   * A subscriber can hold slots on SEVERAL media handlers at once (ADR-0036 §9:
   * an edge sits on a handler both parties are connected to, and different
   * senders' edges may sit on different handlers). Every one of those transports
   * feeds THIS pipeline, because attribution, the replay window and the
   * transmit-key cache are all per SENDER and must be shared — a per-transport
   * receiver would reopen the replay surface once per handler.
   *
   * `hop_sequence` is the one piece of receive state that is NOT per sender: the
   * frame format defines it per (connection, media stream), and each media
   * handler writes its own downlink numbering. So it arrives with the datagram
   * rather than living here. Sharing one monitor across transports would read
   * two independent numberings as one, and every edge that moved between
   * handlers would post a false gap or reorder — inflating a loss signal
   * precisely during the connectivity change that caused it.
   *
   * NEVER THROWS ON A WIRE CONDITION: every malformed, unverifiable,
   * undecryptable or replayed frame is a bounded, counted drop and a normal
   * return. A receive path that threw on wire input would let one crafted
   * datagram end the read loop, which is a denial of service reachable by anyone
   * who can reach the port.
   *
   * It DOES rethrow a defect — anything that is not a `FrameRejectedError` — so
   * a bug cannot hide as a silent frame loss. The read loop catches it, surfaces
   * it once as a typed media error, and continues; see the caller.
   */
  async accept(datagram: Uint8Array, hopMonitor: HopSequenceMonitor): Promise<void> {
    // AT THE WIRE — which, for a datagram, is the parse boundary. First
    // statement, before any parse.
    this.#accounting.received();
    this.#firstMedia.onFrameReceived();

    // Test-only verification keys (R-30): where this frame was OBSERVED, and
    // which sender its key id names. Recorded, never decided on. Advanced as the
    // parse progresses so a drop at any stage carries what was known by then.
    let observedSlot: ObservedSlot = 'unparsed';
    let observedSender: number | undefined;

    try {
      // `decodeFrame` bounds `payload_length` against the wire maximum BEFORE
      // slicing and returns slices rather than copies, so a reject costs no more
      // than a parse and no allocation is sized from an attacker-controlled
      // field.
      const decoded = decodeFrame(datagram);

      // The key id lives in the SFrame clear header, so it is readable before
      // decryption — which is the point: it SELECTS the key.
      //
      // Read BEFORE the hop observation, because the hop monitor needs the
      // sender to tell a slot refill from a reorder. A parse failure is HELD, not
      // thrown yet: the frame still arrived on the downlink, so it must still be
      // hop-observed (with no sender) or it would read as a relay gap on top of
      // being counted as a reject. The sender is converted ONCE, here, and that
      // one value feeds both the hop monitor and the policy.
      let keyed: { readonly senderId: number } | { readonly error: unknown };
      try {
        keyed = { senderId: keyIdSenderOf(decoded.payload) };
      } catch (error) {
        keyed = { error };
      }

      // The relay region, quarantined: read once, used only for hop-gap
      // bookkeeping, and never allowed to influence what follows.
      const hop = hopMonitor.observe(
        decoded.streamId,
        decoded.hopSequence,
        'error' in keyed ? undefined : keyed.senderId,
      );
      if (hop.undeclaredStreamId) this.#metrics.undeclaredStreamId();
      if (hop.missing > 0) this.#metrics.downlinkGapFrames(hop.missing);
      if (hop.reordered) this.#metrics.downlinkReorder();
      observedSlot = hop.undeclaredStreamId ? 'undeclared' : decoded.streamId;

      if ('error' in keyed) throw keyed.error;
      const { senderId } = keyed;
      observedSender = senderId;
      // Layer 1: the key id names this sender on this slot. Pre-verify.
      this.#verify?.keyed(observedSlot, senderId);

      // THE ONE RECEIVE POLICY (`receivePolicy.ts`), audio branch: resolve,
      // verify, slot-edge gate BEFORE open, open.
      const { opened, lane } = await receiveByKind({ decoded, senderId }, this.#gate, {
        roster: this.#roster,
        keys: this.#keys,
        cache: this.#cache,
        replay: this.#replay,
      });

      // The count is safe to expose where a per-frame SIZE would not be
      // (ADR-0036 §11's voice-activity trace) ONLY because the encoder runs with
      // DTX off — see `lifecycle/muteState.ts`.
      // Layer 2 (verified under this sender's roster key AND decrypted) and the
      // wrap outcome ride the ONE accept tail (`frameAccounting.ts`).
      this.#accounting.accepted(opened.wrapOutcome, observedSlot, senderId);
      this.#onAccepted?.({ senderId: Number(opened.senderId), plaintext: opened.plaintext });
      lane.decode({ data: opened.plaintext, timestampUs: 0 });
    } catch (err) {
      if (err instanceof FrameRejectedError) {
        // The token, verbatim. No mapping table, no bucket, no `other`: the
        // structural codec tokens stay individually visible, which is what keeps
        // `unknown_version` able to reveal a version-skewed rollback and keeps
        // `sum by(reason)` comparable with the media handler.
        this.#accounting.dropped(err.rejectReason, observedSlot, observedSender);
        return;
      }
      // Anything else is a defect rather than a wire condition, and it must not
      // end the read loop. It is deliberately NOT counted on the drop counter:
      // that counter's reasons are a closed vocabulary, and an uncounted defect
      // is visible as `received > accepted + sum(drops)` — a broken identity is
      // a better signal than a wrong label.
      throw err;
    }
  }
}
