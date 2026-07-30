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
import { MeetingUnauthorizedError } from '../../errors/MeetingError.js';
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

const TOKEN_CREDS: JoinCredentials = {
  mode: 'token',
  userToken: 'caller-supplied-user-token',
  displayName: 'Ada',
};

const REGISTER: JoinCredentials = {
  mode: 'register',
  email: 'ada@demo.test',
  password: 'hunter2hunter2',
  displayName: 'Ada',
};

/**
 * A `fetch` that RECORDS every request URL, so a negative can be asserted on the
 * recorded list rather than on per-endpoint booleans.
 *
 * Spy rather than MSW deliberately (MSW *is* a dependency and is used at the
 * HTTP-client tier in `http/__tests__/`). Two reasons: this is the
 * session-orchestration tier, whose established idiom is an injected `fetchImpl`; and
 * MSW intercepts and SERVES, so a handler-based test proves what a response looked
 * like, not that no request happened — a regression that re-added the AC call would
 * get a well-formed register response and pass.
 */
function recordingFetch(): { fetchImpl: FetchLike; urls: string[] } {
  const urls: string[] = [];
  const inner = fakeFetch();
  const fetchImpl = ((input: string | URL, init?: RequestInit) => {
    urls.push(typeof input === 'string' ? input : input.toString());
    return inner(input, init);
  }) as unknown as FetchLike;
  return { fetchImpl, urls };
}

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

  // ---------------------------------------------------------------------------
  // Task #58 (e): join with a token makes NO auth call
  // ---------------------------------------------------------------------------

  it('(e) join with mode:token issues NO request to the AC origin and presents the caller token to GC', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const { fetchImpl, urls } = recordingFetch();
    const session = makeSession(mocks, { fetchImpl });

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    // ALLOWLIST, not an enumerated denylist: asserting "no /auth/register and no
    // /auth/user/token" stays green if a future change routes through some OTHER AC
    // endpoint (refresh, introspect). Assert nothing hit the AC origin at all, on the
    // recorded URL list, so an unexpected third endpoint surfaces too.
    const acOrigin = AC_TEMPLATE.replace('{subdomain}', 'demo');
    expect(urls.filter((u) => u.startsWith(acOrigin))).toEqual([]);

    // POSITIVE outcome in the same test: without this the negative above is satisfied
    // by an early throw before `#authenticate` is ever reached.
    const gcJoins = urls.filter((u) => u.includes('/api/v1/meetings/'));
    expect(gcJoins).toHaveLength(1);
    expect(session.state).toBe(MeetingSessionState.Joined);
  });

  it('(e) CONTROL: the same recording spy DOES capture an AC call on the register path', async () => {
    // Proves the spy is wired. Without this control, a mis-wired spy makes the
    // zero-AC-request assertion above green forever.
    const mocks = new Map<string, MockWebTransport>();
    const { fetchImpl, urls } = recordingFetch();
    const session = makeSession(mocks, { fetchImpl });

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: REGISTER,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    expect(urls.some((u) => u.includes('/api/v1/auth/register'))).toBe(true);
  });

  it('(e) the token path still populates participantName on the MC JoinRequest', async () => {
    // `#joinSignaling` reads `options.credentials.displayName` as an UN-NARROWED union
    // property access. It compiles only because every variant declares `displayName`.
    // A later variant omitting it, "fixed" by narrowing to silence the compile error,
    // would ship nameless participants while the zero-AC-request assertion above
    // stayed green. Pin the behaviour, not just the type (@paired-client F2).
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    const mcMsgs = decodeClientMessages(mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0));
    const joinReq = mcMsgs.find((m) => m.message.case === 'joinRequest');
    expect(joinReq).toBeDefined();
    if (joinReq?.message.case !== 'joinRequest') throw new Error('unreachable');
    expect(joinReq.message.value.participantName).toBe('Ada');
  });

  it('(e) a malformed caller token is rejected locally, before any network call', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const { fetchImpl, urls } = recordingFetch();
    const session = makeSession(mocks, { fetchImpl });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        // CRLF injection attempt against the `Authorization: Bearer` header.
        credentials: { mode: 'token', userToken: 'abc\r\nX-Injected: 1' },
      }),
    ).rejects.toThrow();
    expect(urls).toEqual([]);
  });

  it('(F3) a GC 401 at join is reported as credential_invalid, not gc_join', async () => {
    // @observability F3: before the token path the user token handed to joinMeeting was
    // seconds old and SDK-minted, so a credential rejection there was rare. A
    // caller-supplied token of arbitrary age makes it a ROUTINE gc_join outcome — which
    // would otherwise mix "meeting join denied" with "your token is dead" under one
    // label, two populations with different oncall responses.
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const unauthorizedFetch = ((input: string | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.includes('/api/v1/meetings/')) {
        return Promise.resolve(
          new Response(JSON.stringify({ error: { code: 'unauthorized', message: 'bad token' } }), {
            status: 401,
            headers: JSON_HEADERS,
          }),
        );
      }
      return Promise.resolve(new Response('not found', { status: 404 }));
    }) as unknown as FetchLike;
    const session = makeSession(mocks, { metricsSink: sink, fetchImpl: unauthorizedFetch });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: TOKEN_CREDS,
      }),
    ).rejects.toBeInstanceOf(MeetingUnauthorizedError);

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'credential_invalid' },
      1,
    );
  });

  it('(F3) a GC 404 at join stays gc_join — the refinement is credential-specific', async () => {
    // Control for the test above: without it, a gcJoinFailureStage that returned
    // credential_invalid for EVERY GC failure would pass and the split would be
    // meaningless.
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const notFoundFetch = ((input: string | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.includes('/api/v1/meetings/')) {
        return Promise.resolve(
          new Response(JSON.stringify({ error: { code: 'not_found', message: 'no meeting' } }), {
            status: 404,
            headers: JSON_HEADERS,
          }),
        );
      }
      return Promise.resolve(new Response('not found', { status: 404 }));
    }) as unknown as FetchLike;
    const session = makeSession(mocks, { metricsSink: sink, fetchImpl: notFoundFetch });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: TOKEN_CREDS,
      }),
    ).rejects.toBeTruthy();

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'gc_join' },
      1,
    );
  });

  it('(F-O1) a malformed meeting code stays gc_join — NOT credential_invalid', async () => {
    // `joinMeeting` runs `validateMeetingCode` as its first statement, so a user's typo
    // throws a ValidationError inside the GC-join try block. Routing that to
    // `credential_invalid` would page oncall toward auth for a mistyped code.
    // The 404 control could not catch this: a malformed code never reaches HTTP, so it
    // is outside that control's sample space (@observability F-O1).
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: 'BAD!!',
        credentials: TOKEN_CREDS,
      }),
    ).rejects.toBeTruthy();

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'gc_join' },
      1,
    );
  });

  it('(C1) a GC 403 stays gc_join — an authorization denial is not a dead credential', async () => {
    // GC returns 403 for decisions on a VALID, live token: org meeting limit exceeded,
    // insufficient permissions, external participants not allowed. Its contract is
    // explicit — 401 = invalid/missing token, 403 = user not allowed to join. Routing
    // 403 to credential_invalid puts the primary "meeting join denied" status into the
    // credential bucket, which is the population mixing the refinement exists to
    // prevent (@paired-client C1).
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const forbiddenFetch = ((input: string | URL) => {
      const url = typeof input === 'string' ? input : input.toString();
      if (url.includes('/api/v1/meetings/')) {
        return Promise.resolve(
          new Response(
            JSON.stringify({ error: { code: 'forbidden', message: 'org meeting limit exceeded' } }),
            { status: 403, headers: JSON_HEADERS },
          ),
        );
      }
      return Promise.resolve(new Response('not found', { status: 404 }));
    }) as unknown as FetchLike;
    const session = makeSession(mocks, { metricsSink: sink, fetchImpl: forbiddenFetch });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: TOKEN_CREDS,
      }),
    ).rejects.toBeTruthy();

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'gc_join' },
      1,
    );
  });

  it('an unknown credential mode reports `internal`, not `signup`', async () => {
    // An unrecognized mode is an SDK-contract violation, not an AC call that failed —
    // `internal` is the one case where that bucket is genuinely right (@observability).
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { metricsSink: sink });

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: {
          mode: 'sso',
          email: 'a@b.test',
          password: 'x',
        } as unknown as JoinCredentials,
      }),
    ).rejects.toThrow();

    sink.assertCounter(
      'dt_client_join_attempts_total',
      { status: 'failure', failure_stage: 'internal' },
      1,
    );
  });

  it('the exhaustiveness throw carries the MODE ONLY — never the credential object', async () => {
    // The branch exists for when the type contract is violated, so it must assume `c` is
    // a real credentials object. Serializing it would put `password` on Error.message,
    // which propagates out of join() to an SDK embedder (@security F-SEC-4 + three others).
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: {
          mode: 'sso',
          email: 'ada@demo.test',
          password: 'hunter2hunter2',
        } as unknown as JoinCredentials,
      }),
    ).rejects.toThrow(
      expect.objectContaining({
        message: expect.not.stringContaining('hunter2hunter2') as unknown as string,
      }),
    );
  });

  it('#authenticate throws on an unknown credential mode rather than silently mis-routing', async () => {
    // The `never` default is unreachable through the type system — that is the point.
    // But the runtime guard is what protects a caller who bypasses types (a JS consumer,
    // a cast, a deserialized value), and an untested `default` is exactly where a future
    // variant would silently take the wrong branch.
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);

    await expect(
      session.join({
        orgSubdomain: 'demo',
        meetingCode: MEETING_CODE,
        credentials: {
          mode: 'sso',
          email: 'a@b.test',
          password: 'x',
        } as unknown as JoinCredentials,
      }),
    ).rejects.toThrow(/unhandled credential mode/);
  });
});
