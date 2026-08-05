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
// Requires a live host-side Kind cluster (see e2e/global-setup.ts + README).

import { expect, test } from 'playwright/test';
import {
  assertTokenOnlyJoinTraffic,
  bootstrapMeeting,
  joinAsUser,
  randomCredentials,
  recordRequests,
  signInViaUi,
  signUpViaUi,
  waitForAllMediaConnected,
  waitForJoined,
  waitForParticipantJoined,
} from './fixtures.js';
import { mcConnectedStatusSum, waitForMcConnectedStatusAbove } from './mcMetrics.js';

test.describe('join happy path (R-40/R-44/R-46)', () => {
  test('sign-up, create, and token-based join with active/active media', async ({
    page,
    browser,
  }) => {
    // --- Context 1: UI sign-up -> UI create-meeting ---
    const creds = randomCredentials('primary');
    await page.goto('/');
    await signUpViaUi(page, creds);

    await page.getByTestId('meeting-title').fill('E2E happy path meeting');
    await page.getByTestId('create-button').click();
    const meetingCode =
      (await page.getByTestId('created-meeting-code').textContent())?.trim() ?? '';
    expect(meetingCode, 'create view must display the meeting code').not.toBe('');

    // Assertion (e) window opens at THIS context's join action (per-context
    // window — the sign-up/create exchanges above are legitimately outside it).
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

    // (d) The SDK's one post-join MediaConnectionUpdate (R-21: sent once
    // connectAll settles, BEFORE `joined` is emitted) reached MC's R-60 handler
    // (task #6) with state=CONNECTED — observed MC-side via Prometheus.
    // Completed BEFORE context 2 joins so the increment is attributable to
    // context 1's update (@test Gate 1 refinement).
    await waitForMcConnectedStatusAbove(mcStatusBaseline);

    // --- Context 2: same user, second participant (R-46 roster propagation) ---
    const context2 = await browser.newContext();
    try {
      const page2 = await context2.newPage();
      await page2.goto('/');
      // Fresh credential->token exchange (sign-IN, not a join-time re-auth);
      // reuses this spec's single registered user to respect the suite's
      // registration budget (rate-limit SSoT + budget math: e2e/README.md
      // §Budgets). Participant identity is per-join, not per-user.
      await signInViaUi(page2, creds);

      const recorder2 = recordRequests(context2); // context 2's OWN join window
      await joinAsUser(page2, meetingCode);
      const joined2 = await waitForJoined(page2);

      // Per-join identity contract: two joins => two distinct participants,
      // even for the same user (participant_id minted per connection).
      expect(joined2.participantId).not.toBe(joined.participantId);

      // (c) Context 1 sees ParticipantJoined for context 2 within 5s.
      await waitForParticipantJoined(page, joined2.participantId, 5_000);

      // (e) Token-only invariant (task #58 c-iii) in BOTH join windows.
      assertTokenOnlyJoinTraffic(recorder2.stop(), creds, 'context-2');
    } finally {
      await context2.close();
    }
    assertTokenOnlyJoinTraffic(recorder.stop(), creds, 'context-1');
  });

  test('bootstrapMeeting: an API-created meeting is joinable from the browser', async ({
    page,
  }) => {
    // Proves the harness building block (#19's multi-user specs depend on it):
    // meeting created via direct GC POST /api/v1/meetings — NOT the demo UI.
    const creds = randomCredentials('bootstrap');
    await page.goto('/');
    const token = await signUpViaUi(page, creds);
    const meetingCode = await bootstrapMeeting(token, 'E2E bootstrapped meeting');

    const recorder = recordRequests(page.context());
    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);

    expect(joined.participantId, 'JoinResponse must carry a participant_id').not.toBe('');
    assertTokenOnlyJoinTraffic(recorder.stop(), creds, 'bootstrap-join');
  });
});
