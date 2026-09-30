// File: packages/web-app/e2e/solo-participant.spec.ts
//
// ADR-0036 story 2 R-3: a participant never hears their own audio. Alone in a
// meeting, a client hears nothing — with an explicit "fewer sources" slot state,
// never a spinner — and is directed to send nothing.
//
// (Formerly `media-loopback.spec.ts`: story 1's hear-yourself spec, whose
// expectation R-3 superseded. Renamed in story 2 task 15 so the name no longer
// promises a loopback that no longer exists.)
//
// This is the Env-Test tier of ADR-0028: real Chromium, real WebTransport with
// `serverCertificateHashes` pinning, real QUIC to a real media handler. No
// setting that weakens certificate validation, web security or origin trust
// appears anywhere in this suite.
//
// Browser-tier proof of client STRUCTURAL mute (egress flat while muted) needs a
// second participant to hold this client in a slot, so it lives with the
// multi-party specs: `server-mute.spec.ts`, "client STRUCTURAL mute".
//
// ---------------------------------------------------------------------------
// WHY FLAT IS NOT VACUOUS HERE
// ---------------------------------------------------------------------------
//
// "Nothing moved" is trivially true of a client that never started. So the
// PRIMARY assertion is a specific, non-default wire token: slot 0 polls to
// exactly `fewer_sources`, which proves MC's assignment ARRIVED. Only after
// that are the flat counters meaningful, and the flat-window helper carries its
// own sample-count vacuity check.
//
// Requires a live host-side Kind cluster (see e2e/global-setup.ts + README).

import { expect, test } from 'playwright/test';
import {
  authAsSharedUser,
  bootstrapMeeting,
  expectCountersFlatOverWindow,
  joinAsUser,
  startAudio,
  waitForAllMediaConnected,
  waitForJoined,
} from './fixtures.js';

test.describe('solo participant: loopback removed (ADR-0036 story 2 R-3)', () => {
  test('a solo participant hears nothing and is directed to send nothing', async ({ page }) => {
    // Registration budget: ZERO. The shared valid user signs in, and the
    // meeting is created Node-side rather than through the create view.
    await page.goto('/');
    const token = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(token, 'E2E solo participant meeting');

    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);
    // R-33, observable from the browser: MC offers EVERY registered handler and
    // the transport dials all of them (active/active). The claim lives in
    // `waitForAllMediaConnected`, which diffs connected against offered and names
    // what is missing — strictly stronger than the handler COUNT that used to be
    // asserted here, back when MC scoped the list to one placed handler.
    //
    // DELIBERATELY NOT A COUNT: `e2e/env.ts` keeps MH endpoints out of this tier
    // by design, so an "equals the full registered set" assertion could only be
    // satisfied by hard-coding 2 or importing Kind topology. Registered-set
    // equality is asserted where the set is knowable — MC's own tests and
    // `crates/env-tests/tests/27_mc_slot_placement.rs`.
    expect(joined.mediaServers.length, 'media_servers must not be empty').toBeGreaterThan(0);
    await waitForAllMediaConnected(page, joined.mediaServers);
    await startAudio(page);

    // PRIMARY — the positive control. A specific, non-default wire token: MC's
    // assignment arrived and says "nobody to hear", not merely "not awaiting".
    const slot = page.getByTestId('slot-0');
    await expect(slot, 'the declared slot must be rendered').toBeVisible();
    await expect
      .poll(async () => slot.getAttribute('data-slot-state'), {
        message:
          'a solo participant must see its slot in the explicit fewer-sources state (R-3). ' +
          '"awaiting-assignment" means no StreamAssignments arrived; "active" means MC routed ' +
          'the participant to itself — the loopback this story removes.',
        timeout: 20_000,
      })
      .toBe('fewer_sources');
    const assignedAtMs = Date.now();

    // SECONDARY — both counters flat. Egress is flat because MC directs no
    // audio stream to a publisher nobody holds (a SendDirective with NO
    // streams, ADR-0036 §5), so no send instruction is ever applied — NOT
    // because of mute: nothing here mutes. Ingress is flat
    // because MC pushed no edge into this client. `waitForFirstMediaFrame` is
    // deliberately not called: no frame is ever expected.
    await expectCountersFlatOverWindow(page, {
      fromMs: assignedAtMs,
      fields: ['framesSent', 'framesAccepted'],
      whyFlat:
        'a solo participant is held by nobody (a SendDirective with no streams, §5) and holds nobody ' +
        '(no edge into it) — a moving counter means a self-edge survived (R-3).',
    });
  });

  test('slot state is rendered from the wire, not inferred from silence', async ({ page }) => {
    // ADR-0036 §6: "Slot state is explicit on the wire. Absence of frames is not
    // a signal." For a solo participant the wire says, specifically,
    // `fewer_sources` — asserting merely "not awaiting-assignment" would be
    // vacuous under R-3.
    await page.goto('/');
    const token = await authAsSharedUser(page);
    const meetingCode = await bootstrapMeeting(token, 'E2E slot state meeting');

    await joinAsUser(page, meetingCode);
    const joined = await waitForJoined(page);
    // Non-empty plus connect-to-all; see the first test for why there is no
    // handler-count assertion at this tier.
    expect(joined.mediaServers.length, 'media_servers must not be empty').toBeGreaterThan(0);
    await waitForAllMediaConnected(page, joined.mediaServers);
    await startAudio(page);

    const slot = page.getByTestId('slot-0');
    await expect(slot, 'the declared slot must be rendered').toBeVisible();
    // The attribute carries the WIRE token, so this asserts on the protocol's
    // vocabulary rather than on display copy a copy edit could change.
    await expect
      .poll(async () => slot.getAttribute('data-slot-state'), {
        message: "a solo participant's slot must reach exactly the fewer-sources state",
        timeout: 20_000,
      })
      .toBe('fewer_sources');
  });
});
