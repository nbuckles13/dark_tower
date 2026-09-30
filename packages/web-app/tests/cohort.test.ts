// Node-tier unit tests for the N+1 cohort + AC auth-budget logic
// (`e2e/cohort.ts`, story 2 R-30). Pure module, no E2E environment needed.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';

import {
  AC_REGISTRATION_RATE_LIMIT_MAX_KEY,
  AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY,
  assertCohortFitsAuthWindow,
  COHORT_ENV_VAR,
  cohortSize,
  cohortWindowSpend,
  decodeCohort,
  encodeCohort,
  parseAcAuthRateLimit,
  SUITE_RECEIVE_SLOTS,
} from '../e2e/cohort.js';

const AC_CONFIG_ENV = fileURLToPath(
  new URL('../../../infra/services/ac-service/config.env', import.meta.url),
);

const member = (i: number) => ({
  email: `e2e-test-${i}@darktower.test`,
  password: `synthetic-password-${i}`,
  displayName: `E2E cohort ${i}`,
});

describe('cohortSize', () => {
  test('is N+1', () => {
    expect(cohortSize(1)).toBe(2);
    expect(cohortSize(3)).toBe(4);
  });
  test.each([0, -1, 2.5, Number.NaN])('rejects N=%s', (n) => {
    expect(() => cohortSize(n)).toThrow(RangeError);
  });
  test('the suite is locked at N=3 (four participants)', () => {
    expect(SUITE_RECEIVE_SLOTS).toBe(3);
    expect(cohortSize(SUITE_RECEIVE_SLOTS)).toBe(4);
  });
});

describe('cohort env hand-off', () => {
  test('round-trips exactly the credential fields', () => {
    const cohort = [member(1), member(2), member(3), member(4)];
    expect(decodeCohort(encodeCohort(cohort), 4)).toEqual(cohort);
  });

  test('a missing variable names global setup', () => {
    expect(() => decodeCohort(undefined, 4)).toThrow(/global-setup\.ts registers the N\+1 cohort/);
    expect(() => decodeCohort('', 4)).toThrow(COHORT_ENV_VAR);
  });

  test('rejects malformed JSON and non-arrays', () => {
    expect(() => decodeCohort('{not json', 4)).toThrow(/not valid JSON/);
    expect(() => decodeCohort('{}', 4)).toThrow(/JSON array/);
  });

  test('rejects a cohort of the wrong size', () => {
    expect(() => decodeCohort(encodeCohort([member(1), member(2)]), 4)).toThrow(
      /carries 2 members, expected 4/,
    );
  });

  test('rejects a malformed member WITHOUT echoing its password', () => {
    const raw = JSON.stringify([member(1), { email: 'x@darktower.test', password: 'leak-me' }]);
    let message = '';
    try {
      decodeCohort(raw, 2);
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).toMatch(/member 1 is malformed/);
    expect(message).not.toContain('leak-me');
  });

  test('rejects two members sharing an account', () => {
    const dup = { ...member(2), email: member(1).email };
    expect(() => decodeCohort(encodeCohort([member(1), dup]), 2)).toThrow(/not distinct/);
  });
});

describe('parseAcAuthRateLimit', () => {
  const text = (max: string, win: string) =>
    `# comment ${AC_REGISTRATION_RATE_LIMIT_MAX_KEY}=999\n\n${AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY}=${win}\n${AC_REGISTRATION_RATE_LIMIT_MAX_KEY}=${max}\n`;

  test('reads both keys and ignores a key named only in a comment', () => {
    expect(parseAcAuthRateLimit(text('7', '2'), 'fixture')).toEqual({
      maxAttempts: 7,
      windowMinutes: 2,
    });
  });

  test('accepts a quoted value', () => {
    expect(parseAcAuthRateLimit(text('"7"', '1'), 'fixture').maxAttempts).toBe(7);
  });

  test('a missing key throws, naming the key and the file', () => {
    expect(() =>
      parseAcAuthRateLimit(`${AC_REGISTRATION_RATE_LIMIT_WINDOW_KEY}=1\n`, 'fixture.env'),
    ).toThrow(`fixture.env declares no ${AC_REGISTRATION_RATE_LIMIT_MAX_KEY}`);
  });

  test.each(['abc', '0', '-3', '1.5', ''])('a malformed value (%j) throws', (bad) => {
    expect(() => parseAcAuthRateLimit(text(bad, '1'), 'fixture')).toThrow(/not a positive integer/);
  });

  test('the real Kind AC config parses (the SSoT global setup reads)', () => {
    const limit = parseAcAuthRateLimit(readFileSync(AC_CONFIG_ENV, 'utf8'), AC_CONFIG_ENV);
    expect(limit.maxAttempts).toBeGreaterThanOrEqual(1);
    expect(limit.windowMinutes).toBeGreaterThanOrEqual(1);
  });
});

describe('assertCohortFitsAuthWindow', () => {
  test('the window spend is N+1 registrations plus N+1 first-test sign-ins', () => {
    expect(cohortWindowSpend(3)).toEqual({ registrations: 4, firstTestSignIns: 4, total: 8 });
  });

  test('the suite N fits the real Kind AC config', () => {
    const limit = parseAcAuthRateLimit(readFileSync(AC_CONFIG_ENV, 'utf8'), AC_CONFIG_ENV);
    expect(() => assertCohortFitsAuthWindow(SUITE_RECEIVE_SLOTS, limit)).not.toThrow();
  });

  test('passes at exactly the limit and fails one above it', () => {
    expect(() => assertCohortFitsAuthWindow(3, { maxAttempts: 8, windowMinutes: 1 })).not.toThrow();
    expect(() => assertCohortFitsAuthWindow(3, { maxAttempts: 7, windowMinutes: 1 })).toThrow(
      /needs 8 successful AC token issues .*\(4 registrations \+ 4 first-test sign-ins\)/,
    );
  });

  test('a prod-default-sized limit fails at setup, not mid-suite', () => {
    expect(() => assertCohortFitsAuthWindow(3, { maxAttempts: 5, windowMinutes: 60 })).toThrow(
      AC_REGISTRATION_RATE_LIMIT_MAX_KEY,
    );
  });
});

describe('the base AC config IS the value the Kind cluster runs', () => {
  // Global setup reads the BASE config.env. If a Kind overlay ever patched the
  // registration rate limit (the configmap-*-patch.yaml pattern), the cluster
  // would run a number the setup check never validated.
  const KIND_OVERLAY = fileURLToPath(
    new URL('../../../infra/kubernetes/overlays/kind/', import.meta.url),
  );
  const NEEDLE = 'AC_REGISTRATION_RATE_LIMIT_';

  function filesUnder(dir: string): string[] {
    return readdirSync(dir).flatMap((name) => {
      const path = join(dir, name);
      return statSync(path).isDirectory() ? filesUnder(path) : [path];
    });
  }

  test('no Kind overlay file patches AC_REGISTRATION_RATE_LIMIT_*', () => {
    const files = filesUnder(KIND_OVERLAY);
    expect(files.length, 'the overlay walk visited no files').toBeGreaterThan(0);
    const patching = files.filter((f) => readFileSync(f, 'utf8').includes(NEEDLE));
    expect(
      patching,
      'a Kind overlay patches the AC registration rate limit: global-setup.ts must read the ' +
        'overlay value (or the rendered ConfigMap) instead of the base config.env',
    ).toEqual([]);
  });

  test('positive control: the same scan hits the base config.env', () => {
    expect(readFileSync(AC_CONFIG_ENV, 'utf8').includes(NEEDLE)).toBe(true);
  });
});
