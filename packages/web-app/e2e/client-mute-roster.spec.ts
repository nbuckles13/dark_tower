// File: packages/web-app/e2e/client-mute-roster.spec.ts
//
// Story 3 R-15 — CLIENT MUTE IS A ROSTER FACT, visible to everyone (ADR-0036 §5
// amendment 2026-10-24, reversing story 2's S-2). The 2026-10-24 manual-test
// defect: an observer showed a participant NO slot of its own carried as
// "mic: unknown" / "camera: unknown", because client mute was read from slots.
//
// Four participants, every one declaring ONE audio and ONE video slot. JOIN
// ORDER IS THE PRECONDITION (R-3 fill rule (2): the earliest eligible joiner
// without a slot of the kind; pair completion takes the other kind):
//
//   1. DECOY joins first — it fills the OBSERVER's 1+1 slots when the observer
//      joins (earliest joiner).
//   2. SUBJECT joins, then client-mutes its CAMERA before the observer exists.
//   3. OBSERVER joins: it learns SUBJECT from `existing_participants`
//      (mic live, camera off by them) — the roster entry itself, at once.
//   4. SUBJECT toggles mic mute, mic unmute, camera unmute, camera mute: the
//      observer's row follows each.
//   5. LATE joins AUDIO-ONLY while the observer runs a DOM recorder: the
//      observer learns LATE from `ParticipantJoined` ALONE (ADR-0036 §5
//      amendment 2026-10-25: the join declares every kind's state, no
//      post-join report). The recorder proves the row was CREATED with LATE's
//      exact state — mic `live`, camera `not_published` — and never rendered
//      anything else (there is no "unknown" to pass through); the camera shows
//      "not publishing" as explicit DOM text. Then LATE mutes its mic and the
//      observer follows.
//
// SUBJECT and LATE hold NO slot of EITHER kind at the observer, so nothing here
// can pass on slot state. That is asserted from the test's own slot oracle AND
// cross-checked against the observer's bus, before and after every toggle
// (a mute never moves a slot — stability), failing loudly with its own message.
//
// "Joined already muted" is NOT driven here: the app has no pre-join mute
// control, so the declaration this app sends always carries `client_muted =
// false`. The SDK's declaration of a pre-join mute and the store/row rendering
// a `ParticipantJoined` that says `client_muted` are pinned at the unit and
// component tiers.
//
// No sleeps; liveness-bounded polls only; retries 0 (playwright.config.ts).

import { expect, test } from 'playwright/test';
import type { CohortMemberPlan } from './cohort.js';
import type { PlanMember } from './cohortContexts.js';
import type { ClientMediaStateToken } from '@darktower/sdk-core';
import { INDICATOR_TEXT } from '../src/lib/participantState.js';
import {
  installRosterStateRecorder,
  latestBusEventsOf,
  recordedRosterState,
  ROSTER_CLIENT_STATE_ATTRIBUTE,
  setMuteViaUi,
  type MuteKind,
} from './fixtures.js';
import { waitForRosterToken } from './receiveEvidence.js';
import { VideoScenario } from './videoScenario.js';

const ONE_EACH: CohortMemberPlan = { publish: 'both', audioReceiveSlots: 1, videoReceiveSlots: 1 };
/** LATE publishes no camera: its camera must read `not_published`, never a guess. */
const AUDIO_ONLY: CohortMemberPlan = {
  publish: 'audio',
  audioReceiveSlots: 1,
  videoReceiveSlots: 1,
};

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

/** Poll until `observer`'s roster row for `subject` reads the client-state token per kind. */
async function expectClientState(
  observer: PlanMember,
  subject: PlanMember,
  want: Record<MuteKind, ClientMediaStateToken>,
): Promise<void> {
  for (const kind of ['audio', 'video'] as const) {
    await waitForRosterToken(observer, subject, ROSTER_CLIENT_STATE_ATTRIBUTE[kind], want[kind]);
  }
}

test.describe('story 3 R-15: client state of a participant outside the observer slots', () => {
  test('the observer sees every client state from the roster, never from slots, never unknown', async ({
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
      await waitForRosterToken(decoy, subject, ROSTER_CLIENT_STATE_ATTRIBUTE.video, 'client_muted');

      // 3. OBSERVER joins; SUBJECT reaches it on `existing_participants`.
      const observer = await scenario.join(await scenario.open(2, ONE_EACH), ONE_EACH);
      await scenario.expectVideo(observer.label, 'decoy fills the observer slots');
      await expectOutsideObserverSlots(scenario, observer, [subject], 'observer joined');
      await expectClientState(observer, subject, { audio: 'live', video: 'client_muted' });

      // 4. Each SUBJECT toggle reaches the observer; no slot moves.
      const toggles: ReadonlyArray<[MuteKind, boolean, Record<MuteKind, ClientMediaStateToken>]> = [
        ['audio', true, { audio: 'client_muted', video: 'client_muted' }],
        ['audio', false, { audio: 'live', video: 'client_muted' }],
        ['video', false, { audio: 'live', video: 'live' }],
        ['video', true, { audio: 'live', video: 'client_muted' }],
      ];
      for (const [kind, muted, want] of toggles) {
        await setMuteViaUi(subject.page, kind, muted);
        scenario.muted(subject.label, kind, { client: muted });
        await expectClientState(observer, subject, want);
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

      // 5. LATE joins AUDIO-ONLY under the recorder: learned from ParticipantJoined alone.
      await installRosterStateRecorder(observer.page);
      const late = await scenario.join(await scenario.open(3, AUDIO_ONLY), AUDIO_ONLY);
      await scenario.expectVideo(observer.label, 'decoy still holds the observer video slot');
      await expectOutsideObserverSlots(scenario, observer, [subject, late], 'late joined');
      await expectClientState(observer, late, { audio: 'live', video: 'not_published' });
      // Said in words, not only as a token.
      await expect(
        observer.page.getByTestId(`mute-video-client-${late.joined.participantId}`),
        `${observer.label}: ${late.label}'s camera must read "not publishing"`,
      ).toHaveText(new RegExp(INDICATOR_TEXT.client.video.not_published));
      const beforeToggle = await recordedRosterState(observer.page, late.joined.participantId);
      // EXACT from the first render: the row was created with LATE's state and
      // never showed anything else (no "unknown", no wrong value).
      expect(beforeToggle, JSON.stringify(beforeToggle)).toEqual({
        audio: ['live'],
        video: ['not_published'],
      });
      await setMuteViaUi(late.page, 'audio', true);
      scenario.muted(late.label, 'audio', { client: true });
      await expectClientState(observer, late, { audio: 'client_muted', video: 'not_published' });
      await expectOutsideObserverSlots(scenario, observer, [subject, late], 'late muted mic');
      const after = await recordedRosterState(observer.page, late.joined.participantId);
      expect(after, JSON.stringify(after)).toEqual({
        audio: ['live', 'client_muted'],
        video: ['not_published'],
      });
    } finally {
      await scenario.close();
    }
  });
});
