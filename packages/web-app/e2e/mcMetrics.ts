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
// PER-INSTANCE counter-delta semantics (robust to pod rollover). Every helper
// here reads a `sum by (instance)(...)` snapshot as a MAP {instance -> value}
// and passes when ANY currently-present instance's value exceeds its OWN
// baseline (an instance absent from the baseline — a fresh pod after a rollover
// — counts once its value exceeds zero). The pure decision + parsing live in
// `./instanceCounters`, the deliberate mirror of the Rust
// `crates/env-tests/src/fixtures/metrics.rs` twin. Cross-language parity is held
// by convention + mirrored unit tests, NOT structurally — nothing enforces the
// two stay identical, so a change to either side must add a matching case to
// both test suites (the NaN/+Inf/empty-sample cases exist because the runtimes'
// default coercions genuinely diverged there). The `> baseline` direction stays
// safe against concurrent INCREMENTS (a concurrent suite can only push a pod's
// counter further up).
//
// WHY per-instance, and why the label is `instance` (NOT `pod`) — confirmed by
// @observability against `infra/kubernetes/observability/prometheus-config.yaml`:
// the mc/mh scrape jobs use `role: pod` with relabel_configs that only `keep` on
// the app label + container port; no relabel emits a `pod` target_label, so
// Prometheus's default `instance` = `__address__` = pod IP:port is the per-pod
// identity, fresh on every rollover. Two traps this depends on:
//   1. A `pod` target_label exists ONLY in
//      `infra/kubernetes/observability/promtail-config.yaml` (logs/Loki) — NEVER
//      the metrics path. For metrics the identity is `instance`.
//   2. The mc/mh Deployments carry a STABLE pod metadata label `instance: mc-0`
//      that is NOT propagated into series today (no `labelmap` in the mc/mh
//      jobs). Adding `labelmap __meta_kubernetes_pod_label_(.+)` later would
//      override the default `instance` with that stable value and SILENTLY break
//      this fix (a rolled pod would keep `instance=mc-0`, so no "new instance"
//      ever appears). Do not add such a labelmap without revisiting this module.
// Staleness: each rollover mints a new `instance` series; the old one persists
// up to Prometheus's staleness window (~5min, `query.lookback-delta` default)
// then vanishes. Both may be transiently present during an assertion window —
// the per-instance decision tolerates the CHURN direction (an expiring series
// just drops out of the map, never inflating another instance). This replaces
// the former cluster-wide `sum(...)` which the old-pod series (still inside the
// staleness window at baseline time) INFLATED, timing out the fresh pods'
// assertion (26_mh_quic counter-delta flake, 2026-08-05 / 2026-08-28 — see
// docs/TODO.md §Env-Test Resilience & Runbook Validation).
//
// RESIDUAL FALSE-PASS WINDOW (@security, Gate 3): it is NOT false-pass-proof "by
// construction". An instance absent from the baseline is compared against 0, so
// a *pre-existing* pod that was merely missing from the baseline READ (unscraped
// beyond `query.lookback-delta`, or a transient empty result) and then reappears
// carries its full prior value past the check without the test having driven any
// increment. The old cluster-wide form could not produce this (its churn failure
// was one-directional, false-negative only); rollover-robustness was traded for
// this narrow window. Full rationale on `anyInstanceExceedsBaseline` in
// `./instanceCounters`.

import { e2eEnv } from './env.js';
import {
  anyInstanceExceedsBaseline,
  formatInstanceMap,
  resultsToInstanceMap,
  type InstanceCounters,
  type PromResultRow,
} from './instanceCounters.js';

const CONNECTED_SUM_PROMQL = 'sum by (instance) (mc_participant_mh_status_total{state="connected"})';

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
// We aggregate over ALL reasons (NO label filter) on purpose: a Playwright
// `context.close()` does not deterministically drive a clean WebTransport
// CONNECTION_CLOSE, so the reason MC assigns can be `voluntary` (clean close →
// ClientClosed → immediate) OR `timeout` (ambiguous loss → ConnectionLost →
// grace expiry). Aggregating every reason is immune to that classification
// variance while staying strictly monotonic. NOTE (per-instance honesty,
// @observability): unlike the `{state=...}`/`{event_type=...}` selectors, this
// query is UNFILTERED, so `sum by (instance)` collapses the counter's up-to-4
// `reason` series WITHIN each pod into one per-pod total-across-reasons — that
// per-pod total is exactly what the monotonic per-instance delta check wants.
// So it drops into `pollUntilAnyInstanceAbove` unchanged. Per-participant
// attribution is proven separately by the bus `participantLeft(participantId)`
// event; this counter is the SERVER-SIDE proof MC actually recorded the leave
// (a dead/unreachable MC cannot increment its own counter — same false-pass
// killer as the join-failure helper above). The spec ALSO drives the demo's own
// teardown (unmount → session.disconnect() → clean voluntary close), so the
// common path settles in ~1 scrape interval; the wide budget below is grace-path
// insurance, not the expected latency.
const LEAVES_SUM_PROMQL = 'sum by (instance) (mc_participant_leaves_total)';

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
  return `sum by (instance) (mc_session_join_failures_total{error_type="${errorType}"})`;
}

interface PromQueryResponse {
  readonly status?: string;
  readonly data?: {
    readonly result?: readonly PromResultRow[];
  };
}

/**
 * Current PER-INSTANCE snapshot for `promql` (a `sum by (instance)(...)` query),
 * as a map {instance -> value}. An EMPTY instant vector (series never yet
 * observed anywhere — e.g. a failure-only counter before its first occurrence)
 * yields an EMPTY map, the legitimately-zero case. Query/transport/status
 * failures THROW (global-setup proved Prometheus healthy; a failure here is a
 * real problem, NOT an absent series) — the fail-loud contract the Rust twin
 * (`instance_counter_map`) now matches. Empty-vector and query-error are kept
 * strictly distinct.
 */
async function promInstantByInstance(promql: string): Promise<InstanceCounters> {
  const url = `${e2eEnv.prometheusUrl}/api/v1/query?query=${encodeURIComponent(promql)}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(5000) });
  // The trailing clause is a CROSS-LANGUAGE grep key (matches the Rust
  // `instance_map_from_query` panic): `Triage Prometheus/port-forward` routes a
  // mid-suite Prometheus failure to the operator lane in both the env-test and
  // browser-E2E logs (docs/runbooks/devloop-validation.md §8).
  const failLoudTail =
    'Global-setup proved Prometheus healthy, so a query error here is a real problem — ' +
    'NOT a zero reading. Triage Prometheus/port-forward, not the service under test.';
  if (!response.ok) {
    throw new Error(`Prometheus query failed: ${url} responded ${response.status}. ${failLoudTail}`);
  }
  const body = (await response.json()) as PromQueryResponse;
  if (body.status !== 'success') {
    throw new Error(
      `Prometheus query returned status "${body.status ?? 'undefined'}" (${url}). ${failLoudTail}`,
    );
  }
  return resultsToInstanceMap(promql, body.data?.result ?? []);
}

/** Poll options shared by every counter-delta wait (defaults match the Rust Scenario 7 helper). */
interface PollOptions {
  readonly timeoutMs?: number;
  readonly intervalMs?: number;
}

/**
 * The ONE poll loop (the counter-delta helpers must not re-encode the poll
 * idiom): wait until SOME currently-present instance's value for `promql`
 * exceeds its OWN baseline (see {@link anyInstanceExceedsBaseline}), else throw
 * `timeoutMessage(lastObserved, timeoutMs)`.
 *
 * `lastObserved` is the CURRENT per-instance map rendered as a string — the
 * triage aid that replaces the old (now-incorrect) below-baseline "churn hint":
 * under per-instance comparison a monotonic value never spuriously drops below
 * its baseline (stale series just disappear from the map), so a timeout is
 * genuinely "no present instance incremented" — and the printed map shows which
 * instances were seen (a fresh post-rollover pod at 0, an old pod that never
 * moved, etc.).
 */
async function pollUntilAnyInstanceAbove(
  promql: string,
  baseline: InstanceCounters,
  { timeoutMs = 60_000, intervalMs = 2_000 }: PollOptions,
  timeoutMessage: (lastObserved: string, timeoutMs: number) => string,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const current = await promInstantByInstance(promql);
    if (anyInstanceExceedsBaseline(baseline, current)) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(timeoutMessage(formatInstanceMap(current), timeoutMs));
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, intervalMs));
  }
}

/** Current per-instance `mc_participant_mh_status_total{state="connected"}` snapshot. */
export async function mcConnectedStatusByInstance(): Promise<InstanceCounters> {
  return promInstantByInstance(CONNECTED_SUM_PROMQL);
}

/**
 * Poll until the connected-status counter rises past `baseline` on some
 * instance. Throws with the current per-instance map on timeout (default budget
 * matches the Rust Scenario 7 helper).
 */
export async function waitForMcConnectedStatusAbove(
  baseline: InstanceCounters,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilAnyInstanceAbove(
    CONNECTED_SUM_PROMQL,
    baseline,
    options,
    (lastObserved, timeoutMs) =>
      `mc_participant_mh_status_total{state="connected"} did not rise above baseline ` +
      `${formatInstanceMap(baseline)} on any instance within ${timeoutMs}ms (last observed: ` +
      `${lastObserved}). The browser SDK either did not send MediaConnectionUpdate(state=CONNECTED) ` +
      `or MC did not record it (R-60 handler, story task #6). Compare the Rust driver: ` +
      `crates/env-tests/tests/26_mh_quic.rs Scenario 7.`,
  );
}

/** Current per-instance `mc_participant_leaves_total` snapshot (all reasons — see anchor). */
export async function mcParticipantLeavesByInstance(): Promise<InstanceCounters> {
  return promInstantByInstance(LEAVES_SUM_PROMQL);
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
  baseline: InstanceCounters,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilAnyInstanceAbove(
    LEAVES_SUM_PROMQL,
    baseline,
    { timeoutMs: 90_000, ...options },
    (lastObserved, timeoutMs) =>
      `mc_participant_leaves_total (all reasons) did not rise above baseline ` +
      `${formatInstanceMap(baseline)} on any instance within ${timeoutMs}ms (last observed: ` +
      `${lastObserved}). MC never recorded the participant departure — either the ParticipantLeft ` +
      `broadcast did not fire (roster-removal choke-point crates/mc-service/src/actors/meeting.rs ` +
      `remove_and_broadcast_left) or it exceeded the config-derived worst-case ` +
      `(idle_timeout + grace_period + grace-check + scrape SLA; see ` +
      `docs/observability/metrics/mc-service.md).`,
  );
}

/** Current per-instance `mc_session_join_failures_total{error_type=<errorType>}` snapshot. */
export async function mcSessionJoinFailureByInstance(
  errorType: McJoinFailureErrorType,
): Promise<InstanceCounters> {
  return promInstantByInstance(joinFailureSumPromql(errorType));
}

/**
 * Poll until MC's join-failure counter for `errorType` rises past `baseline` on
 * some instance — the server-side proof MC actively rejected a join at its auth
 * gate (see the {@link McJoinFailureErrorType} label anchor). Throws with the
 * current per-instance map on timeout.
 */
export async function waitForMcSessionJoinFailureAbove(
  baseline: InstanceCounters,
  errorType: McJoinFailureErrorType,
  options: PollOptions = {},
): Promise<void> {
  await pollUntilAnyInstanceAbove(
    joinFailureSumPromql(errorType),
    baseline,
    options,
    (lastObserved, timeoutMs) =>
      `mc_session_join_failures_total{error_type="${errorType}"} did not rise above ` +
      `baseline ${formatInstanceMap(baseline)} on any instance within ${timeoutMs}ms (last ` +
      `observed: ${lastObserved}). MC never recorded the expected join rejection — either the ` +
      `join never reached MC's auth gate (a transport-level failure BEFORE the gate is a ` +
      `different bug than the rejection this spec drives) or MC classified it under a different ` +
      `error_type (crates/mc-service/src/errors.rs:error_type_label()).`,
  );
}
