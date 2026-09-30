// R-43: meeting-join view with a MOCKED MeetingSession (real join needs
// WebTransport — out of scope for the component tier). Asserts the join controls
// + live roster from the sdk-svelte store + redacted last-error.

import { expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { SdkError, SdkErrorCode } from '@darktower/sdk-core';
import type { DemoConfig } from '../lib/config.js';
import type { AuthSession } from '../lib/types.js';
import { MockMeetingSession } from './helpers/MockMeetingSession.js';

const holder = vi.hoisted(() => ({ session: undefined as MockMeetingSession | undefined }));

vi.mock('../lib/session.js', () => ({
  buildMeetingSession: () => holder.session,
  buildAuthClient: () => ({}),
  buildMeetingClient: () => ({}),
  // Signature is now `(config, authTokenProvider)` since telemetry gained a
  // per-export token getter; this suite does not exercise export, so the stub
  // ignores its arguments entirely.
  configureTelemetryIfEnabled: () => {},
}));

// Imported after the mock is registered (vi.mock is hoisted regardless).
import JoinMeeting from '../views/JoinMeeting.svelte';

const config: DemoConfig = {
  acOriginTemplate: 'http://{subdomain}.localhost:5173',
  gcBaseUrl: '',
  env: 'test',
  devCertHashes: [],
  receiveSlots: { count: 1, source: 'default' },
};

const auth: AuthSession = {
  subdomain: 'demo',
  userToken: 'user-token',
};

test('renders join controls and a live roster from the store', async () => {
  const session = new MockMeetingSession();
  holder.session = session;
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });

  await expect.element(screen.getByTestId('meeting-code')).toBeInTheDocument();
  await expect.element(screen.getByTestId('join-button')).toBeInTheDocument();

  session.fire('participantJoined', { participant: { participantId: 'a', name: 'Ann' } });
  await expect.element(screen.getByTestId('participant-a')).toHaveTextContent('Ann');

  session.fire('participantJoined', { participant: { participantId: 'b', name: 'Bob' } });
  session.fire('participantLeft', { participantId: 'a', reason: 'voluntary' });
  await expect.element(screen.getByTestId('participant-a')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('participant-b')).toHaveTextContent('Bob');
});

test('surfaces an error via last-error without leaking a non-allowlisted field', async () => {
  const session = new MockMeetingSession();
  holder.session = session;
  const screen = await render(JoinMeeting, { config, auth, onSessionInvalid: () => {} });

  const err = new SdkError(SdkErrorCode.Signaling, 'join rejected', { status: 401 });
  (err as { token?: string }).token = 'SECRET_TOKEN_123';
  session.fire('error', err);

  await expect.element(screen.getByTestId('last-error')).toHaveTextContent('SIGNALING');
  await expect.element(screen.getByTestId('last-error')).not.toHaveTextContent('SECRET_TOKEN_123');
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

const assignment = (senderId: number, slotState: 'active' | 'source_muted', slotId: number) => ({
  slotId,
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
    assignments: [assignment(258, 'source_muted', 0), assignment(259, 'source_muted', 1)],
    unreachableSenderIds: [],
  });
  session.fire('participantMuteChanged', {
    participantId: 'c',
    audioServerMuted: true,
    serverMutedBy: 'h',
  });
  const b = screen.getByTestId('participant-b');
  const c = screen.getByTestId('participant-c');
  const h = screen.getByTestId('participant-h');
  // b: slot source_muted, no server mute -> client-muted.
  await expect.element(b).toHaveAttribute('data-client-muted', 'true');
  await expect.element(b).toHaveAttribute('data-server-muted', 'false');
  // c: server-muted -> the slot's source_muted says nothing about their own mic.
  await expect.element(c).toHaveAttribute('data-server-muted', 'true');
  await expect.element(c).toHaveAttribute('data-client-muted', 'unknown');
  await expect.element(screen.getByTestId('participant-server-muted-c')).toHaveTextContent('Hal');
  // h: in no slot of ours -> no wire fact either way.
  await expect.element(h).toHaveAttribute('data-client-muted', 'unknown');
});

test('host controls are absent for a non-host', async () => {
  const { screen } = await joinedRig({ isHost: false });
  await expect.element(screen.getByTestId('participant-b')).toBeInTheDocument();
  await expect.element(screen.getByTestId('server-mute-b')).not.toBeInTheDocument();
});

test('the host affordance sends target + action, and its label follows the WIRE, not the click', async () => {
  const { session, screen } = await joinedRig({ isHost: true });
  const button = screen.getByTestId('server-mute-b');
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');
  await button.click();
  await vi.waitFor(() =>
    expect(session.serverMuteCalls).toEqual([{ participantId: 'b', muted: true }]),
  );
  // No wire update yet: still unpressed.
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');

  session.fire('participantMuteChanged', {
    participantId: 'b',
    audioServerMuted: true,
    serverMutedBy: 'self',
  });
  await expect.element(button).toHaveAttribute('aria-pressed', 'true');
  await expect.element(screen.getByTestId('participant-server-muted-b')).toHaveTextContent('you');

  // A relayed unmute request is shown to the host; it lifts nothing.
  session.fire('unmuteRequested', { participantId: 'b' });
  await expect.element(screen.getByTestId('unmute-requested-b')).toBeInTheDocument();
  await expect
    .element(screen.getByTestId('participant-b'))
    .toHaveAttribute('data-server-muted', 'true');

  await button.click();
  await vi.waitFor(() =>
    expect(session.serverMuteCalls[1]).toEqual({ participantId: 'b', muted: false }),
  );
  session.fire('participantMuteChanged', { participantId: 'b', audioServerMuted: false });
  await expect.element(button).toHaveAttribute('aria-pressed', 'false');
  await expect.element(screen.getByTestId('unmute-requested-b')).not.toBeInTheDocument();
  expect(screen.container.textContent?.toLowerCase()).not.toMatch(/\bban/);
});
