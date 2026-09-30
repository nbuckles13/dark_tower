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

import { MEDIA_KEK_SOURCES } from '@darktower/sdk-core';

/** The alert whose selector the R-27 exactness read-back checks. */
export const MISSING_KEY_MATERIAL_ALERT = 'MCMediaMissingKeyMaterial';

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
