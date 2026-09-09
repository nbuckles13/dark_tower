// R-29 / R-23 / @security / @semantic-guard: the `window.__darktower_test__` bus.
// Verifies (a) events land in the replay buffer (the #18/#19 contract), (b) the
// `joined` projection DROPS bindingToken + correlationId (whitelist), and (c)
// errors cross via toJSON — a non-allowlisted field (rogue token) never leaks.

import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import type { MediaFrameCounts } from '@darktower/sdk-core';
import { E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS, installE2EHooks } from '../lib/e2eBus.js';
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
  session.fire('muteChanged', { audioMuted: true });
  session.fire('muteChanged', { audioMuted: false });

  const firstMedia = events().find((e) => e['type'] === 'firstMediaFrame');
  // Exact key set, not just "contains elapsedMs": a whitelist that is only
  // checked for what it INCLUDES cannot catch a field someone spreads in later.
  expect(Object.keys(firstMedia ?? {}).sort()).toEqual(['elapsedMs', 'type']);
  expect(firstMedia?.['elapsedMs']).toBe(87);

  const mutes = events().filter((e) => e['type'] === 'muteState');
  expect(mutes.map((e) => e['audioMuted'])).toEqual([true, false]);
  expect(Object.keys(mutes[0] ?? {}).sort()).toEqual(['audioMuted', 'type']);
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
