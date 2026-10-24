// File: packages/sdk-core/src/media/pipeline/videoIngress.ts
//
// HOT PATH. Per-frame code only: no logging, no metric names, no label
// construction. Metric handles arrive pre-bound.
//
// ---------------------------------------------------------------------------
// ONE INCOMING GROUP STREAM -> WHOLE FRAMES -> THE ONE POLICY -> ITS SLOT
// ---------------------------------------------------------------------------
//
//   chunks -> UniStreamFrameReader (whole frames, MAX checked before alloc)
//          -> decodeFrame -> slot binding (relay id) -> receiveByKind('video')
//          -> the slot's pipeline (jitter buffer + decoder + render decision)
//
// COUNTED AT THE PARSE BOUNDARY: a frame is received at its first byte, and
// every received frame ends exactly once as accepted or as one reject reason,
// so `received = accepted + sum(drops{reason not in
// VIDEO_POST_DECODE_REJECT_REASONS})` holds under every drop path.
//
// SLOT BINDING (lead ruling E2). Each downlink group stream belongs to exactly
// one of this subscriber's slots, and its frames carry relay `stream_id` = that
// slot id. The FIRST frame binds the stream: a `stream_id` that is not one of
// this client's DECLARED VIDEO slots (an undeclared id, or an AUDIO slot id —
// the namespaces share one id space) is `stream_slot_mismatch`, the existing
// `undeclared_stream_id` diagnostic is bumped beside it, and the stream is
// aborted. Any later frame naming a different slot is `stream_slot_mismatch`
// and aborts the stream too. The relay id SELECTS the pipeline; it never
// reaches attribution, which is the VERIFIED key-id sender everywhere.
//
// THE MEDIA KIND IS THE TRANSPORT'S. Everything arriving here is video, by
// construction: group streams feed video pipelines only, datagrams feed audio
// lanes only, and nothing reads a kind from the frame.
//
// RECOVERY (R-13). Every drop maps to a recovery reason or to none through ONE
// exhaustive table, {@link RECOVERY_REASON_FOR}; the request's fields are the
// FAILING frame's own header values (security R1: they feed the request only).
// No sender readable means no request (security R3).

import { decodeFrame, type DecodedFrame } from '../frame/frameCodec.js';
import { keyIdSenderOf } from '../frame/sframe.js';
import { FrameRejectedError, type RejectReason } from '../frame/rejectReason.js';
import type { MediaKindMetrics, MediaRecoverySource } from '../setup/mediaMetrics.js';
import { MEDIA_RECOVERY_SOURCES } from '../setup/mediaMetrics.js';
import type { FirstMediaObserver } from '../setup/measurement.js';
import type { FrameAccounting } from './frameAccounting.js';
import type { ObservedSlot, ReceiveVerificationRecorder } from './receiveVerification.js';
import { receiveByKind, type ReceiveContext, type ReceiveGate } from './receivePolicy.js';
import {
  UniStreamFrameReader,
  type FrameAllocator,
  type StreamFrameHeader,
} from './streamReader.js';
import type { VideoSlotPipeline } from './videoSlotPipeline.js';

const { VerifyFailed, DecryptFailed } = MEDIA_RECOVERY_SOURCES;

/**
 * Which reject reasons are a slot's LOSS of decodability, and the recovery
 * reason each requests (story 3 R-13; security Gate 1 table). EXHAUSTIVE over
 * `RejectReason`, so a new token fails to compile until it is classified.
 *
 *   * `null` — no request: an expected drop (replay, stale tail), a drop by
 *     policy (self; a server-muted sender is repaired at the unmute edge
 *     instead, `server_mute_lifted`), a stream-level fault, a prefix reject (no
 *     header to name), or a non-dropping outcome.
 *   * Roster, KEK and key-bearing LAG on a new source's keyframe (no roster
 *     entry, no KEK, no transmit key) DO request: with no keyframe interval the
 *     slot would otherwise stay undecodable.
 */
export const RECOVERY_REASON_FOR: Readonly<Record<RejectReason, MediaRecoverySource | null>> = {
  unknown_version: null,
  reserved_flag_bit_set: null,
  key_bearing_discardable: null,
  payload_length_exceeds_max: null,
  extensions_too_large: null,
  payload_length_exceeds_available: VerifyFailed,
  truncated: VerifyFailed,
  extensions_malformed: VerifyFailed,
  trailing_bytes: VerifyFailed,
  no_transmit_key: DecryptFailed,
  decode_over_bound: null,
  stream_partial_reset: null,
  stream_slot_mismatch: null,
  signature_invalid: VerifyFailed,
  decrypt_failed: DecryptFailed,
  unwrap_failed: DecryptFailed,
  replay_detected: null,
  wrap_key_id_mismatch: null,
  no_kek_for_generation: DecryptFailed,
  no_roster_entry: VerifyFailed,
  kek_generation_stale: null,
  sender_not_assigned: null,
  self_sender: null,
  sender_server_muted: null,
};

/** Construction options for {@link VideoIngress}. */
export interface VideoIngressOptions {
  readonly metrics: MediaKindMetrics;
  readonly accounting: FrameAccounting;
  readonly ctx: ReceiveContext;
  readonly gate: Extract<ReceiveGate, { kind: 'video' }>;
  /** The pipeline for a DECLARED VIDEO slot id, or `undefined` for any other id. */
  readonly slotFor: (slotId: number) => VideoSlotPipeline | undefined;
  readonly firstMedia: FirstMediaObserver;
  readonly isStopped: () => boolean;
  /** Test-only receive verification (story 2 R-30, extended to video). Absent in production. */
  readonly verification?: ReceiveVerificationRecorder;
  /** The reader's allocation seam (tests prove MAX is checked before allocation). */
  readonly alloc?: FrameAllocator;
}

/** One reader outcome, in stream order. */
type ReaderEvent =
  | {
      readonly kind: 'frame';
      readonly bytes: Uint8Array;
      readonly header: StreamFrameHeader | undefined;
    }
  | { readonly kind: 'reject'; readonly reason: RejectReason };

interface StreamState {
  readonly url: string;
  slot: VideoSlotPipeline | undefined;
  slotId: number | undefined;
  abort: boolean;
}

/** The video ingress: one call to {@link readStream} per incoming group stream. */
export class VideoIngress {
  readonly #o: VideoIngressOptions;
  /** Readers of the streams being read now, so teardown can cancel them. */
  readonly #readers = new Set<ReadableStreamDefaultReader<Uint8Array>>();

  /** Every stream read in progress, so a caller can await quiescence. */
  readonly #reads = new Set<Promise<void>>();

  constructor(options: VideoIngressOptions) {
    this.#o = options;
  }

  /**
   * Resolves once every stream read in progress has ENDED (its stream finished,
   * reset or was aborted). For deterministic tests and orderly teardown; a read
   * waiting on a live stream does not settle until that stream ends.
   */
  async settled(): Promise<void> {
    while (this.#reads.size > 0) await Promise.allSettled([...this.#reads]);
  }

  /** Streams being read now. Bounded by the owner's concurrency cap. */
  get openStreams(): number {
    return this.#readers.size;
  }

  /** Teardown: cancel every stream being read. Idempotent. */
  cancelAll(): void {
    for (const reader of this.#readers) {
      void reader.cancel().catch(() => {
        // Already ended; the transport close is what matters.
      });
    }
    this.#readers.clear();
  }

  /**
   * Read one incoming group stream to its end. NEVER throws on a wire
   * condition or a stream reset: every frame is a counted outcome. A defect
   * (anything not a `FrameRejectedError`) propagates to the caller.
   */
  readStream(readable: ReadableStream<Uint8Array>, url: string): Promise<void> {
    const read = this.#read(readable, url);
    this.#reads.add(read);
    const forget = (): void => {
      this.#reads.delete(read);
    };
    read.then(forget, forget);
    return read;
  }

  async #read(readable: ReadableStream<Uint8Array>, url: string): Promise<void> {
    const reader = readable.getReader();
    this.#readers.add(reader);
    const state: StreamState = { url, slot: undefined, slotId: undefined, abort: false };
    // The reader's outcomes, IN STREAM ORDER: whole frames and a prefix reject.
    const events: ReaderEvent[] = [];
    const frameReader = new UniStreamFrameReader(
      {
        onFrameStart: () => {
          this.#o.accounting.received();
          this.#o.firstMedia.onFrameReceived();
        },
        onFrame: (bytes, header) => events.push({ kind: 'frame', bytes, header }),
        onPrefixReject: (reason) => events.push({ kind: 'reject', reason }),
      },
      this.#o.alloc,
    );
    let reset = false;
    try {
      for (;;) {
        let chunk: ReadableStreamReadResult<Uint8Array>;
        try {
          chunk = await reader.read();
        } catch {
          reset = true;
          break;
        }
        if (chunk.done) break;
        frameReader.push(chunk.value);
        // Processed IN ORDER, each to completion, before the next read: one
        // stream's frames never interleave with each other.
        while (events.length > 0 && !state.abort) {
          const next = events.shift() as ReaderEvent;
          if (next.kind === 'reject') {
            // Framing is lost: no later boundary can be trusted.
            this.#drop(state, next.reason);
            state.abort = true;
            break;
          }
          await this.#frame(state, next.bytes, next.header);
          if (this.#o.isStopped()) return;
        }
        if (state.abort) {
          this.#abortRemaining(state, events, frameReader);
          void reader.cancel().catch(() => {});
          return;
        }
      }
      const ended = frameReader.end();
      if (this.#o.isStopped()) return;
      if (ended.partial) this.#drop(state, 'stream_partial_reset');
      if (reset) this.#groupAbandoned(state, frameReader.lastHeader);
    } finally {
      this.#readers.delete(reader);
      try {
        reader.releaseLock();
      } catch {
        // Already released.
      }
    }
  }

  /**
   * The stream named the wrong slot: frames ALREADY READ behind the offending
   * one (whole, or started) are dropped with the same token so every received
   * frame still ends in exactly one outcome. Bytes never read are not frames
   * and are not counted. (After a prefix reject the reader has stopped, so
   * nothing follows.)
   */
  #abortRemaining(
    state: StreamState,
    events: ReaderEvent[],
    frameReader: UniStreamFrameReader,
  ): void {
    for (const event of events.splice(0)) {
      if (event.kind === 'frame') this.#drop(state, 'stream_slot_mismatch');
    }
    if (frameReader.end().partial) this.#drop(state, 'stream_slot_mismatch');
  }

  /** One WHOLE frame. Every path ends in exactly one counted outcome. */
  async #frame(
    state: StreamState,
    bytes: Uint8Array,
    header: StreamFrameHeader | undefined,
  ): Promise<void> {
    let decoded: DecodedFrame;
    try {
      decoded = decodeFrame(bytes);
    } catch (err) {
      if (!(err instanceof FrameRejectedError)) throw err;
      this.#drop(state, err.rejectReason);
      this.#lossFromHeader(state, header, err.rejectReason);
      return;
    }

    // SLOT BINDING from the first frame; any later change aborts.
    if (state.slotId === undefined) {
      const slot = this.#o.slotFor(decoded.streamId);
      if (!slot) {
        this.#o.metrics.undeclaredStreamId();
        this.#drop(state, 'stream_slot_mismatch', 'undeclared');
        state.abort = true;
        return;
      }
      state.slotId = decoded.streamId;
      state.slot = slot;
    } else if (decoded.streamId !== state.slotId) {
      this.#drop(state, 'stream_slot_mismatch');
      state.abort = true;
      return;
    }
    const slot = state.slot as VideoSlotPipeline;
    const slotId = state.slotId;
    // R-35 local tally: a frame naming this slot arrived (count + wire bytes).
    this.#o.accounting.arrived(slotId, bytes.length);

    let senderId: number;
    try {
      senderId = keyIdSenderOf(decoded.payload);
    } catch (err) {
      if (!(err instanceof FrameRejectedError)) throw err;
      // No readable key id: no sender, so no recovery request (security R3).
      this.#drop(state, err.rejectReason);
      return;
    }
    // Layer 1: the key id names this sender on this slot. Pre-verify.
    this.#o.verification?.keyed(slotId, senderId);

    try {
      const { opened } = await receiveByKind({ decoded, senderId }, this.#o.gate, this.#o.ctx);
      // Layer 2 + the wrap outcome: the ONE accept tail (`frameAccounting.ts`).
      this.#o.accounting.accepted(opened.wrapOutcome, slotId, senderId);
      // Decode in ANY slot state: the video decode-before-live exception.
      slot.push({
        senderId,
        gopCounter: decoded.gopCounter,
        isKeyframe: decoded.flags.independentlyDecodable,
        data: opened.plaintext,
        url: state.url,
      });
    } catch (err) {
      if (!(err instanceof FrameRejectedError)) throw err;
      const reason = err.rejectReason;
      this.#o.accounting.dropped(reason, slotId, senderId);
      if (reason === 'sender_server_muted') {
        // Verified header: repaired once the unmute arrives (`server_mute_lifted`).
        slot.recordMutedDrop(senderId, decoded.gopCounter, state.url);
        return;
      }
      const recovery = RECOVERY_REASON_FOR[reason];
      if (recovery) slot.loss(senderId, decoded.gopCounter, recovery, state.url);
    }
  }

  /**
   * Count one drop that happened before a key id was parsed from a DECODED
   * frame. The evidence records it unattributed: a reader-header sender feeds
   * only a recovery request, never an evidence hook (security R1).
   */
  #drop(state: StreamState, reason: RejectReason, observed?: ObservedSlot): void {
    this.#o.accounting.dropped(reason, observed ?? state.slotId ?? 'unparsed', undefined);
  }

  /** A codec reject on a whole frame: a loss, if its header names a sender on a known slot. */
  #lossFromHeader(
    state: StreamState,
    header: StreamFrameHeader | undefined,
    reason: RejectReason,
  ): void {
    const recovery = RECOVERY_REASON_FOR[reason];
    if (!recovery || !header) return;
    const slot = state.slot ?? this.#o.slotFor(header.relayStreamId);
    slot?.loss(header.senderId, header.gopCounter, recovery, state.url);
  }

  /** MH reset the group's stream (R-12): a loss iff a header was parsed on it. */
  #groupAbandoned(state: StreamState, header: StreamFrameHeader | undefined): void {
    if (!header) return;
    const slot = state.slot ?? this.#o.slotFor(header.relayStreamId);
    slot?.loss(
      header.senderId,
      header.gopCounter,
      MEDIA_RECOVERY_SOURCES.GroupAbandoned,
      state.url,
    );
  }
}
