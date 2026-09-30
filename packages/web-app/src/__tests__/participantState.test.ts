// Story 2 task 15: `lib/participantState.ts` — per-participant wire facts to tokens.

import { expect, test } from 'vitest';
import type { StreamAssignmentEvent } from '@darktower/sdk-core';
import {
  PARTICIPANT_TEXT,
  nameForParticipantId,
  nameForSenderId,
  participantIndicators,
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
): StreamAssignmentEvent {
  return { slotId: 0, senderId, mediaHandlerUrl: 'https://mh', slotState };
}

function facts(over: Partial<MediaFacts> = {}): MediaFacts {
  return { slots: [], unreachableSenderIds: [], serverMutes: new Map(), ...over };
}

test('names resolve through the roster; self is "you"; unknown is a bounded placeholder', () => {
  expect(nameForParticipantId('b', ctx)).toBe('Bea');
  expect(nameForParticipantId('self', ctx)).toBe(PARTICIPANT_TEXT.you);
  expect(nameForParticipantId('zzz', ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
  expect(nameForParticipantId(undefined, ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
  expect(nameForSenderId(259, ctx)).toBe('Cid');
  expect(nameForSenderId(1, ctx)).toBe(PARTICIPANT_TEXT.unknownParticipant);
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
  ['active slot -> not muted', [slot(258, 'active')], new Map(), 'false'],
  [
    'source_muted slot, no server mute -> client-muted',
    [slot(258, 'source_muted')],
    new Map(),
    'true',
  ],
  ['no slot carries them -> unknown', [], new Map(), 'unknown'],
  ['withheld slot -> unknown', [slot(258, 'withheld_congestion')], new Map(), 'unknown'],
  [
    'server-muted -> unknown (the slot state says nothing about their mic)',
    [slot(258, 'source_muted')],
    new Map([['b', { serverMutedBy: 'c' }]]),
    'unknown',
  ],
] as const)('client mute: %s', (_label, slots, serverMutes, expected) => {
  expect(participantIndicators(roster[0]!, facts({ slots, serverMutes }), ctx).clientMuted).toBe(
    expected,
  );
});

test('server mute carries the resolved name of whoever applied it', () => {
  const view = participantIndicators(
    roster[0]!,
    facts({ serverMutes: new Map([['b', { serverMutedBy: 'c' }]]) }),
    ctx,
  );
  expect(view).toMatchObject({ serverMuted: true, serverMutedByName: 'Cid' });
  expect(participantIndicators(roster[0]!, facts(), ctx).serverMutedByName).toBeUndefined();
});
