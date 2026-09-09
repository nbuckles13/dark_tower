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
  store.applyStreamAssignments({ assignments: [slot({ slotId: 0 }), slot({ slotId: 1 })] });
  expect(store.slots).toHaveLength(2);

  store.applyStreamAssignments({ assignments: [slot({ slotId: 0 })] });
  expect(store.slots).toHaveLength(1);
  expect(store.slots[0]?.slotId).toBe(0);
});

test('slot state is carried through verbatim, including the muted and unreachable cases', () => {
  // ADR-0036 §6: "withheld by congestion", "fewer sources" and "source
  // unreachable" are indistinguishable to a client — all present as no media —
  // and render completely differently. The store must not collapse them.
  const store = new MediaStore();
  store.applyStreamAssignments({
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

  session.fire('streamAssignments', { assignments: [slot(), slot({ slotId: 1 })] });
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
    session.fire('streamAssignments', { assignments: [slot()] });
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
