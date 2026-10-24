// R-43: meeting-join view with a MOCKED MeetingSession (real join needs
// WebTransport — out of scope for the component tier). Asserts the join controls
// + live roster from the sdk-svelte store + redacted last-error.

import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import {
  DEFAULT_PUBLISH,
  SdkError,
  SdkErrorCode,
  SignalingError,
  SignalingErrorCode,
} from '@darktower/sdk-core';
import type * as SdkCore from '@darktower/sdk-core';
import type { DemoConfig } from '../lib/config.js';
import type { AuthSession } from '../lib/types.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

const holder = vi.hoisted(() => ({
  /** Story 3 C-C2: the pre-join self-check's own preview, observed. */
  previewStops: 0,
  previewOpens: 0,
  session: undefined as MockMeetingSession | undefined,
  /** The options the view built the session with (publish set, levers, e2e). */
  builds: [] as Array<{ publish?: readonly string[] | undefined }>,
  /** Make the next build throw, as the SDK's construction-time refusal does. */
  throwOnBuild: undefined as Error | undefined,
}));

vi.mock('../lib/session.js', () => ({
  buildMeetingSession: (_config: unknown, opts: { publish?: readonly string[] } = {}) => {
    holder.builds.push(opts);
    const refusal = holder.throwOnBuild;
    holder.throwOnBuild = undefined;
    if (refusal) throw refusal;
    return holder.session;
  },
  buildAuthClient: () => ({}),
  buildMeetingClient: () => ({}),
  // Signature is now `(config, authTokenProvider)` since telemetry gained a
  // per-export token getter; this suite does not exercise export, so the stub
  // ignores its arguments entirely.
  configureTelemetryIfEnabled: () => {},
}));

// Story 3 C-C2: the pre-join self-check's own capture, observed through the
// SDK's ONE opener (a partial mock: everything else is the real SDK).
vi.mock('@darktower/sdk-core', async (importOriginal) => {
  const real = await importOriginal<typeof SdkCore>();
  return {
    ...real,
    openLocalPreview: async () => {
      holder.previewOpens += 1;
      return {
        stream: new MediaStream(),
        video: undefined,
        audio: undefined,
        stop: () => {
          holder.previewStops += 1;
        },
      };
    },
  };
});

// Imported after the mock is registered (vi.mock is hoisted regardless).
import JoinMeeting from '../views/JoinMeeting.svelte';

const config: DemoConfig = {
  acOriginTemplate: 'http://{subdomain}.localhost:5173',
  gcBaseUrl: '',
  env: 'test',
  devCertHashes: [],
  audioReceiveSlots: { count: 1, source: 'default' },
  videoReceiveSlots: { count: 0, source: 'default' },
};

const auth: AuthSession = {
  subdomain: 'demo',
  userToken: 'user-token',
};

/** Render the view and submit a join (the session is built on SUBMIT, story 3 C-C1). */
async function submitted(session: MockMeetingSession) {
  holder.session = session;
  holder.builds = [];
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();
  return screen;
}

test('renders join controls and a live roster from the store', async () => {
  const session = new MockMeetingSession();
  holder.session = session;
  const pre = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  await expect.element(pre.getByTestId('meeting-code')).toBeInTheDocument();
  await expect.element(pre.getByTestId('join-button')).toBeInTheDocument();
  await expect.element(pre.getByTestId('meeting-state')).toHaveTextContent('idle');
  pre.unmount();

  const screen = await submitted(session);
  await vi.waitFor(() => expect(session.joinCalls).toBe(1));
  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('participant-a')).toHaveTextContent('Ann');

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  session.fire('participantLeft', { participantId: 'a', reason: 'voluntary' });
  await expect.element(screen.getByTestId('participant-a')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('participant-b')).toHaveTextContent('Bob');
});

test('surfaces an error via last-error without leaking a non-allowlisted field', async () => {
  const session = new MockMeetingSession();
  const screen = await submitted(session);
  await vi.waitFor(() => expect(session.joinCalls).toBe(1));

  const err = new SdkError(SdkErrorCode.Signaling, 'join rejected', { status: 401 });
  (err as { token?: string }).token = 'SECRET_TOKEN_123';
  session.fire('error', err);

  await expect.element(screen.getByTestId('last-error')).toMatchTextContent('SIGNALING');
  await expect.element(screen.getByTestId('last-error')).not.toMatchTextContent('SECRET_TOKEN_123');
});

// ---------------------------------------------------------------------------
// Story 3 R-2: the publish-set chooser
// ---------------------------------------------------------------------------

test("the chooser starts from the SDK default publish set, and its result is the join's publish set", async () => {
  const session = new MockMeetingSession();
  holder.session = session;
  holder.builds = [];
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  // From DEFAULT_PUBLISH, never a restated literal.
  const mic = screen.getByTestId('publish-microphone');
  const cam = screen.getByTestId('publish-camera');
  if (DEFAULT_PUBLISH.includes('audio')) await expect.element(mic).toBeChecked();
  else await expect.element(mic).not.toBeChecked();
  if (DEFAULT_PUBLISH.includes('video')) await expect.element(cam).toBeChecked();
  else await expect.element(cam).not.toBeChecked();
  // Nothing is built before submit: the publish set is fixed at construction.
  expect(holder.builds).toEqual([]);

  await cam.click();
  await mic.click();
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();
  await vi.waitFor(() => expect(holder.builds).toHaveLength(1));
  const expected = [
    ...(DEFAULT_PUBLISH.includes('audio') ? [] : ['audio']),
    ...(DEFAULT_PUBLISH.includes('video') ? [] : ['video']),
  ];
  expect(holder.builds[0]?.publish).toEqual(expected);
});

test('a choice that would neither publish nor receive shows the SDK refusal — never a silent empty meeting', async () => {
  const session = new MockMeetingSession();
  holder.throwOnBuild = new SignalingError(
    SignalingErrorCode.NothingToPublishOrReceive,
    'this participant would neither publish nor receive',
  );
  const screen = await submitted(session);
  const refusal = screen.getByTestId('last-error');
  await expect.element(refusal).toHaveTextContent('neither publish nor receive');
  await expect.element(refusal).toHaveTextContent('Choose the microphone or the camera');
  // Nothing joined.
  expect(session.joinCalls).toBe(0);
  await expect.element(screen.getByTestId('in-meeting')).not.toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Story 2 task 15: roster rows carry reachability, server mute and client mute
// as WIRE TOKENS; the host affordance is target + action, labelled from the wire
// ---------------------------------------------------------------------------

async function joinedRig(opts: { isHost?: boolean } = {}) {
  const session = new MockMeetingSession();
  session.isHost = opts.isHost ?? false;
  holder.session = session;
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();
  await vi.waitFor(() => expect(session.joinCalls).toBe(1));
  session.fire('joined', {
    participantId: 'self',
    senderId: 1,
    existingParticipants: [
      { participantId: 'b', name: 'Bea', senderId: 258 },
      { participantId: 'c', name: 'Cid', senderId: 259 },
      { participantId: 'h', name: 'Hal', senderId: 260 },
    ],
    mediaServers: [],
    correlationId: 'x',
    bindingToken: 'y',
  });
  session.fire('stateChange', 'joined');
  return { session, screen };
}

const assignment = (
  senderId: number,
  slotState: 'active' | 'avatar_client_muted' | 'avatar_server_muted',
  slotId: number,
) => ({
  slotId,
  mediaKind: 'audio' as const,
  senderId,
  mediaHandlerUrl: 'https://mh-0:4433',
  slotState,
});

test('an unreachable participant is ON the roster, marked with the wire token — not muted, not a slot', async () => {
  const { session, screen } = await joinedRig();
  session.fire('streamAssignments', {
    assignments: [assignment(258, 'active', 0)],
    unreachableSenderIds: [259],
  });
  const c = screen.getByTestId('participant-c');
  await expect.element(c).toHaveAttribute('data-reachability', 'source_unreachable');
  await expect.element(c).toHaveAttribute('data-server-muted', 'false');
  await expect.element(c).toHaveAttribute('data-client-muted', 'unknown');
  await expect
    .element(screen.getByTestId('participant-b'))
    .toHaveAttribute('data-reachability', 'reachable');

  // Replace, never merge: reachable again means the mark goes.
  session.fire('streamAssignments', {
    assignments: [assignment(258, 'active', 0), assignment(259, 'active', 1)],
    unreachableSenderIds: [],
  });
  await expect.element(c).toHaveAttribute('data-reachability', 'reachable');
});

test('server mute and client mute are DISTINCT tokens; client mute is unknown with no slot evidence', async () => {
  const { session, screen } = await joinedRig();
  session.fire('streamAssignments', {
    assignments: [
      assignment(258, 'avatar_client_muted', 0),
      assignment(259, 'avatar_server_muted', 1),
    ],
    unreachableSenderIds: [],
  });
  session.fire('participantMuteChanged', {
    participantId: 'c',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'h',
  });
  const b = screen.getByTestId('participant-b');
  const c = screen.getByTestId('participant-c');
  const h = screen.getByTestId('participant-h');
  // b: slot avatar_client_muted, no server mute -> client-muted.
  await expect.element(b).toHaveAttribute('data-client-muted', 'true');
  await expect.element(b).toHaveAttribute('data-server-muted', 'false');
  // c: server-muted -> the slot's avatar state says nothing about their own mic.
  await expect.element(c).toHaveAttribute('data-server-muted', 'true');
  await expect.element(c).toHaveAttribute('data-client-muted', 'unknown');
  await expect.element(screen.getByTestId('participant-server-muted-c')).toMatchTextContent('Hal');
  // h: in no slot of ours -> no wire fact either way.
  await expect.element(h).toHaveAttribute('data-client-muted', 'unknown');
});

test('host controls are absent for a non-host', async () => {
  const { screen } = await joinedRig({ isHost: false });
  await expect.element(screen.getByTestId('participant-b')).toBeInTheDocument();
  await expect.element(screen.getByTestId('server-mute-audio-b')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('server-mute-video-b')).not.toBeInTheDocument();
});

test('the host affordance sends target + action, and its label follows the WIRE, not the click', async () => {
  const { session, screen } = await joinedRig({ isHost: true });
  const button = screen.getByTestId('server-mute-audio-b');
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');
  await button.click();
  await vi.waitFor(() =>
    expect(session.serverMuteCalls).toEqual([{ participantId: 'b', kind: 'audio', muted: true }]),
  );
  // No wire update yet: still unpressed.
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');

  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: true,
    videoServerMuted: false,
    serverMutedBy: 'self',
  });
  await expect.element(button).toHaveAttribute('aria-pressed', 'true');
  await expect.element(screen.getByTestId('participant-server-muted-b')).toMatchTextContent('you');

  // A relayed unmute request is shown to the host; it lifts nothing.
  session.fire('unmuteRequested', { participantId: 'b' });
  await expect.element(screen.getByTestId('unmute-requested-b')).toBeInTheDocument();
  await expect
    .element(screen.getByTestId('participant-b'))
    .toHaveAttribute('data-server-muted', 'true');

  await button.click();
  await vi.waitFor(() =>
    expect(session.serverMuteCalls[1]).toEqual({ participantId: 'b', kind: 'audio', muted: false }),
  );
  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: false,
  });
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');
  await expect.element(screen.getByTestId('unmute-requested-b')).not.toBeInTheDocument();
  expect(screen.container.textContent?.toLowerCase()).not.toMatch(/\bban/);
});

test('the host has a SEPARATE affordance per kind; a video request never touches audio', async () => {
  const { session, screen } = await joinedRig({ isHost: true });
  const video = screen.getByTestId('server-mute-video-b');
  const audio = screen.getByTestId('server-mute-audio-b');
  await video.click();
  await vi.waitFor(() =>
    expect(session.serverMuteCalls).toEqual([{ participantId: 'b', kind: 'video', muted: true }]),
  );
  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: false,
    videoServerMuted: true,
    serverMutedBy: 'self',
  });
  await expect.element(video).toHaveAttribute('aria-pressed', 'true');
  await expect.element(audio).toHaveAttribute('aria-pressed', 'false');
  const row = screen.getByTestId('participant-b');
  await expect.element(row).toHaveAttribute('data-server-muted-video', 'true');
  await expect.element(row).toHaveAttribute('data-server-muted', 'false');
  await expect
    .element(row.getByTestId('mute-video-server-b'))
    .toHaveAttribute('data-muted', 'true');
});

// ---------------------------------------------------------------------------
// Story 3 R-7: the pre-join debug panel (one line in the idle branch)
// ---------------------------------------------------------------------------

test('the pre-join debug toggle is present, hidden by default, and the panel is GONE once Join is submitted', async () => {
  localStorage.setItem('darktower.debugPanel.v1', '1');
  const session = new MockMeetingSession();
  holder.session = session;
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  // Persisted open: the panel shows, but the self-check is NOT started (no gesture).
  await expect.element(screen.getByTestId('debug-panel')).toBeInTheDocument();
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('idle');
  // No statistics overlay before join (there is no session).
  await expect.element(screen.getByTestId('debug-stats-empty')).not.toBeInTheDocument();
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();
  await vi.waitFor(() => expect(session.joinCalls).toBe(1));
  // Unmounted with the idle branch: an own preview capture would be released here.
  await expect.element(screen.getByTestId('debug-panel-root')).not.toBeInTheDocument();
  localStorage.removeItem('darktower.debugPanel.v1');
  screen.unmount();
});

test('the pre-join panel is hidden by default with empty storage', async () => {
  localStorage.removeItem('darktower.debugPanel.v1');
  holder.session = new MockMeetingSession();
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  await expect.element(screen.getByTestId('debug-panel-toggle')).toBeInTheDocument();
  await expect.element(screen.getByTestId('debug-panel')).not.toBeInTheDocument();
  screen.unmount();
});

test('a RUNNING pre-join self-check is released BEFORE the session joins (single-webcam hosts)', async () => {
  localStorage.setItem('darktower.debugPanel.v1', '1');
  holder.previewOpens = 0;
  holder.previewStops = 0;
  const session = new MockMeetingSession();
  let stopsWhenJoinRan: number | undefined;
  session.onJoin = () => {
    stopsWhenJoinRan = holder.previewStops;
  };
  holder.session = session;
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });
  await screen.getByTestId('self-check-start').click();
  await expect.element(screen.getByTestId('self-check-status')).toHaveTextContent('running');
  expect(holder.previewOpens).toBe(1);
  expect(holder.previewStops).toBe(0);
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();
  await vi.waitFor(() => expect(session.joinCalls).toBe(1));
  // The preview was already stopped when join() ran — before any session capture.
  expect(stopsWhenJoinRan).toBe(1);
  localStorage.removeItem('darktower.debugPanel.v1');
  screen.unmount();
});
