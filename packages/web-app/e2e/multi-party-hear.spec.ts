// File: packages/web-app/e2e/multi-party-hear.spec.ts
//
// Story 2 S1 — HEAR BY CONTENT (R-1, R-2, R-30, R-31; ADR-0036 §5, §6, §9), plus
// the R-27 client-metrics read-back that needs a multi-party meeting.
//
// Four participants (N = SUITE_RECEIVE_SLOTS = 3), every one connected to every
// handler MC offers. Each receiver, for each other sender — all 12 ordered pairs
// — asserts all three layers at the slot MC assigned:
//   1. the frame's key id names the expected sender at the expected slot, and no
//      other slot carries a frame keyed to that sender (misrouting);
//   2. it verified and decrypted under that sender's roster key;
//   3. the decoded tone is that sender's frequency and no other's (two-sided).
// Nothing assumes which handler carries a pair (R-33: MC's choice). On a missing
// sender the S1 diagnostic (MH admission rejections, baseline taken BEFORE the
// joins) separates a budget rejection from misrouting.
//
// Latency is observed, never gated; every timeout is a liveness bound.

import { expect, test } from 'playwright/test';
import {
  closeMembers,
  joinMember,
  openMember,
  tonesOf,
  type JoinedMember,
  type Member,
} from './cohortContexts.js';
import { cohortSize, SUITE_RECEIVE_SLOTS } from './cohort.js';
import { bootstrapMeeting, flushTelemetry, waitForFirstMediaFrame } from './fixtures.js';
import {
  clientCounterByInstance,
  loadedAlertClientNames,
  mhAdmissionRejectionsByInstance,
  waitForClientCounterAbove,
} from './mcMetrics.js';
import {
  MISSING_KEY_MATERIAL_ALERT,
  MISSING_KEY_MATERIAL_SELECTED_NAMES,
} from './clientMetricNames.js';
import { expectHearsAtAllLayers } from './receiveEvidence.js';

test.describe('story 2 S1: N+1 participants hear each other by content', () => {
  test('every receiver hears every other sender at all three layers (N=3, 12 pairs)', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const size = cohortSize(SUITE_RECEIVE_SLOTS);
    const opened: Member[] = [];
    try {
      for (let i = 0; i < size; i += 1) opened.push(await openMember(browser, i));
      const host = opened[0]!;
      const meetingCode = await bootstrapMeeting(host.token, 'E2E S1 hear-by-content');

      // Taken BEFORE the joins: a baseline after them would hide the rejection
      // the diagnostic looks for.
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      // R-27 part 1 / part 2 baselines, before any member exports media counts.
      const sentBaseline = await clientCounterByInstance('sent');
      const receivedBaseline = await clientCounterByInstance('received');

      const members: JoinedMember[] = [];
      for (const member of opened) members.push(await joinMember(member, meetingCode));
      const cohort = tonesOf(members);

      // Time to first media, per receiver: OBSERVED, NEVER GATED (ADR-0036 §10).
      // Recorded as an annotation for the run's record; no threshold exists.
      for (const m of members) {
        const ms = await waitForFirstMediaFrame(m.page);
        test
          .info()
          .annotations.push({ type: 'observed-first-media-ms', description: `${m.label}=${ms}` });
      }

      for (const receiver of members) {
        for (const sender of members) {
          if (sender === receiver) continue;
          await expectHearsAtAllLayers(receiver, sender, cohort, admissionBaseline);
        }
      }

      // Every participant has sent and received: force one export from each
      // before reading the pipe back.
      await Promise.all(members.map((m) => flushTelemetry(m.page)));

      await test.step('R-27 part 1: send-counter pipe liveness and _total survival', async () => {
        // A SOLO client sends nothing (R-3: its send directive has no streams), so
        // this runs here, where every client is held by the others. The query is
        // by the exact exported `_total` name: a rise proves both the pipe and
        // the suffix; a lost suffix returns empty and fails loudly.
        await waitForClientCounterAbove(
          'sent',
          sentBaseline,
          'R-27 part 1: the send counter this meeting drove never reached Prometheus under its exact name.',
        );
      });

      await test.step('R-27 part 2: alert-string exactness on the LOADED rule', async () => {
        const loaded = await loadedAlertClientNames(MISSING_KEY_MATERIAL_ALERT);
        expect(
          [...loaded].sort(),
          `selector-drift: the loaded ${MISSING_KEY_MATERIAL_ALERT} rule selects different ` +
            `dt_client_* names from the shared constant (e2e/clientMetricNames.ts)`,
        ).toEqual([...MISSING_KEY_MATERIAL_SELECTED_NAMES].sort());
        // The rule's denominator, read back by that exact string.
        await waitForClientCounterAbove(
          'received',
          receivedBaseline,
          'R-27 part 2: the alert denominator this meeting drove never reached Prometheus under ' +
            'the exact name the loaded rule selects.',
        );
        // RESIDUAL (recorded in main.md): `dt_client_media_frames_dropped_total`
        // is registered lazily and a healthy meeting drops nothing, so its LIVE
        // exactness is unproven until a deterministic drop trigger exists. It is
        // covered by the static drift guard (alert == constant == emitter literal,
        // reasons within the SDK vocabulary; tests/clientMetricNames.test.ts), by
        // part 1's proof that the exporter passes emitter literals through
        // verbatim, and by the loaded-rule equality above.
      });
    } finally {
      await closeMembers(opened);
    }
  });
});
