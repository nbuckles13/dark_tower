// File: packages/web-app/e2e/mhLeg.ts
//
// Story 4 C3: the LEG SCAFFOLD for the browser MH-replacement legs
// (`mh-replacement.spec.ts`; ADR-0039 §4, §7, §9; test-gates.md §Browser). The
// browser twin of the Rust `MhPool::run_leg` / `up_scraped` /
// `teardown_and_settle` (`crates/env-tests/src/fixtures/mh_pool.rs`), the
// owned-placement search (`owned_placement.rs`) and the removed-MH settle
// (`gc_fleet.rs`). `mhPool.ts` hands this file the parts it says "C3 owns": the
// derived `assertBaseline` ceiling and the post-teardown settle wait.
//
// EVERY BOUND IS DERIVED and FAILURE-ONLY (a poll ceiling, never a sleep, never
// a latency target): from deployed config (`configEnv.ts:resilienceTimings`),
// from a service's published config-echo gauge (`mcMetrics.ts`), from the
// suite's existing liveness bounds (`fixtures.ts:HEAR_SENDER_TIMEOUT_MS`,
// `JOIN_LIVENESS_MS`), or from a Rust constant twinned in `mhLegRules.ts`
// (pinned by `tests/mhLeg.test.ts`). The one named number that is not a
// ceiling, {@link LEG_BACKSTOP_MS}, never gates.
//
// HYGIENE — a TEST-SCOPED FIXTURE (`legTest`'s `leg`), not inline code, so the
// AFTER steps run even when the test TIMES OUT: Playwright runs test-scoped
// fixture teardown in its own "After Hooks" slot after the test function, with
// a timeout of max(project timeout, the test's timeout) — playwright 1.64
// `lib/worker/workerProcessEntry.js` (`afterHooksSlot`, `teardownScope("test",
// …)`), docs "Test timeouts".
//   BEFORE  the pool is {baseline B, nothing foreign}; NOTHING run-owned is live
//           (a leak from an earlier timed-out or killed leg is owned by this
//           same run, which the baseline verdict alone accepts). No fleet-wide
//           MC check (operator ruling: tests never couple through a shared
//           counter); a meeting that could be handed this leg's run-owned MH is
//           one released on MH behind MC's back, and the suites that do that
//           prove their OWN meeting ended (Rust `fixtures::meeting_end`);
//   AFTER   on EVERY exit path, in order, each an explicit asserted step (never a
//           Drop-style backstop): every opened context leaves and closes; EACH
//           meeting the leg opened is PROVED ended and released on MC from its
//           own Loki lines (`mcLoki.ts`; so the owner teardown cannot make MC
//           re-place a live meeting); `mh.sh drain --all --owner <run>` with the
//           leak check; no GC replica still counts a removed MH selectable; the
//           baseline again.
//   A teardown failure is the fixture's OWN error: Playwright reports it beside
//   the test body's error (both in `testInfo.errors`), so neither hides the other.
//
// NOTHING IDENTIFYING LEAVES: logs and annotations carry slot numbers, handler
// refs and handler URLs (config, not identity) — never a token, an email or a
// meeting code.

import { test as base, type Browser, type Page, type TestInfo } from 'playwright/test';
import {
  closeMembers,
  joinMember,
  openMember,
  type ContextLevers,
  type JoinedMember,
  type Member,
} from './cohortContexts.js';
import { resilienceTimings, type ResilienceTimings } from './configEnv.js';
import {
  bootstrapMeeting,
  handlerUrls,
  HEAR_SENDER_TIMEOUT_MS,
  JOIN_LIVENESS_MS,
  pollConverge,
} from './fixtures.js';
import { instanceForPodIp } from './instanceCounters.js';
import {
  gcRecomputeSuccessByInstance,
  gcSelectableByInstance,
  gcStalenessSecondsByReplica,
  mcDisconnectGraceMs,
  mcReplacementRetryBudgetMs,
  mhScrapeTimestampByInstance,
  mhScrapeUpByInstance,
  servingGcInstances,
} from './mcMetrics.js';
import {
  createMhPool,
  MH_SCRAPE_UP_TIMEOUT,
  runLeakVerdict,
  servingSlotCount,
  type MhPool,
  type SlotEntry,
} from './mhPool.js';
import {
  LOG_AGGREGATION_CEILING_MS,
  METRICS_SCRAPE_CEILING_MS,
  OWNED_MH_NOT_PLACED,
  PLACEMENT_MISS_CEILING,
  REMOVED_MH_STILL_SELECTABLE,
  placementAttempts,
  placementHit,
  rebaseline,
  replicaSettled,
  resilienceCeilings,
  type ReplicaSettle,
  type ResilienceCeilings,
} from './mhLegRules.js';
import {
  classifyMember,
  meetingEndBudgetMs,
  proveMeetingsEnded,
  scopeStartNs,
  type MemberDeparture,
} from './mcLoki.js';

/** Poll CADENCE for the Prometheus and pool reads below (a cadence, never a bound). */
const POLL_MS = 2_000;

/**
 * The Playwright per-test timeout's BACKSTOP share. It NEVER gates: every gating
 * wait carries its own derived ceiling, and no wait here uses this as its
 * timeout. It absorbs only what the caller cannot see in advance: mh.sh's
 * internally derived `up` / `drain` / restart bounds (contract §5: derived from
 * the live pod's probes and grace, not exposed before the call) and its BUSY
 * retry bound. The placement search's joins are budgeted separately, from the
 * named join liveness bounds, once the attempt count is known.
 */
export const LEG_BACKSTOP_MS = 300_000;

/**
 * One placement attempt's liveness allowance: the join's two named bounds
 * (`JOIN_LIVENESS_MS`: MC JoinResponse, every handler connected) plus the own-
 * tone read's hearing liveness. Composed, never restated.
 */
const PLACEMENT_ATTEMPT_ALLOWANCE_MS = 2 * JOIN_LIVENESS_MS + HEAR_SENDER_TIMEOUT_MS;

// ---------------------------------------------------------------------------
// Bus reads (each member's OWN bus; never tokens)
// ---------------------------------------------------------------------------

/** The member's reconnecting-overlay history and transport-fault COUNT (the bus arrays MUST exist). */
export async function busContinuity(member: Pick<Member, 'label' | 'page'>): Promise<{
  readonly overlays: ReadonlyArray<{ readonly slotId: number; readonly reconnecting: boolean }>;
  readonly faultCount: number;
}> {
  const read = await member.page.evaluate(() => {
    const bus = window.__darktower_test__;
    return {
      overlays: bus?.reconnectingOverlays as unknown,
      faults: bus?.transportFaults as unknown,
    };
  });
  if (!Array.isArray(read.overlays) || !Array.isArray(read.faults)) {
    throw new Error(
      `HARNESS: ${member.label}'s bus has no reconnectingOverlays/transportFaults arrays — an ` +
        'absent bus must never read as "no overlay, no fault"',
    );
  }
  return {
    overlays: read.overlays as Array<{ slotId: number; reconnecting: boolean }>,
    faultCount: read.faults.length,
  };
}

// ---------------------------------------------------------------------------
// The Prometheus-cadence kernel and the waits built on it
// ---------------------------------------------------------------------------

/** `fixtures.ts:pollConverge` (THE poll loop) on a Prometheus cadence of `cadenceMs`. */
function pollProm<T>(
  read: () => Promise<T>,
  done: (value: T) => boolean,
  boundMs: number,
  cadenceMs: number,
): Promise<{ readonly converged: boolean; readonly last: T }> {
  return pollConverge(read, done, boundMs, () => sleep(cadenceMs));
}

/** The removal-settle budget (twin of `gc_fleet::removal_settle_budget`): max B + 2 ticks + scrape. */
async function removalSettleBudgetMs(timings: ResilienceTimings): Promise<number> {
  const replicas = await servingGcInstances();
  const staleness = await gcStalenessSecondsByReplica(replicas);
  const maxB = Math.max(...staleness.values());
  if (!(maxB > 0)) {
    throw new Error(`PRECONDITION: GC publishes a non-positive staleness bound (${maxB} s)`);
  }
  return Math.round(maxB * 1000) + 2 * timings.gcHealthTickMs + METRICS_SCRAPE_CEILING_MS;
}

/**
 * Twin of `gc_fleet::wait_removed_mh_unselectable`: after the owner teardown, on
 * EVERY serving GC replica, a whole recompute ran and the selectable rows are at
 * most the serving slots in `mh.sh list`. Never summed across replicas.
 */
async function waitRemovedMhUnselectable(pool: MhPool, budgetMs: number): Promise<void> {
  const replicas = await servingGcInstances();
  const live = servingSlotCount(await pool.list());
  const first = await gcRecomputeSuccessByInstance();
  // Per replica, the first PRESENT sample is the baseline. A series absent at
  // wait start (scrape gap, not yet scraped) is baselined at its first
  // appearance, never at 0, and still needs +SETTLE_RECOMPUTES past that.
  const bases = new Map<string, number | undefined>(replicas.map((r) => [r, first.get(r)]));
  const { converged, last } = await pollProm(
    async () => {
      const [selectable, recompute] = await Promise.all([
        gcSelectableByInstance(),
        gcRecomputeSuccessByInstance(),
      ]);
      const pending: string[] = [];
      for (const r of replicas) {
        const now = recompute.get(r);
        // A reset counter (process restart) re-baselines at 0 BEFORE the
        // comparison; an unseen baseline is adopted AFTER it, so the first
        // present sample never settles on its own (gc_fleet.rs, same order).
        const seen = bases.get(r) !== undefined;
        if (seen) bases.set(r, rebaseline(bases.get(r), now));
        const view: ReplicaSettle = {
          selectable: selectable.get(r),
          recomputeBaseline: bases.get(r),
          recomputeNow: now,
        };
        if (!seen) bases.set(r, rebaseline(undefined, now));
        if (!replicaSettled(view, live)) {
          pending.push(
            `${r}: selectable=${view.selectable ?? 'ABSENT'} recompute=` +
              `${view.recomputeBaseline ?? 'ABSENT'}->${view.recomputeNow ?? 'ABSENT'}`,
          );
        }
      }
      return pending;
    },
    (pending) => pending.length === 0,
    budgetMs,
    POLL_MS,
  );
  if (!converged) {
    throw new Error(
      `SETTLE (${REMOVED_MH_STILL_SELECTABLE}): within ${budgetMs} ms (B + 2 GC ticks + scrape) ` +
        `GC replica(s) still count more selectable rows than the ${live} serving slot(s), or ` +
        `never completed a recompute: [${last.join('; ')}]`,
    );
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// ---------------------------------------------------------------------------
// The leg context and fixture
// ---------------------------------------------------------------------------

/** What a leg body gets. */
export interface LegContext {
  readonly pool: MhPool;
  readonly timings: ResilienceTimings;
  readonly ceilings: ResilienceCeilings;
  /** Open a member context the fixture will leave and close on every exit path. */
  open(index: number, levers?: ContextLevers): Promise<Member>;
  /** Leave and close one member now (a placement miss), asserted. */
  release(member: Member): Promise<void>;
  /** `pool.up()`, then wait for Prometheus to scrape it (before ANY metric baseline). */
  upScraped(): Promise<SlotEntry>;
  /**
   * Wait until Prometheus scraped `entry`'s pod with `up == 1` in a sample TAKEN
   * at or after `notBeforeMs` (epoch ms). After an in-place restart the pod IP
   * (so the instance) is unchanged and the pre-restart `up == 1` would satisfy a
   * plain `up` read: pass the time the restart was observed before re-baselining.
   */
  waitScraped(entry: SlotEntry, notBeforeMs?: number): Promise<void>;
  /**
   * Create meetings (probe member `probeIndex`, unblocked) until one is placed
   * on `owned` with exactly GC's per-meeting handler count; derived attempts.
   * Extends the test timeout by the derived attempt allowance.
   */
  placeOnOwned(
    probeIndex: number,
    owned: SlotEntry,
    title: string,
  ): Promise<{ probe: JoinedMember; meetingCode: string; survivors: string[] }>;
}

/** The leg's derived bounds (read once, before the leg). */
interface LegBounds {
  readonly timings: ResilienceTimings;
  readonly ceilings: ResilienceCeilings;
  readonly settleBudgetMs: number;
  /** The per-meeting end proof's FAILURE ceiling: MC's published grace + log ingest. */
  readonly meetingEndCeilingMs: number;
}

async function legBounds(testInfo: TestInfo): Promise<LegBounds> {
  const timings = resilienceTimings();
  const retryBudgetMs = await mcReplacementRetryBudgetMs();
  const ceilings = resilienceCeilings(timings, retryBudgetMs, HEAR_SENDER_TIMEOUT_MS);
  const settleBudgetMs = await removalSettleBudgetMs(timings);
  // MC holds a browser leaver through its disconnect grace today (docs/TODO.md,
  // "MC classifies the browser SDK's CLEAN disconnect as `server_initiated`"), so
  // the end proof's ceiling is that grace, read from MC's config echo (the same
  // source as Rust `metrics::mc_disconnect_grace`), plus the log-ingest ceiling.
  const meetingEndCeilingMs = (await mcDisconnectGraceMs()) + LOG_AGGREGATION_CEILING_MS;
  testInfo.setTimeout(
    // before/after baseline + the removed-MH settle + the meeting-end proof's whole
    // budget (`meetingEndBudgetMs`: its shared deadline + one release window) +
    // the leg's own ceilings (kill/drain view, replacement), + the backstop.
    // Playwright gives fixture teardown its own slot of this size after a timeout.
    3 * settleBudgetMs +
      meetingEndBudgetMs(meetingEndCeilingMs) +
      ceilings.replacementMs +
      ceilings.survivorMs +
      LEG_BACKSTOP_MS,
  );
  testInfo.annotations.push({
    type: 'resilience-ceilings-ms',
    description: JSON.stringify({ ...ceilings, settleBudgetMs, meetingEndCeilingMs }),
  });
  return { timings, ceilings, settleBudgetMs, meetingEndCeilingMs };
}

/** BEFORE: baseline, nothing run-owned live. */
async function beforeLeg(pool: MhPool, bounds: LegBounds): Promise<void> {
  await pool.assertBaseline(bounds.settleBudgetMs);
  const leak = runLeakVerdict(await pool.list(), await pool.owner());
  if (leak !== null) {
    throw new Error(
      `PRECONDITION (${leak}) BEFORE the leg: an earlier leg of this run left it (a timeout or ` +
        `a killed runner). Reclaim: ${await pool.reclaimCommand()}`,
    );
  }
}

// ---------------------------------------------------------------------------
// The leg's meetings: what the per-meeting end proof needs (`mcLoki.ts`)
// ---------------------------------------------------------------------------

/** GC's join: `GET /api/v1/meetings/{code}` (create is a POST to the collection). */
const GC_JOIN_PATH = /^\/api\/v1\/meetings\/[^/]+$/;

/**
 * One member page's GC joins: the `meetingId` of EVERY 2xx GC join response the
 * page received (the SDK's join retry can make several). The listener is
 * installed when the member OPENS, before anything can trigger a join. Only
 * `meetingId` is read from the parsed body; the body (it carries the meeting
 * token) is never kept, logged or annotated.
 */
interface JoinWatch {
  readonly meetingIds: string[];
  readonly pending: Promise<void>[];
}

function watchGcJoins(page: Page): JoinWatch {
  const watch: JoinWatch = { meetingIds: [], pending: [] };
  page.on('response', (response) => {
    if (response.request().method() !== 'GET' || !response.ok()) return;
    if (!GC_JOIN_PATH.test(new URL(response.url()).pathname)) return;
    watch.pending.push(
      response.json().then(
        (body: unknown) => {
          const id = (body as { meetingId?: unknown } | null)?.meetingId;
          watch.meetingIds.push(typeof id === 'string' ? id : '<no meetingId>');
        },
        () => {
          watch.meetingIds.push('<unparseable join response>');
        },
      ),
    );
  });
  return watch;
}

/**
 * What `member` contributes to the end proof, read BEFORE it closes: its GC
 * meeting id(s) and every MC participant id its bus `joined` events carried
 * (the SDK fills it from the JoinResponse). Classified by `mcLoki.classifyMember`:
 * no GC join = nothing to prove; a GC join without a participant id fails loudly.
 */
async function departureOf(member: Member, watch: JoinWatch): Promise<MemberDeparture | undefined> {
  await Promise.all(watch.pending);
  const participantIds = await member.page.evaluate(
    () =>
      window.__darktower_test__?.events
        .filter((e) => e['type'] === 'joined')
        .map((e) => e['participantId'])
        .filter((p): p is string => typeof p === 'string') ?? [],
  );
  return classifyMember(member.label, watch.meetingIds, participantIds);
}

/** The leg's meeting bookkeeping, shared by the context and AFTER. */
interface LegMeetings {
  /** Scope start of every Loki read, taken before the leg's first join. */
  readonly sinceNs: bigint;
  readonly watches: Map<Member, JoinWatch>;
  /** Members already left (placement misses, `release`), recorded before they closed. */
  readonly departed: MemberDeparture[];
}

/** Record `member`'s departure (if it joined), THEN close it — the close runs even if recording fails. */
async function recordAndClose(member: Member, meetings: LegMeetings): Promise<void> {
  try {
    const watch = meetings.watches.get(member);
    if (watch === undefined) throw new Error(`member ${member.label} was opened outside the leg`);
    const d = await departureOf(member, watch);
    if (d !== undefined) meetings.departed.push(d);
  } finally {
    await closeMembers([member]);
  }
}

/** AFTER, every exit path: each step asserted, every failure collected and thrown together. */
async function afterLeg(
  pool: MhPool,
  opened: Member[],
  bounds: LegBounds,
  meetings: LegMeetings,
): Promise<void> {
  const failures: string[] = [];
  const step = async (name: string, fn: () => Promise<void>): Promise<void> => {
    try {
      await fn();
    } catch (err) {
      failures.push(`${name}: ${messageOf(err)}`);
    }
  };
  for (const member of opened.splice(0)) {
    await step(`leave ${member.label}`, () => recordAndClose(member, meetings));
  }
  // BEFORE the owner teardown, on purpose: a meeting still live when the
  // run-owned MH goes would make MC acquire a replacement for it (the orphan
  // class). `leaveGracefully` proves only that the SDK reached `disconnecting`,
  // not that MC heard the leave, so each meeting is PROVED ended and released
  // from MC's own lines (`mcLoki.ts`). Do not "simplify" this back into a
  // fleet-counter wait or drop it. The departure check accepts MC's grace-expiry
  // line beside its clean-close line because MC classifies the SDK's leave as
  // `server_initiated` today (docs/TODO.md, "MC classifies the browser SDK's
  // CLEAN disconnect as `server_initiated`"); that is also why this step costs
  // about one disconnect grace until that is fixed.
  if (meetings.departed.length > 0) {
    await step('meetings ended (per-meeting proof)', () =>
      proveMeetingsEnded(meetings.departed, meetings.sinceNs, bounds.meetingEndCeilingMs),
    );
  }
  await step('owner teardown', () => pool.teardown());
  await step('removed MH unselectable', () =>
    waitRemovedMhUnselectable(pool, bounds.settleBudgetMs),
  );
  await step('baseline restored', () => pool.assertBaseline(bounds.settleBudgetMs));
  if (failures.length === 0) return;
  const reclaim = await pool
    .reclaimCommand()
    .catch(
      (e: unknown) => `<mh.sh> drain --all --owner <run> (reclaimCommand failed: ${messageOf(e)})`,
    );
  const text = `LEG TEARDOWN failed (reclaim: ${reclaim}):\n  ${failures.join('\n  ')}`;
  console.error(`[mh-replacement] ${text}`);
  throw new Error(text);
}

function legContext(
  browser: Browser,
  testInfo: TestInfo,
  pool: MhPool,
  bounds: LegBounds,
  opened: Member[],
  meetings: LegMeetings,
): LegContext {
  const initialHandlers = bounds.timings.initialHandlers;
  const ctx: LegContext = {
    pool,
    timings: bounds.timings,
    ceilings: bounds.ceilings,
    async open(index, levers) {
      const m = await openMember(browser, index, levers);
      meetings.watches.set(m, watchGcJoins(m.page));
      opened.push(m);
      return m;
    },
    async release(member) {
      const i = opened.indexOf(member);
      if (i >= 0) opened.splice(i, 1);
      await recordAndClose(member, meetings);
    },
    async upScraped() {
      const entry = await pool.up();
      await ctx.waitScraped(entry);
      return entry;
    },
    async waitScraped(entry, notBeforeMs) {
      const ip = entry.pod_ip;
      if (ip === null) {
        throw new Error(`PRECONDITION: slot ${entry.slot} has no pod_ip to pin its scrape`);
      }
      const { converged, last } = await pollProm(
        async () => {
          const [up, at] = await Promise.all([
            mhScrapeUpByInstance(),
            mhScrapeTimestampByInstance(),
          ]);
          return { up: instanceForPodIp(up, ip), at: instanceForPodIp(at, ip) };
        },
        ({ up, at }) =>
          up === 1 && (notBeforeMs === undefined || (at !== undefined && at * 1000 >= notBeforeMs)),
        METRICS_SCRAPE_CEILING_MS,
        POLL_MS / 2,
      );
      if (!converged) {
        throw new Error(
          `${MH_SCRAPE_UP_TIMEOUT}: Prometheus never scraped slot ${entry.slot} (${ip}) with up == 1` +
            (notBeforeMs === undefined ? '' : ` in a sample taken after ${notBeforeMs}`) +
            ` within ${METRICS_SCRAPE_CEILING_MS} ms (last: up=${last.up ?? 'ABSENT'}, ` +
            `sampled at ${last.at ?? 'ABSENT'}); no metric baseline may be taken`,
        );
      }
    },
    async placeOnOwned(probeIndex, owned, title) {
      if (initialHandlers < 2) {
        throw new Error(
          `PRECONDITION (${OWNED_MH_NOT_PLACED}): GC_MH_INITIAL_HANDLERS_PER_MEETING=` +
            `${initialHandlers} leaves no survivor beside the owned MH`,
        );
      }
      const selectable = servingSlotCount(await pool.list());
      const attempts = placementAttempts(selectable, initialHandlers, PLACEMENT_MISS_CEILING);
      testInfo.setTimeout(testInfo.timeout + attempts * PLACEMENT_ATTEMPT_ALLOWANCE_MS);
      const observed: string[][] = [];
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        const member = await ctx.open(probeIndex);
        const meetingCode = await bootstrapMeeting(member.token, title);
        const probe = await joinMember(member, meetingCode);
        const urls = handlerUrls(probe.joined.handlerSet);
        if (placementHit(urls, owned.address, initialHandlers)) {
          testInfo.annotations.push({
            type: 'observed-placement-attempts',
            description: `slot ${owned.slot}: ${attempt} of ${attempts}`,
          });
          return {
            probe,
            meetingCode,
            survivors: urls.filter((u) => u !== owned.address).sort(),
          };
        }
        observed.push([...urls].sort());
        await ctx.release(member);
      }
      throw new Error(
        `${OWNED_MH_NOT_PLACED}: ${attempts} meeting(s) (derived: ${selectable} selectable MHs, ` +
          `${initialHandlers} per meeting, miss ceiling ${PLACEMENT_MISS_CEILING}) never placed on ` +
          `slot ${owned.slot} (${owned.address}) at ${initialHandlers} handlers; observed sets ` +
          JSON.stringify(observed),
      );
    },
  };
  return ctx;
}

/**
 * THE leg test: `legTest('…', async ({ leg }) => …)`. The `leg` fixture derives
 * every bound and sets the test timeout from them (+ {@link LEG_BACKSTOP_MS}),
 * asserts BEFORE, hands the body its {@link LegContext}, then runs AFTER in
 * Playwright's fixture-teardown slot — on a pass, a failure AND a timeout.
 */
export const legTest = base.extend<{ leg: LegContext }>({
  leg: async ({ browser }, use, testInfo) => {
    const bounds = await legBounds(testInfo);
    const pool = createMhPool();
    await beforeLeg(pool, bounds);
    const opened: Member[] = [];
    const meetings: LegMeetings = { sinceNs: scopeStartNs(), watches: new Map(), departed: [] };
    try {
      await use(legContext(browser, testInfo, pool, bounds, opened, meetings));
    } finally {
      await afterLeg(pool, opened, bounds, meetings);
    }
  },
});
