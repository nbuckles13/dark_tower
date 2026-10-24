// Node tier for `e2e/configEnv.ts` — the one config.env reader, and the MC
// rotation timings S6 budgets against, read from their homes (never restated).

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  MC_KEK_ROTATION_DEBOUNCE_KEY,
  mcConfigRsPath,
  mediaCodecProfilePath,
  readVideoProfile,
  VP8_PROBE_PROFILE_DEFINE,
  vp8ProbeProfileDefine,
  parseConfigEnv,
  parseMcDefaultGraceSeconds,
  readMcReceiveCaps,
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

// ---------------------------------------------------------------------------
// Story 3 R-28: the media codec profile reader (the VP8 probe's ONE profile source)
// ---------------------------------------------------------------------------
//
// MANDATORY POSITIVE CONTROL — do not skip, weaken or delete. The Layer-7 probe
// reads the DEPLOYED profile through `readVideoProfile`; a key renamed on the TS
// side would otherwise first surface there, after the whole Rust suite. Reading
// the REAL `profile.env` here makes that a Layer-1–6 FAIL.

/** The file's own `KEY=value` for `key`, read with a regex independent of the reader. */
function fileValue(text: string, key: string): string {
  const m = new RegExp(`^${key}=(\\S+)\\s*$`, 'm').exec(text);
  if (m?.[1] === undefined) throw new Error(`test setup: profile.env has no ${key} line`);
  return m[1];
}

describe('readVideoProfile', () => {
  const path = mediaCodecProfilePath();
  const text = readFileSync(path, 'utf8');

  it('reads the REAL base profile.env, value for value', () => {
    expect(readVideoProfile(text, path)).toEqual({
      codec: fileValue(text, 'MEDIA_VIDEO_CODEC'),
      width: Number(fileValue(text, 'MEDIA_VIDEO_WIDTH')),
      height: Number(fileValue(text, 'MEDIA_VIDEO_HEIGHT')),
      frameRateHz: Number(fileValue(text, 'MEDIA_VIDEO_FRAME_RATE_HZ')),
      maxBitrateBps: Number(fileValue(text, 'MEDIA_VIDEO_MAX_BITRATE_BPS')),
    });
  });

  const valid = [
    'MEDIA_VIDEO_CODEC=vp8',
    'MEDIA_VIDEO_WIDTH=320',
    'MEDIA_VIDEO_HEIGHT=180',
    'MEDIA_VIDEO_FRAME_RATE_HZ=15',
    'MEDIA_VIDEO_MAX_BITRATE_BPS=250000',
  ];

  it('reads a deployed-style dump (every key, any order, unrelated keys ignored)', () => {
    const dump = ['MEDIA_AUDIO_CODEC=opus', ...valid.slice().reverse()].join('\n');
    expect(readVideoProfile(dump, 'dump.env')).toEqual({
      codec: 'vp8',
      width: 320,
      height: 180,
      frameRateHz: 15,
      maxBitrateBps: 250_000,
    });
  });

  it.each(valid.map((line) => line.split('=')[0]!))('a missing %s throws, naming it', (key) => {
    const without = valid.filter((l) => !l.startsWith(`${key}=`)).join('\n');
    expect(() => readVideoProfile(without, 'dump.env')).toThrow(`dump.env declares no ${key}`);
  });

  it.each(['h264', '', 'VP8'])('an unsupported codec (%j) throws', (codec) => {
    const text2 = valid.map((l) =>
      l.startsWith('MEDIA_VIDEO_CODEC=') ? `MEDIA_VIDEO_CODEC=${codec}` : l,
    );
    expect(() => readVideoProfile(text2.join('\n'), 'dump.env')).toThrow(
      /not a codec the probe checks/,
    );
  });

  it('a non-integer dimension throws', () => {
    const text2 = valid.map((l) =>
      l.startsWith('MEDIA_VIDEO_WIDTH=') ? 'MEDIA_VIDEO_WIDTH=320.5' : l,
    );
    expect(() => readVideoProfile(text2.join('\n'), 'dump.env')).toThrow(/not a positive integer/);
  });
});

describe('vp8ProbeProfileDefine: what vitest.config.ts injects is DERIVED, never restated', () => {
  it('equals readVideoProfile over the real base profile.env', () => {
    const path = mediaCodecProfilePath();
    const define = vp8ProbeProfileDefine((p) => readFileSync(p, 'utf8'));
    expect(Object.keys(define)).toEqual([VP8_PROBE_PROFILE_DEFINE]);
    expect(JSON.parse(define[VP8_PROBE_PROFILE_DEFINE]!)).toEqual(
      readVideoProfile(readFileSync(path, 'utf8'), path),
    );
  });

  it('reads the file it is pointed at (a changed profile changes the define)', () => {
    const define = vp8ProbeProfileDefine(
      () =>
        'MEDIA_VIDEO_CODEC=vp8\nMEDIA_VIDEO_WIDTH=2\nMEDIA_VIDEO_HEIGHT=4\nMEDIA_VIDEO_FRAME_RATE_HZ=6\nMEDIA_VIDEO_MAX_BITRATE_BPS=8\n',
      'other.env',
    );
    expect(JSON.parse(define[VP8_PROBE_PROFILE_DEFINE]!)).toEqual({
      codec: 'vp8',
      width: 2,
      height: 4,
      frameRateHz: 6,
      maxBitrateBps: 8,
    });
  });

  it('vitest.config.ts injects it through the helper (one call, no literal)', () => {
    const config = readFileSync(new URL('../vitest.config.ts', import.meta.url), 'utf8');
    expect(config).toContain('...vp8ProbeProfileDefine(');
    expect(config).not.toContain(VP8_PROBE_PROFILE_DEFINE);
  });
});

describe('readMcReceiveCaps (story 3 C-C1: the independent cap source)', () => {
  it('reads both caps from the REAL MC config.env (the Kind overlay does not patch them)', () => {
    const path = serviceConfigEnvPath('mc-service');
    const caps = readMcReceiveCaps(readFileSync(path, 'utf8'), path);
    expect(Number.isInteger(caps.audio) && caps.audio >= 0).toBe(true);
    expect(Number.isInteger(caps.video) && caps.video >= 0).toBe(true);
  });
  it('accepts 0 (the video-off lever) and refuses a missing or malformed key', () => {
    expect(
      readMcReceiveCaps('MC_MAX_AUDIO_RECEIVE_SLOTS=8\nMC_MAX_VIDEO_RECEIVE_SLOTS=0', 'x'),
    ).toEqual({
      audio: 8,
      video: 0,
    });
    expect(() => readMcReceiveCaps('MC_MAX_AUDIO_RECEIVE_SLOTS=8', 'x')).toThrow(/declares no/);
    expect(() =>
      readMcReceiveCaps('MC_MAX_AUDIO_RECEIVE_SLOTS=-1\nMC_MAX_VIDEO_RECEIVE_SLOTS=4', 'x'),
    ).toThrow(/non-negative integer/);
  });
});
