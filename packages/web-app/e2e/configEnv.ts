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

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
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
 * A required integer key of at least `min`. `source` names the file; `purpose`
 * says what the suite needs it for, so a missing key reads as a cause, not a
 * lookup miss. The one reader body; the two named bounds below wrap it.
 */
export function requireIntAtLeast(
  values: ReadonlyMap<string, string>,
  key: string,
  source: string,
  purpose: string,
  min: 0 | 1,
): number {
  const raw = values.get(key);
  if (raw === undefined) {
    throw new Error(`${source} declares no ${key}; ${purpose}`);
  }
  const unquoted = unquote(raw);
  if (!/^[0-9]+$/.test(unquoted) || Number(unquoted) < min) {
    throw new Error(
      `${source} ${key}="${raw}" is not a ${min === 1 ? 'positive' : 'non-negative'} integer`,
    );
  }
  return Number(unquoted);
}

/** A required positive-integer key. */
export function requirePositiveInt(
  values: ReadonlyMap<string, string>,
  key: string,
  source: string,
  purpose: string,
): number {
  return requireIntAtLeast(values, key, source, purpose, 1);
}

/** A required NON-NEGATIVE integer key (0 is legal: e.g. the video-off cap lever, R-26). */
export function requireNonNegativeInt(
  values: ReadonlyMap<string, string>,
  key: string,
  source: string,
  purpose: string,
): number {
  return requireIntAtLeast(values, key, source, purpose, 0);
}

/** MC's per-kind receive-slot caps (story 3 R-1), the keys in MC's own ConfigMap. */
export const MC_MAX_AUDIO_RECEIVE_SLOTS_KEY = 'MC_MAX_AUDIO_RECEIVE_SLOTS';
export const MC_MAX_VIDEO_RECEIVE_SLOTS_KEY = 'MC_MAX_VIDEO_RECEIVE_SLOTS';

/** MC's per-kind receive-slot caps. */
export interface McReceiveCaps {
  readonly audio: number;
  readonly video: number;
}

/**
 * Read MC's per-kind caps from the text of `infra/services/mc-service/config.env`
 * — INDEPENDENTLY of MC's own report on the join (`receiveSlots.*.serverCap`),
 * so a video-over-subscription spec is not circular (paired-test P2).
 *
 * FAILS CLOSED: the Kind overlay does not patch these keys (only the otel
 * collector and the egress budget are patched for MC/MH), so the repo file IS
 * what Kind runs. If a Kind patch ever changed a cap, the spec's equality check
 * against MC's reported cap goes RED rather than silently green.
 */
export function readMcReceiveCaps(configEnvText: string, source: string): McReceiveCaps {
  const values = parseConfigEnv(configEnvText);
  const purpose = "cannot predict MC's per-kind slot truncation";
  return {
    audio: requireNonNegativeInt(values, MC_MAX_AUDIO_RECEIVE_SLOTS_KEY, source, purpose),
    video: requireNonNegativeInt(values, MC_MAX_VIDEO_RECEIVE_SLOTS_KEY, source, purpose),
  };
}

/** `infra/services/<service>/config.env`, as an absolute path. */
export function serviceConfigEnvPath(service: string): string {
  return fileURLToPath(new URL(`../../../infra/services/${service}/config.env`, import.meta.url));
}

/** MC's KEK rotation debounce W, the ConfigMap's REQUIRED key (story 2 R-12). */
export const MC_KEK_ROTATION_DEBOUNCE_KEY = 'MC_KEK_ROTATION_DEBOUNCE_SECONDS';

/**
 * THE reader of a Rust `pub const NAME: u64 = N;` (or `const`) from a source
 * file the suite treats as a value's one home. Throws, naming the source and the
 * constant, when it is not found.
 */
export function readRustU64Const(sourceText: string, name: string, source: string): number {
  const match = new RegExp(`^\\s*(?:pub\\s+)?const ${name}:\\s*u64\\s*=\\s*(\\d+)\\s*;`, 'm').exec(
    sourceText,
  );
  if (match?.[1] === undefined) {
    throw new Error(`${source} no longer declares ${name}`);
  }
  return Number(match[1]);
}

/** MC timings the rotation scenario (S6) budgets against. */
export interface McRotationTimings {
  /** W: rotation happens at most once per W after the OLDEST un-rotated removal. */
  readonly debounceSeconds: number;
}

/**
 * Read W (required). The disconnect grace is NOT part of the budget: the SDK's
 * `disconnect()` sends a LeaveRequest and MC removes the leaver at once, so the
 * leave rotation's debounce starts at the leave.
 */
export function readMcRotationTimings(
  configEnvText: string,
  configEnvSource: string,
): McRotationTimings {
  const values = parseConfigEnv(configEnvText);
  const debounceSeconds = requirePositiveInt(
    values,
    MC_KEK_ROTATION_DEBOUNCE_KEY,
    configEnvSource,
    'cannot budget the KEK rotation scenario (S6)',
  );
  return { debounceSeconds };
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

/**
 * The env var naming the DEPLOYED profile dump layer7.sh writes (Phase 1g).
 * Required by its readers (global-setup's VP8 probe; the late-joiner spec's
 * no-keyframe-interval check) — no fallback to the repo file.
 */
export const MEDIA_CODEC_PROFILE_ENV_VAR = 'E2E_MEDIA_CODEC_PROFILE_ENV';

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

/**
 * MC's rotation timing read from its home (MC's ConfigMap source), and the
 * LEAVE -> ROTATION budget W in ms (a deliberate leave removes at once) — the
 * ONE home both KEK-rotation specs budget against. A liveness bound, never a
 * latency gate.
 */
export function mcRotationBudget(): McRotationTimings & { readonly budgetMs: number } {
  const env = serviceConfigEnvPath('mc-service');
  const timings = readMcRotationTimings(readFileSync(env, 'utf8'), env);
  return { ...timings, budgetMs: timings.debounceSeconds * 1000 };
}

/**
 * Margin on top of {@link mcRotationBudget} for the rotation POLL: the push
 * reaches the page and the bus samples it on its own tick, then the next poll
 * reads it. Shared by both rotation specs so the two cannot drift.
 */
export const ROTATION_POLL_MARGIN_MS = 30_000;

// ---------------------------------------------------------------------------
// Story 4 C3: the MH-replacement legs' timings (`mhLeg.ts`, `mh-replacement.spec.ts`)
// ---------------------------------------------------------------------------
//
// FAILS CLOSED BY CONSTRUCTION: `resilienceTimings` scans the Kind overlay and
// throws if any file there names one of these keys, so the repo base files ARE
// what Kind runs whenever it returns. Every value feeds a FAILURE-ONLY ceiling
// (a poll bound, never a sleep and never a latency target).

/** MC's heartbeat deadline N (ADR-0039 §3): MC declares a silent handler Dead after N. */
export const MC_MH_HEARTBEAT_TIMEOUT_KEY = 'MC_MH_HEARTBEAT_TIMEOUT_MS';
/** MC's connect settle window: how long MC waits for a fresh connection report before placing. */
export const MC_MEDIA_CONNECT_SETTLE_KEY = 'MC_MEDIA_CONNECT_SETTLE_MS';
/** MH's QUIC max idle timeout: how long a client takes to notice a silently dead MH. */
export const MH_MAX_IDLE_TIMEOUT_KEY = 'MH_MAX_IDLE_TIMEOUT_MS';
/** GC's handlers per new meeting (the owned-placement search's `s`). */
export const GC_MH_INITIAL_HANDLERS_KEY = 'GC_MH_INITIAL_HANDLERS_PER_MEETING';

/** The config inputs of the resilience legs. All in ms except `initialHandlers`. */
export interface ResilienceTimings {
  readonly heartbeatDeadlineMs: number;
  readonly connectSettleMs: number;
  readonly mhIdleTimeoutMs: number;
  readonly initialHandlers: number;
  /** GC's health-checker tick (the removal-settle budget's `2 x tick` term). */
  readonly gcHealthTickMs: number;
}

/**
 * GC's health-checker tick from its ONE home, the source constant
 * (`crates/gc-service/src/tasks/generic_health_checker.rs`
 * `DEFAULT_CHECK_INTERVAL_SECONDS`; the Rust twin is `gc_fleet::gc_health_tick`
 * over the same line). Throws if the constant cannot be found.
 */
export function parseGcHealthTickSeconds(sourceText: string, source: string): number {
  const seconds = readRustU64Const(sourceText, 'DEFAULT_CHECK_INTERVAL_SECONDS', source);
  if (seconds <= 0) {
    throw new Error(`${source} DEFAULT_CHECK_INTERVAL_SECONDS is not positive (${seconds})`);
  }
  return seconds;
}

/** The keys the resilience legs read from the BASE config.env files. */
export const RESILIENCE_CONFIG_KEYS = [
  MC_MH_HEARTBEAT_TIMEOUT_KEY,
  MC_MEDIA_CONNECT_SETTLE_KEY,
  MH_MAX_IDLE_TIMEOUT_KEY,
  GC_MH_INITIAL_HANDLERS_KEY,
] as const;

/**
 * PURE: the overlay files (`{path, text}`) that name any of
 * {@link RESILIENCE_CONFIG_KEYS}. The base files are what Kind runs ONLY while
 * this is empty, so a non-empty result is a loud PRECONDITION: a future Kind
 * patch of one of these keys must make the reader read the overlay, never let
 * the ceilings silently drift from the deployed value.
 */
export function overlayFilesPatchingResilienceKeys(
  files: ReadonlyArray<{ readonly path: string; readonly text: string }>,
): string[] {
  return files
    .filter((f) => RESILIENCE_CONFIG_KEYS.some((k) => new RegExp(`\\b${k}\\b`).test(f.text)))
    .map((f) => f.path);
}

/** PURE: the resilience timings from the three config.env texts and GC's tick source. */
export function readResilienceTimings(inputs: {
  readonly mc: { readonly text: string; readonly source: string };
  readonly mh: { readonly text: string; readonly source: string };
  readonly gc: { readonly text: string; readonly source: string };
  readonly gcTick: { readonly text: string; readonly source: string };
}): ResilienceTimings {
  const mc = parseConfigEnv(inputs.mc.text);
  const mh = parseConfigEnv(inputs.mh.text);
  const gc = parseConfigEnv(inputs.gc.text);
  const why = 'cannot derive the MH-replacement ceilings (story 4 C3)';
  return {
    heartbeatDeadlineMs: requirePositiveInt(mc, MC_MH_HEARTBEAT_TIMEOUT_KEY, inputs.mc.source, why),
    connectSettleMs: requirePositiveInt(mc, MC_MEDIA_CONNECT_SETTLE_KEY, inputs.mc.source, why),
    mhIdleTimeoutMs: requirePositiveInt(mh, MH_MAX_IDLE_TIMEOUT_KEY, inputs.mh.source, why),
    initialHandlers: requirePositiveInt(gc, GC_MH_INITIAL_HANDLERS_KEY, inputs.gc.source, why),
    gcHealthTickMs: parseGcHealthTickSeconds(inputs.gcTick.text, inputs.gcTick.source) * 1000,
  };
}

/** GC's health-checker source, as an absolute path (Rust `gc_fleet::GC_HEALTH_TICK_SRC`). */
export function gcHealthTickSrcPath(): string {
  return fileURLToPath(
    new URL('../../../crates/gc-service/src/tasks/generic_health_checker.rs', import.meta.url),
  );
}

/** The Kind overlay tree (`infra/kubernetes/overlays/kind`), as an absolute path. */
export function kindOverlayDir(): string {
  return fileURLToPath(new URL('../../../infra/kubernetes/overlays/kind/', import.meta.url));
}

/**
 * {@link readResilienceTimings} over the repo files — FAIL-CLOSED BY
 * CONSTRUCTION: first every file under the Kind overlay is scanned, and any one
 * naming a resilience key throws (the base would no longer be what Kind runs).
 */
export function resilienceTimings(): ResilienceTimings {
  const overlay = kindOverlayDir();
  const files = (readdirSync(overlay, { recursive: true }) as string[])
    .map((rel) => join(overlay, rel))
    .filter((path) => statSync(path).isFile())
    .map((path) => ({ path, text: readFileSync(path, 'utf8') }));
  if (files.length === 0) {
    throw new Error(`PRECONDITION: ${overlay} holds no files — the overlay scan would be vacuous`);
  }
  const patching = overlayFilesPatchingResilienceKeys(files);
  if (patching.length > 0) {
    throw new Error(
      `PRECONDITION (resilience-key-patched): the Kind overlay patches a key the MH-replacement ` +
        `ceilings read from the base config.env (${RESILIENCE_CONFIG_KEYS.join(', ')}): ` +
        `${patching.join(', ')}. Read the overlay value instead (configEnv.ts:resilienceTimings)`,
    );
  }
  const file = (source: string) => ({ text: readFileSync(source, 'utf8'), source });
  return readResilienceTimings({
    mc: file(serviceConfigEnvPath('mc-service')),
    mh: file(serviceConfigEnvPath('mh-service')),
    gc: file(serviceConfigEnvPath('gc-service')),
    gcTick: file(gcHealthTickSrcPath()),
  });
}
