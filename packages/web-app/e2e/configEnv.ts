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
  const unquoted = raw.replace(/^"(.*)"$/, '$1');
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
