// Node-tier tests for `e2e/mcLoki.ts`, the browser twin of the Rust meeting-end
// proof: the pure verdicts (one case per failure token) and the drift pins of
// every log line and shared token against their ONE Rust home
// (`crates/env-tests/src/fixtures/loki.rs`, `meeting_end.rs`).

import { describe, expect, test } from 'vitest';

import {
  CREATED_NEVER_SEEN,
  LOKI_QUERY_FAILED,
  MC_CLEAN_CLOSE,
  MC_CREATED,
  MC_GRACE_EXPIRED,
  MC_LEAVE_REQUEST,
  MC_MEETING_ENDED,
  MC_MEETING_RELEASED,
  MEETING_ID_UNRESOLVED,
  MEETING_NOT_ENDED,
  MEETING_NOT_RELEASED,
  PARTICIPANT_ID_UNRESOLVED,
  PRECONDITION_LOKI_UNAVAILABLE,
  SCOPE_SKEW_SLACK_MS,
  classifyMember,
  classifyQueryRange,
  createdAnchor,
  departureEntries,
  logqlFilter,
  matchingEntries,
  meetingEndBudgetMs,
  meetingLogql,
  missingDepartures,
  newestDeparture,
  pickEnd,
  releasedVerdict,
  resolveMeetingId,
  type Entry,
  type MemberDeparture,
} from '../e2e/mcLoki.js';
import { RUST_CONST_NOT_FOUND, logLineConst, readRepo, strConst } from './_rustSource.js';

const ID = '4b1e9f2a-0c7d-4e8b-9a35-6f0d2c1b7e44';

const entry = (ts: number, fields: Record<string, unknown>): Entry => ({
  tsNs: BigInt(ts),
  json: { fields },
});
const member = (label: string, pids: string[], ids: string[] = [ID]): MemberDeparture => ({
  label,
  meetingIds: ids,
  participantIds: pids,
});

describe('drift pins (one Rust home each)', () => {
  const loki = readRepo('crates/env-tests/src/fixtures/loki.rs');
  const meetingEnd = readRepo('crates/env-tests/src/fixtures/meeting_end.rs');

  test.each([
    ['MC_CREATED', MC_CREATED],
    ['MC_MEETING_ENDED', MC_MEETING_ENDED],
    ['MC_MEETING_RELEASED', MC_MEETING_RELEASED],
    ['MC_CLEAN_CLOSE', MC_CLEAN_CLOSE],
    ['MC_GRACE_EXPIRED', MC_GRACE_EXPIRED],
    ['MC_LEAVE_REQUEST', MC_LEAVE_REQUEST],
  ])('loki.rs %s', (name, line) => {
    expect(logLineConst(loki, name)).toEqual(line);
  });

  test('shared tokens and the scope slack', () => {
    expect(strConst(loki, 'LOKI_QUERY_FAILED')).toBe(LOKI_QUERY_FAILED);
    expect(strConst(loki, 'PRECONDITION_LOKI_UNAVAILABLE')).toBe(PRECONDITION_LOKI_UNAVAILABLE);
    expect(strConst(meetingEnd, 'MEETING_NOT_ENDED')).toBe(MEETING_NOT_ENDED);
    expect(strConst(meetingEnd, 'MEETING_NOT_RELEASED')).toBe(MEETING_NOT_RELEASED);
    const slack = /pub const SCOPE_SKEW_SLACK: Duration = Duration::from_secs\((\d+)\);/.exec(
      loki,
    )?.[1];
    expect(slack, 'positive control').toBeTruthy();
    expect(SCOPE_SKEW_SLACK_MS).toBe(Number(slack) * 1000);
  });

  test('the extractor fails with its own token on a missing or reshaped const', () => {
    const src = 'pub const OTHER: &str = "x";\npub const BAD: &str = "a\\"b";\n';
    expect(() => strConst(src, 'MEETING_NOT_ENDED')).toThrow(RUST_CONST_NOT_FOUND);
    expect(() => strConst(src, 'BAD')).toThrow(RUST_CONST_NOT_FOUND);
    expect(() => logLineConst(src, 'MC_CREATED')).toThrow(RUST_CONST_NOT_FOUND);
    expect(() =>
      logLineConst(
        'pub const L: LogLine = LogLine { app: "a", target: concat!("b"), message: "c", };',
        'L',
      ),
    ).toThrow(RUST_CONST_NOT_FOUND);
  });
});

describe('query + matcher', () => {
  test('a Loki error is its own token, never "no lines"', () => {
    expect(() => classifyQueryRange(400, 'parse error', 'q')).toThrow(LOKI_QUERY_FAILED);
    expect(() => classifyQueryRange(200, '<html>', 'q')).toThrow(LOKI_QUERY_FAILED);
    expect(() => classifyQueryRange(200, '{"status":"success","data":{}}', 'q')).toThrow(
      LOKI_QUERY_FAILED,
    );
    expect(classifyQueryRange(200, '{"status":"success","data":{"result":[]}}', 'q')).toEqual([]);
  });

  test('entries come back oldest first with exact ns', () => {
    const body = JSON.stringify({
      status: 'success',
      data: {
        result: [
          {
            values: [
              ['1731600000000000002', 'b'],
              ['1731600000000000001', 'a'],
            ],
          },
        ],
      },
    });
    expect(classifyQueryRange(200, body, 'q').map((e) => e.tsNs)).toEqual([
      1731600000000000001n,
      1731600000000000002n,
    ]);
  });

  test('the matcher takes only this line for this meeting', () => {
    const raw = (ts: number, target: string, message: string, meeting: string) => ({
      tsNs: BigInt(ts),
      line: JSON.stringify({ target, fields: { message, meeting_id: meeting } }),
    });
    const l = MC_MEETING_ENDED;
    const got = matchingEntries(l, ID, [
      raw(1, l.target, l.message, ID),
      raw(2, l.target, l.message, '00000000-0000-0000-0000-000000000000'),
      raw(3, 'mc.actor.meeting', l.message, ID),
      raw(4, l.target, MC_CREATED.message, ID),
      { tsNs: 5n, line: `${ID} ${l.message}` },
    ]);
    expect(got.map((e) => e.tsNs)).toEqual([1n]);
  });

  test('the LogQL is the Rust shape and refuses a non-UUID id', () => {
    expect(logqlFilter(MC_CLEAN_CLOSE.message)).toBe('Clean transport close');
    expect(meetingLogql(MC_CLEAN_CLOSE, ID)).toBe(
      `{app="mc-service"} |= "${ID}" |= "Clean transport close"`,
    );
    expect(() => meetingLogql(MC_CREATED, 'x" |= "y')).toThrow(MEETING_ID_UNRESOLVED);
  });
});

describe('verdicts', () => {
  test('meeting-id-unresolved: none, two across retries, or not a UUID', () => {
    expect(resolveMeetingId(member('A', ['p'], [ID, ID]))).toBe(ID);
    expect(() => resolveMeetingId(member('A', ['p'], []))).toThrow(MEETING_ID_UNRESOLVED);
    expect(() =>
      resolveMeetingId(member('A', ['p'], [ID, '00000000-0000-0000-0000-000000000000'])),
    ).toThrow(MEETING_ID_UNRESOLVED);
    expect(() => resolveMeetingId(member('A', ['p'], ['<no meetingId>']))).toThrow(
      MEETING_ID_UNRESOLVED,
    );
  });

  test('departure-never-seen: a participant id no departure names', () => {
    const ms = [member('A', ['pa']), member('B', ['pb'])];
    expect(missingDepartures(ms, [entry(1, { participant_id: 'pa' })])).toEqual(['B']);
    expect(
      missingDepartures(ms, [
        entry(1, { participant_id: 'pa' }),
        entry(2, { participant_id: 'pb' }),
      ]),
    ).toEqual([]);
    // Another participant's departure does not count for B.
    expect(
      missingDepartures(ms, [
        entry(1, { participant_id: 'pa' }),
        entry(2, { participant_id: 'px' }),
      ]),
    ).toEqual(['B']);
  });

  /** A raw Loki value for `line` naming meeting `ID` and participant `pid`. */
  const rawLine = (ts: number, line: typeof MC_LEAVE_REQUEST, pid: string) => ({
    tsNs: BigInt(ts),
    line: JSON.stringify({
      target: line.target,
      fields: { message: line.message, meeting_id: ID, participant_id: pid },
    }),
  });

  test('ONLY the LeaveRequest line is a departure', () => {
    const ms = [member('A', ['pa'])];
    const ds = departureEntries(ID, [rawLine(1, MC_LEAVE_REQUEST, 'pa')]);
    expect(missingDepartures(ms, ds)).toEqual([]);
  });

  test('ADVERSE: a departure proven only by a grace-expiry (or clean-close) line FAILS the proof', () => {
    // The leave never reached MC: it held the participant through its grace.
    // Counting this would let a P4 regression pass, one grace slower.
    const ms = [member('A', ['pa'])];
    for (const line of [MC_GRACE_EXPIRED, MC_CLEAN_CLOSE]) {
      const ds = departureEntries(ID, [rawLine(1, line, 'pa')]);
      expect(ds).toEqual([]);
      expect(missingDepartures(ms, ds)).toEqual(['A']);
    }
  });

  test('created-never-seen; the anchor is the newest created line, never below the scope', () => {
    expect(() => createdAnchor(10n, [])).toThrow(CREATED_NEVER_SEEN);
    expect(() => createdAnchor(10n, [entry(5, {})])).toThrow(CREATED_NEVER_SEEN);
    expect(createdAnchor(10n, [entry(12, {}), entry(30, {})])).toBe(30n);
  });

  const ended = (ts: number, cause = 'empty', reason = 'last_participant_left') =>
    entry(ts, { cause, reason });

  test('before the anchor is not this incarnation; exactly one end at/after it', () => {
    // anchor 30, last departure 33
    expect(pickEnd([ended(20)], 30n, 33n)).toBeUndefined();
    expect(pickEnd([ended(20), ended(35)], 30n, 33n)?.tsNs).toBe(35n);
    expect(() => pickEnd([ended(34), ended(36)], 30n, 33n)).toThrow(MEETING_NOT_ENDED);
  });

  test('an end before the last departure fails (MC ended it with a member still in)', () => {
    expect(() => pickEnd([ended(31)], 30n, 33n)).toThrow(/ended before member\(s\) departed/);
  });

  test('the newest departure counts only our participants, at/after the anchor', () => {
    const ms = [member('A', ['pa']), member('B', ['pb'])];
    const ds = [
      entry(31, { participant_id: 'pa' }),
      entry(34, { participant_id: 'pb' }),
      entry(50, { participant_id: 'px' }), // someone else
      entry(10, { participant_id: 'pb' }), // before the anchor
    ];
    expect(newestDeparture(ms, ds, 30n)).toBe(34n);
    expect(newestDeparture(ms, [], 30n)).toBe(30n);
  });

  test('end-recreate-end: only the end of the anchored incarnation counts', () => {
    // Incarnation 1 created at 10, ended by its join window at 20 (a join retry);
    // incarnation 2 created at 25, its members departed at 40, ended at 45.
    const created = [entry(10, {}), entry(25, {})];
    const anchor = createdAnchor(5n, created);
    expect(anchor).toBe(25n);
    const ends = [ended(20, 'empty', 'join_window_expired'), ended(45)];
    expect(pickEnd(ends, anchor, 40n)?.tsNs).toBe(45n);
    // Anchored on the scope start instead, both ends would count: an error.
    expect(() => pickEnd(ends, 5n, 40n)).toThrow(MEETING_NOT_ENDED);
  });

  test('wrong cause/reason is meeting-not-ended', () => {
    expect(() => pickEnd([ended(31, 'empty', 'join_window_expired')], 30n, 30n)).toThrow(
      MEETING_NOT_ENDED,
    );
    expect(() => pickEnd([ended(31, 'closed', 'closed')], 30n, 30n)).toThrow(MEETING_NOT_ENDED);
  });

  test('participant-id-unresolved: a GC join without an MC participant id is loud', () => {
    expect(classifyMember('A', [], [])).toBeUndefined();
    expect(() => classifyMember('A', [ID], [])).toThrow(PARTICIPANT_ID_UNRESOLVED);
    expect(classifyMember('A', [ID, ID], ['pa', 'pa'])).toEqual({
      label: 'A',
      meetingIds: [ID, ID],
      participantIds: ['pa'],
    });
  });

  test('the Playwright budget covers the shared deadline plus one release window', () => {
    expect(meetingEndBudgetMs(75_000)).toBe(150_000);
  });

  const rel = (ts: number, handlers: number, released: number, failed: number, t = false) =>
    entry(ts, { handlers, released, failed, quiesce_timed_out: t });

  test('released: missing, complete, and every incomplete shape', () => {
    expect(releasedVerdict([], 10n)).toBeUndefined();
    expect(releasedVerdict([rel(5, 2, 2, 0)], 10n)).toBeUndefined();
    expect(releasedVerdict([rel(11, 2, 2, 0)], 10n)?.tsNs).toBe(11n);
    for (const bad of [
      rel(11, 2, 1, 1), // failed > 0
      rel(11, 2, 2, 0, true), // quiesce timed out
      rel(11, 0, 0, 0), // empty plan: vacuous
      rel(11, 2, 1, 0), // released < handlers
      entry(11, {}), // fields missing
    ]) {
      expect(() => releasedVerdict([bad], 10n)).toThrow(MEETING_NOT_RELEASED);
    }
    expect(() => releasedVerdict([rel(11, 1, 1, 0), rel(12, 1, 1, 0)], 10n)).toThrow(
      MEETING_NOT_RELEASED,
    );
  });
});
