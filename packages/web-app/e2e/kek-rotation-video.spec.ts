// File: packages/web-app/e2e/kek-rotation-video.spec.ts
//
// Story 3 — KEK ROTATION ON LEAVE WITH VIDEO FLOWING (R-14).
//
// Three participants publish both kinds (N=3, M=3). C leaves; MC rotates the
// meeting KEK (grace + W, from MC's own config — a liveness bound, never a
// latency gate). Neither T nor a leave-triggered rotation forces a keyframe
// (R-14): the video transmit keys rotate in-band on KEK receipt.
//
// "No slot loses decodability for longer than the stop rule allows": from just
// before the leave until AFTER both remaining clients have installed the new
// generation, A's video slot showing B and B's slot showing A are sampled
// CONCURRENTLY, in CONSECUTIVE windows on the shared stop rule (`windowSampling.ts`), and EVERY
// window must pass all three layers — the verified-and-decrypted count advances
// (layer 2), the rendered sender stays B / A with no fall-back to an avatar or
// empty (layer 1), and the pattern stays live (layer 3). A rotation that broke
// decodability for longer than one window fails the window it lands in. The
// remaining clients' bus `kekGeneration` proves the rotation happened inside
// the sampled span (never a vacuous pass over a span with no rotation).
//
// LEAVER LOCKOUT (security criterion): a departed participant cannot open
// post-rotation VIDEO — a browser that left tears down, so this is the client
// integration test `packages/sdk-core/src/media/lifecycle/__tests__/
// videoReceiver.test.ts` "a video LEAVER cannot open post-rotation video
// (positive control: pre-rotation accepted)" (R-14's stated home), not
// duplicated here.

import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import { mcRotationBudget, ROTATION_POLL_MARGIN_MS } from './configEnv.js';
import { kekGeneration } from './fixtures.js';
import { assertVideoWindow, sampleVideoWindow } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const TIMINGS = mcRotationBudget();
/** Grace + W plus the shared poll margin: a liveness bound, never a latency gate. */
const ROTATION_BUDGET_MS = TIMINGS.budgetMs + ROTATION_POLL_MARGIN_MS;
const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3 R-14: KEK rotation on leave with video flowing', () => {
  test('video keeps rendering and decrypting at the remaining receivers across the rotation', async ({
    browser,
  }) => {
    test.setTimeout(ROTATION_BUDGET_MS + 180_000);
    const { scenario, host } = await VideoScenario.create(browser, 'E2E KEK rotation video', BOTH);
    try {
      await scenario.join(host, BOTH);
      await scenario.join(await scenario.open(1, BOTH), BOTH);
      await scenario.join(await scenario.open(2, BOTH), BOTH);
      const a = scenario.member('A');
      const b = scenario.member('B');
      for (const m of [a, b]) await scenario.expectVideo(m.label, 'before the leave');
      const pairSlot = (receiver: string, occupant: string) => {
        const e = scenario
          .videoExpectations(receiver)
          .find((x) => x.expectation.occupant === occupant);
        if (e === undefined)
          throw new Error(`HARNESS: ${receiver} has no video slot for ${occupant}`);
        return e;
      };
      // Stability (R-3): these slots never move across the leave.
      const watched = [
        { receiver: a, slot: pairSlot('A', 'B') },
        { receiver: b, slot: pairSlot('B', 'A') },
      ];
      const before = await Promise.all([a, b].map((m) => kekGeneration(m.page)));
      for (const g of before) expect(g, 'a KEK generation is held before the leave').toBeDefined();

      await scenario.leave('C');

      let windows = 0;
      let rotatedAt: number | undefined;
      const deadline = Date.now() + ROTATION_BUDGET_MS;
      await scenario.diagnosed('video across the KEK rotation', async () => {
        // Consecutive windows until one FULL window after the rotation landed.
        for (;;) {
          // BOTH receivers sampled CONCURRENTLY each round, so neither watched
          // slot goes unobserved while the other is sampled.
          const round = await Promise.all(
            watched.map(({ receiver }) =>
              sampleVideoWindow(receiver, `rotation window ${windows}`),
            ),
          );
          watched.forEach(({ receiver, slot }, i) =>
            assertVideoWindow(
              receiver,
              [slot],
              round[i]!,
              `across the KEK rotation (window ${windows})`,
            ),
          );
          windows += 1;
          const now = await Promise.all([a, b].map((m) => kekGeneration(m.page)));
          const rotated =
            now.every((g, i) => g !== undefined && g > before[i]!) && now[0] === now[1];
          if (rotated && rotatedAt === undefined) {
            rotatedAt = windows;
            continue; // one more full window AFTER the rotation
          }
          if (rotatedAt !== undefined) break;
          if (Date.now() > deadline) {
            throw new Error(
              `LIVENESS: the remaining clients did not both install a newer KEK generation within ` +
                `grace (${TIMINGS.graceSeconds}s) + W (${TIMINGS.debounceSeconds}s) of the leave ` +
                `(generations ${JSON.stringify(before)} -> ${JSON.stringify(now)})`,
            );
          }
        }
      });
      test.info().annotations.push({
        type: 'rotation-windows',
        description: `${windows} consecutive windows, rotation observed after window ${rotatedAt}`,
      });
      expect(rotatedAt, 'non-vacuity: the rotation landed inside the sampled span').toBeDefined();
    } finally {
      await scenario.close();
    }
  });
});
