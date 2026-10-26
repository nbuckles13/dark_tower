// Story 2 task 15 / story 3 C-C1: `lib/participantState.ts` — per-participant wire facts
// to tokens, per kind (four indicators), bounded names, and the self camera state.

import { expect, test } from 'vitest';
import { MAX_DISPLAY_NAME_LENGTH } from '@darktower/sdk-core';
import type {
  ClientMediaStateToken,
  ParticipantMediaStates,
  RosterParticipant,
} from '@darktower/sdk-core';
import { MediaStore } from '@darktower/sdk-svelte';
import {
  CAMERA_FAILURE_TEXT,
  INDICATOR_TEXT,
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
import { mediaStates } from './helpers/mediaStates.js';

const roster: RosterParticipant[] = [
  { participantId: 'b', name: 'Bea', senderId: 258, mediaStates: mediaStates() },
  { participantId: 'c', name: 'Cid', senderId: 259, mediaStates: mediaStates() },
  { participantId: 'n', name: 'Nil', mediaStates: mediaStates() }, // MC published no sender id
];
const ctx = { roster, selfParticipantId: 'self' };

/** Facts as the cells would report them, from a complete per-kind snapshot. */
function facts(
  over: { unreachableSenderIds?: number[]; states?: ParticipantMediaStates } = {},
): MediaFacts {
  const states = over.states ?? mediaStates();
  return {
    unreachableSenderIds: over.unreachableSenderIds ?? [],
    kind: (k) => ({
      clientState: states[k].clientState,
      serverMuted: states[k].serverMuted,
      serverMutedBy: states[k].serverMutedBy,
    }),
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
  const named = {
    roster: [{ participantId: 'l', name: long, mediaStates: mediaStates() }],
    selfParticipantId: 'self',
  };
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

// Story 3 R-15 (ADR-0036 §5 amendment 2026-10-25): the client state is MC's
// per-kind `ClientMediaState`, rendered verbatim — no "unknown" exists.
const CLIENT_STATES: readonly ClientMediaStateToken[] = ['not_published', 'live', 'client_muted'];

test.each(
  CLIENT_STATES.flatMap((audio) =>
    CLIENT_STATES.flatMap((video) =>
      [false, true].map((serverMuted) => [audio, video, serverMuted] as const),
    ),
  ),
)(
  'client state: audio=%s video=%s (server-muted both kinds: %s) -> rendered per kind',
  (audio, video, serverMuted) => {
    const by = serverMuted ? { serverMuted: true, serverMutedBy: 'c' } : {};
    const view = participantIndicators(
      roster[0]!,
      facts({
        states: mediaStates({
          audio: { clientState: audio, ...by },
          video: { clientState: video, ...by },
        }),
      }),
      ctx,
    );
    // The client indicator reports what the participant did, the server one
    // what the host did — independent.
    expect(view.clientState).toEqual({ audio, video });
    expect(view.serverMuted).toEqual({ audio: serverMuted, video: serverMuted });
  },
);

test('every client state has its own text per kind, and "not publishing" is said plainly', () => {
  for (const kind of ['audio', 'video'] as const) {
    const texts = CLIENT_STATES.map((st) => INDICATOR_TEXT.client[kind][st]);
    expect(new Set(texts).size).toBe(CLIENT_STATES.length);
  }
  expect(INDICATOR_TEXT.client.audio.not_published).toBe('mic: not publishing');
  expect(INDICATOR_TEXT.client.video.not_published).toBe('camera: not publishing');
  // No "unknown" anywhere in the participant copy.
  expect(JSON.stringify(INDICATOR_TEXT)).not.toMatch(/unknown/);
});

test('a client state never moves a server indicator, and a server mute never moves a client one', () => {
  const serverOnly = participantIndicators(
    roster[0]!,
    facts({ states: mediaStates({ audio: { serverMuted: true, serverMutedBy: 'c' } }) }),
    ctx,
  );
  expect(serverOnly.clientState).toEqual({ audio: 'live', video: 'live' });
  const clientOnly = participantIndicators(
    roster[0]!,
    facts({
      states: mediaStates({
        audio: { clientState: 'client_muted' },
        video: { clientState: 'client_muted' },
      }),
    }),
    ctx,
  );
  expect(clientOnly.serverMuted).toEqual({ audio: false, video: false });
  expect(clientOnly.serverMutedByName).toEqual({ audio: undefined, video: undefined });
});

test('a participant with NO sender id (no slot can ever carry them) still shows their state', () => {
  expect(
    participantIndicators(
      roster[2]!,
      facts({ states: mediaStates({ video: { clientState: 'not_published' } }) }),
      ctx,
    ).clientState,
  ).toEqual({ audio: 'live', video: 'not_published' });
});

test('an UNSEEDED cell for a listed participant fails LOUDLY, never renders a guess', () => {
  const unseeded: MediaFacts = {
    unreachableSenderIds: [],
    kind: () => ({ clientState: undefined, serverMuted: false, serverMutedBy: undefined }),
  };
  expect(() => participantIndicators(roster[0]!, unseeded, ctx)).toThrow(/never seeded/);
});

// Through the REAL store and `mediaFactsFor`: what a row renders reads the
// participant cell only — never a slot.
function storeWith(apply: (store: MediaStore) => void): MediaStore {
  const store = new MediaStore();
  apply(store);
  return store;
}

const bea = roster[0]!; // sender 258

test('NO slot fallback: a slot saying avatar_client_muted does not override the state live', () => {
  const store = storeWith((s) => {
    s.applyRosterEntry(bea);
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
  expect(participantIndicators(bea, mediaFactsFor(store, bea), ctx).clientState).toEqual({
    audio: 'live',
    video: 'live',
  });
});

test('mic and camera are independent: an update moves exactly the kinds it states', () => {
  const store = storeWith((s) => {
    s.applyRosterEntry(bea);
    s.applyParticipantMute({
      participantId: 'b',
      mediaStates: mediaStates({
        audio: { serverMuted: true, serverMutedBy: 'c' },
        video: { clientState: 'client_muted' },
      }),
    });
  });
  expect(participantIndicators(bea, mediaFactsFor(store, bea), ctx)).toMatchObject({
    clientState: { audio: 'live', video: 'client_muted' },
    serverMuted: { audio: true, video: false },
    serverMutedByName: { audio: 'Cid', video: undefined },
  });
});

test('server mute carries the resolved name of whoever applied EACH kind', () => {
  const view = participantIndicators(
    roster[0]!,
    facts({
      states: mediaStates({
        audio: { serverMuted: true, serverMutedBy: 'c' },
        video: { serverMuted: true, serverMutedBy: 'self' },
      }),
    }),
    ctx,
  );
  expect(view).toMatchObject({
    serverMuted: { audio: true, video: true },
    serverMutedByName: { audio: 'Cid', video: PARTICIPANT_TEXT.you },
  });
  expect(participantIndicators(roster[0]!, facts(), ctx).serverMutedByName).toEqual({
    audio: undefined,
    video: undefined,
  });
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
