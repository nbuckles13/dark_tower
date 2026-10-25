// Story 2 task 15 / story 3 C-C1: `lib/participantState.ts` — per-participant wire facts
// to tokens, per kind (four indicators), bounded names, and the self camera state.

import { expect, test } from 'vitest';
import { MAX_DISPLAY_NAME_LENGTH } from '@darktower/sdk-core';
import { MediaStore } from '@darktower/sdk-svelte';
import {
  CAMERA_FAILURE_TEXT,
  PARTICIPANT_TEXT,
  SELF_CAMERA_STATES,
  SELF_CAMERA_TEXT,
  boundedName,
  nameForParticipantId,
  nameForSenderId,
  mediaFactsFor,
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

const NOT_SERVER_MUTED = {
  audioServerMuted: false,
  videoServerMuted: false,
  serverMutedBy: undefined,
};

const NO_CLIENT_FACT = { audioClientMuted: undefined, videoClientMuted: undefined };

function facts(over: Partial<MediaFacts> = {}): MediaFacts {
  return {
    unreachableSenderIds: [],
    serverMute: NOT_SERVER_MUTED,
    clientMute: NO_CLIENT_FACT,
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

// Story 3 R-15: client mute is the participant's ROSTER fact (the store's
// participant cell), per kind, tri-state: `unknown` ONLY before a fact.
const FACT = { none: undefined, off: false, on: true } as const;
const TOKEN = { none: 'unknown', off: 'false', on: 'true' } as const;
const FACT_KEYS = ['none', 'off', 'on'] as const;

test.each(
  FACT_KEYS.flatMap((audio) =>
    FACT_KEYS.flatMap((video) =>
      [false, true].map((serverMuted) => [audio, video, serverMuted] as const),
    ),
  ),
)(
  'client mute: audio=%s video=%s (server-muted both kinds: %s) -> the fact, per kind',
  (audio, video, serverMuted) => {
    const view = participantIndicators(
      roster[0]!,
      facts({
        clientMute: { audioClientMuted: FACT[audio], videoClientMuted: FACT[video] },
        serverMute: serverMuted
          ? { audioServerMuted: true, videoServerMuted: true, serverMutedBy: 'c' }
          : NOT_SERVER_MUTED,
      }),
      ctx,
    );
    // Server mute no longer forces `unknown`: the client indicator reports
    // what the participant did, the server one what the host did.
    expect(view.clientMuted).toEqual({ audio: TOKEN[audio], video: TOKEN[video] });
    expect(view.serverMuted).toEqual({ audio: serverMuted, video: serverMuted });
  },
);

test('a client-mute fact never moves a server indicator, and a server mute never moves a client one', () => {
  const serverOnly = participantIndicators(
    roster[0]!,
    facts({ serverMute: { ...NOT_SERVER_MUTED, audioServerMuted: true, serverMutedBy: 'c' } }),
    ctx,
  );
  expect(serverOnly.clientMuted).toEqual({ audio: 'unknown', video: 'unknown' });
  const clientOnly = participantIndicators(
    roster[0]!,
    facts({ clientMute: { audioClientMuted: true, videoClientMuted: true } }),
    ctx,
  );
  expect(clientOnly.serverMuted).toEqual({ audio: false, video: false });
  expect(clientOnly.serverMutedByName).toBeUndefined();
});

test('a participant with NO sender id (no slot can ever carry them) still shows their client mute', () => {
  expect(
    participantIndicators(
      roster[2]!,
      facts({ clientMute: { audioClientMuted: false, videoClientMuted: true } }),
      ctx,
    ).clientMuted,
  ).toEqual({ audio: 'false', video: 'true' });
});

// Through the REAL store and `mediaFactsFor`: what a row renders reads the
// participant cell only — never a slot, and independent of arrival order.
function storeWith(apply: (store: MediaStore) => void): MediaStore {
  const store = new MediaStore();
  apply(store);
  return store;
}

const bea = roster[0]!; // sender 258

test('NO slot fallback: a slot saying avatar_client_muted does not override the fact false', () => {
  const store = storeWith((s) => {
    s.applyRosterEntry({ participantId: 'b', audioClientMuted: false, videoClientMuted: false });
    s.applyStreamAssignments({
      unreachableSenderIds: [],
      assignments: [
        {
          slotId: 0,
          mediaKind: 'audio',
          senderId: 258,
          mediaHandlerUrl: 'https://mh',
          slotState: 'avatar_client_muted',
        },
        {
          slotId: 1,
          mediaKind: 'video',
          senderId: 258,
          mediaHandlerUrl: 'https://mh',
          slotState: 'avatar_client_muted',
        },
      ],
    });
  });
  expect(participantIndicators(bea, mediaFactsFor(store, bea), ctx).clientMuted).toEqual({
    audio: 'false',
    video: 'false',
  });
  // And a slot alone, with no fact, is still `unknown` — never read as a fact.
  const slotOnly = storeWith((s) =>
    s.applyStreamAssignments({
      unreachableSenderIds: [],
      assignments: [
        {
          slotId: 0,
          mediaKind: 'audio',
          senderId: 258,
          mediaHandlerUrl: 'https://mh',
          slotState: 'avatar_client_muted',
        },
      ],
    }),
  );
  expect(participantIndicators(bea, mediaFactsFor(slotOnly, bea), ctx).clientMuted.audio).toBe(
    'unknown',
  );
});

test('arrival order does not matter: server-then-client and client-then-server render the same', () => {
  const server = {
    participantId: 'b',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'c',
  } as const;
  const client = { participantId: 'b', audioClientMuted: true, videoClientMuted: false } as const;
  const a = storeWith((s) => {
    s.applyParticipantMute(server);
    s.applyRosterEntry(client);
  });
  const b = storeWith((s) => {
    s.applyRosterEntry(client);
    s.applyParticipantMute(server);
  });
  const viewA = participantIndicators(bea, mediaFactsFor(a, bea), ctx);
  expect(viewA).toEqual(participantIndicators(bea, mediaFactsFor(b, bea), ctx));
  expect(viewA).toMatchObject({
    clientMuted: { audio: 'true', video: 'false' },
    serverMuted: { audio: true, video: false },
    serverMutedByName: 'Cid',
  });
});

test('mic and camera are independent: moving one kind leaves the other fact alone', () => {
  const store = storeWith((s) => {
    s.applyRosterEntry({ participantId: 'b', audioClientMuted: false, videoClientMuted: false });
    s.applyParticipantMute({
      participantId: 'b',
      audioServerMuted: false,
      videoServerMuted: false,
      videoClientMuted: true,
    });
  });
  expect(participantIndicators(bea, mediaFactsFor(store, bea), ctx).clientMuted).toEqual({
    audio: 'false',
    video: 'true',
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
