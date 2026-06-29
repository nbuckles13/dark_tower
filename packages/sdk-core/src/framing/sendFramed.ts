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
  // R-58: single injection call site (no hand-rolled propagation.inject). Runs
  // synchronously under whatever context the caller has made active.
  injectIntoClientMessage(message);
  const bytes = toBinary(schema, message);
  const frame = encodeFrame(bytes);
  const writer = stream.writable.getWriter();
  try {
    await writer.write(frame);
  } finally {
    // Release immediately — the long-lived connection holds no writable lock.
    writer.releaseLock();
  }
}
