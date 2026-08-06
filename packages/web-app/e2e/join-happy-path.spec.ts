// File: packages/web-app/e2e/join-happy-path.spec.ts
//
// Task #18 (R-40/R-44/R-46): the browser-driver happy path for the Env-Test tier
// (ADR-0028 amendment). Real Chromium, the real WebTransport API (incl.
// serverCertificateHashes pinning), and the real demo DOM — the layer the Rust
// env-test `crates/env-tests/tests/24_join_flow.rs` (UNCHANGED) cannot reach.
// Division of responsibilities: see e2e/README.md.
//
// Flow contract (post task #58): `MeetingSession.join` is token-based — it
// reuses the authenticated session's userToken and never re-authenticates.
// There is NO login step at join time; the old JoinMeeting forced-login
// band-aid (commit 7b69288) was removed by task #58 and is asserted absent
// here (assertion (e), structural guard).
//
// Registration budget (SSoT: e2e/README.md §Budgets): this file spends TWO
// registrations for the WHOLE suite — the shared valid user V (via
// `authAsSharedUser`, once per run, reused everywhere via sign-in) and `userB`
// (the ONE genuinely-distinct second party for the distinct-user two-party test).
// Every context-A / host / rejoin role reuses V by SIGN-IN (0 registrations),
// and the leave/rejoin test's second participant is a second V session (identity
// is minted per-join), so it too costs 0.
//
// Requires a live host-side Kind cluster (see e2e/global-setup.ts + README).

import { expect, test } from 'playwright/test';
import {
  assertTokenOnlyJoinTraffic,
  authAsSharedUser,
  bootstrapMeeting,
  expectRosterMissing,
  expectRosterShows,
  joinAsUser,
  randomCredentials,
  recordRequests,
  signUpViaUi,
  SHARED_USER,
  waitForAllMediaConnected,
  waitForJoined,
  waitForParticipantJoined,
  waitForParticipantLeft,
} from './fixtures.js';
import {
  mcConnectedStatusSum,
  mcParticipantLeavesSum,
  waitForMcConnectedStatusAbove,
  waitForMcParticipantLeavesAbove,
} from './mcMetrics.js';

test.describe('join happy path (R-40/R-44/R-46)', () => {
  test('token-based join with active/active media (assertions a/b/d/e)', async ({ page }) => {
    // Shared valid user V (0-registration sign-in; V is registered once per run).
    await page.goto('/');
    await authAsSharedUser(page);

    await page.getByTestId('meeting-title').fill('E2E happy path meeting');
    await page.getByTestId('create-button').click();
    const meetingCode =
      (await page.getByTestId('created-meeting-code').textContent())?.trim() ?? '';
    expect(meetingCode, 'create view must display the meeting code').not.toBe('');

    // Assertion (e) window opens at THIS context's join action (per-context
    // window — the sign-in/create exchanges above are legitimately outside it).
    const recorder = recordRequests(page.context());
    // Assertion (d) baseline: captured immediately before the join action
    // (narrowest false-pass window — @test Gate 1 refinement).
    const mcStatusBaseline = await mcConnectedStatusSum();

    // --- Token-based join (NO login step) ---
    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);

    // (a) MC JoinResponse observed via the __darktower_test__ bus.
    expect(joined.participantId, 'JoinResponse must carry a participant_id').not.toBe('');

    // (b) At least one MH handshake succeeded for EVERY media_servers URL
    // (active/active). The list itself must be non-empty; the exact count is
    // cluster topology and deliberately not hardcoded here.
    expect(joined.mediaServers.length, 'media_servers must not be empty').toBeGreaterThan(0);
    await waitForAllMediaConnected(page, joined.mediaServers);

    // (d) The SDK's one post-join MediaConnectionUpdate (R-21) reached MC's R-60
    // handler (task #6) with state=CONNECTED — observed MC-side via Prometheus.
    await waitForMcConnectedStatusAbove(mcStatusBaseline);

    // (e) Token-only invariant (task #58 c-iii).
    assertTokenOnlyJoinTraffic(recorder.stop(), SHARED_USER, 'happy-path');
  });

  test('distinct-user two-party join renders both peers in the roster DOM (gaps 1 + 4)', async ({
    browser,
    page,
  }) => {
    // Context A: the shared valid user V (host). Meeting created Node-side (GC
    // POST), not via the demo create view.
    await page.goto('/');
    const hostToken = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(hostToken, 'E2E two-party meeting');

    await joinAsUser(page, meetingCode);
    const joinedA = await waitForJoined(page);
    expect(joinedA.participantId, 'host JoinResponse must carry a participant_id').not.toBe('');

    // Context B: a GENUINELY DIFFERENT account — registered fresh (NOT signInViaUi
    // of the same user; this is the coverage gap the same-user precedent left open).
    const userB = randomCredentials('userB');
    const context2 = await browser.newContext();
    try {
      const page2 = await context2.newPage();
      await page2.goto('/');
      await signUpViaUi(page2, userB);

      const recorderB = recordRequests(context2); // context B's OWN join window
      await joinAsUser(page2, meetingCode);
      const joinedB = await waitForJoined(page2);

      // Gap (1): the two contexts are DISTINCT participants. (Distinct accounts
      // are proven by the roster DOM below — userB's registered display name vs
      // V's — NOT by the bus `userId`, which e2eBus projects as 0 for every user;
      // see main.md §Issues.)
      expect(joinedB.participantId, 'second party must get its own participant_id').not.toBe(
        joinedA.participantId,
      );

      // (c) Context A sees ParticipantJoined for context B within 5s (R-46).
      await waitForParticipantJoined(page, joinedB.participantId, 5_000);

      // Gap (4): the rendered roster DOM shows BOTH peers with correct display
      // names — each context renders its peer (the roster excludes self), and the
      // two distinct names (userB's registered name vs V's) are the real
      // genuinely-distinct-account signal. Context A must show userB; context B
      // must show V by its TOKEN-CARRIED name (V signed in → no client displayName).
      await expectRosterShows(page, joinedB.participantId, userB.displayName);
      await expectRosterShows(page2, joinedA.participantId, SHARED_USER.displayName);

      // (e) Token-only invariant for the distinct second party's join, too.
      assertTokenOnlyJoinTraffic(recorderB.stop(), userB, 'two-party/context-B');
    } finally {
      await context2.close();
    }
  });

  test('participant leave, MC counter, and clean rejoin (gap 2)', async ({ browser, page }) => {
    // Host (context A) = V. The departing/rejoining peer is a SECOND V session
    // (participant identity is minted per-join, so two V joins are two distinct
    // participants) — this needs 0 registrations and keeps the display-name proof
    // meaningful: a signed-in client has NO client-side displayName
    // (SignIn.svelte), so a correct roster name proves the token carries it.
    await page.goto('/');
    const hostToken = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(hostToken, 'E2E leave/rejoin meeting');
    await joinAsUser(page, meetingCode);
    const host = await waitForJoined(page);

    // --- Second participant joins, then departs ---
    const leaverContext = await browser.newContext();
    let departedId: string;
    try {
      const leaverPage = await leaverContext.newPage();
      await leaverPage.goto('/');
      await authAsSharedUser(leaverPage); // 0 registrations (V already registered)
      await joinAsUser(leaverPage, meetingCode);
      const leaver = await waitForJoined(leaverPage);
      departedId = leaver.participantId;
      expect(departedId, 'the second session must get its own participant_id').not.toBe(
        host.participantId,
      );
      await waitForParticipantJoined(page, departedId, 5_000);
      await expectRosterShows(page, departedId, SHARED_USER.displayName);

      // Departure baseline captured immediately before the leave gesture.
      const leavesBaseline = await mcParticipantLeavesSum();

      // Drive the demo's OWN teardown for a DETERMINISTIC clean close: navigating
      // away from the join view unmounts JoinMeeting → onDestroy →
      // session.disconnect() → a clean CONNECTION_CLOSE (MC classifies ClientClosed
      // → voluntary → immediate leave, no grace). Waiting for the create view
      // confirms the unmount fired. Then close the context (still the task's
      // "close the second context"; the disconnect makes it fast + deterministic).
      await leaverPage.getByTestId('nav-create').click();
      await expect(
        leaverPage.getByTestId('meeting-title'),
        'leaver: create view must render (JoinMeeting unmounted → disconnect fired)',
      ).toBeVisible();
      await leaverContext.close();

      // Context A observes the departure three ways: the bus event, the roster
      // DOM removal, and MC's server-side leave counter (all-reasons monotonic
      // sum — see mcMetrics.ts; 90s ceiling is grace-path insurance, common path
      // ~1 scrape interval given the clean close).
      await waitForParticipantLeft(page, departedId);
      await expectRosterMissing(page, departedId);
      await waitForMcParticipantLeavesAbove(leavesBaseline);
    } finally {
      // Idempotent: no-op if already closed above; guards an early failure.
      if (!leaverContext.pages().every((p) => p.isClosed())) {
        await leaverContext.close().catch(() => undefined);
      }
    }

    // --- Rejoin from a FRESH context → clean re-entry ---
    const rejoinContext = await browser.newContext();
    try {
      const rejoinPage = await rejoinContext.newPage();
      await rejoinPage.goto('/');
      await authAsSharedUser(rejoinPage); // signInViaUi per gap (2): no client displayName
      await joinAsUser(rejoinPage, meetingCode);
      const rejoined = await waitForJoined(rejoinPage);

      // A NEW participant id (not the departed one) + a fresh joined event.
      expect(rejoined.participantId, 'rejoin must mint a NEW participant_id').not.toBe(departedId);

      // Context A sees the rejoin, and the roster DOM shows the rejoined peer's
      // TOKEN-CARRIED registered name — the sign-up path alone would stay green if
      // the name were ever band-aided client-side (task 63 second variant).
      await waitForParticipantJoined(page, rejoined.participantId, 5_000);
      await expectRosterShows(page, rejoined.participantId, SHARED_USER.displayName);
    } finally {
      await rejoinContext.close();
    }
  });

  test('bootstrapMeeting: an API-created meeting is joinable from the browser', async ({
    page,
  }) => {
    // Proves the harness building block (the multi-user specs depend on it):
    // meeting created via direct GC POST /api/v1/meetings — NOT the demo UI.
    await page.goto('/');
    const token = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(token, 'E2E bootstrapped meeting');

    const recorder = recordRequests(page.context());
    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);

    expect(joined.participantId, 'JoinResponse must carry a participant_id').not.toBe('');
    assertTokenOnlyJoinTraffic(recorder.stop(), SHARED_USER, 'bootstrap-join');
  });
});
