// File: packages/web-app/e2e/mhPool.ts
//
// The Playwright harness's driver for the Kind MH pool (ADR-0039 §7, §9;
// contract docs/debates/2026-10-28-mh-replacement-and-migration/mh-sh-contract.txt
// §§2-8). A THIN wrapper over `infra/kind/scripts/mh.sh --json`, the TS twin of
// `crates/env-tests/src/fixtures/mh_pool.rs` (same verbs, same named rejection
// cases, same outcome assertions; every twin site says so).
//
// * EXEC: `execFile`, argv arrays only, NEVER a shell. mh.sh selects the cluster
//   itself (`kind-$DT_CLUSTER_NAME`), so this file makes no kubectl call at all.
// * PATH + CLUSTER: `DT_MH_SH` and `DT_CLUSTER_NAME`, exported by
//   scripts/layer7.sh, read at CALL time (a spec that never touches the pool
//   does not throw on import). Unset is a loud PRECONDITION; no fallback (mh.sh's
//   own default cluster could be another one).
// * BASELINE: `assertBaseline(ceilingMs)` takes a ceiling that MUST be derived
//   like Rust `gc_fleet::removal_settle_budget` (live GC staleness gauge + 2
//   health ticks + scrape ceiling), never a literal. C3 owns that TS read, with
//   the post-teardown settle wait.
// * OWNER: the per-run org subdomain (`e2eEnv.orgSubdomain`, E2E_ORG_SUBDOMAIN),
//   the same token Rust's `MhPool::for_run` uses; read lazily.
// * INJECTOR: kill / freeze / resume are `mh.sh signal <k> KILL|STOP|CONT`, the
//   ONE home of the privileged recipe (debug container, uid 65532, never PID 1,
//   shareProcessNamespace preflight, liveness budget). Only run-owned MHs.
//
// Pinned copies (tests/mhPool.test.ts fails on drift): MH_TOKENS and SLOT_KEYS
// against mh.sh; SCHEMA_VERSION against mh.sh; BASELINE_OWNER against
// deploy.sh; the BUSY bound/interval and the fixture tokens against mh_pool.rs.
//
// CALLERS: mh.sh's `up` bound is derived and can exceed Playwright's per-test
// timeout, so a spec that calls `up` sets `test.setTimeout` above it. The
// post-teardown "removed MH no longer GC-selectable" settle wait (Rust:
// `MhPool::teardown_and_settle`) is NOT here: C3 owns it for the browser legs.
// FreezeGuard: the tsconfig lib has no `esnext.disposable`, so callers MUST
// `try { ... } finally { await frozen.dispose(); }` (dispose is a best-effort
// CONT that never throws and logs the resume command).

import { execFile } from 'node:child_process';
import { statSync } from 'node:fs';
import { isAbsolute } from 'node:path';

/** mh.sh's token table (contract §3), token -> exit code. Pinned to mh.sh `MH_TOKENS`. */
export const MH_TOKENS: Readonly<Record<string, number>> = {
  ok: 0,
  'mh-rollout-failed': 1,
  'drain-timeout': 1,
  'not-available-timeout': 1,
  'kubectl-failed': 1,
  'internal-error': 1,
  'signal-failed': 1,
  'debug-container-timeout': 1,
  usage: 2,
  'no-kubeconfig': 2,
  'no-template': 2,
  'prerequisite-missing': 2,
  'mh-no-share-process-namespace': 2,
  'mh-no-free-slot': 3,
  'last-serving-mh': 4,
  'stuck-terminating': 4,
  'foreign-owner': 4,
  'mh-not-serving': 4,
  'mh-pool-busy': 5,
};

/** `list --json` v1 per-slot keys, in mh.sh's order. Pinned to mh.sh `MH_JQ_PUBLIC`. */
export const SLOT_KEYS = [
  'slot',
  'host_port',
  'address',
  'pod_ip',
  'pod_uid',
  'state',
  'terminating_past_grace',
  'deployment',
  'pod',
  'revision',
  'stale',
  'owner',
  'restart_count',
] as const;

export const SCHEMA_VERSION = 1;

// Fixture-side tokens. One home: mh_pool.rs (pinned line by line).
export const FOREIGN_MH_ACTIVITY = 'foreign-mh-activity';
export const MH_RUN_LEAK = 'mh-run-leak';
export const FREEZE_BUDGET_EXCEEDED = 'freeze-budget-exceeded';
export const RESTART_COUNT_CHANGED = 'restart-count-changed';
export const RESTART_NOT_OBSERVED = 'restart-not-observed';
export const MH_SCRAPE_UP_TIMEOUT = 'mh-scrape-up-timeout';

/** Rust `BUSY_RETRY_BOUND` / `BUSY_RETRY_INTERVAL` (pinned). */
export const BUSY_RETRY_BOUND_MS = 60_000;
export const BUSY_RETRY_INTERVAL_MS = 2_000;
/** Poll interval while waiting for an in-place restart (the CEILING is mh.sh's `restart_budget_seconds`). */
const RESTART_POLL_MS = 2_000;

/** deploy.sh `MH_BASELINE_OWNER` (pinned). */
export const BASELINE_OWNER = 'baseline';
const PROCESS_GONE = 'gone';
const PROCESS_STOPPED = 'T';

export type SlotState = 'free' | 'starting' | 'serving' | 'draining' | 'terminating';
const SLOT_STATES: readonly SlotState[] = [
  'free',
  'starting',
  'serving',
  'draining',
  'terminating',
];

export interface SlotEntry {
  slot: number;
  host_port: number;
  address: string;
  pod_ip: string | null;
  pod_uid: string | null;
  state: SlotState;
  terminating_past_grace: boolean;
  deployment: string | null;
  pod: string | null;
  revision: string | null;
  stale: boolean;
  owner: string | null;
  restart_count: number | null;
}

export interface PoolSnapshot {
  templateRevision: string;
  baseline: number;
  slots: SlotEntry[];
}

export interface MhShResult {
  status: 'OK' | 'FAIL';
  reason: string;
  slot: number | null;
  detail: string | null;
  exitCode: number;
  json: Record<string, unknown>;
}

/** A contract-conforming mh.sh FAIL (REASON surfaced verbatim). */
export class MhShError extends Error {
  constructor(
    readonly verb: string,
    readonly reason: string,
    readonly exitCode: number,
    readonly detail: string | null,
  ) {
    super(
      `FAIL (mh-sh): \`mh.sh ${verb}\` STATUS=FAIL REASON=${reason} EXIT=${exitCode}` +
        (detail === null ? '' : ` DETAIL=${detail}`),
    );
    this.name = 'MhShError';
  }
}

/** The path: unset, blank, relative or not a file is a PRECONDITION (Rust `mh_sh_path_from`). */
export function mhShPathFrom(value: string | undefined): string {
  const hint =
    'scripts/layer7.sh exports it (an absolute path to infra/kind/scripts/mh.sh); outside ' +
    'Layer 7 set it yourself, e.g. DT_MH_SH="$PWD/infra/kind/scripts/mh.sh"';
  if (value === undefined || value.trim() === '') {
    throw new Error(`PRECONDITION (mh-sh-unset): DT_MH_SH is unset or empty; ${hint}`);
  }
  if (!isAbsolute(value)) {
    throw new Error(`PRECONDITION (mh-sh-unset): DT_MH_SH=${value} is not absolute; ${hint}`);
  }
  const isFile = (() => {
    try {
      return statSync(value).isFile();
    } catch {
      return false;
    }
  })();
  if (!isFile) {
    throw new Error(
      `PRECONDITION (mh-sh-unset): DT_MH_SH=${value} is not an existing file; ${hint}`,
    );
  }
  return value;
}

/**
 * PURE: parse one mh.sh run. Twin of Rust `parse_mh_sh_result`, with the SAME
 * named rejection cases: one-line, fields, known-token, exit-group, ok-reason,
 * json-ok-agrees. Throws `Error('<case>: ...')`; never echoes raw stdout.
 */
export function parseMhShResult(
  stderr: string,
  stdout: string,
  exitCode: number | null,
): MhShResult {
  const lines = stderr.split('\n');
  const resultLines = lines.filter((l) => l.startsWith('MH_RESULT ')).length;
  if (resultLines > 1) {
    throw new Error(
      `one-line: mh.sh printed ${resultLines} MH_RESULT lines (contract §2: exactly one)`,
    );
  }
  const last = [...lines].reverse().find((l) => l.trim() !== '');
  if (last === undefined)
    throw new Error('one-line: mh.sh printed nothing on stderr (no MH_RESULT line)');
  if (!last.startsWith('MH_RESULT ')) {
    throw new Error("one-line: mh.sh's last stderr line is not its MH_RESULT line");
  }
  let rest = last.slice('MH_RESULT '.length);
  let detail: string | null = null;
  const d = rest.indexOf(' DETAIL=');
  if (d >= 0) {
    detail = rest.slice(d + ' DETAIL='.length);
    rest = rest.slice(0, d);
  }
  let status: 'OK' | 'FAIL' | null = null;
  let reason: string | null = null;
  let slot: number | null = null;
  for (const field of rest.split(/\s+/).filter((f) => f !== '')) {
    const eq = field.indexOf('=');
    const key = eq < 0 ? field : field.slice(0, eq);
    const value = eq < 0 ? '' : field.slice(eq + 1);
    if (key === 'STATUS' && (value === 'OK' || value === 'FAIL')) status = value;
    else if (key === 'REASON' && value !== '') reason = value;
    else if (key === 'SLOT') {
      if (!/^\d+$/.test(value))
        throw new Error(`fields: MH_RESULT SLOT=${JSON.stringify(value)} is not a slot number`);
      slot = Number(value);
    } else throw new Error(`fields: MH_RESULT has a malformed field ${JSON.stringify(field)}`);
  }
  if (status === null) throw new Error('fields: MH_RESULT has no valid STATUS');
  if (reason === null) throw new Error('fields: MH_RESULT has no REASON');
  const group = MH_TOKENS[reason];
  if (group === undefined) {
    throw new Error(`known-token: REASON=${reason} is not in mh.sh's token table (MH_TOKENS)`);
  }
  if (exitCode === null) throw new Error('exit-group: mh.sh was killed by a signal (no exit code)');
  if (exitCode !== group) {
    throw new Error(
      `exit-group: REASON=${reason} belongs to exit ${group}, mh.sh exited ${exitCode}`,
    );
  }
  if ((status === 'OK') !== (reason === 'ok')) {
    throw new Error(`ok-reason: STATUS=${status} with REASON=${reason} (OK iff REASON=ok)`);
  }
  let json: unknown;
  try {
    json = JSON.parse(stdout.trim());
  } catch (e) {
    throw new Error(
      `json-ok-agrees: mh.sh --json stdout is not one JSON object (${String(e)}); ${last}`,
      { cause: e },
    );
  }
  if (typeof json !== 'object' || json === null || Array.isArray(json)) {
    throw new Error('json-ok-agrees: mh.sh --json stdout is not one JSON object');
  }
  const obj = json as Record<string, unknown>;
  if (obj.ok !== (status === 'OK')) {
    throw new Error(
      `json-ok-agrees: mh.sh's stdout ok=${JSON.stringify(obj.ok)} disagrees with its MH_RESULT STATUS=${status}`,
    );
  }
  return { status, reason, slot, detail, exitCode, json: obj };
}

function slotEntryFrom(raw: unknown, where: string): SlotEntry {
  if (typeof raw !== 'object' || raw === null) throw new Error(`${where} is not an object`);
  const o = raw as Record<string, unknown>;
  for (const k of SLOT_KEYS) {
    if (!(k in o)) throw new Error(`${where} has no \`${k}\` key (schema v1 pins the key set)`);
  }
  const str = (k: string): string | null => {
    const v = o[k];
    if (v === null) return null;
    if (typeof v !== 'string') throw new Error(`${where}.${k} is not a string or null`);
    return v;
  };
  const int = (k: string): number => {
    const v = o[k];
    if (typeof v !== 'number' || !Number.isInteger(v) || v < 0)
      throw new Error(`${where}.${k} is not an integer`);
    return v;
  };
  const bool = (k: string): boolean => {
    const v = o[k];
    if (typeof v !== 'boolean') throw new Error(`${where}.${k} is not a boolean`);
    return v;
  };
  const state = o.state;
  if (typeof state !== 'string' || !SLOT_STATES.includes(state as SlotState)) {
    throw new Error(`${where}.state ${JSON.stringify(state)} is not a slot state`);
  }
  const address = str('address');
  if (address === null) throw new Error(`${where}.address is null`);
  return {
    slot: int('slot'),
    host_port: int('host_port'),
    address,
    pod_ip: str('pod_ip'),
    pod_uid: str('pod_uid'),
    state: state as SlotState,
    terminating_past_grace: bool('terminating_past_grace'),
    deployment: str('deployment'),
    pod: str('pod'),
    revision: str('revision'),
    stale: bool('stale'),
    owner: str('owner'),
    restart_count: o.restart_count === null ? null : int('restart_count'),
  };
}

/** PURE: validate a `list --json` object (twin of Rust `parse_list_value`). */
export function parseList(doc: Record<string, unknown>): PoolSnapshot {
  if (doc.ok !== true) throw new Error('mh.sh list --json: `ok` is not true');
  if (doc.schema_version !== SCHEMA_VERSION) {
    throw new Error(
      `mh.sh list --json: schema_version ${JSON.stringify(doc.schema_version)}, this reader understands ${SCHEMA_VERSION}`,
    );
  }
  const templateRevision = doc.template_revision;
  if (typeof templateRevision !== 'string' || templateRevision === '') {
    throw new Error('mh.sh list --json: no template_revision');
  }
  const baseline = doc.baseline;
  if (typeof baseline !== 'number' || !Number.isInteger(baseline) || baseline < 0) {
    throw new Error('mh.sh list --json: baseline is not a non-negative integer');
  }
  if (!Array.isArray(doc.slots)) throw new Error('mh.sh list --json: no slots array');
  if (doc.slots.length === 0)
    throw new Error('mh.sh list --json: zero slots (every slot always appears)');
  const slots = doc.slots.map((s, i) => slotEntryFrom(s, `mh.sh list --json: slots[${i}]`));
  if (new Set(slots.map((s) => s.slot)).size !== slots.length) {
    throw new Error('mh.sh list --json: duplicate slot numbers');
  }
  return { templateRevision, baseline, slots };
}

/**
 * PURE: twin of Rust `baseline_verdict`. Returns the failure, or null when
 * clean. `reclaim(owner)` builds the hint per foreign owner (the pool's
 * `reclaimCommandFor`, so the printed command is the executed one).
 */
export function baselineVerdict(
  snap: PoolSnapshot,
  run: string,
  reclaim: (owner: string) => string,
): string | null {
  const problems: string[] = [];
  const foreign = new Set<string>();
  for (const s of snap.slots.filter((x) => x.pod !== null)) {
    if (s.owner !== BASELINE_OWNER && s.owner !== run) {
      problems.push(`slot ${s.slot} (${s.state}) owned by ${JSON.stringify(s.owner)}`);
      if (s.owner !== null) foreign.add(s.owner);
    }
  }
  const serving = snap.slots.filter(
    (s) => s.state === 'serving' && s.owner === BASELINE_OWNER,
  ).length;
  if (serving !== snap.baseline) {
    problems.push(
      `baseline-owned serving MHs: ${serving}, B = ${snap.baseline} (mh.sh list \`baseline\`)`,
    );
  }
  if (problems.length === 0) return null;
  const hints = [...foreign].sort().map(reclaim);
  return (
    `${FOREIGN_MH_ACTIVITY}: the MH pool is not {baseline B, this run ${run}}: ${problems.join('; ')}. ` +
    'Layer 7 supports one suite per cluster; inspect `$DT_MH_SH list`' +
    (hints.length === 0 ? '' : `; reclaim a leaked run with: ${hints.join(' ; ')}`)
  );
}

/** PURE: twin of Rust `restarted_in_place`. */
export function restartedInPlace(before: SlotEntry, after: SlotEntry): boolean {
  return (
    after.pod_uid !== null &&
    after.pod_uid === before.pod_uid &&
    before.restart_count !== null &&
    after.restart_count === before.restart_count + 1 &&
    after.state === 'serving'
  );
}

/** PURE: twin of Rust `assert_same_incarnation`. Returns the failure, or null. */
export function sameIncarnation(before: SlotEntry, after: SlotEntry): string | null {
  if (before.pod_uid !== after.pod_uid || before.restart_count !== after.restart_count) {
    return (
      `${RESTART_COUNT_CHANGED}: slot ${before.slot} went pod_uid ${before.pod_uid} restart_count ` +
      `${before.restart_count} -> pod_uid ${after.pod_uid} restart_count ${after.restart_count} ` +
      'across freeze/resume: a kubelet restart or another incarnation, not a zombie'
    );
  }
  return null;
}

/**
 * PURE: twin of Rust `freeze_overrun`. `elapsedMs` runs from BEFORE the STOP
 * call to AFTER the CONT call returned (an upper bound on the real frozen
 * time). Returns the failure, or null.
 */
export function freezeOverrun(elapsedMs: number, budgetMs: number): string | null {
  if (elapsedMs < budgetMs) return null;
  return (
    `${FREEZE_BUDGET_EXCEEDED}: the MH was frozen for ${elapsedMs} ms, at or past the live ` +
    `liveness-probe budget ${budgetMs} ms ((failureThreshold-1) x periodSeconds + timeoutSeconds): ` +
    'kubelet may already have restarted it. Resume sooner'
  );
}

export type Signal = 'KILL' | 'STOP' | 'CONT';

/** The ONE `mh.sh signal` argv (twin of Rust `signal_args`): executed AND printed. */
export function signalArgs(slot: number, sig: Signal, owner: string): string[] {
  return ['signal', String(slot), sig, '--owner', owner];
}

/** The ONE owner-teardown argv (twin of Rust `teardown_args`): executed AND printed. */
export function teardownArgs(owner: string): string[] {
  return ['drain', '--all', '--owner', owner];
}

/** DT_CLUSTER_NAME: unset or blank is a PRECONDITION (twin of Rust `dt_cluster_name_from`). */
export function clusterFrom(value: string | undefined): string {
  if (value === undefined || value.trim() === '') {
    throw new Error(
      'PRECONDITION (cluster-unset): DT_CLUSTER_NAME is unset or empty; scripts/layer7.sh exports ' +
        "it (the devloop cluster mh.sh targets as kind-<name>). There is no default: mh.sh's own " +
        'default could be another cluster',
    );
  }
  return value;
}

export interface SignalOutcome {
  entry: SlotEntry;
  processState: string;
  /** STOP only (null otherwise). */
  livenessBudgetMs: number | null;
  /** KILL only (null otherwise): mh.sh's derived `up` bound, the restart ceiling. */
  restartBudgetMs: number | null;
}

function positiveSecondsMs(json: Record<string, unknown>, key: string): number | null {
  const raw = json[key];
  if (raw === undefined || raw === null) return null;
  if (typeof raw !== 'number' || !Number.isInteger(raw) || raw <= 0) {
    throw new Error(`${key} is not a positive integer`);
  }
  return raw * 1000;
}

/** PURE: twin of Rust `parse_signal`. */
export function parseSignal(json: Record<string, unknown>, signal: Signal): SignalOutcome {
  const entry = slotEntryFrom(json, 'mh.sh signal OK object');
  const ps = json.process_state;
  if (typeof ps !== 'string' || ps === '')
    throw new Error("mh.sh signal's OK object has no process_state");
  const consistent =
    signal === 'KILL'
      ? ps === PROCESS_GONE
      : signal === 'STOP'
        ? ps === PROCESS_STOPPED
        : ps !== PROCESS_STOPPED && ps !== PROCESS_GONE;
  if (!consistent) throw new Error(`mh.sh signal ${signal} reported OK with process_state=${ps}`);
  const livenessBudgetMs = positiveSecondsMs(json, 'liveness_budget_seconds');
  const restartBudgetMs = positiveSecondsMs(json, 'restart_budget_seconds');
  if (signal === 'STOP' && livenessBudgetMs === null) {
    throw new Error('mh.sh signal STOP reported no liveness_budget_seconds');
  }
  if (signal === 'KILL' && restartBudgetMs === null) {
    throw new Error('mh.sh signal KILL reported no restart_budget_seconds');
  }
  return { entry, processState: ps, livenessBudgetMs, restartBudgetMs };
}

interface RawRun {
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

function execRaw(file: string, cluster: string, args: readonly string[]): Promise<RawRun> {
  return new Promise((resolve, reject) => {
    // argv array, no shell (execFile never spawns one unless `shell` is set).
    // The validated cluster goes INTO the child, so the cluster mh.sh mutates
    // is by construction the one every printed command names.
    execFile(
      file,
      [...args],
      {
        encoding: 'utf8',
        maxBuffer: 16 * 1024 * 1024,
        env: { ...process.env, DT_CLUSTER_NAME: cluster },
      },
      (err, stdout, stderr) => {
        if (err === null) {
          resolve({ exitCode: 0, stdout, stderr });
          return;
        }
        const code = (err as NodeJS.ErrnoException & { code?: unknown }).code;
        if (typeof code === 'number') {
          resolve({ exitCode: code, stdout, stderr });
        } else if ((err as { signal?: unknown }).signal) {
          resolve({ exitCode: null, stdout, stderr });
        } else {
          reject(new Error(`PRECONDITION (mh-sh-run): cannot run ${file}: ${err.message}`));
        }
      },
    );
  });
}

export interface MhPoolOptions {
  /** mh.sh path; defaults to DT_MH_SH (validated at call time). */
  mhSh?: string;
  /** Run owner; defaults to e2eEnv.orgSubdomain (loaded lazily). */
  owner?: string;
  /** DT_CLUSTER_NAME; defaults to the env var (unset is a PRECONDITION). */
  cluster?: string;
  sleep?: (ms: number) => Promise<void>;
  now?: () => number;
  busyBoundMs?: number;
  busyIntervalMs?: number;
}

export interface FreezeGuard {
  readonly slot: number;
  readonly budgetMs: number;
  /** How long the STOP call took (a CONT costs about the same). */
  readonly stopCallMs: number;
  elapsedMs(): number;
  /**
   * CONT (always sent), then: the budget over the whole frozen span, the
   * process left T, restart_count and pod_uid unchanged. Marked resumed only
   * once the CONT returned OK, so a failed CONT leaves dispose() armed.
   */
  resume(): Promise<SlotEntry>;
  /** Best-effort CONT if still frozen; never throws; logs the resume command. */
  dispose(): Promise<void>;
}

export interface MhPool {
  readonly owner: () => Promise<string>;
  list(): Promise<PoolSnapshot>;
  up(): Promise<SlotEntry>;
  drain(slot: number, opts?: { noWait?: boolean }): Promise<void>;
  teardown(): Promise<void>;
  kill(slot: number): Promise<SlotEntry>;
  freeze(slot: number): Promise<FreezeGuard>;
  /**
   * Poll the baseline predicate until clean, within `ceilingMs`. The ceiling
   * MUST be derived like Rust `gc_fleet::removal_settle_budget` (live GC
   * staleness gauge + 2 health ticks + the scrape ceiling), never a literal;
   * C3 owns that read for the browser legs.
   */
  assertBaseline(ceilingMs: number): Promise<void>;
  reclaimCommand(): Promise<string>;
}

/** The pool driver. Every verb is async (Playwright is never blocked). */
export function createMhPool(opts: MhPoolOptions = {}): MhPool {
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const now = opts.now ?? (() => Date.now());
  const busyBound = opts.busyBoundMs ?? BUSY_RETRY_BOUND_MS;
  const busyInterval = opts.busyIntervalMs ?? BUSY_RETRY_INTERVAL_MS;
  const path = (): string => mhShPathFrom(opts.mhSh ?? process.env.DT_MH_SH);
  const cluster = (): string => clusterFrom(opts.cluster ?? process.env.DT_CLUSTER_NAME);
  let cachedOwner: string | undefined = opts.owner;
  const owner = async (): Promise<string> => {
    if (cachedOwner === undefined) {
      const { e2eEnv } = await import('./env.js');
      cachedOwner = e2eEnv.orgSubdomain;
    }
    return cachedOwner;
  };
  /** The paste-ready line for `args` (twin of Rust `command_line`). */
  const commandLine = (args: readonly string[]): string =>
    `DT_CLUSTER_NAME=${cluster()} ${path()} ${args.join(' ')}`;
  const reclaimCommandFor = (o: string): string => commandLine(teardownArgs(o));
  const reclaimCommand = async (): Promise<string> => reclaimCommandFor(await owner());

  /** Twin of Rust `run_with_busy_retry`: only BUSY is retried, count-based. */
  async function run(args: readonly string[]): Promise<MhShResult> {
    const file = path();
    const target = cluster(); // PRECONDITION before any mutation can target a default cluster
    const attempts = Math.floor(busyBound / Math.max(busyInterval, 1)) + 1;
    for (let attempt = 1; ; attempt++) {
      const raw = await execRaw(file, target, [...args, '--json']);
      let result: MhShResult;
      try {
        result = parseMhShResult(raw.stderr, raw.stdout, raw.exitCode);
      } catch (e) {
        throw new Error(
          `PRECONDITION (mh-sh-contract): \`mh.sh ${args.join(' ')}\`: ${(e as Error).message}`,
          { cause: e },
        );
      }
      if (result.reason !== 'mh-pool-busy') return result;
      if (attempt >= attempts) {
        throw new Error(
          `BUSY (mh-pool-busy): \`mh.sh ${args.join(' ')}\` still busy after ${attempts} attempts over ` +
            `${busyBound} ms (> a crashed holder's lease MH_LEASE_S + MH_LEASE_FUTURE_SKEW_S): ` +
            `${result.detail ?? 'holder=<unknown>'} is renewing, so this is a LIVE concurrent mutator`,
        );
      }
      await sleep(busyInterval);
    }
  }

  async function ok(args: readonly string[]): Promise<MhShResult> {
    const r = await run(args);
    if (r.status !== 'OK') throw new MhShError(args.join(' '), r.reason, r.exitCode, r.detail);
    return r;
  }

  async function list(): Promise<PoolSnapshot> {
    return parseList((await ok(['list'])).json);
  }

  async function entry(slot: number): Promise<SlotEntry> {
    const e = (await list()).slots.find((s) => s.slot === slot);
    if (e === undefined) throw new Error(`PRECONDITION (mh-sh-list): slot ${slot} not listed`);
    return e;
  }

  async function signal(slot: number, sig: Signal): Promise<SignalOutcome> {
    const r = await ok(signalArgs(slot, sig, await owner()));
    return parseSignal(r.json, sig);
  }

  /** The backstop (twin of Rust `best_effort_cont`): never throws; logs the resume command. */
  async function bestEffortCont(slot: number, why: string): Promise<void> {
    let resumeCmd = `<mh.sh> signal ${slot} CONT`;
    try {
      const args = signalArgs(slot, 'CONT', await owner());
      resumeCmd = commandLine(args);
      await execRaw(path(), cluster(), [...args, '--json']);
    } catch {
      // best effort: never throw from a backstop
    }
    console.error(
      `${why}; sent a best-effort CONT to MH slot ${slot}. If it is still stopped, resume it with: ${resumeCmd}`,
    );
  }

  return {
    owner,
    list,
    reclaimCommand,
    async up() {
      const o = await owner();
      const e = slotEntryFrom((await ok(['up', '--owner', o])).json, 'mh.sh up OK object');
      if (e.owner !== o)
        throw new Error('PRECONDITION (mh-sh-contract): up returned a slot not owned by this run');
      return e;
    },
    async drain(slot, { noWait = false } = {}) {
      const args = ['drain', String(slot), '--owner', await owner()];
      if (noWait) args.push('--no-wait');
      await ok(args);
    },
    async teardown() {
      const o = await owner();
      const r = await run(teardownArgs(o));
      if (r.status !== 'OK') {
        throw new Error(
          `TEARDOWN (mh-teardown-failed): owner ${o} not drained: REASON=${r.reason}. Reclaim manually: ${reclaimCommandFor(o)}`,
        );
      }
      const leaked = (await list()).slots.filter(
        (s) => s.pod !== null && (s.owner === o || s.terminating_past_grace),
      );
      if (leaked.length > 0) {
        throw new Error(
          `TEARDOWN (${MH_RUN_LEAK}: after teardown the pool still holds slot(s) ${leaked.map((s) => s.slot).join(', ')}). ` +
            `Reclaim manually: ${reclaimCommandFor(o)}`,
        );
      }
    },
    async kill(slot) {
      const before = await entry(slot);
      const out = await signal(slot, 'KILL');
      const ceilingMs = out.restartBudgetMs;
      if (ceilingMs === null) throw new Error('parseSignal requires a KILL restart budget');
      const deadline = now() + ceilingMs;
      for (;;) {
        const after = await entry(slot);
        if (restartedInPlace(before, after)) return after;
        if (now() >= deadline) {
          throw new Error(
            `${RESTART_NOT_OBSERVED}: slot ${slot} after KILL: pod_uid ${before.pod_uid} -> ${after.pod_uid}, ` +
              `restart_count ${before.restart_count} -> ${after.restart_count}, state ${after.state} ` +
              `(polled for ${ceilingMs} ms, mh.sh's restart_budget_seconds)`,
          );
        }
        await sleep(RESTART_POLL_MS);
      }
    },
    async freeze(slot) {
      const started = now(); // BEFORE the STOP call
      const before = await entry(slot);
      let out: SignalOutcome;
      try {
        out = await signal(slot, 'STOP');
      } catch (e) {
        // The SIGSTOP may have landed before the call failed: never leave it
        // frozen without a guard.
        await bestEffortCont(slot, 'the STOP call failed');
        throw e;
      }
      const stopCallMs = now() - started;
      const budgetMs = out.livenessBudgetMs;
      if (budgetMs === null) {
        await bestEffortCont(slot, 'the STOP answer had no budget');
        throw new Error('parseSignal requires a STOP liveness budget');
      }
      let resumed = false;
      const guard: FreezeGuard = {
        slot,
        budgetMs,
        stopCallMs,
        elapsedMs: () => now() - started,
        async resume() {
          await signal(slot, 'CONT'); // always sent; throws with the guard still armed
          resumed = true;
          const overrun = freezeOverrun(now() - started, budgetMs);
          const after = await entry(slot);
          if (overrun !== null) throw new Error(overrun);
          const changed = sameIncarnation(before, after);
          if (changed !== null) throw new Error(changed);
          return after;
        },
        async dispose() {
          if (resumed) return;
          resumed = true;
          await bestEffortCont(slot, 'FreezeGuard disposed while frozen');
        },
      };
      // A fresh post-STOP read: the same incarnation must still be there.
      let changed: string | null;
      try {
        changed = sameIncarnation(before, await entry(slot));
      } catch (e) {
        await guard.dispose();
        throw e;
      }
      if (changed !== null) {
        await guard.dispose();
        throw new Error(changed);
      }
      return guard;
    },
    async assertBaseline(ceilingMs) {
      const run = await owner();
      const deadline = now() + ceilingMs;
      let delay = 500;
      for (;;) {
        const verdict = baselineVerdict(await list(), run, reclaimCommandFor);
        if (verdict === null) return;
        if (now() >= deadline) throw new Error(`${verdict} (polled for ${ceilingMs} ms)`);
        await sleep(Math.min(delay, Math.max(deadline - now(), 0)));
        delay = Math.min(delay * 2, 5_000);
      }
    },
  };
}
