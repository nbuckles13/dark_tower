// File: packages/sdk-core/src/media/pipeline/sendId.ts
//
// THE ONE stamping function for the uplink relay `stream_id` (story 3 R-20,
// wire freeze amendment 3: relay ids are HOP-LOCAL).
//
// On the uplink the publisher stamps the frame's 16-bit relay-region
// `stream_id` with MC's SEND ID for that stream — `SendStream.stream_number`
// (audio 0, video 1; signaling.proto `SendStream.stream_number`, "IT IS ALSO
// THE UPLINK RELAY ID"). MH attributes the frame by (connection-bound sender,
// that id) against MC's `source_streams`. It is NEVER a slot id: a subscriber's
// slot ids live in a different value space (the DOWNLINK relay id MH writes),
// and stamping one is exactly the stale story-2 bug this replaces
// (`declaredSlotIds[0] ?? 0`).
//
// Audio uses this now; story 3 C-A2's video egress must use the same function.

import { STREAM_EXCLUSIVE_BOUND } from '../frame/keyId.js';

/** The send id as MC directed it for one stream. */
export interface DirectedSendStream {
  readonly streamNumber: number;
}

/**
 * The uplink relay `stream_id` for `stream`: its directed `stream_number`,
 * or `undefined` when that value is outside the 8-bit key-id stream domain
 * (`keyId.ts:STREAM_EXCLUSIVE_BOUND`; the proto carries 8-bit semantics in a
 * `uint32` and states that MC enforces the range). The caller treats
 * `undefined` as a fault and sends nothing — never a substituted value.
 */
export function uplinkRelayStreamId(stream: DirectedSendStream): number | undefined {
  const n = stream.streamNumber;
  return Number.isInteger(n) && n >= 0 && BigInt(n) < STREAM_EXCLUSIVE_BOUND ? n : undefined;
}
