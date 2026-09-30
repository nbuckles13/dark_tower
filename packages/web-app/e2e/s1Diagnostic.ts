// File: packages/web-app/e2e/s1Diagnostic.ts
//
// Story 2 S1 diagnostic: when a receiver is missing an expected sender, was it
// a CONFIG-CAUSED EGRESS-BUDGET REJECTION at MH admission, or MISROUTING?
//
// Why this exists: a defaulted or undersized Kind egress budget
// (`MH_EGRESS_BUDGET_BPS`, set for Kind in
// `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml`)
// rejects admission partway through the largest scenario, and "some receivers
// are missing some senders" is then indistinguishable by symptom from the
// routing bug the suite exists to catch. The evidence is SERVER-SIDE and
// independent of the client metrics pipe: MH's own admission counter, read
// through Prometheus (`mcMetrics.ts` owns the PromQL and the fetch). Edges
// retained through MC's disconnect grace across back-to-back scenarios are a
// real config-caused cause too, which is why the report carries the edges and
// ceiling gauges.
//
// Pure — no `./env`, no fetch — so `tests/s1Diagnostic.test.ts` runs hermetically.
//
// METRIC NAMES: the SSoT is `crates/mh-service/src/observability/metrics.rs`
// (`resolve_session_handles`, `publish_egress_admission`,
// `StreamAdmissionOutcome::as_label`). TypeScript cannot import a Rust const, so
// each name is written ONCE below and `tests/s1Diagnostic.test.ts` reads that
// file and fails on drift.

import {
  anyInstanceExceedsBaseline,
  formatInstanceMap,
  type InstanceCounters,
} from './instanceCounters.js';

/**
 * The MH Prometheus scrape job — SSoT `job_name: 'mh-service'` in
 * `infra/kubernetes/observability/prometheus.yml` (drift-tested). A rename
 * would otherwise empty the `up` read and make every diagnosis unobservable.
 */
export const MH_SCRAPE_JOB = 'mh-service';

/** Counter, one series per `outcome`; zero-registered at MH boot. */
export const MH_ADMISSION_TOTAL = 'mh_media_stream_admission_total';
/** The `outcome` value for a stream-ceiling (egress budget) refusal. */
export const MH_ADMISSION_REJECTED_OUTCOME = 'rejected_stream_ceiling';
/** Gauge: share of admission decisions refused over MH's sliding window. */
export const MH_ADMISSION_REJECTION_RATIO = 'mh_media_stream_admission_rejection_ratio';
/** Gauge: the derived egress stream ceiling admission enforces. */
export const MH_EGRESS_STREAM_CEILING = 'mh_media_egress_stream_ceiling';
/** Gauge: installed egress streams on the handler. */
export const MH_EGRESS_EDGES = 'mh_media_egress_edges';

/** The per-instance evidence the classifier reads. */
export interface AdmissionEvidence {
  /** Rejected-outcome counter, per MH instance, BEFORE the scenario's joins. */
  readonly baselineRejected: InstanceCounters;
  /** The same counter now. */
  readonly currentRejected: InstanceCounters;
  /** Context, per instance (may be empty; never decides the verdict). */
  readonly rejectionRatio: InstanceCounters;
  readonly streamCeiling: InstanceCounters;
  readonly egressEdges: InstanceCounters;
  /**
   * `up` of every MH scrape target, per instance. Concluding "misrouting"
   * requires every target up: a down target's series go stale and simply drop
   * out of the `sum by (instance)` read, which would otherwise look clean.
   */
  readonly scrapeUp: InstanceCounters;
}

export type MissingSenderVerdict =
  /** MH refused admission during the scenario: configuration, not routing. */
  | 'config_budget_rejection'
  /** MH admitted everything it was asked to; the sender went missing elsewhere. */
  | 'misrouting'
  /** The rejected counter was not observable — the diagnostic proves nothing. */
  | 'unobservable';

export interface MissingSenderDiagnosis {
  readonly verdict: MissingSenderVerdict;
  readonly report: string;
}

/**
 * Classify a missing sender from MH admission evidence.
 *
 * - `config_budget_rejection`: the `rejected_stream_ceiling` counter moved on
 *   ANY MH instance since the baseline (per-instance delta, robust to a pod
 *   rollover — `anyInstanceExceedsBaseline`, with its documented residual
 *   window).
 * - `misrouting`: every baseline instance is still visible and none moved.
 * - `unobservable`: the baseline or current read is EMPTY (the counter is
 *   registered at zero from MH boot, so empty means "not scraped", never
 *   "zero"), or — with no rejection seen — an MH scrape target is not up, or
 *   an instance present at baseline is missing now. Folding any of these into `misrouting` would be the confident
 *   wrong answer this helper exists to avoid.
 *
 * `what` names the missing edge ("C missing B") for the report.
 */
export function classifyMissingSender(
  what: string,
  evidence: AdmissionEvidence,
): MissingSenderDiagnosis {
  const context =
    `${MH_ADMISSION_TOTAL}{outcome="${MH_ADMISSION_REJECTED_OUTCOME}"} baseline ` +
    `${formatInstanceMap(evidence.baselineRejected)} -> now ${formatInstanceMap(evidence.currentRejected)}; ` +
    `${MH_ADMISSION_REJECTION_RATIO} ${formatInstanceMap(evidence.rejectionRatio)}; ` +
    `${MH_EGRESS_STREAM_CEILING} ${formatInstanceMap(evidence.streamCeiling)}; ` +
    `${MH_EGRESS_EDGES} ${formatInstanceMap(evidence.egressEdges)}; ` +
    `up ${formatInstanceMap(evidence.scrapeUp)}`;

  if (evidence.baselineRejected.size === 0) {
    // An empty BASELINE compares every current instance against 0, and the
    // lifetime counter is NOT 0 in Layer 7 (the Rust env-tests drive
    // rejections on these same pods just before this suite) — so proceeding
    // would report a budget rejection that hides a real misrouting.
    return {
      verdict: 'unobservable',
      report:
        `S1 diagnostic for "${what}": UNOBSERVABLE — the BASELINE read of MH's admission counter ` +
        `was empty, so no in-window delta can be formed. Take mhAdmissionRejectionsByInstance ` +
        `before the scenario's joins, while MH is being scraped. ${context}`,
    };
  }
  if (evidence.currentRejected.size === 0) {
    return {
      verdict: 'unobservable',
      report:
        `S1 diagnostic for "${what}": UNOBSERVABLE — the CURRENT read of MH's admission counter ` +
        `returned no series, and it is registered at zero from boot, so MH is not being scraped. ` +
        `Neither budget rejection nor misrouting can be concluded. ${context}`,
    };
  }
  // Positive evidence wins even under partial visibility: a rejection seen on
  // any instance (incl. a fresh post-rollover pod — residual documented on
  // anyInstanceExceedsBaseline) is a rejection.
  if (anyInstanceExceedsBaseline(evidence.baselineRejected, evidence.currentRejected)) {
    return {
      verdict: 'config_budget_rejection',
      report:
        `S1 diagnostic for "${what}": CONFIG-CAUSED BUDGET REJECTION — MH refused egress stream ` +
        `admission during this scenario, so the missing sender is a capacity refusal, not a ` +
        `routing bug. Check MH_EGRESS_BUDGET_BPS in ` +
        `infra/kubernetes/overlays/kind/services/mh-service/configmap-egress-budget-patch.yaml ` +
        `against the scenario's edges, and edges retained from a previous scenario through MC's ` +
        `disconnect grace. ${context}`,
    };
  }
  const down = [...evidence.scrapeUp.entries()]
    .filter(([, up]) => up !== 1)
    .map(([instance]) => instance)
    .sort();
  if (evidence.scrapeUp.size === 0 || down.length > 0) {
    return {
      verdict: 'unobservable',
      report:
        `S1 diagnostic for "${what}": UNOBSERVABLE (partial) — ` +
        (down.length > 0
          ? `MH scrape target(s) ${down.join(', ')} are down`
          : 'no MH scrape target reported `up`') +
        `, so a rejection on that handler cannot be ruled out and "misrouting" cannot be ` +
        `concluded. ${context}`,
    };
  }
  // Concluding "misrouting" ALSO needs every baseline instance still visible: an instance seen at baseline
  // and missing now (unscraped, down, or rolled) may be exactly the handler
  // that rejected. A NEW instance is the rollover case and needs nothing extra.
  const unseen = [...evidence.baselineRejected.keys()]
    .filter((instance) => !evidence.currentRejected.has(instance))
    .sort();
  if (unseen.length > 0) {
    return {
      verdict: 'unobservable',
      report:
        `S1 diagnostic for "${what}": UNOBSERVABLE (partial) — MH instance(s) ${unseen.join(', ')} ` +
        `present at baseline are missing from the current read, so a rejection there cannot be ` +
        `ruled out and "misrouting" cannot be concluded. Check those MH pods and their scrape. ` +
        `${context}`,
    };
  }
  return {
    verdict: 'misrouting',
    report:
      `S1 diagnostic for "${what}": MISROUTING — MH admitted every stream it was asked to ` +
      `install during this scenario (no stream-ceiling rejection on any instance), so the ` +
      `egress budget did not cause this. Investigate routing/assignment. ${context}`,
  };
}
