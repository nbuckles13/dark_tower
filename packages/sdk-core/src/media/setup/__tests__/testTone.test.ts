// File: packages/sdk-core/src/media/setup/__tests__/testTone.test.ts
//
// Story 2 R-7 tone derivation. The expectations are LITERAL vectors written from
// the documented formula (`f = 600 + ((id - 1) mod 24) * 25` Hz), NOT computed by
// calling the function — a test that recomputed them would pass any formula.

import { describe, expect, it } from 'vitest';

import {
  TEST_TONE_BAND_END_HZ,
  TEST_TONE_BAND_START_HZ,
  testToneFrequencyHz,
} from '../testTone.js';

describe('testToneFrequencyHz', () => {
  it.each([
    [1, 600],
    [2, 625],
    [3, 650],
    [24, 1175],
    [25, 600],
    [26, 625],
    [48, 1175],
    [258, 1025],
    [65_535, 950],
  ])('sender %i -> %i Hz', (senderId, hz) => {
    expect(testToneFrequencyHz(senderId)).toBe(hz);
  });

  it('keeps every tone inside a band narrower than one octave', () => {
    expect(TEST_TONE_BAND_START_HZ).toBe(600);
    expect(TEST_TONE_BAND_END_HZ).toBe(1175);
    expect(TEST_TONE_BAND_END_HZ).toBeLessThan(2 * TEST_TONE_BAND_START_HZ);
    for (let id = 1; id <= 100; id += 1) {
      const hz = testToneFrequencyHz(id);
      expect(hz).toBeGreaterThanOrEqual(600);
      expect(hz).toBeLessThanOrEqual(1175);
    }
  });

  it('sender ids 24 apart share a tone (documented, detectable collision)', () => {
    expect(testToneFrequencyHz(5)).toBe(testToneFrequencyHz(29));
  });

  it.each([[0], [-1], [1.5], [65_536], [Number.NaN]])(
    'REFUSES %s — no tone without an assigned sender id, and no default',
    (senderId) => {
      expect(() => testToneFrequencyHz(senderId)).toThrow(RangeError);
    },
  );
});
