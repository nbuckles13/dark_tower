// File: packages/sdk-core/src/__tests__/joinLabelSpread.test.ts
//
// SECONDARY, STATIC half of the join-label-bag rule (story 2 task 13). The
// PRIMARY check is behavioural — `session/__tests__/meeting-session.test.ts`,
// "join label bag", drives join -> startMedia -> leave through a recording sink
// and asserts every emission carrying `meeting_id_hash` is on the roster. This
// scan covers emission sites that flow does not exercise.
//
// RESIDUAL, STATED SO IT IS NOT CITED AS MORE THAN IT IS: the scan collects
// `dt_client_*` LITERALS in files that spread the join bag. A metric name that
// reaches a spreading helper through a variable or an import from another file is
// invisible to it; the behavioural check is what covers that, for the paths it
// drives.

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import { readGrandfatheredRoster } from './joinLabelRoster.js';
import { stripComments } from './stripComments.js';
import { sourceFiles } from './sourceFiles.js';

/** Production sources: no tests, no generated proto, no declaration files. */
const SCAN_SCOPE = { skipDirs: ['__tests__', 'proto'], includeDts: false } as const;

const SRC = new URL('..', import.meta.url).pathname;

/** A spread of the join label bag, in either spelling the SDK uses. */
const JOIN_BAG_SPREAD = /\.\.\.\s*(this\.#)?metricLabels\b/;
const METRIC_LITERAL = /'(dt_client_[a-z0-9_]+)'/g;

/** `dt_client_*` literals in a source that spreads the join bag; `[]` if it does not. */
export function spreadSiteLiterals(source: string): { spreads: boolean; names: string[] } {
  const code = stripComments(source);
  if (!JOIN_BAG_SPREAD.test(code)) return { spreads: false, names: [] };
  return { spreads: true, names: [...code.matchAll(METRIC_LITERAL)].map((m) => m[1] ?? '') };
}

describe('the join label bag is spread only by grandfathered metrics (static)', () => {
  const roster = readGrandfatheredRoster();

  it('reads a NON-EMPTY roster from client.md (extraction control, not a content result)', () => {
    expect(
      roster.length,
      'EXTRACTION FAILURE: the Grandfathered member table in docs/observability/metrics/client.md ' +
        'parsed to nothing — fix the parser or the table, do not loosen this check',
    ).toBeGreaterThan(0);
    expect(roster).toContain('dt_client_mh_connection_total');
  });

  it('finds the known spread sites (scan control)', () => {
    const spreading = sourceFiles(SRC, SCAN_SCOPE).filter(
      (f) => spreadSiteLiterals(readFileSync(f, 'utf8')).spreads,
    );
    expect(
      spreading.map((f) => f.slice(SRC.length)).sort(),
      'EXTRACTION FAILURE: no join-bag spread site found — the scan is matching nothing',
    ).toEqual(expect.arrayContaining(['media/MediaTransport.ts', 'session/MeetingSession.ts']));
  });

  it('every metric literal in a spreading file is on the roster', () => {
    const violations: string[] = [];
    for (const file of sourceFiles(SRC, SCAN_SCOPE)) {
      const { spreads, names } = spreadSiteLiterals(readFileSync(file, 'utf8'));
      if (!spreads) continue;
      for (const name of names) {
        if (!roster.includes(name)) violations.push(`${file.slice(SRC.length)}: ${name}`);
      }
    }
    expect(
      violations,
      'VIOLATION: a non-grandfathered metric shares a file with the join bag',
    ).toEqual([]);
  });

  it('NEGATIVE CONTROL: a non-roster metric beside a join-bag spread is caught', () => {
    const synthetic = [
      "this.#metricsSink?.counter('dt_client_media_new_thing_total', {",
      '  ...this.#metricLabels,',
      '});',
    ].join('\n');
    const { spreads, names } = spreadSiteLiterals(synthetic);
    expect(spreads).toBe(true);
    expect(names.filter((n) => !roster.includes(n))).toEqual(['dt_client_media_new_thing_total']);
  });
});
