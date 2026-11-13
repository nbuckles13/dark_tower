// Node-tier unit tests for `e2e/mhLegRules.ts` (the pure rules of the story 4
// C3 MH-replacement legs) and the C3 readers in `e2e/configEnv.ts`. Every
// twinned constant is pinned against its ONE Rust home with a positive control
// (the extraction must find something), so a drift on either side fails here.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import type { BusHandlerSet } from '../src/lib/e2eBus.js';
import { instanceForPodIp } from '../e2e/instanceCounters.js';
import {
  overlayFilesPatchingResilienceKeys,
  parseGcHealthTickSeconds,
  readResilienceTimings,
  readRustU64Const,
  resilienceTimings,
} from '../e2e/configEnv.js';
import { runLeakVerdict, servingSlotCount, type PoolSnapshot } from '../e2e/mhPool.js';
import {
  FOREIGN_MC_MEETINGS,
  GC_SETTLE_HEALTHY_SERIES,
  GC_SETTLE_RECOMPUTE_OPERATION,
  GC_SETTLE_RECOMPUTE_SERIES,
  GC_SETTLE_RECOMPUTE_SUCCESS,
  GC_SETTLE_STATES,
  METRICS_SCRAPE_CEILING_MS,
  OWNED_MH_NOT_PLACED,
  PLACEMENT_MISS_CEILING,
  SERVICE_JOB_SCRAPE_SETTLE_MS,
  SETTLE_RECOMPUTES,
  makeBeforeBreak,
  newRefs,
  placementAttempts,
  placementHit,
  rebaseline,
  refOf,
  refUnion,
  replicaSettled,
  resilienceCeilings,
} from '../e2e/mhLegRules.js';

const REPO = fileURLToPath(new URL('../../../', import.meta.url));
const read = (rel: string) => readFileSync(join(REPO, rel), 'utf8');

const set = (version: number, refs: number[]): BusHandlerSet => ({
  version: String(version),
  handlers: refs.map((ref) => ({ ref, url: `https://mh.test:${4000 + ref}` })),
});

describe('placementAttempts (twin of owned_placement::placement_attempts)', () => {
  test('k is the SMALLEST count whose miss probability is at most the ceiling', () => {
    for (const [m, s] of [
      [3, 2],
      [4, 2],
      [6, 2],
      [5, 3],
    ] as const) {
      const k = placementAttempts(m, s, PLACEMENT_MISS_CEILING);
      const p = (n: number) => ((m - s) / m) ** n;
      expect(p(k), `m=${m} s=${s}`).toBeLessThanOrEqual(PLACEMENT_MISS_CEILING);
      expect(p(k - 1), `m=${m} s=${s} minimal`).toBeGreaterThan(PLACEMENT_MISS_CEILING);
    }
  });

  test('one attempt when every meeting takes every MH', () => {
    expect(placementAttempts(2, 2, PLACEMENT_MISS_CEILING)).toBe(1);
  });

  test('degenerate inputs fail loudly with the token', () => {
    expect(() => placementAttempts(0, 2, PLACEMENT_MISS_CEILING)).toThrow(OWNED_MH_NOT_PLACED);
    expect(() => placementAttempts(3, 0, PLACEMENT_MISS_CEILING)).toThrow(OWNED_MH_NOT_PLACED);
    expect(() => placementAttempts(3, 2, 1)).toThrow(OWNED_MH_NOT_PLACED);
  });

  test('placementHit: owned present AND exact member count', () => {
    expect(placementHit(['u0', 'own'], 'own', 2)).toBe(true);
    expect(placementHit(['u0', 'u1'], 'own', 2)).toBe(false);
    expect(placementHit(['own'], 'own', 2)).toBe(false);
  });
});

describe('removed-MH settle kernels (twins of gc_fleet.rs)', () => {
  test('rebaseline: reset -> 0, unseen adopts the first sample, else keeps', () => {
    expect(rebaseline(10, 3)).toBe(0);
    expect(rebaseline(undefined, 7)).toBe(7);
    expect(rebaseline(undefined, undefined)).toBeUndefined();
    expect(rebaseline(10, 12)).toBe(10);
  });

  test('replicaSettled needs a WHOLE recompute and selectable <= live', () => {
    const ok = { selectable: 2, recomputeBaseline: 5, recomputeNow: 5 + SETTLE_RECOMPUTES };
    expect(replicaSettled(ok, 2)).toBe(true);
    expect(replicaSettled({ ...ok, recomputeNow: 6 }, 2), 'one recompute is not whole').toBe(false);
    expect(replicaSettled({ ...ok, selectable: 3 }, 2), 'a removed row still selectable').toBe(
      false,
    );
    expect(replicaSettled({ ...ok, selectable: undefined }, 2), 'absent is not settled').toBe(
      false,
    );
    expect(
      replicaSettled({ selectable: 0, recomputeBaseline: 0, recomputeNow: 0 }, 2),
      'a frozen zero-init counter observed nothing',
    ).toBe(false);
  });
});

describe('resilienceCeilings', () => {
  const t = { heartbeatDeadlineMs: 6000, mhIdleTimeoutMs: 9000, connectSettleMs: 1500 };
  test('composed from the later detection, the settle, the retry budget and the hearing liveness', () => {
    expect(resilienceCeilings(t, 40_000, 30_000)).toEqual({
      viewMs: 10_500,
      survivorMs: 40_500,
      replacementMs: 80_500,
    });
  });
  test('a non-positive input is a PRECONDITION, never a zero ceiling', () => {
    expect(() => resilienceCeilings(t, 0, 30_000)).toThrow('PRECONDITION');
    expect(() => resilienceCeilings({ ...t, connectSettleMs: NaN }, 1, 1)).toThrow('PRECONDITION');
  });
});

describe('handler-set history rules', () => {
  test('refUnion / newRefs / refOf', () => {
    const history = [set(1, [1, 2]), set(2, [2]), set(3, [2, 3])];
    const pre = refUnion(history.slice(0, 2));
    expect([...pre].sort()).toEqual([1, 2]);
    expect(newRefs(history[2], pre)).toEqual([3]);
    expect(newRefs(undefined, pre)).toEqual([]);
    expect(refOf(history[0], 'https://mh.test:4001', 'x')).toBe(1);
    expect(() => refOf(history[1], 'https://mh.test:4001', 'x')).toThrow('no handler');
  });

  test('makeBeforeBreak accepts add-then-remove', () => {
    const history = [set(1, [1, 2]), set(2, [1, 2, 3]), set(3, [2, 3])];
    expect(makeBeforeBreak(history, 1, new Set([1, 2]))).toEqual({
      ok: true,
      madeAt: 1,
      brokeAt: 2,
      newRef: 3,
    });
  });

  test('makeBeforeBreak rejects the crash shape (remove, then add)', () => {
    const history = [set(1, [1, 2]), set(2, [2]), set(3, [2, 3])];
    const v = makeBeforeBreak(history, 1, new Set([1, 2]));
    expect(v.ok).toBe(false);
  });

  test('makeBeforeBreak rejects a removal that never came, and a grant that left again', () => {
    expect(makeBeforeBreak([set(1, [1, 2]), set(2, [1, 2, 3])], 1, new Set([1, 2])).ok).toBe(false);
    expect(
      makeBeforeBreak([set(1, [1, 2]), set(2, [1, 2, 3]), set(3, [2])], 1, new Set([1, 2])).ok,
    ).toBe(false);
  });
});

describe('instanceForPodIp (twin of metrics::instance_for_pod_ip)', () => {
  test('matches on the ip AND the colon only; an ambiguous pin throws', () => {
    const map = new Map([
      ['10.0.0.10:8083', 3],
      ['10.0.0.1:8083', 7],
    ]);
    expect(instanceForPodIp(map, '10.0.0.1')).toBe(7);
    expect(instanceForPodIp(map, '10.0.0.10')).toBe(3);
    expect(instanceForPodIp(map, '10.0.0.2')).toBeUndefined();
    expect(() =>
      instanceForPodIp(
        new Map([
          ['10.0.0.1:8083', 1],
          ['10.0.0.1:9090', 2],
        ]),
        '10.0.0.1',
      ),
    ).toThrow('more than one');
  });
});

describe('configEnv C3 readers', () => {
  const f = (text: string, source: string) => ({ text, source });
  const tick = f('pub const DEFAULT_CHECK_INTERVAL_SECONDS: u64 = 5;\n', 'tick.rs');
  const inputs = {
    mc: f('MC_MH_HEARTBEAT_TIMEOUT_MS=6000\nMC_MEDIA_CONNECT_SETTLE_MS=1500\n', 'mc.env'),
    mh: f('MH_MAX_IDLE_TIMEOUT_MS="6000"\n', 'mh.env'),
    gc: f(
      '# GC_MH_INITIAL_HANDLERS_PER_MEETING=9\nGC_MH_INITIAL_HANDLERS_PER_MEETING=2\n',
      'gc.env',
    ),
    gcTick: tick,
  };

  test('reads every input (a comment is not a key)', () => {
    expect(readResilienceTimings(inputs)).toEqual({
      heartbeatDeadlineMs: 6000,
      connectSettleMs: 1500,
      mhIdleTimeoutMs: 6000,
      initialHandlers: 2,
      gcHealthTickMs: 5000,
    });
  });

  test('a missing key FAILS CLOSED, naming file and key', () => {
    expect(() => readResilienceTimings({ ...inputs, mh: f('', 'mh.env') })).toThrow(
      /mh\.env declares no MH_MAX_IDLE_TIMEOUT_MS/,
    );
    expect(() => parseGcHealthTickSeconds('const X: u64 = 5;', 'tick.rs')).toThrow('tick.rs');
  });

  test('readRustU64Const: pub or private, anchored at line start', () => {
    expect(readRustU64Const('pub const A: u64 = 7;\n', 'A', 'x.rs')).toBe(7);
    expect(readRustU64Const('  const A: u64 = 3;', 'A', 'x.rs')).toBe(3);
    expect(() => readRustU64Const('// pub const A: u64 = 7;', 'A', 'x.rs')).toThrow('x.rs');
    expect(() => readRustU64Const('pub const AB: u64 = 7;', 'A', 'x.rs')).toThrow('no longer');
  });

  test('an overlay file naming a resilience key is reported (fail closed)', () => {
    expect(
      overlayFilesPatchingResilienceKeys([
        { path: 'a.yaml', text: 'MC_MEETING_JOIN_WINDOW_SECONDS: "5"' },
        { path: 'b.yaml', text: '  MH_MAX_IDLE_TIMEOUT_MS: "9000"' },
      ]),
    ).toEqual(['b.yaml']);
    expect(
      overlayFilesPatchingResilienceKeys([{ path: 'c.yaml', text: 'XMH_MAX_IDLE_TIMEOUT_MSX' }]),
    ).toEqual([]);
  });

  test('the REPO files parse (positive control: the keys exist where Kind reads them)', () => {
    const t = resilienceTimings();
    expect(t.heartbeatDeadlineMs).toBeGreaterThan(0);
    expect(t.mhIdleTimeoutMs).toBeGreaterThan(0);
    expect(t.initialHandlers).toBeGreaterThanOrEqual(2);
  });
});

describe('pool snapshot predicates (mhPool.ts)', () => {
  const slot = (n: number, state: string, owner: string | null, past = false) => ({
    slot: n,
    host_port: 4000 + n,
    address: `https://mh.test:${4000 + n}`,
    pod_ip: state === 'free' ? null : `10.0.0.${n}`,
    pod_uid: state === 'free' ? null : `uid-${n}`,
    state,
    terminating_past_grace: past,
    deployment: null,
    pod: state === 'free' ? null : `mh-${n}`,
    revision: null,
    stale: false,
    owner,
    restart_count: state === 'free' ? null : 0,
  });
  const snap = (slots: ReturnType<typeof slot>[]): PoolSnapshot =>
    ({ templateRevision: 'r', baseline: 2, slots }) as PoolSnapshot;

  test('servingSlotCount counts only serving', () => {
    expect(
      servingSlotCount(
        snap([slot(0, 'serving', 'baseline'), slot(1, 'starting', 'run'), slot(2, 'free', null)]),
      ),
    ).toBe(1);
  });

  test('runLeakVerdict: run-owned or past-grace pods leak; baseline and free do not', () => {
    expect(
      runLeakVerdict(snap([slot(0, 'serving', 'baseline'), slot(1, 'free', null)]), 'run'),
    ).toBeNull();
    expect(runLeakVerdict(snap([slot(2, 'serving', 'run')]), 'run')).toMatch(
      /mh-run-leak.*slot\(s\) 2/,
    );
    expect(runLeakVerdict(snap([slot(3, 'terminating', 'other', true)]), 'run')).toMatch(
      /slot\(s\) 3/,
    );
  });
});

describe('drift pins (one Rust home each)', () => {
  const placement = read('crates/env-tests/src/fixtures/owned_placement.rs');
  const gcFleet = read('crates/env-tests/src/fixtures/gc_fleet.rs');
  const metrics = read('crates/env-tests/src/fixtures/metrics.rs');
  const eventual = read('crates/env-tests/src/eventual.rs');
  const strConst = (src: string, name: string) =>
    new RegExp(`pub const ${name}: &str = "([^"]*)";`).exec(src)?.[1];

  test('owned_placement.rs: token and miss ceiling', () => {
    expect(strConst(placement, 'OWNED_MH_NOT_PLACED')).toBe(OWNED_MH_NOT_PLACED);
    const ceiling = /pub const PLACEMENT_MISS_CEILING: f64 = ([0-9.e-]+);/.exec(placement)?.[1];
    expect(ceiling, 'positive control').toBeTruthy();
    expect(Number(ceiling)).toBe(PLACEMENT_MISS_CEILING);
  });

  test('gc_fleet.rs: SETTLE_* tokens and SETTLE_RECOMPUTES', () => {
    expect(strConst(gcFleet, 'SETTLE_HEALTHY_SERIES')).toBe(GC_SETTLE_HEALTHY_SERIES);
    expect(strConst(gcFleet, 'SETTLE_STATES')).toBe(GC_SETTLE_STATES);
    expect(strConst(gcFleet, 'SETTLE_RECOMPUTE_SERIES')).toBe(GC_SETTLE_RECOMPUTE_SERIES);
    expect(strConst(gcFleet, 'SETTLE_RECOMPUTE_OPERATION')).toBe(GC_SETTLE_RECOMPUTE_OPERATION);
    // Rust escapes the inner quotes: status=\"success\".
    const success = /pub const SETTLE_RECOMPUTE_SUCCESS: &str = "((?:[^"\\]|\\.)*)";/.exec(
      gcFleet,
    )?.[1];
    expect(success?.replace(/\\"/g, '"')).toBe(GC_SETTLE_RECOMPUTE_SUCCESS);
    const recomputes = /pub const SETTLE_RECOMPUTES: f64 = ([0-9.]+);/.exec(gcFleet)?.[1];
    expect(recomputes, 'positive control').toBeTruthy();
    expect(Number(recomputes)).toBe(SETTLE_RECOMPUTES);
  });

  test('metrics.rs FOREIGN_MC_MEETINGS; eventual.rs MetricsScrape ceiling', () => {
    expect(strConst(metrics, 'FOREIGN_MC_MEETINGS')).toBe(FOREIGN_MC_MEETINGS);
    const secs = /ConsistencyCategory::MetricsScrape => Duration::from_secs\((\d+)\)/.exec(
      eventual,
    )?.[1];
    expect(secs, 'positive control').toBeTruthy();
    expect(METRICS_SCRAPE_CEILING_MS).toBe(Number(secs) * 1000);
  });

  test('metrics.rs SERVICE_JOB_SCRAPE_SETTLE = interval + margin', () => {
    const n = (name: string) =>
      Number(new RegExp(`pub const ${name}: u64 = (\\d+);`).exec(metrics)?.[1]);
    expect(n('SERVICE_JOB_SCRAPE_INTERVAL_SECS'), 'positive control').toBeGreaterThan(0);
    expect(SERVICE_JOB_SCRAPE_SETTLE_MS).toBe(
      (n('SERVICE_JOB_SCRAPE_INTERVAL_SECS') + n('SCRAPE_SETTLE_MARGIN_SECS')) * 1000,
    );
  });

  test('GC health tick: the TS reader parses the source line the Rust twin reads', () => {
    const src = read('crates/gc-service/src/tasks/generic_health_checker.rs');
    expect(parseGcHealthTickSeconds(src, 'generic_health_checker.rs')).toBeGreaterThan(0);
    expect(gcFleet).toContain('crates/gc-service/src/tasks/generic_health_checker.rs');
  });
});
