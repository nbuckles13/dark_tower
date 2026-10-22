// File: packages/web-app/src/__tests__/receiveSlotsConfig.test.ts
//
// Story 3 R-1 / R-23: `VITE_DT_AUDIO_RECEIVE_SLOTS` / `VITE_DT_VIDEO_RECEIVE_SLOTS`
// are read through the SDK's single parser at config load. Absent -> the SDK
// default, REPORTED as the default; present-but-malformed -> THROW at load
// (never a silent fallback); 0 is legal. The retired `VITE_DT_RECEIVE_SLOTS`
// FAILS LOUDLY if set, naming both replacements (no alias). Same input classes
// `scripts/dev-web.sh` refuses.

import { describe, expect, it } from 'vitest';
import {
  ClientConfigError,
  DEFAULT_RECEIVE_AUDIO_SLOTS,
  DEFAULT_RECEIVE_VIDEO_SLOTS,
  RETIRED_RECEIVE_SLOTS_MESSAGE,
} from '@darktower/sdk-core';

import { loadConfig } from '../lib/config.js';

const AUDIO = 'VITE_DT_AUDIO_RECEIVE_SLOTS';
const VIDEO = 'VITE_DT_VIDEO_RECEIVE_SLOTS';
const RETIRED = 'VITE_DT_RECEIVE_SLOTS';

function withEnv<T>(values: Record<string, string | undefined>, run: () => T): T {
  const env = import.meta.env as unknown as Record<string, unknown>;
  const saved = Object.fromEntries(Object.keys(values).map((k) => [k, env[k]]));
  for (const [k, v] of Object.entries(values)) {
    if (v === undefined) delete env[k];
    else env[k] = v;
  }
  try {
    return run();
  } finally {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) delete env[k];
      else env[k] = v;
    }
  }
}

describe('per-kind receive-slot variables', () => {
  it('absent: the SDK defaults, marked source "default"', () => {
    withEnv({ [AUDIO]: undefined, [VIDEO]: undefined, [RETIRED]: undefined }, () => {
      const config = loadConfig();
      expect(config.audioReceiveSlots).toEqual({
        count: DEFAULT_RECEIVE_AUDIO_SLOTS,
        source: 'default',
      });
      expect(config.videoReceiveSlots).toEqual({
        count: DEFAULT_RECEIVE_VIDEO_SLOTS,
        source: 'default',
      });
    });
  });

  it('present: the configured N and M, 0 included', () => {
    withEnv({ [AUDIO]: '3', [VIDEO]: '0', [RETIRED]: undefined }, () => {
      const config = loadConfig();
      expect(config.audioReceiveSlots).toEqual({ count: 3, source: 'configured' });
      expect(config.videoReceiveSlots).toEqual({ count: 0, source: 'configured' });
    });
  });

  it.each([[''], ['03'], ['-2'], ['3.0'], [' 3'], ['abc']])(
    'malformed %j THROWS at config load, for either kind',
    (raw) => {
      withEnv({ [AUDIO]: raw, [VIDEO]: undefined, [RETIRED]: undefined }, () => {
        expect(() => loadConfig()).toThrow(ClientConfigError);
      });
      withEnv({ [AUDIO]: undefined, [VIDEO]: raw, [RETIRED]: undefined }, () => {
        expect(() => loadConfig()).toThrow(ClientConfigError);
      });
    },
  );

  it('the RETIRED VITE_DT_RECEIVE_SLOTS fails loudly, naming BOTH new variables', () => {
    withEnv({ [AUDIO]: '3', [VIDEO]: '3', [RETIRED]: '3' }, () => {
      let err: unknown;
      try {
        loadConfig();
      } catch (e) {
        err = e;
      }
      expect(err).toBeInstanceOf(ClientConfigError);
      expect((err as Error).message).toBe(RETIRED_RECEIVE_SLOTS_MESSAGE);
      expect((err as Error).message).toContain(AUDIO);
      expect((err as Error).message).toContain(VIDEO);
    });
  });
});
