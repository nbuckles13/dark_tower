// File: packages/web-app/tests/debugNeverExported.test.ts
//
// STORY 3 R-7 / R-35: THE LOCAL DEBUG PANEL IS NEVER EXPORTED AND SENDS NOTHING
// — proven mechanically, not by review (observability's and security's
// acceptance criteria).
//
// WHAT IS SCANNED. The IMPORT CLOSURE of `src/views/DebugPanel.svelte`, walked
// from the file itself through every relative import — not a hand-kept list —
// and asserted EQUAL to the expected set, so a new helper the panel starts
// importing fails here until it is reviewed into the set. Plus the SDK's
// `media/lifecycle/localStats.ts`, the SDK side of the surface.
//
// WHAT IS DENIED (raw source, COMMENTS INCLUDED — nothing is stripped):
//   * any import of the e2e bus / analysis, the telemetry modules, a metrics
//     sink or exporter, or a transport / signalling seam — by module specifier
//     AND by imported name (a named import from the bare `@darktower/sdk-core`
//     barrel is caught too);
//   * non-relative imports other than `svelte` and TYPE-ONLY `@darktower/sdk-core`,
//     except three named SDK values the self-check needs (its own-capture
//     opener, the typed capture error and the closed failure vocabulary);
//   * the `dt_client_` metric prefix, `console.`, `window`, `globalThis`, the
//     test-bus global, dynamic `import(`, and every frequency-domain analyser
//     read (the mic meter is time-domain RMS only).
//
// REVERSE DIRECTION. The export paths never reference the local surface: the
// web app's e2e bus and analysis, the SDK's `mediaMetrics.ts` and every file in
// the SDK's `telemetry/` directory.
//
// VACUITY IS ITS OWN FAILURE. A missing or empty file, a closure that does not
// match, or a positive control the matcher misses fails with a message starting
// `VACUITY:` — distinct from a content hit (`DENY:`) — so nobody "fixes" a
// vacuity failure by loosening the matcher.

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const HERE = dirname(fileURLToPath(import.meta.url));
const WEB_SRC = resolve(HERE, '../src');
const SDK_SRC = resolve(HERE, '../../sdk-core/src');

const ROOT = join(WEB_SRC, 'views/DebugPanel.svelte');
const EXPECTED_CLOSURE = [
  'views/DebugPanel.svelte',
  'lib/debugStats.ts',
  'lib/selfCheck.ts',
  'lib/debugToggle.ts',
].sort();
/** The SDK side of the local surface: scanned with the same deny list. */
const SDK_SURFACE = [
  join(SDK_SRC, 'media/lifecycle/localStats.ts'),
  join(SDK_SRC, 'session/localDebugSurface.ts'),
];
/**
 * The own-capture opener legitimately reads `globalThis.navigator?.mediaDevices`
 * (its default acquisition). A NARROW per-file allowance: exactly that one
 * expression is stripped, once, before the text deny — every other rule,
 * `globalThis` elsewhere included, still applies.
 */
const SDK_LOCAL_PREVIEW = join(SDK_SRC, 'media/setup/localPreview.ts');
const LOCAL_PREVIEW_ALLOWED = 'globalThis.navigator?.mediaDevices';

/** The real export-path modules, resolved (never typed from memory): must exist. */
const E2E_BUS = join(WEB_SRC, 'lib/e2eBus.ts');
const E2E_ANALYSIS = join(WEB_SRC, 'lib/e2eAnalysis.ts');
const MEDIA_METRICS = join(SDK_SRC, 'media/setup/mediaMetrics.ts');
const TELEMETRY_DIR = join(SDK_SRC, 'telemetry');

/** Module specifiers (substring, case-sensitive) no debug module may import. */
const DENIED_SPECIFIERS = [
  'e2eBus',
  'e2eAnalysis',
  'telemetry',
  'MetricsSink',
  'mediaMetrics',
  'IWebTransport',
  'MediaTransport',
  'transport/',
  'SignalingClient',
  'signaling/',
  'session.js',
] as const;

/** Imported NAMES no debug module may bring in, from ANY specifier (the barrel included). */
const DENIED_NAMES = [
  'MetricsSink',
  'getMetricsSink',
  'setMetricsSink',
  'flushMetrics',
  'OtelMetricsSink',
  'NoopMetricsSink',
  'configureTelemetry',
  'installE2EHooks',
  'MediaMetrics',
  'MediaKindMetrics',
  'IWebTransport',
  'MediaTransport',
  'BrowserWebTransport',
  'SignalingClient',
  'MeetingSession',
] as const;

/** Raw-text tokens (comments included). */
const DENIED_TEXT = [
  'dt_client_',
  'console.',
  'globalThis',
  'window',
  '__darktower_test__',
  'import(',
  'getFloatFrequencyData',
  'getByteFrequencyData',
] as const;

/** The only VALUE imports allowed from the SDK barrel (everything else type-only). */
const ALLOWED_SDK_VALUES = new Set(['CaptureFailure', 'MediaCaptureError', 'openLocalPreview']);
const SDK = '@darktower/sdk-core';

interface ImportDecl {
  readonly specifier: string;
  readonly typeOnly: boolean;
  /** Imported names; type-only members flagged. */
  readonly names: readonly { readonly name: string; readonly typeOnly: boolean }[];
}

/** The script text of a `.svelte` file, or the file itself. */
function scriptOf(path: string, text: string): string {
  if (!path.endsWith('.svelte')) return text;
  return [...text.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n');
}

/** Every static import declaration in `source`. */
export function importsOf(source: string): ImportDecl[] {
  const out: ImportDecl[] = [];
  // `import … from` AND re-exports (`export { x } from`, `export * from`): a
  // re-export pulls a module in exactly as an import does.
  const re =
    /^\s*(?:import\s+(type\s+)?([\s\S]*?)|export\s+(type\s+)?(\*(?:\s+as\s+\w+)?|\{[^}]*\}))\s*from\s*['"]([^'"]+)['"]/gm;
  for (const raw of source.matchAll(re)) {
    const m = [raw[0], raw[1] ?? raw[3], raw[2] ?? raw[4], raw[5]] as const;
    const typeOnly = m[1] !== undefined;
    const clause = m[2] ?? '';
    const names: { name: string; typeOnly: boolean }[] = [];
    const braces = /\{([\s\S]*)\}/.exec(clause);
    for (const part of (braces?.[1] ?? '').split(',')) {
      const trimmed = part.trim();
      if (trimmed === '') continue;
      const memberType = trimmed.startsWith('type ');
      const name = (memberType ? trimmed.slice(5) : trimmed).split(/\s+as\s+/)[0]!.trim();
      names.push({ name, typeOnly: typeOnly || memberType });
    }
    const outside = clause
      .replace(/\{[\s\S]*\}/, '')
      .replace(/,/g, ' ')
      .trim();
    for (const d of outside.split(/\s+/).filter((x) => x !== '' && x !== '*' && x !== 'as')) {
      names.push({ name: d, typeOnly });
    }
    out.push({ specifier: m[3]!, typeOnly, names });
  }
  for (const m of source.matchAll(/^\s*import\s+['"]([^'"]+)['"]/gm)) {
    out.push({ specifier: m[1]!, typeOnly: false, names: [] });
  }
  return out;
}

/** Content findings for one source (empty = clean). */
export function denyHits(source: string, script: string = source): string[] {
  const hits: string[] = [];
  for (const token of DENIED_TEXT) {
    if (source.includes(token)) hits.push(`DENY: text '${token}'`);
  }
  for (const decl of importsOf(script)) {
    for (const bad of DENIED_SPECIFIERS) {
      if (decl.specifier.includes(bad)) hits.push(`DENY: import from '${decl.specifier}'`);
    }
    for (const { name } of decl.names) {
      if ((DENIED_NAMES as readonly string[]).includes(name)) {
        hits.push(`DENY: imported name '${name}' from '${decl.specifier}'`);
      }
    }
    const relativeSpec = decl.specifier.startsWith('.');
    if (relativeSpec || decl.specifier === 'svelte') continue;
    if (decl.specifier !== SDK) {
      hits.push(`DENY: non-relative import '${decl.specifier}' is not allow-listed`);
      continue;
    }
    for (const { name, typeOnly } of decl.names) {
      if (!typeOnly && !ALLOWED_SDK_VALUES.has(name)) {
        hits.push(`DENY: value import '${name}' from '${SDK}' (only types, plus the allow-list)`);
      }
    }
  }
  return hits;
}

/** Resolve a relative specifier from `from` to an existing file. */
function resolveLocal(from: string, specifier: string): string {
  const base = resolve(dirname(from), specifier);
  for (const candidate of [base, base.replace(/\.js$/, '.ts'), `${base}.ts`]) {
    if (existsSync(candidate) && candidate !== dirname(from)) return candidate;
  }
  throw new Error(`VACUITY: cannot resolve '${specifier}' from ${from}`);
}

/** The relative-import closure from `root`. */
function closureOf(root: string): string[] {
  const seen = new Set<string>();
  const stack = [root];
  while (stack.length > 0) {
    const file = stack.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    for (const decl of importsOf(scriptOf(file, readFileSync(file, 'utf8')))) {
      if (decl.specifier.startsWith('.')) stack.push(resolveLocal(file, decl.specifier));
    }
  }
  return [...seen];
}

function readNonEmpty(path: string): string {
  if (!existsSync(path)) throw new Error(`VACUITY: ${path} does not exist`);
  const text = readFileSync(path, 'utf8');
  if (text.trim() === '') throw new Error(`VACUITY: ${path} is empty`);
  return text;
}

describe('the local debug panel is never exported and sends nothing (R-7, R-35)', () => {
  const closure = closureOf(ROOT);

  it('VACUITY: the walked import closure is exactly the expected set, every file non-empty', () => {
    const rel = closure.map((f) => relative(WEB_SRC, f)).sort();
    expect(rel, 'VACUITY: the debug import closure changed; review the new module in').toEqual(
      EXPECTED_CLOSURE,
    );
    for (const file of closure) readNonEmpty(file);
  });

  it('DENY: no debug module imports an export path, a sink, a transport, or an unlisted value', () => {
    for (const file of [...closure, ...SDK_SURFACE]) {
      const text = readNonEmpty(file);
      expect(denyHits(text, scriptOf(file, text)), relative(resolve(HERE, '../..'), file)).toEqual(
        [],
      );
    }
  });

  it('DENY: the SDK own-capture opener, with its one narrow globalThis allowance', () => {
    const text = readNonEmpty(SDK_LOCAL_PREVIEW);
    const occurrences = text.split(LOCAL_PREVIEW_ALLOWED).length - 1;
    expect(occurrences, `VACUITY: expected exactly one '${LOCAL_PREVIEW_ALLOWED}'`).toBe(1);
    const stripped = text.replace(LOCAL_PREVIEW_ALLOWED, '');
    expect(denyHits(stripped), relative(resolve(HERE, '../..'), SDK_LOCAL_PREVIEW)).toEqual([]);
  });

  it('VACUITY: positive controls — the matcher hits real and synthetic offenders', () => {
    // A REAL production import of the bus (MeetingRoom installs it) is caught.
    const room = readNonEmpty(join(WEB_SRC, 'views/MeetingRoom.svelte'));
    const roomHits = denyHits(room, scriptOf('x.svelte', room));
    expect(
      roomHits.some((h) => h.includes("'../lib/e2eBus.js'")),
      'VACUITY: the matcher missed the real e2eBus import in MeetingRoom.svelte',
    ).toBe(true);
    // The real bus module exists where the deny pattern points.
    readNonEmpty(E2E_BUS);
    readNonEmpty(E2E_ANALYSIS);
    const synthetic: Array<[string, string]> = [
      ["import { getMetricsSink } from '@darktower/sdk-core';", 'getMetricsSink'],
      ["import { flushMetrics } from '@darktower/sdk-core';", 'flushMetrics'],
      ["import type { MetricsSink } from '../telemetry/MetricsSink.js';", 'telemetry'],
      ["import { installE2EHooks } from '../lib/e2eBus.js';", 'e2eBus'],
      ["import { MediaTransport } from '@darktower/sdk-core';", 'MediaTransport'],
      ["import type { IWebTransport } from '@darktower/sdk-core';", 'IWebTransport'],
      ["import { listMicrophones } from '@darktower/sdk-core';", 'listMicrophones'],
      ["import x from 'some-lib';", 'some-lib'],
      ["const name = 'dt_client_x';", 'dt_client_'],
      ['console.log(stats);', 'console.'],
      ['window.__debug = view;', 'window'],
      ['globalThis.stats = view;', 'globalThis'],
      ['analyser.getByteFrequencyData(buf);', 'getByteFrequencyData'],
      ["const m = await import('../lib/e2eBus.js');", 'import('],
      ["export { installE2EHooks } from '../lib/e2eBus.js';", 'e2eBus'],
      ["export * from '../telemetry/index.js';", 'telemetry'],
      ["export type { MetricsSink } from '@darktower/sdk-core';", 'MetricsSink'],
    ];
    for (const [source, marker] of synthetic) {
      const hits = denyHits(source);
      expect(
        hits.some((h) => h.includes(marker)),
        `VACUITY: the matcher missed a synthetic offender: ${source}`,
      ).toBe(true);
    }
    // And a clean type-only SDK import plus the two allowed values pass.
    expect(
      denyHits(
        "import { MediaCaptureError, openLocalPreview, type LocalPreview } from '@darktower/sdk-core';\nimport type { LocalMediaStats } from '@darktower/sdk-core';",
      ),
    ).toEqual([]);
  });

  it('REVERSE: the export paths never reference the local surface (with their own controls)', () => {
    const LOCAL_SURFACE = [
      'localPreview',
      'openLocalPreview',
      'localStats',
      'localCaptureTracks',
      'LocalMediaStats',
      'localSendStats',
      'slotTally',
      'bytesSent',
      'captureTrack',
      'debugStats',
      'DebugPanel',
    ];
    const telemetryFiles = readdirSync(TELEMETRY_DIR)
      .filter((f) => f.endsWith('.ts'))
      .map((f) => join(TELEMETRY_DIR, f));
    expect(
      telemetryFiles.length,
      'VACUITY: no files in the SDK telemetry directory',
    ).toBeGreaterThan(0);
    const targets: Array<[string, string]> = [
      // [file, a token it is KNOWN to contain — the scan's positive control]
      [E2E_BUS, 'sendTotals'],
      [E2E_ANALYSIS, 'getFloatFrequencyData'],
      [MEDIA_METRICS, 'dt_client_media_frames_sent_total'],
      ...telemetryFiles.map((f): [string, string] => [f, 'export']),
    ];
    for (const [file, known] of targets) {
      const text = readNonEmpty(file);
      expect(text.includes(known), `VACUITY: ${file} lacks its known token '${known}'`).toBe(true);
      for (const token of LOCAL_SURFACE) {
        expect(text.includes(token), `DENY: ${file} references '${token}'`).toBe(false);
      }
    }
  });
});
