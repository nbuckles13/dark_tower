// File: packages/web-app/tests/clientMetricNames.test.ts
//
// Fail-closed drift guard for `e2e/clientMetricNames.ts` (story 2 R-27; owned
// with observability). What this file adds is the link to the SHARED ASSERTED
// NAMES in `e2e/clientMetricNames.ts`. Since story 2 task 16,
// `dt-guard client-metrics-export` also checks the alert side (rule 2: every
// `dt_client_*` name in a loaded rule is exported and every selected or grouped
// label is kept; G7: the reason alternation partitions the vectors' dropping
// tokens), but it knows nothing of this constant. The live read-back in the
// multi-party spec then proves the LOADED rule agrees.
//
// The rule is read STRUCTURALLY (the `- alert:` list item, then its `expr:` key
// and block scalar by indentation) rather than by a regex over the whole file, so
// a name in another rule, a comment or an annotation cannot satisfy it. A YAML
// library is not a dependency of this package; the extractor below reads the
// one shape Prometheus rule files use and fails loudly on anything else.
//
// Each positive control carries its OWN reason text, distinct from a content
// failure: "rule not found", "no names extracted", "no reason tokens".

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ALL_REJECT_REASONS, MEDIA_KIND_LABELS } from '@darktower/sdk-core';
import {
  ALL_ASSERTED_CLIENT_NAMES,
  MISSING_KEY_MATERIAL_ALERT,
  MISSING_KEY_MATERIAL_SELECTED_NAMES,
  MISSING_KEY_MATERIAL_GROUPING,
  MISSING_KEY_MATERIAL_SUM_COUNT,
  clientNamesIn,
  sumGroupingsIn,
  reasonAlternationIn,
  MEDIA_KIND_AUDIO,
} from '../e2e/clientMetricNames.js';

const REPO = resolve(__dirname, '../../..');
const RULES = resolve(REPO, 'infra/docker/prometheus/rules/mc-alerts.yaml');
const CLIENT_RULES = resolve(REPO, 'infra/docker/prometheus/rules/client-alerts.yaml');
const EMITTER = resolve(REPO, 'packages/sdk-core/src/media/setup/mediaMetrics.ts');

/**
 * The `expr` of the rule whose `alert:` equals `name`, or `undefined` if no such
 * rule. THROWS if the rule exists but its `expr` is not a recognisable scalar.
 */
function alertExpr(yamlText: string, name: string): string | undefined {
  const lines = yamlText.split('\n');
  const head = lines.findIndex((l) => new RegExp(`^(\\s*)- alert:\\s*${name}\\s*$`).test(l));
  if (head < 0) return undefined;
  const itemIndent = (lines[head]!.match(/^(\s*)-/)?.[1]?.length ?? 0) + 2;
  for (let i = head + 1; i < lines.length; i += 1) {
    const line = lines[i]!;
    if (line.trim() === '' || line.trim().startsWith('#')) continue;
    const indent = line.length - line.trimStart().length;
    if (indent < itemIndent) break; // the next list item or the end of this one
    if (indent !== itemIndent || !line.trimStart().startsWith('expr:')) continue;
    const value = line.trimStart().slice('expr:'.length).trim();
    if (!/^[|>][-+]?$/.test(value)) return value.replace(/^["']|["']$/g, '');
    const block: string[] = [];
    for (let j = i + 1; j < lines.length; j += 1) {
      const next = lines[j]!;
      if (next.trim() !== '' && next.length - next.trimStart().length <= itemIndent) break;
      block.push(next.trim());
    }
    return block.join('\n');
  }
  throw new Error(`rule ${name} has no expr: key at indent ${itemIndent}`);
}

describe('alertExpr (the structural extractor) — self-test', () => {
  const doc = [
    'groups:',
    '  - name: g',
    '    rules:',
    '      - alert: Other',
    '        expr: other_metric > 0',
    '      - alert: Target',
    '        # dt_client_in_a_comment_total',
    '        expr: |',
    '          sum(rate(dt_client_a_total[5m]))',
    '          /',
    '          sum(rate(dt_client_b_total[5m]))',
    '        annotations:',
    '          summary: "dt_client_in_annotation_total"',
    '      - alert: Next',
    '        expr: dt_client_next_total',
  ].join('\n');

  it('returns only the named rule block, excluding comments, annotations and neighbours', () => {
    const expr = alertExpr(doc, 'Target');
    expect(clientNamesIn(expr ?? '')).toEqual(new Set(['dt_client_a_total', 'dt_client_b_total']));
  });

  it('reads an inline expr', () => {
    expect(alertExpr(doc, 'Next')).toBe('dt_client_next_total');
  });

  it('returns undefined for an absent rule', () => {
    expect(alertExpr(doc, 'Missing')).toBeUndefined();
  });
});

describe('sumGroupingsIn — self-test', () => {
  it('reports grouped, bare and unrecognised sums in order', () => {
    expect(sumGroupingsIn('sum by (media_kind) (rate(a[5m])) / sum(rate(b[5m]))')).toEqual([
      'media_kind',
      '',
    ]);
    expect(sumGroupingsIn('sum without (x) (a)')).toEqual(['<unrecognised sum shape>']);
    expect(sumGroupingsIn('max(a)')).toEqual([]);
  });
});

describe(`${MISSING_KEY_MATERIAL_ALERT} <-> clientMetricNames <-> emitter`, () => {
  const expr = alertExpr(readFileSync(RULES, 'utf8'), MISSING_KEY_MATERIAL_ALERT);

  it('POSITIVE CONTROL (rule-not-found): the alert exists in the committed rule file', () => {
    expect(
      expr,
      `rule-not-found: no "- alert: ${MISSING_KEY_MATERIAL_ALERT}" in ${RULES}`,
    ).toBeDefined();
  });

  it('POSITIVE CONTROL (no-names-extracted): its expression names at least one dt_client_ series', () => {
    expect(
      clientNamesIn(expr ?? '').size,
      'no-names-extracted: the extractor found no dt_client_* name in the rule expression',
    ).toBeGreaterThan(0);
  });

  it('the names the rule selects are SET-EQUAL to MISSING_KEY_MATERIAL_SELECTED_NAMES', () => {
    expect(
      [...clientNamesIn(expr ?? '')].sort(),
      'selector-drift: the rule and the shared constant name different series',
    ).toEqual([...MISSING_KEY_MATERIAL_SELECTED_NAMES].sort());
  });

  it('POSITIVE CONTROL (no-reason-tokens): the rule selects a non-empty reason alternation', () => {
    expect(
      reasonAlternationIn(expr ?? '').length,
      'no-reason-tokens: no reason=~"..." alternation extracted from the rule',
    ).toBeGreaterThan(0);
  });

  it('every selected reason is one the SDK emits (a SUBSET of ALL_REJECT_REASONS)', () => {
    const known: readonly string[] = ALL_REJECT_REASONS;
    const unknown = reasonAlternationIn(expr ?? '').filter((r) => !known.includes(r));
    expect(unknown, 'reason-drift: the rule selects reasons the SDK never emits').toEqual([]);
  });

  it('is PER KIND: numerator, denominator AND guard are all `sum by (media_kind)`', () => {
    const groupings = sumGroupingsIn(expr ?? '');
    expect(
      groupings.length,
      `grouping-count: expected ${MISSING_KEY_MATERIAL_SUM_COUNT} sum aggregations (numerator, denominator, guard)`,
    ).toBe(MISSING_KEY_MATERIAL_SUM_COUNT);
    expect(
      groupings,
      'grouping-drift: every sum must group by media_kind; an ungrouped guard under `and` silently kills the rule',
    ).toEqual(Array(MISSING_KEY_MATERIAL_SUM_COUNT).fill(MISSING_KEY_MATERIAL_GROUPING));
  });

  it('every asserted name is a quoted literal the emitter actually registers', () => {
    const emitter = readFileSync(EMITTER, 'utf8');
    const missing = ALL_ASSERTED_CLIENT_NAMES.filter((n) => !emitter.includes(`'${n}'`));
    expect(missing, `emitter-drift: not registered in ${EMITTER}`).toEqual([]);
  });
});

describe('the R-29 media_kind read-back value', () => {
  it('is a value of the SDK media_kind vocabulary, and is `audio`', () => {
    // Imported, not retyped; pinned so a rename on the SDK side cannot silently
    // retarget the read-back to a kind that does not flow yet.
    expect(Object.values(MEDIA_KIND_LABELS)).toContain(MEDIA_KIND_AUDIO);
    expect(MEDIA_KIND_AUDIO).toBe('audio');
  });
});

/**
 * Every `alert:` name in a rules file, in file order (the list-item shape
 * `alertExpr` reads).
 */
function alertNames(yamlText: string): string[] {
  return [...yamlText.matchAll(/^\s*- alert:\s*(\S+)\s*$/gm)].map((m) => m[1]!);
}

/** Every value a `<label>="x"` or `<label>=~"a|b"` matcher selects in `expr`. */
function labelValuesIn(expr: string, label: string): string[] {
  const out: string[] = [];
  const re = new RegExp(`\\b${label}\\s*(=~|=)\\s*"([^"]*)"`, 'g');
  for (let m = re.exec(expr); m !== null; m = re.exec(expr)) {
    const body = m[2] ?? '';
    out.push(...(m[1] === '=~' ? body.split('|') : [body]).filter((t) => t !== ''));
  }
  return out;
}

/** A histogram series name back to the instrument the SDK registers. */
function instrumentName(series: string): string {
  return series.replace(/_(bucket|count|sum)$/, '');
}

// Story 3 OBS-B: the client rules' APPLY for NAMES and LABEL VALUES (the live
// pipe is the browser read-backs; label KEYS are `dt-guard client-metrics-export`
// rule 2). A selector naming a series, or a label value, the SDK never emits
// yields an empty result rather than an error, so it is checked here.
describe('client-alerts.yaml selectors <-> SDK vocabulary (story 3 OBS-B)', () => {
  const text = readFileSync(CLIENT_RULES, 'utf8');
  const rules = alertNames(text).map((name) => ({ name, expr: alertExpr(text, name) ?? '' }));
  const reasons = rules.flatMap((r) => labelValuesIn(r.expr, 'reason'));
  const kinds = rules.flatMap((r) => labelValuesIn(r.expr, 'media_kind'));

  it('POSITIVE CONTROL (no-client-rules): at least one rule is found, each with an expr', () => {
    expect(rules.length, `no-client-rules: no "- alert:" item in ${CLIENT_RULES}`).toBeGreaterThan(
      0,
    );
    const empty = rules.filter((r) => r.expr === '').map((r) => r.name);
    expect(empty, 'no-expr: rules whose expr was not extracted').toEqual([]);
  });

  it('POSITIVE CONTROL (no-reason-values): at least one reason value is selected', () => {
    expect(
      reasons.length,
      'no-reason-values: no reason matcher extracted from any client rule',
    ).toBeGreaterThan(0);
  });

  it('POSITIVE CONTROL (no-media-kind-values): at least one media_kind value is selected', () => {
    expect(
      kinds.length,
      'no-media-kind-values: no media_kind matcher extracted from any client rule',
    ).toBeGreaterThan(0);
  });

  it('every selected reason is in the SDK frame-reject vocabulary (ALL_REJECT_REASONS)', () => {
    const known: readonly string[] = ALL_REJECT_REASONS;
    const unknown = reasons.filter((r) => !known.includes(r));
    expect(
      unknown,
      'reason-value-drift: a client rule selects a reason the SDK never emits',
    ).toEqual([]);
    expect(known, 'sender_server_muted must be in the SDK video reject vocabulary (A5)').toContain(
      'sender_server_muted',
    );
  });

  it('every selected media_kind is a value of MEDIA_KIND_LABELS', () => {
    const known: readonly string[] = Object.values(MEDIA_KIND_LABELS);
    const unknown = kinds.filter((k) => !known.includes(k));
    expect(
      unknown,
      'media-kind-value-drift: a client rule selects a media_kind the SDK never emits',
    ).toEqual([]);
  });

  it('every dt_client_* name a client rule selects is a literal the emitter registers', () => {
    const emitter = readFileSync(EMITTER, 'utf8');
    const names = new Set(rules.flatMap((r) => [...clientNamesIn(r.expr)].map(instrumentName)));
    expect(
      names.size,
      'no-names-extracted: no dt_client_* name in any client rule',
    ).toBeGreaterThan(0);
    const missing = [...names].filter((n) => !emitter.includes(`'${n}'`));
    expect(
      missing,
      `emitter-drift: selected by a client rule but not registered in ${EMITTER}`,
    ).toEqual([]);
  });
});
