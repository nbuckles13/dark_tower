// Story 3 R-20: the uplink relay stream_id is MC's send id, from ONE function.

import { describe, expect, it } from 'vitest';

import { uplinkRelayStreamId } from '../sendId.js';

describe('uplinkRelayStreamId', () => {
  it.each([0, 1, 7, 255])('returns the directed stream_number %i verbatim', (n) => {
    expect(uplinkRelayStreamId({ streamNumber: n })).toBe(n);
  });

  it.each([256, -1, 1.5, Number.NaN])('refuses %s outside the 8-bit key-id stream domain', (n) => {
    expect(uplinkRelayStreamId({ streamNumber: n })).toBeUndefined();
  });
});
