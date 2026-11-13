// File: packages/web-app/e2e/mh-replacement.spec.ts
//
// Story 4 C3 — SURVIVE HANDLER LOSS in real browsers (R-1, R-2, R-3, R-20,
// R-39; ADR-0039 §4 and §9; test-gates.md §Browser, §Round 3 "Browser stranded
// A/B leg", §Crash assertions). Three legs, each inside `mhLeg.ts:legTest`'s `leg` fixture
// (baseline + no-foreign-meeting BEFORE; explicit, asserted leave / meetings
// drained / owner teardown / removed-MH settle / baseline AFTER, on every exit
// path):
//
//   CRASH     `kill -KILL` the run-owned MH carrying the meeting (mh.sh signal,
//             a debug container; NEVER a forced, zero-grace pod delete,
//             which defeats the hostPort slot lock, R-28).
//             Every pair's tone resumes within a config-derived ceiling, every
//             participant's final set holds a handler_ref absent from its
//             pre-kill history, and the reconnecting overlay was observed.
//   PLANNED   `mh up` a target, then `mh drain` the carrying MH. Each
//             participant's set history is add-then-remove (make-before-break),
//             no reconnecting overlay and no transport fault for the whole leg
//             (the deterministic negative of the crash leg's positive), tone
//             continues within the ceiling.
//   STRANDED  A and B cannot reach the survivor (`blockHandlers`); C can. Kill
//             the carrying MH: A's and B's final sets hold a new ref and A and B
//             hear each other within the ceiling — exactly a fresh join, no
//             special case (ADR-0039 §4).
//
// THE OWNED MH. mh.sh `signal` / `drain` are owner-gated, so every leg kills or
// drains only an MH it brought up, and a meeting must be placed on it: GC places
// weighted-random, so `placeOnOwned` creates meetings (a probe member, unblocked)
// until one's join-time set names the owned MH — a DERIVED attempt count
// (`owned_placement.rs` twin), never retried, never skipped.
//
// NON-VACUITY (the kill must matter). Contexts are active/active and MC
// colocates a meeting's edges, so with everyone unblocked the kill could
// interrupt nothing. A member blocked from the survivor (B in CRASH and PLANNED,
// A and B in STRANDED) can reach ONLY the owned MH — proven from its own bus
// before the event — so every stream to or from it is on that MH by
// construction. The positive control is keyed on exactly those slots: the slot
// carrying B's stream at A and at C, and B's own slots. Other slots' overlays
// are recorded, never required (which handler MC colocates the rest on is MC's
// preference, not a contract).
//
// URLS AND REFS. Handler URLs come from each page's bus `handlerSets` (MC's
// set; `JoinResponse.media_servers` is gone) and are matched EXACTLY against the
// slot's `address` from `mh.sh list`. Incarnations are told apart by REF, never
// by URL: an in-place restart REUSES the slot's URL (the replacement may even be
// the restarted owned MH), so nothing here assumes the URL changed.
//
// TIMING. Every ceiling is FAILURE-ONLY and derived (`mhLeg.ts:resilienceCeilings`
// over deployed config, MC's published retry budget, and the suite's hearing
// liveness). The media-dark window is RECORDED as an annotation, never gated
// (ADR-0036 §10). No token or identity is logged: refs, slots and handler URLs
// only.

import { expect } from 'playwright/test';
import { joinMember, tonesOf, type JoinedMember } from './cohortContexts.js';
import { withVideoDiagnostics } from './failureDiagnostics.js';
import { connectedUrls, expectHearsSender, handlerSets, pollBus } from './fixtures.js';
import type { InstanceCounters } from './instanceCounters.js';
import { mhAdmissionRejectionsByInstance } from './mcMetrics.js';
import { busContinuity, legTest as test } from './mhLeg.js';
import { makeBeforeBreak, newRefs, refOf, refUnion } from './mhLegRules.js';
import {
  acceptedProbe,
  expectHearsAtAllLayers,
  latestAssignments,
  observeWindow,
} from './receiveEvidence.js';

type Pair = readonly [receiver: JoinedMember, sender: JoinedMember];

/** Every ordered pair of `members`. */
function orderedPairs(members: readonly JoinedMember[]): Pair[] {
  return members.flatMap((r) => members.filter((s) => s !== r).map((s) => [r, s] as const));
}

/** Prove `member` connected ONLY to `url` (the stranding premise, before any event). */
async function expectConnectedOnlyTo(member: JoinedMember, url: string): Promise<void> {
  await expect
    .poll(() => connectedUrls(member.page), {
      message: `${member.label} (blockHandlers = the survivors) connects to the owned MH only`,
    })
    .toEqual([url]);
}

/** The slot at `receiver` carrying `sender`'s audio (by sender id, never by index). */
async function slotCarrying(receiver: JoinedMember, sender: JoinedMember): Promise<number> {
  const hit = (await latestAssignments(receiver)).find((a) => a.senderId === sender.tone.senderId);
  if (hit === undefined) {
    throw new Error(`${receiver.label}: no slot carries ${sender.label} before the event`);
  }
  return hit.slotId;
}

/**
 * `fixtures.ts:pollBus` (THE bus-cadence kernel) on `member`'s page until
 * `done`, bounded by the absolute `deadlineAt` from a derived ceiling; throws
 * `fail(last)` when it did not converge.
 */
async function pollUntil<T>(
  member: JoinedMember,
  read: () => Promise<T>,
  done: (v: T) => boolean,
  deadlineAt: number,
  fail: (last: T) => string,
): Promise<T> {
  const { converged, last } = await pollBus(
    member.page,
    read,
    done,
    Math.max(0, deadlineAt - Date.now()),
  );
  if (!converged) throw new Error(fail(last));
  return last;
}

/** Each member's pre-event state from its OWN bus. */
interface PreEvent {
  readonly union: ReadonlySet<number>;
  readonly latestRefs: readonly number[];
  readonly ownedRef: number;
  readonly overlayCount: number;
  readonly faultCount: number;
}

async function preEvent(member: JoinedMember, ownedUrl: string): Promise<PreEvent> {
  const history = await handlerSets(member.page);
  const latest = history.at(-1);
  const { overlays, faultCount } = await busContinuity(member);
  return {
    union: refUnion(history),
    latestRefs: (latest?.handlers ?? []).map((h) => h.ref).sort((x, y) => x - y),
    ownedRef: refOf(latest, ownedUrl, `${member.label} pre-event`),
    overlayCount: overlays.length,
    faultCount,
  };
}

/** Wait until `member`'s latest set holds a ref absent from its pre-event history. */
async function awaitNewRef(
  member: JoinedMember,
  pre: PreEvent,
  deadlineAt: number,
  phase: string,
): Promise<number[]> {
  const sets = await pollUntil(
    member,
    () => handlerSets(member.page),
    (h) => newRefs(h.at(-1), pre.union).length > 0,
    deadlineAt,
    (h) =>
      `${phase}: ${member.label}'s latest set holds no handler_ref outside its pre-event ` +
      `history ${JSON.stringify([...pre.union])} by the derived ceiling: ${JSON.stringify(h)}`,
  );
  return newRefs(sets.at(-1), pre.union);
}

/** Wait until `member`'s bus recorded a reconnecting overlay SET (on `slotId`, or on any slot). */
async function awaitOverlaySet(
  member: JoinedMember,
  pre: PreEvent,
  slotId: number | 'any',
  deadlineAt: number,
  phase: string,
): Promise<void> {
  await pollUntil(
    member,
    async () => (await busContinuity(member)).overlays.slice(pre.overlayCount),
    (after) => after.some((o) => o.reconnecting && (slotId === 'any' || o.slotId === slotId)),
    deadlineAt,
    (after) =>
      `${phase} (positive control): ${member.label} never showed the reconnecting overlay on ` +
      `${slotId === 'any' ? 'any slot' : `slot ${slotId}`} — its media was on the killed MH by ` +
      `construction. Overlay transitions after the event: ${JSON.stringify(after)}`,
  );
}

/**
 * Every pair in `pairs` hears its sender again by `deadlineAt` (two-sided tone,
 * `expectHearsSender`), then frames from each sender ADVANCE at each receiver in
 * one window (so a stale pre-event lane can never pass). The time from `fromMs`
 * to each tone verdict is RECORDED (an upper bound on the media-dark window),
 * never gated.
 */
async function expectPairsResume(
  pairs: readonly Pair[],
  {
    cohort,
    deadlineAt,
    fromMs,
    admissionBaseline,
    phase,
  }: {
    readonly cohort: ReturnType<typeof tonesOf>;
    /** Epoch ms: the derived ceiling's end. */
    readonly deadlineAt: number;
    /** Epoch ms: the event the recorded window is measured from. */
    readonly fromMs: number;
    readonly admissionBaseline: InstanceCounters;
    readonly phase: string;
  },
): Promise<void> {
  // On failure, the suite's ONE labelled dump (`failureDiagnostics.ts`: the S1
  // MH-rejection classifier over the post-event baseline, MH/MC diagnostic
  // reads, each page's own drops), appended to the ORIGINAL error.
  const pages = [...new Set(pairs.flatMap(([r, s]) => [r, s]))];
  await withVideoDiagnostics(`${phase}: pairs resume`, admissionBaseline, pages, async () => {
    for (const [r, s] of pairs) {
      await expectHearsSender(r.page, r.tone, s.tone, cohort, {
        timeoutMs: Math.max(1, deadlineAt - Date.now()),
        admissionBaseline,
      });
      test.info().annotations.push({
        type: 'observed-resume-ms',
        description: `${phase} ${r.label}<-${s.label}=${Date.now() - fromMs}`,
      });
    }
    await observeWindow(
      pairs.map(([r, s]) => acceptedProbe(r, s, 'advance')),
      `${phase}: every pair's frames flow after the event`,
    );
  });
}

test.describe('story 4 C3: survive handler loss (MH replacement)', () => {
  test('CRASH: kill -KILL the carrying MH; tone resumes, new handler_ref, overlay observed', async ({
    leg,
  }) => {
    const owned = await leg.upScraped();
    // Baseline only after Prometheus scraped the new MH, before any join.
    const admission = await mhAdmissionRejectionsByInstance();
    const { probe: a, meetingCode, survivors } = await leg.placeOnOwned(0, owned, 'E2E C3 crash');
    const b = await joinMember(await leg.open(1, { blockHandlers: survivors }), meetingCode, {
      expectAllHandlers: false,
    });
    const c = await joinMember(await leg.open(2), meetingCode);
    const members = [a, b, c];
    const cohort = tonesOf(members);
    await expectConnectedOnlyTo(b, owned.address);
    for (const [r, s] of orderedPairs(members))
      await expectHearsAtAllLayers(r, s, cohort, admission);

    const pre = new Map(
      await Promise.all(members.map(async (m) => [m, await preEvent(m, owned.address)] as const)),
    );
    const slotOfBAtA = await slotCarrying(a, b);
    const slotOfBAtC = await slotCarrying(c, b);

    // KILL (debug container; `kill` asserts the in-place restart: same pod_uid,
    // restart_count + 1, serving again within mh.sh's restart budget).
    const killedAt = Date.now();
    const restarted = await leg.pool.kill(owned.slot);
    const restartObservedAt = Date.now();
    expect(restarted.pod_uid, 'CRASH: same pod, never a pod delete').toBe(owned.pod_uid);
    // Same pod IP after an in-place restart: require a scrape of the NEW process.
    await leg.waitScraped(restarted, restartObservedAt);
    const postKillAdmission = await mhAdmissionRejectionsByInstance();
    const survivorBy = killedAt + leg.ceilings.survivorMs;
    const replacementBy = killedAt + leg.ceilings.replacementMs;

    // POSITIVE CONTROL: the overlay, on the slots that were on the killed MH by construction.
    await awaitOverlaySet(a, pre.get(a)!, slotOfBAtA, survivorBy, 'CRASH A[slot of B]');
    await awaitOverlaySet(c, pre.get(c)!, slotOfBAtC, survivorBy, 'CRASH C[slot of B]');
    await awaitOverlaySet(b, pre.get(b)!, 'any', survivorBy, 'CRASH B[any]');

    // REPLACEMENT: every participant's final set holds a ref outside its pre-kill history.
    for (const m of members) await awaitNewRef(m, pre.get(m)!, replacementBy, 'CRASH replacement');

    // RESUMPTION: A<->C may use the survivor; every pair with B needs the replacement.
    await expectPairsResume(
      [
        [a, c],
        [c, a],
      ],
      {
        cohort,
        deadlineAt: survivorBy,
        fromMs: killedAt,
        admissionBaseline: postKillAdmission,
        phase: 'CRASH survivor',
      },
    );
    await expectPairsResume(
      orderedPairs(members).filter(([r, s]) => r === b || s === b),
      {
        cohort,
        deadlineAt: replacementBy,
        fromMs: killedAt,
        admissionBaseline: postKillAdmission,
        phase: 'CRASH replacement',
      },
    );

    // FINAL (read again after resumption): still a new ref; recorded overlays (never gated).
    for (const m of members) {
      expect(
        newRefs((await handlerSets(m.page)).at(-1), pre.get(m)!.union).length,
        `CRASH final: ${m.label}'s final set holds a handler_ref absent from its pre-kill history`,
      ).toBeGreaterThan(0);
      const after = (await busContinuity(m)).overlays.slice(pre.get(m)!.overlayCount);
      test.info().annotations.push({
        type: 'observed-overlay-transitions',
        description: `CRASH ${m.label}=${JSON.stringify(after)}`,
      });
    }
  });

  test('PLANNED: up a target, drain the carrying MH; add-then-remove, no overlay, no fault, tone continues', async ({
    leg,
  }) => {
    const owned = await leg.upScraped();
    const admission = await mhAdmissionRejectionsByInstance();
    const { probe: a, meetingCode, survivors } = await leg.placeOnOwned(0, owned, 'E2E C3 planned');
    const b = await joinMember(await leg.open(1, { blockHandlers: survivors }), meetingCode, {
      expectAllHandlers: false,
    });
    const c = await joinMember(await leg.open(2), meetingCode);
    const members = [a, b, c];
    const cohort = tonesOf(members);
    await expectConnectedOnlyTo(b, owned.address);
    for (const [r, s] of orderedPairs(members))
      await expectHearsAtAllLayers(r, s, cohort, admission);

    // The NEGATIVE window opens here — BEFORE the target exists — and closes
    // only after the post-removal tone check.
    const pre = new Map(
      await Promise.all(members.map(async (m) => [m, await preEvent(m, owned.address)] as const)),
    );

    const target = await leg.upScraped();
    // Blocking drain: returns once the pod is gone (mh.sh's own derived bound,
    // the pod's grace + margin), by which time MC must have cut over.
    await leg.pool.drain(owned.slot);
    const drainedAt = Date.now();

    const grants = new Map<JoinedMember, string>();
    for (const m of members) {
      const p = pre.get(m)!;
      const history = await pollUntil(
        m,
        () => handlerSets(m.page),
        (h) => !(h.at(-1)?.handlers ?? []).some((x) => x.ref === p.ownedRef),
        drainedAt + leg.ceilings.viewMs,
        (h) =>
          `PLANNED: the draining handler (ref ${p.ownedRef}) never left ${m.label}'s set: ` +
          JSON.stringify(h),
      );
      const verdict = makeBeforeBreak(history, p.ownedRef, p.union);
      if (!verdict.ok) throw new Error(`PLANNED make-before-break (${m.label}): ${verdict.detail}`);
      const final = history.at(-1)!;
      const grantUrl = final.handlers.find((h) => h.ref === verdict.newRef)!.url;
      expect(
        grantUrl,
        `PLANNED: ${m.label} was granted the draining handler's address back`,
      ).not.toBe(owned.address);
      grants.set(m, grantUrl);
      expect(
        final.handlers.map((h) => h.ref).sort((x, y) => x - y),
        `PLANNED final set: ${m.label} holds its pre-drain set with the draining ref swapped for the grant`,
      ).toEqual(
        [...p.latestRefs.filter((r) => r !== p.ownedRef), verdict.newRef].sort((x, y) => x - y),
      );
    }
    expect(
      new Set(grants.values()).size,
      'PLANNED: every participant was given the same grant',
    ).toBe(1);
    test.info().annotations.push({
      type: 'observed-grant',
      description: `target slot ${target.slot} (${target.address}); granted ${[...grants.values()][0]}`,
    });

    await expectPairsResume(orderedPairs(members), {
      cohort,
      deadlineAt: drainedAt + leg.ceilings.survivorMs,
      fromMs: drainedAt,
      admissionBaseline: admission,
      phase: 'PLANNED',
    });

    // The NEGATIVE closes: nothing was ever shown reconnecting and no transport faulted.
    for (const m of members) {
      const p = pre.get(m)!;
      const now = await busContinuity(m);
      expect(
        now.overlays.slice(p.overlayCount),
        `PLANNED: ${m.label} showed a reconnecting overlay during a planned transfer`,
      ).toEqual([]);
      expect(now.faultCount - p.faultCount, `PLANNED: ${m.label} raised a transport fault`).toBe(0);
    }
  });

  test('STRANDED: A and B blocked from the survivor; after the kill they hear each other on the replacement', async ({
    leg,
  }) => {
    const owned = await leg.upScraped();
    const admission = await mhAdmissionRejectionsByInstance();
    // C (index 2, unblocked) probes the placement; A and B then join stranded.
    const {
      probe: c,
      meetingCode,
      survivors,
    } = await leg.placeOnOwned(2, owned, 'E2E C3 stranded');
    const a = await joinMember(await leg.open(0, { blockHandlers: survivors }), meetingCode, {
      expectAllHandlers: false,
    });
    const b = await joinMember(await leg.open(1, { blockHandlers: survivors }), meetingCode, {
      expectAllHandlers: false,
    });
    const members = [a, b, c];
    const cohort = tonesOf(members);
    for (const m of [a, b]) await expectConnectedOnlyTo(m, owned.address);
    for (const [r, s] of orderedPairs(members))
      await expectHearsAtAllLayers(r, s, cohort, admission);

    const pre = new Map(
      await Promise.all(members.map(async (m) => [m, await preEvent(m, owned.address)] as const)),
    );

    const killedAt = Date.now();
    const restarted = await leg.pool.kill(owned.slot);
    const restartObservedAt = Date.now();
    expect(restarted.pod_uid, 'STRANDED: same pod, never a pod delete').toBe(owned.pod_uid);
    await leg.waitScraped(restarted, restartObservedAt);
    const postKillAdmission = await mhAdmissionRejectionsByInstance();
    const replacementBy = killedAt + leg.ceilings.replacementMs;

    for (const m of [a, b]) {
      await awaitOverlaySet(
        m,
        pre.get(m)!,
        'any',
        killedAt + leg.ceilings.survivorMs,
        `STRANDED ${m.label}`,
      );
      await awaitNewRef(m, pre.get(m)!, replacementBy, 'STRANDED replacement');
    }
    await expectPairsResume(
      [
        [a, b],
        [b, a],
      ],
      {
        cohort,
        deadlineAt: replacementBy,
        fromMs: killedAt,
        admissionBaseline: postKillAdmission,
        phase: 'STRANDED',
      },
    );
    for (const m of [a, b]) {
      expect(
        newRefs((await handlerSets(m.page)).at(-1), pre.get(m)!.union).length,
        `STRANDED final: ${m.label}'s final set holds a handler_ref absent from its pre-kill history`,
      ).toBeGreaterThan(0);
    }
  });
});
