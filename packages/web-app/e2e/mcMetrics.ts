// File: packages/web-app/e2e/mcMetrics.ts
//
// MC-side (and, for the story-2 S1 diagnostic, MH admission) Prometheus
// observations for the browser E2E suite. This module is the ONE home of every
// PromQL literal the suite uses — specs never inline PromQL.
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
// counter -> Prometheus scrape at the `mc-service` job's per-job
// `scrape_interval` in infra/kubernetes/observability/prometheus.yml).
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
// @observability against `infra/kubernetes/observability/prometheus.yml` (the file carrying `scrape_configs`):
// the mc/mh scrape jobs use `role: pod` with relabel_configs that only `keep` on
// the app label + container port; no relabel emits a `pod` target_label, so
// Prometheus's default `instance` = `__address__` = pod IP:port is the per-pod
// identity, fresh on every rollover. Two traps this depends on:
//   1. A `pod` target_label exists ONLY in
//      `infra/kubernetes/observability/promtail.yaml` (logs/Loki) — NEVER
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
import {
  KEK_GENERATIONS_RETAINED,
  KEK_UPDATES,
  KEK_UPDATE_SOURCE,
  CONTENT_KIND_AUDIO,
  CONTENT_KIND_VIDEO,
  MEDIA_KIND_AUDIO,
  MEDIA_KIND_VIDEO,
  RECEIVED_COUNTER,
  SEND_COUNTER,
  clientNamesIn,
} from './clientMetricNames.js';
import {
  classifyMissingSender,
  MH_ADMISSION_REJECTED_OUTCOME,
  MH_ADMISSION_REJECTION_RATIO,
  MH_ADMISSION_TOTAL,
  MH_EGRESS_EDGES,
  MH_EGRESS_STREAM_CEILING,
  MH_SCRAPE_JOB,
  type MissingSenderDiagnosis,
} from './s1Diagnostic.js';

const CONNECTED_SUM_PROMQL =
  'sum by (instance) (mc_participant_mh_status_total{state="connected"})';

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
  return resultsToInstanceMap(promql, await promInstantRows(promql));
}

/** The validated raw result rows of an instant query (fail-loud like every read here). */
async function promInstantRows(promql: string): Promise<readonly PromResultRow[]> {
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
    throw new Error(
      `Prometheus query failed: ${url} responded ${response.status}. ${failLoudTail}`,
    );
  }
  const body = (await response.json()) as PromQueryResponse;
  if (body.status !== 'success') {
    throw new Error(
      `Prometheus query returned status "${body.status ?? 'undefined'}" (${url}). ${failLoudTail}`,
    );
  }
  return body.data?.result ?? [];
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
  { timeoutMs = 60_000, intervalMs = 2_000, minDelta }: PollOptions & { minDelta?: number },
  timeoutMessage: (lastObserved: string, timeoutMs: number) => string,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const current = await promInstantByInstance(promql);
    if (anyInstanceExceedsBaseline(baseline, current, minDelta)) {
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
 * 5s grace-check` (defaults 10 + 30 + 5 = 45s), PLUS one scrape of the
 * `mc-service` job this counter is read through (its per-job `scrape_interval`,
 * no coarser than the global one) — under 60s, so the 90s default is that worst case
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

// ============================================================================
// Story 2 S1 diagnostic — MH egress admission (server-side, independent of the
// client metrics pipe). Names and classification: `./s1Diagnostic`.
// ============================================================================

const MH_REJECTED_SUM_PROMQL = `sum by (instance) (${MH_ADMISSION_TOTAL}{outcome="${MH_ADMISSION_REJECTED_OUTCOME}"})`;
const MH_RATIO_PROMQL = `max by (instance) (${MH_ADMISSION_REJECTION_RATIO})`;
const MH_CEILING_PROMQL = `max by (instance) (${MH_EGRESS_STREAM_CEILING})`;
const MH_EDGES_PROMQL = `max by (instance) (${MH_EGRESS_EDGES})`;
const MH_UP_PROMQL = `max by (instance) (up{job="${MH_SCRAPE_JOB}"})`;

/**
 * Per-MH-instance stream-ceiling rejection counter. Take this BEFORE a
 * multi-party scenario's first join and hand it to {@link diagnoseMissingSender}:
 * a baseline taken after the joins would hide the very rejection it looks for.
 */
export async function mhAdmissionRejectionsByInstance(): Promise<InstanceCounters> {
  return promInstantByInstance(MH_REJECTED_SUM_PROMQL);
}

/**
 * The S1 diagnostic: call it when a receiver is missing an expected sender and
 * put the returned `report` in the failure message.
 *
 * Waits up to `settleMs` for an in-window rejection to reach Prometheus before
 * concluding anything else — a rejection that happened after MH's last scrape
 * would otherwise read as misrouting. The default is three scrapes of the
 * `mh-service` job (`scrape_interval: 5s` in
 * `infra/kubernetes/observability/prometheus.yml`). This is a bounded wait for
 * evidence, never a latency assertion.
 */
export async function diagnoseMissingSender(
  baselineRejected: InstanceCounters,
  what: string,
  { settleMs = 15_000, intervalMs = 2_000 }: { settleMs?: number; intervalMs?: number } = {},
): Promise<MissingSenderDiagnosis> {
  const deadline = Date.now() + settleMs;
  let currentRejected = await promInstantByInstance(MH_REJECTED_SUM_PROMQL);
  while (!anyInstanceExceedsBaseline(baselineRejected, currentRejected) && Date.now() < deadline) {
    await new Promise((resolveSleep) => setTimeout(resolveSleep, intervalMs));
    currentRejected = await promInstantByInstance(MH_REJECTED_SUM_PROMQL);
  }
  const [rejectionRatio, streamCeiling, egressEdges, scrapeUp] = await Promise.all([
    promInstantByInstance(MH_RATIO_PROMQL),
    promInstantByInstance(MH_CEILING_PROMQL),
    promInstantByInstance(MH_EDGES_PROMQL),
    promInstantByInstance(MH_UP_PROMQL),
  ]);
  return classifyMissingSender(what, {
    baselineRejected,
    currentRejected,
    rejectionRatio,
    streamCeiling,
    egressEdges,
    scrapeUp,
  });
}

// ============================================================================
// Story 3 C-C1 — VIDEO failure diagnostics (MH pending / keyframe events, MC
// video slot states). FLEET / POD-LEVEL, DIAGNOSTICS ONLY: never a
// per-participant assertion (that comes from the participant's own bus). Names
// from `docs/observability/metrics/{mh,mc}-service.md`; every PromQL string of
// the suite lives in this module (`failureDiagnostics.ts` only orchestrates).
// ============================================================================

const MH_EDGES_PENDING_PROMQL = 'max by (instance) (mh_media_edges_pending)';
const MH_EDGE_PENDING_OLDEST_PROMQL = 'max by (instance) (mh_media_edge_pending_oldest_seconds)';
const MH_KEYFRAME_REQUESTS_SENT_PROMQL =
  'sum by (instance, trigger) (mh_media_keyframe_requests_sent_total)';
const MH_KEYFRAME_WRITE_FAILURES_PROMQL =
  'sum by (instance) (mh_media_keyframe_request_write_failures_total)';
const MC_VIDEO_SLOT_STATES_PROMQL = `sum by (instance, slot_state) (mc_media_slot_states_total{media_kind="${MEDIA_KIND_VIDEO}", content_kind="${CONTENT_KIND_VIDEO}"})`;
const MC_UNREACHABLE_SENDERS_PROMQL = 'sum by (instance) (mc_media_unreachable_senders_total)';

/** One labelled diagnostic read: its name and the raw rows, formatted. */
export interface DiagnosticRead {
  readonly label: string;
  readonly value: string;
}

/**
 * Read every video diagnostic at once. NEVER THROWS: a failed read is reported
 * as its own text, so a broken Prometheus can never replace the original
 * assertion failure this dump decorates.
 */
export async function videoDiagnosticReads(): Promise<readonly DiagnosticRead[]> {
  const reads: ReadonlyArray<readonly [string, string, readonly string[]]> = [
    ['MH pending edges (a missing keyframe event if > 0 and stuck)', MH_EDGES_PENDING_PROMQL, []],
    ['MH oldest pending edge age, s', MH_EDGE_PENDING_OLDEST_PROMQL, []],
    ['MH keyframe requests sent, by trigger', MH_KEYFRAME_REQUESTS_SENT_PROMQL, ['trigger']],
    ['MH keyframe request write failures', MH_KEYFRAME_WRITE_FAILURES_PROMQL, []],
    ['MC video slot states conveyed, by state', MC_VIDEO_SLOT_STATES_PROMQL, ['slot_state']],
    ['MC unreachable senders named', MC_UNREACHABLE_SENDERS_PROMQL, []],
  ];
  return Promise.all(
    reads.map(async ([label, query, by]) => {
      try {
        const rows = await promInstantRows(query);
        const value =
          rows.length === 0
            ? '(empty)'
            : rows
                .map((r) => {
                  const key = [
                    r.metric?.['instance'] ?? '?',
                    ...by.map((k) => r.metric?.[k] ?? '?'),
                  ];
                  return `${key.join('/')}=${r.value?.[1] ?? '?'}`;
                })
                .join(', ');
        return { label, value };
      } catch (err) {
        return { label, value: `READ FAILED: ${err instanceof Error ? err.message : String(err)}` };
      }
    }),
  );
}

// ============================================================================
// Story 2 R-27 / S6 — CLIENT series read back out of Prometheus.
//
// FLEET-LEVEL EVIDENCE ONLY. Client series carry `instance` = the collector pod
// (`./instanceCounters` header) and no participant label, so these prove the
// PIPE (SDK -> GC proxy -> collector -> Prometheus), the exact exported NAME
// (`_total` survival) and the alert's applicability — never a per-participant
// claim, which comes from that participant's own E2E bus. No `job`/`instance`
// selector and no exact label set (`otel_scope_*` is version-dependent).
//
// Absent reads as 0 at baseline (`resultsToInstanceMap` of an empty vector is an
// empty map): these counters are registered lazily, so a first-ever series is
// BORN by the scenario, and a baseline -> above delta is the only read that can
// see it. PromQL `increase()` cannot: it has no pre-birth sample, so a series
// born at N and then flat reads ~0 on correct behaviour.
//
// Budget: the SDK export is forced (`flushMetrics()`), then one collector scrape
// — the default 60 s poll bound covers it with margin (the collector's scrape job
// in `infra/kubernetes/observability/prometheus.yml`). A liveness bound, never a
// latency gate.
// ============================================================================

const SENT_SUM_PROMQL = `sum by (instance) (${SEND_COUNTER})`;
const RECEIVED_SUM_PROMQL = `sum by (instance) (${RECEIVED_COUNTER})`;
// Story 3 OBS-A (R-29): the same counter split by kind. The audio arm selects
// BOTH kind labels (`content_kind` co-carried since 2026-10-26), so it proves all
// three hops carry both (SDK emission, GC datapoint tier, collector keep_keys +
// exact value rules); the KIND-LESS arm (`=""` matches series WITHOUT the label)
// selects a series missing EITHER label and must NOT rise, which proves the two
// labels arrive all-or-none and no SDK path emits a bag without them.
const RECEIVED_AUDIO_SUM_PROMQL = `sum by (instance) (${RECEIVED_COUNTER}{media_kind="${MEDIA_KIND_AUDIO}", content_kind="${CONTENT_KIND_AUDIO}"})`;
// Story 3 C-C1 (R-29): the VIDEO arms — received AND sent, so both halves of
// the video client series are proven to carry `media_kind="video"` (and
// `content_kind="main"`: camera) end to end.
// The series is born when video is first DIRECTED (lazily), so only a baseline
// taken before the joins -> above delta can see it.
const RECEIVED_VIDEO_SUM_PROMQL = `sum by (instance) (${RECEIVED_COUNTER}{media_kind="${MEDIA_KIND_VIDEO}", content_kind="${CONTENT_KIND_VIDEO}"})`;
const SENT_VIDEO_SUM_PROMQL = `sum by (instance) (${SEND_COUNTER}{media_kind="${MEDIA_KIND_VIDEO}", content_kind="${CONTENT_KIND_VIDEO}"})`;
const RECEIVED_KINDLESS_SUM_PROMQL = `sum by (instance) (${RECEIVED_COUNTER}{media_kind=""} or ${RECEIVED_COUNTER}{content_kind=""})`;
const KEK_UPDATES_SUM_PROMQL = `sum by (instance) (${KEK_UPDATES}{source="${KEK_UPDATE_SOURCE}"})`;
const KEK_RETAINED_SUM_PROMQL = `sum by (instance) (${KEK_GENERATIONS_RETAINED})`;

/** Which client counter a read-back is about. */
export type ClientCounter =
  | 'sent'
  | 'received'
  | 'received_audio'
  | 'received_video'
  | 'sent_video'
  | 'received_kindless'
  | 'kek_update'
  | 'kek_retained';

const CLIENT_COUNTER_PROMQL: Record<ClientCounter, string> = {
  sent: SENT_SUM_PROMQL,
  received: RECEIVED_SUM_PROMQL,
  received_audio: RECEIVED_AUDIO_SUM_PROMQL,
  received_video: RECEIVED_VIDEO_SUM_PROMQL,
  sent_video: SENT_VIDEO_SUM_PROMQL,
  received_kindless: RECEIVED_KINDLESS_SUM_PROMQL,
  kek_update: KEK_UPDATES_SUM_PROMQL,
  kek_retained: KEK_RETAINED_SUM_PROMQL,
};

/**
 * The collector accumulator's REJECTED datapoints, by cause — a diagnostic for a
 * client counter that fell short, never an assertion. `delta_to_cumulative`
 * counts every point it processes on `otelcol_deltatocumulative_datapoints`
 * (collector self-telemetry, `otel-collector-telemetry` job) and attaches
 * `error=<Go error type>` to the ones it DROPS (e.g. `delta.ErrOutOfOrder`,
 * `delta.ErrOlderStart`, `limit`). A drop there loses an increment PERMANENTLY,
 * so no poll budget recovers it. The name is matched with and without `_total`
 * because the suffix has moved between collector versions.
 */
const ACCUMULATOR_DROPS_BY_CAUSE_PROMQL =
  'sum by (error) ({__name__=~"otelcol_deltatocumulative_datapoints(_total)?",error!=""})';

/** Accumulator drops, `error` cause -> count (empty = none ever). */
export type AccumulatorDrops = ReadonlyMap<string, number>;

/** Current accumulator-drop snapshot. Take it BEFORE the action under test. */
export async function collectorAccumulatorDropsByCause(): Promise<AccumulatorDrops> {
  const map = new Map<string, number>();
  for (const row of await promInstantRows(ACCUMULATOR_DROPS_BY_CAUSE_PROMQL)) {
    const raw = row.value?.[1];
    const parsed = raw === undefined || raw.trim() === '' ? Number.NaN : Number(raw);
    if (!Number.isFinite(parsed)) {
      throw new Error(
        `Prometheus returned a non-numeric sample "${String(raw)}" for ${ACCUMULATOR_DROPS_BY_CAUSE_PROMQL}`,
      );
    }
    map.set(row.metric?.['error'] ?? '<no error label>', parsed);
  }
  return map;
}

/** "since baseline" rendering of accumulator drops, for a failure message. */
async function describeAccumulatorDropsSince(baseline: AccumulatorDrops): Promise<string> {
  let current: AccumulatorDrops;
  try {
    current = await collectorAccumulatorDropsByCause();
  } catch (e) {
    return `Collector accumulator-drop diagnostic could not be read: ${String(e)}`;
  }
  const rises = [...current.entries()]
    .map(([cause, value]) => [cause, value - (baseline.get(cause) ?? 0)] as const)
    .filter(([, delta]) => delta > 0)
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  if (rises.length === 0) {
    return (
      'Collector delta_to_cumulative dropped NO datapoints since the baseline, so the missing ' +
      'increment never reached the accumulator: look upstream (the SDK export, GC proxy, the ' +
      "collector's filters/memory_limiter via otelcol_processor_incoming/outgoing_items)."
    );
  }
  return (
    `Collector delta_to_cumulative DROPPED datapoints since the baseline: {${rises
      .map(([cause, delta]) => `${cause}=+${delta}`)
      .join(', ')}} (collector-wide, so a concurrent suite can contribute). A drop there loses ` +
    'the increment permanently; ErrOutOfOrder means concurrent same-stream exports reached the ' +
    'accumulator out of stamp order — check batch/client_metric_order is still immediately ' +
    'before transform/client_metric_clock in infra/services/otel-collector/collector.yaml.'
  );
}

/** Current per-instance snapshot of a client counter (empty = never exported yet). */
export async function clientCounterByInstance(which: ClientCounter): Promise<InstanceCounters> {
  return promInstantByInstance(CLIENT_COUNTER_PROMQL[which]);
}

/**
 * Poll until the client counter rose by at least `minDelta` (default: any rise)
 * past `baseline` on some instance. The query is by the EXACT exported name, so a
 * pipeline that lost the `_total` suffix returns empty and times out loudly here.
 */
export async function waitForClientCounterAbove(
  which: ClientCounter,
  baseline: InstanceCounters,
  why: string,
  options: PollOptions & {
    minDelta?: number;
    /**
     * A {@link collectorAccumulatorDropsByCause} snapshot taken before the
     * action under test. When given, a timeout names any accumulator drops
     * since then, so a lost increment is distinguishable from a slow one.
     */
    accumulatorDropsBaseline?: AccumulatorDrops;
  } = {},
): Promise<void> {
  try {
    await pollUntilAnyInstanceAbove(
      CLIENT_COUNTER_PROMQL[which],
      baseline,
      options,
      (lastObserved, timeoutMs) =>
        `${CLIENT_COUNTER_PROMQL[which]} did not rise ${
          options.minDelta === undefined ? 'above' : `by >= ${options.minDelta} over`
        } baseline ${formatInstanceMap(baseline)} within ${timeoutMs}ms (last observed: ` +
        `${lastObserved}). ${why} If the vector is EMPTY, nothing with this exact name reached ` +
        `Prometheus: check the SDK export (flushMetrics), GC's /api/v1/telemetry proxy, the ` +
        `collector's Prometheus exporter (add_metric_suffixes) and its scrape job.`,
    );
  } catch (e) {
    if (options.accumulatorDropsBaseline === undefined || !(e instanceof Error)) throw e;
    e.message = `${e.message} ${await describeAccumulatorDropsSince(options.accumulatorDropsBaseline)}`;
    throw e;
  }
}

/**
 * Throw unless the client counter is still AT OR BELOW `baseline` on every
 * instance — the absence half of a read-back. Run it AFTER a positive rise in the
 * same window (e.g. {@link waitForClientCounterAbove} on a sibling selector), so
 * "did not rise" is judged over a window in which the pipe demonstrably carried
 * data rather than over an idle one.
 */
export async function expectClientCounterNotAbove(
  which: ClientCounter,
  baseline: InstanceCounters,
  why: string,
): Promise<void> {
  const current = await promInstantByInstance(CLIENT_COUNTER_PROMQL[which]);
  if (anyInstanceExceedsBaseline(baseline, current)) {
    throw new Error(
      `${CLIENT_COUNTER_PROMQL[which]} ROSE above baseline ${formatInstanceMap(baseline)} ` +
        `(now ${formatInstanceMap(current)}). ${why}`,
    );
  }
}

interface PromRulesResponse {
  readonly status?: string;
  readonly data?: {
    readonly groups?: ReadonlyArray<{
      readonly rules?: ReadonlyArray<{
        readonly type?: string;
        readonly name?: string;
        readonly query?: string;
      }>;
    }>;
  };
}

/**
 * The `dt_client_*` names the alert rule Prometheus has LOADED selects — the
 * running rule, not the committed file. Throws a distinct `rule-not-loaded`
 * error when Prometheus has no such alert.
 */
export async function loadedAlertClientNames(alert: string): Promise<Set<string>> {
  const url = `${e2eEnv.prometheusUrl}/api/v1/rules?type=alert`;
  const response = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!response.ok) {
    throw new Error(`Prometheus rules query failed: ${url} responded ${response.status}`);
  }
  const body = (await response.json()) as PromRulesResponse;
  if (body.status !== 'success') {
    // A QUERY failure, kept distinct from `rule-not-loaded` so triage is not
    // sent to reload the observability overlay for a Prometheus error.
    throw new Error(
      `Prometheus rules query returned status "${body.status ?? 'undefined'}" (${url}). ` +
        'Global-setup proved Prometheus healthy, so a query error here is a real problem — ' +
        'NOT a missing rule. Triage Prometheus/port-forward, not the service under test.',
    );
  }
  const rule = (body.data?.groups ?? [])
    .flatMap((g) => g.rules ?? [])
    .find((r) => r.type === 'alerting' && r.name === alert);
  if (rule?.query === undefined) {
    throw new Error(
      `rule-not-loaded: Prometheus at ${e2eEnv.prometheusUrl} has no loaded alert ${alert}. ` +
        `The committed rule file and the running rules have diverged (reload the Kind ` +
        `observability overlay), which is a DIFFERENT fault from a selector drift.`,
    );
  }
  return clientNamesIn(rule.query);
}
