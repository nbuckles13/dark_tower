// File: packages/sdk-core/src/session/__tests__/meeting-session.test.ts
//
// R-41: unit tests for the `MeetingSession` facade — the full join orchestration
// (auth → GC token → MC signaling → MH handshake → MediaConnectionUpdate), the
// explicit state machine, high-level event emission, clean disconnect, and the
// five `dt_client_*` join metrics. The MC + MH transports are `MockWebTransport`s
// driven through a url→mock `connect` factory; AC/GC are a fake `fetch`.

import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  FakeAudioCodecs,
  FakeCaptureSource,
  makeAudioData,
  InMemoryMetricsSink,
  RecordingPlaybackSink,
  FakeVideoDecoders,
  type MockWebTransport,
} from '@darktower/test-utils';

import { create, toBinary } from '@bufbuild/protobuf';

import { MeetingSession } from '../MeetingSession.js';
import { encodeFrame } from '../../framing/length-prefix.js';
import { RosterIdentityKeys } from '../../media/setup/rosterKeys.js';
import { MeetingSessionState } from '../events.js';
import type { JoinCredentials, MeetingSessionOptions } from '../events.js';
import { MediaConnectionError } from '../../errors/MediaConnectionError.js';
import { SignalingError, SignalingErrorCode } from '../../errors/SignalingError.js';
import {
  ClientConfigError,
  DEFAULT_CLIENT_CONFIG,
  DEFAULT_RECEIVE_AUDIO_SLOTS,
  DEFAULT_RECEIVE_VIDEO_SLOTS,
} from '../../config/clientConfig.js';
import { selectCaptureSource } from '../mediaSelection.js';
import { TransmitKeyManager } from '../../media/lifecycle/transmitKeys.js';
import { VideoCodecUnsupportedError } from '../../media/setup/vp8.js';
import { Codec } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { createMicrophoneCapture } from '../../media/setup/capture.js';
import {
  JOIN_BAG_MARKER,
  joinBagViolations,
  readGrandfatheredRoster,
} from '../../__tests__/joinLabelRoster.js';
import { MeetingUnauthorizedError } from '../../errors/MeetingError.js';
import {
  ConnectionState,
  KeyframeRequestSchema,
  MediaKind,
  MhServerMessageSchema,
  SlotState,
} from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import type { FetchLike } from '../../http/types.js';
import {
  configureTelemetry,
  getTracer,
  resetTelemetryForTest,
} from '../../telemetry/telemetryConfig.js';
import {
  ErrorCode,
  decodeOutboundClientMessages,
  framedError,
  framedJoinResponse,
  framedParticipantJoined,
  framedSendDirective,
  framedSendDirectiveWithoutStreams,
  framedStreamAssignments,
  framedParticipantMuteUpdate,
  framedUnmuteRequestRelay,
  framedMeetingKekUpdate,
  framedParticipantLeft,
} from '../../signaling/__tests__/helpers.js';
import { VideoReceiver } from '../../media/lifecycle/VideoReceiver.js';
import { AudioPipeline } from '../../media/lifecycle/AudioPipeline.js';
import type { VideoSlotRenderChangedEvent } from '../events.js';
import { LeaveReason } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
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
    // The node tier has no WebCodecs: a passing VP8 probe by default (the probe
    // itself and its failure path are tested on their own).
    vp8Probe: () => Promise.resolve(),
    ...extra,
  });
}

/**
 * Drive the MC mock to a joined state, then settle the MH mocks per `mhOutcome`.
 *
 * `joinInit` widens the JoinResponse for callers that need more than the default
 * — notably a `senderId`, without which `startMedia()` fails closed (no sender id
 * means no key id, and this SDK sends no unsigned frames under any degradation).
 */
async function driveJoin(
  mocks: Map<string, MockWebTransport>,
  mhOutcome: 'all' | 'partial' | 'none',
  joinInit: Parameters<typeof framedJoinResponse>[0] = {},
): Promise<void> {
  // 1) MC: ready → JoinRequest stream opens → feed JoinResponse with media servers.
  await waitFor(() => mocks.has(MC_ENDPOINT));
  const mc = mocks.get(MC_ENDPOINT)!;
  mc.simulateReady();
  await waitFor(() => mc.getOpenedBidiStreams().length > 0);
  mc.simulateServerMessage(0, framedJoinResponse({ mediaServers: MEDIA_SERVERS, ...joinInit }));

  // 2) MH: drive each per the requested outcome.
  await waitFor(() => MEDIA_SERVERS.every((u) => mocks.has(u)));
  MEDIA_SERVERS.forEach((url, i) => {
    const mh = mocks.get(url)!;
    const ok = mhOutcome === 'all' || (mhOutcome === 'partial' && i === 0);
    if (ok) mh.simulateReady();
    else mh.simulateError(new Error('mh refused'));
  });
}

afterEach(async () => {
  await resetTelemetryForTest();
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
    // The join-flow sample (media zero-initialisations carry the media set, not this one).
    const sample = sink
      .getRecordedMetrics()
      .find((m) => m.name === 'dt_client_join_attempts_total')!;
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

  it('register 409 (email already exists) → rejects, failure_stage signup', async () => {
    // AC answers a duplicate-email registration with 409 CONFLICT (not 401); the
    // signup stage label must not depend on which AC status the failure carried.
    const sink = new InMemoryMetricsSink();
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, {
      fetchImpl: statusFetch({ authStatus: 409 }),
      metricsSink: sink,
    });
    const credentials: JoinCredentials = {
      mode: 'register',
      email: 'taken@demo.test',
      password: 'hunter2hunter2',
      displayName: 'Dup',
    };
    await expect(
      session.join({ orgSubdomain: 'demo', meetingCode: MEETING_CODE, credentials }),
    ).rejects.toBeDefined();
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

describe('MeetingSession.startMedia (ADR-0036 §5/§6)', () => {
  const KEK = new Uint8Array(32).fill(0x11);

  interface MediaRig {
    readonly session: MeetingSession;
    readonly mocks: Map<string, MockWebTransport>;
    readonly capture: FakeCaptureSource;
    readonly codecs: FakeAudioCodecs;
    readonly playback: RecordingPlaybackSink;
    readonly sink: InMemoryMetricsSink;
  }

  // `null` means "MC assigned none". Deliberately not `undefined`: a default
  // parameter would substitute 258 for an explicit `undefined`, which is how the
  // no-sender-id case silently became the happy path the first time this was
  // written.
  async function joinedWithMedia(senderId: number | null = 258): Promise<MediaRig> {
    const mocks = new Map<string, MockWebTransport>();
    const capture = new FakeCaptureSource();
    const codecs = new FakeAudioCodecs();
    const playback = new RecordingPlaybackSink();
    const sink = new InMemoryMetricsSink();
    const session = makeSession(mocks, {
      metricsSink: sink,
      captureFactory: async () => capture as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: playback.factory as never,
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });

    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    await waitFor(() => mc.getOpenedBidiStreams().length > 0);
    mc.simulateServerMessage(
      0,
      framedJoinResponse({
        mediaServers: MEDIA_SERVERS,
        ...(senderId !== null ? { senderId } : {}),
        meetingKek: KEK,
        kekGeneration: 0,
      }),
    );
    await waitFor(() => MEDIA_SERVERS.every((u) => mocks.has(u)));
    for (const url of MEDIA_SERVERS) mocks.get(url)!.simulateReady();
    await joining;
    return { session, mocks, capture, codecs, playback, sink };
  }

  it('sends NO post-join receive capability: the join carried the declaration (story 3 R-2)', async () => {
    const rig = await joinedWithMedia();
    await rig.session.startMedia();
    const messages = decodeOutboundClientMessages(
      rig.mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0),
    );
    expect(messages.some((m) => m.message.case === 'joinRequest')).toBe(true);
    expect(messages.some((m) => m.message.case === 'receiveCapability')).toBe(false);
    rig.session.disconnect();
  });

  it('counts the KEK arriving through the seam, and never the key itself', async () => {
    const rig = await joinedWithMedia();
    await rig.session.startMedia();
    const kekUpdates = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_kek_updates_total');
    expect(kekUpdates).toHaveLength(1);
    expect(kekUpdates[0]?.labels.source).toBe('join_response');
    expect(Object.keys(kekUpdates[0]?.labels ?? {})).not.toContain('meeting_id_hash');
    rig.session.disconnect();
  });

  it('is idempotent — a second call returns the running pipeline', async () => {
    const rig = await joinedWithMedia();
    const first = await rig.session.startMedia();
    const second = await rig.session.startMedia();
    expect(second).toBe(first);
    expect(rig.session.media).toBe(first);
    rig.session.disconnect();
  });

  it('FAILS LOUDLY when MC assigned no sender id, rather than degrading', async () => {
    // Without a sender id there is no key id; without an identity key there is
    // no signature. This SDK does not send or accept unsigned frames under any
    // degradation, so there is nothing to fall back to.
    const rig = await joinedWithMedia(null);
    await expect(rig.session.startMedia()).rejects.toThrow(/sender id/);
    rig.session.disconnect();
  });

  it('refuses before a settled join', async () => {
    const session = makeSession(new Map());
    await expect(session.startMedia()).rejects.toThrow(/settled join/);
  });

  it('does not take the meeting down when media fails to start', async () => {
    // Degraded audio beats a dropped meeting. The join stays joined and the
    // signaling session stays open.
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, {
      captureFactory: async () => {
        throw new Error('microphone access was denied');
      },
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await waitFor(() => mocks.has(MC_ENDPOINT));
    const mc = mocks.get(MC_ENDPOINT)!;
    mc.simulateReady();
    await waitFor(() => mc.getOpenedBidiStreams().length > 0);
    mc.simulateServerMessage(
      0,
      framedJoinResponse({ mediaServers: MEDIA_SERVERS, senderId: 258, meetingKek: KEK }),
    );
    await waitFor(() => MEDIA_SERVERS.every((u) => mocks.has(u)));
    for (const url of MEDIA_SERVERS) mocks.get(url)!.simulateReady();
    await joining;

    await expect(session.startMedia()).rejects.toThrow('microphone access was denied');
    expect(session.state).toBe(MeetingSessionState.Joined);
    session.disconnect();
  });

  it('applies MC directed bitrate when present and the configured default otherwise', async () => {
    // `EncodingParameters.max_bitrate_bps` is the wire SSoT; the config value is
    // the fallback for its absence only, and there is deliberately no local
    // ceiling applied on top.
    const rig = await joinedWithMedia();
    await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    mc.simulateServerMessage(0, framedSendDirective({ targets: [MEDIA_SERVERS[0]!] }));
    await waitFor(() => rig.capture.started);
    expect(rig.capture.started).toBe(true);
    rig.session.disconnect();
  });

  it('stops capture on disconnect — the microphone indicator must go out', async () => {
    const rig = await joinedWithMedia();
    await rig.session.startMedia();
    rig.session.disconnect();
    await waitFor(() => rig.capture.stopCount > 0);
    expect(rig.capture.stopCount).toBe(1);
  });

  it('reports client mute to MC while suppressing locally first', async () => {
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    pipeline.setAudioMuted(true);
    // Local state changes synchronously; the report is fire-and-forget.
    expect(pipeline.muteState.audioMuted).toBe(true);
    await waitFor(() =>
      decodeOutboundClientMessages(rig.mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0)).some(
        (m) => m.message.case === 'muteRequest',
      ),
    );
    rig.session.disconnect();
  });

  it('a directive with NO audio stream withdraws sending, so resuming rotates the key (§4/§5)', async () => {
    // MC sends `streams: []` to a publisher nobody holds — e.g. when its last
    // holder leaves. Ignoring that kept the stale instruction: the client kept
    // sending into a handler with no edge for it, and never having seen "empty"
    // it skipped the transmit-key rotation §4 requires on resume. Rotation is
    // the observable: it happens ONLY on an empty -> non-empty transition.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    const start = pipeline.transmitGeneration;

    mc.simulateServerMessage(0, framedSendDirective({ targets: [MEDIA_SERVERS[0]!] }));
    await waitFor(() => pipeline.transmitGeneration === start + 1n);

    // The last holder leaves: no audio stream at all. Then a new holder arrives.
    mc.simulateServerMessage(0, framedSendDirectiveWithoutStreams());
    mc.simulateServerMessage(0, framedSendDirective({ targets: [MEDIA_SERVERS[0]!] }));
    await waitFor(() => pipeline.transmitGeneration === start + 2n);
    expect(pipeline.transmitGeneration).toBe(start + 2n);
    rig.session.disconnect();
  });

  it("passes MC's header version through to the pipeline verbatim, never defaulting it", async () => {
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const applied = vi.spyOn(pipeline, 'setSendDirective');
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    // 4 is unsendable: the listener must hand it over as-is, so the pipeline's
    // fail-closed refusal (not a silent coercion here) decides.
    mc.simulateServerMessage(
      0,
      framedSendDirective({ targets: [MEDIA_SERVERS[0]!], headerVersion: 4 }),
    );
    await waitFor(() => applied.mock.calls.length === 1);
    expect(applied.mock.calls[0]![0].headerVersion).toBe(4);
    rig.session.disconnect();
  });

  it('a first directive with NO audio stream withdraws nothing and invents nothing', async () => {
    // A solo participant's first directive: there is no earlier instruction to
    // withdraw, so nothing is applied — no placeholder stream number or bitrate.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const applied = vi.spyOn(pipeline, 'setSendDirective');
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    // A trailing assignment on the same ordered stream is the drain point: once
    // it reaches the pipeline, the directive before it has been handled.
    const drained = vi.spyOn(pipeline, 'setReceiveAssignments');
    mc.simulateServerMessage(0, framedSendDirectiveWithoutStreams());
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 0, slotState: SlotState.FEWER_SOURCES_THAN_SLOTS }),
    );
    await waitFor(() => drained.mock.calls.length === 1);
    expect(applied).not.toHaveBeenCalled();
    rig.session.disconnect();
  });

  it('RECEIVES from the StreamAssignments handler with NO send directive at all', async () => {
    // The wiring the pipeline tests cannot see: `startMedia` must bridge
    // `streamAssignments` into the pipeline's receive path. A participant
    // nobody holds gets an empty (or no) send target and must still hear the
    // slots it holds.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({
        slotId: 0,
        senderId: 300,
        mediaHandlerUrl: MEDIA_SERVERS[0]!,
        slotState: SlotState.ACTIVE,
      }),
    );
    // Deliver datagrams until one lands: the assignment is applied
    // asynchronously, and a datagram sent before it is (correctly) not read.
    const mh = rig.mocks.get(MEDIA_SERVERS[0]!)!;
    await waitFor(() => {
      mh.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
      return pipeline.frameCounts.framesReceived > 0;
    });
    expect(pipeline.frameCounts.framesReceived).toBeGreaterThan(0);
    // The structural twin of the solo test's `locked === false`.
    expect(mh.datagrams.readable.locked).toBe(true);
    rig.session.disconnect();
  });

  it('RECEIVES on EVERY handler the assignments name, each on its own transport', async () => {
    // THE CANONICAL CASE, at the session seam (ADR-0036 §9). A is connected to
    // both handlers; B's edge sits on one and C's on the other, so A must read
    // both transports. The pipeline suite proves the loop behaviour; this proves
    // `startMedia` bridges a MULTI-handler snapshot into it, which is the wiring
    // that was previously impossible — two urls used to raise a fault and open
    // nothing.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({
        slotId: 0,
        senderId: 300,
        mediaHandlerUrl: MEDIA_SERVERS[0]!,
        extraSlots: [{ slotId: 1, senderId: 301, mediaHandlerUrl: MEDIA_SERVERS[1]! }],
      }),
    );

    const first = rig.mocks.get(MEDIA_SERVERS[0]!)!;
    const second = rig.mocks.get(MEDIA_SERVERS[1]!)!;
    // STRUCTURE FIRST: a reader on each transport. Stronger than a frame count,
    // which a single shared loop could also satisfy.
    await waitFor(() => first.datagrams.readable.locked && second.datagrams.readable.locked);

    // And both actually deliver. Datagrams are re-sent until each lands, because
    // the assignment applies asynchronously.
    await waitFor(() => {
      second.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
      return pipeline.frameCounts.framesReceived > 0;
    });
    const afterSecond = pipeline.frameCounts.framesReceived;
    await waitFor(() => {
      first.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
      return pipeline.frameCounts.framesReceived > afterSecond;
    });
    expect(pipeline.frameCounts.framesReceived).toBeGreaterThan(afterSecond);
    rig.session.disconnect();
  });

  it('SENDS to every target in the directive, on every named transport', async () => {
    // A's multi-handler send, at the session seam. `targets` carries both
    // handlers, so both transports must carry the audio — the old code took
    // `targets[0]` and silently dropped the rest, which is the "some peers hear
    // me, some do not" failure with every client-side signal green.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    mc.simulateServerMessage(
      0,
      framedSendDirective({ targets: [MEDIA_SERVERS[0]!, MEDIA_SERVERS[1]!] }),
    );

    const first = rig.mocks.get(MEDIA_SERVERS[0]!)!;
    const second = rig.mocks.get(MEDIA_SERVERS[1]!)!;
    // Feed capture until both lanes have carried something. The directive applies
    // asynchronously, so a frame emitted before it lands is correctly not sent.
    await waitFor(() => {
      rig.capture.emit(makeAudioData({ samples: Float32Array.of(1, 2, 3) }));
      return first.getOutboundDatagrams().length > 0 && second.getOutboundDatagrams().length > 0;
    });
    expect(first.getOutboundDatagrams().length).toBeGreaterThan(0);
    expect(second.getOutboundDatagrams().length).toBeGreaterThan(0);
    // Datagrams, not captured frames: the counter sums the lanes.
    expect(pipeline.frameCounts.framesSent).toBeGreaterThanOrEqual(
      first.getOutboundDatagrams().length + second.getOutboundDatagrams().length,
    );
    rig.session.disconnect();
  });

  it('opens no receive path while every assigned slot is empty (solo participant)', async () => {
    // Negative twin of the test above, on the same rig: an EMPTY handler url is
    // "no source assigned", so nothing is read.
    const rig = await joinedWithMedia();
    const pipeline = await rig.session.startMedia();
    const mc = rig.mocks.get(MC_ENDPOINT)!;
    const applied = vi.spyOn(pipeline, 'setReceiveAssignments');
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 0, slotState: SlotState.FEWER_SOURCES_THAN_SLOTS }),
    );
    // DRAIN POINT, not a timer: the assignment reached the PIPELINE (stronger
    // than the session event, which fires before the pipeline applies it), with
    // the empty "no source assigned" url and NO sender — absent, never coerced
    // to 0. That call is synchronous, so once it has returned there is nothing
    // left in flight to wait for.
    await waitFor(() => applied.mock.calls.length === 1);
    expect(applied.mock.calls[0]?.[0]).toEqual([
      { slotId: 0, senderId: undefined, mediaHandlerUrl: '', active: false },
    ]);
    expect(pipeline.assignedSenders).toEqual([]);

    const mh = rig.mocks.get(MEDIA_SERVERS[0]!)!;
    mh.simulateIncomingDatagram(Uint8Array.of(0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    // STRUCTURE: no reader was ever taken on the handler's datagram stream, so
    // no read can be pending. The zero count is then a result, not a race.
    expect(mh.datagrams.readable.locked).toBe(false);
    expect(pipeline.frameCounts.framesReceived).toBe(0);
    rig.session.disconnect();
  });
});

// ===========================================================================
// The four media absence-signals reaching the facade (ADR-0036 §5/§6/§10)
// ===========================================================================
//
// WHY THESE LIVE HERE rather than in the adapter or view suites: those drive a
// MOCK session that fires these events directly. That is right for what they
// assert — a store projects what it is given, a view renders what the store
// holds — but it means neither can fail if the REAL `MeetingSession` never
// forwards anything. The whole design rests on one bridge, and without these it
// is the single link nothing covers: every test above it stays green while a
// joined participant watches a mute indicator that never moves.
//
// The rig (`fakeFetch`, `makeSession`, `driveJoin`) is reused rather than
// restated — a second copy of the GC/AC stub in a sibling file is how two
// versions of the join response start to disagree.

describe('MeetingSession — media absence-signals (ADR-0036 §5/§6/§10)', () => {
  it('forwards streamAssignments from the wire, preserving each slot state verbatim', async () => {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const seen: Array<{ slotId: number; slotState: string }> = [];
    session.on('streamAssignments', (event) => {
      for (const a of event.assignments) seen.push({ slotId: a.slotId, slotState: a.slotState });
    });

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    const mc = mocks.get(MC_ENDPOINT)!;
    // §6: these three are indistinguishable to a client — all present as no
    // media — and render completely differently. Each must survive the trip to
    // the embedder as a DISTINCT value; collapsing any pair produces exactly the
    // one-spinner-for-three-causes client §6 complains about.
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 0, slotState: SlotState.AVATAR_CLIENT_MUTED }),
    );
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 1, slotState: SlotState.WITHHELD_BY_CONGESTION }),
    );
    mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 2, slotState: SlotState.SOURCE_UNREACHABLE }),
    );

    await waitFor(() => seen.length === 3);
    expect(seen).toEqual([
      { slotId: 0, slotState: 'avatar_client_muted' },
      { slotId: 1, slotState: 'withheld_by_congestion' },
      { slotId: 2, slotState: 'source_unreachable' },
    ]);

    session.disconnect();
  });

  it('bridges assignments that arrive BEFORE media is started', async () => {
    // The assignments bridge is wired in `#wireSignaling`, not in `startMedia()`,
    // and this is why: MC can assign slots to a client that has not opened its
    // microphone, and a UI showing "waiting for the controller" must see that
    // resolve. Wiring it alongside the pipeline events would make this silently
    // impossible — and the failure would look like a controller problem.
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    let assignments = 0;
    session.on('streamAssignments', () => {
      assignments += 1;
    });

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    await driveJoin(mocks, 'all');
    await joinP;

    expect(session.media, 'no pipeline yet — startMedia() was never called').toBeUndefined();
    mocks
      .get(MC_ENDPOINT)!
      .simulateServerMessage(
        0,
        framedStreamAssignments({ slotId: 0, slotState: SlotState.ACTIVE }),
      );

    await waitFor(() => assignments === 1);
    session.disconnect();
  });

  it('seeds OUR OWN identity key into the roster, or the loopback can never decode', async () => {
    // THE REGRESSION THIS PINS, found by the Layer 7 env-test and by nothing
    // else: MC's roster carries `existing_participants` — everyone except us —
    // so nothing in the signalling path ever teaches the receive path our own
    // key. In a one-participant meeting every frame MH returns carries our own
    // sender id, `identityKeyFor` returns `undefined`, and EVERY frame is
    // dropped `no_roster_entry` while audio is visibly arriving at the wire.
    //
    // Unit-tier loopbacks cannot catch this: they seed their own roster by hand
    // (see `media/lifecycle/__tests__`), which is precisely the step this
    // asserts the session performs for itself.
    //
    // Asserted at the roster's own `upsert` seam — the same one `#feedRosterKeys`
    // uses for peers — rather than by reaching into a private field, so this
    // fails if the key stops reaching the resolver the ingress path consults.
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);

    const upserts: Array<{ senderId: number; keyBytes: number }> = [];
    const realUpsert = RosterIdentityKeys.prototype.upsert;
    vi.spyOn(RosterIdentityKeys.prototype, 'upsert').mockImplementation(async function (
      this: RosterIdentityKeys,
      entry: Parameters<typeof realUpsert>[0],
    ) {
      upserts.push({ senderId: entry.senderId, keyBytes: entry.identityPublicKey.length });
      return realUpsert.call(this, entry);
    });

    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    await driveJoin(mocks, 'all', { senderId: 258 });
    await joinP;

    const own = upserts.filter((u) => u.senderId === 258);
    expect(
      own,
      'the session must publish its OWN (senderId, identity public key) into the roster — ' +
        'without it every returned frame is dropped no_roster_entry',
    ).toHaveLength(1);
    // A real Ed25519 public key, not a placeholder: `upsert` records an ABSENCE
    // for a wrong-length key rather than throwing, so a zero-length "key" would
    // satisfy a call-count-only assertion while leaving the resolver empty.
    expect(own[0]!.keyBytes).toBe(32);

    session.disconnect();
  });

  it('startMedia subscribes the pipeline emitter BEFORE awaiting start()', async () => {
    // `startMedia()` cannot complete under `environment: 'node'` — real
    // WebCodecs and getUserMedia are absent — so this asserts the SUBSCRIPTION
    // and its ORDERING rather than a running pipeline. The ordering is the part
    // that matters: `firstMediaFrame` is armed inside `start()`, so a bridge
    // registered after the await could miss it entirely on a fast loopback, and
    // the symptom would be an in-meeting view stuck on "waiting for audio" while
    // audio played.
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks);
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: TOKEN_CREDS,
    });
    // A sender id is REQUIRED: without one `startMedia()` fails closed before
    // constructing a pipeline (no sender id -> no key id -> no signable frame),
    // and this test would then assert on an empty list for the wrong reason.
    await driveJoin(mocks, 'all', { senderId: 258 });
    await joinP;

    const subscribed: string[] = [];
    const { AudioPipeline } = await import('../../media/lifecycle/AudioPipeline.js');
    const realOn = AudioPipeline.prototype.on;
    vi.spyOn(AudioPipeline.prototype, 'on').mockImplementation(function (
      this: InstanceType<typeof AudioPipeline>,
      type: never,
      listener: never,
    ) {
      subscribed.push(type as unknown as string);
      return realOn.call(this, type, listener);
    } as typeof AudioPipeline.prototype.on);

    await session.startMedia().catch(() => undefined);

    expect(
      subscribed,
      'all three pipeline events must be bridged, and before start() is awaited',
    ).toEqual(expect.arrayContaining(['muteChanged', 'firstMediaFrame', 'fault']));

    session.disconnect();
  });
});

// ---------------------------------------------------------------------------
// Story 2 task 13: receive-slot count N, the advertised server cap, and the
// capture-source selection (R-1, R-7, R-23)
// ---------------------------------------------------------------------------

describe('MeetingSession — publish set, per-kind receive slots and caps (story 3 R-1, R-2, R-23)', () => {
  const KEK = new Uint8Array(32).fill(0x11);

  async function joinedWith(opts: {
    readonly audio?: { count: number; source: 'configured' | 'default' };
    readonly video?: { count: number; source: 'configured' | 'default' };
    readonly publish?: readonly ('audio' | 'video')[];
    readonly maxAudioReceiveSlots?: number;
    readonly maxVideoReceiveSlots?: number;
    readonly extra?: Partial<MeetingSessionOptions>;
  }): Promise<{
    session: MeetingSession;
    mocks: Map<string, MockWebTransport>;
    sink: InMemoryMetricsSink;
  }> {
    const mocks = new Map<string, MockWebTransport>();
    const codecs = new FakeAudioCodecs();
    const sink = new InMemoryMetricsSink();
    const session = makeSession(mocks, {
      metricsSink: sink,
      captureFactory: async () => new FakeCaptureSource() as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: new RecordingPlaybackSink().factory as never,
      ...(opts.audio ? { audioReceiveSlots: opts.audio } : {}),
      ...(opts.video ? { videoReceiveSlots: opts.video } : {}),
      ...(opts.publish ? { publish: opts.publish } : {}),
      ...opts.extra,
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all', {
      senderId: 258,
      meetingKek: KEK,
      ...(opts.maxAudioReceiveSlots !== undefined
        ? { maxAudioReceiveSlots: opts.maxAudioReceiveSlots }
        : {}),
      ...(opts.maxVideoReceiveSlots !== undefined
        ? { maxVideoReceiveSlots: opts.maxVideoReceiveSlots }
        : {}),
    });
    await joining;
    return { session, mocks, sink };
  }

  function outbound(mocks: Map<string, MockWebTransport>) {
    return decodeOutboundClientMessages(mocks.get(MC_ENDPOINT)!.getOutboundBidiWrites(0));
  }

  function joinRequest(mocks: Map<string, MockWebTransport>) {
    const join = outbound(mocks).find((m) => m.message.case === 'joinRequest');
    if (join?.message.case !== 'joinRequest') throw new Error('no JoinRequest was sent');
    return join.message.value;
  }

  /** (slot id, wire kind) pairs of a `ReceiveCapability`, in wire order. */
  function slotsOf(capability: { slots: { slotId: number; mediaKind: MediaKind }[] } | undefined) {
    return capability?.slots.map((s) => [s.slotId, s.mediaKind]);
  }

  it('the JOIN carries the publish set and the declaration: audio 0..N-1 then video N..N+M-1, in id order', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: 2, source: 'configured' },
      video: { count: 3, source: 'configured' },
    });
    const join = joinRequest(rig.mocks);
    expect(join.publishKinds).toEqual([MediaKind.AUDIO]);
    expect(slotsOf(join.receiveCapability)).toEqual([
      [0, MediaKind.AUDIO],
      [1, MediaKind.AUDIO],
      [2, MediaKind.VIDEO_CAMERA],
      [3, MediaKind.VIDEO_CAMERA],
      [4, MediaKind.VIDEO_CAMERA],
    ]);
    rig.session.disconnect();
  });

  it.each([
    [
      'N = 0',
      0,
      2,
      [
        [0, MediaKind.VIDEO_CAMERA],
        [1, MediaKind.VIDEO_CAMERA],
      ],
    ],
    [
      'M = 0',
      2,
      0,
      [
        [0, MediaKind.AUDIO],
        [1, MediaKind.AUDIO],
      ],
    ],
  ] as const)('a kind may declare 0 slots (%s)', async (_label, n, m, expected) => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: n, source: 'configured' },
      video: { count: m, source: 'configured' },
    });
    expect(slotsOf(joinRequest(rig.mocks).receiveCapability)).toEqual(expected);
    rig.session.disconnect();
  });

  it('the join-carried declaration is the ONLY initial one: startMedia sends no post-join ReceiveCapability (shim a deleted)', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: 2, source: 'configured' },
      video: { count: 1, source: 'configured' },
    });
    await rig.session.startMedia();
    expect(slotsOf(joinRequest(rig.mocks).receiveCapability)).toHaveLength(3);
    expect(outbound(rig.mocks).filter((m) => m.message.case === 'receiveCapability')).toEqual([]);
    rig.session.disconnect();
  });

  // --- Story 3 C-A2: the VP8 gate, the session-owned keys, video mute -------

  it('declares CODEC_VP8 in supported_codecs only when the probe passed (video in use)', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const ok = await joinedWith({ publish: ['audio', 'video'] });
    expect(joinRequest(ok.mocks).capabilities?.supportedCodecs).toEqual([Codec.VP8]);
    ok.session.disconnect();
    const failing = await joinedWith({
      publish: ['audio', 'video'],
      extra: { vp8Probe: () => Promise.reject(new VideoCodecUnsupportedError('decoder', 'no')) },
    });
    expect(joinRequest(failing.mocks).capabilities?.supportedCodecs).toEqual([]);
    failing.session.disconnect();
  });

  it('runs NO probe and declares nothing when video is neither published nor received', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    let probed = 0;
    const rig = await joinedWith({
      extra: {
        vp8Probe: () => {
          probed += 1;
          return Promise.resolve();
        },
      },
    });
    expect(probed).toBe(0);
    expect(joinRequest(rig.mocks).capabilities?.supportedCodecs).toEqual([]);
    rig.session.disconnect();
  });

  it.each([
    ['published', { publish: ['audio', 'video'] as const }],
    ['received (M > 0)', { video: { count: 1, source: 'configured' as const } }],
  ])(
    'a failed VP8 probe with video %s fails startMedia LOUDLY, typed, with a fatal fault — never audio-only',
    async (_n, opts) => {
      vi.spyOn(console, 'info').mockImplementation(() => {});
      const rig = await joinedWith({
        ...opts,
        extra: { vp8Probe: () => Promise.reject(new VideoCodecUnsupportedError('encoder', 'no')) },
      });
      const faults: { stage: string; fatal: boolean }[] = [];
      rig.session.on('mediaFault', (f) => faults.push(f));
      await expect(rig.session.startMedia()).rejects.toBeInstanceOf(VideoCodecUnsupportedError);
      expect(faults).toEqual([expect.objectContaining({ stage: 'encoder', fatal: true })]);
      expect(rig.session.media).toBeUndefined();
      rig.session.disconnect();
    },
  );

  it('clears the session transmit keys on a startMedia FAILURE after the manager exists', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const clear = vi.spyOn(TransmitKeyManager.prototype, 'clear');
    const rig = await joinedWith({
      extra: {
        captureFactory: () => Promise.reject(new Error('no microphone')),
      },
    });
    await expect(rig.session.startMedia()).rejects.toThrow();
    await waitFor(() => clear.mock.calls.length > 0);
    expect(clear).toHaveBeenCalledTimes(1);
    clear.mockRestore();
    rig.session.disconnect();
  });

  it('a VIDEO-ONLY session rotates its transmit keys on a KEK receipt (ONE session-level subscription)', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      publish: ['video'],
      audio: { count: 0, source: 'configured' },
      extra: {
        videoEncoderFactory: async () => ({
          encode: () => undefined,
          encodeQueueSize: 0,
          close: () => undefined,
        }),
        videoCaptureFactory: async () => ({
          start: async () => undefined,
          stop: () => undefined,
          grantedDeviceId: undefined,
        }),
      },
    });
    const pipeline = await rig.session.startMedia();
    const before = pipeline.transmitGeneration;
    rig.mocks
      .get(MC_ENDPOINT)!
      .simulateServerMessage(0, framedMeetingKekUpdate(new Uint8Array(32).fill(9), 1));
    await waitFor(() => pipeline.transmitGeneration === before + 1n);
    // Exactly one rotation per receipt: no second subscription anywhere.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(pipeline.transmitGeneration).toBe(before + 1n);
    rig.session.disconnect();
  });

  it.each([
    ['video-only', ['video'] as const],
    ['receive-only', [] as const],
  ])(
    'a %s participant NEVER acquires the microphone: zero audio capture calls (privacy, R-2)',
    async (_n, publish) => {
      vi.spyOn(console, 'info').mockImplementation(() => {});
      let micCalls = 0;
      const rig = await joinedWith({
        publish,
        extra: {
          captureFactory: async () => {
            micCalls += 1;
            return new FakeCaptureSource() as never;
          },
          videoEncoderFactory: async () => ({
            encode: () => undefined,
            encodeQueueSize: 0,
            close: () => undefined,
          }),
          videoCaptureFactory: async () => ({
            start: async () => undefined,
            stop: () => undefined,
            grantedDeviceId: undefined,
          }),
        },
      });
      const pipeline = await rig.session.startMedia();
      await pipeline.setCaptureDevice('another-mic');
      rig.session.setAudioMuted(false);
      expect(micCalls).toBe(0);
      expect(pipeline.captureOpen).toBe(false);
      // Receiving still works: the pipeline started.
      expect(rig.session.media).toBe(pipeline);
      rig.session.disconnect();
    },
  );

  it('an AUDIO encoder that cannot be built is counted on encoder_errors{audio} (both kinds count a build failure)', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      extra: { encoderFactory: () => Promise.reject(new Error('unsupported')) },
    });
    await expect(rig.session.startMedia()).rejects.toThrow();
    const errors = rig.sink
      .getRecordedMetrics()
      .filter(
        (m) => m.name === 'dt_client_media_encoder_errors_total' && m.labels.media_kind === 'audio',
      )
      .reduce((n, m) => n + m.value, 0);
    expect(errors).toBe(1);
    rig.session.disconnect();
  });

  it('an audio publisher DOES acquire the microphone (positive control)', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    let micCalls = 0;
    const rig = await joinedWith({
      extra: {
        captureFactory: async () => {
          micCalls += 1;
          return new FakeCaptureSource() as never;
        },
      },
    });
    const pipeline = await rig.session.startMedia();
    expect(micCalls).toBe(1);
    expect(pipeline.captureOpen).toBe(true);
    rig.session.disconnect();
  });

  it('setVideoMuted goes through the video publisher and never touches the microphone', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      publish: ['audio', 'video'],
      extra: {
        videoEncoderFactory: async () => ({
          encode: () => undefined,
          encodeQueueSize: 0,
          close: () => undefined,
        }),
        videoCaptureFactory: async () => ({
          start: async () => undefined,
          stop: () => undefined,
          grantedDeviceId: undefined,
        }),
      },
    });
    const pipeline = await rig.session.startMedia();
    expect(rig.session.video).toBeDefined();
    expect(rig.session.videoCaptureSource).toEqual({ mode: 'camera' });
    rig.session.setVideoMuted(true);
    expect(rig.session.muteState).toEqual({ audioMuted: false, videoMuted: true });
    expect(pipeline.captureOpen).toBe(true);
    expect(rig.session.cameraTrackLive).toBe(false);
    rig.session.disconnect();
  });

  it('the publish set is COPIED and FROZEN: mutating the caller array changes nothing', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const publish: ('audio' | 'video')[] = ['audio'];
    const rig = await joinedWith({ publish });
    publish.length = 0;
    expect(rig.session.receiveSlots.publish).toEqual(['audio']);
    expect(Object.isFrozen(rig.session.receiveSlots.publish)).toBe(true);
    expect(joinRequest(rig.mocks).publishKinds).toEqual([MediaKind.AUDIO]);
    rig.session.disconnect();
  });

  it('ACCEPTS a video publish set (story 3 C-A2 enables it) and carries it on the join', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({ publish: ['audio', 'video'] });
    expect(rig.session.receiveSlots.publish).toEqual(['audio', 'video']);
    expect(joinRequest(rig.mocks).publishKinds).toEqual([MediaKind.AUDIO, MediaKind.VIDEO_CAMERA]);
    rig.session.disconnect();
  });

  it('refuses a duplicate publish kind, typed', () => {
    expect(() => makeSession(new Map(), { publish: ['audio', 'audio'] })).toThrow(SignalingError);
  });

  it('refuses "publishes nothing and declares 0 + 0" LOCALLY, typed, before any dial', () => {
    const mocks = new Map<string, MockWebTransport>();
    let err: unknown;
    try {
      makeSession(mocks, {
        publish: [],
        audioReceiveSlots: { count: 0, source: 'configured' },
        videoReceiveSlots: { count: 0, source: 'configured' },
      });
    } catch (e) {
      err = e;
    }
    expect((err as SignalingError).signalingCode).toBe(
      SignalingErrorCode.NothingToPublishOrReceive,
    );
    expect(mocks.size).toBe(0);
  });

  it('a receive-only participant (publishes nothing, declares slots) is NOT refused', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({ publish: [], audio: { count: 1, source: 'configured' } });
    expect(joinRequest(rig.mocks).publishKinds).toEqual([]);
    rig.session.disconnect();
  });

  it('per-kind caps are readable, alongside effective N, M and the publish set', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: 3, source: 'configured' },
      video: { count: 2, source: 'configured' },
      maxAudioReceiveSlots: 8,
      maxVideoReceiveSlots: 0,
    });
    expect(rig.session.receiveSlots).toEqual({
      publish: ['audio'],
      audio: { declared: 3, source: 'configured', serverCap: { state: 'known', value: 8 } },
      // 0 is LEGAL (video off) and stays known-0, never folded into unknown.
      video: { declared: 2, source: 'configured', serverCap: { state: 'known', value: 0 } },
    });
    rig.session.disconnect();
  });

  it('ABSENT caps (an MC that does not send them) read as unknown, never coerced to 0', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({});
    expect(rig.session.receiveSlots.audio.serverCap).toEqual({ state: 'unknown' });
    expect(rig.session.receiveSlots.video.serverCap).toEqual({ state: 'unknown' });
    rig.session.disconnect();
  });

  it('OVER-CAP IS NOT REFUSED LOCALLY: the whole declaration is sent and the retired counter never emits', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: 5, source: 'configured' },
      maxAudioReceiveSlots: 4,
    });
    await rig.session.startMedia();
    expect(slotsOf(joinRequest(rig.mocks).receiveCapability)).toHaveLength(5);
    expect(rig.session.media).toBeDefined();
    expect(
      rig.sink
        .getRecordedMetrics()
        .some((m) => m.name === 'dt_client_media_receive_slots_rejected_total'),
    ).toBe(false);
    rig.session.disconnect();
  });

  it('an N above the client ceiling fails at CONSTRUCTION with the typed error, before any use', () => {
    const mocks = new Map<string, MockWebTransport>();
    const ceiling = DEFAULT_CLIENT_CONFIG.media.ingress.maxDecodeLanes;
    expect(() =>
      makeSession(mocks, { audioReceiveSlots: { count: ceiling + 1, source: 'configured' } }),
    ).toThrow(ClientConfigError);
    expect(mocks.size).toBe(0);
  });

  it('reports the SDK defaults with source "default" when nothing is configured', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({});
    expect(rig.session.receiveSlots.audio).toMatchObject({
      declared: DEFAULT_RECEIVE_AUDIO_SLOTS,
      source: 'default',
    });
    expect(rig.session.receiveSlots.video).toMatchObject({
      declared: DEFAULT_RECEIVE_VIDEO_SLOTS,
      source: 'default',
    });
    rig.session.disconnect();
  });

  it('logs the publish set, N, M and both caps as numbers and fixed tokens only', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({
      audio: { count: 2, source: 'configured' },
      video: { count: 1, source: 'configured' },
      maxAudioReceiveSlots: 8,
      maxVideoReceiveSlots: 0,
    });
    const line = info.mock.calls.map((c) => c.join(' ')).find((l) => l.includes('dt-media-slots'));
    expect(line).toBe(
      '[dt-media-slots] publishing [audio]; declared N=2 audio (configured), M=1 video ' +
        '(configured); server caps audio 8, video 0 (video disabled)',
    );
    rig.session.disconnect();
  });

  it('zero-initialises the directive-fault and unattributed keyframe series at join, and no video series', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const rig = await joinedWith({});
    const zeroed = rig.sink
      .getRecordedMetrics()
      .filter((m) => m.value === 0 && m.name.startsWith('dt_client_media_'))
      .map((m) => `${m.name}|${m.labels['outcome'] ?? ''}|${m.labels['media_kind'] ?? ''}`)
      .sort();
    expect(zeroed).toEqual(
      [
        'dt_client_media_keyframe_requests_unattributed_total|malformed|',
        'dt_client_media_mh_control_channel_ended_total|finished|',
        'dt_client_media_mh_control_channel_ended_total|reset|',
        'dt_client_media_keyframe_requests_unattributed_total|not_sending|',
        'dt_client_media_send_directive_header_version_refused_total||',
        'dt_client_media_send_directive_kind_missing_total||audio',
      ].sort(),
    );
    expect(rig.sink.getRecordedMetrics().some((m) => m.labels['media_kind'] === 'video')).toBe(
      false,
    );
    rig.session.disconnect();
  });
});

describe('MeetingSession — capture source (story 2 R-7)', () => {
  it('the DEFAULT capture is the microphone when __DT_TEST_TONE__ is off (@security A3)', async () => {
    // The flag is `false` under every vitest config, so this run is the
    // production branch: a default-on tone would fail here.
    expect(__DT_TEST_TONE__).toBe(false);
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, {
      encoderFactory: new FakeAudioCodecs().encoderFactory as never,
    });
    // Before startMedia there is no source.
    expect(session.captureSource).toBeUndefined();
    // Structural: the selection's default branch names the microphone factory
    // and the tone synthesis is reachable only inside the gate.
    const { readFileSync } = await import('node:fs');
    const source = readFileSync(new URL('../mediaSelection.ts', import.meta.url), 'utf8');
    const gate = source.indexOf('if (__DT_TEST_TONE__) {');
    expect(gate).toBeGreaterThan(0);
    expect(source.indexOf('createTestToneCapture({')).toBeGreaterThan(gate);
    expect(source.indexOf("'test_tone'")).toBeGreaterThan(gate);
    // Behavioural: with the flag off, no injected factory -> the microphone.
    const chosen = selectCaptureSource(undefined, 258);
    expect(chosen.info).toEqual({ mode: 'microphone' });
    expect(chosen.factory).toBe(createMicrophoneCapture);
    // An injected factory is respected and reported as the microphone.
    const injected = async () => new FakeCaptureSource() as never;
    expect(selectCaptureSource(injected, 258).factory).toBe(injected);
  });

  it('reports the microphone mode on the gauge and the session after startMedia', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const mocks = new Map<string, MockWebTransport>();
    const capture = new FakeCaptureSource();
    const codecs = new FakeAudioCodecs();
    const playback = new RecordingPlaybackSink();
    const sink = new InMemoryMetricsSink();
    const session = makeSession(mocks, {
      metricsSink: sink,
      captureFactory: async () => capture as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: playback.factory as never,
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all', { senderId: 258, meetingKek: new Uint8Array(32).fill(1) });
    await joining;
    await session.startMedia();
    expect(session.captureSource).toEqual({ mode: 'microphone' });
    const gauges = sink
      .getRecordedMetrics()
      .filter((m) => m.name === 'dt_client_media_capture_source');
    expect(gauges.length).toBeGreaterThan(0);
    expect(gauges.every((g) => g.labels.mode === 'microphone' && g.value === 1)).toBe(true);
    session.disconnect();
  });
});

describe('join label bag — PRIMARY, behavioural (ADR-0036 §11; story 2 task 13)', () => {
  // Every emission that carries the join bag (`meeting_id_hash`) must be a metric
  // on the frozen grandfathered roster READ from client.md. Driven through the
  // real join -> startMedia -> leave flow with a recording sink, so a name that
  // reaches the spreading helper by any route is seen.
  it('only grandfathered metrics carry meeting_id_hash across join, startMedia and leave', async () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const roster = readGrandfatheredRoster();
    expect(
      roster.length,
      'EXTRACTION FAILURE: empty grandfathered roster parsed from client.md',
    ).toBeGreaterThan(0);

    const mocks = new Map<string, MockWebTransport>();
    const sink = new InMemoryMetricsSink();
    const codecs = new FakeAudioCodecs();
    const session = makeSession(mocks, {
      metricsSink: sink,
      captureFactory: async () => new FakeCaptureSource() as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: new RecordingPlaybackSink().factory as never,
      audioReceiveSlots: { count: 2, source: 'configured' },
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'partial', {
      senderId: 258,
      meetingKek: new Uint8Array(32).fill(1),
      maxAudioReceiveSlots: 8,
    });
    await joining;
    await session.startMedia();
    session.disconnect();

    const emissions = sink.getRecordedMetrics();
    // The bag was LIVE: without this, a flow that emitted nothing would pass.
    expect(emissions.some((e) => JOIN_BAG_MARKER in e.labels)).toBe(true);
    // And the media side ran too, so its emissions were actually checked.
    expect(emissions.some((e) => e.name === 'dt_client_media_capture_source')).toBe(true);
    expect(
      joinBagViolations(emissions, roster),
      'VIOLATION: join bag on a non-roster metric',
    ).toEqual([]);
  });

  it('NEGATIVE CONTROL: a non-roster emission carrying the bag is a violation, for the right reason', () => {
    const roster = readGrandfatheredRoster();
    expect(roster.length).toBeGreaterThan(0);
    const violations = joinBagViolations(
      [
        { name: 'dt_client_join_attempts_total', labels: { meeting_id_hash: 'h', status: 'ok' } },
        { name: 'dt_client_media_new_thing_total', labels: { meeting_id_hash: 'h' } },
        { name: 'dt_client_media_frames_sent_total', labels: { org_id: 'o' } },
      ],
      roster,
    );
    expect(violations).toEqual(['dt_client_media_new_thing_total']);
  });
});

// ---------------------------------------------------------------------------
// Story 2 R-8 / R-10 / R-11: host hint, server mute, unmute request
// ---------------------------------------------------------------------------

/** A syntactically valid (unsigned) meeting JWT carrying `claims`. */
function meetingJwt(claims: Record<string, unknown>): string {
  const seg = (v: unknown): string =>
    btoa(JSON.stringify(v)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  return `${seg({ alg: 'EdDSA' })}.${seg(claims)}.c2ln`;
}

/** `fakeFetch`, but the GC join response carries `meetingToken`. */
function fetchWithMeetingToken(meetingToken: string): FetchLike {
  const inner = fakeFetch();
  return (async (input: string | URL, init?: RequestInit) => {
    const response = await inner(input, init);
    const url = typeof input === 'string' ? input : input.toString();
    if (!url.includes('/api/v1/meetings/')) return response;
    const body = (await response.json()) as Record<string, unknown>;
    return new Response(JSON.stringify({ ...body, token: meetingToken }), {
      status: 200,
      headers: JSON_HEADERS,
    });
  }) as unknown as FetchLike;
}

describe('MeetingSession — host hint and server mute (story 2 R-8/R-10/R-11)', () => {
  async function joined(meetingToken: string): Promise<{
    session: MeetingSession;
    mc: MockWebTransport;
  }> {
    const mocks = new Map<string, MockWebTransport>();
    const session = makeSession(mocks, { fetchImpl: fetchWithMeetingToken(meetingToken) });
    const joinP = session.join({
      orgSubdomain: 'demo',
      meetingCode: MEETING_CODE,
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all');
    await joinP;
    return { session, mc: mocks.get(MC_ENDPOINT)! };
  }

  it('isHost follows the meeting token role claim, exactly "host"', async () => {
    expect((await joined(meetingJwt({ role: 'host', sub: 'u' }))).session.isHost).toBe(true);
    expect((await joined(meetingJwt({ role: 'participant' }))).session.isHost).toBe(false);
  });

  it('an unreadable role is NOT host, WARNs once, and the WARN carries no token material', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { session } = await joined('opaque-meeting-token');
    expect(session.isHost).toBe(false);
    const roleWarns = warn.mock.calls.filter((c) => c[0] === '[dt-meeting-role]');
    expect(roleWarns).toHaveLength(1);
    expect(JSON.stringify(roleWarns)).not.toContain('opaque-meeting-token');
  });

  it('setServerMute sends target + action; requestUnmute sends an unmute request', async () => {
    const { session, mc } = await joined(meetingJwt({ role: 'host' }));
    await session.setServerMute('participant-b', 'audio', true);
    await session.requestUnmute();
    const messages = decodeOutboundClientMessages(mc.getOutboundBidiWrites(0));
    const mute = messages.find((m) => m.message.case === 'serverMuteRequest');
    expect(mute?.message.value).toMatchObject({
      participantId: 'participant-b',
      audioMuted: true,
      videoMuted: false,
    });
    expect(messages.some((m) => m.message.case === 'unmuteRequest')).toBe(true);
  });

  it('refuses both before a settled join', async () => {
    const session = makeSession(new Map());
    await expect(session.setServerMute('b', 'audio', true)).rejects.toBeInstanceOf(SignalingError);
    await expect(session.requestUnmute()).rejects.toBeInstanceOf(SignalingError);
  });

  it('currentKekGeneration is undefined before any KEK, then follows the held CURRENT generation', async () => {
    expect(makeSession(new Map()).currentKekGeneration).toBeUndefined();
    const { session, mc } = await joined(meetingJwt({ role: 'participant' }));
    mc.simulateServerMessage(0, framedMeetingKekUpdate(new Uint8Array(32).fill(7), 3));
    await waitFor(() => session.currentKekGeneration === 3);
  });

  it('bridges server-mute updates and the unmute-request relay onto the session', async () => {
    const { session, mc } = await joined(meetingJwt({ role: 'host' }));
    const mutes: unknown[] = [];
    const requests: unknown[] = [];
    session.on('participantMuteChanged', (e) => mutes.push(e));
    session.on('unmuteRequested', (e) => requests.push(e));
    mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({
        participantId: 'b',
        audioServerMuted: true,
        serverMutedBy: 'h',
      }),
    );
    mc.simulateServerMessage(0, framedUnmuteRequestRelay('b'));
    await waitFor(() => mutes.length > 0 && requests.length > 0);
    expect(mutes[0]).toEqual({
      participantId: 'b',
      audioServerMuted: true,
      videoServerMuted: false,
      serverMutedBy: 'h',
    });
    expect(requests[0]).toEqual({ participantId: 'b' });
  });
});

// ---------------------------------------------------------------------------
// Story 3 C-A1: cached directive/assignments (E3), per-kind mute, R-18 through
// the session, and the MH control sink
// ---------------------------------------------------------------------------

describe('MeetingSession — story 3 C-A1 session wiring', () => {
  const KEK = new Uint8Array(32).fill(0x22);
  const SELF = 'participant-self';
  const MH = MEDIA_SERVERS[0]!;

  async function joinedRig(): Promise<{
    session: MeetingSession;
    mocks: Map<string, MockWebTransport>;
    mc: MockWebTransport;
    capture: FakeCaptureSource;
    sink: InMemoryMetricsSink;
    resumed: string[];
    sentCount: (name: string, labels?: Record<string, string>) => number;
    outbound: () => ReturnType<typeof decodeOutboundClientMessages>;
  }> {
    const mocks = new Map<string, MockWebTransport>();
    const capture = new FakeCaptureSource();
    const codecs = new FakeAudioCodecs();
    const sink = new InMemoryMetricsSink();
    const session = makeSession(mocks, {
      metricsSink: sink,
      captureFactory: async () => capture as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: new RecordingPlaybackSink().factory as never,
    });
    const resumed: string[] = [];
    session.on('captureResumedAfterServerMute', (e) => resumed.push(e.kind));
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all', { participantId: SELF, senderId: 258, meetingKek: KEK });
    await joining;
    const mc = mocks.get(MC_ENDPOINT)!;
    return {
      session,
      mocks,
      mc,
      capture,
      sink,
      resumed,
      sentCount: (name, labels = {}) =>
        sink
          .getRecordedMetrics()
          .filter(
            (m) => m.name === name && Object.entries(labels).every(([k, v]) => m.labels[k] === v),
          )
          .reduce((n, m) => n + m.value, 0),
      outbound: () => decodeOutboundClientMessages(mc.getOutboundBidiWrites(0)),
    };
  }

  /** Let the read loops and listeners run: bounded zero-delay task turns, never a timed sleep. */
  async function settle(turns = 25): Promise<void> {
    for (let i = 0; i < turns; i += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  }

  it('E3: a directive and assignments that arrive BEFORE startMedia are applied when the pipeline is built', async () => {
    const rig = await joinedRig();
    rig.mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 0, senderId: 259, mediaHandlerUrl: MH }),
    );
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    const pipeline = await rig.session.startMedia();
    expect(pipeline.assignedSenders).toEqual([259]);
    rig.capture.emit(makeAudioData({ samples: Float32Array.of(1, 1, 1) }));
    await settle();
    expect(rig.mocks.get(MH)!.getOutboundDatagrams().length).toBeGreaterThan(0);

    // A LIVE update after the replay goes straight through (no double-apply race).
    rig.mc.simulateServerMessage(
      0,
      framedStreamAssignments({ slotId: 0, senderId: 260, mediaHandlerUrl: MH }),
    );
    await waitFor(() => pipeline.assignedSenders[0] === 260);
    expect(pipeline.assignedSenders).toEqual([260]);
    rig.session.disconnect();
  });

  it('per-kind client mute: each kind changes alone, and BOTH kinds are reported explicitly', async () => {
    const rig = await joinedRig();
    await rig.session.startMedia();
    rig.session.setVideoMuted(true);
    rig.session.setAudioMuted(true);
    rig.session.setVideoMuted(false);
    await settle();
    expect(rig.session.muteState).toEqual({ audioMuted: true, videoMuted: false });
    const reports = rig
      .outbound()
      .filter((m) => m.message.case === 'muteRequest')
      .map((m) => m.message.value as { audioMuted?: boolean; videoMuted?: boolean })
      .map((v) => [v.audioMuted, v.videoMuted]);
    expect(reports).toEqual([
      [false, true],
      [true, true],
      [true, false],
    ]);
    rig.session.disconnect();
  });

  it("a host's per-kind server mute carries the target's CURRENT state for the other kind", async () => {
    const rig = await joinedRig();
    rig.mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({
        participantId: 'b',
        audioServerMuted: false,
        videoServerMuted: true,
      }),
    );
    await settle();
    await rig.session.setServerMute('b', 'audio', true);
    const sent = rig
      .outbound()
      .filter((m) => m.message.case === 'serverMuteRequest')
      .map((m) => m.message.value);
    expect(sent).toEqual([
      expect.objectContaining({ participantId: 'b', audioMuted: true, videoMuted: true }),
    ]);
    rig.session.disconnect();
  });

  async function startedSending(): Promise<Awaited<ReturnType<typeof joinedRig>>> {
    const rig = await joinedRig();
    await rig.session.startMedia();
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    return rig;
  }

  const muteSelf = (muted: boolean) =>
    framedParticipantMuteUpdate({ participantId: SELF, audioServerMuted: muted });

  it('R-18: unmute update BEFORE the resumed directive fires exactly one event', async () => {
    const rig = await startedSending();
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, muteSelf(false));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    expect(rig.resumed).toEqual(['audio']);
    rig.session.disconnect();
  });

  it('R-18: the resumed directive BEFORE the unmute update fires exactly one event; the later update adds none', async () => {
    const rig = await startedSending();
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    rig.mc.simulateServerMessage(0, muteSelf(false));
    await settle();
    expect(rig.resumed).toEqual(['audio']);
    rig.session.disconnect();
  });

  it('R-18: the unmute update LOST still fires exactly one event', async () => {
    const rig = await startedSending();
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    expect(rig.resumed).toEqual(['audio']);
    rig.session.disconnect();
  });

  it('R-18 S2: a server mute and directives that arrive BEFORE startMedia still fire exactly one event at startMedia', async () => {
    // The tracker is subscribed from signalling creation, not from startMedia():
    // a refactor moving it would lose this pre-media mute silently.
    const rig = await joinedRig();
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    // Positive control: nothing is sent before media starts, so no event yet.
    expect(rig.resumed).toEqual([]);
    await rig.session.startMedia();
    await settle();
    expect(rig.resumed).toEqual(['audio']);
    rig.session.disconnect();
  });

  it('R-18 S2, false-positive side: with no mute recorded, a pre-media directive raises nothing at startMedia', async () => {
    const rig = await joinedRig();
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    await rig.session.startMedia();
    await settle();
    expect(rig.resumed).toEqual([]);
    rig.session.disconnect();
  });

  it('R-18: client-muted means no event', async () => {
    const rig = await startedSending();
    rig.session.setAudioMuted(true);
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    expect(rig.resumed).toEqual([]);
    rig.session.disconnect();
  });

  it("R-18: ANOTHER participant's server-mute update never touches the self latch", async () => {
    const rig = await startedSending();
    rig.mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({ participantId: 'someone-else', audioServerMuted: true }),
    );
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [] }));
    rig.mc.simulateServerMessage(0, framedSendDirective({ streamNumber: 0, targets: [MH] }));
    await settle();
    expect(rig.resumed).toEqual([]);
    rig.session.disconnect();
  });

  it('S1: a self unmute while still sending changes no egress state and emits nothing', async () => {
    const rig = await startedSending();
    rig.mc.simulateServerMessage(0, muteSelf(true));
    rig.mc.simulateServerMessage(0, muteSelf(false));
    await settle();
    rig.capture.emit(makeAudioData({ samples: Float32Array.of(2, 2, 2) }));
    await settle();
    // Still sending throughout: the mute update itself never stopped capture.
    expect(rig.mocks.get(MH)!.getOutboundDatagrams().length).toBeGreaterThan(0);
    expect(rig.resumed).toEqual([]);
    rig.session.disconnect();
  });

  function framedKeyframeRequest(streamNumber: number): Uint8Array {
    return encodeFrame(
      toBinary(
        MhServerMessageSchema,
        create(MhServerMessageSchema, {
          message: {
            case: 'keyframeRequest',
            value: create(KeyframeRequestSchema, { streamNumber, gopCounter: 3 }),
          },
        }),
      ),
    );
  }

  it('every keyframe request is refused not_sending, attributed only to a directed published kind', async () => {
    const rig = await startedSending();
    const mh = rig.mocks.get(MH)!;
    mh.simulateServerMessage(0, framedKeyframeRequest(0)); // audio's directed stream
    mh.simulateServerMessage(0, framedKeyframeRequest(9)); // no published kind
    mh.simulateServerMessage(0, encodeFrame(new Uint8Array([0xff, 0xff])));
    await settle();
    expect(
      rig.sentCount('dt_client_media_keyframe_requests_received_total', {
        media_kind: 'audio',
        outcome: 'not_sending',
      }),
    ).toBe(1);
    expect(
      rig.sentCount('dt_client_media_keyframe_requests_unattributed_total', {
        outcome: 'not_sending',
      }),
    ).toBe(1);
    expect(
      rig.sentCount('dt_client_media_keyframe_requests_unattributed_total', {
        outcome: 'malformed',
      }),
    ).toBe(1);
    // None was honoured, and nothing about a request reaches a label.
    expect(
      rig.sentCount('dt_client_media_keyframe_requests_received_total', { outcome: 'honoured' }),
    ).toBe(0);
    for (const m of rig.sink.getRecordedMetrics()) {
      expect(Object.keys(m.labels)).not.toContain('stream_number');
      expect(Object.keys(m.labels)).not.toContain('gop_counter');
    }
    rig.session.disconnect();
  });
});

describe('MeetingSession — video receive wiring (story 3 C-B)', () => {
  const KEK = new Uint8Array(32).fill(0x11);
  const PEER = {
    participantId: 'p-video',
    name: 'V',
    senderId: 300,
    identityPublicKey: new Uint8Array(32).fill(3),
  };

  async function videoRig(metricsSink: InMemoryMetricsSink = new InMemoryMetricsSink()) {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    const mocks = new Map<string, MockWebTransport>();
    const codecs = new FakeAudioCodecs();
    const decoders = new FakeVideoDecoders();
    const session = makeSession(mocks, {
      metricsSink,
      captureFactory: async () => new FakeCaptureSource() as never,
      encoderFactory: codecs.encoderFactory as never,
      decoderFactory: codecs.decoderFactory as never,
      playbackFactory: new RecordingPlaybackSink().factory as never,
      videoDecoderFactory: decoders.factory as never,
      audioReceiveSlots: { count: 1, source: 'configured' },
      videoReceiveSlots: { count: 2, source: 'configured' },
    });
    const joining = session.join({
      meetingCode: MEETING_CODE,
      orgSubdomain: 'acme',
      credentials: LOGIN,
    });
    await driveJoin(mocks, 'all', { senderId: 258, meetingKek: KEK, participants: [PEER] });
    await joining;
    await session.startMedia();
    return { session, mocks, decoders, mc: mocks.get(MC_ENDPOINT)! };
  }

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('startMedia builds the receiver with exactly M decoders; video assignments reach it; renders are bridged', async () => {
    const rig = await videoRig();
    expect(Object.keys(rig.session.videoReceive ?? {}).sort()).toEqual([
      'evidence',
      'setVideoSink',
      'unfulfilledSnapshot',
    ]);
    expect(Object.isFrozen(rig.session.videoReceive)).toBe(true);
    expect(rig.decoders.constructions).toBe(2);
    const renders: VideoSlotRenderChangedEvent[] = [];
    rig.session.on('videoSlotRenderChanged', (e) => renders.push(e));
    // Video slots are ids 1 and 2 (audio 0).
    rig.mc.simulateServerMessage(
      0,
      framedStreamAssignments({
        slotId: 1,
        senderId: 300,
        mediaHandlerUrl: MEDIA_SERVERS[0]!,
        slotState: SlotState.AVATAR_NO_VIDEO,
        mediaKind: MediaKind.VIDEO_CAMERA,
      }),
    );
    await waitFor(() => renders.length > 0);
    expect(renders).toEqual([
      { slotId: 1, state: { kind: 'avatar', participantSenderId: 300, reason: 'avatar_no_video' } },
    ]);
    expect(rig.session.videoReceive?.evidence().find((e) => e.slotId === 1)?.mcState).toBe(
      'avatar_no_video',
    );
    // The pairing view reads assignments AND live senders.
    expect(rig.session.slotPairing.get(300)).toEqual({ videoSlotId: 1 });
    rig.session.videoReceive?.setVideoSink(1, { drawFrame: () => {} });
    rig.session.disconnect();
  });

  it('disconnect records every unfulfilled exit and the gauge 0 SYNCHRONOUSLY, before the final flush', async () => {
    const sink = new InMemoryMetricsSink();
    const rig = await videoRig(sink);
    rig.mc.simulateServerMessage(
      0,
      framedStreamAssignments({
        slotId: 1,
        mediaHandlerUrl: MEDIA_SERVERS[0]!,
        slotState: SlotState.ACTIVE,
        mediaKind: MediaKind.VIDEO_CAMERA,
      }),
    );
    await waitFor(() => rig.session.videoReceive?.evidence()[0]?.unfulfilled === true);
    rig.session.disconnect();
    // No await: what the disconnect's flush can see.
    const named = (n: string) => sink.getRecordedMetrics().filter((m) => m.name === n);
    expect(named('dt_client_media_slot_unfulfilled_duration_ms')).toHaveLength(1);
    expect(named('dt_client_media_slots_unfulfilled').at(-1)?.value).toBe(0);
  });

  it('the video server-mute -> unmute EDGE reaches the receiver with the roster sender id', async () => {
    const rig = await videoRig();
    const lifted = vi.spyOn(VideoReceiver.prototype, 'videoServerMuteLifted');
    rig.mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({
        participantId: 'p-video',
        audioServerMuted: false,
        videoServerMuted: true,
      }),
    );
    rig.mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({
        participantId: 'p-video',
        audioServerMuted: true,
        videoServerMuted: true,
      }),
    );
    rig.mc.simulateServerMessage(
      0,
      framedParticipantMuteUpdate({
        participantId: 'p-video',
        audioServerMuted: true,
        videoServerMuted: false,
      }),
    );
    await waitFor(() => lifted.mock.calls.length > 0);
    expect(lifted.mock.calls).toEqual([[300]]);
    rig.session.disconnect();
  });

  it('a roster forget purges transmit keys in BOTH the audio pipeline and the video receiver', async () => {
    const rig = await videoRig();
    const video = vi.spyOn(VideoReceiver.prototype, 'purgeTransmitKeys');
    const audio = vi.spyOn(AudioPipeline.prototype, 'purgeTransmitKeys');
    rig.mc.simulateServerMessage(0, framedParticipantLeft('p-video', LeaveReason.VOLUNTARY));
    await waitFor(() => video.mock.calls.length > 0);
    expect(video.mock.calls).toEqual([[300]]);
    expect(audio.mock.calls).toEqual([[300]]);
    rig.session.disconnect();
  });
});
