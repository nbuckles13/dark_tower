// File: packages/sdk-core/src/media/__tests__/hotPathLayout.test.ts
//
// THE TYPESCRIPT ANALOGUE OF ADR-0036 §11'S DIRECTORY-SCOPED DENY.
//
// ---------------------------------------------------------------------------
// WHY THIS EXISTS AT ALL
// ---------------------------------------------------------------------------
//
// §11 requires that no log, metric or span emission be reachable from the media
// forward path, and mandates ONE layout constraint to make that checkable:
// "lifecycle, setup, and teardown are SIBLINGS of the media directory, not
// children, so the directory boundary and the hot-path boundary are the same
// boundary." It also requires the deny be scoped to a DIRECTORY rather than a
// file list, "which narrows silently on refactor while the guard keeps passing".
//
// `dt-guard media-telemetry-deny` implements that for Rust and reads only
// `crates/`. Nothing mechanical reaches TypeScript: `metric_labels.rs` scans
// `crates/`, and `ts_pii.rs` scans `console.*` / `logger.*` call sites only. So
// this test IS the control for the SDK.
//
// ---------------------------------------------------------------------------
// THE SCOPE FAILS ITSELF, DELIBERATELY
// ---------------------------------------------------------------------------
//
// The Rust guard "fails if its configured directory is absent or empty", and
// this one does too. A scope check that silently passes over a directory nobody
// renamed it to is the failure mode ADR-0036 §11 names: a control that reads as
// coverage and covers nothing.
//
// ---------------------------------------------------------------------------
// TWO CONSTRAINTS, AND THE SECOND IS A POSITIVE ONE
// ---------------------------------------------------------------------------
//
//   1. HOT PATH (`pipeline/**`): no logging sink, no `dt_client_` literal, no
//      import of the logger. Cached-handle METHOD calls are allowed — banning
//      them would ban the pattern the control exists to enforce.
//
//   2. SIBLINGS (`lifecycle/**`, `setup/**`, `teardown/**`): these may log by
//      DESIGN — that is what the sibling layout is for — and they may equally
//      emit metrics and span attributes, because the deny is scoped by directory
//      rather than by signal type. And they are where the KEK, the roster keys
//      and the transmit keys live. So the label constraint is asserted
//      POSITIVELY: exactly ONE file under `media/**` may name a `dt_client_`
//      metric, and it is the allow-list label constructor.
//
//      A positive reachability property rather than an enumerated bad-pattern
//      scan, because an enumeration catches `log(kek)` and misses `log({kek})` —
//      the object-graph spelling that reads as innocuous and serialises
//      everything. It also degrades safely: a new sibling emission that forgets
//      the constructor FAILS, rather than passing because nobody enumerated its
//      shape.
//
// ---------------------------------------------------------------------------
// HOW THIS CONTROL NARROWS, STATED SO IT IS NOT CITED AS MORE THAN IT IS
// ---------------------------------------------------------------------------
//
// This is a source scan over an enumerated set of sink spellings. An enumerated
// list narrows silently on refactor while continuing to report green — the exact
// defect §11 cites when it chose directory scoping over a file list. What is
// directory-scoped here is the SCOPE; the SINK LIST is not, and it cannot be,
// because TypeScript has no macro forms to deny. Read it as the weaker form it
// is: it catches the spellings below, in these directories, and nothing else.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const MEDIA_ROOT = new URL('..', import.meta.url).pathname;
const HOT_PATH_DIR = join(MEDIA_ROOT, 'pipeline');
const SIBLING_DIRS = ['lifecycle', 'setup', 'teardown'].map((d) => join(MEDIA_ROOT, d));

/** The ONE file permitted to name a `dt_client_` metric anywhere under `media/**`. */
const METRIC_HOME = join(MEDIA_ROOT, 'setup', 'mediaMetrics.ts');

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      // `__tests__` is not production code and is not in scope: a test may
      // legitimately name a metric it asserts on.
      if (entry === '__tests__') continue;
      out.push(...sourceFiles(path));
      continue;
    }
    if (entry.endsWith('.ts')) out.push(path);
  }
  return out;
}

/** Strip comments so a file DOCUMENTING a rule does not trip it. */
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');
}

describe('media hot-path layout (ADR-0036 §11)', () => {
  it('fails if the hot-path directory is absent or empty — the scope fails itself', () => {
    // Not a formality. A scope check that silently passes over a directory
    // nobody renamed it to is a control that reads as coverage and covers
    // nothing, which is the failure mode §11 names by name.
    const files = sourceFiles(HOT_PATH_DIR);
    expect(
      files.length,
      `media/pipeline contains no .ts sources; the hot-path deny below is covering nothing`,
    ).toBeGreaterThan(0);
  });

  it('has lifecycle, setup and teardown as SIBLINGS of the hot path, not children', () => {
    // The layout constraint itself. If a sibling were moved under `pipeline/`,
    // the directory boundary and the hot-path boundary would stop coinciding and
    // the deny above would start banning code it is supposed to permit.
    for (const dir of SIBLING_DIRS) {
      expect(
        sourceFiles(dir).length,
        `${dir} contains no .ts sources; the sibling layout §11 requires is not in place`,
      ).toBeGreaterThan(0);
      expect(dir.startsWith(HOT_PATH_DIR)).toBe(false);
    }
  });

  it('has no logging sink reachable from per-frame code', () => {
    for (const file of sourceFiles(HOT_PATH_DIR)) {
      const source = stripComments(readFileSync(file, 'utf8'));
      expect(source, `${file}: console.* is not reachable from the hot path`).not.toMatch(
        /\bconsole\s*\./,
      );
      expect(source, `${file}: logger.* is not reachable from the hot path`).not.toMatch(
        /\blogger\s*\./,
      );
      expect(source, `${file}: the bounded event logger is not reachable`).not.toMatch(
        /logJoinEvent|telemetry\/logger/,
      );
    }
  });

  it('names no metric in per-frame code — handles are resolved once, at setup', () => {
    for (const file of sourceFiles(HOT_PATH_DIR)) {
      const source = stripComments(readFileSync(file, 'utf8'));
      expect(
        source,
        `${file}: a metric NAME in the hot path means a registry lookup per frame; ` +
          `call a method on the handle built in setup/mediaMetrics.ts instead`,
      ).not.toContain('dt_client_');
    }
  });

  it('confines every dt_client_ metric name under media/** to the allow-list constructor', () => {
    // THE POSITIVE CONSTRAINT. `lifecycle/`, `setup/` and `teardown/` are
    // outside §11's deny by design and hold the KEK, the roster keys and the
    // transmit keys — so a `sender_id`, key id or stream id could reach a LABEL
    // from any of them with nothing between it and production. Rather than
    // enumerate bad label spellings (which an object-graph spelling defeats),
    // assert that exactly one file constructs media labels at all.
    const offenders: string[] = [];
    for (const dir of [HOT_PATH_DIR, ...SIBLING_DIRS]) {
      for (const file of sourceFiles(dir)) {
        if (file === METRIC_HOME) continue;
        if (stripComments(readFileSync(file, 'utf8')).includes('dt_client_')) {
          offenders.push(file);
        }
      }
    }
    expect(
      offenders,
      `these files name a dt_client_ metric outside ${METRIC_HOME}; media labels must be built ` +
        `by mediaMetricLabels, which takes two named strings and therefore cannot inherit a ` +
        `meeting, participant or stream dimension`,
    ).toEqual([]);
  });

  it('builds the media label set by allow-list, never by pruning a spread', () => {
    // Comments stripped: this file DOCUMENTS why `meeting_id_hash` must not
    // appear, and a rule that trips on its own rationale trains people to delete
    // the rationale.
    const source = stripComments(readFileSync(METRIC_HOME, 'utf8'));
    // A deletion-based helper is one refactor away from re-inheriting a newly
    // added join label; an allow-list cannot be. Asserted on the shape, because
    // the property is what the constructor ACCEPTS, not what it happens to emit
    // today.
    expect(source).toContain('client_version: identity.clientVersion');
    expect(source).toContain('org_id: identity.orgId');
    expect(source).not.toContain('meeting_id_hash');
  });
});
