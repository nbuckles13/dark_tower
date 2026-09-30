// File: packages/sdk-core/src/media/__tests__/helpers.ts
//
// Test-local helpers (R-41). Decode captured outbound frames back into proto
// envelopes by stripping the reused 4-byte BE length prefix; build a url→mock
// `connect` factory so each MH gets its own `MockWebTransport`.

import { fromBinary } from '@bufbuild/protobuf';
import { MockWebTransport } from '@darktower/test-utils';

import { FrameDecoder } from '../../framing/length-prefix.js';
import { DEFAULT_CLIENT_CONFIG, transmitRewrapLatencyMs } from '../../config/clientConfig.js';
import { MeetingKekHolder, type MeetingKekHolderOptions } from '../setup/kekSource.js';
import type { SenderGate } from '../pipeline/ingress.js';
import type { DecodeLane } from '../pipeline/receiveLanes.js';
import type { EncodedAudioFrame } from '../setup/seams.js';
import {
  ClientMessageSchema,
  MhClientMessageSchema,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import type {
  ClientMessage,
  MhClientMessage,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';

/**
 * A `connect` factory backed by a url→mock map. Lazily creates one
 * `MockWebTransport` per URL (so the test can drive each MH independently) and
 * records it in `mocks` for inspection.
 */
export function makeConnect(mocks: Map<string, MockWebTransport>) {
  return (url: string): MockWebTransport => {
    let mock = mocks.get(url);
    if (mock === undefined) {
      mock = new MockWebTransport();
      mocks.set(url, mock);
    }
    return mock;
  };
}

/** Decode every framed `MhClientMessage` written to a captured bidi stream. */
export function decodeMhClientMessages(chunks: readonly Uint8Array[]): MhClientMessage[] {
  const decoder = new FrameDecoder();
  const out: MhClientMessage[] = [];
  for (const chunk of chunks) {
    for (const frame of decoder.push(chunk)) {
      out.push(fromBinary(MhClientMessageSchema, frame));
    }
  }
  return out;
}

/** Decode every framed `ClientMessage` written to a captured bidi stream (MC side). */
export function decodeClientMessages(chunks: readonly Uint8Array[]): ClientMessage[] {
  const decoder = new FrameDecoder();
  const out: ClientMessage[] = [];
  for (const chunk of chunks) {
    for (const frame of decoder.push(chunk)) {
      out.push(fromBinary(ClientMessageSchema, frame));
    }
  }
  return out;
}

/** Poll `predicate` across microtasks/timers until true (or throw on timeout). */
export async function waitFor(predicate: () => boolean, tries = 200): Promise<void> {
  for (let i = 0; i < tries; i++) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('waitFor: predicate did not become true in time');
}

// ---------------------------------------------------------------------------
// KEK holder fixtures
// ---------------------------------------------------------------------------

/**
 * A W whose derived retention is NOMINAL under the default config, so a test
 * that is not about the retention rule produces no anomaly counts or warnings.
 * Chosen as MC's own default; the expected window is always read back through
 * `deriveKekRetention`, never restated as `W / 2`.
 */
export const NOMINAL_DEBOUNCE_SECONDS = 60;

/**
 * An empty KEK holder with this client's real re-wrap latency.
 *
 * Tests reach the holder only through `install` — the same entry point the
 * signaling intake uses — never through a priming door.
 */
export function emptyKekHolder(
  options: Partial<Omit<MeetingKekHolderOptions, 'rewrapLatencyMs'>> = {},
): MeetingKekHolder {
  return new MeetingKekHolder({
    rewrapLatencyMs: transmitRewrapLatencyMs(DEFAULT_CLIENT_CONFIG.media),
    ...options,
  });
}

/** A holder with `kek` installed as the current generation, as a join response would. */
export function kekHolderWith(
  kek: Uint8Array,
  generation = 0,
  options: Partial<Omit<MeetingKekHolderOptions, 'rewrapLatencyMs'>> = {},
): MeetingKekHolder {
  const holder = emptyKekHolder(options);
  const result = holder.install(kek, generation, NOMINAL_DEBOUNCE_SECONDS, 'join_response');
  if (result !== 'installed') throw new Error(`fixture KEK was not installed: ${result}`);
  return holder;
}

// ---------------------------------------------------------------------------
// Slot-edge gate fixture
// ---------------------------------------------------------------------------

/**
 * A slot-edge gate that admits EXACTLY `senderIds`, recording what each lane
 * decoded. For ingress tests that are not about the gate itself.
 *
 * Deliberately not an admit-everything gate: even a test uninterested in the
 * gate states which senders are assigned, so a frame from any other sender is
 * dropped as `sender_not_assigned` exactly as in production.
 */
export function gateFor(...senderIds: number[]): SenderGate & {
  readonly decodedBy: ReadonlyMap<number, readonly EncodedAudioFrame[]>;
} {
  const decodedBy = new Map<number, EncodedAudioFrame[]>(senderIds.map((id) => [id, []]));
  return {
    decodedBy,
    laneFor(senderId: number): DecodeLane | undefined {
      const decoded = decodedBy.get(senderId);
      return decoded ? { decode: (frame) => decoded.push(frame) } : undefined;
    },
  };
}

/**
 * `StreamAssignments` naming `urls`, one slot each (slot i for url i), every
 * filled slot sourced from `senderId`; an empty url is an unfilled slot.
 *
 * For suites about the RECEIVE TRANSPORTS, which previously passed bare handler
 * URLs. The slot-edge gate only admits senders on DECLARED slots, so with the
 * usual single declared slot only the first url's sender gets a decoder — while
 * every url still selects a transport, exactly as production derives both from
 * one message.
 */
export function assignmentsOn(
  urls: readonly string[],
  senderId: number,
): { slotId: number; senderId: number | undefined; mediaHandlerUrl: string; active: boolean }[] {
  return urls.map((mediaHandlerUrl, slotId) => ({
    slotId,
    senderId: mediaHandlerUrl === '' ? undefined : senderId,
    mediaHandlerUrl,
    // A filled slot is MC-active; an unfilled one is not.
    active: mediaHandlerUrl !== '',
  }));
}
