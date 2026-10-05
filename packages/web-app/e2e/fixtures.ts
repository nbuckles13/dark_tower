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
import type { Browser, BrowserContext, Page, Request, Response } from 'playwright/test';
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
import {
  AC_REGISTRATION_RATE_LIMIT_MAX_KEY,
  AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY,
  COHORT_ENV_VAR,
  cohortSize,
  decodeCohort,
  SUITE_RECEIVE_SLOTS,
} from './cohort.js';
import { E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS } from '../src/lib/e2eBus.js';
import { scanRecordedRequests, type RecordedRequest, type ScanReport } from './credentialScan.js';
import { windowStep } from './windowSampling.js';
import { e2eEnv, toLoopbackUrl } from './env.js';
import type { InstanceCounters } from './instanceCounters.js';
import { diagnoseMissingSender } from './mcMetrics.js';
import {
  evaluateSenderLane,
  type BusLaneAnalysis,
  type BusSlotAssignment,
  type CohortTone,
  type ToneVerdict,
} from './toneDetector.js';

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
 * Fresh throwaway credentials. AC rate-limits REGISTRATION ATTEMPTS per source
 * IP — successful and failed registrations count, sign-ins do not (see
 * `./cohort`'s header) —
 * with the Kind limit's SSoT in `infra/services/ac-service/config.env`
 * (`AC_REGISTRATION_RATE_LIMIT_*`); the suite's budget in that unit is tracked
 * in e2e/README.md §Budgets.
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
 * The suite's ONE shared valid user for single-party and two-party specs (see
 * e2e/README.md §Budgets). Registered once per worker process and reused via
 * `signInViaUi` — 0 further REGISTRATIONS; a sign-in spends no registration
 * budget (AC's per-IP limit counts registration attempts only).
 *
 * BUDGET ↔ workers=1 COUPLING (do not decouple silently): "registered once"
 * rests on `workers: 1` (playwright.config.ts) — a single worker process makes
 * the module-level `sharedRegistration` memo below a per-run singleton ON A
 * GREEN RUN. Playwright restarts the worker after a failing test, which costs
 * one re-registration of V per failure; raising `workers` would re-register V
 * per worker. The multi-party COHORT does not ride this memo for exactly that
 * reason: it is registered once per run in `global-setup.ts` (see `./cohort`).
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
// The N+1 multi-party cohort (story 2 R-30) — see ./cohort for the budget
// ============================================================================

/**
 * Register the suite's N+1 distinct accounts through the real sign-up UI, ONE
 * context each (the app's auth session is per-context in-memory state). Called
 * ONCE per run by `global-setup.ts`, never by a spec. Cost: N+1 successful token
 * issues; a 429 fails loudly via {@link captureAccessToken}.
 */
export async function registerCohort(
  browser: Browser,
  receiveSlots: number,
): Promise<TestCredentials[]> {
  const cohort: TestCredentials[] = [];
  for (let i = 0; i < cohortSize(receiveSlots); i += 1) {
    const creds = randomCredentials(`cohort-${String.fromCharCode(65 + i)}`);
    const context = await browser.newContext({ baseURL: e2eEnv.baseUrl });
    try {
      const page = await context.newPage();
      await page.goto('/');
      await signUpViaUi(page, creds);
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      throw new Error(`cohort member ${i} registration failed: ${detail}`, { cause: err });
    } finally {
      await context.close();
    }
    cohort.push(creds);
  }
  return cohort;
}

/** The run's cohort (N+1 = `cohortSize(SUITE_RECEIVE_SLOTS)` distinct accounts). */
export function suiteCohort(): TestCredentials[] {
  return decodeCohort(process.env[COHORT_ENV_VAR], cohortSize(SUITE_RECEIVE_SLOTS));
}

/**
 * Sign `page` in as cohort member `index` (0-based). 0 registrations, so no
 * registration budget spent (a sign-in never counts). Returns the access token.
 */
export async function authAsCohortMember(page: Page, index: number): Promise<string> {
  const cohort = suiteCohort();
  const creds = cohort[index];
  if (creds === undefined) {
    throw new RangeError(`cohort has ${cohort.length} members; no member ${index}`);
  }
  return signInViaUi(page, creds);
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
  if (response.status() === 429) {
    // Named, never retried: the bucket is exhausted, and a retry loop would
    // only spend more of it (and hide a budget regression behind a slow pass).
    throw new Error(
      `auth exchange ${path} was RATE-LIMITED (HTTP 429): AC's per-source-IP bucket of ` +
        `registration attempts (successful AND failed registrations) is exhausted. Limit SSoT: ` +
        `infra/services/ac-service/config.env ${AC_REGISTRATION_RATE_LIMIT_MAX_KEY} / ` +
        `${AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY}. The bucket is shared with the Rust env-tests run from ` +
        `this host just before this suite and with every other spec's registrations — see ` +
        `e2e/README.md §Budgets. Do not retry; wait out the window or fix the budget.`,
    );
  }
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
  // The row also carries mute/reachability indicators and (for a host) controls,
  // so the exact-name check reads the row's NAME element, not the whole row.
  await expect(
    page.getByTestId(`participant-name-${participantId}`),
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
 * join path puts no credential on WT either: join is token-based end-to-end).
 * The type's home is `./credentialScan`, the pure scan. */
export type { RecordedRequest } from './credentialScan.js';

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
 * The token-only invariant over one context's join window — see
 * `./credentialScan` for the five properties (the scan is the pure half, so a
 * node-tier test proves every failure class). The window MUST contain a forced
 * telemetry export (`flushTelemetry(page)` before `recorder.stop()`): property 5
 * is unconditional in this suite, which asserts telemetry is configured
 * (`expectBuildKnobs`).
 *
 * Returns the per-surface counts so a caller can see what the checks ran over.
 */
export function assertTokenOnlyJoinTraffic(
  records: readonly RecordedRequest[],
  creds: TestCredentials,
  label: string,
): ScanReport['surfaces'] {
  const report = scanRecordedRequests(records, creds);
  expect(
    report.findings.map((f) => `${f.reason}: ${f.detail}`),
    `[${label}] token-only join traffic violations (reason: detail — already redacted)`,
  ).toEqual([]);
  // Belt and braces on the positive control, stated as counts.
  expect(report.surfaces.telemetry.scanned, `[${label}] telemetry-not-recorded`).toBeGreaterThan(0);
  expect(report.surfaces.telemetry.bearerChecked, `[${label}] telemetry-not-bearer-checked`).toBe(
    report.surfaces.telemetry.scanned,
  );
  return report.surfaces;
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
// Media path (story 1 task #20; story 2 R-3 removed loopback — ADR-0036 §5/§6/§10)
//
// Callers: the multi-party specs (story 2 task 15) — `multi-party-hear.spec.ts`
// (first media, observed), `server-mute.spec.ts` (client structural mute, the
// egress/ingress advance checks) — and `solo-participant.spec.ts` (flat window).
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
export const FLAT_WINDOW_OBSERVE_MS = 2_500;
export const MIN_FLAT_WINDOW_SAMPLES = 4;

/**
 * The upper bound on a flat/advance window, as a multiple of its span. Both
 * windows (`receiveEvidence.ts:observeWindow` and
 * {@link expectCountersFlatOverWindow}) stop only once they have spanned their
 * observation window AND reached {@link MIN_FLAT_WINDOW_SAMPLES} (stop rule:
 * `windowSampling.ts:windowStep`); slow reads or a slipping in-page sampler under
 * load extend them, up to `factor x span`, where they FAIL as a harness error
 * rather than ever lowering the floor. Four: room for reads several times slower
 * than the sample interval. The bound is evaluated between reads, and in
 * `observeWindow` each read is also raced against it; an in-page history read
 * that never returns is caught by the Playwright test timeout, not by this bound.
 */
export const FLAT_WINDOW_MAX_OBSERVE_FACTOR = 4;
/** The bound at the default span — for docs and the default path. */
export const FLAT_WINDOW_MAX_OBSERVE_MS = FLAT_WINDOW_MAX_OBSERVE_FACTOR * FLAT_WINDOW_OBSERVE_MS;

/**
 * Frames already queued at the instant of mute may still drain — ADR-0036 §5
 * stops CAPTURE within one frame, which is a different claim from un-queueing
 * what the egress queue already holds. The flatness baseline is therefore taken
 * after this settle rather than at the mute itself; taking it at the mute would
 * make the assertion fail on correct behaviour.
 */
export const FLAT_WINDOW_SETTLE_MS = 750;

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
      'missing from the roster resolver.)';
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
 * Wait for a peer's audio to arrive through the media handler, and RETURN the
 * observed end-to-end latency.
 *
 * A `firstMediaFrame` event means a frame a peer captured, encoded, encrypted,
 * signed and sent arrived via MH and completed verify -> replay -> unwrap ->
 * decrypt -> decode. Since story 2 R-3 a client never receives its own audio,
 * so this needs a peer that SHARES A
 * CONNECTED HANDLER with this client — ADR-0036 §9's visibility rule. With every
 * participant connected to every handler that is any peer; it stops being any
 * peer only under partial connectivity, where a peer sharing none is carried in
 * `unreachable_sender_ids` and is not a candidate for this event at all.)
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
 * Assert that the chosen frame counters stay FLAT over an observation window
 * that starts `settleMs` after `fromMs`.
 *
 * The shared home for "nothing moved" on the frame-count sampler, used by the
 * mute assertion and by the solo-participant assertion. Its vacuity control is
 * asserted separately and FIRST: flatness is trivially true of zero samples, so
 * a stalled sampler must be reported as a HARNESS failure, never as a pass.
 */
export async function expectCountersFlatOverWindow(
  page: Page,
  opts: {
    readonly fromMs: number;
    readonly fields: readonly ('framesSent' | 'framesAccepted')[];
    readonly whyFlat: string;
    readonly settleMs?: number;
  },
): Promise<void> {
  const settleMs = opts.settleMs ?? FLAT_WINDOW_SETTLE_MS;
  await page.waitForTimeout(settleMs + FLAT_WINDOW_OBSERVE_MS);

  const settledAtMs = opts.fromMs + settleMs;
  const settled = async () => (await frameCountSamples(page)).filter((s) => s.atMs >= settledAtMs);
  let samples = await settled();
  // The same stop rule as `observeWindow` (`windowSampling.ts:windowStep`): the
  // in-page 250 ms timer slips under CPU load, so a correct mute can reach the
  // end of the span short of the floor. Keep reading the timestamped history
  // until the floor is met or the bound is reached; on overrun the HARNESS
  // expect below is the loud failure. The floor is never lowered.
  const extraFrom = Date.now();
  while (
    windowStep({
      samples: samples.length,
      elapsedMs: FLAT_WINDOW_OBSERVE_MS + (Date.now() - extraFrom),
      windowMs: FLAT_WINDOW_OBSERVE_MS,
      minSamples: MIN_FLAT_WINDOW_SAMPLES,
      maxMs: FLAT_WINDOW_MAX_OBSERVE_MS,
    }) === 'continue'
  ) {
    await page.waitForTimeout(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
    samples = await settled();
  }

  expect(
    samples.length,
    `HARNESS: the frame-count window reached its hard bound of ${FLAT_WINDOW_MAX_OBSERVE_MS}ms ` +
      `(span ${FLAT_WINDOW_OBSERVE_MS}ms, extended while short of the floor) with only ` +
      `${samples.length} sample(s) (needed >= ${MIN_FLAT_WINDOW_SAMPLES}). This is a ` +
      `HARNESS failure, not a media failure: with no samples, flatness is vacuously true. Check ` +
      `that the __E2E_HOOKS__ bus sampler is running and that media was started.`,
  ).toBeGreaterThanOrEqual(MIN_FLAT_WINDOW_SAMPLES);

  for (const field of opts.fields) {
    const baseline = samples[0]![field];
    const moved = samples.filter((s) => s[field] !== baseline);
    expect(
      moved,
      `${field} moved ${baseline} -> ${samples[samples.length - 1]![field]} across ` +
        `${samples.length} samples spanning ` +
        `${samples[samples.length - 1]!.atMs - samples[0]!.atMs}ms (window from t=${opts.fromMs}, ` +
        `baseline taken after a ${settleMs}ms settle). It must be FLAT: ${opts.whyFlat}`,
    ).toEqual([]);
  }
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
  await expectCountersFlatOverWindow(page, {
    fromMs: mutedAtMs,
    fields: ['framesSent'],
    whyFlat:
      'MUTE REGRESSION — ADR-0036 §5 enforces client mute at capture, so a counter that keeps ' +
      'advancing means the indicator is telling the user something the send path is not doing.',
  });
}

// ============================================================================
// Layer 3 (story 2 R-30): own tone, lane analyses, two-sided hear assertion
// ============================================================================

/**
 * The latest event of EACH of `types`, read in ONE `page.evaluate` that scans
 * the replay buffer from the end and returns only those events — so a
 * multi-type read is one consistent snapshot (the sampler cannot tick between
 * reads), and a poll never serialises the whole cumulative buffer, which grows
 * a full-spectrum `receiveAnalysis` record every sampler tick.
 */
export async function latestBusEventsOf<T extends string>(
  page: Page,
  types: readonly T[],
): Promise<Record<T, Readonly<Record<string, unknown>> | undefined>> {
  return page.evaluate(
    (wanted) => {
      const found: Record<string, unknown> = {};
      const events = window.__darktower_test__?.events ?? [];
      for (let i = events.length - 1; i >= 0 && Object.keys(found).length < wanted.length; i -= 1) {
        const event = events[i];
        const type = event?.['type'];
        if (typeof type === 'string' && wanted.includes(type) && !(type in found)) {
          found[type] = event;
        }
      }
      return found;
    },
    types as readonly string[],
  ) as Promise<Record<T, Readonly<Record<string, unknown>> | undefined>>;
}

/**
 * Assert this participant's build declares the suite's N
 * (`SUITE_RECEIVE_SLOTS`) and MC's advertised cap admits it — the runtime check
 * `src/lib/config.ts` defers to the suite. `playwright.config.ts` derives the
 * dev server's `VITE_DT_RECEIVE_SLOTS` from `SUITE_RECEIVE_SLOTS`, but with
 * `reuseExistingServer` a server started elsewhere (e.g. `dev-web.sh`, whose
 * demo N may differ) runs its own N; a mismatch would otherwise surface as
 * `not_assigned` / fewer-sources — the misrouting-shaped symptom S1 separates.
 */
export async function expectDeclaredReceiveSlots(
  page: Page,
  label: string,
  expectedSlots: number = SUITE_RECEIVE_SLOTS,
): Promise<void> {
  await expect
    .poll(
      async () => (await latestBusEventsOf(page, ['receiveSlots'])).receiveSlots !== undefined,
      {
        message: `${label}: no receiveSlots bus event — the join never completed`,
        timeout: 15_000,
      },
    )
    .toBe(true);
  const { receiveSlots: slots } = await latestBusEventsOf(page, ['receiveSlots']);
  expect(
    slots?.['declared'],
    `${label}: the build declares N=${String(slots?.['declared'])}, the scenario needs ` +
      `N=${expectedSlots} (suite default SUITE_RECEIVE_SLOTS=${SUITE_RECEIVE_SLOTS}; a per-context ` +
      `N comes from the receiveSlots test lever). VITE_DT_RECEIVE_SLOTS is fixed when the dev ` +
      `server starts; with reuseExistingServer a server started elsewhere keeps its own N — ` +
      `stop it and let Playwright start one.`,
  ).toBe(expectedSlots);
  expect(slots?.['serverCapState'], `${label}: MC advertised no receive-slot cap`).toBe('known');
  expect(
    slots?.['serverCap'],
    `${label}: MC's cap (MC_MAX_RECEIVE_SLOTS) is below the scenario's N=${expectedSlots}`,
  ).toBeGreaterThanOrEqual(expectedSlots);
}

/**
 * The build knobs the multi-party suite needs, asserted BEFORE any media
 * assertion (@operations B, @observability, @test G1): the per-context test
 * levers (`DT_TEST_LEVERS=1`) and SDK telemetry (`VITE_TELEMETRY_ENDPOINT`). All
 * three knobs — these two and `DT_TEST_TONE=1` ({@link readOwnTone}) — reach
 * only a dev server Playwright STARTS (`playwright.config.ts` webServer env); a
 * reused one keeps whatever it was started with, so the failure names that cause
 * instead of surfacing later as "C never heard A".
 */
export async function expectBuildKnobs(page: Page, label: string): Promise<void> {
  const { buildKnobs } = await latestBusEventsOf(page, ['buildKnobs']);
  const reused =
    'The dev server was reused without it (reuseExistingServer): stop it and let Playwright ' +
    'start `pnpm dev` with the webServer env in playwright.config.ts.';
  expect(
    buildKnobs,
    `${label}: no buildKnobs bus event — the E2E bus is not installed`,
  ).toBeDefined();
  expect(
    buildKnobs?.['testLevers'],
    `${label}: build-knob-missing: DT_TEST_LEVERS=1 is not set on this dev server. ${reused}`,
  ).toBe(true);
  expect(
    buildKnobs?.['telemetryConfigured'],
    `${label}: build-knob-missing: VITE_TELEMETRY_ENDPOINT is not set on this dev server, so no ` +
      `telemetry exports and the credential-scan positive control would observe nothing. ${reused}`,
  ).toBe(true);
  expect(
    buildKnobs?.['telemetrySinkActive'],
    `${label}: build-knob-missing: the endpoint is set but the SDK holds no telemetry provider ` +
      `(configureTelemetryIfEnabled was skipped or failed), so a forced flush would export nothing.`,
  ).toBe(true);
}

/**
 * Force a metric export NOW through the E2E bus (the SDK's `flushMetrics`) and
 * wait for it. Rejects `telemetry_not_configured` when telemetry is off — never a
 * silent no-op. Used inside every join-recording window (the credential-scan
 * positive control) and before every client read-back.
 */
export async function flushTelemetry(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const bus = window.__darktower_test__;
    if (bus === undefined) throw new Error('E2E bus absent: cannot flush telemetry');
    await bus.flushMetrics();
  });
}

/**
 * This participant's identity and ANNOUNCED tone — the expected tone every other
 * receiver checks for. Read from the participant's own bus (`joined.senderId`,
 * `captureSource.toneHz`), never derived: the detector shares no code with the
 * SDK's tone derivation. ASSERTS tone mode rather than assuming the dev server
 * was started with `DT_TEST_TONE=1` (`reuseExistingServer` may reuse one that
 * was not). Call after {@link startAudio}; waits for the one-shot event. Also
 * asserts the build's declared N first ({@link expectDeclaredReceiveSlots}), so
 * every multi-party participant passes through both build-knob checks.
 */
export async function readOwnTone(
  page: Page,
  label: string,
  expectedSlots: number = SUITE_RECEIVE_SLOTS,
): Promise<CohortTone> {
  await expectBuildKnobs(page, label);
  await expectDeclaredReceiveSlots(page, label, expectedSlots);
  await expect
    .poll(
      async () => (await latestBusEventsOf(page, ['captureSource'])).captureSource !== undefined,
      { message: `${label}: no captureSource bus event — media never started`, timeout: 15_000 },
    )
    .toBe(true);
  const { captureSource: source, joined } = await latestBusEventsOf(page, [
    'captureSource',
    'joined',
  ]);
  expect(
    source?.['mode'],
    `${label}: capture source is not the test tone — start the dev server with DT_TEST_TONE=1 ` +
      `(a reused server may have been started without it)`,
  ).toBe('test_tone');
  const senderId = joined?.['senderId'];
  const toneHz = source?.['toneHz'];
  if (typeof senderId !== 'string' || typeof toneHz !== 'number') {
    throw new Error(
      `${label}: bus lacks its own senderId/toneHz (joined.senderId=${String(senderId)}, ` +
        `captureSource.toneHz=${String(toneHz)})`,
    );
  }
  return { label, senderId, toneHz };
}

/** The receiver's latest `slotAssignments` view and `receiveAnalysis` lanes, one snapshot. */
export async function latestReceiveView(page: Page): Promise<{
  readonly assignments: readonly BusSlotAssignment[];
  readonly lanes: readonly BusLaneAnalysis[];
}> {
  const latest = await latestBusEventsOf(page, ['slotAssignments', 'receiveAnalysis']);
  const assignments = latest.slotAssignments?.['assignments'];
  const lanes = latest.receiveAnalysis?.['lanes'];
  return {
    assignments: Array.isArray(assignments) ? (assignments as BusSlotAssignment[]) : [],
    lanes: Array.isArray(lanes) ? (lanes as BusLaneAnalysis[]) : [],
  };
}

/** Liveness bound for a lane to fill an FFT window and settle. Not a latency gate. */
const HEAR_SENDER_TIMEOUT_MS = 30_000;

/** Verdicts meaning "the sender is not reaching this receiver at all". */
const MISSING_SENDER_VERDICTS: ReadonlySet<ToneVerdict['kind']> = new Set([
  'not_assigned',
  'lane_missing',
  'silent',
]);

/**
 * Two-sided layer-3 assertion: `receiver` (whose `page` this is) hears `sender`'s
 * tone as the dominant tone of the sender's lane, the sender holds an ACTIVE slot
 * in the receiver's latest assignment, and NO other cohort tone (the receiver's
 * own included) is present in that lane. Polls until the verdict is `ok` — the
 * lane needs a full FFT window of decoded audio first — and on timeout fails
 * with the LAST verdict, which names the failure class (not_assigned /
 * lane_missing / not_ready / silent / expected_absent / foreign_present).
 *
 * S1 DIAGNOSTIC: pass `admissionBaseline` — `mhAdmissionRejectionsByInstance()`
 * taken BEFORE the scenario's joins (it cannot be taken after a failure). On a
 * missing-sender verdict the failure then carries `diagnoseMissingSender`'s
 * report (budget rejection / misrouting / unobservable); without one it says
 * the diagnostic was unavailable. The timeout is a liveness bound, never a
 * latency gate.
 */
export async function expectHearsSender(
  page: Page,
  receiver: CohortTone,
  sender: CohortTone,
  cohort: readonly CohortTone[],
  {
    timeoutMs = HEAR_SENDER_TIMEOUT_MS,
    admissionBaseline,
  }: { timeoutMs?: number; admissionBaseline?: InstanceCounters } = {},
): Promise<void> {
  // A plain loop, not `expect.poll`: a ToneHarnessError / ToneCollisionError
  // from the detector must fail AT ONCE, not be retried until the timeout.
  const deadline = Date.now() + timeoutMs;
  let last: ToneVerdict;
  for (;;) {
    const view = await latestReceiveView(page);
    last = evaluateSenderLane({ receiver, sender, cohort, ...view });
    if (last.kind === 'ok') return;
    if (Date.now() > deadline) break;
    // The bus's own sampling cadence: reading faster only sees the same
    // `receiveAnalysis` record twice.
    await page.waitForTimeout(E2E_FRAME_COUNT_SAMPLE_INTERVAL_MS);
  }
  let diagnosis = '';
  if (MISSING_SENDER_VERDICTS.has(last.kind)) {
    diagnosis =
      admissionBaseline === undefined
        ? ' — missing sender; no MH admission baseline was taken, so the S1 diagnostic is ' +
          'unavailable (pass admissionBaseline from mhAdmissionRejectionsByInstance() before the joins).'
        : ` — ${(await diagnoseMissingSender(admissionBaseline, `${receiver.label} missing ${sender.label}`)).report}`;
  }
  throw new Error(`layer 3 verdict ${last.kind} after ${timeoutMs}ms: ${last.detail}${diagnosis}`);
}
