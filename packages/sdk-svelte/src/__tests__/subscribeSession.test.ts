// R-43: subscribeSession — the pure event→state SPOT. Drives REAL typed events
// through a mock MeetingSession (TypedEventEmitter subclass) and asserts the
// store reacts; then asserts the aggregate unsubscribe removes every listener.

import { expect, test } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import { MeetingStore } from '../stores/MeetingStore.svelte.js';
import { subscribeSession } from '../stores/bindMeetingSession.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

test('seeds store from session.state at subscribe time', () => {
  const session = new MockMeetingSession();
  session.state = MeetingSessionState.ConnectingMc;
  const store = new MeetingStore();
  subscribeSession(store, session);
  expect(store.meetingState).toBe(MeetingSessionState.ConnectingMc);
});

test('reacts to each SDK event', () => {
  const session = new MockMeetingSession();
  const store = new MeetingStore();
  subscribeSession(store, session);

  session.fire('stateChange', MeetingSessionState.Joining);
  expect(store.meetingState).toBe(MeetingSessionState.Joining);

  session.fire('joined', {
    participantId: 'self',
    userId: 7n,
    existingParticipants: [{ participantId: 'a', name: 'Ann' }],
    mediaServers: [],
    correlationId: 'c',
    bindingToken: 'b',
  });
  expect(store.participants.map((p) => p.participantId)).toEqual(['a']);

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  expect(store.participants.map((p) => p.participantId)).toEqual(['a', 'b']);

  session.fire('participantLeft', { participantId: 'a', reason: 'voluntary' });
  expect(store.participants.map((p) => p.participantId)).toEqual(['b']);

  session.fire('mediaConnected', 'https://mh-0:4433');
  expect(store.mediaConnections).toEqual(['https://mh-0:4433']);

  session.fire('error', new SdkError(SdkErrorCode.Signaling, 'nope'));
  expect(store.lastError?.code).toBe(SdkErrorCode.Signaling);
});

test('aggregate unsubscribe stops all further updates', () => {
  const session = new MockMeetingSession();
  const store = new MeetingStore();
  const unsubscribe = subscribeSession(store, session);

  session.fire('stateChange', MeetingSessionState.Joined);
  expect(store.meetingState).toBe(MeetingSessionState.Joined);

  unsubscribe();

  // After unsubscribe every listener is gone — subsequent events are ignored.
  session.fire('stateChange', MeetingSessionState.Disconnecting);
  session.fire('participantJoined', { participant: { participantId: 'x', name: 'X' } });
  session.fire('mediaConnected', 'https://mh-9:4433');
  expect(store.meetingState).toBe(MeetingSessionState.Joined);
  expect(store.participants).toEqual([]);
  expect(store.mediaConnections).toEqual([]);
});
