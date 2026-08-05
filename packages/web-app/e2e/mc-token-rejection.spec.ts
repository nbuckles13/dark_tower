// File: packages/web-app/e2e/mc-token-rejection.spec.ts
//
// Task #19 (R-45): MC rejects a join whose meeting_id cannot match the (real,
// valid) meeting token's claim — the token-binding check at MC's auth gate
// (`crates/mc-service/src/webtransport/connection.rs` step 6) — and the demo
// surfaces the typed SignalingError.
//
// Drive mechanism: the SDK is handed a REAL meeting token and a REAL
// mcAssignment, but the GC join response's `meetingId` is route-rewritten to a
// random UUID (see fixtures.ts rewriteJoinResponseMeetingId). MC validates the
// token fine (step 5), then step 6's binding check finds claims.meeting_id !=
// the presented meeting_id and replies ErrorCode::Unauthorized with the
// fail-closed generic message, closing the session.
//
// FALSE-PASS KILLER — the MC-side Prometheus proof: a dead/unreachable MC ALSO
// yields a `SIGNALING:`-class error in the DOM (transport failure), so the DOM
// assertion alone cannot distinguish "MC actively rejected at its auth gate"
// from "MC was never reached". A dead MC structurally CANNOT increment its own
// `mc_session_join_failures_total{error_type="jwt_validation"}` counter — the
// `> baseline` delta is the proof the rejection actually executed server-side.
//
// Fail-closed contract preserved: MC's client-visible message is the bounded
// generic "Invalid or expired token" for BOTH step 5 and step 6 — this spec
// asserts ONLY the typed `SIGNALING:` prefix and the server-side counter, never
// rejection detail, so it cannot create pressure to leak WHY a join was refused.
//
// Token hygiene (@semantic-guard item 8): the real meeting token flows only
// inside the route-fulfilled response body, by value — never logged, never
// interpolated into titles or assertion messages.
//
// Registration budget: ONE registration (+1 GC meeting create via
// bootstrapMeeting). See e2e/README.md §Budgets.

import { test } from 'playwright/test';
import { SdkErrorCode } from '@darktower/sdk-core';
import {
  bootstrapMeeting,
  expectLastErrorCode,
  expectNoJoinedEvent,
  joinAsUser,
  randomCredentials,
  rewriteJoinResponseMeetingId,
  signUpViaUi,
} from './fixtures.js';
import { mcSessionJoinFailureSum, waitForMcSessionJoinFailureAbove } from './mcMetrics.js';

test.describe('MC token rejection (R-45)', () => {
  test('MC rejects a join whose meeting_id does not match the token claim, surfacing a typed SignalingError', async ({
    page,
  }) => {
    // Real user + real meeting (Node-side GC create — not the demo UI): every
    // credential in play is genuine EXCEPT the meeting_id binding we corrupt.
    const creds = randomCredentials('mc-token-reject');
    await page.goto('/');
    const token = await signUpViaUi(page, creds);
    const meetingCode = await bootstrapMeeting(token, 'E2E mc-token-rejection meeting');

    // Corrupt ONLY the meetingId in the GC join response the SDK will consume.
    await rewriteJoinResponseMeetingId(page, meetingCode);

    // Counter baseline captured immediately before the join action (narrowest
    // false-pass window; `> baseline` on a monotonic counter is safe under
    // concurrent suites — see mcMetrics.ts header).
    const baseline = await mcSessionJoinFailureSum('jwt_validation');

    await joinAsUser(page, meetingCode);

    // (1) Client surface: the join promise rejects with a typed SignalingError
    // (MC ErrorMessage → the SDK's proto-error mapping); errorText renders the
    // SIGNALING: prefix. Only the bounded generic surface is asserted — no
    // dependence on rejection detail leaking (fail-closed contract).
    await expectLastErrorCode(page, SdkErrorCode.Signaling);

    // (2) Server-side proof: MC's auth-gate rejection counter moved. This is
    // what a dead/unreachable MC (whose transport failure also renders
    // SIGNALING: in the DOM) cannot fake. Label anchor: see McJoinFailureErrorType
    // in mcMetrics.ts (step 5 AND step 6 both record jwt_validation).
    await waitForMcSessionJoinFailureAbove(baseline, 'jwt_validation');

    // (3) Terminal condition established (error rendered + counter moved) —
    // point-in-time scan.
    await expectNoJoinedEvent(page, 'mc-token-rejection');
  });
});
