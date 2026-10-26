// R-43: subscribeSession — the pure event→state SPOT. Drives REAL typed events
// through a mock MeetingSession (TypedEventEmitter subclass) and asserts the
// store reacts; then asserts the aggregate unsubscribe removes every listener.

import { expect, test } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import { MeetingStore } from '../stores/MeetingStore.svelte.js';
import { subscribeSession } from '../stores/bindMeetingSession.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';
import { mediaStates } from './helpers/mediaStates.js';

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
    senderId: 7,
    existingParticipants: [{ participantId: 'a', name: 'Ann', mediaStates: mediaStates() }],
    receiveSlotCaps: { audio: 8, video: 8 },
    mediaServers: [],
    correlationId: 'c',
    bindingToken: 'b',
  });
  expect(store.participants.map((p) => p.participantId)).toEqual(['a']);

  session.fire('participantJoined', {
    participant: { participantId: 'b', name: 'Bob', mediaStates: mediaStates() },
  });
  expect(store.participants.map((p) => p.participantId)).toEqual(['a', 'b']);

  session.fire('participantLeft', { participantId: 'a', reason: 'voluntary' });
  expect(store.participants.map((p) => p.participantId)).toEqual(['b']);

  session.fire('mediaConnected', 'https://mh-0:4433');
  expect(store.mediaConnections).toEqual(['https://mh-0:4433']);

  session.fire('error', new SdkError(SdkErrorCode.Signaling, 'nope'));
  expect(store.lastError?.code).toBe(SdkErrorCode.Signaling);

  session.fire('participantMuteChanged', {
    participantId: 'b',
    mediaStates: mediaStates({ audio: { serverMuted: true } }),
  });
  expect(store.media.participant('b').kind('audio').serverMuted).toBe(true);
  session.fire('unmuteRequested', { participantId: 'b', kinds: ['audio'] });
  expect(store.media.participant('b').unmuteRequested).toBe(true);

  // Story 3 C-C1: render state, the R-18 latch and camera health, same binding.
  session.fire('videoSlotRenderChanged', { slotId: 4, state: { kind: 'active', senderId: 9 } });
  expect(store.media.slot(4).render).toEqual({ kind: 'active', senderId: 9 });
  expect(store.media.pairOf(9).videoSlotId).toBe(4);
  session.fire('captureResumedAfterServerMute', { kind: 'video' });
  expect(store.media.resumeNoticePending('video')).toBe(true);
  session.fire('cameraHealthChanged', { failure: 'ended' });
  expect(store.media.cameraFailure).toBe('ended');
});

test("the roster carries each participant's sender id through, never coerced", () => {
  const session = new MockMeetingSession();
  const store = new MeetingStore();
  subscribeSession(store, session);
  session.fire('joined', {
    participantId: 'self',
    senderId: 7,
    existingParticipants: [
      { participantId: 'a', name: 'Ann', senderId: 9, mediaStates: mediaStates() },
    ],
    receiveSlotCaps: { audio: 8, video: 8 },
    mediaServers: [],
    correlationId: 'c',
    bindingToken: 'b',
  });
  session.fire('participantJoined', {
    participant: { participantId: 'b', name: 'Bob', mediaStates: mediaStates() },
  });
  expect(store.participants.map((p) => p.senderId)).toEqual([9, undefined]);
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
  session.fire('participantJoined', {
    participant: { participantId: 'x', name: 'X', mediaStates: mediaStates() },
  });
  session.fire('mediaConnected', 'https://mh-9:4433');
  session.fire('participantMuteChanged', {
    participantId: 'x',
    mediaStates: mediaStates({ audio: { serverMuted: true } }),
  });
  session.fire('captureResumedAfterServerMute', { kind: 'audio' });
  session.fire('videoSlotRenderChanged', { slotId: 4, state: { kind: 'unavailable' } });
  session.fire('cameraHealthChanged', { failure: 'ended' });
  expect(store.media.participant('x').kind('audio').serverMuted).toBe(false);
  expect(store.media.resumeNoticePending('audio')).toBe(false);
  expect(store.media.slot(4).render).toBeUndefined();
  expect(store.media.cameraFailure).toBeUndefined();
  expect(store.meetingState).toBe(MeetingSessionState.Joined);
  expect(store.participants).toEqual([]);
  expect(store.mediaConnections).toEqual([]);
});
