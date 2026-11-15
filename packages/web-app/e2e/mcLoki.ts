// File: packages/web-app/e2e/mcLoki.ts
//
// The browser twin of the Rust meeting-end proof (`crates/env-tests/src/fixtures/
// loki.rs` + `meeting_end.rs`): prove, from MC's OWN per-meeting log lines in
// Loki, that a leg's meeting ended and was released on its media handlers —
// never from a fleet counter (operator ruling: env-tests never couple through
// shared counters). Used by `mhLeg.ts`'s AFTER step, BEFORE the owner teardown:
// a meeting still live when the run-owned MH goes would make MC acquire a
// replacement for it.
//
// THE PROOF, per meeting, in order (pure verdicts below, node-tested):
//   1. DEPARTURE (positive control): ONE departure line per participant id the
//      page received (bus `joined`), where MC's "Clean transport close" OR its
//      "Disconnect grace period expired" counts, each naming THIS meeting id.
//      Either counts because MC today classifies the SDK's leave as
//      `server_initiated` and holds it through the grace (docs/TODO.md, "MC
//      classifies the browser SDK's CLEAN disconnect as `server_initiated`",
//      owner meeting-controller). Accepting either fails CLOSED on an ingest
//      gap and survives that product fix (the wait then shrinks by itself);
//      requiring the grace line would assert the bug.
//   2. ANCHOR: max(scope start, newest "Meeting actor created" for the id) —
//      a join retry can end and recreate a meeting under the same id. No
//      created line is its own failure, never "accept any".
//   3. ENDED: exactly one "Meeting ended" at/after the anchor and the newest
//      departure (MC's own Loki clock), cause=empty, reason=last_participant_left.
//   4. RELEASED: exactly one "Meeting released on its media handlers" after it:
//      handlers >= 1, released == handlers, failed == 0, no quiesce timeout.
//
// The meeting id comes from an INDEPENDENT source (never from the lines being
// verified): the GC join responses the page itself received (`mhLeg.ts`).
// Every log string and shared token is pinned to its Rust const by
// `tests/mcLoki.test.ts`.
//
// NOTHING IDENTIFYING LEAVES: failures name labels, meeting ids and the LogQL;
// never a participant id or a token.

/** One MC log line: Loki `app`, JSON `target`, exact `fields.message` (`loki.rs` LogLine). */
export interface LogLine {
  readonly app: string;
  readonly target: string;
  readonly message: string;
}

/** `loki.rs` MC_CREATED. */
export const MC_CREATED: LogLine = {
  app: 'mc-service',
  target: 'mc.actor.controller',
  message: 'Meeting actor created',
};
/** `loki.rs` MC_MEETING_ENDED. */
export const MC_MEETING_ENDED: LogLine = {
  app: 'mc-service',
  target: 'mc.actor.controller',
  message: 'Meeting ended; releasing it on its media handlers',
};
/** `loki.rs` MC_MEETING_RELEASED. */
export const MC_MEETING_RELEASED: LogLine = {
  app: 'mc-service',
  target: 'mc.teardown',
  message: 'Meeting released on its media handlers',
};
/** `loki.rs` MC_CLEAN_CLOSE. */
export const MC_CLEAN_CLOSE: LogLine = {
  app: 'mc-service',
  target: 'mc.actor.meeting',
  message: 'Clean transport close — removing participant immediately (grace skipped)',
};
/** `loki.rs` MC_GRACE_EXPIRED. */
export const MC_GRACE_EXPIRED: LogLine = {
  app: 'mc-service',
  target: 'mc.actor.meeting',
  message: 'Disconnect grace period expired, removing participant',
};

/** `loki.rs` SCOPE_SKEW_SLACK: widens a scope start only; never orders. */
export const SCOPE_SKEW_SLACK_MS = 5_000;

/** `loki.rs` LOKI_QUERY_FAILED. */
export const LOKI_QUERY_FAILED = 'loki-query-failed';
/** `meeting_end.rs` MEETING_NOT_ENDED. */
export const MEETING_NOT_ENDED = 'meeting-not-ended';
/** `meeting_end.rs` MEETING_NOT_RELEASED. */
export const MEETING_NOT_RELEASED = 'meeting-not-released';
/** `loki.rs` PRECONDITION_LOKI_UNAVAILABLE. */
export const PRECONDITION_LOKI_UNAVAILABLE = 'PRECONDITION-loki-unavailable';
/** The browser proof's own tokens (no Rust twin). */
export const MEETING_ID_UNRESOLVED = 'meeting-id-unresolved';
export const PARTICIPANT_ID_UNRESOLVED = 'participant-id-unresolved';
export const CREATED_NEVER_SEEN = 'created-never-seen';
export const DEPARTURE_NEVER_SEEN = 'departure-never-seen';

/** One matched entry: Loki ts (ns, exact) and the parsed JSON line. */
export interface Entry {
  readonly tsNs: bigint;
  readonly json: Record<string, unknown>;
}

function fieldsOf(e: Entry): Record<string, unknown> {
  const f = e.json['fields'];
  return typeof f === 'object' && f !== null ? (f as Record<string, unknown>) : {};
}

/** A string field (`fields.<name>`). */
export function field(e: Entry, name: string): string | undefined {
  const v = fieldsOf(e)[name];
  return typeof v === 'string' ? v : undefined;
}

function numField(e: Entry, name: string): number | undefined {
  const v = fieldsOf(e)[name];
  return typeof v === 'number' ? v : undefined;
}

function boolField(e: Entry, name: string): boolean | undefined {
  const v = fieldsOf(e)[name];
  return typeof v === 'boolean' ? v : undefined;
}

/** The hyphenated lowercase UUID form MC logs; nothing else enters a LogQL query. */
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** A raw Loki stream value: [ts ns as a string, line]. */
export interface RawEntry {
  readonly tsNs: bigint;
  readonly line: string;
}

/**
 * PURE: classify one `query_range` response (twin of Rust `classify_query_range`):
 * a non-2xx, an unparseable body, or a 200 without `status: success` /
 * `data.result` THROWS {@link LOKI_QUERY_FAILED} — never an empty result.
 */
export function classifyQueryRange(status: number, body: string, query: string): RawEntry[] {
  const fail = (why: string): never => {
    throw new Error(`${LOKI_QUERY_FAILED}: \`${query}\`: ${why}`);
  };
  if (status < 200 || status >= 300) fail(`status ${status}, body ${body.slice(0, 300)}`);
  let json: unknown;
  try {
    json = JSON.parse(body);
  } catch (e) {
    return fail(`unparseable body: ${String(e)}`);
  }
  const root = json as { status?: unknown; data?: { result?: unknown } };
  if (root.status !== 'success' || !Array.isArray(root.data?.result)) {
    fail(`unexpected response shape: ${body.slice(0, 300)}`);
  }
  const out: RawEntry[] = [];
  for (const stream of root.data!.result as Array<{ values?: unknown }>) {
    if (!Array.isArray(stream.values)) fail(`unexpected response shape: ${body.slice(0, 300)}`);
    for (const v of stream.values as unknown[]) {
      const pair = v as unknown[];
      if (!Array.isArray(pair) || typeof pair[0] !== 'string' || typeof pair[1] !== 'string') {
        fail(`unexpected value shape: ${body.slice(0, 300)}`);
      }
      let ts: bigint;
      try {
        ts = BigInt(pair[0] as string);
      } catch {
        return fail(`unparseable timestamp: ${String(pair[0])}`);
      }
      out.push({ tsNs: ts, line: pair[1] as string });
    }
  }
  return out.sort((a, b) => (a.tsNs < b.tsNs ? -1 : a.tsNs > b.tsNs ? 1 : 0));
}

/** PURE: the LogQL line filter (twin of Rust `logql_filter`): the plain-ASCII prefix. */
export function logqlFilter(message: string): string {
  let end = 0;
  for (const ch of message) {
    const c = ch.codePointAt(0)!;
    if (c > 0x7e || c < 0x20 || ch === '"' || ch === '\\') break;
    end += ch.length;
  }
  return message.slice(0, end).trimEnd();
}

/** PURE: the LogQL for `line` scoped to `meetingId` (twin of Rust `meeting_logql`). */
export function meetingLogql(line: LogLine, meetingId: string): string {
  if (!UUID_RE.test(meetingId)) {
    throw new Error(`${MEETING_ID_UNRESOLVED}: "${meetingId}" is not a UUID`);
  }
  return `{app="${line.app}"} |= "${meetingId}" |= "${logqlFilter(line.message)}"`;
}

/** PURE: the entries of `raw` that ARE `line` for `meetingId`, structurally. */
export function matchingEntries(line: LogLine, meetingId: string, raw: RawEntry[]): Entry[] {
  const out: Entry[] = [];
  for (const r of raw) {
    let json: unknown;
    try {
      json = JSON.parse(r.line);
    } catch {
      continue; // not one of the (JSON) lines
    }
    if (typeof json !== 'object' || json === null) continue;
    const e: Entry = { tsNs: r.tsNs, json: json as Record<string, unknown> };
    if (
      e.json['target'] === line.target &&
      field(e, 'message') === line.message &&
      field(e, 'meeting_id') === meetingId
    ) {
      out.push(e);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// PURE verdicts (node-tested, one per failure token)
// ---------------------------------------------------------------------------

/** What one member contributes: its label, its GC-join meeting ids, its MC participant ids. */
export interface MemberDeparture {
  readonly label: string;
  /** `meetingId` of EVERY 2xx GC join response the page received (retries included). */
  readonly meetingIds: readonly string[];
  /** Every distinct participant id the page's bus `joined` events carried. */
  readonly participantIds: readonly string[];
}

/**
 * PURE: the ONE meeting id a member's GC join responses name. None, more
 * than one, or a non-UUID is {@link MEETING_ID_UNRESOLVED}.
 */
export function resolveMeetingId(m: MemberDeparture): string {
  const ids = [...new Set(m.meetingIds)];
  if (ids.length !== 1 || !UUID_RE.test(ids[0]!)) {
    throw new Error(
      `${MEETING_ID_UNRESOLVED}: member ${m.label}'s GC join response(s) name ` +
        `${ids.length === 0 ? 'no meeting id' : `${ids.length} value(s) [${ids.join(', ')}]`}; ` +
        'exactly one UUID expected',
    );
  }
  return ids[0]!;
}

/**
 * PURE: what one member page contributes, from its recorded GC joins and bus
 * participant ids. No GC join at all: nothing (`undefined`). A GC join but NO
 * MC participant id: its MC join failed while GC's join already created MC's
 * meeting actor, so the leg is broken and that meeting would go unproven —
 * {@link PARTICIPANT_ID_UNRESOLVED}, never a skip.
 */
export function classifyMember(
  label: string,
  meetingIds: readonly string[],
  participantIds: readonly string[],
): MemberDeparture | undefined {
  if (meetingIds.length === 0) return undefined;
  if (participantIds.length === 0) {
    throw new Error(
      `${PARTICIPANT_ID_UNRESOLVED}: member ${label} received a GC join for meeting(s) ` +
        `[${[...new Set(meetingIds)].join(', ')}] but its bus carries no MC \`joined\` participant ` +
        'id: its MC join failed (or the e2e bus is absent/reshaped), so its meeting cannot be proved',
    );
  }
  return { label, meetingIds: [...meetingIds], participantIds: [...new Set(participantIds)] };
}

/** PURE: the labels of members with a participant id no departure line names. */
export function missingDepartures(
  members: readonly MemberDeparture[],
  departures: readonly Entry[],
): string[] {
  const named = new Set(departures.map((e) => field(e, 'participant_id')));
  return members.filter((m) => m.participantIds.some((p) => !named.has(p))).map((m) => m.label);
}

/** PURE: the anchor = max(sinceNs, newest created line); none is {@link CREATED_NEVER_SEEN}. */
export function createdAnchor(sinceNs: bigint, created: readonly Entry[]): bigint {
  const after = created.filter((e) => e.tsNs >= sinceNs);
  if (after.length === 0) {
    throw new Error(
      `${CREATED_NEVER_SEEN}: no "${MC_CREATED.message}" line for the meeting since the leg ` +
        'started — nothing to anchor this incarnation on (never "accept any")',
    );
  }
  return after.reduce((m, e) => (e.tsNs > m ? e.tsNs : m), sinceNs);
}

/** PURE: the newest departure of `members`' participants at/after `anchor` (else `anchor`). */
export function newestDeparture(
  members: readonly MemberDeparture[],
  departures: readonly Entry[],
  anchor: bigint,
): bigint {
  const ours = new Set(members.flatMap((m) => m.participantIds));
  return departures
    .filter((e) => ours.has(field(e, 'participant_id') ?? '') && e.tsNs >= anchor)
    .reduce((max, e) => (e.tsNs > max ? e.tsNs : max), anchor);
}

/**
 * PURE: the end of THIS incarnation. `undefined` while no end line is at/after
 * `anchor`; exactly one, at/after `departedAt`, with cause=empty /
 * reason=last_participant_left is returned; anything else throws
 * {@link MEETING_NOT_ENDED} — including an end BEFORE the last member departed
 * (MC ended the meeting with our participant still in it).
 */
export function pickEnd(
  ends: readonly Entry[],
  anchor: bigint,
  departedAt: bigint,
): Entry | undefined {
  const after = ends.filter((e) => e.tsNs >= anchor);
  if (after.length === 0) return undefined;
  if (after.length > 1) {
    throw new Error(
      `${MEETING_NOT_ENDED}: ${after.length} end lines for this incarnation (exactly one expected)`,
    );
  }
  const one = after[0]!;
  if (one.tsNs < departedAt) {
    throw new Error(
      `${MEETING_NOT_ENDED}: the meeting ended before member(s) departed (end at ${one.tsNs}, ` +
        `last departure at ${departedAt}, MC's Loki clock)`,
    );
  }
  if (field(one, 'cause') !== 'empty' || field(one, 'reason') !== 'last_participant_left') {
    throw new Error(
      `${MEETING_NOT_ENDED}: the meeting ended, but not because its last participant left: ` +
        `cause ${String(field(one, 'cause'))}, reason ${String(field(one, 'reason'))}`,
    );
  }
  return one;
}

/**
 * PURE: the completed release after `endTs`. `undefined` while absent; exactly
 * one with handlers >= 1, released == handlers, failed == 0 and no quiesce
 * timeout is returned; anything else throws {@link MEETING_NOT_RELEASED}.
 */
export function releasedVerdict(rels: readonly Entry[], endTs: bigint): Entry | undefined {
  const after = rels.filter((e) => e.tsNs >= endTs);
  if (after.length === 0) return undefined;
  if (after.length > 1) {
    throw new Error(`${MEETING_NOT_RELEASED}: ${after.length} release lines after the end`);
  }
  const one = after[0]!;
  const [handlers, released, failed, timedOut] = [
    numField(one, 'handlers'),
    numField(one, 'released'),
    numField(one, 'failed'),
    boolField(one, 'quiesce_timed_out'),
  ];
  if (
    handlers === undefined ||
    handlers < 1 ||
    released !== handlers ||
    failed !== 0 ||
    timedOut !== false
  ) {
    throw new Error(
      `${MEETING_NOT_RELEASED}: release incomplete: handlers ${String(handlers)}, released ` +
        `${String(released)}, failed ${String(failed)}, quiesce_timed_out ${String(timedOut)} ` +
        '(expected handlers >= 1, released == handlers, failed 0, no quiesce timeout)',
    );
  }
  return one;
}

// ---------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------

/** Host wall clock as Loki ns (scope bounds only). */
export function nowNs(): bigint {
  return BigInt(Date.now()) * 1_000_000n;
}

/** A leg's scope start: now minus {@link SCOPE_SKEW_SLACK_MS}. */
export function scopeStartNs(): bigint {
  return nowNs() - BigInt(SCOPE_SKEW_SLACK_MS) * 1_000_000n;
}

/**
 * The Loki base URL. `env.ts` is imported lazily so the pure verdicts above stay
 * node-testable without a Layer-7 environment.
 */
async function lokiUrl(): Promise<string> {
  const { e2eEnv } = await import('./env.js');
  const url = e2eEnv.lokiUrl;
  if (url === undefined) {
    throw new Error(
      `${PRECONDITION_LOKI_UNAVAILABLE}: E2E_LOKI_URL is unset. The MH-replacement legs prove each ` +
        'meeting ended from per-meeting MC log lines; Layer 7 exports it from ENV_TEST_LOKI_URL ' +
        '(scripts/layer7.sh). An unset Loki FAILS the leg, never skips the proof.',
    );
  }
  return url;
}

/** Every `line` entry for `meetingId` since `sinceNs` (throws {@link LOKI_QUERY_FAILED}). */
async function entries(line: LogLine, meetingId: string, sinceNs: bigint): Promise<Entry[]> {
  const query = meetingLogql(line, meetingId);
  const params = new URLSearchParams({
    query,
    start: sinceNs.toString(),
    end: nowNs().toString(),
    limit: '1000',
    direction: 'forward',
  });
  const base = await lokiUrl(); // PRECONDITION failure stays its own error
  let response: Response;
  try {
    response = await fetch(`${base}/loki/api/v1/query_range?${params.toString()}`, {
      signal: AbortSignal.timeout(10_000),
    });
  } catch (e) {
    throw new Error(`${LOKI_QUERY_FAILED}: \`${query}\`: transport error: ${String(e)}`, {
      cause: e,
    });
  }
  const raw = classifyQueryRange(response.status, await response.text(), query);
  return matchingEntries(line, meetingId, raw);
}

/** Poll `read` until `decide` returns a value, or throw `timeout()` at the `deadline` (epoch ms). */
async function poll<T>(
  read: () => Promise<Entry[]>,
  decide: (es: Entry[]) => T | undefined,
  deadline: number,
  timeout: () => string,
): Promise<T> {
  for (;;) {
    const got = decide(await read());
    if (got !== undefined) return got;
    if (Date.now() >= deadline) throw new Error(timeout());
    await new Promise((r) => setTimeout(r, 1_000));
  }
}

/**
 * Prove, for every meeting among `members` (grouped by their resolved GC
 * meeting id), the four steps in the header.
 *
 * BUDGET (the same shape as Rust `meeting_end`): ONE deadline, `ceilingMs`
 * (MC's published disconnect grace + the log-ingest ceiling) from the call,
 * shared by the departure, created and end steps of EVERY meeting (they ended
 * concurrently on MC); then ONE release deadline, `ceilingMs` after that,
 * shared by every meeting's release (each still gets at least `ceilingMs` from
 * its own end line). The whole call is therefore bounded by 2 x `ceilingMs` —
 * {@link meetingEndBudgetMs}, which `mhLeg.ts:legBounds` reserves, so the
 * specific token always surfaces before Playwright's timeout.
 */
export async function proveMeetingsEnded(
  members: readonly MemberDeparture[],
  sinceNs: bigint,
  ceilingMs: number,
): Promise<void> {
  const deadline = Date.now() + ceilingMs;
  // ONE release deadline for every meeting: every end line is seen by
  // `deadline`, so each release still gets at least `ceilingMs` from its own
  // end (the Rust twin's "from the end line" window), and the whole call is
  // bounded by `deadline + ceilingMs` = meetingEndBudgetMs, however many
  // meetings (placement misses + the hit) it proves.
  const releaseDeadline = deadline + ceilingMs;
  const byMeeting = new Map<string, MemberDeparture[]>();
  for (const m of members) {
    const id = resolveMeetingId(m);
    byMeeting.set(id, [...(byMeeting.get(id) ?? []), m]);
  }
  for (const [meetingId, ms] of byMeeting) {
    // 1. Departure positive control: clean close OR grace expiry, per participant.
    const departures = await poll(
      async () => [
        ...(await entries(MC_CLEAN_CLOSE, meetingId, sinceNs)),
        ...(await entries(MC_GRACE_EXPIRED, meetingId, sinceNs)),
      ],
      (ds) => (missingDepartures(ms, ds).length === 0 ? ds : undefined),
      deadline,
      () =>
        `${DEPARTURE_NEVER_SEEN}: meeting ${meetingId}: no "${MC_CLEAN_CLOSE.message}" nor ` +
        `"${MC_GRACE_EXPIRED.message}" line for member(s) ${ms.map((m) => m.label).join(', ')} ` +
        `within ${ceilingMs} ms (LogQL \`${meetingLogql(MC_CLEAN_CLOSE, meetingId)}\`). MC never ` +
        'removed them, or Loki is not ingesting mc-service.',
    );
    // 2. Anchor this incarnation.
    const created = await poll(
      () => entries(MC_CREATED, meetingId, sinceNs),
      (cs) => (cs.length > 0 ? cs : undefined),
      deadline,
      () =>
        `${CREATED_NEVER_SEEN}: meeting ${meetingId}: no "${MC_CREATED.message}" line since the ` +
        `leg started within ${ceilingMs} ms; LogQL \`${meetingLogql(MC_CREATED, meetingId)}\``,
    );
    const anchor = createdAnchor(sinceNs, created);
    const departedAt = newestDeparture(ms, departures, anchor);
    // 3. Exactly one end of this incarnation, not before the last departure.
    const end = await poll(
      () => entries(MC_MEETING_ENDED, meetingId, sinceNs),
      (es) => pickEnd(es, anchor, departedAt),
      deadline,
      () =>
        `${MEETING_NOT_ENDED}: MC did not end an emptied meeting: product defect, not env (see ` +
        `195b7059), or a participant of this meeting was not left by the leg. Meeting ` +
        `${meetingId}: every member departed, yet no "${MC_MEETING_ENDED.message}" line within ` +
        `${ceilingMs} ms; LogQL \`${meetingLogql(MC_MEETING_ENDED, meetingId)}\``,
    );
    // 4. Exactly one completed release after the end, in its own window.
    await poll(
      () => entries(MC_MEETING_RELEASED, meetingId, sinceNs),
      (rs) => releasedVerdict(rs, end.tsNs),
      releaseDeadline,
      () =>
        `${MEETING_NOT_RELEASED}: meeting ${meetingId} ended but MC logged no ` +
        `"${MC_MEETING_RELEASED.message}" after it within ${ceilingMs} ms; LogQL ` +
        `\`${meetingLogql(MC_MEETING_RELEASED, meetingId)}\``,
    );
  }
}

/** The Playwright budget {@link proveMeetingsEnded} can spend at most: 2 x its ceiling. */
export function meetingEndBudgetMs(ceilingMs: number): number {
  return 2 * ceilingMs;
}
