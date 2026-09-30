// File: packages/web-app/e2e/over-subscription.spec.ts
//
// Story 2 S2 — UNDER-FILL AND OVER-SUBSCRIPTION (R-2; ADR-0036 §6).
//
// INTERPRETATION, recorded in the devloop's main.md: the task text's "a receiver
// declaring 2 slots with 3 other senders sees the explicit fewer-sources state
// on the third" has no third slot to show it (N=2). Both R-2 clauses are
// covered instead, in two phases with one receiver R that declares N=2 through
// the per-context `receiveSlots` test lever:
//   1. UNDER-FILL — one sender: slot 0 `active`, slot 1 `fewer_sources` — an
//      under-filled grid (an empty cell with its text), never a spinner;
//   2. OVER-SUBSCRIPTION — two more senders join, one at a time (each joined and
//      assigned at R before the next starts, so "earliest" is not scheduling
//      luck): R's two slots hold the two EARLIEST senders; the third is in none
//      of R's assignments and has ZERO layer-1/2 at R, WHILE its own send counter
//      rises AND a full-N receiver's accepted-from-third advances — so the third
//      is demonstrably sending and heard, just not by R.

import { expect, test } from 'playwright/test';
import { closeMembers, joinMember, openMember, type Member } from './cohortContexts.js';
import { cohortSize, SUITE_RECEIVE_SLOTS } from './cohort.js';
import { bootstrapMeeting } from './fixtures.js';
import { mhAdmissionRejectionsByInstance } from './mcMetrics.js';
import {
  acceptedProbe,
  keyedProbe,
  expectHearsAtAllLayers,
  latestAssignments,
  observeWindow,
  sentProbe,
  waitForSenderSlotState,
} from './receiveEvidence.js';

/** R's declared N — below the number of other senders, by construction. */
const R_SLOTS = 2;

test.describe('story 2 S2: under-fill and over-subscription', () => {
  test('a 2-slot receiver: fewer_sources when under-filled; the earliest two fill it, the third is not held', async ({
    browser,
  }) => {
    test.setTimeout(180_000);
    expect(
      cohortSize(SUITE_RECEIVE_SLOTS),
      'S2 needs the receiver plus 3 senders',
    ).toBeGreaterThanOrEqual(4);
    const opened: Member[] = [];
    try {
      const receiver = await openMember(browser, 0, { receiveSlots: String(R_SLOTS) });
      opened.push(receiver);
      const meetingCode = await bootstrapMeeting(receiver.token, 'E2E S2 over-subscription');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();

      const r = await joinMember(receiver, meetingCode, { expectedSlots: R_SLOTS });

      // Phase 1 — under-fill.
      const s1m = await openMember(browser, 1);
      opened.push(s1m);
      const s1 = await joinMember(s1m, meetingCode);
      await waitForSenderSlotState(r, s1, 'active');
      const slot1 = r.page.getByTestId('slot-1');
      await expect(slot1, 'the unfilled slot is rendered as a cell').toBeVisible();
      await expect(slot1).toHaveAttribute('data-slot-state', 'fewer_sources');
      await expect(slot1, 'an unfilled cell names nobody').not.toHaveAttribute('data-sender-id');
      await expect(r.page.getByTestId('slot-list').locator('li'), 'exactly N=2 cells').toHaveCount(
        R_SLOTS,
      );
      await expectHearsAtAllLayers(r, s1, [r.tone, s1.tone], admissionBaseline);

      // Phase 2 — over-subscription, one sender at a time.
      const s2m = await openMember(browser, 2);
      opened.push(s2m);
      const s2 = await joinMember(s2m, meetingCode);
      await waitForSenderSlotState(r, s2, 'active');
      const s3m = await openMember(browser, 3);
      opened.push(s3m);
      const s3 = await joinMember(s3m, meetingCode);
      // s3 is assigned at its full-N peers before anything is concluded at R.
      await waitForSenderSlotState(s1, s3, 'active');

      const cohort = [r.tone, s1.tone, s2.tone, s3.tone];
      await expectHearsAtAllLayers(r, s1, cohort, admissionBaseline);
      await expectHearsAtAllLayers(r, s2, cohort, admissionBaseline);
      const assignments = await latestAssignments(r);
      expect(
        assignments.map((a) => a.senderId).sort(),
        "R's two slots hold the two EARLIEST senders, and nothing else (R-2 join order)",
      ).toEqual([s1.tone.senderId, s2.tone.senderId].sort());
      expect(
        assignments.every((a) => a.slotState === 'active'),
        "both of R's slots are active",
      ).toBe(true);

      // The third sender: absent at R, WHILE it sends and a full-N receiver hears it.
      await expectHearsAtAllLayers(s1, s3, cohort, admissionBaseline);
      await observeWindow(
        [
          // Zero at BOTH layers at R: nothing keyed to s3 even arrives (L1), and so
          // nothing verifies (L2).
          keyedProbe(r, s3, 'zero'),
          acceptedProbe(r, s3, 'zero'),
          // R's own mover: R keeps receiving a sender it holds, so R's zeros are
          // read from a live sampler, not a stale snapshot.
          acceptedProbe(r, s1, 'advance'),
          sentProbe(s3, 'advance'),
          acceptedProbe(s1, s3, 'advance'),
        ],
        'S2: the third sender is held by a full-N receiver but not by the 2-slot receiver',
      );
    } finally {
      await closeMembers(opened);
    }
  });
});
