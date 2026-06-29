// File: packages/sdk-core/src/session/__tests__/meeting-session.test.ts
//
// R-41: unit tests for the `MeetingSession` facade — the full join orchestration
// (auth → GC token → MC signaling → MH handshake → MediaConnectionUpdate), the
// explicit state machine, high-level event emission, clean disconnect, and the
// five `dt_client_*` join metrics. The MC + MH transports are `MockWebTransport`s
// driven through a url→mock `connect` factory; AC/GC are a fake `fetch`.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { context, propagation } from '@opentelemetry/api';
import { InMemoryMetricsSink, type MockWebTransport } from '@darktower/test-utils';

import { MeetingSession } from '../MeetingSession.js';
import { MeetingSessionState } from '../events.js';
import type { JoinCredentials, MeetingSessionOptions } from '../events.js';
import { MediaConnectionError } from '../../errors/MediaConnectionError.js';
import { ConnectionState } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { FetchLike } from '../../http/types.js';
import {
  configureTelemetry,
  getTracer,
  resetTelemetryForTest,
} from '../../telemetry/telemetryConfig.js';
import {
  ErrorCode,
  framedError,
  framedJoinResponse,
  framedParticipantJoined,
} from '../../signaling/__tests__/helpers.js';
import {
  decodeClientMessages,
  decodeMhClientMessages,
  makeConnect,
  waitFor,
} from '../../media/__tests__/helpers.js';

const AC_TEMPLATE = 'https://{subdomain}.localhost:8443';
const GC_BASE = 'https://gc.localhost:8444';
const MEETING_CODE = 'abc123ABC456'; // 12 base62
const MC_ENDPOINT = 'https://mc.localhost:4433';
const MEETING_ID = 'mtg-uuid-00000000';
const MEDIA_SERVERS = ['https://mh-a.example:4433', 'https://mh-b.example:4433'];

const JSON_HEADERS = { 'content-type': 'application/json' };

function fakeFetch(): FetchLike {
  return (async (input: string | URL) => {
    const url = typeof input === 'string' ? input : input.toString();
    if (url.includes('/api/v1/auth/user/token')) {
      return new Response(
        JSON.stringify({ accessToken: 'user-jwt', tokenType: 'Bearer', expiresIn: 3600 }),
        { status: 200, headers: JSON_HEADERS },
      );
    }
    if (url.includes('/api/v1/auth/register')) {
      return new Response(
        JSON.stringify({
          userId: 'u-1',
          email: 'ada@demo.test',
          displayName: 'Ada',
          accessToken: 'user-jwt',
          tokenType: 'Bearer',
          expiresIn: 3600,
        }),
        { status: 200, headers: JSON_HEADERS },
      );
    }
    if (url.includes('/api/v1/meetings/')) {
      return new Response(
        JSON.stringify({
          token: 'meeting-jwt',
          expiresIn: 3600,
          meetingId: MEETING_ID,
          meetingName: 'Standup',
          mcAssignment: {
            mcId: 'mc1',
            webtransportEndpoint: MC_ENDPOINT,
            grpcEndpoint: 'https://mc.localhost:50051',
          },
        }),
        { status: 200, headers: JSON_HEADERS },
      );
    }
    return new Response('not found', { status: 404 });
  }) as unknown as FetchLike;
}

const LOGIN: JoinCredentials = {
  mode: 'login',
  email: 'ada@demo.test',
  password: 'hunter2hunter2',
  displayName: 'Ada',
};

function makeSession(
  mocks: Map<string, MockWebTransport>,
  extra: Partial<MeetingSessionOptions> = {},
): MeetingSession {
  return new MeetingSession({
    acOriginTemplate: AC_TEMPLATE,
    gcBaseUrl: GC_BASE,
    fetchImpl: fakeFetch(),
    connect: makeConnect(mocks),
    clock: () => 1_000,
    connectTimeoutMs: 5_000,
    ...extra,
  });
}

/** Drive the MC mock to a joined state, then settle the MH mocks per `mhOutcome`. */
async function driveJoin(
  mocks: Map<string, MockWebTransport>,
  mhOutcome: 'all' | 'partial' | 'none',
): Promise<void> {
  // 1) MC: ready → JoinRequest stream opens → feed JoinResponse with media servers.
  await waitFor(() => mocks.has(MC_ENDPOINT));
  const mc = mocks.get(MC_ENDPOINT)!;
  mc.simulateReady();
  await waitFor(() => mc.getOpenedBidiStreams().length > 0);
  mc.simulateServerMessage(0, framedJoinResponse({ mediaServers: MEDIA_SERVERS }));

  // 2) MH: drive each per the requested outcome.
  await waitFor(() => MEDIA_SERVERS.every((u) => mocks.has(u)));
  MEDIA_SERVERS.forEach((url, i) => {
    const mh = mocks.get(url)!;
    const ok = mhOutcome === 'all' || (mhOutcome === 'partial' && i === 0);
    if (ok) mh.simulateReady();
    else mh.simulateError(new Error('mh refused'));
  });
}

afterEach(() => {
  resetTelemetryForTest();
  context.disable();
  propagation.disable();
  vi.restoreAllMocks();
});

describe('MeetingSession.join — state machine (R-22)', () => {
  it('walks idle→fetching-token→connecting-mc→joining→joined and emits joined+mediaConnected', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const states: string[] = [];
    const mediaConnected: string[] = [];
    let joinedFired = false;
    session.on('stateChange', (s) => states.push(s));
    session.on('mediaConnected', (u) => mediaConnected.push(u));
    session.on('joined', () => {
      joinedFired = true;
    });

    expect(session.state).toBe(MeetingSessionState.Idle);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    const joined = await joinP;

    expect(states).toEqual([
      MeetingSessionState.FetchingToken,
      MeetingSessionState.ConnectingMc,
      MeetingSessionState.Joining,
      MeetingSessionState.Joined,
    ]);
    expect(session.state).toBe(MeetingSessionState.Joined);
    expect(joinedFired).toBe(true);
    expect(mediaConnected.sort()).toEqual([...MEDIA_SERVERS].sort());
    expect(joined.mediaServers).toEqual(MEDIA_SERVERS);
  });

  it('bridges participantJoined from the signaling layer', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const participants: string[] = [];
    session.on('participantJoined', (e) => participants.push(e.participant.participantId));

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    mocks.get(MC_ENDPOINT)!.simulateServerMessage(0, framedParticipantJoined('p-99', 'Zoe'));
    await waitFor(() => participants.length > 0);
    expect(participants).toEqual(['p-99']);
    session.disconnect();
  });

  it('all-MH-fail → still sends the MediaConnectionUpdate, rejects, and ends in disconnecting', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const states: string[] = [];
    session.on('stateChange', (s) => states.push(s));

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'none');
    await expect(joinP).rejects.toBeInstanceOf(MediaConnectionError);

    // R-21 / @operations: the MediaConnectionUpdate IS sent on the MC stream on the
    // all-fail REJECT branch (the "ALWAYS send afterwards" behavior), so MC learns
    // every MH failed and is not left waiting. Pin it by CONTENT, not just frame count:
    // decode the MC stream and assert the update is present with ALL statuses FAILED.
    const mcMsgs = decodeClientMessages(mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0));
    const update = mcMsgs.find((m) => m.message.case === 'mediaConnectionUpdate');
    expect(update).toBeDefined();
    if (update?.message.case !== 'mediaConnectionUpdate') throw new Error('unreachable');
    expect(update.message.value.statuses).toHaveLength(MEDIA_SERVERS.length);
    expect(update.message.value.statuses.every((s) => s.state === ConnectionState.FAILED)).toBe(
      true,
    );
    expect(states).toContain(MeetingSessionState.Disconnecting);
    expect(session.state).toBe(MeetingSessionState.Disconnecting);
  });

  it('wire-level redaction (F-T1): a JWT-echoing MH failure leaves the on-wire MhConnectionStatus failureReason/failureCode JWT-clean', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });

    // MC joins, then EVERY MH becomes ready and rejects the bidi-open with an error
    // whose message ECHOES the meeting JWT ('meeting-jwt' from fakeFetch) — the
    // worst-case leak vector for the client-controlled MhConnectionStatus fields.
    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    await waitFor(() => mc.getOpenedBidiStreams().length > 0);
    mc.simulateServerMessage(0, framedJoinResponse({ mediaServers: MEDIA_SERVERS }));
    await waitFor(() => MEDIA_SERVERS.every((u) => mocks.has(u)));
    for (const url of MEDIA_SERVERS) {
      const mh = mocks.get(url)!;
      vi.spyOn(mh, 'createBidirectionalStream').mockRejectedValue(
        new Error('mh handshake echoed token=meeting-jwt'),
      );
      mh.simulateReady();
    }
    await joinP.catch(() => {});

    // Decode the ACTUAL outbound MediaConnectionUpdate proto off the MC wire.
    const mcMsgs = decodeClientMessages(mc.getOutboundBidiWrites(0));
    const update = mcMsgs.find((m) => m.message.case === 'mediaConnectionUpdate');
    expect(update).toBeDefined();
    if (update?.message.case !== 'mediaConnectionUpdate') throw new Error('unreachable');
    expect(update.message.value.statuses).toHaveLength(MEDIA_SERVERS.length);
    // F-T1: the proto-flagged client-controlled leak surface is JWT-clean on the wire
    // (failureReason/failureCode are SDK-static; the cause never reaches them).
    for (const s of update.message.value.statuses) {
      expect(s.failureReason).not.toContain('meeting-jwt');
      expect(s.failureCode).not.toContain('meeting-jwt');
    }
    // Belt-and-suspenders: the whole decoded update is JWT-free (BigInt-safe
    // serialize — the proto `observedAt` timestamp is a bigint).
    const serialized = JSON.stringify(update, (_k, v) =>
      typeof v === 'bigint' ? v.toString() : v,
    );
    expect(serialized).not.toContain('meeting-jwt');
  });
});

describe('MeetingSession.configure', () => {
  it('delegates to configureTelemetry and returns a metrics sink', () => {
    const sink = MeetingSession.configure({
      telemetryEndpoint: 'https://gc.example/api/v1/telemetry',
      env: 'test',
    });
    expect(sink).toBeDefined();
    expect(typeof sink.counter).toBe('function');
  });
});

describe('MeetingSession.disconnect (R-22/R-23)', () => {
  it('tears down media + signaling transports, idempotently', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    const mcClose = vi.spyOn(mocks.get(MC_ENDPOINT)!, 'close');
    const mhClose = vi.spyOn(mocks.get(MEDIA_SERVERS[0]!)!, 'close');

    session.disconnect();
    session.disconnect(); // idempotent

    expect(session.state).toBe(MeetingSessionState.Disconnecting);
    expect(mcClose).toHaveBeenCalledTimes(1);
    expect(mhClose).toHaveBeenCalledTimes(1);
  });

  it('still tears down transports even if a stateChange(Disconnecting) listener throws (@operations)', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    const mcClose = vi.spyOn(mocks.get(MC_ENDPOINT)!, 'close');
    const mhClose = vi.spyOn(mocks.get(MEDIA_SERVERS[0]!)!, 'close');
    // A consumer listener that throws on the Disconnecting transition must NOT be able
    // to leak the MC/MH connections — teardown runs in a finally.
    session.on('stateChange', (s) => {
      if (s === MeetingSessionState.Disconnecting) throw new Error('listener boom');
    });

    expect(() => session.disconnect()).toThrow('listener boom');
    // Resources were released despite the throwing listener.
    expect(mcClose).toHaveBeenCalledTimes(1);
    expect(mhClose).toHaveBeenCalledTimes(1);
  });
});

describe('MeetingSession — join metrics (R-24/R-25 M2)', () => {
  it('emits all five metrics with the PII-clean label set on the happy path', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'success', failure_stage: 'none' },
      1,
    );
    sink.assertHistogramObserved('dt_client_time_to_signaling_ready_ms', {});
    sink.assertHistogramObserved('dt_client_time_to_first_mh_connected_ms', {});
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'success', close_reason: 'normal' },
      1,
    );
    sink.assertCounterAtLeast('dt_client_mh_connection_total', { status: 'success' }, 2);

    // Label set: client_version + org_id + 16-hex meeting_id_hash; no PII.
    const sample = sink.getRecordedMetrics()[0]!;
    expect(sample.labels['org_id']).toBe('demo');
    expect(sample.labels['client_version']).toBe('0.0.0-test');
    expect(sample.labels['meeting_id_hash']).toMatch(/^[0-9a-f]{16}$/);
    const allKeys = new Set(sink.getRecordedMetrics().flatMap((m) => Object.keys(m.labels)));
    for (const banned of ['email', 'user_id', 'password', 'meeting_id']) {
      expect(allKeys.has(banned)).toBe(false);
    }
  });

  it('emits join_attempts_total{status:failure, failure_stage:mh_connect} on all-MH-fail', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'none');
    await joinP.catch(() => {});

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'mh_connect' },
      1,
    );
    // The MC connection itself succeeded → success/normal on the signaling counter.
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'success', close_reason: 'normal' },
      1,
    );
  });

  it('does NOT emit any join log (R-26 deferred) — no console.info join lines', async () => {
    const infoSpy = vi.spyOn(console, 'info').mockImplementation(() => {});
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;
    const joinLogs = infoSpy.mock.calls.filter((c) => String(c[0]).includes('[dt-join]'));
    expect(joinLogs).toHaveLength(0);
    session.disconnect();
  });
});

describe('MeetingSession.join — failure paths (R-22)', () => {
  const GC_BODY = {
    token: 'meeting-jwt',
    expiresIn: 3600,
    meetingId: MEETING_ID,
    meetingName: 'Standup',
    mcAssignment: { mcId: 'mc1', webtransportEndpoint: MC_ENDPOINT, grpcEndpoint: 'g' },
  };

  function statusFetch(opts: { authStatus?: number; gcStatus?: number }): FetchLike {
    return (async (input: string | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.includes('/api/v1/auth/')) {
        if (opts.authStatus && opts.authStatus !== 200) {
          return new Response(JSON.stringify({ error: { code: 'UNAUTHORIZED', message: 'bad' } }), {
            status: opts.authStatus,
            headers: JSON_HEADERS,
          });
        }
        return new Response(
          JSON.stringify({ accessToken: 'user-jwt', tokenType: 'Bearer', expiresIn: 3600 }),
          { status: 200, headers: JSON_HEADERS },
        );
      }
      if (url.includes('/api/v1/meetings/')) {
        if (opts.gcStatus && opts.gcStatus !== 200) {
          return new Response(JSON.stringify({ error: { code: 'NOT_FOUND', message: 'no' } }), {
            status: opts.gcStatus,
            headers: JSON_HEADERS,
          });
        }
        return new Response(JSON.stringify(GC_BODY), { status: 200, headers: JSON_HEADERS });
      }
      return new Response('nf', { status: 404 });
    }) as unknown as FetchLike;
  }

  it('register credentials drive the happy path (register branch)', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const credentials: JoinCredentials = {
      mode: 'register',
      email: 'new@demo.test',
      password: 'hunter2hunter2',
      displayName: 'Newbie',
    };
    const joinP = session.join({ orgSubdomain: 'demo', meetingCode: MEETING_CODE, credentials });
    await driveJoin(mocks, 'all');
    await expect(joinP).resolves.toBeDefined();
    expect(session.state).toBe(MeetingSessionState.Joined);
  });

  it('auth failure → rejects, failure_stage signup, no signaling counter, lands disconnecting', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, {
      fetchImpl: statusFetch({ authStatus: 401 }),
      metricsSink: sink,
    });
    await expect(
      session.join({ orgSubdomain: 'demo', meetingCode: MEETING_CODE, credentials: LOGIN }),
    ).rejects.toBeDefined();

    expect(session.state).toBe(MeetingSessionState.Disconnecting);
    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'signup' },
      1,
    );
    expect(sink.getCounter('dt_client_signaling_connection_total', {})).toBe(0);
  });

  it('GC join failure → rejects, failure_stage gc_join', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, {
      fetchImpl: statusFetch({ gcStatus: 404 }),
      metricsSink: sink,
    });
    await expect(
      session.join({ orgSubdomain: 'demo', meetingCode: MEETING_CODE, credentials: LOGIN }),
    ).rejects.toBeDefined();
    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'gc_join' },
      1,
    );
  });

  it('MC server REJECTS the join (UNAUTHORIZED ErrorMessage) → stage mc_join_response, close_reason auth_failed', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink, joinTimeoutMs: 5_000 });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });

    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    await waitFor(() => mc.getOpenedBidiStreams().length > 0);
    // The channel OPENED, then the server sent an auth-class ErrorMessage instead of a
    // JoinResponse → a server join-REJECTION → stage mc_join_response (NOT connect).
    mc.simulateServerMessage(0, framedError(ErrorCode.UNAUTHORIZED, 'bad token'));

    await expect(joinP).rejects.toBeDefined();
    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'mc_join_response' },
      1,
    );
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'failure', close_reason: 'auth_failed' },
      1,
    );
  });

  it('MC connect-level failure (join deadline, no server response) → stage mc_signaling_connect, close_reason timeout', async () => {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink, joinTimeoutMs: 20 });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });

    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    // Never feed a JoinResponse → the join deadline elapses → SignalingErrorCode.Timeout,
    // a LOCAL cause (no server response) → stage mc_signaling_connect.
    await expect(joinP).rejects.toBeDefined();

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'mc_signaling_connect' },
      1,
    );
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'failure', close_reason: 'timeout' },
      1,
    );
  });
});

describe('MeetingSession — signaling close_reason mapping (R-25)', () => {
  async function joinWithMcError(code: ErrorCode): Promise<InMemoryMetricsSink> {
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    await waitFor(() => mc.getOpenedBidiStreams().length > 0);
    mc.simulateServerMessage(0, framedError(code, 'x'));
    await joinP.catch(() => {});
    return sink;
  }

  it('INTERNAL_ERROR → close_reason server_error', async () => {
    const sink = await joinWithMcError(ErrorCode.INTERNAL_ERROR);
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'failure', close_reason: 'server_error' },
      1,
    );
  });

  it('NOT_FOUND (no close code) → close_reason unknown (default mapping)', async () => {
    const sink = await joinWithMcError(ErrorCode.NOT_FOUND);
    sink.assertCounter(
      'dt_client_signaling_connection_total',
      { status: 'failure', close_reason: 'unknown' },
      1,
    );
  });
});

describe('MeetingSession — R-58 end-to-end trace threading', () => {
  it('MH envelope + MediaConnectionUpdate trace_id EQUALS the join span trace_id, after real awaits, under the real StackContextManager (Lead-mandated R-58 regression guard)', async () => {
    configureTelemetry({ telemetryEndpoint: 'https://gc.example/api/v1/telemetry', env: 'test' });
    // Capture the REAL join span's trace_id (the dt_client.join span SignalingClient
    // creates) — NOT a hand-set active span — so the equality assertion proves the
    // threading survives the real async path (auth → token → join → connectAll) under
    // the actually-registered synchronous StackContextManager.
    const tracer = getTracer()!;
    const realStartSpan = tracer.startSpan.bind(tracer);
    let joinSpanTraceId: string | undefined;
    vi.spyOn(tracer, 'startSpan').mockImplementation((name, opts, ctx) => {
      const span = realStartSpan(name, opts, ctx);
      if (name === 'dt_client.join') joinSpanTraceId = span.spanContext().traceId;
      return span;
    });

    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;
    expect(joinSpanTraceId).toMatch(/^[0-9a-f]{32}$/);

    // MH envelope (browser→MH) — injected via the threaded runInJoinContext under the
    // active dt_client.join span (NOT bare context.active(), which would be empty).
    const mhEnv = decodeMhClientMessages(
      mocks.get(MEDIA_SERVERS[0]!)!.getOutboundBidiWrites(0),
    )[0]!;
    expect(mhEnv.traceParent).toMatch(/^00-[0-9a-f]{32}-[0-9a-f]{16}-0[01]$/);
    const mhTraceId = mhEnv.traceParent.split('-')[1];

    // MediaConnectionUpdate (browser→MC) — injected on the same join span.
    const mcMsgs = decodeClientMessages(mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0));
    const update = mcMsgs.find((m) => m.message.case === 'mediaConnectionUpdate')!;
    expect(update.traceParent).toMatch(/^00-[0-9a-f]{32}-/);
    const updateTraceId = update.traceParent.split('-')[1];

    // HARD equality: both outbound envelopes' trace_id EQUALS the actual dt_client.join
    // span's trace_id, captured across the real awaits. Empty/root would fail this —
    // this is the regression guard that proves production threading works (not a
    // hand-set span around the inject).
    expect(mhTraceId).toBe(joinSpanTraceId);
    expect(updateTraceId).toBe(joinSpanTraceId);
    session.disconnect();
  });
});
