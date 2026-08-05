// File: packages/web-app/e2e/meeting-not-found.spec.ts
//
// Task #19 (R-45): a join against an unknown meeting code surfaces GC's 404 as
// a TYPED error in the demo DOM — and, critically, does NOT trip the
// session-drop policy (a not-found is not a credential rejection; the retained
// token stays valid and usable).
//
// False-pass hardening (three independent discriminants):
//   (1) the GC join response is asserted 404-EXACT via waitForResponse — a 401
//       (dead credential), 429 (rate limit), 400 (shape rejection), or a dead
//       cluster (timeout) CANNOT satisfy it. 404-exact also means a future GC
//       status-code collision on this path fails loudly here (@security, Gate 1);
//   (2) `last-error` renders with the typed `MEETING:` prefix
//       (`MeetingNotFoundError`, code MEETING) AND the auth nav does NOT appear
//       — `isSessionRejection` fires ONLY on `MeetingUnauthorizedError`, so
//       nav-signin staying hidden discriminates the 404 path from the 401
//       session-drop path (the auth-rejection spec asserts the inverse);
//   (3) no `joined` bus event (point-in-time scan after the terminal condition).
//
// Registration budget: ONE registration (a real user is required — an invalid
// token would be rejected 401 at the auth middleware BEFORE the lookup, testing
// the wrong hop). See e2e/README.md §Budgets.

import { expect, test } from 'playwright/test';
import { SdkErrorCode } from '@darktower/sdk-core';
import {
  expectLastErrorCode,
  expectNoJoinedEvent,
  joinCapturingGcStatus,
  randomCredentials,
  randomMeetingCode,
  signUpViaUi,
} from './fixtures.js';

test.describe('meeting not found (R-45)', () => {
  test('GC 404 for an unknown meeting code surfaces a typed MeetingNotFoundError without dropping the session', async ({
    page,
  }) => {
    // Real user, real token: the rejection under test must be the DB lookup
    // miss, not an auth-middleware rejection upstream of it.
    const creds = randomCredentials('meeting-not-found');
    await page.goto('/');
    await signUpViaUi(page, creds);

    // randomMeetingCode() is well-formed for BOTH format authorities (client
    // validateMeetingCode + GC's pre-DB-lookup shape check — see the fixture's
    // coupled-format comment), so the ONLY rejection left is a real 404.
    const status = await joinCapturingGcStatus(page, randomMeetingCode());
    expect(
      status,
      'GC must reject an unknown meeting code with exactly 404 (MeetingNotFoundError) — ' +
        'not 400 (shape), 401 (credential), or 429 (rate limit)',
    ).toBe(404);

    // Typed surface: errorText renders `${code}: ${message}`; the MEETING:
    // prefix pins MeetingNotFoundError's family without message-text matching.
    await expectLastErrorCode(page, SdkErrorCode.Meeting);

    // The 404 is NOT a session rejection: the retained token is still valid, so
    // the session-drop policy must NOT fire. Point-in-time check, valid because
    // the drop is synchronous with the join rejection we just observed rendered:
    // if isSessionRejection had fired, the shell would already have swapped the
    // authed nav back in (and unmounted the join view with our last-error).
    await expect(
      page.getByTestId('nav-signin'),
      'a 404 must NOT drop the session — isSessionRejection fires only on MeetingUnauthorizedError (401)',
    ).not.toBeVisible();

    // Terminal condition established (typed error rendered) — point-in-time scan.
    await expectNoJoinedEvent(page, 'meeting-not-found');
  });
});
