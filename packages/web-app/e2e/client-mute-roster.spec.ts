// File: packages/web-app/e2e/client-mute-roster.spec.ts
//
// Story 3 R-15 — CLIENT MUTE IS A ROSTER FACT, visible to everyone (ADR-0036 §5
// amendment 2026-10-24, reversing story 2's S-2). The 2026-10-24 manual-test
// defect: an observer showed a participant NO slot of its own carried as
// "mic: unknown" / "camera: unknown", because client mute was read from slots.
//
// Four participants, all publishing both kinds, every one declaring ONE audio
// and ONE video slot. JOIN ORDER IS THE PRECONDITION (R-3 fill rule (2): the
// earliest eligible joiner without a slot of the kind; pair completion takes
// the other kind):
//
//   1. DECOY joins first — it fills the OBSERVER's 1+1 slots when the observer
//      joins (earliest joiner).
//   2. SUBJECT joins, then client-mutes its CAMERA before the observer exists.
//   3. OBSERVER joins: it learns SUBJECT from `existing_participants`
//      (mic on, camera off by them) — explicit, never `unknown`.
//   4. SUBJECT toggles mic mute, mic unmute, camera unmute, camera mute: the
//      observer's row follows each.
//   5. LATE joins while the observer runs a DOM recorder: the observer learns
//      LATE via `ParticipantJoined` (both fields ABSENT — MC has no fact until
//      LATE's post-join report) and LATE's report. The recorder proves no WRONG
//      value ever rendered (a `true` before LATE muted anything); then LATE
//      mutes its mic and the observer follows. The app has no pre-join mute,
//      so ParticipantJoined-absent vs a stray `false` is not distinguishable
//      in the DOM here; "absent stays unknown" is pinned at the SDK and store
//      unit tiers, where ordering is controlled.
//
// SUBJECT and LATE hold NO slot of EITHER kind at the observer, so nothing here
// can pass on slot state. That is asserted from the test's own slot oracle AND
// cross-checked against the observer's bus, before and after every toggle
// (a mute never moves a slot — stability), failing loudly with its own message.
//
// No sleeps; liveness-bounded polls only; retries 0 (playwright.config.ts).

import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import type { PlanMember } from './cohortContexts.js';
import {
  installRosterMuteRecorder,
  latestBusEventsOf,
  recordedRosterMute,
  ROSTER_CLIENT_MUTE_ATTRIBUTE,
  setMuteViaUi,
  type MuteKind,
} from './fixtures.js';
import { waitForRosterToken } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const ONE_EACH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 1, videoReceiveSlots: 1 };

/**
 * PRECONDITION: none of `subjects` holds a slot of either kind at `observer`.
 * The oracle says so (the test's own expectation, from join order), and the
 * observer's latest bus assignments name none of their sender ids. The bus
 * check names a VIDEO avatar's sender too; a live video slot names no sender
 * (R-5), which is why `expectVideo` below confirms the oracle's occupant renders.
 */
async function expectOutsideObserverSlots(
  scenario: VideoScenario,
  observer: PlanMember,
  subjects: readonly PlanMember[],
  when: string,
): Promise<void> {
  const expected = scenario.oracle.expected(observer.label);
  const occupants = [...expected.audio, ...expected.video].map((s) => s.occupant);
  for (const s of subjects) {
    if (occupants.includes(s.label)) {
      throw new Error(
        `PRECONDITION (${when}): the oracle puts ${s.label} in a slot of ${observer.label} ` +
          `(${JSON.stringify(expected)}) — this spec could pass on slot state; fix the join order`,
      );
    }
  }
  const { slotAssignments } = await latestBusEventsOf(observer.page, ['slotAssignments']);
  const raw = slotAssignments?.['assignments'];
  if (!Array.isArray(raw) || raw.length === 0) {
    throw new Error(
      `PRECONDITION (${when}): ${observer.label} has no slot assignments on its bus — ` +
        'cannot prove the subjects are outside its slots',
    );
  }
  const named = (raw as Array<{ senderId?: string }>).map((a) => a.senderId);
  for (const s of subjects) {
    if (named.includes(s.senderId)) {
      throw new Error(
        `PRECONDITION (${when}): ${observer.label}'s bus names ${s.label} (sender ${s.senderId}) ` +
          `in a slot: ${JSON.stringify(raw)}`,
      );
    }
  }
}

/**
 * A recorded token sequence from its first FACT on: everything before it must
 * be `unknown` (no fact yet), and a fact is never followed by `unknown` (a
 * fact is never cleared while the participant stays) — so the returned tail
 * still carries any later `unknown` for the caller's exact comparison.
 */
function fromFirstFact(sequence: readonly string[]): string[] {
  const first = sequence.findIndex((v) => v !== 'unknown');
  return first < 0 ? [] : sequence.slice(first);
}

/** Poll until `observer`'s roster row for `subject` reads the client-mute token per kind. */
async function expectClientMute(
  observer: PlanMember,
  subject: PlanMember,
  want: Record<MuteKind, 'true' | 'false'>,
): Promise<void> {
  for (const kind of ['audio', 'video'] as const) {
    await waitForRosterToken(observer, subject, ROSTER_CLIENT_MUTE_ATTRIBUTE[kind], want[kind]);
  }
}

test.describe('story 3 R-15: client mute of a participant outside the observer slots', () => {
  test('the observer sees every client-mute fact from the roster, never from slots', async ({
    browser,
  }) => {
    test.setTimeout(300_000);
    const { scenario, host } = await VideoScenario.create(
      browser,
      'E2E client mute roster',
      ONE_EACH,
    );
    try {
      // 1-2. DECOY, then SUBJECT; SUBJECT turns its camera off before the observer exists.
      const decoy = await scenario.join(host, ONE_EACH);
      const subject = await scenario.join(await scenario.open(1, ONE_EACH), ONE_EACH);
      await setMuteViaUi(subject.page, 'video', true);
      scenario.muted(subject.label, 'video', { client: true });
      // Sync point: MC has the fact (the decoy, an existing observer, shows it).
      await waitForRosterToken(decoy, subject, ROSTER_CLIENT_MUTE_ATTRIBUTE.video, 'true');

      // 3. OBSERVER joins; SUBJECT reaches it on `existing_participants`.
      const observer = await scenario.join(await scenario.open(2, ONE_EACH), ONE_EACH);
      await scenario.expectVideo(observer.label, 'decoy fills the observer slots');
      await expectOutsideObserverSlots(scenario, observer, [subject], 'observer joined');
      await expectClientMute(observer, subject, { audio: 'false', video: 'true' });

      // 4. Each SUBJECT toggle reaches the observer; no slot moves.
      const toggles: ReadonlyArray<[MuteKind, boolean, Record<MuteKind, 'true' | 'false'>]> = [
        ['audio', true, { audio: 'true', video: 'true' }],
        ['audio', false, { audio: 'false', video: 'true' }],
        ['video', false, { audio: 'false', video: 'false' }],
        ['video', true, { audio: 'false', video: 'true' }],
      ];
      for (const [kind, muted, want] of toggles) {
        await setMuteViaUi(subject.page, kind, muted);
        scenario.muted(subject.label, kind, { client: muted });
        await expectClientMute(observer, subject, want);
        // A live video slot names no sender (R-5): after the camera comes back,
        // confirm the RENDERED occupant of the observer's video slot is still
        // the decoy, not the subject.
        if (kind === 'video' && !muted) {
          await scenario.expectVideo(observer.label, 'decoy still holds the observer video slot');
        }
        await expectOutsideObserverSlots(
          scenario,
          observer,
          [subject],
          `subject ${kind} ${muted ? 'muted' : 'unmuted'}`,
        );
      }

      // 5. LATE joins under the recorder: learned via ParticipantJoined + its report.
      await installRosterMuteRecorder(observer.page);
      const late = await scenario.join(await scenario.open(3, ONE_EACH), ONE_EACH);
      await scenario.expectVideo(observer.label, 'decoy still holds the observer video slot');
      await expectOutsideObserverSlots(scenario, observer, [subject, late], 'late joined');
      await expectClientMute(observer, late, { audio: 'false', video: 'false' });
      const beforeToggle = await recordedRosterMute(observer.page, late.joined.participantId);
      // LATE muted nothing yet: `true` would be a wrong value rendered; only
      // `unknown` (no fact yet) may precede the fact.
      for (const kind of ['audio', 'video'] as const) {
        expect(
          fromFirstFact(beforeToggle[kind]),
          `${observer.label}'s row for ${late.label} (${kind}) may show only 'unknown' before ` +
            `the fact 'false' — recorded ${JSON.stringify(beforeToggle[kind])}`,
        ).toEqual(['false']);
      }
      await setMuteViaUi(late.page, 'audio', true);
      scenario.muted(late.label, 'audio', { client: true });
      await expectClientMute(observer, late, { audio: 'true', video: 'false' });
      await expectOutsideObserverSlots(scenario, observer, [subject, late], 'late muted mic');
      const after = await recordedRosterMute(observer.page, late.joined.participantId);
      expect(fromFirstFact(after.audio), JSON.stringify(after.audio)).toEqual(['false', 'true']);
      expect(fromFirstFact(after.video), JSON.stringify(after.video)).toEqual(['false']);
    } finally {
      await scenario.close();
    }
  });
});
