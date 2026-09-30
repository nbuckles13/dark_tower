// File: packages/web-app/e2e/cohortContexts.ts
//
// Multi-party browsing contexts for the story-2 scenarios (S1, S2, S3, S6, S10a):
// one context per cohort member, each with its own join window recorded and
// scanned, and a GRACEFUL leave before every close.
//
// FIXTURE DISCIPLINE (@test F1-F6, @operations 3/4):
//   * a fresh meeting per test, created with the HOST member's own token (GC
//     makes the meeting's creator host — `crates/gc-service/src/handlers/
//     meetings.rs`), never shared across tests;
//   * every context left GRACEFULLY — the demo's own teardown (`nav-create`
//     unmounts the join view -> `session.disconnect()` -> the MH transports and
//     then signalling close cleanly) — and then closed, in a `finally`, on
//     failure too. That is the user's path, and it drops the participant's MH
//     connections (so its edges) at once. RESIDUAL, recorded in main.md and
//     docs/TODO.md: MC today classifies this clean close as `server_initiated`
//     and still holds the participant through its disconnect grace (30 s) before
//     removing it from the roster, so a previous meeting's participant can
//     outlive the test that made it; the S1 diagnostic's budget verdict is the
//     backstop if that ever reaches MH admission. Becomes a no-op once MC
//     classifies the close as `ClientClosed`;
//   * peak concurrent contexts = the cohort size (4 at N=3), never stacked: a
//     test closes its own before the next opens;
//   * per-context test levers (`src/lib/testLevers.ts`) arrive by init script,
//     before any page script runs.

import { expect, type Browser, type BrowserContext, type Page } from 'playwright/test';
import {
  assertTokenOnlyJoinTraffic,
  authAsCohortMember,
  flushTelemetry,
  joinAsUser,
  readOwnTone,
  recordRequests,
  startAudio,
  suiteCohort,
  waitForAllMediaConnected,
  waitForJoined,
  type JoinedBusEvent,
} from './fixtures.js';
import type { CohortTone } from './toneDetector.js';
import { SUITE_RECEIVE_SLOTS } from './cohort.js';

/** The per-context levers, as the app's `parseTestLevers` accepts them. */
export interface ContextLevers {
  readonly blockHandlers?: readonly string[];
  readonly receiveSlots?: string;
  readonly forceHostControls?: boolean;
}

/** One cohort member's browsing context. */
export interface Member {
  readonly label: string;
  readonly index: number;
  readonly context: BrowserContext;
  readonly page: Page;
  /** The member's AC access token (Node-side, for `bootstrapMeeting` by the host). */
  readonly token: string;
}

/** A member after a completed join + media start. */
export interface JoinedMember extends Member {
  readonly joined: JoinedBusEvent;
  readonly tone: CohortTone;
}

/** Labels by cohort index: A, B, C, ... */
export function memberLabel(index: number): string {
  return String.fromCharCode(65 + index);
}

/**
 * Open a context for cohort member `index`, install its levers, sign it in.
 * One successful AC token issue; zero registrations.
 */
export async function openMember(
  browser: Browser,
  index: number,
  levers?: ContextLevers,
): Promise<Member> {
  const context = await browser.newContext();
  if (levers !== undefined) {
    await context.addInitScript((value) => {
      (window as { __dt_test_levers__?: unknown }).__dt_test_levers__ = value;
    }, levers);
  }
  const page = await context.newPage();
  await page.goto('/');
  const token = await authAsCohortMember(page, index);
  return { label: memberLabel(index), index, context, page, token };
}

/**
 * Join `meetingCode`, start media, and check the build knobs, N and the member's
 * own tone — all inside ONE recorded window that ends with a FORCED telemetry
 * export and the token-only scan over it (unconditional; `./credentialScan`).
 *
 * `expectAllHandlers` (default true): every offered handler connects. A context
 * with a `blockHandlers` lever passes `false` and asserts its own connected set.
 */
export async function joinMember(
  member: Member,
  meetingCode: string,
  {
    expectAllHandlers = true,
    expectedSlots = SUITE_RECEIVE_SLOTS,
  }: { expectAllHandlers?: boolean; expectedSlots?: number } = {},
): Promise<JoinedMember> {
  const creds = suiteCohort()[member.index];
  if (creds === undefined) throw new RangeError(`no cohort member ${member.index}`);
  const recorder = recordRequests(member.context);
  let stopped = false;
  try {
    await joinAsUser(member.page, meetingCode);
    const joined = await waitForJoined(member.page);
    expect(
      joined.mediaServers.length,
      `${member.label}: media_servers must not be empty`,
    ).toBeGreaterThan(0);
    if (expectAllHandlers) await waitForAllMediaConnected(member.page, joined.mediaServers);
    await startAudio(member.page);
    const tone = await readOwnTone(member.page, member.label, expectedSlots);
    // The positive control's forced export, INSIDE the window.
    await flushTelemetry(member.page);
    stopped = true;
    assertTokenOnlyJoinTraffic(recorder.stop(), creds, `${member.label}/join`);
    return { ...member, joined, tone };
  } finally {
    if (!stopped) recorder.stop();
  }
}

/**
 * Leave the meeting the way a user does — the demo's own teardown — and wait
 * for the SDK to report it disconnecting, so the clean close is on its way
 * before the context goes. A page that never joined just navigates; a page that
 * joined and fails to tear down THROWS.
 */
export async function leaveGracefully(page: Page): Promise<void> {
  if (page.isClosed()) return;
  // Only a page that JOINED has a session to tear down. Decided from the bus
  // before leaving, so a joined page whose teardown then fails is LOUD, never
  // tolerated as "nothing to disconnect" (it would leak into the next test).
  const joined = await page.evaluate(
    () => window.__darktower_test__?.events.some((e) => e['type'] === 'joined') ?? false,
  );
  await page.getByTestId('nav-create').click({ timeout: 5_000 });
  if (!joined) return;
  try {
    await page.waitForFunction(
      () =>
        window.__darktower_test__?.events.some(
          (e) => e['type'] === 'stateChange' && e['state'] === 'disconnecting',
        ) ?? false,
      undefined,
      { timeout: 5_000 },
    );
  } catch {
    throw new Error(
      'graceful leave: the page joined but the SDK never reported disconnecting within 5 s ' +
        'after the join view unmounted — its session may still be live',
    );
  }
}

/**
 * Leave and close every member, collecting (not short-circuiting on) failures,
 * so one broken page never leaks the others' sessions into the next test.
 */
export async function closeMembers(members: ReadonlyArray<Member | undefined>): Promise<void> {
  const failures: string[] = [];
  for (const member of members) {
    if (member === undefined) continue;
    try {
      await leaveGracefully(member.page);
    } catch (err) {
      failures.push(`${member.label} leave: ${err instanceof Error ? err.message : String(err)}`);
    }
    await member.context.close().catch((err: unknown) => {
      failures.push(`${member.label} close: ${String(err)}`);
    });
  }
  if (failures.length > 0) {
    // Loud, never swallowed. Logged as well as thrown: thrown from a `finally`
    // it would replace a test-body failure, and the log keeps both visible.
    const text = `teardown failed: ${failures.join('; ')}`;
    console.error(`[e2e] ${text}`);
    throw new Error(text);
  }
}

/** The cohort's announced tones, for the two-sided detector. */
export function tonesOf(members: readonly JoinedMember[]): CohortTone[] {
  return members.map((m) => m.tone);
}
