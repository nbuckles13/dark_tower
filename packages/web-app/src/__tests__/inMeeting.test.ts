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
    mediaKind: 'audio',
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
async function mount(
  microphones: MediaDeviceInfo[] = [],
  opts: { audio?: number; video?: number; publish?: readonly ('audio' | 'video')[] } = {},
): Promise<Rig> {
  stubEnumerateDevices(async () => microphones);
  const session = new MockMeetingSession();
  session.state = MeetingSessionState.Joined;
  session.receiveSlots = {
    publish: opts.publish ?? ['audio'],
    audio: {
      declared: opts.audio ?? 1,
      slotIds: Array.from({ length: opts.audio ?? 1 }, (_, i) => i),
      source: 'default',
      serverCap: { state: 'unknown' },
    },
    video: {
      declared: opts.video ?? 0,
      slotIds: Array.from({ length: opts.video ?? 0 }, (_, i) => (opts.audio ?? 1) + i),
      source: 'default',
      serverCap: { state: 'unknown' },
    },
  };
  const store = new MeetingStore();
  subscribeSession(store, session);
  const screen = await render(InMeeting, {
    session: session as unknown as MeetingSession,
    store,
    selfParticipantId: 'self',
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
  await expect.element(screen.getByTestId('start-media')).toBeInTheDocument();
  // The mute control is ABSENT, not disabled: a control that renders and does
  // nothing is the failure mode this view exists to avoid.
  await expect.element(screen.getByTestId('mute-toggle')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('media-status')).toHaveTextContent('media not started');
});

test('starting audio reveals the mute control and reports waiting for return audio', async () => {
  const { screen } = await mount();
  await screen.getByTestId('start-media').click();

  await expect.element(screen.getByTestId('mute-toggle')).toBeInTheDocument();
  // "waiting", not "no audio": before the first frame comes back these are
  // different claims, and only one of them is true yet.
  await expect
    .element(screen.getByTestId('media-status'))
    .toHaveTextContent('no media from other participants yet');
});

test('a start failure surfaces bounded text and leaves the start control available', async () => {
  const { session, screen } = await mount();
  session.failNextStartMedia = true;

  await screen.getByTestId('start-media').click();

  await expect.element(screen.getByTestId('media-error')).toBeInTheDocument();
  // Recoverable: a denied permission that the user then grants must not require
  // a remount to retry.
  await expect.element(screen.getByTestId('start-media')).toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Mute — the indicator's source of truth
// ---------------------------------------------------------------------------

test('the mute control drives the SDK and the indicator follows the SDK echo', async () => {
  const { session, screen } = await mount();
  await screen.getByTestId('start-media').click();
  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('unmuted');

  await screen.getByTestId('mute-toggle').click();

  // Drives: the SDK was actually told (through the session's per-kind API).
  expect(session.sessionMuteCalls).toEqual([{ kind: 'audio', muted: true }]);
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
  await screen.getByTestId('start-media').click();

  session.fire('muteChanged', { audioMuted: true, videoMuted: false });

  await expect.element(screen.getByTestId('mute-state')).toHaveTextContent('muted');
  await expect.element(screen.getByTestId('mute-state')).toHaveAttribute('data-muted', 'true');
});

test('the indicator does NOT move when frames stop arriving', async () => {
  // THE TEST THAT FAILS AN ABSENCE-INFERRED INDICATOR, and the direction that
  // actually matters: inferring mute from silence means a transport stall
  // displays "muted" next to a live microphone. Here media has been flowing and
  // then stops (no further events at all) — the indicator must stay put.
  const { session, screen } = await mount();
  await screen.getByTestId('start-media').click();
  session.fire('firstMediaFrame', 42);
  await expect
    .element(screen.getByTestId('media-status'))
    .toHaveTextContent('receiving media from other participants');

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
  const { session, screen } = await mount([], { audio: 4 });
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, slotState: 'avatar_client_muted' }),
      slot({ slotId: 1, slotState: 'withheld_by_congestion' }),
      slot({ slotId: 2, slotState: 'fewer_sources_than_slots' }),
      slot({ slotId: 3, slotState: 'source_unreachable' }),
    ],
  });

  // The raw wire token reaches the DOM, so tests assert on the protocol's
  // vocabulary rather than on display copy a copy edit can change.
  await expect
    .element(screen.getByTestId('slot-0'))
    .toHaveAttribute('data-slot-state', 'avatar_client_muted');
  await expect
    .element(screen.getByTestId('slot-1'))
    .toHaveAttribute('data-slot-state', 'withheld_by_congestion');
  await expect
    .element(screen.getByTestId('slot-2'))
    .toHaveAttribute('data-slot-state', 'fewer_sources_than_slots');
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
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [slot({ slotState: 'active' })],
  });
  await expect.element(screen.getByTestId('slot-0')).toHaveAttribute('data-slot-active', 'true');

  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [slot({ slotState: 'avatar_client_muted' })],
  });

  await expect
    .element(screen.getByTestId('slot-0'))
    .toHaveAttribute('data-slot-state', 'avatar_client_muted');
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
  await screen.getByTestId('start-media').click();

  expect(session.startMediaDeviceIds).toEqual(['mic-a']);
});

test('changing the device while running performs a REAL swap on the SDK', async () => {
  // The test that fails a decorative picker: without the `setCaptureDevice`
  // call, the `<select>` still shows the new device and nothing else changes.
  const { session, screen } = await mount([device('mic-a', 'A'), device('mic-b', 'B')]);
  await screen.getByTestId('start-media').click();
  const select = screen.getByTestId('mic-select').element() as HTMLSelectElement;
  await vi.waitFor(() => expect(select.options.length).toBe(3));

  await screen.getByTestId('mic-select').selectOptions('mic-b');

  await vi.waitFor(() => expect(session.media?.deviceCalls).toEqual(['mic-b']));
});

test('a failed device swap surfaces bounded text rather than failing silently', async () => {
  const { session, screen } = await mount([device('mic-a', 'A'), device('mic-b', 'B')]);
  await screen.getByTestId('start-media').click();
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
  await screen.getByTestId('start-media').click();

  session.fire('mediaFault', {
    stage: 'decoder',
    message: 'the audio decoder failed; playback has stopped',
    fatal: true,
  });

  await expect
    .element(screen.getByTestId('media-fault'))
    .toMatchTextContent('the audio decoder failed');
});

// ---------------------------------------------------------------------------
// Story 2 task 15: the slot grid's identities, and this participant's server mute
// ---------------------------------------------------------------------------

test('each filled slot names the roster participant MC placed there; an unfilled one is an empty cell', async () => {
  const { session, screen } = await mount([], { audio: 2 });
  session.fire('joined', {
    participantId: 'self',
    senderId: 1,
    existingParticipants: [{ participantId: 'b', name: 'Bea', senderId: 258 }],
    mediaServers: [],
    correlationId: 'c',
    bindingToken: 't',
  });
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 258, slotState: 'active' }),
      slot({ slotId: 1, senderId: undefined, slotState: 'fewer_sources_than_slots' }),
    ],
  });
  await expect.element(screen.getByTestId('slot-0')).toHaveAttribute('data-sender-id', '258');
  await expect.element(screen.getByTestId('slot-0')).toMatchTextContent('Bea');
  await expect.element(screen.getByTestId('slot-1')).not.toHaveAttribute('data-sender-id');
  await expect
    .element(screen.getByTestId('slot-1'))
    .toHaveAttribute('data-slot-state', 'fewer_sources_than_slots');
});

test('a slot whose sender the roster does not know reads as unknown, never the raw id', async () => {
  const { session, screen } = await mount();
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [slot({ slotId: 0, senderId: 999, slotState: 'active' })],
  });
  await expect.element(screen.getByTestId('slot-0')).toMatchTextContent('unknown participant');
  expect(screen.getByTestId('slot-0').element().textContent).not.toContain('999');
});

test('own SERVER mute is a separate indicator, driven only by the wire; asking to unmute changes nothing', async () => {
  const { session, screen } = await mount();
  await screen.getByTestId('start-media').click();
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'false');
  await expect.element(screen.getByTestId('request-unmute')).not.toBeInTheDocument();

  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'nobody-we-know',
  });
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'true');
  // The host's id is not on our roster: bounded placeholder, never the raw string.
  expect(screen.getByTestId('server-mute-state').element().textContent).not.toContain(
    'nobody-we-know',
  );
  // Client mute is untouched by a server mute: two indicators, composing.
  await expect.element(screen.getByTestId('mute-state')).toHaveAttribute('data-muted', 'false');

  await screen.getByTestId('request-unmute').click();
  await vi.waitFor(() => expect(session.unmuteRequestCalls).toBe(1));
  // R-10: the ask lifts nothing — the indicator waits for MC.
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'true');

  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'false');
});

test('nothing in the view calls a server mute a ban', async () => {
  const { session, screen } = await mount();
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: true,
    videoServerMuted: false,
  });
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'true');
  expect(screen.container.textContent?.toLowerCase()).not.toMatch(/\bban/);
});

test('the "asked" label belongs to ONE server-mute episode: a re-mute starts with nothing asked', async () => {
  const { session, screen } = await mount();
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: true,
    videoServerMuted: false,
  });
  await screen.getByTestId('request-unmute').click();
  await vi.waitFor(() => expect(session.unmuteRequestCalls).toBe(1));
  await expect.element(screen.getByTestId('request-unmute')).toMatchTextContent('Asked the host');

  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  await expect.element(screen.getByTestId('request-unmute')).not.toBeInTheDocument();
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: true,
    videoServerMuted: false,
  });
  await expect
    .element(screen.getByTestId('request-unmute'))
    .toHaveTextContent('Ask the host to unmute you');
});

// ---------------------------------------------------------------------------
// Story 3 C-C1: the paired video grid, four indicators, R-18, R-19, R-28
// ---------------------------------------------------------------------------

const ROSTER_JOIN = {
  participantId: 'self',
  senderId: 1,
  existingParticipants: [
    { participantId: 'b', name: 'Bea', senderId: 258 },
    { participantId: 'c', name: 'Cid', senderId: 259 },
  ],
  mediaServers: [],
  correlationId: 'c',
  bindingToken: 't',
};

test('the video grid has one tile per declared video slot, raw wire token on data-slot-state', async () => {
  const { session, screen } = await mount([], { audio: 2, video: 2, publish: ['audio', 'video'] });
  session.fire('joined', ROSTER_JOIN);
  // Slot ids: audio 0..1, video 2..3. Nothing assigned yet.
  await expect
    .element(screen.getByTestId('video-slot-2'))
    .toHaveAttribute('data-slot-state', 'awaiting-assignment');
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 2, mediaKind: 'video', senderId: 258, slotState: 'avatar_client_muted' }),
      slot({ slotId: 3, mediaKind: 'video', senderId: undefined, slotState: 'unavailable' }),
    ],
  });
  session.fire('videoSlotRenderChanged', {
    slotId: 2,
    state: { kind: 'avatar', participantSenderId: 258, reason: 'avatar_client_muted' },
  });
  session.fire('videoSlotRenderChanged', { slotId: 3, state: { kind: 'unavailable' } });
  const two = screen.getByTestId('video-slot-2');
  await expect.element(two).toHaveAttribute('data-slot-state', 'avatar_client_muted');
  await expect.element(two).toHaveAttribute('data-rendered', 'avatar');
  await expect.element(two).toHaveAttribute('data-participant-id', 'b');
  await expect.element(two).toHaveTextContent('Bea turned their camera off');
  const three = screen.getByTestId('video-slot-3');
  await expect.element(three).toHaveAttribute('data-rendered', 'empty');
  await expect.element(three).toHaveTextContent('this meeting allows fewer video slots');
});

test('audio and video PAIR by sender id, never by slot index (R-3)', async () => {
  const { session, screen } = await mount([], { audio: 2, video: 2, publish: ['audio'] });
  session.fire('joined', ROSTER_JOIN);
  // Bea holds AUDIO slot 1 and the LIVE video slot 2 (MC names no live source;
  // the verified render tag does). Index-pairing would pair audio 0 with video 2.
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 259, slotState: 'active' }),
      slot({ slotId: 1, senderId: 258, slotState: 'avatar_client_muted' }),
      slot({ slotId: 2, mediaKind: 'video', senderId: undefined, slotState: 'active' }),
      slot({ slotId: 3, mediaKind: 'video', senderId: 259, slotState: 'avatar_no_video' }),
    ],
  });
  session.fire('videoSlotRenderChanged', { slotId: 2, state: { kind: 'active', senderId: 258 } });
  await expect
    .element(screen.getByTestId('video-slot-2'))
    .toHaveAttribute('data-paired-audio-slot', '1');
  await expect
    .element(screen.getByTestId('video-slot-2'))
    .toHaveAttribute('data-participant-id', 'b');
  await expect
    .element(screen.getByTestId('video-slot-2'))
    .toHaveAttribute('data-rendered', 'video');
  // Bea's tile shows her AUDIO state inline: one participant, not two.
  await expect
    .element(screen.getByTestId('video-slot-audio-2'))
    .toHaveTextContent('Bea muted their microphone');
  await expect.element(screen.getByTestId('slot-1')).toHaveAttribute('data-paired-video-slot', '2');
  await expect.element(screen.getByTestId('slot-0')).toHaveAttribute('data-paired-video-slot', '3');
});

test('an ACTIVE slot whose first frame has not decoded keeps its previous rendering — no spinner', async () => {
  const { session, screen } = await mount([], { audio: 0, video: 1, publish: ['video'] });
  session.fire('joined', ROSTER_JOIN);
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, mediaKind: 'video', senderId: 258, slotState: 'avatar_client_muted' }),
    ],
  });
  session.fire('videoSlotRenderChanged', {
    slotId: 0,
    state: { kind: 'avatar', participantSenderId: 258, reason: 'avatar_client_muted' },
  });
  await expect
    .element(screen.getByTestId('video-slot-0'))
    .toHaveAttribute('data-rendered', 'avatar');
  // MC turns it live; the SDK keeps the avatar until the first frame decodes (R-5).
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, mediaKind: 'video', senderId: undefined, slotState: 'active' }),
    ],
  });
  const tile = screen.getByTestId('video-slot-0');
  await expect.element(tile).toHaveAttribute('data-slot-state', 'active');
  await expect.element(tile).toHaveAttribute('data-rendered', 'avatar');
  expect(tile.element().textContent?.toLowerCase()).not.toMatch(/loading|connecting|spinner/);
});

test('the VideoTile binds the slot sink once media has started, and the canvas shows only while live', async () => {
  const { session, screen } = await mount([], { audio: 0, video: 1, publish: ['video'] });
  session.fire('joined', ROSTER_JOIN);
  await screen.getByTestId('start-media').click();
  await vi.waitFor(() => expect(session.sinks.get(0)).toBeDefined());
  const canvas = screen.getByTestId('video-canvas-0');
  await expect.element(canvas).not.toBeVisible();
  session.fire('videoSlotRenderChanged', { slotId: 0, state: { kind: 'active', senderId: 258 } });
  await expect.element(canvas).toBeVisible();
});

test('four indicators per participant, each its own token: client/server x audio/video', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 1, publish: ['audio'] });
  session.fire('joined', ROSTER_JOIN);
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 258, slotState: 'avatar_client_muted' }),
      slot({ slotId: 1, mediaKind: 'video', senderId: 258, slotState: 'avatar_server_muted' }),
    ],
  });
  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: true,
    serverMutedBy: 'self',
  });
  const tile = screen.getByTestId('video-slot-1');
  await expect
    .element(tile.getByTestId('mute-video-server-b'))
    .toHaveAttribute('data-muted', 'true');
  await expect
    .element(tile.getByTestId('mute-audio-server-b'))
    .toHaveAttribute('data-muted', 'false');
  await expect
    .element(tile.getByTestId('mute-audio-client-b'))
    .toHaveAttribute('data-muted', 'true');
  // Server-muted video says nothing about their own camera.
  await expect
    .element(tile.getByTestId('mute-video-client-b'))
    .toHaveAttribute('data-muted', 'unknown');
  const texts = ['audio-client', 'video-client', 'audio-server', 'video-server'].map((k) =>
    tile.getByTestId(`mute-${k}-b`).element().textContent?.trim(),
  );
  expect(new Set(texts).size).toBe(4);
});

test('names are rendered as TEXT, never markup, and bounded (security: XSS payload)', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 1, publish: ['audio'] });
  const payload =
    '<img src=x onerror="window.__xss=1">' + 'a'.repeat(80) + '<script>window.__xss=2</script>';
  session.fire('joined', {
    ...ROSTER_JOIN,
    existingParticipants: [{ participantId: 'b', name: payload, senderId: 258 }],
  });
  session.fire('streamAssignments', {
    unreachableSenderIds: [],
    assignments: [
      slot({ slotId: 0, senderId: 258, slotState: 'active' }),
      slot({ slotId: 1, mediaKind: 'video', senderId: 258, slotState: 'avatar_no_video' }),
    ],
  });
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'b',
  });
  await expect.element(screen.getByTestId('video-slot-1')).toHaveTextContent('<img');
  expect(screen.container.querySelector('img')).toBeNull();
  expect(screen.container.querySelector('script')).toBeNull();
  expect((window as { __xss?: number }).__xss).toBeUndefined();
  const shown = screen.getByTestId('server-mute-state').element().textContent ?? '';
  expect(shown).toContain('<img');
  // Bounded: the payload is longer than the display-name limit.
  expect(shown).not.toContain('__xss=2');
});

test('client VIDEO mute drives the SDK and the indicator follows the echo', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 0, publish: ['audio', 'video'] });
  await screen.getByTestId('start-media').click();
  await expect
    .element(screen.getByTestId('video-mute-state'))
    .toHaveAttribute('data-muted', 'false');
  await screen.getByTestId('video-mute-toggle').click();
  expect(session.sessionMuteCalls).toEqual([{ kind: 'video', muted: true }]);
  await expect
    .element(screen.getByTestId('video-mute-state'))
    .toHaveAttribute('data-muted', 'true');
  // Audio untouched.
  await expect.element(screen.getByTestId('mute-state')).toHaveAttribute('data-muted', 'false');
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveAttribute('data-camera-state', 'muted');
});

test('R-19: a failed camera is shown to its owner WITH the reason', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 0, publish: ['audio', 'video'] });
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveAttribute('data-camera-state', 'on');
  session.fire('cameraHealthChanged', { failure: 'permission_denied' });
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveAttribute('data-camera-state', 'failed');
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveTextContent('camera permission was denied');
  session.fire('cameraHealthChanged', { failure: undefined });
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveAttribute('data-camera-state', 'on');
});

test('own SERVER video mute has its own indicator; the camera reads host_off', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 0, publish: ['audio', 'video'] });
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: false,
    videoServerMuted: true,
  });
  await expect
    .element(screen.getByTestId('server-mute-state-video'))
    .toHaveAttribute('data-server-muted', 'true');
  await expect
    .element(screen.getByTestId('server-mute-state'))
    .toHaveAttribute('data-server-muted', 'false');
  await expect
    .element(screen.getByTestId('self-camera-state'))
    .toHaveAttribute('data-camera-state', 'host_off');
  await expect.element(screen.getByTestId('request-unmute')).toBeInTheDocument();
});

test('R-28: a media-start fault (VP8 unusable) surfaces in the UI, and no media controls appear', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 1, publish: ['audio', 'video'] });
  session.failNextStartMedia = true;
  await screen.getByTestId('start-media').click();
  session.fire('mediaFault', {
    stage: 'encoder',
    message: 'this browser cannot encode and decode VP8 video, so media cannot start',
    fatal: true,
  });
  await expect
    .element(screen.getByTestId('media-fault'))
    .toHaveTextContent('cannot encode and decode VP8');
  await expect.element(screen.getByTestId('media-error')).toBeInTheDocument();
  await expect.element(screen.getByTestId('mute-toggle')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('video-mute-toggle')).not.toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// R-18: the non-suppressible "the host turned it back on" notice
// ---------------------------------------------------------------------------

test('R-18: the notice appears ONLY from the SDK event, per kind, as an alertdialog', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 0, publish: ['audio', 'video'] });
  await expect.element(screen.getByTestId('resume-notice-video')).not.toBeInTheDocument();
  // A server-unmute update alone does not raise it (C-A1's SDK event is the SSoT).
  session.fire('participantMuteChanged', {
    participantId: 'self',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  await expect.element(screen.getByTestId('resume-notice-video')).not.toBeInTheDocument();
  session.fire('captureResumedAfterServerMute', { kind: 'video' });
  session.fire('captureResumedAfterServerMute', { kind: 'audio' });
  const video = screen.getByTestId('resume-notice-video');
  await expect.element(video).toBeVisible();
  await expect.element(video).toHaveAttribute('role', 'alertdialog');
  await expect.element(video).toHaveAttribute('aria-live', 'assertive');
  await expect.element(video).toHaveTextContent('The host turned your camera back on');
  // Two kinds, two notices.
  await expect.element(screen.getByTestId('resume-notice-audio')).toBeVisible();
});

test('R-18: nothing hides the notice but an acknowledgement — not Escape, a click outside, or a re-mount', async () => {
  const { session, store, screen } = await mount([], {
    audio: 1,
    video: 0,
    publish: ['audio', 'video'],
  });
  session.fire('captureResumedAfterServerMute', { kind: 'video' });
  await expect.element(screen.getByTestId('resume-notice-video')).toBeVisible();
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await screen.getByTestId('media-status').click();
  // A second resume of the same kind keeps ONE pending notice.
  session.fire('captureResumedAfterServerMute', { kind: 'video' });
  await expect.element(screen.getByTestId('resume-notice-video')).toBeVisible();
  // A full re-mount of the view (a grid rebuild) keeps it: the latch is the store's.
  screen.unmount();
  const again = await render(InMeeting, {
    session: session as unknown as MeetingSession,
    store,
    selfParticipantId: 'self',
  });
  await expect.element(again.getByTestId('resume-notice-video')).toBeVisible();
  await again.getByTestId('resume-notice-ack-video').click();
  await expect.element(again.getByTestId('resume-notice-video')).not.toBeInTheDocument();
  // Acknowledging alone mutes nothing.
  expect(session.sessionMuteCalls).toEqual([]);
});

test('R-18: the inline action client-mutes THAT kind at once, then clears only that notice', async () => {
  const { session, screen } = await mount([], { audio: 1, video: 0, publish: ['audio', 'video'] });
  await screen.getByTestId('start-media').click();
  session.fire('captureResumedAfterServerMute', { kind: 'video' });
  session.fire('captureResumedAfterServerMute', { kind: 'audio' });
  await screen.getByTestId('resume-notice-mute-video').click();
  expect(session.sessionMuteCalls).toEqual([{ kind: 'video', muted: true }]);
  await expect.element(screen.getByTestId('resume-notice-video')).not.toBeInTheDocument();
  await expect
    .element(screen.getByTestId('video-mute-state'))
    .toHaveAttribute('data-muted', 'true');
  await expect.element(screen.getByTestId('resume-notice-audio')).toBeVisible();
  await screen.getByTestId('resume-notice-mute-audio').click();
  expect(session.sessionMuteCalls.at(-1)).toEqual({ kind: 'audio', muted: true });
});
