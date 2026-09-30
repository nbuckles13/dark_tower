// File: packages/web-app/e2e/kek-rotation.spec.ts
//
// Story 2 S6, JOINER HALF — KEK rotation on leave (R-12, R-14, R-17; ADR-0036 §4).
//
// A participant leaves; MC rotates the meeting KEK once the debounce W has run
// from the removal; the REMAINING clients install the new generation; a
// participant joining AFTER the rotation opens current frames. (The leaver half
// — a departed holder cannot open post-rotation frames — is a client
// integration test, since a departed browser tears down; R-17.)
//
// EVIDENCE, per the rule in `crates/env-tests/README.md`:
//   * PRIMARY, per participant: each remaining client's own bus shows its KEK
//     generation advance (`kekGeneration`, sampled from the SDK's current
//     generation — public, it is in every key id). The joiner is gated on the
//     remaining clients having advanced to the SAME generation, so "the joiner
//     opens current frames" holds by construction: the joiner receives only the
//     current KEK, and opens the remaining senders' frames at all three layers.
//   * SECONDARY, fleet-level (Prometheus; client series carry the collector as
//     `instance`, no participant label): `dt_client_media_kek_updates_total
//     {source="kek_update"}` and `dt_client_media_kek_generations_retained_total`
//     (a COUNTER) each rise by at least the number of remaining clients, after
//     every remaining page's forced flush has resolved.
//     SUBSTITUTION, recorded in main.md: the task text says "assert increase()
//     over the rotation window". PromQL `increase()` cannot see a series' birth
//     value, and both series are typically BORN by this rotation, so it would
//     read ~0 on correct behaviour. A per-instance baseline taken before the
//     leave (absent = 0) and polled to baseline + k keeps "a counter, assert a
//     delta" without that blind spot.
//
// BUDGET: from MC's own configuration — W (`MC_KEK_ROTATION_DEBOUNCE_SECONDS`,
// infra/services/mc-service/config.env) plus the disconnect grace (MC's config or
// its code default). The leave is graceful, but MC today classifies the SDK's
// clean disconnect as `server_initiated` and holds the participant through its
// grace first (recorded in main.md), so the budget covers grace + W. If this ever
// times out, compare `mc_meeting_kek_rotation_window_seconds` on the running MC
// with the value read here. A liveness bound, never a latency gate.

import { readFileSync } from 'node:fs';
import { expect, test } from 'playwright/test';
import {
  closeMembers,
  joinMember,
  leaveGracefully,
  openMember,
  tonesOf,
  type JoinedMember,
  type Member,
} from './cohortContexts.js';
import { mcConfigRsPath, readMcRotationTimings, serviceConfigEnvPath } from './configEnv.js';
import { bootstrapMeeting, flushTelemetry, latestBusEventsOf } from './fixtures.js';
import {
  clientCounterByInstance,
  mhAdmissionRejectionsByInstance,
  waitForClientCounterAbove,
} from './mcMetrics.js';
import { expectHearsAtAllLayers } from './receiveEvidence.js';

const MC_ENV = serviceConfigEnvPath('mc-service');
const MC_RS = mcConfigRsPath();
const TIMINGS = readMcRotationTimings(
  readFileSync(MC_ENV, 'utf8'),
  MC_ENV,
  readFileSync(MC_RS, 'utf8'),
  MC_RS,
);
/** Grace + W, the worst case from the leave to the rotation push. */
const ROTATION_BUDGET_MS = (TIMINGS.graceSeconds + TIMINGS.debounceSeconds) * 1000;
/** Margin for joins, the post-rotation joiner and the Prometheus read-back. */
const SCENARIO_MARGIN_MS = 150_000;

async function kekGeneration(member: JoinedMember): Promise<number | undefined> {
  const { kekGeneration: event } = await latestBusEventsOf(member.page, ['kekGeneration']);
  const generation = event?.['generation'];
  return typeof generation === 'number' ? generation : undefined;
}

test.describe('story 2 S6 (joiner half): KEK rotation on leave', () => {
  test('a leave rotates the KEK on the remaining clients, and a later joiner opens current frames', async ({
    browser,
  }) => {
    test.setTimeout(ROTATION_BUDGET_MS + SCENARIO_MARGIN_MS);
    const opened: Member[] = [];
    try {
      for (let i = 0; i < 3; i += 1) opened.push(await openMember(browser, i));
      const meetingCode = await bootstrapMeeting(opened[0]!.token, 'E2E S6 rotation on leave');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      const members: JoinedMember[] = [];
      for (const m of opened) members.push(await joinMember(m, meetingCode));
      const [a, b, leaver] = members as [JoinedMember, JoinedMember, JoinedMember];
      const remaining = [a, b];
      await expectHearsAtAllLayers(a, b, tonesOf(members), admissionBaseline);

      const before = await Promise.all(remaining.map(kekGeneration));
      for (const [i, g] of before.entries()) {
        expect(g, `${remaining[i]!.label} holds a KEK generation before the leave`).toBeDefined();
      }
      const updatesBaseline = await clientCounterByInstance('kek_update');
      const retainedBaseline = await clientCounterByInstance('kek_retained');

      await leaveGracefully(leaver.page);
      await leaver.context.close();

      // PRIMARY: every remaining client advanced, to the SAME generation.
      await expect
        .poll(
          async () => {
            const now = await Promise.all(remaining.map(kekGeneration));
            const advanced = now.every((g, i) => g !== undefined && g > before[i]!);
            return advanced && now[0] === now[1] ? 'rotated' : JSON.stringify(now);
          },
          {
            message:
              `the remaining clients did not both install a newer KEK generation within grace ` +
              `(${TIMINGS.graceSeconds}s) + W (${TIMINGS.debounceSeconds}s) of the leave`,
            timeout: ROTATION_BUDGET_MS + 30_000,
            intervals: [1_000],
          },
        )
        .toBe('rotated');

      // SECONDARY: fleet-level, after every remaining page's flush resolved.
      await Promise.all(remaining.map((m) => flushTelemetry(m.page)));
      await waitForClientCounterAbove(
        'kek_update',
        updatesBaseline,
        'S6: the rotation push reached the remaining clients but not Prometheus.',
        { minDelta: remaining.length },
      );
      await waitForClientCounterAbove(
        'kek_retained',
        retainedBaseline,
        'S6: the remaining clients did not report retaining the demoted generation.',
        { minDelta: remaining.length },
      );

      // The joiner, AFTER the rotation: holds only the current KEK, and opens
      // the remaining senders' frames — and they open the joiner's.
      const joinerMember = await openMember(browser, 3);
      opened.push(joinerMember);
      const joiner = await joinMember(joinerMember, meetingCode);
      const rotated = await kekGeneration(a);
      await expect
        .poll(() => kekGeneration(joiner), {
          message: 'the joiner holds the rotated generation',
          timeout: 10_000,
        })
        .toBe(rotated);
      const cohort = [a.tone, b.tone, joiner.tone];
      for (const sender of remaining)
        await expectHearsAtAllLayers(joiner, sender, cohort, admissionBaseline);
      for (const receiver of remaining)
        await expectHearsAtAllLayers(receiver, joiner, cohort, admissionBaseline);
    } finally {
      await closeMembers(opened.filter((m) => !m.page.isClosed()));
    }
  });
});
