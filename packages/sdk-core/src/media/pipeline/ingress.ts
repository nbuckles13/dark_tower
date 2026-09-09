// File: packages/sdk-core/src/media/pipeline/ingress.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. Metric handles arrive pre-bound from `../setup/mediaMetrics.ts`.
//
// One QUIC datagram -> decode -> resolve key -> verify -> replay -> unwrap ->
// decrypt -> Opus decode -> playback.
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
// NOTE FOR THE VIDEO STORY: this counting point must move from the transport
// boundary to the PARSE boundary once several frames share one stream, and the
// identity must be re-established there.
//
// ---------------------------------------------------------------------------
// THERE IS NO SELF-TRUST BRANCH
// ---------------------------------------------------------------------------
//
// In this story the frames arriving are this client's own, returned by MH. The
// pipeline does not know that and must not: the loopback is MC's assignment and
// MH's forwarding. Every frame runs the identical resolve-then-verify path a
// peer's frame takes, and there is no branch anywhere that accepts a frame
// because this client signed it.
//
// The identity key is resolved from `key_id.sender_id` AND NOTHING ELSE — never
// from the slot assignment, never from the relay `stream_id`, never from a
// "current speaker" cache. `RosterIdentityKeys` has no method that takes any of
// those, so the wrong lookup is not expressible.
//
// ---------------------------------------------------------------------------
// THE RELAY REGION IS UNAUTHENTICATED AND IS QUARANTINED TO ONE CONSUMER
// ---------------------------------------------------------------------------
//
// `stream_id` and `hop_sequence` are written by MH and signed by nobody. They
// reach exactly one place — the hop-sequence monitor, which creates no state for
// an undeclared `stream_id` — and they never reach key selection, roster lookup,
// attribution, the replay window, or any drop-or-play decision.

import { decodeFrame } from '../frame/frameCodec.js';
import { parseSframe } from '../frame/sframe.js';
import { unpackKeyId } from '../frame/keyId.js';
import {
  openVerifiedFrame,
  verifyFrame,
  type ReceiverKeys,
  type ReplayWindow,
  type TransmitKeyCache,
} from '../frame/receivePath.js';
import { FrameRejectedError } from '../frame/rejectReason.js';
import type { RejectReason } from '../frame/rejectReason.js';
import type { MediaMetrics } from '../setup/mediaMetrics.js';
import type { AudioDecoderSeam } from '../setup/seams.js';
import type { FirstMediaObserver } from '../setup/measurement.js';
import type { HopSequenceMonitor } from './hopSequenceMonitor.js';

/** Resolves a verification key from a frame's own `key_id.sender_id`. */
export interface IdentityKeyResolver {
  /** `undefined` when this client holds no usable key for `senderId`. */
  identityKeyFor(senderId: number): CryptoKey | undefined;
}

/** A frame that completed the receive path. Attribution comes from the key id. */
export interface AcceptedFrame {
  /** The sender, derived from `key_id.sender_id` and nothing else. */
  readonly senderId: number;
  /** Decrypted Opus payload. */
  readonly plaintext: Uint8Array;
}

/** Construction options for {@link IngressPipeline}. */
export interface IngressPipelineOptions {
  readonly metrics: MediaMetrics;
  readonly roster: IdentityKeyResolver;
  readonly keys: ReceiverKeys;
  readonly cache: TransmitKeyCache;
  readonly replay: ReplayWindow;
  readonly hopMonitor: HopSequenceMonitor;
  readonly firstMedia: FirstMediaObserver;
  /** Called for every accepted frame, before the decoder is fed. */
  readonly onAccepted?: (frame: AcceptedFrame) => void;
}

/**
 * Drives one datagram through the receive path.
 *
 * The decoder is attached separately so a decoder fault can detach and replace
 * it without rebuilding the pipeline's receiver state — the replay window and
 * the transmit-key cache must survive a decoder restart, or a restart would
 * reopen the replay surface.
 */
export class IngressPipeline {
  readonly #metrics: MediaMetrics;
  readonly #roster: IdentityKeyResolver;
  readonly #keys: ReceiverKeys;
  readonly #cache: TransmitKeyCache;
  readonly #replay: ReplayWindow;
  readonly #hopMonitor: HopSequenceMonitor;
  readonly #firstMedia: FirstMediaObserver;
  readonly #onAccepted: ((frame: AcceptedFrame) => void) | undefined;

  #decoder: AudioDecoderSeam | undefined;
  /**
   * Frames that completed the receive path, since construction. Monotone.
   *
   * A plain field read, sampled by an embedder — never a per-frame emission.
   * Bumped at the single site beside `MediaMetrics.frameAccepted()`.
   */
  #framesAccepted = 0;
  /**
   * Datagrams that arrived, since construction. Monotone.
   *
   * The LEFT-HAND side of `received = accepted + sum(drops by reason)`. Without
   * it a reader cannot tell "nothing is arriving" from "arriving and being
   * rejected" — two states with opposite remediations (a server forwarding
   * failure vs a client key-attribution bug) that look identical when only
   * `accepted` is visible. That ambiguity cost a live misdiagnosis at this
   * task's Gate 2.
   */
  #framesReceived = 0;
  /** Frames rejected for any wire reason. The `sum(drops)` term. Monotone. */
  #framesDropped = 0;
  /**
   * The most recent reject token, or `undefined` if nothing has been dropped.
   *
   * Safe to expose and to project, and **the TYPE is what makes that true** —
   * not this comment. `RejectReason` is the frozen sixteen-token union whose
   * SSoT is `proto/test-vectors/frame-v2.vectors.json`, so the field is bounded
   * by construction and carries no participant, key or payload information.
   *
   * Typed as the union rather than `string` deliberately: a bounded telemetry
   * token is always one refactor away from becoming a metric label or a log
   * field, and `string` is the annotation that lets a `DOMException.message` or
   * an interpolated identifier land here and compile. That is the journey
   * ADR-0036 §11 and `label-taxonomy.md` R2 exist to block. Same discipline as
   * `MediaMetrics.frameDropped(reason: RejectReason)` beside the call sites.
   */
  #lastDropReason: RejectReason | undefined;

  constructor(options: IngressPipelineOptions) {
    this.#metrics = options.metrics;
    this.#roster = options.roster;
    this.#keys = options.keys;
    this.#cache = options.cache;
    this.#replay = options.replay;
    this.#hopMonitor = options.hopMonitor;
    this.#firstMedia = options.firstMedia;
    this.#onAccepted = options.onAccepted;
  }

  /**
   * Frames accepted since construction (monotone).
   *
   * Named `Accepted`, not `Received`, and the distinction is the one
   * `MediaMetrics.frameAccepted` already draws: a received frame may be dropped
   * for a wire reason, so `received = accepted + sum(drops by reason)`. This
   * counts the left-hand side's *accepted* term, so it moves only for a frame
   * that verified, decrypted and reached the decoder.
   */
  get framesAccepted(): number {
    return this.#framesAccepted;
  }

  /** Datagrams that arrived at the wire (monotone). The identity's left side. */
  get framesReceived(): number {
    return this.#framesReceived;
  }

  /** Frames rejected for a wire reason (monotone). The `sum(drops)` term. */
  get framesDropped(): number {
    return this.#framesDropped;
  }

  /** The most recent reject token; `undefined` before the first drop. Bounded by type. */
  get lastDropReason(): RejectReason | undefined {
    return this.#lastDropReason;
  }

  /** Attach (or detach, with `undefined`) the audio decoder. */
  setDecoder(decoder: AudioDecoderSeam | undefined): void {
    this.#decoder = decoder;
  }

  /**
   * Process one datagram.
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
  async accept(datagram: Uint8Array): Promise<void> {
    // AT THE WIRE. First statement, before any parse.
    // ONE INCREMENT, TWO READERS — same rule as the accepted and dropped pairs
    // below. Kept adjacent so the getter and
    // `dt_client_media_frames_received_total` cannot drift apart.
    this.#metrics.frameReceived();
    this.#framesReceived += 1;
    this.#firstMedia.onFrameReceived();

    try {
      // `decodeFrame` bounds `payload_length` against the wire maximum BEFORE
      // slicing and returns slices rather than copies, so a reject costs no more
      // than a parse and no allocation is sized from an attacker-controlled
      // field. That is why there is no separate pre-parse byte cap here: adding
      // one would need a reject token outside the frozen sixteen.
      const decoded = decodeFrame(datagram);

      // The relay region, quarantined: read once, used only for hop-gap
      // bookkeeping, and never allowed to influence what follows.
      const hop = this.#hopMonitor.observe(decoded.streamId, decoded.hopSequence);
      if (hop.undeclaredStreamId) this.#metrics.undeclaredStreamId();
      if (hop.missing > 0) this.#metrics.downlinkGapFrames(hop.missing);
      if (hop.reordered) this.#metrics.downlinkReorder();

      // The key id lives in the SFrame clear header, so it is readable before
      // decryption — which is the point: it SELECTS the key.
      const sframe = parseSframe(decoded.payload);
      const parts = unpackKeyId(sframe.keyId);
      const senderId = Number(parts.senderId);

      const identityKey = this.#roster.identityKeyFor(senderId);
      if (!identityKey) {
        // FAIL CLOSED. `undefined` covers unknown participant, participant with
        // an EMPTY published key, and participant with an unusable key — all
        // three mean this frame cannot be verified. It is never "no key, so skip
        // verification": that reading fails open and is the natural one, which
        // is exactly why it is refused here in one place rather than guarded at
        // each call site.
        // ONE INCREMENT, TWO READERS. Both drop sites must record, or the
        // identity `received = accepted + sum(drops)` silently stops holding for
        // a reader of these getters — and a half-instrumented reason field is
        // worse than none, because it reads as "no drops of that kind".
        this.#metrics.frameDropped('no_roster_entry');
        this.#framesDropped += 1;
        this.#lastDropReason = 'no_roster_entry';
        return;
      }

      // VERIFY BEFORE DECRYPT, structurally: `openVerifiedFrame` below is
      // unreachable without the brand `verifyFrame` applies.
      const verified = await verifyFrame(decoded, identityKey);
      const opened = await openVerifiedFrame(verified, this.#cache, this.#keys, this.#replay);

      // ---------------------------------------------------------------------
      // REPORTED BY EXCLUSION. DO NOT "SIMPLIFY" THIS BACK TO NAMING THE TWO
      // VALUES — it reads like the same thing and is not.
      // ---------------------------------------------------------------------
      //
      // What is spelled here are the three UNINTERESTING outcomes; the two that
      // are counted are whatever remains of the `WrapOutcome` union, and
      // TypeScript narrows `outcome` to exactly those.
      //
      // The property this buys is not "no hardcoded string" — it is that
      // **`wrap_key_id_mismatch`'s pending rename will not have to find this
      // call site at all.** That token names a cause the receiver cannot
      // substantiate (the wrap's bound key id is nowhere on the wire, so a
      // mis-bound wrap and a wrong KEK are one indistinguishable GCM tag
      // mismatch), and a protocol-owned rename across the vectors, the
      // generator and both language sides is tracked in `docs/TODO.md`. A
      // consumer that named the value would be one more site for that change to
      // locate and move; by exclusion there is no site. That is the difference
      // between a constraint satisfied and a constraint made unnecessary, and it
      // is lost the moment someone rewrites this as an equality check on the two
      // names — silently, with the suite still green.
      //
      // Both reported outcomes are NON-DROPPING: the frame was accepted and
      // plays. Counting either as a drop would break
      // `received = accepted + sum(drops by reason)` for a frame that was not
      // dropped.
      const outcome = opened.wrapOutcome;
      if (outcome !== 'absent' && outcome !== 'cached' && outcome !== 'already_held') {
        this.#metrics.wrapOutcome(outcome);
      }

      // ONE INCREMENT, TWO READERS. `#framesAccepted` and the counter are two
      // statements encoding one fact, and they are kept adjacent deliberately: a
      // `++` placed anywhere else in this function would agree today and drift
      // the first time a drop path lands between the two sites, at which point
      // the getter and `dt_client_media_frames_accepted_total` disagree and
      // nothing fails. Do not separate them.
      //
      // The count is safe to expose where a per-frame SIZE would not be
      // (ADR-0036 §11's voice-activity trace) ONLY because the encoder runs with
      // DTX off — see `lifecycle/muteState.ts`, "with DTX, absence of frames
      // becomes a signal". Enabling DTX makes the frame RATE speech-dependent
      // and turns both this getter and the production counter into that trace.
      this.#metrics.frameAccepted();
      this.#framesAccepted += 1;
      this.#onAccepted?.({ senderId: Number(opened.senderId), plaintext: opened.plaintext });
      this.#decoder?.decode({ data: opened.plaintext, timestampUs: 0 });
    } catch (err) {
      if (err instanceof FrameRejectedError) {
        // The token, verbatim. No mapping table, no bucket, no `other`: the
        // eight structural codec tokens stay individually visible, which is what
        // keeps `unknown_version` able to reveal a version-skewed rollback and
        // keeps `sum by(reason)` comparable with the media handler.
        this.#metrics.frameDropped(err.rejectReason);
        this.#framesDropped += 1;
        // The verbatim token, from the frozen sixteen. No mapping table here
        // either — see the comment above this catch.
        this.#lastDropReason = err.rejectReason;
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
