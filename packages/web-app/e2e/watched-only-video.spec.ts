// File: packages/web-app/e2e/watched-only-video.spec.ts
//
// Story 3 — WATCHED-ONLY VIDEO DIRECTIVES: THE CAMERA IS OFF WHEN UNWATCHED (R-8).
//
// MC directs a publisher's video only while some subscriber holds it in a video
// slot, and withdraws the directive when the last watcher leaves. The camera
// light follows: live <=> (directed AND NOT client-muted AND camera healthy).
// B runs in CAMERA mode (`forceCameraCapture`) with a test-owned `getUserMedia`
// recorder, so the REAL track's `readyState` is asserted beside the SDK's
// `cameraTrackLive` and B's own video send counter. (B, not the host A, is the
// camera-mode subject: the recorder must be installed before navigation.)
//
//   1. A and B both declare M=0: nobody watches B's video. B's camera is OFF —
//      no live camera track, `cameraTrackLive` false, video send counter ZERO —
//      while B's audio send counter ADVANCES in the same window.
//   2. C joins with M=3: it watches B, so B is directed: a live camera track,
//      `cameraTrackLive` true, and C sees B at layers 1/2 (B is camera mode).
//   3. C leaves: the last watcher is gone, the directive is withdrawn, and B's
//      camera track ENDS.

import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import {
  expectCameraTrackEnded,
  expectNewLiveCameraTrack,
  installVideoTrackRecorder,
  recordedVideoTrackStates,
} from './fixtures.js';
import { expectSendTracks, observeWindow, sentProbe, videoSentProbe } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const UNWATCHING: CohortMemberPlan = {
  publish: 'both',
  audioReceiveSlots: 3,
  videoReceiveSlots: 0,
};
const WATCHING: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 3, videoReceiveSlots: 3 };

test.describe('story 3 R-8: video is directed only while watched', () => {
  test('an unwatched camera is off; a watcher lights it; the last watcher leaving ends it', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const { scenario, host } = await VideoScenario.create(
      browser,
      'E2E watched-only video',
      UNWATCHING,
    );
    try {
      await scenario.join(host, UNWATCHING);
      const b = await scenario.join(
        await scenario.open(1, UNWATCHING, { forceCameraCapture: true }, installVideoTrackRecorder),
        UNWATCHING,
      );
      const off = {
        directed: false,
        clientVideoMuted: false,
        cameraHealthy: true,
        publishesAudio: true,
      };

      // 1. Unwatched: camera off, nothing of video sent, audio flowing.
      await expectSendTracks(b, off);
      expect(
        (await recordedVideoTrackStates(b.page)).filter((s) => s === 'live'),
        'an unwatched publisher holds NO live camera track',
      ).toEqual([]);
      await scenario.diagnosed('unwatched: no video leaves B', () =>
        observeWindow([videoSentProbe(b, 'zero'), sentProbe(b, 'advance')], 'B unwatched'),
      );

      // 2. A watcher joins: B is directed, the camera lights.
      await scenario.join(await scenario.open(2, WATCHING), WATCHING);
      await expectSendTracks(b, { ...off, directed: true });
      await expectNewLiveCameraTrack(b.page, 0, 'a watcher lights the camera');
      await scenario.expectVideo('C', 'C watches A and B');

      // 3. The last watcher leaves: the directive is withdrawn, the track ends.
      const tracks = (await recordedVideoTrackStates(b.page)).length;
      await scenario.leave('C');
      await expectSendTracks(b, off);
      await expectCameraTrackEnded(b.page, tracks - 1, 'the last watcher left');
    } finally {
      await scenario.close();
    }
  });
});
