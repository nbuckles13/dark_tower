// Task #58 (f): the app shell's session state machine.
//
// The Sign-up / Sign-in nav is a SECURITY CONTROL, not UX polish — once a session
// exists the auth views must be unreachable, and once the session is dropped the auth
// affordance must come back. This file pins all three states.
//
// TIER: drives the REAL `AuthApiClient` against a stubbed global `fetch`, like
// `authViews.test.ts` (whose header says the same). Deliberately NOT the module-mock
// form used by `joinMeeting.test.ts` — that file mocks `lib/session.js` because the
// session is not what it tests, whereas here the assertion IS that the
// `onAuthed` -> `auth` transition drives nav rendering. Mocking the module that
// produces `onAuthed` would reduce this to "a mock set a variable". Both tiers are
// correct for their own file; don't "harmonize" them.
//
// STATES ARE SEPARATE `test()` BLOCKS, not one sequential walk: a failure in state 2
// would otherwise hide states 3-5, the failure message would name the file rather
// than the behaviour, and the states would become order-coupled.

import { afterEach, expect, test, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import type { DemoConfig } from '../lib/config.js';
import App from '../App.svelte';

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

/**
 * A `fetch` stub that authenticates, then fails every subsequent call with `status`,
 * RECORDING each URL.
 *
 * The recorded list is how these tests prove the rejected call actually happened.
 * Asserting a rendered `last-error` does NOT work here and the reason is instructive:
 * the recovery unmounts the view that renders it, so the element is legitimately gone
 * by the time an assertion could see it. A test that waited for it would be asserting
 * the absence of the recovery.
 */
function authThenFail(status: number): { fetchImpl: ReturnType<typeof vi.fn>; urls: string[] } {
  const urls: string[] = [];
  let authed = false;
  const fetchImpl = vi.fn(async (input: string | URL) => {
    urls.push(typeof input === 'string' ? input : input.toString());
    if (!authed) {
      authed = true;
      return jsonResponse({ accessToken: 'user-token', tokenType: 'Bearer', expiresIn: 3600 });
    }
    return jsonResponse({ error: { code: 'unauthorized', message: 'token rejected' } }, status);
  });
  return { fetchImpl, urls };
}

/** Drive the real sign-up form to a real authenticated shell. */
async function signUp(screen: Awaited<ReturnType<typeof render>>): Promise<void> {
  await screen.getByTestId('email').fill('user@example.com');
  await screen.getByTestId('password').fill('correct horse battery');
  await screen.getByTestId('display-name').fill('Ann');
  await screen.getByTestId('org-subdomain').fill('demo');
  await screen.getByTestId('create-account-button').click();
  await expect.element(screen.getByTestId('nav-create')).toBeInTheDocument();
}

// A leaked global `fetch` stub is a cross-test-ordering flake, and flakes are the one
// thing that must not be deferred (ADR-0028).
afterEach(() => {
  vi.unstubAllGlobals();
});

// --- state 1: unauthenticated -------------------------------------------------
//
// The POSITIVE control for state 2. Without it, a typo'd selector makes the
// `.not.toBeInTheDocument()` assertion below green forever.
test('unauthenticated shell renders the Sign-up and Sign-in nav', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => jsonResponse({})),
  );
  const screen = await render(App, { config });

  await expect.element(screen.getByTestId('nav-signup')).toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-signin')).toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-create')).not.toBeInTheDocument();
});

// --- state 2: authenticated ---------------------------------------------------
test('authenticated shell removes the Sign-up and Sign-in nav', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      jsonResponse({ accessToken: 'user-token', tokenType: 'Bearer', expiresIn: 3600 }),
    ),
  );
  const screen = await render(App, { config });
  await signUp(screen);

  await expect.element(screen.getByTestId('nav-signup')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-signin')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-join')).toBeInTheDocument();
});

// --- state 2b: the control's effect, not its implementation -------------------
//
// The component tier cannot force `view` (it is internal `$state`, and adding a prop
// for testability would be production API for test convenience). So this asserts what
// the control PROVIDES: with a session, no credential input exists anywhere in the
// shell. The stronger claim — "unreachable even if `view` were forced" — is
// review-verified only and is stated as such in the devloop output.
test('authenticated shell exposes no credential input anywhere', async () => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () =>
      jsonResponse({ accessToken: 'user-token', tokenType: 'Bearer', expiresIn: 3600 }),
    ),
  );
  const screen = await render(App, { config });
  await signUp(screen);

  await expect.element(screen.getByTestId('password')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('email')).not.toBeInTheDocument();
});

// --- state 3: session death via JOIN ------------------------------------------
//
// Drives a REAL 401 through the stubbed fetch rather than invoking the callback
// directly: calling `onSessionInvalid()` would assert that a callback clears state,
// not that a 401 invokes it — and a callback wired to nothing would still pass.
//
// Asserts the Sign-in VIEW, not just the nav button: "the nav returns" is satisfied
// by a blank <main> (nav present, nothing rendered), which is precisely the state the
// bidirectional derived view exists to prevent.
test('a 401 at join drops the session and restores the sign-in view', async () => {
  const { fetchImpl, urls } = authThenFail(401);
  vi.stubGlobal('fetch', fetchImpl);
  const screen = await render(App, { config });
  await signUp(screen);

  await screen.getByTestId('nav-join').click();
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();

  // Recovery: nav back AND the view actually renders. Asserting only "the nav
  // returns" would pass with a blank <main> — nav present, nothing rendered — which
  // is precisely the state the bidirectional derived view exists to prevent.
  await expect.element(screen.getByTestId('nav-signin')).toBeInTheDocument();
  await expect.element(screen.getByTestId('signin-button')).toBeInTheDocument();
  await expect.element(screen.getByTestId('email')).toBeInTheDocument();

  // The rejected request really happened — guards against a test that never reached
  // the join and therefore proves nothing about recovery.
  expect(urls.some((u) => u.includes('/api/v1/meetings/'))).toBe(true);
});

// --- state 4: session death via CREATE ----------------------------------------
//
// Its own assertion rather than sharing state 3's: `onSessionInvalid` is wired at two
// production call sites, and two paths behind one test is a coin flip on which one
// regresses. The create path is the MORE likely to rot — it is the less-travelled
// flow and nothing else exercises it.
test('a 401 at create-meeting drops the session and restores the sign-in view', async () => {
  const { fetchImpl, urls } = authThenFail(401);
  vi.stubGlobal('fetch', fetchImpl);
  const screen = await render(App, { config });
  await signUp(screen);

  // Sign-up lands on the create view already.
  await screen.getByTestId('meeting-title').fill('Standup');
  await screen.getByTestId('create-button').click();

  await expect.element(screen.getByTestId('nav-signin')).toBeInTheDocument();
  await expect.element(screen.getByTestId('signin-button')).toBeInTheDocument();
  expect(urls.some((u) => u.endsWith('/api/v1/meetings'))).toBe(true);
});

// --- state 5: 403 must NOT be session death -----------------------------------
//
// The INVERSE of states 3/4, and the more valuable assertion of the pair: it pins that
// an authorization denial does not nuke a good credential.
//
// This test originally asserted the opposite, on the premise that "a revoked token
// yields 403 where an expired one yields 401." That premise does not hold against GC:
// every 403 these calls can produce is a decision on a VALID, live token — org meeting
// limit exceeded, insufficient permissions, external participants not allowed. GC's
// contract is explicit (401 = invalid/missing token, 403 = user not allowed to join),
// and its auth middleware documents 401 only for missing/invalid tokens.
//
// The user-visible cost of getting this wrong: someone who hits the org meeting limit
// is silently signed out, their valid session discarded, and dropped on Sign in — where
// they re-authenticate, retry, and hit the same limit. A loop with no explanation,
// because the error text is destroyed when the view unmounts (see `authThenFail`).
test('a 403 at join PRESERVES the session — an authorization denial is not credential death', async () => {
  const { fetchImpl, urls } = authThenFail(403);
  vi.stubGlobal('fetch', fetchImpl);
  const screen = await render(App, { config });
  await signUp(screen);

  await screen.getByTestId('nav-join').click();
  await screen.getByTestId('meeting-code').fill('abc123ABC456');
  await screen.getByTestId('join-button').click();

  // The request happened (@test T3) …
  await vi.waitFor(() => expect(urls.some((u) => u.includes('/api/v1/meetings/'))).toBe(true));
  // … and the session SURVIVED it: still authenticated, no auth nav, error shown in place.
  await expect.element(screen.getByTestId('last-error')).toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-signin')).not.toBeInTheDocument();
  await expect.element(screen.getByTestId('nav-join')).toBeInTheDocument();
});
