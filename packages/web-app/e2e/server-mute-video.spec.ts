// File: packages/web-app/e2e/server-mute-video.spec.ts
//
// Story 3 — SERVER VIDEO MUTE, THE R-18 NOTICE, AND COMPOSITION WITH CLIENT
// MUTE (R-16, R-18, R-15).
//
// Four participants publishing both kinds (N=3, M=3); A is the host (it created
// the meeting) and drives the PER-KIND host affordance through the DOM.
//
//   1. A server-mutes B's VIDEO: MC withdraws B's video directive, so B's camera
//      ENDS (`cameraTrackLive` false AND B's video send counter flat while its
//      audio send counter advances); every receiver shows B's video slot
//      `avatar_server_muted`; B's AUDIO keeps flowing (heard at three layers);
//      B's own view shows the host's video mute and `host_off`.
//   2. A server-UNMUTES B's video: capture resumes, and B sees the
//      NON-SUPPRESSIBLE R-18 notice. It survives Escape, a click outside and a
//      real re-render (D leaves, so B's grid rebuilds); only an acknowledgement
//      clears it. B uses its inline action: client video mute AT ONCE (camera
//      off, video counter flat beside the audio mover), and the notice goes.
//   3. With B client-muted, A server-mutes and server-unmutes B's video again:
//      capture does NOT resume (camera off, video counter flat beside the audio
//      mover) and NO notice is shown; receivers show `avatar_client_muted`.
//   4. B unmutes itself: seen again at three layers.
//
// The patched-client ingress-drop half (a client that keeps sending while
// server-muted) belongs to the media-handler Rust env-tests
// (`crates/env-tests/tests/35_mc_server_mute_teardown.rs` and MH-A1's
// relabel-while-muted case); a conforming browser cannot exercise it.

import { expect, test } from 'playwright/test';
import { asAudioMember, tonesOf } from './cohortContexts.js';
import type { CohortMemberPlan } from './cohort.js';
import { setServerMuteViaUi, VIDEO_SLOT_STATE_LIVENESS_MS } from './fixtures.js';
import {
  expectHearsAtAllLayers,
  expectSendTracks,
  observeWindow,
  sentProbe,
  videoSentProbe,
} from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3 R-16 / R-18: server video mute, per kind, and the resume notice', () => {
  test('server video mute ends the camera everywhere; unmute resumes with a non-suppressible notice; client mute composes', async ({
    browser,
  }) => {
    test.setTimeout(360_000);
    const { scenario, host } = await VideoScenario.create(browser, 'E2E server mute video', BOTH);
    try {
      await scenario.join(host, BOTH);
      for (let i = 1; i < 4; i += 1) await scenario.join(await scenario.open(i, BOTH), BOTH);
      const a = scenario.member('A');
      const b = scenario.member('B');
      const c = scenario.member('C');
      const d = scenario.member('D');
      const live = {
        directed: true,
        clientVideoMuted: false,
        cameraHealthy: true,
        publishesAudio: true,
      };
      for (const r of [a, c, d]) await scenario.expectVideo(r.label, 'baseline');
      await expectSendTracks(b, live);

      // 1. The host server-mutes B's VIDEO through the per-kind DOM affordance.
      await setServerMuteViaUi(a.page, b.joined.participantId, 'video', true);
      scenario.muted('B', 'video', { server: true });
      await expectSendTracks(b, { ...live, directed: false });
      await scenario.diagnosed('B server-muted: B sends no video', () =>
        observeWindow(
          [videoSentProbe(b, 'flat'), sentProbe(b, 'advance'), videoSentProbe(c, 'advance')],
          'B video server-muted',
        ),
      );
      for (const r of [a, c, d]) await scenario.expectVideo(r.label, 'B video server-muted');
      // The OTHER kind keeps flowing.
      const audio = [a, b, c, d].map(asAudioMember);
      await expectHearsAtAllLayers(
        audio[0]!,
        audio[1]!,
        tonesOf(audio),
        scenario.admissionBaseline,
      );
      await expect(b.page.getByTestId('server-mute-state-video')).toHaveAttribute(
        'data-server-muted',
        'true',
      );
      await expect(b.page.getByTestId('server-mute-state')).toHaveAttribute(
        'data-server-muted',
        'false',
      );
      await expect(b.page.getByTestId('self-camera-state')).toHaveAttribute(
        'data-camera-state',
        'host_off',
      );
      await expect(b.page.getByTestId('resume-notice-video')).toHaveCount(0);

      // 2. Server UNMUTE: capture resumes, and the R-18 notice appears.
      await setServerMuteViaUi(a.page, b.joined.participantId, 'video', false);
      scenario.muted('B', 'video', { server: false });
      const notice = b.page.getByTestId('resume-notice-video');
      await expect(notice, 'R-18: B is notified, at that moment').toBeVisible({
        timeout: VIDEO_SLOT_STATE_LIVENESS_MS,
      });
      await expect(notice).toHaveAttribute('role', 'alertdialog');
      await expectSendTracks(b, live);
      // Non-suppressible: Escape, a click outside, and a REAL re-render.
      await b.page.keyboard.press('Escape');
      await b.page.getByTestId('media-status').click();
      await scenario.leave('D');
      for (const r of [a, b, c]) await scenario.expectVideo(r.label, 'D left (B grid rebuilt)');
      await expect(
        notice,
        'the notice survives Escape, a click outside and a re-render',
      ).toBeVisible();
      // The inline action: client video mute AT ONCE.
      await b.page.getByTestId('resume-notice-mute-video').click();
      scenario.muted('B', 'video', { client: true });
      await expect(notice).toHaveCount(0);
      await expect(b.page.getByTestId('video-mute-state')).toHaveAttribute('data-muted', 'true');
      await expectSendTracks(b, { ...live, clientVideoMuted: true });
      await scenario.diagnosed('B muted from the notice: no video leaves B', () =>
        observeWindow(
          [videoSentProbe(b, 'flat'), sentProbe(b, 'advance'), videoSentProbe(c, 'advance')],
          'B client video mute from the R-18 notice',
        ),
      );

      // 3. Client-muted B: a server mute + unmute does NOT resume capture, no notice.
      await setServerMuteViaUi(a.page, b.joined.participantId, 'video', true);
      scenario.muted('B', 'video', { server: true });
      for (const r of [a, c])
        await scenario.expectVideo(r.label, 'B server-muted while client-muted');
      await setServerMuteViaUi(a.page, b.joined.participantId, 'video', false);
      scenario.muted('B', 'video', { server: false });
      for (const r of [a, c])
        await scenario.expectVideo(r.label, 'B server-unmuted, still client-muted');
      await expectSendTracks(b, { ...live, clientVideoMuted: true });
      await scenario.diagnosed('client-muted B: server unmute resumes nothing', () =>
        observeWindow(
          [videoSentProbe(b, 'flat'), sentProbe(b, 'advance'), videoSentProbe(c, 'advance')],
          'B server-unmuted while client-muted',
        ),
      );
      await expect(notice, 'no resume, so no notice').toHaveCount(0);

      // 4. B unmutes itself: seen again.
      await b.page.getByTestId('video-mute-toggle').click();
      scenario.muted('B', 'video', { client: false });
      await expectSendTracks(b, live);
      for (const r of [a, c]) await scenario.expectVideo(r.label, 'B unmuted itself');
    } finally {
      await scenario.close();
    }
  });
});
