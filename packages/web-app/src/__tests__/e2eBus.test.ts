// R-29 / R-23 / @security / @semantic-guard: the `window.__darktower_test__` bus.
// Verifies (a) events land in the replay buffer (the #18/#19 contract), (b) the
// `joined` projection DROPS bindingToken + correlationId (whitelist), and (c)
// errors cross via toJSON — a non-allowlisted field (rogue token) never leaks.

import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import type { MediaFrameCounts } from '@darktower/sdk-core';
import { E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS, installE2EHooks } from '../lib/e2eBus.js';
import { createE2EInstrumentation, type E2EInstrumentation } from '../lib/e2eAnalysis.js';
import { parseSendCounts, parseVideoSlots } from '../../e2e/videoBusShape.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

/** Build a `MediaFrameCounts` from a partial, so a test states only what it varies. */
function counts(over: Partial<MediaFrameCounts>): MediaFrameCounts {
  return {
    framesSent: 0,
    framesReceived: 0,
    framesAccepted: 0,
    framesDropped: 0,
    lastDropReason: undefined,
    ...over,
  };
}

/** Disposers from every `installE2EHooks` in a test, so no sampler outlives it. */
let disposers: Array<() => void> = [];

function install(session: MockMeetingSession): void {
  disposers.push(installE2EHooks(session));
}

beforeEach(() => {
  delete window.__darktower_test__;
  disposers = [];
});
afterEach(() => {
  for (const dispose of disposers) dispose();
  delete window.__darktower_test__;
});

function events(): ReadonlyArray<Readonly<Record<string, unknown>>> {
  return window.__darktower_test__?.events ?? [];
}

test('installs the bus and records bounded events in the replay buffer', () => {
  const session = new MockMeetingSession();
  install(session);
  expect(window.__darktower_test__).toBeDefined();

  session.fire('stateChange', MeetingSessionState.Joining);
  session.fire('mediaConnected', 'https://mh-0:4433');

  const types = events().map((e) => e['type']);
  expect(types).toContain('stateChange');
  expect(types).toContain('mediaConnected');
  expect(events().find((e) => e['type'] === 'stateChange')?.['state']).toBe(
    MeetingSessionState.Joining,
  );
});

test('late subscribers still observe an already-fired event (replay buffer)', () => {
  const session = new MockMeetingSession();
  install(session);
  session.fire('stateChange', MeetingSessionState.Joined);

  // A Playwright spec attaching after `joined` reads the buffer, not just a live feed.
  expect(events().some((e) => e['type'] === 'stateChange' && e['state'] === 'joined')).toBe(true);
});

test('joined projection drops bindingToken and correlationId (whitelist)', () => {
  const session = new MockMeetingSession();
  install(session);
  session.fire('joined', {
    participantId: 'self',
    senderId: 123,
    existingParticipants: [{ participantId: 'a', name: 'Ann' }],
    mediaServers: ['https://mh-0:4433'],
    correlationId: 'CORR_SECRET',
    bindingToken: 'BINDING_SECRET',
  });

  const joined = events().find((e) => e['type'] === 'joined');
  expect(joined).toBeDefined();
  expect(joined?.['participantId']).toBe('self');
  expect(joined?.['senderId']).toBe('123'); // stringified per-meeting sender id
  expect(joined?.['bindingToken']).toBeUndefined();
  expect(joined?.['correlationId']).toBeUndefined();
  expect(JSON.stringify(joined)).not.toContain('BINDING_SECRET');
  expect(JSON.stringify(joined)).not.toContain('CORR_SECRET');
});

test('error crosses via toJSON — a non-allowlisted field never leaks', () => {
  const session = new MockMeetingSession();
  install(session);
  const err = new SdkError(SdkErrorCode.Signaling, 'join rejected', { status: 401 });
  (err as { token?: string }).token = 'SECRET_TOKEN_123';
  session.fire('error', err);

  const errorEvent = events().find((e) => e['type'] === 'error');
  expect(errorEvent?.['code']).toBe(SdkErrorCode.Signaling);
  expect(errorEvent?.['status']).toBe(401);
  expect(JSON.stringify(errorEvent)).not.toContain('SECRET_TOKEN_123');
});

// ---------------------------------------------------------------------------
// Media projections (ADR-0036 §5/§10) — task 20
// ---------------------------------------------------------------------------

test('first-media and mute-state project exactly their whitelisted scalars', () => {
  const session = new MockMeetingSession();
  install(session);

  session.fire('firstMediaFrame', 87);
  session.fire('muteChanged', { audioMuted: true, videoMuted: false });
  session.fire('muteChanged', { audioMuted: false, videoMuted: false });

  const firstMedia = events().find((e) => e['type'] === 'firstMediaFrame');
  // Exact key set, not just "contains elapsedMs": a whitelist that is only
  // checked for what it INCLUDES cannot catch a field someone spreads in later.
  expect(Object.keys(firstMedia ?? {}).sort()).toEqual(['elapsedMs', 'type']);
  expect(firstMedia?.['elapsedMs']).toBe(87);

  const mutes = events().filter((e) => e['type'] === 'muteState');
  expect(mutes.map((e) => e['audioMuted'])).toEqual([true, false]);
  expect(mutes.map((e) => e['videoMuted'])).toEqual([false, false]);
  expect(Object.keys(mutes[0] ?? {}).sort()).toEqual(['audioMuted', 'type', 'videoMuted']);
});

test('media faults are NOT projected onto the bus', () => {
  // A DOM-rendered operator signal, deliberately not a test observable: its
  // message is prose, and the bus contract is scalars.
  const session = new MockMeetingSession();
  install(session);
  session.fire('mediaFault', { stage: 'decoder', message: 'the decoder failed', fatal: true });
  expect(events().some((e) => e['type'] === 'mediaFault')).toBe(false);
});

test('frame counts are SAMPLED from the pipeline getter, not emitted per frame', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    install(session);

    // Before `startMedia()` there is no pipeline, so nothing is sampled — a
    // zero-filled sample here would put readings inside a window where "flat"
    // means nothing.
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 3);
    expect(events().some((e) => e['type'] === 'mediaFrameCounts')).toBe(false);

    const pipeline = await session.startMedia();
    pipeline.frameCounts = counts({ framesSent: 5, framesReceived: 4, framesAccepted: 4 });
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    pipeline.frameCounts = counts({
      framesSent: 9,
      framesReceived: 9,
      framesAccepted: 8,
      framesDropped: 1,
      lastDropReason: 'no_roster_entry',
    });
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);

    const samples = events().filter((e) => e['type'] === 'mediaFrameCounts');
    expect(samples.map((e) => e['framesSent'])).toEqual([5, 9]);
    expect(samples.map((e) => e['framesAccepted'])).toEqual([4, 8]);
    // EXACT key set. The first sample has no drops yet, so `lastDropReason` is
    // absent rather than present-and-undefined — the field's PRESENCE is what
    // means "a drop has happened", which is why it is spread conditionally.
    expect(Object.keys(samples[0] ?? {}).sort()).toEqual([
      'atMs',
      'framesAccepted',
      'framesDropped',
      'framesReceived',
      'framesSent',
      'type',
    ]);
    // The second sample carries a drop, so the reason appears — the complete
    // receive identity, which is what distinguishes "nothing arriving" from
    // "arriving and rejected".
    expect(Object.keys(samples[1] ?? {}).sort()).toEqual([
      'atMs',
      'framesAccepted',
      'framesDropped',
      'framesReceived',
      'framesSent',
      'lastDropReason',
      'type',
    ]);
    expect(samples[1]?.['lastDropReason']).toBe('no_roster_entry');
    expect(samples.map((e) => e['framesReceived'])).toEqual([4, 9]);
  } finally {
    vi.useRealTimers();
  }
});

test('the disposer stops the sampler', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    const dispose = installE2EHooks(session);
    const pipeline = await session.startMedia();
    pipeline.frameCounts = counts({ framesSent: 1, framesReceived: 1, framesAccepted: 1 });
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    const afterFirst = events().filter((e) => e['type'] === 'mediaFrameCounts').length;
    // Positive control: the sampler was actually running, so zero new samples
    // below means "stopped", not "never started".
    expect(afterFirst).toBeGreaterThan(0);

    dispose();
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 4);

    expect(events().filter((e) => e['type'] === 'mediaFrameCounts')).toHaveLength(afterFirst);
  } finally {
    vi.useRealTimers();
  }
});

// ---------------------------------------------------------------------------
// Story 2 R-1 / R-7 / R-23 / R-30
// ---------------------------------------------------------------------------

test('receiveSlots: publish set, N, M, sources and per-kind caps are projected on join, numbers only', () => {
  const session = new MockMeetingSession();
  session.receiveSlots = {
    publish: ['audio'],
    audio: {
      declared: 3,
      slotIds: [0, 1, 2],
      source: 'configured',
      serverCap: { state: 'known', value: 8 },
    },
    video: { declared: 0, slotIds: [], source: 'default', serverCap: { state: 'unknown' } },
  };
  install(session);
  session.fire('joined', {
    participantId: 'self',
    senderId: 7,
    existingParticipants: [],
    mediaServers: [],
    correlationId: 'corr',
    bindingToken: 'bind',
  });
  const event = events().find((e) => e['type'] === 'receiveSlots');
  expect(event).toEqual({
    type: 'receiveSlots',
    publish: ['audio'],
    audio: { declared: 3, source: 'configured', serverCapState: 'known', serverCap: 8 },
    // An unknown cap is never coerced to a number (0 would read "video disabled").
    video: { declared: 0, source: 'default', serverCapState: 'unknown' },
  });
});

test('slotAssignments: the EXPECTED sender per slot, stringified, whitelist-only', () => {
  const session = new MockMeetingSession();
  install(session);
  session.fire('streamAssignments', {
    assignments: [
      {
        slotId: 0,
        mediaKind: 'audio',
        senderId: 7,
        mediaHandlerUrl: 'https://mh:1',
        slotState: 'active',
      },
      {
        slotId: 1,
        mediaKind: 'audio',
        senderId: undefined,
        mediaHandlerUrl: '',
        slotState: 'fewer_sources_than_slots',
      },
    ],
    unreachableSenderIds: [9],
  });
  expect(events().find((e) => e['type'] === 'slotAssignments')).toEqual({
    type: 'slotAssignments',
    assignments: [
      { slotId: 0, kind: 'audio', senderId: '7', slotState: 'active' },
      { slotId: 1, kind: 'audio', slotState: 'fewer_sources_than_slots' },
    ],
    unreachableSenderIds: ['9'],
  });
});

test('captureSource is announced ONCE when media starts, so the harness asserts the mode', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    install(session);
    await session.startMedia();
    session.captureSource = { mode: 'test_tone', toneHz: 625 };
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 3);
    const announced = events().filter((e) => e['type'] === 'captureSource');
    expect(announced).toEqual([{ type: 'captureSource', mode: 'test_tone', toneHz: 625 }]);
  } finally {
    vi.useRealTimers();
  }
});

test('videoCaptureSource is announced ONCE; patternId never rides it outside a __DT_TEST_PATTERN__ build', async () => {
  vi.useFakeTimers();
  try {
    expect(__DT_TEST_PATTERN__).toBe(false);
    const session = new MockMeetingSession();
    install(session);
    await session.startMedia();
    session.videoCaptureSource = { mode: 'test_pattern', patternId: 258 };
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 3);
    const announced = events().filter((e) => e['type'] === 'videoCaptureSource');
    // The define is false under every test runner: the id is withheld.
    expect(announced).toEqual([{ type: 'videoCaptureSource', mode: 'test_pattern' }]);
  } finally {
    vi.useRealTimers();
  }
});

test('sendTracks samples the camera light and the open microphone as booleans only', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    install(session);
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 2);
    expect(events().filter((e) => e['type'] === 'sendTracks')).toEqual([]);
    await session.startMedia();
    session.cameraTrackLive = true;
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    expect(
      events()
        .filter((e) => e['type'] === 'sendTracks')
        .at(-1),
    ).toEqual({
      type: 'sendTracks',
      cameraTrackLive: true,
      micTrackOpen: true,
    });
  } finally {
    vi.useRealTimers();
  }
});

test('receive layers and analysis are SAMPLED from the injected instrumentation, whitelist-only', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    const instrumentation = {
      recorder: undefined as never,
      playbackFactory: undefined as never,
      verification: () => ({
        layers: [{ slot: 0, senderId: 7, keyed: 5, verified: 4, rogue: 'x' }],
        drops: [
          { slot: 'undeclared' as const, senderId: null, reason: 'no_roster_entry', count: 1 },
        ],
        overflow: 0,
      }),
      analysis: () => [
        {
          senderId: 7,
          levelDbfs: -20,
          ready: true,
          sampleRateHz: 48_000,
          fftSize: 8192,
          smoothingTimeConstant: 0,
          binHz: 48_000 / 8192,
          bandStartHz: 560,
          bandEndHz: 1200,
          spectrumDb: [-90, -10, -90],
          pcm: [1, 2, 3],
        },
      ],
    };
    disposers.push(installE2EHooks(session, instrumentation as never));
    // Nothing before media starts.
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    expect(events().some((e) => e['type'] === 'receiveLayers')).toBe(false);

    await session.startMedia();
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    const layers = events().find((e) => e['type'] === 'receiveLayers');
    expect(layers?.['layers']).toEqual([{ slot: 0, senderId: '7', keyed: 5, verified: 4 }]);
    expect(layers?.['drops']).toEqual([
      { slot: 'undeclared', senderId: null, reason: 'no_roster_entry', count: 1 },
    ]);
    const analysis = events().find((e) => e['type'] === 'receiveAnalysis');
    const lane = (analysis?.['lanes'] as Record<string, unknown>[])[0]!;
    expect(Object.keys(lane).sort()).toEqual([
      'bandEndHz',
      'bandStartHz',
      'binHz',
      'fftSize',
      'levelDbfs',
      'ready',
      'sampleRateHz',
      'senderId',
      'smoothingTimeConstant',
      'spectrumDb',
    ]);
    expect(lane['senderId']).toBe('7');
    expect(JSON.stringify(events())).not.toContain('rogue');
    expect(JSON.stringify(events())).not.toContain('pcm');
  } finally {
    vi.useRealTimers();
  }
});

test('without instrumentation (never injected outside a test build) nothing is sampled', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    install(session);
    await session.startMedia();
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 2);
    expect(events().some((e) => e['type'] === 'receiveLayers')).toBe(false);
    expect(events().some((e) => e['type'] === 'receiveAnalysis')).toBe(false);
  } finally {
    vi.useRealTimers();
  }
});

// ---------------------------------------------------------------------------
// Story 2 task 15: server mute, unmute requests, roster sender ids, KEK
// generation, build knobs, and the gated flush command
// ---------------------------------------------------------------------------

test('participantMute / unmuteRequested are whitelist projections of the wire', () => {
  const session = new MockMeetingSession();
  install(session);
  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'h',
    ...({ rogue: 'SECRET' } as object),
  });
  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  session.fire('unmuteRequested', { participantId: 'b' });
  expect(events().filter((e) => e['type'] === 'participantMute')).toEqual([
    {
      type: 'participantMute',
      participantId: 'b',
      audioServerMuted: true,
      videoServerMuted: false,
      serverMutedBy: 'h',
    },
    {
      type: 'participantMute',
      participantId: 'b',
      audioServerMuted: false,
      videoServerMuted: false,
    },
  ]);
  expect(events().find((e) => e['type'] === 'unmuteRequested')).toEqual({
    type: 'unmuteRequested',
    participantId: 'b',
  });
});

test('roster projections carry the stringified sender id, and only when MC set one', () => {
  const session = new MockMeetingSession();
  install(session);
  session.fire('participantJoined', {
    participant: { participantId: 'b', name: 'Bea', senderId: 5 },
  });
  session.fire('participantJoined', { participant: { participantId: 'c', name: 'Cid' } });
  expect(events().filter((e) => e['type'] === 'participantJoined')).toEqual([
    { type: 'participantJoined', participantId: 'b', name: 'Bea', senderId: '5' },
    { type: 'participantJoined', participantId: 'c', name: 'Cid' },
  ]);
});

test('the KEK generation is emitted on CHANGE only', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    install(session);
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 2);
    session.currentKekGeneration = 1;
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 3);
    session.currentKekGeneration = 2;
    await vi.advanceTimersByTimeAsync(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 3);
    expect(
      events()
        .filter((e) => e['type'] === 'kekGeneration')
        .map((e) => e['generation']),
    ).toEqual([1, 2]);
  } finally {
    vi.useRealTimers();
  }
});

test('buildKnobs reports the levers define and whether telemetry is configured', () => {
  const session = new MockMeetingSession();
  disposers.push(installE2EHooks(session, undefined, { telemetryConfigured: true }));
  expect(events().find((e) => e['type'] === 'buildKnobs')).toEqual({
    type: 'buildKnobs',
    testLevers: false, // the vitest define
    testPattern: false, // the vitest define
    telemetryConfigured: true,
    // No SDK telemetry is configured under this test runner: intent alone is not state.
    telemetrySinkActive: false,
  });
});

test('flushMetrics REJECTS when telemetry is not configured — never a silent no-op', async () => {
  const session = new MockMeetingSession();
  install(session);
  await expect(window.__darktower_test__!.flushMetrics()).rejects.toThrow(
    'telemetry_not_configured',
  );
});

test('flushMetrics REJECTS when the config asked for telemetry but the SDK holds no provider', async () => {
  const session = new MockMeetingSession();
  disposers.push(installE2EHooks(session, undefined, { telemetryConfigured: true }));
  await expect(window.__darktower_test__!.flushMetrics()).rejects.toThrow(
    'telemetry_not_configured',
  );
});

// ---------------------------------------------------------------------------
// Story 3 C-C1: videoSlots + sendCounts, round-tripped through the e2e
// contract's strict parsers (`e2e/videoBusShape.ts` is THE contract)
// ---------------------------------------------------------------------------

function videoInstrumentation(
  surfaces: Record<number, { framesRendered: number; lumas: number[] | undefined }>,
): E2EInstrumentation {
  const real = createE2EInstrumentation();
  if (real === undefined) throw new Error('E2E instrumentation absent under __E2E_HOOKS__');
  return {
    ...real,
    // Honour `withLumas` like the real sampler: no lumas unless asked.
    surface: (slotId, { withLumas }) => {
      const s = surfaces[slotId];
      return s === undefined ? undefined : { ...s, lumas: withLumas ? s.lumas : undefined };
    },
  };
}

const LUMAS = Array.from({ length: 24 }, (_, i) => (i % 2 === 0 ? 255 : 0));

test('videoSlots projects the allowlisted per-slot fields and round-trips the strict parser', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    session.videoReceive = {
      setVideoSink: () => undefined,
      evidence: () => session.videoEvidence,
      unfulfilledSnapshot: () => [],
    };
    session.videoEvidence = [
      // Live, painted: lumas present, the VERIFIED sender stringified.
      {
        slotId: 3,
        mcState: 'active',
        render: 'active',
        lastRenderedSender: 258,
        unfulfilled: false,
        framesDecoded: 9,
      },
      // Avatar: no lumas even though the surface has a paint history.
      {
        slotId: 4,
        mcState: 'avatar_client_muted',
        render: 'avatar',
        lastRenderedSender: undefined,
        unfulfilled: false,
        framesDecoded: 0,
      },
      // MC has not named it yet: OMITTED (no placeholder token).
      {
        slotId: 5,
        mcState: undefined,
        render: 'none',
        lastRenderedSender: undefined,
        unfulfilled: false,
        framesDecoded: 0,
      },
    ];
    disposers.push(
      installE2EHooks(
        session,
        videoInstrumentation({
          3: { framesRendered: 7, lumas: LUMAS },
          4: { framesRendered: 2, lumas: LUMAS },
        }),
      ),
    );
    vi.advanceTimersByTime(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    const raw = events().find((e) => e['type'] === 'videoSlots');
    const parsed = parseVideoSlots(raw);
    expect(parsed.slots).toEqual([
      {
        slotId: 3,
        mcState: 'active',
        rendered: 'video',
        renderedSenderId: '258',
        framesDecoded: 9,
        framesRendered: 7,
        lumas: LUMAS,
      },
      {
        slotId: 4,
        mcState: 'avatar_client_muted',
        rendered: 'avatar',
        framesDecoded: 0,
        framesRendered: 2,
      },
    ]);
    // Never a key id, generation, stream number, GoP, size or timestamp.
    const text = JSON.stringify(raw);
    for (const forbidden of [
      'keyId',
      'generation',
      'stream',
      'gop',
      'bytes',
      'atMs',
      'unfulfilled',
    ]) {
      expect(text).not.toContain(forbidden);
    }
  } finally {
    vi.useRealTimers();
  }
});

test('sendCounts projects frames sent and keyframes by the closed cause set, round-tripping the parser', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    session.videoSendCounts = {
      framesSent: 40,
      keyframes: { directive_start: 1, request: 3, abandonment: 0 },
    };
    disposers.push(installE2EHooks(session, videoInstrumentation({})));
    vi.advanceTimersByTime(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    const raw = events().find((e) => e['type'] === 'sendCounts');
    expect(parseSendCounts(raw)).toEqual({
      video: { framesSent: 40, keyframes: { directive_start: 1, request: 3, abandonment: 0 } },
    });
  } finally {
    vi.useRealTimers();
  }
});

test('no videoSlots / sendCounts before the video paths exist', async () => {
  vi.useFakeTimers();
  try {
    const session = new MockMeetingSession();
    disposers.push(installE2EHooks(session, videoInstrumentation({})));
    vi.advanceTimersByTime(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS * 2);
    expect(events().some((e) => e['type'] === 'videoSlots')).toBe(false);
    expect(events().some((e) => e['type'] === 'sendCounts')).toBe(false);
  } finally {
    vi.useRealTimers();
  }
});

test('the render-surface sampler reads 24 central-half block lumas from the PAINTED canvas', async () => {
  const inst = createE2EInstrumentation();
  if (inst === undefined) throw new Error('E2E instrumentation absent');
  const canvas = document.createElement('canvas');
  canvas.width = 120;
  canvas.height = 80;
  const g = canvas.getContext('2d')!;
  // Left half white, right half black.
  g.fillStyle = '#000';
  g.fillRect(0, 0, 120, 80);
  g.fillStyle = '#fff';
  g.fillRect(0, 0, 60, 80);
  let painted = 0;
  inst.onSurface({ slotId: 3, canvas, framesRendered: () => painted }, 3);
  // Nothing painted yet: no lumas (absent, never zero-filled).
  expect(inst.surface(3, { withLumas: true })).toEqual({ framesRendered: 0, lumas: undefined });
  painted = 1;
  // Not asked for lumas: the canvas is not read.
  expect(inst.surface(3, { withLumas: false })).toEqual({ framesRendered: 1, lumas: undefined });
  const sample = inst.surface(3, { withLumas: true });
  expect(sample?.lumas).toHaveLength(24);
  expect(sample?.lumas?.slice(0, 6)).toEqual([255, 255, 255, 0, 0, 0]);
  inst.onSurface(undefined, 3);
  expect(inst.surface(3, { withLumas: true })).toBeUndefined();
});
