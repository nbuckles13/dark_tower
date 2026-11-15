// File: packages/sdk-core/src/signaling/__tests__/leave.test.ts
//
// The deliberate leave (meeting-end safety P4; signaling.proto `LeaveRequest`):
// `close()` after a settled join sends ONE LeaveRequest, opens the leave window
// (every inbound message dropped ahead of every sink, no sends, no 'error'),
// and closes the transport when MC closes the session (its acknowledgement) or
// at the bound, whichever comes first.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockWebTransport } from '@darktower/test-utils';

import { SignalingClient } from '../SignalingClient.js';
import type { SignalingClientOptions } from '../SignalingClient.js';
import type { MeetingKekSink } from '../kekIntake.js';
import { SignalingError } from '../../errors/SignalingError.js';
import {
  DEFAULT_TEST_HANDLER_SET,
  ErrorCode,
  LeaveReason,
  decodeOutboundClientMessages,
  framedError,
  framedJoinResponse,
  framedMediaHandlerSet,
  framedMeetingKekUpdate,
  framedParticipantJoined,
  framedParticipantLeft,
  waitFor,
} from './helpers.js';

const BOUND_MS = 1_000;

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

interface Rig {
  client: SignalingClient;
  wt: MockWebTransport;
  installs: number[];
  handlerSets: unknown[];
  errors: SignalingError[];
  events: string[];
}

async function joined(options: SignalingClientOptions = {}): Promise<Rig> {
  const wt = new MockWebTransport();
  const installs: number[] = [];
  const handlerSets: unknown[] = [];
  const kekSink: MeetingKekSink = {
    install: (_kek, generation) => {
      installs.push(generation);
      return { outcome: 'installed' } as unknown as ReturnType<MeetingKekSink['install']>;
    },
  };
  const client = new SignalingClient({
    connect: () => wt,
    kekSink,
    handlerSetSink: (intake) => handlerSets.push(intake),
    leaveAckTimeoutMs: BOUND_MS,
    ...options,
  });
  const errors: SignalingError[] = [];
  const events: string[] = [];
  client.on('error', (e) => errors.push(e));
  client.on('participantJoined', () => events.push('participantJoined'));
  client.on('participantLeft', () => events.push('participantLeft'));
  const joinPromise = client.join({
    webtransportEndpoint: 'https://mc.example:4433',
    meetingId: 'meet-1',
    joinToken: 'secret-jwt',
    participantName: 'Ada',
    publish: [{ kind: 'audio', clientMuted: false }],
    receiveSlots: [],
  });
  wt.simulateReady();
  await waitFor(() => wt.getOpenedBidiStreams().length > 0);
  wt.simulateServerMessage(0, framedJoinResponse({}));
  await joinPromise;
  installs.length = 0;
  handlerSets.length = 0;
  wt.clearInspector();
  return { client, wt, installs, handlerSets, errors, events };
}

function leaveRequests(wt: MockWebTransport): number {
  return decodeOutboundClientMessages(wt.getOutboundBidiWrites(0)).filter(
    (m) => m.message.case === 'leaveRequest',
  ).length;
}

describe('SignalingClient — deliberate leave (LeaveRequest)', () => {
  it('writes the LeaveRequest BEFORE the transport closes, and closes on MC’s close without waiting out the bound', async () => {
    const { client, wt } = await joined();
    const closeSpy = vi.spyOn(wt, 'close');
    client.close();
    await waitFor(() => leaveRequests(wt) === 1);
    expect(closeSpy).not.toHaveBeenCalled();

    const started = Date.now();
    wt.simulateClose(0); // MC closes the session: the acknowledgement
    await waitFor(() => closeSpy.mock.calls.length > 0);
    expect(Date.now() - started).toBeLessThan(BOUND_MS);
    await expect(client.terminated).resolves.toBe('acknowledged');
  });

  it('drops the binding material synchronously, before anything is deferred', async () => {
    const { client } = await joined();
    expect(client.bindingToken).toBeDefined(); // positive control
    client.close();
    expect(client.bindingToken).toBeUndefined();
    expect(client.correlationId).toBeUndefined();
  });

  it('closes the transport at the bound when MC never closes', async () => {
    const { client, wt } = await joined();
    vi.useFakeTimers();
    const closeSpy = vi.spyOn(wt, 'close');
    client.close();
    await vi.advanceTimersByTimeAsync(BOUND_MS - 1);
    expect(closeSpy).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(2);
    expect(closeSpy).toHaveBeenCalled();
    expect(leaveRequests(wt)).toBe(1);
    await expect(client.terminated).resolves.toBe('timed_out');
  });

  it('closes at once when the leave write fails (the leave never arrived: MC applies its grace)', async () => {
    const { client, wt } = await joined();
    vi.useFakeTimers();
    const closeSpy = vi.spyOn(wt, 'close');
    // Make the stream's writer reject.
    vi.spyOn(WritableStream.prototype, 'getWriter').mockImplementation(() => {
      throw new Error('write failed');
    });
    client.close();
    await vi.advanceTimersByTimeAsync(0);
    expect(closeSpy).toHaveBeenCalled();
    await expect(client.terminated).resolves.toBe('write_failed');
  });

  it('drops EVERY inbound message in the window: no KEK, handler set, roster or event reaches a sink, and no error is emitted', async () => {
    const rig = await joined();
    // POSITIVE CONTROL: outside the window the same frames DO reach the sinks,
    // so the empty result below is the gate, not a malformed frame or sink.
    rig.wt.simulateServerMessage(0, framedMeetingKekUpdate(new Uint8Array(32).fill(3), 1));
    rig.wt.simulateServerMessage(0, framedMediaHandlerSet(DEFAULT_TEST_HANDLER_SET));
    rig.wt.simulateServerMessage(0, framedParticipantJoined('p-early', 'Early'));
    await waitFor(
      () => rig.installs.length > 0 && rig.handlerSets.length > 0 && rig.events.length > 0,
    );
    rig.installs.length = 0;
    rig.handlerSets.length = 0;
    rig.events.length = 0;

    rig.client.close();
    rig.wt.simulateServerMessage(0, framedMeetingKekUpdate(new Uint8Array(32).fill(7), 2));
    rig.wt.simulateServerMessage(0, framedMediaHandlerSet(DEFAULT_TEST_HANDLER_SET));
    rig.wt.simulateServerMessage(0, framedParticipantJoined('p-late', 'Late'));
    rig.wt.simulateServerMessage(0, framedParticipantLeft('p-x', LeaveReason.VOLUNTARY));
    rig.wt.simulateServerMessage(0, framedError(ErrorCode.INTERNAL_ERROR, 'x'));
    await new Promise((r) => setTimeout(r, 20));
    rig.wt.simulateClose(0);
    await new Promise((r) => setTimeout(r, 20));

    expect(rig.installs).toEqual([]);
    expect(rig.handlerSets).toEqual([]);
    expect(rig.events).toEqual([]);
    expect(rig.errors).toEqual([]);
  });

  it('refuses every send in the window', async () => {
    const { client } = await joined();
    client.close();
    await expect(client.sendMuteRequest('audio', true)).rejects.toBeInstanceOf(SignalingError);
    await expect(client.sendMediaConnectionUpdate([])).rejects.toBeInstanceOf(SignalingError);
  });

  it('a second close() in the window sends no second leave and does not shorten the wait', async () => {
    const { client, wt } = await joined();
    vi.useFakeTimers();
    const closeSpy = vi.spyOn(wt, 'close');
    client.close();
    client.close();
    await vi.advanceTimersByTimeAsync(BOUND_MS / 2);
    expect(closeSpy).not.toHaveBeenCalled();
    expect(leaveRequests(wt)).toBe(1);
    await vi.advanceTimersByTimeAsync(BOUND_MS);
    expect(closeSpy).toHaveBeenCalledTimes(1);
  });

  it('sends no leave when the join has not settled (a plain close)', async () => {
    const wt = new MockWebTransport();
    const client = new SignalingClient({ connect: () => wt, leaveAckTimeoutMs: BOUND_MS });
    const joinPromise = client.join({
      webtransportEndpoint: 'https://mc.example:4433',
      meetingId: 'meet-1',
      joinToken: 'secret-jwt',
      participantName: 'Ada',
      publish: [{ kind: 'audio', clientMuted: false }],
      receiveSlots: [],
    });
    wt.simulateReady();
    await waitFor(() => wt.getOpenedBidiStreams().length > 0);
    client.close();
    await expect(joinPromise).rejects.toBeInstanceOf(SignalingError);
    expect(leaveRequests(wt)).toBe(0);
    await expect(client.terminated).resolves.toBe('not_sent');
  });

  it('sends no leave on an error-path teardown (MC refusing the session)', async () => {
    const { wt } = await joined();
    wt.simulateServerMessage(0, framedError(ErrorCode.UNAUTHORIZED, 'expired'));
    await new Promise((r) => setTimeout(r, 20));
    expect(leaveRequests(wt)).toBe(0);
  });
});
