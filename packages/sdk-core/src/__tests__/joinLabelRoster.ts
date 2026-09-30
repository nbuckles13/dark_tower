// File: packages/sdk-core/src/__tests__/joinLabelRoster.ts
//
// Test helpers for the join-label-bag rule (ADR-0036 §11; story 2 task 13): the
// join-flow label bag carrying `meeting_id_hash` may be spread ONLY by metrics on
// the frozen grandfathered roster in `docs/observability/metrics/client.md`.
//
// The roster is READ from that file, never restated here — a second copy would
// be a second home that drifts. The collector-side strip makes a violation of
// this SDK rule harmless AND invisible at once, so these checks are what protect
// the rule while the strip protects the store.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { repoRoot } from '../media/frame/__tests__/repoRoot.js';

/** The label that marks the join bag. */
export const JOIN_BAG_MARKER = 'meeting_id_hash';

/**
 * The grandfathered members, parsed from the `| Grandfathered member |` table in
 * `docs/observability/metrics/client.md`. Returns `[]` when the table is not
 * found — callers MUST assert non-emptiness with a message distinct from a
 * violation, or a moved table passes vacuously.
 */
export function readGrandfatheredRoster(): string[] {
  const catalog = readFileSync(
    join(repoRoot(), 'docs', 'observability', 'metrics', 'client.md'),
    'utf8',
  );
  const lines = catalog.split('\n');
  const header = lines.findIndex((l) => /^\s*\|\s*Grandfathered member\s*\|/.test(l));
  if (header < 0) return [];
  const members: string[] = [];
  // header, separator, then rows until the first non-table line.
  for (let i = header + 2; i < lines.length; i += 1) {
    const row = lines[i]?.trim() ?? '';
    if (!row.startsWith('|')) break;
    const name = /^\|\s*`(dt_client_[a-z0-9_]+)`\s*\|/.exec(row)?.[1];
    if (name) members.push(name);
  }
  return members;
}

/** One recorded emission, as `InMemoryMetricsSink` records it. */
export interface RecordedEmission {
  readonly name: string;
  readonly labels: Readonly<Record<string, unknown>>;
}

/** Names of emissions carrying the join bag that are NOT on the roster. */
export function joinBagViolations(
  emissions: readonly RecordedEmission[],
  roster: readonly string[],
): string[] {
  const allowed = new Set(roster);
  return [
    ...new Set(
      emissions
        .filter((e) => JOIN_BAG_MARKER in e.labels && !allowed.has(e.name))
        .map((e) => e.name),
    ),
  ];
}
