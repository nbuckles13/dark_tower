// File: packages/web-app/tests/testDefines.test.ts
//
// The ONE predicate behind every opt-in test define (`vite/testDefines.ts`):
// `__DT_TEST_TONE__` (story 2 R-7) and `__DT_TEST_LEVERS__` (story 2 task 15).
// Opt-in, exactly `1`, and a THROW in production — the accepted vocabulary is
// pinned so a looser parse cannot creep in on one define only (@test, @security).

import { describe, expect, it } from 'vitest';

import { TEST_LEVERS_ENV, TEST_TONE_ENV, resolveOptInTestDefine } from '../vite/testDefines.js';

describe.each([TEST_TONE_ENV, TEST_LEVERS_ENV])('resolveOptInTestDefine(%s)', (envName) => {
  it.each([
    ['development', undefined],
    ['development', ''],
    ['production', undefined],
    ['production', ''],
  ])('%s with the variable=%j is OFF — never default-on', (mode, raw) => {
    expect(resolveOptInTestDefine(envName, mode, raw)).toBe(false);
  });

  it('development with =1 is ON', () => {
    expect(resolveOptInTestDefine(envName, 'development', '1')).toBe(true);
  });

  it('production with =1 THROWS rather than coercing to off', () => {
    expect(() => resolveOptInTestDefine(envName, 'production', '1')).toThrow(
      `${envName}=1 in a production build`,
    );
  });

  it.each([['true'], ['yes'], ['0'], ['1 '], ['TRUE']])(
    'any other spelling (%j) THROWS in every mode',
    (raw) => {
      expect(() => resolveOptInTestDefine(envName, 'development', raw)).toThrow(/exactly "1"/);
      expect(() => resolveOptInTestDefine(envName, 'production', raw)).toThrow(/exactly "1"/);
    },
  );
});
