// File: packages/web-app/e2e/server-mute.spec.ts
//
// Story 2 S3 — SERVER MUTE (R-8, R-9, R-10, R-11; ADR-0036 §5, §7) — and the
// browser-tier proof of CLIENT STRUCTURAL MUTE that story 2 task 6 removed with
// the loopback spec and task 15 owns restoring.
//
// Two different things are called muting (`signaling.proto`, "Mute state"):
//   * client mute — the participant about themselves, enforced at CAPTURE: no
//     media leaves the device, so its own send counter goes FLAT;
//   * server mute — the host about someone else. Since story 3 MC-B MC
//     WITHDRAWS that kind's send directive (empty targets), so the muted
//     client's send counter ALSO goes flat and nobody receives; MH's ingress
//     drop is the backstop against a patched client, proved by the Rust
//     patched-publisher env-test (`crates/env-tests/tests/35_mc_server_mute_teardown.rs`),
//     which a conforming browser cannot exercise.
// The send counter therefore no longer tells them apart; the slot state
// (`avatar_server_muted` vs `avatar_client_muted`), the roster tokens and the
// muted client's own `server-mute-state` do, and every window below samples the
// counters beside a LIVE control in the same window (flat alone is vacuous),
// each from that participant's own E2E bus (never a Prometheus aggregate).
//
// Wire tokens only: slot `data-slot-state`, roster `data-server-muted` /
// `data-client-muted`, and the SDK error's `serverCode` on the bus.

import { expect, test } from 'playwright/test';
import {
  closeMembers,
  joinMember,
  openMember,
  tonesOf,
  type JoinedMember,
  type Member,
} from './cohortContexts.js';
import {
  bootstrapMeeting,
  expectEgressAdvances,
  expectEgressFlatWhileMuted,
  expectIngressAdvances,
  latestBusEventsOf,
  setMuteViaUi,
} from './fixtures.js';
import { mhAdmissionRejectionsByInstance } from './mcMetrics.js';
import {
  acceptedProbe,
  expectHearsAtAllLayers,
  expectToneAbsent,
  latestAssignments,
  observeWindow,
  sentProbe,
  waitForRosterToken,
  waitForSenderSlotState,
  type Probe,
} from './receiveEvidence.js';

/** Click the host affordance for `target` in `actor`'s roster. */
async function clickServerMute(actor: JoinedMember, target: JoinedMember): Promise<void> {
  await actor.page.getByTestId(`server-mute-audio-${target.joined.participantId}`).click();
}

/** A probe that reads 1 while `receiver`'s slot for `sender` reads a muted avatar state. */
function mutedSlotProbe(receiver: JoinedMember, sender: JoinedMember): Probe {
  return {
    label: `${receiver.label}.slot(${sender.label}).isMutedAvatar`,
    observer: receiver.label,
    expect: 'zero',
    read: async () =>
      (await latestAssignments(receiver)).some(
        (a) =>
          a.senderId === sender.tone.senderId &&
          (a.slotState === 'avatar_server_muted' || a.slotState === 'avatar_client_muted'),
      )
        ? 1
        : 0,
  };
}

test.describe('story 2 S3: server mute, and client structural mute', () => {
  test('a host server-mutes B: every receiver goes flat on B and B stops sending; B cannot self-clear; unmute restores', async ({
    browser,
  }) => {
    test.setTimeout(240_000);
    const opened: Member[] = [];
    try {
      for (let i = 0; i < 4; i += 1) opened.push(await openMember(browser, i));
      const meetingCode = await bootstrapMeeting(opened[0]!.token, 'E2E S3 server mute');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      const members: JoinedMember[] = [];
      for (const m of opened) members.push(await joinMember(m, meetingCode));
      const [host, b, c, d] = members as [JoinedMember, JoinedMember, JoinedMember, JoinedMember];
      const cohort = tonesOf(members);
      const receivers = [host, c, d];

      // Baseline: every receiver hears B.
      for (const r of receivers) await expectHearsAtAllLayers(r, b, cohort, admissionBaseline);

      // The host mutes B through the REAL affordance in a real host context (the
      // end-to-end check that the token role hint and MC's authority agree).
      await clickServerMute(host, b);
      for (const r of receivers) {
        // Flat vocabulary (story 3): the server cause wins on the slot.
        await waitForSenderSlotState(r, b, 'avatar_server_muted');
        await waitForRosterToken(r, b, 'data-server-muted', 'true');
        // Distinct from client mute: a server-muted slot says nothing about
        // B's own microphone while B is server-muted.
        await waitForRosterToken(r, b, 'data-client-muted', 'unknown');
      }
      // B's own client: server-muted shown, its client mute untouched.
      await expect(b.page.getByTestId('server-mute-state')).toHaveAttribute(
        'data-server-muted',
        'true',
      );
      await expect(b.page.getByTestId('mute-state')).toHaveAttribute('data-muted', 'false');

      const flatOnB = (why: string) =>
        observeWindow(
          [
            ...receivers.map((r) => acceptedProbe(r, b, 'flat')),
            // B stops SENDING: the server mute withdrew B's audio directive
            // (story 3 R-16). MH's ingress drop is the backstop, proved in Rust.
            sentProbe(b, 'flat'),
            // B's OWN page mover (same-page rule): a server mute withdraws only
            // B's SEND, so B still receives C — B's sampler is live while its
            // send is flat.
            acceptedProbe(b, c, 'advance'),
            // The meeting is otherwise alive, in the same window.
            acceptedProbe(host, c, 'advance'),
            acceptedProbe(c, d, 'advance'),
            acceptedProbe(d, host, 'advance'),
          ],
          why,
        );
      await flatOnB('S3: B server-muted');
      for (const r of receivers) await expectToneAbsent(r, b);

      // R-10: B asks to be unmuted — the host is NOTIFIED, and nothing lifts.
      await b.page.getByTestId('request-unmute').click();
      await expect(
        host.page.getByTestId(`unmute-requested-${b.joined.participantId}`),
        "the host sees B's unmute request",
      ).toBeVisible({ timeout: 10_000 });
      await flatOnB('S3: B asked to be unmuted (R-10: only a host lifts a server mute)');
      await expect(b.page.getByTestId('server-mute-state')).toHaveAttribute(
        'data-server-muted',
        'true',
      );

      // The host unmutes: B is heard again at every receiver, all three layers.
      await clickServerMute(host, b);
      for (const r of receivers) {
        await waitForRosterToken(r, b, 'data-server-muted', 'false');
        await expectHearsAtAllLayers(r, b, cohort, admissionBaseline);
      }
      await observeWindow(
        receivers.map((r) => acceptedProbe(r, b, 'advance')),
        'S3: unmute restores B at every receiver',
      );
      expect(
        (await b.page.locator('body').textContent())?.toLowerCase() ?? '',
        'nothing calls a server mute a ban (R-10)',
      ).not.toMatch(/\bban/);
    } finally {
      await closeMembers(opened);
    }
  });

  test('client STRUCTURAL mute: egress flat at capture while others keep flowing; composes with server mute', async ({
    browser,
  }) => {
    // Restores the browser-tier proof story 2 task 6 removed with the loopback
    // spec: `expectEgressFlatWhileMuted` has a caller again, now with a SECOND
    // context that must keep receiving in the same window (flat alone is vacuous).
    test.setTimeout(180_000);
    const opened: Member[] = [];
    try {
      for (let i = 0; i < 3; i += 1) opened.push(await openMember(browser, i));
      const meetingCode = await bootstrapMeeting(opened[0]!.token, 'E2E client structural mute');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      const members: JoinedMember[] = [];
      for (const m of opened) members.push(await joinMember(m, meetingCode));
      const [host, b, c] = members as [JoinedMember, JoinedMember, JoinedMember];
      const cohort = tonesOf(members);
      await expectHearsAtAllLayers(host, c, cohort, admissionBaseline);
      await expectHearsAtAllLayers(b, c, cohort, admissionBaseline);

      // C mutes ITSELF.
      await expectEgressAdvances(c.page, 'C before mute');
      const mutedAt = await setMuteViaUi(c.page, 'audio', true);
      await Promise.all([
        expectEgressFlatWhileMuted(c.page, mutedAt),
        observeWindow(
          [
            acceptedProbe(b, host, 'advance'),
            acceptedProbe(host, b, 'advance'),
            // Two-sided at the receivers: nothing from C arrives while C is muted.
            acceptedProbe(host, c, 'flat'),
            acceptedProbe(b, c, 'flat'),
          ],
          'client mute: the rest of the meeting keeps flowing while C is flat',
        ),
      ]);
      for (const r of [host, b]) {
        await waitForSenderSlotState(r, c, 'avatar_client_muted');
        await waitForRosterToken(r, c, 'data-client-muted', 'true');
        await waitForRosterToken(r, c, 'data-server-muted', 'false');
      }

      // C unmutes: egress and C's arrival at the others resume.
      await setMuteViaUi(c.page, 'audio', false);
      await expectEgressAdvances(c.page, 'C after unmute');
      await expectIngressAdvances(host.page, 'the host still receiving after C unmuted');
      for (const r of [host, b]) await expectHearsAtAllLayers(r, c, cohort, admissionBaseline);

      // COMPOSE: B client-mutes, the host server-mutes B, B client-UNmutes.
      // B's client mute is lifted, but B STILL does not send: the server mute
      // keeps B's audio directive withdrawn (story 3 R-16), so lifting one mute
      // neither lifts nor masks the other (R-10, R-15). Nobody receives B.
      await setMuteViaUi(b.page, 'audio', true);
      await clickServerMute(host, b);
      await waitForRosterToken(c, b, 'data-server-muted', 'true');
      await setMuteViaUi(b.page, 'audio', false);
      await expect(b.page.getByTestId('server-mute-state')).toHaveAttribute(
        'data-server-muted',
        'true',
      );
      await observeWindow(
        [
          sentProbe(b, 'flat'),
          // B's own page mover (same-page rule): B still receives C.
          acceptedProbe(b, c, 'advance'),
          acceptedProbe(host, b, 'flat'),
          acceptedProbe(c, b, 'flat'),
          acceptedProbe(c, host, 'advance'),
          // The host's own mover, so its flat-on-B is read from a live sampler.
          acceptedProbe(host, c, 'advance'),
        ],
        'compose: B unmuted itself but is still server-muted',
      );
    } finally {
      await closeMembers(opened);
    }
  });

  test('a NON-host server-mute request is REFUSED by MC, and the refused client stays joined', async ({
    browser,
  }) => {
    // @security M2: hiding the button is cosmetic; MC is the authority. The
    // `forceHostControls` test lever renders the affordance in a NON-host
    // context so what is exercised is MC's refusal.
    test.setTimeout(150_000);
    const opened: Member[] = [];
    try {
      opened.push(await openMember(browser, 0));
      opened.push(await openMember(browser, 1));
      opened.push(await openMember(browser, 2, { forceHostControls: true }));
      const meetingCode = await bootstrapMeeting(opened[0]!.token, 'E2E non-host refusal');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      const members: JoinedMember[] = [];
      for (const m of opened) members.push(await joinMember(m, meetingCode));
      const [host, b, nonHost] = members as [JoinedMember, JoinedMember, JoinedMember];
      const cohort = tonesOf(members);
      await expectHearsAtAllLayers(host, b, cohort, admissionBaseline);
      await expectHearsAtAllLayers(nonHost, b, cohort, admissionBaseline);

      await clickServerMute(nonHost, b);
      await expect
        .poll(
          async () => (await latestBusEventsOf(nonHost.page, ['error'])).error?.['serverCode'],
          {
            message: "the non-host's request must come back as MC's FORBIDDEN refusal",
            timeout: 10_000,
          },
        )
        .toBe('FORBIDDEN');
      // The refusal is a request-level answer: the refused client is still in
      // the meeting (post-join FORBIDDEN does not close the connection).
      await expect(nonHost.page.getByTestId('meeting-state')).toHaveText('joined');
      // B is NOT muted — shown with movers, never by an absence alone.
      await observeWindow(
        [
          acceptedProbe(host, b, 'advance'),
          acceptedProbe(nonHost, b, 'advance'),
          acceptedProbe(b, nonHost, 'advance'),
          mutedSlotProbe(host, b),
          mutedSlotProbe(nonHost, b),
        ],
        'M2: a refused server mute changes nothing, and the refused client keeps receiving',
      );
      await waitForRosterToken(host, b, 'data-server-muted', 'false');
    } finally {
      await closeMembers(opened);
    }
  });
});
