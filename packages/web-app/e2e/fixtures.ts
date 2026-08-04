// File: packages/web-app/e2e/fixtures.ts
//
// Task #18: browser E2E fixtures for the join happy path. All DOM driving uses
// the demo app's existing data-testids; all join-state observation uses the
// R-29 `window.__darktower_test__` replay-buffered bus (src/lib/e2eBus.ts) —
// the stable, whitelist-projected E2E contract. No production code is touched.
//
// SECURITY (reviewed by @security, task #18 Gate 1):
//   - Credentials are per-run throwaways (random email/password, disposable org
//     accounts on the local dev cluster) — never hardcoded, never logged here.
//   - The access token is captured from the auth exchange's response body via a
//     narrowly-scoped `waitForResponse` (auto-removed after the read). The
//     exchange is already recorded in Playwright traces; this adds no new
//     exposure surface. Tokens are passed by value and never logged/persisted.
//   - Playwright artifacts (traces) DO contain this synthetic traffic — they are
//     gitignored and must never leave the workstation (see e2e/README.md).

import { expect } from 'playwright/test';
import type { BrowserContext, Page, Request } from 'playwright/test';
// Resolved to sdk-core SOURCE via this package's tsconfig `paths` (Playwright
// honors tsconfig path mapping), keeping GC's R-53 camelCase wire mapping
// single-sourced instead of re-encoding it in a raw fetch (@dry-reviewer).
import { MeetingApiClient, type AuthTokenResponse } from '@darktower/sdk-core';
import { e2eEnv } from './env.js';

// ============================================================================
// Credentials
// ============================================================================

/** Per-run throwaway credentials for the seeded `demo` org. */
export interface TestCredentials {
  readonly email: string;
  readonly password: string;
  readonly displayName: string;
}

const runId = crypto.randomUUID().slice(0, 8);
let userCounter = 0;

/**
 * Runtime-generated throwaway password — a function call, never a literal
 * assignment, so there is no hardcoded value to leak (and nothing for the
 * ts-no-secrets guard to flag). AC requires only min-8 chars
 * (ac-service user_service.rs); a UUID comfortably clears it.
 */
function randomPassword(): string {
  return crypto.randomUUID();
}

/**
 * Fresh throwaway credentials. AC registration is rate-limited to 5/hour — keep
 * the suite at 2 registrations per run (see e2e/README.md budget note).
 */
export function randomCredentials(label: string): TestCredentials {
  userCounter += 1;
  return {
    email: `e2e-${runId}-${userCounter}@darktower.test`,
    password: randomPassword(),
    displayName: `E2E ${label} ${runId}`,
  };
}

// ============================================================================
// Auth (UI-driven)
// ============================================================================

/** Fill the shared auth fields (SignUp and SignIn use the same testids). */
async function fillAuthFields(page: Page, creds: TestCredentials): Promise<void> {
  await page.getByTestId('email').fill(creds.email);
  await page.getByTestId('password').fill(creds.password);
  await page.getByTestId('org-subdomain').fill(e2eEnv.orgSubdomain);
}

/**
 * Wait for the auth exchange response on `path` while `action` drives the UI,
 * and return the issued access token. Narrowly scoped: the response predicate
 * matches only the given auth path and the listener ends with the wait.
 */
async function captureAccessToken(
  page: Page,
  path: string,
  action: () => Promise<void>,
): Promise<string> {
  const [response] = await Promise.all([
    page.waitForResponse((r) => new URL(r.url()).pathname === path, { timeout: 15_000 }),
    action(),
  ]);
  expect(response.ok(), `auth exchange ${path} failed: HTTP ${response.status()}`).toBe(true);
  // Partial<AuthTokenResponse>: the sdk-core wire type (R-53 camelCase SSoT —
  // covers both /register's RegisterResponse extends AuthTokenResponse and
  // /user/token's AuthTokenResponse), Partial because presence is checked
  // defensively below (@dry-reviewer, task #18 review).
  const body = (await response.json()) as Partial<AuthTokenResponse>;
  expect(typeof body.accessToken, `auth exchange ${path} returned no accessToken`).toBe('string');
  return body.accessToken as string;
}

/**
 * Drive the sign-up view (R-40) and return the issued access token (needed
 * Node-side by {@link bootstrapMeeting}). Resolves once the authed shell is
 * rendered (create view visible).
 */
export async function signUpViaUi(page: Page, creds: TestCredentials): Promise<string> {
  await page.getByTestId('nav-signup').click();
  await fillAuthFields(page, creds);
  await page.getByTestId('display-name').fill(creds.displayName);
  const token = await captureAccessToken(page, '/api/v1/auth/register', () =>
    page.getByTestId('create-account-button').click(),
  );
  await expect(page.getByTestId('meeting-title')).toBeVisible();
  return token;
}

/**
 * Drive the sign-in view for an EXISTING user. A fresh credential->token
 * exchange is legitimate here — the task-#58 invariant forbids re-auth at JOIN
 * time, which assertion (e)'s recorder window enforces separately.
 */
export async function signInViaUi(page: Page, creds: TestCredentials): Promise<string> {
  await page.getByTestId('nav-signin').click();
  await fillAuthFields(page, creds);
  const token = await captureAccessToken(page, '/api/v1/auth/user/token', () =>
    page.getByTestId('signin-button').click(),
  );
  await expect(page.getByTestId('meeting-title')).toBeVisible();
  return token;
}

// ============================================================================
// Meeting bootstrap (Node-side, NOT through the demo UI)
// ============================================================================

/**
 * Create a meeting via direct GC `POST /api/v1/meetings` (task #18 requirement:
 * not through the demo UI) and return its meeting code. The building block for
 * multi-user specs (#19) that need meetings without driving the create view.
 */
export async function bootstrapMeeting(adminToken: string, title: string): Promise<string> {
  const client = new MeetingApiClient({ gcBaseUrl: e2eEnv.gcUrl });
  const response = await client.createMeeting({ displayName: title }, { userToken: adminToken });
  expect(response.meetingCode, 'GC createMeeting returned no meetingCode').toBeTruthy();
  return response.meetingCode;
}

// ============================================================================
// Join + bus observation
// ============================================================================

/** The bus projection of the MC JoinResponse (see src/lib/e2eBus.ts `joined`). */
export interface JoinedBusEvent {
  readonly type: 'joined';
  readonly participantId: string;
  readonly userId: string;
  readonly participants: ReadonlyArray<{ participantId: string; name: string }>;
  readonly mediaServers: readonly string[];
}

/**
 * Drive the join view: token-based `MeetingSession.join` using the retained
 * session token (post-task-#58 contract — there is NO login step at join time).
 */
export async function joinAsUser(page: Page, meetingCode: string): Promise<void> {
  await page.getByTestId('nav-join').click();
  await page.getByTestId('meeting-code').fill(meetingCode);
  await page.getByTestId('join-button').click();
}

/** Read the replay buffer (empty array when the bus is absent). */
export async function busEvents(
  page: Page,
): Promise<ReadonlyArray<Readonly<Record<string, unknown>>>> {
  return page.evaluate(() => window.__darktower_test__?.events ?? []);
}

/** Shared failure context: what the bus actually saw (or that it is missing). */
async function busFailureContext(page: Page): Promise<string> {
  const busPresent = await page.evaluate(() => window.__darktower_test__ !== undefined);
  if (!busPresent) {
    return (
      `window.__darktower_test__ is ABSENT. The server at ${e2eEnv.baseUrl} is not a ` +
      `dev-mode Vite server: the __E2E_HOOKS__ bus is dead-code-eliminated outside dev ` +
      `mode. With reuseExistingServer, a squatting \`pnpm preview\`/prod server gets ` +
      `reused as-is — stop it and let Playwright start \`pnpm dev\`, or restart it in ` +
      `dev mode.`
    );
  }
  const events = await busEvents(page);
  // Explicit 1s bound: `last-error` is CONDITIONALLY rendered, and an
  // error-message builder must never wait meaningfully — an unbounded
  // auto-wait here would swallow this whole diagnostic on pure-hang failures
  // (the config's actionTimeout is a backstop, not a substitute; @test review).
  const lastError = await page
    .getByTestId('last-error')
    .textContent({ timeout: 1_000 })
    .catch(() => null);
  return (
    `bus events observed so far: ${JSON.stringify(events)}` +
    (lastError ? `; demo last-error: "${lastError}"` : '')
  );
}

/**
 * Wait for the `joined` bus event (MC JoinResponse received) and return its
 * projection. On timeout, the error names what WAS observed — including the
 * squatting-server failure mode when the bus itself is missing.
 */
export async function waitForJoined(page: Page, timeoutMs = 30_000): Promise<JoinedBusEvent> {
  try {
    await page.waitForFunction(
      () => window.__darktower_test__?.events.some((e) => e['type'] === 'joined') ?? false,
      undefined,
      { timeout: timeoutMs },
    );
  } catch {
    throw new Error(
      `no 'joined' bus event within ${timeoutMs}ms. ${await busFailureContext(page)}`,
    );
  }
  const events = await busEvents(page);
  const joined = events.find((e) => e['type'] === 'joined') as JoinedBusEvent | undefined;
  if (
    joined === undefined ||
    typeof joined.participantId !== 'string' ||
    !Array.isArray(joined.mediaServers)
  ) {
    throw new Error(`malformed 'joined' bus event: ${JSON.stringify(joined)}`);
  }
  return joined;
}

/**
 * Assertion (b): wait until a `mediaConnected` bus event exists for EVERY URL in
 * `media_servers` (active/active — all MHs, not just one). Failure names the
 * missing URLs, m-of-n.
 */
export async function waitForAllMediaConnected(
  page: Page,
  mediaServers: readonly string[],
  timeoutMs = 30_000,
): Promise<void> {
  try {
    await page.waitForFunction(
      (urls) => {
        const bus = window.__darktower_test__;
        if (bus === undefined) return false;
        const connected = new Set(
          bus.events.filter((e) => e['type'] === 'mediaConnected').map((e) => e['mhUrl']),
        );
        return urls.every((url) => connected.has(url));
      },
      [...mediaServers],
      { timeout: timeoutMs },
    );
  } catch {
    const events = await busEvents(page);
    const connected = events
      .filter((e) => e['type'] === 'mediaConnected')
      .map((e) => e['mhUrl'] as string);
    const missing = mediaServers.filter((url) => !connected.includes(url));
    throw new Error(
      `MH WebTransport handshakes incomplete within ${timeoutMs}ms: ` +
        `${connected.length} of ${mediaServers.length} media_servers connected; ` +
        `missing: ${JSON.stringify(missing)}. ${await busFailureContext(page)}`,
    );
  }
}

/**
 * Assertion (c): wait (bounded — the 5s contract is the assertion) until the
 * FIRST context's bus sees `participantJoined` for `participantId`.
 */
export async function waitForParticipantJoined(
  page: Page,
  participantId: string,
  timeoutMs = 5_000,
): Promise<void> {
  try {
    await page.waitForFunction(
      (id) =>
        window.__darktower_test__?.events.some(
          (e) => e['type'] === 'participantJoined' && e['participantId'] === id,
        ) ?? false,
      participantId,
      { timeout: timeoutMs },
    );
  } catch {
    throw new Error(
      `no 'participantJoined' bus event for participant ${participantId} within ` +
        `${timeoutMs}ms (the R-46 roster-propagation contract). ` +
        `${await busFailureContext(page)}`,
    );
  }
}

// ============================================================================
// Assertion (e): token-only join traffic (task #58 item c-iii)
// ============================================================================

/** One recorded HTTP request (WebTransport traffic is not visible here — and the
 * join path puts no credential on WT either: join is token-based end-to-end). */
export interface RecordedRequest {
  readonly url: string;
  readonly method: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly postData: string | null;
}

export interface RequestRecorder {
  /** Stop recording and return everything captured in this window. */
  stop(): readonly RecordedRequest[];
}

/**
 * Open a PER-CONTEXT recording window. Call at the context's OWN join action —
 * never earlier — so legitimate pre-join auth exchanges (sign-up/sign-in) stay
 * outside the window (@test Requirement A).
 */
export function recordRequests(context: BrowserContext): RequestRecorder {
  const records: RecordedRequest[] = [];
  const listener = (request: Request): void => {
    records.push({
      url: request.url(),
      method: request.method(),
      headers: request.headers(),
      postData: request.postData(),
    });
  };
  context.on('request', listener);
  return {
    stop(): readonly RecordedRequest[] {
      context.off('request', listener);
      return records;
    },
  };
}

/**
 * The token-only invariant, value-based: within the join window,
 *   1. NO request carries the actual email/password strings anywhere
 *      (URL, headers, body) — values, not field names, so renames can't dodge
 *      it, and both raw AND percent-encoded forms are scanned so a credential
 *      smuggled into a query string (`@` -> `%40`) can't dodge it either;
 *   2. NO request targets /api/v1/auth/* at all (structural guard: the forced
 *      re-login band-aid of commit 7b69288 must never be re-encoded);
 *   3. ANY `Authorization` header in the window is `Bearer ...` — nothing else
 *      (closes the `Basic base64(email:password)` channel, which the value-scan
 *      cannot see);
 *   4. the GC meeting request(s) authenticate via `Authorization: Bearer`.
 */
export function assertTokenOnlyJoinTraffic(
  records: readonly RecordedRequest[],
  creds: TestCredentials,
  label: string,
): void {
  const leaks: string[] = [];
  const authCalls: string[] = [];
  const nonBearerAuthHeaders: string[] = [];
  const meetingRequests: RecordedRequest[] = [];
  // Raw + percent-encoded forms. Only the email actually differs under
  // encodeURIComponent (the password is a UUID), but scanning all four is
  // self-documenting and rename-proof (@security, task #18 review). Each needle
  // carries its redaction marker: the failure message must show WHERE a value
  // appeared without repeating it — echoing the credential in an assertion
  // error would itself be a leak into whatever log sink renders the failure
  // (@semantic-guard, credential-leak item 8).
  const needles: ReadonlyArray<readonly [string, string]> = [
    [creds.email, '[email]'],
    [creds.password, '[password]'],
    [encodeURIComponent(creds.email), '[email:urlencoded]'],
    [encodeURIComponent(creds.password), '[password:urlencoded]'],
  ];
  // EVERY string echoed into an expect failure message goes through this —
  // the exact regression this assertion detects is a credential on a request,
  // and the failure diagnostic must show WHERE without repeating the value
  // into whatever log sink renders it. New push/expect sites in this function
  // must use it too (@semantic-guard, credential-leak item 8).
  const redact = (s: string): string =>
    needles.reduce((acc, [needle, marker]) => acc.replaceAll(needle, marker), s);

  for (const record of records) {
    const haystack = `${record.url}\n${JSON.stringify(record.headers)}\n${record.postData ?? ''}`;
    if (needles.some(([needle]) => haystack.includes(needle))) {
      leaks.push(redact(`${record.method} ${record.url}`));
    }
    const authHeader = record.headers['authorization'];
    if (authHeader !== undefined && !authHeader.startsWith('Bearer ')) {
      const scheme = authHeader.split(' ')[0] ?? '<empty>';
      nonBearerAuthHeaders.push(redact(`${record.method} ${record.url} (scheme: ${scheme})`));
    }
    const pathname = new URL(record.url).pathname;
    if (pathname.startsWith('/api/v1/auth/')) {
      authCalls.push(redact(`${record.method} ${record.url}`));
    }
    if (pathname.startsWith('/api/v1/meetings')) {
      meetingRequests.push(record);
    }
  }

  expect(leaks, `[${label}] requests carrying raw credentials at join time`).toEqual([]);
  expect(authCalls, `[${label}] auth endpoints hit at join time (re-login regression)`).toEqual([]);
  expect(
    nonBearerAuthHeaders,
    `[${label}] non-Bearer Authorization headers at join time (Basic/Digest carry credentials)`,
  ).toEqual([]);
  expect(
    meetingRequests.length,
    `[${label}] expected at least one GC /api/v1/meetings request in the join window`,
  ).toBeGreaterThan(0);
  for (const request of meetingRequests) {
    expect(
      request.headers['authorization'] ?? '',
      `[${label}] GC request ${redact(request.url)} must authenticate via bearer token`,
    ).toMatch(/^Bearer /);
  }
}
