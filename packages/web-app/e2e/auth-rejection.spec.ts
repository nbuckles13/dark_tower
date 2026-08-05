// File: packages/web-app/e2e/auth-rejection.spec.ts
//
// Task #19 (R-45): credential rejections surface as TYPED errors in the demo UI.
//
// Mechanism note (Gate-1 Q6, reviewer-ruled): R-45's "unauthenticated
// meeting-join surfaces a typed AuthError" wording predates task #58's
// token-join contract. Post-#58 the join hop never talks to AC — an
// unauthenticated JOIN is rejected by GC with 401, whose typed error is
// `MeetingUnauthorizedError` and whose demo-UI surface is the typed-error-GATED
// session drop (`isSessionRejection` in src/lib/errorText.ts fires ONLY on that
// class, restoring the auth nav). The literal AuthError surface lives at the AC
// hop (sign-in rejection). This file covers BOTH hops:
//   Test A — the unauthenticated meeting-join (GC hop, session-drop surface)
//   Test B — the AC credential rejection (the literal `AUTH:` typed surface)
//
// False-pass hardening: every rejection is asserted status-EXACT via
// `waitForResponse` (a dead cluster, a 429 rate-limit, or a 404 CANNOT satisfy
// a 401-exact assertion), and Test A additionally asserts the session-drop
// behavior that ONLY `MeetingUnauthorizedError` can trigger.
//
// Registration budget: ZERO — Test A route-fulfills the register exchange
// (AC never hit); Test B signs in with never-registered credentials.

import { expect, test } from 'playwright/test';
import { SdkErrorCode } from '@darktower/sdk-core';
import {
  expectLastErrorCode,
  expectNoJoinedEvent,
  fulfillAuthRegister,
  garbageToken68,
  joinCapturingGcStatus,
  randomCredentials,
  randomMeetingCode,
  signInExpectingRejection,
  signUpViaUi,
} from './fixtures.js';

test.describe('auth rejection (R-45)', () => {
  test('unauthenticated meeting-join: GC rejects the token with 401 and the demo drops the session', async ({
    page,
  }) => {
    // The demo retains a runtime-generated garbage token: the register exchange
    // is route-FULFILLED (see fulfillAuthRegister — the only way to reach the
    // authed shell holding an invalid credential, since the auth nav is hidden
    // once a session exists). The token is well-formed token68, so the SDK's
    // local charset guard passes and GC makes the rejection decision.
    const creds = randomCredentials('auth-reject');
    const garbage = garbageToken68();
    await fulfillAuthRegister(page, creds, garbage);
    await page.goto('/');
    const retained = await signUpViaUi(page, creds);
    // Boolean compare, NOT .toBe(garbage): a string .toBe prints Expected/
    // Received VALUES on mismatch — and the precise failure this sanity check
    // exists to catch (the register route silently not intercepting) is the one
    // where `retained` would be a REAL live AC token. Redacted by design
    // (@security, task #19 review; same discipline as expectLastErrorCode /
    // assertTokenOnlyJoinTraffic's redact()).
    expect(
      retained === garbage,
      'harness sanity: the demo did not retain the injected token — fulfillAuthRegister route did not intercept (token values redacted from this message by design)',
    ).toBe(true);

    // Join with a well-formed code. The code's existence is irrelevant: GC's
    // auth middleware rejects the credential with 401 either way — and the
    // 401-EXACT assertion is what discriminates this from a 404 (unknown code),
    // a 429 (rate limit), or a dead cluster (timeout in joinCapturingGcStatus).
    const status = await joinCapturingGcStatus(page, randomMeetingCode());
    expect(
      status,
      'GC must reject an unauthenticated join with exactly 401 (MeetingUnauthorizedError)',
    ).toBe(401);

    // The demo-UI surface of the typed error: the session-drop policy
    // (src/lib/errorText.ts isSessionRejection) fires ONLY on
    // MeetingUnauthorizedError — a generic failure leaves the session in place.
    // The auth nav reappearing IS the typed-error assertion.
    await expect(
      page.getByTestId('nav-signin'),
      'session drop must restore the sign-in affordance (fires only on MeetingUnauthorizedError)',
    ).toBeVisible();

    // Terminal condition established (session dropped) — point-in-time scan.
    await expectNoJoinedEvent(page, 'auth-rejection/unauthenticated-join');
  });

  test('AC rejects invalid credentials at sign-in with a typed AuthError in the demo UI', async ({
    page,
  }) => {
    // Never-registered credentials: AC's /user/token rejects with 401 →
    // AuthUnauthorizedError (code AUTH) → SignIn renders it in last-error.
    const creds = randomCredentials('never-registered');
    await page.goto('/');
    const status = await signInExpectingRejection(page, creds);
    expect(
      status,
      'AC must reject unknown credentials with exactly 401 — a 429 rate-limit must not pass as the credential rejection',
    ).toBe(401);

    // The literal R-45 surface: a typed AuthError rendered in the demo DOM.
    // errorText renders `${err.code}: ${err.message}` — the `AUTH:` prefix pins
    // the AuthError family (SdkErrorCode.Auth) without message-text matching.
    await expectLastErrorCode(page, SdkErrorCode.Auth);
  });
});
