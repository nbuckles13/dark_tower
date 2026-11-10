// Node-tier unit tests for `e2e/mhPool.ts`, the Playwright MH pool driver
// (story 4 T-1). A stub mh.sh (a Node script in a fresh temp dir) records its
// argv and replays canned stdout/stderr/exit codes, so every path runs
// hermetically: the path precondition, the strict MH_RESULT parse (the same
// named cases as Rust's `parse_mh_sh_result`), the BUSY retry bound, owner
// teardown and leak check, the injector's outcome assertions, and the
// FreezeGuard dispose backstop. Drift pins tie every re-encoded value to its
// one home (mh.sh, mh_pool.rs, kube.rs, eventual.rs), each with a positive
// control. No real token or credential appears anywhere here.

import { chmodSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import {
  BASELINE_OWNER,
  BUSY_RETRY_BOUND_MS,
  BUSY_RETRY_INTERVAL_MS,
  FOREIGN_MH_ACTIVITY,
  FREEZE_BUDGET_EXCEEDED,
  MH_RUN_LEAK,
  MH_SCRAPE_UP_TIMEOUT,
  MH_TOKENS,
  RESTART_COUNT_CHANGED,
  RESTART_NOT_OBSERVED,
  SCHEMA_VERSION,
  SLOT_KEYS,
  baselineVerdict,
  clusterFrom,
  createMhPool,
  mhShPathFrom,
  parseList,
  parseMhShResult,
  type PoolSnapshot,
  type SlotEntry,
} from '../e2e/mhPool.js';

const REPO = fileURLToPath(new URL('../../../', import.meta.url));
const read = (rel: string) => readFileSync(join(REPO, rel), 'utf8');
const OWNER = 'e2e-0123456789abcdef';

interface Canned {
  stdout: unknown;
  result: string;
  code: number;
}

const STUB = `#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const dir = path.dirname(process.argv[1]);
fs.appendFileSync(path.join(dir, 'calls.log'), JSON.stringify(process.argv.slice(2)) + '\\n');
fs.appendFileSync(path.join(dir, 'cluster.log'), String(process.env.DT_CLUSTER_NAME) + '\\n');
const responses = JSON.parse(fs.readFileSync(path.join(dir, 'responses.json'), 'utf8'));
const nFile = path.join(dir, 'n');
const n = fs.existsSync(nFile) ? Number(fs.readFileSync(nFile, 'utf8')) : 0;
fs.writeFileSync(nFile, String(n + 1));
const r = responses[Math.min(n, responses.length - 1)];
process.stdout.write(typeof r.stdout === 'string' ? r.stdout : JSON.stringify(r.stdout));
process.stderr.write('human text\\n' + r.result + '\\n');
process.exit(r.code);
`;

function stub(responses: Canned[]): {
  mhSh: string;
  calls: () => string[][];
  clusters: () => string[];
} {
  const dir = mkdtempSync(join(tmpdir(), 'mhpool-'));
  const mhSh = join(dir, 'mh.sh');
  writeFileSync(mhSh, STUB);
  chmodSync(mhSh, 0o755);
  writeFileSync(join(dir, 'responses.json'), JSON.stringify(responses));
  const log = join(dir, 'calls.log');
  const clusterLog = join(dir, 'cluster.log');
  return {
    mhSh,
    clusters: () =>
      existsSync(clusterLog) ? readFileSync(clusterLog, 'utf8').trim().split('\n') : [],
    calls: () =>
      existsSync(log)
        ? readFileSync(log, 'utf8')
            .trim()
            .split('\n')
            .map((l) => JSON.parse(l) as string[])
        : [],
  };
}

function entry(
  slot: number,
  state: string,
  owner: string | null,
  over: Partial<SlotEntry> = {},
): SlotEntry {
  const occupied = owner !== null;
  return {
    slot,
    host_port: 4440 + slot,
    address: `https://127.0.0.1:${4440 + slot}`,
    pod_ip: occupied ? `10.244.0.${slot + 10}` : null,
    pod_uid: occupied ? `uid-${slot}` : null,
    state: state as SlotEntry['state'],
    terminating_past_grace: false,
    deployment: occupied ? `mh-s${slot}` : null,
    pod: occupied ? `mh-s${slot}-x` : null,
    revision: occupied ? 'r1' : null,
    stale: false,
    owner,
    restart_count: occupied ? 0 : null,
    ...over,
  };
}

const listDoc = (slots: SlotEntry[], baseline = 1) => ({
  ok: true,
  schema_version: 1,
  template_revision: 'r1',
  baseline,
  slots,
});
const okList = (slots: SlotEntry[], baseline = 1): Canned => ({
  stdout: listDoc(slots, baseline),
  result: 'MH_RESULT STATUS=OK REASON=ok',
  code: 0,
});
const busy: Canned = {
  stdout: { ok: false },
  result: 'MH_RESULT STATUS=FAIL REASON=mh-pool-busy DETAIL=holder=e2e-ffff@host:1',
  code: 5,
};

/** A fake clock: sleep advances it (no real waits anywhere). */
function clock() {
  let t = 0;
  const slept: number[] = [];
  return {
    now: () => t,
    sleep: async (ms: number) => {
      slept.push(ms);
      t += ms;
    },
    advance: (ms: number) => {
      t += ms;
    },
    slept,
  };
}

function pool(s: { mhSh: string }, c = clock()) {
  return createMhPool({
    mhSh: s.mhSh,
    owner: OWNER,
    cluster: 'devloop-x',
    sleep: c.sleep,
    now: c.now,
  });
}

describe('mhShPathFrom', () => {
  test('unset, blank, relative and missing are loud PRECONDITIONs', () => {
    expect(() => mhShPathFrom(undefined)).toThrow(/PRECONDITION \(mh-sh-unset\).*unset/);
    expect(() => mhShPathFrom('  ')).toThrow(/unset or empty/);
    expect(() => mhShPathFrom('infra/kind/scripts/mh.sh')).toThrow(/not absolute/);
    expect(() => mhShPathFrom('/nonexistent/mh.sh')).toThrow(/not an existing file/);
    const real = join(REPO, 'infra/kind/scripts/mh.sh');
    expect(mhShPathFrom(real)).toBe(real);
  });

  test('a verb with DT_MH_SH unset fails before running anything', async () => {
    const saved = process.env.DT_MH_SH;
    delete process.env.DT_MH_SH;
    try {
      await expect(createMhPool({ owner: OWNER }).list()).rejects.toThrow(
        /PRECONDITION \(mh-sh-unset\)/,
      );
    } finally {
      if (saved !== undefined) process.env.DT_MH_SH = saved;
    }
  });
});

describe('parseMhShResult (same named cases as Rust)', () => {
  const ok = (stderr: string, stdout: string, code: number | null) =>
    parseMhShResult(stderr, stdout, code);
  test('OK and FAIL with DETAIL to end of line', () => {
    const r = ok('x\nMH_RESULT STATUS=OK REASON=ok SLOT=3 DETAIL=a b\n', '{"ok":true}', 0);
    expect([r.status, r.reason, r.slot, r.detail]).toEqual(['OK', 'ok', 3, 'a b']);
    const f = ok('MH_RESULT STATUS=FAIL REASON=mh-no-free-slot\n', '{"ok":false}', 3);
    expect([f.status, f.reason, f.slot, f.detail]).toEqual(['FAIL', 'mh-no-free-slot', null, null]);
  });
  test.each([
    ['', '{}', 0, /^one-line: mh.sh printed nothing/],
    ['oops', '{}', 0, /^one-line: mh.sh's last stderr line/],
    [
      'MH_RESULT STATUS=OK REASON=ok\nMH_RESULT STATUS=OK REASON=ok',
      '{"ok":true}',
      0,
      /^one-line: mh.sh printed 2/,
    ],
    [
      'MH_RESULT STATUS=MAYBE REASON=ok',
      '{"ok":true}',
      0,
      /^fields: MH_RESULT has a malformed field/,
    ],
    ['MH_RESULT STATUS=OK', '{"ok":true}', 0, /^fields: MH_RESULT has no REASON/],
    ['MH_RESULT STATUS=OK REASON=ok SLOT=x', '{"ok":true}', 0, /^fields: MH_RESULT SLOT/],
    ['MH_RESULT STATUS=FAIL REASON=made-up', '{"ok":false}', 1, /^known-token: REASON=made-up/],
    [
      'MH_RESULT STATUS=FAIL REASON=drain-timeout',
      '{"ok":false}',
      0,
      /^exit-group: REASON=drain-timeout belongs to exit 1, mh.sh exited 0/,
    ],
    ['MH_RESULT STATUS=OK REASON=ok', '{"ok":true}', null, /^exit-group: mh.sh was killed/],
    ['MH_RESULT STATUS=FAIL REASON=ok', '{"ok":false}', 0, /^ok-reason:/],
    [
      'MH_RESULT STATUS=OK REASON=ok',
      'not json',
      0,
      /^json-ok-agrees: mh.sh --json stdout is not one JSON object/,
    ],
    ['MH_RESULT STATUS=OK REASON=ok', '{"ok":false}', 0, /^json-ok-agrees: mh.sh's stdout ok=/],
  ] as const)('refuses %j', (stderr, stdout, code, re) => {
    expect(() => ok(stderr, stdout, code)).toThrow(re);
  });
});

describe('parseList', () => {
  test('golden v1 document', () => {
    const p = parseList(listDoc([entry(0, 'serving', 'baseline'), entry(1, 'free', null)]));
    expect(p.baseline).toBe(1);
    expect(p.slots.map((s) => s.state)).toEqual(['serving', 'free']);
  });
  test('unknown schema_version, zero slots and duplicates are refused', () => {
    expect(() => parseList({ ...listDoc([entry(0, 'free', null)]), schema_version: 2 })).toThrow(
      /schema_version/,
    );
    expect(() => parseList(listDoc([]))).toThrow(/zero slots/);
    expect(() => parseList(listDoc([entry(1, 'free', null), entry(1, 'free', null)]))).toThrow(
      /duplicate/,
    );
  });
  test.each(SLOT_KEYS.map((k) => [k]))('a missing `%s` key is refused', (key) => {
    const e: Record<string, unknown> = { ...entry(0, 'free', null) };
    delete e[key];
    expect(() => parseList(listDoc([e as unknown as SlotEntry]))).toThrow(
      new RegExp(`no \`${key}\` key`),
    );
  });
  test.each([['ok'], ['schema_version'], ['template_revision'], ['baseline'], ['slots']])(
    'a missing top-level `%s` is refused',
    (key) => {
      const d: Record<string, unknown> = { ...listDoc([entry(0, 'free', null)]) };
      delete d[key];
      expect(() => parseList(d)).toThrow(/mh.sh list --json/);
    },
  );
  test('an additive extra key passes', () => {
    const e = { ...entry(0, 'free', null), future_field: 'x' };
    expect(() => parseList({ ...listDoc([e]), future_top: 1 })).not.toThrow();
  });
});

describe('baselineVerdict (twin of Rust baseline_verdict)', () => {
  const snap = (slots: SlotEntry[], baseline = 1): PoolSnapshot => ({
    templateRevision: 'r1',
    baseline,
    slots,
  });
  const hint = (o: string) => `reclaim ${o}`;
  test('clean, foreign (with a reclaim hint), short, over-count, stale', () => {
    expect(
      baselineVerdict(
        snap([entry(0, 'serving', 'baseline'), entry(1, 'serving', OWNER)]),
        OWNER,
        hint,
      ),
    ).toBeNull();
    const foreign = baselineVerdict(
      snap([entry(0, 'serving', 'baseline'), entry(1, 'serving', 'e2e-ffff')]),
      OWNER,
      hint,
    );
    expect(foreign).toMatch(new RegExp(`^${FOREIGN_MH_ACTIVITY}: .*slot 1`));
    expect(foreign).toContain('reclaim a leaked run with: reclaim e2e-ffff');
    expect(baselineVerdict(snap([entry(0, 'terminating', 'baseline')]), OWNER, hint)).toMatch(
      /serving MHs: 0, B = 1/,
    );
    expect(
      baselineVerdict(
        snap([entry(0, 'serving', 'baseline'), entry(1, 'serving', 'baseline')]),
        OWNER,
        hint,
      ),
    ).toMatch(/serving MHs: 2, B = 1/);
    expect(
      baselineVerdict(snap([entry(0, 'serving', 'baseline', { stale: true })]), OWNER, hint),
    ).toBeNull();
  });
});

describe('createMhPool against a stub mh.sh', () => {
  test('list passes argv with no shell, --json appended', async () => {
    const s = stub([okList([entry(0, 'serving', 'baseline')])]);
    await pool(s).list();
    expect(s.calls()).toEqual([['list', '--json']]);
  });

  test('up passes the run owner and refuses a slot owned by someone else', async () => {
    const mine = stub([
      {
        stdout: { ok: true, ...entry(2, 'serving', OWNER) },
        result: 'MH_RESULT STATUS=OK REASON=ok SLOT=2',
        code: 0,
      },
    ]);
    expect((await pool(mine).up()).slot).toBe(2);
    expect(mine.calls()).toEqual([['up', '--owner', OWNER, '--json']]);
    const theirs = stub([
      {
        stdout: { ok: true, ...entry(2, 'serving', 'other') },
        result: 'MH_RESULT STATUS=OK REASON=ok SLOT=2',
        code: 0,
      },
    ]);
    await expect(pool(theirs).up()).rejects.toThrow(/not owned by this run/);
  });

  test('a FAIL surfaces the REASON verbatim as MhShError', async () => {
    const s = stub([
      { stdout: { ok: false }, result: 'MH_RESULT STATUS=FAIL REASON=mh-no-free-slot', code: 3 },
    ]);
    await expect(pool(s).up()).rejects.toMatchObject({
      name: 'MhShError',
      reason: 'mh-no-free-slot',
      exitCode: 3,
    });
  });

  test('unknown token and exit/group mismatch are contract breaks', async () => {
    const unknown = stub([
      { stdout: { ok: false }, result: 'MH_RESULT STATUS=FAIL REASON=made-up', code: 1 },
    ]);
    await expect(pool(unknown).list()).rejects.toThrow(
      /PRECONDITION \(mh-sh-contract\).*known-token/,
    );
    const mismatch = stub([
      { stdout: { ok: false }, result: 'MH_RESULT STATUS=FAIL REASON=foreign-owner', code: 1 },
    ]);
    await expect(pool(mismatch).drain(1)).rejects.toThrow(/exit-group/);
  });

  test('BUSY is retried until OK, sleeping the interval', async () => {
    const c = clock();
    const s = stub([busy, busy, okList([entry(0, 'serving', 'baseline')])]);
    await pool(s, c).list();
    expect(s.calls()).toHaveLength(3);
    expect(c.slept).toEqual([BUSY_RETRY_INTERVAL_MS, BUSY_RETRY_INTERVAL_MS]);
  });

  test('BUSY exhaustion is loud, names the holder, and is bounded', async () => {
    const c = clock();
    const s = stub([busy]);
    await expect(
      createMhPool({
        mhSh: s.mhSh,
        owner: OWNER,
        cluster: 'devloop-x',
        sleep: c.sleep,
        now: c.now,
        busyBoundMs: 6_000,
        busyIntervalMs: 2_000,
      }).list(),
    ).rejects.toThrow(/BUSY \(mh-pool-busy\).*after 4 attempts.*holder=e2e-ffff@host:1/);
    expect(s.calls()).toHaveLength(4);
  });

  test('teardown drains by owner and fails on a run-owned leftover', async () => {
    const clean = stub([
      { stdout: { ok: true, drained: [] }, result: 'MH_RESULT STATUS=OK REASON=ok', code: 0 },
      okList([entry(0, 'serving', 'baseline')]),
    ]);
    await pool(clean).teardown();
    expect(clean.calls()[0]).toEqual(['drain', '--all', '--owner', OWNER, '--json']);
    const leak = stub([
      { stdout: { ok: true, drained: [] }, result: 'MH_RESULT STATUS=OK REASON=ok', code: 0 },
      okList([entry(0, 'serving', 'baseline'), entry(3, 'draining', OWNER)]),
    ]);
    await expect(pool(leak).teardown()).rejects.toThrow(
      new RegExp(
        `${MH_RUN_LEAK}.*slot\\(s\\) 3.*DT_CLUSTER_NAME=devloop-x .*drain --all --owner ${OWNER}`,
      ),
    );
  });

  const signalOk = (
    sig: string,
    state: string,
    e: SlotEntry,
    budget: number | null = null,
    restart: number | null = sig === 'KILL' ? 30 : null,
  ): Canned => ({
    stdout: {
      ok: true,
      ...e,
      signal: sig,
      pid: 42,
      process_state: state,
      liveness_budget_seconds: budget,
      restart_budget_seconds: restart,
    },
    result: `MH_RESULT STATUS=OK REASON=ok SLOT=${e.slot}`,
    code: 0,
  });
  const signalFail = (reason: string, code: number): Canned => ({
    stdout: { ok: false },
    result: `MH_RESULT STATUS=FAIL REASON=${reason}`,
    code,
  });

  test('mh.sh runs against the validated cluster, never the inherited one', async () => {
    const saved = process.env.DT_CLUSTER_NAME;
    process.env.DT_CLUSTER_NAME = 'inherited-other';
    try {
      const s = stub([okList([entry(0, 'serving', 'baseline')])]);
      await pool(s).list();
      expect(s.clusters()).toEqual(['devloop-x']);
    } finally {
      if (saved === undefined) delete process.env.DT_CLUSTER_NAME;
      else process.env.DT_CLUSTER_NAME = saved;
    }
  });

  test('drain passes slot, owner and --no-wait as argv', async () => {
    const s = stub([
      { stdout: { ok: true }, result: 'MH_RESULT STATUS=OK REASON=ok SLOT=3', code: 0 },
    ]);
    const p = pool(s);
    await p.drain(3);
    await p.drain(3, { noWait: true });
    expect(s.calls()).toEqual([
      ['drain', '3', '--owner', OWNER, '--json'],
      ['drain', '3', '--owner', OWNER, '--no-wait', '--json'],
    ]);
  });

  test('DT_CLUSTER_NAME unset is a PRECONDITION before anything runs', async () => {
    expect(() => clusterFrom(undefined)).toThrow(/PRECONDITION \(cluster-unset\)/);
    expect(() => clusterFrom(' ')).toThrow(/unset or empty/);
    expect(clusterFrom('c')).toBe('c');
    const s = stub([okList([entry(0, 'serving', 'baseline')])]);
    const saved = process.env.DT_CLUSTER_NAME;
    delete process.env.DT_CLUSTER_NAME;
    try {
      await expect(createMhPool({ mhSh: s.mhSh, owner: OWNER }).list()).rejects.toThrow(
        /cluster-unset/,
      );
      expect(s.calls()).toEqual([]);
    } finally {
      if (saved !== undefined) process.env.DT_CLUSTER_NAME = saved;
    }
  });

  test('kill waits for an in-place restart (same uid, +1, serving)', async () => {
    const before = entry(2, 'serving', OWNER);
    const s = stub([
      okList([entry(0, 'serving', 'baseline'), before]),
      signalOk('KILL', 'gone', before),
      okList([entry(0, 'serving', 'baseline'), { ...before, state: 'starting', restart_count: 1 }]),
      okList([entry(0, 'serving', 'baseline'), { ...before, restart_count: 1 }]),
    ]);
    expect((await pool(s).kill(2)).restart_count).toBe(1);
    expect(s.calls()[1]).toEqual(['signal', '2', 'KILL', '--owner', OWNER, '--json']);
  });

  test("kill without a restart fails loudly at mh.sh's restart budget", async () => {
    const before = entry(2, 'serving', OWNER);
    const c = clock();
    const s = stub([okList([before]), signalOk('KILL', 'gone', before, null, 6), okList([before])]);
    await expect(pool(s, c).kill(2)).rejects.toThrow(
      new RegExp(`${RESTART_NOT_OBSERVED}.*polled for 6000 ms`),
    );
    expect(c.slept.reduce((x, y) => x + y, 0)).toBe(6000);
  });

  test('a KILL answer without restart_budget_seconds is a contract break', async () => {
    const before = entry(2, 'serving', OWNER);
    const s = stub([okList([before]), signalOk('KILL', 'gone', before, null, null)]);
    await expect(pool(s).kill(2)).rejects.toThrow(/no restart_budget_seconds/);
  });

  test('freeze -> resume: budget, state, and unchanged restart count both ways', async () => {
    const e = entry(2, 'serving', OWNER);
    const frozenEntry = { ...e, state: 'starting' as const };
    const good = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([frozenEntry]),
      signalOk('CONT', 'S', frozenEntry),
      okList([e]),
    ]);
    const c = clock();
    const guard = await pool(good, c).freeze(2);
    expect(guard.budgetMs).toBe(25_000);
    c.advance(10_000);
    expect((await guard.resume()).restart_count).toBe(0);
    expect(good.calls().map((a) => a.slice(0, 3))).toEqual([
      ['list', '--json'],
      ['signal', '2', 'STOP'],
      ['list', '--json'],
      ['signal', '2', 'CONT'],
      ['list', '--json'],
    ]);

    const restarted = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([e]),
      signalOk('CONT', 'S', e),
      okList([{ ...e, restart_count: 1 }]),
    ]);
    await expect((await pool(restarted).freeze(2)).resume()).rejects.toThrow(
      new RegExp(RESTART_COUNT_CHANGED),
    );

    // Overrun wins over the restart it causes (checked before the incarnation).
    const late = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([e]),
      signalOk('CONT', 'S', e),
      okList([{ ...e, restart_count: 1 }]),
    ]);
    const c2 = clock();
    const g2 = await pool(late, c2).freeze(2);
    c2.advance(25_000);
    await expect(g2.resume()).rejects.toThrow(new RegExp(FREEZE_BUDGET_EXCEEDED));
    expect(late.calls().some((a) => a[2] === 'CONT')).toBe(true);
  });

  test('a restart between STOP and the post-STOP read is refused (and CONT sent)', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([{ ...e, restart_count: 1 }]),
      { stdout: { ok: true }, result: 'MH_RESULT STATUS=OK REASON=ok', code: 0 },
    ]);
    await expect(pool(s).freeze(2)).rejects.toThrow(new RegExp(RESTART_COUNT_CHANGED));
    expect(s.calls()[3]).toEqual(['signal', '2', 'CONT', '--owner', OWNER, '--json']);
  });

  test('a failed STOP call still sends a best-effort CONT, then rethrows', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([okList([e]), signalFail('signal-failed', 1), signalFail('signal-failed', 1)]);
    await expect(pool(s).freeze(2)).rejects.toMatchObject({ reason: 'signal-failed' });
    expect(s.calls()[2]).toEqual(['signal', '2', 'CONT', '--owner', OWNER, '--json']);
  });

  test('a STOP that did not take effect is refused', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([okList([e]), signalOk('STOP', 'S', e, 25), signalOk('CONT', 'S', e)]);
    await expect(pool(s).freeze(2)).rejects.toThrow(/process_state=S/);
  });

  test('a failed CONT leaves the guard armed: dispose retries the CONT', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([e]),
      signalFail('debug-container-timeout', 1),
      signalOk('CONT', 'S', e),
    ]);
    const guard = await pool(s).freeze(2);
    await expect(guard.resume()).rejects.toMatchObject({ reason: 'debug-container-timeout' });
    await guard.dispose();
    const conts = s.calls().filter((a) => a[0] === 'signal' && a[2] === 'CONT');
    expect(conts).toHaveLength(2);
  });

  test('dispose while frozen sends a best-effort CONT and never throws', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([e]),
      { stdout: 'garbage', result: 'nonsense', code: 9 },
    ]);
    const guard = await pool(s).freeze(2);
    await expect(guard.dispose()).resolves.toBeUndefined();
    expect(s.calls()[3]).toEqual(['signal', '2', 'CONT', '--owner', OWNER, '--json']);
    await guard.dispose(); // idempotent
    expect(s.calls()).toHaveLength(4);
  });

  test('assertBaseline polls until clean (positive control) and fails loudly when not', async () => {
    const clean = okList([entry(0, 'serving', 'baseline')]);
    const foreign = okList([entry(0, 'serving', 'baseline'), entry(1, 'serving', 'e2e-ffff')]);
    const c = clock();
    const settles = stub([foreign, clean]);
    await pool(settles, c).assertBaseline(30_000);
    expect(settles.calls()).toHaveLength(2);
    expect(c.slept.length, 'it slept between the two reads').toBe(1);

    const stays = stub([foreign]);
    await expect(pool(stays, clock()).assertBaseline(10_000)).rejects.toThrow(
      new RegExp(`${FOREIGN_MH_ACTIVITY}.*drain --all --owner e2e-ffff.*polled for 10000 ms`),
    );
  });

  test('no verb ever asks mh.sh (or kubectl) to delete a pod or force anything', async () => {
    const e = entry(2, 'serving', OWNER);
    const s = stub([
      okList([e]),
      signalOk('STOP', 'T', e, 25),
      okList([e]),
      signalOk('CONT', 'S', e),
      okList([e]),
    ]);
    await (await pool(s).freeze(2)).resume();
    const argv = s.calls().flat();
    expect(argv.length, 'positive control: calls were recorded').toBeGreaterThan(0);
    expect(argv.filter((a) => /^(delete|--force|--grace-period)/.test(a))).toEqual([]);
  });
});

describe('drift pins (one home each)', () => {
  const mhSh = read('infra/kind/scripts/mh.sh');
  const mhPoolRs = read('crates/env-tests/src/fixtures/mh_pool.rs');

  test('MH_TOKENS == mh.sh MH_TOKENS, both directions, codes included', () => {
    const body = /readonly -a MH_TOKENS=\(([\s\S]*?)\n\)/.exec(mhSh)?.[1] ?? '';
    const theirs = Object.fromEntries(
      [...body.matchAll(/"([a-z0-9-]+):(\d)"/g)].map((m) => [m[1], Number(m[2])]),
    );
    expect(Object.keys(theirs).length, 'vacuity: the extraction found tokens').toBeGreaterThan(10);
    expect(MH_TOKENS).toEqual(theirs);
  });

  test('SLOT_KEYS == mh.sh MH_JQ_PUBLIC', () => {
    const inner = /readonly MH_JQ_PUBLIC='def public: \{([^}]*)\}/.exec(mhSh)?.[1] ?? '';
    const keys = inner
      .split(',')
      .map((k) => k.trim())
      .filter((k) => k !== '');
    expect(keys.length, 'vacuity').toBeGreaterThan(5);
    expect([...SLOT_KEYS]).toEqual(keys);
  });

  test('BUSY bound/interval == mh_pool.rs', () => {
    const secs = (name: string) =>
      Number(
        new RegExp(
          `pub const ${name}: std::time::Duration = std::time::Duration::from_secs\\((\\d+)\\);`,
        ).exec(mhPoolRs)?.[1],
      );
    expect(secs('BUSY_RETRY_BOUND'), 'positive control').toBeGreaterThan(0);
    expect(BUSY_RETRY_BOUND_MS).toBe(secs('BUSY_RETRY_BOUND') * 1000);
    expect(BUSY_RETRY_INTERVAL_MS).toBe(secs('BUSY_RETRY_INTERVAL') * 1000);
  });

  test('SCHEMA_VERSION == mh.sh MH_SCHEMA_VERSION; BASELINE_OWNER == deploy.sh', () => {
    const schema = /^readonly MH_SCHEMA_VERSION=(\d+)$/m.exec(mhSh)?.[1];
    expect(schema, 'positive control: MH_SCHEMA_VERSION found').toBeTruthy();
    expect(SCHEMA_VERSION).toBe(Number(schema));
    const deploy = read('infra/kind/scripts/deploy.sh');
    const owner = /^\s*MH_BASELINE_OWNER="([^"]+)"$/m.exec(deploy)?.[1];
    expect(owner, 'positive control: MH_BASELINE_OWNER found').toBeTruthy();
    expect(BASELINE_OWNER).toBe(owner);
  });

  test('fixture tokens == mh_pool.rs consts', () => {
    const ours: Record<string, string> = {
      FOREIGN_MH_ACTIVITY,
      MH_RUN_LEAK,
      FREEZE_BUDGET_EXCEEDED,
      RESTART_COUNT_CHANGED,
      RESTART_NOT_OBSERVED,
      MH_SCRAPE_UP_TIMEOUT,
    };
    const theirs = Object.fromEntries(
      [...mhPoolRs.matchAll(/pub const ([A-Z_]+): &str = "([a-z-]+)";/g)].map((m) => [m[1], m[2]]),
    );
    for (const [name, value] of Object.entries(ours)) {
      expect(theirs[name], `mh_pool.rs ${name}`).toBe(value);
    }
  });
});
