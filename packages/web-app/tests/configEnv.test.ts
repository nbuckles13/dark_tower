// Node tier for `e2e/configEnv.ts` — the one config.env reader, and the MC
// rotation timings S6 budgets against, read from their homes (never restated).

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  MC_KEK_ROTATION_DEBOUNCE_KEY,
  mcConfigRsPath,
  parseConfigEnv,
  parseMcDefaultGraceSeconds,
  readMcRotationTimings,
  requirePositiveInt,
  serviceConfigEnvPath,
} from '../e2e/configEnv.js';

describe('parseConfigEnv / requirePositiveInt', () => {
  it('ignores comments and blanks, trims, and reads quoted values', () => {
    const values = parseConfigEnv(`# X=9\n\n  X = "7" \nY=3\n`);
    expect(requirePositiveInt(values, 'X', 'f', 'p')).toBe(7);
    expect(requirePositiveInt(values, 'Y', 'f', 'p')).toBe(3);
  });

  it('a missing key names the file, the key and the purpose', () => {
    expect(() => requirePositiveInt(parseConfigEnv(''), 'K', 'f.env', 'needed for S6')).toThrow(
      'f.env declares no K; needed for S6',
    );
  });

  it.each(['0', '-1', '1.5', 'x', ''])('a malformed value (%j) throws', (bad) => {
    expect(() => requirePositiveInt(parseConfigEnv(`K=${bad}`), 'K', 'f', 'p')).toThrow(
      /not a positive integer/,
    );
  });
});

describe('MC rotation timings', () => {
  it('reads W from the config and the grace from the code default when the config omits it', () => {
    const t = readMcRotationTimings(
      `${MC_KEK_ROTATION_DEBOUNCE_KEY}=45\n`,
      'mc.env',
      'pub const DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS: u64 = 12;',
      'config.rs',
    );
    expect(t).toEqual({ debounceSeconds: 45, graceSeconds: 12 });
  });

  it('a config-set grace wins over the code default', () => {
    const t = readMcRotationTimings(
      `${MC_KEK_ROTATION_DEBOUNCE_KEY}=45\nMC_DISCONNECT_GRACE_PERIOD_SECONDS=5\n`,
      'mc.env',
      'pub const DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS: u64 = 12;',
      'config.rs',
    );
    expect(t.graceSeconds).toBe(5);
  });

  it('a vanished code default throws rather than guessing', () => {
    expect(() => parseMcDefaultGraceSeconds('nothing here', 'config.rs')).toThrow(
      /no longer declares/,
    );
  });

  it('the REAL MC config and code parse (the SSoT S6 reads)', () => {
    const env = serviceConfigEnvPath('mc-service');
    const rs = mcConfigRsPath();
    const t = readMcRotationTimings(readFileSync(env, 'utf8'), env, readFileSync(rs, 'utf8'), rs);
    expect(t.debounceSeconds).toBeGreaterThanOrEqual(1);
    expect(t.graceSeconds).toBeGreaterThanOrEqual(1);
  });
});
