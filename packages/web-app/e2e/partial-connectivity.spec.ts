// File: packages/web-app/e2e/partial-connectivity.spec.ts
//
// Story 2 S10a — PARTIAL CONNECTIVITY (R-33; ADR-0036 §9), in real browsers.
//
// Visibility follows OBSERVED connectivity: a participant hears exactly the
// senders it shares at least one connected handler with, and a rostered
// participant it shares none with is reported to it as UNREACHABLE on the wire
// (`StreamAssignments.unreachable_sender_ids`) — never inferred from silence,
// and rendered distinctly from fewer-sources and from muted.
//
// The partition is made the way it happens for real — some contexts cannot
// reach some handlers — by the dev-only `blockHandlers` test lever
// (`src/lib/testLevers.ts`, `DT_TEST_LEVERS=1`, bundle-absence-proven). The lever
// only REFUSES dials to URLs MC offered; the URLs are taken from A's own
// `joined.mediaServers` and chosen by EXACT MATCH, never by position (MC: "order
// carries no meaning") and never by pod ordinal, so no infra topology appears
// here. The Kind MC image carries no placement lever and needs none.
//
//   A: every handler.   B: every handler except u1.   C: every handler except u0.
//   => A hears B and C; B and C each hear only A; B and C see each other on the
//      roster marked `source_unreachable`.
//
// The routing itself is ALSO proven below the browser by the Rust mock-client
// env-test from story task 20
// (`crates/env-tests/tests/27_mc_slot_placement.rs`,
// `test_canonical_partial_connectivity_across_two_handlers`); this spec adds the
// real client's per-entity hearing and the rendered state.

import { expect, test } from 'playwright/test';
import {
  closeMembers,
  joinMember,
  openMember,
  type JoinedMember,
  type Member,
} from './cohortContexts.js';
import { bootstrapMeeting, busEvents } from './fixtures.js';
import { mhAdmissionRejectionsByInstance } from './mcMetrics.js';
import {
  acceptedProbe,
  expectHearsAtAllLayers,
  latestAssignments,
  observeWindow,
  waitForRosterToken,
  type Probe,
} from './receiveEvidence.js';

/** The handler URLs this context's own bus reports connected. */
async function connectedSet(member: JoinedMember): Promise<string[]> {
  return (await busEvents(member.page))
    .filter((e) => e['type'] === 'mediaConnected')
    .map((e) => e['mhUrl'] as string)
    .sort();
}

/** How many handlers `member`'s own bus reports connected — must stay flat. */
function connectedCountProbe(member: JoinedMember): Probe {
  return {
    label: `${member.label}.connectedHandlers`,
    observer: member.label,
    expect: 'flat',
    read: async () => (await connectedSet(member)).length,
  };
}

/** 1 once `receiver`'s roster row for `subject` has LOST its unreachable mark. */
function unreachableMarkLostProbe(receiver: JoinedMember, subject: JoinedMember): Probe {
  return {
    label: `${receiver.label}.roster(${subject.label}).unreachableMarkLost`,
    observer: receiver.label,
    expect: 'zero',
    read: async () =>
      (await receiver.page
        .getByTestId(`participant-${subject.joined.participantId}`)
        .getAttribute('data-reachability')) === 'source_unreachable'
        ? 0
        : 1,
  };
}

test.describe('story 2 S10a: partial connectivity (client handler lever)', () => {
  test('A on both handlers, B and C on one each: A hears both, B and C hear only A and see each other unreachable', async ({
    browser,
  }) => {
    test.setTimeout(180_000);
    const opened: Member[] = [];
    try {
      const aMember = await openMember(browser, 0);
      opened.push(aMember);
      const meetingCode = await bootstrapMeeting(aMember.token, 'E2E S10a partial connectivity');
      const admissionBaseline = await mhAdmissionRejectionsByInstance();
      const a = await joinMember(aMember, meetingCode);
      const offered = [...a.joined.mediaServers].sort();
      expect(
        offered.length,
        'S10a needs at least two handlers in the meeting set (the Kind environment runs two)',
      ).toBeGreaterThanOrEqual(2);
      const [u0, u1] = offered as [string, string];

      const bMember = await openMember(browser, 1, { blockHandlers: [u1] });
      opened.push(bMember);
      const b = await joinMember(bMember, meetingCode, { expectAllHandlers: false });
      const cMember = await openMember(browser, 2, { blockHandlers: [u0] });
      opened.push(cMember);
      const c = await joinMember(cMember, meetingCode, { expectAllHandlers: false });

      // The lever's premise, proven BEFORE any hearing assertion (@test G3):
      // everyone was offered the same set, and each connected where intended.
      for (const m of [b, c]) {
        expect([...m.joined.mediaServers].sort(), `${m.label} was offered A's handler set`).toEqual(
          offered,
        );
      }
      await expect
        .poll(() => connectedSet(a), { message: 'A connects to every handler' })
        .toEqual(offered);
      await expect.poll(() => connectedSet(b), { message: 'B connects to u0 only' }).toEqual([u0]);
      await expect.poll(() => connectedSet(c), { message: 'C connects to u1 only' }).toEqual([u1]);

      // A hears B and C; B and C each hear A — all three layers.
      const cohort = [a.tone, b.tone, c.tone];
      await expectHearsAtAllLayers(a, b, cohort, admissionBaseline);
      await expectHearsAtAllLayers(a, c, cohort, admissionBaseline);
      await expectHearsAtAllLayers(b, a, cohort, admissionBaseline);
      await expectHearsAtAllLayers(c, a, cohort, admissionBaseline);

      // B and C see each other ON the roster, marked with the WIRE token —
      // distinct from muted, and occupying no slot (so never fewer-sources).
      for (const [viewer, other] of [
        [b, c],
        [c, b],
      ] as const) {
        await waitForRosterToken(viewer, other, 'data-reachability', 'source_unreachable');
        await waitForRosterToken(viewer, other, 'data-server-muted', 'false');
        expect(
          (await latestAssignments(viewer)).some((s) => s.senderId === other.tone.senderId),
          `${viewer.label}: an unreachable participant occupies no slot`,
        ).toBe(false);
        const unreachable = (await busEvents(viewer.page))
          .filter((e) => e['type'] === 'slotAssignments')
          .at(-1)?.['unreachableSenderIds'];
        expect(unreachable, `${viewer.label}: the wire set names ${other.label}`).toEqual([
          other.tone.senderId,
        ]);
      }
      // A, connected everywhere, sees nobody unreachable.
      for (const other of [b, c])
        await waitForRosterToken(a, other, 'data-reachability', 'reachable');

      // Two-sided, in ONE window: nothing from C at B (and B at C) WHILE A's
      // frames advance at both, and the unreachable mark holds (structurally
      // persistent — it does not flicker away while nothing changes).
      await observeWindow(
        [
          acceptedProbe(b, c, 'zero'),
          acceptedProbe(b, a, 'advance'),
          acceptedProbe(c, b, 'zero'),
          acceptedProbe(c, a, 'advance'),
          unreachableMarkLostProbe(b, c),
          unreachableMarkLostProbe(c, b),
          // No late extra connect sneaks in during the window.
          connectedCountProbe(b),
          connectedCountProbe(c),
        ],
        'S10a: B and C share no handler; each still hears A',
      );
    } finally {
      await closeMembers(opened);
    }
  });
});
