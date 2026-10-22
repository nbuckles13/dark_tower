// File: packages/sdk-core/src/media/__tests__/mhControlChannel.test.ts
//
// Story 3 C-A1: the MH control channel on the connect bidi stream. MH -> client
// `MhServerMessage` read loop (length-prefixed, size-bounded, malformed input
// counted and never thrown) and the client -> MH `recovery_request` send.
// Deterministic: chunks are driven by the test; settling is by awaiting the
// loop's own progress (bounded macrotask yields), never wall time.

import { create, toBinary } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import type { MockWebTransport } from '@darktower/test-utils';

import { MediaTransport } from '../MediaTransport.js';
import { MediaConnectionError } from '../../errors/MediaConnectionError.js';
import { encodeFrame, MAX_MESSAGE_SIZE } from '../../framing/length-prefix.js';
import {
  KeyframeRequestSchema,
  MhClientMessageSchema,
  MhServerMessageSchema,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { fromBinary } from '@bufbuild/protobuf';
import type { MhControlChannelEnding, MhControlSink, MhKeyframeRequest } from '../events.js';
import { makeConnect, waitFor } from './helpers.js';

const URL_A = 'https://mh-a.example:4433';
const JWT = 'meeting-jwt-value';

interface Recorded {
  readonly requests: MhKeyframeRequest[];
  /** The handler URL each request was bound to (the connection it was READ on). */
  readonly urls: string[];
  malformed: number;
  readonly ended: MhControlChannelEnding[];
}

function recordingSink(): { sink: MhControlSink; seen: Recorded } {
  const seen: Recorded = { requests: [], urls: [], malformed: 0, ended: [] };
  return {
    seen,
    sink: {
      keyframeRequest: (url, request) => {
        seen.urls.push(url);
        seen.requests.push(request);
      },
      malformed: () => {
        seen.malformed += 1;
      },
      ended: (_url, cause) => seen.ended.push(cause),
    },
  };
}

async function connected(): Promise<{
  mt: MediaTransport;
  mh: MockWebTransport;
  seen: Recorded;
}> {
  const mocks = new Map<string, MockWebTransport>();
  const { sink, seen } = recordingSink();
  const mt = new MediaTransport({ connect: makeConnect(mocks), controlSink: sink });
  const p = mt.connectAll([URL_A], JWT);
  mocks.get(URL_A)!.simulateReady();
  await p;
  return { mt, mh: mocks.get(URL_A)!, seen };
}

function keyframeRequest(streamNumber: number, gopCounter: number): Uint8Array {
  return toBinary(
    MhServerMessageSchema,
    create(MhServerMessageSchema, {
      message: {
        case: 'keyframeRequest',
        value: create(KeyframeRequestSchema, { streamNumber, gopCounter }),
      },
    }),
  );
}

/**
 * Let every pending stream read and reaction run: a bounded number of
 * zero-delay task turns (the same yield `waitFor` uses), never a timed sleep.
 */
async function settle(turns = 10): Promise<void> {
  for (let i = 0; i < turns; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
}

/** A big-endian 4-byte length prefix with no body (so no allocation can follow). */
function prefixOnly(length: number): Uint8Array {
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, length, false);
  return out;
}

describe('MhServerMessage read loop', () => {
  it('delivers a well-formed keyframe_request to the sink', async () => {
    const { mh, seen } = await connected();
    mh.simulateServerMessage(0, encodeFrame(keyframeRequest(1, 7)));
    await waitFor(() => seen.requests.length === 1);
    expect(seen.requests).toEqual([{ streamNumber: 1, gopCounter: 7 }]);
    // H is the connection the request was read on (security S6), never payload.
    expect(seen.urls).toEqual([URL_A]);
    expect(seen.malformed).toBe(0);
  });

  it('reassembles a length prefix split across chunks, and a body split across chunks', async () => {
    const { mh, seen } = await connected();
    const framed = encodeFrame(keyframeRequest(1, 9));
    mh.simulateServerMessage(0, framed.slice(0, 2));
    mh.simulateServerMessage(0, framed.slice(2, 5));
    mh.simulateServerMessage(0, framed.slice(5));
    await waitFor(() => seen.requests.length === 1);
    expect(seen.requests).toEqual([{ streamNumber: 1, gopCounter: 9 }]);
  });

  it('counts undecodable protobuf as malformed and KEEPS READING', async () => {
    const { mh, seen } = await connected();
    mh.simulateServerMessage(0, encodeFrame(new Uint8Array([0xff, 0xff, 0xff])));
    mh.simulateServerMessage(0, encodeFrame(keyframeRequest(1, 3)));
    await waitFor(() => seen.requests.length === 1);
    expect(seen.malformed).toBe(1);
  });

  it('counts an unset variant as malformed and keeps reading', async () => {
    const { mh, seen } = await connected();
    const unset = toBinary(
      MhServerMessageSchema,
      create(MhServerMessageSchema, { traceParent: 'x' }),
    );
    mh.simulateServerMessage(0, encodeFrame(unset));
    mh.simulateServerMessage(0, encodeFrame(keyframeRequest(1, 4)));
    await waitFor(() => seen.requests.length === 1);
    expect(seen.malformed).toBe(1);
  });

  it('refuses an OVERSIZED length from the prefix ALONE, before any body arrives, and stops', async () => {
    const { mh, seen } = await connected();
    // Only the 4-byte prefix is delivered: the refusal cannot depend on the body.
    mh.simulateServerMessage(0, prefixOnly(MAX_MESSAGE_SIZE + 1));
    await waitFor(() => seen.malformed === 1);
    // The stream is desynchronised: a later well-formed message is not read.
    mh.simulateServerMessage(0, encodeFrame(keyframeRequest(1, 5)));
    await settle();
    expect(seen.requests).toEqual([]);
    expect(seen.malformed).toBe(1);
  });

  it('counts a ZERO-length message as malformed and stops', async () => {
    const { mh, seen } = await connected();
    mh.simulateServerMessage(0, prefixOnly(0));
    await waitFor(() => seen.malformed === 1);
    mh.simulateServerMessage(0, encodeFrame(keyframeRequest(1, 6)));
    await settle();
    expect(seen.requests).toEqual([]);
  });

  it("this client's own TEARDOWN ends the loop uncounted and never throws", async () => {
    const { mt, seen } = await connected();
    mt.disconnect();
    await settle();
    expect(seen.malformed).toBe(0);
    expect(seen.ended).toEqual([]);
  });

  it('MH FINISHING its send half is reported as `finished`, once', async () => {
    const { mh, seen } = await connected();
    mh.simulateBidiStreamEnd(0);
    await waitFor(() => seen.ended.length === 1);
    await settle();
    expect(seen.ended).toEqual(['finished']);
  });

  it('MH RESETTING the control stream is reported as `reset`, once', async () => {
    const { mh, seen } = await connected();
    mh.simulateBidiStreamReset(0, 3);
    await waitFor(() => seen.ended.length === 1);
    await settle();
    expect(seen.ended).toEqual(['reset']);
  });
});

describe('recovery_request control send', () => {
  it('frames MhClientMessage.recovery_request on the connect stream, after the connect envelope', async () => {
    const { mt, mh } = await connected();
    await mt.sendRecoveryRequest(URL_A, { slotId: 2, senderId: 300, gopCounter: 11 });
    const frames = mh.getOutboundBidiFrames(0);
    expect(frames).toHaveLength(2);
    const first = fromBinary(MhClientMessageSchema, frames[0]!);
    expect(first.message.case).toBe('connectRequest');
    const second = fromBinary(MhClientMessageSchema, frames[1]!);
    expect(second.message.case).toBe('recoveryRequest');
    expect(second.message.value).toMatchObject({ slotId: 2, senderId: 300, gopCounter: 11 });
  });

  it('serialises concurrent sends to one handler, in call order', async () => {
    const { mt, mh } = await connected();
    await Promise.all([
      mt.sendRecoveryRequest(URL_A, { slotId: 1, senderId: 300, gopCounter: 1 }),
      mt.sendRecoveryRequest(URL_A, { slotId: 1, senderId: 300, gopCounter: 2 }),
    ]);
    const counters = mh
      .getOutboundBidiFrames(0)
      .slice(1)
      .map((f) => {
        const m = fromBinary(MhClientMessageSchema, f);
        return m.message.case === 'recoveryRequest' ? m.message.value.gopCounter : -1;
      });
    expect(counters).toEqual([1, 2]);
  });

  it('after the control stream ENDS, a send reports the missing channel, not a write failure', async () => {
    const { mt, mh } = await connected();
    mh.simulateClose();
    await settle();
    const err = await mt
      .sendRecoveryRequest(URL_A, { slotId: 0, senderId: 1, gopCounter: 0 })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(MediaConnectionError);
    expect((err as Error).message).toBe('no control channel is open to that media handler');
  });

  it('after an inbound DESYNC the outbound half stays usable (entry kept deliberately)', async () => {
    const { mt, mh } = await connected();
    mh.simulateServerMessage(0, prefixOnly(MAX_MESSAGE_SIZE + 1));
    await settle();
    await mt.sendRecoveryRequest(URL_A, { slotId: 1, senderId: 2, gopCounter: 3 });
    expect(mh.getOutboundBidiFrames(0)).toHaveLength(2);
  });

  it('refuses a send to a handler with no control channel, typed and bounded', async () => {
    const { mt } = await connected();
    const err = await mt
      .sendRecoveryRequest('https://unknown.example', { slotId: 0, senderId: 1, gopCounter: 0 })
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(MediaConnectionError);
    expect((err as Error).message).not.toContain(JWT);
  });
});
