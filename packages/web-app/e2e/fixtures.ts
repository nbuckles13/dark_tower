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
import type { BrowserContext, Page, Request, Response } from 'playwright/test';
// Resolved to sdk-core SOURCE via this package's tsconfig `paths` (Playwright
// honors tsconfig path mapping), keeping GC's R-53 camelCase wire mapping
// single-sourced instead of re-encoding it in a raw fetch (@dry-reviewer).
// SdkErrorCode is the SSoT for the demo's `last-error` code prefixes (errorText
// renders `${err.code}: ${err.message}`) — expectLastErrorCode types against it
// so a typo'd prefix is a compile error (@code-reviewer, task #19 Gate 1).
import {
  MeetingApiClient,
  type AuthTokenResponse,
  type RegisterResponse,
  type SdkErrorCode,
} from '@darktower/sdk-core';
import { e2eEnv, toLoopbackUrl } from './env.js';

// ============================================================================
// Credentials
// ============================================================================

/**
 * Per-run throwaway credentials for the run's organization — `e2eEnv.orgSubdomain`,
 * the per-run org layer 7 provisions (R-7), NOT the shared seeded `demo` org.
 * `fillAuthFields` is what binds them to it; see its note.
 */
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
 * Fresh throwaway credentials. AC registration is rate-limited — the limit's
 * SSoT for the Kind cluster this suite targets is
 * `infra/services/ac-service/configmap.yaml` (`AC_REGISTRATION_RATE_LIMIT_*`,
 * relaxed to 100/min for dev/test); the suite's per-run registration budget is
 * tracked in e2e/README.md §Budgets (2/run: the shared user V + one distinct
 * second party — see {@link SHARED_USER}).
 */
export function randomCredentials(label: string): TestCredentials {
  userCounter += 1;
  return {
    email: `e2e-${runId}-${userCounter}@darktower.test`,
    password: randomPassword(),
    displayName: `E2E ${label} ${runId}`,
  };
}

/**
 * The suite's ONE shared valid user — the single-source-of-truth for the
 * registration budget (see e2e/README.md §Budgets). Registered exactly ONCE per
 * run and reused everywhere via `signInViaUi` (0 further registrations), so the
 * whole suite spends 2 registrations: this user + the one genuinely-distinct
 * second party (`userB`) in the distinct-user two-party test.
 *
 * BUDGET ↔ workers=1 COUPLING (do not decouple silently): "registered once per
 * run" rests on `workers: 1` (playwright.config.ts). A single worker process
 * means the module-level `sharedRegistration` memo below is a per-RUN singleton.
 * Raising `workers` would give each worker its OWN module instance → V would be
 * re-registered per worker, multiplying the budget invisibly. If `workers` ever
 * changes, this memo must move to a global-setup / setup-project that registers
 * V once and hands its creds to workers.
 */
export const SHARED_USER = randomCredentials('shared');

/** Success-only memo: holds the registration promise ONLY once it has resolved. */
let sharedRegistration: Promise<void> | undefined;

/**
 * Register {@link SHARED_USER} exactly once, in an ISOLATED throwaway context.
 *
 * The isolation is load-bearing: some specs install a `page.route('**...register…')`
 * fulfill (auth-rejection's `fulfillAuthRegister`) that would intercept a real
 * sign-up. Routes are per-context, so registering V in its own fresh context is
 * immune to whatever the caller's page has mocked.
 *
 * Fail-loud, write-once: on failure the memo is RESET (never cached as a
 * partial/undefined identity) and a single clearly-named diagnostic is thrown —
 * V is the suite's single valid-user SPOF, so its registration failure must read
 * at the real cause, not as N cryptic downstream `signInViaUi(V)` timeouts.
 */
async function ensureSharedUserRegistered(page: Page): Promise<void> {
  if (sharedRegistration === undefined) {
    sharedRegistration = (async (): Promise<void> => {
      const browser = page.context().browser();
      if (browser === null) {
        throw new Error('no Browser handle on the page context (persistent context?)');
      }
      const context = await browser.newContext();
      try {
        const registrationPage = await context.newPage();
        await registrationPage.goto('/');
        await signUpViaUi(registrationPage, SHARED_USER);
      } finally {
        await context.close();
      }
    })().catch((err: unknown) => {
      sharedRegistration = undefined; // do not cache a failed registration
      const detail = err instanceof Error ? err.message : String(err);
      throw new Error(`shared user V registration failed: ${detail}`);
    });
  }
  return sharedRegistration;
}

/**
 * Authenticate `page` as the shared valid user {@link SHARED_USER}, registering
 * it once per run on first use (see {@link ensureSharedUserRegistered}). Returns
 * the issued access token (needed Node-side by {@link bootstrapMeeting}). This is
 * the 0-registration way to get a page into the authed shell holding a real,
 * valid session.
 */
export async function authAsSharedUser(page: Page): Promise<string> {
  await ensureSharedUserRegistered(page);
  return signInViaUi(page, SHARED_USER);
}

// ============================================================================
// Wire paths (module-local single encoding)
// ============================================================================

// The wire paths this harness observes, each encoded ONCE here (observer-side
// duplicates of sdk-core's module-private consts — the tracked barrel
// extraction in docs/TODO.md §Cross-Service Duplication makes each a one-site
// import swap later; @dry-reviewer, task #19 review). Consumed by BOTH the
// waitForResponse predicates and the route globs below.
const REGISTER_PATH = '/api/v1/auth/register';
const LOGIN_PATH = '/api/v1/auth/user/token';

/** GC's per-meeting join path for `code` (predicates + route globs). */
function meetingPath(code: string): string {
  return `/api/v1/meetings/${code}`;
}

// ============================================================================
// Auth (UI-driven)
// ============================================================================

/**
 * Fill the shared auth fields (SignUp and SignIn use the same testids).
 *
 * The org-subdomain fill is LOAD-BEARING, not boilerplate (@dry-reviewer, task
 * #3 review): `SignUp.svelte` and `SignIn.svelte` prefill `demo` in their
 * `$state`, so a spec that skips this fill and rides the prefill signs into the
 * shared `demo` org — silently un-fixing R-7 for the one suite whose
 * meeting-cap exhaustion motivated the per-run org, while every gate stays
 * green. Always fill from `e2eEnv.orgSubdomain`; never rely on the view default.
 */
async function fillAuthFields(page: Page, creds: TestCredentials): Promise<void> {
  await page.getByTestId('email').fill(creds.email);
  await page.getByTestId('password').fill(creds.password);
  await page.getByTestId('org-subdomain').fill(e2eEnv.orgSubdomain);
}

/**
 * Wait for the response whose pathname is exactly `path` while `action` drives
 * the UI, and return it. THE one encoding of the response-capture idiom
 * (@dry-reviewer, task #19 review): PATH-only predicate + 15s timeout,
 * narrowly scoped (the listener ends with the wait). The predicate matching on
 * path ONLY is load-bearing — callers assert status/body EXACTLY afterwards,
 * so a wrong-status outcome is a named assertion failure, never an opaque
 * timeout (@test, task #19 Gate 1).
 */
async function captureResponse(
  page: Page,
  path: string,
  action: () => Promise<void>,
): Promise<Response> {
  const [response] = await Promise.all([
    page.waitForResponse((r) => new URL(r.url()).pathname === path, { timeout: 15_000 }),
    action(),
  ]);
  return response;
}

/**
 * Wait for the auth exchange response on `path` while `action` drives the UI,
 * and return the issued access token.
 */
async function captureAccessToken(
  page: Page,
  path: string,
  action: () => Promise<void>,
): Promise<string> {
  const response = await captureResponse(page, path, action);
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
  const token = await captureAccessToken(page, REGISTER_PATH, () =>
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
  const token = await captureAccessToken(page, LOGIN_PATH, () =>
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

/** Open the join view and fill the meeting code (shared by both join drivers). */
async function fillJoinForm(page: Page, meetingCode: string): Promise<void> {
  await page.getByTestId('nav-join').click();
  await page.getByTestId('meeting-code').fill(meetingCode);
}

/**
 * Drive the join view: token-based `MeetingSession.join` using the retained
 * session token (post-task-#58 contract — there is NO login step at join time).
 */
export async function joinAsUser(page: Page, meetingCode: string): Promise<void> {
  await fillJoinForm(page, meetingCode);
  await page.getByTestId('join-button').click();
}

/**
 * Drive the join view AND capture the HTTP status of GC's join response for
 * `meetingCode` (task #19 negative paths). The waitForResponse predicate matches
 * on PATH only — the caller asserts the status EXACTLY afterwards, so a
 * wrong-status outcome is a named assertion failure, never an opaque timeout
 * (@test, task #19 Gate 1; same discipline as captureAccessToken above).
 */
export async function joinCapturingGcStatus(page: Page, meetingCode: string): Promise<number> {
  await fillJoinForm(page, meetingCode);
  const response = await captureResponse(page, meetingPath(meetingCode), () =>
    page.getByTestId('join-button').click(),
  );
  return response.status();
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

/**
 * Gap (2): wait (bounded) until `page`'s bus sees `participantLeft` for
 * `participantId` — the SDK received MC's `ParticipantLeft` broadcast for the
 * departed peer (task-#64 leave contract / R-46 roster propagation, departure
 * side). Own diagnostic (NOT a bare event-string swap of `waitForParticipantJoined`):
 * a LEFT that never arrives means a different thing than a JOINED that never
 * arrives — the peer's teardown did not propagate, or exceeded MC's grace/idle
 * worst case (see mcMetrics.ts `waitForMcParticipantLeavesAbove` for the config
 * SSoT). The default budget covers MC's broadcast worst case
 * (idle_timeout + grace_period + grace-check ≈ 45s, no Prometheus scrape hop) —
 * the spec drives a clean close so it resolves in ~1 tick in practice.
 */
export async function waitForParticipantLeft(
  page: Page,
  participantId: string,
  timeoutMs = 60_000,
): Promise<void> {
  try {
    await page.waitForFunction(
      (id) =>
        window.__darktower_test__?.events.some(
          (e) => e['type'] === 'participantLeft' && e['participantId'] === id,
        ) ?? false,
      participantId,
      { timeout: timeoutMs },
    );
  } catch {
    throw new Error(
      `no 'participantLeft' bus event for participant ${participantId} within ` +
        `${timeoutMs}ms (the departed peer's teardown did not propagate, or exceeded ` +
        `MC's grace/idle worst case). ${await busFailureContext(page)}`,
    );
  }
}

// ============================================================================
// Roster DOM assertions (gap (4): tie bus truth to the rendered roster)
// ============================================================================

/**
 * Assert the live-roster DOM (`JoinMeeting.svelte` `participant-list`) renders a
 * `<li data-testid="participant-${participantId}">` whose text is EXACTLY
 * `expectedName`. Ties the bus/JoinResponse truth to what the demo actually
 * paints. When the joining peer authenticated via `signInViaUi` (no client-side
 * displayName — `SignIn.svelte`), a correct name here proves the name is carried
 * by the meeting token, not band-aided client-side.
 */
export async function expectRosterShows(
  page: Page,
  participantId: string,
  expectedName: string,
): Promise<void> {
  const entry = page.getByTestId(`participant-${participantId}`);
  await expect(entry, `roster must render participant ${participantId}`).toBeVisible();
  await expect(
    entry,
    `roster entry for ${participantId} must show its (token-carried) display name`,
  ).toHaveText(expectedName);
}

/** Assert the roster DOM renders NO entry for `participantId` (post-departure). */
export async function expectRosterMissing(page: Page, participantId: string): Promise<void> {
  await expect(
    page.getByTestId(`participant-${participantId}`),
    `roster must NOT render departed participant ${participantId}`,
  ).toHaveCount(0);
}

// ============================================================================
// Negative-path helpers (task #19, R-45)
// ============================================================================

/**
 * A random, WELL-FORMED meeting code that (statistically) matches no meeting.
 *
 * FORMAT AUTHORITIES (coupled — @dry-reviewer, task #19 Gate 1): must pass BOTH
 * the SDK's client-side guard (`packages/sdk-core/src/validation/limits.ts`
 * `MEETING_CODE_REGEX` = 12 alphanumerics, checked FIRST by `joinMeeting`) and
 * GC's pre-DB-lookup shape check (`crates/gc-service/src/handlers/meetings.rs`
 * `MEETING_CODE_LENGTH`), so a rejection is a REAL 404 lookup miss — never a
 * client-side ValidationError or a GC 400. UUID hex chars are a strict subset
 * of the allowed alphabet.
 */
export function randomMeetingCode(): string {
  return crypto.randomUUID().replaceAll('-', '').slice(0, 12);
}

/**
 * A random, well-formed-but-INVALID JWT-shaped token. Runtime-generated (never
 * a literal — nothing to leak, nothing for the secrets guards to flag).
 *
 * FORMAT AUTHORITY (coupled): three dot-joined segments of UUID hex — a strict
 * subset of the RFC 7235 token68 charset that
 * `packages/sdk-core/src/validation/limits.ts:validateUserToken()` enforces, so
 * the SDK's local header-injection guard passes and the REJECTION decision is
 * made by the server (GC's `require_user_auth`), which is exactly what the
 * unauthenticated-join spec must prove.
 */
export function garbageToken68(): string {
  const segment = (): string => crypto.randomUUID().replaceAll('-', '');
  return `${segment()}.${segment()}.${segment()}`;
}

/**
 * Route-FULFILL the AC register exchange with a synthetic `RegisterResponse`
 * carrying `accessToken` (task #19 auth-rejection Test A). The request never
 * leaves the browser: AC is never hit, no registration budget is consumed, and
 * the demo ends up retaining exactly the token we hand it — the ONLY way to put
 * the UI into its authed shell holding a known-garbage credential (the auth nav
 * is hidden once a session exists, task #58 B3, so there is no UI path to an
 * invalid-token state).
 */
export async function fulfillAuthRegister(
  page: Page,
  creds: TestCredentials,
  accessToken: string,
): Promise<void> {
  const body: RegisterResponse = {
    accessToken,
    tokenType: 'Bearer',
    expiresIn: 3600,
    userId: crypto.randomUUID(),
    email: creds.email,
    displayName: creds.displayName,
  };
  await page.route(`**${REGISTER_PATH}`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(body),
    }),
  );
}

/**
 * Drive the sign-in view EXPECTING the exchange to be rejected; returns the AC
 * response's HTTP status for the caller to assert EXACTLY (401-exact
 * discriminates a credential rejection from a 429 rate-limit — @security, task
 * #19 Gate 1). Counterpart of `signInViaUi`, which asserts success.
 */
export async function signInExpectingRejection(
  page: Page,
  creds: TestCredentials,
): Promise<number> {
  await page.getByTestId('nav-signin').click();
  await fillAuthFields(page, creds);
  const response = await captureResponse(page, LOGIN_PATH, () =>
    page.getByTestId('signin-button').click(),
  );
  return response.status();
}

/**
 * Intercept GC's join response for `meetingCode` and rewrite ONLY `meetingId`
 * to a random UUID (task #19 mc-token-rejection): the SDK proceeds holding the
 * REAL meeting token and the REAL `mcAssignment`, but presents MC a
 * `meeting_id` that cannot match the token's claim — driving MC's step-6
 * binding check (`crates/mc-service/src/webtransport/connection.rs`) to its
 * `Unauthorized` reply. The token passes through the fulfilled body BY VALUE
 * and is never logged or interpolated anywhere (@semantic-guard item 8).
 *
 * ---------------------------------------------------------------------------
 * FAILURE CLASS: Node resolver vs browser resolver for `*.localhost`.
 * READ THIS BEFORE WRITING ANOTHER `route.fetch()` IN THIS HARNESS.
 *
 * `route.fetch()` does NOT run in the browser — it runs in the Playwright NODE
 * process, and by default replays the intercepted request's URL verbatim. That
 * URL carries the per-run org subdomain (`http://e2e-<hex>.localhost:5173/...`,
 * R-7). Chromium resolves any `*.localhost` label internally; Node does not, so
 * the verbatim replay dies with `getaddrinfo ENOTFOUND e2e-<hex>.localhost`.
 * Story R-7 task #3 Gate 2 is the worked example: 7 purely browser-side specs
 * passed and this one — the harness's only Node-side hop onto the page origin —
 * failed. `toLoopbackUrl` (env.ts owns the ONE hostname swap) is the fix, and
 * any new Node-side replay of a browser URL needs it too.
 *
 * ONLY the URL moves; the intercepted request's own headers ride along
 * unchanged, so `Authorization: Bearer <user token>` still reaches GC and the
 * join is still made as the real signed-in user.
 *
 * `Host` is deliberately NOT fixed up here, because on THIS path nothing reads
 * it: `vite.config.ts` proxies `/api/v1/meetings` with `changeOrigin: true`, so
 * the proxy rewrites Host to the GC target whatever arrives, and GC's
 * `join_meeting` resolves the org from the JWT's `org_id` claim
 * (`crates/gc-service/src/handlers/meetings.rs`), never from Host. The contrast
 * is load-bearing: `/api/v1/auth` is proxied `changeOrigin: false` precisely
 * because AC's ADR-0020 org extraction DOES read Host, so a Node-side hop onto
 * an AC path would have to carry the org-subdomain Host explicitly rather than
 * inheriting one from the rewritten URL.
 * ---------------------------------------------------------------------------
 */
export async function rewriteJoinResponseMeetingId(page: Page, meetingCode: string): Promise<void> {
  await page.route(`**${meetingPath(meetingCode)}`, async (route) => {
    const real = await route.fetch({ url: toLoopbackUrl(route.request().url()) });
    const body = (await real.json()) as Record<string, unknown>;
    await route.fulfill({
      response: real,
      body: JSON.stringify({ ...body, meetingId: crypto.randomUUID() }),
    });
  });
}

/**
 * Remove the {@link rewriteJoinResponseMeetingId} interception for `meetingCode`
 * (gap (3) mc-token-rejection recovery: the recovery join must run WITHOUT the
 * rewrite active). Same glob the rewrite installs — the real GC response flows
 * through untouched afterwards.
 */
export async function clearJoinResponseRewrite(page: Page, meetingCode: string): Promise<void> {
  await page.unroute(`**${meetingPath(meetingCode)}`);
}

/**
 * Assert NO `joined` bus event exists — a POINT-IN-TIME scan, deliberately not
 * a wait-for-absence (which would trade flake for coverage). Call ONLY after
 * the spec's terminal condition is established (error rendered / counter
 * moved), so "not joined yet" cannot masquerade as "did not join" (@test, task
 * #19 Gate 1).
 */
export async function expectNoJoinedEvent(page: Page, label: string): Promise<void> {
  const events = await busEvents(page);
  const joined = events.filter((e) => e['type'] === 'joined');
  expect(
    joined,
    `[${label}] a 'joined' bus event exists — the join SUCCEEDED where this spec requires rejection`,
  ).toEqual([]);
}

/**
 * Wait (bounded) for the demo's `last-error` surface and assert it renders the
 * TYPED code prefix — `errorText` renders `${err.code}: ${err.message}`, so the
 * leading `CODE:` pins the SdkError subclass family without depending on the
 * free-form message text. Returns the full rendered text for optional further
 * assertion. On absence, the failure names what the bus DID see.
 */
export async function expectLastErrorCode(
  page: Page,
  code: SdkErrorCode,
  timeoutMs = 15_000,
): Promise<string> {
  const lastError = page.getByTestId('last-error');
  try {
    await expect(lastError).toBeVisible({ timeout: timeoutMs });
  } catch {
    throw new Error(
      `no last-error rendered within ${timeoutMs}ms (expected the typed '${code}:' ` +
        `prefix). ${await busFailureContext(page)}`,
    );
  }
  const text = (await lastError.textContent())?.trim() ?? '';
  expect(
    text.startsWith(`${code}:`),
    `last-error must carry the typed '${code}:' prefix, got: "${text}"`,
  ).toBe(true);
  return text;
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
    // DELIBERATELY independent literals (NOT the module consts above): a test
    // asserting the ABSENCE of auth calls must not derive the forbidden prefix
    // from the same encoding the drivers use — see docs/TODO.md
    // §Cross-Service Duplication, #18 entry, nuance (b). Do not "clean up".
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

// ============================================================================
// Join-after-error recovery (gap (3), R-45 recovery tail)
// ============================================================================

/**
 * Recover from a failed/spent join by re-entering the join view and joining
 * `meetingCode` in the SAME page session (NO reload), asserting success. Returns
 * the recovery join's `joined` bus projection.
 *
 * `MeetingSession` is single-use (`JoinMeeting.svelte`): a spent join view cannot
 * join again — the view must REMOUNT to build a fresh session. So this navigates
 * to the create view (which unmounts `JoinMeeting` → `onDestroy` →
 * `session.disconnect()`) and WAITS for the create view to render before
 * re-entering join — a GATED remount, not back-to-back clicks that would race the
 * single-use teardown (fatal under retries=0). It then delegates to
 * {@link joinAsUser} (which owns nav-join + meeting-code fill — not re-encoded).
 *
 * Caller owns the spec-specific preamble (e.g. `clearJoinResponseRewrite`,
 * `bootstrapMeeting`, or re-authenticating a dropped session) BEFORE calling.
 */
export async function recoverByJoining(page: Page, meetingCode: string): Promise<JoinedBusEvent> {
  await page.getByTestId('nav-create').click();
  await expect(
    page.getByTestId('meeting-title'),
    'recovery: the create view must render (JoinMeeting unmounted) before re-entering join',
  ).toBeVisible();
  await joinAsUser(page, meetingCode);
  return waitForJoined(page);
}

// ============================================================================
// Media loopback (task #20, ADR-0036 §5/§6/§10)
// ============================================================================

/**
 * One sample of the SDK's monotone frame counters, as the bus projects it.
 *
 * The bus SAMPLES a counter rather than emitting per frame — `pipeline/egress.ts`
 * is the hot path under ADR-0036 §11, whose per-frame invariant is zero
 * allocation and zero registry lookup. So a "flat while muted" assertion is made
 * over a handful of readings, and the number of readings is itself asserted (see
 * {@link expectEgressFlatWhileMuted}) because flatness over zero samples is
 * vacuously true.
 */
export interface FrameCountSample {
  readonly framesSent: number;
  /** Datagrams that arrived, counted at the wire BEFORE any parse. */
  readonly framesReceived: number;
  /** Frames that verified, decrypted and reached the decoder. */
  readonly framesAccepted: number;
  /** Frames rejected for a wire reason. */
  readonly framesDropped: number;
  /** The most recent reject token; absent before the first drop. Bounded. */
  readonly lastDropReason: string | undefined;
  readonly atMs: number;
}

/** How long to wait for the first media frame to come back through MH. */
const FIRST_MEDIA_TIMEOUT_MS = 20_000;

/**
 * How long a muted window is observed before it is judged flat, and the minimum
 * number of samples that window must contain.
 *
 * The sample floor is the anti-vacuity control: without it, a stalled sampler
 * produces zero readings and "every reading is equal" passes, reporting a broken
 * harness as a working mute.
 */
const MUTE_OBSERVATION_MS = 2_500;
const MIN_MUTE_SAMPLES = 4;

/**
 * Frames already queued at the instant of mute may still drain — ADR-0036 §5
 * stops CAPTURE within one frame, which is a different claim from un-queueing
 * what the egress queue already holds. The flatness baseline is therefore taken
 * after this settle rather than at the mute itself; taking it at the mute would
 * make the assertion fail on correct behaviour.
 */
const MUTE_SETTLE_MS = 750;

/**
 * Render the receive-path accounting identity from the newest sample.
 *
 * `received = accepted + sum(drops by reason)`. Printing only `accepted` leaves
 * "nothing is arriving" and "arriving and being rejected" indistinguishable —
 * two states with OPPOSITE remediations (a relay that stopped forwarding vs a
 * client that cannot attribute the sender). An earlier version of these messages
 * stated that ambiguity rather than resolving it, and a reader with the source
 * open still misdiagnosed it. So the message now names the conclusion.
 */
function diagnoseCounters(samples: readonly FrameCountSample[]): string {
  const last = samples[samples.length - 1];
  if (last === undefined) {
    return (
      `NO frame-count samples on the bus at all (${samples.length}). That is a HARNESS ` +
      `condition, not a media one: either media was never started or the __E2E_HOOKS__ ` +
      `sampler is not running.`
    );
  }
  const counts =
    `sent=${last.framesSent} received=${last.framesReceived} ` +
    `accepted=${last.framesAccepted} dropped=${last.framesDropped}` +
    (last.lastDropReason !== undefined ? ` lastDropReason=${last.lastDropReason}` : '') +
    ` over ${samples.length} samples`;

  let reading: string;
  if (last.framesSent === 0) {
    reading =
      'READING: nothing left this client. The fault is upstream of the wire — capture, ' +
      'encode, or the send path. MH and the receive path are not implicated.';
  } else if (last.framesReceived === 0) {
    reading =
      'READING: frames left but NOTHING came back. The fault is between this client and ' +
      'MH — forwarding, the assignment, or the return path. The receive path is not ' +
      'implicated: it never saw a datagram.';
  } else if (last.framesAccepted === 0) {
    reading =
      'READING: datagrams ARE arriving and every one is being REJECTED. The fault is in ' +
      "this client's receive path, NOT in MH forwarding — see lastDropReason above for " +
      "which check failed. (A `no_roster_entry` here means the sender's identity key is " +
      'missing from the roster resolver; in a loopback the sender is this client itself.)';
  } else {
    reading = 'READING: media is flowing in both directions.';
  }
  return `${counts}. ${reading}`;
}

/** Read every frame-count sample the bus has recorded so far. */
export async function frameCountSamples(page: Page): Promise<readonly FrameCountSample[]> {
  const raw = await busEvents(page);
  return raw
    .filter((e) => e['type'] === 'mediaFrameCounts')
    .map((e) => ({
      framesSent: e['framesSent'] as number,
      framesReceived: e['framesReceived'] as number,
      framesAccepted: e['framesAccepted'] as number,
      framesDropped: e['framesDropped'] as number,
      lastDropReason: e['lastDropReason'] as string | undefined,
      atMs: e['atMs'] as number,
    }));
}

/**
 * Start the media pipeline from the in-meeting view.
 *
 * The microphone permission and the capture itself come from Chromium's
 * fake-device / fake-ui launch flags, already set in `playwright.config.ts`. No
 * flag is added here, and none that weakens certificate validation, web security
 * or origin trust appears anywhere in this suite — MC/MH trust flows exclusively
 * through `serverCertificateHashes` pinning, which such a flag would turn into
 * decoration while every assertion stayed green.
 */
export async function startAudio(page: Page): Promise<void> {
  await expect(
    page.getByTestId('in-meeting'),
    'the in-meeting view must render after a settled join',
  ).toBeVisible();
  await page.getByTestId('start-audio').click();
  await expect(
    page.getByTestId('mute-toggle'),
    'the mute control must appear once the media pipeline is running',
  ).toBeVisible();
}

/**
 * Wait for audio to come back through the media handler, and RETURN the observed
 * end-to-end latency.
 *
 * **This is the functional pass/fail of the loopback**: a `firstMediaFrame` event
 * means a frame this client captured, encoded, encrypted, signed and sent came
 * back from MH and completed verify -> replay -> unwrap -> decrypt -> decode.
 *
 * **The returned number is OBSERVED, NEVER GATED** (ADR-0036 §10). Nothing in
 * this suite compares it against a threshold; a wall-clock target on a local
 * cluster is a permanent flake, and ADR-0028 forbids quarantining gates, so the
 * test would end up deleted and the headline objective would have zero coverage.
 * The timeout below is a LIVENESS bound on the event existing at all — not a
 * latency budget, and it must not be tightened into one.
 */
export async function waitForFirstMediaFrame(
  page: Page,
  timeoutMs = FIRST_MEDIA_TIMEOUT_MS,
): Promise<number> {
  try {
    await page.waitForFunction(
      () => window.__darktower_test__?.events.some((e) => e['type'] === 'firstMediaFrame') ?? false,
      undefined,
      { timeout: timeoutMs },
    );
  } catch {
    const samples = await frameCountSamples(page);
    throw new Error(
      `no audio returned from the media handler: no 'firstMediaFrame' bus event within ` +
        `${timeoutMs}ms after start-audio. This is a LIVENESS failure (no media at all), not a ` +
        `latency failure — the bound is not a budget. ` +
        `${diagnoseCounters(samples)} ${await busFailureContext(page)}`,
    );
  }
  const events = await busEvents(page);
  const first = events.find((e) => e['type'] === 'firstMediaFrame');
  const elapsedMs = first?.['elapsedMs'];
  if (typeof elapsedMs !== 'number' || !Number.isFinite(elapsedMs) || elapsedMs < 0) {
    throw new Error(`malformed 'firstMediaFrame' bus event: ${JSON.stringify(first)}`);
  }
  return elapsedMs;
}

/** Click the mute control and wait for the SDK's echo to reach the DOM. */
export async function setMuteViaUi(page: Page, muted: boolean): Promise<number> {
  await page.getByTestId('mute-toggle').click();
  // The DOM assertion is on the SDK's ECHO, not on the click: the indicator is
  // driven by `muteChanged`, so waiting here proves the SDK actually applied it.
  await expect(
    page.getByTestId('mute-state'),
    `the mute indicator must follow the SDK's client-mute state (requested ${String(muted)})`,
  ).toHaveText(muted ? 'muted' : 'unmuted');
  const busState = await page.evaluate(() => {
    const events = window.__darktower_test__?.events ?? [];
    const mutes = events.filter((e) => e['type'] === 'muteState');
    return mutes[mutes.length - 1]?.['audioMuted'];
  });
  expect(
    busState,
    'the DOM indicator and the SDK client-mute state must agree — a UI that can disagree with ' +
      'what gates capture is the hot-mic defect ADR-0036 §5 exists to prevent',
  ).toBe(muted);
  return Date.now();
}

/**
 * Wait until one frame counter strictly exceeds its current value.
 *
 * ONE algorithm, parameterised by field — the two exported wrappers below differ
 * only in which counter they watch, their timeout, and their diagnosis. The
 * duplicated part worth collapsing is the PREDICATE: both encode "how to find
 * the newest frame-count sample on the bus", and a fix to that (skipping a
 * malformed sample, tolerating a sampler gap) applied to one copy and not the
 * other leaves two assertions silently testing different things.
 *
 * The callback is serialized into the page, so the field name travels in the
 * argument object rather than through a closure.
 */
async function waitForCounterAbove(
  page: Page,
  field: 'framesSent' | 'framesAccepted',
  timeoutMs: number,
  describeFailure: (baseline: number, after: readonly FrameCountSample[]) => string,
): Promise<void> {
  const before = await frameCountSamples(page);
  const baseline = before[before.length - 1]?.[field] ?? 0;
  try {
    await page.waitForFunction(
      (arg: { floor: number; field: string }) => {
        const events = window.__darktower_test__?.events ?? [];
        const samples = events.filter((e) => e['type'] === 'mediaFrameCounts');
        const last = samples[samples.length - 1];
        return last !== undefined && (last[arg.field] as number) > arg.floor;
      },
      { floor: baseline, field },
      { timeout: timeoutMs },
    );
  } catch {
    throw new Error(describeFailure(baseline, await frameCountSamples(page)));
  }
}

/**
 * Assert the egress counter STRICTLY ADVANCES over a short window.
 *
 * The positive control for everything else here. Without it, "flat while muted"
 * passes just as well on a pipeline that never sent a frame in its life, and the
 * whole spec becomes decorative.
 */
export async function expectEgressAdvances(
  page: Page,
  label: string,
  windowMs = 1_500,
): Promise<void> {
  await waitForCounterAbove(
    page,
    'framesSent',
    windowMs + 5_000,
    (baseline, after) =>
      `egress did not advance (${label}): framesSent stayed at ${baseline}. The client is not ` +
      `putting audio on the wire, so every mute assertion in this spec would have passed ` +
      `vacuously. ${diagnoseCounters(after)}`,
  );
}

/** Assert decoded audio is still arriving — the "unmute resumes audio" half. */
export async function expectIngressAdvances(page: Page, label: string): Promise<void> {
  await waitForCounterAbove(
    page,
    'framesAccepted',
    10_000,
    (baseline, after) =>
      `decoded audio did not resume (${label}): framesAccepted stayed at ${baseline}. ` +
      diagnoseCounters(after),
  );
}

/**
 * Assert the egress counter is FLAT for the whole muted window — the STRUCTURAL
 * proof that mute produces silence.
 *
 * Structural, not acoustic, and deliberately: sampling audio energy would prove
 * only that this client's playback went quiet, which is also what a broken
 * decoder looks like. A flat send counter proves no encoded audio LEFT THE
 * DEVICE, which is the property ADR-0036 §5 actually states ("under client mute,
 * no media leaves the device... enforced client-side at capture").
 *
 * @param mutedAtMs when mute was applied, from {@link setMuteViaUi}.
 */
export async function expectEgressFlatWhileMuted(page: Page, mutedAtMs: number): Promise<void> {
  await page.waitForTimeout(MUTE_SETTLE_MS + MUTE_OBSERVATION_MS);

  const settledAtMs = mutedAtMs + MUTE_SETTLE_MS;
  const samples = (await frameCountSamples(page)).filter((s) => s.atMs >= settledAtMs);

  // VACUITY CONTROL, asserted SEPARATELY and with its own message. "Every
  // sample is equal" is trivially true of zero samples, so a stalled sampler
  // would otherwise be reported as a working mute — the exact false green this
  // spec exists to make impossible.
  expect(
    samples.length,
    `the frame-count sampler produced only ${samples.length} sample(s) in the ` +
      `${MUTE_OBSERVATION_MS}ms muted window (needed >= ${MIN_MUTE_SAMPLES}). This is a HARNESS ` +
      `failure, not a mute failure: with no samples, flatness is vacuously true. Check that the ` +
      `__E2E_HOOKS__ bus sampler is running and that media was started.`,
  ).toBeGreaterThanOrEqual(MIN_MUTE_SAMPLES);

  const baseline = samples[0]!.framesSent;
  const moved = samples.filter((s) => s.framesSent !== baseline);
  expect(
    moved,
    `MUTE REGRESSION: framesSent moved ${baseline} -> ${samples[samples.length - 1]!.framesSent} ` +
      `across ${samples.length} samples spanning ` +
      `${samples[samples.length - 1]!.atMs - samples[0]!.atMs}ms while muted ` +
      `(mute applied at t=${mutedAtMs}, baseline taken after a ${MUTE_SETTLE_MS}ms queue-drain ` +
      `settle). Egress must be FLAT while muted: ADR-0036 §5 enforces client mute at capture, so ` +
      `no encoded audio should exist to leave the device. A counter that keeps advancing means ` +
      `the indicator is telling the user something the send path is not doing.`,
  ).toEqual([]);
}
