// File: packages/web-app/e2e/video-over-subscription.spec.ts
//
// Story 3 — VIDEO OVER-SUBSCRIPTION AND TRUNCATION (R-1, R-4).
//
// A receiver declares MORE video slots than MC's per-kind cap allows. MC
// TRUNCATES: the join succeeds, slots up to the cap fill per the fill rule, and
// every slot past the cap reports a permanent `unavailable` (rendered empty,
// copy "this meeting allows fewer video slots") — visible, never a silent
// participant and never a join failure.
//
// The cap is read INDEPENDENTLY of MC's own report (`configEnv.readMcReceiveCaps`
// over MC's ConfigMap source, which the Kind overlay does not patch), and MC's
// reported cap (`receiveSlots.video.serverCap` on the bus) must EQUAL it — so a
// misreporting MC that truncated consistently with its own report still fails.

import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import { latestBusEventsOf } from './fixtures.js';
import { mcCaps, VideoScenario } from './videoScenario.js';

test.describe("story 3 R-1: a video declaration above MC's cap is truncated, never refused", () => {
  test('the excess slots read unavailable, the join succeeds and the others render', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const cap = mcCaps().video;
    expect(
      cap,
      'non-vacuity: the cluster must allow video (MC_MAX_VIDEO_RECEIVE_SLOTS > 0)',
    ).toBeGreaterThan(0);
    const over: CohortMemberPlan = {
      publish: 'both',
      audioReceiveSlots: 3,
      videoReceiveSlots: cap + 1,
    };
    const sender: CohortMemberPlan = {
      publish: 'both',
      audioReceiveSlots: 3,
      videoReceiveSlots: 3,
    };
    const { scenario, host } = await VideoScenario.create(
      browser,
      'E2E video over-subscription',
      over,
    );
    try {
      // The over-subscribing receiver joins FIRST, so its join is not helped by anyone.
      const r = await scenario.join(host, over, { videoCap: 'truncates' });
      const { receiveSlots } = await latestBusEventsOf(r.page, ['receiveSlots']);
      expect(
        (receiveSlots?.['video'] as Record<string, unknown> | undefined)?.['serverCap'],
        "MC's reported video cap must EQUAL its ConfigMap's MC_MAX_VIDEO_RECEIVE_SLOTS",
      ).toBe(cap);
      for (let i = 1; i < 4; i += 1) await scenario.join(await scenario.open(i, sender), sender);

      const expectations = scenario.videoExpectations('A');
      const unavailable = expectations.filter((e) => e.expectation.state === 'unavailable');
      expect(unavailable.length, 'exactly the declaration minus the cap is unavailable').toBe(1);
      await scenario.expectVideo('A', 'over-cap declaration');
      for (const e of unavailable) {
        const tile = r.page.getByTestId(`video-slot-${e.slotId}`);
        await expect(tile).toHaveAttribute('data-slot-state', 'unavailable');
        await expect(tile).toHaveAttribute('data-rendered', 'empty');
        await expect(tile).toContainText('this meeting allows fewer video slots');
      }
      // The join SUCCEEDED and stays joined.
      await expect(r.page.getByTestId('meeting-state')).toHaveText('joined');
    } finally {
      await scenario.close();
    }
  });
});
