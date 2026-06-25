// File: packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts
//
// R-41 (signaling portion): unit tests for `SignalingClient` against
// `MockWebTransport`. Messages are built at runtime via protobuf-es (no hex
// fixtures); outbound frames are decoded by stripping the reused 4-byte BE prefix.

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  context,
  propagation,
  ROOT_CONTEXT,
  SpanStatusCode,
  trace,
  TraceFlags,
  type SpanContext,
} from '@opentelemetry/api';
import { TraceState } from '@opentelemetry/core';
import { MockWebTransport } from '@darktower/test-utils';

import { SignalingClient } from '../SignalingClient.js';
import type { SignalingClientOptions } from '../SignalingClient.js';
import type { SignalingJoinParams } from '../events.js';
import { ParticipantLeaveReason } from '../events.js';
import { SignalingError, SignalingErrorCode } from '../../errors/SignalingError.js';
import { MAX_MESSAGE_SIZE } from '../../framing/length-prefix.js';
import { CloseReason, normalizeCloseReason } from '../../telemetry/closeReason.js';
import {
  configureTelemetry,
  getTracer,
  resetTelemetryForTest,
} from '../../telemetry/telemetryConfig.js';
import type {
  IWebTransport,
  WebTransportBidirectionalStream,
} from '../../transport/IWebTransport.js';
import {
  ErrorCode,
  LeaveReason,
  concatBytes,
  decodeOutboundClientMessages,
  framedError,
  framedJoinResponse,
  framedParticipantJoined,
  framedParticipantJoinedEmpty,
  framedParticipantLeft,
  framedStreamPublished,
  waitFor,
  type JoinResponseInit,
} from './helpers.js';

function startJoin(
  wt: IWebTransport,
  overrides: Partial<SignalingJoinParams> = {},
  options: SignalingClientOptions = {},
): { client: SignalingClient; joinPromise: Promise<unknown> } {
  const client = new SignalingClient({ connect: () => wt, ...options });
  const joinPromise = client.join({
    webtransportEndpoint: 'https://mc.example:4433',
    meetingId: 'meet-1',
    joinToken: 'secret-jwt',
    participantName: 'Ada',
    ...overrides,
  });
  return { client, joinPromise };
}

async function reachJoined(wt: MockWebTransport, joinResp: JoinResponseInit = {}): Promise<void> {
  wt.simulateReady();
  await waitFor(() => wt.getOpenedBidiStreams().length > 0);
  wt.simulateServerMessage(0, framedJoinResponse(joinResp));
}

afterEach(() => {
  // Clear any telemetry config a test installed (also disables the propagator +
  // global context manager) so non-telemetry tests see `getTracer() === undefined`.
  resetTelemetryForTest();
  context.disable();
  propagation.disable();
});

describe('SignalingClient — connect + JoinRequest (R-16)', () => {
  it('opens the first bidi stream and sends a ClientMessage{joinRequest} with empty correlation/binding', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt, {
      meetingId: 'meet-42',
      joinToken: 'the-jwt',
      participantName: 'Ada',
    });
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    expect(wt.getOpenedBidiStreams()).toHaveLength(1);
    const messages = decodeOutboundClientMessages(wt.getOutboundBidiWrites(0));
    expect(messages).toHaveLength(1);
    const cm = messages[0]!;
    expect(cm.message.case).toBe('joinRequest');
    if (cm.message.case === 'joinRequest') {
      expect(cm.message.value.meetingId).toBe('meet-42');
      expect(cm.message.value.joinToken).toBe('the-jwt');
      expect(cm.message.value.participantName).toBe('Ada');
      expect(cm.message.value.correlationId).toBe('');
      expect(cm.message.value.bindingToken).toBe('');
    }

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('populates capabilities on the JoinRequest', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt, {
      capabilities: {
        videoCodecs: ['VP9', 'AV1'],
        audioCodecs: ['Opus'],
        supportsSimulcast: true,
        maxVideoStreams: 3,
      },
    });
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    const cm = decodeOutboundClientMessages(wt.getOutboundBidiWrites(0))[0]!;
    expect(cm.message.case).toBe('joinRequest');
    if (cm.message.case === 'joinRequest') {
      const caps = cm.message.value.capabilities;
      expect(caps?.videoCodecs).toEqual(['VP9', 'AV1']);
      expect(caps?.audioCodecs).toEqual(['Opus']);
      expect(caps?.supportsSimulcast).toBe(true);
      expect(caps?.maxVideoStreams).toBe(3);
    }

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('frames the outbound message with a 4-byte big-endian length prefix (round-trip)', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    const all = concatBytes(wt.getOutboundBidiWrites(0));
    const declaredLen = new DataView(all.buffer, all.byteOffset, 4).getUint32(0, false);
    expect(all.byteLength).toBe(4 + declaredLen);
    // And it round-trips back to a ClientMessage.
    expect(decodeOutboundClientMessages([all])[0]!.message.case).toBe('joinRequest');

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });
});

describe('SignalingClient — JoinResponse + typed events (R-17)', () => {
  it('emits onJoined with a BigInt userId, roster, and media-server URLs', async () => {
    const wt = new MockWebTransport();
    const onJoined = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('joined', onJoined);

    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    wt.simulateServerMessage(
      0,
      framedJoinResponse({
        participantId: 'p-self',
        userId: 123456789012345n,
        participants: [
          { participantId: 'p1', name: 'Bob' },
          { participantId: 'p2', name: 'Carol' },
        ],
        mediaServers: ['https://mh1.example', 'https://mh2.example'],
        correlationId: 'corr-1',
        bindingToken: 'bind-1',
      }),
    );

    const joined = (await joinPromise) as Awaited<ReturnType<SignalingClient['join']>>;
    expect(joined.participantId).toBe('p-self');
    expect(typeof joined.userId).toBe('bigint');
    expect(joined.userId).toBe(123456789012345n);
    expect(joined.existingParticipants).toEqual([
      { participantId: 'p1', name: 'Bob' },
      { participantId: 'p2', name: 'Carol' },
    ]);
    expect(joined.mediaServers).toEqual(['https://mh1.example', 'https://mh2.example']);
    expect(onJoined).toHaveBeenCalledTimes(1);

    client.close();
  });

  it('stores correlationId and bindingToken (readable via getters)', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    expect(client.correlationId).toBeUndefined();
    expect(client.bindingToken).toBeUndefined();

    await reachJoined(wt, { correlationId: 'corr-9', bindingToken: 'bind-9' });
    await joinPromise;

    expect(client.correlationId).toBe('corr-9');
    expect(client.bindingToken).toBe('bind-9');
    client.close();
  });

  it('emits onParticipantJoined for a ParticipantJoined ServerMessage', async () => {
    const wt = new MockWebTransport();
    const onJoinedParticipant = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('participantJoined', onJoinedParticipant);
    await reachJoined(wt);
    await joinPromise;

    wt.simulateServerMessage(0, framedParticipantJoined('p7', 'Dave'));
    await waitFor(() => onJoinedParticipant.mock.calls.length > 0);
    expect(onJoinedParticipant).toHaveBeenCalledWith({
      participant: { participantId: 'p7', name: 'Dave' },
    });
    client.close();
  });

  it('emits onParticipantLeft with a mapped reason', async () => {
    const wt = new MockWebTransport();
    const onLeft = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('participantLeft', onLeft);
    await reachJoined(wt);
    await joinPromise;

    wt.simulateServerMessage(0, framedParticipantLeft('p1', LeaveReason.KICKED));
    await waitFor(() => onLeft.mock.calls.length > 0);
    expect(onLeft).toHaveBeenCalledWith({
      participantId: 'p1',
      reason: ParticipantLeaveReason.Kicked,
    });
    client.close();
  });

  it('ignores a ParticipantJoined whose participant field is unset (no event, no throw)', async () => {
    const wt = new MockWebTransport();
    const onJoinedParticipant = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('participantJoined', onJoinedParticipant);
    await reachJoined(wt);
    await joinPromise;

    wt.simulateServerMessage(0, framedParticipantJoinedEmpty());
    // Follow with a real one to prove the loop kept running and only the valid one fires.
    wt.simulateServerMessage(0, framedParticipantJoined('p3', 'Faye'));
    await waitFor(() => onJoinedParticipant.mock.calls.length > 0);
    expect(onJoinedParticipant).toHaveBeenCalledTimes(1);
    expect(onJoinedParticipant).toHaveBeenCalledWith({
      participant: { participantId: 'p3', name: 'Faye' },
    });
    client.close();
  });

  it('off() unsubscribes a listener', async () => {
    const wt = new MockWebTransport();
    const onLeft = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    const unsubscribe = client.on('participantLeft', onLeft);
    await reachJoined(wt);
    await joinPromise;

    unsubscribe();
    wt.simulateServerMessage(0, framedParticipantLeft('p1', LeaveReason.VOLUNTARY));
    // Give the read loop a chance to dispatch; the listener must NOT fire.
    await new Promise((r) => setTimeout(r, 5));
    expect(onLeft).not.toHaveBeenCalled();
    client.close();
  });

  it('decodes and debug-logs an unhandled variant by case name only (no throw, no event)', async () => {
    const wt = new MockWebTransport();
    const logs: string[] = [];
    const onError = vi.fn();
    const { client, joinPromise } = startJoin(wt, {}, { logger: { debug: (m) => logs.push(m) } });
    client.on('error', onError);
    await reachJoined(wt);
    await joinPromise;

    wt.simulateServerMessage(0, framedStreamPublished('p1'));
    await waitFor(() => logs.some((l) => l.includes('streamPublished')));
    expect(logs.some((l) => l.includes('streamPublished'))).toBe(true);
    // The payload itself is never logged — only the case name.
    expect(logs.join('\n')).not.toContain('p1');
    expect(onError).not.toHaveBeenCalled();
    client.close();
  });
});

describe('SignalingClient — ErrorMessage → SignalingError (R-18)', () => {
  const EXPECTED: ReadonlyArray<readonly [ErrorCode, SignalingErrorCode, string]> = [
    [ErrorCode.UNKNOWN, SignalingErrorCode.Unknown, 'UNKNOWN'],
    [ErrorCode.INVALID_REQUEST, SignalingErrorCode.InvalidRequest, 'INVALID_REQUEST'],
    [ErrorCode.UNAUTHORIZED, SignalingErrorCode.Unauthorized, 'UNAUTHORIZED'],
    [ErrorCode.FORBIDDEN, SignalingErrorCode.Forbidden, 'FORBIDDEN'],
    [ErrorCode.NOT_FOUND, SignalingErrorCode.NotFound, 'NOT_FOUND'],
    [ErrorCode.CONFLICT, SignalingErrorCode.Conflict, 'CONFLICT'],
    [ErrorCode.INTERNAL_ERROR, SignalingErrorCode.InternalError, 'INTERNAL_ERROR'],
    [ErrorCode.CAPACITY_EXCEEDED, SignalingErrorCode.CapacityExceeded, 'CAPACITY_EXCEEDED'],
    [ErrorCode.STREAM_ERROR, SignalingErrorCode.StreamError, 'STREAM_ERROR'],
  ];

  it('maps every proto ErrorCode to its stable SignalingErrorCode (exhaustive)', async () => {
    // Guard: the table must cover every proto ErrorCode (a new code fails here).
    const numericCodes = Object.values(ErrorCode).filter(
      (v): v is ErrorCode => typeof v === 'number',
    );
    expect(EXPECTED.map(([c]) => c).sort()).toEqual([...numericCodes].sort());

    for (const [code, expectedSignalingCode, expectedName] of EXPECTED) {
      const wt = new MockWebTransport();
      const { joinPromise } = startJoin(wt);
      wt.simulateReady();
      await waitFor(() => wt.getOpenedBidiStreams().length > 0);
      wt.simulateServerMessage(0, framedError(code, ''));

      const err = await joinPromise.then(
        () => {
          throw new Error('expected rejection');
        },
        (e: unknown) => e,
      );
      expect(err).toBeInstanceOf(SignalingError);
      expect((err as SignalingError).signalingCode).toBe(expectedSignalingCode);
      expect((err as SignalingError).serverCode).toBe(expectedName);
    }
  });

  it('redacts to a fixed allowlist via toJSON (signalingCode, no token/details)', async () => {
    const wt = new MockWebTransport();
    const { joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    wt.simulateServerMessage(0, framedError(ErrorCode.CAPACITY_EXCEEDED, 'at capacity'));
    const err = (await joinPromise.catch((e: unknown) => e)) as SignalingError;

    const json = err.toJSON();
    expect(json).toMatchObject({
      name: 'SignalingError',
      code: 'SIGNALING',
      signalingCode: SignalingErrorCode.CapacityExceeded,
      serverCode: 'CAPACITY_EXCEEDED',
    });
    expect(JSON.stringify(err)).not.toContain('secret-jwt');
  });

  for (const [name, code] of [
    ['UNAUTHORIZED', ErrorCode.UNAUTHORIZED],
    ['FORBIDDEN', ErrorCode.FORBIDDEN],
  ] as const) {
    it(`closes the connection with the typed auth reason on ${name}`, async () => {
      const wt = new MockWebTransport();
      const { joinPromise } = startJoin(wt);
      wt.simulateReady();
      await waitFor(() => wt.getOpenedBidiStreams().length > 0);
      wt.simulateServerMessage(0, framedError(code, ''));

      await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
      const info = await wt.closed;
      expect(info.closeCode).toBe(3401);
      expect(info.reason).toBe('auth');
      expect(normalizeCloseReason(info.closeCode)).toBe(CloseReason.AuthFailed);
    });
  }
});

describe('SignalingClient — trace injection (R-19)', () => {
  it('populates a W3C traceParent on the outbound ClientMessage when telemetry is configured', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    const cm = decodeOutboundClientMessages(wt.getOutboundBidiWrites(0))[0]!;
    expect(cm.traceParent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-0[01]$/);
    // A fresh root span carries no vendor tracestate — do NOT require it non-empty.
    expect(cm.traceState).toBe('');

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('propagates seeded vendor tracestate onto the outbound ClientMessage', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    const wt = new MockWebTransport();

    const parent: SpanContext = {
      traceId: '0af7651916cd43dd8448eb211c80319c',
      spanId: 'b7ad6b7169203331',
      traceFlags: TraceFlags.SAMPLED,
      isRemote: true,
      traceState: new TraceState('vendor=value'),
    };
    const seededCtx = trace.setSpanContext(ROOT_CONTEXT, parent);

    // Seed the active context at the moment join() creates its root span.
    const client = new SignalingClient({ connect: () => wt });
    const joinPromise = context.with(seededCtx, () =>
      client.join({
        webtransportEndpoint: 'https://mc.example:4433',
        meetingId: 'm',
        joinToken: 'secret-jwt',
        participantName: 'Ada',
      }),
    );
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    const cm = decodeOutboundClientMessages(wt.getOutboundBidiWrites(0))[0]!;
    expect(cm.traceState).toBe('vendor=value');

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('ends the dt_client.join span with OK status exactly once on a successful join', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    // Capture the join span's lifecycle (this is the ONLY test that configures
    // telemetry AND reaches a SUCCESSFUL join, so it solely covers
    // #settleJoinSuccess's setStatus(OK) + end()).
    const tracer = getTracer()!;
    const realStartSpan = tracer.startSpan.bind(tracer);
    let endCalls = 0;
    let okStatus = false;
    let exceptionCalls = 0;
    vi.spyOn(tracer, 'startSpan').mockImplementation((name, options, ctx) => {
      const span = realStartSpan(name, options, ctx);
      vi.spyOn(span, 'end').mockImplementation(() => {
        endCalls++;
      });
      vi.spyOn(span, 'setStatus').mockImplementation((s) => {
        if (s.code === SpanStatusCode.OK) okStatus = true;
        return span;
      });
      vi.spyOn(span, 'recordException').mockImplementation(() => {
        exceptionCalls++;
      });
      return span;
    });

    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    await reachJoined(wt, { participantId: 'p-traced' });
    const joined = (await joinPromise) as { participantId: string };

    expect(joined.participantId).toBe('p-traced');
    expect(endCalls).toBe(1);
    expect(okStatus).toBe(true);
    expect(exceptionCalls).toBe(0); // success path records no exception
    client.close();
    expect(endCalls).toBe(1); // settle-once: close() does not re-end the span
  });

  it('records a BOUNDED exception on the join span — server message text never reaches telemetry', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    // Spy on the span's recordException to capture exactly what would be exported.
    const tracer = getTracer()!;
    const recorded: Array<{ name?: string; message?: string }> = [];
    const realStartSpan = tracer.startSpan.bind(tracer);
    vi.spyOn(tracer, 'startSpan').mockImplementation((name, options, ctx) => {
      const span = realStartSpan(name, options, ctx);
      vi.spyOn(span, 'recordException').mockImplementation((e) => {
        recorded.push(e as { name?: string; message?: string });
      });
      return span;
    });

    const wt = new MockWebTransport();
    const { joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    // Server-authored free-form reason carrying a sentinel that must NOT be exported.
    wt.simulateServerMessage(0, framedError(ErrorCode.UNAUTHORIZED, 'SECRET-SERVER-REASON-9f3c'));

    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
    expect(recorded).toHaveLength(1);
    // Bounded: only the stable signalingCode, never the server text.
    expect(recorded[0]!.message).toBe(SignalingErrorCode.Unauthorized);
    expect(JSON.stringify(recorded)).not.toContain('SECRET-SERVER-REASON');
  });

  it('is a safe no-op when telemetry is not configured (empty trace fields, no throw)', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOutboundBidiWrites(0).length > 0);

    const cm = decodeOutboundClientMessages(wt.getOutboundBidiWrites(0))[0]!;
    expect(cm.traceParent).toBe('');
    expect(cm.traceState).toBe('');

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });
});

describe('SignalingClient — framing edge cases (R-16/R-18)', () => {
  it('buffers a JoinResponse split across two stream chunks', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);

    const framed = framedJoinResponse({ participantId: 'p-split', userId: 7n });
    const split = Math.floor(framed.byteLength / 2);
    wt.simulateServerMessage(0, framed.slice(0, split));
    wt.simulateServerMessage(0, framed.slice(split));

    const joined = (await joinPromise) as { participantId: string };
    expect(joined.participantId).toBe('p-split');
    client.close();
  });

  it('drains multiple frames delivered in a single chunk', async () => {
    const wt = new MockWebTransport();
    const onJoined = vi.fn();
    const onParticipantJoined = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('joined', onJoined);
    client.on('participantJoined', onParticipantJoined);

    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);

    const combined = concatBytes([
      framedJoinResponse({ participantId: 'p-self' }),
      framedParticipantJoined('p9', 'Eve'),
    ]);
    wt.simulateServerMessage(0, combined);

    await joinPromise;
    await waitFor(() => onParticipantJoined.mock.calls.length > 0);
    expect(onJoined).toHaveBeenCalledTimes(1);
    expect(onParticipantJoined).toHaveBeenCalledWith({
      participant: { participantId: 'p9', name: 'Eve' },
    });
    client.close();
  });

  it('rejects an oversize frame with a Framing SignalingError', async () => {
    const wt = new MockWebTransport();
    const { joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);

    // A 4-byte prefix declaring a length above MAX_MESSAGE_SIZE — FrameDecoder
    // rejects on the prefix, before buffering any body.
    const oversize = new Uint8Array(8);
    new DataView(oversize.buffer).setUint32(0, MAX_MESSAGE_SIZE + 1, false);
    wt.simulateServerMessage(0, oversize);

    const err = (await joinPromise.catch((e: unknown) => e)) as SignalingError;
    expect(err).toBeInstanceOf(SignalingError);
    expect(err.signalingCode).toBe(SignalingErrorCode.Framing);
    // Underlying FramingError preserved on Error.cause (not in toJSON).
    expect((err as SignalingError & { cause?: unknown }).cause).toBeDefined();
    expect(err.toJSON()).not.toHaveProperty('cause');
  });
});

describe('SignalingClient — lifecycle & teardown', () => {
  it('rejects a second join() call', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    await expect(
      client.join({
        webtransportEndpoint: 'https://mc.example',
        meetingId: 'm',
        joinToken: 't',
        participantName: 'n',
      }),
    ).rejects.toBeInstanceOf(SignalingError);
    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('awaits transport.ready before opening the first bidi stream', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    // Give microtasks a chance — the stream must NOT open until ready resolves.
    await new Promise((r) => setTimeout(r, 0));
    expect(wt.getOpenedBidiStreams()).toHaveLength(0);

    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length === 1);

    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
  });

  it('rejects the pending join with a Transport error (numeric closeCode preserved) when the transport closes before join', async () => {
    const wt = new MockWebTransport();
    const { joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    wt.simulateClose(1011, 'server error');

    const err = (await joinPromise.catch((e: unknown) => e)) as SignalingError;
    expect(err).toBeInstanceOf(SignalingError);
    expect(err.signalingCode).toBe(SignalingErrorCode.Transport);
    expect(err.closeCode).toBe(1011);
  });

  it('rejects the pending join with a Transport error when the transport errors', async () => {
    const wt = new MockWebTransport();
    const { joinPromise } = startJoin(wt);
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    wt.simulateError(new Error('boom'));

    const err = (await joinPromise.catch((e: unknown) => e)) as SignalingError;
    expect(err).toBeInstanceOf(SignalingError);
    expect(err.signalingCode).toBe(SignalingErrorCode.Transport);
  });

  it('surfaces a post-join transport error via the error event (not a dangling rejection)', async () => {
    const wt = new MockWebTransport();
    const onError = vi.fn();
    const { client, joinPromise } = startJoin(wt);
    client.on('error', onError);
    await reachJoined(wt);
    await joinPromise;

    wt.simulateError(new Error('late boom'));
    await waitFor(() => onError.mock.calls.length > 0);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls[0]![0]).toBeInstanceOf(SignalingError);
    expect((onError.mock.calls[0]![0] as SignalingError).signalingCode).toBe(
      SignalingErrorCode.Transport,
    );
  });

  it('close() tears down the transport cleanly', async () => {
    const wt = new MockWebTransport();
    const { client, joinPromise } = startJoin(wt);
    await reachJoined(wt);
    await joinPromise;

    client.close();
    await expect(wt.closed).resolves.toBeDefined();
    // Idempotent.
    expect(() => client.close()).not.toThrow();
  });

  it('rejects join when the outbound stream write fails', async () => {
    const failingTransport: IWebTransport = {
      ready: Promise.resolve(),
      closed: new Promise<never>(() => {}),
      datagrams: {
        readable: new ReadableStream<Uint8Array>(),
        writable: new WritableStream<Uint8Array>(),
      },
      createBidirectionalStream(): Promise<WebTransportBidirectionalStream> {
        return Promise.resolve({
          readable: new ReadableStream<Uint8Array>(),
          writable: new WritableStream<Uint8Array>({
            write() {
              throw new Error('write failed');
            },
          }),
        });
      },
      close() {},
    };
    const { joinPromise } = startJoin(failingTransport);

    const err = (await joinPromise.catch((e: unknown) => e)) as SignalingError;
    expect(err).toBeInstanceOf(SignalingError);
    expect(err.signalingCode).toBe(SignalingErrorCode.Transport);
  });

  it('rejects join with a typed Timeout and tears down when no JoinResponse arrives', async () => {
    vi.useFakeTimers();
    try {
      // MC accepts the bidi stream + JoinRequest write, then stalls: never sends a
      // JoinResponse and never closes (MockWebTransport: no simulateServerMessage /
      // simulateClose). Without the deadline the read loop would hang forever.
      const wt = new MockWebTransport();
      const { joinPromise } = startJoin(wt, {}, { joinTimeoutMs: 10_000 });
      // Attach the rejection handler BEFORE advancing so the timer-driven reject is
      // never momentarily unhandled.
      const settled = joinPromise.catch((e: unknown) => e);
      wt.simulateReady();
      // Advancing flushes the connect chain (stream open + JoinRequest write) and
      // then fires the 10s deadline.
      await vi.advanceTimersByTimeAsync(10_000);

      expect(wt.getOpenedBidiStreams()).toHaveLength(1);
      expect(wt.getOutboundBidiWrites(0).length).toBeGreaterThan(0);
      const err = (await settled) as SignalingError;
      expect(err).toBeInstanceOf(SignalingError);
      expect(err.signalingCode).toBe(SignalingErrorCode.Timeout);
      // Torn down: transport closed (reader cancelled in teardown).
      await expect(wt.closed).resolves.toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not arm a deadline when joinTimeoutMs <= 0 (opt-out)', async () => {
    vi.useFakeTimers();
    try {
      const wt = new MockWebTransport();
      const { client, joinPromise } = startJoin(wt, {}, { joinTimeoutMs: 0 });
      let settled = false;
      void joinPromise.then(
        () => {
          settled = true;
        },
        () => {
          settled = true;
        },
      );
      wt.simulateReady();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(settled).toBe(false); // no deadline fired

      client.close(); // settle to avoid a leaked pending join
      await joinPromise.catch(() => {});
    } finally {
      vi.useRealTimers();
    }
  });
});
