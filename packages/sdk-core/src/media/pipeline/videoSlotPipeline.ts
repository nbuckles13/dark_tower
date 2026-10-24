// File: packages/sdk-core/src/media/pipeline/videoSlotPipeline.ts
//
// HOT PATH (per accepted frame and per decoded frame). No logging, no metric
// names, no label construction; metric handles arrive pre-bound.
//
// ---------------------------------------------------------------------------
// ONE VIDEO RECEIVE SLOT: JITTER BUFFER + ONE DECODER, FOR THE SLOT'S LIFETIME
// ---------------------------------------------------------------------------
//
// Exactly M of these exist (R-6), one per declared video slot, each built
// EAGERLY with one decoder — so decode resources are bounded by M and never by
// sender count. A NEW SOURCE ON THE SLOT REUSES IT: no new decoder and NO
// jitter-buffer flush. The old source's queued frames play out ahead of the new
// source's keyframe, and the new source's timeline is RE-BASED onto the slot's
// playout timeline so relative delay is unchanged.
//
// TIMESTAMPS (lead ruling E1). The published payload carries no presentation
// timestamp — ADR-0036 §2's "presentation timestamp inside the ciphertext" has
// no producer (C-A2 seals bare VP8; recorded in `docs/TODO.md`). So each whole
// frame is stamped with its ARRIVAL time on the injected clock, and the slot
// keeps ONE monotonic playout timeline: `timelineUs = arrivalUs + offsetUs`,
// where `offsetUs` is re-chosen at each source switch so the new source's
// first frame lands one nominal frame interval after the old source's last.
// Nothing sender-chosen sets timing.
//
// THE GROUP GATE (security Gate 1, item 3). The decoder is only ever fed one
// coherent group: a KEYFRAME (the SIGNED `independentlyDecodable` flag) of a
// newer group of the current sender, or of any other sender, starts a group; a
// delta frame is admitted only into the current group. So a new sender's
// deltas are never applied to another sender's reference picture — pixels
// derived from A are never rendered under B's attribution — and trailing frames
// of a superseded source are discarded. Keyed only on verified/decrypted data:
// the verified key-id sender, the signed flag and the signed `gop_counter`.
//
// RENDER ATTRIBUTION THROUGH THE ASYNC DECODER (security item 2). The sender of
// a decoded `VideoFrame` comes from the INPUT chunk that produced it — a FIFO
// filled at `decode()` and matched by timestamp at output — never from "the
// slot's current source". An output matching no queued input fails closed.
//
// FAULT ISOLATION. A decoder error closes ONLY this slot's decoder; the slot
// waits for a keyframe and replaces it lazily, at most once per
// `decoderRestartBackoffMs` (injected clock). No other slot and no audio lane
// is touched.
//
// RECOVERY (R-13). A slot that loses decodability raises ONE loss event per
// `(sender, gop_counter)` pair — the FIRST failure at that pair — with the
// fields from the FAILING frame's own header. State is the LAST reported pair
// only (security R2/S1: no monotonic clause, so an unverified huge counter
// cannot suppress a later genuine request). RESIDUAL, ACCEPTED: requests are
// bounded by groups received for honest input and by frames received under
// hostile input (amplification <= 1, at most one queued per slot); MH forwards
// only an exact counter match for the slot's current source.
//
// EVERY `VideoFrame` IS CLOSED EXACTLY ONCE: on replacement, on a guard or
// bound drop, on an unmatched output, on an avatar switch, and at stop.

import type { VideoReceiveConfig } from '../../config/clientConfig.js';
import type { SlotStateToken } from '../../signaling/events.js';
import type { MediaKindMetrics, MediaRecoverySource } from '../setup/mediaMetrics.js';
import type { VideoDecoderFactory, VideoDecoderSeam, VideoSink } from '../setup/seams.js';
import { buildVp8DecoderConfig, vp8IsKeyFrame, vp8KeyframeDimensions } from '../setup/vp8.js';
import { DecoderRestartGate } from './decoderRestartGate.js';
import type { FrameAccounting } from './frameAccounting.js';
import {
  RENDER_RULES,
  mayRender,
  sameRenderState,
  staticRenderState,
  type RenderGuards,
  type RenderRule,
  type VideoRenderState,
} from './renderDecision.js';

/** One accepted (verified, decrypted) frame for this slot. */
export interface SlotFrame {
  /** The VERIFIED key-id sender. */
  readonly senderId: number;
  /** The signed `gop_counter`. */
  readonly gopCounter: number;
  /** The signed `independentlyDecodable` flag. */
  readonly isKeyframe: boolean;
  /** Decrypted VP8 bytes. */
  readonly data: Uint8Array;
  /** The handler whose transport delivered it (a recovery request's target). */
  readonly url: string;
}

/** One loss event: the request's three fields, its target and its cause. */
export interface SlotLossEvent {
  readonly url: string;
  readonly slotId: number;
  readonly senderId: number;
  readonly gopCounter: number;
  readonly reason: MediaRecoverySource;
}

/** The unfulfilled-slot tracker as a slot drives it (`lifecycle/unfulfilledSlots.ts`). */
export interface UnfulfilledSink {
  enter(slotId: number): void;
  exit(slotId: number): void;
  isUnfulfilled(slotId: number): boolean;
}

/** Construction options for {@link VideoSlotPipeline}. */
export interface VideoSlotPipelineOptions {
  readonly slotId: number;
  readonly config: VideoReceiveConfig;
  readonly decoderRestartBackoffMs: number;
  readonly decoderFactory: VideoDecoderFactory;
  readonly metrics: MediaKindMetrics;
  /** The video kind's ONE accounting (shared with the stream ingress). */
  readonly accounting: FrameAccounting;
  readonly guards: RenderGuards;
  readonly clock: () => number;
  readonly unfulfilled: UnfulfilledSink;
  readonly onRenderChanged: (slotId: number, state: VideoRenderState) => void;
  readonly onLoss: (event: SlotLossEvent) => void;
  /** A frame DECODED (post-bounds) from verified `senderId` — decode activity. */
  readonly onDecoded: (senderId: number) => void;
  /** A frame was DRAWN; `activeForMs` is time since this ACTIVE entry, first draw only. */
  readonly onDrawn: (activeForMs: number | undefined) => void;
  /** This slot's decoder faulted. Non-fatal; the owner dedupes. */
  readonly onDecoderFault: () => void;
  /** The embedder's sink threw while drawing. Non-fatal; the owner dedupes. */
  readonly onSinkError: () => void;
}

interface Group {
  readonly senderId: number;
  readonly gopCounter: number;
}

interface Queued {
  readonly senderId: number;
  readonly gopCounter: number;
  readonly isKeyframe: boolean;
  readonly data: Uint8Array;
  readonly timelineUs: number;
  readonly arrivalMs: number;
  readonly url: string;
}

interface InFlight {
  readonly senderId: number;
  readonly gopCounter: number;
  readonly timelineUs: number;
  readonly url: string;
}

interface Held {
  readonly frame: VideoFrame;
  readonly senderId: number;
  readonly gopCounter: number;
}

const US_PER_MS = 1_000;
const RATE_WINDOW_MS = 1_000;

/** One video receive slot. See the module header. */
export class VideoSlotPipeline {
  readonly slotId: number;
  readonly #o: VideoSlotPipelineOptions;
  readonly #decoderConfig: VideoDecoderConfig;
  readonly #nominalIntervalUs: number;

  #decoder: VideoDecoderSeam | undefined;
  /** Incremented per decoder instance, so a late output or error from a discarded one is inert. */
  #instance = 0;
  readonly #restart: DecoderRestartGate;

  readonly #queue: Queued[] = [];
  #queuedBytes = 0;
  readonly #inFlight: InFlight[] = [];
  /** The group the decoder is (or will be) fed. */
  #group: Group | undefined;
  /** True until a keyframe starts a group, and after any loss of the current group. */
  #awaitingKeyframe = true;

  /** The sender whose arrival times the timeline currently follows. */
  #timelineSource: number | undefined;
  #timelineOffsetUs = 0;
  #lastTimelineUs: number | undefined;

  /** Output times of the last `decodeMaxFrameRateHz` decoded frames (rate bound). */
  readonly #outputTimes: number[] = [];

  #held: Held | undefined;
  /** The decoded group current at the last ACTIVE->avatar switch: its trailing frames are ignored. */
  #trailing: Group | undefined;
  #mcState: SlotStateToken | undefined;
  #render: VideoRenderState = { kind: 'none' };
  #lastRenderedSender: number | undefined;
  /** Frames this slot's decoder produced within bounds (cumulative; R-32 evidence, a count only). */
  #framesDecoded = 0;
  /** Post-accept LOCAL drops (the `decodeQueueDropped` sites), for the R-35 overlay. */
  #localDrops = 0;
  /** Accepted keyframes that reached this slot (R-35 overlay). A count only. */
  #keyframesSeen = 0;
  /** When this slot last turned ACTIVE, until its first draw. */
  #activeEnteredAtMs: number | undefined;

  /** The LAST reported loss pair (R2/S1). */
  #lastReported: Group | undefined;
  /** The last frame dropped `sender_server_muted` at arrival, for the unmute-edge repair. */
  #mutedDrop: (Group & { readonly url: string }) | undefined;

  #sink: VideoSink | undefined;
  #stopped = false;

  constructor(options: VideoSlotPipelineOptions) {
    this.#o = options;
    this.slotId = options.slotId;
    this.#restart = new DecoderRestartGate(options.decoderRestartBackoffMs, options.clock);
    // Configured FROM the slot's bounds, never from the bitstream (R-6).
    this.#decoderConfig = buildVp8DecoderConfig({
      width: options.config.decodeMaxWidth,
      height: options.config.decodeMaxHeight,
    });
    // DRY P6: derived from the existing rate bound; only needs to be > 0.
    this.#nominalIntervalUs = Math.ceil(1_000_000 / options.config.decodeMaxFrameRateHz);
    // EAGER: built with the slot, so exactly M decoders exist from the start.
    this.#createDecoder();
  }

  /** The current render state. */
  get renderState(): VideoRenderState {
    return this.#render;
  }

  /** MC's latest state for this slot, or `undefined` before any assignment named it. */
  get mcState(): SlotStateToken | undefined {
    return this.#mcState;
  }

  /** The VERIFIED sender of the last frame drawn, or `undefined` before any draw. */
  get lastRenderedSender(): number | undefined {
    return this.#lastRenderedSender;
  }

  /** Frames decoded within bounds so far (cumulative). A count only: no timing, no size. */
  get framesDecoded(): number {
    return this.#framesDecoded;
  }

  /** Post-accept local drops so far (cumulative). LOCAL R-35 overlay only. */
  get localDrops(): number {
    return this.#localDrops;
  }

  /** Accepted keyframes that reached this slot (cumulative). LOCAL R-35 overlay only. */
  get keyframesSeen(): number {
    return this.#keyframesSeen;
  }

  /** Frames held in the jitter buffer right now. LOCAL R-35 overlay only; sampled. */
  get jitterDepth(): number {
    return this.#queue.length;
  }

  /** The verified sender this slot is live with, for the pairing view. */
  get liveSender(): number | undefined {
    return this.#render.kind === 'active' ? this.#render.senderId : undefined;
  }

  /**
   * Attach (or detach with `undefined`) the embedder's sink. A live slot's held
   * frame is drawn at once — through the SAME never-render re-check as every
   * other draw path (security F1): a sender muted since its frame decoded is
   * never drawn on a re-attached sink.
   */
  setSink(sink: VideoSink | undefined): void {
    this.#sink = sink;
    const held = this.#held;
    if (!sink || this.#render.kind !== 'active' || !held || this.#stopped) return;
    if (!mayRender(held.senderId, this.#o.guards)) {
      this.#discardHeld();
      return;
    }
    this.#draw(held);
  }

  /** MC's latest state for this slot. `undefined`: the slot left the snapshot. */
  setMcState(token: SlotStateToken | undefined, participantSenderId: number | undefined): void {
    if (this.#stopped) return;
    const before = this.#rule();
    this.#mcState = token;
    const after = this.#rule();
    if (after === 'live') {
      if (before === 'live') return;
      this.#activeEnteredAtMs = this.#o.clock();
      // avatar->ACTIVE: show a held frame decoded while not live, if eligible;
      // otherwise KEEP showing what the slot showed (no spinner) and count it
      // unfulfilled until the first draw.
      if (this.#held && this.#eligible(this.#held.senderId, this.#held.gopCounter)) {
        if (mayRender(this.#held.senderId, this.#o.guards)) {
          this.#draw(this.#held);
          return;
        }
        this.#discardHeld();
      }
      this.#o.unfulfilled.enter(this.slotId);
      return;
    }
    if (before === 'live') {
      // ACTIVE->avatar (or any non-live): switch AT ONCE and ignore trailing
      // frames — the group decoded so far can never be drawn in this slot again.
      const last = this.#held;
      this.#trailing = last ? { senderId: last.senderId, gopCounter: last.gopCounter } : undefined;
      this.#discardHeld(false);
      this.#activeEnteredAtMs = undefined;
      this.#o.unfulfilled.exit(this.slotId);
    }
    this.#setRender(staticRenderState(token, participantSenderId) ?? { kind: 'none' });
  }

  /** Accept one verified, decrypted frame. */
  push(frame: SlotFrame): void {
    if (this.#stopped) return;
    if (frame.isKeyframe) this.#keyframesSeen += 1;
    // Drain (and expire) what is already queued FIRST, so a loss discovered now
    // gates THIS frame, and capacity is judged on what is actually waiting.
    this.#pump();
    if (frame.isKeyframe) {
      if (!this.#admitKeyframe(frame)) return;
    } else if (
      // A delta-FLAGGED frame whose bitstream is a VP8 key frame is a
      // flag/bitstream mismatch (security F2): it would reach the native decoder
      // as a 'delta' carrying an unchecked key-frame geometry. Discarded, as the
      // key-flagged-but-not-a-key-frame case is.
      vp8IsKeyFrame(frame.data) ||
      this.#awaitingKeyframe ||
      this.#group === undefined ||
      this.#group.senderId !== frame.senderId ||
      this.#group.gopCounter !== frame.gopCounter
    ) {
      // Not the group the decoder is fed: a new sender's delta before its
      // keyframe, a trailing frame of a superseded source, or a frame after a
      // loss. Never fed onto another group's reference picture.
      this.#localDrop();
      return;
    }

    const cfg = this.#o.config;
    if (
      this.#queue.length >= cfg.jitterMaxFrames ||
      this.#queuedBytes + frame.data.length > cfg.jitterMaxBytes
    ) {
      // Local overflow: this frame is lost, so its group is no longer decodable.
      this.#localDrop();
      this.#awaitingKeyframe = true;
      this.loss(frame.senderId, frame.gopCounter, 'local_drop', frame.url);
      return;
    }

    const arrivalMs = this.#o.clock();
    this.#queue.push({
      senderId: frame.senderId,
      gopCounter: frame.gopCounter,
      isKeyframe: frame.isKeyframe,
      data: frame.data,
      timelineUs: this.#timeline(frame.senderId, arrivalMs, frame.isKeyframe),
      arrivalMs,
      url: frame.url,
    });
    this.#queuedBytes += frame.data.length;
    this.#pump();
  }

  /**
   * A frame on this slot failed at `(senderId, gopCounter)` — its OWN header
   * values (possibly unverified: they feed only the request, security R1).
   * Raises one loss event per pair; repeats at the same pair send nothing.
   */
  loss(senderId: number, gopCounter: number, reason: MediaRecoverySource, url: string): void {
    if (this.#stopped) return;
    const last = this.#lastReported;
    if (last && last.senderId === senderId && last.gopCounter === gopCounter) return;
    this.#lastReported = { senderId, gopCounter };
    if (this.#group && this.#group.senderId === senderId && this.#group.gopCounter === gopCounter) {
      this.#awaitingKeyframe = true;
    }
    this.#o.onLoss({ url, slotId: this.slotId, senderId, gopCounter, reason });
    if (this.#rule() === 'live') this.#o.unfulfilled.enter(this.slotId);
  }

  /** A frame on this slot was dropped `sender_server_muted` at arrival (verified header). */
  recordMutedDrop(senderId: number, gopCounter: number, url: string): void {
    if (this.#stopped) return;
    this.#mutedDrop = { senderId, gopCounter, url };
  }

  /**
   * `senderId`'s video server mute was LIFTED. If this slot dropped that
   * sender's frame as muted, re-request it once (`server_mute_lifted`) — unless
   * a newer group from that sender has already been admitted.
   */
  serverMuteLifted(senderId: number): void {
    const dropped = this.#mutedDrop;
    if (this.#stopped || !dropped || dropped.senderId !== senderId) return;
    this.#mutedDrop = undefined;
    const group = this.#group;
    if (group && group.senderId === senderId && group.gopCounter > dropped.gopCounter) return;
    this.loss(senderId, dropped.gopCounter, 'server_mute_lifted', dropped.url);
  }

  /** Release everything. Idempotent. */
  stop(): void {
    if (this.#stopped) return;
    this.#stopped = true;
    this.#instance += 1;
    this.#decoder?.close();
    this.#decoder = undefined;
    this.#discardHeld(false);
    this.#queue.length = 0;
    this.#queuedBytes = 0;
    this.#inFlight.length = 0;
    this.#mutedDrop = undefined;
    this.#sink = undefined;
  }

  // ----------------------------------------------------------------------

  #rule(): RenderRule {
    return this.#mcState === undefined ? 'none' : RENDER_RULES[this.#mcState];
  }

  /** The keyframe half of the group gate. `false`: discarded (already counted). */
  #admitKeyframe(frame: SlotFrame): boolean {
    const group = this.#group;
    if (group && group.senderId === frame.senderId && frame.gopCounter <= group.gopCounter) {
      // An older (or repeated) group of the current sender: trailing.
      this.#localDrop();
      return false;
    }
    // Pre-decode VP8 header check (security item 5): an over-bound keyframe
    // never reaches the native decoder. Bounds-checked; a short or malformed
    // header is a typed discard.
    const dims = vp8KeyframeDimensions(frame.data);
    if (!dims.ok) {
      this.#localDrop();
      return false;
    }
    if (
      dims.width > this.#o.config.decodeMaxWidth ||
      dims.height > this.#o.config.decodeMaxHeight
    ) {
      this.#countOverBound(frame.senderId);
      return false;
    }
    if (this.#mutedDrop && this.#mutedDrop.senderId !== frame.senderId) {
      // The slot's source changed: a stored muted drop is no longer this slot's to repair.
      this.#mutedDrop = undefined;
    }
    this.#group = { senderId: frame.senderId, gopCounter: frame.gopCounter };
    this.#awaitingKeyframe = false;
    return true;
  }

  /**
   * The playout timestamp for a frame arriving at `arrivalMs`. A keyframe from a
   * sender other than the timeline's current source RE-BASES the timeline so
   * the new source continues one nominal interval after the old source's last
   * frame; within a source, arrival spacing is preserved. Strictly monotonic.
   */
  #timeline(senderId: number, arrivalMs: number, isKeyframe: boolean): number {
    const arrivalUs = arrivalMs * US_PER_MS;
    if (isKeyframe && senderId !== this.#timelineSource) {
      this.#timelineSource = senderId;
      this.#timelineOffsetUs =
        this.#lastTimelineUs === undefined
          ? 0
          : this.#lastTimelineUs + this.#nominalIntervalUs - arrivalUs;
    }
    let t = arrivalUs + this.#timelineOffsetUs;
    if (this.#lastTimelineUs !== undefined && t <= this.#lastTimelineUs) {
      t = this.#lastTimelineUs + 1;
    }
    this.#lastTimelineUs = t;
    return t;
  }

  /** Feed the decoder from the jitter buffer while it has room. Event-driven; no timer. */
  #pump(): void {
    const cfg = this.#o.config;
    while (this.#queue.length > 0 && !this.#stopped) {
      const head = this.#queue[0] as Queued;
      if (this.#o.clock() - head.arrivalMs > cfg.jitterMaxHoldMs) {
        this.#expireGroupOf(head);
        continue;
      }
      if (!this.#decoder) {
        if (!head.isKeyframe) {
          this.#dequeue();
          this.#localDrop();
          continue;
        }
        if (!this.#restart.mayRestart()) return;
        this.#createDecoder();
        if (!this.#decoder) return;
      }
      const decoder = this.#decoder as VideoDecoderSeam;
      if (decoder.decodeQueueSize >= cfg.decoderQueueTarget) return;
      this.#dequeue();
      this.#inFlight.push({
        senderId: head.senderId,
        gopCounter: head.gopCounter,
        timelineUs: head.timelineUs,
        url: head.url,
      });
      try {
        decoder.decode({
          data: head.data,
          timestampUs: head.timelineUs,
          isKeyframe: head.isKeyframe,
        });
      } catch {
        // WebCodecs `decode()` throws SYNCHRONOUSLY when the codec already
        // closed itself (InvalidStateError) or wants a key frame (DataError).
        // That is THIS slot's decoder fault: never an exception escaping into
        // the stream reader (whose remaining frames would go uncounted) or into
        // a WebCodecs output callback.
        this.#onDecoderError(this.#instance);
        return;
      }
    }
  }

  /**
   * ONE post-accept local drop: the production counter and the overlay's plain
   * field, adjacent, so they cannot drift (the `frameAccounting.ts` rule).
   */
  #localDrop(): void {
    this.#o.metrics.decodeQueueDropped();
    this.#localDrops += 1;
  }

  #dequeue(): Queued | undefined {
    const head = this.#queue.shift();
    if (head) this.#queuedBytes -= head.data.length;
    return head;
  }

  /** `head` was held past `jitterMaxHoldMs`: discard its whole queued group (local drop). */
  #expireGroupOf(head: Queued): void {
    for (let i = this.#queue.length - 1; i >= 0; i -= 1) {
      const q = this.#queue[i] as Queued;
      if (q.senderId === head.senderId && q.gopCounter === head.gopCounter) {
        this.#queue.splice(i, 1);
        this.#queuedBytes -= q.data.length;
        this.#localDrop();
      }
    }
    this.loss(head.senderId, head.gopCounter, 'local_drop', head.url);
  }

  #createDecoder(): void {
    const instance = (this.#instance += 1);
    try {
      this.#decoder = this.#o.decoderFactory({
        config: this.#decoderConfig,
        onOutput: (frame) => this.#onOutput(instance, frame),
        onError: () => this.#onDecoderError(instance),
      });
    } catch {
      this.#decoder = undefined;
      this.#onDecoderError(instance);
    }
  }

  /** THIS slot's decoder failed: replace only it; the slot waits for a keyframe. */
  #onDecoderError(instance: number): void {
    if (instance !== this.#instance || this.#stopped) return;
    this.#instance += 1;
    this.#o.metrics.decoderError();
    this.#decoder?.close();
    this.#decoder = undefined;
    this.#restart.fault();
    const failing = this.#inFlight[0];
    this.#inFlight.length = 0;
    // Queued frames before the first queued keyframe can no longer decode.
    while (this.#queue.length > 0 && !(this.#queue[0] as Queued).isKeyframe) {
      this.#dequeue();
      this.#localDrop();
    }
    if (this.#queue.length === 0) this.#awaitingKeyframe = true;
    this.#o.onDecoderFault();
    if (failing) this.loss(failing.senderId, failing.gopCounter, 'decoder_error', failing.url);
    this.#pump();
  }

  #onOutput(instance: number, frame: VideoFrame): void {
    if (instance !== this.#instance || this.#stopped) {
      frame.close();
      return;
    }
    const at = this.#inFlight.findIndex((m) => m.timelineUs === frame.timestamp);
    if (at < 0) {
      // An output matching no input this slot submitted (or out of order): its
      // attribution is unknown, so it fails closed.
      frame.close();
      this.#localDrop();
      this.#pump();
      return;
    }
    const meta = this.#inFlight[at] as InFlight;
    // Inputs ahead of the match produced no output (a decoder may skip).
    this.#inFlight.splice(0, at + 1);
    if (this.#outputExceedsBounds(frame)) {
      frame.close();
      this.#countOverBound(meta.senderId);
      this.#pump();
      return;
    }
    this.#framesDecoded += 1;
    this.#o.onDecoded(meta.senderId);
    this.#offer(frame, meta);
    this.#pump();
  }

  /** Whether decoded output is above the configured geometry or rate bound (R-6). */
  #outputExceedsBounds(frame: VideoFrame): boolean {
    const cfg = this.#o.config;
    if (frame.codedWidth > cfg.decodeMaxWidth || frame.codedHeight > cfg.decodeMaxHeight) {
      return true;
    }
    const now = this.#o.clock();
    const times = this.#outputTimes;
    if (times.length >= cfg.decodeMaxFrameRateHz) {
      if (now - (times[0] as number) < RATE_WINDOW_MS) return true;
      times.shift();
    }
    times.push(now);
    return false;
  }

  #countOverBound(senderId: number): void {
    // A frame drop reason, after accept: the ONE stated exception to the
    // parse-boundary identity (`VIDEO_POST_DECODE_REJECT_REASONS`).
    this.#o.accounting.dropped('decode_over_bound', this.slotId, senderId);
  }

  /** Whether a decoded frame of `(senderId, gop)` is not a trailing frame of a switched-away group. */
  #eligible(senderId: number, gopCounter: number): boolean {
    const t = this.#trailing;
    return !(t && t.senderId === senderId && gopCounter <= t.gopCounter);
  }

  /** A decoded frame: hold it (decode-before-live) and draw it if the slot is live. */
  #offer(frame: VideoFrame, meta: InFlight): void {
    if (!this.#eligible(meta.senderId, meta.gopCounter)) {
      frame.close();
      this.#localDrop();
      return;
    }
    const held: Held = { frame, senderId: meta.senderId, gopCounter: meta.gopCounter };
    if (this.#rule() === 'live') {
      if (!mayRender(meta.senderId, this.#o.guards)) {
        // The never-render re-check (a sender muted after its frame arrived).
        frame.close();
        this.#localDrop();
        return;
      }
      this.#replaceHeld(held);
      this.#draw(held);
      return;
    }
    // Not live: hold the latest decoded frame so it is ready when the slot turns ACTIVE.
    this.#replaceHeld(held);
  }

  #replaceHeld(held: Held): void {
    const previous = this.#held;
    this.#held = held;
    if (previous && previous.frame !== held.frame) previous.frame.close();
  }

  /** Close the held frame. `count`: it was refused by the never-render re-check. */
  #discardHeld(count = true): void {
    const held = this.#held;
    this.#held = undefined;
    if (!held) return;
    held.frame.close();
    if (count) this.#localDrop();
  }

  /**
   * The slot shows `held` (its render state goes live on the verified sender).
   * It counts as RENDERED — exiting unfulfilled, observing the first-render
   * latencies, becoming `lastRenderedSender` — only when it was DELIVERED to an
   * attached `VideoSink` without throwing (observability F2). No sink attached:
   * not rendered yet (`setSink` delivers the held frame on attach). A throwing
   * sink: not rendered, and a sink fault is raised.
   */
  #draw(held: Held): void {
    this.#setRender({ kind: 'active', senderId: held.senderId });
    if (!this.#drawToSink(held.frame)) return;
    this.#lastRenderedSender = held.senderId;
    this.#o.unfulfilled.exit(this.slotId);
    const entered = this.#activeEnteredAtMs;
    this.#activeEnteredAtMs = undefined;
    this.#o.onDrawn(entered === undefined ? undefined : this.#o.clock() - entered);
  }

  /** Deliver to the sink. `true` only when a sink took the frame without throwing. */
  #drawToSink(frame: VideoFrame): boolean {
    const sink = this.#sink;
    if (!sink) return false;
    try {
      sink.drawFrame(frame);
      return true;
    } catch {
      // The embedder's sink failed. Ownership is unchanged (the SDK still
      // closes the frame); nothing here may log a platform message — the owner
      // raises a fixed-text fault.
      this.#o.onSinkError();
      return false;
    }
  }

  #setRender(state: VideoRenderState): void {
    if (sameRenderState(this.#render, state)) return;
    this.#render = state;
    this.#o.onRenderChanged(this.slotId, state);
  }
}
