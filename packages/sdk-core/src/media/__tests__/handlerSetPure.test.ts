// File: packages/sdk-core/src/media/__tests__/handlerSetPure.test.ts
//
// The three pure rules behind the handler-set reconcile (ADR-0039 §4, §7):
// the ref-only diff, the full-jitter redial delay, and the close-code
// disposition. Each table is derived from its source of truth (the generated
// enum, the config), never retyped.

import { describe, expect, it } from 'vitest';

import { diffHandlerSet, handlerRef } from '../handlerSet.js';
import { redialDelayMs } from '../redialBackoff.js';
import { classifyClose, type CloseDisposition } from '../closeCode.js';
import { MhCloseCode } from '../../proto/dark_tower/signaling/v1/signaling_pb.js';
import { DEFAULT_CLIENT_CONFIG, MH_KEEPALIVE_FLOOR_MS } from '../../config/clientConfig.js';

const r = handlerRef;

describe('diffHandlerSet — keyed on handler_ref ONLY', () => {
  it('splits added / removed / retained, in the next set order', () => {
    const d = diffHandlerSet([r(1), r(2), r(3)], [r(4), r(2), r(1)]);
    expect(d.added).toEqual([r(4)]);
    expect(d.removed).toEqual([r(3)]);
    expect(d.retained).toEqual([r(2), r(1)]);
  });

  it('identical refs are a token-only refresh: nothing added, nothing removed', () => {
    const d = diffHandlerSet([r(1), r(2)], [r(1), r(2)]);
    expect(d.added).toEqual([]);
    expect(d.removed).toEqual([]);
  });

  it('a first set adds everything; an empty set removes everything', () => {
    expect(diffHandlerSet([], [r(1)]).added).toEqual([r(1)]);
    expect(diffHandlerSet([r(1)], []).removed).toEqual([r(1)]);
  });
});

describe('redialDelayMs — full jitter, first retry floored', () => {
  const base = { baseMs: 1_000, capMs: 30_000, floorMs: MH_KEEPALIVE_FLOOR_MS };

  it('random()=0: the FIRST retry is never earlier than the keepalive floor; later ones may be 0', () => {
    const zero = { ...base, random: () => 0 };
    expect(redialDelayMs(0, zero)).toBe(MH_KEEPALIVE_FLOOR_MS);
    expect(redialDelayMs(1, zero)).toBe(0);
  });

  it('the first retry keeps its jitter ABOVE the floor (not max(floor, jitter))', () => {
    const half = { ...base, random: () => 0.5 };
    expect(redialDelayMs(0, half)).toBe(MH_KEEPALIVE_FLOOR_MS + 500);
  });

  it('uniform in [0, min(cap, base * 2^n)]: the window doubles, then saturates at the cap', () => {
    const top = { ...base, random: () => 0.999_999 };
    expect(redialDelayMs(1, top)).toBe(1_999);
    expect(redialDelayMs(3, top)).toBe(7_999);
    // 2^5 * 1000 = 32000 > cap: the window is the cap.
    expect(redialDelayMs(5, top)).toBe(29_999);
    expect(redialDelayMs(60, top)).toBe(29_999);
    expect(redialDelayMs(5000, top)).toBe(29_999);
  });

  it('the defaults are consistent with the rule (base <= cap)', () => {
    const { redialBaseMs, redialCapMs } = DEFAULT_CLIENT_CONFIG.media.handlers;
    expect(redialBaseMs).toBeLessThanOrEqual(redialCapMs);
  });
});

describe('classifyClose — every MhCloseCode, derived from the generated enum', () => {
  const EXPECTED: Record<keyof typeof MhCloseCode, CloseDisposition> = {
    UNSPECIFIED: 'contract_fault',
    WRONG_HANDLER: 'retry',
    BINDING_CONFLICT: 'retry',
    MC_UNAVAILABLE: 'retry',
    HANDLER_RELEASED: 'released',
    DECLINED: 'declined',
  };

  it('the expectation table covers EXACTLY the generated enum (a new code fails here)', () => {
    const generated = Object.keys(MhCloseCode).filter((k) => Number.isNaN(Number(k)));
    expect(generated.sort()).toEqual(Object.keys(EXPECTED).sort());
  });

  it.each(Object.entries(EXPECTED))('%s -> %s', (name, disposition) => {
    const code = MhCloseCode[name as keyof typeof MhCloseCode];
    expect(classifyClose(code)).toBe(disposition);
  });

  it('no code (a lost session) is loss; an unknown code is a contract fault', () => {
    expect(classifyClose(undefined)).toBe('loss');
    expect(classifyClose(9999)).toBe('contract_fault');
    expect(classifyClose(-1)).toBe('contract_fault');
  });
});
