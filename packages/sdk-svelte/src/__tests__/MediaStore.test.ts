// File: packages/sdk-svelte/src/__tests__/MediaStore.test.ts
//
// The media stores, and the two properties they exist to provide:
//
//   1. Every value is a PROJECTION of an SDK event, never a derivation. The
//      indicator a user reads must be the state that gated capture (ADR-0036
//      §5), so a test that drove the store by any other route would be testing
//      the wrong thing.
//   2. A mute toggle does not invalidate the roster projection ("minimizing
//      re-renders on mute and unmute"). Asserted by COUNTING recomputations,
//      with a positive control — a claim about re-renders that is not measured
//      is a comment.

import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { MeetingSessionState } from '@darktower/sdk-core';
import type { StreamAssignmentEvent } from '@darktower/sdk-core';
import { MediaStore } from '../stores/MediaStore.svelte.js';
import BoundHarness from './helpers/BoundHarness.svelte';
import GranularityHarness from './helpers/GranularityHarness.svelte';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

function slot(overrides: Partial<StreamAssignmentEvent> = {}): StreamAssignmentEvent {
  return {
    slotId: 0,
    senderId: 258,
    mediaHandlerUrl: 'https://mh-0:4433',
    slotState: 'active',
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// The container, driven directly
// ---------------------------------------------------------------------------

test('starts unmuted with no media, no slots and no fault', () => {
  const store = new MediaStore();
  // `false`, not "unknown": `MuteState` starts unmuted, so there is no third
  // state for a UI to mishandle.
  expect(store.audioMuted).toBe(false);
  expect(store.firstMediaFrameMs).toBeUndefined();
  expect(store.hasReceivedMedia).toBe(false);
  expect(store.slots).toEqual([]);
  expect(store.lastMediaFault).toBeUndefined();
});

test('mute state mirrors the SDK snapshot in both directions', () => {
  const store = new MediaStore();
  store.applyMuteChanged({ audioMuted: true });
  expect(store.audioMuted).toBe(true);
  store.applyMuteChanged({ audioMuted: false });
  expect(store.audioMuted).toBe(false);
});

test('first-media is recorded once and never moves', () => {
  // The SDK observes this at most once per session; a second write would turn a
  // startup measurement into something that drifts, and a UI showing a changing
  // "time to first audio" would be reporting a number that means nothing.
  const store = new MediaStore();
  store.applyFirstMediaFrame(87);
  store.applyFirstMediaFrame(9_999);
  expect(store.firstMediaFrameMs).toBe(87);
  expect(store.hasReceivedMedia).toBe(true);
});

test('slot assignments REPLACE rather than merge', () => {
  // The message is MC's complete current view of this subscriber's slots, so
  // merging would resurrect a slot MC has dropped — a grid cell for a
  // participant the controller has already removed.
  const store = new MediaStore();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [slot({ slotId: 0 }), slot({ slotId: 1 })],
  });
  expect(store.slots).toHaveLength(2);

  store.applyStreamAssignments({ unreachableSenderIds: [], assignments: [slot({ slotId: 0 })] });
  expect(store.slots).toHaveLength(1);
  expect(store.slots[0]?.slotId).toBe(0);
});

test('the unreachable set REPLACES rather than merges — a flicker does not latch', () => {
  // ---------------------------------------------------------------------------
  // WHAT THIS PINS, AND FOR WHOM
  // ---------------------------------------------------------------------------
  //
  // `StreamAssignments.unreachable_sender_ids` (ADR-0036 §9) names the roster
  // participants THIS subscriber shares no connected handler with. Without a
  // settle rule on the controller side, a peer can enter and leave that set
  // during an ordinary multi-handler join, between its first and second handler
  // connection.
  //
  // So the property that matters operationally is not "unreachable renders" — it
  // is that leaving the set returns to normal WITHOUT needing a "now reachable"
  // signal of its own. A merged set would latch the transient into a permanent
  // "can't hear X" badge on a meeting whose audio is fine, which is
  // indistinguishable to the user from the real fault and sends oncall chasing a
  // connectivity break that does not exist.
  //
  // This is the seam task 15 renders from, so the no-latch property is pinned
  // here, where it is observable, rather than asserted of a DOM that does not
  // exist yet.
  const store = new MediaStore();
  expect(store.unreachableSenderIds).toEqual([]);

  // B and C are each unreachable to the other: the canonical three-participant
  // case, seen from B.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259, 260],
  });
  expect(store.unreachableSenderIds).toEqual([259, 260]);

  // C's second handler connection lands. C is reachable now, and says so only by
  // being ABSENT from the new set.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259],
  });
  expect(store.unreachableSenderIds).toEqual([259]);

  // Everyone is connected everywhere: the set empties. A merge would still be
  // holding 259 and 260 here, which is the bug.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [],
  });
  expect(store.unreachableSenderIds).toEqual([]);
});

test('the unreachable set is independent of the slot list', () => {
  // A message MAY carry zero assignments and a non-empty unreachable set — every
  // other participant is elsewhere — and that is well-formed, not malformed
  // (`signaling.proto`). An unreachable participant consumes NO slot, so the two
  // must not be derived from one another in either direction.
  const store = new MediaStore();
  store.applyStreamAssignments({ assignments: [], unreachableSenderIds: [259] });
  expect(store.slots).toEqual([]);
  expect(store.unreachableSenderIds).toEqual([259]);

  // And slots can refill while the set stays put.
  store.applyStreamAssignments({
    assignments: [slot({ slotId: 0, senderId: 258 })],
    unreachableSenderIds: [259],
  });
  expect(store.slots).toHaveLength(1);
  expect(store.unreachableSenderIds).toEqual([259]);
});

test('slot state is carried through verbatim, including the muted and unreachable cases', () => {
  // ADR-0036 §6: "withheld by congestion", "fewer sources" and "source
  // unreachable" are indistinguishable to a client — all present as no media —
  // and render completely differently. The store must not collapse them.
  const store = new MediaStore();
  store.applyStreamAssignments({
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, slotState: 'source_muted' }),
      slot({ slotId: 1, slotState: 'withheld_congestion' }),
      slot({ slotId: 2, slotState: 'source_unreachable' }),
    ],
  });
  expect(store.slots.map((s) => s.slotState)).toEqual([
    'source_muted',
    'withheld_congestion',
    'source_unreachable',
  ]);
});

test('media faults are held as the bounded SDK value', () => {
  const store = new MediaStore();
  store.applyMediaFault({ stage: 'decoder', message: 'the audio decoder failed', fatal: true });
  expect(store.lastMediaFault?.stage).toBe('decoder');
});

// ---------------------------------------------------------------------------
// Bound through a real component, via real SDK events
// ---------------------------------------------------------------------------

test('bindMeetingSession wires all four media events into store.media', async () => {
  const session = new MockMeetingSession();
  const screen = await render(BoundHarness, { session });

  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('unmuted');

  session.fire('muteChanged', { audioMuted: true });
  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('muted');

  session.fire('firstMediaFrame', 42);
  await expect.element(screen.getByTestId('first-media-ms')).toHaveTextContent('42');

  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [slot(), slot({ slotId: 1 })],
  });
  await expect.element(screen.getByTestId('slot-count')).toHaveTextContent('2');

  session.fire('mediaFault', { stage: 'playback', message: 'playback stopped', fatal: false });
  await expect.element(screen.getByTestId('media-fault')).toHaveTextContent('playback');
});

test('unmount tears down the media subscriptions too — no leak', async () => {
  // The four media listeners join the SAME aggregate unsubscribe as the roster
  // listeners. A separate binding helper would have been a second teardown to
  // forget, which is what this asserts did not happen.
  const session = new MockMeetingSession();
  const screen = await render(BoundHarness, { session });

  session.fire('muteChanged', { audioMuted: true });
  await expect.element(screen.getByTestId('audio-muted')).toHaveTextContent('muted');

  screen.unmount();

  expect(() => {
    session.fire('muteChanged', { audioMuted: false });
    session.fire('firstMediaFrame', 1);
    session.fire('streamAssignments', { unreachableSenderIds: [], assignments: [slot()] });
    session.fire('mediaFault', { stage: 'crypto', message: 'x', fatal: false });
  }).not.toThrow();
});

// ---------------------------------------------------------------------------
// Re-render granularity
// ---------------------------------------------------------------------------

test('a mute toggle does not recompute the roster projection', async () => {
  const counts = { roster: 0, mute: 0 };
  const session = new MockMeetingSession();
  const screen = await render(GranularityHarness, { session, counts });

  session.fire('stateChange', MeetingSessionState.Joined);
  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann');

  const rosterBaseline = counts.roster;
  const muteBaseline = counts.mute;

  session.fire('muteChanged', { audioMuted: true });
  await expect.element(screen.getByTestId('mute-text')).toHaveTextContent('muted');
  session.fire('muteChanged', { audioMuted: false });
  await expect.element(screen.getByTestId('mute-text')).toHaveTextContent('unmuted');

  // The property: two mute transitions, zero roster recomputations. This is
  // what the separate `$state` cell in `MediaStore` and the non-reactive
  // `MeetingStore.media` field buy; folding mute into the roster store, or into
  // one `$state` object holding both, breaks exactly this line.
  expect(counts.roster).toBe(rosterBaseline);

  // POSITIVE CONTROL — without it the assertion above passes just as well on a
  // harness whose derivations never recompute at all (an unrendered `$derived`,
  // a broken subscription, a store that stopped updating).
  expect(counts.mute).toBeGreaterThan(muteBaseline);

  const rosterBeforeJoin = counts.roster;
  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann,Bob');
  expect(counts.roster).toBeGreaterThan(rosterBeforeJoin);
});

test('a roster change does not recompute the mute projection', async () => {
  // The inverse direction, which matters during a call: a participant joining
  // must not invalidate the mute indicator every grid re-layout.
  const counts = { roster: 0, mute: 0 };
  const session = new MockMeetingSession();
  const screen = await render(GranularityHarness, { session, counts });

  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann');

  const muteBaseline = counts.mute;
  const rosterBaseline = counts.roster;

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  await expect.element(screen.getByTestId('roster-text')).toHaveTextContent('Ann,Bob');

  expect(counts.mute).toBe(muteBaseline);
  expect(counts.roster).toBeGreaterThan(rosterBaseline);
});

// ---------------------------------------------------------------------------
// Server mute + unmute requests (story 2 R-10/R-11) — wire-only, replace-on-change
// ---------------------------------------------------------------------------

test('server mutes come ONLY from participantMuteChanged, keyed by participant, replaced per change', () => {
  const store = new MediaStore();
  expect(store.serverMutes.size).toBe(0);
  store.applyParticipantMute({ participantId: 'b', audioServerMuted: true, serverMutedBy: 'h' });
  const first = store.serverMutes;
  expect(first.get('b')).toEqual({ serverMutedBy: 'h' });
  store.applyParticipantMute({ participantId: 'c', audioServerMuted: true });
  // Replaced, never mutated in place: a reader holding the old map sees no change.
  expect(store.serverMutes).not.toBe(first);
  expect(first.has('c')).toBe(false);
  store.applyParticipantMute({ participantId: 'b', audioServerMuted: false });
  expect([...store.serverMutes.keys()]).toEqual(['c']);
});

test('an unmute request is held only while the requester IS server-muted, and clears on lift', () => {
  const store = new MediaStore();
  // A relay for someone not server-muted is not a pending request.
  store.applyUnmuteRequested({ participantId: 'b' });
  expect(store.unmuteRequests.size).toBe(0);

  store.applyParticipantMute({ participantId: 'b', audioServerMuted: true, serverMutedBy: 'h' });
  store.applyUnmuteRequested({ participantId: 'b' });
  expect([...store.unmuteRequests]).toEqual(['b']);
  // The request lifts NOTHING: b is still server-muted (R-10).
  expect(store.serverMutes.has('b')).toBe(true);

  store.applyParticipantMute({ participantId: 'b', audioServerMuted: false });
  expect(store.unmuteRequests.size).toBe(0);
});
