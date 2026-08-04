// File: packages/web-app/e2e/mcMetrics.ts
//
// Task #18 assertion (d): MC-side observation that the browser SDK's post-join
// `MediaConnectionUpdate` (state=CONNECTED) was received AND recorded by MC's
// R-60 handler (story task #6), read through Prometheus.
//
// CROSS-DRIVER PARALLEL (deliberate, reviewed): this asserts the SAME series as
// the Rust env-test `crates/env-tests/tests/26_mh_quic.rs` Scenario 7
// (`mc_participant_mh_status_total`, recorded in
// `crates/mc-service/src/observability/metrics.rs:record_participant_mh_status`)
// with a DIFFERENT driver — the real browser SDK over the real WebTransport API
// instead of a synthetic Rust client. Same 60s budget / 2s interval as the Rust
// helper (chain: WT frame -> MC bridge-loop decode -> participant actor record ->
// counter -> Prometheus scrape, 15s scrape SLA).
//
// The `> baseline` direction on a monotonic counter is concurrency-safe:
// concurrent suites (e.g. the Rust env-tests) can only push the sum further up,
// never produce a false negative.
//
// This module is the ONE home of the PromQL literal — specs never inline it.

import { e2eEnv } from './env.js';

const CONNECTED_SUM_PROMQL = 'sum(mc_participant_mh_status_total{state="connected"})';

interface PromQueryResponse {
  readonly status?: string;
  readonly data?: {
    readonly result?: ReadonlyArray<{ readonly value?: readonly [number, string] }>;
  };
}

/**
 * Current cluster-wide `mc_participant_mh_status_total{state="connected"}` sum.
 * An empty instant vector (series never yet observed) reads as baseline 0 —
 * mirrors the Rust helper's `unwrap_or(0.0)`. Query/transport failures THROW
 * (global-setup proved Prometheus healthy; a failure here is a real problem,
 * not an absent series).
 */
export async function mcConnectedStatusSum(): Promise<number> {
  const url = `${e2eEnv.prometheusUrl}/api/v1/query?query=${encodeURIComponent(
    CONNECTED_SUM_PROMQL,
  )}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!response.ok) {
    throw new Error(`Prometheus query failed: ${url} responded ${response.status}`);
  }
  const body = (await response.json()) as PromQueryResponse;
  if (body.status !== 'success') {
    throw new Error(`Prometheus query returned status "${body.status ?? 'undefined'}" (${url})`);
  }
  const raw = body.data?.result?.[0]?.value?.[1];
  if (raw === undefined) {
    return 0; // empty instant vector: series not yet observed anywhere
  }
  const parsed = Number.parseFloat(raw);
  if (Number.isNaN(parsed)) {
    throw new Error(
      `Prometheus returned a non-numeric sample "${raw}" for ${CONNECTED_SUM_PROMQL}`,
    );
  }
  return parsed;
}

/**
 * Poll until the connected-status sum exceeds `baseline`. Throws with the last
 * observed value on timeout (default budget matches the Rust Scenario 7 helper).
 */
export async function waitForMcConnectedStatusAbove(
  baseline: number,
  { timeoutMs = 60_000, intervalMs = 2_000 }: { timeoutMs?: number; intervalMs?: number } = {},
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const current = await mcConnectedStatusSum();
    if (current > baseline) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(
        `mc_participant_mh_status_total{state="connected"} did not rise above baseline ` +
          `${baseline} within ${timeoutMs}ms (last observed: ${current}). The browser SDK ` +
          `either did not send MediaConnectionUpdate(state=CONNECTED) or MC did not record ` +
          `it (R-60 handler, story task #6). Compare the Rust driver: ` +
          `crates/env-tests/tests/26_mh_quic.rs Scenario 7.`,
      );
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, intervalMs));
  }
}
