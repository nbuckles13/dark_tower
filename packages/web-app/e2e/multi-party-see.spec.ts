// File: packages/web-app/e2e/multi-party-see.spec.ts
//
// Story 3 — SEE AND HEAR BY CONTENT (R-1, R-5, R-32, R-33; R-29 read-back).
//
// Four participants (the suite's registered cohort), every one publishing BOTH
// kinds, each declaring N=3 audio and M=3 video slots (per-context levers;
// effective N/M asserted at runtime). Joins are serialised, and the test's OWN
// slot oracle (`slotOracle.ts`, driven by this spec's calls in join order)
// predicts every slot. For every receiver and every video slot:
//   * layer 1: the last RENDERED frame's verified key-id sender is the occupant
//     the oracle expects (MC does not name live video sources, R-5);
//   * layer 2: frames from that sender verified and decrypted at that slot
//     (the receive-layer counters advance);
//   * layer 3: the render-surface pattern decodes to that sender's join-bound
//     id, the guard blocks pass, and the counter changes across samples;
//   * plus the slot's MC state and rendered kind.
// In the SAME cohort, every receiver hears every other sender by tone (story 2
// S1's three audio layers). Every video publisher's announced pattern id is its
// own join-bound sender id (a separate check; layer 3 never uses it).
//
// R-29: `dt_client_media_frames_{received,sent}_total{media_kind="video",
// content_kind="main"}` rise
// above baselines taken BEFORE the first join (the series is born when video is
// first directed, so only a delta can see it), and nothing arrives missing
// either kind label over the same window.
//
// No wall-clock gate: every wait is a liveness bound, every window uses the
// shared stop rule (`windowSampling.ts`).

import { test } from 'playwright/test';
import { asAudioMember, tonesOf } from './cohortContexts.js';
import type { CohortMemberPlan } from './cohort.js';
import { flushTelemetry } from './fixtures.js';
import {
  clientCounterByInstance,
  expectClientCounterNotAbove,
  waitForClientCounterAbove,
} from './mcMetrics.js';
import { expectHearsAtAllLayers, expectPatternAnnounced } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3: N+1 participants see and hear each other by content', () => {
  test('every receiver sees every other sender at three layers and hears them (N=3, M=3)', async ({
    browser,
  }) => {
    test.setTimeout(300_000);
    // R-29 baselines BEFORE anything can direct video.
    const receivedVideo = await clientCounterByInstance('received_video');
    const sentVideo = await clientCounterByInstance('sent_video');
    const receivedKindless = await clientCounterByInstance('received_kindless');
    const { scenario, host } = await VideoScenario.create(browser, 'E2E see-by-content', BOTH);
    try {
      await scenario.join(host, BOTH);
      for (let i = 1; i < 4; i += 1) await scenario.join(await scenario.open(i, BOTH), BOTH);
      const members = [...scenario.members.values()];

      for (const m of members)
        await scenario.diagnosed(`${m.label} pattern`, () => expectPatternAnnounced(m));

      for (const receiver of members) {
        await scenario.expectVideo(receiver.label, 'every other participant at three layers');
      }

      const audio = members.map(asAudioMember);
      const cohort = tonesOf(audio);
      for (const receiver of audio) {
        for (const sender of audio) {
          if (sender === receiver) continue;
          await expectHearsAtAllLayers(receiver, sender, cohort, scenario.admissionBaseline);
        }
      }

      await test.step('R-29: video client series carry media_kind="video", content_kind="main" end to end', async () => {
        await Promise.all(members.map((m) => flushTelemetry(m.page)));
        await waitForClientCounterAbove(
          'received_video',
          receivedVideo,
          'R-29: the received-frames counter never reached Prometheus with media_kind="video", ' +
            'content_kind="main". EMPTY means a kind label was stripped (GC MEDIA_DATAPOINT_EXTRA / ' +
            'collector keep_keys) or no video was received.',
        );
        await waitForClientCounterAbove(
          'sent_video',
          sentVideo,
          'R-29: the sent-frames counter never reached Prometheus with media_kind="video", ' +
            'content_kind="main".',
        );
        // The absence half, over the SAME window the video arms just proved live.
        await expectClientCounterNotAbove(
          'received_kindless',
          receivedKindless,
          'R-29: received frames reached Prometheus WITHOUT media_kind or WITHOUT content_kind.',
        );
      });
    } finally {
      await scenario.close();
    }
  });
});
