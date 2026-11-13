// File: packages/sdk-core/src/config/__tests__/handlerSetConfig.test.ts
//
// The handler-set reconcile knobs (story 4 C1, `MediaConfig.handlers`): each key
// at its inclusive bounds and one past them, the base <= cap relationship, and
// the ONE config log line. Bounds are read from `HANDLER_SET_CONFIG_RANGES` (the
// one home), never retyped.

import { describe, expect, it } from 'vitest';

import {
  ClientConfigError,
  DEFAULT_CLIENT_CONFIG,
  HANDLER_SET_CONFIG_RANGES,
  MH_KEEPALIVE_FLOOR_MS,
  handlerSetConfigLogLine,
  validateMediaConfig,
  type HandlerSetConfig,
  type MediaConfig,
} from '../clientConfig.js';

function withHandlers(overrides: Partial<HandlerSetConfig>): MediaConfig {
  // Keep base <= cap for the single-key range arms: widen the cap to its max
  // when testing the base, and the base to its min when testing the cap.
  return {
    ...DEFAULT_CLIENT_CONFIG.media,
    handlers: { ...DEFAULT_CLIENT_CONFIG.media.handlers, ...overrides },
  };
}

function rejection(config: MediaConfig): ClientConfigError | undefined {
  try {
    validateMediaConfig(config);
    return undefined;
  } catch (e) {
    return e as ClientConfigError;
  }
}

const KEYS = Object.keys(HANDLER_SET_CONFIG_RANGES) as (keyof HandlerSetConfig)[];

/** Overrides that keep the base <= cap relationship out of the way of a range arm. */
function neutral(key: keyof HandlerSetConfig): Partial<HandlerSetConfig> {
  const R = HANDLER_SET_CONFIG_RANGES;
  if (key === 'redialBaseMs') {
    return { redialCapMs: R.redialCapMs[1], reconnectAbandonMs: R.reconnectAbandonMs[1] };
  }
  if (key === 'redialCapMs') {
    return { redialBaseMs: R.redialBaseMs[0], reconnectAbandonMs: R.reconnectAbandonMs[1] };
  }
  if (key === 'reconnectingCapMs') return { resumeAbandonMs: R.resumeAbandonMs[1] };
  if (key === 'resumeAbandonMs') return { reconnectingCapMs: R.reconnectingCapMs[0] };
  if (key === 'reconnectAbandonMs') return { redialCapMs: R.redialCapMs[0] };
  return {};
}

describe('media.handlers — inclusive ranges, throw-never-clamp', () => {
  it('the shipped defaults validate', () => {
    expect(() => validateMediaConfig(DEFAULT_CLIENT_CONFIG.media)).not.toThrow();
  });

  it.each(KEYS)('%s: min and max accepted; min-1 and max+1 rejected, naming the key', (key) => {
    const [min, max] = HANDLER_SET_CONFIG_RANGES[key];
    expect(rejection(withHandlers({ ...neutral(key), [key]: min }))).toBeUndefined();
    expect(rejection(withHandlers({ ...neutral(key), [key]: max }))).toBeUndefined();
    for (const bad of [min - 1, max + 1, 1.5 + min, Number.NaN]) {
      const err = rejection(withHandlers({ ...neutral(key), [key]: bad }));
      expect(err).toBeInstanceOf(ClientConfigError);
      expect(err?.configKey).toBe(`media.handlers.${key}`);
    }
  });

  it('reconnectingCapMs > resumeAbandonMs is rejected; equal is accepted (story 4 C2)', () => {
    const err = rejection(withHandlers({ reconnectingCapMs: 40_000, resumeAbandonMs: 39_999 }));
    expect(err?.configKey).toBe('media.handlers.reconnectingCapMs');
    expect(
      rejection(withHandlers({ reconnectingCapMs: 40_000, resumeAbandonMs: 40_000 })),
    ).toBeUndefined();
  });

  it('removalGraceMs must be LESS than reconnectingCapMs (equal is rejected; one less accepted)', () => {
    const err = rejection(withHandlers({ reconnectingCapMs: 9_000, removalGraceMs: 9_000 }));
    expect(err?.configKey).toBe('media.handlers.removalGraceMs');
    expect(
      rejection(withHandlers({ reconnectingCapMs: 9_000, removalGraceMs: 8_999 })),
    ).toBeUndefined();
  });

  it('reconnectAbandonMs below one full redial window is rejected; equal is accepted', () => {
    const window = MH_KEEPALIVE_FLOOR_MS + 20_000;
    const err = rejection(withHandlers({ redialCapMs: 20_000, reconnectAbandonMs: window - 1 }));
    expect(err?.configKey).toBe('media.handlers.reconnectAbandonMs');
    expect(
      rejection(withHandlers({ redialCapMs: 20_000, reconnectAbandonMs: window })),
    ).toBeUndefined();
  });

  it('every pair of range extremes stays jointly satisfiable (the ranges cannot contradict the relations)', () => {
    const R = HANDLER_SET_CONFIG_RANGES;
    expect(R.reconnectAbandonMs[1]).toBeGreaterThanOrEqual(
      MH_KEEPALIVE_FLOOR_MS + R.redialCapMs[1],
    );
    expect(R.resumeAbandonMs[1]).toBeGreaterThanOrEqual(R.reconnectingCapMs[1]);
    expect(R.removalGraceMs[0]).toBeLessThan(R.reconnectingCapMs[0]);
  });

  it('the shipped caps sit inside the server replacement timeline arithmetic (defaults documented)', () => {
    const h = DEFAULT_CLIENT_CONFIG.media.handlers;
    expect(h.admissionWindowMs).toBe(2_000);
    expect(h.reconnectingCapMs).toBe(30_000);
    expect(h.removalGraceMs).toBe(2_000);
    expect(h.resumeAbandonMs).toBe(60_000);
    expect(h.reconnectAbandonMs).toBe(60_000);
  });

  it('redialBaseMs > redialCapMs is rejected; equal is accepted', () => {
    const err = rejection(withHandlers({ redialBaseMs: 5_000, redialCapMs: 4_999 }));
    expect(err?.configKey).toBe('media.handlers.redialBaseMs');
    expect(rejection(withHandlers({ redialBaseMs: 5_000, redialCapMs: 5_000 }))).toBeUndefined();
  });
});

describe('the [dt-media-handlers-config] line', () => {
  it('carries every key, its provenance and the compiled floor — numbers and fixed tokens only', () => {
    const h = DEFAULT_CLIENT_CONFIG.media.handlers;
    expect(handlerSetConfigLogLine(DEFAULT_CLIENT_CONFIG.media, 'default')).toBe(
      'media handler-set config (default): ' +
        `redialBaseMs=${h.redialBaseMs}; redialCapMs=${h.redialCapMs}; ` +
        `emptySetNoticeMs=${h.emptySetNoticeMs}; releasedGraceMs=${h.releasedGraceMs}; ` +
        `admissionWindowMs=${h.admissionWindowMs}; reconnectingCapMs=${h.reconnectingCapMs}; ` +
        `removalGraceMs=${h.removalGraceMs}; ` +
        `resumeAbandonMs=${h.resumeAbandonMs}; reconnectAbandonMs=${h.reconnectAbandonMs}; ` +
        `firstRedialFloorMs=${MH_KEEPALIVE_FLOOR_MS} (compiled)`,
    );
    expect(handlerSetConfigLogLine(withHandlers({ redialBaseMs: 250 }), 'configured')).toMatch(
      /^media handler-set config \(configured\): redialBaseMs=250; /,
    );
  });
});
