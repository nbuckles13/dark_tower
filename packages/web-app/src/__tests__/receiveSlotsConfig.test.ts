// File: packages/web-app/src/__tests__/receiveSlotsConfig.test.ts
//
// Story 2 R-1 / R-23: `VITE_DT_RECEIVE_SLOTS` is read through the SDK's single
// parser at config load. Absent -> the SDK default, REPORTED as the default;
// present-but-malformed -> THROW at load (never a silent fallback). Same input
// classes `scripts/dev-web.sh` refuses, so a value cannot pass the launcher's
// preflight and be read differently here.

import { describe, expect, it } from 'vitest';
import { ClientConfigError, DEFAULT_RECEIVE_AUDIO_SLOTS } from '@darktower/sdk-core';

import { loadConfig } from '../lib/config.js';

function withSlots<T>(value: string | undefined, run: () => T): T {
  const env = import.meta.env as unknown as Record<string, unknown>;
  const saved = env['VITE_DT_RECEIVE_SLOTS'];
  if (value === undefined) delete env['VITE_DT_RECEIVE_SLOTS'];
  else env['VITE_DT_RECEIVE_SLOTS'] = value;
  try {
    return run();
  } finally {
    if (saved === undefined) delete env['VITE_DT_RECEIVE_SLOTS'];
    else env['VITE_DT_RECEIVE_SLOTS'] = saved;
  }
}

describe('VITE_DT_RECEIVE_SLOTS', () => {
  it('absent: the SDK default, marked source "default"', () => {
    withSlots(undefined, () => {
      expect(loadConfig().receiveSlots).toEqual({
        count: DEFAULT_RECEIVE_AUDIO_SLOTS,
        source: 'default',
      });
    });
  });

  it('present: the configured N', () => {
    withSlots('3', () => {
      expect(loadConfig().receiveSlots).toEqual({ count: 3, source: 'configured' });
    });
  });

  it.each([[''], ['0'], ['03'], ['-2'], ['3.0'], [' 3'], ['abc']])(
    'malformed %j THROWS at config load',
    (raw) => {
      withSlots(raw, () => {
        expect(() => loadConfig()).toThrow(ClientConfigError);
      });
    },
  );
});
