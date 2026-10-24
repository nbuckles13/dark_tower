// Node-tier unit tests for `e2e/processCpu.ts`, the story 3 R-28 capacity
// evidence's process-level CPU reader: `/proc/<pid>/stat` parsing (incl. a comm
// that contains spaces and parentheses), the browser-tree walk, the renderer
// fingerprint pick (unique / ambiguous / absent — never a guess), the
// mapping-completeness refusal, and the percent arithmetic. Pure inputs only.

import { describe, expect, test } from 'vitest';

import {
  assertMappingComplete,
  chromiumProcessType,
  deltaTicks,
  descendantsOf,
  parseStatPpid,
  parseStatTicks,
  percentOfCore,
  pickFingerprintedPid,
  type ProcRow,
} from '../e2e/processCpu.js';

// Fields after comm: state ppid pgrp session tty tpgid flags minflt cminflt majflt cmajflt utime stime ...
const stat = (comm: string, ppid: number, utime: number, stime: number) =>
  `4242 (${comm}) S ${ppid} 1 1 0 -1 4194560 10 0 0 0 ${utime} ${stime} 0 0 20 0 9 0 100 0 0`;

describe('parseStatTicks / parseStatPpid', () => {
  test('utime + stime, and the parent pid', () => {
    expect(parseStatTicks(stat('chrome', 7, 120, 30))).toBe(150);
    expect(parseStatPpid(stat('chrome', 7, 120, 30))).toBe(7);
  });

  test('a comm with spaces and parentheses cannot shift the fields', () => {
    const s = stat('ev il) 1 2 (x', 9, 5, 6);
    expect(parseStatTicks(s)).toBe(11);
    expect(parseStatPpid(s)).toBe(9);
  });

  test('unparsable input fails loudly as a HARNESS error', () => {
    expect(() => parseStatTicks('4242 chrome S 1')).toThrow(/HARNESS/);
    expect(() => parseStatTicks('4242 (chrome) S 1 2')).toThrow(/HARNESS/);
    expect(() => parseStatPpid('4242 chrome S 1')).toThrow(/HARNESS: \/proc stat has no '\)'/);
  });
});

describe('chromiumProcessType', () => {
  test('reads --type=, and a process without one is the browser', () => {
    expect(chromiumProcessType('/x/chrome\0--type=renderer\0--foo')).toBe('renderer');
    expect(chromiumProcessType('/x/chrome\0--type=gpu-process')).toBe('gpu-process');
    expect(chromiumProcessType('/x/chrome\0--remote-debugging-pipe')).toBe('browser');
  });
  test('a REWRITTEN child title (space-joined, no NULs) still yields its type', () => {
    expect(chromiumProcessType('/x/chrome-headless-shell --type=renderer --no-sandbox')).toBe(
      'renderer',
    );
    expect(chromiumProcessType('/x/chrome --no-sandbox --headless')).toBe('browser');
    // a flag that merely ENDS in "--type=" text is not the type flag
    expect(chromiumProcessType('/x/chrome --foo--type=renderer')).toBe('browser');
  });
});

describe('descendantsOf — the worker-rooted browser tree', () => {
  const row = (pid: number, ppid: number, type?: string): ProcRow => ({
    pid,
    ppid,
    cmdline: type === undefined ? '/x/chrome' : `/x/chrome\0--type=${type}`,
    ticks: pid,
  });
  test('walks every generation below the root and nothing beside it', () => {
    const rows = [
      row(10, 1), // the worker
      row(11, 10), // browser
      row(12, 11, 'zygote'),
      row(13, 12, 'renderer'), // grandchild via the zygote
      row(14, 11, 'gpu-process'),
      row(20, 1), // the Vite server: a sibling, never included
      row(21, 20, 'renderer'),
    ];
    const tree = descendantsOf(rows, 10);
    expect(tree.map((p) => p.pid).sort()).toEqual([11, 12, 13, 14]);
    expect(tree.find((p) => p.pid === 13)?.type).toBe('renderer');
  });
});

describe('pickFingerprintedPid — never a guess', () => {
  test('exactly one renderer clears the floor and none is near it: that one', () => {
    expect(
      pickFingerprintedPid(
        new Map([
          [1, 140],
          [2, 3],
          [3, 10],
        ]),
        90,
        'A',
      ),
    ).toBe(1);
  });
  test('two at the floor: ambiguous, refused', () => {
    expect(() =>
      pickFingerprintedPid(
        new Map([
          [1, 140],
          [2, 95],
        ]),
        90,
        'A',
      ),
    ).toThrow(/HARNESS: A: renderer fingerprint is ambiguous/);
  });
  test('one hot and one near the floor: ambiguous, refused', () => {
    expect(() =>
      pickFingerprintedPid(
        new Map([
          [1, 140],
          [2, 50],
        ]),
        90,
        'A',
      ),
    ).toThrow(/HARNESS/);
  });
  test('none at the floor: absent, refused', () => {
    expect(() => pickFingerprintedPid(new Map([[1, 10]]), 90, 'A')).toThrow(/HARNESS/);
  });
});

describe('deltaTicks / percentOfCore / assertMappingComplete', () => {
  test('a pid new in the window counts from zero', () => {
    expect([
      ...deltaTicks(
        new Map([[1, 100]]),
        new Map([
          [1, 130],
          [2, 40],
        ]),
      ),
    ]).toEqual([
      [1, 30],
      [2, 40],
    ]);
  });
  test('percent of one core, one decimal', () => {
    expect(percentOfCore(150, 30_000, 100)).toBe(5);
    expect(percentOfCore(3_000, 30_000, 100)).toBe(100);
    expect(percentOfCore(1, 3_000, 100)).toBe(0.3);
    expect(() => percentOfCore(1, 0, 100)).toThrow(RangeError);
  });
  test('an unmapped renderer that burned CPU refuses the evidence; a quiet one does not', () => {
    expect(() =>
      assertMappingComplete(
        new Map([
          [1, 500],
          [9, 200],
        ]),
        new Set([1]),
        150,
      ),
    ).toThrow(/HARNESS: renderer\(s\) \[\[9,200\]\]/);
    expect(() =>
      assertMappingComplete(
        new Map([
          [1, 500],
          [9, 20],
        ]),
        new Set([1]),
        150,
      ),
    ).not.toThrow();
  });
});
