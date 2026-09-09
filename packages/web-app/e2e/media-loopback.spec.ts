// File: packages/web-app/e2e/media-loopback.spec.ts
//
// Task #20 (ADR-0036 story 1): the headline objective, in a real browser against
// the live cluster — **a participant hears their own audio returned through MH**,
// and muting stops it.
//
// This is the Env-Test tier of ADR-0028: real Chromium, real WebTransport with
// `serverCertificateHashes` pinning, real WebCodecs, real WebCrypto, real QUIC
// datagrams through a real media handler. Capture is synthesized and the
// microphone permission auto-granted by the fake-device / fake-ui launch flags
// already present in `playwright.config.ts` — nothing is added here, and no
// setting that weakens certificate validation, web security or origin trust
// appears anywhere in this suite. Such a setting would make the pinning
// assertion decoration while every test below stayed green.
//
// ---------------------------------------------------------------------------
// WHAT THIS PROVES THAT THE UNIT TIER CANNOT
// ---------------------------------------------------------------------------
//
// `sdk-core`'s loopback test feeds a pipeline's own egress into its own ingress.
// That proves WIRING and nothing about composition. Here the frames actually
// leave the machine, are routed by MC's assignment and forwarded by MH, and come
// back — so this is the first place the client's codec, the Rust relay, and the
// controller's slot assignment are proven to agree with each other.
//
// ---------------------------------------------------------------------------
// TWO ASSERTION DISCIPLINES, BOTH LOAD-BEARING
// ---------------------------------------------------------------------------
//
// 1. **Latency is OBSERVED, NEVER GATED** (§10). The functional pass/fail is
//    whether audio came back AT ALL. The measured round trip is annotated and
//    printed; nothing compares it to a threshold. A wall-clock target on a local
//    cluster is a permanent flake, ADR-0028 forbids quarantining gates, so the
//    test would be deleted and the headline objective would end with zero
//    coverage. If a threshold ever appears here, that is the defect §10
//    describes — not a tightening.
//
// 2. **Mute is proven STRUCTURALLY, not acoustically.** Sampling audio energy
//    would show only that this client's playback went quiet, which is equally
//    what a dead decoder looks like. A flat send counter proves no encoded audio
//    LEFT THE DEVICE, which is what §5 actually requires.
//
// Every assertion here has a positive control, because each one passes trivially
// on a pipeline that never worked: silence is indistinguishable from success for
// a mute test, and "no threshold breached" is indistinguishable from "nothing
// measured" for a latency observation.
//
// Requires a live host-side Kind cluster (see e2e/global-setup.ts + README).

import { expect, test } from 'playwright/test';
import {
  authAsSharedUser,
  bootstrapMeeting,
  expectEgressAdvances,
  expectEgressFlatWhileMuted,
  expectIngressAdvances,
  frameCountSamples,
  joinAsUser,
  setMuteViaUi,
  startAudio,
  waitForAllMediaConnected,
  waitForFirstMediaFrame,
  waitForJoined,
} from './fixtures.js';

test.describe('media loopback through MH (ADR-0036 story 1)', () => {
  test('own audio returns through MH; mute is structurally silent; unmute resumes', async ({
    page,
  }, testInfo) => {
    // Registration budget: ZERO. The shared valid user signs in (registered once
    // per run by the first spec that needs it), and the meeting is created
    // Node-side rather than through the create view.
    await page.goto('/');
    const token = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(token, 'E2E media loopback meeting');

    // --- Join, and confirm the media path's preconditions ------------------
    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);
    expect(joined.mediaServers.length, 'media_servers must not be empty').toBeGreaterThan(0);
    await waitForAllMediaConnected(page, joined.mediaServers);

    // --- Start capture -----------------------------------------------------
    await startAudio(page);

    // --- THE HEADLINE ASSERTION -------------------------------------------
    //
    // Functional pass/fail: audio this client captured, encoded, encrypted,
    // signed and sent came back through MH and completed verify -> replay ->
    // unwrap -> decrypt -> Opus decode. Every one of those steps is real.
    const firstMediaMs = await waitForFirstMediaFrame(page);

    // OBSERVED AND REPORTED, NEVER GATED. The annotation carries the number into
    // the run's artifacts; the log line puts it where someone watching a devloop
    // will see it. Neither is a comparison.
    testInfo.annotations.push({
      type: 'observed-loopback-latency-ms',
      description: `${firstMediaMs} (OBSERVED ONLY — ADR-0036 §10 forbids gating on this)`,
    });
    console.log(`[media-loopback] observed round trip to first decoded frame: ${firstMediaMs}ms`);

    // The ONLY assertions on the number: that it is a real measurement. A
    // threshold here would be the §10 defect.
    expect(Number.isFinite(firstMediaMs), 'first-media latency must be a real measurement').toBe(
      true,
    );
    expect(firstMediaMs).toBeGreaterThanOrEqual(0);

    // --- Positive control, before anything is asserted about mute ----------
    //
    // Without this, every assertion below passes just as well on a pipeline that
    // never sent a frame: "flat while muted" is trivially true of a dead client.
    await expectEgressAdvances(page, 'before mute');

    // --- Mute: the indicator and the send path must agree ------------------
    const mutedAtMs = await setMuteViaUi(page, true);
    // `setMuteViaUi` already asserts that the DOM indicator and the SDK's
    // client-mute state on the bus agree. That pairing is the point: an
    // indicator that can disagree with what gates capture is a hot mic wearing a
    // "muted" label.
    await expectEgressFlatWhileMuted(page, mutedAtMs);

    // --- Unmute: audio resumes, at BOTH ends -------------------------------
    await setMuteViaUi(page, false);
    // Sent: the capture gate reopened.
    await expectEgressAdvances(page, 'after unmute');
    // AND accepted: the audio is actually coming back. Asserting only the send
    // side would call it a pass if MH had stopped forwarding during the mute —
    // the resumption failure that matters most to a user, and the one a
    // send-only assertion cannot see.
    await expectIngressAdvances(page, 'after unmute');

    // --- Report what was observed, for the devloop record ------------------
    const samples = await frameCountSamples(page);
    const last = samples[samples.length - 1];
    console.log(
      `[media-loopback] final counters: sent=${last?.framesSent} accepted=${last?.framesAccepted} ` +
        `over ${samples.length} samples`,
    );
  });

  test('slot state is rendered from the wire, not inferred from silence', async ({ page }) => {
    // ADR-0036 §6: "Slot state is explicit on the wire. Absence of frames is not
    // a signal." This asserts the client renders MC's assignment rather than
    // guessing — the property that keeps a congestion hold, an under-filled grid
    // and an unreachable participant from all showing the same spinner.
    await page.goto('/');
    const token = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(token, 'E2E slot state meeting');

    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);
    await waitForAllMediaConnected(page, joined.mediaServers);
    await startAudio(page);

    const slot = page.getByTestId('slot-0');
    await expect(slot, 'the declared slot must be rendered').toBeVisible();

    // The attribute carries the WIRE token, so this asserts on the protocol's
    // vocabulary rather than on display copy a copy edit could change.
    await expect
      .poll(async () => slot.getAttribute('data-slot-state'), {
        message:
          'the slot must reach an MC-assigned state. Staying at "awaiting-assignment" means ' +
          'no StreamAssignments arrived — which is a REAL condition the client renders ' +
          'honestly rather than a test failure to paper over, but it means the controller ' +
          'never filled the slot this client declared.',
        timeout: 20_000,
      })
      .not.toBe('awaiting-assignment');
  });
});
