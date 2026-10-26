// File: packages/web-app/e2e/clientMetricNames.ts
//
// THE shared asserted-name constants for the client metrics the browser suite
// reads back out of Prometheus (story 2 R-27, S6). Owned by observability (the
// names and the alert they feed); client task 15 owns the assertions.
//
// No `./env` import, so node-tier vitest can load this module without the E2E
// environment. PromQL built from these names lives ONLY in `mcMetrics.ts` (the
// suite's one PromQL home); specs and fixtures never spell a `dt_client_*` name.
//
// DRIFT: `tests/clientMetricNames.test.ts` ties these to BOTH ends — the committed
// alert rule's expression (`infra/docker/prometheus/rules/mc-alerts.yaml`) and the
// emitter's literals (`packages/sdk-core/src/media/setup/mediaMetrics.ts`) —
// fail-closed. The live read-back ties them to the rule Prometheus LOADED.

import { MEDIA_KEK_SOURCES, publishedKindMetricLabels } from '@darktower/sdk-core';

/** The alert whose selector the R-27 exactness read-back checks. */
export const MISSING_KEY_MATERIAL_ALERT = 'MCMediaMissingKeyMaterial';

/**
 * The grouping labels EVERY `sum` in that alert's expression carries (story 3
 * OBS-B: the rule is per kind — numerator, denominator AND non-zero guard; both
 * kind labels since 2026-10-26, as `sumGroupingsIn` reports them). An
 * ungrouped guard under `and` would match no label set and silently kill the
 * rule, so the drift test requires all three.
 */
export const MISSING_KEY_MATERIAL_GROUPING = 'media_kind,content_kind';

/** How many `sum` aggregations that alert's expression has (numerator, denominator, guard). */
export const MISSING_KEY_MATERIAL_SUM_COUNT = 3;

/** Every `dt_client_*` name that alert's expression selects. Set-equal to the rule. */
export const MISSING_KEY_MATERIAL_SELECTED_NAMES = [
  'dt_client_media_frames_dropped_total',
  'dt_client_media_frames_received_total',
] as const;

/** The received-frames counter: the alert's denominator, and a multi-party read-back. */
export const RECEIVED_COUNTER =
  'dt_client_media_frames_received_total' satisfies (typeof MISSING_KEY_MATERIAL_SELECTED_NAMES)[number];

/** The send counter: R-27 part 1 (pipe liveness and `_total`-suffix survival). */
export const SEND_COUNTER = 'dt_client_media_frames_sent_total';

/** KEK arrivals by source (S6 asserts the rotation's `kek_update` arm). */
export const KEK_UPDATES = 'dt_client_media_kek_updates_total';

/** The `source` label value of a rotation push — imported, never retyped. */
export const KEK_UPDATE_SOURCE = MEDIA_KEK_SOURCES.KekUpdate;

/**
 * The `media_kind` + `content_kind` label values the R-29 read-backs select
 * (story 3 OBS-A; `content_kind` co-carried since 2026-10-26) — taken from the
 * SDK's ONE projection (`publishedKindMetricLabels`), never retyped
 * (`dt-guard ts-name-guard-dt-client`'s kind-literal rule covers this directory).
 * The audio read-back is `multi-party-hear.spec.ts`'s; the VIDEO read-back is
 * `multi-party-see.spec.ts`'s (story 3 C-C1). Mic and camera are both
 * `content_kind="main"`; nothing publishes screen share yet.
 */
export const MEDIA_KIND_AUDIO = publishedKindMetricLabels('audio').media_kind;
export const MEDIA_KIND_VIDEO = publishedKindMetricLabels('video').media_kind;
export const CONTENT_KIND_AUDIO = publishedKindMetricLabels('audio').content_kind;
export const CONTENT_KIND_VIDEO = publishedKindMetricLabels('video').content_kind;

/** Previous KEK generations demoted and retained — a COUNTER (S6 asserts a delta). */
export const KEK_GENERATIONS_RETAINED = 'dt_client_media_kek_generations_retained_total';

/** Every name above, for the emitter-literal drift check. */
export const ALL_ASSERTED_CLIENT_NAMES: readonly string[] = [
  ...MISSING_KEY_MATERIAL_SELECTED_NAMES,
  SEND_COUNTER,
  KEK_UPDATES,
  KEK_GENERATIONS_RETAINED,
];

/**
 * Pull every `dt_client_*` metric name out of a PromQL expression. Shared by the
 * static drift guard (the committed rule) and the live read-back (the LOADED
 * rule), so both compare the same way.
 */
export function clientNamesIn(expr: string): Set<string> {
  return new Set(expr.match(/dt_client_[a-z0-9_]+/g) ?? []);
}

/** The `reason=~"a|b|c"` alternation tokens in an expression (empty when absent). */
export function reasonAlternationIn(expr: string): string[] {
  const match = /reason=~"([^"]*)"/.exec(expr);
  return match?.[1] ? match[1].split('|').filter((t) => t !== '') : [];
}

/**
 * The grouping clause of every `sum` aggregation in a PromQL expression, in
 * order, whitespace removed: `'media_kind,content_kind'` for
 * `sum by (media_kind, content_kind) (`, `''` for a bare `sum(`.
 * A `sum` with any other modifier shape (e.g. `sum without (...)`) is reported as
 * the placeholder `'<unrecognised sum shape>'`, so the caller's equality check
 * fails loudly rather than skipping it.
 */
export function sumGroupingsIn(expr: string): string[] {
  const out: string[] = [];
  const re = /\bsum\s*(by\s*\(([^)]*)\)\s*)?\(/g;
  for (let m = re.exec(expr); m !== null; m = re.exec(expr)) {
    out.push(m[1] === undefined ? '' : (m[2] ?? '').replace(/\s+/g, ''));
  }
  const total = expr.match(/\bsum\b/g)?.length ?? 0;
  for (let i = out.length; i < total; i += 1) out.push('<unrecognised sum shape>');
  return out;
}
