// File: packages/web-app/e2e/publish-mixes.spec.ts
//
// Story 3 — PUBLISH-CAPABILITY MIXES (R-2, R-3, R-4).
//
//   1. Audio-only (A), video-only (B), both (C) and receive-only (D), each
//      choosing its set THROUGH THE JOIN VIEW'S CHOOSER, each declaring N=3,
//      M=3. Receive-only and audio-only participants occupy VIDEO slots as
//      `avatar_no_video` (R-3: video slots take any participant); a video-only
//      publisher has no audio representation; every other receiver's video
//      slots match the test's own oracle at three layers; audio publishers are
//      heard (layers 1/2 at every receiver, all three at audio receivers).
//   2. A publish-nothing-receive-nothing join (N=0, M=0, nothing chosen) is
//      REFUSED with the SDK's refusal text — never a silent empty meeting, never
//      a `joined` event.
//   3. An N=0 receiver (video only) and an M=0 receiver (audio only) each work.

import { expect, test } from 'playwright/test';
import { asAudioMember, openMember, closeMembers, tonesOf } from './cohortContexts.js';
import type { CohortMemberPlan } from './cohort.js';
import { busEvents, joinWithPublishViaUi, suiteCohort } from './fixtures.js';
import { SLOT_STATE_TEXT } from '../src/lib/slotState.js';
import { boundedName } from '../src/lib/participantState.js';
import {
  expectHearsAtAllLayers,
  expectReceiveLayers,
  latestAssignments,
} from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

/** The display name cohort member `index` registered with. */
const cohortName = (index: number): string => suiteCohort()[index]!.displayName;

const plan = (publish: CohortMemberPlan['publish'], n = 3, m = 3): CohortMemberPlan => ({
  publish,
  audioReceiveSlots: n,
  videoReceiveSlots: m,
});

test.describe('story 3: publish-capability mixes', () => {
  test('audio-only, video-only, both and receive-only: avatars for the non-video, by content for the rest', async ({
    browser,
  }) => {
    test.setTimeout(300_000);
    const plans = [plan('audio'), plan('video'), plan('both'), plan('none')];
    const { scenario, host } = await VideoScenario.create(browser, 'E2E publish mixes', plans[0]!);
    try {
      await scenario.join(host, plans[0]!);
      for (let i = 1; i < 4; i += 1)
        await scenario.join(await scenario.open(i, plans[i]!), plans[i]!);
      const [a, b, c, d] = ['A', 'B', 'C', 'D'].map((l) => scenario.member(l));

      // Every receiver's video slots, per the oracle (avatar_no_video for A and D).
      for (const m of [a!, b!, c!, d!]) {
        await scenario.expectVideo(m.label, 'publish mix');
        // The DOM carries the raw token, and an avatar_no_video tile's copy names
        // its occupant and the reason — from the app's own copy table, never a
        // restated literal.
        for (const e of scenario.videoExpectations(m.label)) {
          const tile = m.page.getByTestId(`video-slot-${e.slotId}`);
          await expect(tile).toHaveAttribute('data-slot-state', e.expectation.state);
          if (e.expectation.state === 'avatar_no_video' && e.occupant !== undefined) {
            const name = boundedName(cohortName(scenario.member(e.occupant.label).index));
            await expect(tile).toContainText(
              SLOT_STATE_TEXT.avatar_no_video({ name, kind: 'video' }),
            );
          }
        }
      }
      expect(
        scenario.videoExpectations('C').some((e) => e.expectation.state === 'avatar_no_video'),
        'non-vacuity: C must hold at least one avatar_no_video slot (A or D)',
      ).toBe(true);

      // Audio: A and C are heard everywhere; B (video-only) holds no audio slot.
      const audioSenders = [asAudioMember(a!), asAudioMember(c!)];
      const cohort = tonesOf(audioSenders);
      await scenario.diagnosed('audio of A and C', async () => {
        await expectHearsAtAllLayers(
          audioSenders[0]!,
          audioSenders[1]!,
          cohort,
          scenario.admissionBaseline,
        );
        await expectHearsAtAllLayers(
          audioSenders[1]!,
          audioSenders[0]!,
          cohort,
          scenario.admissionBaseline,
        );
        for (const r of [b!, d!]) {
          for (const s of audioSenders) await expectReceiveLayers(r, s);
        }
      });
      for (const r of [a!, c!, d!]) {
        expect(
          (await latestAssignments(r)).some((s) => s.senderId === b!.senderId),
          `${r.label}: the video-only publisher B has NO audio representation (R-3)`,
        ).toBe(false);
      }
    } finally {
      await scenario.close();
    }
  });

  test('a join that would neither publish nor receive is REFUSED, visibly, and never joins', async ({
    browser,
  }) => {
    const member = await openMember(browser, 0, { audioReceiveSlots: '0', videoReceiveSlots: '0' });
    try {
      await joinWithPublishViaUi(member.page, 'abcdefghijkl', { audio: false, video: false });
      const refusal = member.page.getByTestId('last-error');
      await expect(refusal).toHaveAttribute('data-refused', 'true');
      await expect(refusal).toContainText('SIGNALING:');
      await expect(refusal).toContainText('Choose the microphone or the camera');
      await expect(member.page.getByTestId('in-meeting')).toHaveCount(0);
      expect(
        (await busEvents(member.page)).some((e) => e['type'] === 'joined'),
        'a refused join must never reach MC',
      ).toBe(false);
    } finally {
      await closeMembers([member]);
    }
  });

  test('an N=0 receiver (video only) and an M=0 receiver (audio only) each work', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const videoOnlyReceiver = plan('both', 0, 3);
    const audioOnlyReceiver = plan('both', 3, 0);
    const full = plan('both');
    const { scenario, host } = await VideoScenario.create(
      browser,
      'E2E N=0 / M=0',
      videoOnlyReceiver,
    );
    try {
      await scenario.join(host, videoOnlyReceiver);
      await scenario.join(await scenario.open(1, audioOnlyReceiver), audioOnlyReceiver);
      await scenario.join(await scenario.open(2, full), full);
      const [a, b, c] = ['A', 'B', 'C'].map((l) => scenario.member(l));

      // A (N=0) sees B and C, and has no audio slots at all.
      await scenario.expectVideo('A', 'N=0 receiver');
      await expect(a!.page.getByTestId('slot-list').locator('li')).toHaveCount(0);
      // B (M=0) has no video grid, and hears A and C.
      await expect(b!.page.getByTestId('video-grid')).toHaveCount(0);
      const cohort = tonesOf([a!, b!, c!].map(asAudioMember));
      await scenario.diagnosed('B hears', async () => {
        for (const s of [a!, c!]) {
          await expectHearsAtAllLayers(
            asAudioMember(b!),
            asAudioMember(s),
            cohort,
            scenario.admissionBaseline,
          );
        }
      });
      // C sees and hears both.
      await scenario.expectVideo('C', 'full receiver');
    } finally {
      await scenario.close();
    }
  });
});
