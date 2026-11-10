// Node tier: the README's media-handler-set config table (story 4 C1) must equal
// the SDK's defaults and ranges. The SDK (`clientConfig.ts`
// `DEFAULT_CLIENT_CONFIG.media.handlers`, `HANDLER_SET_CONFIG_RANGES`) is the
// single source; the README restates it for operators, and this test is the
// drift signal. Expected values are IMPORTED, never retyped here.

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { DEFAULT_CLIENT_CONFIG, HANDLER_SET_CONFIG_RANGES } from '@darktower/sdk-core';

const README = new URL('../README.md', import.meta.url);

interface Row {
  readonly key: string;
  readonly defaultMs: number;
  readonly min: number;
  readonly max: number;
}

/** Parse the marked table: `| \`key\` | default | min–max | purpose |`. */
function parseTable(markdown: string): Row[] {
  const begin = markdown.indexOf('<!-- handler-set-config:begin -->');
  const end = markdown.indexOf('<!-- handler-set-config:end -->');
  if (begin < 0 || end < begin) throw new Error('handler-set-config markers not found');
  const rows: Row[] = [];
  for (const line of markdown.slice(begin, end).split('\n')) {
    const m = /^\|\s*`(\w+)`\s*\|\s*(\d+)\s*\|\s*(\d+)\s*[–-]\s*(\d+)\s*\|/.exec(line);
    if (m) rows.push({ key: m[1]!, defaultMs: +m[2]!, min: +m[3]!, max: +m[4]! });
  }
  return rows;
}

/** The SDK's truth, as rows. */
function sdkRows(): Row[] {
  const defaults = DEFAULT_CLIENT_CONFIG.media.handlers;
  return (Object.keys(HANDLER_SET_CONFIG_RANGES) as (keyof typeof defaults)[]).map((key) => ({
    key,
    defaultMs: defaults[key],
    min: HANDLER_SET_CONFIG_RANGES[key][0],
    max: HANDLER_SET_CONFIG_RANGES[key][1],
  }));
}

describe('README media-handler-set config table', () => {
  it('lists every SDK key with its exact default and inclusive range', () => {
    const rows = parseTable(readFileSync(README, 'utf8'));
    // Positive control: the table parsed at all (a vacuous [] would match nothing).
    expect(rows.length).toBeGreaterThan(0);
    expect(rows).toEqual(sdkRows());
  });

  it('DEMONSTRATES that it fires: a drifted default fails the comparison', () => {
    const drifted = readFileSync(README, 'utf8').replace(
      /\| `redialBaseMs` \| \d+ \|/,
      '| `redialBaseMs` | 123456 |',
    );
    expect(parseTable(drifted)).not.toEqual(sdkRows());
  });
});
