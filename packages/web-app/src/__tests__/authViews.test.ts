// R-43: sign-up + sign-in views drive the REAL AuthApiClient against a stubbed
// `fetch` (no module mock) — exercises the client request path + the view. Also
// asserts the exact data-testid contract (org-subdomain present on BOTH views).

import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { DemoConfig } from '../lib/config.js';
import type { AuthResult } from '../lib/types.js';
import SignUp from '../views/SignUp.svelte';
import SignIn from '../views/SignIn.svelte';

const config: DemoConfig = {
  acOriginTemplate: 'http://{subdomain}.localhost:5173',
  gcBaseUrl: '',
  env: 'test',
  devCertHashes: [],
};

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

test('sign-up renders the required testids and registers on submit', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      jsonResponse({ accessToken: 'user-token', tokenType: 'Bearer', expiresIn: 3600 }),
    ),
  );
  let result: AuthResult | undefined;
  const screen = render(SignUp, { config, onAuthed: (r: AuthResult) => (result = r) });

  for (const id of ['email', 'password', 'display-name', 'org-subdomain']) {
    await expect.element(screen.getByTestId(id)).toBeInTheDocument();
  }

  await screen.getByTestId('email').fill('user@example.com');
  await screen.getByTestId('password').fill('correct horse battery');
  await screen.getByTestId('display-name').fill('Ann');
  await screen.getByTestId('org-subdomain').fill('demo');
  await screen.getByTestId('create-account-button').click();

  await vi.waitFor(() => expect(result).toBeDefined());
  expect(result?.mode).toBe('register');
  expect(result?.userToken).toBe('user-token');
});

test('sign-in renders org-subdomain and surfaces a typed error on 401', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      jsonResponse({ error: { code: 'unauthorized', message: 'bad credentials' } }, 401),
    ),
  );
  const screen = render(SignIn, { config, onAuthed: () => {} });

  for (const id of ['email', 'password', 'org-subdomain']) {
    await expect.element(screen.getByTestId(id)).toBeInTheDocument();
  }

  await screen.getByTestId('email').fill('user@example.com');
  await screen.getByTestId('password').fill('wrong password value');
  await screen.getByTestId('org-subdomain').fill('demo');
  await screen.getByTestId('signin-button').click();

  await expect.element(screen.getByTestId('last-error')).toBeInTheDocument();
});
