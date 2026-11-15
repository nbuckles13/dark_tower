// File: packages/web-app/e2e/mhLegRules.ts
//
// Story 4 C3: the PURE rules of the browser MH-replacement legs (`mhLeg.ts`,
// `mh-replacement.spec.ts`). No `./env`, no Playwright, so the node tier
// (`tests/mhLeg.test.ts`) exercises every kernel hermetically and pins every
// twinned constant against its ONE Rust home (`owned_placement.rs`,
// `gc_fleet.rs`, `eventual.rs`, `metrics.rs`), the same split as
// `./instanceCounters` and `./cohort`.

import type { BusHandlerSet } from '../src/lib/e2eBus.js';

// ---------------------------------------------------------------------------
// Twinned constants (one home each in Rust; pinned by tests/mhLeg.test.ts)
// ---------------------------------------------------------------------------

/** `owned_placement.rs` OWNED_MH_NOT_PLACED. */
export const OWNED_MH_NOT_PLACED = 'owned-mh-not-placed';
/** `owned_placement.rs` PLACEMENT_MISS_CEILING: a flake budget, not a timing value. */
export const PLACEMENT_MISS_CEILING = 1e-6;
/** `gc_fleet.rs` SETTLE_RECOMPUTES: a WHOLE recompute after the wait started. */
export const SETTLE_RECOMPUTES = 2;
/** `eventual.rs` ConsistencyCategory::MetricsScrape: the scrape-visibility ceiling. */
export const METRICS_SCRAPE_CEILING_MS = 30_000;
/** `eventual.rs` ConsistencyCategory::LogAggregation: the log-ingest (Loki) ceiling. */
export const LOG_AGGREGATION_CEILING_MS = 45_000;
/** Reason tokens of this file's own waits (the Rust legs use the same words). */
export const REMOVED_MH_STILL_SELECTABLE = 'removed-mh-still-selectable';

// ---------------------------------------------------------------------------
// PURE kernels (node-tested)
// ---------------------------------------------------------------------------

/**
 * Twin of `owned_placement::placement_attempts`: the attempt count k over
 * `selectable` MHs with `perMeeting` picks each, so the miss probability
 * `((m - s) / m)^k` is at most `missCeiling`. A fresh run-owned MH carries no
 * load (GC weight 1 - load is maximal), so the uniform bound is an upper bound.
 */
export function placementAttempts(
  selectable: number,
  perMeeting: number,
  missCeiling: number,
): number {
  if (!Number.isInteger(selectable) || selectable <= 0) {
    throw new Error(
      `${OWNED_MH_NOT_PLACED}: no selectable MH: the owned MH is not even registered`,
    );
  }
  if (!Number.isInteger(perMeeting) || perMeeting <= 0) {
    throw new Error(
      `${OWNED_MH_NOT_PLACED}: GC_MH_INITIAL_HANDLERS_PER_MEETING=0 places no handler`,
    );
  }
  if (!(missCeiling > 0 && missCeiling < 1)) {
    throw new Error(`${OWNED_MH_NOT_PLACED}: miss ceiling ${missCeiling} is not in (0, 1)`);
  }
  if (perMeeting >= selectable) return 1;
  const miss = (selectable - perMeeting) / selectable;
  return Math.max(1, Math.ceil(Math.log(missCeiling) / Math.log(miss)));
}

/** Twin of `owned_placement::placement_hit`: the set names `owned`, at exactly `members` handlers. */
export function placementHit(urls: readonly string[], owned: string, members: number): boolean {
  return urls.includes(owned) && urls.length === members;
}

/** Twin of `gc_fleet::rebaseline`: a counter below its baseline restarted (baseline 0). */
export function rebaseline(base: number | undefined, now: number | undefined): number | undefined {
  if (base !== undefined && now !== undefined && now < base) return 0;
  if (base === undefined) return now;
  return base;
}

/** One GC replica's view at one poll (twin of `gc_fleet::ReplicaSettle`). */
export interface ReplicaSettle {
  readonly selectable: number | undefined;
  readonly recomputeBaseline: number | undefined;
  readonly recomputeNow: number | undefined;
}

/** Twin of `gc_fleet::replica_settled`: a whole recompute ran AND selectable rows <= live slots. */
export function replicaSettled(r: ReplicaSettle, live: number): boolean {
  const recomputed =
    r.recomputeNow !== undefined &&
    r.recomputeBaseline !== undefined &&
    r.recomputeNow > 0 &&
    r.recomputeNow >= r.recomputeBaseline + SETTLE_RECOMPUTES;
  return recomputed && r.selectable !== undefined && r.selectable <= live;
}

/** The failure-only ceilings of one leg, in ms. */
export interface ResilienceCeilings {
  /**
   * Media between participants that can use a SURVIVOR (or, planned, the
   * grant) resumes: the loss is noticed by MC at N and by the client at the MH
   * idle timeout (the later of the two), MC re-places after its connect
   * settle window, and the receiver's lane needs the hearing liveness to fill.
   */
  readonly survivorMs: number;
  /** As survivor, plus MC's published replacement retry budget (GC grant + register + set push). */
  readonly replacementMs: number;
  /** The set change reaching every client after a planned cutover: max(N, idle) + settle. */
  readonly viewMs: number;
}

/** PURE: the ceilings from the deployed timings, the retry budget and the hearing liveness. */
export function resilienceCeilings(
  t: {
    readonly heartbeatDeadlineMs: number;
    readonly mhIdleTimeoutMs: number;
    readonly connectSettleMs: number;
  },
  replacementRetryBudgetMs: number,
  hearMs: number,
): ResilienceCeilings {
  for (const [name, v] of Object.entries({ ...t, replacementRetryBudgetMs, hearMs })) {
    if (!(Number.isFinite(v) && v > 0)) {
      throw new Error(`PRECONDITION: ceiling input ${name}=${v} is not positive`);
    }
  }
  const viewMs = Math.max(t.heartbeatDeadlineMs, t.mhIdleTimeoutMs) + t.connectSettleMs;
  return {
    viewMs,
    survivorMs: viewMs + hearMs,
    replacementMs: viewMs + replacementRetryBudgetMs + hearMs,
  };
}

/** PURE: every ref a member's bus EVER held (the union over its whole history). */
export function refUnion(sets: readonly BusHandlerSet[]): ReadonlySet<number> {
  return new Set(sets.flatMap((s) => s.handlers.map((h) => h.ref)));
}

/** PURE: the refs of `set` absent from `before`. */
export function newRefs(set: BusHandlerSet | undefined, before: ReadonlySet<number>): number[] {
  return (set?.handlers ?? []).map((h) => h.ref).filter((r) => !before.has(r));
}

/** The ref naming `url` in `set`, or a loud failure. */
export function refOf(set: BusHandlerSet | undefined, url: string, who: string): number {
  const hit = set?.handlers.find((h) => h.url === url);
  if (hit === undefined) {
    throw new Error(`${who}: no handler ${url} in its set ${JSON.stringify(set ?? null)}`);
  }
  return hit.ref;
}

/** The verdict of {@link makeBeforeBreak}: the two indices, or the failure text. */
export type MakeBeforeBreak =
  | {
      readonly ok: true;
      readonly madeAt: number;
      readonly brokeAt: number;
      readonly newRef: number;
    }
  | { readonly ok: false; readonly detail: string };

/**
 * PURE: the planned transfer's order in ONE member's wire-ordered set history:
 * a set holding BOTH `oldRef` and a ref outside `preRefs` (the grant) comes
 * strictly BEFORE the first later set without `oldRef`, and the final set still
 * holds that grant. A crash-shaped history (old removed before any grant) fails.
 */
export function makeBeforeBreak(
  history: readonly BusHandlerSet[],
  oldRef: number,
  preRefs: ReadonlySet<number>,
): MakeBeforeBreak {
  const refs = history.map((s) => s.handlers.map((h) => h.ref));
  const madeAt = refs.findIndex((r) => r.includes(oldRef) && r.some((x) => !preRefs.has(x)));
  if (madeAt < 0) {
    return {
      ok: false,
      detail: `no set held the draining ref ${oldRef} AND a new ref: ${JSON.stringify(refs)}`,
    };
  }
  const newRef = refs[madeAt]!.find((x) => !preRefs.has(x))!;
  const firstWithout = refs.findIndex((r) => !r.includes(oldRef));
  if (firstWithout >= 0 && firstWithout < madeAt) {
    return {
      ok: false,
      detail: `ref ${oldRef} left (set #${firstWithout}) BEFORE the grant was added (set #${madeAt}): break-before-make ${JSON.stringify(refs)}`,
    };
  }
  const brokeAt = refs.findIndex((r, i) => i > madeAt && !r.includes(oldRef));
  if (brokeAt < 0) {
    return {
      ok: false,
      detail: `ref ${oldRef} never left after the grant: ${JSON.stringify(refs)}`,
    };
  }
  if (!refs.at(-1)!.includes(newRef)) {
    return {
      ok: false,
      detail: `the grant ${newRef} is not in the final set: ${JSON.stringify(refs)}`,
    };
  }
  return { ok: true, madeAt, brokeAt, newRef };
}

/**
 * The removed-MH settle tokens: twins of `gc_fleet.rs` `SETTLE_*` (which
 * deploy.sh shares too). `mcMetrics.ts` composes its PromQL from these.
 */
export const GC_SETTLE_HEALTHY_SERIES = 'gc_mh_available';
export const GC_SETTLE_STATES = 'pending|healthy|degraded';
export const GC_SETTLE_RECOMPUTE_SERIES = 'gc_db_queries_total';
export const GC_SETTLE_RECOMPUTE_OPERATION = 'count_available_mh';
export const GC_SETTLE_RECOMPUTE_SUCCESS = 'status="success"';
