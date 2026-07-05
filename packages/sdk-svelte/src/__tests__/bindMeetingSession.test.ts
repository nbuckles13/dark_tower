// R-43: bindMeetingSession in a real component — reactivity through the DOM plus
// the behavior-based unmount-leak assertion (@test top-priority): after unmount,
// firing events must NOT mutate the store (proves onDestroy ran the aggregate
// unsubscribe — not a spy on `.off`, the actual no-leak property).

import { expect, test } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { MeetingSessionState } from '@darktower/sdk-core';
import type { MeetingStore } from '../stores/MeetingStore.svelte.js';
import BoundHarness from './helpers/BoundHarness.svelte';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

test('component renders reactive store state and live roster', async () => {
  const session = new MockMeetingSession();
  const screen = render(BoundHarness, { session });

  await expect.element(screen.getByTestId('state')).toHaveTextContent(MeetingSessionState.Idle);

  session.fire('stateChange', MeetingSessionState.Joined);
  await expect.element(screen.getByTestId('state')).toHaveTextContent(MeetingSessionState.Joined);

  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('participant-a')).toHaveTextContent('Ann');

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  await expect.element(screen.getByTestId('participant-b')).toHaveTextContent('Bob');

  session.fire('participantLeft', { participantId: 'a', reason: 'voluntary' });
  await expect.element(screen.getByTestId('participant-a')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('participant-b')).toBeInTheDocument();
});

test('unmount tears down the subscription — no leak (post-unmount events ignored)', async () => {
  const session = new MockMeetingSession();
  let store: MeetingStore | undefined;
  const screen = render(BoundHarness, {
    session,
    onStore: (s) => {
      store = s;
    },
  });

  session.fire('stateChange', MeetingSessionState.Joining);
  await expect.element(screen.getByTestId('state')).toHaveTextContent(MeetingSessionState.Joining);
  expect(store?.meetingState).toBe(MeetingSessionState.Joining);

  // Unmount fires the component's onDestroy → aggregate unsubscribe.
  screen.unmount();

  // Events fired after unmount must NOT mutate the (still-referenced) store,
  // and must not throw — the listeners are gone.
  expect(() => {
    session.fire('stateChange', MeetingSessionState.Disconnecting);
    session.fire('participantJoined', { participant: { participantId: 'z', name: 'Zed' } });
    session.fire('mediaConnected', 'https://mh-0:4433');
  }).not.toThrow();

  expect(store?.meetingState).toBe(MeetingSessionState.Joining);
  expect(store?.participants).toEqual([]);
  expect(store?.mediaConnections).toEqual([]);
});
