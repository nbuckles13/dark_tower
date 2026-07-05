// R-43 / R-12: create-meeting view drives the REAL MeetingApiClient against a
// stubbed `fetch` and displays the returned meeting code. Asserts the
// meeting-title / scheduled-start / create-button / created-meeting-code testids.

import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { DemoConfig } from '../lib/config.js';
import type { AuthResult } from '../lib/types.js';
import CreateMeeting from '../views/CreateMeeting.svelte';

const config: DemoConfig = {
  acOriginTemplate: 'http://{subdomain}.localhost:5173',
  gcBaseUrl: '',
  env: 'test',
  devCertHashes: [],
};

const auth: AuthResult = {
  subdomain: 'demo',
  email: 'user@example.com',
  password: 'correct horse battery',
  displayName: 'Ann',
  mode: 'login',
  userToken: 'user-token',
};

afterEach(() => {
  vi.unstubAllGlobals();
});

test('creates a meeting and shows the returned code', async () => {
  const fetchMock = vi.fn(
    async () =>
      new Response(
        JSON.stringify({
          meetingId: 'm1',
          meetingCode: 'ABC123DEF456',
          displayName: 'Standup',
          status: 'scheduled',
          maxParticipants: 100,
          enableE2eEncryption: false,
          requireAuth: true,
          recordingEnabled: false,
          allowGuests: false,
          allowExternalParticipants: false,
          waitingRoomEnabled: false,
          createdAt: '2026-07-05T00:00:00Z',
        }),
        { status: 201, headers: { 'content-type': 'application/json' } },
      ),
  );
  vi.stubGlobal('fetch', fetchMock);

  const screen = render(CreateMeeting, { config, auth, onGoJoin: () => {} });

  for (const id of ['meeting-title', 'scheduled-start', 'create-button']) {
    await expect.element(screen.getByTestId(id)).toBeInTheDocument();
  }

  await screen.getByTestId('meeting-title').fill('Standup');
  await screen.getByTestId('create-button').click();

  await expect
    .element(screen.getByTestId('created-meeting-code'))
    .toHaveTextContent('ABC123DEF456');
  // The user token rides only in the Authorization header (R-23), never the URL.
  const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
  const headers = init.headers as Record<string, string>;
  expect(headers['authorization']).toBe('Bearer user-token');
});
