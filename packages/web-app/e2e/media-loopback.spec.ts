// File: packages/web-app/e2e/media-loopback.spec.ts
//
// ADR-0036 story 2 R-3 (loopback REMOVED): a participant never hears their own
// audio. Alone in a meeting, a client hears nothing — with an explicit
// "fewer sources" slot state, never a spinner — and is directed to send nothing.
//
// (The file keeps its story-1 name so the Layer-7 lane's spec inventory does
// not churn; its subject is now the solo case of the multi-party model.)
//
// This is the Env-Test tier of ADR-0028: real Chromium, real WebTransport with
// `serverCertificateHashes` pinning, real QUIC to a real media handler. No
// setting that weakens certificate validation, web security or origin trust
// appears anywhere in this suite.
//
// ---------------------------------------------------------------------------
// COVERAGE LOSS, STATED LOUDLY
// ---------------------------------------------------------------------------
//
// Story 1's test 1 here was the only browser-tier proof of STRUCTURAL client
// mute: egress flat while muted, then resuming. That proof needs egress to
// advance, which needs someone to hold this client in a slot. A solo client is
// held by nobody, so egress never advances and the proof cannot live here.
//
// WHAT THE GAP IS NOW, AND WHAT IT IS NOT. It is the missing SECOND BROWSER
// CONTEXT — this spec has no helper to drive two participants — and nothing
// else. It is NOT "the pair would have no edges": that was true under task 6's
// round-robin placement, which put ranks 0 and 1 on different handlers, and it
// is false under the ADR-0036 §9 edge model, where two browsers each connected
// to both Kind handlers share both and co-location puts their edge on one. So a
// reader who knows round-robin is gone must not conclude the coverage is back:
// the edge now exists and the proof still does not.
//
// OWNER: story 2 task 15 (the multi-context browser S-tests on the task-14 N+1
// helper) must name client structural mute explicitly. Until it lands,
// `expectEgressFlatWhileMuted` has no browser caller.
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
