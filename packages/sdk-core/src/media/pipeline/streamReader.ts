// File: packages/sdk-core/src/media/pipeline/streamReader.ts
//
// HOT PATH (per chunk, per frame). No logging, no metric names, no label
// construction.
//
// ---------------------------------------------------------------------------
// THE INCREMENTAL UNI-STREAM FRAME READER (story 3 R-21, client half)
// ---------------------------------------------------------------------------
//
// One video group arrives on one unidirectional QUIC stream, frames back to back
// and delimited by `payload_length`; MH forwards cut-through, so one frame's
// bytes arrive across many reads. This reader turns chunks into WHOLE frames:
//
//   * The frame's extent comes from `frameCodec.ts:peekFrameExtent` — the SAME
//     prefix parser `decodeFrame` runs, so `payload_length > MAX_PAYLOAD_BYTES`
//     is refused from the header alone (`payload_length_exceeds_max`) BEFORE any
//     buffer is sized from it. Until then the reader holds at most
//     `FRAME_EXTENT_PEEK_BYTES` in one fixed scratch buffer.
//   * Then ONE allocation of exactly the frame's length (through the `alloc`
//     seam, so a test can prove it), filled in place — never grown by concat.
//   * Only a WHOLE frame leaves this module. Nothing here verifies, decrypts or
//     decodes; a partial payload is never handed anywhere (cut-through is
//     MH-only).
//
// A prefix reject ends the stream: framing is lost, so no later boundary can be
// trusted. A whole frame that later fails `decodeFrame` does not: its extent was
// valid, so the next frame starts where it ends.
//
// COUNTING POINT: a frame is RECEIVED at its first byte (`onFrameStart`), so a
// frame cut by a reset or FIN is received and then dropped
// `stream_partial_reset` exactly once — the identity
// `received = accepted + sum(drops)` holds per frame, never per stream.

import {
  FRAME_EXTENT_PEEK_BYTES,
  peekFrameExtent,
  readRelayStreamId,
  type FrameExtent,
} from '../frame/frameCodec.js';
import { KEY_ID_BYTES } from '../frame/keyId.js';
import { FrameRejectedError, type RejectReason } from '../frame/rejectReason.js';
import { keyIdSenderOf } from '../frame/sframe.js';

/**
 * What a frame's leading bytes say, once its prefix AND its 8 key-id bytes are
 * buffered. ALL UNVERIFIED: these feed slot binding (relay id) and a recovery
 * request's fields only — never attribution (security R1).
 */
export interface StreamFrameHeader {
  /** The MH-written relay `stream_id` (this subscriber's slot id). */
  readonly relayStreamId: number;
  /** `key_id.sender_id`, unverified. */
  readonly senderId: number;
  /** The signed-but-not-yet-verified `gop_counter`. */
  readonly gopCounter: number;
}

/** Callbacks, invoked synchronously from {@link UniStreamFrameReader.push}. */
export interface StreamReaderSink {
  /** A frame's first byte arrived: count it received. */
  onFrameStart(): void;
  /** A WHOLE frame is buffered. `header` is set when its key id was readable. */
  onFrame(bytes: Uint8Array, header: StreamFrameHeader | undefined): void;
  /** The current frame's prefix was refused. The reader stops; abort the stream. */
  onPrefixReject(reason: RejectReason): void;
}

/** How a stream ended, as the reader saw it. */
export type StreamEndOutcome =
  /** Ended on a frame boundary (or before any byte). */
  | { readonly partial: false }
  /** Ended inside a frame: that frame is lost (`stream_partial_reset`). */
  | { readonly partial: true };

/** The reader's allocation seam: exactly one call per frame, with its exact length. */
export type FrameAllocator = (length: number) => Uint8Array;

const defaultAlloc: FrameAllocator = (length) => new Uint8Array(length);

/** One stream's incremental reader. See the module header. */
export class UniStreamFrameReader {
  readonly #sink: StreamReaderSink;
  readonly #alloc: FrameAllocator;
  /** Scratch for the leading bytes, before the extent is known. Fixed size. */
  readonly #peek = new Uint8Array(FRAME_EXTENT_PEEK_BYTES);
  #peekLen = 0;
  /** The current frame's buffer, once its extent is known. */
  #frame: Uint8Array | undefined;
  #filled = 0;
  #extent: Extract<FrameExtent, { complete: true }> | undefined;
  /** Whether a frame is in progress (its first byte has arrived). */
  #inFrame = false;
  /** The current frame's header, once its key id is buffered. */
  #currentHeader: StreamFrameHeader | undefined;
  /** The current frame's payload cannot name a sender. */
  #headerUnavailable = false;
  /** The latest header parsed on this stream (current frame's, else the last complete one's). */
  #lastHeader: StreamFrameHeader | undefined;
  #stopped = false;

  constructor(sink: StreamReaderSink, alloc: FrameAllocator = defaultAlloc) {
    this.#sink = sink;
    this.#alloc = alloc;
  }

  /**
   * The latest header parsed on this stream, or `undefined` if no frame's key
   * id was ever readable. A reset's recovery request takes its fields from here;
   * none means the request is not sent (security R3).
   */
  get lastHeader(): StreamFrameHeader | undefined {
    return this.#lastHeader;
  }

  /** Whether a prefix reject stopped this reader. */
  get stopped(): boolean {
    return this.#stopped;
  }

  /** Feed one chunk. Never throws on wire content. */
  push(chunk: Uint8Array): void {
    let i = 0;
    while (i < chunk.length && !this.#stopped) {
      if (!this.#inFrame) {
        this.#inFrame = true;
        this.#peekLen = 0;
        this.#currentHeader = undefined;
        this.#headerUnavailable = false;
        this.#sink.onFrameStart();
      }
      if (this.#frame === undefined) {
        i = this.#pushPeek(chunk, i);
      } else {
        i = this.#pushBody(chunk, i);
      }
    }
  }

  /** The stream ended (FIN or reset). Reports whether a frame was cut. */
  end(): StreamEndOutcome {
    const partial = this.#inFrame && !this.#stopped;
    this.#inFrame = false;
    this.#frame = undefined;
    this.#extent = undefined;
    this.#stopped = true;
    return partial ? { partial: true } : { partial: false };
  }

  #pushPeek(chunk: Uint8Array, at: number): number {
    // Every frame is longer than the peek window (fixed prefix + ext length +
    // relay region + signature alone exceed it), so filling the window never
    // reads past this frame's end.
    const n = Math.min(chunk.length - at, FRAME_EXTENT_PEEK_BYTES - this.#peekLen);
    this.#peek.set(chunk.subarray(at, at + n), this.#peekLen);
    this.#peekLen += n;
    let extent: FrameExtent;
    try {
      extent = peekFrameExtent(this.#peek.subarray(0, this.#peekLen));
    } catch (err) {
      if (!(err instanceof FrameRejectedError)) throw err;
      this.#stopped = true;
      this.#inFrame = false;
      this.#sink.onPrefixReject(err.rejectReason);
      return chunk.length;
    }
    if (!extent.complete) return at + n;
    // The extent is known and bounded: the ONE allocation for this frame.
    const frame = this.#alloc(extent.totalLength);
    frame.set(this.#peek.subarray(0, this.#peekLen), 0);
    this.#frame = frame;
    this.#filled = this.#peekLen;
    this.#extent = extent;
    this.#maybeHeader();
    return at + n;
  }

  #pushBody(chunk: Uint8Array, at: number): number {
    const frame = this.#frame as Uint8Array;
    const n = Math.min(chunk.length - at, frame.length - this.#filled);
    frame.set(chunk.subarray(at, at + n), this.#filled);
    this.#filled += n;
    this.#maybeHeader();
    if (this.#filled === frame.length) {
      const header = this.#currentHeader;
      this.#frame = undefined;
      this.#extent = undefined;
      this.#inFrame = false;
      this.#sink.onFrame(frame, header);
    }
    return at + n;
  }

  /**
   * Parse the current frame's header once its key id bytes are buffered. Every
   * field position is the codec's (`peekFrameExtent`, `readRelayStreamId`) or
   * the SFrame layer's (`keyIdSenderOf`); this reader knows none of them.
   */
  #maybeHeader(): void {
    const extent = this.#extent;
    const frame = this.#frame;
    if (this.#currentHeader || this.#headerUnavailable || !extent || !frame) return;
    if (this.#filled < extent.payloadOffset + KEY_ID_BYTES) return;
    let senderId: number;
    try {
      senderId = keyIdSenderOf(
        frame.subarray(extent.payloadOffset, extent.payloadOffset + extent.payloadLength),
      );
    } catch (err) {
      if (!(err instanceof FrameRejectedError)) throw err;
      // A payload too short for an SFrame header names no sender (decodeFrame
      // and the policy reject it): no header, so no reset request (R3).
      this.#headerUnavailable = true;
      return;
    }
    const header: StreamFrameHeader = {
      relayStreamId: readRelayStreamId(frame, extent.relayRegionOffset),
      senderId,
      gopCounter: extent.gopCounter,
    };
    this.#currentHeader = header;
    this.#lastHeader = header;
  }
}
