// R-43: MeetingStore mutator/getter reactivity (Vitest Browser Mode, Chromium).
// Drives the store's typed mutators and reads the reactive getters. Does NOT
// re-test any sdk-core logic — only the adapter's reactive container.

import { expect, test } from 'vitest';
import { MeetingSessionState, SdkError, SdkErrorCode } from '@darktower/sdk-core';
import type { JoinedEvent, RosterParticipant } from '@darktower/sdk-core';
import { MeetingStore } from '../stores/MeetingStore.svelte.js';
import { mediaStates } from './helpers/mediaStates.js';
import type { HandlerRef } from '@darktower/sdk-core';

function roster(id: string, name: string): RosterParticipant {
  return { participantId: id, name, mediaStates: mediaStates() };
}

function joined(existing: readonly RosterParticipant[]): JoinedEvent {
  return {
    participantId: 'self',
    senderId: 42,
    existingParticipants: existing,
    receiveSlotCaps: { audio: 8, video: 8 },
    handlerSet: { version: '0', handlers: [] },
    correlationId: 'corr',
    bindingToken: 'binding-secret',
  };
}

test('initial state is idle with empty roster / media / no error', () => {
  const store = new MeetingStore();
  expect(store.meetingState).toBe(MeetingSessionState.Idle);
  expect(store.participants).toEqual([]);
  expect(store.mediaConnections).toEqual([]);
  expect(store.lastError).toBeUndefined();
});

test('setState reflects the latest transition', () => {
  const store = new MeetingStore();
  store.setState(MeetingSessionState.ConnectingMc);
  expect(store.meetingState).toBe(MeetingSessionState.ConnectingMc);
  store.setState(MeetingSessionState.Joined);
  expect(store.meetingState).toBe(MeetingSessionState.Joined);
});

test('applyJoined seeds the roster from existingParticipants', () => {
  const store = new MeetingStore();
  store.applyJoined(joined([roster('a', 'Ann'), roster('b', 'Bob')]));
  expect(store.participants.map((p) => p.participantId)).toEqual(['a', 'b']);
});

test('applyParticipantJoined appends and is idempotent on participantId', () => {
  const store = new MeetingStore();
  store.applyParticipantJoined({ participant: roster('a', 'Ann') });
  store.applyParticipantJoined({ participant: roster('b', 'Bob') });
  store.applyParticipantJoined({ participant: roster('a', 'Ann-dup') });
  expect(store.participants.map((p) => p.participantId)).toEqual(['a', 'b']);
});

test('applyParticipantLeft removes by participantId', () => {
  const store = new MeetingStore();
  store.applyJoined(joined([roster('a', 'Ann'), roster('b', 'Bob')]));
  store.applyParticipantLeft({ participantId: 'a', reason: 'voluntary' });
  expect(store.participants.map((p) => p.participantId)).toEqual(['b']);
});

test('applyMediaConnected appends unique handlers, dedup BY REF (a new ref at a reused url is new)', () => {
  const store = new MeetingStore();
  const h = (ref: number, url: string) => ({ ref: ref as HandlerRef, url });
  store.applyMediaConnected(h(1, 'https://mh-0:4433'));
  store.applyMediaConnected(h(2, 'https://mh-1:4433'));
  // A redial of ref 1: not a new entry.
  store.applyMediaConnected(h(1, 'https://mh-0:4433'));
  // A NEW ref at the same url as ref 1: a different handler.
  store.applyMediaConnected(h(3, 'https://mh-0:4433'));
  expect(store.mediaConnections.map((c) => c.ref)).toEqual([1, 2, 3]);
});

test('applyError stores the typed SdkError (code preserved)', () => {
  const store = new MeetingStore();
  const err = new SdkError(SdkErrorCode.Network, 'boom', { status: 503 });
  store.applyError(err);
  expect(store.lastError).toBe(err);
  expect(store.lastError?.code).toBe(SdkErrorCode.Network);
});
