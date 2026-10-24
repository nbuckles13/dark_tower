// File: packages/web-app/e2e/late-joiner-video.spec.ts
//
// Story 3 — ON-DEMAND KEYFRAMES FOR A LATE JOINER (R-10, R-33).
//
// Three participants publish video with groups already flowing; a fourth joins
// LATE and sees all three at three layers. Nothing periodic could have served
// it: there is NO keyframe interval anywhere —
//   * the DEPLOYED media-codec profile carries no keyframe/GoP-interval key
//     (proved non-vacuously: the same dump parses to the five MEDIA_VIDEO_* keys
//     first), and the SDK's encoder config has none (sdk-core `vp8.ts`,
//     unit-tested there);
//   * the keyframe cause vocabulary on the bus has no periodic cause
//     (`videoBusShape.KEYFRAME_CAUSES`, pinned by its node test).
// So the late joiner's first frames are new GROUP STARTS produced on demand:
// every publisher's keyframes-produced total ADVANCES between a sample before
// the late join and one after the late joiner's first render (a delta >= 1 —
// never a steady-state flatness claim, which loss-driven requests would make
// flaky).

import { readFileSync } from 'node:fs';
import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import { MEDIA_CODEC_PROFILE_ENV_VAR, parseConfigEnv, readVideoProfile } from './configEnv.js';
import { keyframesProduced } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3 R-10: a late joiner is served by on-demand keyframes', () => {
  test('a participant joining with video already flowing sees it, with no keyframe interval anywhere', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const dump = process.env[MEDIA_CODEC_PROFILE_ENV_VAR];
    if (dump === undefined || dump === '') {
      throw new Error(
        `${MEDIA_CODEC_PROFILE_ENV_VAR} is not set: the deployed profile is required`,
      );
    }
    const text = readFileSync(dump, 'utf8');
    // Positive control first: the dump parses to the profile's video keys.
    readVideoProfile(text, dump);
    const intervalKeys = [...parseConfigEnv(text).keys()].filter((k) =>
      /KEYFRAME|GOP|KEY_INT/i.test(k),
    );
    expect(
      intervalKeys,
      'the deployed media-codec-profile must carry NO keyframe interval',
    ).toEqual([]);

    const { scenario, host } = await VideoScenario.create(browser, 'E2E late joiner video', BOTH);
    try {
      await scenario.join(host, BOTH);
      await scenario.join(await scenario.open(1, BOTH), BOTH);
      await scenario.join(await scenario.open(2, BOTH), BOTH);
      const publishers = ['A', 'B', 'C'].map((l) => scenario.member(l));
      for (const p of publishers)
        await scenario.expectVideo(p.label, 'video flowing before the late join');
      const before = await Promise.all(publishers.map(keyframesProduced));

      const late = await scenario.join(await scenario.open(3, BOTH), BOTH);
      await scenario.expectVideo(late.label, 'late joiner, three layers');

      const after = await Promise.all(publishers.map(keyframesProduced));
      publishers.forEach((p, i) => {
        expect(
          after[i]! - before[i]!,
          `${p.label}: the late joiner's first group must be a keyframe produced ON DEMAND ` +
            `(keyframes ${before[i]} -> ${after[i]})`,
        ).toBeGreaterThanOrEqual(1);
      });
    } finally {
      await scenario.close();
    }
  });
});
