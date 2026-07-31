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
  configureTelemetryIfEnabled: () => {},
}));

// Imported after the mock is registered (vi.mock is hoisted regardless).
import JoinMeeting from '../views/JoinMeeting.svelte';

const config: DemoConfig = {
  acOriginTemplate: 'http://{subdomain}.localhost:5173',
  gcBaseUrl: '',
  env: 'test',
  devCertHashes: [],
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
