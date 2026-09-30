// File: packages/web-app/tests/clientMetricNames.test.ts
//
// Fail-closed drift guard for `e2e/clientMetricNames.ts` (story 2 R-27; owned
// with observability). Nothing else links the alert's names to the emitter:
// `dt-guard application-metrics` cannot see `dt_client_*`, and the
// client-metrics-export guard does not check alert -> emitter. The live
// read-back in the multi-party spec then proves the LOADED rule agrees.
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
import { ALL_REJECT_REASONS } from '@darktower/sdk-core';
import {
  ALL_ASSERTED_CLIENT_NAMES,
  MISSING_KEY_MATERIAL_ALERT,
  MISSING_KEY_MATERIAL_SELECTED_NAMES,
  clientNamesIn,
  reasonAlternationIn,
} from '../e2e/clientMetricNames.js';

const REPO = resolve(__dirname, '../../..');
const RULES = resolve(REPO, 'infra/docker/prometheus/rules/mc-alerts.yaml');
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

  it('every asserted name is a quoted literal the emitter actually registers', () => {
    const emitter = readFileSync(EMITTER, 'utf8');
    const missing = ALL_ASSERTED_CLIENT_NAMES.filter((n) => !emitter.includes(`'${n}'`));
    expect(missing, `emitter-drift: not registered in ${EMITTER}`).toEqual([]);
  });
});
