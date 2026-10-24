// Story 2 task 15 / story 3 C-C1: `lib/participantState.ts` — per-participant wire facts
// to tokens, per kind (four indicators), bounded names, and the self camera state.

import { expect, test } from 'vitest';
import { MAX_DISPLAY_NAME_LENGTH } from '@darktower/sdk-core';
import type { StreamAssignmentEvent } from '@darktower/sdk-core';
import {
  CAMERA_FAILURE_TEXT,
  PARTICIPANT_TEXT,
  SELF_CAMERA_STATES,
  SELF_CAMERA_TEXT,
  boundedName,
  nameForParticipantId,
  nameForSenderId,
  participantIndicators,
  selfCameraState,
  type MediaFacts,
} from '../lib/participantState.js';

const roster = [
  { participantId: 'b', name: 'Bea', senderId: 258 },
  { participantId: 'c', name: 'Cid', senderId: 259 },
  { participantId: 'n', name: 'Nil' }, // MC published no sender id
];
const ctx = { roster, selfParticipantId: 'self' };

function slot(
  senderId: number,
  slotState: StreamAssignmentEvent['slotState'],
  mediaKind: 'audio' | 'video' = 'audio',
): StreamAssignmentEvent {
  return { slotId: 0, mediaKind, senderId, mediaHandlerUrl: 'https://mh', slotState };
}

const NOT_SERVER_MUTED = {
  audioServerMuted: false,
  videoServerMuted: false,
  serverMutedBy: undefined,
};

function facts(over: Partial<MediaFacts> = {}): MediaFacts {
  return {
    audioSlot: undefined,
    videoSlot: undefined,
    videoRender: undefined,
    unreachableSenderIds: [],
    serverMute: NOT_SERVER_MUTED,
    ...over,
  };
}

test('names resolve through the roster; self is "you"; unknown is a bounded placeholder', () => {
  expect(nameForParticipantId('b', ctx)).toBe('Bea');
  expect(nameForParticipantId('self', ctx)).toBe(PARTICIPANT_TEXT.you);
  expect(nameForParticipantId('zzz', ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
  expect(nameForParticipantId(undefined, ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
  expect(nameForSenderId(259, ctx)).toBe('Cid');
  expect(nameForSenderId(1, ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
});

test('a rendered name is BOUNDED to the SDK display-name limit', () => {
  const long = 'x'.repeat(MAX_DISPLAY_NAME_LENGTH + 50);
  const named = { roster: [{ participantId: 'l', name: long }], selfParticipantId: 'self' };
  const shown = nameForParticipantId('l', named);
  expect(shown.length).toBe(MAX_DISPLAY_NAME_LENGTH + 1);
  expect(boundedName('short')).toBe('short');
});

test('reachability is the wire token when the sender id is in the unreachable set', () => {
  const view = participantIndicators(roster[1]!, facts({ unreachableSenderIds: [259] }), ctx);
  expect(view.reachability).toBe('source_unreachable');
  expect(
    participantIndicators(roster[0]!, facts({ unreachableSenderIds: [259] }), ctx).reachability,
  ).toBe('reachable');
  // No sender id: cannot be matched, so never marked from nothing.
  expect(
    participantIndicators(roster[2]!, facts({ unreachableSenderIds: [259] }), ctx).reachability,
  ).toBe('reachable');
});

test.each([
  ['active audio slot -> not muted', { audioSlot: slot(258, 'active') }, 'false'],
  [
    'avatar_client_muted audio slot, no server mute -> client-muted',
    { audioSlot: slot(258, 'avatar_client_muted') },
    'true',
  ],
  ['no slot carries them -> unknown', {}, 'unknown'],
  [
    'only a VIDEO slot carries them -> audio is unknown',
    { videoSlot: slot(258, 'avatar_client_muted', 'video') },
    'unknown',
  ],
  ['withheld slot -> unknown', { audioSlot: slot(258, 'withheld_by_congestion') }, 'unknown'],
  [
    'audio server-muted -> unknown (the slot state says nothing about their mic)',
    {
      audioSlot: slot(258, 'avatar_server_muted'),
      serverMute: { ...NOT_SERVER_MUTED, audioServerMuted: true, serverMutedBy: 'c' },
    },
    'unknown',
  ],
] as const)('client AUDIO mute: %s', (_label, over, expected) => {
  expect(participantIndicators(roster[0]!, facts(over), ctx).clientMuted.audio).toBe(expected);
});

test('client VIDEO mute is per kind: avatar_client_muted on their video slot, or false while live', () => {
  expect(
    participantIndicators(
      roster[0]!,
      facts({ videoSlot: slot(258, 'avatar_client_muted', 'video') }),
      ctx,
    ).clientMuted,
  ).toEqual({ audio: 'unknown', video: 'true' });
  // A live video slot names no sender; the verified render tag does (R-5).
  expect(
    participantIndicators(
      roster[0]!,
      facts({
        videoSlot: { ...slot(258, 'active', 'video'), senderId: undefined },
        videoRender: { kind: 'active', senderId: 258 },
      }),
      ctx,
    ).clientMuted.video,
  ).toBe('false');
  // Video server-muted: unknown for video, audio untouched.
  expect(
    participantIndicators(
      roster[0]!,
      facts({
        audioSlot: slot(258, 'active'),
        videoSlot: slot(258, 'avatar_server_muted', 'video'),
        serverMute: { ...NOT_SERVER_MUTED, videoServerMuted: true },
      }),
      ctx,
    ),
  ).toMatchObject({
    clientMuted: { audio: 'false', video: 'unknown' },
    serverMuted: { audio: false, video: true },
  });
});

test('server mute carries the resolved name of whoever applied it', () => {
  const view = participantIndicators(
    roster[0]!,
    facts({ serverMute: { ...NOT_SERVER_MUTED, audioServerMuted: true, serverMutedBy: 'c' } }),
    ctx,
  );
  expect(view).toMatchObject({
    serverMuted: { audio: true, video: false },
    serverMutedByName: 'Cid',
  });
  expect(participantIndicators(roster[0]!, facts(), ctx).serverMutedByName).toBeUndefined();
});

test('self camera state: precedence not-publishing > failed > host > self > on, copy exhaustive', () => {
  const base = {
    publishesVideo: true,
    cameraFailure: undefined,
    videoServerMuted: false,
    videoMuted: false,
  };
  expect(selfCameraState({ ...base, publishesVideo: false, cameraFailure: 'ended' })).toBe(
    'not_publishing',
  );
  expect(selfCameraState({ ...base, cameraFailure: 'ended', videoServerMuted: true })).toBe(
    'failed',
  );
  expect(selfCameraState({ ...base, videoServerMuted: true, videoMuted: true })).toBe('host_off');
  expect(selfCameraState({ ...base, videoMuted: true })).toBe('muted');
  expect(selfCameraState(base)).toBe('on');
  for (const state of SELF_CAMERA_STATES) expect(SELF_CAMERA_TEXT[state]).not.toBe('');
  expect(new Set(Object.values(CAMERA_FAILURE_TEXT)).size).toBe(
    Object.keys(CAMERA_FAILURE_TEXT).length,
  );
});
