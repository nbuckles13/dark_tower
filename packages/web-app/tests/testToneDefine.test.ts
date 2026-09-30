// File: packages/web-app/tests/testToneDefine.test.ts
//
// Story 2 R-7: the ONE predicate behind `__DT_TEST_TONE__` (`vite/testTone.ts`).
// Opt-in, exactly `1`, and a THROW in production — the accepted vocabulary is
// pinned so a looser parse cannot creep in on one side only (@test, @security).

import { describe, expect, it } from 'vitest';

import { resolveTestTone } from '../vite/testTone.js';

describe('resolveTestTone', () => {
  it.each([
    ['development', undefined],
    ['development', ''],
    ['production', undefined],
    ['production', ''],
  ])('%s with DT_TEST_TONE=%j is OFF — never default-on', (mode, raw) => {
    expect(resolveTestTone(mode, raw)).toBe(false);
  });

  it('development with DT_TEST_TONE=1 is ON', () => {
    expect(resolveTestTone('development', '1')).toBe(true);
  });

  it('production with DT_TEST_TONE=1 THROWS rather than coercing to off', () => {
    expect(() => resolveTestTone('production', '1')).toThrow(/production/);
  });

  it.each([['true'], ['yes'], ['0'], ['1 '], ['TRUE']])(
    'any other spelling (%j) THROWS in every mode',
    (raw) => {
      expect(() => resolveTestTone('development', raw)).toThrow(/exactly "1"/);
      expect(() => resolveTestTone('production', raw)).toThrow(/exactly "1"/);
    },
  );
});
