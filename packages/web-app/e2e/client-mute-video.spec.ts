// File: packages/web-app/e2e/client-mute-video.spec.ts
//
// Story 3 — CLIENT MUTE, PER KIND (R-17, R-15).
//
// Four participants, all publishing both kinds (N=3, M=3). D runs in CAMERA
// mode (`forceCameraCapture`, test-levers build only) under the suite's fake
// device, with a test-owned `getUserMedia` recorder, so the REAL camera track's
// `readyState` is asserted — the camera light, independently of the SDK's own
// report (paired-test G5). Layer 3 does not apply to D's video (no pattern), by
// a named branch in the evidence judges; layers 1/2 and the rendered state do.
//
//   1. B mutes its VIDEO through the DOM: B's camera is off (`cameraTrackLive`
//      false) AND B's video send counter is FLAT while its audio send counter
//      ADVANCES in the same window; every other receiver shows B's video slot
//      as `avatar_client_muted`, and still sees everyone else.
//   2. C mutes its AUDIO: C's microphone track stays OPEN, C's audio send counter
//      is flat, and C's video keeps flowing (its video send counter advances, and
//      receivers still see C at three layers).
//   3. Unmutes resume: B's video is seen again at three layers; C is heard.
//   4. D (camera mode) mutes video: the recorded track ENDS; unmute hands out a
//      NEW live track.
//
// No wall-clock gate; windows on the shared stop rule; liveness bounds only.

import { expect, test } from 'playwright/test';
import { asAudioMember, tonesOf } from './cohortContexts.js';
import type { CohortMemberPlan } from './cohort.js';
import {
  expectCameraTrackEnded,
  expectNewLiveCameraTrack,
  installVideoTrackRecorder,
  recordedVideoTrackStates,
  setMuteViaUi,
} from './fixtures.js';
import {
  expectHearsAtAllLayers,
  expectSendTracks,
  observeWindow,
  sentProbe,
  videoSentProbe,
} from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const BOTH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3 R-17: client mute, per kind', () => {
  test('video mute ends the camera and shows avatar_client_muted; audio mute keeps the mic open and video flowing', async ({
    browser,
  }) => {
    test.setTimeout(300_000);
    const { scenario, host } = await VideoScenario.create(browser, 'E2E client mute video', BOTH);
    try {
      await scenario.join(host, BOTH);
      await scenario.join(await scenario.open(1, BOTH), BOTH);
      await scenario.join(await scenario.open(2, BOTH), BOTH);
      await scenario.join(
        await scenario.open(3, BOTH, { forceCameraCapture: true }, installVideoTrackRecorder),
        BOTH,
      );
      const [a, b, c, d] = ['A', 'B', 'C', 'D'].map((l) => scenario.member(l)) as [
        ReturnType<typeof scenario.member>,
        ReturnType<typeof scenario.member>,
        ReturnType<typeof scenario.member>,
        ReturnType<typeof scenario.member>,
      ];
      for (const m of [a, b, c, d]) await scenario.expectVideo(m.label, 'baseline');
      const live = {
        directed: true,
        clientVideoMuted: false,
        cameraHealthy: true,
        publishesAudio: true,
      };
      await expectSendTracks(b, live);

      // 1. B client-mutes VIDEO through the DOM.
      await setMuteViaUi(b.page, 'video', true);
      scenario.muted('B', 'video', { client: true });
      await expectSendTracks(b, { ...live, clientVideoMuted: true });
      await scenario.diagnosed('B video muted: nothing of that kind leaves B', () =>
        observeWindow(
          [videoSentProbe(b, 'flat'), sentProbe(b, 'advance'), videoSentProbe(a, 'advance')],
          'B client video mute',
        ),
      );
      for (const r of [a, c, d]) await scenario.expectVideo(r.label, 'B video client-muted');

      // 2. C client-mutes AUDIO: mic stays open, nothing sent, video flows.
      await setMuteViaUi(c.page, 'audio', true);
      scenario.muted('C', 'audio', { client: true });
      await expectSendTracks(c, live);
      await scenario.diagnosed('C audio muted: video keeps flowing', () =>
        observeWindow([sentProbe(c, 'flat'), videoSentProbe(c, 'advance')], 'C client audio mute'),
      );
      for (const r of [a, b, d]) await scenario.expectVideo(r.label, 'C audio client-muted');

      // 3. Unmute both: B is seen again, C is heard again.
      await setMuteViaUi(b.page, 'video', false);
      scenario.muted('B', 'video', { client: false });
      await setMuteViaUi(c.page, 'audio', false);
      scenario.muted('C', 'audio', { client: false });
      await expectSendTracks(b, live);
      for (const r of [a, c, d]) await scenario.expectVideo(r.label, 'B video unmuted');
      const audio = [a, b, c, d].map(asAudioMember);
      await expectHearsAtAllLayers(
        audio[0]!,
        audio[2]!,
        tonesOf(audio),
        scenario.admissionBaseline,
      );

      // 4. D (camera mode): the REAL track ends on mute; unmute hands out a new live one.
      const before = await recordedVideoTrackStates(d.page);
      expect(
        before.length,
        "anti-vacuity: the recorder saw D's camera track",
      ).toBeGreaterThanOrEqual(1);
      expect(before.at(-1)).toBe('live');
      await setMuteViaUi(d.page, 'video', true);
      scenario.muted('D', 'video', { client: true });
      await expectCameraTrackEnded(d.page, before.length - 1, 'D client video mute');
      await setMuteViaUi(d.page, 'video', false);
      scenario.muted('D', 'video', { client: false });
      await expectNewLiveCameraTrack(d.page, before.length, 'D client video unmute');
      for (const r of [a, b, c]) await scenario.expectVideo(r.label, 'D camera back');
    } finally {
      await scenario.close();
    }
  });
});
