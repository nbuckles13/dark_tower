// File: packages/sdk-core/src/telemetry/nameGuard.ts
//
// R-24: the `dt_client_*` metric-name guard at the sink boundary. Production
// sinks (`OtelMetricsSink`, `ConsoleMetricsSink`) run every metric name through
// `assertClientMetricName` before emitting. `NoopMetricsSink` does not (it
// emits nothing) and the test-only `InMemoryMetricsSink` does not (passive
// recorder — it must accept non-compliant names to test guard-wrapping).
//
// LOCKSTEP WITH CI: the regex below is COPIED VERBATIM from the compiled
// `crates/dt-guard/src/ts_metric_naming.rs:68` static `R26_NAME_RE`, NOT from
// the `{0,53}` doc-comment form that appears elsewhere. The two forms diverge
// on a trailing underscore and the single-char body: `dt_client_foo_` passes
// `{0,53}` but FAILS the compiled rule. Using the compiled pattern here keeps
// the RUNTIME guard and the CI static guard from drifting — a name that passes
// this guard at runtime also passes `dt-guard` in CI, and vice-versa. If the
// CI regex ever changes, change THIS line and the lockstep regression test in
// `__tests__/nameGuard.test.ts` in the same commit.
const DT_CLIENT_NAME_RE = /^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$/;

/**
 * How a guard violation is handled. Runtime-injectable seam (NOT a build-time
 * `import.meta.env.PROD` constant-fold) so a single test config can exercise
 * BOTH branches:
 *   - `'throw'` — dev/test: a non-compliant name is a programming error and
 *     must fail loudly so it is caught before it ships.
 *   - `'warn'`  — prod: never crash a user's join flow over a metric name;
 *     `console.warn` and drop the emission (the sink no-ops the bad metric).
 */
export type GuardMode = 'throw' | 'warn';

/**
 * Validate a client metric name against the canonical `dt_client_*` convention.
 *
 * @returns `true` if the name is compliant and emission should proceed;
 *   `false` if it was rejected in `'warn'` mode (caller must skip the emit).
 * @throws if the name is non-compliant and `mode` is `'throw'`.
 */
export function assertClientMetricName(name: string, mode: GuardMode): boolean {
  if (DT_CLIENT_NAME_RE.test(name)) {
    return true;
  }
  const message =
    `Metric name ${JSON.stringify(name)} violates the dt_client_* naming ` +
    `convention (must match ${DT_CLIENT_NAME_RE.source}). ` +
    `See docs/observability/metrics/client.md.`;
  if (mode === 'throw') {
    throw new Error(message);
  }
  // prod: warn and drop — never throw inside a user's join flow.
  console.warn(`[darktower-sdk-core] ${message} (metric dropped)`);
  return false;
}

/** Exposed for the lockstep regression test; not part of the public barrel. */
export const DT_CLIENT_NAME_PATTERN = DT_CLIENT_NAME_RE;
