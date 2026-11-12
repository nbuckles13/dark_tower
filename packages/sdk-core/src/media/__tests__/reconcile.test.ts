// File: packages/sdk-core/src/media/__tests__/reconcile.test.ts
//
// THE CLIENT GATES of ADR-0039 §4, §5, §7 (test-gates.md §"Client"), on
// `MediaTransport`'s handler-set reconcile, against `MockWebTransport` (a NEW
// mock per dial — never deduped by URL) and a hand-driven `FakeScheduler`
// (no sleeps, no wall time: a test reads a pending timer's delay and fires
// exactly the one it means to). Every negative has its positive arm in the
// same test. Tokens are OPAQUE non-JWT strings, so an accidental parse fails.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { FakeScheduler, InMemoryMetricsSink, type MockWebTransport } from '@darktower/test-utils';

import { MediaTransport } from '../MediaTransport.js';
import { handlerRef, type HandlerRef } from '../handlerSet.js';
import type { MhConnectionStatusReport } from '../../signaling/events.js';
import { MhCloseCode } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import {
  DEFAULT_CLIENT_CONFIG,
  MH_KEEPALIVE_FLOOR_MS,
  type HandlerSetConfig,
} from '../../config/clientConfig.js';
import { connectRequestsOn, flushMicrotasks, intakeOf, makeDialRecorder } from './helpers.js';

const A = 'https://mh-a.example:4433';
const B = 'https://mh-b.example:4433';
const C = 'https://mh-c.example:4433';
const R1 = handlerRef(1);
const R2 = handlerRef(2);
const R3 = handlerRef(3);

const HANDLERS: HandlerSetConfig = {
  ...DEFAULT_CLIENT_CONFIG.media.handlers,
  redialBaseMs: 1_000,
  redialCapMs: 30_000,
  emptySetNoticeMs: 15_000,
  releasedGraceMs: 2_000,
};

afterEach(() => {
  vi.restoreAllMocks();
});

interface Rig {
  readonly mt: MediaTransport;
  readonly sched: FakeScheduler;
  readonly dials: ReturnType<typeof makeDialRecorder>;
  readonly sink: InMemoryMetricsSink;
  readonly reports: MhConnectionStatusReport[];
  readonly removed: HandlerRef[];
  readonly overdue: number[];
  /** Make the newest dial to `url` succeed and let the handshake finish. */
  ready(url: string): Promise<MockWebTransport>;
  /** Make the newest dial to `url` fail. */
  refuse(url: string): Promise<void>;
  /** Fire the earliest pending timer (deterministic; returns its delay). */
  fireNext(): Promise<number>;
  /** Pending timers whose delay is not the connect deadline. */
  redialDelays(): number[];
}

const CONNECT_TIMEOUT_MS = 7_777;

function makeRig(random: () => number = () => 0): Rig {
  const sched = new FakeScheduler();
  const dials = makeDialRecorder();
  const sink = new InMemoryMetricsSink();
  const mt = new MediaTransport({
    connect: dials.connect,
    scheduler: sched,
    random,
    clock: sched.clock,
    metricsSink: sink,
    handlers: HANDLERS,
    connectTimeoutMs: CONNECT_TIMEOUT_MS,
  });
  const reports: MhConnectionStatusReport[] = [];
  const removed: HandlerRef[] = [];
  const overdue: number[] = [];
  mt.on('status', (r) => reports.push(r));
  mt.on('handlerRemoved', (ref) => removed.push(ref));
  mt.on('handlersOverdue', () => overdue.push(sched.clock()));
  return {
    mt,
    sched,
    dials,
    sink,
    reports,
    removed,
    overdue,
    async ready(url) {
      const mock = dials.latest(url);
      mock.simulateReady();
      await flushMicrotasks(50);
      return mock;
    },
    async refuse(url) {
      dials.latest(url).simulateError(new Error('refused'));
      await flushMicrotasks(50);
    },
    async fireNext() {
      const t = sched.fireNext();
      await flushMicrotasks(50);
      return t?.delayMs ?? -1;
    },
    redialDelays: () =>
      sched
        .pending()
        .map((t) => t.delayMs)
        .filter((d) => d !== CONNECT_TIMEOUT_MS),
  };
}

/** Join with `handlers`, connect each, return the rig. */
async function joined(
  handlers: { ref: number; url: string; token?: string }[],
  random?: () => number,
): Promise<Rig> {
  const rig = makeRig(random);
  rig.mt.applyHandlerSet(intakeOf(1n, handlers));
  const armed = rig.mt.arm();
  for (const h of handlers) await rig.ready(h.url);
  await armed;
  return rig;
}

describe('G1 / T1 — join and add share ONE dial path; the dial token is the only credential', () => {
  it('an add dials exactly like the join (envelope shape, metric, state, report)', async () => {
    const rig = await joined([{ ref: 1, url: A, token: 'tok-A1' }]);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A, token: 'tok-A1' },
        { ref: 2, url: B, token: 'tok-B1' },
      ]),
    );
    expect(rig.mt.getState(R2)).toBe('connecting');
    await rig.ready(B);
    expect(rig.mt.getState(R2)).toBe('connected');

    // Same envelope shape: both carry only media_dial_token, from the held token.
    expect(connectRequestsOn(rig.dials.latest(A))).toEqual([{ mediaDialToken: 'tok-A1' }]);
    expect(connectRequestsOn(rig.dials.latest(B))).toEqual([{ mediaDialToken: 'tok-B1' }]);
    // Same metric: one success per dial, bucket = position in the triggering set.
    rig.sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '0' },
      1,
    );
    rig.sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '1' },
      1,
    );
    // Same report: CONNECTED, with ref + the version held at the transition.
    expect(rig.reports.map((r) => [r.handlerRef, r.handlerSetVersion, r.state])).toEqual([
      [R1, 1n, 'connected'],
      [R2, 2n, 'connected'],
    ]);
    rig.mt.disconnect();
  });

  it("every dial — armed, add, redial, post-park — carries exactly its ref's latest dial token", async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    // An add.
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b1' },
      ]),
    );
    await rig.ready(B);
    // A loss redial of the JOIN-TIME ref.
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    await rig.fireNext();
    await rig.ready(A);
    // A post-park dial.
    rig.dials.latest(B).simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b2' },
      ]),
    );
    await rig.ready(B);

    const all = rig.dials.dials.flatMap((d) => connectRequestsOn(d.mock));
    expect(all).toEqual([
      { mediaDialToken: 'tok-1' },
      { mediaDialToken: 'b1' },
      { mediaDialToken: 'tok-1' },
      { mediaDialToken: 'b2' },
    ]);
    rig.mt.disconnect();
  });
});

describe('G2 — removal is a directed removal: closed, quiet, unreported', () => {
  it('closes a transport the server left OPEN, notifies consumers BEFORE the close, reports nothing', async () => {
    const rig = await joined([
      { ref: 1, url: A },
      { ref: 2, url: B },
    ]);
    const bMock = rig.dials.latest(B);
    const closeB = vi.spyOn(bMock, 'close');
    const closedBeforeNotice: boolean[] = [];
    rig.mt.on('handlerRemoved', () => closedBeforeNotice.push(closeB.mock.calls.length > 0));
    const reportsBefore = rig.reports.length;

    rig.mt.applyHandlerSet(intakeOf(2n, [{ ref: 1, url: A }]));

    expect(rig.removed).toEqual([R2]);
    expect(closedBeforeNotice).toEqual([false]);
    expect(closeB).toHaveBeenCalledTimes(1);
    expect(rig.mt.getState(R2)).toBeUndefined();
    expect(rig.mt.getDatagramChannel(R2)).toBeUndefined();
    // A server close right after the set that removed it: still nothing.
    bMock.simulateClose(MhCloseCode.HANDLER_RELEASED);
    await flushMicrotasks(50);
    expect(rig.reports).toHaveLength(reportsBefore);
    expect(rig.redialDelays()).toEqual([]);

    // Positive arm: the SAME kind of end on an IN-SET ref is a loss report + redial.
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    expect(rig.reports.at(-1)).toMatchObject({ handlerRef: R1, state: 'disconnected' });
    expect(rig.redialDelays()).toHaveLength(1);
    rig.mt.disconnect();
  });

  it('a removal during an in-flight dial aborts it: closed, and NOTHING when it would have resolved', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 3, url: C },
      ]),
    );
    const cMock = rig.dials.latest(C);
    const closeC = vi.spyOn(cMock, 'close');
    const metricsBefore = rig.sink.getRecordedMetrics().length;
    const reportsBefore = rig.reports.length;

    rig.mt.applyHandlerSet(intakeOf(3n, [{ ref: 1, url: A }]));
    expect(closeC).toHaveBeenCalledTimes(1);
    // The aborted dial's deadline is gone too: nothing pending.
    expect(rig.sched.size).toBe(0);
    cMock.simulateReady();
    await flushMicrotasks(50);

    expect(rig.sink.getRecordedMetrics()).toHaveLength(metricsBefore);
    expect(rig.reports).toHaveLength(reportsBefore);
    expect(rig.mt.getState(R3)).toBeUndefined();
    rig.mt.disconnect();
  });
});

describe('G3 — a new ref at an IDENTICAL url is a real close + open', () => {
  it('remove ref 1 + add ref 2, same url: the old session closed, a fresh one dialed', async () => {
    const rig = await joined([{ ref: 1, url: A, token: 'tok-1' }]);
    const first = rig.dials.latest(A);
    const closeFirst = vi.spyOn(first, 'close');

    rig.mt.applyHandlerSet(intakeOf(2n, [{ ref: 2, url: A, token: 'tok-2' }]));
    const second = await rig.ready(A);

    expect(rig.dials.count(A)).toBe(2);
    expect(second).not.toBe(first);
    expect(closeFirst).toHaveBeenCalledTimes(1);
    expect(connectRequestsOn(second)).toEqual([{ mediaDialToken: 'tok-2' }]);
    expect(rig.mt.getState(R1)).toBeUndefined();
    expect(rig.mt.getState(R2)).toBe('connected');
    rig.mt.disconnect();
  });
});

describe('G4 — a token-only refresh is no dial and no close; the latest token is reused', () => {
  it('same refs + urls, new tokens: zero connects, zero closes; the next redial carries the NEW token', async () => {
    const rig = await joined([{ ref: 1, url: A, token: 'tok-A1' }]);
    const mock = rig.dials.latest(A);
    const close = vi.spyOn(mock, 'close');

    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 'tok-A2' }]));
    await flushMicrotasks(50);
    expect(rig.dials.count(A)).toBe(1);
    expect(close).not.toHaveBeenCalled();
    expect(rig.mt.getState(R1)).toBe('connected');

    // Positive arm: a loss → the redial is a real dial, with the refreshed token.
    mock.simulateError(new Error('lost'));
    await flushMicrotasks(50);
    await rig.fireNext();
    expect(rig.dials.count(A)).toBe(2);
    await rig.ready(A);
    expect(connectRequestsOn(rig.dials.latest(A))).toEqual([{ mediaDialToken: 'tok-A2' }]);
    rig.mt.disconnect();
  });
});

describe('G4b — a token-only refresh never bypasses a pending redial or doubles a dial', () => {
  it('ref LOST with a redial pending: no dial, the SAME timer, and firing it carries the NEW token', async () => {
    const rig = await joined([{ ref: 1, url: A, token: 'old' }]);
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    const before = rig.sched.pending();
    expect(before).toHaveLength(1);
    // MC-D: the FAILED/DISCONNECTED report makes MC re-push at once.
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 'new' }]));
    await flushMicrotasks(50);
    expect(rig.dials.count(A)).toBe(1);
    expect(rig.sched.pending()).toEqual(before);
    await rig.fireNext();
    await rig.ready(A);
    expect(connectRequestsOn(rig.dials.latest(A))).toEqual([{ mediaDialToken: 'new' }]);
    rig.mt.disconnect();
  });

  it('ref still CONNECTING (dial in flight): a refresh makes no second connect', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b1' },
      ]),
    );
    expect(rig.dials.count(B)).toBe(1);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b2' },
      ]),
    );
    expect(rig.dials.count(B)).toBe(1);
    await rig.ready(B);
    // The in-flight dial reads the token at request build: the refreshed one.
    expect(connectRequestsOn(rig.dials.latest(B))).toEqual([{ mediaDialToken: 'b2' }]);
    rig.mt.disconnect();
  });
});

describe('G5 / T2 — DECLINED: park, FAILED, no dial until a DIFFERENT token, then exactly one', () => {
  it('triggered on an ADDED ref: parked across a cap-length advance; a same-token set keeps the park', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b-1' },
      ]),
    );
    const bMock = await rig.ready(B);

    bMock.simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    expect(rig.mt.getState(R2)).toBe('parked');
    expect(rig.mt.diagnostics().declinedCloses).toBe(1);
    expect(rig.reports.at(-1)).toMatchObject({
      handlerRef: R2,
      handlerSetVersion: 2n,
      state: 'failed',
      failureCode: 'DECLINED',
    });

    rig.sched.advance(10 * HANDLERS.redialCapMs);
    await flushMicrotasks(50);
    expect(rig.dials.count(B)).toBe(1);

    // The DISTINGUISHING negative: a set repeating the declined token keeps the park.
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b-1' },
      ]),
    );
    await flushMicrotasks(50);
    expect(rig.dials.count(B)).toBe(1);
    expect(rig.mt.getState(R2)).toBe('parked');

    // A refresh with a NEWER token releases it: exactly one dial, with that token.
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B, token: 'b-2' },
      ]),
    );
    await flushMicrotasks(50);
    expect(rig.dials.count(B)).toBe(2);
    // Exactly one: nothing but that dial's connect deadline is pending for it.
    expect(rig.redialDelays()).toEqual([]);
    await rig.ready(B);
    expect(rig.dials.count(B)).toBe(2);
    expect(connectRequestsOn(rig.dials.latest(B))).toEqual([{ mediaDialToken: 'b-2' }]);
    expect(rig.mt.getState(R2)).toBe('connected');
    rig.mt.disconnect();
  });

  it('a refresh landing while the declined dial was in flight: NO park — exactly one dial with the newer token', async () => {
    // The dial wrote 't1'; MC's refresh ('t2') arrives before MH's verdict.
    const rig = await joined([{ ref: 1, url: A, token: 't1' }]);
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 't2' }]));
    rig.dials.latest(A).simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    expect(rig.mt.getState(R1)).toBe('connecting');
    expect(rig.dials.count(A)).toBe(2);
    expect(rig.reports.at(-1)).toMatchObject({ handlerRef: R1, state: 'failed' });
    await rig.ready(A);
    expect(connectRequestsOn(rig.dials.latest(A))).toEqual([{ mediaDialToken: 't2' }]);
    expect(rig.dials.count(A)).toBe(2);

    // Positive arm: with NO refresh since that dial, DECLINED parks and dials nothing.
    rig.dials.latest(A).simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    expect(rig.mt.getState(R1)).toBe('parked');
    rig.sched.advance(10 * HANDLERS.redialCapMs);
    await flushMicrotasks(50);
    expect(rig.dials.count(A)).toBe(2);
    rig.mt.disconnect();
  });

  it('DECLINED twice → FAILED twice (the re-push trigger is never change-gated)', async () => {
    const rig = await joined([{ ref: 1, url: A, token: 't1' }]);
    rig.dials.latest(A).simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 't2' }]));
    await rig.ready(A);
    rig.dials.latest(A).simulateClose(MhCloseCode.DECLINED);
    await flushMicrotasks(50);
    const failed = rig.reports.filter((r) => r.state === 'failed');
    expect(failed).toHaveLength(2);
    expect(rig.mt.diagnostics().declinedCloses).toBe(2);
    rig.mt.disconnect();
  });
});

describe('G6 — the close-code table (ADR-0039 §7)', () => {
  it.each([
    ['WRONG_HANDLER', MhCloseCode.WRONG_HANDLER],
    ['BINDING_CONFLICT', MhCloseCode.BINDING_CONFLICT],
    ['MC_UNAVAILABLE', MhCloseCode.MC_UNAVAILABLE],
  ])(
    '%s: counted, NO report, backoff redial (first no earlier than the floor), token reused',
    async (_n, code) => {
      const rig = await joined([{ ref: 1, url: A, token: 'held' }]);
      const before = rig.reports.length;
      rig.dials.latest(A).simulateClose(code);
      await flushMicrotasks(50);

      expect(rig.reports).toHaveLength(before);
      expect(rig.mt.diagnostics().retryCloses).toBe(1);
      expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
      await rig.fireNext();
      await rig.ready(A);
      expect(connectRequestsOn(rig.dials.latest(A))).toEqual([{ mediaDialToken: 'held' }]);
      rig.mt.disconnect();
    },
  );

  it('HANDLER_RELEASED, ref already left the set: no event at all', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    const mock = rig.dials.latest(A);
    rig.mt.applyHandlerSet(intakeOf(2n, []));
    const before = rig.reports.length;
    mock.simulateClose(MhCloseCode.HANDLER_RELEASED);
    await flushMicrotasks(50);
    expect(rig.reports).toHaveLength(before);
    expect(rig.mt.diagnostics().releasedCloses).toBe(0);
    rig.mt.disconnect();
  });

  it('HANDLER_RELEASED in set + a set update inside the grace: no report, normal redial', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    const before = rig.reports.length;
    rig.dials.latest(A).simulateClose(MhCloseCode.HANDLER_RELEASED);
    await flushMicrotasks(50);
    expect(rig.redialDelays()).toEqual([HANDLERS.releasedGraceMs]);
    expect(rig.mt.diagnostics().releasedCloses).toBe(1);
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 'refreshed' }]));
    expect(rig.reports).toHaveLength(before);
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
    rig.mt.disconnect();
  });

  it('HANDLER_RELEASED in set, grace expires: DISCONNECTED, then a redial', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.dials.latest(A).simulateClose(MhCloseCode.HANDLER_RELEASED);
    await flushMicrotasks(50);
    expect(rig.mt.diagnostics().releasedCloses).toBe(1);
    expect(await rig.fireNext()).toBe(HANDLERS.releasedGraceMs);
    expect(rig.reports.at(-1)).toMatchObject({ handlerRef: R1, state: 'disconnected' });
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
    rig.mt.disconnect();
  });

  it.each([
    ['zero', MhCloseCode.UNSPECIFIED],
    ['unknown', 9999],
  ])(
    'a %s code: ONE contract-fault WARN per session, handled as loss, never fatal',
    async (_n, code) => {
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const rig = await joined([{ ref: 1, url: A }]);
      rig.dials.latest(A).simulateClose(code);
      await flushMicrotasks(50);
      expect(rig.reports.at(-1)).toMatchObject({ handlerRef: R1, state: 'disconnected' });
      expect(rig.mt.diagnostics().contractFaultCloses).toBe(1);
      // Still usable: the redial connects, and a second bad close does not WARN again.
      await rig.fireNext();
      await rig.ready(A);
      expect(rig.mt.getState(R1)).toBe('connected');
      rig.dials.latest(A).simulateClose(code);
      await flushMicrotasks(50);
      expect(rig.mt.diagnostics().contractFaultCloses).toBe(2);
      const faultWarns = warn.mock.calls.filter((c) => c[0] === '[dt-signaling-contract]');
      expect(faultWarns).toHaveLength(1);
      // Bounded text: no url, no token.
      expect(String(faultWarns[0]![1])).not.toContain('mh-a');
      rig.mt.disconnect();
    },
  );

  it('no code (closed rejects: crash / idle timeout): DISCONNECTED + redial', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.dials.latest(A).simulateError(new Error('idle timeout'));
    await flushMicrotasks(50);
    expect(rig.reports.at(-1)).toMatchObject({ handlerRef: R1, state: 'disconnected' });
    expect(rig.mt.diagnostics().lossCloses).toBe(1);
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
    rig.mt.disconnect();
  });
});

describe('T4 — the first-retry floor and the attempt reset', () => {
  it('first retry = floor (random 0) whatever the trigger; a failed redial backs off WITHOUT the floor', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.dials.latest(A).simulateClose(MhCloseCode.BINDING_CONFLICT);
    await flushMicrotasks(50);
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
    await rig.fireNext();
    await rig.refuse(A);
    // n = 1 with random 0: no floor, zero jitter.
    expect(rig.redialDelays()).toEqual([0]);
    await rig.fireNext();
    await rig.ready(A);
    // Reset after a connect: the NEXT loss is n = 0 again.
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS]);
    rig.mt.disconnect();
  });

  it('random 0.999…: the window doubles from the base and saturates at the cap', async () => {
    const rig = await joined([{ ref: 1, url: A }], () => 0.999_999);
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS + 999]);
    const seen: number[] = [];
    for (let i = 0; i < 7; i++) {
      await rig.fireNext();
      await rig.refuse(A);
      seen.push(...rig.redialDelays());
    }
    expect(seen).toEqual([1_999, 3_999, 7_999, 15_999, 29_999, 29_999, 29_999]);
    rig.mt.disconnect();
  });
});

describe('T4b — a retry-class close is NOT a healthy connect: the exponent keeps growing', () => {
  it.each([
    ['BINDING_CONFLICT', MhCloseCode.BINDING_CONFLICT],
    ['MC_UNAVAILABLE', MhCloseCode.MC_UNAVAILABLE],
    ['unknown code', 9999],
  ])(
    'connect then %s, repeatedly: delays grow to the cap; a later LOSS after a healthy connect returns to the floor',
    async (_n, code) => {
      vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const rig = await joined([{ ref: 1, url: A }], () => 0.999_999);
      const seen: number[] = [];
      for (let i = 0; i < 7; i++) {
        rig.dials.latest(A).simulateClose(code);
        await flushMicrotasks(50);
        seen.push(...rig.redialDelays());
        await rig.fireNext();
        await rig.ready(A);
      }
      expect(seen).toEqual([
        MH_KEEPALIVE_FLOOR_MS + 999,
        1_999,
        3_999,
        7_999,
        15_999,
        29_999,
        29_999,
      ]);
      // Positive arm: a transport LOSS after the (now healthy) connect resets to n = 0.
      rig.dials.latest(A).simulateError(new Error('lost'));
      await flushMicrotasks(50);
      expect(rig.redialDelays()).toEqual([MH_KEEPALIVE_FLOOR_MS + 999]);
      rig.mt.disconnect();
    },
  );
});

describe('G7 — backoff stops the moment the ref leaves the set', () => {
  it('a pending redial is cancelled by the removal; firing everything dials nothing', async () => {
    const rig = await joined([
      { ref: 1, url: A },
      { ref: 2, url: B },
    ]);
    rig.dials.latest(B).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    expect(rig.redialDelays()).toHaveLength(1);
    rig.mt.applyHandlerSet(intakeOf(2n, [{ ref: 1, url: A }]));
    expect(rig.redialDelays()).toEqual([]);
    rig.sched.advance(10 * HANDLERS.redialCapMs);
    await flushMicrotasks(50);
    expect(rig.dials.count(B)).toBe(1);
    // Positive arm: the in-set ref's redial still fires.
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    await rig.fireNext();
    expect(rig.dials.count(A)).toBe(2);
    rig.mt.disconnect();
  });
});

describe('G8 — one report per REAL transition', () => {
  it('N failed redials of one lost ref → exactly ONE loss report, then CONNECTED on recovery', async () => {
    const rig = await joined([{ ref: 1, url: A }]);
    rig.dials.latest(A).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    for (let i = 0; i < 4; i++) {
      await rig.fireNext();
      await rig.refuse(A);
    }
    await rig.fireNext();
    await rig.ready(A);
    // CONNECTED (join), DISCONNECTED (loss), FAILED (first failed redial), CONNECTED.
    expect(rig.reports.map((r) => r.state)).toEqual([
      'connected',
      'disconnected',
      'failed',
      'connected',
    ]);
    expect(rig.reports.every((r) => r.handlerRef === R1 && r.handlerSetVersion === 1n)).toBe(true);
    rig.mt.disconnect();
  });
});

describe('G9 — an empty join set is "media connecting"', () => {
  it('join resolves with no dial, no fault, no report; the first set with a member dials normally', async () => {
    const rig = makeRig();
    rig.mt.applyHandlerSet(intakeOf(1n, []));
    await expect(rig.mt.arm()).resolves.toBeUndefined();
    expect(rig.dials.dials).toHaveLength(0);
    expect(rig.reports).toEqual([]);

    rig.mt.applyHandlerSet(intakeOf(2n, [{ ref: 1, url: A }]));
    await rig.ready(A);
    expect(rig.mt.getState(R1)).toBe('connected');
    expect(rig.overdue).toEqual([]);
    rig.sched.advance(HANDLERS.emptySetNoticeMs * 2);
    expect(rig.overdue).toEqual([]);
    rig.mt.disconnect();
  });

  it('past the bound with no handler: exactly ONE notice, the session kept', async () => {
    const rig = makeRig();
    rig.mt.applyHandlerSet(intakeOf(1n, []));
    await rig.mt.arm();
    rig.sched.advance(HANDLERS.emptySetNoticeMs - 1);
    expect(rig.overdue).toEqual([]);
    rig.sched.advance(1);
    expect(rig.overdue).toHaveLength(1);
    rig.sched.advance(HANDLERS.emptySetNoticeMs * 10);
    expect(rig.overdue).toHaveLength(1);
    // Kept: a later set still connects.
    rig.mt.applyHandlerSet(intakeOf(2n, [{ ref: 1, url: A }]));
    await rig.ready(A);
    expect(rig.mt.getState(R1)).toBe('connected');
    rig.mt.disconnect();
  });
});

describe('G12 — mh_index_bucket is the position in the TRIGGERING set', () => {
  it('ref B at index 0 at join, index 1 in a later set: its redial counts bucket 1', async () => {
    const rig = await joined([{ ref: 2, url: B }]);
    rig.sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '0' },
      1,
    );
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B },
      ]),
    );
    await rig.ready(A);
    rig.dials.latest(B).simulateError(new Error('lost'));
    await flushMicrotasks(50);
    await rig.fireNext();
    await rig.ready(B);
    // A (index 0) and the join-time B: bucket 0 twice; B's redial at index 1.
    rig.sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '0' },
      2,
    );
    rig.sink.assertCounter(
      'dt_client_mh_connection_total',
      { status: 'success', mh_index_bucket: '1' },
      1,
    );
    rig.mt.disconnect();
  });
});

describe('T3 — sets before and during arm', () => {
  it('a set before arm dials nothing; arm dials it; a set during the armed dials does not double-dial', async () => {
    const rig = makeRig();
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A }]));
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 'refreshed' }]));
    expect(rig.dials.dials).toHaveLength(0);

    const armed = rig.mt.arm();
    expect(rig.dials.count(A)).toBe(1);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B },
      ]),
    );
    expect(rig.dials.count(A)).toBe(1);
    expect(rig.dials.count(B)).toBe(1);
    await rig.ready(A);
    await rig.ready(B);
    await armed;
    expect(connectRequestsOn(rig.dials.latest(A))[0]).toEqual({ mediaDialToken: 'tok-1' });
    // The ref added WHILE the armed dials were in flight dials once, on its token.
    expect(connectRequestsOn(rig.dials.latest(B))).toEqual([{ mediaDialToken: 'tok-2' }]);
    rig.mt.disconnect();
  });
});

describe('T7 / ops — session end leaves nothing behind', () => {
  it('disconnect cancels every timer and in-flight dial: no later set or timer dials', async () => {
    // Token CLEARING at disconnect (`#dialTokens.clear()`) is deliberately not
    // observable from outside — no getter exists, by design — and is pinned by
    // the residency scan (`__tests__/serverMessageSinkScan.test.ts`) and review.
    const rig = await joined([
      { ref: 1, url: A },
      { ref: 2, url: B },
    ]);
    // A pending redial, a pending grace, and a dial in flight.
    rig.dials.latest(A).simulateError(new Error('lost'));
    rig.dials.latest(B).simulateClose(MhCloseCode.HANDLER_RELEASED);
    rig.mt.applyHandlerSet(
      intakeOf(2n, [
        { ref: 1, url: A },
        { ref: 2, url: B },
        { ref: 3, url: C },
      ]),
    );
    await flushMicrotasks(50);
    expect(rig.sched.size).toBeGreaterThan(0);

    rig.mt.disconnect();
    expect(rig.sched.size).toBe(0);
    const dialsAtEnd = rig.dials.dials.length;
    rig.mt.applyHandlerSet(intakeOf(3n, [{ ref: 4, url: A }]));
    await rig.mt.arm();
    await flushMicrotasks(50);
    expect(rig.dials.dials).toHaveLength(dialsAtEnd);
  });

  it('join failing after a set was taken but BEFORE arm: no dial and no timer ever happens', async () => {
    const rig = makeRig();
    rig.mt.applyHandlerSet(intakeOf(1n, [{ ref: 1, url: A, token: 'never-sent' }]));
    rig.mt.disconnect();
    await rig.mt.arm();
    await flushMicrotasks(50);
    expect(rig.dials.dials).toHaveLength(0);
    expect(rig.sched.size).toBe(0);
  });
});
