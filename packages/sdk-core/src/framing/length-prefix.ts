// File: packages/sdk-core/src/framing/length-prefix.ts
//
// R-16 (framing portion): 4-byte big-endian length-prefix wire codec over raw
// byte payloads. This MUST match the Meeting Controller's wire contract
// EXACTLY — the authoritative anchor is the Rust side:
//   crates/mc-service/src/webtransport/connection.rs
//     :33   const MAX_MESSAGE_SIZE = 64 * 1024
//     :648  frame.put_u32(len)   -> 4-byte BE length prefix
//     :607  reject msg_len > MAX_MESSAGE_SIZE  ("Message too large")
//     :617  reject msg_len == 0                ("Empty message")
//
// The length-prefix is a hand-rolled transport envelope (NOT a proto-defined
// message), so it is intentionally out of scope for the proto/test-vectors/
// shared TS<->Rust vector harness (that covers the 42-byte media frame header).
// The byte-level contract test (length-prefix.test.ts test 10) plus this
// comment are the contract anchors keeping the two sides in sync.
//
// SECURITY (per @security Gate-2): the declared length is validated BEFORE any
// body buffer is allocated or accumulated, mirroring MC's check-before-alloc.
// `FrameDecoder`'s carry buffer therefore can never grow past one in-flight
// frame (<= 4-byte prefix + MAX_MESSAGE_SIZE bytes), even under dribbled
// partial chunks.
//
// No metric / trace / log emissions originate from this module. Error paths are
// modelled as the typed `FramingError` (stable `code` discriminants) so later
// observability instrumentation (R-24/25/26) is a pure add.

/**
 * Maximum frame payload size, matching MC's `MAX_MESSAGE_SIZE`
 * (`connection.rs:33`). Frames larger than this are rejected on both encode and
 * decode.
 */
export const MAX_MESSAGE_SIZE = 64 * 1024;

/** Width of the big-endian length prefix, in bytes. */
const LENGTH_PREFIX_BYTES = 4;

/**
 * Stable discriminants for framing failures. Kept as a const-object union so
 * downstream observability can map each cause to a metric label without
 * coupling to error message strings.
 */
export const FramingErrorCode = {
  /** Payload length was zero. Mirrors MC "Empty message". */
  EmptyMessage: 'EMPTY_MESSAGE',
  /** Payload length exceeded {@link MAX_MESSAGE_SIZE}. Mirrors MC "Message too large". */
  MessageTooLarge: 'MESSAGE_TOO_LARGE',
} as const;

export type FramingErrorCode = (typeof FramingErrorCode)[keyof typeof FramingErrorCode];

/**
 * Typed error for all framing-codec failures. Carries the offending byte
 * length and the enforced cap so callers / instrumentation have structured
 * context (not stringly-typed).
 */
export class FramingError extends Error {
  readonly code: FramingErrorCode;
  /** The offending payload length in bytes. */
  readonly length: number;
  /** The enforced maximum ({@link MAX_MESSAGE_SIZE}). */
  readonly max: number;

  constructor(code: FramingErrorCode, message: string, length: number) {
    super(message);
    this.name = 'FramingError';
    this.code = code;
    this.length = length;
    this.max = MAX_MESSAGE_SIZE;
    // Restore prototype chain for `instanceof` across the ES2022 transpile.
    Object.setPrototypeOf(this, FramingError.prototype);
  }
}

/**
 * Encode a single payload into a length-prefixed frame:
 * `[4-byte BE u32 length][payload bytes]`.
 *
 * Matches MC's `write_raw_framed` (`connection.rs:642-649`).
 *
 * @throws {FramingError} `EMPTY_MESSAGE` if `payload` is empty;
 *   `MESSAGE_TOO_LARGE` if `payload.byteLength > MAX_MESSAGE_SIZE`.
 */
export function encodeFrame(payload: Uint8Array): Uint8Array {
  const len = payload.byteLength;
  if (len === 0) {
    throw new FramingError(FramingErrorCode.EmptyMessage, 'Empty message', len);
  }
  if (len > MAX_MESSAGE_SIZE) {
    throw new FramingError(
      FramingErrorCode.MessageTooLarge,
      `Message too large: ${len} > ${MAX_MESSAGE_SIZE}`,
      len,
    );
  }

  const frame = new Uint8Array(LENGTH_PREFIX_BYTES + len);
  // Big-endian u32 length prefix (matches `BytesMut::put_u32`).
  const view = new DataView(frame.buffer, frame.byteOffset, LENGTH_PREFIX_BYTES);
  view.setUint32(0, len, /* littleEndian */ false);
  frame.set(payload, LENGTH_PREFIX_BYTES);
  return frame;
}

/**
 * Stateful streaming decoder for length-prefixed frames.
 *
 * A frame may arrive split across multiple `ReadableStream` chunks, and
 * multiple frames may arrive in one chunk. `push` buffers leftover bytes
 * between calls and emits zero or more complete payloads each time.
 *
 * SECURITY: the declared length is range-checked the instant the 4-byte prefix
 * is complete — BEFORE any body bytes are awaited or retained. An oversize or
 * zero-length declaration throws immediately, so the internal carry buffer is
 * bounded by one in-flight frame (`<= 4 + MAX_MESSAGE_SIZE`). Dribbled partial
 * chunks cannot grow it without bound.
 */
export class FrameDecoder {
  /** Leftover bytes not yet forming a complete frame. */
  #buffer: Uint8Array = new Uint8Array(0);

  /**
   * Append `chunk` to the internal buffer and return every complete frame
   * payload now available, in order. Returns an empty array if no frame has
   * fully arrived yet.
   *
   * @throws {FramingError} if a fully-received length prefix declares a length
   *   of zero (`EMPTY_MESSAGE`) or greater than {@link MAX_MESSAGE_SIZE}
   *   (`MESSAGE_TOO_LARGE`). The throw happens before the body is buffered.
   */
  push(chunk: Uint8Array): Uint8Array[] {
    if (chunk.byteLength > 0) {
      this.#buffer = concat(this.#buffer, chunk);
    }

    const frames: Uint8Array[] = [];

    // Loop: a single chunk may complete several frames.
    for (;;) {
      // Not enough bytes for a length prefix yet — buffer and wait.
      if (this.#buffer.byteLength < LENGTH_PREFIX_BYTES) {
        break;
      }

      const view = new DataView(this.#buffer.buffer, this.#buffer.byteOffset, LENGTH_PREFIX_BYTES);
      const msgLen = view.getUint32(0, /* littleEndian */ false);

      // SECURITY: validate the declared length BEFORE buffering/awaiting the
      // body, mirroring MC `connection.rs:607,617`.
      if (msgLen > MAX_MESSAGE_SIZE) {
        throw new FramingError(
          FramingErrorCode.MessageTooLarge,
          `Message too large: ${msgLen} > ${MAX_MESSAGE_SIZE}`,
          msgLen,
        );
      }
      if (msgLen === 0) {
        throw new FramingError(FramingErrorCode.EmptyMessage, 'Empty message', msgLen);
      }

      const frameEnd = LENGTH_PREFIX_BYTES + msgLen;
      // Body not fully arrived yet — buffer and wait for more chunks.
      if (this.#buffer.byteLength < frameEnd) {
        break;
      }

      // Slice out the payload (copy so the returned view is independent of the
      // shifting internal buffer).
      const payload = this.#buffer.slice(LENGTH_PREFIX_BYTES, frameEnd);
      frames.push(payload);

      // Advance the buffer past the consumed frame.
      this.#buffer = this.#buffer.slice(frameEnd);
    }

    return frames;
  }

  /** Number of leftover bytes currently buffered (for tests/diagnostics). */
  get bufferedByteLength(): number {
    return this.#buffer.byteLength;
  }
}

/**
 * Concatenate the carry buffer `a` with an incoming chunk `b`. The empty-`a`
 * fast path is reachable (the buffer starts empty and is emptied after each
 * fully-consumed frame); `b` is never empty here because `push` only calls
 * `concat` for non-empty chunks, so no empty-`b` branch is carried.
 */
function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  if (a.byteLength === 0) return b;
  const out = new Uint8Array(a.byteLength + b.byteLength);
  out.set(a, 0);
  out.set(b, a.byteLength);
  return out;
}
