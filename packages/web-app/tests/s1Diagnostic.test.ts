// Node-tier unit tests for the story-2 S1 diagnostic (`e2e/s1Diagnostic.ts`):
// the budget-rejection / misrouting / unobservable classification, and a drift
// check of the MH metric names against their SSoT, the Rust recording sites.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import {
  classifyMissingSender,
  MH_ADMISSION_REJECTED_OUTCOME,
  MH_ADMISSION_REJECTION_RATIO,
  MH_ADMISSION_TOTAL,
  MH_EGRESS_EDGES,
  MH_EGRESS_STREAM_CEILING,
  MH_SCRAPE_JOB,
  type AdmissionEvidence,
} from '../e2e/s1Diagnostic.js';

const MH_METRICS_RS = fileURLToPath(
  new URL('../../../crates/mh-service/src/observability/metrics.rs', import.meta.url),
);

const map = (entries: Record<string, number>) => new Map(Object.entries(entries));

function evidence(
  baseline: Record<string, number>,
  current: Record<string, number>,
  up: Record<string, number> = Object.fromEntries(Object.keys(current).map((k) => [k, 1])),
): AdmissionEvidence {
  return {
    scrapeUp: map(up),
    baselineRejected: map(baseline),
    currentRejected: map(current),
    rejectionRatio: map({ '10.0.0.5:8083': 0.25 }),
    streamCeiling: map({ '10.0.0.5:8083': 8 }),
    egressEdges: map({ '10.0.0.5:8083': 8 }),
  };
}

describe('classifyMissingSender', () => {
  test('a rejection counted during the scenario is a config-caused budget rejection', () => {
    const d = classifyMissingSender(
      'C missing B',
      evidence(
        { '10.0.0.5:8083': 0, '10.0.0.6:8083': 2 },
        { '10.0.0.5:8083': 1, '10.0.0.6:8083': 2 },
      ),
    );
    expect(d.verdict).toBe('config_budget_rejection');
    expect(d.report).toContain('C missing B');
    expect(d.report).toContain('MH_EGRESS_BUDGET_BPS');
    // The context gauges ride along for triage.
    expect(d.report).toContain(`${MH_EGRESS_STREAM_CEILING} {10.0.0.5:8083=8}`);
    expect(d.report).toContain(`${MH_EGRESS_EDGES} {10.0.0.5:8083=8}`);
    expect(d.report).toContain(`${MH_ADMISSION_REJECTION_RATIO} {10.0.0.5:8083=0.25}`);
  });

  test('no instance moved, series present: misrouting', () => {
    const d = classifyMissingSender(
      'C missing B',
      evidence({ '10.0.0.5:8083': 3 }, { '10.0.0.5:8083': 3 }),
    );
    expect(d.verdict).toBe('misrouting');
    expect(d.report).toMatch(/MISROUTING/);
  });

  test('rejections that PREDATE the baseline do not make it a budget rejection', () => {
    // A non-zero lifetime counter (and ratio) from an earlier scenario is not
    // this scenario's evidence; only the in-window delta decides.
    const d = classifyMissingSender('x', evidence({ '10.0.0.5:8083': 9 }, { '10.0.0.5:8083': 9 }));
    expect(d.verdict).toBe('misrouting');
  });

  test('an MH pod that rolled mid-scenario and rejected on the fresh instance counts', () => {
    const d = classifyMissingSender('x', evidence({ '10.0.0.5:8083': 4 }, { '10.0.0.9:8083': 1 }));
    expect(d.verdict).toBe('config_budget_rejection');
  });

  test('an EMPTY current read is unobservable, never misrouting', () => {
    const d = classifyMissingSender('x', evidence({ '10.0.0.5:8083': 0 }, {}));
    expect(d.verdict).toBe('unobservable');
    expect(d.report).toMatch(/CURRENT read .* not being scraped/);
  });

  test('an EMPTY baseline is unobservable even when the lifetime counter is non-zero', () => {
    // Layer 7's Rust env-tests drive rejections on these pods first; against an
    // empty baseline that pre-existing count would read as an in-window rejection.
    const d = classifyMissingSender('x', evidence({}, { '10.0.0.5:8083': 6 }));
    expect(d.verdict).toBe('unobservable');
    expect(d.report).toMatch(/BASELINE read .* before the scenario's joins/);
  });

  test('a baseline instance missing now (partial scrape) is unobservable, naming it', () => {
    const d = classifyMissingSender(
      'x',
      evidence({ '10.0.0.5:8083': 0, '10.0.0.6:8083': 0 }, { '10.0.0.5:8083': 0 }),
    );
    expect(d.verdict).toBe('unobservable');
    expect(d.report).toContain('10.0.0.6:8083');
  });

  test('a rollover with no rejection on the fresh pod is unobservable, not misrouting', () => {
    // The old pod vanished: it may have rejected before it went.
    const d = classifyMissingSender('x', evidence({ '10.0.0.5:8083': 4 }, { '10.0.0.9:8083': 0 }));
    expect(d.verdict).toBe('unobservable');
  });

  test('one MH target down with no delta on the other is unobservable, not misrouting', () => {
    const d = classifyMissingSender(
      'x',
      evidence(
        { '10.0.0.5:8083': 0 },
        { '10.0.0.5:8083': 0 },
        { '10.0.0.5:8083': 1, '10.0.0.6:8083': 0 },
      ),
    );
    expect(d.verdict).toBe('unobservable');
    expect(d.report).toContain('10.0.0.6:8083 are down');
  });

  test('no up series at all is unobservable', () => {
    const d = classifyMissingSender('x', evidence({ a: 0 }, { a: 0 }, {}));
    expect(d.verdict).toBe('unobservable');
  });

  test('a delta seen on an up instance wins even while another target is down', () => {
    const d = classifyMissingSender('x', evidence({ a: 0 }, { a: 1 }, { a: 1, b: 0 }));
    expect(d.verdict).toBe('config_budget_rejection');
  });

  test('all targets up, every baseline instance visible, no delta: misrouting', () => {
    const d = classifyMissingSender('x', evidence({ a: 0, b: 0 }, { a: 0, b: 0 }, { a: 1, b: 1 }));
    expect(d.verdict).toBe('misrouting');
  });

  test('a NEW instance next to a fully-visible baseline does not block misrouting', () => {
    const d = classifyMissingSender(
      'x',
      evidence({ '10.0.0.5:8083': 2 }, { '10.0.0.5:8083': 2, '10.0.0.9:8083': 0 }),
    );
    expect(d.verdict).toBe('misrouting');
  });
});

describe('metric names match their SSoT (crates/mh-service/src/observability/metrics.rs)', () => {
  const source = readFileSync(MH_METRICS_RS, 'utf8');
  const declared = (name: string): boolean => source.includes(`"${name}"`);

  test.each([
    MH_ADMISSION_TOTAL,
    MH_ADMISSION_REJECTED_OUTCOME,
    MH_ADMISSION_REJECTION_RATIO,
    MH_EGRESS_STREAM_CEILING,
    MH_EGRESS_EDGES,
  ])('%s is a string literal at its Rust recording site', (name) => {
    expect(declared(name)).toBe(true);
  });

  test('positive control: the check fails for a name the Rust does not declare', () => {
    expect(declared(`${MH_ADMISSION_TOTAL}_renamed`)).toBe(false);
  });
});

describe('the MH scrape job name matches its SSoT (prometheus.yml)', () => {
  const yml = readFileSync(
    fileURLToPath(
      new URL('../../../infra/kubernetes/observability/prometheus.yml', import.meta.url),
    ),
    'utf8',
  );
  const declaresJob = (job: string): boolean => yml.includes(`job_name: '${job}'`);

  test(`job_name: '${MH_SCRAPE_JOB}' is declared`, () => {
    expect(declaresJob(MH_SCRAPE_JOB)).toBe(true);
  });

  test('positive control: an undeclared job name is not found', () => {
    expect(declaresJob(`${MH_SCRAPE_JOB}-renamed`)).toBe(false);
  });
});
