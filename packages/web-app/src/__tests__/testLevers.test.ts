// Story 2 task 15: the per-context test levers (`lib/testLevers.ts`). The
// non-vacuity guards for S10a live here (@security C2).

import { describe, expect, it } from 'vitest';
import type { IWebTransport, WebTransportConnectFn } from '@darktower/sdk-core';
import {
  blockingConnect,
  checkLeversApplied,
  parseTestLevers,
  readTestLevers,
} from '../lib/testLevers.js';

const U0 = 'https://mh-a.example:4433';
const U1 = 'https://mh-b.example:4433';

describe('parseTestLevers', () => {
  it('no global means no levers', () => {
    expect(parseTestLevers(undefined)).toBeUndefined();
  });

  it('parses every lever into a FROZEN copy that later writes cannot reach', () => {
    const raw = { blockHandlers: [U1], receiveSlots: '2', forceHostControls: true };
    const levers = parseTestLevers(raw)!;
    raw.blockHandlers.push(U0);
    expect(levers.blockHandlers).toEqual([U1]);
    expect(Object.isFrozen(levers)).toBe(true);
    expect(Object.isFrozen(levers.blockHandlers)).toBe(true);
    expect(levers.receiveSlots).toEqual({ count: 2, source: 'configured' });
    expect(levers.forceHostControls).toBe(true);
  });

  it('defaults absent keys to no-op values', () => {
    expect(parseTestLevers({})).toEqual({
      blockHandlers: [],
      receiveSlots: undefined,
      forceHostControls: false,
    });
  });

  it.each([
    ['a non-object', 'x', /must be an object/],
    ['an array', [], /must be an object/],
    ['null', null, /must be an object/],
    ['an unknown key', { blockHandler: [U1] }, /unknown key/],
    ['a non-array block list', { blockHandlers: U1 }, /array of non-empty strings/],
    ['an empty block entry', { blockHandlers: [''] }, /array of non-empty strings/],
    ['a non-string block entry', { blockHandlers: [1] }, /array of non-empty strings/],
    ['a numeric receiveSlots', { receiveSlots: 2 }, /receiveSlots must be a string/],
    ['a malformed receiveSlots', { receiveSlots: '02' }, /receive-slot count/],
    ['a non-boolean forceHostControls', { forceHostControls: 'yes' }, /boolean/],
  ])('THROWS on %s', (_label, raw, message) => {
    expect(() => parseTestLevers(raw)).toThrow(message);
  });
});

describe('readTestLevers', () => {
  it('is inert when the build define is off (every test runner)', () => {
    (window as { __dt_test_levers__?: unknown }).__dt_test_levers__ = { blockHandlers: [U0] };
    try {
      expect(readTestLevers()).toBeUndefined();
    } finally {
      delete (window as { __dt_test_levers__?: unknown }).__dt_test_levers__;
    }
  });
});

describe('blockingConnect', () => {
  const dialled: string[] = [];
  const base: WebTransportConnectFn = (url) => {
    dialled.push(url);
    return {} as IWebTransport;
  };

  it('refuses an exact-match blocked URL and passes every other URL through unchanged', () => {
    dialled.length = 0;
    const connect = blockingConnect(base, parseTestLevers({ blockHandlers: [U1] })!);
    expect(() => connect(U1)).toThrow(/dial refused/);
    connect(U0);
    connect(`${U1}/`); // not an exact match: the lever never normalises
    expect(dialled).toEqual([U0, `${U1}/`]);
  });

  it('is the identity when nothing is blocked', () => {
    expect(blockingConnect(base, parseTestLevers({})!)).toBe(base);
  });
});

describe('checkLeversApplied — the S10a non-vacuity guard', () => {
  it('accepts a block of one of two offered handlers', () => {
    expect(checkLeversApplied(parseTestLevers({ blockHandlers: [U1] })!, [U0, U1])).toBeUndefined();
  });

  it('refuses a blocked URL MC did not offer (the block would be vacuous)', () => {
    expect(
      checkLeversApplied(parseTestLevers({ blockHandlers: ['https://elsewhere:1'] })!, [U0, U1]),
    ).toMatch(/did not offer/);
  });

  it('refuses a block list that leaves ZERO offered handlers', () => {
    expect(checkLeversApplied(parseTestLevers({ blockHandlers: [U0, U1] })!, [U0, U1])).toMatch(
      /every offered handler/,
    );
  });

  it('has nothing to check with no block list', () => {
    expect(checkLeversApplied(parseTestLevers({})!, [U0])).toBeUndefined();
  });
});
