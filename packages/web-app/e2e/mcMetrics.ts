// File: packages/web-app/e2e/mcMetrics.ts
//
// MC-side Prometheus observations for the browser E2E suite. This module is the
// ONE home of every PromQL literal the suite uses — specs never inline PromQL.
//
// Task #18 assertion (d): the browser SDK's post-join `MediaConnectionUpdate`
// (state=CONNECTED) was received AND recorded by MC's R-60 handler (story task
// #6), read through Prometheus.
//
// Task #19 (mc-token-rejection): MC's join-time auth-gate rejection counter —
// the server-side proof that MC ACTIVELY rejected a join (a dead/unreachable MC
// yields a client-side Transport error but structurally cannot increment MC's
// own rejection counter, so this delta kills that false-pass).
//
// CROSS-DRIVER PARALLEL (deliberate, reviewed): the connected-status helper
// asserts the SAME series as the Rust env-test `crates/env-tests/tests/26_mh_quic.rs`
// Scenario 7 (`mc_participant_mh_status_total`, recorded in
// `crates/mc-service/src/observability/metrics.rs:record_participant_mh_status`)
// with a DIFFERENT driver — the real browser SDK over the real WebTransport API
// instead of a synthetic Rust client. Same 60s budget / 2s interval as the Rust
// helper (chain: WT frame -> MC bridge-loop decode -> participant actor record ->
// counter -> Prometheus scrape, 15s scrape SLA).
//
// The `> baseline` direction on a monotonic counter is safe against concurrent
// INCREMENTS: concurrent suites (e.g. the Rust env-tests) can only push the sum
// further up. It is NOT safe against series CHURN: an MC pod rollout inside
// Prometheus's staleness window (e.g. Layer-7 Phase-1c rebuild-all still
// draining old pods) leaves the old pods' series in the instant vector at
// baseline time and expires them mid-assertion — the sum DROPS, the baseline is
// inflated, and the wait times out (a false NEGATIVE, loud; never a false
// pass). Exactly this bit the Rust twin on 2026-08-05 (26_mh_quic
// counter-delta flake — diagnosis in
// docs/devloop-outputs/2026-08-04-e2e-negative-specs-pipeline/main.md §Issues).
// The deferred env-tests-crate hardening (settle-wait / per-current-pod query)
// must sweep these TS mirrors too.

import { e2eEnv } from './env.js';

const CONNECTED_SUM_PROMQL = 'sum(mc_participant_mh_status_total{state="connected"})';

// Task-64 departure counter (browser-E2E gap (2), leave/teardown/rejoin).
//
// LABEL ANCHOR (@observability): `mc_participant_leaves_total` is incremented
// exactly ONCE per `ParticipantLeft` broadcast at MC's single roster-removal
// choke-point (`crates/mc-service/src/observability/metrics.rs:442`
// `record_participant_leave`, called from `actors::meeting`
// `remove_and_broadcast_left`). Its `reason` label is the bounded, enum-derived
// set `voluntary | timeout | removed | meeting_ended` (`actors/messages.rs`
// `LeaveReason::label()`, EXHAUSTIVE match — a new variant is a compile error).
//
// We sum over ALL reasons (NO label filter) on purpose: a Playwright
// `context.close()` does not deterministically drive a clean WebTransport
// CONNECTION_CLOSE, so the reason MC assigns can be `voluntary` (clean close →
// ClientClosed → immediate) OR `timeout` (ambiguous loss → ConnectionLost →
// grace expiry). Summing every reason is immune to that classification variance
// while staying strictly monotonic — so it drops into `pollUntilSumAbove`
// (`> baseline`) unchanged and inherits the series-churn hint. Per-participant
// attribution is proven separately by the bus `participantLeft(participantId)`
// event; this counter is the SERVER-SIDE proof MC actually recorded the leave
// (a dead/unreachable MC cannot increment its own counter — same false-pass
// killer as the join-failure helper above). The spec ALSO drives the demo's own
// teardown (unmount → session.disconnect() → clean voluntary close), so the
// common path settles in ~1 scrape interval; the wide budget below is grace-path
// insurance, not the expected latency.
const LEAVES_SUM_PROMQL = 'sum(mc_participant_leaves_total)';

/**
 * Bounded MC join-failure `error_type` label values this suite asserts on.
 *
 * LABEL ANCHOR (@observability, task #19 Gate 1): values come from
 * `crates/mc-service/src/errors.rs:error_type_label()` (enum-variant-derived,
 * ADR-0011 bounded cardinality). `jwt_validation` is what BOTH join-time auth
 * rejections in `crates/mc-service/src/webtransport/connection.rs` record —
 * step 5 (token signature/expiry invalid) and step 6 (token `meeting_id`
 * binding mismatch, the path mc-token-rejection.spec.ts drives) both emit
 * `mc_session_join_failures_total{error_type="jwt_validation"}`. NB: MC's
 * `SessionBinding` error maps to a DIFFERENT label (`session_binding`) — if MC
 * ever reclassifies the step-6 mismatch onto it, this type (and the spec) must
 * follow; the spec then fails loudly rather than silently passing on the wrong
 * series. Typed as a literal union so a typo'd label is a compile error, not a
 * spec that can never pass (@code-reviewer, task #19 Gate 1).
 */
export type McJoinFailureErrorType = 'jwt_validation';

function joinFailureSumPromql(errorType: McJoinFailureErrorType): string {
  return `sum(mc_session_join_failures_total{error_type="${errorType}"})`;
}

interface PromQueryResponse {
  readonly status?: string;
  readonly data?: {
    readonly result?: ReadonlyArray<{ readonly value?: readonly [number, string] }>;
  };
}

/**
 * Current cluster-wide instant sum for `promql`. An empty instant vector
 * (series never yet observed) reads as 0 — mirrors the Rust helper's
 * `unwrap_or(0.0)`. Query/transport failures THROW (global-setup proved
 * Prometheus healthy; a failure here is a real problem, not an absent series).
 */
async function promInstantSum(promql: string): Promise<number> {
  const url = `${e2eEnv.prometheusUrl}/api/v1/query?query=${encodeURIComponent(promql)}`;
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
    throw new Error(`Prometheus returned a non-numeric sample "${raw}" for ${promql}`);
  }
  return parsed;
}

/** Poll options shared by every counter-delta wait (defaults match the Rust Scenario 7 helper). */
interface PollOptions {
  readonly timeoutMs?: number;
  readonly intervalMs?: number;
}

/**
 * The ONE poll loop (extracted for task #19 — the second counter-delta helper
 * must not re-encode the poll idiom): wait until the instant sum of `promql`
 * exceeds `baseline`, else throw `timeoutMessage(lastObserved, timeoutMs)`.
 */
async function pollUntilSumAbove(
  promql: string,
  baseline: number,
  { timeoutMs = 60_000, intervalMs = 2_000 }: PollOptions,
  timeoutMessage: (lastObserved: number, timeoutMs: number) => string,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const current = await promInstantSum(promql);
    if (current > baseline) {
      return;
    }
    if (Date.now() > deadline) {
      // Churn hint appended CENTRALLY so every counter-delta consumer inherits
      // it (@test/@observability, task #19 review): a monotonic sum observed
      // BELOW its baseline can only mean series churn — an MC pod rollout
      // expired the old-pod series that inflated the baseline — never a
      // missing increment. The Rust twin's identical flake cost a forensics
      // session (2026-08-05); this makes the next triage a read.
      const churnHint =
        current < baseline
          ? ` NB: last observed is BELOW the baseline — the monotonic sum went DOWN, ` +
            `which means series churn from an MC pod rollout (stale old-pod series ` +
            `inflated the baseline, then expired), not a missing increment. See the ` +
            `module-header staleness caveat and docs/TODO.md §"Env-Test Resilience & ` +
            `Runbook Validation" (stale-series-robust baseline capture).`
          : '';
      throw new Error(timeoutMessage(current, timeoutMs) + churnHint);
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, intervalMs));
  }
}

/** Current cluster-wide `mc_participant_mh_status_total{state="connected"}` sum. */
export async function mcConnectedStatusSum(): Promise<number> {
  return promInstantSum(CONNECTED_SUM_PROMQL);
}

/**
 * Poll until the connected-status sum exceeds `baseline`. Throws with the last
 * observed value on timeout (default budget matches the Rust Scenario 7 helper).
 */
export async function waitForMcConnectedStatusAbove(
  baseline: number,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilSumAbove(
    CONNECTED_SUM_PROMQL,
    baseline,
    options,
    (lastObserved, timeoutMs) =>
      `mc_participant_mh_status_total{state="connected"} did not rise above baseline ` +
      `${baseline} within ${timeoutMs}ms (last observed: ${lastObserved}). The browser SDK ` +
      `either did not send MediaConnectionUpdate(state=CONNECTED) or MC did not record ` +
      `it (R-60 handler, story task #6). Compare the Rust driver: ` +
      `crates/env-tests/tests/26_mh_quic.rs Scenario 7.`,
  );
}

/** Current cluster-wide `mc_participant_leaves_total` sum (all reasons — see anchor). */
export async function mcParticipantLeavesSum(): Promise<number> {
  return promInstantSum(LEAVES_SUM_PROMQL);
}

/**
 * Poll until MC's all-reasons participant-leave counter exceeds `baseline` — the
 * server-side proof that MC recorded a roster departure (gap (2)). Throws with
 * the last observed value on timeout.
 *
 * BUDGET (config-derived, NOT a fixed rationale — @observability): the ceiling
 * must cover MC's worst-case roster-remove latency, catalogued in
 * `docs/observability/metrics/mc-service.md` §"Worst-case roster-remove latency"
 * as `MC_QUIC_MAX_IDLE_TIMEOUT_SECONDS + MC_DISCONNECT_GRACE_PERIOD_SECONDS +
 * 5s grace-check` (defaults 10 + 30 + 5 = 45s), PLUS the ~15s Prometheus scrape
 * SLA this counter is read through ≈ 60s — so the 90s default is that worst case
 * with margin (and stays under the 120s/test ceiling). This is LARGER than the
 * join-side 60s helper precisely because a departure's worst case includes the
 * disconnect grace path. In practice the spec drives a clean close, so this
 * resolves in ~1 scrape interval; 90s is the grace-path safety net, not the norm.
 */
export async function waitForMcParticipantLeavesAbove(
  baseline: number,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilSumAbove(
    LEAVES_SUM_PROMQL,
    baseline,
    { timeoutMs: 90_000, ...options },
    (lastObserved, timeoutMs) =>
      `mc_participant_leaves_total (all reasons) did not rise above baseline ` +
      `${baseline} within ${timeoutMs}ms (last observed: ${lastObserved}). MC never ` +
      `recorded the participant departure — either the ParticipantLeft broadcast did ` +
      `not fire (roster-removal choke-point crates/mc-service/src/actors/meeting.rs ` +
      `remove_and_broadcast_left) or it exceeded the config-derived worst-case ` +
      `(idle_timeout + grace_period + grace-check + scrape SLA; see ` +
      `docs/observability/metrics/mc-service.md). (A below-baseline reading gets the ` +
      `series-churn hint appended centrally by the poll loop.)`,
  );
}

/** Current cluster-wide `mc_session_join_failures_total{error_type=<errorType>}` sum. */
export async function mcSessionJoinFailureSum(errorType: McJoinFailureErrorType): Promise<number> {
  return promInstantSum(joinFailureSumPromql(errorType));
}

/**
 * Poll until MC's join-failure counter for `errorType` exceeds `baseline` —
 * the server-side proof MC actively rejected a join at its auth gate (see the
 * {@link McJoinFailureErrorType} label anchor). Throws with the last observed
 * value on timeout.
 */
export async function waitForMcSessionJoinFailureAbove(
  baseline: number,
  errorType: McJoinFailureErrorType,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilSumAbove(
    joinFailureSumPromql(errorType),
    baseline,
    options,
    (lastObserved, timeoutMs) =>
      `mc_session_join_failures_total{error_type="${errorType}"} did not rise above ` +
      `baseline ${baseline} within ${timeoutMs}ms (last observed: ${lastObserved}). ` +
      `MC never recorded the expected join rejection — either the join never reached ` +
      `MC's auth gate (a transport-level failure BEFORE the gate is a different bug ` +
      `than the rejection this spec drives) or MC classified it under a different ` +
      `error_type (crates/mc-service/src/errors.rs:error_type_label()). ` +
      `(A below-baseline reading gets the series-churn hint appended centrally ` +
      `by the poll loop.)`,
  );
}
