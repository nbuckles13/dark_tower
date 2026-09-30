// File: packages/web-app/e2e/server-mute.spec.ts
//
// Story 2 S3 — SERVER MUTE (R-8, R-9, R-10, R-11; ADR-0036 §5, §7) — and the
// browser-tier proof of CLIENT STRUCTURAL MUTE that story 2 task 6 removed with
// the loopback spec and task 15 owns restoring.
//
// Two different things are called muting (`signaling.proto`, "Mute state"):
//   * client mute — the participant about themselves, enforced at CAPTURE: no
//     media leaves the device, so its own send counter goes FLAT;
//   * server mute — the host about someone else, enforced at MH INGRESS: the
//     muted client keeps SENDING (its counter keeps rising) and nobody receives.
// The send counter is what tells them apart, so every mute window below samples
// it beside the receivers' per-sender accepted counts, each from that
// participant's own E2E bus (never a Prometheus aggregate).
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
  await actor.page.getByTestId(`server-mute-${target.joined.participantId}`).click();
}

/** A probe that reads 1 while `receiver`'s slot for `sender` reads source_muted. */
function mutedSlotProbe(receiver: JoinedMember, sender: JoinedMember): Probe {
  return {
    label: `${receiver.label}.slot(${sender.label}).isSourceMuted`,
    expect: 'zero',
    read: async () =>
      (await latestAssignments(receiver)).some(
        (a) => a.senderId === sender.tone.senderId && a.slotState === 'source_muted',
      )
        ? 1
        : 0,
  };
}

test.describe('story 2 S3: server mute, and client structural mute', () => {
  test('a host server-mutes B: every receiver goes flat on B while B keeps sending; B cannot self-clear; unmute restores', async ({
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
        await waitForSenderSlotState(r, b, 'source_muted');
        await waitForRosterToken(r, b, 'data-server-muted', 'true');
        // Distinct from client mute: the slot's source_muted says nothing about
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
            // B keeps SENDING — the difference from client mute (R-9: MH drops at ingress).
            sentProbe(b, 'advance'),
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
      const mutedAt = await setMuteViaUi(c.page, true);
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
        await waitForSenderSlotState(r, c, 'source_muted');
        await waitForRosterToken(r, c, 'data-client-muted', 'true');
        await waitForRosterToken(r, c, 'data-server-muted', 'false');
      }

      // C unmutes: egress and C's arrival at the others resume.
      await setMuteViaUi(c.page, false);
      await expectEgressAdvances(c.page, 'C after unmute');
      await expectIngressAdvances(host.page, 'the host still receiving after C unmuted');
      for (const r of [host, b]) await expectHearsAtAllLayers(r, c, cohort, admissionBaseline);

      // COMPOSE: B client-mutes, the host server-mutes B, B client-UNmutes. B's
      // egress resumes (client mute lifted) but nobody receives B (server mute
      // holds) — the two mutes are independent (R-10).
      await setMuteViaUi(b.page, true);
      await clickServerMute(host, b);
      await waitForRosterToken(c, b, 'data-server-muted', 'true');
      await setMuteViaUi(b.page, false);
      await expect(b.page.getByTestId('server-mute-state')).toHaveAttribute(
        'data-server-muted',
        'true',
      );
      await observeWindow(
        [
          sentProbe(b, 'advance'),
          acceptedProbe(host, b, 'flat'),
          acceptedProbe(c, b, 'flat'),
          acceptedProbe(c, host, 'advance'),
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
