// File: packages/sdk-core/src/framing/sendFramed.ts
//
// Shared client→server send pipeline (extract-on-second-use, @dry-reviewer /
// @code-reviewer-A reconciliation). ONE place that turns a protobuf envelope into
// a trace-injected, length-prefixed frame on a WebTransport bidi stream. Reused by:
//   - SignalingClient `join()` + `sendMediaConnectionUpdate()`  → ClientMessage  (browser→MC)
//   - MediaTransport `#connectOne()`                            → MhClientMessage (browser→MH)
// so the MH path does NOT copy-paste the inject/frame/write pipeline.
//
// Sited in `framing/` (the foundational layer both `signaling/` and `media/`
// already import down from) so the dependency edge runs signaling→framing and
// media→framing — never an inversion.
//
// R-58 trace: the SINGLE `injectIntoClientMessage` call site for BOTH outbound
// envelopes. It is the FIRST, SYNCHRONOUS step, so the CALLER controls which
// context is active at inject time by wrapping the call in `context.with(...)`
// (the global StackContextManager is synchronous and does NOT survive an `await`,
// so the caller must establish the join span context around THIS call, not around
// some outer async region). The async write tail runs after, context-independent.

import { toBinary } from '@bufbuild/protobuf';
import type { Message } from '@bufbuild/protobuf';
import type { GenMessage } from '@bufbuild/protobuf/codegenv2';

import { injectIntoClientMessage } from '../telemetry/tracePropagation.js';
import type { TraceCarrier } from '../telemetry/tracePropagation.js';
import type { WebTransportBidirectionalStream } from '../transport/IWebTransport.js';
import { encodeFrame } from './length-prefix.js';

/**
 * Trace-inject, serialize, length-prefix, and write a single protobuf envelope to
 * `stream.writable`. Generic over the protobuf schema so MC `ClientMessage` and MH
 * `MhClientMessage` share one pipeline.
 *
 * Steps (in order): `injectIntoClientMessage(message)` (SYNC, under the caller's
 * active context) → `toBinary(schema, message)` → `encodeFrame` (4-byte BE prefix,
 * 64 KiB cap) → acquire writer → `write` → release lock.
 *
 * @throws {FramingError} if the encoded payload is empty or exceeds the cap.
 * @throws on a writer/transport write failure (propagated to the caller).
 */
export async function sendFramedMessage<T extends Message & TraceCarrier>(
  stream: WebTransportBidirectionalStream,
  schema: GenMessage<T>,
  message: T,
): Promise<void> {
  await writeFrame(stream, encodeFramedMessage(schema, message));
}

/**
 * The SYNCHRONOUS half of {@link sendFramedMessage}: trace-inject (under the
 * caller's active context), serialize and length-prefix. Split out so a caller
 * that serialises its writes (one writer at a time on a shared stream) can encode
 * — and capture trace context — at CALL time and queue only the write.
 *
 * @throws {FramingError} if the encoded payload is empty or exceeds the cap.
 */
export function encodeFramedMessage<T extends Message & TraceCarrier>(
  schema: GenMessage<T>,
  message: T,
): Uint8Array {
  injectIntoClientMessage(message);
  return encodeFrame(toBinary(schema, message));
}

/** The write half: acquire the writer, write one framed message, release the lock. */
export async function writeFrame(
  stream: WebTransportBidirectionalStream,
  frame: Uint8Array,
): Promise<void> {
  const writer = stream.writable.getWriter();
  try {
    await writer.write(frame);
  } finally {
    writer.releaseLock();
  }
}
