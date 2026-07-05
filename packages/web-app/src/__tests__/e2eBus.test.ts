// R-29 / R-23 / @security / @semantic-guard: the `window.__darktower_test__` bus.
// Verifies (a) events land in the replay buffer (the #18/#19 contract), (b) the
// `joined` projection DROPS bindingToken + correlationId (whitelist), and (c)
// errors cross via toJSON — a non-allowlisted field (rogue token) never leaks.

import { afterEach, beforeEach, expect, test } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import { installE2EHooks } from '../lib/e2eBus.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

beforeEach(() => {
  delete window.__darktower_test__;
});
afterEach(() => {
  delete window.__darktower_test__;
});

function events(): ReadonlyArray<Readonly<Record<string, unknown>>> {
  return window.__darktower_test__?.events ?? [];
}

test('installs the bus and records bounded events in the replay buffer', () => {
  const session = new MockMeetingSession();
  installE2EHooks(session);
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
  installE2EHooks(session);
  session.fire('stateChange', MeetingSessionState.Joined);

  // A Playwright spec attaching after `joined` reads the buffer, not just a live feed.
  expect(events().some((e) => e['type'] === 'stateChange' && e['state'] === 'joined')).toBe(true);
});

test('joined projection drops bindingToken and correlationId (whitelist)', () => {
  const session = new MockMeetingSession();
  installE2EHooks(session);
  session.fire('joined', {
    participantId: 'self',
    userId: 123n,
    existingParticipants: [{ participantId: 'a', name: 'Ann' }],
    mediaServers: ['https://mh-0:4433'],
    correlationId: 'CORR_SECRET',
    bindingToken: 'BINDING_SECRET',
  });

  const joined = events().find((e) => e['type'] === 'joined');
  expect(joined).toBeDefined();
  expect(joined?.['participantId']).toBe('self');
  expect(joined?.['userId']).toBe('123'); // stringified bigint
  expect(joined?.['bindingToken']).toBeUndefined();
  expect(joined?.['correlationId']).toBeUndefined();
  expect(JSON.stringify(joined)).not.toContain('BINDING_SECRET');
  expect(JSON.stringify(joined)).not.toContain('CORR_SECRET');
});

test('error crosses via toJSON — a non-allowlisted field never leaks', () => {
  const session = new MockMeetingSession();
  installE2EHooks(session);
  const err = new SdkError(SdkErrorCode.Signaling, 'join rejected', { status: 401 });
  (err as { token?: string }).token = 'SECRET_TOKEN_123';
  session.fire('error', err);

  const errorEvent = events().find((e) => e['type'] === 'error');
  expect(errorEvent?.['code']).toBe(SdkErrorCode.Signaling);
  expect(errorEvent?.['status']).toBe(401);
  expect(JSON.stringify(errorEvent)).not.toContain('SECRET_TOKEN_123');
});
