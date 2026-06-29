// File: packages/sdk-core/src/media/__tests__/helpers.ts
//
// Test-local helpers (R-41). Decode captured outbound frames back into proto
// envelopes by stripping the reused 4-byte BE length prefix; build a url→mock
// `connect` factory so each MH gets its own `MockWebTransport`.

import { fromBinary } from '@bufbuild/protobuf';
import { MockWebTransport } from '@darktower/test-utils';

import { FrameDecoder } from '../../framing/length-prefix.js';
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
