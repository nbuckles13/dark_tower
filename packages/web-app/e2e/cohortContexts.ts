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
  expectBuildKnobs,
  expectDeclaredReceiveSlots,
  flushTelemetry,
  joinAsUser,
  joinWithPublishViaUi,
  latestBusEventsOf,
  readOwnTone,
  recordRequests,
  startMedia,
  suiteCohort,
  waitForAllMediaConnected,
  waitForJoined,
  type JoinedBusEvent,
} from './fixtures.js';
import type { CohortTone } from './toneDetector.js';
import {
  SUITE_AUDIO_RECEIVE_SLOTS,
  leversForPlan,
  type CohortMemberPlan,
  type PublishSet,
} from './cohort.js';

/** The per-context levers, as the app's `parseTestLevers` accepts them. */
export interface ContextLevers {
  readonly blockHandlers?: readonly string[];
  /** audio N, the `VITE_DT_AUDIO_RECEIVE_SLOTS` form */
  readonly audioReceiveSlots?: string;
  readonly forceHostControls?: boolean;
  /**
   * Story 3: the member's publish set (`cohort.ts:leversForPlan`). TYPE ONLY
   * until the client's publish-path task (C-A2) teaches the app's strict lever
   * parser this key. No spec passes it before then, so the parser's unknown-key
   * throw is never reached.
   */
  readonly publish?: PublishSet;
  /** video M, the `VITE_DT_VIDEO_RECEIVE_SLOTS` form (parsed by the app since C-A1). */
  readonly videoReceiveSlots?: string;
  /** Story 3 C-C1: capture from the REAL (fake-device) camera instead of the test pattern. */
  readonly forceCameraCapture?: boolean;
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
 * Zero registrations, so no registration budget spent (a sign-in never counts).
 */
export async function openMember(
  browser: Browser,
  index: number,
  levers?: ContextLevers,
  /** Test-owned init scripts installed before any page script (e.g. `installVideoTrackRecorder`). */
  beforeNavigate?: (context: BrowserContext) => Promise<void>,
): Promise<Member> {
  const context = await browser.newContext();
  if (beforeNavigate !== undefined) await beforeNavigate(context);
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
    expectedSlots = SUITE_AUDIO_RECEIVE_SLOTS,
  }: { expectAllHandlers?: boolean; expectedSlots?: number } = {},
): Promise<JoinedMember> {
  const { joined, result: tone } = await recordedJoin(member, {
    join: (page) => joinAsUser(page, meetingCode),
    expectAllHandlers,
    inside: () => readOwnTone(member.page, member.label, expectedSlots),
  });
  return { ...member, joined, tone };
}

/**
 * THE ONE recorded join window (both join helpers delegate here): the token-only
 * credential scan's recording spans the join, the handler connections, media
 * start, the caller's in-window checks (`inside`) and a FORCED telemetry export
 * (the scan's positive control), and is stopped and scanned on every path.
 */
async function recordedJoin<T>(
  member: Member,
  {
    join,
    expectAllHandlers,
    inside,
  }: {
    join: (page: Member['page']) => Promise<void>;
    expectAllHandlers: boolean;
    inside: (joined: JoinedBusEvent) => Promise<T>;
  },
): Promise<{ joined: JoinedBusEvent; result: T }> {
  const creds = suiteCohort()[member.index];
  if (creds === undefined) throw new RangeError(`no cohort member ${member.index}`);
  const recorder = recordRequests(member.context);
  let stopped = false;
  try {
    await join(member.page);
    const joined = await waitForJoined(member.page);
    expect(
      joined.mediaServers.length,
      `${member.label}: media_servers must not be empty`,
    ).toBeGreaterThan(0);
    if (expectAllHandlers) await waitForAllMediaConnected(member.page, joined.mediaServers);
    await startMedia(member.page);
    const result = await inside(joined);
    // The positive control's forced export, INSIDE the window.
    await flushTelemetry(member.page);
    stopped = true;
    assertTokenOnlyJoinTraffic(recorder.stop(), creds, `${member.label}/join`);
    return { joined, result };
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

// ---------------------------------------------------------------------------
// Story 3 C-C1: plan members (publish set + per-kind N / M per context)
// ---------------------------------------------------------------------------

/** A plan member after its join + media start. `tone` exists only for audio publishers. */
export interface PlanMember extends Member {
  readonly joined: JoinedBusEvent;
  readonly plan: CohortMemberPlan;
  /** The meeting-scoped sender id from the member's OWN join binding (stringified). */
  readonly senderId: string;
  readonly tone: CohortTone | undefined;
}

/** Whether a plan publishes `kind`. */
export function planPublishes(plan: CohortMemberPlan, kind: 'audio' | 'video'): boolean {
  return plan.publish === 'both' || plan.publish === kind;
}

/** The per-context levers for a plan member: N and M (the publish set is driven through the DOM chooser). */
export function planLevers(
  plan: CohortMemberPlan,
  extra: Omit<ContextLevers, 'audioReceiveSlots' | 'videoReceiveSlots' | 'publish'> = {},
): ContextLevers {
  const { audioReceiveSlots, videoReceiveSlots } = leversForPlan(plan);
  return { ...extra, audioReceiveSlots, videoReceiveSlots };
}

/** Open a plan member's context (its N / M levers installed). */
export async function openPlanMember(
  browser: Browser,
  index: number,
  plan: CohortMemberPlan,
  extra: Omit<ContextLevers, 'audioReceiveSlots' | 'videoReceiveSlots' | 'publish'> = {},
  beforeNavigate?: (context: BrowserContext) => Promise<void>,
): Promise<Member> {
  return openMember(browser, index, planLevers(plan, extra), beforeNavigate);
}

/**
 * Join as a PLAN member: the publish set is chosen THROUGH THE DOM CHOOSER
 * (`joinWithPublishViaUi`, R-2), media is started, and inside the same recorded
 * window the member asserts its build knobs (incl. the test pattern), its
 * EFFECTIVE N and M (`expectDeclaredReceiveSlots`, so a reused dev server or a
 * dropped lever fails loudly), its effective publish set from the bus, and —
 * for an audio publisher — its own tone. Joins are serialised by the caller
 * (the oracle's input is the join order, paired-test F5).
 */
export async function joinPlanMember(
  member: Member,
  meetingCode: string,
  plan: CohortMemberPlan,
  {
    expectAllHandlers = true,
    videoCap = 'admits',
  }: { expectAllHandlers?: boolean; videoCap?: 'admits' | 'truncates' } = {},
): Promise<PlanMember> {
  const publishesAudio = planPublishes(plan, 'audio');
  const publishesVideo = planPublishes(plan, 'video');
  const { joined, result: tone } = await recordedJoin(member, {
    join: (page) =>
      joinWithPublishViaUi(page, meetingCode, { audio: publishesAudio, video: publishesVideo }),
    expectAllHandlers,
    inside: async () => {
      await expectBuildKnobs(member.page, member.label);
      await expectDeclaredReceiveSlots(
        member.page,
        member.label,
        plan.audioReceiveSlots,
        plan.videoReceiveSlots,
        videoCap,
      );
      const { receiveSlots } = await latestBusEventsOf(member.page, ['receiveSlots']);
      expect(
        receiveSlots?.['publish'],
        `${member.label}: the session's EFFECTIVE publish set must be the plan's (chooser -> SDK)`,
      ).toEqual([...(publishesAudio ? ['audio'] : []), ...(publishesVideo ? ['video'] : [])]);
      return publishesAudio
        ? readOwnTone(member.page, member.label, 'checked-by-caller')
        : undefined;
    },
  });
  const senderId = joined.senderId;
  if (senderId === undefined || senderId === '') {
    throw new Error(`HARNESS: ${member.label}'s joined bus event carries no senderId`);
  }
  return { ...member, joined, plan, senderId, tone };
}

/** Narrow a plan member to the story-2 audio member shape (it must publish audio). */
export function asAudioMember(m: PlanMember): JoinedMember {
  if (m.tone === undefined) {
    throw new Error(`HARNESS: ${m.label} publishes no audio, so it has no tone`);
  }
  return { ...m, tone: m.tone };
}
