// File: packages/web-app/e2e/processCpu.ts
//
// Story 3 R-28 capacity EVIDENCE (task 21): process-level CPU per browsing
// context, read from Linux `/proc`. Used ONLY by the opt-in evidence run
// (`capacity.evidence.ts`, `playwright.capacity.config.ts`); never by a gating
// spec, and nothing here asserts a CPU number — CPU is host-dependent and is
// RECORDED, never judged.
//
// WHY A FINGERPRINT. Chromium gives every Playwright browsing context its own
// renderer process, but nothing (CDP target info, the renderer command line)
// names which context a renderer pid belongs to. So the harness makes the page
// busy-loop in-page for a fixed span WHILE IT IS IDLE (opened, not yet joined)
// and picks the renderer whose CPU delta stands out. No match, or more than one
// candidate, is a loud HARNESS error — never a guess. `assertMappingComplete`
// re-checks at window end that every renderer that burned CPU in the window is
// a mapped one (a renderer swap after the fingerprint would otherwise put a
// context's load on an unmapped pid and silently under-report it).
//
// The pure parts (stat parsing, the pick, the arithmetic) are driven by
// `tests/processCpu.test.ts`; the `/proc` reads are thin wrappers over them.

import { readdirSync, readFileSync } from 'node:fs';

/** Cumulative user+system CPU time of one process, in clock ticks. */
export interface ProcCpu {
  readonly pid: number;
  readonly ticks: number;
}

/**
 * The `/proc/<pid>/stat` fields AFTER the comm, i.e. from field 3 (state) on.
 * Split after the LAST `)`, because the comm (field 2) is parenthesised and may
 * itself contain spaces or parentheses. No `)` at all is a HARNESS error.
 */
export function statFieldsAfterComm(stat: string): string[] {
  const close = stat.lastIndexOf(')');
  if (close < 0) throw new Error(`HARNESS: /proc stat has no ')' after comm: ${stat.slice(0, 80)}`);
  return stat
    .slice(close + 1)
    .trim()
    .split(/\s+/);
}

/** Parse `/proc/<pid>/stat` -> utime + stime (fields 14 and 15, 1-based). */
export function parseStatTicks(stat: string): number {
  const rest = statFieldsAfterComm(stat);
  // rest[0] is field 3, so utime (field 14) is rest[11] and stime (15) is rest[12].
  const utime = Number(rest[11]);
  const stime = Number(rest[12]);
  if (!Number.isInteger(utime) || !Number.isInteger(stime) || utime < 0 || stime < 0) {
    throw new Error(`HARNESS: /proc stat utime/stime unparsable: ${stat.slice(0, 120)}`);
  }
  return utime + stime;
}

/**
 * Chromium process type from its command line (`browser` when there is no
 * `--type=`). Chromium REWRITES its child processes' titles, so a child's
 * `/proc/<pid>/cmdline` is one space-joined string rather than NUL-separated
 * argv: the flag is matched on either separator.
 */
export function chromiumProcessType(cmdline: string): string {
  const m = /(?:^|[\s\0])--type=([^\s\0]+)/.exec(cmdline);
  return m === null ? 'browser' : m[1]!;
}

/** `/proc/<pid>/stat` -> parent pid (field 4). */
export function parseStatPpid(stat: string): number {
  const ppid = Number(statFieldsAfterComm(stat)[1]);
  if (!Number.isInteger(ppid) || ppid < 0) {
    throw new Error(`HARNESS: /proc stat ppid unparsable: ${stat.slice(0, 120)}`);
  }
  return ppid;
}

/** One process of the browser tree, with its Chromium type and CPU ticks. */
export interface ChromiumProcess extends ProcCpu {
  readonly type: string;
}

/** A raw `/proc` row before the tree walk. */
export interface ProcRow {
  readonly pid: number;
  readonly ppid: number;
  readonly cmdline: string;
  readonly ticks: number;
}

/**
 * The processes DESCENDED from `rootPid` (exclusive). The Playwright worker
 * launches the browser as its child, so from the worker's pid this is exactly
 * this run's browser tree (browser, zygotes, renderers, GPU, utility) — never
 * the Vite server, which the runner's main process starts.
 */
export function descendantsOf(rows: readonly ProcRow[], rootPid: number): ChromiumProcess[] {
  const children = new Map<number, ProcRow[]>();
  for (const r of rows) {
    const siblings = children.get(r.ppid);
    if (siblings === undefined) children.set(r.ppid, [r]);
    else siblings.push(r);
  }
  const out: ChromiumProcess[] = [];
  const stack = [...(children.get(rootPid) ?? [])];
  while (stack.length > 0) {
    const r = stack.pop()!;
    out.push({ pid: r.pid, type: chromiumProcessType(r.cmdline), ticks: r.ticks });
    stack.push(...(children.get(r.pid) ?? []));
  }
  return out;
}

/** errno codes of the readdir-then-read race: the process exited in between. */
const EXITED_MID_SCAN = new Set(['ENOENT', 'ESRCH']);

/**
 * Read every live `/proc/<pid>` row. ONLY a process that exits mid-scan is
 * skipped; any other read error (EACCES, EMFILE, ...) would silently drop
 * processes and under-report CPU, so it is rethrown.
 */
export function readProcRows(): ProcRow[] {
  const out: ProcRow[] = [];
  for (const entry of readdirSync('/proc')) {
    if (!/^\d+$/.test(entry)) continue;
    try {
      const stat = readFileSync(`/proc/${entry}/stat`, 'utf8');
      const cmdline = readFileSync(`/proc/${entry}/cmdline`, 'utf8');
      out.push({
        pid: Number(entry),
        ppid: parseStatPpid(stat),
        cmdline,
        ticks: parseStatTicks(stat),
      });
    } catch (e) {
      const code = (e as NodeJS.ErrnoException | undefined)?.code;
      if (code !== undefined && EXITED_MID_SCAN.has(code)) continue;
      throw e;
    }
  }
  return out;
}

/** This worker's browser tree, now. */
export function browserTree(): ChromiumProcess[] {
  return descendantsOf(readProcRows(), process.pid);
}

/** Ticks per pid. */
export function ticksByPid(procs: readonly ProcCpu[]): Map<number, number> {
  return new Map(procs.map((p) => [p.pid, p.ticks]));
}

/** Per-pid tick delta between two snapshots (a pid absent from `before` counts from 0). */
export function deltaTicks(
  before: ReadonlyMap<number, number>,
  after: ReadonlyMap<number, number>,
): Map<number, number> {
  const out = new Map<number, number>();
  for (const [pid, t] of after) out.set(pid, t - (before.get(pid) ?? 0));
  return out;
}

/**
 * The fingerprint pick: the ONE renderer whose delta reaches `floorTicks` while
 * every other renderer stays below `floorTicks / 2`. Anything else throws.
 */
export function pickFingerprintedPid(
  deltas: ReadonlyMap<number, number>,
  floorTicks: number,
  label: string,
): number {
  const hot = [...deltas].filter(([, d]) => d >= floorTicks);
  const warm = [...deltas].filter(([, d]) => d >= floorTicks / 2 && d < floorTicks);
  if (hot.length !== 1 || warm.length !== 0) {
    throw new Error(
      `HARNESS: ${label}: renderer fingerprint is ambiguous or absent — ` +
        `${hot.length} renderer(s) at or above ${floorTicks} ticks, ${warm.length} near it ` +
        `(deltas ${JSON.stringify([...deltas])}). Never guessed; re-run on a quieter host.`,
    );
  }
  return hot[0]![0];
}

/** Percent of ONE core over `elapsedMs` for `ticks` at `ticksPerSecond`. */
export function percentOfCore(ticks: number, elapsedMs: number, ticksPerSecond: number): number {
  if (elapsedMs <= 0) throw new RangeError('elapsedMs must be > 0');
  return Math.round(((ticks / ticksPerSecond) * 1000 * 100 * 10) / elapsedMs) / 10;
}

/**
 * Every renderer that burned at least `floorTicks` in the window must be a
 * mapped one; otherwise a context's load sits on an unmapped pid.
 */
export function assertMappingComplete(
  rendererDeltas: ReadonlyMap<number, number>,
  mapped: ReadonlySet<number>,
  floorTicks: number,
): void {
  const unmapped = [...rendererDeltas].filter(([pid, d]) => d >= floorTicks && !mapped.has(pid));
  if (unmapped.length > 0) {
    throw new Error(
      `HARNESS: renderer(s) ${JSON.stringify(unmapped)} burned CPU in the window but map to no ` +
        `browsing context (a renderer swap after the fingerprint?). The per-context CPU would be ` +
        `under-reported, so the evidence is refused.`,
    );
  }
}
