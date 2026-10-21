// File: packages/web-app/e2e/configEnv.ts
//
// THE one reader of a service's `infra/services/<svc>/config.env` for the browser
// suite (@dry-reviewer, @code-reviewer). The suite reads cluster configuration it
// must size itself against — AC's auth-rate limit (`./cohort`), MC's KEK rotation
// debounce W (S6's budget) — rather than restating a number that would drift.
//
// No `./env` import: node vitest loads this directly.
//
// Rules, once: comments and blank lines are ignored (a key named only in a
// comment is not a key); `KEY=value` with surrounding whitespace trimmed; a value
// may be double-quoted. A REQUIRED key that is missing or malformed THROWS, naming
// the file and the key — defaulting would size the suite against a number the
// cluster does not run.

import { fileURLToPath } from 'node:url';

import { SUPPORTED_PROBE_CODECS, type ProbeCodec } from './vp8Probe.js';

/**
 * The header's one quoting rule: a value may be wrapped in double quotes, which
 * are not part of it. Every reader below unquotes through this.
 */
export function unquote(raw: string): string {
  return raw.replace(/^"(.*)"$/, '$1');
}

/** Parse `KEY=value` lines into a map. */
export function parseConfigEnv(text: string): ReadonlyMap<string, string> {
  const values = new Map<string, string>();
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === '' || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq <= 0) continue;
    values.set(trimmed.slice(0, eq).trim(), trimmed.slice(eq + 1).trim());
  }
  return values;
}

/**
 * A required positive-integer key. `source` names the file; `purpose` says what
 * the suite needs it for, so a missing key reads as a cause, not a lookup miss.
 */
export function requirePositiveInt(
  values: ReadonlyMap<string, string>,
  key: string,
  source: string,
  purpose: string,
): number {
  const raw = values.get(key);
  if (raw === undefined) {
    throw new Error(`${source} declares no ${key}; ${purpose}`);
  }
  const unquoted = unquote(raw);
  if (!/^[0-9]+$/.test(unquoted) || Number(unquoted) < 1) {
    throw new Error(`${source} ${key}="${raw}" is not a positive integer`);
  }
  return Number(unquoted);
}

/** `infra/services/<service>/config.env`, as an absolute path. */
export function serviceConfigEnvPath(service: string): string {
  return fileURLToPath(new URL(`../../../infra/services/${service}/config.env`, import.meta.url));
}

/** MC's KEK rotation debounce W, the ConfigMap's REQUIRED key (story 2 R-12). */
export const MC_KEK_ROTATION_DEBOUNCE_KEY = 'MC_KEK_ROTATION_DEBOUNCE_SECONDS';
/** MC's disconnect grace; OPTIONAL in the ConfigMap, code-defaulted when absent. */
export const MC_DISCONNECT_GRACE_KEY = 'MC_DISCONNECT_GRACE_PERIOD_SECONDS';

/**
 * The code default of MC's disconnect grace, read from its one home
 * (`crates/mc-service/src/config.rs:DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS`) —
 * never restated here. Throws if the constant cannot be found.
 */
export function parseMcDefaultGraceSeconds(configRsText: string, source: string): number {
  const match = /DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS:\s*u64\s*=\s*(\d+)\s*;/.exec(configRsText);
  if (match?.[1] === undefined) {
    throw new Error(`${source} no longer declares DEFAULT_DISCONNECT_GRACE_PERIOD_SECONDS`);
  }
  return Number(match[1]);
}

/** MC timings the rotation scenario (S6) budgets against. */
export interface McRotationTimings {
  /** W: rotation happens at most once per W after the OLDEST un-rotated removal. */
  readonly debounceSeconds: number;
  /** Disconnect grace before a departed participant's removal (and so the rotation) starts. */
  readonly graceSeconds: number;
}

/** Read W (required) and the grace (ConfigMap value, else the code default). */
export function readMcRotationTimings(
  configEnvText: string,
  configEnvSource: string,
  configRsText: string,
  configRsSource: string,
): McRotationTimings {
  const values = parseConfigEnv(configEnvText);
  const debounceSeconds = requirePositiveInt(
    values,
    MC_KEK_ROTATION_DEBOUNCE_KEY,
    configEnvSource,
    'cannot budget the KEK rotation scenario (S6)',
  );
  const graceSeconds = values.has(MC_DISCONNECT_GRACE_KEY)
    ? requirePositiveInt(values, MC_DISCONNECT_GRACE_KEY, configEnvSource, '')
    : parseMcDefaultGraceSeconds(configRsText, configRsSource);
  return { debounceSeconds, graceSeconds };
}

/** `crates/mc-service/src/config.rs`, as an absolute path. */
export function mcConfigRsPath(): string {
  return fileURLToPath(new URL('../../../crates/mc-service/src/config.rs', import.meta.url));
}

// ---------------------------------------------------------------------------
// The shared media codec profile (story 3 R-28; the VP8 substrate probe)
// ---------------------------------------------------------------------------
//
// THE one home of the video key list the suite reads. Both probe callers go
// through `readVideoProfile`, over the same `KEY=value` format:
//   * CI (component tier): the repo's BASE source, `mediaCodecProfilePath()`,
//     injected into the browser-mode test as one `define` (`vp8ProbeProfileDefine`);
//   * Layer 7 (`global-setup.ts`): the DEPLOYED ConfigMap, which
//     `scripts/layer7.sh` dumps verbatim (every key; bash lists none) to the
//     file named by `E2E_MEDIA_CODEC_PROFILE_ENV`.
// The values are config, not identity, and are never restated anywhere.

// The codec allowlist's one home is `vp8Probe.ts` (browser-safe; this module
// imports `node:url`, so the probe cannot import a runtime value from here).
export { SUPPORTED_PROBE_CODECS, type ProbeCodec };

/** The five profile keys the VP8 probe reads, named once. */
export const MEDIA_VIDEO_CODEC_KEY = 'MEDIA_VIDEO_CODEC';
export const MEDIA_VIDEO_WIDTH_KEY = 'MEDIA_VIDEO_WIDTH';
export const MEDIA_VIDEO_HEIGHT_KEY = 'MEDIA_VIDEO_HEIGHT';
export const MEDIA_VIDEO_FRAME_RATE_HZ_KEY = 'MEDIA_VIDEO_FRAME_RATE_HZ';
export const MEDIA_VIDEO_MAX_BITRATE_BPS_KEY = 'MEDIA_VIDEO_MAX_BITRATE_BPS';

/** The video encoding the profile directs, as the probe needs it. */
export interface VideoCodecProfile {
  readonly codec: ProbeCodec;
  readonly width: number;
  readonly height: number;
  readonly frameRateHz: number;
  readonly maxBitrateBps: number;
}

/** `infra/services/media-codec-profile/profile.env` (a generator env file, not a `config.env`). */
export function mediaCodecProfilePath(): string {
  return fileURLToPath(
    new URL('../../../infra/services/media-codec-profile/profile.env', import.meta.url),
  );
}

/**
 * Read the video half of the profile from `text` (`source` names it). Every key
 * REQUIRED; a missing, malformed or unsupported value THROWS — the probe must
 * check the encoding the cluster directs, never a default.
 */
export function readVideoProfile(text: string, source: string): VideoCodecProfile {
  const values = parseConfigEnv(text);
  const purpose = 'cannot run the VP8 substrate probe (R-28) at the directed profile';
  const rawCodec = values.get(MEDIA_VIDEO_CODEC_KEY);
  if (rawCodec === undefined) {
    throw new Error(`${source} declares no ${MEDIA_VIDEO_CODEC_KEY}; ${purpose}`);
  }
  const codec = unquote(rawCodec);
  if (!(SUPPORTED_PROBE_CODECS as readonly string[]).includes(codec)) {
    throw new Error(
      `${source} ${MEDIA_VIDEO_CODEC_KEY}="${rawCodec}" is not a codec the probe checks ` +
        `(${SUPPORTED_PROBE_CODECS.join(', ')})`,
    );
  }
  return {
    codec: codec as ProbeCodec,
    width: requirePositiveInt(values, MEDIA_VIDEO_WIDTH_KEY, source, purpose),
    height: requirePositiveInt(values, MEDIA_VIDEO_HEIGHT_KEY, source, purpose),
    frameRateHz: requirePositiveInt(values, MEDIA_VIDEO_FRAME_RATE_HZ_KEY, source, purpose),
    maxBitrateBps: requirePositiveInt(values, MEDIA_VIDEO_MAX_BITRATE_BPS_KEY, source, purpose),
  };
}

/** The build-time global the component-tier probe reads (test runner only — never `vite.config.ts`). */
export const VP8_PROBE_PROFILE_DEFINE = '__E2E_VP8_PROBE_PROFILE__';

/**
 * The `define` entry `vitest.config.ts` injects: the repo's BASE profile, read
 * through {@link readVideoProfile} — the ONE construction, so the define cannot
 * be a restated literal (`tests/configEnv.test.ts` pins it). The `profilePath`
 * parameter exists for that test only.
 */
export function vp8ProbeProfileDefine(
  readText: (path: string) => string,
  profilePath: string = mediaCodecProfilePath(),
): Record<string, string> {
  return {
    [VP8_PROBE_PROFILE_DEFINE]: JSON.stringify(
      readVideoProfile(readText(profilePath), profilePath),
    ),
  };
}
