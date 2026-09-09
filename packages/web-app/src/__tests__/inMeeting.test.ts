// File: packages/web-app/src/__tests__/inMeeting.test.ts
//
// The in-meeting view, and the one property it exists to guarantee: **every
// "nothing is arriving" condition is rendered from an explicit SDK signal, never
// inferred.**
//
// The tests are written to fail on the WRONG implementations rather than to
// confirm the right one, because the wrong ones all look plausible:
//
//   * an indicator set from the click reads correctly in the happy case and lies
//     the moment the SDK refuses;
//   * an indicator derived from frame counts inverts DANGEROUSLY — a transport
//     stall would show "muted" beside a live microphone;
//   * a device picker that updates its own `<select>` and calls nothing looks
//     identical to one that switches the device.
//
// Runs in real Chromium (this package's vitest browser config), so `<select>`
// change events and ARIA state are the platform's, not a simulation's.

import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { MeetingSessionState } from '@darktower/sdk-core';
import type { MeetingSession, StreamAssignmentEvent } from '@darktower/sdk-core';
import { MeetingStore, subscribeSession } from '@darktower/sdk-svelte';
import InMeeting from '../views/InMeeting.svelte';
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

interface Rig {
  readonly session: MockMeetingSession;
  readonly store: MeetingStore;
  readonly screen: Awaited<ReturnType<typeof render>>;
}

/**
 * Mount the view bound to a mock session through the REAL adapter binding.
 *
 * `subscribeSession` rather than a hand-built store: the property under test is
 * that the view renders what the SDK reports, and a store populated by the test
 * directly would skip the very wiring that carries it.
 *
 * The device list is controlled at the PLATFORM boundary
 * (`navigator.mediaDevices.enumerateDevices`) rather than by replacing the SDK's
 * `listMicrophones`. Two reasons: an ESM export cannot be spied in browser mode
 * anyway, and stubbing one layer lower keeps the SDK's own `audioinput`
 * filtering inside the tested path instead of mocking it away.
 */
async function mount(microphones: MediaDeviceInfo[] = []): Promise<Rig> {
  stubEnumerateDevices(async () => microphones);
  const session = new MockMeetingSession();
  session.state = MeetingSessionState.Joined;
  const store = new MeetingStore();
  subscribeSession(store, session);
  const screen = await render(InMeeting, {
    session: session as unknown as MeetingSession,
    store,
  });
  return { session, store, screen };
}

/** Restores whatever `enumerateDevices` was before the test replaced it. */
let restoreEnumerate: (() => void) | undefined;

function stubEnumerateDevices(impl: () => Promise<MediaDeviceInfo[]>): void {
  const target = navigator.mediaDevices;
  const original = target.enumerateDevices;
  Object.defineProperty(target, 'enumerateDevices', {
    configurable: true,
    writable: true,
    value: impl,
  });
  restoreEnumerate = () => {
    Object.defineProperty(target, 'enumerateDevices', {
      configurable: true,
      writable: true,
      value: original,
    });
  };
}

afterEach(() => {
  restoreEnumerate?.();
  restoreEnumerate = undefined;
});

function device(id: string, label: string): MediaDeviceInfo {
  return {
    deviceId: id,
    kind: 'audioinput',
    label,
    groupId: 'g',
    toJSON: () => ({}),
  } as MediaDeviceInfo;
}

// ---------------------------------------------------------------------------
// Starting audio
// ---------------------------------------------------------------------------

test('shows a start control and no mute control until media is running', async () => {
  const { screen } = await mount();
  await expect.element(screen.getByTestId('start-audio')).toBeInTheDocument();
  // The mute control is ABSENT, not disabled: a control that renders and does
  // nothing is the failure mode this view exists to avoid.
  await expect.element(screen.getByTestId('mute-toggle')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('media-status')).toHaveTextContent('audio not started');
});

test('starting audio reveals the mute control and reports waiting for return audio', async () => {
  const { screen } = await mount();
  await screen.getByTestId('start-audio').click();

  await expect.element(screen.getByTestId('mute-toggle')).toBeInTheDocument();
  // "waiting", not "no audio": before the first frame comes back these are
  // different claims, and only one of them is true yet.
  await expect
    .element(screen.getByTestId('media-status'))
    .toHaveTextContent('waiting for audio to return');
});

test('a start failure surfaces bounded text and leaves the start control available', async () => {
  const { session, screen } = await mount();
  session.failNextStartMedia = true;

  await screen.getByTestId('start-audio').click();

  await expect.element(screen.getByTestId('media-error')).toBeInTheDocument();
  // Recoverable: a denied permission that the user then grants must not require
  // a remount to retry.
  await expect.element(screen.getByTestId('start-audio')).toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Mute — the indicator's source of truth
// ---------------------------------------------------------------------------

test('the mute control drives the SDK and the indicator follows the SDK echo', async () => {
  const { session, screen } = await mount();
  await screen.getByTestId('start-audio').click();
  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('unmuted');

  await screen.getByTestId('mute-toggle').click();

  // Drives: the SDK was actually told.
  expect(session.media?.muteCalls).toEqual([true]);
  // Reflects: and the indicator moved because the SDK reported back.
  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('muted');
  await expect.element(screen.getByTestId('mute-toggle')).toHaveAttribute('aria-pressed', 'true');

  await screen.getByTestId('mute-toggle').click();
  expect(session.media?.muteCalls).toEqual([true, false]);
  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('unmuted');
});

test('the indicator follows an SDK-originated mute the UI never requested', async () => {
  // THE TEST THAT FAILS A CLICK-DRIVEN INDICATOR. Nothing was clicked here; the
  // SDK simply reported a state change, as it would if mute were set through the
  // SDK directly. A view holding its own boolean shows "unmuted" throughout.
  const { session, screen } = await mount();
  await screen.getByTestId('start-audio').click();

  session.fire('muteChanged', { audioMuted: true });

  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('muted');
  await expect.element(screen.getByTestId('mute-state')).toHaveAttribute('data-muted', 'true');
});

test('the indicator does NOT move when frames stop arriving', async () => {
  // THE TEST THAT FAILS AN ABSENCE-INFERRED INDICATOR, and the direction that
  // actually matters: inferring mute from silence means a transport stall
  // displays "muted" next to a live microphone. Here media has been flowing and
  // then stops (no further events at all) — the indicator must stay put.
  const { session, screen } = await mount();
  await screen.getByTestId('start-audio').click();
  session.fire('firstMediaFrame', 42);
  await expect
    .element(screen.getByTestId('media-status'))
    .toHaveTextContent('audio returning from the handler');

  // Silence. No mute event, no frames, nothing.
  await new Promise((resolve) => setTimeout(resolve, 50));

  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('unmuted');
});

// ---------------------------------------------------------------------------
// Slot state — rendered from the wire
// ---------------------------------------------------------------------------

test('before any assignment the slot reads as awaiting the controller', async () => {
  // Distinct from every wire state, including `unspecified`: "MC has not
  // answered yet" and "MC sent a state it did not classify" are different facts,
  // and one of them is a protocol surprise worth seeing.
  const { screen } = await mount();
  await expect
    .element(screen.getByTestId('slot-0'))
    .toHaveAttribute('data-slot-state', 'awaiting-assignment');
});

test('each wire slot state renders distinctly, including the far-end-muted case', async () => {
  const { session, screen } = await mount();
  session.fire('streamAssignments', {
    assignments: [
      slot({ slotId: 0, slotState: 'source_muted' }),
      slot({ slotId: 1, slotState: 'withheld_congestion' }),
      slot({ slotId: 2, slotState: 'fewer_sources' }),
      slot({ slotId: 3, slotState: 'source_unreachable' }),
    ],
  });

  // The raw wire token reaches the DOM, so tests assert on the protocol's
  // vocabulary rather than on display copy a copy edit can change.
  await expect
    .element(screen.getByTestId('slot-0'))
    .toHaveAttribute('data-slot-state', 'source_muted');
  await expect
    .element(screen.getByTestId('slot-1'))
    .toHaveAttribute('data-slot-state', 'withheld_congestion');
  await expect
    .element(screen.getByTestId('slot-2'))
    .toHaveAttribute('data-slot-state', 'fewer_sources');
  await expect
    .element(screen.getByTestId('slot-3'))
    .toHaveAttribute('data-slot-state', 'source_unreachable');

  // And they read DIFFERENTLY to a person — ADR-0036 §6's actual complaint is
  // the naive client that shows one spinner for all three.
  const texts = await Promise.all(
    [0, 1, 2, 3].map((id) => screen.getByTestId(`slot-${id}`).element().textContent),
  );
  expect(new Set(texts.map((t) => t?.trim())).size).toBe(4);
});

test('far-end muted is rendered from the wire, not from that slot being silent', async () => {
  const { session, screen } = await mount();
  session.fire('streamAssignments', { assignments: [slot({ slotState: 'active' })] });
  await expect.element(screen.getByTestId('slot-0')).toHaveAttribute('data-slot-active', 'true');

  session.fire('streamAssignments', { assignments: [slot({ slotState: 'source_muted' })] });

  await expect
    .element(screen.getByTestId('slot-0'))
    .toHaveAttribute('data-slot-state', 'source_muted');
  await expect.element(screen.getByTestId('slot-0')).toHaveAttribute('data-slot-active', 'false');
});

// ---------------------------------------------------------------------------
// Device selection
// ---------------------------------------------------------------------------

test('renders the microphone list, and unlabelled devices read as placeholders', async () => {
  // Labels are empty until permission is granted. That is the platform's
  // behaviour, so the picker renders the empty case honestly rather than
  // inventing a name that implies knowledge we do not have.
  const { screen } = await mount([device('mic-a', 'Studio Mic'), device('mic-b', '')]);

  await expect.element(screen.getByTestId('mic-select')).toBeInTheDocument();
  const select = screen.getByTestId('mic-select').element() as HTMLSelectElement;
  await vi.waitFor(() => expect(select.options.length).toBe(3));
  expect([...select.options].map((o) => o.textContent?.trim())).toEqual([
    'System default',
    'Studio Mic',
    'Microphone 2',
  ]);
});

test('choosing a device before start passes it to startMedia', async () => {
  const { session, screen } = await mount([device('mic-a', 'Studio Mic')]);
  const select = screen.getByTestId('mic-select').element() as HTMLSelectElement;
  await vi.waitFor(() => expect(select.options.length).toBe(2));

  await screen.getByTestId('mic-select').selectOptions('mic-a');
  await screen.getByTestId('start-audio').click();

  expect(session.startMediaDeviceIds).toEqual(['mic-a']);
});

test('changing the device while running performs a REAL swap on the SDK', async () => {
  // The test that fails a decorative picker: without the `setCaptureDevice`
  // call, the `<select>` still shows the new device and nothing else changes.
  const { session, screen } = await mount([device('mic-a', 'A'), device('mic-b', 'B')]);
  await screen.getByTestId('start-audio').click();
  const select = screen.getByTestId('mic-select').element() as HTMLSelectElement;
  await vi.waitFor(() => expect(select.options.length).toBe(3));

  await screen.getByTestId('mic-select').selectOptions('mic-b');

  await vi.waitFor(() => expect(session.media?.deviceCalls).toEqual(['mic-b']));
});

test('a failed device swap surfaces bounded text rather than failing silently', async () => {
  const { session, screen } = await mount([device('mic-a', 'A'), device('mic-b', 'B')]);
  await screen.getByTestId('start-audio').click();
  const select = screen.getByTestId('mic-select').element() as HTMLSelectElement;
  await vi.waitFor(() => expect(select.options.length).toBe(3));
  session.media!.failNextDeviceChange = true;

  await screen.getByTestId('mic-select').selectOptions('mic-b');

  await expect.element(screen.getByTestId('media-error')).toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Faults
// ---------------------------------------------------------------------------

test('a media fault renders as a fault rather than presenting as silence', async () => {
  const { session, screen } = await mount();
  await screen.getByTestId('start-audio').click();

  session.fire('mediaFault', {
    stage: 'decoder',
    message: 'the audio decoder failed; playback has stopped',
    fatal: true,
  });

  await expect
    .element(screen.getByTestId('media-fault'))
    .toHaveTextContent('the audio decoder failed');
});
