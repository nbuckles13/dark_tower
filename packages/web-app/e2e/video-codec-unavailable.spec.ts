// File: packages/web-app/e2e/video-codec-unavailable.spec.ts
//
// Story 3 — VP8 UNAVAILABLE FAILS LOUDLY IN THE UI (R-28 UI surfacing; R-8).
//
// The SUBSTRATE half of R-28 is global-setup's independent VP8 probe
// (`vp8Probe.ts`): a Chromium that cannot do VP8 fails the whole browser layer
// before any spec. THIS spec proves the CLIENT half: when the SDK's own
// join-time probe (`media/setup/vp8.ts:probeVp8Support`, which calls
// `VideoEncoder.isConfigSupported`) says no, media start is REFUSED AS A WHOLE
// — the SDK throws and raises a fatal `mediaFault` before any pipeline exists —
// and the UI shows it: the fault and the error are rendered, no mute controls
// appear, and nothing is sent (no `sendTracks` / `sendCounts` ever). Never an
// audio-only degradation.
//
// The unavailability is made by a TEST-OWNED init script in ONE context that
// answers `VideoEncoder.isConfigSupported` with "unsupported" — no browser flag,
// no app lever. POSITIVE CONTROL in the same spec: another context with no
// override publishes video, and its video send counter advances, so the fault
// comes from the override and not from a broken build.

import { expect, test } from 'playwright/test';
import type { BrowserContext } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import {
  busEvents,
  joinWithPublishViaUi,
  VIDEO_SLOT_STATE_LIVENESS_MS,
  waitForJoined,
} from './fixtures.js';
import { observeWindow, videoSentProbe } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

/** Make THIS context's WebCodecs report VP8 encoding unsupported (test-owned, pre-navigation). */
async function vp8EncoderUnsupported(context: BrowserContext): Promise<void> {
  await context.addInitScript(() => {
    const Encoder = (globalThis as { VideoEncoder?: { isConfigSupported: unknown } }).VideoEncoder;
    if (Encoder === undefined) return;
    Encoder.isConfigSupported = async (config: VideoEncoderConfig) => ({
      supported: false,
      config,
    });
  });
}

test.describe('story 3 R-28: an unusable VP8 fails media start loudly, never degrading to audio', () => {
  test('the SDK media-start fault surfaces in the UI; a control context publishes video', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const { scenario, host } = await VideoScenario.create(browser, 'E2E VP8 unavailable', BOTH);
    try {
      await scenario.join(host, BOTH);
      await scenario.join(await scenario.open(1, BOTH), BOTH);
      const control = scenario.member('A');

      const broken = await scenario.open(2, BOTH, {}, vp8EncoderUnsupported);
      await joinWithPublishViaUi(broken.page, scenario.meetingCode, { audio: true, video: true });
      await waitForJoined(broken.page);
      await broken.page.getByTestId('start-media').click();

      const fault = broken.page.getByTestId('media-fault');
      await expect(fault, 'the SDK media-start fault must surface in the UI').toBeVisible({
        timeout: VIDEO_SLOT_STATE_LIVENESS_MS,
      });
      await expect(fault).toContainText('VP8');
      await expect(broken.page.getByTestId('media-error')).toBeVisible();
      // Refused AS A WHOLE: no running controls, and nothing sent of EITHER kind.
      await expect(broken.page.getByTestId('start-media')).toBeVisible();
      await expect(broken.page.getByTestId('mute-toggle')).toHaveCount(0);
      await expect(broken.page.getByTestId('video-mute-toggle')).toHaveCount(0);

      // POSITIVE CONTROL: the same build, without the override, publishes video.
      // Run FIRST: it spans a full sampled window, so the absence checks below
      // are read only AFTER a degraded pipeline would have had time to appear.
      await scenario.diagnosed('control publishes video', () =>
        observeWindow([videoSentProbe(control, 'advance')], 'control context video send'),
      );
      const types = new Set((await busEvents(broken.page)).map((e) => e['type']));
      expect(types.has('sendTracks'), 'no pipeline: no send tracks').toBe(false);
      expect(types.has('sendCounts'), 'no pipeline: no video send').toBe(false);
      expect(types.has('mediaFrameCounts'), 'no audio-only degradation').toBe(false);
      await expect(
        broken.page.getByTestId('start-media'),
        'still refused after the window',
      ).toBeVisible();
    } finally {
      await scenario.close();
    }
  });
});
